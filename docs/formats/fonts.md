# Fonts (`RLCFont`, `FONT*.GP/.GPI`, `*.RLC`)

Text is drawn by `ShowString` (`0x483382`) with an `RLCFont`. Almost all fonts are sprite
packs (`FONTG60.GP`, `FONTM12..40.GP`, `MAINFONT.GP`, `ROM10.GP`) with one frame per byte of
the (CP1252) string; a few old fonts are `.RLC` picture lists. Implemented in
`acr_formats::font` and `acr_formats::rlc`; `acr font-png` renders a string for checking.

## `RLCFont` (24 bytes)

| offset | type | meaning | evidence |
|---|---|---|---|
| 0 | i32 | first item (RLC fonts; `RegisterRLCFont` sets 0) | `0x483030` |
| 4 | i32 | last item | `0x483030` |
| 8 | u8 | colour mode << 4 (`SetColorTable(n)`, `SetWhiteColor` = 1, `SetRedColor` = 2, `SetBlackColor` = 0) | `0x48382D`, `0x483809`, `0x4837F7`, `0x48381B` |
| 12 | i32 | shadow sprite pack, -1 = none | ctor `0x480740`, `SetShadowGP` `0x401730` |
| 16 | i16 | shadow dx | `SetShadowGP` |
| 18 | i16 | shadow dy | `SetShadowGP` (`SetStdShadow` `0x401700`: same pack, 1, 1; unused) |
| 20 | i32 | sprite pack index if < 0x1000, else pointer to the loaded RLC file | `SetGPIndex` `0x48077D`, ctor `0x48379C` |

## Drawing a string (`ShowString`)

Sprite font (pack index < 0x1000):

1. The font's colour mode is stored in the pack's flag byte: `GPS.ImageFlags[gp] =
   (flags & 7) | (mode << 4)` (`0x4833A9`).
2. For every byte up to the terminating NUL (`ShowCharUNICODE`, `0x483119`):
   - if the font has a shadow pack: `ImageFlags[shadow] = 1` (colour mode 0) and
     `ShowGP(x + dx, y + dy, shadow, byte)`;
   - `ShowGP(x, y, gp, byte)`;
   - `x += GetRLCWidthUNICODE(byte)`.
   The shadow of a glyph is drawn after the previous glyph, so a wide shadow (`FontG60s`)
   darkens the right edge of the preceding letter.
3. `ShowGP` (`0x48BE47`) draws frame `byte` with its top-left at `(x - shift[byte], y)`
   (`.GPI` shift, `0x48BFB6`). Font frames only contain chunks of encoding `0x26` (4-bit
   darkness levels, stored halved) or `0x27` (8-bit levels; `ROM10`), plus empty `0x3F`
   chunks. For these encodings the colour mode `ImageFlags[gp] >> 4` selects the lookup table
   (`0x48C4F8` for `0x26`, `0x48C78A` for `0x27`): the pixel below, `c`, becomes
   `table[level * 256 + c]`.

RLC font: `ShowRLCItem(x, y, font, first + byte, 0)` and `x += width(first + byte)`
(`0x483458`); pixels are copied opaquely.

### Advance, height

- `GetRLCWidthUNICODE` (`0x482E7E`) / `0x482D86`: for sprite fonts a space (0x20) is as wide as
  the frame of `'c'`; otherwise `GP_System::GetGPWidth(gp, byte)` (`0x486529`): the `.GPI`
  advance byte if the pack has a `.GPI`, else `Lx` of the frame; 0 if the byte is past the
  last frame. RLC fonts: the item's width (i16 at item offset 0), 0 past the last item.
- `GetRLCStrWidth` (`0x4834DC`) sums the advances over `strlen(text)` bytes.
- `GetRLCHeight(font, c)` (`0x482FD7`): `GetGPHeight` = `Ly` of the frame (sprite fonts) or
  the item height (RLC). UI code almost always measures `'W'` (11 calls) or `'y'` (6).
- `GP_Header::GetLx` / `GetLy` (`0x486BB5` / `0x486C2D`): the maximum of `x + w` / `y + h`
  (and 0) over all chunk headers of the frame, empty chunks included.
- `y` is the top of the frame box; there is no baseline. Glyph pixels start at the chunk
  offsets inside the frame (e.g. `FONTG60` capitals at y = 8, lower case at y = 17).

### `0x7F` escapes (icons)

