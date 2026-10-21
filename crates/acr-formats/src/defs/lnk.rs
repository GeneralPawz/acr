//! Piece link files (`PIECES\<piece>.SMP.LNK`), companions of map pieces ([`super::smp`]).
//!
//! Layout (little-endian), verified on all 802 shipped files:
//!
//! ```text
//! "PSMS" | u32 size (file length - 4) | u16 cells | u16 links | u32 flags (mostly 0x800000)
//! cells  * { i8 x, i8 y }                            footprint in grid cells
//! links  * { u8 kind, u8 direction (0..=7), i16 dx, i16 dy, u32 value, u32 0 }
//! ```
//!
//! The editor reads them as `%s.lnk` next to the piece. The footprint is evident from
//! the data; what `kind` and `value` select is open.

use super::Issue;

const MAGIC: &[u8; 4] = b"PSMS";
const HEADER_LEN: usize = 16;
const LINK_LEN: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub kind: u8,
    /// One of 8 directions.
    pub direction: u8,
    pub dx: i16,
    pub dy: i16,
    pub value: u32,
    pub reserved: u32,
}

#[derive(Debug, Clone, Default)]
pub struct PieceLinks {
    pub flags: u32,
    pub cells: Vec<(i8, i8)>,
    pub links: Vec<Link>,
    pub issues: Vec<Issue>,
}

impl PieceLinks {
    /// Parses a link file. Never fails; problems are collected in [`PieceLinks::issues`].
    pub fn parse(data: &[u8]) -> Self {
        let mut out = PieceLinks::default();
        let mut issues = Vec::new();
        let mut issue = |m: String| issues.push(Issue { line: 0, message: m });
        if data.len() < HEADER_LEN || &data[..4] != MAGIC {
            issue("not a PSMS file".into());
            out.issues = issues;
            return out;
        }
        let u16_at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let (size, cells, links) = (u32_at(4) as usize, u16_at(8) as usize, u16_at(10) as usize);
        out.flags = u32_at(12);
        let expected = HEADER_LEN + 2 * cells + LINK_LEN * links;
        if data.len() != expected {
            issue(format!("{} bytes, counts imply {expected}", data.len()));
        }
        if size + 4 != data.len() {
            issue(format!("size field {size} does not match file length {}", data.len()));
        }
        let cell_data = data.get(HEADER_LEN..HEADER_LEN + 2 * cells).unwrap_or_default();
        out.cells = cell_data.as_chunks::<2>().0.iter().map(|&[x, y]| (x as i8, y as i8)).collect();
        let link_data = data.get(HEADER_LEN + 2 * cells..).unwrap_or_default();
        out.links = link_data
            .as_chunks::<LINK_LEN>()
            .0
            .iter()
            .take(links)
            .map(|c| Link {
                kind: c[0],
                direction: c[1],
                dx: i16::from_le_bytes([c[2], c[3]]),
                dy: i16::from_le_bytes([c[4], c[5]]),
                value: u32::from_le_bytes([c[6], c[7], c[8], c[9]]),
                reserved: u32::from_le_bytes([c[10], c[11], c[12], c[13]]),
            })
            .collect();
        out.issues = issues;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut d = b"PSMS".to_vec();
        d.extend(30u32.to_le_bytes());
        d.extend([2, 0, 1, 0, 0, 0, 0x80, 0]);
        d.extend([0, 0xFD, 1, 0xFD]);
        d.extend([3, 7, 0xFF, 0xFF, 0xFE, 0xFF, 0x0C, 0, 0, 0, 0, 0, 0, 0]);
        d
    }

    #[test]
    fn parses_links() {
        let l = PieceLinks::parse(&sample());
        assert!(l.issues.is_empty(), "{:?}", l.issues);
        assert_eq!(l.flags, 0x80_0000);
        assert_eq!(l.cells, [(0, -3), (1, -3)]);
        assert_eq!(l.links[0], Link { kind: 3, direction: 7, dx: -1, dy: -2, value: 12, reserved: 0 });
    }

    #[test]
    fn survives_truncation() {
        let data = sample();
        for len in 0..data.len() {
            assert!(!PieceLinks::parse(&data[..len]).issues.is_empty());
        }
    }
}
