//! The static game data the simulation needs, collected without I/O.
//!
//! The host asks [`SimAssets::start`] for the files to read, hands every file it reads to
//! [`SimAssets::add_file`] (which may ask for more) and builds the catalog, sprite shapes
//! and formations with [`SimAssets::build`] once [`SimAssets::ready`] says so. Reading can
//! be blocking (tools) or asynchronous (the game in the browser).

use std::collections::BTreeMap;

use acr_formats::defs::{NationDef, NationList, OrderGroups, OrderList, SpriteResources, UnitDef, name_list};
use acr_formats::m3d::SpriteGroup;
use acr_formats::normalize_path;
use acr_formats::sprite_list::SpriteList;

use crate::catalog::{Catalog, ObjectType};
use crate::formation::Formations;
use crate::map::SpriteShapes;

const NAME_LIST: &str = "NMLIST.TXT";
const ORDERS: &str = "ORDERS.LST";
const ORDER_GROUPS: &str = "ORD_GROUPS.LST";
const NRES: &str = "NRES.DAT";
const ECONOMY: &str = "ECONOMY.TXT";
const MORALE: &str = "MORALE.DAT";
const NATION_LIST: &str = "NATIONS.LST";
const WEAPONS: &str = "WEAPON.NDS";
const WEAPON_ANIMS: &str = "WEAPON.ADS";
const TEXTURES: &str = "TEXTURES.LST";
/// Sprite lists whose blocking polygons go into the lock map.
const SHAPE_LISTS: [(SpriteGroup, &str); 5] = [
    (SpriteGroup::Trees, "TREELIST"),
    (SpriteGroup::Stones, "STONLIST"),
    (SpriteGroup::Holes, "HOLELIST"),
    (SpriteGroup::Complex, "COMPLEX"),
    (SpriteGroup::Meat, "MEAT"),
];

/// What the simulation is built from.
pub struct SimData {
    pub catalog: Catalog,
    pub shapes: SpriteShapes,
    pub formations: Formations,
}

#[derive(Default)]
pub struct SimAssets {
    /// Every file still expected, by normalized path.
    pending: BTreeMap<String, ()>,
    nations: Vec<String>,
    files: BTreeMap<String, Vec<u8>>,
    defs: BTreeMap<String, UnitDef>,
}

impl SimAssets {
    /// The files to read first. Nation files follow once `NATIONS.LST` is read.
    pub fn start(&mut self, _paths: &[String]) -> Vec<String> {
        let mut first: Vec<String> =
            [NATION_LIST, NAME_LIST, ORDERS, ORDER_GROUPS, NRES, ECONOMY, MORALE, WEAPONS, WEAPON_ANIMS, TEXTURES]
                .iter()
                .map(|p| (*p).to_owned())
                .collect();
        for (_, name) in SHAPE_LISTS {
            first.push(format!("{name}.LST"));
            first.push(format!("{name}.RSR"));
        }
        for p in &first {
            self.pending.insert(normalize_path(p), ());
        }
        first
    }

    /// Whether `path` is a file these assets asked for and have not received yet.
    pub fn wants(&self, path: &str) -> bool {
        self.pending.contains_key(&normalize_path(path))
    }

    /// Takes a file (`None` if it could not be read); returns further files to read.
    pub fn add_file(&mut self, path: &str, bytes: Option<&[u8]>) -> Vec<String> {
        let key = normalize_path(path);
        if self.pending.remove(&key).is_none() {
            return Vec::new();
        }
        let Some(bytes) = bytes else { return Vec::new() };
        if key == NATION_LIST {
            // Only the listed files are nations; the archives hold stale ones too (0x4ea7f0).
            // The catalog follows the list order (0x60e4a0).
            let more = NationList::parse(bytes).files();
            self.nations.clone_from(&more);
            for p in &more {
                self.pending.insert(normalize_path(p), ());
            }
            return more;
        }
        if key == NAME_LIST {
            let more: Vec<String> = name_list(bytes)
                .into_iter()
                .map(|n| format!("{}.MD", n.trim_end_matches(".MD").trim_end_matches(".md")))
                .collect();
            for p in &more {
                self.pending.insert(normalize_path(p), ());
            }
            return more;
        }
        if key.ends_with(".MD") {
            let def = UnitDef::parse(bytes);
            if let Some(name) = def.name() {
                self.defs.insert(name.to_ascii_uppercase(), def);
            }
            return Vec::new();
        }
        self.files.insert(key, bytes.to_vec());
        Vec::new()
    }

    pub fn ready(&self) -> bool {
        self.pending.is_empty() && self.files.contains_key(ORDERS)
    }

