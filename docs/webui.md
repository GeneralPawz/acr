# The Svelte UI

Decision (tracking issue #316): menus, dialogs and later the HUD are drawn by one Svelte app, as DOM
over the game canvas in the browser build (a transparent webview over the window natively, not
built yet). **Rust keeps all UI state and logic**: the screens (`ui::Screen`, `ui::dialog::Dialog`)
and their rules in [docs/ui.md](ui.md) stay as they are, the headless driver and the e2e scenarios
run on them, and the Svelte app draws a view model Rust publishes and sends raw input back.

What exists now (#317, browser part): the protocol, the bridge, the classic skin and the main menu.
Every other screen is still drawn by the engine, under the UI's input overlay.

## Parts

| Part | Where |
|---|---|
| Protocol (serde, TypeScript export) | `crates/acr-ui-protocol`; `cargo test -p acr-ui-protocol --features ts` writes `ui/src/lib/protocol/` (committed, checked for no diff) |
| Game side | `crates/acr-game/src/webui/`: `mod.rs` (state, diffing), `view.rs`, `input.rs`, `keys.rs`, `assets.rs`, `font_atlas.rs`, `web.rs` (wasm exports) |
| App | `ui/` (Svelte 5, Vite 8, TypeScript, Vitest; Bun 1.4.4, no Node needed): `cargo xtask ui` (`--watch`) builds `ui/dist/acr-ui.js` + `.css` |
| Browser page | `crates/acr-game/web/index.html`: `/?game=/game/&ui=svelte` mounts it after the game started; the default is `ui=engine` |
| Tooling | `cargo xtask web/serve/dist` build and copy the bundle to `target/web/game/ui/` (`--no-ui` or no `bun` skips it, `ACR_UI_DIST` names a bundle built elsewhere); `cargo xtask ui-diff A.png B.png [--out D.png]` |

## Bundle embedding

`crates/acr-game/build.rs` reads `ACR_UI_DIST` (a folder; it must contain `acr-ui.js`, else the build
fails) or `ui/dist`, and generates `ui_bundle.rs` into `OUT_DIR`: `FILES: &[(&str, &[u8])]` (path with
`/` below the folder, sorted, `include_bytes!`) and `PRESENT`. `webui::bundle` includes it. A missing
`ui/dist` gives `PRESENT = false` and a `cargo:warning`, never an error; the wasm library embeds
nothing (the page loads `ui/` beside it). `cargo xtask dist` builds the bundle first and passes it as
`ACR_UI_DIST`, so `AmericanConquest.exe` carries it.

CI: the `ui` job (Bun from `ui/.bun-version`) checks the protocol export, runs `bun run check|test|build`,
uploads `ui/dist` as artifact `acr-ui`, and runs the `webui::bundle` test with `ACR_UI_DIST` set to it.
The `wasm` and (paused) `windows` jobs of `ci.yml`, and the `web` and `windows` jobs of `release.yml`,
need `ui` and download the artifact into `ACR_UI_DIST`. The Windows-only clippy of `acr-game` waits for
the native host (#319).

Without a bundle everything compiles and the game uses the engine's UI; the driver, `cargo test`
and clippy never need Bun.

## The bridge (browser)

- The page calls the wasm exports `acr_ui_connect(cb)` (receiver of JSON messages), `acr_ui_send(json)`,
  `acr_ui_asset(path) -> Promise<Uint8Array>` and `acr_audio_unlock()`; they reach the game as
  `Event::Ui`, `Event::UiAsset` and `Event::AudioUnlock` through a thread-local clone of the event
  loop proxy. `acr_audio_unlock` is called inside the pointer or key event, since forwarded events
  leave the user gesture; fullscreen (F11, Alt+Enter) is handled in the page for the same reason.
- `ui/` sends `ready { protocol, screens, viewport, dpr }`; the game answers `hello` and from then on
  publishes after each frame's advance (`WebUi::publish`): only changed topics (`mode`, `loading`,
  `stage`, `screen`, `cursor`), at most one message a frame, `rev` growing by one. A gap makes the UI
  send `resync`; the answer carries every topic. Scope `ui.bridge` of the profiler.
- **Claiming:** the registry (`ui/src/screens/registry.ts`, now `MainMenu`) is announced in `ready`.
  `Ui::set_external` makes the engine skip exactly the screens the UI claims, and the cursor, but only
  while `view::screen` can describe the screen completely; otherwise the screen goes on being drawn by
  the engine (`engine_drawn: true`), never half.
- **Input:** the page-wide `GameViewport` forwards pointer (physical pixels, moves coalesced per
  animation frame), wheel, keys and blur. `webui::input` feeds them to `Game::cursor_moved`,
  `mouse_input`, `mouse_wheel` and `keyboard_input`, so hit-testing, press semantics, hover sounds and
  keys stay the original's in Rust. DOM keys become winit keys in `webui/keys.rs`.
- **Cursor:** a CSS `cursor:` from the cursor's frame (`frame/...` asset and hotspot); at a canvas scale
  other than 1 it is not scaled.

## Assets

Decoded at runtime from the player's files (`webui::assets::resolve`), never stored:

| Path (game paths percent-encoded) | Answer |
|---|---|
| `bmp/<path>` | PNG of a bitmap |
| `frame/<menu\|game\|sprite>/<player>/<gp>/<frame>` | PNG of a sprite frame's bounding box; `menu` and `game` use the menus' and the game screen's palette, `sprite` the pack's own |
| `font/<set>/<FontId>.png` / `.json` | glyph atlas of bytes 0x20..=0xFF / `{height, glyphs: {"<char>": [x, y, w, h, left, top, advance]}}` |
| `text/<set>/<FontId>/<fade>/<hex bytes>` | a whole string as a PNG, the exact fallback |

`GameText.svelte` lays glyph images out like `ShowString` (pen + advance, later glyphs over earlier ones,
shadows included in each glyph). A string with bytes the atlas lacks (escapes, control bytes) or a fade
goes through `text/` as a sprite part. Failures are errors (`AssetError`: 400 bad path, 404 missing, 500
undecodable), never placeholders.

## Parity

A screen joins the registry when `cargo xtask ui-diff` of its screenshots, `ui=engine` against
`ui=svelte` at 1024x768, device pixel ratio 1, shows at least 99.5% of the pixels within +-8 per
channel. Main menu: 100.000% (#317).

## Driver and parity tooling (#320)

- `--ui engine|classic` of the game's driver (`cargo xtask e2e|drive --ui classic`): classic plays the
  UI's side headless. The driver announces `Ready` (viewport, dpr 1) and claims every screen
  `view::screen` can describe; the messages go to a collecting outbox (`Game.ui_outbox`, no page).
  After each command the driver resolves every asset path the published patches refer to
  (`webui::referenced`, `WebUi::check_asset`) and fails the command on an asset that cannot be made or a
  screen whose description failed on an asset; screens the protocol cannot express stay with the engine
  (not an error). `--ui touch` is refused in the `ready` line until the touch skin exists.
- Requests: `{"cmd":"view"}` resyncs and answers the whole `ViewPatch` as `view`;
  `{"cmd":"ui_send","msg":{...}}` hands a `FromUi` message to the game (`webui::input::apply`), erroring on
  a message that does not parse or arrives before the UI is connected. Input still enters through the
  game's own handlers, so every scenario runs unchanged: `cargo xtask e2e --ui classic`.
- The web driver (`acr_drive`) also takes `click`, `key` and `text`.
- `tools/browser/ui_parity.py --screens <names>`: see docs/testing.md "Browser".

## Not yet

The touch skin, the HUD, the native webview host (`wry`, spike on Windows), the other screens, and the start screen's layout for other screen ratios
(#316 section 8).
