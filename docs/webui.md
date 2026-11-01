# The Svelte UI

Decision (tracking issue #316): menus, dialogs and later the HUD are drawn by one Svelte app, as DOM
over the game canvas in the browser build (a transparent webview over the window natively, not
built yet). **Rust keeps all UI state and logic**: the screens (`ui::Screen`, `ui::dialog::Dialog`)
and their rules in [docs/ui.md](ui.md) stay as they are, the headless driver and the e2e scenarios
run on them, and the Svelte app draws a view model Rust publishes and sends raw input back.

What exists now (#317, #322, browser part): the protocol, the bridge, the classic skin, the main menu,
the player name and the options. Every other screen is still drawn by the engine, under the UI's input overlay.

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
- **Claiming:** the registry (`ui/src/screens/registry.ts`: `MainMenu`, `PlayerName`, `Options`) is announced in `ready`.
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
channel. Main menu, player name and options: 100.000%, layout 0 px (#317, #322).

Typing goes as raw keys: the engine's input box takes the key's text as CP1252 (`typed` in
`webui/keys.rs`, `cp1252` in `lib.rs`), so `ÄÖü€` is stored as the original stores it. A real `<input>`
overlay for IME and soft keyboards is not built yet.

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

## Native host spike (Android, #333)

`tools/android/build-webview-spike.sh` builds the example `crates/acr-game/examples/webview_spike_android.rs`
(a cdylib, empty on other targets) plus a small dex (`tools/android/webview-spike/`) as its own app,
`io.github.generalpawz.acr.webviewspike`, never the game's package. Start it with
`adb shell am start -n io.github.generalpawz.acr.webviewspike/android.app.NativeActivity --es mode full --es fixed 255,0,0`
(`mode` full / capture / shrink, `hold` seconds, `fixed` r,g,b clear colour); it logs `SPIKE ...` lines
under the logcat tag `spike`. A `NativeActivity` (the game's host) draws an animated wgpu (Vulkan) clear; a
transparent WebView (`setBackgroundColor(TRANSPARENT)`, hardware layer) in a `PopupWindow` covers the screen
with a half-transparent panel, three white swatches of alpha 1 / .5 / .25, a button (`@JavascriptInterface`
to Rust) and `evaluateJavascript` back. OnePlus 13 (Android 16, Adreno 830, 1080x2376, landscape), WebView
Chrome 156, 2026-11-01.

**Answer: yes, the game shows through with correct alpha; the host needs Java code and its own touch routing.**

| Check | Result |
|---|---|
| Hosting | `NativeActivity` calls `takeSurface`/`takeInputQueue`, so the app owns the activity's surface and views added to its content are never drawn. A `PopupWindow` is its own window and its own SurfaceFlinger layer (type 1000) above the game's, so it works |
| wgpu visible through transparent areas | **Yes.** Clear (255,0,0): outside the panel 255,0,0; swatches 255,255,255 / 255,128,128 / 255,64,64 (alpha 1 / .5 / .25 exactly); panel rgba(20,20,20,.55) shows 126,11,11 (expected 125.75,11,11). A screenshot the user took by hand (JPEG) gives 125,11,11 and the same swatches. Screenshot by `adb exec-out screencap -p` |
| SurfaceFlinger composition | `dumpsys SurfaceFlinger`: the game layer and the popup layer are both `DEVICE` (hardware composer overlays), `usesClientComposition=false`: no GPU composition of the two |
| touch, full-screen popup | A full-screen popup takes every touch (windows do not pass touches through). With the page's interactive rectangles reported to Java (`acr.regions`), `onTouch` returns false inside them and forwards the touch to Rust outside (`nativeTouch`): `adb shell input tap` on the button gave one page click (0.1 ms to Rust), on the panel's empty area and outside the panel reached the game (forwarded to Rust). Modes `capture` (no forwarding) and `shrink` (popup the size of the content) are in the code and were not run |
| IPC JS to Rust, median / p99 | 0.34 / 1.65 ms (300 pings, last run); 0.15-0.47 / 0.98-3.2 ms over five runs |
| `evaluateJavascript`, Rust to page, median / p99 | 1.10 / 1.83 ms (300 calls at 60 Hz, 2 bytes); 1.31 / 2.40 ms with 4 KB (1.2 MB); over runs 0.5-1.3 / 1.7-4.0 ms. The call itself returns in 0.10 / 0.49 ms (0.16 / 0.60 ms at 4 KB) |
| frame time, webview vs none | vsync-bound (AutoVsync, 60 Hz): baseline median 16.37 ms, p99 19.7; idle webview 16.58 / 19.4; during JS to Rust 16.56 / 18.7; during Rust to JS 16.49 / 20.0 and 16.54 / 19.6 (4 KB). No frame over 27 ms in these runs. Touch input switches the display to 90 or 120 Hz (8-11 ms frames), which makes runs with touches not comparable |
| `dumpsys gfxinfo` (the popup's own HWUI frames) | 81 frames: p50 6 ms, p99 61 ms, 2 missed vsync, GPU p50 1 ms / p99 6 ms (a run with taps and app switches); a second run p50 7 ms, GPU p99 4 ms |

What a real Android host needs:

- Java: `hasCode=true` and a small dex (the `@JavascriptInterface` object cannot be made from Rust). The
  game's `AndroidManifest.xml` has `hasCode=false` now.
- The natives must be registered with `RegisterNatives` from Rust: `NativeActivity` loads the library outside
  the class loader's lookup, so `Java_..._nativeX` names fail with `UnsatisfiedLinkError`.
- A native thread cannot `FindClass` app classes: load the Java class through
  `activity.getClassLoader().loadClass`.
- `WebView` calls (`evaluateJavascript`, `loadData...`) on the UI thread (`runOnUiThread`).
- Touch routing as above: the page publishes its interactive rectangles, Java forwards everything else to
  the game's touch handler (a transparent window is not touch-transparent). The popup is not focusable, so
  keys stay with the activity.
- Pause and resume: the popup lives with the activity; winit's `suspended`/`resumed` recreates the window
  and the surface, not the WebView.

Not measured: battery and CPU cost over time (no long run, the phone is the user's), a heavy scene under the
webview, the `capture` and `shrink` modes, a soft keyboard (the popup is not focusable), screen rotation.

## Not yet

The touch skin, the HUD, the native webview host (`wry`, spike on Windows), the other screens, and the start screen's layout for other screen ratios
(#316 section 8).
