"""MCP server `acr-studio`: a live, headless ACR game on the render VM's GPU.

One session is one game process (one field), started from a worktree's release build on the
VM and kept running between tool calls, so an agent can pose, frame, look and adjust without
restarting the game or writing step files. Screenshots come back trimmed to their content
and scaled down, as images in the tool result.

Tools: open_session, send, shot, fetch_image, close_session. The driver requests are the
game's (docs/testing.md; skill acr-driver), e.g. {"cmd":"studio",...}, {"cmd":"pose",...},
{"cmd":"camera","frame":[0],"view":"side"}, {"cmd":"measure","id":0}.

Why it used to hang (fixed here):
  * Claude Code runs ONE server process for the main session and all its subagents. The tools
    were plain functions, which FastMCP calls on its event loop: one agent's build blocked every
    other agent's call (and even the delivery of finished answers) until it ended. Tools are now
    async and do their blocking work in threads, so sessions run in parallel.
  * Child processes inherited the server's stdin, the JSON-RPC pipe. On Windows a child that
    touches an inherited pipe while the server's reader thread has a read pending on it hangs
    until the next message arrives (git rev-parse sat for 30+ min; a subprocess timeout does not
    help because the git.exe wrapper's child keeps the output pipe open), and ssh could even eat
    protocol bytes. Every child now gets stdin=DEVNULL unless it is fed explicitly.

Limits: open_session as a whole (sync + build + start) ACR_STUDIO_OPEN_TIMEOUT, default 360 s,
with progress notifications every few seconds; the game must print its ready line within
ACR_STUDIO_START_TIMEOUT (90 s); each request ACR_STUDIO_SEND_TIMEOUT (60 s). The build runs
detached on the VM (its own log per call): when the limit is hit it keeps going there and the
error says so. A timed-out session is torn down (its game killed by the PID in <run>/pid).
Sessions idle for ACR_STUDIO_IDLE_REAP (1800 s) are closed when another session opens.
"""

import io
import json
import os
import queue
import shlex
import subprocess
import sys
import threading
import time
import uuid


class _NoStdinPopen(subprocess.Popen):
    """Children never inherit the server's stdin (the MCP pipe); see the module docstring."""

    def __init__(self, *args, **kwargs):
        if kwargs.get("stdin") is None:
            kwargs["stdin"] = subprocess.DEVNULL
        super().__init__(*args, **kwargs)


subprocess.Popen = _NoStdinPopen  # also covers subprocess.run inside vm_cargo / vm_drive

import anyio  # noqa: E402
from mcp.server.fastmcp import Context, FastMCP, Image  # noqa: E402
from PIL import Image as PILImage  # noqa: E402

BIN = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "bin")
sys.path.insert(0, BIN)
import shots  # noqa: E402
import vm_cargo  # noqa: E402
import vm_drive  # noqa: E402

mcp = FastMCP("acr-studio", instructions=(
    "Live headless ACR game on the render VM GPU. open_session(worktree) builds that worktree's "
    "game on the VM and starts it; send(requests) drives it (driver JSON, skill acr-driver; start "
    "with {\"cmd\":\"studio\",...}); shot() returns a trimmed screenshot; close_session when done. "
    "One session = one field. open_session returns ready or an ERROR within 6 min; after one "
    "failed open_session use acr-vm-drive instead of retrying."))

SESSIONS = {}
SESSIONS_LOCK = threading.Lock()
SHOWCASE_LOCK = threading.Lock()   # one showcase sync at a time (parallel opens share ~/showcase)

OPEN_TIMEOUT = int(os.environ.get("ACR_STUDIO_OPEN_TIMEOUT", 360))     # whole open_session
START_TIMEOUT = int(os.environ.get("ACR_STUDIO_START_TIMEOUT", 90))    # until the ready line
SEND_TIMEOUT = int(os.environ.get("ACR_STUDIO_SEND_TIMEOUT", 60))      # per request
IDLE_REAP = int(os.environ.get("ACR_STUDIO_IDLE_REAP", 1800))          # idle sessions closed
SSH_TIMEOUT = 60                                                        # short ssh helpers
FALLBACK = ("Fall back to acr-vm-drive now (skill acr-driver; same driver requests as a steps "
            "file) instead of retrying open_session.")


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


def _mmss(s):
    s = max(0, int(s))
    return f"{s // 60}:{s % 60:02d}"


