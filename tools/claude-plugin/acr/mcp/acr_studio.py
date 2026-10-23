"""MCP server `acr-studio`: a live, headless ACR game on the render VM's GPU.

One session is one game process (one field), started from a worktree's release build on the
VM and kept running between tool calls, so an agent can pose, frame, look and adjust without
restarting the game or writing step files. Screenshots come back trimmed to their content
and scaled down, as images in the tool result.

Tools: open_session, send, shot, fetch_image, close_session. The driver requests are the
game's (docs/testing.md; skill acr-driver), e.g. {"cmd":"studio",...}, {"cmd":"pose",...},
{"cmd":"camera","frame":[0],"view":"side"}, {"cmd":"measure","id":0}.
"""

import io
import json
import os
import queue
import shlex
import subprocess
import sys
import threading
import uuid

from mcp.server.fastmcp import FastMCP, Image
from PIL import Image as PILImage

BIN = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "bin")
sys.path.insert(0, BIN)
import shots  # noqa: E402
import vm_cargo  # noqa: E402
import vm_drive  # noqa: E402

mcp = FastMCP("acr-studio", instructions=(
    "Live headless ACR game on the render VM GPU. open_session(worktree) builds that worktree's "
    "game on the VM and starts it; send(requests) drives it (driver JSON, skill acr-driver; start "
    "with {\"cmd\":\"studio\",...}); shot() returns a trimmed screenshot; close_session when done. "
    "One session = one field."))

SESSIONS = {}


class Session:
    def __init__(self, name, rout, proc):
        self.name, self.rout, self.proc = name, rout, proc
        self.lines = queue.Queue()
        self.n = 0
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def request(self, req, timeout=300):
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()
        line = self.lines.get(timeout=timeout)
        if line is None:
            raise RuntimeError("the game ended")
        return json.loads(line)


def _short(answer, limit=1500):
    answer = dict(answer)
    log = answer.pop("log", None)
    if log:
        answer["log"] = log[-5:]
    text = json.dumps(answer)
    return text if len(text) <= limit else text[:limit] + "...(cut)"


def _get(session_id):
    s = SESSIONS.get(session_id)
    if not s:
        raise ValueError(f"no session {session_id}; open one with open_session")
    return s


def _fetch(remote):
    data = subprocess.run(vm_cargo.SSH + [f"cat {shlex.quote(remote)}"], capture_output=True).stdout
    if not data:
        raise RuntimeError(f"could not fetch {remote}")
    return data


def _image(data, max_px, trim, save_to):
    im = PILImage.open(io.BytesIO(data))
    if trim:
        im = im.crop(shots.content_box(im, 12))
    im = shots.fit(im, max_px)
    buf = io.BytesIO()
    im.convert("RGB").save(buf, "PNG")
    if save_to:
        os.makedirs(os.path.dirname(os.path.abspath(save_to)), exist_ok=True)
        with open(save_to, "wb") as f:
            f.write(data)
    return Image(data=buf.getvalue(), format="png")


