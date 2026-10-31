# Menus and dialog system

How the original menus of `dmcr.exe` work, recovered from the disassembly, and how
`crates/acr-game/src/ui/` rebuilds them. Addresses are virtual addresses in `dmcr.exe`
(v1.46); the exe exports its C++ symbols (`DialogsSystem`, `SimpleDialog`, ...), so most
functions have names. The widget code is the same as in the published Cossacks: Back to War
sources (`Dialogs.cpp`), constants included.

The Svelte UI draws these screens as DOM from the same Rust state (docs/webui.md).

## Screen loop

Every menu screen is a function that builds a `DialogsSystem` on the stack, then loops:

```
ItemChoose = -1
do { ProcessMessages(); StdKeys();          // StdKeys 0x59bde0: Enter -> 0xf5, Esc -> 0xf6
     DS.MarkToDraw(); DS.ProcessDialogs();  // 0x47cfcd: input + drawing of all widgets
     FrameLimiter(); DS.RefreshView();      // cursor, flip
} while (ItemChoose == -1)
```

Buttons set `UserParam` and `OnUserClick = MMItemChoose` (0x59bd60), which stores
`UserParam` in the global `ItemChoose` (0x28ccc58). The screen then acts on the value:
`0xf5` (mcmOk) and `0xf6` (mcmCancel) are OK and cancel everywhere. In ACR a screen is a
`Screen` with a `Dialog`; `Dialog::press`/`key` return the chosen value.

All menu screens use palette `1\agew_1.pal` (`LoadPalette` at the start of the main menu,
options, campaign, ...); their bitmaps carry exactly that palette. `LoadFog(1)` selects
the cursor pack `Cursor_01.GP`.

## Widgets (`SimpleDialog`, ctor 0x47c44a)

Flag byte: bit0 Enabled, bit1 Active (keyboard focus), bit2 PrevMouseOver, bit3 MouseOver,
bit4 NeedToDraw, bit5 MouseOverActive, bit6 IsActive (drop-down open), bit7 Visible.
`+0x7..+0x13` x, y, x1, y1 (inclusive), `+0x17` UserParam, `+0x1b` hint, `+0x2b` OnClick,
`+0x2f` OnDraw, `+0x3b` OnKeyDown, `+0x3f` OnMouseOver, `+0x43` Refresh, `+0x4b`
OnUserClick, `+0x4f` drop-down zone handler, `+0x53` overlay draw, `+0x5b` scroll owner.

`ProcessDialogs` 0x47cfcd, per widget in creation order:

- Hit test: `x <= mx <= x1 && y <= my <= y1`, **and** the mouse inside the current clip
  window. `addClipper` (0x493e61, draw 0x493dd8) changes that window for all later widgets,
  so it clips drawing and hit testing.
- A click is the **press** of the left button (latched on `WM_LBUTTONDOWN`). The first
  widget with `OnUserClick` under the mouse takes it: the click sound plays
  (`DialogsSystem::ClickSound` = `STANDART`; `OkSound` for 0xf5, some screens set it to
  `START`), then `OnUserClick` runs. Then `OnClick` runs; returning true moves the keyboard
  focus to that widget.
- Disabled widgets are drawn but not hovered or clicked.
- An open combo box (`DS+0x1446`) takes all mouse input inside its drop-down zone; a click
  outside it closes the list and still reaches the widget underneath.
- Hover sound: a widget with `AssignSound(GETS("@MOUSESOUND"), 0x4321)` plays `SCROLL`
  (`Sound\MenuSvitok1..3.wav`, one at random) when the mouse enters it.

Sounds are named in `SoundList.txt` (`NAME count` + paths); `GETS` reads `Const.lst`
(`@MOUSESOUND SCROLL`).

| Widget | Creator | Behaviour |
|---|---|---|
| Picture | `addPicture` 0x46d6dc | bitmap at x, y (backgrounds) |
| GPPicture | `addGPPicture` 0x46e4b9 | GP frame with its origin at x, y |
| TextButton | `addTextButton` 0x46ebc9, draw 0x46ea22 | `(x, y, text, active, passive, disabled, align)`; font: disabled, else hovered → active, else passive; align 1 centres on x; rect = text width × (font height + 1) |
| GP_Button | `addGP_Button` 0x470a5c | `(x, y, gp, active, passive)`: active frame under the mouse |
| GP_TextButton | `addGP_TextButton` 0x46f0eb, draw 0x46ee14 | text centred on a GP frame that is drawn only under the mouse |
| ListBox | `addGP_ListBox` 0x474459, draw 0x4737c4 | rows of 26 px, selected row highlighted with frame `base + row % 3`, selection on press (follows the mouse while held), Up/Down/PgUp/PgDn when focused; scroll bar linked with 32 units per row |
| VScrollBar | `addNewGP_VScrollBar` 0x472884, draw 0x4723ef | frames base+0/1 up arrow, +2/3 down arrow, +4 thumb, +5..7 track; arrows repeat every 100 ms, track click pages, thumb drags; hidden while max ≤ 0 |
| Slider | `addGP_ScrollBarL` 0x472add, draw 0x471d7f | horizontal; only the thumb drags |
| ComboBox | `addGP_ComboBox` 0x47aad6 | frames up+0/1 box hover/normal, +2 list header, +3/4 row highlight, +5/6 rows (variant `random.lst[i*17] & 1`), +7 footer; text at (x+24, y+3), list rows at (x+25, row+2); opens and selects on press |
| TextViewer | `addTextViewer` 0x47715b | word wrap (`GetNextLine` 0x476386), wrapped lines justified (≤ 5 px extra per space), blank lines dropped, `~` invisible |
| InputBox | `addInputBox` 0x475150 | text at +8, +4, caret `|` blinking every 250 ms |

Status line: screens with hints add a centred `SmallYellowFont` text button at (440, 745)
and copy the hovered widget's hint into it every frame.

The mouse cursor is frame 0 of `Cursor_01.GP` with hotspot (0, 0) (`curs_xy.dat`), drawn
last (0x496efe).

## Fonts and texts

Fonts: see [formats/fonts.md](formats/fonts.md). Texts come from `GetTextByID` (0x60a020),
which reads `Comment.txt`, `Missions\miss_*.txt`, `Text\*.txt` and `moretx.dat`
(0x609f40); each line is `ID text`, the first definition wins, an unknown ID shows itself.

Text stays bytes from file to screen: every font maps byte values to glyphs, so whatever
single-byte code page a localisation uses (CP1252 in this German data; other localisations
may use another) shows the way the original shows it without being decoded. Only typed input
is converted: the original takes the code page byte from `ToAscii`, the game encodes
the typed character as CP1252 (`acr_formats::CodePage`) and drops characters CP1252
lacks. Decoding to Unicode (`acr_formats::text`) is for tools and logs.

## Screens

### Main menu (0x5c6eb0)

- Background `Interf2\Background_Main_Menu.bmp` (with English entries painted in; they are
  covered by the map).
- `addClipper(342, 464, 676, 707)`; four pieces of a map, `Interf2\MainMenu` frames 1..4 at
  (255·i, 210).
- Entries: `addTextButton` centred at x = 510, y = 475 + 35·i, fonts `fonG60w` (hover) /
  `fonG60y5`, hover sound. Main: Einzelspieler 0xf1, Mehrspieler 0xf2, Laden 0xf7,
  Optionen 0xfc, Editor 0xf3, Beenden 0xff.
- Multiplayer entries at (180, 263 + 35·i): Internetspiel 3, Deathmatch 1, Historische
  Schlacht 2, Zurück 5. Single player entries at (830, 263 + 35·i): Kampagne 1,
  Einzelmission 2, Zufallskarte 3, Zurück 5.
- Choosing single player or multiplayer slides map and entries in 25 frames by
  (∓340/25, +250/25) = (∓13, +10) per frame (integer division), back the same way.
  Escape in a submenu means back.
- Frame `Interf2\MainMenu` 0 at (339, 458) over everything; `BuildVersion` "V 1.46+" in
  `SmallYellowFont1` right-aligned at y = 751.
