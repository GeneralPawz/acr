# `GSCfmt` archives (`ALL.GSC`, `Patch01.GS1`, `Patch02.GS1`)

Implementation: `crates/acr-formats/src/gsc.rs`. Verified by reading every entry of all
three shipped archives (`acr list`, `acr extract`): the last entry of `ALL.GSC` ends
exactly at the file size (698 138 183 bytes).

Little-endian. 14-byte header:

| offset | size | field |
|---|---|---|
| 0 | 6 | `"GSCfmt"` |
| 6 | 2 | version, `00 02` |
| 8 | 1 | unknown, `0xCD` |
| 9 | 1 | XOR key, `0x78` |
| 10 | 4 | entry count |

Then `count` entries of 81 bytes:

| offset | size | field |
|---|---|---|
| 0 | 4 | name hash: upper-cased name in a zeroed 64-byte buffer, summed as 16 big-endian u32 (`0x405098`); `CGSCarch::GetFileHandle` compares it before the name. Matches all 5101 entries of the three archives. |
| 4 | 64 | NUL-padded name, `\` separated |
| 68 | 4 | bitwise NOT of the offset, relative to the end of the entry table |
| 72 | 4 | size |
| 76 | 4 | reserved, always 0 |
| 80 | 1 | 1 = every byte XOR-ed with the key |

Everything except `.GP` sprite packs is encrypted. Later archives and loose files
override earlier ones (`Vfs` mount order: `ALL.GSC`, `*.GS1` by name, loose files).

Contents of `ALL.GSC` (4659 entries): `GP` sprite packs (1665), `SMP`, `LNK`, `MD` unit
definitions (plain text), `TXT`, `BMP`, `DAT`, `GRD`, `LST`, `RLC`, `PAL`, `MSP`, `TERR`,
`NDS`, ...
