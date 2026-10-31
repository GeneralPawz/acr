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

For phones, `top_bar=touch` in `settings.txt` (the default on Android, `classic` elsewhere;
not on an options page) replaces the top bar by two buttons of at least 48 dp: **M** (top
left) opens the M drop-down with finger-sized rows (game menu, surrender, ...), the gold icon (top right) shows or hides the six resource amounts
(tap again to close). Taps elsewhere along the top edge reach the map ([ui.md](ui.md)).

The game menu's *Spieleinstellungen* has five more pages ("Weitere Optionen »"): blood and
decay, how long the dead lie, whether craters and blood stay (*Schlachtfeldspuren*); the 3D
graphics; and ACR's optional rules on two pages (from the next battle) ([ui.md](ui.md)).

How the original's keys work inside is described in [ui.md](ui.md#keys-0x4a30e8-and-scrolling-0x4c5910).

## Mouse

| Action | Original | Modern |
|---|---|---|
| Select a unit or building | left click | left click (also Ctrl + left click) |
| Select with a rectangle | left drag | left drag |
| Add to the selection | Shift + click or drag | Shift + click or drag |
| Take out of the selection | – | **Ctrl + click** on a selected unit (its whole formation), or on a type or formation icon of the selection panel |
| Pick formations in the army list (top left) | click selects it, Shift + click adds it | click selects one (the anchor), **Ctrl + click** toggles one, **Shift + click** the range from the anchor, **Ctrl + Shift + click** adds the range |
| Select all units of that type on the screen | double click (Shift adds) | double click (Shift adds) |
| Move, attack, gather, enter, repair (context order) | right click | right click |
| Move and face a direction | right drag (more than 30 map pixels) | right drag |
| Rally point (only buildings selected) | right click | right click |
| Leave the build or rally point mode | right click | right click |
| Bombard the ground with cannons (not in missions) | the attack-ground icon, then a left or right click on the map | the same |
| Zoom in and out | – (key L: 4× zoomed-out view) | **mouse wheel**, about the point under the cursor, 0.5× to 2× |
| Pan by grabbing the map | – | **Ctrl + left drag** or **middle drag** |
| Turn and tilt the free camera (`camera=free`, ACR) | – (keys `,` `.` Page Up/Down) | **Shift + right drag**: right turns like `.`, up tilts like Page Up (0.29° per window pixel); the keys work as well |
| Scroll at the window edge | cursor within 6 px of the edge | the same, or off ("Rand aus") |
| Centre the view | left click or drag on the minimap | the same |
| Order to a point on the minimap | right click on the minimap | the same |

The army list at the top left (the `ORDERS_LIST` icon opens it, docs/ui.md) picks like a
file list with the modern controls (`InGame::army_pick`, `panel_pick` in
`ingame/controls.rs`): a plain click selects that formation alone and makes it the anchor;
Ctrl + click puts a formation in or out of the selection and moves the anchor there;
Shift + click selects the formations from the anchor to the clicked one in the list's
order (slot order, the numbers on the icons); Ctrl + Shift + click adds that range. Plain
and Shift + click start a new selection; with Ctrl the free units and other selected
objects stay. The picked formations have the yellow frame. A move order then sends them as
a group (`group_move`, on by default with the modern controls; docs/simulation.md "Group
moves"), each to its own place. The original (0x6064b0) selects on a click and adds with
Shift.

Ctrl + left click without moving on a unit that is selected takes it out of the selection:
a free unit alone, a formation member with its whole formation (officer and drummer too)
and the formations grouped with it. So with horsemen and infantry selected, Ctrl + click on
a horseman drops that horseman, and Ctrl + click on the horsemen's icon in the selection
panel drops all of them. On the panel, a type icon stands for the selected free units of
that type, a formation icon for that formation's members. Anywhere else (no selected unit hit, or while placing a building, a rally point,
a patrol point or an attack-ground point) Ctrl + left click does exactly what a plain left
click does: it selects, places a building or sets a rally point. Shift + Ctrl + click is a
plain Shift + click. In the original, Ctrl changes nothing about
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
| Kill the selected own units (buildings: set on fire) | Delete | Delete |
| Centre the view on the selection | – | **Home** |
| Select all own troops (no buildings) | – | **Ctrl + A** |
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
| Mortars (rule `mortars`): set a zone, clear the zones, fire on/off | the icons (ui.md "Mortar buttons") | the icons, or **Alt + Z**, **Alt + X**, **Alt + F** |
| Fullscreen (ACR) | F11, Alt + Enter | F11, Alt + Enter |

