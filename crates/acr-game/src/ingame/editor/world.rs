//! What the editor puts on the map beyond units: a nation's ships and buildings.

use acr_sim::catalog::{Medium, ObjectType};
use acr_sim::fixed::Pos;
use acr_sim::{TypeId, World};

/// The nation tag in brackets at the end of a type id (`ShpFregat(SP)`), per entry of the
/// combo box's nations: ships and buildings belong to a nation by it, as their ids are named
/// for the kind (`ShpPinta`) or shared by a nation's kin (`MayBlg2(AC)` is Aztec).
const TAGS: [&str; 12] = ["MA", "SP", "EN", "FR", "US", "IN", "AC", "PU", "DE", "HU", "IR", "SI"];

/// The nation tag of an id: the text in the last brackets.
fn tag(id: &str) -> Option<&str> {
    id.strip_suffix(')')?.rsplit_once('(').map(|(_, t)| t)
}

/// Whether `t` is listed under `entry` of the combo box (`prefix`: its lower-case type-id
/// prefix): ships and buildings by their nation tag, units by the prefix of their id.
pub fn of_nation(t: &ObjectType, entry: usize, prefix: &str) -> bool {
    if t.building || t.medium == Medium::Water {
        return TAGS.get(entry).is_some_and(|&c| tag(&t.id) == Some(c));
    }
    t.id.to_ascii_lowercase().starts_with(prefix)
}

/// Why a picked type cannot be put at `(x, y)`, if so: land units and buildings not on deep
/// water, ships only on it ([`World::sea_allows`]).
pub fn refuse(world: &World, kind: TypeId, x: i32, y: i32) -> Option<&'static str> {
    if !world.sea_allows(kind, Pos::from_pixels(x, y)) {
        return Some(if world.catalog.get(kind).medium == Medium::Water {
            "Nur auf tiefem Wasser"
        } else {
            "Nicht auf Wasser"
        });
    }
    None
}

/// Puts the building `kind` of `owner` at `(x, y)`, finished and free of charge; its footprint
/// must be free (no other building, no locked ground, no enemy near). Returns its name.
pub fn place_building(world: &mut World, kind: TypeId, owner: u8, (x, y): (i32, i32)) -> Result<String, String> {
    acr_sim::construction::place_free(world, owner, kind, x, y, None)
        .map(|_| world.catalog.get(kind).id.clone())
        .ok_or_else(|| "Hier nicht bebaubar".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nation_tag_is_the_last_brackets() {
        assert_eq!(tag("ShpFregat(SP)"), Some("SP"));
        assert_eq!(tag("MayBlg2(AC)"), Some("AC"));
        assert_eq!(tag("Plain"), None);
    }

    #[test]
    fn every_nation_of_the_combo_has_a_tag() {
        assert_eq!(TAGS.len(), super::super::FIRST_EXTRA);
    }
}
