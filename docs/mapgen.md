# Random map generator

How `dmcr.exe` 1.46 builds a random map, recovered from its disassembly (addresses are
virtual addresses in that file), and how `crates/acr-mapgen` reproduces it.

**Fidelity:** the three random maps the original saved in the game folder
(`RN1 21A6 323454 01148139 4002013.m3d`, `RN1 3281 …`, `RN1 3C0C …`) are reproduced from their
names alone with identical heights (`SURF`), textures (`TILE`), water (`SEA2`), territories
(`CAEP`), sprites (`TRE1`, same order), units (`UNI3`, same order), passability lists
(`LOC1`) and start resources (`RESU`). Only the ground paint layer `UORG` differs in places
where the original reads memory it never initialised (see *Deviations*).
`acr mapgen-check <game-dir>` prints the comparison; the ignored test
`crates/acr-mapgen/tests/original.rs` asserts it (`ACR_GAME_DIR=… cargo test -p acr-mapgen
-- --ignored`). Generating takes 0.1–0.3 s (release build).

## Settings and the map name

The original stores the settings in the map name (parsed by 0x456E14 with
`"%s%x%x%s%d"`), e.g. `RN1 21A6 323454 01148139 4002013`:

| part | meaning |
|---|---|
| `RN1` | size: third character − `'/'` = size shift (`RN0` normal, `RN1` large, `RN2` huge) |
| `21A6` | seed (hex); its low 13 bits start the random number generator |
| `323454` | menu choices, one nibble each from the lowest: landscape, land shape (= players − 2), relief, start resources, minerals, climate |
| `01148139` | nation per colour slot, rightmost = slot 0: `0` none, otherwise the `AI\AI.DAT` index + 1 (base 36) |
| `4002013` | game options, decimal digits from the lowest: 0 start units (`Data\Start%d.dat`, 0 = 18 peasants), 3 peace time (300, 600, 1200, 1800, 2700, 3600, 5400, 7200, 10800, 14400 s), 1, 2, 4 stored in `CAEP`, 5, 6 kept by the engine (unknown) |

Evidence: the three saved maps decode to settings whose regenerated maps match byte for byte,
including the per-slot nations (`AI\AI.DAT`: `SpnKri(SP)`, `EngKri(US)`, … are exactly the
peasants found in `UNI3`), the peace time 1200 in `CAEP` (digit 3 = 2) and the `RESU` bonus.

`acr_mapgen::Settings::from_name` / `name` convert both ways. In the game,
`crates/acr-game/src/random_map.rs` builds the settings from the menu (colour = slot, the
menu's `TEXT\NATIONS.TXT` order mapped to `AI\AI.DAT` order).

## Data files

| file | use |
|---|---|
| `RANDOM.LST` | the engine's random table (8192 values) |
| `Terrain\Res.dat` | menu choices: minerals (3 mine counts), start resources (8 amounts), reliefs, landscape styles (per player count a `.MSP` file) |
| `Terrain\*.MSP` | land shapes: `"COST"`, count, offsets; per shape a 0x1E-byte header (start points, groups) and run lengths of a 256×256 water/land mask |
| `Terrain\cost%d.dat` | per climate: coast textures by distance to the coast (`levels step`, then per level a texture list) |
| `Terrain\terr2.dat` | `climate size relief file`: the `.terr` file of the relief |
| `Pieces\*.terr` | piece lists with attempts and start links: `list count n (kind length)×n` |
| `Pieces\*.lst`, `*.SMP`, `*.SMP.LNK` | map pieces and their link files |
| `Pieces\MINA0.LST` | mines (groups of 4) |
| `Terrain\feature%d%d.dat` | per climate and landscape: forests and animals around start points, `list count distance` |
| `Terrain\fract_set%d.txt` | per climate: 8 inland textures by noise level |
| `Terrain\factures_set%d.dat` | per climate: ground paint variants per texture (`UORG`) |
| `Terrain\Hills%d.dat`, `Mount.dat` | texturing rules; loaded (a random `Hills%d` is picked) but never applied to random maps |
| `AI\AI.DAT` | nations of the menu and their peasant |
| `Data\Start%d.dat` | start units per nation: `#peasant`, 6 extra resources (wood food stone gold iron coal), unit letters, centre, a character grid (32 px per character) |

`TRSS.LST` and `STNN0.LST` (trees and stones near start points) are referenced but not
shipped; the generator skips them like the original.

## Random numbers

`mrand` (0x4526F0): a cursor into `RANDOM.LST`; `cursor += 1`, on passing 0x1FFF a 16-bit
salt grows by 0x3571, the cursor wraps, the result is `(table[cursor] ^ salt) & 0x7FFF`. The
generator sets `cursor = seed & 0x1FFF`, `salt = 0` (0x457207). Draws are mostly
`rand * n >> 15`; the piece chains use `rand % n`. Every draw happens in the original's order,
which is why the maps match.

## Algorithm (0x45CD31)

1. **Land shape.** Shape variant `cursor · count >> 15` (the cursor itself, before any
   draw) of the style's `.MSP`; the run lengths alternate water (0) and land (255).
2. **Blur** `30 >> (s−1)` times: 12/16 centre + 1/16 per neighbour, border copied.
   `sample(x, y)` interpolates the mask bilinearly in world pixels (0x45CB13); minus 0x800
   it is a signed coast distance.
3. **Water** (`SEA2`, every 32 px cell): depth `120 − d/16` (scaled by size), brightness
   `120 − (120−t)²·120/105²` with `t = d/20 + rand&7 + 117`.
4. **Coast textures** from `cost%d.dat` by `d / step`; they get bit 7 = "taken".
5. **Start points:** 20 random swaps of the group order, then per group one of its points
   (the engine draws with the count of group `i` but searches group `order[i]`).
6. **Territories** on the 128 px grid: start cells, then repeated growth into free land
   (right, left, down, up neighbour), value `slot·16 + min(15, dist/4)` with the engine's
   distance `(max + |dx| + |dy|) / 2`.
7. **Relief rules:** a random `Hills%d.dat` is loaded (draws until `rand&7` names an
   existing file) but not applied.
8. **Start areas** (0x45F85C): `players` of the start points are chosen at random; each
   chosen point reserves a 750 px radius and gets the next nation's start units (from
   `Data\Start%d.dat`, or 18 peasants at random offsets).
