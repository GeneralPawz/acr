//! Unit combat states and orders on a synthetic flat map: stand ground, hold fire,
//! attack-move, patrol and formation melee.

use std::sync::Arc;

use acr_formats::defs::nres::weapon_kinds;
use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::{BOrder, Formations};
use acr_sim::map::MapGrid;
use acr_sim::weapon::Weapons;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, TypeId, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 10\r\n\
    ATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nSEARCH_ENEMY_RADIUS 700\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n\
    DET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const OFFICER: &str = "NAME Off\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nOFFICER\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
const ORDERS: &[u8] = b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n";
const GROUPS: &[u8] = b"1\r\nLINE 0 1 #LINE15\r\n";

fn setup() -> (World, TypeId, TypeId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let off = c.add(ObjectType::from_def("Off(EN)", &UnitDef::parse(OFFICER.as_bytes())));
    let f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f)), pik, off)
}

fn u(w: &World, id: EntityId) -> &acr_sim::Unit {
    &w.units[id.0 as usize]
}

#[test]
fn standing_units_do_not_chase() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(pik, 1, Pos::from_pixels(2300, 2000), 128);
    w.step(&[
        Command::StandGround { player: 0, units: vec![a], on: true },
        Command::HoldFire { player: 1, units: vec![b], hold: true },
    ]);
    let start = u(&w, a).pos;
    for _ in 0..400 {
        w.step(&[]);
    }
    assert!(u(&w, a).stand_ground);
    assert_eq!(u(&w, a).pos, start, "the standing pikeman stayed");
    assert_eq!((u(&w, a).life, u(&w, b).life), (100, 100));
    // A move ends stand ground; the pikeman now attacks once it is close.
    w.step(&[Command::Move { player: 0, units: vec![a], x: 2200, y: 2000 }]);
    assert!(!u(&w, a).stand_ground);
    for _ in 0..600 {
        w.step(&[]);
    }
    assert!(u(&w, b).life < 100, "the moved pikeman fought");
}

#[test]
fn hold_fire_stops_auto_attacks_but_not_orders() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(pik, 1, Pos::from_pixels(2150, 2000), 128);
    w.step(&[
        Command::HoldFire { player: 0, units: vec![a], hold: true },
        Command::HoldFire { player: 1, units: vec![b], hold: true },
    ]);
    for _ in 0..300 {
        w.step(&[]);
    }
    assert_eq!((u(&w, a).life, u(&w, b).life), (100, 100), "nobody fought");
    // The player's attack order (priority 0xFE) overrides hold fire and ends it.
    w.step(&[Command::Attack { player: 0, units: vec![a], target: b }]);
    assert!(!u(&w, a).no_search_victim && u(&w, a).target() == Some(b));
    for _ in 0..300 {
        w.step(&[]);
    }
    assert!(u(&w, b).life < 100);
}

#[test]
fn attack_move_fights_on_the_way_and_goes_on() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(1500, 2000), 0);
    let b = w.spawn(pik, 1, Pos::from_pixels(1900, 2000), 128);
    w.units[b.0 as usize].life = 20;
    w.step(&[
        Command::HoldFire { player: 1, units: vec![b], hold: true },
        Command::AttackMove { player: 0, units: vec![a], x: 2600, y: 2000 },
    ]);
    assert!(u(&w, a).in_patrol, "an attack-move is a priority-0 move");
    let mut fought = false;
    for _ in 0..1500 {
        w.step(&[]);
        fought |= u(&w, a).target() == Some(b);
    }
    assert!(fought && u(&w, b).death != 0, "the enemy on the way was killed");
    let (x, _) = u(&w, a).pos.pixels();
    assert!((x - 2600).abs() < 40, "and the pikeman went on to the target: x {x}");
}

#[test]
fn patrol_walks_back_and_forth() {
    let (mut w, pik, _) = setup();
    let a = w.spawn(pik, 0, Pos::from_pixels(1500, 2000), 0);
    w.step(&[Command::Patrol { player: 0, units: vec![a], x: 1900, y: 2000 }]);
    let (mut far, mut back) = (false, false);
    for _ in 0..800 {
        w.step(&[]);
        let (x, _) = u(&w, a).pos.pixels();
        far |= x > 1850;
        back |= far && x < 1550;
    }
    assert!(far && back, "reached both ends");
}

