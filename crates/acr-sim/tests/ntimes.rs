//! `nTimes` of an attack on an object (`AttackObj` 0x51a930's last argument, order+0xf):
//! the attack ends after that many shots or strikes (0x51ea72, 0x51ec30).

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::combat::{Placement, attack_obj_times};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::{EntityId, Order};
use acr_sim::{Pos, TypeId, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 10\r\n\
    ATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nDET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n\
    WEAPONKIND 0 PIKA\r\nUNITRADIUS 8\r\n[END]\r\n";

/// A tough target that never fights back.
const POST: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 10000\r\nGEOMETRY 1 10 40\r\n\
    MATHERIAL 1 BODY\r\nUNITRADIUS 8\r\n[END]\r\n";

const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK1 9 0 0 62\r\n@UATTACK1 9 0 62 0\r\n@ATTACK1 9 0 0 67\r\n@ATTACK 9 0 0 52\r\n\
    @ATTACK3 9 0 68 101\r\n@TRANSX3 9 0 0 35\r\n@TRANS3X 9 0 35 0\r\nSETACTIVEPOINT #ATTACK1 13 0 0\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 15\r\nDAMAGE 1 380\r\n\
    ATTACK_RADIUS 0 40 80\r\nDET_RADIUS 0 0 250\r\nATTACK_RADIUS 1 250 850\r\nDET_RADIUS 1 250 7000\r\n\
    ADD_ATTACK_RADIUS 0 32\r\nATTACK_PAUSE 0 20\r\nATTACK_PAUSE 1 550\r\nWEAPONKIND 0 MECH\r\nWEAPONKIND 1 VISTREL\r\n\
    WEAPON 1 SHOT\r\nDAMAGEDEC 1 300\r\nNOPAUSEDATTACK 0\r\nSLOWRECHARGE\r\nUSETRANSX 0\r\n\
    MATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\nZPOINTS 33 33\r\nUNITRADIUS 8\r\n[END]\r\n";

const NRES: &[u8] = b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nPIKA 11 17 O\r\nVISTREL 12 18 HB\r\n[UNITKINDS]\r\n";

fn setup() -> (World, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    let post = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(POST.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), pik, mus, post)
}

fn attack_times(w: &World, id: EntityId) -> Option<u16> {
    match w.units[id.0 as usize].orders.last() {
        Some(Order::Attack { times, .. }) => Some(*times),
        _ => None,
    }
}

/// Steps until the unit's reload starts (a shot or a strike); returns the tick.
fn until_reload(w: &mut World, id: EntityId, ticks: usize) -> Option<usize> {
    (1..=ticks).find(|_| {
        let before = w.units[id.0 as usize].delay;
        w.step(&[]);
        w.units[id.0 as usize].delay > before
    })
}

/// Two shots: after the first the order counts 1, after the second it is gone although the
/// target lives (0x51ea72).
#[test]
fn a_musketeer_fires_the_ordered_number_of_shots() {
    let (mut w, _, mus, post) = setup();
    let s = w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 0);
    let t = w.spawn(post, 1, Pos::from_pixels(2400, 2000), 128);
    assert!(attack_obj_times(&mut w, s.0 as usize, t.0 as usize, 0x90, Placement::Replace, 2));
    assert_eq!(attack_times(&w, s), Some(2));
    until_reload(&mut w, s, 200).expect("first shot");
    assert_eq!(attack_times(&w, s), Some(1), "one shot left");
    until_reload(&mut w, s, 600).expect("second shot");
    assert_eq!(attack_times(&w, s), None, "the attack ended: {:?}", w.units[s.0 as usize].orders);
    let target = &w.units[t.0 as usize];
    assert!(target.life < 10_000 && target.death == 0, "hit twice, alive: {}", target.life);
}

/// One strike: a pikeman's attack with `nTimes` 1 ends with its first hit (0x51ec30), and
/// switching the target of the running attack keeps the count.
#[test]
fn a_pikeman_strikes_once() {
    let (mut w, pik, _, post) = setup();
    let p = w.spawn(pik, 0, Pos::from_pixels(2000, 2000), 0);
    let a = w.spawn(post, 1, Pos::from_pixels(2060, 2000), 128);
    let b = w.spawn(post, 1, Pos::from_pixels(2000, 2060), 128);
    assert!(attack_obj_times(&mut w, p.0 as usize, a.0 as usize, 0x90, Placement::Replace, 1));
    assert!(attack_obj_times(&mut w, p.0 as usize, b.0 as usize, 0x90, Placement::Replace, 0));
    assert_eq!(attack_times(&w, p), Some(1), "the count stays with the order");
    for _ in 0..200 {
        w.step(&[]);
        if w.units[b.0 as usize].life < 10_000 {
            break;
        }
    }
    assert!(w.units[b.0 as usize].life < 10_000, "struck");
    assert_eq!(attack_times(&w, p), None, "one strike, then the attack ends");
    w.step(&[]);
    assert_eq!(w.units[b.0 as usize].life, 10_000 - 10, "exactly one blow");
}
