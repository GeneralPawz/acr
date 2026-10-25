"""The logic of the acr-studio MCP server (acr_studio.py is only the MCP shim). The shim
reloads this module when the file changes, so a server that has been running for days picks
up fixes without a restart; open sessions live in studio_state and survive the reload.

Every function here is blocking and runs in a worker thread of its own; the shim bounds each
call with a hard deadline, so nothing in here can stall the server's MCP pipe.
"""

import io
import json
import os
import queue
import shlex
import subprocess
import threading
import time

import studio_state as S
import shots
import vm_cargo
import vm_drive

OPEN_TIMEOUT = int(os.environ.get("ACR_STUDIO_OPEN_TIMEOUT", 270))     # whole open_session
START_TIMEOUT = int(os.environ.get("ACR_STUDIO_START_TIMEOUT", 90))    # until the ready line
SEND_TIMEOUT = int(os.environ.get("ACR_STUDIO_SEND_TIMEOUT", 60))      # per request
IDLE_REAP = int(os.environ.get("ACR_STUDIO_IDLE_REAP", 1800))          # idle sessions closed
STALE = IDLE_REAP + 600       # a studio game whose `alive` file is older than this has no owner
KEEPALIVE_EVERY = 60
SSH_TIMEOUT = 60
RUNS = "/home/claude/runs"
FALLBACK = ("Fall back to acr-vm-drive now (skill acr-driver; same driver requests as a steps "
            "file) instead of retrying open_session.")
log = S.log


class StudioError(RuntimeError):
    pass


def ssh(cmd, timeout=SSH_TIMEOUT):
    """One short ssh command, bounded by `timeout`. Returns stdout text ('' on failure)."""
    try:
        r = vm_cargo.bounded(vm_cargo.SSH + [cmd], timeout)
        return r.stdout.decode("utf-8", errors="replace")
    except (subprocess.TimeoutExpired, OSError) as e:
        log(f"ssh failed ({type(e).__name__}): {cmd[:120]}")
        return ""


def kill_remote(rout):
    """Kills the game recorded in <rout>/pid (only while that PID is still an AmericanConquest
    process) and removes the run folder. By PID only, no pattern match."""
    q = shlex.quote(rout)
    ssh(f'p=$(cat {q}/pid 2>/dev/null); if [ -n "$p" ] && tr "\\0" " " < /proc/$p/cmdline 2>/dev/null '
        f'| grep -q AmericanConquest; then kill $p; for i in 1 2 3 4 5 6; do kill -0 $p 2>/dev/null '
        f'|| break; sleep 0.5; done; kill -9 $p 2>/dev/null; fi; rm -rf {q}', timeout=30)


def log_tail(rout, n=25):
    q = shlex.quote(rout)
    return ssh(f"tail -n {n} {q}/game.log {q}/stderr.log 2>&1").strip() or "(no log)"


def vm_load():
    """One line about how busy the VM is (shown while building and in timeout errors)."""
    out = ssh("cut -d' ' -f1-3 /proc/loadavg; pgrep -c -x rustc; pgrep -c -f /opt/blender/blender; "
              "pgrep -c -f 'release/AmericanConquest'", timeout=15).split("\n")
    if len(out) < 4:
        return "VM load unknown"
    return f"VM load {out[0]}, {out[1]} rustc, {out[2]} blender, {out[3]} games"


def mmss(s):
    s = max(0, int(s))
    return f"{s // 60}:{s % 60:02d}"


def worktree_root(path):
    """The worktree folder holding `path` (the first parent with a .git entry). Pure Python:
    a `git rev-parse` child once sat for 30+ minutes on an inherited MCP pipe."""
    p = os.path.abspath(vm_cargo.local_path(path))
    while True:
        if os.path.exists(os.path.join(p, ".git")):
            return p
        up = os.path.dirname(p)
        if up == p:
            raise StudioError(f"not inside a git worktree: {path}")
        p = up


class Job:
    """The clock and current phase of one open_session; the shim reports it as progress."""

    def __init__(self, total=OPEN_TIMEOUT):
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
        log(f"  {mmss(self.elapsed())} {phase}")
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
        if not self.lock.acquire(timeout=timeout):
            raise StudioError(f"another request of this session is still running after {timeout:.0f} s")
        try:
            self.last_used = time.monotonic()
            try:
                self.proc.stdin.write(json.dumps(req) + "\n")
                self.proc.stdin.flush()
            except (OSError, ValueError):
                raise StudioError("the game ended (write failed)")
            return self.read_line(timeout)
        finally:
            self.lock.release()

    def kill(self):
        """Ends the local ssh and the remote game (by PID) and removes the run folder."""
        try:
            self.proc.kill()
        except OSError:
            pass
        kill_remote(self.rout)


