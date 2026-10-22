# Menus and dialog system

How the original menus of `dmcr.exe` work, recovered from the disassembly, and how
`crates/acr-game/src/ui/` rebuilds them. Addresses are virtual addresses in `dmcr.exe`
(v1.46); the exe exports its C++ symbols (`DialogsSystem`, `SimpleDialog`, ...), so most
functions have names. The widget code is the same as in the published Cossacks: Back to War
sources (`Dialogs.cpp`), constants included.

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
  Editor: 0x59c2a0.

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
The original lays it out for the screen size (`RealLx` × `RealLy`). ACR uses the
1024 × 768 canvas of the menus, scaled and letterboxed, while the map fills the whole
window. The view has no bottom panel: `0x676b84` is 1 and is never written, so
`PanelLy` (144) only moves the icon rows. Text and lines use palette set 0
(`0\agew_1.pal`) and its font colour tables. Sprite packs use the palette from
`acr_formats::sprite_palette`. The frame function 0x4c1a20 draws, in this order:
minimap (0x4c0a40), unit stats (inside 0x606710), icons (0x5a9fa0), then the
resource panel and "M" drop-down (0x4c1a70).

Layout constants (0x5aa6d2): `Const.lst` defines none of `Zone1X`, `PropertyX`, … , so
the defaults apply: `Zone1X/Y` = 6/6, `PropertyX/Y` = 182/6, `IconLx/Ly` = 40/40,
`AddIconLx/Ly` = 2/2, `ZonesNx/Ny` = 12/6.

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
- Single player rows: `UM_MENU` (game menu), `UM_INF1`, `UM_INF3`, `UM_MINI` (minimap
  size), `UI_LOCK1`, `UM_TRANS2`, `4XSCALE_ON`, `UM_SURRENDER`, `UM_REPLAY` (restart).
  ACR runs the game menu, the minimap toggle and restart; the other rows do nothing yet.

### Unit and command icons (0x606710 builds, 0x5a9fa0 draws)

- Icons are `OneIcon`s (0x44 bytes, drawn by 0x494c96) of `ICONS.GP`. The frame is the
  line index of the icon's name in `ICONLIST.TXT` (lookup 0x4f0516).
- Grid: 12 columns, 42 px apart, from (`PropertyX`, `RealLy − 144 + 6`) = (182, 630).
  Three rows are shown. Two sets share the grid:
  - the selected types (0x28f8190), in the first free slots;
  - the commands (0x28f9e30), at fixed slots `x + 12·y`.
- Box: the box from (180, 630) to (182 + 42·columns, 755) is drawn with
  `Border` and corner table 0x668580 = TL 0, TR 2, BL 6, BR 8, left 12, right 14, top 9,
  bottom 11 (0x47f48a: edges tiled every 64 px and centred on the box lines). The
  separate left box of 0x5aa1bb is dead code: `0x28f819c` is never written.
- A type icon has:
  - an outline in player colour `0xd0 + 4·nation`;
  - a life bar: two lines of colour 255 with the rest in colour 0, length
    `38·life/max`;
  - the count in `WhiteFont`, shadowed by `BlackFont` at ±1, at the top left.
  Clicking it keeps only that type selected.
- A highlighted icon gets a frame in `GetPaletteColor(255, 200, 64)`. A pressed icon is
  drawn 1 px down-right. A disabled one is drawn transparent.
- Single selected type: numbers drawn straight over the map, in `SmallWhiteFont`, with
  `SMICONS.GP` icons from `NRES.DAT [WEAPONS]`:
  - up to 2 attacks at (53 + 55·n, 630), text at x + 18, y + 4;
  - the shield `#GENSHIELD` at (53, 651);
  - up to 6 protections in 3 columns at (13 + 55·c, 681 + 19·r).
