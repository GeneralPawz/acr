# Notes for coding agents

ACR re-implements *American Conquest* 1.46 from `dmcr.exe`. Match the original's behaviour
and name the addresses you followed in the docs (`docs/`).

## Testing

Follow [docs/testing.md](docs/testing.md), cheapest first:

1. Unit tests for the rule you changed (`cargo test -p <crate>`).
2. Anything visible in the game: a scenario in `tests/e2e/`, run headless with
   `cargo xtask e2e --filter <name> -v --game-dir <American Conquest folder>`.
   Mission message boxes are dismissed automatically (`--dialogs report` to see them).
3. Finding your way: `{"cmd":"ui"}` through `cargo xtask drive`, or
   `cargo xtask jev explore --goal "..."` (cheap Jev decisions; the key is read from a
   local file and must never be printed or committed).
4. Screenshots only for visual questions: `--gpu` and one cropped `screenshot`.
5. Expensive reasoning only when Jev answers "needs reasoning" or a scenario fails unclearly.

Never drive a visible game window with simulated mouse or keyboard input, and never test by
taking screenshots in a loop.

## Working in this repository

- Several sessions and agents share the machine: work in your own git worktree, remove only
  the worktrees and branches you created, and never run `target/release/AmericanConquest.exe`
  directly (copy it under a unique name if you need a windowed run).
- The original game data is not in the repository; never commit files from it except the
  icon that is already there.
- The menus' Svelte UI (`ui/`, docs/webui.md): Rust keeps all UI state; `cargo test -p acr-ui-protocol
  --features ts` must leave no diff in `ui/src/lib/protocol/`, and in `ui/` run
  `bun run check && bun run test && bun run build`. `crates/acr-game/build.rs` embeds `ui/dist` or
  `ACR_UI_DIST` (`webui::bundle`); without it the build only warns, so `cargo test` and clippy never need Bun.
- Keep the browser build compiling:
  `cargo clippy --target wasm32-unknown-unknown -p acr-game -p acr-render -- -D warnings`.
