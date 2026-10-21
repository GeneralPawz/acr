//! Map pieces (`*.SMP`): terrain templates the map editor stamps onto a map.
//!
//! `BUILDINGS\<NAME>.SMP` belongs to the building whose `.MD` names it with `PIECE`;
//! `PIECES\*.SMP` (listed by `PIECES\*.LST`) and `UserPieces\*.smp` are editor pieces
//! (rocks, forests, villages). Each piece may have a `<file>.LNK` companion
//! ([`super::lnk`]). These are not sound samples. Layout (little-endian):
//!
//! ```text
//! "SAMP"
//! chunks   [u8;4] tag | u32 size (including these 8 bytes) | payload
//!   VERT   u32 n | n * { i16 x, i16 y, u8 texture, u8[3] (each 0..=2), i16 value }
//!   SPRT   u32 n | n * { u16 group ("AG", "ST" or 0), i32 x, i32 y, u16 sprite }
//!   LOCK   u32 a | u32 b | (a + b) * { i16 x, i16 y }
//!          (size field is 16 + 2*(a+b) in 122 files; the real size is 16 + 4*(a+b))
//!   OBJS   u32 n | n * { i32 x, i32 y, u8 owner, [u8;31] object id, NUL padded }
//!   PIX1   u32 bytes | u32 n | n * { i16 x, i16 y, i32[4], u16 len, [u8; len] }
//!   NRG1   u32 bytes | u16[6] header | bytes-12 bytes
//!   ZONE   u32 0, then zone data up to the end of the file (not decoded)
//! end      u32 0xFFFFFFFF (absent after ZONE)
//! ```
//!
//! Only the chunk structure, `VERT`, `SPRT`, `LOCK` and `OBJS` record layouts are
//! verified (by consuming every file exactly); field meanings are partly guesses.

use super::Issue;
use crate::text::CodePage;

const MAGIC: &[u8; 4] = b"SAMP";
const END: [u8; 4] = [0xFF; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vertex {
    /// Position in terrain grid units relative to the piece origin.
    pub x: i16,
    pub y: i16,
    pub texture: u8,
    /// Three values in `0..=2`, meaning open.
    pub blend: [u8; 3],
    /// Probably a height offset.
    pub value: i16,
}

/// A map sprite (tree, stone, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteObject {
    /// Sprite group tag: `AG` or `ST` read as ASCII, or 0.
    pub group: [u8; 2],
    pub x: i32,
    pub y: i32,
    pub sprite: u16,
}

/// A unit or building placed with the piece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedObject {
    pub x: i32,
    pub y: i32,
    /// 0 or 7 in the shipped data; probably the owning player.
    pub owner: u8,
    /// Nation member id (`EngCen(EN)`).
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixRecord {
    pub x: i16,
    pub y: i16,
    pub values: [i32; 4],
    /// Usually empty; meaning open.
    pub extra: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Piece {
    pub vertices: Vec<Vertex>,
    pub sprites: Vec<SpriteObject>,
    /// The two cell lists of `LOCK`, meaning open.
    pub lock: [Vec<(i16, i16)>; 2],
    pub objects: Vec<PlacedObject>,
    pub pix: Vec<PixRecord>,
    pub nrg_header: [u16; 6],
    pub nrg: Vec<u8>,
    pub zone: Option<Vec<u8>>,
    /// Chunks with unknown tags, kept raw.
    pub unknown: Vec<([u8; 4], Vec<u8>)>,
    /// Whether the `LOCK` size field has the writer bug described in the module docs.
    pub lock_size_bug: bool,
    pub issues: Vec<Issue>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let b = self.data.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(b)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes(2)?.try_into().ok()?))
    }

    fn i16(&mut self) -> Option<i16> {
        Some(self.u16()? as i16)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(self.u32()? as i32)
    }

    /// Reads `n` records, failing early if they cannot fit.
    fn records<T>(&mut self, n: u32, size: usize, mut f: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        if (n as usize).checked_mul(size)? > self.data.len() - self.pos {
            return None;
        }
        (0..n).map(|_| f(self)).collect()
    }
}

