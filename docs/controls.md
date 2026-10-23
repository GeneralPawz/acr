# Controls

ACR has two in-game control schemes:

- **Original** (the default): the keys and mouse of American Conquest 1.46, unchanged.
- **Modern** (optional): wheel zoom, drag panning, W/A/S/D and a few conveniences on top of
  the original controls.

Choose the scheme in the options, in the main menu or in the game menu: the combo box beside
the heading *Spieloptionen*.

| Line | Scheme | Edge scrolling |
|---|---|---|
| Steuerung: Original | original | on |
| Steuerung: Modern | modern | on |
| Steuerung: Modern, Rand aus | modern | off |

The choice is stored with the other options when you press OK: in
`%LOCALAPPDATA%\ACR\settings.txt` on the desktop (or the folder `ACR_SETTINGS_DIR` names),
in the browser's `localStorage` entry `acr-settings` on the web. The scroll speed slider
(*Scrolltempo*) also sets how fast the keys pan in the modern scheme.

How the original's keys work inside is described in [ui.md](ui.md#keys-0x4a30e8-and-scrolling-0x4c5910).

## Mouse

| Action | Original | Modern |
|---|---|---|
| Select a unit or building | left click | left click (also Ctrl + left click) |
| Select with a rectangle | left drag | left drag |
| Add to the selection | Shift + click or drag | Shift + click or drag |
| Select all units of that type on the screen | double click (Shift adds) | double click (Shift adds) |
| Move, attack, gather, enter, repair (context order) | right click | right click |
| Move and face a direction | right drag (more than 30 map pixels) | right drag |
| Rally point (only buildings selected) | right click | right click |
| Leave the build or rally point mode | right click | right click |
| Bombard the ground with cannons (not in missions) | the attack-ground icon, then a left or right click on the map | the same |
| Zoom in and out | – (key L: 4× zoomed-out view) | **mouse wheel**, about the point under the cursor, 0.5× to 2× |
| Pan by grabbing the map | – | **Ctrl + left drag** or **middle drag** |
| Scroll at the window edge | cursor within 6 px of the edge | the same, or off ("Rand aus") |
| Centre the view | left click or drag on the minimap | the same |
| Order to a point on the minimap | right click on the minimap | the same |

Ctrl + left click without moving does exactly what a plain left click does: it selects,
places a building or sets a rally point. In the original, Ctrl changes nothing about
selection, so the modern scheme only takes Ctrl + left *drag* (a rectangle selection in the
original) for panning. Ctrl + right click keeps its meaning (no repair order). A drag only
pans once the cursor has moved more than 4 pixels. Over the interface (minimap, icons,
panels) the left button works as always, also with Ctrl. The wheel does nothing in the
4× zoomed-out view of key L; leaving that view returns to the wheel's zoom.

## Keys

| Action | Original | Modern |
|---|---|---|
| Scroll | arrow keys (speed 4) | arrow keys and **W/A/S/D**, at the *Scrolltempo* speed, the same on the screen at any zoom |
| Store control group | Ctrl + 0..9 | Ctrl + 0..9 |
| Recall control group | 0..9 | 0..9; **pressed twice** quickly (0.5 s): also centre the view on the group |
| Add control group to the selection | Shift + 0..9 | Shift + 0..9 |
| Jump to the next selected unit | Space | Space |
| Centre the view on the selection | – | **Home** |
| Leave the build, rally point or attack-ground mode | Escape | Escape |
| Game menu | F12, Escape (ACR) | F12, Escape when nothing is to be cancelled |
| Cancel a drag (rectangle, facing arrow, pan) | – | **Escape** (the rectangle restores the previous selection) |
| Life bars | Tab | Tab |
| Pause | Pause | Pause, **Ctrl + Space, P** (see below) |
| Mission objectives | F1 | F1 |
| Diplomacy panel | F8 | F8 |
| Unit panel | U | U |
| Game info panel | I | I |
| Minimap size | M | M |
| Inaccessible areas | Q | Q |
| Transparent buildings | O | O |
| 4× zoomed-out view | L | L |
| Slow/fast game mode | Ctrl + D (no effect: the simulation has one mode) | Ctrl + D |
| Command and production icons | letters, see below | the same, but **Alt + W/A/S/D** for W, A, S, D |
| Fullscreen (ACR) | F11, Alt + Enter | F11, Alt + Enter |

Escape in the modern scheme first leaves a build or rally point mode, then cancels a drag
in progress, and only then opens the game menu.

### Leader chords (modern)

In the modern scheme, **Ctrl + Space** followed by a letter (within 2 s) does what one of the
original's special keys does, so no Pause or function key is needed. Ctrl + Space shows the
letters on the message list; any other key ends the chord unused, and Ctrl or Shift pressed
on the way keep it. The original keys work as before.

| Chord | Does | Original key |
|---|---|---|
| Ctrl + Space, P | pause or go on | Pause |
| Ctrl + Space, M | game menu | F12 |
| Ctrl + Space, O | mission objectives | F1 |
| Ctrl + Space, D | diplomacy panel | F8 |
| Ctrl + Space, L | life bars | Tab |
| Ctrl + Space, F | fullscreen | F11 |
| Ctrl + Space, N | jump to the next selected unit | Space |

