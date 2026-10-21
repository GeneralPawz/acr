//! `RLC` picture lists (`*.RLC`): the older run-length sprite format, used for a few fonts,
//! cursors and effects.
//!
//! Reverse engineered from `LoadRLC` (`0x482739`) and `ShowRLC` (`0x4809F2`) in `dmcr.exe`;
//! see `docs/formats/fonts.md`. Layout (little-endian):
//!
//! ```text
//! file    "RLC" 0x1B | u32 count (the original uses the low 16 bits) | u32 offset[count]
//! item    at offset (from the file start): i16 width | i16 height
//!         then per row (height rows): u8 runs, then per run: u8 skip, u8 len, len palette indices
//!         skips are relative to the end of the previous run in the row
//! ```
//!
//! Pixels are copied opaquely (no blending, no transparent index).

use std::fmt;

pub const MAGIC: &[u8; 4] = b"RLC\x1B";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlcError {
    pub offset: usize,
    pub reason: &'static str,
}

impl fmt::Display for RlcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {:#x}", self.reason, self.offset)
    }
}

impl std::error::Error for RlcError {}

/// A horizontal run of opaque pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlcRun {
    pub x: u16,
    pub y: u16,
    /// Palette indices.
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlcItem {
    pub width: i16,
    pub height: i16,
    pub runs: Vec<RlcRun>,
}

#[derive(Debug, Clone)]
pub struct RlcFile {
    pub items: Vec<RlcItem>,
}

impl RlcFile {
    pub fn parse(data: &[u8]) -> Result<Self, RlcError> {
        let err = |offset, reason| RlcError { offset, reason };
        if data.get(..4) != Some(MAGIC) {
            return Err(err(0, "missing RLC magic"));
        }
        let u32_at = |o: usize| {
            data.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or(err(o, "header truncated"))
        };
        let count = (u32_at(4)? & 0xFFFF) as usize;
        let items = (0..count).map(|i| parse_item(data, u32_at(8 + 4 * i)? as usize)).collect::<Result<_, _>>()?;
        Ok(Self { items })
    }
}

fn parse_item(data: &[u8], start: usize) -> Result<RlcItem, RlcError> {
    let byte = |o: usize| data.get(o).copied().ok_or(RlcError { offset: o, reason: "item truncated" });
    let i16_at = |o: usize| Ok::<_, RlcError>(i16::from_le_bytes([byte(o)?, byte(o + 1)?]));
    let (width, height) = (i16_at(start)?, i16_at(start + 2)?);
    let mut runs = Vec::new();
    let mut pos = start + 4;
    for y in 0..height.max(0) as u16 {
        let count = byte(pos)?;
        pos += 1;
        let mut x = 0u16;
        for _ in 0..count {
            let (skip, len) = (byte(pos)?, usize::from(byte(pos + 1)?));
            pos += 2;
            x += u16::from(skip);
            let pixels = data.get(pos..pos + len).ok_or(RlcError { offset: pos, reason: "run truncated" })?.to_vec();
            pos += len;
            runs.push(RlcRun { x, y, pixels });
            x += len as u16;
        }
    }
    Ok(RlcItem { width, height, runs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_items() {
        let mut data = MAGIC.to_vec();
        data.extend(1u32.to_le_bytes());
        data.extend(12u32.to_le_bytes());
        data.extend(3i16.to_le_bytes());
        data.extend(2i16.to_le_bytes());
        data.extend([2, 0, 1, 7, 1, 1, 8, 0]);
        let file = RlcFile::parse(&data).unwrap();
        assert_eq!(file.items.len(), 1);
        let item = &file.items[0];
        assert_eq!((item.width, item.height), (3, 2));
        assert_eq!(item.runs, vec![RlcRun { x: 0, y: 0, pixels: vec![7] }, RlcRun { x: 2, y: 0, pixels: vec![8] }]);
        assert!(RlcFile::parse(&data[..data.len() - 2]).is_err());
    }
}
