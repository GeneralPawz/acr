//! A troop sent into a house and back out must walk on (#365): leaving through the door
//! does not leave the unit stuck there.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, World};

const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nCANSTORM\r\n[END]\r\n";
const HOUSE: &str = "NAME Hou\r\nBUILDING\r\nLIFE 3000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    LOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\nCONCENTRATOR 1 1 3\r\nBORNPOINTS 2 1 3 1 4\r\n\
    UNITABSORBER 5\r\nEXITPAUSE 3\r\nMATHERIAL 1 BUILDING\r\n[END]\r\n";

fn run(leave: impl Fn(&mut World, EntityId, EntityId)) {
    let mut c = Catalog::new();
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let hou = c.add(ObjectType::from_def("Hou(EN)", &UnitDef::parse(HOUSE.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let b = w.spawn(hou, 0, Pos::from_pixels(2048, 2048), 0);
    let u = w.spawn(sol, 0, Pos::from_pixels(2100, 2300), 0);
    w.step(&[Command::Enter { player: 0, units: vec![u], building: b }]);
    for _ in 0..400 {
        w.step(&[]);
    }
    assert_eq!(w.units[u.0 as usize].inside_of, Some(b), "the unit went in");
    leave(&mut w, b, u);
    for _ in 0..200 {
        w.step(&[]);
    }
    let (tx, ty) = (2400, 2700);
    w.step(&[Command::Move { player: 0, units: vec![u], x: tx, y: ty }]);
    for _ in 0..600 {
        w.step(&[]);
    }
    let (px, py) = w.units[u.0 as usize].pos.pixels();
    assert!((px - tx).abs() < 48 && (py - ty).abs() < 48, "stuck at ({px}, {py})");
}

#[test]
fn leave_command_then_move_reaches_the_target() {
    run(|w, b, _| {
        w.step(&[Command::Leave { player: 0, building: b, kind: None }]);
    });
}

#[test]
fn leave_mine_then_move_reaches_the_target() {
    run(|w, b, u| {
        let kind = w.units[u.0 as usize].kind;
        acr_sim::economy::leave_mine(w, b.0 as usize, kind);
    });
}

#[test]
fn leave_with_a_type_lets_one_unit_of_it_out() {
    let mut c = Catalog::new();
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let other_def = SOLDIER.replace("NAME Sol", "NAME Oth");
    let other = c.add(ObjectType::from_def("Oth(EN)", &UnitDef::parse(other_def.as_bytes())));
    let hou = c.add(ObjectType::from_def("Hou(EN)", &UnitDef::parse(HOUSE.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let b = w.spawn(hou, 0, Pos::from_pixels(2048, 2048), 0);
    let a: Vec<EntityId> = (0..2).map(|_| w.spawn(sol, 0, Pos::from_pixels(2100, 2300), 0)).collect();
    let o = w.spawn(other, 0, Pos::from_pixels(2100, 2300), 0);
    w.step(&[Command::Enter { player: 0, units: vec![a[0], a[1], o], building: b }]);
    for _ in 0..400 {
        w.step(&[]);
    }
    assert_eq!(w.units[b.0 as usize].inside.len(), 3, "all three went in");
    let leave = Command::Leave { player: 0, building: b, kind: Some(sol) };
    let mut left = Vec::new();
    for _ in 0..3 {
        w.step(std::slice::from_ref(&leave));
        for _ in 0..20 {
            w.step(&[]);
        }
        left.push(w.units[b.0 as usize].inside.len());
    }
    assert_eq!(left, [2, 1, 1], "one soldier per command, then none of the type is left");
    assert_eq!(w.units[b.0 as usize].inside, vec![o], "the other type stayed");
}
