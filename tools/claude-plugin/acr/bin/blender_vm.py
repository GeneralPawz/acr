"""Runs a Blender asset build (`xtask assets`) on the render VM instead of this PC.

Set `BLENDER` to `blender-vm.cmd` (next to this file). xtask calls it like Blender:
    -b --factory-startup --python <root>/assets/blender/build.py -- <script> --out <glb>
    [--compress X] [--ref <ref.json>] [--refs <dir>] [--blend <file>]
The repo's `assets/` and the reference folder go to the VM (tar over ssh), Blender runs
there on the RX 9060 XT, the outputs (glb, stats, blend) come back. Falls back to local
Blender when the VM is unreachable (env BLENDER_LOCAL, default the 5.2 install). On the VM
itself (hostname `claude-render` or `ACR_VM_LOCAL=1`) it simply runs /opt/blender/blender.
"""

import io
import os
import socket
import subprocess
import sys
import tarfile

HOST = "claude@192.168.178.130"
KEY = os.path.expanduser("~/.ssh/claude-render")
SSH = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "IdentitiesOnly=yes", "-i", KEY, HOST]
LOCAL = os.environ.get("BLENDER_LOCAL", r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe")
REMOTE_BLENDER = "/opt/blender/blender"
# Bought/downloaded model sources (not in the repo, #287): copied once to the VM; asset
# scripts find them through ACR_SOURCES (locally e.g. D:\ACR-models\extracted).
REMOTE_SOURCES = os.environ.get("ACR_SOURCES_VM", "/home/claude/sources")


def norm(p):
    return os.path.normcase(os.path.abspath(p))


def main(args):
    if os.environ.get("ACR_VM_LOCAL") == "1" or (os.name != "nt" and socket.gethostname() == "claude-render"):
        # Already on the VM: Blender runs here, no staging needed.
        env = dict(os.environ)
        env.setdefault("ACR_SOURCES", REMOTE_SOURCES)
        return subprocess.run([REMOTE_BLENDER] + args, env=env).returncode
    build = args[args.index("--python") + 1]
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(build))))
    job = "acr-" + format(abs(hash(norm(root))) % 10**8, "08d") + "-" + str(os.getpid())
    rroot = f"/home/claude/jobs/{job}"

    def remote(p):
        rel = os.path.relpath(os.path.abspath(p), root)
        if rel.startswith(".."):
            raise SystemExit(f"blender-vm: {p} is outside {root}")
        return rroot + "/" + rel.replace("\\", "/")

    # Inputs: assets/ and any --ref/--refs folders; outputs: --out, its stats, --blend.
    inputs = [os.path.join(root, "assets")]
    outputs = []
    out_args = list(args)
    path_flags = {"--python", "--out", "--ref", "--refs", "--blend"}
    for i, a in enumerate(args):
        if a in path_flags and i + 1 < len(args):
            p = args[i + 1]
            out_args[i + 1] = remote(p)
            if a == "--refs":
                inputs.append(p)
            elif a == "--out":
                outputs += [p, os.path.splitext(p)[0] + ".stats.json"]
            elif a == "--blend":
                outputs.append(p)
    # The asset script right after "--".
    if "--" in args:
        k = args.index("--") + 1
        out_args[k] = remote(args[k])
    if subprocess.run(SSH + ["true"], capture_output=True).returncode != 0:
        print("blender-vm: VM unreachable, building locally", file=sys.stderr)
        return subprocess.run([LOCAL] + args).returncode

    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        for p in inputs:
            if os.path.exists(p):
                tar.add(p, arcname=os.path.relpath(p, root).replace("\\", "/"),
                        filter=lambda t: None if "__pycache__" in t.name or t.name.endswith(".wav") or t.name.endswith(".ogg") else t)
    dirs = sorted({os.path.dirname(remote(o)) for o in outputs})
    q = lambda s: "'" + s.replace("'", "'\\''") + "'"
    cmd = (f"mkdir -p {rroot} {' '.join(map(q, dirs))} && tar xzf - -C {rroot} && "
           f"ACR_SOURCES={q(REMOTE_SOURCES)} {REMOTE_BLENDER} {' '.join(map(q, out_args))}")
    r = subprocess.run(SSH + [cmd], input=buf.getvalue())
    if r.returncode == 0:
        # Fetch the outputs back as one tar.
        rels = [os.path.relpath(o, root).replace("\\", "/") for o in outputs]
        fetch = f"cd {rroot} && tar czf - $(for f in {' '.join(map(q, rels))}; do [ -e \"$f\" ] && echo \"$f\"; done)"
        data = subprocess.run(SSH + [fetch], capture_output=True).stdout
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
            for m in tar.getmembers():
                if m.isfile():
                    dest = os.path.join(root, *m.name.split("/"))
                    os.makedirs(os.path.dirname(dest), exist_ok=True)
                    with open(dest, "wb") as f:
                        f.write(tar.extractfile(m).read())
    subprocess.run(SSH + [f"rm -rf {rroot}"], capture_output=True)
    return r.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
