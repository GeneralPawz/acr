# The Svelte UI

Decision (tracking issue #316): menus, dialogs and later the HUD are drawn by one Svelte app, as DOM
over the game canvas in the browser build (a transparent webview over the window natively, not
built yet). **Rust keeps all UI state and logic**: the screens (`ui::Screen`, `ui::dialog::Dialog`)
and their rules in [docs/ui.md](ui.md) stay as they are, the headless driver and the e2e scenarios
run on them, and the Svelte app draws a view model Rust publishes and sends raw input back.

What exists now (#317, #322, browser part): the protocol, the bridge, the classic skin, the main menu,
the player name, the options, the campaigns, the single missions, the load dialog of the main menu, the random map setup, the loading screen and the connection screen of multiplayer (`Placeholder`), and (#325) the HUD's top bar, "M" drop-down, hints, messages and cursor over the running game (#326), the minimap, command panel, unit stats and unit panel (#327), the game panel and the test editor's window (#328), and (#325) the dialogs over the running game: the game menu, the options with their "Weitere Optionen" and ACR rule pages, the save dialog and the load dialog of the game menu, the mission's message boxes, choices and history. Every other screen is still drawn by the engine, under the UI's input overlay.

## Parts

| Part | Where |
|---|---|
| Protocol (serde, TypeScript export) | `crates/acr-ui-protocol`; `cargo test -p acr-ui-protocol --features ts` writes `ui/src/lib/protocol/` (committed, checked for no diff) |
| Game side | `crates/acr-game/src/webui/`: `mod.rs` (state, diffing), `view.rs`, `input.rs`, `keys.rs`, `assets.rs`, `font_atlas.rs`, `web.rs` (wasm exports) |
| App | `ui/` (Svelte 5, Vite 8, TypeScript, Vitest; Bun 1.4.4, no Node needed): `cargo xtask ui` (`--watch`) builds `ui/dist/acr-ui.js` + `.css` |
| Browser page | `crates/acr-game/web/index.html`: mounts it after the game started; it is the default, `/?game=/game/&ui=engine` selects the engine UI instead |
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
- **Claiming:** the registry (`ui/src/screens/registry.ts`: `MainMenu`, `PlayerName`, `Options`, `Campaigns`, `SingleMission`, `LoadGame`, `RandomMap`, `Placeholder`, `Loading`, `GameMenu`, `GameOptions`, `SaveGameDialog`, `MessageWindow`, `HistoryWindow`, and `HUD`: `TopBar`, `DropDown`, `Hints`, `Messages`, `Minimap`, `CommandPanel`, `UnitStats`, `GamePanel`, `Editor`) is announced in `ready` (`screens`, `hud`).
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

- `--ui engine|classic|touch` of the game's driver (`cargo xtask e2e|drive --ui classic`): classic plays the
  UI's side headless. The driver announces `Ready` (viewport, dpr 1) and claims every screen
  `view::screen` can describe; the messages go to a collecting outbox (`Game.ui_outbox`, no page).
  After each command the driver resolves every asset path the published patches refer to
  (`webui::referenced`, `WebUi::check_asset`) and fails the command on an asset that cannot be made or a
  screen whose description failed on an asset; screens the protocol cannot express stay with the engine
  (not an error). `--ui touch` connects the same way and sends widget clicks as intents (next section).
- Requests: `{"cmd":"view"}` resyncs and answers the whole `ViewPatch` as `view`;
  `{"cmd":"ui_send","msg":{...}}` hands a `FromUi` message to the game (`webui::input::apply`), erroring on
  a message that does not parse or arrives before the UI is connected. Input still enters through the
  game's own handlers, so every scenario runs unchanged: `cargo xtask e2e --ui classic`.
- The web driver (`acr_drive`) also takes `click`, `key` and `text`.
- `tools/browser/ui_parity.py --screens <names>`: see docs/testing.md "Browser".

## Intents (#330)

The touch skin (#316 section 5, built in #332) does not hit-test: it sends what a click on one of its own
components means, `FromUi::Intent { seq, id, action }`, and Rust checks it and applies it. The classic skin
never sends one. An intent that cannot be applied changes nothing and is answered `ToUi::Rejected { seq,
reason }` (the reason is for the log; the driver turns it into a failed command). Each setter ends in the
code the press ends in (`Dialog::press_plain`, `choose_param`, `reveal`, the HUD's `icon_event`), so the
`Outcome` and the state are those of the original input, and `docs/ui.md` stays the one place for the rules.

| `id` | `action` | Effect |
| --- | --- | --- |
| `<Screen>/<index>` (the widget's id in the driver's tree, also `WidgetView.id`) | `click` (left) | `Dialog::activate`: a widget with a `param` reports it with the click sound; a combo box opens or closes; a radio button or check changes state; an input box or text viewer takes the focus |
| same, a list | `select_row { row }` | `Dialog::select_row`: row `row` (index into all rows) is selected and scrolled into view as the keys do; a list with a `param` reports it instead, as the press does |
| same, a combo box | `select_option { option }` | `Dialog::select_option`: the click on the box and on the entry, open or not |
| same, a slider | `set_value { value }` | `Dialog::set_value`: the drag to `value` (`0..=max`) |
| same, an input box | `set_text { text }` | `Dialog::set_text`: focus and the text as if typed into the emptied box (CP1252, printable, at most `capacity - 1`) |
| `hud/<slot>`, `army/<slot>` | `click { button, mods }` | `InGame::hud_event`: the command of the icon, left or right (`Hud::icon_click`); `mods` are held for this click (Ctrl: an endless order) |
| `minimap` | `minimap_at { uv, button }` | `InGame::hud_event`: left centres the camera, right orders the selection to the point (`Hud::minimap_pixel`) |

Refused: a screen that is not the top one, not built yet or busy (the main menu sliding), a widget that does
not exist, is disabled, hidden or outside its clip window, the wrong kind for the action, a row, entry or
value out of range, text that does not fit or has a control character, an icon that does nothing, a
middle-button click. Nothing is clamped or substituted. `Dialog::check` answers without applying; `Ui::intent`
checks first and then feeds `Input::Intent` to the screen, so the screen reacts as it does to a click (its
`ItemChoose` handling, the selection of a list showing a briefing).

`Dialog::set_key` gives a widget a name that survives layout changes (`WidgetView.key`): `ok`, `cancel`,
`start`, `name`, a list's own name (`missions`, `saves`), `choice_<row>`, `option_<n>`, `player_<i>`. Every screen
names its buttons, lists, combo boxes, sliders, input boxes and text viewers; the pictures and static texts have
none. Keys are unique within a screen.

`InGame.interface.hud_input` (default on; `InGame::set_hud_input`): off, a window pixel never reaches the HUD
(press, right press, minimap drag), so the HUD takes `hud_event`s only. The top bar's drop-down, the unit and
game panels and the test editor's window have no intent yet; they stay pixel-driven.

Driver, `--ui touch`: connected like `--ui classic`, `hud_input` off in a running game, and a left click
(`click`, `move` excepted) on a target the tree names goes in as an intent: a row `Screen/3#5` as `select_row`, an
entry `Screen/3~2` as `select_option`, a button, text, combo box, input box or viewer as `click`, an icon
`hud/N` or `army/N` as `click` with the step's `mods` (right clicks on `hud/N` too). The answer carries it as
`intent`. A frame runs before the intent, as the pixel click's move gives one. The automatic dismissal of
mission boxes (`--dialogs auto`) sends its intent too. Everything else stays raw pixels: `pos` and map targets,
lists, sliders and scroll bars by their centre, the editor's icons, double clicks. A refused intent fails the
command (`intent N refused: ...`). Scenarios whose steps click the HUD by pixel
(`mission_history`: the M button; `touch_*`: the engine's own touch controls) pin `--ui engine` in their `args`;
the first `--ui` wins over the runner's.

`cargo xtask e2e --ui touch` passes every scenario the way the default and `--ui classic` do; `touch_intents`
checks the intents themselves.

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
(`TopBar`, `DropDown`, `Hints`, `Messages`; announced in `ready.hud`), on the same transparent host as the dialogs
(`WebUi::hud_claim`, only `Host::Web` or the driver's classic skin). The engine goes on drawing the world, the world
overlays (brackets, life bars, move markers), the selection rectangle, the minimap, the command and army panels and the
side panels.

- **Topic:** `ViewPatch.hud` (`HudPatch { groups, removed }`): a group (`HudGroup { name, layer, anchor, scale, parts }`)
  is a layer of `Part`s in game canvas pixels (`Stage::game`), replaced as a whole and stacked by `layer`;
  `scale` about `anchor` is `hud_scale` (the hint is stretched about the mouse as `Hud::draw_hint` lays it out).
  A group that only moved (the same draws shifted, as a tooltip follows the mouse) goes out as a `HudMove` with the new `offset`
  (`WebUi::hud_patch`, `shift`), not as its parts again.
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
  open panel included); `center_top_bar` moves them in Rust; `drop_down` (2): the "M" drop-down; `hint` (3): the tooltip, at `hud_scale` where the engine scales it; `messages` (4): the mission's hint lines, the central text and the end of the game
  (`Mission::draws`; the save notice and the editor's overlay stay with the engine).
- **Cursor:** the `cursor` topic of the menus already covers the game: `Ui::cursor_draw` is the in-game cursor frame and its
  hotspot, and the engine draws none while a UI is connected.
- **Input** is the viewport's, as over the dialogs: Rust hit-tests the "M" button, the rows and the plaques.
- **Driver:** `--ui classic` claims all four; `{"cmd":"view"}` shows the groups (scenarios `svelte_hud`, `svelte_hud_scale`).
  `drive::tree::messages` still reads the game's whole overlay.

Parity (`ui_parity.py`, the blank grass field, the mouse waits mid-screen; pixels over the part's area `HUD_AREAS`, layout
from every sprite's and fill's published rect against the page's (`data-acr-part-rect`), at 1024x768, device pixel ratio 1):
`TopBar` 100.000%, `DropDown` 100.000%, `Hints` 100.000% (the area starts below the cursor, which only the engine draws in a screenshot), `Messages` 100.000% (the leader chords' hint line), layout 0 px. At a window whose scale is not whole the page cannot match the engine at the edges of sprites and
strings (the numbers stay at 1024x768, where the scale is 1): `TopBarWide` (1920x1080, scale 1.40625) 96.683%, `TopBarCentered` 96.648%,
`TopBarTouch` (1584x720, scale 0.9375) 99.495%, `TopBarTouchOpen` 98.114%, so those are measured, not within 99.5%. Whole-string
images for the strings (`text/`, the exact fallback) were tried and changed nothing (96.669% to 96.683%), so the glyph layout is
not the cause: the pixels in the interior match and the engine's edge pixels are darker by 8 to 20 levels. The engine's image pass
filters straight-alpha textures (`image.wgsl`, `ALPHA_BLENDING`, sRGB textures), the browser composites premultiplied images; the
two cannot be made equal from the DOM, only by changing how the engine filters at a scale that is not whole (#464). Until then
`WebUi::hud_claim` claims the HUD only at a whole canvas scale; at any other scale the engine keeps drawing it. Not measured by pixels: a hint at `hud_scale` (no icon to hover in the blank field of the
page; `svelte_hud_scale` checks its group) and the cursor (a screenshot has no system cursor; the viewport's CSS cursor was
read back).

Patch size and rate (`tools/browser/ui_patches.py`, swiftshader, 1024x768, blank field, six seconds each): idle 0.3 patches
a second, 227 bytes a second; the mouse sweeping over the resource values (a hint that moves with it) 20 patches a second, median 96 bytes
(a shift; 1.8 KB when the hint changes), 4.5 KB/s (34 KB/s before shifts); over the map 1 patch (61 bytes); the drop-down open with the mouse over its rows 3 patches a second of
5.6 KB (the whole group on each hover change), 17 KB/s. The page's frame intervals stay at the 16.7 ms vsync (median) in every phase;
the resource values of a game that gathers resend the 700 bytes of `top_bar_text` when one changes.

## The HUD II: minimap, command panel, stats, army list (#327)

Same rules as above (`hud::Claim`, transparent host, whole canvas scale, diffing before resolving); four more parts in
`ready.hud`, each a `Claim` flag, and the groups they add (layer, low first; the top bar's and the later ones moved up):

| Part | Groups (layer) | Draws |
|---|---|---|
| `Minimap` | `minimap` (0), `minimap_frame` (1), `minimap_view` (2) | the map image alone; the two player-coloured lines and the `Border` frame; the camera rectangle (or the free camera's footprint) and the formation mark of the army icon under the mouse |
| `CommandPanel` | `commands` (3) | the type boxes, the command, production and build-menu icons (counts, endless marks as fills, pressed offset, gray `GRAY.SET` icons, half-transparent disabled ones) and the market's plaques |
| `UnitStats` | `stats` (4), `unit_panel` (6) | the numbers of the single selected type (life, attacks, shield, protections, reload bar, kills, info lines; split out of the command panel's draws, `Layer::Stats`) and the unit side panel (key U) with its portrait; the game panel (key I) stays with the engine (`Layer::GamePanel`, never claimed) |
| `ArmyList` | `army` (5) | the army icons |

No protocol type was added: the minimap and the portrait are textures the engine composes (`Sprite::Image`), which the page fetches
as a PNG asset, `image/<name>/<version>` (`AssetCache::add_image`, the last three versions kept); the part is an ordinary
`Part::Sprite` whose url carries the version, so a group is resent when its draws are the same but an image is new (`SentGroup.images`).
The gray icons are `remap/<player>/<gp>/<frame>/<table>/<row>` (`Sprite::GameFrameRemap`). The minimap gets a new version at
most four times a second (`MINIMAP_EVERY`, `WebUi::set_image`; the driver takes every one), and is composed without the camera
rectangle when the UI claims it (`Hud::set_ui_camera`), so a scrolling camera only changes the four fills of `minimap_view` and never
recomposes or refetches the image; unit dots (2 x 2) and the fog stay in the image, one 240 x 240 picture instead of hundreds of parts.
The driver's asset check skips image versions a newer one replaced (`newest_images`). The `hud_scale` zoom (anchor, scale, corner pull)
is the section's, as before. Input stays the viewport's: Rust hit-tests the icons, the minimap and the army list.

Scenario `svelte_hud_panels` (and `produce_unit`, `endless_production`, `build_site`, `formation`, `formation_group`, `army_list`,
`attack_ground` under `--ui classic`): the minimap's image and camera group, a selection filling the panel and the stats, a minimap click moving the camera.

Parity (`ui_parity.py`, the blank grass field with ten peasants picked by a double click, 1024x768, dpr 1; all claimed parts drawn by the page): `CommandPanel`
100.000%, `UnitStats` (one peasant) 100.000%, `UnitPanel` (key U, the portrait) 100.000%, `Minimap` 100.000%, layout 0 px each. `ArmyList` is not in the registry yet and stays engine-drawn (#467): it has
no pixel run: the list needs a formation, and the officers' formation icons stay disabled on the blank field (a mission map cannot be started by the
browser's driver); its icons are the command panel's `draw_icon`, and `army_list` passes under `--ui classic`.

Patch size and rate (`ui_patches.py`, new phases: forty peasants patrolling, ten selected): 6 patches a second, 3.5 KB/s (median 257 bytes: a count or
the camera, the new minimap picture is one 150 byte group and an asset fetch four times a second); the mouse moving over the command panel
12.7 patches a second, 5.2 KB/s. With the minimap and its frame in one group it was 12.8 KB/s. The page's frame intervals in these phases are 33 ms
(median) with the UI and with `?ui=engine`: the walking units cost SwiftShader, not the UI.

## The HUD III: game panel, test editor, mortars (#328)

Same rules again (`hud::Claim`, transparent host, whole canvas scale, diffing before resolving); two more parts in
`ready.hud`, no protocol type: every draw of both is a frame, a string or a fill.

| Part | Group (layer) | Draws |
|---|---|---|
| `GamePanel` | `game_panel` (6, beside `unit_panel`: one panel is open at a time) | the game panel (key I): the paper box, the tab strip, the page of each tab (forces, deaths, market, income) clipped to its window, the scroll bar |
| `Editor` | `editor` (12, over `messages`) | the test editor's window as `Editor::draws` makes it: the frames of both boxes, title row with handle, fold and swords/`Schlacht` buttons, icon grid, combo box, colour swatches, the extras box, the Natur and Gelaende lists, the battle view, the corner grip |

The editor's window is taken out of the engine's overlay beside the mission's messages (`InGame::engine_overlay`); the save notice
stays with the engine. Moving the window sends a `HudMove` (101 bytes) because the combo box's and the scroll bar's clips were
widened to the window's content (they clipped to the whole screen, which does not move with the window).
Input stays the viewport's and the engine's hit-testing (`editor_icons`, the driver's labels) decides every click.

Parity (`ui_parity.py`, 1024x768, dpr 1, blank grass field, counted over the window's box, layout 0 px each): `GamePanel` (forces) 100.000%,
`GamePanelDeaths` 100.000%, `GamePanelMarket` 100.000% (the first run after a rebuild failed once while the panel was still loading, then 100.000% twice), `GamePanelIncome` 100.000%;
`Editor` (placing view) 100.000%, `EditorCombo` (nation combo box open) 100.000%, `EditorNature` and `EditorTerrain` (the Natur and Gelaende lists) 100.000%, `EditorBattle` and
`EditorBattlePlaced` (battle view, before and after "Aufstellen") 100.000%, `EditorMoved` (the window dragged by its handle with the page's mouse) 100.000%.

Scenarios under `--ui classic`: `svelte_game_panel` (open, every tab, close), `svelte_editor` (window, drag, battle view, Aufstellen) and
`svelte_mortar` (mortar icons in the command group, zone mode, a zone set by a drag, fire); the existing `editor_*` scenarios pass unchanged.

Patch rate (`ui_patches.py --editor`, swiftshader, blank field with the editor, six seconds each): idle 0.3 patches a second (the window's
first resolve when its files finish loading), the window dragged 21 patches a second, median 101 bytes, 2.1 KB/s (before the clip change:
16 patches a second of 30 KB, 499 KB/s), the game panel open 0.2 a second (one 8 KB patch), frame intervals at the 16.7 ms vsync in every phase.

Stayed with the engine:

- **Diplomacy panel (F8, and tab 4 of the game panel):** DipServer's CML pages are drawn into the engine's overlay (`InGame::dip_draw`); every draw is a frame,
  string or fill, so the protocol could carry them, but only a campaign mission with a `DipServer.dll` has them, and the browser's driver cannot start
  a mission map (and its world moves, so there is no deterministic frame). Not measured, so not claimed; follow-up.
- **Mortar zones on the map:** the dashed circles, the minimum-range ring and the zone under the cursor are world overlays (like the brackets), drawn by the engine in world
  coordinates. The four mortar icons are part of the command panel's group and are drawn by the UI (measured with the command panel's route, which has no mortar:
  the icons are the same `draw_icon` draws).
- **The mortar icons' parity** has no pixel run of its own: the mortars rule needs a mortar unit (`ArtMrt`) and the page's driver cannot start with the rule.
- **Pause text:** still in the engine's overlay.

## Native host (Linux, #333)

The desktop build can show the menus through the Svelte UI: `native_ui=svelte` in `settings.txt`
(`engine` by default, as the original; read at start, no in-game switch yet). It takes effect only in a
build with feature `webview-gtk` (Linux; `libwebkit2gtk-4.1-dev libgtk-3-dev`) that embeds the bundle
(`webui::bundle::PRESENT`); otherwise the engine draws and a log line says why (`webui::native_ui_active`).
The spike below found that a transparent child webview cannot show the wgpu frame under it
(wry#1319), so the host is **opaque and only for full-screen menus**:

- `webui::native` (`webui/native.rs`, written against wry's cross-platform API): one wry child webview
  covers the whole window (bounds follow `Resized` and `ScaleFactorChanged`), black background
  (`ui/public/index.html`). The window runs on X11 (XWayland under Wayland: `gdk::set_allowed_backends`
  and winit `with_x11`, chosen before the event loop exists, which is why `native.rs` reads
  `native_ui` from `settings.txt` first); GTK is pumped in `about_to_wait`, which keeps the loop
  waking at least every 8 ms while the webview is shown. Xlib errors above the core codes (wry's child
  window on XWayland) are dropped by winit's error hook.
- `acr://localhost/` serves `webui::bundle::file`; `acr://localhost/asset/v1/<path>` goes as
  `Event::UiAsset` to `webui::assets::resolve` (status and `x-acr-error` as `ui/src/lib/bridge.ts`
  `nativeBridge` reads them). `AssetReply` is `Send` off the browser for wry's handler.
- UI to Rust: the IPC handler sends `Event::Ui`, the same event as the browser's `acr_ui_send`, into
  `webui::input` (hit-testing, keys and cursor stay Rust's). Rust to UI: the same `ToUi` diffs of
  `WebUi::publish` as `evaluate_script("window.__acr_ui.recv(...)")` (`WebUi` is created by the page's `ready`).
- **Visibility:** the webview is shown exactly while `WebUi::shown()` (the UI draws the current screen:
  a screen of the registry that is not `Screen::in_game`, so no `OVER_GAME` screen) and the intro movies
  are over; hidden otherwise (focus goes back to the window) and the engine draws as without the setting.
  While shown, keys and mouse go to the webview and reach Rust over IPC; while hidden, winit gets them.
  The UI's cursor is used only while shown (`External::cursor`); the engine draws its own otherwise.
- **Engine-drawn, always:** the HUD (`WebUi::hud_claim` is empty on a non-transparent host), the dialogs
  over the game (game menu, options, save and load, message, choice and history windows), the loading
  of a game's world, the intro movies. The frame is still rendered under the shown webview.
- A hidden webview starts as such: wry maps a child window that starts hidden, so `Host::new` toggles
  its visibility once.

Checked on GNOME/XWayland (RADV): main menu shown (`native_ui=svelte`), the first mission with the engine
HUD and its message window, webview hidden. Not measured: hiding after showing during a menu-to-game
transition (needs input into the window), Wayland-native windows (wry has no child webviews there),
HiDPI scale, resize flicker, Windows and macOS hosts.

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

The touch skin, the rest of the HUD (the pause text, the diplomacy panel, the world overlays), the native host on Windows and macOS, the other screens, and the start screen's layout for other screen ratios
(#316 section 8).
