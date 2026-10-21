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

In ACR the editor entry does nothing; multiplayer and load show their background and
title only.

## Open questions

- The hint position of the main menu (`SmallWhiteFont` at (18, 701)) is unused because no
  main menu entry has a hint.
- `ShowGP` with a negative frame (list highlight base -3) is assumed to draw nothing.