`0x7F` followed by four hex digits (`0x482DFE`, invalid digits count as 0) is one icon:
`UNIFONTS::LoadFonts` (`0x490F58`) reads `UNICODE.DAT`, which maps code ranges of the packs
`MainFont` and `rom10` to frames of `Interface\ISymb2`, `signs_0`, `signs_2`
(fields `dx dy first count frame colored pack`, entries of 25 bytes). For every other font the
escape has width 0 and draws nothing; `acr_formats::font` implements only that case. RLC fonts
measure the escape as 0 wide in `GetRLCStrWidth` but `ShowString` draws item 0x7F and the
four digits like normal bytes (implemented as such).

## `.GPI` (`GP_System::LoadGP`, `0x487147`)

Optional, next to the `.GP` (`"%s.gpi"`, `0x48742D`). `n` = frame count of the pack:

| offset | type | meaning |
|---|---|---|
| 0 | i8[n] | shift: the frame is drawn this many pixels further left (`GetGPShift`, `0x48659D`) |
| n | i8[n] | advance width (`GetGPWidth`) |

The shadow packs (`FONT*S.GPI`) have the same shifts as their fonts.

## Colour tables

`LoadFog(set)` (`0x483CA2`, called by the screens with the palette set they use; the main
menu calls it with 1 and then loads `1\agew_1.pal`) loads the tables from `<set>\`. 32 rows
of 256 bytes each:

| mode | file | address | notes |
|---|---|---|---|
| 0 (and 7..15) | `AGEW_1D.GRD` | `0x739E10` | darkening; also all shadows |
| 1 | `AGEW_1BR.GRD` | `0x7A3B28` | row 0 forced to identity (`0x483C61`) |
| 2 | `AGEW_1R.GRD` | `0x737A10` | row 0 forced to identity |
| 3 | `ORANGE.GRD` | `0x797F28` | row 0 forced to identity; `LoadOptionalTable(3, ..)` |
| 4 | `OPT2.GRD` | `0x79BF28` | `LoadOptionalTable(4, ..)` |
| 5 | `OPT3.GRD` | `0x79FF28` | `LoadOptionalTable(5, ..)` |
| 6 | `OPT4.GRD` | `0x799F28` | `LoadOptionalTable(6, ..)` |

A missing file (set 2 has no `OPT2..4.GRD`) leaves the previously loaded table in place.
`LoadOptionalTable(n, file)` (`0x4838EC`) replaces tables 3..6 for some screens:
`2\dark.grd` as 5 (`0x5C4601`), `1\orange.grd` as 3 and `1\white.grd` as 4 (`0x5CE0FD`,
`0x5CE12F`), `2\orange.grd` as 3 (`0x5DFEA5`, `0x5E06A5`).

Row 31 of every font table is a single colour (the text colour) and row `r` is that colour
blended over the background with an opacity that grows roughly linearly with `r` (fit
residual 5–15 per channel). `font::ColorTable` keeps the table for exact 8-bit use
(`remap`) and derives one colour plus a least-squares opacity per row for RGBA rendering.
Text colours of row 31 per set:

| mode | set 0 | set 1 (menus) | set 2 |
|---|---|---|---|
| 0 `AGEW_1D` | black | black | black |
| 1 `AGEW_1BR` | white | white | (216, 200, 170) |
| 2 `AGEW_1R` | dark red (124, 0, 0) | white | dark red |
| 3 `ORANGE` | (243, 234, 180) | (255, 212, 170) | (212, 169, 121) |
| 4 `OPT2` | (235, 214, 178) | (227, 207, 174) | (set 0/1) |
| 5 `OPT3` | (72, 40, 40) | (77, 52, 37) | (set 0/1) |
| 6 `OPT4` | (56, 40, 24) | (56, 41, 24) | (set 0/1) |

Fading text (`0x6003D8`): the global `0x7C3428` is added to the table address of modes 1..4,
i.e. those tables are read `k` rows lower (`k` up to 31); `0x483C61` fills the 31 rows before
tables 1..3 with the identity so that faded levels leave the background unchanged
(`Font::draw_faded`).

## Global fonts (set up at `0x4A54EB`, called from `0x4A1582`)

| font | pack | colour mode | shadow (pack, dx, dy) |
|---|---|---|---|
| `fonG60y5` | `FontG60` | 6 | – |
| `fonG60w` | `FontG60` | 2 | `FontG60s`, 0, 0 |
| `fon40y1` | `FontM40` | 3 | `FontM40s`, 0, 0 |
| `fon40y5` | `FontM40` | 6 | `FontM40s`, 0, 0 |
| `fon30y1` | `FontM30` | 2 | `FontM30s`, 0, 0 |
| `fon30y3` | `FontM30` | 3 | `FontM30s`, 0, 0 |
| `fon30y5` | `FontM30` | 5 | – |
| `fon18w` | `FontM18` | 1 | `FontM18s`, 1, 1 |
| `fon18y3` | `FontM18` | 3 | `FontM18s`, 1, 1 |
| `fon18y5` | `FontM18` | 6 | – |
| `fon16y1` | `FontM16` | 1 | `FontM16s`, 1, 1 |
| `fon16y3` | `FontM16` | 3 | `FontM16s`, 1, 1 |
| `fon16y5` | `FontM16` | 6 | – |
| `fon13y2` | `FontM13` | 3 | `FontM13s`, 0, 0 |
| `BigWhiteFont` | `FontM40` | 1 | – |
| `BigYellowFont` | `FontM40` | 3 | – |
| `BigRedFont` | `FontM40` | 0 (never set) | – |
| `BigBlackFont` | `FontM40` | 1 | `FontM40s`, 0, 0 |
| `WhiteFont` | `FontM18` | 1 | `FontM18s`, 1, 1 |
| `YellowFont` | `FontM18` | 3 | `FontM18s`, 1, 1 |
| `RedFont` | `FontM18` | 2 | `FontM18s`, 1, 1 |
| `BlackFont` | `FontM18` | 6 | – |
| `SpecialWhiteFont` | `FontM16` | 1 | `FontM16s`, 1, 1 |
| `SpecialYellowFont` | `FontM16` | 3 | `FontM16s`, 1, 1 |
| `SpecialRedFont` | `FontM16` | 2 | `FontM16s`, 1, 1 |
| `SpecialBlackFont` | `FontM16` | 6 | – |
| `SmallWhiteFont`, `SmallWhiteFont1` | `FontM13` | 1 | – |
| `SmallRedFont`, `SmallRedFont1` | `FontM13` | 2 | – |
| `SmallYellowFont`, `SmallYellowFont1` | `FontM13` | 3 | – |
| `SmallBlackFont`, `SmallBlackFont1` | `FontM13` | 6 | – |
| `fn8`, `fn10` | `FontM12` | 1 | – |

All global fonts are sprite fonts. Screens also build local fonts, e.g. the main menu
(`0x5C6F50`) uses `LocalGP("rom10")` with `SetWhiteColor` (mode 1), and passes
`fonG60w` / `fonG60y5` to `addTextButton` for its entries; the background
`INTERF2\BACKGROUND_MAIN_MENU.BMP` has the English entries baked in, in the dark brown of
`fonG60y5` with palette set 1. `RLCFont(char*)` (`0x48379C`) loads `.RLC` fonts by names that
come from the text tables (`@CUFONT0`, `@MNMENUPASF`, ...) plus `gf24w.rlc`, `gf24y.rlc`,
`rf32y.rlc`.

## `.RLC` (`LoadRLC` `0x482739`, `ShowRLC` `0x4809F2`)

| offset | type | meaning |
|---|---|---|
| 0 | `"RLC" 0x1B` | magic (not checked) |
| 4 | u32 | item count (low 16 bits used) |
| 8 | u32[count] | item offsets from the file start |

Item: i16 width, i16 height, then per row a run count and per run `u8 skip, u8 len,
len palette indices` (skip relative to the end of the previous run). `ShowRLCItem`
(`0x482888`) draws variants with colour arguments 1..7 (`0x482209`..`0x4822BD`) and item
numbers >= 0x1000 mirrored; fonts always use the plain copy. The shipped RLC fonts
(`GF24W.RLC`, ...) are stylized Cossacks fonts with incomplete Latin glyphs.

## Open questions

- Icons of `MainFont` / `rom10` (`UNICODE.DAT`) are not implemented.
- The RGBA approximation of the tables: per row one colour (row 31) and a fitted opacity;
  the 8-bit original differs by the quantization of the tables (residual 5–15 per channel).
- Which palette set (and hence which text colours) each screen uses, and which screens swap
  tables with `LoadOptionalTable`, is screen code.
- `addTextButton` decides which of its fonts is the passive and which the hover look (the
  baked-in background text suggests `fonG60y5` is passive).
