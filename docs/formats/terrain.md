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
0x7AFB28 = 0x7ABB28 + 64·256, loaded by 0x483CA2). Rows 0..64 are the identity, row `64 + k`
is about `k/32` darker and rows from 95 on are black; the terrain's levels 1..13 darken by up
to about 40 %. The fog of war uses the whole table (see `docs/ui.md`).

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

## Zoomed-out view

The zoomed-out view (key L, see `docs/ui.md`) draws the ground with its own renderer,
`MiniMap4X.cpp`. Implementation: `vs_zoom`/`fs_zoom` in `terrain.wgsl`, `ZOOM_TRIANGLES` and
`zoom_layers` in `crates/acr-render/src/terrain.rs`.

**Cache.** The picture is kept in blocks of 32 × 32 screen pixels (2 column pairs × 8 rows),
one 1024-byte bitmap each (block pointers `0x28eed58`, block → slot `0x28eed5c`, slot →
block `0x28eed48`/`0x28eed4c`, slots handed out by 0x5fb3d1). Every frame 0x5fb93e first
drops the blocks in the dirty rectangle (0x5ff131), then 0x5ffbd6 renders the missing
blocks of the view, joined into runs, by 0x5fdddd, and 0x5fb75a copies each visible block to
the screen. The dirty rectangle (`0x678300..0x67830c`, in column pairs and rows) grows by
0x5ff043 (a height changed, 0x44a29c) and 0x5fef71 (water depth, 0x53a31b; the editor's
ground paint, 0x5ef13c). Nothing else invalidates it: objects baked into the picture (below)
stay as they were until their block is evicted. ACR draws the view every frame instead.

**Ground (0x5fdddd).** The block range is rendered into a buffer with twice the rows and 256
spare rows on top (`0x28ead10`), at a quarter of the world size: vertex `(col, row)` sits at
`(8·col, 8·row − (col odd ? 4 : 0) − max(H, 0) >> 1)`, i.e. world / 4 in both axes before the
rows are halved. Negative heights count as 0, the random lift is not used. Row by row
(including 40 rows below the range, for heights), every cell draws four triangles in this
order:

| call | shape | P1, P2 (vertical edge) | P3 (apex) | light of P2 |
|---|---|---|---|---|
| 0x5fa3cc | ▷ | v, v+W | v+W+1 | own |
| 0x5fa3cc | ▷ | v+1, v+W+1 | v+2 | own |
| 0x5fa936 | ◁ | v+1, v+W+1 | v | own |
| 0x5fa936 | ◁ | v+2, v+W+2 | v+W+1 | `v+W+1` (the call passes the wrong local) |

A triangle covers 8 columns by 8 rows (0x5f7ff0 ▷, 0x5f8af7 ◁, walking the columns up and
down); each pixel is moved up by the height interpolated from the corners (8.8 fixed point).
Later triangles overwrite earlier ones. Per triangle with textures `a, b, c` at P1, P2, P3:

1. Base: the smallest index of the three (`a` if `a ≤ b, c`; else `b` if `b ≤ c, a`; else
   `c`), written opaque.
2. Overlays: the other distinct textures, in the order `b, c` / `a, c` / `b, a` for the
   three cases. Each has a weight per corner, `255 − r` at the corners with that texture
   and `r` at the others, where `r = RANDOM.LST[vertex & 0x1fff] & 0x7f` (0x5fa44a). The
   interpolated weight `w` selects, per pixel: `w > 190` the texel, `> 153` `TR4[texel][old]`,
   `> 102` `TR[old][texel]`, `> 65` `TR4[old][texel]`, `> 51` the value left from the
   previous pixel (ACR uses `TR4[old][texel]`), else the old pixel.
3. Light (0x5f960b, ◁: 0x5f9cea): the vertex light levels (0x446ac0, as in the normal
   view) are interpolated and every pixel becomes `AGEW_1DR[63 + level + bit][pixel]`,
   `bit` = `RANDOM.LST[counter] & 0x100` with a running counter. Row 63 is the identity.

