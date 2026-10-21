# `.m3d` maps (`3DMP`, `4DMP`)

Reverse engineered from the map loader and saver in `dmcr.exe` 1.46 (disassembled; addresses
below are virtual addresses in that file) and checked against all 63 maps of the installation
(game folder root, `Missions/**`, `History_battl/**`; the archives contain none).
`acr m3d-check` parses every map and cross-checks the decoded objects against the game data;
`acr m3d-png` renders single layers for visual checks.

Implementation: `crates/acr-formats/src/m3d.rs`.

**Coverage:** 63 of 63 maps parse, and every chunk decoder consumes its payload exactly (the
parser rejects leftovers). That makes the *structure* of every byte known. The *meaning* of
some fields is still open; those are marked "unknown" or "guess" below and listed at the end.

## File

Little-endian. A 12-byte header, then chunks until `ENDM` or the end of the file.

| offset | type | field |
|---|---|---|
| 0 | `[u8;4]` | `"3DMP"`, `"4DMP"` or `"5DMP"` |
| 4 | u32 | terrain vertices per row, `2·(128<<s) + 1` (513 / 1025) |
| 8 | u32 | terrain vertex rows, `2·(128<<s)` (512 / 1024) |

Chunk: `[u8;4]` tag, u32 length, then `length - 4` bytes. The length counts itself but not the
tag.

Evidence: the header reader (0x53FC0B) computes the size shift `s = magic - "2DMP"` and accepts
only 1..=3; the writer (0x53FBB0) stores the two globals that the map-size setup (0x4B7F97)
derives from `s`. The loader reads both u32s and discards them. 60 maps are `3DMP`, the three
random maps (`RN1 *.m3d`) are `4DMP`; no `5DMP` map ships.

The loader (0x545D23) reads a tag and a length, dispatches on the tag and lets the chunk reader
consume its own data; the length is only used to seek past unknown tags. The saver (0x54592B)
writes the chunks in the order of the table below and then `ENDM`. The `UORG` writer declares 4
bytes more than it writes, so its length also covers the closing `ENDM`: every shipped map ends
exactly at the end of `UORG`'s declared payload, whose last 4 bytes are `ENDM`.

All grid sizes follow from `s` (values for `3DMP`, s = 1):

| quantity | formula | `3DMP` |
|---|---|---|
| terrain vertices | `(2·(128<<s)+1) × 2·(128<<s)` | 513 × 512 |
| map side in world pixels | `32 · 2·(128<<s)` | 16 384 |
| playable side (objects) | `32 · (240<<s)` | 15 360 |
| area grid (`TOP1`, `TOPW`, `LOC1`), 64-px cells | `128<<s` | 256 |
| coarse grid (`CAEP`, `RIV1`, `DXIP`), 128-px cells | `64<<s` | 128 |

All units lie inside the playable square (checked); 5 695 sprites (decoration) lie between it
and the map edge. Grids are row-major (`y · width + x`) with x pointing right on the preview,
as the renders show.

## Chunks

