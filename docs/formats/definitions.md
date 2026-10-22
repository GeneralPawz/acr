# Object definitions: units, buildings, nations, sounds, map pieces

Reverse engineered from American Conquest 1.46 (`ALL.GSC` + `Patch01.GS1` + `Patch02.GS1`)
and the keyword tables and `sscanf` formats in `dmcr.exe`. Every rule below is checked by
`acr defs-check <game-dir>`, which parses all files and resolves the cross references;
`acr unit <game-dir> <NAME>` prints one parsed definition with its sprite packs.

Implementation: `crates/acr-formats/src/defs/` (`md.rs`, `nds.rs`, `sounds.rs`,
`smp.rs`, `lnk.rs`) and `crates/acr-formats/src/text.rs`.

| files | count | format | parser |
|---|---:|---|---|
| `*.MD` in the archive root | 252 | unit/building definition | `UnitDef` |
| `DESCRIPTION\*.MD` | 217 | CP1252 prose, one description per object | `CodePage::Cp1252` |
| `NMLIST.TXT` | 1 | names of the `.MD` files the engine loads (231) | `defs::name_list` |
| `*.NDS` (nations) | 16 + `TEXT\IROKEZ.NDS` | nation: members, build menus, tech tree | `NationDef` |
| `WEAPON.NDS` | 1 | weapon definitions in the same container | `NdsFile` |
| `NATIONS.LST` | 1 | nation list | `NationList` |
| `SOUNDLIST.TXT` | 1 | named sound groups | `SoundList` |
| `ICONLIST.TXT` | 1 | icon names, referenced by `ICON` and upgrades | `defs::name_list` |
| `BUILDINGS\*.SMP`, `PIECES\*.SMP` | 86 + 804 (+ 752 loose in `UserPieces`) | map piece (binary) | `Piece` |
| `PIECES\*.SMP.LNK` | 802 | piece links (binary) | `PieceLinks` |

`.SMP`/`.LNK` are **not** sound samples or animation links, contrary to the issue title:
they are map-editor terrain pieces. Sounds are bound to animations by the `SOUND`
command of `.MD` files and defined in `SOUNDLIST.TXT`; animations are defined inside the
`.MD` files and reference `.GP` sprite packs directly.

## Coverage

`acr defs-check` on the full installation:

- **MD**: 252 of 252 definitions parse (19 338 commands, 171 distinct keywords, 3 320
  animations, 47 217 frame references); **0 unknown keywords**. 250 files parse without an
  issue; the other two reference an undefined animation (`SETACTIVEPOINT #ATTACK` in
  `ANMMED.MD`, `ANMMEDSM.MD`, a data error). 52 commands carry surplus tokens that the
  engine skips (see *Lexical rules*).
- Every one of the 47 217 frame references resolves to an existing frame of an existing
  `.GP`, except 84 in three files that are not in `NMLIST.TXT` (`ARTPUS_TEMP.MD`,
  `ANMENG.MD`, `ANMMED_SM.MD`), i.e. leftovers the game never loads.
- **NDS**: all 17 nation files and `WEAPON.NDS` parse without issues: 381 distinct
  members, 1 957 upgrades, 151 weapons. Every member resolves to a loaded `.MD` by `NAME`.
- **NATIONS.LST**, **SOUNDLIST.TXT** (94 sounds, 257 file entries): no issues.
- **SMP**: all 1 642 pieces (890 archived) and 803 `.LNK` files consume their bytes exactly.
  The chunk structure and the `VERT`, `SPRT`, `LOCK`, `OBJS`, `PIX1` record layouts are
  verified; `NRG1` and `ZONE` payloads are kept raw.

Remaining unresolved references (all data quirks, reported by `defs-check`): 16 `SOUND`
commands for animations the unit lacks, 7 `PIECE` files that do not exist, 6 upgrades
in `USA.NDS [UPGRADEPLACE]` that are not defined, 5 upgrade targets `empty(FR)` in
`USA.NDS`, 1 icon in `TEXT\IROKEZ.NDS`, 4 missing `.wav` files, 18 nations of the leftover
*Cossacks* block of `NATIONS.LST`. Everything else (icons, sounds, weapons, sprite packs)
resolves.

## Text encodings

All text files are single-byte. Which code page applies depends on the file and even the
part of a line (`crates/acr-formats/src/text.rs`):

| text | code page | evidence |
|---|---|---|
| localised texts: `DESCRIPTION\*.MD`, mission texts | CP1252 | German umlauts at `0xE4`, `0xF6`, `0xFC`, `0xDF` |
| `MESSAGE` of `.MD`, Russian tokens in `.NDS` | CP866 | `8A AE AF A5 A9 E9 A8 AA` = `Копейщик` (pikeman) in `ANMBIZ.MD`; in CP1251 it decodes to garbage |
| developer comments after `//` in `.MD` | CP1251 | `F1 F2 EE FF F2 FC` = `стоять` (stand) after `USERLC` lines; in CP866 it is box-drawing garbage |