@mcp.tool()
def open_session(worktree: str, build: bool = True, models: str = "", size: str = "1600x900") -> str:
    """Start a live game from `worktree` (absolute path of an ACR worktree) on the VM GPU.
    build: sync and build the worktree's release game on the VM first (about 1 min; false
    reuses the last build). models: an absolute local folder of .glb files, or
    'wt:target/assets/units', laid over the showcase models. Returns the session id and the
    ready line. Next: send([{"cmd":"studio",...}]) or a blank start."""
    wt = subprocess.run(["git", "-C", worktree, "rev-parse", "--show-toplevel"],
                        capture_output=True, text=True, check=True).stdout.strip()
    name = os.path.basename(wt)
    sid = uuid.uuid4().hex[:8]
    rout = f"/home/claude/runs/{name}-studio-{sid}"
    vm_drive.sync_dir(os.path.join(vm_drive.SHOWCASE, "models"), "~/showcase/models")
    vm_drive.sync_dir(os.path.join(vm_drive.SHOWCASE, "settings"), "~/showcase/settings")
    layer = f"mkdir -p {rout}/models && cp -s ~/showcase/models/* {rout}/models/ 2>/dev/null; "
    if models.startswith("wt:"):
        layer += f"cp -f ~/wt/{name}/{models[3:]}/*.glb {rout}/models/ 2>/dev/null; "
    elif models:
        vm_drive.sync_dir(models, f"~/overrides/{name}")
        layer += f"cp -f ~/overrides/{name}/*.glb {rout}/models/ 2>/dev/null; "
    if build:
        vm_cargo.sync(wt, name)
        b = subprocess.run(vm_cargo.SSH + ["bash -lc " + shlex.quote(
            f"export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER=sccache SCCACHE_DIR=$HOME/.cache/sccache "
            f"CARGO_TARGET_DIR=$HOME/target/{name}; cd ~/wt/{name} && cargo build --release -q -p acr-game 2>&1 | tail -20")],
            capture_output=True, text=True)
        if "error" in b.stdout:
            return "build failed:\n" + b.stdout[-3000:]
    script = (f"{layer} cd {rout} && export ACR_GAME_DIR=\"$HOME/game/American Conquest\" "
              f"ACR_MODELS_DIR={rout}/models ACR_SETTINGS_DIR=$HOME/showcase/settings ACR_VOLUME=0 "
              f"ACR_LOG={rout}/game.log VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json DISPLAY=:5 && "
              f"exec $HOME/target/{name}/release/AmericanConquest --drive --drive-gpu --size {size}")
    proc = subprocess.Popen(vm_cargo.SSH + ["bash -lc " + shlex.quote(script)], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, encoding="utf-8")
    s = Session(name, rout, proc)
    try:
        ready = s.lines.get(timeout=120)
    except queue.Empty:
        proc.kill()
        return "the game did not start within 120 s"
    if not ready:
        return "the game exited at start (is the build there? try build=true)"
    SESSIONS[sid] = s
    return f"session {sid} ({name}); ready: {_short(json.loads(ready), 400)}"


@mcp.tool()
def send(session_id: str, requests: list[dict]) -> str:
    """Send driver requests in order (one JSON object each, as in docs/testing.md) and return
    one short answer line per request. A screenshot `path` that is relative lands in the
    session's run folder on the VM; use shot() to see one instead."""
    s = _get(session_id)
    out = []
    for req in requests:
        if isinstance(req.get("path"), str) and not req["path"].startswith("/"):
            req["path"] = f"{s.rout}/{req['path']}"
        out.append(_short(s.request(req)))
    return "\n".join(out)


@mcp.tool()
def shot(session_id: str, camera: dict | None = None, max_px: int = 640, trim: bool = True, save_to: str = ""):
    """Take a screenshot now and return it as an image: trimmed to the content (everything that
    differs from the corner colour, best on a white field) and scaled to at most max_px.
    camera: an optional camera request's fields to apply first (e.g. {"frame":[0],"view":"side",
    "part":"hands"}). save_to: an absolute local path for the full-size PNG."""
    s = _get(session_id)
    if camera:
        a = s.request({"cmd": "camera", **camera})
        if not a.get("ok"):
            return _short(a)
    s.n += 1
    remote = f"{s.rout}/shot{s.n:03d}.png"
    a = s.request({"cmd": "screenshot", "path": remote})
    if not a.get("ok"):
        return _short(a)
    return _image(_fetch(remote), max_px, trim, save_to)


@mcp.tool()
def fetch_image(session_id: str, path: str, max_px: int = 900, trim: bool = False, save_to: str = ""):
    """Return an image the game wrote in this session (e.g. a studio `sheet` path), scaled to
    at most max_px; save_to keeps the full file locally."""
    s = _get(session_id)
    remote = path if path.startswith("/") else f"{s.rout}/{path}"
    return _image(_fetch(remote), max_px, trim, save_to)


@mcp.tool()
def close_session(session_id: str) -> str:
    """End the game and delete its run folder on the VM."""
    s = SESSIONS.pop(session_id, None)
    if not s:
        return "no such session"
    try:
        s.request({"cmd": "quit"}, timeout=10)
    except Exception:
        pass
    s.proc.kill()
    subprocess.run(vm_cargo.SSH + [f"rm -rf {s.rout}"], capture_output=True)
    return f"closed {session_id}"


if __name__ == "__main__":
    mcp.run()