**Ctrl + A** (modern) selects every own unit that takes orders: living, not a building,
not inside a building, not out of the fight; formations come whole. The original binds no
Ctrl + A (`Keyboard.dat` has no modifier entries, and the A pan key passes Ctrl + A on),
so the original scheme ignores it. A move of such a selection sends every formation under
the `group_move` rule, which is on by default with the modern controls (docs/simulation.md
"Group moves": the original sends at most 32 formations).

Escape in the modern scheme first leaves a build or rally point mode, then cancels a drag
in progress, and only then opens the game menu.

**Delete** kills the selected own objects at once, without asking, as in the original
(pseudo key 0xc8, network command 0x21, `KillSelected` 0x4dc83c; ACR's `Command::Destruct`):
units die with their death animation and leave a corpse, a finished building of a
`SLOWDEATH` type drops to 1999 life and burns down, other buildings collapse. It works in
both schemes and in the editor.

### What can be selected

Only living, fighting units: a dying man or a corpse is never picked by a click, a
rectangle, a double click, a recalled control group or a formation icon, and a selected
unit leaves the selection the tick it starts to die (the original's `Die` takes it out of
selection and formation at once). Under the hit-locations rule a man out of the fight (a
crawler with a leg gone, or a man without a working arm) cannot be selected either and
leaves the selection: he takes no orders anyway and drags himself away from the enemy. The
original has no such men. A man only knocked down gets up again and stays selectable. The
same rule gives the soldier counts on the formation icons (selection panel and army list):
they drop the moment a soldier dies or goes out of the fight, not when his corpse vanishes.

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
| Ctrl + Space, C | free camera on or off (ACR) | none |
| Ctrl + Space, R | performance snapshot: "Leistungsdaten gespeichert" (ACR, [testing.md](testing.md#performance-recorder)) | none |

**Ctrl + Space, C** switches the free camera (`camera=free`, [rendering-3d.md](rendering-3d.md#settings))
for the running battle and says so on the message list with its mouse and keys ("Freie
Kamera: Umschalt + rechts ziehen dreht und neigt; , und . drehen, Bild auf/ab neigen, Ende
Grundstellung"). Under the free camera **Shift + right drag** on the map orbits it (the
drag gives no order; a right drag without Shift still moves and faces the selection, and
with the original controls Shift + right drag is a plain right drag); settings.txt keeps
its value, and OK in the game options sets the camera back to it. The view keeps its
centre both ways (the free camera turns about the ground point in the window's middle), and
the turn and tilt stay from the last time. It works with models on or off. C was chosen
because O (the letter a user may expect for "orbit") is the mission objectives.

### Keys the modern scheme takes

W, A, S and D are command and production letters in the original (see below: S is
`Stand_Ground_ON`, D `Attack_OFF`, and the production keys of the nation files use W 30
times, A 8 times). The modern scheme deliberately uses them for panning and moves these icon keys to
**Alt + W/A/S/D**; `Keyboard.dat` has no Alt entries, so nothing else moves. Ctrl + W/A/S/D
keep their original meaning. With the mortars rule and mortars selected, Alt + Z, Alt + X
and Alt + F press the mortar icons (also no `Keyboard.dat` entries). No other key changes: Home, the middle button and the wheel
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

The main menu's "Editor" opens a blank grass field for two human players with the original
editor's placing window ("Einheit wählen", docs/ui.md "Editor: the placing window") at the
right, in the column of the side panels (`crates/acr-game/src/ingame/editor.rs`). Unlike the
original's editor it places many units at once and whole formations ready-formed; it acts on
the local world directly (like the test driver's `spawn`), so an editor game is not a network
game. The rules of the settings apply (`hit_locations`, `body_spacing`). F2 or a click on the
title folds the window; the wheel over it scrolls the icons.

| In the window | What it does |
|---|---|
| The eight colours | the owner of the placed units: players 1 to 7 and, the eighth, the neutral player (wild animals) |
| Nation combo box | lists the 12 playable nations, then Tiere and Kanonen (ACR's own lists); a click opens it, a line picks it |
| Icon grid | the nation's units under Infanterie, Kavallerie and Artillerie, as `ICONS.GP` icons; a click picks the type at Basis (again: unpicks); while picked, a left click on the map places it. A right click opens the icon's training levels, see below. Marine (ships, only on deep water, as `sea_ground`) and Gebaeude follow, buildings put finished and free where their footprint is free; vegetation and terrain are not placed yet (#406) |
| Scroll bar | the arrows scroll a row, the track a page; no dragging |

Under the window ACR's own controls:

| Line | What it does |
|---|---|
| Anzahl | 1, 5, 10, 15, 36, 72, 120, 196, 392, 588, 784 or 980 |
| Formation | lose (a square block) or the formations the type's officers lead: Linie, Kolonne, Karree for infantry, the cavalry wedges as named in `ORD_GROUPS.LST` |
| Ausbau | the picked type's training level: `Ausbau n/m: name`, n of its m upgrades applied (set by the icons' right-click drop-down) |
| Richtung | the facing, in eight steps; kept per owner |
| Besitzer | the owner picked among the colours |
| Kampf | a click cycles AUS (every player is everybody's ally, nobody attacks), AN alle gegen alle (every player fights every other), Teams (owners with the same team number are allied). In Teams a right click on a colour's swatch cycles its team 1 to 4, shown as a small digit on the swatch (start: 1, 2, 3, 4, 1, 2, 3). The neutral player (animals) is hostile to everyone unless the mode is AUS. The rule sets the alliance masks (`NMask`) of the nations and objects, as the mission scripts do; the editor hands them to the script state every frame, since that copies them to the world each tick |
| Kanonen sofort laden | reloads every cannon on the field at once (its reload counter cleared) |
| Kanonen: sofort nachladen | AN: every cannon is reloaded before each tick, so it fires as soon as it is aimed (a test switch; starts AUS) |
| Ich steuere | which player's units the mouse selects and commands; a click goes to the next player (neutral included) that has living units |

The test driver lists the window's controls in `ui` as `editor/<n>` (names like `Anzahl +`,
`Ausbau Basis`, `Nation Frankreich`, `Spieler 2`, `Neutral`, an icon by its name and type id),
so `tests/e2e/editor_window.json` and `editor_levels.json` click them by label; `editor_players.json` and `editor_teams.json` cover the Kampf modes, `editor_world.json` and `editor_ships.json` the buildings and ships.

**Training levels.** A right click on a unit icon opens a drop-down (the combo box's list look)
under it: "Basis" (no upgrade), then one line per upgrade that changes how the type fights
(damage, protection, shield, life, range, reload pause, speed, cannon spread; not prices or
build times; `upgrade::of_type`) in the nation file's order. A line is the upgrade's text
as the game's upgrade hint shows it, cut before its parenthesised effect, and the effect as
the upgrade defines it: `Spezialmusketen kaufen  Nachladen -35%`; an upgrade without a text
shows the effect alone (`Schaden +10%`). Lines are cut to about 320 pixels ("..."). A choice picks the
type at that level: exactly the upgrades up to and including that line. Escape, a click
outside or a choice closes it; a right click on the map still stops placing (and, with the
drop-down open, closes it). The level is the picked type's: picking another icon with the left
click starts at Basis again. The test driver's `state` with `detail` lists each object's
`adv` (life, damage and pause per attack index of its owner's characteristics).

The levels apply the upgrades that change the type's characteristics (damage,
protection, life, range, cannon spread `RAZBROS`, reload pause, speed; `upgrade::of_type`,
in the nation file's order) as if researched, without marking them researched. Like researched
upgrades it holds per owner and type: placing a type at another level changes that owner's
units of the type already standing too, so compare levels between Spieler 1 and Spieler 2. A
formation's officer, drummer and flag bearer get the same level, as far as their own upgrades go.

A formation is placed standing on its slots with its officer, drummer and flag bearer, in the
smallest size that holds the count; above the largest size (196) several are placed side by
side across the facing. Right click stops placing; F2 or the title folds the panel; the
wheel over the panel scrolls the list. Fighting starts switched off.
