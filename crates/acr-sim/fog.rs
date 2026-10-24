//! Fog of war around a unit on a synthetic map.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::fog::Fog;
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{Pos, World};

#[test]
fn a_unit_lights_its_surroundings_and_the_light_fades() {
    let mut c = Catalog::new();
    let def = UnitDef::parse(b"NAME Scout\r\nLIFE 10\r\nGEOMETRY 1 10 40\r\nVISION 3\r\n[END]\r\n");
    let kind = c.add(ObjectType::from_def("Scout(EN)", &def));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let id = w.spawn(kind, 0, Pos::from_pixels(4000, 4000), 0);
    let mut fog = Fog::new(1);
    for _ in 0..20 {
        fog.reveal(&w, 0);
        fog.blur(&w);
    }
    assert!(fog.visible(&w, 4000, 4000), "the unit's own spot");
    assert!(fog.visible(&w, 4200, 4000), "inside VISION 3 (axis points at 256 px)");
    assert!(!fog.visible(&w, 5000, 4000), "far away stays dark");
    // Without the unit the light fades back to black.
    w.remove(id);
    for _ in 0..400 {
        fog.reveal(&w, 0);
        fog.blur(&w);
    }
    assert!(!fog.visible(&w, 4000, 4000));
}
