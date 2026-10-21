//! Ground textures and the tables the terrain renderer composes them with.
//!
//! Reverse engineered from `dmcr.exe` 1.46; see `docs/formats/terrain.md` for the evidence.
//!
//! - `TILES3.BMP`: the ground textures, 64 × 64 palette indices each ([`TileSet`]).
//! - `TEXTURES.LST`: per-texture flags ([`TextureFlags`]).
//! - `MS.MSK`, `MS_A.MSK` .. `MS_D.MSK`: triangle-shaped transition masks ([`MaskTable`]).
//! - `RANDOM.LST`: the engine's table of 8192 random `u16` ([`RandomTable`]).

/// Edge length of a ground texture in pixels.
pub const TILE_SIZE: usize = 64;

/// The ground textures of `TILES3.BMP`.
///
/// The engine skips the 1078-byte header of the 8-bit bitmap and keeps the pixel rows as
/// stored (bottom-up), 256 bytes wide: texture `t` starts at byte
/// `(t & 3) * 64 + (t >> 2) * 16384`, i.e. four textures side by side per 64 rows. Indices
/// at or above [`TileSet::count`] read texture 0 (0x4960E3).
#[derive(Clone, PartialEq, Eq)]
pub struct TileSet {
    count: usize,
    /// Raw pixel rows, 256 bytes each.
    pixels: Vec<u8>,
}

impl TileSet {
    const HEADER: usize = 0x436;
    const STRIDE: usize = 4 * TILE_SIZE;

    pub fn parse(bmp: &[u8]) -> Option<Self> {
        let pixels = bmp.get(Self::HEADER..)?;
        // The engine counts textures as file size / 4096, including the header.
        let count = bmp.len() >> 12;
        let needed = count.div_ceil(4) * Self::STRIDE * TILE_SIZE;
        let mut pixels = pixels.to_vec();
        pixels.resize(needed.max(pixels.len()), 0);
        (count > 0).then_some(Self { count, pixels })
    }

    /// Number of textures (136 in the shipped file).
    pub fn count(&self) -> usize {
        self.count
    }

    /// Palette index of texture `tile` at `(x, y)`, both wrapped to 0..64.
    pub fn texel(&self, tile: u8, x: usize, y: usize) -> u8 {
        let tile = if usize::from(tile) < self.count { usize::from(tile) } else { 0 };
        let base = (tile & 3) * TILE_SIZE + (tile >> 2) * Self::STRIDE * TILE_SIZE;
        self.pixels[base + (y % TILE_SIZE) * Self::STRIDE + x % TILE_SIZE]
    }
}

impl std::fmt::Debug for TileSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TileSet({} textures)", self.count())
    }
}

/// Flags of the 256 texture indices from `TEXTURES.LST` (loader 0x446EA0).
///
/// Lines are `<name> <index> <flags>`; each flag letter sets a bit. Only the bits the
/// renderer reads are named here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureFlags(pub [u16; 256]);

impl TextureFlags {
    /// `L`: locked land.
    pub const LOCKED: u16 = 0x1;
    /// `W`: water.
    pub const WATER: u16 = 0x4;
    /// `P`: drawn without the random vertex lift (0x49BEDF).
    pub const PLAIN: u16 = 0x8;
    /// `R`.
    pub const ROAD: u16 = 0x20;
    /// `B`: vertices of this texture get light level 0 (0x446AC0).
    pub const BRIGHT: u16 = 0x40;

    pub fn parse(text: &str) -> Self {
        let mut flags = [0u16; 256];
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let (Some(name), Some(index), Some(letters)) = (words.next(), words.next(), words.next()) else {
                continue;
            };
            if name.starts_with("//") || name.starts_with('#') {
                continue;
            }
            let Some(slot) = index.parse::<u8>().ok().map(|i| &mut flags[usize::from(i)]) else { continue };
            for c in letters.bytes() {
                *slot |= match c {
                    b'L' => Self::LOCKED,
                    b'W' => Self::WATER,
                    b'P' => Self::PLAIN,
                    b'R' => Self::ROAD,
                    b'B' => Self::BRIGHT,
                    _ => 0,
                };
            }
        }
        Self(flags)
    }

    pub fn has(&self, texture: u8, flag: u16) -> bool {
        self.0[usize::from(texture)] & flag != 0
    }
}