impl Piece {
    /// Parses a piece. Never fails; problems are collected in [`Piece::issues`].
    pub fn parse(data: &[u8]) -> Self {
        let mut piece = Piece::default();
        if data.get(..4) != Some(MAGIC) {
            piece.issue(0, "missing SAMP magic");
            return piece;
        }
        let mut pos = 4;
        loop {
            match data.len() - pos {
                0 => {
                    piece.issue(pos, "missing end marker");
                    break;
                }
                4 if data[pos..] == END => break,
                n if n < 8 => {
                    piece.issue(pos, "trailing bytes");
                    break;
                }
                _ => {}
            }
            let tag: [u8; 4] = data[pos..pos + 4].try_into().unwrap();
            let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
            if &tag == b"ZONE" {
                piece.zone = Some(data[pos + 8..].to_vec());
                break;
            }
            let mut r = Reader { data, pos: pos + 8 };
            let parsed = match &tag {
                b"VERT" => piece.parse_vert(&mut r),
                b"SPRT" => piece.parse_sprt(&mut r),
                b"LOCK" => piece.parse_lock(&mut r, size),
                b"OBJS" => piece.parse_objs(&mut r),
                b"PIX1" => piece.parse_pix(&mut r),
                b"NRG1" => piece.parse_nrg(&mut r),
                _ if size >= 8 && pos + size <= data.len() => {
                    piece.unknown.push((tag, data[pos + 8..pos + size].to_vec()));
                    r.pos = pos + size;
                    Some(())
                }
                _ => None,
            };
            let name = CodePage::Cp1252.decode(&tag);
            if parsed.is_none() {
                piece.issue(pos, &format!("{name} chunk truncated or unknown"));
                break;
            }
            let expected = if &tag == b"LOCK" { r.pos - pos } else { size };
            if r.pos - pos != expected {
                piece.issue(pos, &format!("{name} chunk is {} bytes, header says {size}", r.pos - pos));
            }
            pos = r.pos;
        }
        piece
    }

    fn issue(&mut self, offset: usize, message: &str) {
        self.issues.push(Issue { line: 0, message: format!("{message} at byte {offset:#x}") });
    }

    fn parse_vert(&mut self, r: &mut Reader) -> Option<()> {
        let n = r.u32()?;
        self.vertices = r.records(n, 10, |r| {
            let (x, y, texture) = (r.i16()?, r.i16()?, r.u8()?);
            Some(Vertex { x, y, texture, blend: [r.u8()?, r.u8()?, r.u8()?], value: r.i16()? })
        })?;
        Some(())
    }

    fn parse_sprt(&mut self, r: &mut Reader) -> Option<()> {
        let n = r.u32()?;
        self.sprites = r.records(n, 12, |r| {
            Some(SpriteObject { group: r.bytes(2)?.try_into().ok()?, x: r.i32()?, y: r.i32()?, sprite: r.u16()? })
        })?;
        Some(())
    }

    fn parse_lock(&mut self, r: &mut Reader, size: usize) -> Option<()> {
        let (a, b) = (r.u32()?, r.u32()?);
        for (i, n) in [a, b].into_iter().enumerate() {
            self.lock[i] = r.records(n, 4, |r| Some((r.i16()?, r.i16()?)))?;
        }
        let total = a as usize + b as usize;
        self.lock_size_bug = size != 16 + 4 * total && size == 16 + 2 * total;
        if size != 16 + 4 * total && !self.lock_size_bug {
            self.issue(r.pos, &format!("LOCK size field {size} matches neither layout"));
        }
        Some(())
    }

    fn parse_objs(&mut self, r: &mut Reader) -> Option<()> {
        let n = r.u32()?;
        self.objects = r.records(n, 40, |r| {
            let (x, y, owner) = (r.i32()?, r.i32()?, r.u8()?);
            let name = r.bytes(31)?;
            let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
            Some(PlacedObject { x, y, owner, id: CodePage::Cp1252.decode(name) })
        })?;
        Some(())
    }