class Job:
    """The clock and current phase of one open_session; the async side reports it as progress."""

    def __init__(self, total):
        self.t0 = time.monotonic()
        self.total = total
        self.phase = "starting"
        self.cancelled = False

    def elapsed(self):
        return time.monotonic() - self.t0

    def left(self):
        return self.total - self.elapsed()

    def step(self, phase):
        if self.cancelled or self.left() <= 0:
            raise StudioError(f"time limit {self.total} s reached during: {self.phase}")
        self.phase = phase

    def limit(self, cap):
        return max(1.0, min(cap, self.left()))


class Session:
    def __init__(self, name, rout, proc):
        self.name, self.rout, self.proc = name, rout, proc
        self.lines = queue.Queue()
        self.n = 0
        self.lock = threading.Lock()   # one request at a time per game
        self.last_used = time.monotonic()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        try:
            for line in self.proc.stdout:
                self.lines.put(line)
        except Exception:
            pass
        self.lines.put(None)

    def read_line(self, timeout):
        end = time.monotonic() + timeout
        while True:
            try:
                line = self.lines.get(timeout=max(0.05, end - time.monotonic()))
            except queue.Empty:
                raise StudioError(f"no answer within {timeout:.0f} s")
            if line is None:
                raise StudioError("the game ended")
            if line.strip().startswith("{"):
                try:
                    return json.loads(line)
                except ValueError:
                    pass  # a stray non-answer line; keep reading

    def request(self, req, timeout=SEND_TIMEOUT):
        with self.lock:
            self.last_used = time.monotonic()
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
    with SESSIONS_LOCK:
        SESSIONS.pop(sid, None)
    return f"ERROR: {what}; session {sid} was closed and its game killed. Game log tail:\n{tail}"


def _reap_stale(name):
    """Kills earlier studio runs of this worktree (sessions of this server and leftover run
    folders of servers that died) and sessions of any worktree idle for IDLE_REAP seconds
    (agents that ended without close_session)."""
    now = time.monotonic()
    with SESSIONS_LOCK:
        doomed = [(sid, s) for sid, s in SESSIONS.items()
                  if s.name == name or now - s.last_used > IDLE_REAP]
        for sid, _ in doomed:
            SESSIONS.pop(sid, None)
        live = {s.rout for s in SESSIONS.values()}
    for _, s in doomed:
        s.kill()
    for rout in _ssh(f"ls -d /home/claude/runs/{name}-studio-* 2>/dev/null").split():
        if rout not in live:
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


_SHOWCASE_SIG = [None]


def _showcase_sig():
    """Names, sizes and times of the local showcase files: unchanged means no sync needed."""
    sig = []
    for top in ("models", "settings"):
        for root, _, names in os.walk(os.path.join(vm_drive.SHOWCASE, top)):
            for n in names:
                try:
                    st = os.stat(os.path.join(root, n))
                except OSError:
                    continue
                sig.append((root, n, st.st_size, st.st_mtime_ns))
    return hash(tuple(sorted(sig)))


def _sync_showcase(job):
    """One sync at a time (parallel opens share ~/showcase on the VM), skipped when the local
    showcase did not change since this server's last sync."""
    with SHOWCASE_LOCK:
        sig = _showcase_sig()
        if sig != _SHOWCASE_SIG[0]:
            vm_drive.sync_showcase(job.limit(180))
            _SHOWCASE_SIG[0] = sig


BUILD_ENV = ("export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER=sccache SCCACHE_DIR=$HOME/.cache/sccache "
             "CARGO_TARGET_DIR=$HOME/target/{name} CARGO_TERM_COLOR=never")


