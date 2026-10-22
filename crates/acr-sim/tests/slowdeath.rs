//! `SLOWDEATH` buildings burn down below 2000 life (0x4e7410).

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Pos, TypeId, World};

const HOUSE: &str = "NAME Hou\r\nBUILDING\r\nLIFE 5000\r\nLOCATION 0 0 64 64\r\n\
    LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nSLOWDEATH\r\n[END]\r\n";

fn setup() -> (World, TypeId, TypeId) {
    let mut c = Catalog::new();
    let slow = c.add(ObjectType::from_def("Hou(EN)", &UnitDef::parse(HOUSE.as_bytes())));
    let plain = c.add(ObjectType::from_def("Bar(EN)", &UnitDef::parse(HOUSE.replace("SLOWDEATH\r\n", "").as_bytes())));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), slow, plain)
}

fn life(w: &World, id: EntityId) -> i32 {
    w.units[id.0 as usize].life
}

/// Steps until the tick at which the list is filled (`tick & 0x7f == 0x11`) has run.
fn until_listing(w: &mut World) {
    while w.tick.0 & 0x7f != 0x11 {
        w.step(&[]);
    }
    w.step(&[]);
}

/// Listed at the next tick ≡ 0x11 (mod 128), then 5 life per tick until 10 or less, then it
/// dies; an equally damaged building without the flag keeps its life.
#[test]
fn a_damaged_slowdeath_building_burns_down() {
    let (mut w, slow, plain) = setup();
    let a = w.spawn(slow, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(plain, 0, Pos::from_pixels(3000, 2000), 0);
    for id in [a, b] {
        w.units[id.0 as usize].life = 1500;
    }
    w.step(&[]);
    assert_eq!(life(&w, a), 1500, "not listed before the 0x11 tick");
    until_listing(&mut w);
    assert_eq!(w.slow_deaths, [a]);
    let mut ticks = 0;
    while w.units[a.0 as usize].death == 0 {
        w.step(&[]);
        ticks += 1;
        assert!(ticks < 400, "burns down");
    }
    // 1500 → 10 in 298 steps of 5, the 299th pass finds 10 and kills it.
    assert_eq!(ticks, 299);
    assert!(w.slow_deaths.is_empty());
    assert_eq!(life(&w, b), 1500);
}

/// A listed building repaired above half its life leaves the list (and still loses 5); the
/// one after it in the list is skipped that tick, as the engine counts on after shifting.
#[test]
fn a_repaired_building_leaves_and_its_neighbour_is_skipped() {
    let (mut w, slow, _) = setup();
    let a = w.spawn(slow, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(slow, 0, Pos::from_pixels(3000, 2000), 0);
    for id in [a, b] {
        w.units[id.0 as usize].life = 1500;
    }
    until_listing(&mut w);
    assert_eq!(w.slow_deaths, [a, b]);
    w.units[a.0 as usize].life = 3000;
    let before = life(&w, b);
    w.step(&[]);
    assert_eq!(w.slow_deaths, [b]);
    assert_eq!(life(&w, a), 2995);
    assert_eq!(life(&w, b), before, "skipped this tick");
    w.step(&[]);
    assert_eq!(life(&w, b), before - 5);
}
