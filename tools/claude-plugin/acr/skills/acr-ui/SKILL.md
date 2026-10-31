---
name: acr-ui
description: "The ACR interface: canvas coordinates (1024×768 base), the HUD in hud/ and interface.rs, fonts and dialog frames, the Spieleinstellungen pages, adding a settings.txt key end to end, the test editor, the control schemes, German labels and driver checks. Use for HUD, menu, dialog or settings work."
---

# The interface

Load `acr-workflow`, `acr-vm` and `acr-driver` with this. Match the original's layout (docs/ui.md) and cite its addresses.

## Coordinates (`crates/acr-game/src/canvas.rs`)

| Canvas | Use | Rule |
|---|---|---|
| `Canvas::fit` | menus, game menu, options, save/load, mission windows | 1024 × 768 letterboxed: scale `min(w/1024, h/768)`, origin centred |
| `Canvas::screen` | in-game HUD | same scale, origin (0, 0); `RealLx × RealLy` = window ÷ scale, at least 1024 × 768 (`screen_size`) |

Window px = origin + canvas × scale (`to_window`; back with `to_virtual`). The driver's
`pos` is window px. At `acr-vm-drive`'s default 1600 × 900: scale 1.171875, screen 1365 × 768.
Side panels are laid out for `RealLx` = 1024 and moved by `RealLx − 1024`.

## Where things live

| File | Holds |
|---|---|
| `hud/mod.rs` | `Hud` (`draw`, `press`, `drag`, `right_press`, `covers`, `toggle_*`); layout constants `ZONE1` (6, 6), `PROPERTY` (182, 6), `ICON` 40, `ADD_ICON` 2, `PANEL_LY` 144, grid 12 × 3, `RES_PANEL` 716, `RES_STEP` 110; packs `gp::{RESPANEL, ICONS, SMICONS, BORDER, CURSGO}`; fonts `font::*`; `border`, `outline`, `tooltip` |
| `hud/panels.rs`, `minimap.rs`, `cursor.rs`, `data.rs`, `produce.rs` | side panels (U, I), minimap, cursor, `HudData`, production icons |
| `ingame/interface.rs` | the HUD hooked to the game: `left_button`, `right_button`, `key`, `mouse_moved`, `scroll_view`, `apply_settings`, `interface` (the HUD draws of a frame), overlays (brackets, life bars, move marker), `canvas_point` |
| `ingame/panels.rs`, `gun_marks.rs`, `shouts.rs` | what side panels show; cannon range lines; panic shouts |
| `ingame/controls.rs`, `ingame/editor.rs` | modern controls; the test editor |
| `ui/dialog.rs` | `SimpleDialog` widgets: `add_text_button`, `add_gp_text_button`, `add_combo_box`, `add_slider`, `add_list_box`, `add_text_viewer`, `add_input_box`, `press`, `key` |
| `ui/draw.rs` | `Draw::text`/`frame` (menu palette `1\AGEW_1.PAL`), `Draw::game_text`/`game_frame` (game palette `0\AGEW_1.PAL`), `fill`, `clipped` |
| `ui/fonts.rs`, `ui/settings.rs`, `ui/screens/*.rs` | font list; `Settings`; screens (`game_options.rs`, `options.rs`, `game_menu.rs`, …) |

## Fonts and frames

- A font is a named `RLCFont`: `FontId("SmallWhiteFont")`. Only names in `MENU_FONTS`
  (`ui/fonts.rs`) are loaded; add a new one there.
- In use: `YellowFont` resource values; `SmallWhiteFont` labels and `SmallYellowFont`
  clickable lines (editor panel, side panels); `SmallBlackFont1` tooltips (wrap 100);
  `SmallYellowFont` the menus' hint line at (440, 745); `BigWhiteFont` the pause text;
  `fn10` the market; `fon18w`/`fon18y3` game menu rows; `WhiteFont`/`YellowFont` options combos.
- Use `Draw::game_text`/`game_frame` in game and `Draw::text`/`frame` in menus: the other
  palette gives wrong colours.
- Frames: `BORDER.GP` (box edges tiled every 64 px with corner tables, `hud::border` +
  `BOX_CORNERS`; tooltip frames 58–67; panel headers); the options dialog
  `INTERF2\OPTIONS_FOR_GAME.GP`, combos `INTERF2\COMBO2.GP`, sliders `INTERF2\SCROLL2.GP`,
  OK/Abbruch `INTERF2\G_LOAD_GAME.GP`; game menu `Interf2\GameMenu`. The editor panel has no
  frame: a black fill (alpha 170). Formats: docs/formats/fonts.md, gp.md.

## Texts and German labels

- Text is bytes (CP1252) from file to screen; each font maps bytes to glyphs. Original texts
  come from `GetTextByID` (`Text\*.txt`, `Comment.txt`, ...); reuse those IDs where they exist.
