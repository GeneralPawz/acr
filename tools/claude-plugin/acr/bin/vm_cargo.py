"""Runs a build or test command for an ACR worktree on the render VM instead of this PC.

    acr-vm-cargo [--wt DIR] [--name NAME] [--get REMOTE=LOCAL ...] [--no-sync] -- CMD...
    (the plugin's bin/ wrapper for this file; `python vm_cargo.py` works the same)

The worktree's files (git-tracked plus untracked, not ignored; ~9 MB) go to the VM as a
tar, land in a staging folder and are copied over with `rsync --checksum`, so files whose
content did not change keep their time and cargo does not rebuild them. Then CMD runs
there in bash, in ~/wt/NAME, with:
    CARGO_TARGET_DIR=~/target/NAME   (one per worktree, never shared)
    RUSTC_WRAPPER=sccache            (dependencies compiled once for all branches)
    ACR_GAME_DIR=~/game/American Conquest, ACR_SOURCES=~/sources, BLENDER=/opt/blender/blender
At most two commands build at once (VM slots); others wait. Output streams back; the exit
code is CMD's. `--get` fetches files or folders afterwards (REMOTE `target/...` is in
~/target/NAME, other relative paths in the worktree, or absolute starting with ~ or /; LOCAL an absolute Windows path).

On the VM itself (hostname `claude-render`, or `ACR_VM_LOCAL=1`) the same steps run through
the local shell instead of ssh: the worktree is still staged into ~/wt/NAME, so builds,
checks and the studio behave the same from Windows and from a session on the VM. The VM
keeps everything under ~/projects/acr; ~/wt, ~/target, ~/sources, ~/game and the other
old paths are links into it.

The landing checks are `acr-check` on the VM (this folder's acr-check.sh, installed into
~/bin on every sync).

Examples (Git Bash, from a worktree):
    acr-vm-cargo -- acr-check
    acr-vm-cargo -- cargo test -q -p acr-sim --test plough
    acr-vm-cargo -- cargo xtask assets artpus --get 'target/assets/units/artpus.glb=C:/x/artpus.glb'
"""

import argparse
import io
import os
import shlex
import socket
import subprocess
import sys
import tarfile

HOST = "claude@192.168.178.130"
KEY = os.path.expanduser("~/.ssh/claude-render")
# On the VM itself every "remote" command runs in the local shell: `SSH + [cmd]` stays the
# one way to run a VM command, from Windows and from a session on the VM alike.
LOCAL = os.environ.get("ACR_VM_LOCAL") == "1" or (os.name != "nt" and socket.gethostname() == "claude-render")
SSH = (["bash", "-c"] if LOCAL else
       ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "IdentitiesOnly=yes",
        "-o", "ServerAliveInterval=30", "-i", KEY, HOST])
SLOTS = 2
HERE = os.path.dirname(os.path.abspath(__file__))
# Where Git Bash's MSYS root lies on Windows: a POSIX path it rewrote starts with this.
MSYS_ROOTS = [r.replace("\\", "/").rstrip("/") for r in
              (os.environ.get("ACR_MSYS_ROOT", ""), r"C:/Program Files/Git", r"C:/Program Files (x86)/Git") if r]


def unmsys(value):
    """Undoes Git Bash's path conversion of a VM path: `C:/Program Files/Git/home/x` (also after
    `K=`) becomes `/home/x` again. The wrappers switch the conversion off; this covers calling
    the .py directly. Values without that prefix are returned unchanged."""
    key, eq, val = value.partition("=") if "=" in value and not value[:3].endswith(":/") else ("", "", value)
    norm = val.replace("\\", "/")
    for root in MSYS_ROOTS:
        if norm.lower().startswith(root.lower() + "/"):
            val = norm[len(root):]
            break
    return f"{key}{eq}{val}"


def local_path(p):
    """A local path as Windows sees it: `/c/Users/x` (Git Bash form) becomes `C:/Users/x`."""
    if p and len(p) >= 3 and p[0] == "/" and p[1].isalpha() and p[2] == "/":
        return f"{p[1].upper()}:/{p[3:]}"
    return p


def bounded(cmd, timeout=None, input=None):
    """subprocess.run(capture_output=True) that cannot hang past `timeout` (+5 s) on Windows:
    run() waits without a limit for the pipes after killing a timed-out child, which never
    ends while a grandchild keeps them open. Returns CompletedProcess (bytes); raises
    subprocess.TimeoutExpired."""
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE if input is not None else subprocess.DEVNULL,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        out, err = p.communicate(input, timeout=timeout)
    except subprocess.TimeoutExpired:
        p.kill()
        try:
            p.communicate(timeout=5)
        except (subprocess.TimeoutExpired, OSError, ValueError):
            pass
        raise
    return subprocess.CompletedProcess(cmd, p.returncode, out, err)


def files(wt):
    out = subprocess.run(["git", "-C", wt, "ls-files", "-co", "--exclude-standard", "-z"],
                         capture_output=True, check=True).stdout
    return [f for f in out.decode("utf-8").split("\0") if f and os.path.isfile(os.path.join(wt, f))]


