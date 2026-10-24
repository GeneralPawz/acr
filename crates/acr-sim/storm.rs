//! Going into other nations' buildings (`GoToMine` 0x5f43d0, `GoToMineLink` 0x5f46f7): an
//! empty one is taken, a garrisoned one stormed.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, TypeId, World};

const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@ATTACK 9 0 0 19\r\n@DEATH 9 0 0 9\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    MATHERIAL 1 BODY\r\nCANSTORM\r\nSTORMFORCE 10\r\n[END]\r\n";
const BARRACKS: &str = "NAME Bar\r\nBUILDING\r\nLIFE 3000\r\nLOCATION 0 0 64 64\r\nBUILDSTAGES 10\r\n\
    LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nCONCENTRATOR 1 3 3\r\nUNITABSORBER 5\r\nOPPOSIT 1 1\r\n\
    HIGHUNITCANTENTER\r\nMATHERIAL 1 BUILDING\r\n[END]\r\n";

struct Kinds {
    soldier: TypeId,
    defender: TypeId,
    plain: TypeId,
    rider: TypeId,
    barracks: TypeId,
}

fn setup() -> (World, Kinds) {
    let mut c = Catalog::new();
    let def =
        |c: &mut Catalog, id: &str, text: String| c.add(ObjectType::from_def(id, &UnitDef::parse(text.as_bytes())));
    let soldier = def(&mut c, "Sol(EN)", SOLDIER.to_owned());
    let defender = def(&mut c, "Def(EN)", SOLDIER.replace("STORMFORCE 10", "STORMFORCE 5"));
    let plain = def(&mut c, "Pla(EN)", SOLDIER.replace("CANSTORM\r\n", ""));
    let rider = def(&mut c, "Rid(EN)", SOLDIER.replace("CANSTORM\r\n", "CANSTORM\r\nHIGHUNIT\r\n"));
    let barracks = def(&mut c, "Bar(EN)", BARRACKS.to_owned());
    let w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    (w, Kinds { soldier, defender, plain, rider, barracks })
}

fn run_until(w: &mut World, ticks: usize, done: impl Fn(&World) -> bool) -> bool {
    (0..ticks).any(|_| {
        w.step(&[]);
        done(w)
    })
}

/// Into an empty enemy barracks (command 0x2d with Ctrl): the soldier walks to the door and
/// the building goes over to its nation (0x5f4b6c) with the soldier inside.
#[test]
fn an_empty_enemy_building_is_taken() {
    let (mut w, k) = setup();
    let b = w.spawn(k.barracks, 1, Pos::from_pixels(2048, 2048), 0);
    let s = w.spawn(k.soldier, 0, Pos::from_pixels(2300, 2200), 0);
    w.step(&[Command::Enter { player: 0, units: vec![s], building: b }]);
    assert!(run_until(&mut w, 400, |w| w.units[s.0 as usize].inside_of == Some(b)), "went in");
    assert_eq!(w.units[b.0 as usize].owner, 0);
    assert_eq!(w.units[b.0 as usize].nmask, w.nation_mask[0]);
}

/// A garrisoned one is stormed: the stormer plays `#ATTACK`, then a defender (`STORMFORCE` 5,
/// `OPPOSIT 1 1`: `5·r >> 15` ≤ 4 against 10) always falls and the stormer always dies; the
/// next stormer finds it empty and takes it.
#[test]
fn a_garrison_is_stormed_one_by_one() {
    let (mut w, k) = setup();
    let b = w.spawn(k.barracks, 1, Pos::from_pixels(2048, 2048), 0);
    let d = w.spawn(k.defender, 1, Pos::from_pixels(1500, 1500), 0);
    w.step(&[Command::Enter { player: 1, units: vec![d], building: b }]);
    assert!(run_until(&mut w, 400, |w| w.units[d.0 as usize].inside_of == Some(b)), "the defender went in");
    let s1 = w.spawn(k.soldier, 0, Pos::from_pixels(2300, 2200), 0);
    let s2 = w.spawn(k.soldier, 0, Pos::from_pixels(2350, 2250), 0);
    w.step(&[Command::Enter { player: 0, units: vec![s1], building: b }]);
    let random = w.random;
    assert!(run_until(&mut w, 400, |w| w.units[s1.0 as usize].death != 0), "the stormer fell");
    assert_ne!(w.random, random, "two random numbers per strike");
    assert!(w.units[b.0 as usize].inside.is_empty(), "the defender fell too");
    assert!(!w.units[d.0 as usize].alive);
    assert_eq!(w.units[b.0 as usize].owner, 1, "not taken yet");
    w.step(&[Command::Enter { player: 0, units: vec![s2], building: b }]);
    assert!(run_until(&mut w, 400, |w| w.units[s2.0 as usize].inside_of == Some(b)));
    assert_eq!(w.units[b.0 as usize].owner, 0);
}

