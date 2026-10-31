//! The first object's switches: stop production, canister now, load canister.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::Order;
use acr_sim::{Command, Pos, World};

const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nPRICE 1 FOOD 10\r\n[END]\r\n";
const BARRACKS: &str = "NAME Bar\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    CANSTOPBUILD\r\nFARM\r\nBORNPOINTS 1 4 4\r\n[END]\r\n";
const CANNON: &str = "NAME Gun\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n#PSTAND1 9 1 0 0\r\nLIFE 300\r\n\
    GEOMETRY 1 20 20\r\nATTACK_RADIUS 0 50 800\r\nATTACK_RADIUS 1 0 300\r\nDAMAGE 0 100\r\nDAMAGE 1 50\r\n\
    WEAPONINSECTOR 1 0 300 60 10 32 50\r\n[END]\r\n";

#[test]
fn stop_production_holds_the_queue() {
    let mut c = Catalog::new();
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let bar = c.add(ObjectType::from_def("Bar(EN)", &UnitDef::parse(BARRACKS.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.players[0].resources = [1000; 8];
    let b = w.spawn(bar, 0, Pos::from_pixels(2048, 2048), 0);
    acr_sim::economy::census_all(&mut w);
    w.step(&[
        Command::Produce { player: 0, building: b, kind: sol, endless: false },
        Command::StopProduction { player: 0, buildings: vec![b], stop: true },
    ]);
    for _ in 0..600 {
        w.step(&[]);
    }
    assert!(!w.units.iter().any(|u| u.kind == sol), "nothing produced while stopped");
    assert_eq!(w.units[b.0 as usize].queue.len(), 1);
    w.step(&[Command::StopProduction { player: 0, buildings: vec![b], stop: false }]);
    for _ in 0..600 {
        w.step(&[]);
    }
    assert!(w.units.iter().any(|u| u.kind == sol), "produced after resuming");
}

#[test]
fn canister_buttons() {
    let mut c = Catalog::new();
    let gun = c.add(ObjectType::from_def("Gun(EN)", &UnitDef::parse(CANNON.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let g = w.spawn(gun, 0, Pos::from_pixels(2048, 2048), 64);
    w.step(&[Command::LoadCanister { player: 0, units: vec![g] }]);
    assert_eq!(w.units[g.0 as usize].stance, 2, "canister attack index 1 + 1");
    acr_sim::artillery::fire_canister(&mut w, g.0 as usize);
    assert!(
        matches!(w.units[g.0 as usize].orders.last(), Some(Order::AttackPoint { x: 2048, y: 2303, times: 1, .. })),
        "{:?}",
        w.units[g.0 as usize].orders
    );
}