def sync(wt, name, timeout=None):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz", compresslevel=3) as tar:
        for f in files(wt):
            tar.add(os.path.join(wt, f), arcname=f)
        tar.add(os.path.join(HERE, "acr-check.sh"), arcname=".acr-check.sh")
    cmd = (f"set -e; mkdir -p ~/stage/{name} ~/wt/{name} ~/bin; rm -rf ~/stage/{name}; mkdir -p ~/stage/{name}; "
           f"tar xzf - -C ~/stage/{name}; install -m 755 ~/stage/{name}/.acr-check.sh ~/bin/acr-check; "
           f"rm ~/stage/{name}/.acr-check.sh; rsync -rl --checksum --delete "
           f"--exclude=/target --exclude=/target-win ~/stage/{name}/ ~/wt/{name}/")
    if timeout is None:
        r = subprocess.run(SSH + [cmd], input=buf.getvalue())
    else:  # the studio server: must never hang
        r = bounded(SSH + [cmd], timeout, buf.getvalue())
    if r.returncode != 0:
        err = (r.stderr or b"").decode(errors="replace").strip()[-500:] if isinstance(r.stderr, bytes) else ""
        sys.exit(f"vm_cargo: sync failed ({r.returncode}) {err}")


def run(name, command):
    env = (f'export PATH=$HOME/bin:$HOME/.cargo/bin:$PATH CARGO_TARGET_DIR=$HOME/target/{name} '
           f'RUSTC_WRAPPER=sccache SCCACHE_DIR=$HOME/.cache/sccache SCCACHE_CACHE_SIZE=40G '
           f'ACR_GAME_DIR="$HOME/game/American Conquest" ACR_SOURCES=$HOME/sources '
           f'BLENDER=/opt/blender/blender CARGO_TERM_COLOR=never')
    # Wait for one of the build slots, then run in the worktree.
    slot = ("mkdir -p ~/locks; exec 9>/dev/null; while :; do for i in $(seq 1 %d); do "
            "exec 9>~/locks/slot$i; if flock -n 9; then break 2; fi; done; sleep 2; done" % SLOTS)
    script = f"{slot}; {env}; cd ~/wt/{name} && {command}"
    return subprocess.run(SSH + ["bash -lc " + shlex.quote(script)]).returncode


def get(name, pairs):
    failed = False
    for p in pairs:
        if "=" not in p:
            print(f"vm_cargo: --get needs REMOTE=LOCAL, got {p!r}", file=sys.stderr)
            failed = True
            continue
        remote, local = p.split("=", 1)
        # MSYS may have rewritten a leading-slash REMOTE into a Windows path; undo that.
        remote, local = unmsys(remote), local_path(local)
        if not remote.startswith(("/", "~")):
            remote = f"~/wt/{name}/{remote}"
        parent, base = os.path.split(remote.rstrip("/"))
        data = subprocess.run(SSH + [f"cd {parent} && tar czf - {shlex.quote(base)}"], capture_output=True)
        if data.returncode != 0:
            print(f"vm_cargo: cannot fetch {remote}: {data.stderr.decode(errors='replace')}", file=sys.stderr)
            failed = True
            continue
        with tarfile.open(fileobj=io.BytesIO(data.stdout), mode="r:gz") as tar:
            members = tar.getmembers()
            if len(members) == 1 and members[0].isfile():
                os.makedirs(os.path.dirname(os.path.abspath(local)), exist_ok=True)
                with open(local, "wb") as f:
                    f.write(tar.extractfile(members[0]).read())
            else:
                os.makedirs(local, exist_ok=True)
                for m in members:
                    if m.isfile():
                        dest = os.path.join(local, *m.name.split("/")[1:])
                        os.makedirs(os.path.dirname(dest), exist_ok=True)
                        with open(dest, "wb") as f:
                            f.write(tar.extractfile(m).read())
        print(f"vm_cargo: fetched {remote} -> {local}", file=sys.stderr)
    return failed


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=os.getcwd())
    ap.add_argument("--name")
    ap.add_argument("--get", action="append", default=[])
    ap.add_argument("--no-sync", action="store_true")
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    cmd = a.cmd[1:] if a.cmd[:1] == ["--"] else a.cmd
    # `acr-vm-cargo -- CMD --get R=L` : --get after the command is ours, not CMD's.
    rest, i = [], 0
    while i < len(cmd):
        if cmd[i] == "--get" and i + 1 < len(cmd):
            a.get.append(cmd[i + 1])
            i += 2
        elif cmd[i].startswith("--get="):
            a.get.append(cmd[i][6:])
            i += 1
        else:
            rest.append(cmd[i])
            i += 1
    cmd = rest
    wt = subprocess.run(["git", "-C", local_path(a.wt), "rev-parse", "--show-toplevel"],
                        capture_output=True, text=True, check=True).stdout.strip()
    name = a.name or os.path.basename(wt)
    if not a.no_sync:
        sync(wt, name)
    code = 0
    if cmd:
        code = run(name, " ".join(cmd) if len(cmd) > 1 else cmd[0])
    if get(name, a.get) and code == 0:
        code = 3
    return code


if __name__ == "__main__":
    sys.exit(main())