### Keys the modern scheme takes

W, A, S and D are command and production letters in the original (see below: S is
`Stand_Ground_ON`, D `Attack_OFF`, and the production keys of the nation files use W 30
times, A 8 times). The modern scheme deliberately uses them for panning and moves these icon keys to
**Alt + W/A/S/D**; `Keyboard.dat` has no Alt entries, so nothing else moves. Ctrl + W/A/S/D
keep their original meaning. No other key changes: Home, the middle button and the wheel
are unused by the original game screen, and the double tap of a digit still recalls the
group first.

## Icon letters (`Data\Keyboard.dat`)

A letter presses the shown icon of that name (`[CTRL|ALT|SHIFT] KEY : ICON`; all entries of
1.46 are without modifiers). Several icons share a letter; the one shown wins. Production
icons also take the key of their `[FIXED_PRODUCE]` entry in the nation files (`.NDS`); that
key comes first.

| Key | Commands | Buildings | Units |
|---|---|---|---|
| T | `Arm_attack_ON` | `B_Skl` | `U_IncTrb`, `U_IroTam`, `U_AtcMet` |
| Y | `Arm_attack_OFF` | `B_Mel`, `B_SiuMel`, the mills' gather icons | `U_*Lut` (archers of the nations) |
| C | `Cancel` | `B_Cen` (town centre) | |
| S | `Stand_Ground_ON` | `B_Do2` | `U_FrnS15`, `U_SpnS15`, `U_IncPra`, `U_HurS15`, `U_IroStr`, `U_DelS15`, `U_PueS15`, `U_SiuKStr`, `U_SpnD17`, `U_EngD17`, `U_FrnD17` |
| V | `Stand_Ground_OFF` | | |
| D | `Attack_OFF` | `B_Do1` | `U_SpnD18`, `U_EngD18`, `U_FrnD18`, `U_UsaD18` |
| E | `Attack_ON` | `B_Pst` | `U_*Sha` (the nations' shamans and priests) |
| G | `Guard_ON`/`OFF`, `Make_Group_Formation_ON`/`OFF` | | |
| P | `Patrol` | | `U_SpnAlb`, `U_*Pik` (pikemen) |
| J | `Dismiss_Formation` | `B_Kuz` | |
| R | `Formation_Square`, `Formation_Kavalery_Klin` | | `U_SpnRei` |
| K | `Formation_Kare`, `Formation_Kavalery_C` | `B_Kre2` | |
| Z | `Formation_Line`, `Formation_Kavalery_Line` | | `U_SpnM18`, `U_UsaM18`, `U_FrnM18`, `U_EngM18` |
| F | `Add_Formation` | `B_Ver` | |
| B | | `B_Kre`, `B_Kaz` | |
| N | | `B_Rud`, `SpnRud(MA)Inside` | `U_SpnMus`, `U_FrnS17`, `U_EngMus` |
| H | | `B_Hra` | |
| X | | | `U_Pushka` (cannon) |

Keys of the `[FIXED_PRODUCE]` lists of the 12 playable nations (number of entries): A 8,
B 8, C 7, D 14, E 30, F 31, G 4, H 11, I 7, J 11, K 12, L 7, M 4, N 27, O 7, P 9, Q 67,
R 19, S 7, T 26, U 8, W 30, Y 21. The game screen's own keys come first: U, I, M, Q, O and L
toggle their panels and views and never reach these icons.

## Editor (test field)

The main menu's "Editor" opens a blank grass field for two human players with a placing
panel at the top left (`crates/acr-game/src/ingame/editor.rs`). Unlike the original's editor
it places many units at once and whole formations ready-formed; it acts on the local world
directly (like the test driver's `spawn`), so an editor game is not a network game. The
rules of the settings apply (`hit_locations`, `body_spacing`).

| Panel line | What it does |
|---|---|
| ENG … IRO, Tiere, Kanonen | lists the land units of a nation, the animals or the cannons |
| a unit | picks it (again: unpicks); while picked, a left click on the map places it |
| Anzahl | 1, 5, 10, 15, 36, 72, 120, 196, 392, 588, 784 or 980 |
| Formation | lose (a square block) or the formations the type's officers lead: Linie, Kolonne, Karree for infantry, the cavalry wedges as named in `ORD_GROUPS.LST` |
| Richtung | the facing, in eight steps; kept per owner |
| Besitzer | Spieler 1, Spieler 2 or Neutral (wild animals) |
| Kampf | AUS: every player is everybody's ally, nobody attacks; AN: the sides fight |
| Ich steuere | which player's units the mouse selects and commands |

A formation is placed standing on its slots with its officer, drummer and flag bearer, in the
smallest size that holds the count; above the largest size (196) several are placed side by
side across the facing. Right click stops placing; F2 or the title folds the panel; the
wheel over the panel scrolls the list. Fighting starts switched off.
