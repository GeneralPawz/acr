//! The static game data the simulation needs, collected without I/O.
//!
//! The host asks [`SimAssets::start`] for the files to read, hands every file it reads to
//! [`SimAssets::add_file`] (which may ask for more) and builds the catalog, sprite shapes
//! and formations with [`SimAssets::build`] once [`SimAssets::ready`] says so. Reading can
//! be blocking (tools) or asynchronous (the game in the browser).

use std::collections::BTreeMap;

use acr_formats::defs::{NationDef, OrderGroups, OrderList, SpriteResources, UnitDef, name_list};
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
    /// The files to read first. `paths` are all game paths (to find the `.NDS` files).
    pub fn start(&mut self, paths: &[String]) -> Vec<String> {
        let mut nations: Vec<String> = paths
            .iter()
            .filter(|p| {
                let n = normalize_path(p);
                n.ends_with(".NDS") && n != "WEAPON.NDS"
            })
            .cloned()
            .collect();
        nations.sort();
        self.nations = nations.clone();
        let mut first: Vec<String> =
            [NAME_LIST, ORDERS, ORDER_GROUPS, NRES, ECONOMY].iter().map(|p| (*p).to_owned()).collect();
        for (_, name) in SHAPE_LISTS {
            first.push(format!("{name}.LST"));
            first.push(format!("{name}.RSR"));
        }
        first.extend(nations);
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
        if let Some(nres) = self.files.get(NRES) {
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
        SimData { catalog, shapes, formations: Formations::new(&orders, &groups) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_for_definitions_after_the_name_list() {
        let mut a = SimAssets::default();
        let first = a.start(&["ENG.NDS".to_owned(), "WEAPON.NDS".to_owned(), "X.MD".to_owned()]);
        assert!(first.contains(&"ENG.NDS".to_owned()) && !first.contains(&"WEAPON.NDS".to_owned()));
        let more = a.add_file("NMLIST.TXT", Some(b"EngMus\r\n"));
        assert_eq!(more, ["EngMus.MD"]);
        assert!(a.wants("ENGMUS.MD"));
        assert!(!a.ready());
        for p in first {
            a.add_file(&p, Some(b""));
        }
        a.add_file("EngMus.MD", Some(b"NAME EngMus\r\n"));
        assert!(a.ready());
        assert_eq!(a.build().catalog.find("EngMus(EN)"), None);
    }
}
