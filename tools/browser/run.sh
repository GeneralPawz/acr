#!/usr/bin/env bash
# The browser test on the render VM (docs/testing.md "Browser"). Run from a worktree root on the
# VM, e.g. through acr-vm-cargo:
#
#   acr-vm-cargo -- 'bash tools/browser/run.sh --out runs/browser/branch --overlay ~/browser/hd' \
#       --get 'runs/browser/branch=C:/abs/out/branch'
#
# 1. stages a game folder: the original's files (symlinked), ACR/Sounds, and ACR/Models with
#    the showcase models, the --overlay folder's glbs laid over them;
# 2. builds and serves the release web build (`cargo xtask serve --release --game-dir`);
# 3. runs tools/browser/browser_test.py (Playwright's headless Chromium) against it.
#
# Options: --out DIR (relative to the worktree), --overlay DIR, --port N, the rest goes to
# browser_test.py (--unit, --zooms, --step-s, --hold-s, --gpu vulkan|swiftshader|gl, --size).
set -euo pipefail
OUT=runs/browser/out
OVERLAY=""
PORT=8090
PASS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT=$2; shift 2 ;;
    --overlay) OVERLAY=$2; shift 2 ;;
    --port) PORT=$2; shift 2 ;;
    *) PASS+=("$1"); shift ;;
  esac
done
GAME=${ACR_GAME_DIR:-$HOME/game/American Conquest}
MODELS=${ACR_SHOWCASE_MODELS:-$HOME/showcase/models}
VENV=${ACR_BROWSER_VENV:-$HOME/browser/venv}
WT=$(pwd)
STAGE=$WT/target/browser-stage
rm -rf "$STAGE"
mkdir -p "$STAGE/ACR/Models"
for f in "$GAME"/*; do
  [ "$(basename "$f")" = ACR ] || ln -s "$f" "$STAGE/"
done
[ -d "$GAME/ACR/Sounds" ] && ln -s "$GAME/ACR/Sounds" "$STAGE/ACR/Sounds"
for f in "$MODELS"/*.glb; do ln -sf "$f" "$STAGE/ACR/Models/"; done
[ -d "$MODELS/ground" ] && ln -s "$MODELS/ground" "$STAGE/ACR/Models/ground"
if [ -n "$OVERLAY" ]; then
  for f in "$OVERLAY"/*.glb; do ln -sf "$f" "$STAGE/ACR/Models/"; done
fi
mkdir -p "$OUT"
LOG=$WT/$OUT/serve.log
cargo xtask serve --release --game-dir "$STAGE" --port "$PORT" >"$LOG" 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null || true; pkill -P $SERVER 2>/dev/null || true' EXIT
for _ in $(seq 1 600); do
  curl -sf "http://127.0.0.1:$PORT/" >/dev/null 2>&1 && break
  kill -0 $SERVER 2>/dev/null || { tail -30 "$LOG"; exit 1; }
  sleep 1
done
"$VENV/bin/python" tools/browser/browser_test.py --url "http://127.0.0.1:$PORT/?game=/game/" --out "$OUT" "${PASS[@]}"
