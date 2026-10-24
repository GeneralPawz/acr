"""MCP server `acr-studio`: a live, headless ACR game on the render VM's GPU.

One session is one game process (one field), started from a worktree's release build on the
VM and kept running between tool calls, so an agent can pose, frame, look and adjust without
restarting the game or writing step files. Screenshots come back trimmed to their content
and scaled down, as images in the tool result.

Tools: open_session, send, shot, fetch_image, close_session. The driver requests are the
game's (docs/testing.md; skill acr-driver), e.g. {"cmd":"studio",...}, {"cmd":"pose",...},
{"cmd":"camera","frame":[0],"view":"side"}, {"cmd":"measure","id":0}.

Nothing here blocks without a limit: sync + build 10 min, start 2 min, each request 60 s,
other ssh helpers 60 s (env ACR_STUDIO_BUILD_TIMEOUT / _START_TIMEOUT / _SEND_TIMEOUT). A
timeout returns an error with the game log tail and tears the session down. The remote game
writes its PID to <run folder>/pid; it is killed by that PID (never by a pattern).
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

BUILD_TIMEOUT = int(os.environ.get("ACR_STUDIO_BUILD_TIMEOUT", 600))   # sync + cargo build
START_TIMEOUT = int(os.environ.get("ACR_STUDIO_START_TIMEOUT", 120))   # until the ready line
SEND_TIMEOUT = int(os.environ.get("ACR_STUDIO_SEND_TIMEOUT", 60))      # per request
SSH_TIMEOUT = 60                                                        # short ssh helpers


class StudioError(RuntimeError):
    pass


def _ssh(cmd, timeout=SSH_TIMEOUT):
    """One short ssh command, bounded by `timeout`. Returns stdout text ('' on failure)."""
    try:
        r = subprocess.run(vm_cargo.SSH + [cmd], capture_output=True, text=True, errors="replace",
                           timeout=timeout)
        return r.stdout
    except (subprocess.TimeoutExpired, OSError):
        return ""


def _kill_remote(rout):
    """Kills the game recorded in <rout>/pid (only while that PID is still an AmericanConquest
    process) and removes the run folder. By PID only, no pattern match."""
    q = shlex.quote(rout)
    _ssh(f'p=$(cat {q}/pid 2>/dev/null); if [ -n "$p" ] && tr "\\0" " " < /proc/$p/cmdline 2>/dev/null '
         f'| grep -q AmericanConquest; then kill $p; for i in 1 2 3 4 5 6; do kill -0 $p 2>/dev/null '
         f'|| break; sleep 0.5; done; kill -9 $p 2>/dev/null; fi; rm -rf {q}', timeout=30)


def _log_tail(rout, n=25):
    q = shlex.quote(rout)
    return _ssh(f"tail -n {n} {q}/game.log {q}/stderr.log 2>&1").strip() or "(no log)"


class Session:
    def __init__(self, name, rout, proc):
        self.name, self.rout, self.proc = name, rout, proc
        self.lines = queue.Queue()
        self.n = 0
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        try:
            for line in self.proc.stdout:
                self.lines.put(line)
        except Exception:
            pass
        self.lines.put(None)

    def read_line(self, timeout):
        while True:
            try:
                line = self.lines.get(timeout=timeout)
            except queue.Empty:
                raise StudioError(f"no answer within {timeout} s")
            if line is None:
                raise StudioError("the game ended")
            if line.strip().startswith("{"):
                try:
                    return json.loads(line)
                except ValueError:
                    pass  # a stray non-answer line; keep reading

    def request(self, req, timeout=SEND_TIMEOUT):
        try:
            self.proc.stdin.write(json.dumps(req) + "\n")
            self.proc.stdin.flush()
        except OSError:
            raise StudioError("the game ended (write failed)")
        return self.read_line(timeout)

    def kill(self):
        """Ends the local ssh and the remote game (by PID) and removes the run folder."""
        try:
            self.proc.kill()
        except OSError:
            pass
        _kill_remote(self.rout)


def _fail(s, sid, what):
    """A session that timed out or died is unusable (a late answer would desync it): tear it
    down and return an error with the game log tail."""
    tail = _log_tail(s.rout)
    s.kill()
    SESSIONS.pop(sid, None)
    return f"ERROR: {what}; session {sid} was closed and its game killed. Game log tail:\n{tail}"


def _reap_stale(name):
    """Kills and removes earlier studio runs of this worktree: sessions of this server and
    leftover run folders (with a pid file) of servers that died."""
    for sid, s in list(SESSIONS.items()):
        if s.name == name:
            s.kill()
            SESSIONS.pop(sid, None)
    for rout in _ssh(f"ls -d /home/claude/runs/{name}-studio-* 2>/dev/null").split():
        _kill_remote(rout)


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
    try:
        data = subprocess.run(vm_cargo.SSH + [f"cat {shlex.quote(remote)}"], capture_output=True,
                              timeout=SSH_TIMEOUT).stdout
    except subprocess.TimeoutExpired:
        data = b""
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
    ready line. An earlier session of the same worktree is closed first. Timeouts: sync+build
    10 min, start 2 min; a timeout returns an error with the game log tail. Next:
    send([{"cmd":"studio",...}]) or a blank start."""
    wt = subprocess.run(["git", "-C", worktree, "rev-parse", "--show-toplevel"],
                        capture_output=True, text=True, check=True, timeout=30).stdout.strip()
    name = os.path.basename(wt)
    sid = uuid.uuid4().hex[:8]
    rout = f"/home/claude/runs/{name}-studio-{sid}"
    try:
        _reap_stale(name)
        vm_drive.sync_dir(os.path.join(vm_drive.SHOWCASE, "models"), "~/showcase/models", BUILD_TIMEOUT)
        vm_drive.sync_dir(os.path.join(vm_drive.SHOWCASE, "settings"), "~/showcase/settings", BUILD_TIMEOUT)
        layer = f"mkdir -p {rout}/models && cp -s ~/showcase/models/* {rout}/models/ 2>/dev/null; "
        if models.startswith("wt:"):
            layer += f"cp -f ~/wt/{name}/{models[3:]}/*.glb {rout}/models/ 2>/dev/null; "
        elif models:
            vm_drive.sync_dir(models, f"~/overrides/{name}", BUILD_TIMEOUT)
            layer += f"cp -f ~/overrides/{name}/*.glb {rout}/models/ 2>/dev/null; "
        if build:
            vm_cargo.sync(wt, name, BUILD_TIMEOUT)
            b = subprocess.run(vm_cargo.SSH + ["bash -lc " + shlex.quote(
                f"set -o pipefail; export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER=sccache "
                f"SCCACHE_DIR=$HOME/.cache/sccache CARGO_TARGET_DIR=$HOME/target/{name}; cd ~/wt/{name} && "
                f"timeout {max(BUILD_TIMEOUT - 30, 10)} cargo build --release -q -p acr-game 2>&1 | tail -20")],
                capture_output=True, text=True, errors="replace", timeout=BUILD_TIMEOUT)
            if b.returncode != 0 or "error" in b.stdout:
                return "build failed:\n" + b.stdout[-3000:]
    except subprocess.TimeoutExpired as e:
        return f"ERROR: timed out after {e.timeout:.0f} s while syncing or building (limit {BUILD_TIMEOUT} s)"
    except (SystemExit, subprocess.CalledProcessError, OSError) as e:
        return f"ERROR: sync or build step failed: {e}"
    script = (f"{layer} cd {rout} && echo $$ > pid && export ACR_GAME_DIR=\"$HOME/game/American Conquest\" "
              f"ACR_MODELS_DIR={rout}/models ACR_SETTINGS_DIR=$HOME/showcase/settings ACR_VOLUME=0 "
              f"ACR_LOG={rout}/game.log VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json DISPLAY=:5 && "
              f"exec $HOME/target/{name}/release/AmericanConquest --drive --drive-gpu --size {size} "
              f"2>>{rout}/stderr.log")
    proc = subprocess.Popen(vm_cargo.SSH + ["bash -lc " + shlex.quote(script)], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                            encoding="utf-8", errors="replace")
    s = Session(name, rout, proc)
    SESSIONS[sid] = s
    try:
        ready = s.read_line(START_TIMEOUT)
    except StudioError as e:
        return _fail(s, sid, f"the game did not start ({e}; is the build there? try build=true)")
    return f"session {sid} ({name}); ready: {_short(ready, 400)}"