| tag | maps | reader | content | status |
|---|---:|---|---|---|
| `VIE1` | 56 | 0x546591 | preview, 400 × 224 palette indices | known |
| `VIEW` | 6 | – | older preview, 292 × 190 palette indices | known, skipped by 1.46 |
| `SURF` | 63 | 0x53FD72 | i16 height per terrain vertex | known |
| `TILE` | 63 | 0x540021 | u8 texture per terrain vertex | structure known |
| `SECT` | 60 | 0x5400F3 | texture transition variants, 6 bytes per cell | known |
| `TRE1` | 58 | 0x5402F0 | sprites (trees, stones, ...), 14 bytes each | known |
| `TREE` | 5 | 0x540501 | the same, 12 bytes each | known |
| `UNI3` | 63 | 0x54163F | units and buildings, 54 bytes each | mostly known |
| `WAL2` | 63 | 0x5427E5 | walls | structure known, always empty |
| `LOC1` | 63 | 0x53FEF2 | passability overrides | known |
| `SEA2` | 63 | 0x542D80 | two water layers | structure known |
| `RESU` | 63 | 0x542E69 | start resources | known |
| `TOP1` | 62 | 0x543558 | land pathfinding areas | mostly known |
| `TOPW` | 62 | 0x54374C | water pathfinding areas | mostly known |
| `GAT1` | 0 | 0x542C35 | gates | structure only |
| `ZON1` | 63 | 0x543C7E | named zones and unit groups for scripts | mostly known |
| `FOR1` | 63 | 0x544CB8 | formations (brigades) | partly known |
| `.DLL` | 63 | 0x5451E2 | mission script DLL path | known |
| `AIIA` | 63 | 0x545270 | 3 × u32 | unknown |
| `PSMS` | 63 | 0x465192 | records, always 0 | structure only |
| `CAEP` | 42 | 0x545869 | coarse id grid, time, per-player bytes | partly known |
| `RIV1` | 63 | 0x545824 | two river layers | partly known |
| `DXIP` | 63 | 0x54560B | per-cell data blobs | structure only |
| `UORG` | 63 | 0x54568F | a u8 and a u16 layer | unknown |

The loader also accepts `UNIT`, `UNI2`, `WALL`, `WAL1`, `FORM`, `TREE` (older versions); only
`TREE` occurs in shipped maps. `VIE1` is not read by the map loader but by the map preview
code; `VIEW` is read by nothing in 1.46.

### `VIE1` / `VIEW` — preview

`VIE1`: 89 600 bytes = 400 × 224 palette indices (the writer allocates 0x15E00 bytes and
renders a 400 × 224 image into it). `VIEW`: 55 480 bytes = 292 × 190, found by trying the
divisors of the size. Both render cleanly with `0\AGEW_1.PAL`; `VIE1` shows the playable area
with the same layout, coast lines and player colors as the terrain render.

### `SURF`, `TILE` — terrain

`(2·(128<<s)+1) · 2·(128<<s)` values (i16 heights, u8 textures), row-major with 513 vertices
per row. The engine's vertex position helpers (0x4463D5, 0x44641E) divide the vertex index by
513: for vertex `(x, y)` the world position is `(32·x, 32·y − (x odd ? 16 : 0))`, so odd
columns are shifted half a step up (triangulated terrain).

Evidence for the orientation: with 513 values per row, objects, the preview and the height
relief line up (towns on flat ground, the coast where the preview shows it); the transposed
reading puts the sea on the wrong side. Heights lie between −62 and 875 except for 85 outliers
(down to −32 383) in `Tekumse_A5`/`Tekumse_I5`.

`TILE` values are 0..=159 (plus one 224): indices of the 64 × 64 textures of `TILES3.BMP`
(136 textures; higher indices read texture 0). How heights and textures are drawn is described
in [terrain.md](terrain.md).

### `SECT`

`(128<<s) · 2·(128<<s) · 6` bytes (786 432 for `3DMP`), read verbatim; absent from the random
maps. One 6-byte record per cell (column pair × row): the variants 0..2 of the texture
transition masks of the cell's four triangles (0x44864B; see [terrain.md](terrain.md)). Without
`SECT` the renderer takes the variants from `RANDOM.LST`.

### `TRE1` / `TREE` — sprites

u32 count, then per object:

| type | field |
|---|---|
| `[u8;2]` | group code |
| i32 | x (world pixels) |
| i32 | y |
| u16 | index into the group's list |
| u16 | `TRE1` only: extra value (0 in all maps; restored only in some engine state) |

Group codes and the sprite group they select (strings passed to the group loader at
0x4CFC5C): `AG` `treelist`, `ST` `stonlist`, `HO` `holelist`, `PS` `meat`, `SA` `Animations`,
`CO` `complex`. Other codes are dropped by the loader. `m3d-check` verifies every index against
the entry count in the first line of `TREELIST.LST`, `STONLIST.LST`, `HOLELIST.LST`, `MEAT.LST`
and `COMPLEX.LST`. `ANIMATIONS.LST` only lists 5 frame ranges (`# first last @NAME ...`); `SA`
indices go up to 1926, which fits two objects per frame (Σ 2·(last − first + 1) = 1938;
`ANIMATIONS.RSR` lists `...F`/`...B` variants). That rule is a guess that passes for all maps.

