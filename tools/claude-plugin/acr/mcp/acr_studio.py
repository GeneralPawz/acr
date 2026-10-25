"""MCP server `acr-studio`: a live, headless ACR game on the render VM's GPU.

One session is one game process (one field), started from a worktree's release build on the
VM and kept running between tool calls, so an agent can pose, frame, look and adjust without
restarting the game or writing step files. Screenshots come back trimmed to their content
and scaled down, as images in the tool result.

Tools: open_session, send, shot, fetch_image, close_session. The driver requests are the
game's (docs/testing.md; skill acr-driver), e.g. {"cmd":"studio",...}, {"cmd":"pose",...},
{"cmd":"camera","frame":[0],"view":"side"}, {"cmd":"measure","id":0}.

This file is only the MCP shim; the work is in studio_core.py, reloaded whenever that file
(or a bin/ helper) changes, and the open sessions in studio_state.py. Keep this file small.

Why it hung (and what keeps it from hanging):
  * Claude Code runs ONE server process for the main session and all its subagents, for days.
    The 2026-10-24 fix (0af5201) never ran: the server had started the day before and kept the
    old code, whose synchronous tools blocked the event loop; one `git rev-parse` stuck on the
    inherited MCP pipe then wedged every later call of every agent (each hit the client's
    1800 s idle limit). Now studio_core reloads itself, and every answer warns when this shim
    itself is outdated (only a /mcp reconnect replaces it).
  * Each tool call runs in a thread of its own with a hard deadline here; never in anyio's
    shared worker pool, which also carries the server's own stdin reads and stdout writes (a
    pool full of stuck calls would silence the server). A call past its deadline is answered
    with an error and its session torn down; the stuck thread is left behind as a daemon.
  * Children never inherit the server's stdin (the JSON-RPC pipe; a child touching it while
    the reader has a read pending hangs on Windows), subprocess waits are bounded
    (vm_cargo.bounded), and the worktree root is found without git.
  * A watchdog thread logs every thread's stack when the event loop stalls for 20 s.
Log: %LOCALAPPDATA%/acr-studio/server.log (phases, timings, reaps, stalls; shared by servers).

Limits: open_session ACR_STUDIO_OPEN_TIMEOUT (default 270 s) plus 15 s grace, with progress
every 5 s; the game's ready line within ACR_STUDIO_START_TIMEOUT (90 s); each request
ACR_STUDIO_SEND_TIMEOUT (60 s). Stale sessions: this server's idle ones (ACR_STUDIO_IDLE_REAP,
1800 s) and, on the VM, studio games whose run folder's `alive` file (touched every minute
by the owning server) is older than that plus 10 min, are killed at the next open_session.
"""

import asyncio
import importlib
import os
import subprocess
import sys
import threading
import time
import traceback
import uuid


class _NoStdinPopen(subprocess.Popen):
    """Children never inherit the server's stdin (the MCP pipe); see the module docstring."""

    def __init__(self, *args, **kwargs):
        if kwargs.get("stdin") is None:
            kwargs["stdin"] = subprocess.DEVNULL
        super().__init__(*args, **kwargs)


subprocess.Popen = _NoStdinPopen  # also covers subprocess.run inside vm_cargo / vm_drive

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "bin"))
sys.path.insert(0, HERE)

import anyio  # noqa: E402
from mcp.server.fastmcp import Context, FastMCP, Image  # noqa: E402

import studio_state as S  # noqa: E402
import studio_core  # noqa: E402

GRACE = 15
SHIM_MTIME = os.path.getmtime(os.path.abspath(__file__))
_RELOADED = ("shots", "vm_cargo", "vm_drive", "studio_core")   # in dependency order
_CORE = {"mod": studio_core, "mtimes": None}
_RELOAD_LOCK = threading.Lock()

mcp = FastMCP("acr-studio", instructions=(
    "Live headless ACR game on the render VM GPU. open_session(worktree) builds that worktree's "
    "game on the VM and starts it; send(requests) drives it (driver JSON, skill acr-driver; start "
    "with {\"cmd\":\"studio\",...}); shot() returns a trimmed screenshot; close_session when done. "
    "One session = one field. open_session returns ready or an ERROR within 5 min; after one "
    "failed open_session use acr-vm-drive instead of retrying."))


