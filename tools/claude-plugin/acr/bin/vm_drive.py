"""Runs the game's test driver headless on the render VM (RX 9060 XT, Vulkan) and fetches
the screenshots back. Builds the worktree's release game there first.

    acr-vm-drive --steps STEPS.json --out C:/abs/out [--wt DIR] [--name NAME]
                       [--models C:/abs/glbs] [--size 1600x900] [--no-build] [--env K=V ...]
                       [--settings models=all,gore=full,...]

STEPS.json: a list of driver requests (docs/testing.md), e.g.
    [{"cmd":"start","blank":{"editor":true,"hit_locations":true,"hud":false}},
     {"cmd":"spawn","type":"EngM18","x":7680,"y":7680,"dir":0},
     {"cmd":"camera","x":7680,"y":7680},
     {"cmd":"screenshot","path":"a.png"}]
A relative screenshot `path` is written under the run's output folder, which is fetched to
--out together with the driver's answers (answers.jsonl) and the game log (game.log).

Models: the VM keeps a copy of the showcase models (synced from
ACR-analysis/showcase/models before each run, changed files only). --models adds a local
folder of .glb files on top (your rebuilt units); a glb built on the VM by `cargo xtask
assets` in the same worktree is used directly with --models wt:target/assets/units.
One driver process per field: every run is its own process.

Settings: the run reads the showcase's settings.txt (~/showcase/settings). --settings
k=v,k=v copies it into the run folder with those keys replaced (e.g. models=all, shadows=off)
and points ACR_SETTINGS_DIR there. --env K=V exports more variables; a VM path such as
ACR_SETTINGS_DIR=/home/claude/x works as written from Git Bash (the wrapper switches MSYS
path conversion off, and a converted value is turned back). The game logs which settings file
it read and its models mode; that line is printed after the run.
"""

import argparse
import hashlib
import io
import json
import os
import re
import shlex
import subprocess
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import vm_cargo  # noqa: E402

SHOWCASE = r"C:\Users\verwalter\WIP\ACR-analysis\showcase"


def sync_dir(local, remote, timeout=None):
    """Sends the files of `local` whose MD5 differs from `remote` (VM path). A size check alone
    kept stale copies of rebuilt glbs that came out with the same byte size."""
    listing = vm_cargo.bounded(vm_cargo.SSH + [f"mkdir -p {remote} && cd {remote} && find . -maxdepth 1 -type f "
                                               f"-exec md5sum {{}} +"], timeout).stdout.decode(errors="replace")
    have = {}
    for line in listing.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2:
            have[parts[1].strip().removeprefix("./")] = parts[0]

    def md5(path):
        h = hashlib.md5()
        with open(path, "rb") as fh:
            for block in iter(lambda: fh.read(1 << 20), b""):
                h.update(block)
        return h.hexdigest()

    send = [f for f in os.listdir(local) if os.path.isfile(os.path.join(local, f))
            and have.get(f) != md5(os.path.join(local, f))]
    if not send:
        return 0
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        for f in send:
            tar.add(os.path.join(local, f), arcname=f)
    r = vm_cargo.bounded(vm_cargo.SSH + [f"tar xf - -C {remote}"], timeout, buf.getvalue())
    if r.returncode != 0:
        raise subprocess.CalledProcessError(r.returncode, "tar xf (model sync)", r.stdout, r.stderr)
    return len(send)


def sync_showcase(timeout=None):
    """Brings ~/showcase on the VM up to date with the local showcase (models, their
    subfolders such as ground/, settings). Returns how many model files were sent."""
    n = sync_dir(os.path.join(SHOWCASE, "models"), "~/showcase/models", timeout)
    for sub in sorted(os.listdir(os.path.join(SHOWCASE, "models"))):  # e.g. ground/ (texture pack)
        if os.path.isdir(os.path.join(SHOWCASE, "models", sub)):
            n += sync_dir(os.path.join(SHOWCASE, "models", sub), f"~/showcase/models/{sub}", timeout)
    sync_dir(os.path.join(SHOWCASE, "settings"), "~/showcase/settings", timeout)
    return n


