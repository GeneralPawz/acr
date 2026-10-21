//! Sprite lists of map objects (`TREELIST.LST`, `STONLIST.LST`, `HOLELIST.LST`, ...).
//!
//! Loaded by 0x5011AF: the first line names the sprite pack and the entry count
//! (`TREES 472` → `TREES.GP`), then one line per entry: `ID DX DY RADIUS [...]`. Entry `i`
//! is frame `i` of the pack, drawn at `(x − DX, y/2 − height − DY)` for an object at world
//! `(x, y)` (0x4D0930). `//` starts a comment line.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteListEntry {
    pub name: String,
    /// Offset of the object's ground point inside the frame.
    pub dx: i32,
    pub dy: i32,
    /// Footprint radius in pixels.
    pub radius: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteList {
    /// Sprite pack name without extension.
    pub pack: String,
    pub entries: Vec<SpriteListEntry>,
}

impl SpriteList {
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//"));
        let mut head = lines.next()?.split_whitespace();
        let pack = head.next()?.to_owned();
        let count: usize = head.next()?.parse().ok()?;
        let entries = lines
            .take(count)
            .filter_map(|line| {
                let mut w = line.split_whitespace();
                let name = w.next()?.to_owned();
                let mut int = || w.next().and_then(|s| s.parse::<i32>().ok());
                Some(SpriteListEntry { name, dx: int()?, dy: int()?, radius: int().unwrap_or(0) })
            })
            .collect();
        Some(Self { pack, entries })
    }

    /// Archive path of the sprite pack.
    pub fn gp_path(&self) -> String {
        format!("{}.GP", self.pack.to_ascii_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_entries() {
        let list = SpriteList::parse("TREES   2 \r\nD00  74  91  50 \r\n//comment\nD01  181 167 70   \n").unwrap();
        assert_eq!(list.gp_path(), "TREES.GP");
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[1], SpriteListEntry { name: "D01".into(), dx: 181, dy: 167, radius: 70 });
    }
}