Textures are not scaled: each pass reads an 8 × 8 patch of its 64 × 64 texture, one texel
per buffer pixel, at a random offset `(rand() & 15, rand() & 15)` per pass and triangle
(0x5f80e2). ACR takes the offsets from a hash of the triangle, so the picture is stable.

Then the buffer is reduced to the block height (0x5fe62f): output row `r` is buffer row
`2r` (after the 256 spare rows); a pixel still 0 takes the first non-zero pixel up to 19
rows further down (holes on steep slopes).

**Water** (0x5f2a89, per 8 × 8 output block = one `SEA2` cell): as in the normal view (corner
depths at grid rows `2j` and `2j + 2`, skipped if the first three are negative, bilinear,
`WCOST[pixel][(a >> 8) & 0xF0 + 1]`) but without the random noise. Before it, 0x5f2c34 draws
the `UORG` ground paint layer, the twin of 0x5ee4a1; ACR draws that layer in neither view.

**Objects** (0x5fea06): the `STONLIST` and `HOLELIST` objects of the block are drawn into
the picture with `L_Mode\Stones` and `L_Mode\Holes` at `(x >> 2 − DX >> 2, y >> 3 − z >> 2 −
DY >> 2)`, then the `TREELIST` objects with `L_Mode\Der`, sorted by `y >> 3` (0x5fafc9,
flushed by 0x5fb0be). Units, buildings and everything else come later (0x5fbac1), so they
always cover trees. ACR draws these three groups first in the zoomed view
(`scene::draw_order`).

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
here a line part sorts at `y/2 + LOCATION.y + (y1 + y2)/2`, which keeps trees and units behind
a building behind it.

**Building pictures** start at the building's position plus `LOCATION x y` (the corner of the
picture rectangle `LOCATION x y w h`; `USERLC` offsets of buildings are 0). The placement
preview 0x4bdd8e draws the frames at `RealX/16 + LOCATION.x`, `RealY/32 + LOCATION.y −
height`, and the lock cells (from `(RealX + 16a) >> 8`) and `RECTANGLE` are centred on the
same point.

**Idle frames.** Map units (`UNI3` id `EngM18(EN)`) resolve to the `.MD` with `NAME EngM18`
(the parenthesis is the nation). Buildings show all parts of `#STANDLO`, units the first frame
of `#STAND`. The map's direction byte (0..255) picks a stored direction and the mirroring as
the drawing code does ([`docs/simulation.md`](../simulation.md#directions-and-sprites)); frame
`f` of stored direction `k` is `f · dirs + k`.

## Open questions

- Objects: `ANIMATIONS.LST` (`SA`, river animations of
  `POTOK.GP`) has its own format and is not drawn yet; `MEAT.LST` (`PS`) neither. `COMPLEX.LST`
  (fields, `KUK`/`PUK`, animations of `WEAPON.ADS`) is drawn for the simulation's fields only
  (docs/rendering-3d.md "Fields"), not for the map's own `CO` objects. Line
  sorting of buildings is approximated.

- Waves and surf: `WAVE*.DAT`, `WAVE.GRD`, `PRIBOY.GP` and `Water\voda.bmp` (not shipped) are
  loaded around 0x5EF413; their animation is not reproduced yet.
- `UORG` (u8 brightness + u16 texture·64 per 32 × 64-pixel cell) and `GROUND\TEX*.BMP`: a
  paint layer of the editor (0x5EE0F0); whether the game draws it is not known yet.
- `#MULTI` lines of `TEXTURES.LST` (four variants per texture, 0x6DD398) are parsed by the
  engine but their use in the renderer was not found.
- The exact noise of the water edge (the engine indexes `RANDOM.LST` by the interpolated
  depth plus the screen address); a hash is used instead.
- The light dither restarts at each span in the original; it is screen-aligned here.
- Zoomed-out view: the ◁ triangles' texel walk (texture patch mirrored or not) is not traced;
  the patch is mapped left to right. Overlapping triangle edges are rasterised by the GPU's
  rules instead of the original's fixed column pattern.