def fail(s, sid, what):
    """A session that timed out or died is unusable (a late answer would desync it): tear it
    down and return an error with the game log tail."""
    tail = log_tail(s.rout)
    log(f"session {sid} failed: {what}")
    s.kill()
    with S.SESSIONS_LOCK:
        S.SESSIONS.pop(sid, None)
    return f"ERROR: {what}; session {sid} was closed and its game killed. Game log tail:\n{tail}"


def abandon(sid):
    """Called by the shim when a call on `sid` passed its hard deadline: the worker thread is
    stuck, so the session goes (killing the ssh also unblocks that thread)."""
    with S.SESSIONS_LOCK:
        s = S.SESSIONS.pop(sid, None)
    if s:
        log(f"session {sid} abandoned after a stuck call")
        s.kill()


REAP_SCRIPT = r"""
now=$(date +%s); live=" $LIVE "
for p in $(pgrep -u claude -f 'release/AmericanConquest --drive'); do
  cwd=$(readlink /proc/$p/cwd 2>/dev/null); cwd=${cwd% (deleted)}
  case "$cwd" in /home/claude/runs/*-studio-*) ;; *) continue ;; esac   # studio games only
  case "$live" in *" $cwd "*) continue ;; esac                          # this server's sessions
  why=""
  case "$cwd" in /home/claude/runs/$NAME-studio-*) why="earlier session of $NAME" ;; esac
  if [ -z "$why" ]; then
    if [ -f "$cwd/alive" ]; then age=$(( now - $(stat -c %Y "$cwd/alive") ));
    else age=$(ps -o etimes= -p $p | tr -d ' '); fi
    [ "${age:-0}" -gt $STALE ] && why="no keepalive for ${age}s"
  fi
  [ -n "$why" ] || continue
  kill $p; for i in 1 2 3 4 5 6; do kill -0 $p 2>/dev/null || break; sleep 0.5; done; kill -9 $p 2>/dev/null
  echo "reaped $p $cwd ($why)"
done
for d in /home/claude/runs/*-studio-*; do
  [ -d "$d" ] || continue
  case "$live" in *" $d "*) continue ;; esac
  busy=""; for p in $(pgrep -u claude -f 'release/AmericanConquest --drive'); do
    [ "$(readlink /proc/$p/cwd 2>/dev/null)" = "$d" ] && busy=1; done
  [ -z "$busy" ] && [ $(( now - $(stat -c %Y "$d") )) -gt 300 ] && rm -rf "$d" && echo "removed $d"
done
find /home/claude/runs -maxdepth 1 -name 'build-*' -mmin +720 -delete 2>/dev/null
true
"""


def reap(name):
    """Closes this server's earlier sessions of `name` and its sessions idle for IDLE_REAP, then
    on the VM kills studio games nobody owns any more: earlier sessions of this worktree, and
    games whose run folder's `alive` file (touched every minute by the server holding the
    session) is older than STALE, or that have none (servers before the keepalive) and run
    longer than that. acr-vm-drive runs are never touched."""
    now = time.monotonic()
    with S.SESSIONS_LOCK:
        doomed = [(sid, s) for sid, s in S.SESSIONS.items()
                  if s.name == name or now - s.last_used > IDLE_REAP]
        for sid, _ in doomed:
            S.SESSIONS.pop(sid, None)
        live = " ".join(s.rout for s in S.SESSIONS.values())
    for sid, s in doomed:
        log(f"closing session {sid} ({s.name}): replaced or idle")
        s.kill()
    script = f"LIVE={shlex.quote(live)} NAME={shlex.quote(name)} STALE={STALE}\n{REAP_SCRIPT}"
    out = ssh("bash -c " + shlex.quote(script), timeout=45).strip()
    if out:
        log("reaper: " + out.replace("\n", "; "))
    return out


def keepalive_loop():
    """Touches <run>/alive of every open session once a minute, so other servers' reapers
    leave them alone and orphans (server gone, agent gone) are found."""
    while True:
        time.sleep(KEEPALIVE_EVERY)
        try:
            with S.SESSIONS_LOCK:
                routs = [s.rout for s in S.SESSIONS.values()
                         if time.monotonic() - s.last_used <= IDLE_REAP]
            if routs:
                ssh("touch " + " ".join(f"{shlex.quote(r)}/alive" for r in routs), timeout=20)
        except Exception as e:  # noqa: BLE001 - never let the thread die
            log(f"keepalive: {e}")


def ensure_keepalive():
    if S.KEEPALIVE[0] is None or not S.KEEPALIVE[0].is_alive():
        t = threading.Thread(target=keepalive_loop, daemon=True)
        t.start()
        S.KEEPALIVE[0] = t


def get(sid):
    s = S.SESSIONS.get(sid)
    if not s:
        raise StudioError(f"no session {sid}; open one with open_session")
    return s