def _mtimes():
    out = []
    for n in _RELOADED:
        try:
            out.append(os.path.getmtime(sys.modules[n].__file__))
        except (OSError, KeyError, AttributeError, TypeError):
            out.append(0)
    return out


def core():
    """studio_core (and the bin/ helpers it uses), reloaded when one of their files changed."""
    with _RELOAD_LOCK:
        now = _mtimes()
        if _CORE["mtimes"] is None:
            _CORE["mtimes"] = now
        elif now != _CORE["mtimes"]:
            try:
                for n in _RELOADED:
                    importlib.reload(sys.modules[n])
                _CORE["mod"] = sys.modules["studio_core"]
                S.log("reloaded studio_core and helpers (files changed)")
            except Exception:  # noqa: BLE001 - keep serving with the old code
                S.log("reload failed, keeping the old code:\n" + traceback.format_exc())
            _CORE["mtimes"] = _mtimes()
        return _CORE["mod"]


def _note():
    try:
        if os.path.getmtime(os.path.abspath(__file__)) != SHIM_MTIME:
            return ("\nNOTE: the acr-studio server runs an outdated acr_studio.py; ask the user to "
                    "reconnect plugin:acr:acr-studio with /mcp.")
    except OSError:
        pass
    return ""


# The watchdog: the event loop beats every second; a beat older than STALL s means something
# blocks the loop, and every thread's stack goes to the log once per stall.
STALL = 20
_BEAT = [None]


async def _beat():
    while True:
        _BEAT[0] = time.monotonic()
        await anyio.sleep(1)


def _watchdog():
    reported = False
    while True:
        time.sleep(5)
        if _BEAT[0] is None:
            continue
        late = time.monotonic() - _BEAT[0]
        if late > STALL and not reported:
            dump = "\n".join(f"--- thread {tid}\n" + "".join(traceback.format_stack(f))
                             for tid, f in sys._current_frames().items())
            S.log(f"WATCHDOG: event loop stalled {late:.0f} s; thread stacks:\n{dump}")
            reported = True
        elif late <= STALL:
            reported = False


threading.Thread(target=_watchdog, daemon=True).start()


async def _bounded(ctx, fn, limit, phase=lambda: "working", on_timeout=None):
    """Runs fn() in a daemon thread of its own and returns its result, reporting progress
    every 5 s. Past `limit` seconds it raises TimeoutError and calls on_timeout (in another
    thread); the stuck thread is left behind."""
    if _BEAT[0] is None:
        _BEAT[0] = time.monotonic()
        asyncio.get_running_loop().create_task(_beat())
    box, done = {}, threading.Event()

    def work():
        try:
            box["ok"] = fn()
        except BaseException as e:  # noqa: BLE001 - handed to the async side
            box["err"] = e
        done.set()

    threading.Thread(target=work, daemon=True).start()
    t0, last = time.monotonic(), -10.0
    while not done.is_set():
        await anyio.sleep(0.2)
        el = time.monotonic() - t0
        if ctx is not None and el - last >= 5:
            last = el
            try:
                with anyio.fail_after(5):
                    await ctx.report_progress(round(el), limit, f"{phase()} ({studio_core.mmss(el)} "
                                                                f"of max {studio_core.mmss(limit)})")
            except Exception:  # noqa: BLE001
                pass
        if el > limit:
            S.log(f"hard deadline {limit:.0f} s passed during: {phase()}")
            if on_timeout:
                threading.Thread(target=on_timeout, daemon=True).start()
            raise TimeoutError(f"no result within {limit:.0f} s (during: {phase()})")
    if "err" in box:
        raise box["err"]
    return box["ok"]


