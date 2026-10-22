//! What the nations' units and buildings can make: `[FIXED_PRODUCE]`, `[ENABLED]` and
//! `[ACCESSCONTROL]` of the nation files, plus the officers' formation groups.
//!
//! The production icons (0x609230) list the producer's items that are enabled (bit 2 of the
//! object's flags) and accessible (bit 1). `[ENABLED]` sets the first; an object named in
//! `[ACCESSCONTROL]` is accessible only while one of its entries is satisfied: every object
//! listed in it exists (0x42ffed). Items that are not available are left out, not greyed.

use std::collections::{HashMap, HashSet};

use acr_formats::defs::NationDef;

/// One item of a producer's list (`ID x y [KEY]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Object id with the nation suffix, e.g. `EngSkl(EN)`.
    pub id: String,
    /// Slot `x + 12·y`; slots from 36 on are on the second and third page.
    pub slot: usize,
    /// The key that presses the icon (upper case).
    pub key: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Nations {
    /// Producer id (upper case) → items, in file order.
    produce: HashMap<String, Vec<Item>>,
    /// Enabled object ids (upper case).
    enabled: HashSet<String>,
    /// Object id (upper case) → its `[ACCESSCONTROL]` entries (required object ids).
    access: HashMap<String, Vec<Vec<String>>>,
    /// Officer id (upper case) → the formation groups it leads.
    pub officers: HashMap<String, Vec<String>>,
}

impl Nations {
    /// Reads the nation files (`*.NDS` except `WEAPON.NDS`).
    pub fn parse<'a>(files: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let mut n = Self::default();
        for bytes in files {
            let nation = NationDef::parse(bytes);
            for g in &nation.officers {
                let names: Vec<String> = g.formations.iter().map(|f| f.name.to_ascii_uppercase()).collect();
                if let Some(officer) = g.commanders.first() {
                    n.officers.entry(officer.to_ascii_uppercase()).or_insert(names);
                }
            }
            for p in &nation.fixed_produce {
                let items = p
                    .items
                    .iter()
                    .filter(|i| i.x >= 0 && i.y >= 0)
                    .map(|i| Item {
                        id: i.id.clone(),
                        slot: (i.x + 12 * i.y) as usize,
                        key: i.hotkey.as_deref().and_then(|k| k.bytes().next()).map(|k| k.to_ascii_uppercase()),
                    })
                    .collect();
                n.produce.entry(p.id.to_ascii_uppercase()).or_insert(items);
            }
            n.enabled.extend(nation.enabled.iter().map(|e| e.to_ascii_uppercase()));
            for a in &nation.access_control {
                let entry = a.items.iter().map(|i| i.to_ascii_uppercase()).collect();
                n.access.entry(a.head.to_ascii_uppercase()).or_default().push(entry);
            }
        }
        n
    }

    /// The items of a producer.
    pub fn items(&self, producer: &str) -> &[Item] {
        self.produce.get(&producer.to_ascii_uppercase()).map_or(&[], Vec::as_slice)
    }

    /// Whether `id` is enabled and accessible; `exists` answers whether the player has an
    /// object of a (upper case) id.
    pub fn available(&self, id: &str, exists: impl Fn(&str) -> bool) -> bool {
        let id = id.to_ascii_uppercase();
        if !self.enabled.contains(&id) {
            return false;
        }
        match self.access.get(&id) {
            None => true,
            Some(entries) => entries.iter().any(|e| e.iter().all(|r| exists(r))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produce_lists_and_access() {
        let nds = b"[MEMBERS]\r\nEngKri(EN) ENGKRI\r\n[FIXED_PRODUCE]\r\nEngKri(EN) 3\r\nEngSkl(EN) 3 0 Y\r\n\
EngFer(EN) 1 1 E\r\nEngKuz(EN) 4 0 J\r\n[ENABLED]\r\nEngSkl(EN) EngFer(EN)\r\n/EngKuz(EN)\r\n\
[ACCESSCONTROL]\r\nEngFer(EN) 1 EngSkl(EN)\r\n[END]\r\n";
        let n = Nations::parse([&nds[..]]);
        let items = n.items("engkri(en)");
        assert_eq!(items.len(), 3);
        assert_eq!(items[1], Item { id: "EngFer(EN)".into(), slot: 13, key: Some(b'E') });
        assert!(n.available("EngSkl(EN)", |_| false));
        assert!(!n.available("EngKuz(EN)", |_| true), "commented out of [ENABLED]");
        assert!(!n.available("EngFer(EN)", |_| false));
        assert!(n.available("EngFer(EN)", |id| id == "ENGSKL(EN)"));
    }
}