- Single player: Kampagne → player name (0x5c6170) → campaigns (0x5ca690); Einzelmission →
  player name → single missions (0x5d66b0); Zufallskarte → player name → random map
  (0x5cc630 → 0x5cd790). Multiplayer: 0x5c95d0. Load: 0x5d8090. Options: 0x5c9740.
  Editor: not found (0x59c2a0 only draws `Interf2\_Loading` centred, 0x5bb6c0 below is the
  editor's placing window).

In ACR the editor entry does nothing; multiplayer shows its background and title only. The startup movies (`Video\1..10.bik`) play before the main menu.

### Player name (0x5c6170)

`Interf2\Background_Player_Name.bmp`; the names of `players.txt` (at most 10) as
`UniversalButton` strips of `Interf2\TextBox1` at (446, 255 + 26·i), width 467, style
`i % 3`; clicking one copies it into the name field (`addInputBox(594, 524, PlName, 16,
308×40)`, focused). OK (`Buttons` frame 0 at (472, 577), hint `IHINT_ACEPT_NAME`) is
enabled only with a name; cancel at (704, 577). The original shows hints at (490, 745) in
`fon13y2`; ACR uses the common hint line.

### Options (0x5c9740)

Background `Interf2\Background_options.bmp`. Titles `INTF_OPT_TITLE0` (512, 18) and
`INTF_OPT_TITLE1` (680, 217). Section labels at x = 443 (`fon18w`), item labels at x = 453
(`fon18y3`), rows 27 px apart from y = 258. Combo boxes (`Interf2\Combo` frame 0, drop-down
offset 9 − 5, `FontDy` − 2, `OneDy` − 1): resolution (619, 284), music mode `NOCD`/`RNDMUS`
(619, 340), game mode `SLOWMOD`/`FASTMOD` (619, 448). Sliders `Interf2\Scroll` frame 0,
292×12 at x = 621: sound 0..100 (y 376), music 0..100 (y 402), scroll speed `ScrollSpeed − 2`
0..8 (y 484), game speed `100 − FPSTime` (y 510). Record check button `Interf2\Checker`
frames 1/0 at (619, 525) with a `Textbox2` strip and the file name field. OK (0xf5) and
Abbruch (0xff) are `GP_TextButton`s on `Interf2\Buttons` frame 0 at (472, 577) and
(704, 577) in `fon18y5`, `FontDy − 1`.

In ACR the resolution sets the window size (the menus always scale 1024×768).

The music box has the original's two lines (`NOCD`, `RNDMUS`). An earlier ACR third line,
"Spotify mitmischen" (`music_mode=2`, ducking Spotify under the battle noise), was removed:
to listen to other music, switch the game's music off and keep the effects. A
`settings.txt` that still has `music_mode=2` loads as no music.

ACR adds one combo box for the control scheme (#129, [controls.md](controls.md)) beside the
section label `INTF_OPT_GO` at (619, 419), where the background has free paper between the
music slider and the game mode box. Its lines are German texts of ACR's own ("Steuerung:
Original", "Steuerung: Modern", "Steuerung: Modern, Rand aus"); the game texts have none.
The game options have the same box at +239, +366. ACR stores all options when OK is pressed,
as `key=value` lines in `settings.txt` in `%LOCALAPPDATA%\ACR` (the folder of
`game-dir.txt`; `ACR_SETTINGS_DIR` overrides it) or in `localStorage` entry `acr-settings`
in the browser, and reads them at start-up (volumes and window size included). The
original keeps its options elsewhere; ACR does not read or write those.

On a screen wider than 1024 the original anchors the resource bar and the "M" plaque to the
right edge (`RealLx`, 0x4ecc50), and ACR follows. `center_top_bar=on` in `settings.txt`
(off by default; page 2 of the options, "Leiste mittig"; #360) moves the plaque, the
values, their hint rects and the click region left by (`RealLx` − 781) / 2, so the plaque
and the bar (781 wide together) are centred on the screen and the filler tiles run the
whole width. Nothing else moves. It is a UI setting, not a rule: it is not in the Rules hash
or the savegames.

#### Touch top bar

`top_bar=classic|touch` in `settings.txt` (no options page; classic where the key is absent,
touch on Android, `Settings::top_bar_default`) replaces the resource bar and the "M" plaque
by two buttons (`hud/touch.rs`): **M** at the top left opens the same drop-down as the
classic "M" plaque (game menu, unit and game info, minimap, zoom, surrender, restart, ...),
anchored under the button, left aligned, with rows as high as the button (shrunk to fit the
screen, never below 24; the 24-pixel frame sits in the middle of a taller row), so nothing
is unreachable in touch mode; a second tap on M closes it. The gold icon at the top right toggles a compact panel under
it with the six amounts, each beside its icon (the classic bar's own `ResPanel` frame 0
sliced, same values and `YellowFont`, food and gold in the icons' order); a second tap on
the button closes it. The buttons are drawn as the drop-down's `Border` box. They are at
least 48 density-independent pixels high (1.5 times as wide): the layout takes the window's
scale factor (winit `scale_factor`) over the canvas scale, so a phone (1440 x 3168, scale
3.5, canvas scale 1.875) gets 90 canvas pixels, about 9 mm, and a 1584 x 720 window 52. The
area the classic bar covered is map: only the two buttons and the open panel take clicks, and
the hint rects of the resource values are off. It is a UI setting, not a rule: not in the
Rules hash or the savegames. The e2e driver switches it with
`{"cmd": "top_bar", "mode": "touch"}` (docs/testing.md).

### Single missions (0x5d66b0) and campaign missions (0x5c4e30)

Missions come from `Missions\Missions.txt` (0x569c27: id, name, script DLL, map,
description, count, count lines), the order from `Missions\SingleMiss.txt`.

- List: `addNewGP_VScrollBar(495, 136, 476)` and `addGP_ListBox(94, 156, 17 rows,
  Interf2\ListBox, frame 3, 26 px, fon18w/fon18y3)`, focused, `dy = −3`, `dx = 8`.
- Preview: `addBPXView(588, 120, 400×224)` with `Maps2\single<n>.bmp` (campaigns: the
  briefing's `.bmp`), under `Interf2\GameMap` frame 0 at (580, 115).
- Briefing: a `TextViewer` (605, 445, 342×226, fon18y3, line height 21) per mission with
  its own scroll bar (957, 434, 238): the first extra line of the record if it names a
  `.txt`, else the description file.
- Difficulty combo (213, 638), `@RMID_EASY` .. `@RMID_IMPOSIBLE`, hint `DIFFICULT5`.
- `Interf2\GameButtons`: start 0/1 at (899, 706) (`MISS_RUN`), cancel 3/4 at (959, 706).
- Titles `INTF_SMISS_T0` (campaign: `INTF_CMISS_T0`), `T1` (303, 107), `T2` (787, 106),
  `T3` (787, 401), `INTF_SMISS_DIFF` (98, 640).
- Campaign missions: the campaign's background from `Campaigns.txt`, 10 rows, OK sound
  `START`, cancel hint `WAR_CANCEL`. The list shows the campaign's missions up to the last
  one the player has opened (0x5c5432), see below.

The menu screens start the game from their own loop and continue after it, so a game
ends back in the list it was started from: the campaign's or the single missions' list
with the mission still selected; ACR returns random maps to the main menu.

#### Campaign progress

The campaign table at 0x28b7728 (count 0x28b773c, 0x18-byte records at 0x28b7740:
mission count at +0xc, one word per mission at +0x14) holds per mission: bit 0 of the
low byte = open, the three higher bytes = the missions a victory opens — the
`#id ON|OFF a b c` line of `Campaigns.txt`. A victory in campaign mission
(0x676bb8, 0x676bbc) sets bit 0 of those three missions (0x4bcbb1) and writes the
player's file (0x56ad86 → 0x56ac4e). The file `Players\<name>.txt` is `"%d "` words:
the campaign count, then per campaign its mission count and words. Reading (0x56ad98)
stops at the first count that does not fit `Campaigns.txt`; without a file the player
gets `standart player info.txt` (0x56af11). `acr_formats::missions::Progress` holds the
table; ACR writes it through the profile store (`Players\*.txt` on the desktop,
`localStorage` `acr-player:<name>` in the browser), which wins over the installation's
file when reading.

### Campaigns (0x5ca690)

`Interf2\Background_Campaign.bmp`, OK sound `START`. `INTF_CAMPAIGN_T0` and
`INTF_CAMPAIGN_W0` (512, 334). List `addGP_ListBox(311, 377, 5 rows, ListBox frame 6)`
with scroll bar (705, 364, 156); per campaign a `TextViewer` (311, 542, 400×90, fon16y3)
with its description. Start opens the campaign's missions. The encyclopedia video button
(862, 468) is hidden.

### Random map (0x5cc630 → 0x5cd790)

The create-game-room dialog shared with multiplayer, in its single player form:
`Interf2\Random_Map_Background.bmp`, title `INTF_CROOM_SCR1`.

- Players: rows at y = 162 + 26·i. Row 0: name field (93, y − 2, 120×24); others:
  `addGP_ComboBoxDLX(93, y, 128)` with `None` and four difficulties. Nation combo
  `DLX(223, y, 126)` with the nations of `Text\Nations.txt` and `???` (random). Colour
  flag `Interf2\PlayerList` frame `colour + 1` at (355, y + 1), team field frame 10 at
  (398, y + 5) with "X" at (413, y + 2).
- Settings: `addClipper(601, 185, 952, 340)`, labels at x = 606, `DLX(739, y, 208)` combo
  boxes (fon16y1/fon16y3) 26 px apart from y = 186: landscape, relief, resources,
  minerals (`Terrain\Res.dat` sections), map size `MS_*`, the seven `GMTP_TP*` options and
  the climate `#CL_ZONE*`; scroll bar (955, 175). Page tabs `INTF_CROOM_GS0..2` at y = 144.
- Preview `GameMap` frame 0 at (580, 450); `CreateGameRoom` frame 7 at (672, 689);
  `GameButtons` start 1/0 (899, 706), cancel 3/4 (959, 706).

The original writes the generated map as `RN<size> <seed> <options> <name> <n>.m3d`
(0x5d294e) with a preview `Preview\P<a><b><c>`; ACR passes the settings in
`RandomMapSetup` to the game instead.

## In-game interface

`crates/acr-game/src/hud/` draws it and `ingame/interface.rs` connects it to the game.
The original lays it out for the screen size (`RealLx` × `RealLy`, 0x735008 / 0x735004).
The view has no bottom panel: `0x676b84` is 1 and is never written, so
`PanelLy` (144) only moves the icon rows. Text and lines use palette set 0
(`0\agew_1.pal`) and its font colour tables. Sprite packs use the palette from
`acr_formats::sprite_palette`. The frame function 0x4c1a20 draws, in this order:
minimap (0x4c0a40), unit stats (inside 0x606710), icons (0x5a9fa0), then the
resource panel and "M" drop-down (0x4c1a70).

Layout constants (0x5aa6d2): `Const.lst` defines none of `Zone1X`, `PropertyX`, … , so
the defaults apply: `Zone1X/Y` = 6/6, `PropertyX/Y` = 182/6, `IconLx/Ly` = 40/40,
`AddIconLx/Ly` = 2/2, `ZonesNx/Ny` = 12/6.

### Screen size

The original's menus are fixed at 1024 × 768, but the game screen uses the whole screen:
every part of the interface is placed relative to `RealLx`/`RealLy`. ACR keeps the menus
(main menu, mission lists, …) on the letterboxed 1024 × 768 canvas (`Canvas::fit`). The
in-game interface uses `Canvas::screen`: the same scale, `min(width / 1024, height / 768)`,
so the interface has the size it has in the menus, but with the origin at the window's top
left and the window's size divided by the scale as `RealLx` × `RealLy`. One side is
1024 or 768, the other is longer (at least 1024 × 768, also for smaller windows). Mouse
positions go through the same canvas. The anchors:

| Part | Anchor | Where |
|---|---|---|
| Resource bar | frame 1 tiled from x = 0 to `RealLx − 716`, frame 0 there, "M" at `RealLx − 781`, top edge | 0x4ecc50 |
| "M" drop-down | centred on the button, from y = 60 | 0x4c6840 |
| Icon grid and box | `PropertyX` = 182, `RealLy − PanelLy + 6` | 0x5aa206, 0x5aa2f3 |
| Unit numbers | from `RealLy − PanelLy` (0x28ccc54): kills at `+ 6 − 20`, life `+ 46`, lines `+ 56` | 0x607f50 |
| Minimap | `(RealLx − w − 17, RealLy − h − 16)` | 0x4c0a40 |
| Side panels (U, I), diplomacy | column `RealLx − 259 .. RealLx − 15`; y fixed for `RealLy` ≥ 768 | 0x5aa5ac, 0x4ce310 |
| Diplomacy browser window | bottom `RealLy − 298` | 0x5c3770 |
| Hint and message lines | (4, `RealLy − PanelLy − 40`) and up | 0x493fc5 |
| Central text, pause text | centre of the screen | 0x4bc490, 0x4c0130 |
| Tooltips | kept inside `RealLx` × `RealLy` | 0x47d6b2 |
| Game menu, save/load, options, mission windows | centred: `((RealLx − W) / 2, (RealLy − H) / 2)` | 0x5d9c30, 0x5e16b0, … |

ACR draws the centred windows on the letterboxed menu canvas, which puts them in the
middle of the window at the same scale. The side panels are laid out for `RealLx` = 1024
and moved by `RealLx − 1024` (their layout depends on `RealLy` only below 768). The
cursor is drawn anywhere in the window, also beside the menu canvas.

### Resource panel (0x4ecc50)

- `Interf2\ResPanel` frame 1 is tiled every 256 px from x = 0. Frame 0 sits at
  `RealLx − 716`, and frame 2 (the "M" plaque) at `RealLx − 781`. Frames 3 and 4
  ("L", "F") are never drawn.
- Values come from 0x4a09d5: the player's resources minus amounts already reserved.
  They are drawn right aligned in `YellowFont` at `x0 + 119 + 110·slot`, y = 7. Slot =
  resource index with `FOOD` and `GOLD` swapped, so the order on screen is wood, food,
  stone, gold, iron, coal. Values ≥ 10 000 000 show `RES_LOT_MONEY` ("Endlos").
- Hints: viewports 105 × 20 at `(RealLx − 691 + 110·i, 7)` with `RESHINT0..5` (0x5aab4b).
  They are tooltips of the in-game `DialogsSystem` 0x28c8508 (`SmallBlackFont1`, wrap
  width 100), drawn as a box at (mouse + 20, mouse + 32): fill colour 185 plus `Border`
  frames 58..67.

### "M" drop-down (0x4c7360 input, 0x4c6840 build)

- Opening: a press strictly inside `x0 + 23..x0 + 44`, `8..26` of the plaque toggles
  `0x1100a79`.
- Rows: `addGP_TextButtonLimited(0, 0, 24·i, text, Border, 74 + i % 3, 77 + i % 3, width,
  WhiteFont, YellowFont)`. The plank frame repeats along the row (0x46ef12). The rows are
  centred on `x0 + 33`, start at y = 60, and are as wide as the widest text + 24. A
  `Border` box surrounds them.
- Rows: the text ids and the values the click callback 0x4c6550 receives are stored side by
  side (text at `-0x80(%ebp,i,4)`, value at `-0x100(%ebp,i,4)`). The callback dispatches
  through the byte table 0x4c67f2 and the jump table 0x4c6792; every row closes the menu.
  In build order:

  | Row | Value | Condition | Effect |
  |---|---|---|---|
  | `TUTORIAL` | 0x20 | tutorial (`SetTutorial`, 0x1100ac6) | sets 0x1100ac8 for `GetQuestPressed` |
  | `UM_CURR` | 0x21 | tutorial | shows the current tutorial message again (0x562b74) |
  | `UM_MENU` | 0 | always | game menu, like F12 |
  | `UM_INF1` / `UM_INF2` | 1 | `UM_INF2` while the unit panel is open | unit panel, like U (`0x1100a74` = 2) |
  | `UM_INF3` / `UM_INF4` | 2 | `UM_INF4` while the game panel is open | game panel, like I (`0x1100a74` = 1) |
  | `UM_MINI` | 3 | always | minimap size, like M |
  | `UI_LOCK1` / `UI_LOCK2` | 4 | `UI_LOCK2` while shown | inaccessible areas, like Q (`0x90d905`) |
  | `UM_TRANS2` / `UM_TRANS1` | 5 | `UM_TRANS1` while on | transparent buildings, like O (`0x90d680`) |
  | `4XSCALE_ON` / `4XSCALE_OFF` | 0x30 / 0x2f | `4XSCALE_OFF` while zoomed | zoomed-out view, like L (0x5fb4dd) |
  | `UM_SURRENDER` | 6 | not a replay or the editor | gives up |
  | `UM_NORT` | 0x49 | multiplayer | marks the game unrated |
  | `UM_REPLAY` | 0x25 | one player | restart (0x4c7130) |
  | `SENDR_L1` | 0x48 | multiplayer | resource transfer dialog |
  | `UM_HIST` | 0x26 | the local player has history entries (0x4c6cb0) | mission history (0x562bea) |

  The editor (`ED_TLSHOW`, `ED_TOPSHOW`, `NEWMPHDR`) and replay rows (`REP_*`) never appear
  in a game. ACR offers the single player rows without the tutorial ones: no mission
  script calls `SetTutorial` and the API does not implement it.
- Surrender (value 6) asks nothing: it ends the game as lost (`0x1100a78`, defeat state
  `0x11eaf4c` = 1, network command 0x5c → 0x4d930c), like `LooseGame`. ACR shows the defeat
  text and returns to the menus after 7 s (missions show no statistics; random maps have
  none in ACR yet). A finished game cannot be paused.
- Inaccessible areas (0x518306, drawn after the world and before the interface, not in the
  zoomed view): each blocked 16 × 16 cell of the land lock map gets its two diagonals in
  colour 0xfe and, towards free neighbours, its edges in colour 0xff, lifted by the ground
  height at its corners. Cells outside the map count as blocked. ACR draws the map's lock
  layer (`MapGrid::land`).
- Transparency: objects with the building bit (`OneObject+0x27` bit 4, set at creation for
  buildings) are drawn with `ShowGPTransparent` (0x504fad). ACR draws building sprites at
  half opacity.
- Zoom (0x5fb4dd): the world shift 0x90d6b4 goes from 5 to 3, so a 32-pixel cell is 8
  screen pixels, and the view is centred on the cell under the cursor. ACR scales the
  camera by 1/4. The original then moves the cursor to the screen centre
  (`SetCursorPos`) and draws terrain and objects with separate routines (0x5fb93e,
  0x5fbac1); ACR keeps the cursor.
  - Objects (0x5fbac1): every sprite pack loaded from the root folder has a quarter-size
    twin `L_Mode\<name>` with the same frames, loaded with it (0x4870ee; packs in folders
    have none) and mapped by 0x7c0c50; the trees' twin is `L_Mode\Der`, stones and holes
    `L_Mode\Stones`, `L_Mode\Holes` (0x5fde0f). The zoomed view draws the twin at its own
    size at the object's position / 4 with the frame offset `>> 2`. ACR does the same
    (`acr_render::scene::zoom_pack`) and scales the normal frame down for packs without a
    twin (ships, for example).
  - Terrain (0x5fb93e, `MiniMap4X.cpp`): a cached quarter-size picture of the ground,
    rebuilt per 32 × 32 block when marked dirty (0x5ffbd6 → 0x5fdddd): 8 × 8-pixel
    triangles with unscaled texture patches at random offsets, the vertex textures blended
    by random per-vertex weights, the vertex light with a random dither bit, every other
    row kept, water without noise, and stones, holes and trees baked in (so units always
    cover trees). ACR draws it the same way every frame (`TerrainRenderer::zoomed`); see
    the zoomed-out view in [formats/terrain.md](formats/terrain.md).

### Unit and command icons (0x606710 builds, 0x5a9fa0 draws)

- Icons are `OneIcon`s (0x44 bytes, drawn by 0x494c96) of `ICONS.GP`. The frame is the
  line index of the icon's name in `ICONLIST.TXT` (lookup 0x4f0516).
- Grid: 12 columns, 42 px apart, from (`PropertyX`, `RealLy − 144 + 6`) = (182, 630).
  Three rows are shown. Two sets share the grid:
  - the selected types of the free units, then one icon per selected formation
    (0x28f8190), in the first free slots;
  - the commands (0x28f9e30), at fixed slots `x + 12·y`.
- Box: the box from (180, 630) to (182 + 42·columns, 755) is drawn with
  `Border` and corner table 0x668580 = TL 0, TR 2, BL 6, BR 8, left 12, right 14, top 9,
  bottom 11 (0x47f48a: edges tiled every 64 px and centred on the box lines).
- **A single selected type** (0x5aa136: the set of selected types and formations
  0x28f8190 holds exactly one icon, `+0xc` = 1) changes the layout: its icon goes to
  (`Zone1X + 6`, 630) = (12, 630) instead of the grid (the driver's widget `hud/99`), in a box
  from x = 12 to `PropertyX − 2` = 180 open to the right (0x480166, table 0x6685a0: TL 0,
  BL 6, left 12, top 9, bottom 11, no right side), and the commands' box is joined to it
  (0x480198, table 0x6685c0: TL 1, TR 2, BL 7, BR 8, left 13, right 14). Without command
  icons the first box is closed (0x4800c8, table 0x668580). Then the type icon, the
  commands, and **after them** the unit stats (0x607f50, called at 0x5aa278; drawn only
  in this layout), so the reload bar at x = 173..176 shows just left of the joint. ACR
  drew the stats before the box until 2026-10; the box's left edge (x 170..179) hid the
  bar.
- A type icon has:
  - an outline in player colour `0xd0 + 4·nation`;
  - a life bar: two lines of colour 255 with the rest in colour 0, length
    `38·life/max`;
  - the count in `WhiteFont`, shadowed by `BlackFont` at ±1, at the top left.
  Clicking it keeps only that type selected. A formation's icon (0x606d10) shows its
  soldier type (+0x1a) with the number of its soldiers and their summed life, the hint
  `ARMYSTR` ("Armee", 0x606d91) and the formation's number (its slot in the owner's
  brigade list + 1, 0x606e6e); clicking it keeps only that formation selected (0x4d737f).
- The second number (`OneIcon+0x40`, flag 0x100, set by 0x494a0d, drawn at 0x4951a5): right
  aligned at `x + IconLx − 4`, its top at `y + IconLy −` the height of 'W' in `WhiteFont`;
  `BlackFont` at +1 / −1, then `WhiteFont`. Only formations have one.
- A highlighted icon gets a frame in `GetPaletteColor(255, 200, 64)`. A pressed icon is
  drawn 1 px down-right. A disabled one is drawn transparent.
- Army icons (`IconSet` 0x28f7658, built at the end of 0x606710 from 0x607d3b, drawn by
  0x5aa36a outside the editor): 5 columns × 7 rows from (6, 31), under the resource bar at
  the top left, 42 px apart. ACR draws them after the command icons and before the resource
  panel, as the original.
  - Slot 0: the `ORDERS_LIST` icon (`NRES.DAT [CONST] ORDERS_LIST Formation_Kare`, parser
    0x4ebb4e, frame 0x66ada4), when one of the first 30 slots of the player's brigade list
    (`(ny − 1)·nx`) holds an enabled formation (+0x2e bit 0). No hint, no outline. A click
    shows or hides the list (`0x28f9e54`, 0x6064a0).
  - With the list open, every such formation that still has a soldier (members from the
    fourth on whose object exists with the member's serial) at slot `5 + index`: its soldier
    type's icon, the number of those soldiers (flag 8), its number `index + 1` (flag 0x100)
    and the yellow frame when one of them is selected (0x49464d). A click (0x6064b0) selects
    it through `ImSelBrigade` (0x4d75a6 → 0x4e1365, network command 0x3b): without Shift the
    selection is cleared first; the members from the third on (flag bearer and soldiers)
    join it. ACR's modern controls pick like a file list instead (Ctrl toggles, Shift
    takes the range from the anchor, docs/controls.md). The mouse over one (0x606500 → `0x66a8e8` = the brigade's index) marks the
    mean position of its members on the minimap (0x4c107f): an octagon of eight
    `DrawLine`s in `GetPaletteColor(255, 255, 0)`, 5 px across the sides and 2 px across the
    corners, at `minimap + 1 + RealX >> 10`.
  - The driver reports them as `army/<slot>`, labelled `army` and `army <number>` (the
    original has no hints for them).
- Single selected type (0x607f50, called after the icons by 0x5aa278): the first object's
  numbers drawn straight over the map, in `SmallWhiteFont`, with `SMICONS.GP` icons from
  `NRES.DAT [WEAPONS]`:
  All positions hang from `RealLy − PanelLy` (0x28ccc54); at 1024 × 768:
  - `KILLUNI` ("Treffer %d") at (16, 610) if it has kills, even while unfinished; the rest
    only for a finished object;
  - its life, centred at x = 12 + (40 − w) / 2, y = 670;
  - up to 2 attacks at (53 + 55·n, 630), text at x + 18, y + 4: the attack indices 0..11 in
    order whose current damage (ADC+0x78, upgrades included) is not 0 and whose weapon kind
    lacks the `I` flag of `NRES.DAT [WEAPONS]` (0x40 of 0x23bbb84). The damage is
    `base + (current − base)·r/1000` with the type's own `DAMAGE` as base; `r` is 1000, or
    for `EX_SHOTS` buildings the garrison share (units inside with `USAGE` 0x15, 0x18 or
    0x20, ·1000 / `MaxInside`). The bonus is the formation bonus `+0x61` plus, for
    `SKILLDAMAGEMASK` indices, `SKILLDAMAGEBONUS · kills / 10`: shown as `%d+%d` when positive;
    when negative as `%d-%d` with `skill − formation bonus` (sic, 0x6081c2);
  - the shield `#GENSHIELD` at (53, 651): ADC+0xe0 with the formation's defence bonus
    `+0x62` as `%d+%d` / `%d-%d`;
  - up to 6 protections in 3 columns at (13 + 55·c, 681 + 19·r), not for buildings;
  - lines in `fn10` from (13, 680), 12 apart: `FISHSTR` ("Fisch %d/%d", the fish on board
    `+0x6f` and `FISHING`'s second value; ACR's simulation does not fish, so 0),
    `HABITAN`/`MAXHABI` (population and housing,
    `FARM`), `__ARTCAP0..3` ("Max. Kanonen:%d\%d": objects of `MAXINDEPO` group i and depots ·
    `ARTDEPO` i, a line skipped if its value is 0), `INSISTR` and `MAXINSI` ("%d+%d" with
    `AddInside` `+0xa5`) or `MAXINS1` (units inside and the capacity), then for a mine at
    work (0x5f7882: order 0x5f75b3) the name of its resource.
  - `SHOWDELAY` types (NM+0x172a bit 0): a reload bar of four lines in
    `GetPaletteColor(0, 255, 0)` at x = `Zone1LX + Zone1X − 5` .. `− 2` = 173..176, from
    y = 631 + 125·delay/MaxDelay down to 753 (`Zone1LY` = 100), with the sums of
    `delay` (`+0x8c`) and `MaxDelay` (`+0x8f`) over the selected objects of the type
    (the type record of 0x606ae0); not drawn while `delay ≥ MaxDelay` (just fired), full
    while loaded (`delay` 0). It fills from the bottom up as the reload runs, so the player
    sees the reload without looking at the unit. `SHOWDELAY` is set for the guns
    (`ARTPUS.MD` …) and the shooters (`ENGM18.MD` …); checked on the blank field with a
    cannon and three musketeers (`ACR-analysis/reloadbar`).
  A single formation without free units (0x606edd) shows a soldier's numbers with the
  formation bonus, and the reload bar sums over all its members (0x6073a1).
- Command slots wired so far:
  - The officer's formations (0x5f6869..0x5f6da3), for a single selected object or a
    selection of one type whose first object is an officer (`NM+0xa`, the nation's
    `[OFFICERS]`):
    - each of its formations at its `NRES.DAT [ORDERICONS]` slot (`LINE 8 0 …`: column 8,
      row 0) with the line's hint, enabled while a free unit of its drummer type stands
      within 1024 px of the officer (0x52c9d8) or it needs none; a click opens the
      formation (0x28ead08 = formation + 1, 0x5f5c68). A disabled icon closes it, and so
      does selecting another first object (0x6781dc).
    - An open formation shows its soldier types in the slots right of its icon, each with
      the type's icon, its name as hint and as number the largest size (slot count) with
      at least that many free units of the type within 1024 px of the officer, stepped
      down by the high byte of 0x28ead08; a type without a size left is disabled. A click
      sends network command 0x3a (`FormNear`: officer, shape, type; 0x5f5c77).
    - With a formation open, `DECFORM` (`Dec_Formation`, smaller) and `INCFORM`
      (`Inc_Formation`, larger) stand two and one slots left of the officer's first
      formation's icon (0x5f5d49, ±0x100); when no type has a size left the step goes back
      by one.
    - After forming, the formation's flag bearer and soldiers join the selection
      (0x41f198 → `SelectBrigade` 0x4e0f80 from member index 2 on).
  - A single formation without free units (0x606edd): the groups its shape's
    `ORD_GROUPS.LST` group can turn into (LINE, SQUARE, KARE; cavalry PRUS, TRI, SHER) at
    their `[ORDERICONS]` slots with their hints, sending network command 0x41
    (`Reform`, 0x606680; none is highlighted), and `FILLFORM` (`Add_Formation`, hint
    `FILLFRM`) at slot 12 (0x678a80), sending command 0x52 (`Fill`, 0x6066b0).
  - The state commands (0x6074f1..0x607a59), with or without an officer. 0x606889
    collects over the selected objects: the formations (`+0x29` bit 7, ids in 0x28f7690,
    count 0x28f9e24) and the types of the free units (0x28f81f8), `STANDGROUND` types
    (NM+0x172b bit 3), stand ground on/off (`+0x25` bit 4), towers (`USAGE` 5) and other
    buildings, `WEAPONINSECTOR` types (NM+0x2940), attack held/allowed (`+0x25` bit 1),
    `FRIENDLYFIRE` (NM+0x172d bit 5) and `ARMATTACK` (bit 7) types with their switches
    (`+0x2a` bits 1 and 2). Then:
    - slot 24 `SFREORD` ("Formation auflösen", `FREE_ORDER` icon, `Disband`) with
      formations and no free units;
    - with at most 12 formations and no free units (0x60755c) the grouping icon at the
      `UN?GROUP` slot (`NRES.DAT`: `UN?GROUP 14 Make_Group_Formation_OFF
      Make_Group_Formation_ON`, parser 0x4ec123; slot 12 and no icons without the line),
      without an outline: `CM_UNGRP` (first icon, click 0x606700, network command 0x59
      `UngroupFormations`) when the selected formations are exactly one group
      (`CheckUnGroupPossibility` 0x43e0b7), else `CM_DOGRP` (second icon, 0x6066f0,
      network command 0x58 `GroupFormations`) when every selected object that is alive and
      not hidden is in a formation (`CheckGroupPossibility` 0x43dfdb). Grouped formations
      are selected together (see the selection below; the groups: `simulation.md`,
      Formations);
    - with a `STANDGROUND` type and 1..24 formations plus free types: slots 25 `STANDGR` and
      26 `NOSTAND` (highlighted while some unit stands / does not; `StandGround`) and
      `GO&ATTACK` at its `[CONST]` slot (highlighted in the attack-move mode); with free
      units and at most 12 rows also the patrol icon at slot 14 (`HPATROL`, 0x606650): the
      next left click on the map (cursor frame 13, 0x66a90c) sends `Patrol`, Escape ends
      the mode;
    - with towers and no other buildings, with the stand-ground icons, or with a
      `WEAPONINSECTOR` type: `ENABLE_ATTACK` / `DISABLE_ATTACK` (`EATTACK` / `DATTACK`,
      0x4d76cf, `HoldFire`), highlighted while some unit attacks / holds;
    - Attack the ground (0x5f6385): `NRES.DAT`'s `ATTACKGROUND Attack_Ground 13` (icon and
      slot, parsed by 0x4ec18c) with the hint `ATGHINT`, outside missions (SCENINF+0x10,
      0x28b7758, holds the mission script) when every selected object that is not dying is
      of an `ARTPODGOTOVKA` type (NM+0x172c bit 1; the cannons). A click (0x5f5dad) starts
      the mode `0x28ead0b`: cursor frame 9 (0x66a910) over the map; the next left or right
      click on the map (0x4c28dc tests both buttons) sends network command 0x57 (0x4d8344,
      `AttackGround`) at that point with the selection, Escape (0x4a3218) ends the mode. The
      brigade panel (0x607347) offers it for formations of such types, which the data has
      none of.
    - `SHOTMYUNITS` / `KEEPMYUNITS` for `FRIENDLYFIRE` types (network command 0x5e, ACR's
      `FriendlyFire`) and `ARMATTACK` / `ANYATTACK` for `ARMATTACK` types (0x5f,
      `MeleeOnly`), highlighted while some unit has that setting.
    - The first selected object's idle icons (0x5f63f0): a finished own object whose type
      has `SHOWIDLE` (both) or `SHOWIDLEPEASANTS` / `SHOWIDLEMINES` (type flag 0x172f bits
      1 / 2; the town centres) shows `IDLE_PEASANT` / `IDLE_MINE` at their `[CONST]` slots,
      without a hint. A click (0x5f5d0a, network commands 0x42 / 0x43) selects all of the
      player's peasants with no order that are not inside a building (0x4e230f), or all
      finished mines with fewer workers inside than they take (0x4e279e).
    - The first selected object's switches (collector 0x606889, icons 0x607add..0x607d3b) at
      their `NRES.DAT [CONST]` slots (`STOPBUILD off on x y`, `KARTECH icon x y`; `ZASEV`
      has no line and uses the engine's default, frame 152 at slot 1): `STOPBUILD` for a
      `CANSTOPBUILD` type with fewer than 12 rows (network command 0x61, ACR's
      `StopProduction`: production waits); with one selected type `KARTECH` / `PREKARTECH`
      when a unit has canister (0x62 `Canister`: fire one volley 256 px ahead now; 0x63
      `LoadCanister`: take the canister stance), `FISHING` for fishing boats (0x64,
      `StopFishing`) and `ZASEV` for a finished mill (0x65, `StopSowing`: no automatic
      fields; switching it back on plants at once). Each two-state icon shows the frame and
      hint of the current state (`STOPBUILD_OFF`/`_ON`, `FISHING_ON_TXT`/`_OFF_TXT`,
      `MAKEZASEV`/`DONTMAKEZASEV`) and switches to the other. The `STOPBUILD` line's frames
      are swapped: the parser (0x4ec33e) stores the first word in 0x66ae60 and the second in
      0x66ae5c, and the icon code (0x607af1) draws 0x66ae5c while stopped, so a running
      building shows `Production_OFF` and a stopped one `Production_ON` (red).
- Mortar buttons (ACR, #301, not in the original; `ingame/mortar_ui.rs`): with the
  mortars rule on and own mortars (`ArtMrt`) among the selection, four icons in the
  command icons' style (owner outline, yellow frame while on) at slots the guns leave free:
  | Slot | Icon (`ICONLIST.TXT`) | Hint | Does |
  |---|---|---|---|
  | 15 | `Rally_Point` | Zielzone setzen | the next left click on the map adds a zone of 150 px there, a left drag one as wide as the drag (32..800 px); each selected mortar gets `MortarZones` with its zones plus the new one (at most 8, the oldest goes). Cursor frame 9 like the attack-ground mode; Shift keeps the mode; Escape or a right click leaves it |
  | 16 | `Cancel` | Zonen löschen | `MortarZones` without zones (also stops the fire) |
  | 27 | `FireKartech_Cannon` | Feuer frei | `MortarFire` on; highlighted while some selected mortar fires |
  | 28 | `Attack_OFF` | Feuer einstellen | `MortarFire` off; highlighted while some has zones and holds |

  The modern scheme adds the keys to the hints ("(Alt+Z)", [controls.md](controls.md)).
  The marks: a selected mortar shows the hatched grey ring of its minimum range (600 px)
  and a dashed yellow circle with a centre cross per zone (`gun_marks::mortar`); in the zone
  mode the zone being set follows the cursor (150 px) or the drag. Driver check:
  `ACR-analysis/genai/mortar/ui/` (`after.json`, the `state` answers' `mortar` field,
  `sheet_hud.png`, `after/overlay.png`).
- Hotkeys: `Data\Keyboard.dat` (in `Patch02.GS1`) maps `[CTRL|ALT|SHIFT] KEY : ICON`.
  The key presses the shown icon of that name.

### Production and building icons (0x609230)

`hud/produce.rs` reads the slots and keys of the nation files, `ingame/interface.rs`
builds the icons.

- They are built only while one type is selected. The list is the type's
  `[FIXED_PRODUCE]` entry (`ID x y [KEY]`). Each item goes to slot `x + 12·y`. When several
  items share a slot, the later one wins, because `AddIcon` 0x495758 reuses the slot (the
  four mines at `5 0`).
- An item is listed only if it is enabled (bit 2 of the object flags) and accessible
  (bit 1), and its `[STAGEDEPENDENT]` stages are done (`+0xad`). ACR takes this from the
  simulation's tech tree (`economy::producible`), which also applies `[ACCESSCONTROL]`
  (0x42ffed), `[UNITLOCK]` and `ENABLE` upgrades. Items that are not available are left
  out, not greyed. So a random map start offers only the fort (`EngKre`), and the fort
  unlocks houses and the mill.
- Icon:
  - the picture is the object's `ICON`, with an outline in the player colour;
  - the hint is built by 0x606180: `Name - Holz: 500 Stein: 100`. The name is the text
    `NAME.MD`. Each resource with a base price is followed by its current price. The resource
    names are the text ids `Wood`, `Gold`, …;
  - an icon the player cannot afford is drawn through `0\gray.set` (flag 0x200,
    `ShowGPPal` at 0x494dc5).
- For a unit producer:
  - the icon shows the number queued in the selected buildings (flag 8) and the head order's
    progress as its life bar;
  - left click: `Produce` (0x6088c0, 0x4d6226), five times with Shift; with Ctrl one endless
    order (bit 0x2000 of the type word, command 0x4e), queued without the `CanAfford` check;
  - an endless order counts 1200 (`GetProduceCount` 0x49e060): from 1200 the icon draws the
    infinity sign as twelve line segments (0x494ad5 through 0x443ea2: colour 0 shifted by
    (2, 2) under colour 0xae, at (3, 3) of the icon) and, if `count / 1200 > 1`, that number
    at the top right (0x495294). The font of that number (0x90d69c) is taken as the yellow one;
  - right click: `Unproduce` (0x608810, 0x4d6e08), five times with Shift; it removes an
    endless order like any other;
  - the key from the list presses the icon.
- Several selected buildings: ACR sends a new unit to the building with the shortest queue
  and a cancel to the one with most units of that type queued. The original's choice in
  0x4d6226 is not traced.
- Pages: slots from 36 on are pages 2 and 3. Page buttons sit in column 0 (slots 0, 12, 24),
  with the building's `PAGEn` icon and `PAGEnHINT`. They appear for the pages that have icons
  outside column 0 (flags 0x28cff78..7a). The current page is stored per building type
  (`+0x183c`, 0x608c60) and is highlighted.
- Upgrades (entries of kind 1): those the building offers now (`upgrade::available`) at
  their `#POSITION`, with the `ICON` of `[UPGRADE]`. The hint is the message and, on a
  second line (0x49483d), the price as `Holz:250 Gold:25` (0x6062a0). A click sends
  `Upgrade` for the selected buildings. A running research shows its progress as the life
  bar; a right click sends `CancelUpgrade`. The original's click and cancel handlers
  (0x608b00) are not traced.
- Rally point: `NRES.DAT [CONST] SET_DEST 11 2 Rally_Point` for `[CANSETDEST]` buildings.
  The next click on the map sets it. A right click on the map with only buildings selected
  sets it too.
- Leave: units inside a selected building show as one icon of their type with the count, at
  slot 23. A click lets them all out (`Leave`). The original's icons for units inside (around
  0x6072af) are not decoded; the slot is ACR's choice.
- The selected types take the first slots the commands leave free (`AddIcon` without a
  position, 0x495631). Both sets are drawn at the same origin (0x5aaa15).

### Production messages and limits (0x6088c0, 0x609230)

- A left click on a unit item (0x6088c0) checks, in this order: more than 65000 objects →
  `UNILIMIT` ("Einheitenlimit erreicht!") for 50 frames; a field (`SPRITEOBJECT`) → the
  command 0x3d plants fields around the selected mills, without a price check
  (`PlantFields`); a building → the build mode; otherwise `CanAfford` and the order (five
  with Shift).
- `CanAfford` (0x514c65) lists the resources the player lacks, joined by "," without
  spaces, in `NETRES1` (one) or `NETRESS` (several), both "Nicht genügend Ressourcen %s",
  on the message list for 70 frames.
- A grey icon (flag 0x200) is still clickable: the button registration (0x4959d9) does not
  test it.
- Depot limit: `MAXINDEPO g n` (group g < 4, n per depot) and `ARTDEPO a b c d` (a depot,
  +0x174a..+0x174d). The census (0x42ec4f) counts the player's finished depots and its
  objects per group; an item is greyed while its group's count ≥ depots · n (0x609abd).
- `MUSTREPAIR` (0x60988b): a stage upgrade (`UPSTAGE0..4`) of the first selected object is
  greyed and has no click while that object burns (OB+0x26 bit 1) or, being a building
  (OB+0x27 bit 4, set at creation 0x516212 for `BUILDING` types that are not
  `WALLOBJECT`), has less than 2000 life (`WORD +0x78 < 0x7d0`). The text id has no text, so
  the id itself is the hint.
- `FINBLD` ("%s: Einheitenproduktion abgeschlossen", the building's name) for 100 frames
  when an own building's queue becomes empty because its last unit was made.
- Population full stops production silently (0x49ef92); the icon is not greyed.
- Depot limit in production (0x49f21b): when the head order's type has `MAXINDEPO g n` and
  the player has `n` · finished `ARTDEPO` buildings or more objects of group `g`, the
  order is cancelled with a refund (`UnProduce` 0x4dbd47, which takes a waiting order of the
  type first). ACR's simulation counts the objects on the spot instead of using the census.
- `NOT_ENOUGH_PEASANTS` (0x49f16b, `AssignHint1` for 10 frames): a `PEASANTCONVERTOR`
  building whose head order is not a peasant and that has no peasant inside waits, and
  says so every tick while the local player has it selected.

### Market (`ECO::BuildIcons` 0x4a96d9)

A finished building with `TORG` shows the market in the command grid instead of a window:

- Per market slot i (`ECONOMY.TXT` order FOOD, IRON, WOOD, COAL, STONE, GOLD) the sell icon
  at slot `(i >> 1)·12 + (i & 1)` and the buy icon 7 slots right, with the resource name
  as hint; the chosen ones are highlighted. Buttons (icon names from `ECONOMY.TXT`, hints
  from `COMMENT.TXT`): 2 `r_Mins10` `ECONOM1`, 3 `r_Plus10` `ECONOM2`, 4 `r_Buy` `ECONOM5`,
  5 `r_Mins1k` `ECONOM3`, 6 `r_Plus1k` `ECONOM4`, 26 `r_Mins100` `ECONOM6`, 27 `r_Plus100`
  `ECONOM7`, 28 `r_Clear` `ECONOM8`, 29 `r_Mins10k` `ECONOM9`, 30 `r_Plus10k` `ECONOMA`.
  No right clicks.
- The amount buttons add their value (×100 with Shift), never below 0 and not limited to
  the stock; `r_Clear` sets it to 0. Amount and chosen slots stay for the whole game.
- 0x4a9c70 draws `Border` 88 at (180, 590) and 89 at (475, 590) with `ECOSELL` ("Angebot")
  centred at (224, 595) and `ECOBUY` ("Nachfrage") at (518, 595) in `SmallYellowFont`.
  The amount (once a resource to sell is chosen) is centred at (308, 686), the result
  `trunc(amount · Sell / Buy)` (once both are chosen) at (434, 686), in `WhiteFont`.
- `r_Buy` sends the network command 0x3c (nation, sell slot, buy slot, amount) to
  `PerformOperation`; ACR sends `Trade` with the slots' resources.

### Placing a building (0x608a0d, 0x4bdd1e)

- A building item of a peasant starts the build mode (`BuildMode` 0x10fc644,
  `BuildingID` 0x10fca60), but only if the player can pay (`CanAfford` 0x514c65). Walls
  (`WALLOBJECT`) start it without the check.
- Preview: the building's idle frames at the mouse position plus `LOCATION`, drawn through
  a colour table:
  - row `N = 5 + (sin(t / 100 ms) + 2)·8` (13..29) of `0\agew_1w.grd` where the building
    fits (`ShowGPDarkN`);
  - the same row of `0\agew_1r.grd` where it does not (`ShowGPRedN`).
- Fit (0x5142c2): the snapped position is tried first. If the site is blocked, the nearest
  free position within 10 rings of 16 px cells is used (spiral table 0x28a6f10, here square
  rings). The site check is the simulation's (`construction::site`): the finished footprint
  must be free of locks and units. 0x513b2c and the mine-deposit rules (`MINEID0..5` hints)
  are not reproduced.
- A left click on a free site sends `Build` with the selected units and leaves the mode;
  with Shift the mode stays. A right click or Escape leaves the mode.
- Units inside buildings (`Unit::inside_of`) are not drawn, selected or shown on the
  minimap.
- Construction stages draw `#BUILDLO_k`. A building type that appears during the game loads
  its packs then (`ObjectsView::want_units`).
- A finished building draws, besides its `#STANDLO` parts and sections, its current
  animation (OB+0x10d), which is `#WORK` from the moment it is done (0x523924; at creation
  0x554b8e): frame OB+0xec at the picture origin, sorted at the ground point plus `ANMSH`
  (NM+0x2890, default 16), the counter stepped by one per drawn frame while the game runs
  (0x50927a..0x5094a6) and back to 0 at the frame count. Only EngMel, SpnMel (the mills'
  wheels) and SpnRudCo (the coal mine's hoist) have one; their `#STANDLO` pictures leave
  rectangular holes for it, which showed the ground before (issue #185). ACR steps the
  counter once per tick for buildings near the view (`effects::Effects::work_step`). The
  placement preview shows the first frame (0x4be467).
- Map ids are looked up through the nations' `[MEMBERS]` (`MapObjects::set_names`, from the
  catalog): `SpnCoa`, `SpnIro` and `SpnStn` are defined by `SPNRUDCO.MD`, `SPNRUDIR.MD`
  and `SPNRUDST.MD`; by the id's own name they had no picture, icon or definition.
- Sprite frames go into the GPU atlas (2048² pages of 2 bytes per texel) when they are first
  drawn (`SceneAtlas::draws_uploading`), into the free part of the last page. Uploading
  every frame of every loaded pack at the start took 334 pages (2.7 GB) on Miss7F03, more
  than a small GPU (or WebGL) holds. After 24 pages (or twice what one view needed) the
  atlas starts over with the frames of the current view.

### Right click and context cursor (0x4c4668..0x4c4ce2)

The cursor shows what a right click on the map does with the selection:

| Under the cursor | Order | Cursor (`CURSOR_00.GP`) |
|---|---|---|
| enemy (without Ctrl over a building the units may storm) | `Attack` | 1, sword (0x66a924) |
| own unfinished or damaged building, selection has builders (`#WORK`), Ctrl not held | `Repair` | 3, hammer (0x66a920) |
| own building one of the selected units may enter | `Enter` | 11 (0x66a918) |
| with Ctrl: another nation's building one of the selected units may enter (storm it, or take it empty) | `Enter` | 15 (0x66a91c) |
| a source within 64 px (`DetermineResource`, 0x4d3ef0, called at 0x4c4bf5; ripe fields and carcasses count, not only trees and deposits) that a selected unit can gather | `Gather` | by source type (0x66a934): wood 6, minerals 5, field 7, meat 1 |
| anything else | `Move` with the `CursGo` marker | 0, arrow |

With only buildings selected, the right click sets the rally point. Map points account for
the terrain height under the cursor.

The order is given when the button is released (0x4c4dad); a press only starts the drag
(`0x1100b32`, press point 0x1100acc / 0x1100ad0):

- While the button is held, 0x4c5407 draws a facing arrow from the press point to the
  cursor once the drag is longer than 30 map pixels: seven `DrawLine`s in
  `GetPaletteColor(255, 255, 0)` around a shaft 40 wide, with a head 40 long and 80 wide
  (`u = 20·(A, B) / L`, map y halved on the screen). No sprites.
- Released after more than 30 map pixels: a move to the press point facing the drag,
  `dir = (GetDir(dx, dy) + 4) & 0xF8` (32 directions), whatever is under the cursor. The
  move command 2 carries the direction (0x4d5f86); formations re-form facing it, free units
  are arranged across it (0x4ac705). ACR sends `MoveFacing`.
- Otherwise the context order of the table above.
- The `GO&ATTACK` icon toggles the attack-move mode (`0x28f9e55`): moves get flag 8 and
  become attack-moves (`AttackMove`; ACR's attack-move has no direction). `STANDGR` and
  `NOSTAND` switch stand ground (`StandGround`).

### Minimap (0x44c230 image, 0x4c0a40 draw)

- One pixel per `1 << s` map cells, so the image is 240 × 240. A pixel samples vertex
  `(x << s, y << s)` and takes the colour `SHADE[light][first texel of the texture]`:
  - `SHADE` is `AGEW_1DR.GRD` rows 64..96;
  - the texel table is 0x6d4658.
- Water: every second cell with `SEA2` depth > 0x80 takes 0xb3..0xb7 by brightness.
  Non-tree map objects are 0x4e.
- Position: `(RealLx − w − 17, RealLy − h − 16)`. Large mode (`0x668f1d`, the default)
  shows the whole map. Small mode (key M) shows at most 120 px, scrolled with the camera:
  `offx = (camX + viewW/2 − w) >> s`, clamped.
- Units are 2 × 2 dots in colours {208, 212, 216, 220, 224, 228, 232, 173}; the local
  player's selected units are 0xff.
- Camera rectangle: 0xff. Two frame lines in `0xd0 + 4·player` (+1 outside), then the
  `Border` box.
- Mouse: left press or drag centres the view (0x4c5c9a). Right press orders the
  selection there.
- Fog of war (0x485c24, after the dots, so it hides them, before the camera rectangle):
  each 2 × 2 block of pixels whose fog cell (`FogMap` 0x7b7b28, 128 px cells) is 1300 or
  less becomes palette colour 0. There is no explored state. It is drawn while fog is on
  (`0x775e18`) and the player has not been defeated. ACR keeps the local player's
  `acr_sim::fog::Fog` and stamps and blurs it every tick; see [Fog of war](#fog-of-war-0x485606-0x485a3c-0x485d2d)
  for the map view.

### Fog of war (0x485606, 0x485a3c, 0x485d2d)

`acr_render::fog` draws it, `ingame.rs` (`fog_shades`) feeds it. The grid itself is
described in [simulation.md](simulation.md#fog-of-war).

- When (0x4bee1e): after the world, i.e. terrain, objects, selection brackets, life bars,
  `CursGo`, the inaccessible areas and the building preview, and before the interface
  (minimap, icons, panels, texts). Only while fog is on (`0x775e18`), `0x6e45c8` ≠ 1 and
  bit 0 of the local nation's byte `+0x111c8d1` is clear (the same test hides the minimap
  fog and mutes sounds; its meaning is not traced). The blur (0x483c2a) runs just before
  it while `0x668f22` is set and `0x6b8b04`, `0x1100a70` are clear.
- Normal view: 0x485606 spreads the fog cells over a table of screen cell corners
  (`0x75de10`, 128 per row; a screen cell is 32 × 16 screen pixels, i.e. 32 × 32 map
  pixels). Each fog cell gives its 4 × 4 corners from its own four corner values by
  midpoints, rounded down at each step: edge midpoints `(a + b) >> 1`, the centre
  `(a + b + c + d) >> 2`, then the midpoints of those (`acr_render::fog::split`). Fog cell
  `(gx, gy)` is the corner at screen `(128·(gx − 3), 64·(gy − 3))` in map coordinates: the
  fog follows the ground plane, not the height.
- 0x485a3c turns each corner into a darkness row with 0x484e63 (`5` from 1500 up,
  `5 + (1500 − v)·153/300` down to 1200, then 158; table 0x7a9b28 built by 0x484dfb) and
  0x4845ae fills the cell: the row is interpolated bilinearly over the 32 × 16 pixels (16.16
  fixed point) and every pixel becomes `AGEW_1DR[row][pixel]` (`0\agew_1dr.grd`, table
  0x7abb28, all 160 rows). Rows 0..63 are the identity, 64..95 darken linearly (row
  `64 + k` ≈ `k/32` black), 96 and up are black. A cell whose four rows are all ≤ 63 is
  skipped, one with all ≥ 96 is filled with colour 0.
- Zoomed-out view (0x485d2d): no midpoints; one screen cell (32 × 16 pixels, 128 × 128 map
  pixels) per fog cell with the fog cells' own values at its corners.
- There is no explored state and no hidden objects: enemy units are drawn normally and
  disappear only under the black.
- Switches: the editor's F key (0x4a424c, the key handler while `0x90da38` is set), the chat
  cheats `qwe` (fog off) and `viewall` (toggle), the replay rows `REP_FOG`/`REP_NOFOG` of
  the "M" drop-down, and a defeat in a multiplayer game. The game itself has no fog option.
- Sounds (0x5868d3, also 0x586aa2 and 0x586b9c): while fog is on, a world sound at `(x, y)`
  (screen pixels, y = map y / 2) is not played where the fog at `(x, 2y)` is below 900.
- ACR draws it in a shader after the world overlays and before the interface: the rows at
  the corners in view are computed on the CPU (`FogShades`), interpolated per pixel and
  applied as black of the row's opacity fitted against the palette
  (`GroundAssets::fog_darkness`), since sprites are blended in colour, not in palette
  indices. The grid is aligned to the map, not to whole screen cells, so it scrolls
  smoothly. The script's light spots (`SetLightSpot`) are stamped into the grid each tick.
  ACR has no cheats or replays yet, so the fog is always on.

### Cursor and overlays

- Cursor: `LoadFog(0)` selects `CURSOR_00.GP`. Hotspots and animation chains come from
  `curs_xy.dat` (`dx dy next`, a step every 60 ms). Frame 0 (arrow) is the default; the
  context cursors are listed under [Right click and context cursor](#right-click-and-context-cursor-0x4c46680x4c4ce2).
  Storming, healing and the other cursors wait for the simulation.
- Selection brackets (0x4bae50): 3 px corners in `GetPaletteColor(255, 255, 0)` around
  `RECTANGLE dx dy w h` (defaults 0 3 40 64). Soldiers of a formed brigade get none.
- Formation marks (0x41c93f, called by the map drawer at 0x4bd613 after the objects and
  not in the zoomed-out view, for every enabled brigade of the player with a selected
  member, bitmap 0x1100ae8 built by the minimap code 0x4c0e5f): if the brigade has a shape
  (+0x1e), more than 3 members and its slots (+0x2e bit 1, set by `PlaceBrigade`), every
  soldier (member index ≥ 3, not dying) gets a diamond on the ground, four lines from
  (x ± 12, y) to (x, y ± 12) around its position; while the brigade keeps its positions
  (order 0x41c0a9, i.e. after a move order until the next one) a cross of ±2 px marks the
  slot each soldier walks to (+0xe / +0x12). The colour is `GetPaletteColor(255, 255, 0)`
  (0x734f80), the raw index 0xC9 during formation melee (0x41e50c). Each end of a line is
  lifted by the ground height there (0x41c75b). The bounding box drawn after the loop is
  dead code (0x41cbfc jumps past it).
- Life bars (Tab, 0x4bb390, called by the object drawer 0x49d795 while Tab is on): green
  lines at y0 − 12 and y0 − 11 from x0 + 5, `(w − 12)·life/MaxLife` + 2 long, with white
  end caps at x0 + 4 and x0 + w − 5. Only living objects get one: the function returns at
  once when the death counter `Sdoxlo` (OB+0x58) is set (0x4bb3b1), so corpses show none.
  Selected units get none of their own (the brackets 0x4bae50 draw no bar); with Tab off
  only an object flashing in colour 0xd0 in the marker list 0x28c55f8 (set by the right
  click handlers at 0x4c41e8 / 0x4c5218) gets one, which ACR does not draw yet.
  Buildings get the same bar: neither 0x4bb390 nor the box 0x4bad90 tests a building flag,
  and houses, towers and forts have `RECTANGLE` (`ATCKRE.MD`: `RECTANGLE 0 100
  675 345`) and, if they shoot, `SHOWDELAY` (`ENGDO1.MD`, `ENGBLG.MD`, …), so a garrisoned
  house or a fort shows its reload bar under it. ACR measures a building against its
  `MaxLife` (OB `max_life`, raised by the stages) and drew no building bars until 2026-10
  (2D and 3D share the overlay; `ACR-analysis/house-bars/sheet_bars.png`). Not confirmed:
  that the original's sprite queue hands 0x49d795 the building object for every building
  sprite.
  - `SHOWDELAY` types (NM+0x172a bit 0, 0x4bb51a) get the reload bar right under it: two
    lines of the raw palette index 0xd0 (the first player colour, red) at y0 − 10 and
    y0 − 9, `(w − 12)·(D − delay)/D` + 2 long with `D = MaxDelay` (+0x8f, 1 if 0) or
    `delay` (+0x8c) if larger: empty right after a shot, full when loaded. The end caps
    then reach down to y0 − 9.
  - Buildings (OB+0x27 bit 4, 0x4bb6f8) get a garrison label over the bar: an 8 × 8 square in
    `0xd0 + 4·owner` (OB+0x95) at (x0 + 5, y0 − 22) and, 12 px to its right with its bottom 4 px
    above y0 − 10, `SmallWhiteFont` (0x92dc4c, FontM13 colour 1) text `%d/%d` (0x644798): the
    units inside (OB+0xf0) and `MaxInside` (NM+0xd8) + `AddInside` (OB+0xa5). For objects
    whose alliance mask shares no bit with the local player's nation (0x1114079 + 0x1a902·n)
    it reads `?/%d` (0x644790) instead; ACR leaves out the extra test of OB+0xf2 bit 1
    (storm flag, never set). Drawn for every building, also with capacity 0. ACR:
    `inside_label`.
- Gun marks (`ingame/gun_marks.rs`, not in the original, which draws no gun ranges; the
  red X of the canister mark before is gone): a selected own cannon (a type with
  `WEAPONINSECTOR`) shows on the ground what its fire mode covers, in the selection
  brackets' yellow (`GetPaletteColor(255, 255, 0)`) as one-pixel lines following the ground
  with a dark edge one pixel under them, like the brackets and formation marks:
  - **canister** (an idle gun's own volley ahead, `KARTECH`, an attack on men within the
    canister's reach): the `WEAPONINSECTOR` sector, both edges and the arc at its reach
    `r2` dashed (48 px on, 32 off), a dotted arc at half reach (the damage falls off
    linearly to `d2` there); no minimum zone;
  - **ground fire** (`ATTACKGROUND`): a dotted line along the flight to the first graze (the
    aim point), a ring of 24 px there, the lane the ball skips along as two dashed lines
    32 px either side for `artillery::GROUND_LANE` px, with a cross tick at each likely
    skip (each skip half as long as the one before);
  - **direct fire** (an attack on a building, or on men beyond canister reach): a solid line
    to the aim point with a 40 px cross bar and a dashed line on for
    `artillery::DIRECT_LANE` px, where a miss bounces out;
  - the **too-close area** of a ball mode (ground fire: `artillery::GROUND_MIN`; without the
    hit-locations rule the ball's `ATTACK_RADIUS` minimum): a dashed grey ring around the
    gun, hatched with dotted grey diagonals every 64 px.
  The mark follows the gun's current attack (`gun_marks::current`); while aiming it follows
  the cursor instead: the ground point under it in the attack-ground mode, else the enemy
  under it. The mode is the sim's own choice (`artillery::gun_shot`) with the hit-locations
  rule, else the original's by distance. Not drawn in the zoomed-out view.
  - **Zielhilfe** (ACR): with own guns (`WEAPONINSECTOR` types) or, under the mortars rule,
    own mortars selected, command slot 20 shows the original's `CEN02SP` icon (a cannon
    with its dotted flight; `ICONS.GP` frame 93), framed yellow while the guides show, hint
    "Zielhilfe ausblenden" / "Zielhilfe einblenden". A click hides or shows the gun marks
    and the mortars' rings and zones (the zone being set still follows the cursor). The
    switch is the local player's, not a command (no replay, no sim effect); it is kept in
    `settings.txt` as `aim_guides=on|off` (on by default) for later games.
  - Since 2026-10 the gun marks and the formation marks are drawn as lines one window pixel
    wide at every zoom (`Draw::line`, `ui/draw.rs` `line_runs`): at zoom 1 the pixels of
    `DrawLine` (0x4012ff), zoomed in crisp instead of scaled-up blocks. Driver check:
    `ACR-analysis/aim-toggle/` (`after.json`, `sheet_squares.png`, `sheet_gun.png`,
    `sheet_panel.png`).
- Panic shouts (0x600bc8, #209): with Tab on, every flight leg of a panicking unit adds one
  of the eight `FEAR_RNDM0..7` lines of `TEXT\MORALE_TEXT.TXT` ("Wir werden alle sterben
  ...", "Rette sich, wer kann!", picked with the C library's `rand() & 7`), centred at
  `RealX>>4` and 90 px above the unit (`(RealY>>5) − height − 90`), into a floating text list
  (0x600050, list 0x28f2188). `FontM13` in colour 2 (`SmallRedFont`, 0x92dcc8) for units of
  the player's side (alliance mask OB+0x9a), colour 3 (`SmallYellowFont`, 0x90d8ec) for
  others; none for `NIKAKIXMAM` types (NM+0x172e bit 5, animals). A line that would overlap
  another (centres closer than half the summed sizes, 0x5fffc0) is dropped. Each drawn
  frame (0x600320) a line rises 1 px and ages by 1; it fades in over 16 frames
  (`0x7C3428 = (age − 16)·512`) and out after 48 (`(48 − age)·256`), and goes after 80. ACR
  counts two frames per tick and draws none in the zoomed-out view (as 0x600320 does).
- Selection rectangle: `GetPaletteColor(0, 255, 255)`.
- The game menu, load and options dialogs load `cur_mn.rlc` (`LoadPointer` 0x496de2), but
  it is never drawn: the cursor drawer 0x496efe always shows `CURSOR_00.GP` frame 0, so
  dialogs show the arrow, as in ACR.
- `Interface\CursGo` at a move target: one frame per game frame, 17 frames (0x587fa9).

### Keys (0x4a30e8) and scrolling (0x4c5910)

- F12: game menu. Escape leaves the build or rally point mode (0x4a31a3); otherwise ACR
  also opens the menu with it.
- U, I, M, Q, O, L: the drop-down's toggles (unit panel, game panel, minimap size,
  inaccessible areas, transparency, zoom). Each also closes the drop-down.
- Pause (0x4c0130): `PAUSETEXT` in `BigWhiteFont` at ((`RealLx` − w) / 2 − 2,
  `RealLy` / 2 − 8) on a paper box (0x480130: `Border` 28..30, corner table 0x668580) from
  (x − 64, y − 8) to (x + w + 64, y + 27). It does not blink.
- Control groups: Ctrl + digit stores, digit recalls, Shift + digit adds.
- Space: jump to the next selected unit. Pause: pause. Tab: life bars. M: minimap size.
- Selection (0x4da424 → 0x5179d6): a press and release within 5 pixels picks the own object
  under the cursor (nothing → empty selection). A rectangle selects while it is drawn
  (every mouse event): every own object whose box (`RECTANGLE`) meets it, but buildings
  only if no unit is caught. Enemies cannot be selected. Shift adds and never removes;
  Ctrl does nothing. A picked formation member brings its formation's flag bearer and
  soldiers along (0x41f3fb → `SelectBrigade` 0x4e1365 from member index 2 on; the officer
  and drummer only if picked themselves). Then
  `CorrectImSelectionInGroups` (0x43dbff) adds the formations grouped with the selected
  ones (`CM_DOGRP`) the same way; both also run after recalling a control group (0x4d6387).
- Double click (WM_LBUTTONUP 0x4a2d2a: the next release within 600 ms and
  `|dx| + |dy|` < 16 pixels, pseudo key 0xF1, handler 0x4c2ea5): the own units on the screen
  of the first selected unit's type, of its kind (formation members if the selection has
  some, free units if it has some); Shift adds.
- Scrolling is updated once per game frame:
  - arrow keys set the speed to ±4;
  - the cursor within 6 px of the edge accelerates up to `ScrollSpeed`;
  - otherwise the speed decays by 1.
  The view moves half the speed in map cells (32 × 16 px) per game frame. ACR spreads
  that step over the frame so it moves smoothly.
- Game speed: ticks/s = `1000 / (2·FPSTime)`. ACR caps the uncapped setting at 100.
- ACR's optional modern controls (#129, `ingame/controls.rs`, key and mouse reference in
  [controls.md](controls.md)) leave all of the above unchanged in the original scheme. In
  the modern scheme: the wheel sets a zoom level 0.5..2 (four notches double it) and the
  camera scale glides to it about the window point under the cursor; Ctrl + left drag and
  middle drag pan the map (Ctrl + left click without moving is a plain click); W/A/S/D
  scroll like the arrow keys, which then set the speed to `ScrollSpeed` instead of 4 and
  move the view by speed / scale so it moves the same on the screen at any zoom; the edge
  can be switched off; Home centres the selection, a group digit pressed twice within
  0.5 s centres the group, Escape cancels a mode or drag before it opens the menu, and
  Alt + W/A/S/D press the icons W/A/S/D press in the original. The zoomed-out view of key L
  keeps its 1/4 scale and returns to the wheel's level. All hit tests (selection,
  right click, building preview, minimap rectangle, sounds) go through the camera scale.

### Game menu (0x5d9c30) and game options (0x5dbe10)

- Game menu: `Interf2\GameMenu` frames 0 (shadow) and 1 at `((RealLx − 470) >> 1) − 14`,
  `(RealLy − 415) >> 1`. Title `INTF_GMENU_WIN` at +254, +36. Five `GP_TextButton` rows
  at +46, +75 + 26·i, with hover frames 3, 5, 5, 9, 11 and fonts `fon18w`/`fon18y3`:
  - continue (0xf4);
  - save (0xf8);
  - load (0xf7, text not raised);
  - options (0xe7; the menu closes after them);
  - back to the main menu (0xff, no confirmation).
- The game stands still while the menu is open. After the options, save or load dialog
  the menu closes (`0x28ccc6d`, set by both dialogs whether or not they saved or loaded).
  The original disables load (frame 4, passive fonts) in multiplayer games.
- Game options: `Interf2\Options_for_Game` at `((RealLx − 484) >> 1) − 57`,
  `((RealLy − 425) >> 1) − 166`.
  - Combo boxes (`Combo2`, `WhiteFont`/`YellowFont`): resolution, music, game mode.
  - Sliders (`Scroll2`, 292 × 12): sound, music, scroll speed (`ScrollSpeed − 2`) and
    game speed (`100 − FPSTime`).
  - OK and Abbruch on `G_Load_Game` frame 1. Enter and Escape work.
  - OK changes the simulation's tick rate and the scroll speed.

ACR adds five more pages in the same frame (`ui/screens/game_options.rs`). A text button
right on the first heading's line ("Weitere Optionen »", then "Seite 2/6: Grafik »",
"Seite 3/6: Details »", "Seite 4/6: ACR-Regeln »", "Seite 5/6: Weitere »", "Seite 6/6: Zurück »") turns the pages;
OK takes the choices of all six, Abbruch none.
Rows are 27 px apart from the first heading, labels and boxes where the original page has
them (+76 / +239): headings on its heading rows (203, 257, 365), a combo box on every other
row, since the frame has the empty boxes (230, 284, 392) and the sliders'
tracks (311, 338, 419, 446) drawn in. A page with fewer rows (2 and 5 leave the third
group, 365 … 446, empty, 4 all below 284) covers the frame's boxes below its last row with a strip of the
frame from the plain heading row 257 (box column only, one row high). The settings each line
writes are in
[rendering-3d.md](rendering-3d.md#settings).

| Page | Heading | Line | Choices | Setting |
|---|---|---|---|---|
| 2 | Gefecht | Blutdarstellung | Voll / Verringert / Aus (Original) | `gore` |
| 2 | Tote und Spuren | Verwesung | Knochen / Nur halb, each mit / ohne Bodenfleck | `corpse_decay`, `decay_stain` (one box: the page's rows are the frame's) |
| 2 | | Liegezeit | 1 … 60 Minuten | `corpse_minutes` |
| 2 | | Schlachtfeldspuren | Verblassen / Bleiben (ganze Schlacht) | `battlefield_marks` |
| 2 | Anzeige | Leiste mittig | Aus (Original) / An: Leiste mittig | `center_top_bar` (not a rule: centres the top bar on wide screens, see [Options](#options-0x5c9740)) |
| 3 | 3D-Grafik | 3D-Modelle | Aus (Original) / Einheiten / Einheiten und Gebäude | `models` |
| 3 | Darstellung | Kamera | Fest (Original) / Frei | `camera` |
| 3 | | Schatten, Details | An / Schatten aus / Detailstufen aus / Beide aus | `shadows`, `lod` (one box, so the landscape group has room for the weather) |
| 3 | | 3D-Bäume, Steine | An / Aus | `map_models` |
| 3 | Landschaft (nur 3D) | Wetter | Aus (Original) / Regen / Schnee / Nebel | `weather` (a look and its sound, no gameplay effect) |
| 3 | | Wind | An / Aus | `wind` |
| 3 | | Wasser, Boden | Detailliert / Wasser original / Boden original / Original | `water`, `ground` (one box: the page's rows are the frame's) |
| 4 | Grafikstufe (nur 3D) | Voreinstellung | Automatisch / Niedrig / Mittel / Hoch / Ultra | `graphics` ([rendering-3d.md](rendering-3d.md) "Graphics presets"); a new choice also sets the single switches |
| 4 | 3D-Details | Materialdetails | An / Aus | `material_detail` (page 3 is full) |
| 4 | | Bodenbewuchs | An / Aus | `ground_cover` ([ground.md](ground.md)) |
| 5 | ACR-Regeln (neue Schlacht) | Trefferzonen | Aus (Original) / An: Wunden, Glieder, Verbluten | `hit_locations` |
| 5 | Truppen und Gelände | Körperabstand | Aus (Original) / An: Körper brauchen Platz | `body_spacing` |
| 5 | | Leichenhaufen | Aus (Original) / An: als Hindernis | `corpse_piles` ([simulation.md](simulation.md#corpse-piles-optional-rule-222)) |
| 5 | | Mörser | Aus (Original) / An: Mörser mit Zielzonen | `mortars` ([simulation.md](simulation.md#mortar-optional-rule-301)) |
| 5 | Feuer (mit Trefferzonen) | Kugelbrand | Aus (Original) / An: Kugeln setzen in Brand | `cannon_fire` ([simulation.md](simulation.md#shell-fires-256)) |
| 5 | | Kugeln, Gelände | Aus (Original) / An: Kugeln brechen Bäume | `ball_terrain` (unset: on with the modern controls; written only when the line is changed) |
| 6 | Befehle (neue Schlacht) | Gruppenbewegung | Aus (Original) / An: alle Formationen | `group_move` (unset: on with the modern controls) |
| 6 | Kampf | Formationsangriff | Aus (Original) / An: Glieder greifen an | `formation_attack` ([simulation.md](simulation.md#formation-attack-optional-rule-352)) |
| 6 | | Blasrohrbrand | Aus (Original) / An: Pfeile setzen in Brand | `dart_fire` ([simulation.md](simulation.md#shell-fires-256)) |
| 6 | | Gezieltes Feuer | Aus (Original) / An: genauer, längeres Laden | `aimed_fire` ([simulation.md](simulation.md#aimed-fire-optional-rule-aimed_fire-296)) |
| 6 | Tiere | Tiere geben auf | Aus (Original) / An: Jagd endet bei Abstand | `animal_give_up` ([simulation.md](simulation.md#animals-give-up-optional-rule-355)) |

All but the rules (pages 5 and 6, every optional `acr_sim::Rules` flag) take effect at once on OK. The rules are part of the simulation (the
setup a replay or savegame hashes): a battle keeps the rules it began with, the new choice
applies from the next battle (the game setup reads them, `lib.rs`), as the heading of page 5 says. They have pages of their own (#374) since the rules outgrew one page; page 2 keeps the
battle's looks. A new rule is one `Row::Choice` line in `RULES` or `RULES_MORE` (the UI
settings that are not rules go on the page they belong to, such as `center_top_bar` on `BATTLE`). The modern controls'
Ctrl + Space, C switches the free camera for the running battle only
([controls.md](controls.md#leader-chords-modern)); OK on these pages sets it back to the
`Kamera` line. Driver check with
screenshots: `ACR-analysis/settings/after.jsonl`; the `Wetter` line (combo, OK, the
driver's `weather` answering `rain`, the line still `Regen` when reopened):
`ACR-analysis/genai/weather/ui/` (`steps/set_rain.json`, `sheet_page3.png`). The realism
page (paging 1 → 4 → 1, `Leichenhaufen` set to `An`, Enter saving `corpse_piles=on`, with
both control schemes): `ACR-analysis/genai/rules_page/` (`steps_after.json`,
`sheet_before_after.png`).

### Mission windows and messages

`ui/screens/mission.rs` (windows) and `mission_ui.rs` (texts over the map). The
original runs the windows as modal loops inside the script call (Enter/Escape are read
from `LastKey` in the loop); ACR pushes them on the screen stack, which stops the game, and
the mission runs no script and no step until they are closed.

- Page (`ShowPage` → `ProcessMissionText` 0x5e16b0), question (`AskQuestion` → 0x5adfb0)
  and objectives (F1 → 0x5e11c0) share one layout on `Interf2\Messages`, frame `f` (0 for
  page and objectives, 2 for the question) at `((RealLx − W) / 2, (RealLy − H) / 2)`, its
  shadow `f + 1` under it. Relative to that:
  - caption centred at (`W / 2`, 34) in `fon16y5`: `INTF_MESSAGE`, `INTF_CROOM_GS0`,
    `MISSOBJ`;
  - page and question: the header's picture (`"<gp> <frame>"`, `sscanf("%s%d")`, `NONE` =
    none) centred at y = 75, then `Interf2\Line2` frame 0 centred 16 pixels below it; the
    text starts 10 pixels below the line. The objectives text starts at y = 75;
  - `Interf2\VScroll2` at the right edge x = 520 down to y = 495, the text viewer from
    x = 55 to 5 pixels left of it in `fon18y3`, 21 pixels per line;
  - `@MSOK` (0xf5) on `Interf2\Button2` frame 3 (`fon18y5`) at (191, 520); the question has
    `@MSOK` at (75, 521) and `@MSNO` (0xf6) at (307, 521).
  - Enter is OK. Escape is No in the question and OK otherwise. The objectives window
    opens only in a mission with a non-empty objectives text (`miss.txt`).
- Multiple choice (`AskMultilineQuestion(nx, page, orient, "id|id|…")` → 0x5ad540): a box
  `nx · 64` pixels wide, centred horizontally:
  - picture at (16, 16) for `orient` 1 (text right of it), at the right edge for 2, centred
    above the text for 0 (values above 2 count as 0);
  - the page text in `rom11` with `SetRedColor`, disabled; the choices 32 pixels below it,
    each a text viewer at x = 32 in plain `rom11` that turns red under the mouse, 16 pixels
    apart; a click answers its index, the keyboard does nothing;
  - the dialog is then moved so that the whole is centred vertically; the paper box
    (0x480455: `Border` 51..53 tiles, corner table 0x6686e0) is drawn 32 pixels above it.
- Hints (0x493fc5, timers counted down once per frame by 0x4944a9): `AssignHint` sets the
  hint line at (`smapx + 4`, `RealLy − PanelLy − 40`) = (4, 584); `AssignHint1` pushes onto
  four message lines 20, 40, 60, 80 pixels above it (the same text again only renews its
  time). Each is drawn in `BlackFont` 2 pixels down-right, then `WhiteFont`. A time of 0
  shows nothing.
- Central box (0x4bc490): `\` separates lines; `BigWhiteFont` centred on the middle of the
  view, on a paper box (0x480130: `Border` 28..30 tiles, corner table 0x668580) sized for
  as many lines as there are `\` (one fewer than drawn). It shows `ShowCentralText` while
  its time runs and the end of the game (0x4bc7b0).
- End of a mission: `ShowVictory` / `LooseGame` show `WINNER` / `LOOSER` in the central box
  (a central text starting with `#` wins over it) while the game goes on; after 7 s, or at
  once with Escape ("Zum Verlassen ESC drücken", 0x4a31f2), the game loop ends. Missions show
  no statistics (`0x1100ac5` is cleared, 0x5ab55d), and ACR returns to the main menu.

Deviations: hint timers count simulation ticks; the end waits 7 s of game time; the
custom texts of `SetVictoryText`/`SetLoosText` are not supported (no mission uses them);
after a campaign victory the next missions are not opened (no campaign progress yet).

### Diplomacy panel

`diplomacy/` (panel, CML pages) and `ingame/dip.rs` (hooks); DipServer's side is in
`acr_mission::diplomacy`. The tribe dialogs are CML pages: DipServer fills the masks
`Dip\*.cml` (`ALL.GSC`), writes the result to `Dip\tmp.cml` and calls
`ActivateDipDialog("LW_new&|LW_file&Dip/tmp.cml")`. The original shows them with
`intExplorer.dll`, a small browser that builds its widgets with the `DialogsSystem` API, in
the side panel of the game screen. The game goes on while it is open.

- When it opens:
  - DipServer opens a page itself when units of the player enter a neutral tribe's zone
    (`$FIRSTMEET`: "Möchten Sie mit uns handeln?", with a camera jump).
  - Every press of the left button on the map (0x4c446c) calls
    `CheckDipBuilding(MyNation, object under the cursor)`. DipServer answers only for a
    building of a tribe the player has allied, with that tribe's offers.
  - F8 (`LastKey` 0x77 → 0x5c3a60) toggles the panel.
- Panel (0x4ce310 with `0x1100a74` = 1, `0x1100b54` = 4; layout of 0x5aa5ac at
  `RealLy` ≥ 768):
  - box 0x4803fb from (`RealLx` − 259, 48) to (`RealLx` − 16, 469): `Border` paper 28..30
    with corner table 0x668660 (TL 17, TR 87 with the close cross, left 12, right 36,
    top 23) down to y = 441, then a strip with table 0x6686c0 (3, 86, 6, 8, 12, 14, 10, 11);
  - a press strictly inside (`RealLx` − 42..`RealLx` − 16, 43..69) closes it;
  - `DIP_HDR` in `YellowFont` at (`RealLx` − 250, 50) (0x5c3770);
  - browser window `RunExplorer(1, page, RealLx − 262, 77, RealLx − 8, RealLy − 298)`,
    (762, 77, 1016, 470) at 1024 × 768; a page is laid out again when the size changes.
- intExplorer draws no frame of its own. Each `#ebox`/`#sbox` is its own `DialogsSystem`,
  drawn in creation order without clipping to the window.
- Page parser (0x1000a9c0): it scans for `#` anywhere, so tags may follow each other on one
  line. DipServer's masks end without a newline, so the first offer shares the line of the
  last `#font`. `//` starts a comment.
- Rectangles (0x1000a170):
  - keys `x`, `x1`, `xc`, `w`, `y`, `y1`, `yc`, `h`, seeded with the parent's inner
    rectangle (top-level boxes: the window, inclusive x1/y1);
  - in an expression the first term is `lo + n`, later terms add ±n; `N%` is
    N · (hi − lo) / 100; `L`/`R`/`U`/`D`/`C` are the edges and the middle; `w`/`h` use
    lo = 0, hi = width/height;
  - `%NAME` (y only) is the bottom y1 of an element of the same box;
  - only complete pairs change the rectangle: (x, w) → x1 = x + w, (x, x1), (xc, w),
    (x, xc), (x1, xc).
- Tags (handlers in intExplorer):

  | Tag | What it does |
  |---|---|
  | `#font(a,b,c)` (0x1000d663) | a for plain text, metrics and line height; b for links; c for hovered links. Names: `YF` `YellowFont`, `WF` `WhiteFont`, `RF`, `BF`, `S?F` small, `B?F` and `SP?F` big, `XX?F` special, `F8`/`F10`; unknown names are `YellowFont` (0x1000a610) |
  | `#nfont(N, gp, colour, shadow, dx, dy)` (0x1001b510) | a new `RLCFont`. ACR uses the global font built the same way: `Fn16` = `fon16y1`, `Fn16y` = `fon16y3` |
  | `#txt`, `#ctxt`, `#rtxt` (0x100136e0) | text as word-wrapped rows of `TextButton`s, left, centred or right aligned per line. Line height `GetRLCHeight(a, 'y') + 1`; widths in font a. The k-th `{` of the text starts a link with the k-th `{cmd}` of the tag. Plain runs use (a, a, a) and links (c, b, a). The element's bottom is y + lines · line height |
  | `#pix`, `#cpix` (0x10019390) | `addGP_Button(x, y, gp, f3, f1)`; `#cpix` is centred by the width of f3. The bottom is y + height of f3. `.gp` is appended if missing |
  | `#sbox` / `#def_sbox` / `#def_scroll` | the content is clipped to the inner rectangle. The border frames are −1 in all pages, so no border is drawn. Scroll bar `VScroll2` at (inner x1 − 25, inner y), shown while the content reaches below the box (range = overflow + 10, 8 px per arrow step) |
  | `#pan(rect, 1)` | draws only a style defined with `#def_panel`, which no page has |
  | `#time(ms, "com:DS\|cmd")` | one-shot timer that sends the request; the page re-arms it whenever it is reloaded |

- Links and requests:
  - a click on a link (click sound `STANDART`) sends `DS|cmd&args`;
  - intExplorer adds the window name and the access key as two more arguments. It
    serialises the commands (u16 count; u8 name length, name, u16 argument count; u32
    length and bytes per argument) for `DS_SendRequest`, then polls
    `DS_Process`/`DS_GetRequestResult` until 0x80 and executes the answer;
  - DipServer answers `LW_file&Dip/tmp.cml`. A file that starts with a marker
    (`<FRM1>`) replaces only that section of the current page: the 4-second refresh of the
    allied-tribe list (`cnlist1`).
- DipServer requests:

  | Request | Effect |
  |---|---|
  | `cnlist` / `cnlist1` | list of allied tribes |
  | `showtribe&i` | the tribe's page, camera to it |
  | `dipyes&tribe&nation&price` | accept the alliance (`SendDipCommand` 1) |
  | `dipresell` | `SendDipCommand` 2 |
  | `perform&tribe&offer&nation` | buy an offer of `Dip\SBLD-*.DAT`: scouts, saboteurs, peasant killers, mercenaries, resources, guerrillas |

  `SendDipCommand` data reaches `PerformDipCommand` before the next `ProcessDipRelations`,
  which pays and hands over the units through the game API.

Deviations:
- ACR keeps no page history (`LW_new`, back).
- F8 without a page asks for the list of allied tribes. The original shows its start page
  `LF|LW_new|open&req1.txt` of `LF_Server.dll`.
- An unknown `%NAME` would be y = 0 (an absolute screen y) in the original. ACR continues
  below the box's last element.
- The scroll bar's `shrink` (arrows 5 px shorter) is not applied; there is no mouse wheel,
  as in the original.
- `ACR_DIP_PAGE=<file.cml>` (native) shows a page file at the start, for layout checks.
### Side panels (keys U and I, `0x1100a74`)

`hud/panels.rs` draws them, `ingame/panels.rs` fills them. Both use the column from
`RealLx − 259` to `RealLx − 15` between `InfY1` = 48 and `InfY2` = 466 (`InfDY` = 16, 8 at
`RealLy` ≤ 600; `InfY3` = `InfDY` + 428). The close box is `RealLx − 42 .. RealLx − 16`,
`InfDY + 27 .. InfDY + 53`, and mouse input with `x > RealLx − 262`, `y < InfY2 + 8` goes
to the panel (0x4c1c36). U toggles value 2, I value 1; one panel at a time.

- Boxes (0x47f48a): a table of `Border` frames (TL, TR, BL, BR, left, right, top, bottom; −1
  none). Edges are tiled every `max(64, frame size)` between the corners, each corner is
  clipped to its quarter of the box. Paper (0x47fb51) fills the box with 64-pixel tiles
  `fill + (i² + j³) % 3` first. Tables: panel 0x668660 = 17, 87, −1, −1, 12, 36, 23, −1;
  paper 0x6686a0 = 3, 22, 6, 24, 12, 36, 10, 11; footer 0x6686c0 = 3, 86, 6, 8, 12, 14, 10, 11.
- **Unit panel** (`DialogsSystem` 0xbe1b80, built by 0x4c8cd0, run by 0x4cbb10): the type of
  the first selected own unit. Origin (`RealLx − 260`, `InfY1 + 31`) = (764, 79).
  - Frame (0x480343): the panel box from `InfY1` down to the divider `InfY1 + 29 + H`, the
    paper (fill 0x1c) below it. `H` is the height of the header frame: `Border` 0x13 unit,
    0x15 artillery, 0x14 ship, 4 building, drawn at (0, −3) (building −4) over the portrait.
  - `INFO kind frame` of the `.MD` (0x4f993e) picks the portrait: `Units.bmp` 104 × 132,
    `Artillery.bmp` 132 × 114, `Ships.bmp` 132 × 151, `<nation>_BLD.bmp` 240 × 151. The
    frames are stacked in a raw 8-bit BMP (0x436-byte header, palette ignored, bottom-up), the
    last frame first. It is drawn at (1, −3) (buildings (4, −3)).
  - Title: the type's name in `fon16y1` at (8, −29).
  - Page buttons (`addGP_TextButton` on `Border`, the frame only under the mouse,
    `SmallWhiteFont` hovered, `SmallYellowFont` otherwise): units at x = 114, y = 12 + 22·i
    (sprite 0x37); artillery at x = 140, y = 3 + 22·i (sprite 0x39 over a fixed 0x38);
    ships at x = 143, y = 20, 42, 64, 86, 109 (0x5e): General, Descr, Upgr, Next, Prev.
    Buildings (0x39): Descr (12, 158), PriceUpg (132, 158), Next (132, 176), Prev (12, 176).
    The page is kept per kind (`UI_PrmPage` 0x66a958, starting General, PriceUpg for
    buildings). Next and Prev (0x4c8920, 0x4c8a20) step to the next or previous type with
    an `ICON` in the nations' lists, without wrapping.
  - General page from y = `H` + 16, x = 13, 16 pixels per line: `UI_Price`, the six
    resource icons `Border` 37..42 at x = 16 + 35·i, the strip 35 with the non-zero prices in
    `fn10` centred at x = 30 + 34·i, then `UI_Upkeep` and the upkeep lines (`UI_UGold`
    `RESCONSUMER GOLD`·100/80, `UI_UWood`/`UI_UStone` ·100/800, `UI_UIron`/`UI_UCoal` from
    `RASTRATA_NA_VISTREL`), `UI_STAGE` (`BUILDSTAGES`), `UI_CAPT`/`UI_BCAP` (`CAPTURE`),
    `UI_ACCNT` (`VES` / 100) and `STORM_I` (`STORMFORCE`). PriceUpg is the same with a
    24-pixel gap after the icons and `OPPOSIT_I` instead of `STORM_I`.
  - Descr page: `BTN_Descr` at (9, `H` + 13), then `Description\<md>.md` in a text viewer at
    (9, `H` + 29), 205 wide, `SmallYellowFont` with 2 pixels more per line, and an
    `Interf2\VScroll2` bar at (218, `H` + 7).
  - The pages are all built at once and shown by page id (0x4c78e0); a page taller than the
    paper gets an `Interf2\VScroll2` bar at (218, `H` + 7), `InfY2 − divider − 10` long, 10
    pixels per step (0x4ca8a8).
  - Upgrade grid (0x4c84b0; PriceUpg of buildings before `OPPOSIT_I`, Upgr of the other
    kinds first): up to 25 upgrades of the type's nation that name the type (alone or in a
    group), whose message is not their id, of a category below `SHAR` other than `ENABLE`,
    `SINGLE_INSIDE` and `SINGLE_ATTPAUSE`, and not used by the level tables; artillery
    leaves out `BUILD` and `COST`, peasants also get `EFFECT_FOOD..EFFECT_STONE`. Icons five
    per row 42 pixels apart from (7, y − 1), hint `message/UI_Cost` + the price as
    `Holz: 10, Gold: 5` (0x4c8b40, `RDS` names).
  - Upgr level tables (0x4c7990 collects, 0x4cacf0 and 0x4cb25b draw): the `DAMAGE`
    (`WEAPONKIND` with one kind) and `PROTECTION` upgrades for this type alone whose
    `#LEVEL` is 2, 3, … in list order, at most six each. Per table `UI_AttUpg` /
    `UI_ShlUpg` in `SmallWhiteFont`, 16 lower `Border` 0x21 (the level header strip), 16
    lower one row per weapon kind below 8 that a level raises: `Border` 0x20 (0x1f for the
    last row), the kind's `SMICONS.GP` icon at (14, y − 1), the type's base value (highest
    `DAMAGE` of the kind's attack indices, or its `PROTECTION`) centred at x = 44 and each
    level's increase as `+%d` centred at x = 75 + 26·j, in `fn10`, rows 19 apart, 3 more
    after the table. Hints: `UI_ATTk` / `UI_SHLk` on the icon, `UI_SATTk` / `UI_SSHLk` on
    the base, and per cell `UI_ATTk/UI_AttTo<value at this level>/UI_Cost<price>/UI_CostTo
    <price of all levels so far>`. The original marks the upgrades it uses by adding 0x20
    to byte `+0xf` and removes the mark while building the grid.
  - `THIS_UNIT` (0x601d70), only for units (`INFO` kind 0) when the first selected object
    is an own unit of the shown type: a button at (114, 5) (param 7), the other buttons at
    y = 24 + 20·i. Every rebuild with such a unit opens this page; without one it falls
    back to General. While it is shown the panel is rebuilt every 8 s. Texts wrap
    (0x601a70) at word ends and CR/LF, `GetRLCHeight + 1` per line:
    - `THIS_GEN_INF` (`SmallWhiteFont`, x 6..230), 4 lower `FEAR_INF_BORN` (hint `…H`),
      4 lower per fear type with a `FEARFACTOR` its `FEAR_TXWIDEi` (x 6..150, hint `…H`)
      in outlines of colour 0x84 at x 2, 147, 187 (widths 145, 40, 40), the start chance
      `FEARFACTOR · 32.769` and the current one (`PanicP`) as `d/10.d%10%` with
      `d = v·25000/32767`, centred in the boxes; hints `START_MVAL` and `CURR_MVAL0..6`
      (by `d` ≤ 30, < 100, < 300, else; the odd ones when the value differs from the
      start);
    - if any value differs: `FEAR_ACTS` and the factors in boxes at x 2 and 167 (165, 60
      wide), `x%d.%d%` of the factor · 1000, hint `FEAR_INC` or `FEAR_DEC`: near a command
      centre (`FEAR_MULC`), courage beyond ±5 (`FEAR_MULB`), hunger and a lost centre
      (`FEAR_MUL2`, `3`), in a formation the missing officer, drummer and flag bearer
      (`4`..`6`), more than `FP1` % lost (`7`), the formation's kills (`9`),
      comradeship (`A`) and the officer's kills (`8`), else the unit's own kills (`1`).
    ACR adds an outline widget to the dialogs for the boxes.
- **Game panel** (0x4ce310, dialogs 0x10fb148): paper (fill 28) from `InfY1` to
  `InfY2 − 25` in the panel box, the footer box below to `InfY2 + 3`, the tab strip
  `Border` 85 at (`RealLx − 260`, `InfY3`). Tabs at y = `InfY3` + 2, 21 high, x = 32, 69, 105,
  141, 177 (+ `RealLx − 259`), widths 33, 32, 32, 32, 34, hints `NationalForces`,
  `Economica`, `DeathList`, `Doxod`, `Diplomacy` (pages 0, 2, 1, 3, 4). The title is drawn
  in `WhiteFont` at (774, 49), the page from X = 781, Y = 73 clipped to (771, 80)–(1020, 435).
  - Page 0 (0x4cbde0): the players in the game (`Nation+0x1a78b`), teams by their average
    score and players by score, highest first. Each: `Border` 95 + colour at (X − 6, y), the
    name in `WhiteFont` at (X + 60, y + 6), then `Border` 91 at (X − 3, y + 50) and two
    rows `INFO_G2`, `INFO_G1`, both with the score (`City+0xf54`) / 100 as `%.2f` centred at
    X + 165 in `SpecialWhiteFont`, 24 pixels apart. In a mission only the player is named
    (the others are "???" and skipped); random maps add the civilisation in
    `SpecialYellowFont` (not drawn by ACR). The score is changed by objects: created +`VES`,
    died −2·`VES` and +3·`VES` for the killer, captured ∓5·`VES`.
  - Page 1 (0x4ccbb0, 0x4cc660): the local player's losses per type name, units
    (`GI_UNITS`) then buildings (`GI_BLD`), in a table from (774, Y + 31) to 981 with lines
    in colour 0x84, names in `SmallYellowFont`, counts right-aligned to 976, rows 14 apart.
  - Page 2 (0x4ccc30): a 6 × 6 grid (wood, food, stone, gold, iron, coal) centred at
    (X + 41 + 32·i, Y + 50 + 27·j): what 1000 of row j buys of column i at the current rates
    (`SmallWhiteFont`, y − 5) and the normal ones (`SmallYellowFont`, y + 5); an outline
    (cx − 15, cy − 8, 31 × 26, colour 0x96) where the normal value is lower. Icons `Border`
    37 + [0, 3, 2, 1, 4, 5][k] left of the rows and above the columns; `E_Comment` below
    from (X − 3, Y + 210), 15 pixels per line.
  - Page 3 (0x4ccfe0), rows `UNI_LINEDLY1 + 15` = 15 pixels apart (`Unicode.dat` sets
    neither `UNI_LINEDLY1` nor `UNI_LINEDY1`, so 0), table lines in colour 0x84 from X − 5 to
    X + 213 with column lines at X − 5, X + 178 and X + 213:
    - `INFO_R1` in `SmallWhiteFont` at (X, Y + 14), then rows for coal, iron, gold and stone
      (`LoadRDS` ids): the resource name in `SmallYellowFont` and the workers in the working
      mines of that resource (the per-nation sums `0x28eac00..` that 0x5f75b3 adds each tick)
      centred at X + 196;
    - 2 pixels lower `INFO_R2` with wood and food: City fields (0x4cd5ed, +0x93ca54/5c) not
      traced; ACR shows how many own units gather for that resource;
    - 10 pixels lower, if a type costs gold upkeep: `INFO_R4` centred over 210 pixels in
      `WhiteFont`, 20 lower the lines of `INFO_R7` (14 apart), 5 lower `INFO_R5` and
      `INFO_R6` centred at X + 160 and X + 196 on the line of the first header; units
      (`GI_UNITS`) then 3 lower buildings (`GI_BLD`), rows 14 apart with the name, the count
      centred at X + 161 and `RESCONSUMER GOLD v` · count / 80 tenths of a worker (`%d.%d`,
      `%d` when whole) at X + 196, rows of 0 left out, equal names merged (0x4cc530); lines
      at X + 143 and X + 178;
    - 5 lower the total and `INFO_R8`.
    ACR used `v` · count · 10 / 80 before; the original's value is ten times smaller.
  - Page 4 opens the diplomacy panel (`PopupDipDialog` 0x5c3a20).
  - Scrolling (0x4ce6ad): every page returns its height; the page is drawn `0x1100b5c`
    pixels higher, and when the height exceeds `InfY3 − 15 − Y0` the scroll bar
    `Interf2\VScroll2` at (`RealLx − 42`, `InfDY + 63`), `InfY2 − 108` long (0x4ce7e4),
    shows with that excess as its range and gives the offset. A tab click scrolls to the top
    (0x4c6500). The original keeps the last offset while the bar is hidden; ACR uses 0.
  - ACR counts score and losses by watching the world each tick. The simulation counts kills
    but does not name the victims' killers, so each victim's 3·`VES` goes to the first other
    player whose units' kill counts rose (player 7's losses to nobody, as in 0x4e992a). The
    3·`VES` per unit inside a killed building and the credit of a killer-less death to the
    last attacker (`+0xf4`) are not reproduced. An owner change counts as a capture.

### Mission history (#116)

- Every `ShowPage(id)` and `AskQuestion(id)` appends `id` to the local player's history
  (0x5624c5: `Player+0x1a783` array of strings, count `+0x1a787`; no limit, no duplicate
  check; saved with the game). `ShowPageParam` and `AskQuestionParam` append
  `"<header> @ <formatted text>"` instead, once per matching page. `AskComplexQuestion`
  (multiple choice) appends nothing.
- `UM_HIST` (0x562bea) shows the newest entry. An entry starting with `#` is looked up in
  the page table (0x28b77a0 count, 0x28b77b0 ids, 0x28b77b4 headers, 0x28b77a8 texts; the
  last match wins; an unknown id closes the browser); any other entry is split at the
  first `@` (0x672c44) into header and text.
- The `MHISTORY` window 0x5e1d30 (header, text, first flag; the flag is never read) has
  the page window's layout on `Interf2\Messages` frames 4 (window) and 5 (shadow), caption
  `MHISTORY` ("Letzte Nachrichten"), and three `Interf2\Button2` frame 1 buttons at y =
  520: `@MSPREV` (x = 50, returns 1), `@MSCANCEL` (217, 3), `@MSNEXT` (384, 2), all in
  `fon18y5`. Enter and Escape return 3. The browser goes to the older entry on 1 and the
  newer one on 2; at either end the same entry is shown again. It saves the screen under
  the window and restores it afterwards.
- ACR keeps the header and text of every page and question the script shows, in order.
  So the entry of an unknown page is missing (the original stores it and stops browsing
  there), and a page id that matches several pages gives one entry per page.
- The game menu's objectives row (value 0xe8 in 0x5d9c30) is dead code: no row produces
  that value, so ACR does not add one (F1 shows the objectives).

### Open questions

- Whether the original hides a selected type's icon when production icons take its slot.
- The original builds on the button press; ACR too, but the order in which it checks the
  site and the price on the click (0x4c20ac) is not traced.

### Editor: the placing window (0x5bb6c0)

The original editor builds "Einheit wählen" (`ED_HDR01`, `TEXT\EDITOR.TXT`) in the
`DialogsSystem` 0x28cb398 at 0x5bb6c0; its sibling 0x5bb560 builds "Farbe wählen"
(`ED_HDR00`). F2 of ACR's test editor folds it. Origin (`RealLx − 269`, `InfY1 + 31`) =
(755, 79); `Interf2\ToolBox`, `ToolOpt` and `VScroll2` are loaded at 0x5ba450, the combo
frames `Interf2\Combo2` at 0x4eff3b.

- Box: `addBorder(0, −31, 249, 386, 74, 3)` (0x47c05b, style 3 = 0x48039d): paper (fill 28)
  with corner table 0x668640 above the divider at y = 74 and the unit panel's paper
  (0x6686a0) below it. Title text button at (10, −30).
- The eight player colours (0x5bb140, at (10, 10)): bars 18 × 12 at x = 12 + 28·i, y = 12 in
  palette index `0xd0 + 4·i`; the picked one gets a 22 × 16 outline (a flag-1 `ColoredBar`).
  In the single player editor all eight show.
- Nation combo box: `addGP_ComboBoxDLX(10, 36, 224)` on `Combo2` with the nations' names
  (count 0x2395448, names 0x2399450, current 0x28cffd0).
- Scroll bar `addNewGP_VScrollBar(222, 82, 303)` on `VScroll2`; its range is the content's
  end less 0x17e (hidden when negative, 0x5bba8a). Clipper from (4, 75) to the screen edge,
  386 down.
- Content from y = 80: per nation a list of unit ids (`0x111caa4[nation]`, u16, count
  `0x111caa0`) in which `0xffff` starts a category: a text button with the header
  (`0x111ca9c`, a text id of `EDITOR.TXT`: infantry, cavalry, artillery, marine, buildings) at
  (10, y), then y += 20. Units follow five to a row (x = 6 + 44·col, 44 apart): the picture
  is frame `NewMonster+0xe → +0x1822` of GP 0 (`ICONS.GP`, the `ICON` of the `.MD`) and a
  41 × 41 `ColoredBar` at (x − 1, y − 1), colour 0x65 (0xff for the picked one, by 0x5bb630,
  which stores the type in 0x90da78), user value `type + 0x2710`, hint the name. A row that
  is not full adds 44 when the next header or the end comes.
- Where the three nation lists are filled is not known (no store to those globals in the
  code). ACR builds them from its catalog: the land types whose id has the nation's prefix, in
  the nation files' order, split into infantry, cavalry (`spacing::is_rider`) and artillery
  (`COMPLEXROTATE`). Marine (the `Medium::Water` types) and buildings (the building types)
  follow, listed by the nation tag in brackets at the end of their id (`ShpFregat(SP)`,
  `MayBlg2(AC)`; the buildings of the Aztecs carry Maya prefixes), since ships are named for
  their kind. A ship is put only on deep water, a land unit or building not (`World::sea_allows`);
  a building is put finished and free of charge under the footprint checks of building
  (`construction::place_free`: no other building or enemy on it, no locked ground). Vegetation,
  stones and terrain tools (#406) are not done.
- ACR's additions: the combo box ends with "Tiere" and "Kanonen"; the eighth colour is the
  neutral player 7 (wild animals); the controls of docs/controls.md sit in a second paper box
  under the window; the combo box's list, the arrows and track of the scroll bar are clicks
  only (no dragging); a right click on an icon opens a drop-down of the unit's training levels
  (Basis, then the type's upgrades by their text, docs/controls.md "Editor") in the combo
  box's list look, closed by Escape, an outside click or a choice; the fonts (`fon16y1`/`fon16y3`) and the colour of the title are guesses
  (the original's are the globals 0x676be0, 0x9091cc, 0x90d69c).

## Load and save dialogs

Savegames are found by `Save\*.sav` (0x5d7c10): at most 100 files, sorted by last write
time and listed newest first. 0x5d79c0 opens each file, accepts it only with the magic
`KASF` (or a recording's `REC[`/`REC]`) and the current version, and adds two list rows:
the file name with `.sav` and the file time as `%02d:%02d:%02d   %02d.%02d.%d` (local
hour, minute, second, day, month, year). Neither dialog can delete a save.

- **Load from the main menu** (0x5d8090): `Interf2\Background_Load_Game.bmp`.
  - Titles: `INTF_LOAD_TITLE0` (512, 18, `fon40y5`), `INTF_LOAD_TITLE1` (680, 203,
    `fon16y5`, centred).
  - Page tabs: `addUniversalButton` at (506, 244) and (692, 244), 165 wide, `Interf2\Buttons`
    frames 4 (normal) / 5 (selected, hovered), fonts `fon16y3` / `fon16y1` (selected):
    `INTF_LOAD_P0` savegames, `INTF_LOAD_P1` recordings (`*.rec`, `Autorecord\*.rec`;
    0x5d7df0).
  - Per page two `addGP_ListBox`es of 8 rows of `Interf2\LoadGame`, sharing a scroll bar
    (901, 266, 238 high): names at (446, 282, frame 0, clipped to x ≤ 709, focused) and
    dates at (714, 282, frame −3). Both get `dy − 3`, `dx − 16`; the dates copy the
    names' selection every frame.
  - `INTF_LOAD_CG` (446, 528, `fon18y3`) and the selected name (602, 528, `fon18w`).
  - OK `INTF_LOAD` (472, 577) and `INTF_CANCEL` (704, 577), `Buttons` frame 0, `fon18y5`,
    `FontDy − 1`. OK is enabled only with a selected row. A `.rec` file plays the recording,
    any other file is loaded (0x4b75ba) and starts the game.
- **Load over the game** (0x5d8cd0) and **save** (0x5d9400): `DialogsSystem` at
  `((RealLx − 470) >> 1, (RealLy − 415) >> 1)`, the frame `Interf2\G_Load_Game` 4 (shadow)
  and 5 at (−60, −40). Offsets below are from that origin.
  - Scroll bar `Interf2\GScroll` at (455, 30), 255 high; names (3, 41) with highlight
    frames 6..8 and dates (271, 41), 9 rows of 26 px, `fon18w` / `fon18y3`.
  - `INTF_LOAD_TITLE1` (235, −4, `fon16y5`, centred).
  - OK (`INTF_LOAD` / `INTF_SAVE`) at (16, 351) and `INTF_CANCEL` at (248, 351):
    `G_Load_Game` frame 1, `fon18y5`, `FontDy + 4`.
  - Load: `INTF_LOAD_CG` centred at (60, 313), the selected name at (158, 313).
  - Save: `INTF_LOAD_CG` at (2, 313) and the name field `addInputBox(150, 309, 35 bytes,
    302 × 24, fon18y3 / fon18w)`, focused, holding the last saved name (0xbd63a8). Nothing
    is selected at first; choosing a row copies its name without `.sav` into the field,
    typing a different name drops the selection. OK needs a name and sends the save
    command (0x4d6ea0, network command 0x28 with the name).

ACR shows its own savegames (`docs/simulation.md`, Savegames) with the same layout;
the recordings page stays empty. The browser keeps savegames in `localStorage`. While a
loaded game replays to its saved tick, the loading screen shows `INTF_LOAD` and a
percentage (the original restores its memory image without one).

## Game setup

"Start" ends the menus with `Action::StartGame(GameSetup)` (`ui/setup.rs`): either
`GameKind::Mission(MissionSetup { id, name, map, script, texts, difficulty, campaign })`
or `GameKind::RandomMap(RandomMapSetup { seed, landscape, ground, resources, minerals,
size, climate })`, with `players: Vec<PlayerSetup { name, nation, color, team, control }>`
for random maps.

## Open questions

- The colour and team fields of the random map screen cycle on click; the original's
  exact interaction (and the "X" next to the flag) is not verified.
- Campaign progress (missions opened by finishing others) is not kept.
- Multiplayer and the editor are not rebuilt. Recordings (`.rec`) and the original's
  savegames (`KASF`) cannot be loaded; `AUTO.sav` (autosave) is not written.
- In-game: see [In-game interface](#in-game-interface).

- The hint position of the main menu (`SmallWhiteFont` at (18, 701)) is unused because no
  main menu entry has a hint.
- `ShowGP` with a negative frame (list highlight base -3) is assumed to draw nothing.