def _build(job, wt, name, sid):
    """Syncs the worktree and builds its release game on the VM. The build runs detached
    (nohup, own log per call) and is polled, so a time limit here never leaves cargo half
    killed: on the limit it keeps running on the VM and warms the target dir."""
    job.step("syncing the worktree to the VM")
    try:
        vm_cargo.sync(wt, name, job.limit(180))
    except SystemExit as e:
        raise StudioError(str(e))
    base = f"/home/claude/runs/build-{name}-{sid}"
    inner = (f"{BUILD_ENV.format(name=name)}; cd ~/wt/{name} && "
             f"cargo build --release -p acr-game > {base}.log 2>&1; echo $? > {base}.rc")
    _ssh(f"mkdir -p /home/claude/runs && nohup setsid bash -lc {shlex.quote(inner)} "
         f">/dev/null 2>&1 < /dev/null & echo started", timeout=job.limit(30))
    job.step("building")
    while True:
        out = _ssh(f"cat {base}.rc 2>/dev/null; echo '|'; tail -c 400 {base}.log 2>/dev/null",
                   timeout=job.limit(20))
        rc, _, log = out.partition("|")
        rc = rc.strip()
        if rc:
            if rc != "0":
                tail = _ssh(f"grep -E -A6 '^error' {base}.log | head -60; tail -5 {base}.log")
                _ssh(f"rm -f {base}.rc {base}.log")
                raise StudioError(f"the build failed (cargo exit {rc}):\n{tail[-3000:]}")
            _ssh(f"rm -f {base}.rc {base}.log")
            return
        last = [ln.strip() for ln in log.splitlines() if ln.strip()]
        what = last[-1][:80] if last else "waiting for cargo"
        if "Blocking waiting for file lock" in log:
            what = "waiting for another build of this worktree (cargo lock)"
        job.phase = f"building: {what}"
        if job.left() <= 3:
            raise StudioError(
                f"the build was still running after {_mmss(job.elapsed())} ({what}); it keeps "
                f"going on the VM (log {base}.log). A later open_session(build=true) reuses it")
        time.sleep(min(4, max(0.5, job.left() - 3)))


def _open(job, worktree, build, models, size, sid):
    """The blocking part of open_session (runs in a worker thread). Returns (session, ready
    answer) or raises StudioError."""
    job.step("checking the worktree")
    try:
        wt = subprocess.run(["git", "-C", worktree, "rev-parse", "--show-toplevel"], capture_output=True,
                            text=True, check=True, timeout=30).stdout.strip()
    except (subprocess.SubprocessError, OSError) as e:
        raise StudioError(f"not a git worktree: {worktree} ({e})")
    name = os.path.basename(wt)
    rout = f"/home/claude/runs/{name}-studio-{sid}"
    try:
        job.step("closing earlier sessions of this worktree")
        _reap_stale(name)
        job.step("syncing showcase models (waits for parallel opens)")
        _sync_showcase(job)
        layer = f"mkdir -p {rout}/models && cp -rs ~/showcase/models/. {rout}/models/ 2>/dev/null; "
        if models.startswith("wt:"):
            layer += f"cp -f ~/wt/{name}/{models[3:]}/*.glb {rout}/models/ 2>/dev/null; "
        elif models:
            job.step("syncing model overrides")
            vm_drive.sync_dir(models, f"~/overrides/{name}", job.limit(180))
            layer += f"cp -f ~/overrides/{name}/*.glb {rout}/models/ 2>/dev/null; "
        if build:
            _build(job, wt, name, sid)
    except subprocess.TimeoutExpired as e:
        raise StudioError(f"time limit hit during: {job.phase} (ssh step over {e.timeout:.0f} s)")
    except (subprocess.CalledProcessError, OSError) as e:
        raise StudioError(f"{job.phase} failed: {e}")
    job.step("starting the game")
    exe = f"$HOME/target/{name}/release/AmericanConquest"
    nobuild = json.dumps({"ok": False, "error": f"no release build of {name} on the VM; use build=true"})
    script = (f"{layer} cd {rout} && echo $$ > pid && {{ test -x {exe} || {{ echo {shlex.quote(nobuild)}; "
              f"exit 3; }}; }} && export ACR_GAME_DIR=\"$HOME/game/American Conquest\" "
              f"ACR_MODELS_DIR={rout}/models ACR_SETTINGS_DIR=$HOME/showcase/settings ACR_VOLUME=0 "
              f"ACR_LOG={rout}/game.log VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json DISPLAY=:5 && "
              f"exec {exe} --drive --drive-gpu --size {size} 2>>{rout}/stderr.log")
    proc = subprocess.Popen(vm_cargo.SSH + ["bash -lc " + shlex.quote(script)], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                            encoding="utf-8", errors="replace")
    s = Session(name, rout, proc)
    with SESSIONS_LOCK:
        SESSIONS[sid] = s
    job.phase = "waiting for the game's ready line"
    try:
        ready = s.read_line(job.limit(START_TIMEOUT))
    except StudioError as e:
        raise StudioError(_fail(s, sid, f"the game did not start ({e})"))
    if ready.get("ok") is False:
        raise StudioError(_fail(s, sid, ready.get("error", "the game did not start")))
    return s, ready