`MESSAGE` holds the developer's Russian name (`Городской_центр`, town centre); the
in-game names come from the text tables. `text::guess_cyrillic` tells CP866 from CP1251 by
where the frequent lowercase letters fall (а–п are `0xA0..=0xAF` in CP866). Keywords,
ids and paths are ASCII. Note that `0xA0` (CP866 `а`) is not whitespace.

## Lexical rules (`.MD`, `.NDS`, `SOUNDLIST.TXT`, `NATIONS.LST`)

- Tokens are separated by ASCII whitespace (bytes ≤ `0x20`); line ends are `CR LF`.
  Argument lists continue across line breaks (`EX_SHOTS2`, `ADDSECTION` put one record per
  line).
- A token starting with `/` comments out the rest of its line (`//`, but also `/13 0 0`
  and `/EngB17(EN)` disable whole lines).
- After a `.MD` command's arguments the rest of the line is skipped. Evidence: 52 commands
  have surplus tokens, e.g. `#BUILDLO_0 1 1 0 0 0 0` (one frame announced, two given),
  `PRICE 1 FOOD 50 GOLD 10`, `STAGE_FIRES 2 13 ...` with one pair too many. Read
  token-by-token these would be unknown commands (`0`, `GOLD`), which the engine reports
  as errors; the shipped game loads them silently. So `PRICE 1 FOOD 50 GOLD 10` costs
  only food. The parser keeps such tokens in `Command::ignored`.
- `[END]` terminates a `.MD` file.

## Unit and building definitions (`*.MD`)

The engine loads the files listed in `NMLIST.TXT` (231 of the 252 root `.MD` files; the
21 others are leftovers such as `ARTPUS_TEMP`, `HOL*`, `PORT*`) and indexes them by
`NAME` (`FIELD1.MD` defines `FIELD`, `FIELD2.MD` defines `FIELDP`).

Example (`ANMBIZ.MD`, a bison):

```text
NAME AnmBiz
MESSAGE Копейщик
ICON U_Anm
USERLC 0 AnmBizC  SHADOW -160 -132
USERLC 1 AnmBizG  SHADOW -160 -132
USERLC 4 AnmBizH  shadow -160 -132
#STAND     9 1 0 0
@MOTION_L  9 1 0 11
@DEATH     9 4 0 47
LIFE      400
...
[END]
```

### Sprite sets and animations

`USERLC index NAME SHADOW|NONE dx dy` declares sprite set `index` = `NAME.GP` in the
archive root (`AnmBizC` → `ANMBIZC.GP`). `dx dy` is the drawing offset of the frames
(units around `-62 -85`, buildings `0 0`); the shadow word is `SHADOW` in any case or
`NONE`. Developer comments after `USERLC` give the frames per direction of the pack
(`//16 стоять` = 16 frames "stand").

Animations come in two forms; the name after `#`/`@` is the slot:

```text
#NAME dirs n (set frame)*n      explicit list of n frames, e.g. #ATTACK 9 2 0 0 0 1
@NAME dirs set first last       frames first..=last of one set; last < first plays backwards
```