def settings_script(spec, base, dest):
    """Shell lines that copy `base`/settings.txt to `dest` with the keys of `spec`
    ('k=v,k=v') set. Keys and values are checked, so nothing reaches the shell unquoted."""
    pairs = [p.strip() for p in spec.split(",") if p.strip()]
    lines = [f"mkdir -p {dest} && cp {base}/settings.txt {dest}/settings.txt 2>/dev/null || touch {dest}/settings.txt"]
    for p in pairs:
        k, eq, v = p.partition("=")
        if not eq or not re.fullmatch(r"[a-z_0-9]+", k) or not re.fullmatch(r"[A-Za-z0-9_.-]*", v):
            sys.exit(f"vm_drive: --settings wants key=value pairs like models=all, got {p!r}")
        lines.append(f"(grep -q '^{k}=' {dest}/settings.txt && sed -i 's/^{k}=.*/{k}={v}/' {dest}/settings.txt "
                     f"|| echo '{k}={v}' >> {dest}/settings.txt)")
    return " && ".join(lines) + "; "


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--steps", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--wt", default=os.getcwd())
    ap.add_argument("--name")
    ap.add_argument("--models")
    ap.add_argument("--size", default="1600x900")
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--env", action="append", default=[])
    ap.add_argument("--settings", default="", help="k=v,k=v over the showcase settings.txt, e.g. models=all")
    ap.add_argument("--timeout", type=int, default=600)
    a = ap.parse_args()
    a.env = [vm_cargo.unmsys(e) for e in a.env]
    a.steps, a.out, a.wt = (vm_cargo.local_path(x) for x in (a.steps, a.out, a.wt))
    if a.models and not a.models.startswith("wt:"):
        a.models = vm_cargo.local_path(a.models)
    wt = subprocess.run(["git", "-C", a.wt, "rev-parse", "--show-toplevel"],
                        capture_output=True, text=True, check=True).stdout.strip()
    name = a.name or os.path.basename(wt)
    steps = json.load(open(a.steps, encoding="utf-8"))
    run_id = hashlib.sha1(f"{name}{os.getpid()}".encode()).hexdigest()[:8]
    rout = f"/home/claude/runs/{name}-{run_id}"
    for s in steps:
        if "path" in s and not os.path.isabs(s["path"]) and not s["path"].startswith("/"):
            s["path"] = f"{rout}/{s['path']}"

    n = sync_showcase()
    if n:
        print(f"vm_drive: {n} showcase model(s) updated on the VM", file=sys.stderr)
    # The models folder of this run: the showcase models with the overrides on top.
    layer = f"mkdir -p {rout}/models && cp -rs ~/showcase/models/. {rout}/models/ 2>/dev/null; "
    if a.models and a.models.startswith("wt:"):
        layer += f"cp -f ~/wt/{name}/{a.models[3:]}/*.glb {rout}/models/ 2>/dev/null; "
    elif a.models:
        sync_dir(a.models, f"~/overrides/{name}")
        layer += f"cp -f ~/overrides/{name}/*.glb {rout}/models/ 2>/dev/null; "

    if not a.no_build:
        vm_cargo.sync(wt, name)
    req = "\n".join(json.dumps(s) for s in steps) + "\n"
    settings_dir = "$HOME/showcase/settings"
    for e in a.env:
        if e.startswith("ACR_SETTINGS_DIR="):
            settings_dir = shlex.quote(e.split("=", 1)[1])
    if a.settings:
        layer += settings_script(a.settings, settings_dir, f"{rout}/settings")
        settings_dir = f"{rout}/settings"
    a.env = [e for e in a.env if not e.startswith("ACR_SETTINGS_DIR=")]
    extra = " ".join(f"export {shlex.quote(e)};" for e in a.env)
    build = "" if a.no_build else "cargo build --release -q -p acr-game || exit 3; "
    script = (
        f"{layer}"
        "export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER=sccache SCCACHE_DIR=$HOME/.cache/sccache "
        f"CARGO_TARGET_DIR=$HOME/target/{name}; cd ~/wt/{name} && {build}"
        f'export ACR_GAME_DIR="$HOME/game/American Conquest" ACR_MODELS_DIR={rout}/models '
        f"ACR_SETTINGS_DIR={settings_dir} ACR_VOLUME=0 ACR_LOG={rout}/game.log "
        "VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json; "
        f"{extra} cd {rout} && DISPLAY=:5 timeout {a.timeout} "
        f"$CARGO_TARGET_DIR/release/AmericanConquest --drive --drive-gpu --size {a.size} "
        f"< {rout}/steps.jsonl > {rout}/answers.jsonl 2>/dev/null; "
        f"rm -rf {rout}/models; grep -m2 'settings:' {rout}/game.log | sed 's/^/vm_drive: game /' >&2; "
        f"cat {rout}/answers.jsonl | cut -c1-300"
    )
    subprocess.run(vm_cargo.SSH + [f"mkdir -p {rout} && cat > {rout}/steps.jsonl"], input=req.encode(), check=True)
    r = subprocess.run(vm_cargo.SSH + ["bash -lc " + shlex.quote(script)], stdin=subprocess.DEVNULL)
    vm_cargo.get(name, [f"{rout}={a.out}"])
    subprocess.run(vm_cargo.SSH + [f"rm -rf {rout}"], capture_output=True)
    return r.returncode


if __name__ == "__main__":
    sys.exit(main())