9. **Mines** per start area from `MINA0.LST` in distance bands of the territory (pieces need
   all their vertices free), plus extra mines by the minerals setting; **features**
   (forests, animals) from `feature%d%d.dat`; then mines on unowned land.
10. **Piece chains** (0x466C4A, `.terr`): inland textures from a 1024² midpoint-displacement
    noise (`fract_set`), then for every `.terr` line `count << (s−1)` chains: a random cell
    of a 256 px grid and a random `(kind, length)`; each step draws a direction allowed by
    the current link mask, a piece with a link of that kind and direction, and continues
    through the piece's other link. Footprints may not overlap; the last 7 pieces are not
    repeated; chains shorter than 1/4/6 pieces (by length) are dropped; links of kind > 63
    spawn chains of their own. Accepted pieces are pasted grouped by file, keeping the
    higher ground (`LOW_` pieces: the lower); a sprite is placed only on ground its own piece
    won, and sprites a later piece buried are removed. Locked cells on flat ground are
    dropped from the passability list.
11. **Peace borders:** territories are spread over the water; along borders the original
    draws border marks (land: holes that never reach the saved map; water: brightened
    `SEA2` circles).
12. **Elevation:** land rises with the coast distance (`d²/2¹⁴·29·64/5800`, at most 64);
    texture bit 7 is cleared; the ground paint (`UORG`) is filled from `factures_set`.

Pieces (`.SMP`): `VERT` vertices add (direct paste) or replace (chains) heights and set
textures, `SPRT` sprites, `LOCK` passability lists, `OBJS` units (owner 7 = nature and
neutral villages), `NRG1` ground paint with a depth buffer; `PIX1` (decals) and `ZONE`
(script zones) leave no trace in saved random maps.

## Sea battle (ACR addition, #408)

The original has no map without land: its six landscape styles (`Terrain\Res.dat`, section 4:
`LT_Land` to `LT_Continent`, each with a `.MSP` shape per player count) all draw coasts, and
`Res.dat` has no water-only entry. ACR adds landscape 6 (the menu's "Meer" after the six;
the map name carries the nibble 6, which no original map has). `acr_mapgen::generate` takes
that branch before the generator proper (`sea.rs`):

- `SEA2` depth 200 and brightness 0x60 on every cell (`open_water`; ships float above 130,
  land units stop above 128, docs/water.md), heights 0, texture 0, no sprites, no `LOC1`
  (the simulation's lock maps come from the depth), no territories or peace time: the
  fleets are at war from tick 0;
- every player (a slot with a nation) gets `ShpFregat`, `ShpPinta`, `ShpPinta` of its nation (the European ones; the native nations three `ShpKanoe3`; ids carry the `(XX)` of `AI\AI.DAT`'s peasant): two players
  at the west and east end (2/5 of the map side from the middle, facing each other), more
  on a circle around the middle; start resources from `Res.dat` as usual.

The driver's blank field has the same water as ground `sea` (docs/testing.md).

## Deviations

- `UORG` rows 1–2 use two stack variables the original never initialises
  (0x468D80); they are 0 here.
- The original never resets `UORG` and the paint depth buffer between generations, so a map
  generated after another one keeps some of the earlier map's paint (`RN1 3C0C` contains
  50 628 cells of `RN1 3281`, generated before it in the same session). Here every map starts
  from the fresh state (`UORG` 0x81/0).
- `AIIA` is not produced (meaning unknown; random maps of the original carry small values).
- `SECT` is not produced, like in the original's random maps.
- Unit hit points, directions and the `TRE1` extra field are written as 0.
- The texture count comes from `TILES3.BMP` (file size / 4096, as 0x446F4C).
