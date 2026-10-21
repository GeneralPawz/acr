# Terrain rendering

How `dmcr.exe` 1.46 draws the ground of a map, recovered from its disassembly (addresses are
virtual addresses in that file) and checked by rendering every map with the rules below
(`acr-viewer <game-dir> --map NAME`) against the map previews. The map data itself is
described in [m3d.md](m3d.md).

Implementation: `crates/acr-formats/src/terrain.rs` (files), `crates/acr-render/src/terrain.rs`
and `terrain.wgsl` (rules).

The original is a software renderer in 8-bit palette mode. Every step below works on palette
indices; colours only appear when the frame is shown. The terrain uses the palette set 0
(`0\AGEW_1.PAL` and the `0\` tables): with it the textures look right, the other sets give
false colours.

## Mesh and projection

The heights (`SURF`, i16) and textures (`TILE`, u8) are given per vertex, 513 (1025) vertices
per row. Vertex `(col, row)` sits at world pixel `(32·col, 32·row − (col odd ? 16 : 0))`
(0x4463D5, 0x44641E).

A cell is a column pair × a row; its four triangles are drawn by 0x44864B (argument: cell ·
4 + triangle), with `v` = the cell's top-left vertex and `W` = vertices per row:

| triangle | vertices P, Q, R | shape in texture space |
|---|---|---|
| 0 | v, v+W, v+W+1 | ▷ |
| 1 | v, v+1, v+W+1 | ◁ |
| 2 | v+2, v+1, v+W+1 | ▷ |
| 3 | v+2, v+W+2, v+W+1 | ◁ |

Screen position of a vertex: `x = world x`, `y = world y / 2 − height − lift` (0x44639A halves
the world y; 0x449FC0 computes `ConvY(y) / 4 − height` with `ConvY(y) = 2y`, 0x4BAA40). The
height is in screen pixels. `lift` is a per-vertex random offset of 0..7 pixels:
`RANDOM.LST[v % 8133] & 7` (0x49BEDF), 0 for textures flagged `P` in `TEXTURES.LST`; it is
active whenever terrain is drawn (flag 0x6DD7BC, set at terrain init 0x446244).

Objects stand on the ground without the lift: `GetHeight(x, y)` (0x4D0530, 170 callers)
clamps `x` to `0..=playable`, `y` to `32..=playable`, picks the triangle under the point and
evaluates the plane through its three vertices with integer arithmetic (ported exactly as
`TerrainData::height_at`).

## Textures

`TILES3.BMP` (8-bit, 256 × 2176) holds 136 textures of 64 × 64 pixels (0x446EA0). The engine
skips the 0x436-byte header and keeps the rows in file order (bottom-up); texture `t` starts at
byte `(t & 3)·64 + (t >> 2)·16384` (0x4960E3), i.e. four textures per 64 rows. Indices
≥ 136 (file size / 4096) read texture 0. `TILE` values are these indices. `DRAWTEXTURES\
TEX*.BMP` are brush previews of the editor (0x4A416E), `GROUND\TEX*.BMP` belong to the `UORG`
paint layer (0x5EE0F0), not to the terrain.

Textures are mapped in a rotated texture space (0x448B9E):

```
u = −x/2 − y        v = (3x/2 − y) / 2        (world x, y; integer, arithmetic shifts)
```

sampled at `(u & 63, v & 63)`. In this space every triangle has one vertical edge of length 32
and its apex 32 pixels to the side (▷ or ◁ above), which is the shape of the transition masks.

### Blended triangles

A triangle whose three vertices carry the same texture is textured directly. Otherwise the
engine composes a 32 × 32 triangle in a buffer (0x49610B for ▷, 0x49654D for ◁) and maps that
buffer onto the triangle with fixed buffer coordinates per vertex:

| triangle | P | Q | R |
|---|---|---|---|
| 0 | (30, 15) | (0, 0) | (0, 31) |
| 1 | (30, 1) | (30, 30) | (1, 14) |
| 2 | (1, 30) | (30, 14) | (1, 1) |
| 3 | (30, 30) | (1, 14) | (30, 1) |

Buffer pixel `(x, y)` samples a texture at `origin + (x, y)` where `origin` is the texture-space
position of Q; for ◁ triangles the base texture is sampled 16 rows and the overlays 31 rows
higher (0x495F5A; 0x49654D passes `v0 − 0x1F`).

The composer gets the three textures `a, b, c` (triangle 0: Q, P, R; 1: P, R, Q; 2 and 3:
R, Q, P) and a variant 0..2 per vertex `va, vb, vc` (bytes of the cell's 6-byte `SECT` record:
triangle 0: 0, 2, 1; 1: 2, 3, 1; 2: 3, 4, 5; 3: 4, 6, 5 where byte 6 is the next cell's byte 0;
without `SECT`, `RANDOM.LST[(cell·6 + k) & 0x1FFF] % 3`). The mask of vertex X depends on its
own and the previous vertex's variant (order a → b → c → a): `m(X) = (2 − v_prev)·3 + v_X`.

| case | base | overlays (mask index, ▷) | overlays (◁) |
|---|---|---|---|
| a = b | a | c: `27 + m(c)` if c < b, else `18 + m(c)` | `81` / `72` |
| b = c | c | a: `45 + m(a)` if a < c, else `36 + m(a)` | `99` / `90` |
| a = c | c | b: `9 + m(b)` if b < c, else `0 + m(b)` | `63` / `54` |
| a smallest | a | b: `MS.MSK[m(b)]` (hard), c: `18 + m(c)` | b: `54`, c: `72` |
| b smallest | b | a: `36 + m(a)`, c: `18 + m(c)` | a: `90`, c: `72` |
| c smallest | c | a: `36 + m(a)`, b: `0 + m(b)` | a: `90`, b: `54` |

An overlay applies four masks with the same index, in this order (0x495DC1):

| file | operation on buffer pixel `old` with texture pixel `new` | blend table |
|---|---|---|
| `MS_A.MSK` | `new` (0x495C00) | – |
| `MS_B.MSK` | `TR4[new][old]` ≈ ¾ new + ¼ old (0x495CD5) | `AGEW_TR4.GRD` (0x73BE10) |
| `MS_C.MSK` | `TR[new][old]` ≈ ½ + ½ (0x495C5F) | `AGEW_TR.GRD` (0x74BE10) |
| `MS_D.MSK` | `TR4[old][new]` ≈ ¼ new + ¾ old (0x495D4B) | `AGEW_TR4.GRD` |

The hard `MS.MSK` overlay only copies. The `.MSK` files are RLC tables without pixels: `"RLC"`
+ 1 byte, u32 count (108), u32 offsets; per item a 4-byte header (byte 2: rows), then per row a
run count and `(skip, length)` pairs. `LoadRLC` (0x482739) prefixes the file with its size, so
the blitters' `table + 4·i + 0xC` is item `i`. Items 0..53 are ▷ shapes, 54..107 ◁ shapes. The
blend weights were measured with `BlendTable::mix_weight`: 0.75 and 0.50.

## Light

Per vertex, from the heights of six neighbours (0x446AC0). The neighbours are chosen by the
parity of the vertex *index* (not the column; with 513 vertices per row this alternates by row):

```
odd  v: r1 = H[v+W+1], r2 = H[v+1],   l1 = H[v−1],   l2 = H[v+W−1]
even v: r1 = H[v+1],   r2 = H[v−W+1], l1 = H[v−W−1], l2 = H[v−1]
dx = (r1 − l1 + r2 − l2) >> 1      dy = H[v−W] − H[v+W]      dz = 64
q  = (dx·Lx + dy·Ly + dz·Lz) / (trunc(√(dx²+dy²+dz²)) + 1)
b  = clamp((q >> 4) + 16, 1, 31);  b = clamp(32 − CURVE[32 − b], 1, 31);  light = 32 − b
```

`L` is the light vector scaled to length 256 by `SetLight` (0x4693C0); the game starts with
`SetLight(−15, 14, 30)` (0x46993B), i.e. `L = (−106, 99, 213)`. `CURVE` is the table at
0x644470 (`0 1 1 3 5 7 9 11 12 … 12 13 … 13 0`). Flat ground gets level 3, the range in
practice is 1..13. Textures flagged `B` get level 0. Out-of-map heights read as 0.

The span filler (0x445368) interpolates the light linearly over the triangle (16.16 fixed
point) and adds ½ on every other pixel and ¼ on odd rows (an ordered dither), then replaces the
pixel by `SHADE[level][pixel]` with `SHADE` = rows 64..95 of `0\AGEW_1DR.GRD` (table base
0x7AFB28 = 0x7ABB28 + 64·256, loaded by 0x483CA2). Row 64 is the identity, higher rows darken
up to about 45 % at row 92.

## Water

Water is drawn over the finished ground from the `SEA2` depth layer (0x5EF5A6). For every
32 × 32 block of screen pixels (32 world pixels wide, 64 high) it reads the depth `d − 127` at
grid cells `(x, 2j)`, `(x + 1, 2j)`, `(x, 2j + 2)`, `(x + 1, 2j + 2)` — the odd grid rows are not
used — and skips the block if the first three are negative. Per pixel it interpolates the depth
bilinearly (·1024), clamps it to 0..65000, adds `RANDOM.LST[…] >> 2` as noise (up to 16383),
clamps again and replaces the pixel by `WCOST[pixel][(a >> 8) & 0xF0 + 1]` (`WCOST.GRD`, root
folder, table at 0x765E18). The water is flat: it ignores the terrain height.

The minimap (0x44C230) uses the same light and textures (`0x6D4658[tile]`, one colour per
texture) and colours water cells (`depth > 128`) with palette entries 0xB3..0xB7 by the
`SEA2` brightness layer.

## Objects on the terrain

Implementation: `crates/acr-render/src/scene.rs` (`Scene`, `MapObjects`), lists in
`crates/acr-formats/src/sprite_list.rs`.

**Sprite lists.** `TRE1` objects index the lists `TREELIST.LST`, `STONLIST.LST`,
`HOLELIST.LST`, ... (loader 0x5011AF). The first line names the sprite pack and the count
(`TREES 472` → `TREES.GP`), entry `i` is `ID DX DY RADIUS` for frame `i`.

**Placement** (`ShowSprites`, 0x4D0930). The object record (35 bytes at 0x1100D6C) holds x, y,
the height `z` taken when the object is placed, the group and the index. The frame is drawn at
screen `(x − DX, y/2 − z − DY)`, relative to the camera. Units and buildings use their `.MD`
sprite set offset (`USERLC … dx dy`) instead of `DX DY` and stand at `GetHeight(x, y)`; the
random vertex lift of the terrain is not applied to objects.

**Draw order** (z-buffer 0x49C249 / 0x49C3E1, flushed in 0x49D828). After the terrain:

1. "Flat" items, added by 0x49C249: all list sprites except trees (stones, holes, …), drawn
   in the order they were added.
2. Sorted items, added by 0x49C3E1 with a sort point `(X, Y)`: bucketed by `Y + 256`
   (2000 rows, 64 items per row) and drawn row by row from the top. Trees use `Y = y/2 −
   camera`, i.e. the ground position **without** the height, so a tree on a hill sorts like
   one in the valley at the same map row.

Buildings declare per animation part how it sorts (`LINESORT #STANDLO GROUND LINE x1 y1 x2 y2
…`): `GROUND` parts go to the flat list, `LINE` parts sort against a line in frame
coordinates. The flush compares items with lines specially (flags 0x2000 in the bucket loop);
here a line part sorts at `y/2 + (y1 + y2)/2`, which keeps trees and units behind a building
behind it.

**Idle frames.** Map units (`UNI3` id `EngM18(EN)`) resolve to the `.MD` with `NAME EngM18`
(the parenthesis is the nation). Buildings show all parts of `#STANDLO`, units the first frame
of `#STAND`. Frame `f` of direction `d` is `d · N/dirs + f`; the map's direction byte (0..255)
is mapped to the 9 stored directions as a half circle mirrored to the other half (a guess:
mirrored units are not flipped yet).

## Open questions

- Objects: `COMPLEX.LST` (fields, `KUK`/`PUK`) and `ANIMATIONS.LST` (`SA`, river animations of
  `POTOK.GP`) have their own formats and are not drawn yet; `MEAT.LST` (`PS`) neither. Line
  sorting of buildings is approximated, unit directions are a guess.

- Waves and surf: `WAVE*.DAT`, `WAVE.GRD`, `PRIBOY.GP` and `Water\voda.bmp` (not shipped) are
  loaded around 0x5EF413; their animation is not reproduced yet.
- `UORG` (u8 brightness + u16 texture·64 per 32 × 64-pixel cell) and `GROUND\TEX*.BMP`: a
  paint layer of the editor (0x5EE0F0); whether the game draws it is not known yet.
- `#MULTI` lines of `TEXTURES.LST` (four variants per texture, 0x6DD398) are parsed by the
  engine but their use in the renderer was not found.
- The exact noise of the water edge (the engine indexes `RANDOM.LST` by the interpolated
  depth plus the screen address); a hash is used instead.
- The light dither restarts at each span in the original; it is screen-aligned here.