### `UNI3` — units and buildings

u32 count, then 54 bytes per unit (writer 0x540BCA):

| offset | type | field |
|---|---|---|
| 0 | u8 | nation / player slot; the loader skips ≥ 8 |
| 1 | u16 | type index at save time; ignored on load |
| 3 | i32 | x in 1/16 world pixels |
| 7 | i32 | y in 1/16 world pixels |
| 11 | u16 | hit points at save time; ignored by the 1.46 loader |
| 13 | u16 | construction stage at save time; ignored by the 1.46 loader |
| 15 | i16 ×2 | copied to two globals before the unit is created; unknown |
| 19 | u8 | written to two object fields (0xEA, 0xEB); guess: direction (values spread over 0..=255) |
| 20 | u8 | bit 0, bit 1: two object flag bits, unknown |
| 21 | `[u8;33]` | NUL-terminated unit id, e.g. `EngMel(EN)` |

The loader looks the id up in the nation's unit list and creates the unit at `(x, y)`;
buildings are created fully built. Evidence: all 111 094 ids occur in the `[MEMBERS]` section
of a `*.NDS` file, all positions lie in the playable area, and the units appear where the
preview shows each player's color (nation n gets the same color in both).

### `WAL2` — walls

u32 count, then per wall: u8 kind, u32 segment count, u32 wall type, then 26 bytes per segment
(the writer stores 16 small fields: u16, u16, u8 ×4, u16, u16, u8, u8, u16, u8, u16, u32, u8,
u16). All shipped maps have 0 walls, so the segment fields stay raw.

### `LOC1` — passability overrides

u32 `a`, u32 `b`, then `a` and `b` cells of (u16 x, u16 y) on the 64-pixel area grid. The first
list holds manually *locked* cells, the second manually *unlocked* ones. Evidence: in eng01,
Single1 and battle2, 358/375, 3 842/3 906 and 2 493/2 571 cells of the first list fall outside
every land area in `TOP1`, but only 3/182, 4/163 and 25/357 of the second. A few entries are
0xFFFF.

### `SEA2` — water

u32 width, u32 height (`(240<<s) + 2` = 482), then per row `width` bytes of layer A followed by
`width` bytes of layer B (stored with a row stride of 512 in memory). Cells are 32 pixels.
Layer A is low on land and rises smoothly into seas and rivers; layer B is about 128 on land,
darker in water and bright along shores. Named `depth` and `brightness` in the code — guesses
from the renders.

### `RESU` — resources

8 players × 8 u32, plain (the loader XORs them with 0x20D7D only for the in-memory copy).
Typical values: 50 000, or 0x4876E7FF (presumably "unlimited") in missions. Index order is
`NRES.DAT [RESTYPES]`: 0 wood, 1 gold, 2 stone, 3 food, 4 iron, 5 coal (the random map generator
adds the food bonus of `Data\Start%d.dat` at index 3, see [mapgen.md](../mapgen.md)).

### `TOP1`, `TOPW` — pathfinding areas

u32 area count `n`; `n` records of 85 bytes, each followed by its lists; then two `n × n` u16
matrices; then the area grid `(128<<s)²` of u16.

| record offset | type | field |
|---|---|---|
| 0 | u16 | area center x (area-grid cells) |
| 2 | u16 | area center y |
| 7 | u16 | length of list 1 (u16 entries; always 0 in shipped maps) |
| 9 | u32 | stale pointer |
| 13 | u16 | number of links |
| 15 | u32 | stale pointer |
| 19 | u16 | link capacity (= links when saved) |

Links: `(u16 area, u16 distance)`. Matrix 1 is the next area on the way from area `i` to `j`,
matrix 2 the distance, 0xFFFF = unreachable. Evidence: for area 0 of eng01, link `(3, 14)`
matches `next[0][3] = 3`, `dist[0][3] = 14`. The grid holds the area index of each 64-px cell,
0xFFFF for impassable or water cells (renders show sea and cliffs as holes). `TOPW` uses the
same layout for ships. `MapAutosave.m3d` (an editor autosave) has neither chunk, nor `VIE1`.