- Command slots wired so far:
  - formation groups of the officer's `[OFFICERS]` entry, at their `NRES.DAT
    [ORDERICONS]` slots (column 8). The current formation is highlighted. These send
    `Form`, re-forming an existing formation.
  - For a formed brigade: slot 24 `SFREORD` ("Formation auflösen", `Disband`). Slots 25
    and 26 (`STANDGR`/`NOSTAND`) and `GO&ATTACK` (slot 27) are shown but inactive.
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
  - left click: `Produce` (0x6088c0, 0x4d6226), five times with Shift;
  - right click: `Unproduce` (0x608810, 0x4d6e08), five times with Shift;
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

### Placing a building (0x608a0d, 0x4bdd1e)

- A building item of a peasant starts the build mode (`BuildMode` 0x10fc644,
  `BuildingID` 0x10fca60), but only if the player can pay (`CanAfford` 0x514c65; the
  original also prints what is missing, which ACR does not).
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
  its packs then, and the sprite atlas is rebuilt (`ObjectsView::want_units`).

### Right click and context cursor (0x4c4668..0x4c4ce2)

The cursor shows what a right click on the map does with the selection:

| Under the cursor | Order | Cursor (`CURSOR_00.GP`) |
|---|---|---|
| enemy | `Attack` | 1, sword (0x66a924) |
| own unfinished or damaged building, selection has builders (`#WORK`), Ctrl not held | `Repair` | 3, hammer (0x66a920) |
| own building one of the selected units may enter | `Enter` | 11 (0x66a918) |
| a source within 64 px (`DetermineResource`) that a selected unit can gather | `Gather` | by source type (0x66a934): wood 6, minerals 5, field 7, meat 1 |
| anything else | `Move` with the `CursGo` marker | 0, arrow |

With only buildings selected, the right click sets the rally point. Map points account for
the terrain height under the cursor.

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

### Cursor and overlays

- Cursor: `LoadFog(0)` selects `CURSOR_00.GP`. Hotspots and animation chains come from
  `curs_xy.dat` (`dx dy next`, a step every 60 ms). Frame 0 (arrow) is the default; the
  context cursors are listed under [Right click and context cursor](#right-click-and-context-cursor-0x4c46680x4c4ce2).
  Storming, healing and the other cursors wait for the simulation.
- Selection brackets (0x4bae50): 3 px corners in `GetPaletteColor(255, 255, 0)` around
  `RECTANGLE dx dy w h` (defaults 0 3 40 64). Soldiers of a formed brigade get none.
- Life bars (Tab, 0x4bb390): green lines at y0 − 12 and y0 − 11 with white end caps.
- Selection rectangle: `GetPaletteColor(0, 255, 255)`.
- `Interface\CursGo` at a move target: one frame per game frame, 17 frames (0x587fa9).

### Keys (0x4a30e8) and scrolling (0x4c5910)

- F12: game menu. Escape leaves the build or rally point mode (0x4a31a3); otherwise ACR
  also opens the menu with it.
- Control groups: Ctrl + digit stores, digit recalls, Shift + digit adds.
- Space: jump to the next selected unit. Pause: pause. Tab: life bars. M: minimap size.
- A second click on a unit selects all own units of that type on the screen. This is
  not verified in the original.
- Shift adds to a selection.
- Scrolling is updated once per game frame:
  - arrow keys set the speed to ±4;
  - the cursor within 6 px of the edge accelerates up to `ScrollSpeed`;
  - otherwise the speed decays by 1.
  The view moves half the speed in map cells (32 × 16 px) per game frame. ACR spreads
  that step over the frame so it moves smoothly.
- Game speed: ticks/s = `1000 / (2·FPSTime)`. ACR caps the uncapped setting at 100.

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
  - browser window `RunExplorer(1, page, 762, 77, 1016, 470)` at 1024 × 768.
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

### Open questions

- Which command icons the original shows for a selection without an officer (attack
  on/off, friendly fire, arm attack…): the conditions in 0x606710 are only partly
  decoded.
- The in-game menu cursor (`cur_mn.rlc`), and the "PAUSE" text position: ACR centres
  `PAUSETEXT` in `YellowFont`.
- Selection with Ctrl and the double click (`DBLCLICK`) are not traced.
- The right-drag facing arrow (0x4c5407) and fog of war on the minimap are not built.
- The side panels of keys U and I (modes 1..3 of 0x4ce310) are not built; mode 4 is the
  diplomacy panel.
- Production: the unit limit (`+0x173a`), the `MUSTREPAIR` hint and the `+0x1729`
  special items (0x4d7682) are not built. The market has no interface yet (the simulation
  has `Trade`).
- Whether the original hides a selected type's icon when production icons take its slot.
- The original builds on the button press; ACR too, but the order in which it checks the
  site and the price on the click (0x4c20ac) is not traced.

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
