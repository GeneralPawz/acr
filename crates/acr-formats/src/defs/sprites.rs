//! Map sprite lists (`TREELIST.LST`, `STONLIST.LST`, ...) and their resource files (`.RSR`).
//!
//! ```text
//! TREES   472            list name, entry count
//! D00  74  91  50        id, picture hotspot dx dy, radius; the line index is the type index
//! ```
//!
//! The `.RSR` file of a list has `[SECTION]`s; the simulation uses two of them:
//!
//! ```text
//! [INTERNAL_SOURCE]
//! LT129 IRON  20 32      id, resource, amount, radius (mines)
//! [LOCK]
//! D60 6 32 61 12 46 ...  id, point count, polygon points (picture pixels, y at half scale)
//! ```
//!
//! Readers in `dmcr.exe`: the polygon loader at 0x503bad, `[INTERNAL_SOURCE]` at 0x503118.

use std::collections::BTreeMap;

/// One entry of a sprite list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteType {
    pub id: String,
    /// Picture hotspot: the sprite's map position minus this is the picture's origin.
    pub dx: i32,
    pub dy: i32,
    pub radius: i32,
}

/// A sprite list (`*.LST`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpriteList {
    pub name: String,
    pub types: Vec<SpriteType>,
}

fn lines(data: &[u8]) -> impl Iterator<Item = Vec<&str>> {
    data.split(|&b| b == b'\n').map(|l| {
        std::str::from_utf8(l)
            .unwrap_or("")
            .split_ascii_whitespace()
            .take_while(|t| !t.starts_with('/'))
            .collect::<Vec<_>>()
    })
}

impl SpriteList {
    pub fn parse(data: &[u8]) -> Self {
        let mut list = SpriteList::default();
        let mut it = lines(data).filter(|l| !l.is_empty());
        if let Some(head) = it.next() {
            list.name = head[0].to_owned();
        }
        for l in it {
            let int = |i: usize| l.get(i).and_then(|t| t.parse().ok());
            if let (Some(dx), Some(dy)) = (int(1), int(2)) {
                list.types.push(SpriteType { id: l[0].to_owned(), dx, dy, radius: int(3).unwrap_or(0) });
            }
        }
        list
    }
}

/// The parts of a `.RSR` file the simulation needs, keyed by upper-case sprite id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpriteResources {
    /// `[LOCK]`: blocking polygon per sprite id.
    pub locks: BTreeMap<String, Vec<(i32, i32)>>,
    /// `[INTERNAL_SOURCE]`: `(resource, amount, radius)` per sprite id.
    pub sources: BTreeMap<String, (String, i32, i32)>,
}

impl SpriteResources {
    pub fn parse(data: &[u8]) -> Self {
        let mut r = SpriteResources::default();
        let mut section = String::new();
        for l in lines(data).filter(|l| !l.is_empty()) {
            if l[0].starts_with('[') {
                section = l[0].trim_matches(['[', ']']).to_ascii_uppercase();
                continue;
            }
            let int = |i: usize| l.get(i).and_then(|t| t.parse::<i32>().ok());
            let id = l[0].to_ascii_uppercase();
            match section.as_str() {
                "LOCK" => {
                    let n = int(1).unwrap_or(0).max(0) as usize;
                    let pts: Option<Vec<_>> = (0..n).map(|i| Some((int(2 + 2 * i)?, int(3 + 2 * i)?))).collect();
                    if let Some(p) = pts.filter(|p| p.len() >= 3) {
                        r.locks.insert(id, p);
                    }
                }
                "INTERNAL_SOURCE" => {
                    if let (Some(res), Some(amount), Some(radius)) = (l.get(1), int(2), int(3)) {
                        r.sources.insert(id, (res.to_ascii_uppercase(), amount, radius));
                    }
                }
                _ => {}
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_and_resources() {
        let l = SpriteList::parse(b"TREES   2 \r\nD00  74  91  50 \r\nD01  181 167 70   \r\n");
        assert_eq!(l.name, "TREES");
        assert_eq!(l.types[1], SpriteType { id: "D01".into(), dx: 181, dy: 167, radius: 70 });
        let r = SpriteResources::parse(
            b"[INTERNAL_SOURCE]\r\nLT129 IRON  20 32\r\n[LOCK]\r\n//comment\r\nD60 3 32 61 12 46 16 30\r\n[END]\r\n",
        );
        assert_eq!(r.locks["D60"], [(32, 61), (12, 46), (16, 30)]);
        assert_eq!(r.sources["LT129"], ("IRON".into(), 20, 32));
    }
}
