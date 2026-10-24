//! AI building placement on a synthetic flat map: the city centre is the peasants' mean, a
//! house goes up near it, the next one keeps clear of the first.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{Pos, World, aiplace};

const PEASANT: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    LIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\n[END]\r\n";
const HOUSE: &str = "NAME Hou\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\nUSAGE FARM\r\n\
    LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nCHECKPOINTS 4 0 0 4 0 0 4 4 4\r\nBUILDBAR -40 -40 40 40\r\n[END]\r\n";

#[test]
fn the_ai_places_houses_near_its_centre_without_overlap() {
    let mut c = Catalog::new();
    let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(PEASANT.as_bytes())));
    let hou = c.add(ObjectType::from_def("Hou(EN)", &UnitDef::parse(HOUSE.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.players[1].resources = [10_000; 8];
    for i in 0..4 {
        w.spawn(kri, 1, Pos::from_pixels(2000 + 32 * i, 2100), 0);
    }
    w.ai[1] = true;
    assert!(!aiplace::start(&mut w, 1), "the first call only finds the centre");
    assert_eq!(w.cities[1].center, Some((2048 >> 7, 2100 >> 7)));
    assert!(aiplace::start(&mut w, 1));
    let mut placed = Vec::new();
    for _ in 0..2 {
        assert!(aiplace::accept(&mut w, 1, hou));
        assert!(aiplace::project_pending(&w, 1));
        assert!(!aiplace::accept(&mut w, 1, hou), "one project at a time");
        for _ in 0..6 {
            aiplace::process(&mut w, 1, true);
            if !aiplace::project_pending(&w, 1) {
                break;
            }
        }
        let b = w.units.iter().rfind(|u| u.kind == hou).expect("placed");
        placed.push(b.pos.pixels());
    }
    let (a, b) = (placed[0], placed[1]);
    assert!((a.0 - 2048).abs() < 400 && (a.1 - 2100).abs() < 400, "near the centre: {a:?}");
    // The build bars (80 px squares rotated) may not overlap.
    assert!((a.0 - b.0).abs() + (a.1 - b.1).abs() >= 80, "apart: {a:?} {b:?}");
}
