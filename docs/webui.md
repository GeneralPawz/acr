# The Svelte UI

Decision (tracking issue #316): menus, dialogs and later the HUD are drawn by one Svelte app, as DOM
over the game canvas in the browser build (a transparent webview over the window natively, not
built yet). **Rust keeps all UI state and logic**: the screens (`ui::Screen`, `ui::dialog::Dialog`)
and their rules in [docs/ui.md](ui.md) stay as they are, the headless driver and the e2e scenarios
run on them, and the Svelte app draws a view model Rust publishes and sends raw input back.

What exists now (#317, #322, browser part): the protocol, the bridge, the classic skin, the main menu,
the player name, the options, the campaigns, the single missions, the load dialog of the main menu, the random map setup, the loading screen and the connection screen of multiplayer (`Placeholder`), and (#325) the HUD's top bar, "M" drop-down and hints over the running game (#326), and (#325) the dialogs over the running game: the game menu, the options with their "Weitere Optionen" and ACR rule pages, the save dialog and the load dialog of the game menu, the mission's message boxes, choices and history. Every other screen is still drawn by the engine, under the UI's input overlay.

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
  `stage`, `screen`, `cursor`, `hud`), at most one message a frame, `rev` growing by one. A gap makes the UI
  send `resync`; the answer carries every topic. Scope `ui.bridge` of the profiler.
- **Claiming:** the registry (`ui/src/screens/registry.ts`: `MainMenu`, `PlayerName`, `Options`, `Campaigns`, `SingleMission`, `LoadGame`, `RandomMap`, `Placeholder`, `Loading`, `GameMenu`, `GameOptions`, `SaveGameDialog`, `MessageWindow`, `HistoryWindow`, and `HUD`: `TopBar`, `DropDown`, `Hints`) is announced in `ready` (`screens`, `hud`).
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
channel. Main menu, player name and options: 100.000%, layout 0 px (#317, #322). Campaigns, single missions and the campaign's missions (the same `SingleMission` screen, route `CampaignMissions`, ACR's Lake Erie row included): 100.000%, layout 0 px (#323). `LoadGame` (from the main menu): 100.000%, layout 0 px; its widgets' rects are the part inside their clip window, as the driver's tree reports them. `RandomMap` (players, nations, colours, teams, the scrolled option rows with ACR's Meer landscape, start; also with the landscape combo box open, route `RandomMapCombo`): 100.000%, layout 0 px over 64 widgets (#324). `Placeholder`: 100.000%, layout 0 px. `Loading` is claimed without a browser parity run: it lasts under a second in the browser and the page's driver answers nothing meanwhile, so it cannot be caught for a screenshot; it is a single picture, resolved like the backgrounds above, and the UI keeps it on screen in mode `loading`, where the engine's replay progress ("Laden 40%") became the widget `Loading/progress` (`Ui::progress_draw`).

Over the running game (#325; the blank grass field, the pixels counted over the dialog's box, `ui_parity.py` routes): `GameMenu` 100.000%, `GameOptions` 100.000% (also "Weitere Optionen", route `GameOptionsMore`, and the ACR rule page, `GameOptionsRules`), `LoadGame` from the game menu (`GameLoadGame`) 100.000%, `SaveGameDialog` 99.992% (the input box's caret), layout 0 px over 8 to 24 widgets each. `MessageWindow` (the first single mission's briefing) 100.000%, `HistoryWindow` (the same mission's history) 100.000%, layout 0 px over 8 and 10 widgets. `ChoiceWindow` is not in the registry yet and stays engine-drawn: no mission reachable here asks a multiple choice, so its parity was never measured (#462); the UI side is ready (its widgets are the ones of the other windows, its paper box is the `backdrop` widget).

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

## Dialogs over the running game (#325)

The classic skin over the game canvas (#316): in mode `game` the page's DOM lies over the canvas, which goes on
drawing the world and the HUD; the UI draws only a dialog (`ScreenPatch.in_game`) on the menu canvas of `stage`
(`Ui.svelte`), where the engine drew it. `webui::OVER_GAME` lists the screens it may claim (`GameMenu`,
`GameOptions`, `SaveGameDialog`, `LoadGame` from the game menu, `MessageWindow`, `ChoiceWindow`, `HistoryWindow`);
the registry still has to name them (`ChoiceWindow` it does not yet, #462), and any other `Screen::in_game` screen stays with the engine (`WebUi::draws`).
The engine takes a claimed dialog out of its frame (`Ui::set_external`) and nothing else.

- **Colours:** the widgets' draws are those of the menus' screens; over the game `view::widgets` maps them through
  `Draw::in_game` as the engine does (frames from the game palette, `GameFrame`/`GameText`).
- **Backdrop:** what a screen draws under its widgets without being one (`Screen::backdrop`: the choice
  window's paper box, `paper_box`) becomes the widget `<Screen>/backdrop`, first in the list.
- **Input:** every one of these dialogs is modal in the engine (the game stands still, `Ui::dialog_open`, and all input
  goes to the dialog), so the full-page `GameViewport` stays as it is: it forwards pointer, wheel and keys, Rust
  hit-tests them against the dialog and the game gets none of it while the dialog is open, as in the engine. A dialog that is not
  modal would need the viewport to let the pointer through outside its widgets; there is none yet.
- **Host:** the browser page draws transparently over the canvas. A native webview that only draws opaque
  full-screen menus (Linux WebKitGTK #333, Windows #319, undecided) cannot, so `WebUi::new` sets `transparent`
  from the host (`Host::Web`) and on any other host these screens stay with the engine; the check is in Rust,
  the UI needs nothing. A native host that draws transparently turns it on there.
- **Driver:** `--ui classic` claims every screen it can describe, over the game too, and checks their assets.
  Scenarios: `save_load` (game menu, save dialog, load dialog), `ingame_options` and `options_rules_page`,
  `mission_briefing` (a message box) and `mission_history` (the history window). No scenario reaches a `ChoiceWindow`
  (it needs a mission script that asks a multiple choice, none of the ones played so far does).
  The page's driver answers only `ui` while a message box is open (`drive::web_ready`), so the parity route
  dismisses the briefing with a key.

## The HUD (#326)

The classic skin over the game canvas draws the parts of the in-game interface the registry's `HUD` list names
(`TopBar`, `DropDown`, `Hints`; announced in `ready.hud`), on the same transparent host as the dialogs
(`WebUi::hud_claim`, only `Host::Web` or the driver's classic skin). The engine goes on drawing the world, the world
overlays (brackets, life bars, move markers), the selection rectangle, the minimap, the command and army panels and the
side panels, and the parts not named yet.

- **Topic:** `ViewPatch.hud` (`HudPatch { groups, removed }`): a group (`HudGroup { name, layer, anchor, scale, parts }`)
  is a layer of `Part`s in game canvas pixels (`Stage::game`), replaced as a whole and stacked by `layer`;
  `scale` about `anchor` is `hud_scale` (the hint is stretched about the mouse as `Hud::draw_hint` lays it out).
  Rust produces them: `Hud::draw` returns `Section`s (`Layer`), the game leaves the claimed ones out of its layers
  (`hud::Claim`, `InGame::set_hud_claim`, `Claimed` groups via `InGame::take_claimed`) and `WebUi::hud_patch`
  resolves them to parts after the frame's advance. The UI draws them in `Hud.svelte` over the canvas and under any dialog.
  The protocol and the game side know all four parts; the registry says which ones the page claims.
- **Diffing:** a group's draws are compared with the last ones before they are resolved to parts, and only changed groups go
  out, so a changed resource value resends its group (`top_bar_text`, six strings) and not the bar's frames
  (`top_bar`): `hud::claimed` splits the bar's strings from its sprites. The first patch of a connection and the one after a
  `resync` carry the topic even when empty; a `resync` snapshot makes the UI forget groups it does not list. A group whose
  files are still loading keeps its old version.
- **Groups:** `top_bar` (layer 0) and `top_bar_text` (1): the resource bar with the "M" plaque, or the touch bar (`top_bar=touch`,
  open panel included); `center_top_bar` moves them in Rust; `drop_down` (2): the "M" drop-down; `hint` (3): the tooltip, at `hud_scale` where the engine scales it.
- **Input** is the viewport's, as over the dialogs: Rust hit-tests the "M" button, the rows and the plaques.
- **Driver:** `--ui classic` claims all four; `{"cmd":"view"}` shows the groups (scenarios `svelte_hud`, `svelte_hud_scale`).
  `drive::tree::messages` still reads the game's whole overlay.

Parity (`ui_parity.py`, the blank grass field, the mouse waits mid-screen; pixels over the part's area `HUD_AREAS`, layout
from every sprite's and fill's published rect against the page's (`data-acr-part-rect`), at 1024x768, device pixel ratio 1):
`TopBar` 100.000%, `DropDown` 100.000%, `Hints` 100.000% (the area starts below the cursor, which only the engine draws in a screenshot), layout 0 px. At a window whose scale is not whole the
engine's linear filter and the page's differ at glyph edges (the page lays a string out glyph by glyph, the engine scales the
whole string): `TopBarWide` (1920x1080, scale 1.40625) 96.669%, `TopBarCentered` 96.654%, `TopBarTouch` (1584x720, scale 0.9375) 99.495%,
`TopBarTouchOpen` 98.102%, so those are measured, not within 99.5%. Not measured by pixels: a hint at `hud_scale` (no icon to hover in the blank field of the
page; `svelte_hud_scale` checks its group).

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

The touch skin, the rest of the HUD (the minimap, the command and army panels, the side panels, the pause text, the diplomacy panel), the native webview host (`wry`, spike on Windows), the other screens, and the start screen's layout for other screen ratios
(#316 section 8).
