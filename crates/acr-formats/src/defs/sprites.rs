//! Resource files of the map sprite lists (`TREELIST.RSR`, `STONLIST.RSR`, ...; the lists
//! themselves are [`crate::sprite_list`]).
//!
//! An `.RSR` file has `[SECTION]`s; the simulation uses two of them:
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

fn lines(data: &[u8]) -> impl Iterator<Item = Vec<&str>> {
    data.split(|&b| b == b'\n').map(|l| {
        std::str::from_utf8(l)
            .unwrap_or("")
            .split_ascii_whitespace()
            .take_while(|t| !t.starts_with('/'))
            .collect::<Vec<_>>()
    })
}

/// The parts of a `.RSR` file the simulation needs, keyed by upper-case sprite id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpriteResources {
    /// `[LOCK]`: blocking polygon per sprite id.
    pub locks: BTreeMap<String, Vec<(i32, i32)>>,
    /// `[INTERNAL_SOURCE]`: `(resource, amount, radius)` per sprite id (mine deposits).
    pub sources: BTreeMap<String, (String, i32, i32)>,
    /// `[SOURCES]`: `(resource, amount per work cycle, work radius)` per sprite id (trees,
    /// fields, carcasses).
    pub work_sources: BTreeMap<String, (String, i32, i32)>,
    /// `[WORKTRANSFORM] ID NEXT n`: after `n` work cycles the sprite becomes `NEXT`
    /// (`NONE`: it disappears).
    pub work_transform: BTreeMap<String, (String, i32)>,
    /// `[TIMETRANSFORM] ID NEXT n`: after `n` animation frames (`n/2` ticks) likewise.
    pub time_transform: BTreeMap<String, (String, i32)>,
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
                "WORKTRANSFORM" | "TIMETRANSFORM" => {
                    if let (Some(next), Some(n)) = (l.get(1), int(2)) {
                        let map =
                            if section == "WORKTRANSFORM" { &mut r.work_transform } else { &mut r.time_transform };
                        map.insert(id, (next.to_ascii_uppercase(), n));
                    }
                }
                "INTERNAL_SOURCE" | "SOURCES" => {
                    if let (Some(res), Some(amount), Some(radius)) = (l.get(1), int(2), int(3)) {
                        let map = if section == "SOURCES" { &mut r.work_sources } else { &mut r.sources };
                        map.insert(id, (res.to_ascii_uppercase(), amount, radius));
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
    fn parses_resources() {
        let r = SpriteResources::parse(
            b"[INTERNAL_SOURCE]\r\nLT129 IRON  20 32\r\n[LOCK]\r\n//comment\r\nD60 3 32 61 12 46 16 30\r\n[END]\r\n",
        );
        assert_eq!(r.locks["D60"], [(32, 61), (12, 46), (16, 30)]);
        assert_eq!(r.sources["LT129"], ("IRON".into(), 20, 32));
        assert!(r.work_sources.is_empty());
        let w = SpriteResources::parse(b"[SOURCES]\r\nD00 WOOD 4 30\r\n[END]\r\n");
        assert_eq!(w.work_sources["D00"], ("WOOD".into(), 4, 30));
    }
}