- ACR's own labels are German like the game's, as byte strings with Latin-1 escapes:
  `b"K\xf6rperabstand"`, `b"Geb\xe4ude"`, `b"Zur\xfcck \xbb"`. A Rust `"ö"` is two UTF-8 bytes
  and draws as two wrong glyphs. The editor uses ASCII in `&str` (`"Suedost"`).
- The choice that keeps the original's behaviour says so: "Aus (Original)", "Fest (Original)".
- The driver's `ui` tree decodes labels to UTF-8, so `click` by `"label": "Körperabstand"`.

## Spieleinstellungen (`ui/screens/game_options.rs`)

- Page 1 is the original (`Options_for_Game`: sliders, combos, the `Steuerung` combo).
  Pages 2 `BATTLE` and 3 `GRAPHICS` are `&[Row]` (`Heading`, `Choice { label, lines, get,
  set }`, `Note`), one per `ROW_Y` (203 … 446, 27 px apart): headings on 203, 257, 365, a box
  on every other row where the frame has one drawn in.
- **Six pages (#374):** 1 original, 2 blood/decay/marks, 3 graphics and landscape, 4 and 5
  "ACR-Regeln (neue Schlacht)" / "Befehle" with the `acr_sim::Rules` flags (`RULES`, `RULES_MORE`). Pages 2 and 5 have free rows; page 3 is full (combined boxes such as
  "Schatten, Details", "Wasser, Boden"). A new choice goes to the page of its kind; a new optional
  rule goes to `RULES` or `RULES_MORE`. Paging lives in `PAGES` / `rows()` ("Seite x/6"); the row test allows short pages.
- OK takes all pages (`collect`), Abbruch none. Test: `every_choice_round_trips_its_setting`.
- Open it in a game: `key Escape` (game menu), `click "Spieleinstellungen"`, then the page buttons.

## A settings.txt key end to end (model: commit e0cfd78, `decay_stain`)

1. `ui/settings.rs`: field with a doc comment (`key=a|b`, default, doc link), `Default`,
   `encode` (an optional key is written only once set: `decay_stain: Option<bool>`), a `decode`
   arm that clamps; extend `settings_file_round_trip`.
2. Consumer: 3D in `ThreeD::apply` (`three_d.rs`), interface in `InGame::apply_settings`
   (`interface.rs`); a sim rule goes into `acr_sim::Rules` at game start (next battle).
3. Dialog: a `Row::Choice` with German lines (mind the full pages).
4. Driver: a blank-start key in `drive/protocol.rs` (`Start::Blank` field and its parse) and
   its use in `drive/mod.rs` (`Request::Start`); add it to docs/testing.md "The blank field".
5. Docs: the settings table in docs/rendering-3d.md "Settings" (or the topic's doc), the page
   table in docs/ui.md "Game menu (0x5d9c30) and game options (0x5dbe10)".
6. Never edit `ACR-analysis/showcase/settings/settings.txt` (the user's, used by `acr-vm-drive`).

## Controls (`ingame/controls.rs`, docs/controls.md)

- `controls=original|modern` and `edge_scroll=0|1` (`Settings::controls`, `CONTROL_CHOICES`:
  "Steuerung: Original", "Steuerung: Modern", "Steuerung: Modern, Rand aus"). Original is the
  default; every modern entry point returns at once otherwise.
- Modern adds wheel zoom, Ctrl + left or middle drag panning, W/A/S/D, Home, leader chords and
  more; tests use it. New keys must not clash: docs/controls.md "Keys the modern scheme takes".

## Editor (`ingame/editor.rs`, docs/controls.md "Editor (test field)")

- Panel at canvas (8, 64), 300 wide, rows of 16, 14 list rows. `Editor::layout` gives
  `Item { rect, text, hot, act }`; `Act` is the click target; `apply`, `place`, `wheel`.
  `InGame` hooks: `editor_left`, `editor_right`, `editor_wheel`, `editor_key` (F2 folds),
  `editor_overlay`. Driver: blank start `"editor": true`.
- Panel items are not in the `ui` tree: click them by `pos` = item rect centre × scale. Keep
  the panel table in docs/controls.md current.

## Verify with the driver

```json
[{"cmd":"start","blank":{"ground":"grass","editor":true}},
 {"cmd":"wait","ticks":5},
 {"cmd":"ui"},
 {"cmd":"click","pos":[53,103]},
 {"cmd":"screenshot","path":"panel.png","crop":[0,0,400,640]},
 {"cmd":"quit"}]
```

- `pos` [53, 103] is the ENG tab: canvas (45, 88) × 1.171875 at 1600 × 900. The HUD is on by
  default; never pass `"hud": false` when the HUD is under test.
- `ui` lists widgets (`<Screen>/<index>`, HUD icons `hud/<slot>`, rects in window px, `cursor`,
  `messages`); `click` by `label` (exact wins), `id` or `pos`; `move` for hover hints.
- Judge by the tree and `state` first; screenshot only for looks, cropped.
- Before/after: same steps on main and your branch, `acr-shots sheet` side by side.