async def _in_thread(ctx, job, fn):
    """Runs fn() in a worker thread, reporting job.phase as progress every few seconds, and
    returns its result. Gives up (StudioError) shortly after the job's hard limit even if the
    thread is still stuck somewhere."""
    box = {}
    done = threading.Event()

    def work():
        try:
            box["ok"] = fn()
        except BaseException as e:  # noqa: BLE001 - handed to the async side
            box["err"] = e
        done.set()

    threading.Thread(target=work, daemon=True).start()
    last = -10.0
    while not done.is_set():
        await anyio.sleep(0.25)
        el = job.elapsed()
        if el - last >= 5:
            last = el
            try:
                await ctx.report_progress(round(el), job.total,
                                          f"{job.phase} ({_mmss(el)} of max {_mmss(job.total)})")
            except Exception:
                pass
        if el > job.total + 20:
            job.cancelled = True
            raise StudioError(f"hard limit {job.total} s passed during: {job.phase}")
    if "err" in box:
        raise box["err"]
    return box["ok"]


@mcp.tool()
async def open_session(worktree: str, ctx: Context, build: bool = True, models: str = "",
                       size: str = "1600x900") -> str:
    """Start a live game from `worktree` (absolute path of an ACR worktree) on the VM GPU.
    build: sync and build the worktree's release game on the VM first (incremental: well under
    a minute; a fresh worktree can take several; false reuses the last build). models: an
    absolute local folder of .glb files, or 'wt:target/assets/units', laid over the showcase
    models. Returns the session id and the ready line, or an ERROR with the reason within
    6 min in total (progress is reported while it builds). After one ERROR, use acr-vm-drive
    instead of retrying. An earlier session of the same worktree is closed first. Next:
    send([{"cmd":"studio",...}]) or a blank start."""
    sid = uuid.uuid4().hex[:8]
    job = Job(OPEN_TIMEOUT)
    try:
        s, ready = await _in_thread(ctx, job, lambda: _open(job, worktree, build, models, size, sid))
    except StudioError as e:
        msg = str(e)
        if not msg.startswith("ERROR"):
            msg = f"ERROR: open_session failed after {_mmss(job.elapsed())}: {msg}"
        return f"{msg}\n{FALLBACK}"
    return f"session {sid} ({s.name}) ready after {_mmss(job.elapsed())}: {_short(ready, 400)}"


def _send(s, session_id, requests, timeout):
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
async def send(session_id: str, requests: list[dict], timeout: int = SEND_TIMEOUT) -> str:
    """Send driver requests in order (one JSON object each, as in docs/testing.md) and return
    one short answer line per request. A screenshot `path` that is relative lands in the
    session's run folder on the VM; use shot() to see one instead. timeout: seconds to wait for
    each answer (default 60). On a timeout the session is closed, its game killed, and an error
    with the game log tail comes back."""
    s = _get(session_id)
    return await anyio.to_thread.run_sync(_send, s, session_id, requests, timeout)


def _shot(s, session_id, camera, max_px, trim, save_to):
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
async def shot(session_id: str, camera: dict | None = None, max_px: int = 640, trim: bool = True,
               save_to: str = ""):
    """Take a screenshot now and return it as an image: trimmed to the content (everything that
    differs from the corner colour, best on a white field) and scaled to at most max_px.
    camera: an optional camera request's fields to apply first (e.g. {"frame":[0],"view":"side",
    "part":"hands"}). save_to: an absolute local path for the full-size PNG."""
    s = _get(session_id)
    return await anyio.to_thread.run_sync(_shot, s, session_id, camera, max_px, trim, save_to)


@mcp.tool()
async def fetch_image(session_id: str, path: str, max_px: int = 900, trim: bool = False, save_to: str = ""):
    """Return an image the game wrote in this session (e.g. a studio `sheet` path), scaled to
    at most max_px; save_to keeps the full file locally."""
    s = _get(session_id)
    remote = path if path.startswith("/") else f"{s.rout}/{path}"
    data = await anyio.to_thread.run_sync(_fetch, remote)
    return _image(data, max_px, trim, save_to)


def _close(s):
    try:
        s.request({"cmd": "quit"}, timeout=5)
    except Exception:
        pass
    s.kill()


@mcp.tool()
async def close_session(session_id: str) -> str:
    """End the game (killed by its recorded PID) and delete its run folder on the VM."""
    with SESSIONS_LOCK:
        s = SESSIONS.pop(session_id, None)
    if not s:
        return "no such session"
    await anyio.to_thread.run_sync(_close, s)
    return f"closed {session_id}"


if __name__ == "__main__":
    mcp.run()