@mcp.tool()
def send(session_id: str, requests: list[dict], timeout: int = SEND_TIMEOUT) -> str:
    """Send driver requests in order (one JSON object each, as in docs/testing.md) and return
    one short answer line per request. A screenshot `path` that is relative lands in the
    session's run folder on the VM; use shot() to see one instead. timeout: seconds to wait for
    each answer (default 60). On a timeout the session is closed, its game killed, and an error
    with the game log tail comes back."""
    s = _get(session_id)
    out = []
    for req in requests:
        if isinstance(req.get("path"), str) and not req["path"].startswith("/"):
            req["path"] = f"{s.rout}/{req['path']}"
        try:
            out.append(_short(s.request(req, timeout)))
        except StudioError as e:
            out.append(_fail(s, session_id, f"request {json.dumps(req)[:200]}: {e}"))
            break
    return "\n".join(out)


@mcp.tool()
def shot(session_id: str, camera: dict | None = None, max_px: int = 640, trim: bool = True, save_to: str = ""):
    """Take a screenshot now and return it as an image: trimmed to the content (everything that
    differs from the corner colour, best on a white field) and scaled to at most max_px.
    camera: an optional camera request's fields to apply first (e.g. {"frame":[0],"view":"side",
    "part":"hands"}). save_to: an absolute local path for the full-size PNG."""
    s = _get(session_id)
    try:
        if camera:
            a = s.request({"cmd": "camera", **camera})
            if not a.get("ok"):
                return _short(a)
        s.n += 1
        remote = f"{s.rout}/shot{s.n:03d}.png"
        a = s.request({"cmd": "screenshot", "path": remote})
    except StudioError as e:
        return _fail(s, session_id, f"shot: {e}")
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
    """End the game (killed by its recorded PID) and delete its run folder on the VM."""
    s = SESSIONS.pop(session_id, None)
    if not s:
        return "no such session"
    try:
        s.request({"cmd": "quit"}, timeout=5)
    except Exception:
        pass
    s.kill()
    return f"closed {session_id}"


if __name__ == "__main__":
    mcp.run()