def short(answer, limit=1500):
    answer = dict(answer)
    lg = answer.pop("log", None)
    if lg:
        answer["log"] = lg[-5:]
    text = json.dumps(answer)
    return text if len(text) <= limit else text[:limit] + "...(cut)"


def fetch(remote):
    try:
        data = vm_cargo.bounded(vm_cargo.SSH + [f"cat {shlex.quote(remote)}"], SSH_TIMEOUT).stdout
    except (subprocess.TimeoutExpired, OSError):
        data = b""
    if not data:
        raise StudioError(f"could not fetch {remote}")
    return data


def image_png(data, max_px, trim, save_to):
    from PIL import Image as PILImage
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
    return buf.getvalue()


def showcase_sig():
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


def sync_showcase(job):
    """One sync at a time (parallel opens share ~/showcase on the VM), skipped when the local
    showcase did not change since this server's last sync. Waiting for another open's sync
    is bounded too."""
    if not S.SHOWCASE_LOCK.acquire(timeout=job.limit(120)):
        raise StudioError("another session's showcase sync did not finish within 2 min")
    try:
        sig = showcase_sig()
        if sig != S.SHOWCASE_SIG[0]:
            vm_drive.sync_showcase(job.limit(120))
            S.SHOWCASE_SIG[0] = sig
    finally:
        S.SHOWCASE_LOCK.release()


BUILD_ENV = ("export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER=sccache SCCACHE_DIR=$HOME/.cache/sccache "
             "CARGO_TARGET_DIR=$HOME/target/{name} CARGO_TERM_COLOR=never")


def build(job, wt, name, sid):
    """Syncs the worktree and builds its release game on the VM. The build runs detached
    (nohup, own log per call) and is polled, so a time limit here never leaves cargo half
    killed: on the limit it keeps running on the VM and warms the target dir."""
    job.step("syncing the worktree to the VM")
    try:
        vm_cargo.sync(wt, name, job.limit(120))
    except SystemExit as e:
        raise StudioError(str(e))
    base = f"{RUNS}/build-{name}-{sid}"
    inner = (f"{BUILD_ENV.format(name=name)}; cd ~/wt/{name} && "
             f"cargo build --release -p acr-game > {base}.log 2>&1; echo $? > {base}.rc")
    # `;` not `&&`: `a && nohup b &` backgrounds the whole list in a subshell that keeps
    # ssh's output open, so this ssh would only return when the build ends.
    ssh(f"mkdir -p {RUNS}; nohup setsid bash -lc {shlex.quote(inner)} "
        f">/dev/null 2>&1 < /dev/null & echo started", timeout=job.limit(30))
    job.step(f"building ({vm_load()})")
    polls = 0
    while True:
        out = ssh(f"cat {base}.rc 2>/dev/null; echo '|'; tail -c 400 {base}.log 2>/dev/null",
                  timeout=job.limit(20))
        rc, _, blog = out.partition("|")
        rc = rc.strip()
        if rc:
            if rc != "0":
                tail = ssh(f"grep -E -A6 '^error' {base}.log | head -60; tail -5 {base}.log")
                ssh(f"rm -f {base}.rc {base}.log")
                raise StudioError(f"the build failed (cargo exit {rc}):\n{tail[-3000:]}")
            ssh(f"rm -f {base}.rc {base}.log")
            log(f"  {mmss(job.elapsed())} build done")
            return
        last = [ln.strip() for ln in blog.splitlines() if ln.strip()]
        what = last[-1][:80] if last else "waiting for cargo"
        if "Blocking waiting for file lock" in blog:
            what = "waiting for another build of this worktree (cargo lock)"
        polls += 1
        if polls % 8 == 0:
            what += f"; {vm_load()}"
        job.phase = f"building: {what}"
        if job.left() <= 3:
            raise StudioError(
                f"the build was still running after {mmss(job.elapsed())} ({what}; {vm_load()}); "
                f"it keeps going on the VM (log {base}.log), so a later open_session(build=true) "
                f"or acr-vm-drive reuses it")
        time.sleep(min(4, max(0.5, job.left() - 3)))


