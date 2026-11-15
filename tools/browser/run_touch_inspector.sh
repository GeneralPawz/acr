#!/usr/bin/env bash
# The unit inspector's touch test on the render VM (docs/testing.md "Browser"): stages the game
# folder as tools/browser/run.sh does, builds the UI bundle, serves the release web build and runs
# tools/browser/touch_inspector.py. Options: --out DIR (relative to the worktree), --port N; the
# rest goes to touch_inspector.py.
set -euo pipefail
OUT=runs/touch-inspector
PORT=8093
PASS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT=$2; shift 2 ;;
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
mkdir -p "$OUT"
cargo xtask ui
LOG=$WT/$OUT/serve.log
cargo xtask serve --release --game-dir "$STAGE" --port "$PORT" >"$LOG" 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null || true; pkill -P $SERVER 2>/dev/null || true' EXIT
for _ in $(seq 1 900); do
  curl -sf "http://127.0.0.1:$PORT/" >/dev/null 2>&1 && break
  kill -0 $SERVER 2>/dev/null || { tail -30 "$LOG"; exit 1; }
  sleep 1
done
"$VENV/bin/python" tools/browser/touch_inspector.py --url "http://127.0.0.1:$PORT/?game=/game/" --out "$OUT" "${PASS[@]}"