/// A shape of an RLC mask table: per row, `(x, len)` runs of covered pixels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mask {
    pub rows: Vec<Vec<(u8, u8)>>,
}

impl Mask {
    pub fn covers(&self, x: usize, y: usize) -> bool {
        self.rows.get(y).is_some_and(|runs| {
            runs.iter().any(|&(x0, len)| (usize::from(x0)..usize::from(x0) + usize::from(len)).contains(&x))
        })
    }
}

/// An RLC table of shapes without pixel data (`*.MSK`): the layout of [`crate::rlc`] minus the
/// palette indices after each run.
///
/// `"RLC"` + 1 byte, u32 count, `count` u32 offsets from the file start; each item has a
/// 4-byte header (byte 2: row count), then per row a u8 run count and per run a u8 skip
/// and a u8 length. The blitter (0x495C00) only uses the row count and the runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskTable(pub Vec<Mask>);

impl MaskTable {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.get(..3)? != b"RLC" {
            return None;
        }
        let u32_at = |i: usize| data.get(i..i + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
        let count = u32_at(4)?;
        let mut masks = Vec::with_capacity(count);
        for i in 0..count {
            let mut at = u32_at(8 + 4 * i)?;
            let rows = *data.get(at + 2)?;
            at += 4;
            let mut mask = Mask::default();
            for _ in 0..rows {
                let runs = *data.get(at)?;
                at += 1;
                let mut x = 0u8;
                let mut row = Vec::with_capacity(usize::from(runs));
                for _ in 0..runs {
                    let (skip, len) = (*data.get(at)?, *data.get(at + 1)?);
                    at += 2;
                    x = x.wrapping_add(skip);
                    row.push((x, len));
                    x = x.wrapping_add(len);
                }
                mask.rows.push(row);
            }
            masks.push(mask);
        }
        Some(Self(masks))
    }
}

/// `RANDOM.LST`: 8192 random `u16` the engine loads at start (0x557843) and uses for
/// deterministic per-vertex variation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomTable(pub Vec<u16>);

impl RandomTable {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let words = data.get(..0x4000)?.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
        Some(Self(words))
    }

    pub fn get(&self, index: usize) -> u16 {
        self.0[index & 0x1FFF]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_layout_follows_the_engine() {
        let mut bmp = vec![0u8; TileSet::HEADER + 2 * 4096 * 4 - TileSet::HEADER];
        // Texture 5 is in the second block of 64 rows, second column.
        bmp[TileSet::HEADER + 64 * 256 + 3 * 256 + 64 + 2] = 7;
        let tiles = TileSet::parse(&bmp).unwrap();
        assert_eq!(tiles.count(), 8);
        assert_eq!(tiles.texel(5, 2, 3), 7);
        assert_eq!(tiles.texel(5, 66, 67), 7);
        assert_eq!(tiles.texel(200, 0, 0), tiles.texel(0, 0, 0));
    }

    #[test]
    fn flags_from_letters() {
        let flags = TextureFlags::parse("//comment\n80 80 P\nFORSTRELA 3 # FLASH\n#MULTI 1 2 3 4\nX 9 WB\n");
        assert!(flags.has(80, TextureFlags::PLAIN));
        assert_eq!(flags.0[3], 0);
        assert!(flags.has(9, TextureFlags::WATER) && flags.has(9, TextureFlags::BRIGHT));
    }

    #[test]
    fn decodes_masks() {
        let mut d = b"RLC\x1b".to_vec();
        d.extend(1u32.to_le_bytes());
        d.extend(12u32.to_le_bytes());
        d.extend([32, 0, 2, 0]);
        d.extend([1, 3, 2]); // row 0: x 3..5
        d.extend([2, 0, 1, 1, 2]); // row 1: x 0, x 2..4
        let t = MaskTable::parse(&d).unwrap();
        assert_eq!(t.0[0].rows, vec![vec![(3, 2)], vec![(0, 1), (2, 2)]]);
        assert!(t.0[0].covers(4, 0) && !t.0[0].covers(1, 1) && t.0[0].covers(3, 1));
    }
}
