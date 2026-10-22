//! Ships on a synthetic map with a lake west of x = 2000 px: a ship sails, a ferry docks at
//! the shore, a soldier boards it, the ferry crosses and lets him off.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::ship::Dock;
use acr_sim::{Command, Pos, World};

const FERRY: &str = "NAME Par\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 65 1 0 0\r\n#MOTION_L 65 1 0 0\r\nLIFE 1000\r\n\
    GEOMETRY 1 100 16\r\nMEDIA WATER\r\nMOTIONSTYLE SHEEPS\r\nTRANSPORT 250\r\nMATHERIAL 1 WOOD\r\n[END]\r\n";
const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\n[END]\r\n";

/// Water west of `shore` px (land locked there), land east of it.
fn lake(shore: i32) -> MapGrid {
    let mut g = MapGrid::open(1);
    let side = g.land.width;
    let c = shore >> 4;
    g.water.clr_rect(1, 1, c - 1, side - 2);
    g.land.set_rect(0, 0, c, side);
    g
}

#[test]
fn a_ship_accelerates_and_sails_to_its_target() {
    let mut c = Catalog::new();
    let par = c.add(ObjectType::from_def("Par(EN)", &UnitDef::parse(FERRY.as_bytes())));
    let mut w = World::new(Arc::new(lake(2000)), Arc::new(c), Arc::new(Formations::default()));
    let s = w.spawn(par, 0, Pos::from_pixels(500, 2000), 0);
    w.step(&[Command::Move { player: 0, units: vec![s], x: 1500, y: 2000 }]);
    let mut speeds = Vec::new();
    for _ in 0..700 {
        w.step(&[]);
        speeds.push(w.units[s.0 as usize].speed);
    }
    // +6 per tick (counting the tick of the order) up to 2·step = 32 (1/16 px per tick),
    // overshooting once.
    assert_eq!(&speeds[..6], &[12, 18, 24, 30, 36, 32]);
    let (x, _) = w.units[s.0 as usize].pos.pixels();
    assert!((x - 1500).abs() < 40, "arrived: x {x}");
}

#[test]
fn a_ferry_docks_takes_a_soldier_across_and_lets_him_off() {
    let mut c = Catalog::new();
    let par = c.add(ObjectType::from_def("Par(EN)", &UnitDef::parse(FERRY.as_bytes())));
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let mut w = World::new(Arc::new(lake(2000)), Arc::new(c), Arc::new(Formations::default()));
    let f = w.spawn(par, 0, Pos::from_pixels(1200, 2000), 0);
    let s = w.spawn(sol, 0, Pos::from_pixels(2400, 2000), 128);
    // A move onto the land: the ferry docks at the shore.
    w.step(&[Command::Move { player: 0, units: vec![f], x: 2150, y: 2000 }]);
    for _ in 0..1200 {
        w.step(&[]);
        if matches!(w.units[f.0 as usize].dock, Dock::At(_)) {
            break;
        }
    }
    let Dock::At(p) = w.units[f.0 as usize].dock else { panic!("not docked: {:?}", w.units[f.0 as usize].orders) };
    assert!(p.pixels().0 > 2000, "the landing point is on land: {:?}", p.pixels());
    w.step(&[Command::Board { player: 0, units: vec![s], transport: f }]);
    for _ in 0..800 {
        w.step(&[]);
        if w.units[s.0 as usize].inside_of.is_some() {
            break;
        }
    }
    assert_eq!(w.units[s.0 as usize].inside_of, Some(f), "aboard");
    // Along the shore and unload.
    w.step(&[Command::Move { player: 0, units: vec![f], x: 2150, y: 3200 }]);
    for _ in 0..2000 {
        w.step(&[]);
        if matches!(w.units[f.0 as usize].dock, Dock::At(_)) && w.units[f.0 as usize].orders.is_empty() {
            break;
        }
    }
    assert!(matches!(w.units[f.0 as usize].dock, Dock::At(_)), "docked again");
    w.step(&[Command::Unload { player: 0, transport: f }]);
    for _ in 0..600 {
        w.step(&[]);
    }
    let u = &w.units[s.0 as usize];
    assert!(u.inside_of.is_none() && !u.unlimited, "off and on foot: {:?}", u.orders);
    let (x, y) = u.pos.pixels();
    assert!(x > 2000 && y > 2800, "on the far shore: ({x}, {y})");
}