    /// Builds the simulation data from everything received.
    pub fn build(&self) -> SimData {
        let mut catalog = Catalog::new();
        let nations: Vec<NationDef> = self
            .nations
            .iter()
            .filter_map(|path| self.files.get(&normalize_path(path)))
            .map(|bytes| NationDef::parse(bytes))
            .collect();
        for nation in &nations {
            for m in &nation.members {
                if let Some(def) = self.defs.get(&m.md.to_ascii_uppercase()) {
                    catalog.add(ObjectType::from_def(&m.id, def));
                }
            }
        }
        // The tech sections may name members of any nation (one namespace, as in the engine).
        let mut tech = std::mem::take(&mut catalog.tech);
        for nation in &nations {
            tech.add_nation(&catalog, nation);
        }
        catalog.tech = tech;
        catalog.market = self.files.get(ECONOMY).and_then(|b| crate::market::MarketDef::parse(b));
        if let Some(nds) = self.files.get(WEAPONS) {
            let ads = self.files.get(WEAPON_ANIMS).map(Vec::as_slice);
            let tex = self.files.get(TEXTURES).map(Vec::as_slice);
            catalog.set_weapons(crate::weapon::Weapons::parse(nds, ads, tex));
        }
        if let Some(m) = self.files.get(MORALE) {
            catalog.morale = crate::morale::MoraleDef::parse(m);
        }
        if let Some(nres) = self.files.get(NRES) {
            for line in String::from_utf8_lossy(nres).lines() {
                let mut it = line.split_whitespace();
                match (it.next(), it.next().and_then(|v| v.parse::<i32>().ok())) {
                    (Some("ADDRANGE"), Some(v)) => catalog.add_range = v,
                    (Some("ADDPAUSE"), Some(v)) => catalog.add_pause = v,
                    _ => {}
                }
            }
            catalog.set_weapon_kinds(&acr_formats::defs::nres::weapon_kinds(nres));
        }
        let mut shapes = SpriteShapes::default();
        for (group, name) in SHAPE_LISTS {
            let Some(list) =
                self.files.get(&format!("{name}.LST")).and_then(|b| SpriteList::parse(&String::from_utf8_lossy(b)))
            else {
                continue;
            };
            let rsr = self.files.get(&format!("{name}.RSR")).map(|d| SpriteResources::parse(d)).unwrap_or_default();
            shapes.add(group, &list, &rsr);
        }
        let empty = Vec::new();
        let orders = OrderList::parse(self.files.get(ORDERS).unwrap_or(&empty));
        let groups = OrderGroups::parse(self.files.get(ORDER_GROUPS).unwrap_or(&empty));
        let mut formations = Formations::new(&orders, &groups);
        formations.set_officers(&catalog, &nations);
        SimData { catalog, shapes, formations }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &[u8] = b"2\r\nIR Iroquois IROKEZ.NDS (IR)\r\n- - - (XX)\r\n1\r\nCO Cossacks COS.NDS (CO)\r\n";

    #[test]
    fn asks_for_definitions_after_the_name_list() {
        let mut a = SimAssets::default();
        let first = a.start(&[]);
        // WEAPON.NDS is read once, as the weapon table, not as a nation.
        assert_eq!(first.iter().filter(|p| *p == "WEAPON.NDS").count(), 1);
        assert!(a.add_file("NATIONS.LST", Some(LIST)).contains(&"IROKEZ.NDS".to_owned()));
        let more = a.add_file("NMLIST.TXT", Some(b"EngMus\r\n"));
        assert_eq!(more, ["EngMus.MD"]);
        assert!(a.wants("ENGMUS.MD"));
        assert!(!a.ready());
        for p in first {
            a.add_file(&p, Some(b""));
        }
        a.add_file("IROKEZ.NDS", Some(b""));
        a.add_file("EngMus.MD", Some(b"NAME EngMus\r\n"));
        assert!(a.ready());
        assert_eq!(a.build().catalog.find("EngMus(EN)"), None);
    }

    #[test]
    fn loads_only_the_nations_in_the_list() {
        let mut a = SimAssets::default();
        // Stale copies in the archives (`TEXT\IROKEZ.NDS`, `FUNNY.NDS`) are not asked for.
        let first = a.start(&["IROKEZ.NDS".into(), "TEXT\\IROKEZ.NDS".into(), "FUNNY.NDS".into()]);
        assert!(first.iter().all(|p| !p.ends_with("IROKEZ.NDS") && p != "FUNNY.NDS"));
        let list = b"2\r\nIR Iroquois IROKEZ.NDS (IR)\r\nEN England ENG.NDS (EN)\r\n";
        let more = a.add_file("NATIONS.LST", Some(list));
        assert_eq!(more, ["IROKEZ.NDS", "ENG.NDS"]);
        assert_eq!(a.nations, more);
        assert!(!a.wants("FUNNY.NDS") && !a.wants("TEXT\\IROKEZ.NDS"));
    }

    #[test]
    fn loads_no_nations_without_a_list() {
        let mut a = SimAssets::default();
        a.start(&["ENG.NDS".into()]);
        assert!(a.add_file("NATIONS.LST", None).is_empty());
        assert!(a.nations.is_empty());
    }
}
