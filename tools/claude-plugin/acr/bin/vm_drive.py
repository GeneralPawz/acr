"""Runs the game's test driver headless on the render VM (RX 9060 XT, Vulkan) and fetches
the screenshots back. Builds the worktree's release game there first.

    acr-vm-drive --steps STEPS.json --out C:/abs/out [--wt DIR] [--name NAME]
                       [--models C:/abs/glbs] [--size 1600x900] [--no-build] [--env K=V ...]

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
"""

import argparse
import hashlib
import io
import json
import os
import shlex
import subprocess
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import vm_cargo  # noqa: E402

SHOWCASE = r"C:\Users\verwalter\WIP\ACR-analysis\showcase"


def sync_dir(local, remote):
    """Sends the files of `local` whose MD5 differs from `remote` (VM path). A size check alone
    kept stale copies of rebuilt glbs that came out with the same byte size."""
    listing = subprocess.run(vm_cargo.SSH + [f"mkdir -p {remote} && cd {remote} && find . -maxdepth 1 -type f "
                                             f"-exec md5sum {{}} +"], capture_output=True, text=True).stdout
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
    subprocess.run(vm_cargo.SSH + [f"tar xf - -C {remote}"], input=buf.getvalue(), check=True)
    return len(send)


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
    ap.add_argument("--timeout", type=int, default=600)
    a = ap.parse_args()
    wt = subprocess.run(["git", "-C", a.wt, "rev-parse", "--show-toplevel"],
                        capture_output=True, text=True, check=True).stdout.strip()
    name = a.name or os.path.basename(wt)
    steps = json.load(open(a.steps, encoding="utf-8"))
    run_id = hashlib.sha1(f"{name}{os.getpid()}".encode()).hexdigest()[:8]
    rout = f"/home/claude/runs/{name}-{run_id}"
    for s in steps:
        if "path" in s and not os.path.isabs(s["path"]) and not s["path"].startswith("/"):
            s["path"] = f"{rout}/{s['path']}"

    n = sync_dir(os.path.join(SHOWCASE, "models"), "~/showcase/models")
    for sub in sorted(os.listdir(os.path.join(SHOWCASE, "models"))):  # e.g. ground/ (texture pack)
        if os.path.isdir(os.path.join(SHOWCASE, "models", sub)):
            n += sync_dir(os.path.join(SHOWCASE, "models", sub), f"~/showcase/models/{sub}")
    sync_dir(os.path.join(SHOWCASE, "settings"), "~/showcase/settings")
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
    extra = " ".join(f"export {shlex.quote(e)};" for e in a.env)
    build = "" if a.no_build else "cargo build --release -q -p acr-game || exit 3; "
    script = (
        f"{layer}"
        "export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER=sccache SCCACHE_DIR=$HOME/.cache/sccache "
        f"CARGO_TARGET_DIR=$HOME/target/{name}; cd ~/wt/{name} && {build}"
        f'export ACR_GAME_DIR="$HOME/game/American Conquest" ACR_MODELS_DIR={rout}/models '
        f"ACR_SETTINGS_DIR=$HOME/showcase/settings ACR_VOLUME=0 ACR_LOG={rout}/game.log "
        "VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json; "
        f"{extra} cd {rout} && DISPLAY=:5 timeout {a.timeout} "
        f"$CARGO_TARGET_DIR/release/AmericanConquest --drive --drive-gpu --size {a.size} "
        f"< {rout}/steps.jsonl > {rout}/answers.jsonl 2>/dev/null; "
        f"rm -rf {rout}/models; cat {rout}/answers.jsonl | cut -c1-300"
    )
    subprocess.run(vm_cargo.SSH + [f"mkdir -p {rout} && cat > {rout}/steps.jsonl"], input=req.encode(), check=True)
    r = subprocess.run(vm_cargo.SSH + ["bash -lc " + shlex.quote(script)], stdin=subprocess.DEVNULL)
    vm_cargo.get(name, [f"{rout}={a.out}"])
    subprocess.run(vm_cargo.SSH + [f"rm -rf {rout}"], capture_output=True)
    return r.returncode


if __name__ == "__main__":
    sys.exit(main())
