//! Sprite lists of map objects (`TREELIST.LST`, `STONLIST.LST`, `HOLELIST.LST`, ...).
//!
//! Loaded by 0x5011AF: the first line names the sprite pack and the entry count
//! (`TREES 472` → `TREES.GP`), then one line per entry: `ID DX DY RADIUS [...]`. Entry `i`
//! is frame `i` of the pack, drawn at `(x − DX, y/2 − height − DY)` for an object at world
//! `(x, y)` (0x4D0930). `//` starts a comment line.
//!
//! `COMPLEX.LST` (fields) adds `#ANIMATION pause layers z0 dz` after the radius (0x5011af):
//! the entry's picture is the weapon animation of that name (`WEAPON.ADS`), stepped every
//! `pause` ticks; its frames are not the pack's.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteListEntry {
    pub name: String,
    /// Offset of the object's ground point inside the frame.
    pub dx: i32,
    pub dy: i32,
    /// Footprint radius in pixels.
    pub radius: i32,
    /// `#ANIMATION` of a `COMPLEX` entry (with the `#`), drawn instead of a pack frame.
    pub animation: Option<String>,
    /// Ticks per animation step (`pause`, at least 1); 1 without an animation.
    pub pause: i32,
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
                let (dx, dy, radius) = (int()?, int()?, int().unwrap_or(0));
                let animation = w.next().filter(|a| a.starts_with('#')).map(str::to_owned);
                let pause = animation.as_ref().and_then(|_| w.next()?.parse::<i32>().ok()).unwrap_or(1).max(1);
                Some(SpriteListEntry { name, dx, dy, radius, animation, pause })
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
        let entry = &list.entries[1];
        assert_eq!((entry.name.as_str(), entry.dx, entry.dy, entry.radius), ("D01", 181, 167, 70));
        assert_eq!((entry.animation.as_deref(), entry.pause), (None, 1));
    }

    #[test]
    fn complex_entries_name_their_animation() {
        let text = "COMPLEX 2\nKUK00 159 146 30 #KUK00 1 1 0 0\nKUD05 159 146 30 #KUK05 3 1 0 0\n";
        let list = SpriteList::parse(text).unwrap();
        assert_eq!((list.entries[0].animation.as_deref(), list.entries[0].pause), (Some("#KUK00"), 1));
        assert_eq!((list.entries[1].animation.as_deref(), list.entries[1].pause), (Some("#KUK05"), 3));
    }
}