#[test]
fn formation_melee_sends_members_after_enemies() {
    let (mut w, pik, off) = setup();
    let officer = w.spawn(off, 0, Pos::from_pixels(2000, 2000), 0);
    let soldiers: Vec<EntityId> =
        (0..15).map(|i| w.spawn(pik, 0, Pos::from_pixels(1900 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0)).collect();
    w.step(&[Command::Form { player: 0, officer, units: soldiers.clone(), kind: "LINE".into() }]);
    for _ in 0..300 {
        w.step(&[]);
    }
    let enemies: Vec<EntityId> = (0..3).map(|k| w.spawn(pik, 1, Pos::from_pixels(2300, 1950 + 50 * k), 128)).collect();
    for e in &enemies {
        w.units[e.0 as usize].no_search_victim = true;
    }
    let (mut melee, mut members_attacking) = (false, 0);
    for _ in 0..1200 {
        w.step(&[]);
        melee |= matches!(w.brigades[0].orders.last(), Some(BOrder::Bitva(_)));
        members_attacking = members_attacking.max(soldiers.iter().filter(|s| u(&w, **s).target().is_some()).count());
    }
    assert!(melee, "the formation started melee");
    assert!(members_attacking >= 2, "several members fought: {members_attacking}");
    assert!(enemies.iter().all(|e| u(&w, *e).death != 0), "the enemies died");
    assert!(!matches!(w.brigades[0].orders.last(), Some(BOrder::Bitva(_))), "the melee ended");
}

const CANNON: &str = "NAME Can\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@MOTION_L 16 0 0 9\r\n\
    @MOTION_R 16 0 10 19\r\n@MOTION_L0 16 0 0 9\r\n@MOTION_R0 16 0 10 19\r\n@MOTION_L1 16 0 0 19\r\n\
    @MOTION_R1 16 0 0 19\r\n@PATTACK4 16 0 0 19\r\n@UATTACK4 16 0 19 0\r\n@ATTACK4 16 0 0 39\r\n\
    SETACTIVEPOINT #ATTACK4 10 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0\r\n\
    LIFE 1500\r\nGEOMETRY 1 10 40\r\nMOTIONSTYLE COMPLEXROTATE\r\nUSAGE PUSHKA\r\nARTPODGOTOVKA\r\n\
    ATTACK_RADIUS 4 300 3000\r\nATTACK_PAUSE 4 100\r\nWEAPON 4 PPOINTT\r\nWEAPONKIND 4 IADRO\r\n\
    CANKILL 1 BODY\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
const NDS: &[u8] = b"[MEMBERS]\r\n\
    POINT #POINT 0 1 NONE HI_GRAVITY1 RANDOM 3 -1\r\n\
    PPOINTT #POINT 0 0 NONE NO_GRAVITY SLIGHTUP 0 1\r\n\
    NUCL2P #NUCL1 340 35 NONE HI_GRAVITY ANGLE 10 -1\r\n\
    [SYNCWEAPON]\r\nPPOINTT 1 NUCL2P\r\n[END]\r\n";
const ADS: &[u8] = b"@POINT 1 0 0 0\r\n@NUCL1 1 0 0 0\r\n";
const NRES: &[u8] = b"[WEAPONS]\r\n#GENSHIELD 13\r\nIADRO 2 15 HX\r\n[UNITKINDS]\r\n";

/// A gun told to attack and then to stand ground (the tactics controller's order pair)
/// shoots a target in range, drops it when it walks out of range (0x5204aa) and does not
/// move: without the drop `AttackObjLink` would drive the gun after the target.
#[test]
fn a_gun_standing_ground_drops_a_target_out_of_range() {
    let mut c = Catalog::new();
    let can = c.add(ObjectType::from_def("Can(EN)", &UnitDef::parse(CANNON.as_bytes())));
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
    assert_eq!(w.catalog.get(can).role.usage, acr_sim::catalog::usage::PUSHKA);
    let g = w.spawn(can, 0, Pos::from_pixels(1000, 2000), 64);
    let t = w.spawn(pik, 1, Pos::from_pixels(2200, 2000), 128);
    w.units[t.0 as usize].no_search_victim = true;
    let start = u(&w, g).pos;
    w.step(&[
        Command::Attack { player: 0, units: vec![g], target: t },
        Command::StandGround { player: 0, units: vec![g], on: true },
    ]);
    assert!(u(&w, g).stand_ground && u(&w, g).target() == Some(t));
    // The target walks away past the gun's reach (`ATTACK_RADIUS 4 300 3000`).
    w.step(&[Command::Move { player: 1, units: vec![t], x: 6000, y: 2000 }]);
    let mut fired = false;
    for _ in 0..700 {
        w.step(&[]);
        fired |= w.projectiles.iter().count() > 0;
    }
    assert!(fired, "the gun fired while the target was in range");
    assert!(u(&w, t).pos.pixels().0 - 1000 > 3000, "the target left the reach");
    assert_eq!(u(&w, g).target(), None, "the target out of range was dropped");
    assert_eq!(u(&w, g).pos, start, "and the gun did not follow it");
    assert!(u(&w, g).stand_ground);
}
