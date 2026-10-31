//! The editor's placing of a building (#406): finished with its lock points or at a
//! construction stage, free of charge, and refused where the footprint is locked.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{World, construction};

const CENTRE: &str = "NAME Cen\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    LOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\nBUILDLOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\n\
    CHECKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\n[END]\r\n";

fn world() -> (World, acr_sim::catalog::TypeId) {
    let mut c = Catalog::new();
    let cen = c.add(ObjectType::from_def("Cen(EN)", &UnitDef::parse(CENTRE.as_bytes())));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), cen)
}

#[test]
fn a_building_is_placed_finished_and_free() {
    let (mut w, cen) = world();
    let b = construction::place_free(&mut w, 0, cen, 2048, 2048, None).expect("free ground").0 as usize;
    let u = &w.units[b];
    assert_eq!(u.life, u.max_life);
    assert_eq!(u.stage, w.max_stage(u));
    let cells: Vec<_> = w.catalog.get(cen).lock_cells(u.pos).collect();
    assert!(cells.iter().all(|&(x, y)| w.map.land.check_pt(x, y)), "the finished footprint is locked");
}

#[test]
fn buildings_do_not_overlap() {
    let (mut w, cen) = world();
    assert!(construction::place_free(&mut w, 0, cen, 2048, 2048, None).is_some());
    assert!(construction::place_free(&mut w, 0, cen, 2048, 2048, None).is_none());
    assert!(construction::place_free(&mut w, 0, cen, 2048 + 160, 2048, None).is_some());
}

#[test]
fn a_stage_places_an_unfinished_site() {
    let (mut w, cen) = world();
    let b = construction::place_free(&mut w, 0, cen, 2048, 2048, Some(4)).expect("free ground").0 as usize;
    let u = &w.units[b];
    assert_eq!(u.stage, 4);
    assert!(u.life < u.max_life && u.life > 2);
}