`dirs` is the number of stored directions: 1 (buildings), 9 (most units, mirrored to
16), 16 (artillery, `FULLROTATION`: all around), 65 (ships). Frame `f` of a set whose pack holds `N`
frames exists for each direction: **verified** for all 47 217 references of loaded
files, `f < N / dirs` always holds (`ANMBIZG.GP` has 216 = 9 × 24 frames, used as
`@MOTION_L 9 1 0 11` and `@MOTION_R 9 1 12 23`). The drawing code (0x5062a3) shows the
frames are **direction-interleaved**: frame `f` of stored direction `k` is GP frame
`f · dirs + k`. With 9 or 16 stored directions, `k = 0` is south and the west half is
stored; the east half is drawn mirrored. The mapping from the unit direction is described
in [`docs/simulation.md`](../simulation.md#directions-and-sprites).

Animation slots known to the engine (from `dmcr.exe`): `#STAND #PSTAND #MOTION_L
#MOTION_R #MOTION_LB #MOTION_RB #MINISTEP_L/R/LB/RB #ATTACK #PATTACK #UATTACK #DEATH
#DEATHLIE1..3 #WORK #FIST #STANDHI #STANDLO #BUILDHI #BUILD #DAMAGE #TRANS01 #TRANS10
#REST #REST1 #ROTATE_R #ROTATE_L #WORKTREE #WORKSTONE #WORKFIELD #BUILDLO_0..3
#BUILDHI_0..3 #HIANIMATION #TRANSX3 #TRANS3X #SHAR #SHARBUILD`, and `#ATTACKn`,
`#PATTACKn`, `#UATTACKn`, `#PSTANDn`, `#MOTION_*n`, `#MINISTEP_*n` for attack index
`n` = 0..10. All names in the data are among them. `PATTACK`/`UATTACK` are probably
"prepare" and "un-prepare" (the frames of `@UATTACK` run backwards), `L`/`R`/`LB`/`RB`
the two walking phases forwards and backwards, `STANDLO`/`BUILDLO_n` the finished
building and its construction stages.

### Keywords

Grammar notation: `i` integer, `s` word, `#(…)` a count followed by that many groups,
`…` see text. "Cmds" counts commands, "files" the files using them (252 parsed). Meaning
column: what is evident from the file comments (quoted), the engine strings, the
transliterated Russian names or the values; `?` marks guesses, blank means unknown.

| keyword | grammar | cmds | files | meaning |
|---|---|---:|---:|---|
| `#NAME` | `i #(ii)` | 1980 | 249 | animation, frame list (see above) |
| `@NAME` | `iiii` | 1340 | 125 | animation, frame range (see above) |
| `3DBARS` | `#(iiiii)` | 119 | 119 | building: 3D health/progress bar segments? |
| `ADDSECTION` | 15 × `i` | 29 | 15 | building section: stage, sprite set, life, then 4 × (frame x y), see [Building sections](#building-sections-addsection) |
| `ADDSHOTRADIUS` | `i` | 107 | 106 | extra shooting radius (garrison?) |
| `ADD_ATTACK_RADIUS` | `ii` | 26 | 26 | attack index, extra radius |
| `AISHIP` | `sii` | 6 | 6 | AI ship class (`B`) and two parameters |
| `ANIMAL` | | 9 | 9 | flag: animal |
| `ARCHER` | | 12 | 12 | flag: archer |
| `ARMATTACK` | | 61 | 56 | flag: has the "armed attack" toggle (`Arm_attack_ON` icon)? |
| `ARTDEPO` | `iiii` | 5 | 5 | artillery depot capacity per kind? |
| `ARTPODGOTOVKA` | | 2 | 2 | flag: artillery bombardment ("артподготовка") |
| `ATTACK_ANGLES` | `iii` | 26 | 25 | attack index, min and max shot angle |
| `ATTACK_PAUSE` | `ii` | 400 | 195 | attack index, reload pause |
| `ATTACK_RADIUS` | `iii` | 412 | 211 | attack index, min and max range |
| `ATTMASK` | `i #(s)` | 12 | 12 | attack index, materials it may attack |
| `BARABAN` | | 13 | 13 | flag: drummer ("барабан") |
| `BORNPOINTS` | `#(ii)` | 117 | 117 | building: spawn points of produced units |
| `BORN_SOUND` | `s` | 89 | 82 | sound when produced |
| `BREAKANIMATION` | `s` | 111 | 87 | animation that may be interrupted (`#REST`)? |
| `BREFLECT` | | 109 | 106 | flag (buildings) |
| `BUILDBAR` | `iiii` | 119 | 119 | building: construction bar rectangle? |
| `BUILDING` | | 119 | 119 | flag: is a building |
| `BUILDLOCKPOINTS` | `#(ii)` | 118 | 118 | building: cells blocked during construction |
| `BUILDPOINTS` | `#(ii)` | 118 | 118 | building: positions of the builders |
| `BUILDSTAGES` | `i` | 252 | 249 | build/production time |
| `CANBECAPTUREDWHENFREE` | | 131 | 111 | flag: captured when undefended |
| `CANBEKILLEDINSIDE` | | 131 | 111 | flag: garrison can be killed |
| `CANKILL` | `#(s)` | 213 | 212 | materials it can damage |
| `CANSHOOT` | | 37 | 37 | flag |
| `CANSTOPBUILD` | | 61 | 61 | flag: construction can be paused? |
| `CANSTORM` | | 124 | 113 | flag: can storm/capture buildings |
| `CANTCAPTURE` | | 24 | 23 | flag: cannot capture |
| `CAPTURE` | | 20 | 20 | flag: can capture |
| `CATEGORY` | `s` | 2 | 2 | category (`STRELOK`) |
| `CHECKPOINTS` | `#(ii)` | 119 | 119 | building: cells checked for placement |
| `COMMANDCENTER` | | 14 | 14 | flag |
| `CONCENTRATOR` | `#(ii)` | 117 | 117 | building: gathering points |
| `COSTPERCENT` | `i` | 138 | 127 | percent the price changes per built object? |
| `CRAFTPOINTS` | `#(ii)` | 4 | 4 | building: points |
| `DAMAGE` | `ii` | 321 | 167 | attack index, damage |
| `DAMAGEDEC` | `ii` | 39 | 39 | attack index, damage decrease with distance? |
| `DAMPOINTS` | `#(ii)` | 9 | 9 | building: points |
| `DESTRUCT` | `i #(s)` | 124 | 124 | "Max_Born_Probability Amount_of_weapons List_of_weapon" on destruction |
| `DET_RADIUS` | `iii` | 412 | 211 | attack index, min and max detection radius? |
| `DIRECTTRANS` | `ii` | 23 | 17 | direct transition between attack indices? |
| `DOOR` | `#(iiii)` | 99 | 99 | building entrances (two points each) |
| `EVROSHOT` | `i` | 51 | 51 | European shot type? |
| `EVROUNIT` | | 65 | 65 | flag: European unit |
| `EXDAMAGE` | `iii` | 160 | 101 | attack index, two values |
| `EXITPAUSE` | `i` | 6 | 6 | pause when leaving a building? |
| `EXPLMEDIA` | `si` | 126 | 126 | explosion media (`BUILDING`, NRES.DAT `[MEDIA]`), value |
| `EXPLPOINTS` | `i #(ii)` | 1 | 1 | "No_Stage(0or1?) Kolichestvo kardinati blocka" (stage, count, coordinates) |
| `EXSPRITES` | `siiii` | 2 | 2 | sprite object name and parameters (fields) |
| `EX_SHOTS` | `#(9 × i)` | 4 | 4 | building: shot origins, one record per line |
| `EX_SHOTS2` | `#(10 × i)` | 97 | 97 | building: shot origins, one record per line |
| `FARM` | | 104 | 104 | flag: provides housing |
| `FEARFACTOR` | `ii` | 224 | 77 | morale/fear parameters |
| `FEARRADIUS` | `ii` | 157 | 57 | morale/fear parameters |
| `FEARTYPE` | `ii` | 91 | 70 | morale/fear parameters |
| `FIRELIMIT` | `i` | 32 | 32 | |
| `FIRES` | `#(ii)` | 94 | 94 | building: fire positions when damaged |
| `FISHING` | `ii` | 1 | 1 | fishing parameters |
| `FLAGS` | `i #(iiii)` | 3 | 3 | ship flag positions? |
| `FORCE` | `i` | 111 | 111 | strength |
| `FORMLIKESHOOTERS` | | 32 | 32 | flag: forms formations like shooters |
| `FORWARD` | | 3 | 3 | flag (canoes) |
| `FRIENDLYFIRE` | | 53 | 49 | flag |
| `FULLROTATION` | | 2 | 2 | flag |
| `GEOMETRY` | `iii` | 248 | 248 | "Radius1 Radius2 StepSize" |
| `HIGHUNIT` | | 28 | 28 | flag: tall unit (cavalry, artillery) |
| `HIGHUNITCANTENTER` | | 88 | 88 | flag: tall units cannot enter |
| `ICON` | `s` | 251 | 251 | icon from `ICONLIST.TXT` |
| `IDLE` | `iiii` | 1 | 1 | |
| `INFO` | `ii` | 249 | 249 | |
| `LIFE` | `i` | 249 | 249 | hit points |
| `LINEARSORT` | `i` | 8 | 7 | ships: sort parameter |
| `LINESORT` | `s` then `GROUND`, `TOP`, `LINE iiii`, `POINT ii` | 819 | 119 | depth-sorting geometry of a building animation, one entry per part (0x4f9c70): `LINE x1 y1 x2 y2`, `POINT x y` (the line `x y x y`), `GROUND` (line at −10000: drawn flat first), `TOP` (line at 10000) |
| `LOCATION` | `iiii` | 126 | 126 | picture rectangle (x y w h) |
| `LOCKPOINTS` | `#(ii)` | 119 | 119 | building: blocked cells |
| `LOWCOSTRADIUS` | `i` | 113 | 112 | |
| `MATHERIAL` | `#(s)` | 249 | 249 | own material(s): `BODY STONE WOOD IRON BUILDING WOOD_BUILDING STENA` |
| `MAXINDEPO` | `ii` | 3 | 3 | capacity in an artillery depot? |
| `MEDIA` | `s` | 7 | 7 | `LAND` or `WATER` |
| `MESSAGE` | `s` | 252 | 252 | developer name, CP866 |
| `MFARM` | `i` | 98 | 89 | housing provided |
| `MOTIONSTYLE` | `s` | 9 | 9 | `FASTROTATE&MOVE SLOWROTATE SHEEPS COMPLEXROTATE ROTATE&MOVE NEWSHEEPS` ("sheeps" = ships) |
| `MULTIWEAPON` | `iii #(ii)` | 3 | 3 | ships: gun positions |
| `NAEMNIK` | | 5 | 5 | flag: mercenary ("наёмник") |
| `NAME` | `s` | 252 | 252 | object id |
| `NIKAKIXMAM` | | 8 | 8 | flag (animals; "никаких мам", "no mothers") |
| `NO25` | | 8 | 8 | flag |
| `NOPAUSEDATTACK` | `i` | 54 | 39 | attack index without pause? |
| `NO_HUNGRY` | | 9 | 9 | flag: does not eat |
| `OFFICER` | | 7 | 7 | flag: officer |
| `OPPOSIT` | `ii` | 130 | 111 | |
| `ORDER_SOUND` | `s` | 96 | 89 | sound on order |
| `PAGE1`..`PAGE3` | `s` | 103 each | 101 | command panel page icons |
| `PAGE1HINT`..`PAGE3HINT` | `s` | 105 each | 105 | their hints |
| `PEASANT` | | 14 | 7 | flag: peasant |
| `PEASANTABSORBER` | `i` | 5 | 5 | peasants that can work inside |
| `PEASANTCONVERTOR` | | 35 | 35 | flag |
| `PIECE` | `s` | 89 | 89 | map piece placed with the building (`BUILDINGS\EngCen.smp`) |
| `PORT` | `sii` | 8 | 8 | animation and point |
| `PORTION` | `#(si)` | 10 | 10 | resources carried per trip? |
| `PRICE` | `#(si)` | 243 | 243 | cost: resource (`WOOD GOLD STONE FOOD IRON COAL`) and amount |
| `PRIEST` | | 1 | 1 | flag |
| `PRODUCER` | `#(sii)` | 4 | 4 | mines: resource and two rates |
| `PROMAX` | `i` | 112 | 111 | |
| `PROMAX2` | `ii` | 116 | 104 | |
| `PROMAXCTG` | `i` | 33 | 32 | |
| `PROTECTION` | `#(si)` | 140 | 139 | armour per weapon kind (`NRES.DAT [WEAPONS]`) |
| `PSIXOZ` | `i` | 69 | 69 | panic ("психоз")? |
| `RANGE` | `iiiiii` | 137 | 137 | "AttackRange MaxAttackRange MinAttRange MaxNearRange ActionType VisibilityRadius" |
| `RASTRATA_NA_VISTREL` | `i #(si)` | 55 | 53 | attack index, resources spent per shot ("расход на выстрел") |
| `RASTRATA_NA_VISTREL2` | `ii #(si)` | 13 | 9 | as above with one more parameter |
| `RATE` | `ii` | 15 | 10 | attack index (< 4), rate |
| `RAZBROS` | `i` | 135 | 135 | shot dispersion ("разброс") |
| `RECTANGLE` | `iiii` | 249 | 249 | selection rectangle? |
| `RESCONSUMER` | `si` | 27 | 27 | resource consumption |
| `RESOURCEBASE` | `#(s)` | 13 | 13 | resources that can be delivered here |
| `RESSUBST` | | 1 | 1 | flag |
| `ROTATE` | `i` | 7 | 7 | rotation speed |
| `SEARCH_ENEMY_RADIUS` | `i` | 209 | 209 | enemy search radius |
| `SEARCH_ENEMY_RADIUS_SHIFT` | `i` | 67 | 67 | |
| `SELECT_SOUND` | `s` | 89 | 82 | sound on selection |
| `SETACTIVEPOINT` | `s i` + dirs × `ii` | 77 | 66 | "Activepoints for shot": animation, frame, shot origin per direction |
| `SETACTIVEPOINT2` | `s i` + 2·dirs × `ii` | 4 | 3 | two origins per direction |
| `SETANMPARAM` | `iiii` | 119 | 119 | "offset of picture(x,y) sprites/picture sprite_width" |
| `SHAMAN` | | 7 | 7 | flag |
| `SHIELD` | `i` | 6 | 6 | shield value |
| `SHOWDELAY` | | 166 | 163 | flag |
| `SHOWIDLE` | | 18 | 18 | flag: listed by the idle button |
| `SHOWIDLEPEASANTS` | | 8 | 8 | flag |
| `SKILLDAMAGEBONUS` | `i` | 58 | 58 | |
| `SKILLDAMAGEMASK` | `i` | 58 | 58 | |
| `SLOWDEATH` | | 114 | 114 | flag |
| `SLOWRECHARGE` | | 21 | 21 | flag |
| `SMOKE` | `#(ii)` | 116 | 116 | building: smoke positions |
| `SOUND` | `iss` | 332 | 111 | frame, animation, `SOUNDLIST.TXT` sound: plays the sound at that frame? |
| `SPRITEOBJECT` | | 2 | 2 | flag: object is a map sprite (fields) |
| `STAGELOCKPOINTS` | `i #(ii)` | 1 | 1 | blocked cells per stage |
| `STAGE_FIRES` | `i #(ii)` | 21 | 10 | fire positions per stage |
| `STAGE_SMOKE` | `i #(ii)` | 26 | 11 | smoke positions per stage |
| `STANDGROUND` | | 102 | 102 | flag: can stand ground |
| `STOPDISTANCE` | `i` | 21 | 21 | |
| `STORMFORCE` | `i` | 106 | 106 | storming strength |
| `TAKERESSTAGES` | `siii` | 23 | 10 | resource gathering stages |
| `TORG` | | 1 | 1 | flag: trade ("торг") |
| `TRANSFORMTOMEAT` | `i` | 9 | 9 | animals: becomes food when killed |
| `TRANSPORT` | `i` | 1 | 1 | transport capacity (ferry) |
| `TURNOFF` | | 24 | 24 | flag |
| `TWOPARTS` | `si` | 1 | 1 | animation and value (ferry) |
| `UNITABSORBER` | `i` | 113 | 113 | garrison capacity |
| `UNITRADIUS` | `i` | 94 | 94 | collision radius |
| `USAGE` | `s` | 235 | 229 | role from the engine list (`FARM DIPLOMAT LIGHTINFANTRY STRELOK TOWER ...`) |
| `USERLC` | `issii` | 849 | 249 | sprite set (see above) |
| `USETRANSX` | `i` | 43 | 21 | attack index using `#TRANSX3`/`#TRANS3X`? |
| `VES` | `i` | 235 | 232 | weight ("вес") |
| `VISION` | `i` | 163 | 158 | sight, 0..8 |
| `WATERCHECKDIST` | `ii` | 28 | 28 | |
| `WAVES` | `iii #(ii)` | 7 | 7 | ships: wave positions |
| `WEAPON` | `is` | 275 | 162 | attack index, weapon from `WEAPON.NDS` |
| `WEAPONINSECTOR` | `iiiiiii` | 1 | 1 | |
| `WEAPONKIND` | `is` | 387 | 211 | attack index, weapon kind (`NRES.DAT [WEAPONS]`) |
| `ZALP` | `i` | 14 | 14 | volley ("залп") |
| `ZPOINTS` | `ii` | 151 | 150 | |

The engine's keyword table has more entries that no shipped file uses; the parser treats
them as unknown (rest of line kept): `ANMSH ATTACK_SOUND AUTO_NO_ATTACK AUTO_STAND_GROUND
CANNONLIKEMOTION CHECKWATER COST COST_FOOD DEATH_SOUND DOUBLE EXPLRADIUS FASTUNIT FIRE
FOGGING FOUNDATION KIND KINO NEDORAZVITOE NOFULLDESTRUCT REFLECT SHOTDELAY SHOTS
SHOWIDLEMINES SPRITES STANDANYWHERE STRIKE_SOUND TIME UNITKIND USERLCEXT VESACCEL VESLA
WALLOBJECT WATERROUND`.

### Building sections (`ADDSECTION`)

Forts and a few houses are built in parts: `UPSTAGE0..4` upgrades (`[STAGEDEPENDENT]`)
add sections to a finished building. 15 buildings declare them (ATCKRE ATCVER ENGKRE
ENGKRE2 FRNKRE FRNKRE2 HOLDO1 HOLKRE INCDO3 INCKRE MAYKRE PORTKRE PUEGOR SPNKRE USAKRE).

```text
ADDSECTION stage sprite life      e.g. ENGKRE.MD: ADDSECTION 0 0 10000
   f0 x0 y0                       state 0, not built:      10 296 513
   f1 x1 y1                       state 1, being built:    11 296 513
   f2 x2 y2                       state 2, done:           12 296 513
   f3 x3 y3                       state 3, destroyed:      13 296 513
```

The parser (0x4f4be1, `%d` × 15, "Not enough or wrong parameters" unless all 15 are read,
"Too large stage index" for `stage ≥ 5`) allocates NM+0x1714: a mask byte of declared
sections, then 5 records of 0x58 bytes. A record holds the sprite set mapped to its pack
(+1), the four `(frame, x, y)` words (+3 + 6·state), the life (+0x1b) and the
`STAGE_FIRES`/`STAGE_SMOKE`/`STAGELOCKPOINTS` data of the stage (+0x1d..). Every object of
a type with sections starts with OB+0xaa = 0x8000 (0x516150); bits `3s..3s+2` hold the
state of section `s`:

| state | set by | drawn frame (ENGKRE) |
|---|---|---|
| 0 not built | creation; a stage-1 section whose building loses life (0x4e94b3) | 10: the wooden part |
| 1 being built | starting the `UPSTAGEs` upgrade | 11: stone walls in scaffolding |
| 2 done | the finished upgrade (0x532737); adds `life` to the maximum life | 12: the stone building |
| 3 destroyed | life ≤ own life + done sections before it (0x4e94f1, explosions at `STAGE_FIRES`); its upgrade is offered again to rebuild it (see `docs/simulation.md`, Building sections) | 13: its ruin |

Drawing (0x5099a8 in the object draw 0x5062a3; zoomed out 0x5fdb93 with offsets `>> 2`):
when OB+0x25 bit 0 (finished, `Ready`?) or OB+0x101 is set and OB+0x58 (dying?) is 0,
after its `#STANDLO` parts, for each declared section the frame of its state is drawn
unless it is `-1` (0xffff), from the section's sprite set, at the picture origin (object
position + `LOCATION`/`SETANMPARAM` offset, like the `#STANDLO` parts; the frames hold
their place in the picture), and sorted (0x49C3E1) at the sort point origin + `(x, y)`.
States above 3 draw nothing. So the state-0 frame is part of the plain building: the
ENGKRE `#STANDLO` frames 4..9 leave the front-left corner open for section 0's frame 10.
Types whose state-0 line is `-1 0 0` (SPNKRE, FRNKRE, ...) show nothing there until the
section is built. INCDO3 has an empty `#STANDLO` (`1 0`): the house is its two sections.
The placement preview (0x4bdeaf) draws the state-0 frames too.

## Nations (`*.NDS`)

A sequence of `[SECTION]`s (same lexical rules). `NATIONS.LST` names the 15 nation files;
`NEUTRAL`, `NEW` (animals, neutral buildings, ships), `FUNNY`, `TESCOCO` hold extra
objects. Object ids carry the nation suffix: `EngPik(EN)`.

| section | grammar | content |
|---|---|---|
| `[MEMBERS]` | `ID NAME` per line | object id → `.MD` `NAME` |
| `[FIXED_PRODUCE]` | `PRODUCER n` + n × `ID x y [KEY]` | build menu: button column/row and hotkey |
| `[ENABLED]` | ids | available from the start |
| `[UPGRADE]` | see below | tech tree |
| `[UPGRADEPLACE]` | `ID n` + n ids | building → upgrades it offers |
| `[UPGRADEENABLE]` | ids | upgrades available from the start |
| `[UPGRADELINKS]` | `UPGRADE n` + n ids | researching unlocks these |
| `[DISABLED_UPGRADES]` | ids | |
| `[AUTOUPGRADE]` | `UPGRADE n` + n ids | applied automatically? |
| `[ACCESSCONTROL]` | `UPGRADE n` + n ids | upgrade required for these objects? |
| `[UNITLOCK]` | `ID n ID` | meaning open (`ShpParom(AC) 3 ShpParom(AC)` in all files) |
| `[CANSETDEST]` | ids | buildings with a rally point |
| `[PRIVATE]` | ids | |
| `[SPECIAL_UPGRADE]`, `[SPECIAL_UNIT]` | `ID` + 8 integers | |
| `[COUNTRY]` | `NATION n` + n entries | list shown in the game; `#INFANTRY`, `#BUILDINGS`, ... are group headers counted in n |
| `[OFFICERS]` | `CMD+ n` + n × (`NAME k #VARIANT*k m ID*m`) | officer, drummer, flag bearer (`NONE` if absent), then formations with size variants (`#SQUARE15 ... #SQUARE196`) and allowed units |
| `[STAGEDEPENDENT]` | `ID n` | |
| `[UPGRADESTAGE]` | ids | |
| `[END]` | | |

Upgrade (one per header; the effect is everything up to the next header):

```text
ID ICON #POSITION x y #LEVEL n MESSAGE #COST n (RES amount)*n #TIME t
    EFFECT
e.g.
EngPik(EN)ATTACK ATAKA1 #POSITION 1 1 #LEVEL 2 EngPik(EN)ATTACK #COST 2 FOOD 200 GOLD 20 #TIME 750
        DAMAGE WEAPONKIND PIKA EngPik(EN) +1
```

`#STAGELEVEL` may replace `#LEVEL` (engine string; unused). Effect categories found
(upgrades): `PROTECTION` 464, `DAMAGE` 475, `BUILD` 204, `SINGLE_INSIDE` 156, `EFFECT_FOOD`
103, `SPEED` 92, `ENABLE` 87, `PERSONALLIFE` 72, `FISHING` 48, `RAZBROS` 37, `ATTPAUSE`
32, `MORELIFE` 30, `COST` 29, `HEALING` 28, `EFFECT_WOOD` 23, `SHIELD` 22, `UPSTAGE0..3`
26, `LIFE` 12, `ATTRANGE` 8, `GEOLOGY` 4, `MOREFARMS(_SINGLE)` 4, `UNCONSTRUCT` 1. The
engine knows further ones (`WEAPON PRECISE INSIDE GETRES SINGLE_ATTPAUSE SHAR
EFFECT_STONE FEAR FEAR0..5 UPSTAGE4`). Effect shapes:

```text
CATEGORY [WEAPONKIND KIND | ATTINDEX n] (GRP n ID*n | ID) VALUE      VALUE = [+-]n[%]
ENABLE UNITS|UPGRADES|UNITS&UPGRADES GRP n ID*n
PROTECTION GRP n KIND*n ID VALUE                 GRP lists weapon kinds here
COST ID GRP n (RES VALUE)*n                      GRP lists resources here
EFFECT_FOOD VALUE, GEOLOGY, UPSTAGEn, UNCONSTRUCT
```

`Upgrade` keeps the effect as tokens with `category()`, `value()` and `targets()`
helpers; the per-category argument semantics are not modelled yet.

`WEAPON.NDS` uses the same container with its own sections (`[MEMBERS]` lines
`NAME #ANIMATION damage radius transparency gravity propagation speed cycle`, documented
by comments in the file, plus `[CHILDWEAPON]`, `[DETONATOR]`, `[TILEWEAPON]`, ...). Only
its weapon ids are interpreted (151); the sections are kept.

### `NATIONS.LST`

```text
15                                                    count
MAY     Maya     MAY.NDS   #TX 143 137   (MA) (MA)    id, name, file, optional #TX a b, suffixes
```

A second block (`19`) lists the *Cossacks* nations (`BAVARIA.NDS` ...), whose files do
not exist.

## Sounds (`SOUNDLIST.TXT`)

```text
TREE1 4                    name, number of files
DANGER | INGROUP n | FORGROUP a b c     optional modifiers (meaning open)
Sound\ResDer1.wav          n paths relative to the game directory
```

Paths repeat to weight the random choice (`DEASE` lists six death cries). `.MD` files
reference sounds by name in `SOUND`, `SELECT_SOUND`, `ORDER_SOUND`, `BORN_SOUND`; all
references of loaded files resolve.

## Map pieces (`*.SMP`)

Binary, little-endian; `"SAMP"` followed by chunks `tag:[u8;4] size:u32 payload` where
`size` includes the 8-byte chunk header, in this order:

| chunk | payload | notes |
|---|---|---|
| `VERT` | `u32 n`, n × `{i16 x, i16 y, u8 texture, u8[3], i16 value}` | terrain vertices of the piece; the three bytes are always 0..2; `value` probably height |
| `SPRT` | `u32 n`, n × `{u16 group, i32 x, i32 y, u16 sprite}` | map sprites; group `AG` (33 730 records) or `ST` (20 435), 20 with 0 |
| `LOCK` | `u32 a, u32 b`, (a+b) × `{i16 x, i16 y}` | two cell lists; in 122 of the 890 archived files the size field is `16 + 2(a+b)` instead of `16 + 4(a+b)` (writer bug, readers must use the counts) |
| `OBJS` | `u32 n`, n × `{i32 x, i32 y, u8 owner, char id[31]}` | placed objects by nation member id (`EngCen(EN)`); owner 0 or 7 |
| `PIX1` | `u32 bytes, u32 n`, n × `{i16 x, i16 y, i32[4], u16 len, u8[len]}` | `bytes` counts from `n` on; `len` is 0 except in 10 files |
| `NRG1` | `u32 bytes, u16[6]`, `bytes-12` bytes | not decoded |
| `ZONE` | size field 0; data to the end of the file | 67 archived pieces; contains script zone names such as `$DZONE-SBLD-MAY01`; not decoded |
| end | `FF FF FF FF` | absent after `ZONE` |

`BUILDINGS\<NAME>.SMP` is the piece named by `PIECE` in a building's `.MD`. Evidence for
the layouts: every one of the 1 642 files is consumed exactly to its last byte.

## Piece links (`*.SMP.LNK`)

```text
"PSMS" | u32 size (= file length - 4) | u16 cells | u16 links | u32 flags (0x800000: 694 files, 0: 106, 0x8000C0: 2)
cells  × {i8 x, i8 y}
links  × {u8 kind, u8 direction 0..7, i16 dx, i16 dy, u32 value, u32 0}
```

Verified on all 802 files (length = 16 + 2·cells + 14·links). The cells look like the
footprint of the piece on the grid; links connect it to neighbours in one of 8
directions.

## Open questions

- Direction order of the 65-direction ship animations (only 9, 16 and `FULLROTATION` were
  traced).
- Whether the map drawing of buildings uses `SETANMPARAM` or `LOCATION` (the placement
  preview uses `LOCATION`, see `terrain.md`; the two agree in the shipped files checked).
- Meanings marked `?` or blank in the keyword table, notably `INFO`, `OPPOSIT`, `PROMAX*`,
  `FEAR*`, `SKILLDAMAGE*`, `ZPOINTS`, `3DBARS`, `ADDSECTION`, `EX_SHOTS*` columns.
- How the engine handles data errors (`SETACTIVEPOINT` for a missing animation, `SOUND` for
  a missing animation, missing `PIECE` files): presumably ignored.
- `.NDS`: `[UNITLOCK]`, `[SPECIAL_UPGRADE]`/`[SPECIAL_UNIT]` integers, `[AUTOUPGRADE]`,
  `#TX` in `NATIONS.LST`, per-category upgrade effect semantics.
- `SOUNDLIST.TXT` modifiers `DANGER`, `INGROUP n`, `FORGROUP a b c`.
- `.SMP`: `VERT` bytes, `PIX1` values, `NRG1`, `ZONE`, the two `LOCK` lists, `SPRT`
  group tags; whether `OBJS.owner` is a player index.
- `.LNK`: `kind`, `value`, `flags`.
- Whether `DESCRIPTION\*.MD` and other localised texts use CP1252 only because this
  installation is German (other localisations may use other code pages).
