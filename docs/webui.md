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

## Native host spike (Linux, #333)

`cargo run --release -p acr-game --features webview-gtk --example webview_spike` (needs
`libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev`; `SPIKE_AUTO=1` runs the
scripted measurement, `SPIKE_NO_WEBVIEW=1` the baseline, `SPIKE_FIXED=r,g,b` a fixed clear colour for
pixel checks). winit (forced to X11: wry child webviews need it, so XWayland on Wayland) + wgpu Vulkan
clear + a transparent wry 0.57.1 child webview (WebKitGTK 2.52.6), 2026-11-01, AMD Radeon (RADV) on
GNOME/XWayland and llvmpipe on Xvfb.

| Check | Result |
|---|---|
| wgpu visible through transparent areas | **No.** The child X window (`XCreateSimpleWindow` in `wry::webkitgtk`) replaces the swapchain's pixels under it: black on Xvfb, black on XWayland (`xwd` of the toplevel: red clear 255,0,0 without the webview, 0,0,0 with it; the 55 % panel shows 11,11,11). X draws child windows without blending, so only a separate toplevel could be transparent. Same as tauri-apps/wry#1319 (open) |
| resize | `set_bounds` follows; 20 resizes ran. GLX error 168 (`GLXBadWindow`) on XWayland kills winit unless the xlib error hook ignores it (the spike ignores extension errors) |
| IPC JS to Rust, median / p99 | 1.3 / 4.4 ms (XWayland, GPU), 0.85 / 1.9 ms (Xvfb, idle); target < 2 / < 8 |
| `evaluate_script` 4 KB at 60 Hz, 10 s | 600 of 600 received; call 0.04 ms median, 0.24 ms p99; no frame over 25 ms (XWayland) |
| frame time, webview vs none (Present `Immediate`) | mean 0.14 vs 0.11 ms idle, 0.14 vs 0.13 ms during `evaluate_script` (XWayland) |
| F1-F12, Alt+Enter, Escape reach Rust | reach the page (F1, F5, F11, F12, Enter, Escape, Alt+Enter, synthetic XTest on Xvfb); they do not reach winit while the page has focus, so the page forwards them |
| pointer, click | clicks and moves on the forwarding area reach Rust (Xvfb) |
| process memory, webview vs none | 188 + 264 MiB of WebKit processes vs 123 MiB (XWayland) |
| custom protocol 512x512 PNG (895 KB) | 3 ms median, 4 ms p99 (XWayland, GPU) |
| missing runtime | not measured (needs the libraries absent) |

Not measured: flicker on resize and maximise, scale change (both need a person at the display), the
frame cost on a heavy scene. The numbers on Xvfb with llvmpipe are noisy (the machine was shared).

## Not yet

The touch skin, the HUD, the native webview host (`wry`; the Linux spike above says a transparent child webview cannot show the swapchain), the driver's `view` and
`ui_send` requests, the other screens, and the start screen's layout for other screen ratios
(#316 section 8).