def open_session(job, worktree, build_first, models, size, sid, settings=""):
    """The blocking part of open_session. Returns the text answer; raises StudioError."""
    log(f"open_session {sid}: {worktree} build={build_first} models={models!r} settings={settings!r}")
    ensure_keepalive()
    job.step("checking the worktree")
    wt = worktree_root(worktree)
    name = os.path.basename(wt)
    rout = f"{RUNS}/{name}-studio-{sid}"
    try:
        job.step("closing stale sessions (this worktree's earlier ones, orphans)")
        reaped = reap(name)
        job.step("syncing showcase models (waits for parallel opens)")
        sync_showcase(job)
        layer = f"mkdir -p {rout}/models && cp -rs ~/showcase/models/. {rout}/models/ 2>/dev/null; "
        if models.startswith("wt:"):
            layer += f"cp -f ~/wt/{name}/{models[3:]}/*.glb {rout}/models/ 2>/dev/null; "
        elif models:
            job.step("syncing model overrides")
            vm_drive.sync_dir(vm_cargo.local_path(models), f"~/overrides/{name}", job.limit(120))
            layer += f"cp -f ~/overrides/{name}/*.glb {rout}/models/ 2>/dev/null; "
        settings_dir = "$HOME/showcase/settings"
        if settings:
            layer += vm_drive.settings_script(settings, settings_dir, f"{rout}/settings")
            settings_dir = f"{rout}/settings"
        if build_first:
            build(job, wt, name, sid)
    except subprocess.TimeoutExpired as e:
        raise StudioError(f"time limit hit during: {job.phase} (ssh step over {e.timeout:.0f} s)")
    except SystemExit as e:  # vm_drive.settings_script on a bad spec
        raise StudioError(str(e))
    except (subprocess.CalledProcessError, OSError) as e:
        raise StudioError(f"{job.phase} failed: {e}")
    job.step("starting the game")
    exe = f"$HOME/target/{name}/release/AmericanConquest"
    nobuild = json.dumps({"ok": False, "error": f"no release build of {name} on the VM; use build=true"})
    script = (f"{layer} cd {rout} && echo $$ > pid && touch alive && {{ test -x {exe} || {{ echo {shlex.quote(nobuild)}; "
              f"exit 3; }}; }} && export ACR_GAME_DIR=\"$HOME/game/American Conquest\" "
              f"ACR_MODELS_DIR={rout}/models ACR_SETTINGS_DIR={settings_dir} ACR_VOLUME=0 "
              f"ACR_LOG={rout}/game.log VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json DISPLAY=:5 && "
              f"exec {exe} --drive --drive-gpu --size {size} 2>>{rout}/stderr.log")
    proc = subprocess.Popen(vm_cargo.SSH + ["bash -lc " + shlex.quote(script)], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                            encoding="utf-8", errors="replace")
    s = Session(name, rout, proc)
    with S.SESSIONS_LOCK:
        S.SESSIONS[sid] = s
    job.phase = "waiting for the game's ready line"
    try:
        ready = s.read_line(job.limit(START_TIMEOUT))
    except StudioError as e:
        raise StudioError(fail(s, sid, f"the game did not start within {mmss(job.elapsed())} ({e}; {vm_load()})"))
    if ready.get("ok") is False:
        raise StudioError(fail(s, sid, ready.get("error", "the game did not start")))
    setline = ssh(f"grep -m1 'settings:' {rout}/game.log", timeout=10).strip()
    setline = setline[setline.find("settings:"):] if "settings:" in setline else ""
    log(f"open_session {sid} ready after {mmss(job.elapsed())} ({setline})")
    extra = f"\n{setline}" if setline else ""
    if reaped:
        extra += f"\n(reaped on the VM: {reaped.count(chr(10)) + 1} stale run(s))"
    return f"session {sid} ({name}) ready after {mmss(job.elapsed())}: {short(ready, 400)}{extra}"


def send(sid, requests, timeout):
    s = get(sid)
    out = []
    for req in requests:
        if isinstance(req.get("path"), str) and not req["path"].startswith("/"):
            req["path"] = f"{s.rout}/{req['path']}"
        try:
            out.append(short(s.request(req, timeout)))
        except StudioError as e:
            out.append(fail(s, sid, f"request {json.dumps(req)[:200]}: {e}"))
            break
    return "\n".join(out)


def shot(sid, camera, max_px, trim, save_to):
    """Returns PNG bytes, or an error/answer text."""
    s = get(sid)
    try:
        if camera:
            a = s.request({"cmd": "camera", **camera})
            if not a.get("ok"):
                return short(a)
        s.n += 1
        remote = f"{s.rout}/shot{s.n:03d}.png"
        a = s.request({"cmd": "screenshot", "path": remote})
    except StudioError as e:
        return fail(s, sid, f"shot: {e}")
    if not a.get("ok"):
        return short(a)
    return image_png(fetch(remote), max_px, trim, save_to)


def fetch_image(sid, path, max_px, trim, save_to):
    s = get(sid)
    remote = path if path.startswith("/") else f"{s.rout}/{path}"
    return image_png(fetch(remote), max_px, trim, save_to)


def close(sid):
    with S.SESSIONS_LOCK:
        s = S.SESSIONS.pop(sid, None)
    if not s:
        return "no such session"
    try:
        s.request({"cmd": "quit"}, timeout=5)
    except Exception:
        pass
    s.kill()
    log(f"closed {sid}")
    return f"closed {sid}"