### `GAT1` — gates

u32 count, u32 capacity, count × 10 bytes. Absent from all shipped maps.

### `ZON1` — script zones and groups

u32 zone count, u32 group count.

Zone: i32 x, i32 y, i32 radius (world pixels), u32 stale pointer, u8 unknown, u8 name length
(including NUL), name. Group: 49-byte record (stale pointers at 0 and 4, i32 member count at 8,
stale name pointer at 12, then 4 + 4 i32 and a u8 — probably bounding boxes, unknown), u8 name
length, name, then member-count × (i32 x, i32 y) in 1/16 pixels.

The loader rebinds each group member to the nearest unit (0x543BEF). Evidence: every member
position of all 1 147 groups equals the position of a `UNI3` unit. Names match the ones mission
scripts use (`Zattack1`, `Gband4`, ...).

### `FOR1` — formations

u32 count, then per formation: u32 nation, u32 index, u8 ×2, 16 bytes, u8, u16 member count
`m`, `[u8;32]` formation type name (matched against the engine's formation list), u16 ×2,
`m` u16, `m` u16, then `m` × (i32 x, i32 y) positions (x = −1: empty slot). The unknown fields
are copied into the formation object (offsets 0x1C, 0x1D, 0x2F, 0x1F, 0x0C, 0x1A). Evidence:
every non-empty member position of the 233 formations equals a unit position.

### `.DLL`, `AIIA`, `PSMS`

`.DLL`: 200-byte NUL-padded path of the mission script, e.g. `history_battl\Juan\battle2.dll`;
empty in all other maps. `AIIA`: three u32, unknown (0,0,0 in missions; small values such as
4,3,2 in random and history maps — perhaps AI settings). `PSMS`: u32 count, then per record
`[u8;32]` name, u32 `n`, u32 `m`, `n` × (u16, u16), `m` × 14 bytes; the count is 0 everywhere.

### `CAEP` — coarse grid and settings

`(64<<s)²` bytes, u32, `[u8;8]`, `[u8;3]`. The tag reads "PEAC" reversed; the loader also
stores the u32 divided by 60, so it is likely a peace time in seconds (1200 in random maps, 0
in missions). The 8 bytes look like per-player values with 0xFF for empty slots (guess: team).
The grid holds small ids with 0xFF as background; in random maps the values are
`player·16 + 15` and form one region per player island (guess: territories).

### `RIV1` — rivers

Two `(64<<s)²` byte layers. Non-zero only along rivers, with gradients along the course
(guess: flow direction or speed). Also cleared by the `SURF` reader.

### `DXIP`

u32 blob length, then the blob: u32 entry count, per entry u32 coarse-grid cell, u16 length,
data. The writer (0x5EC870) builds each entry from a 32-byte header with 8 u16 counts at 0x10
followed by their sum in bytes. Meaning unknown ("PIXD" reversed; perhaps painted pixel data).

### `UORG` — u8 and u16 layers

`(256<<s) × (128<<s)` u8, then the same number of u16 (writer 0x5EE026). The layers cover the
whole map (x: 32 px per cell, y: 64 px per cell) and show roads, towns and coast lines, but the
u16 values are not RGB565 colors. Unknown.

## Open questions

- `UORG`, `DXIP`, `AIIA`: meaning.
- `UNI3`: the two i16 values at 15, the meaning of the byte at 19 (direction?) and the flag bits.
- `ZON1`: the zone byte and the group bounding-box fields; `FOR1`: the unknown fields.
- `CAEP`: grid ids, per-player bytes and the 3 trailing bytes; `RIV1`: the two layers.
- `RESU`: resource order.
- `SEA2`: confirm the depth/brightness reading against the water renderer.
- `WAL2`, `GAT1`, `PSMS`: no shipped map contains data; fields stay raw.
