"""State of the acr-studio server that must survive a reload of studio_core (the open
sessions, their locks, the local log). Keep this file tiny and stable: it is never reloaded."""

import os
import threading
import time

SESSIONS = {}                      # session id -> studio_core.Session
SESSIONS_LOCK = threading.Lock()
SHOWCASE_LOCK = threading.Lock()   # one showcase sync at a time (parallel opens share ~/showcase)
SHOWCASE_SIG = [None]
KEEPALIVE = [None]                 # the keepalive thread, started once
STARTED = time.time()

LOG_DIR = os.path.join(os.environ.get("LOCALAPPDATA") or os.path.expanduser("~/.cache"), "acr-studio")
LOG_FILE = os.path.join(LOG_DIR, "server.log")
_LOG_LOCK = threading.Lock()


def log(msg):
    """One timestamped line in %LOCALAPPDATA%/acr-studio/server.log (all servers share it; the
    PID tells them apart). Never raises."""
    try:
        os.makedirs(LOG_DIR, exist_ok=True)
        line = f"{time.strftime('%Y-%m-%d %H:%M:%S')} [{os.getpid()}] {msg}\n"
        with _LOG_LOCK:
            if os.path.exists(LOG_FILE) and os.path.getsize(LOG_FILE) > 4 << 20:
                os.replace(LOG_FILE, LOG_FILE + ".1")
            with open(LOG_FILE, "a", encoding="utf-8") as f:
                f.write(line)
    except OSError:
        pass
