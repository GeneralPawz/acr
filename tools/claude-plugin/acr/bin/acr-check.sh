#!/bin/bash
# The landing checks of ACR, step by step: prints PASS/FAIL per step, exits non-zero on any failure.
fail=0
step() { local n="$1"; shift; local log=$(mktemp); if "$@" >"$log" 2>&1; then echo "PASS $n"; else echo "FAIL $n"; tail -40 "$log"; fail=1; fi; rm -f "$log"; }
step fmt cargo fmt --all --check
step clippy cargo clippy --workspace --all-targets -q -- -D warnings
step clippy-wasm cargo clippy -q --target wasm32-unknown-unknown -p acr-game -p acr-render -p acr-physics -- -D warnings
step test cargo test -q -p acr-sim -p acr-render -p acr-game -p acr-physics
step blender-tests python3 -m unittest discover -s assets/blender/tests
exit $fail