/// Only peasants and `CANSTORM` types go into buildings at all, and no `HIGHUNIT` into a
/// `HIGHUNITCANTENTER` one.
#[test]
fn who_may_go_in() {
    let (mut w, k) = setup();
    let b = w.spawn(k.barracks, 0, Pos::from_pixels(2048, 2048), 0);
    let ids: Vec<EntityId> = [k.plain, k.rider, k.soldier]
        .iter()
        .enumerate()
        .map(|(n, &t)| w.spawn(t, 0, Pos::from_pixels(2300 + 40 * n as i32, 2200), 0))
        .collect();
    w.step(&[Command::Enter { player: 0, units: ids.clone(), building: b }]);
    run_until(&mut w, 400, |_| false);
    let inside: Vec<bool> = ids.iter().map(|&i| w.units[i.0 as usize].inside_of == Some(b)).collect();
    assert_eq!(inside, [false, false, true]);
}

/// `GoToMine` (0x5f43d0) with `CreateOrder`'s placements, as `InviteUnitIntoBuilding` passes
/// its `ordType`: front puts the entry before the walk, append after it, replace drops the
/// walk; every placement gives the unit the order's priority `0xFF & 0x7f`.
#[test]
fn enter_order_placements() {
    use acr_sim::motion::{ORD_APPEND, ORD_FRONT, ORD_REPLACE};
    use acr_sim::world::Order;
    let (mut w, k) = setup();
    let b = w.spawn(k.barracks, 0, Pos::from_pixels(2048, 2048), 0);
    let s = w.spawn(k.soldier, 0, Pos::from_pixels(2300, 2200), 0);
    let i = s.0 as usize;
    // `orders.last()` runs first.
    for (ord, len, at) in [(ORD_FRONT, 2, 1), (ORD_APPEND, 2, 0), (ORD_REPLACE, 1, 0)] {
        let u = &mut w.units[i];
        u.orders.clear();
        u.prio = 0;
        acr_sim::motion::send_to(u, Pos::from_pixels(3000, 3000), 5, true);
        assert!(acr_sim::economy::enter_ex(&mut w, i, b.0 as usize, 0xFF, ord));
        let u = &w.units[i];
        assert_eq!(u.orders.len(), len, "order type {ord}");
        assert!(matches!(u.orders[at], Order::Enter { building, prio: 0x7f } if building == b), "order type {ord}");
        assert_eq!(u.prio, 0x7f);
    }
}

/// Members of a formation go in too (the player's command 0x2d sends every selected unit).
#[test]
fn formation_members_go_in() {
    use acr_formats::defs::{OrderGroups, OrderList};
    const OFFICER: &str = "NAME Off\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
        @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nOFFICER\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
    const ORDERS: &[u8] =
        b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n";
    const GROUPS: &[u8] = b"1\r\nLINE 0 1 #LINE15\r\n";
    let mut c = Catalog::new();
    let soldier = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let off = c.add(ObjectType::from_def("Off(EN)", &UnitDef::parse(OFFICER.as_bytes())));
    let barracks = c.add(ObjectType::from_def("Bar(EN)", &UnitDef::parse(BARRACKS.as_bytes())));
    let f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
    let officer = w.spawn(off, 0, Pos::from_pixels(2600, 2600), 0);
    let soldiers: Vec<EntityId> =
        (0..15).map(|i| w.spawn(soldier, 0, Pos::from_pixels(2500 + 20 * (i % 5), 2700 + 20 * (i / 5)), 0)).collect();
    w.step(&[Command::Form { player: 0, officer, units: soldiers.clone(), kind: "LINE".into() }]);
    run_until(&mut w, 300, |_| false);
    assert_eq!(w.brigades.len(), 1);
    let b = w.spawn(barracks, 1, Pos::from_pixels(2048, 2048), 0);
    let mut sel = soldiers.clone();
    sel.push(officer);
    w.step(&[Command::Enter { player: 0, units: sel, building: b }]);
    assert!(
        run_until(&mut w, 600, |w| w.units[b.0 as usize].owner == 0),
        "taken; orders {:?}",
        soldiers.iter().map(|s| w.units[s.0 as usize].orders.clone()).collect::<Vec<_>>()
    );
}