    fn parse_pix(&mut self, r: &mut Reader) -> Option<()> {
        let start = r.pos;
        let (bytes, n) = (r.u32()? as usize, r.u32()?);
        self.pix = r.records(n, 22, |r| {
            let (x, y) = (r.i16()?, r.i16()?);
            let values = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
            let len = r.u16()? as usize;
            Some(PixRecord { x, y, values, extra: r.bytes(len)?.to_vec() })
        })?;
        if r.pos - start != bytes + 4 {
            self.issue(start, "PIX1 records disagree with its byte count");
        }
        Some(())
    }

    fn parse_nrg(&mut self, r: &mut Reader) -> Option<()> {
        let bytes = r.u32()? as usize;
        for h in &mut self.nrg_header {
            *h = r.u16()?;
        }
        self.nrg = r.bytes(bytes.checked_sub(12)?)?.to_vec();
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &[u8; 4], payload: &[u8], size: Option<u32>) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((size.unwrap_or(8 + payload.len() as u32)).to_le_bytes());
        v.extend(payload);
        v
    }

    fn sample(lock_size: Option<u32>) -> Vec<u8> {
        let mut d = MAGIC.to_vec();
        d.extend(chunk(b"VERT", &[1, 0, 0, 0, 0xF2, 0xFF, 4, 0, 9, 0, 1, 2, 0x80, 0], None));
        d.extend(chunk(b"SPRT", &[1, 0, 0, 0, b'A', b'G', 5, 0, 0, 0, 6, 0, 0, 0, 7, 0], None));
        d.extend(chunk(b"LOCK", &[0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 2, 0, 3, 0, 4, 0], lock_size));
        let mut obj = vec![1, 0, 0, 0, 0xF5, 0xFF, 0xFF, 0xFF, 0xF4, 0xFF, 0xFF, 0xFF, 7];
        obj.extend(b"EngCen(EN)");
        obj.resize(44, 0);
        d.extend(chunk(b"OBJS", &obj, None));
        let mut pix = vec![27, 0, 0, 0, 1, 0, 0, 0];
        pix.extend([0u8; 20]);
        pix.extend([1, 0, 0x5A]);
        d.extend(chunk(b"PIX1", &pix, None));
        d.extend(chunk(b"NRG1", &[14, 0, 0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 0xAA, 0xBB], None));
        d.extend(END);
        d
    }

    #[test]
    fn parses_chunks() {
        let p = Piece::parse(&sample(None));
        assert!(p.issues.is_empty(), "{:?}", p.issues);
        assert_eq!(p.vertices[0], Vertex { x: -14, y: 4, texture: 9, blend: [0, 1, 2], value: 128 });
        assert_eq!(p.sprites[0], SpriteObject { group: *b"AG", x: 5, y: 6, sprite: 7 });
        assert_eq!(p.lock[1], [(1, 2), (3, 4)]);
        assert_eq!(p.objects[0], PlacedObject { x: -11, y: -12, owner: 7, id: "EngCen(EN)".into() });
        assert_eq!(p.pix[0].extra, [0x5A]);
        assert_eq!(p.nrg_header, [1, 2, 3, 4, 5, 6]);
        assert_eq!(p.nrg, [0xAA, 0xBB]);
        assert!(!p.lock_size_bug);
    }

    #[test]
    fn accepts_lock_size_bug() {
        let p = Piece::parse(&sample(Some(16 + 2 * 2)));
        assert!(p.issues.is_empty(), "{:?}", p.issues);
        assert!(p.lock_size_bug);
        assert_eq!(p.objects.len(), 1);
    }

    #[test]
    fn survives_truncation() {
        let data = sample(None);
        for len in 0..data.len() {
            assert!(!Piece::parse(&data[..len]).issues.is_empty());
        }
    }
}