@mcp.tool()
async def open_session(worktree: str, ctx: Context, build: bool = True, models: str = "",
                       size: str = "1600x900", settings: str = "") -> str:
    """Start a live game from `worktree` (absolute path of an ACR worktree) on the VM GPU.
    build: sync and build the worktree's release game on the VM first (incremental: well under
    a minute; a fresh worktree can take several; false reuses the last build). models: an
    absolute local folder of .glb files, or 'wt:target/assets/units', laid over the showcase
    models. settings: 'k=v,k=v' over the showcase settings.txt (e.g. 'models=all,shadows=off').
    Returns the session id, the ready line and the settings line the game logged, or an ERROR
    with the reason and the log tail within 5 min in total (progress is reported meanwhile).
    After one ERROR, use acr-vm-drive instead of retrying. An earlier session of the same
    worktree is closed first. Next: send([{"cmd":"studio",...}]) or a blank start."""
    c = core()
    sid = uuid.uuid4().hex[:8]
    job = c.Job(c.OPEN_TIMEOUT)

    def give_up():
        job.cancelled = True
        c.abandon(sid)

    try:
        msg = await _bounded(ctx, lambda: c.open_session(job, worktree, build, models, size, sid, settings),
                             job.total + GRACE, lambda: job.phase, give_up)
    except (c.StudioError, TimeoutError) as e:
        msg = str(e)
        if not msg.startswith("ERROR"):
            msg = f"ERROR: open_session failed after {c.mmss(job.elapsed())}: {msg}"
        S.log(f"open_session {sid}: {msg.splitlines()[0]}")
        return f"{msg}\n{c.FALLBACK}{_note()}"
    except Exception as e:  # noqa: BLE001 - always answer
        S.log(f"open_session {sid} crashed:\n{traceback.format_exc()}")
        return f"ERROR: open_session crashed: {e!r}\n{c.FALLBACK}{_note()}"
    return msg + _note()


@mcp.tool()
async def send(session_id: str, requests: list[dict], ctx: Context, timeout: int = 60) -> str:
    """Send driver requests in order (one JSON object each, as in docs/testing.md) and return
    one short answer line per request. A screenshot `path` that is relative lands in the
    session's run folder on the VM; use shot() to see one instead. timeout: seconds to wait for
    each answer (default 60). On a timeout the session is closed, its game killed, and an error
    with the game log tail comes back."""
    c = core()
    limit = min(timeout * max(1, len(requests)), 900) + GRACE
    try:
        return await _bounded(ctx, lambda: c.send(session_id, requests, timeout), limit,
                              lambda: "driver requests", lambda: c.abandon(session_id)) + _note()
    except (c.StudioError, TimeoutError) as e:
        return f"ERROR: {e}{_note()}"


@mcp.tool()
async def shot(session_id: str, ctx: Context, camera: dict | None = None, max_px: int = 640,
               trim: bool = True, save_to: str = ""):
    """Take a screenshot now and return it as an image: trimmed to the content (everything that
    differs from the corner colour, best on a white field) and scaled to at most max_px.
    camera: an optional camera request's fields to apply first (e.g. {"frame":[0],"view":"side",
    "part":"hands"}). save_to: an absolute local path for the full-size PNG."""
    c = core()
    limit = 2 * c.SEND_TIMEOUT + c.SSH_TIMEOUT + GRACE
    try:
        out = await _bounded(ctx, lambda: c.shot(session_id, camera, max_px, trim, save_to), limit,
                             lambda: "screenshot", lambda: c.abandon(session_id))
    except (c.StudioError, TimeoutError) as e:
        return f"ERROR: {e}{_note()}"
    return Image(data=out, format="png") if isinstance(out, bytes) else out + _note()


@mcp.tool()
async def fetch_image(session_id: str, path: str, ctx: Context, max_px: int = 900, trim: bool = False,
                      save_to: str = ""):
    """Return an image the game wrote in this session (e.g. a studio `sheet` path), scaled to
    at most max_px; save_to keeps the full file locally."""
    c = core()
    try:
        out = await _bounded(ctx, lambda: c.fetch_image(session_id, path, max_px, trim, save_to),
                             c.SSH_TIMEOUT + GRACE, lambda: "fetching the image")
    except (c.StudioError, TimeoutError) as e:
        return f"ERROR: {e}{_note()}"
    return Image(data=out, format="png")


@mcp.tool()
async def close_session(session_id: str, ctx: Context) -> str:
    """End the game (killed by its recorded PID) and delete its run folder on the VM."""
    c = core()
    try:
        return await _bounded(ctx, lambda: c.close(session_id), 60, lambda: "closing") + _note()
    except TimeoutError as e:
        return f"ERROR: {e}; the game is reaped at a later open_session.{_note()}"


if __name__ == "__main__":
    S.log(f"server start, python {sys.version.split()[0]}, {__file__}")
    mcp.run()
