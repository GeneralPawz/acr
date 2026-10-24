//! `LeaveMine` (0x5f518b) and its link (0x5f522e): one unit of the asked type leaves a
//! building after the type's `EXITPAUSE`.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::{EntityId, Order};
use acr_sim::{Pos, TypeId, World};

const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@ATTACK 9 0 0 19\r\n@DEATH 9 0 0 9\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    MATHERIAL 1 BODY\r\nCANSTORM\r\n[END]\r\n";
const BARRACKS: &str = "NAME Bar\r\nBUILDING\r\nLIFE 3000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nCONCENTRATOR 1 3 3\r\nBORNPOINTS 2 3 3 5 5\r\nUNITABSORBER 5\r\n\
    EXITPAUSE 3\r\nMATHERIAL 1 BUILDING\r\n[END]\r\n";

fn setup() -> (World, TypeId, TypeId, EntityId, Vec<EntityId>) {
    let mut c = Catalog::new();
    let def =
        |c: &mut Catalog, id: &str, text: String| c.add(ObjectType::from_def(id, &UnitDef::parse(text.as_bytes())));
    let a = def(&mut c, "Sol(EN)", SOLDIER.to_owned());
    let b = def(&mut c, "Oth(EN)", SOLDIER.replace("LIFE 100", "LIFE 90"));
    let bar = def(&mut c, "Bar(EN)", BARRACKS.to_owned());
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let bid = w.spawn(bar, 0, Pos::from_pixels(2048, 2048), 0);
    let ids: Vec<EntityId> = [a, b, b].iter().map(|&t| w.spawn(t, 0, Pos::from_pixels(2300, 2200), 0)).collect();
    for &id in &ids {
        w.units[id.0 as usize].inside_of = Some(bid);
        w.units[bid.0 as usize].inside.push(id);
    }
    (w, a, b, bid, ids)
}

#[test]
fn exit_pause_defaults_to_8() {
    let t = ObjectType::from_def("X", &UnitDef::parse(SOLDIER.as_bytes()));
    assert_eq!(t.exit_pause, 8);
}

#[test]
fn one_unit_of_the_type_leaves_after_the_pause() {
    let (mut w, _, other, b, ids) = setup();
    let bi = b.0 as usize;
    acr_sim::economy::leave_mine(&mut w, bi, other);
    assert_eq!(w.units[bi].orders.last(), Some(&Order::LeaveMine { kind: other, delay: 3 }));
    for _ in 0..3 {
        w.step(&[]);
        assert_eq!(w.units[bi].inside.len(), 3, "still waiting");
    }
    w.step(&[]);
    assert_eq!(w.units[bi].inside, vec![ids[0], ids[2]], "the first of the type left");
    assert_eq!(w.units[ids[1].0 as usize].inside_of, None);
    assert!(w.units[bi].orders.is_empty(), "the order ended");
}

#[test]
fn without_a_unit_of_the_type_the_order_just_ends() {
    let (mut w, _, _, b, _) = setup();
    let bi = b.0 as usize;
    acr_sim::economy::leave_mine(&mut w, bi, TypeId(99));
    for _ in 0..4 {
        w.step(&[]);
    }
    assert_eq!(w.units[bi].inside.len(), 3);
    assert!(w.units[bi].orders.is_empty());
}

#[test]
fn an_empty_building_gets_no_order() {
    let (mut w, a, _, b, _) = setup();
    let bi = b.0 as usize;
    w.units[bi].inside.clear();
    acr_sim::economy::leave_mine(&mut w, bi, a);
    assert!(w.units[bi].orders.is_empty());
}
