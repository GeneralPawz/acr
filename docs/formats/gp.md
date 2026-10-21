# `GPAK` sprite packs (`*.GP`)

Reverse engineered from the 1680 `.GP` files of American Conquest 1.46. Every rule below
is checked by `acr gp-check`, which decodes all 570 738 frames; 688 frames (0.12 %,
mostly smoke/fire effects such as `DIM32.GP`, `DFIRE*.GP`) still fail and are listed by
that command.

Implementation: `crates/acr-formats/src/gp.rs`, reference rasterizer:
`crates/acr-formats/src/raster.rs`.

## File

| offset | type | field |
|---|---|---|
| 0 | `[u8;4]` | `"GPAK"` |
| 4 | u16 | frame count |
| 6 | u16 | flags (0 or 1, meaning unknown) |
| 8 | u32 | dictionary offset |
| 12 | u16 | dictionary length (≤ 4096 palette indices) |
| 14 | u32[n] | frame offsets |

## Frames and chunks

A frame is a linked list of chunks (layers), drawn in order. Each chunk starts with a
23-byte header:

| offset | type | field |
|---|---|---|
| 0 | i32 | next chunk, relative to the **frame** start; -1 = last |
| 4 | i16 ×2 | x, y inside the frame |
| 8 | i16 ×2 | width, height |
| 12 | i32 | always -1 |
| 16 | u8 | encoding |
| 17 | u16 | shape length + 24, low 14 bits only (wraps on big chunks, not needed) |
| 19 | u16 | unknown |
| 21 | u16 | row count, always = height |

Large buildings are split into separate frames (e.g. a back and a front part) so units
can be drawn between them.

## Shape

One record per row; runs are `(skip, len)` where `skip` counts from the end of the
previous run in the same row.

| tag | payload |
|---|---|
| `0x00` | empty row |
| `0x01..=0x6F` = n | n byte pairs `skip, len` |
| `0x8n..=0xFn` | n bytes; low nibble = skip, high nibble = len; tag bit `0x20` adds 16 to len, `0x40` adds 16 to skip |
| `0x70..=0x7F` | unknown, appears only in the failing 0.12 % |

Evidence: with this grammar the shape section ends exactly at the header's shape length
for > 99.9 % of chunks, and the decoded pixel count matches the color data size for each
encoding below.

## Color data

One padding byte (usually 1–5 or `0xCD`, MSVC's uninitialized-memory fill), then the
pixels of all runs in order, then possibly one more padding byte.

| encoding | storage | role (from rendering) |
|---|---|---|
| `0x00` | dictionary LZ: control bytes MSB first; 0 = literal index, 1 = u16 `offset:12, len-3:4` into the file dictionary | main body of units |
| `0x01` | 2 bits per pixel, LSB first; offset into the player's 4-shade ramp at palette `0xD0 + 4·player` | player color |
| `0x02..=0x04` | 1 palette index per pixel | anti-aliased outline pixels; probably 25/50/75 % blend |
| `0x05` | none | shadow mask |
| `0x26`, `0x27` | 4 bits per pixel, LSB first | soft shadow, 16 darkness levels |
| `0x2A`, `0x2B` | self LZ: control bytes LSB first; 1 = u16 `distance:13, len-3:3`, copy from `distance + 1` pixels back | main body of buildings and interface graphics |
| `0xFF` | no runs | empty frame |

A reference whose length runs past the end of the chunk is clamped.

Evidence for the two LZ variants: with these rules 549 114 of 549 245 dictionary-LZ chunks
and all but 82 of 6 251 self-LZ chunks consume their data exactly. The self-LZ distance
rule (`+1`, relative) was chosen visually: absolute offsets and plain distance both produce
speckled buildings, `distance + 1` produces clean images.

## Open questions

- Exact blend strengths of encodings 2–4, 5 and 0x26/0x27 (likely the `*.XLT` tables).
- Difference between 0x26/0x27 and 0x2A/0x2B.
- Shape tags `0x7n` and nibble tags with bit `0x10` set (`0x9n`, `0xBn`, `0xDn`, `0xFn`).
- Header field at offset 19 and the file flags.
- Which palette each sprite uses (`0\AGEW_1.PAL` fits units and buildings; interface
  graphics need another one).
