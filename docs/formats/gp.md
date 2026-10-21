# `GPAK` sprite packs (`*.GP`)

Reverse engineered from the 1680 `.GP` files of American Conquest 1.46 and confirmed against
the original renderer in `dmcr.exe` (`GP_System::ShowGP` at `0x48BE47`, an exported symbol,
and the routines it calls). `acr gp-check` decodes all 570 738 frames (2 925 011 layers)
without a single failure.

Implementation: `crates/acr-formats/src/gp.rs`, reference rasterizer:
`crates/acr-formats/src/raster.rs`.

## File

| offset | type | field |
|---|---|---|
| 0 | `[u8;4]` | `"GPAK"` (not checked by the original) |
| 4 | u16 | frame count |
| 6 | u16 | flags (0 or 1, meaning unknown) |
| 8 | u32 | dictionary offset |
| 12 | u16 | dictionary length (≤ 4096 palette indices) |
| 14 | u32[n] | frame offsets |

## Frames and chunks

A frame is a linked list of chunks (layers), drawn in order. `ShowGP` stops after 32
chunks; no shipped frame has more. Each chunk starts with a 23-byte header:

| offset | type | field |
|---|---|---|
| 0 | i32 | next chunk, relative to the **frame** start; -1 = last |
| 4 | i16 ×2 | x, y inside the frame |
| 8 | i16 ×2 | width, height |
| 12 | i32 | always -1 |
| 16 | u8 | bits 0–5: encoding; bits 6–7: bits 14–15 of the color data offset |
| 17 | u32 | bits 0–13: color data offset, relative to the chunk start; bits 14–31: unpacked length |
| 21 | u16 | row count, always = height |

`ShowGP` reads the dword at 17 and splits it with `& 0x3FFF` / `>> 14`; it adds `0x4000`
and `0x8000` to the offset for encoding bits 6 and 7, and `0x40000` / `0x80000` to the
unpacked length for encodings `0x2B` / `0x2C`. So the "u16 at 19" of earlier notes is the
upper part of the unpacked length (the byte at 20 is always 0, as no chunk unpacks to
2^24 bytes). The unpacked length is the size of the buffer the compressed encodings
decode into: exactly the pixel count for `0x00`, `0x2A`, `0x2B` and the raw encodings, the
next multiple of 4 above the pixel count for `0x01` (`(n | 3) + 1`, the unpacker works on
whole bytes) and the next multiple of 4 above twice the pixel count for `0x26` (one chunk in
`INTERFACE\STATISTIC_SCREEN.GP` is the only exception).

The last frame of a few files (e.g. `INTERFACE\CURSGO.GP`, `L_MODE\DIMAW.GP`) is an empty
chunk (`0xFF`) cut off after 21 bytes. The original reads only up to byte 20 before skipping
such a chunk, so the decoder accepts it.

Large buildings are split into separate frames (e.g. a back and a front part) so units
can be drawn between them.

## Shape

One record per row; runs are `(skip, len)` where `skip` counts from the end of the
previous run in the same row.

| tag | payload |
|---|---|
| `0x00` | empty row |
| `0x01..=0x7F` = n | n byte pairs `skip, len` |
| `0x80..=0xFF` | `tag & 0x1F` bytes; low nibble = skip, high nibble = len; tag bit `0x20` adds 16 to len, `0x40` adds 16 to skip |

Evidence: every draw routine (e.g. the opaque blitter at `0x4919D2`) tests `0x80` first, uses
the whole tag as pair count otherwise, and masks nibble tags with `0x1F`. So `0x70..=0x7F`
are simply long pair lists and bit `0x10` of a nibble tag is the high bit of its count
(`0x9n`, `0xBn`, `0xDn`, `0xFn` carry 16 + n pairs). With these rules the shape of every
drawn chunk ends exactly one byte before the color data offset, except two chunks in
`INTERFACE\STATISTIC_SCREEN.GP` whose offset points further on.

## Color data

Starts at the offset from the header (the byte between shape and color data is padding,
usually 1–5 or `0xCD`, MSVC's uninitialized-memory fill). The pixels of all runs follow in
order.

| encoding | storage | drawn by the original as (tables: `<n>\*.GRD`) |
|---|---|---|
| `0x00` | dictionary LZ: control bytes MSB first; 0 = literal index, 1 = u16 `offset:12, len-3:4` into the file dictionary | opaque |
| `0x01` | 2 bits per pixel, LSB first | opaque, palette index `0xD0 + 4·player + value` |
| `0x02` | 1 palette index per pixel | `AGEW_TR4.GRD[below][sprite]`, i.e. 25 % sprite |
| `0x03` | 1 palette index per pixel | `AGEW_TR.GRD[below][sprite]`, 50 % |
| `0x04` | 1 palette index per pixel | `AGEW_TR4.GRD[sprite][below]`, 75 % sprite |
| `0x05` | none | `AGEW_1D.GRD` row 12 applied to what is below (shadow) |
| `0x26` | 4 bits per pixel, LSB first; the unpacker doubles each value | `AGEW_1D.GRD[value][below]` |
| `0x27` | 1 byte per pixel, values 1–31 | `AGEW_1D.GRD[value][below]` |
| `0x2A..=0x2C` | self LZ: control bytes LSB first; 1 = u16 `distance:13, len-3:3`, copy from `distance + 1` bytes back | opaque |
| `0x3F` (stored as `0xFF`) | no runs | nothing |

So `0x26` and `0x27` are the same soft shadow with 32 darkness levels, packed as 4 bits
(even levels only) or 8 bits. `0x2A`, `0x2B` (and `0x2C`, unused) are the same self-LZ;
the encoding only extends the unpacked length beyond 18 bits (the 57 `0x2B` chunks are big
building parts of 256–512 K pixels). Encodings 6 and 7 exist in `ShowGP` (raw indices
blended through `ALPHAYR.GRD` / `ALPHAW.GRD`) but no shipped file uses them; every other
value is skipped. Shipped encoding bytes: `00 01 02 03 04 05 26 27 2A 2B 40 42 44 66 6A 6B FF`.

The decompressors (`0x493590` dictionary LZ, `0x493600` self LZ) match the rules above,
including the `distance + 1` and the clamping of the last reference to the unpacked length.

## Open questions

- File flags at offset 6.
- Exact blend strengths of the `*.GRD` tables used by encodings 2–5 and `0x26`/`0x27`.
- Which palette each sprite uses (`0\AGEW_1.PAL` fits units and buildings; interface
  graphics need another one).
