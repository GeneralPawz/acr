//! Looking along the shot before firing (`PredictShot` 0x559030): a musketeer does not fire
//! into a hill but steps aside or gives up. A friend in the line does not stop the shot (ACR
//! skips the engine's friendly-fire look 0x559a40); the friend absorbs the ball unharmed.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::{AnimKind, EntityId};
use acr_sim::{Command, Pos, TypeId, World};

/// A bystander that never attacks by itself.
const BYSTANDER: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    MATHERIAL 1 BODY\r\nUNITRADIUS 8\r\n[END]\r\n";

/// The musketeer of `tests/combat.rs` (with `#PSTAND1`) without dispersion, so every free shot hits.
const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n#PSTAND1 9 1 2 61\r\n\
    @MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n@PATTACK1 9 0 0 62\r\n@UATTACK1 9 0 62 0\r\n@ATTACK1 9 0 0 67\r\n\
    @ATTACK 9 0 0 52\r\n@ATTACK3 9 0 68 101\r\n@TRANSX3 9 0 0 35\r\n@TRANS3X 9 0 35 0\r\nSETACTIVEPOINT #ATTACK1 13 0 0\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 15\r\nDAMAGE 1 380\r\n\
    ATTACK_RADIUS 0 40 80\r\nDET_RADIUS 0 0 250\r\nATTACK_RADIUS 1 250 850\r\nDET_RADIUS 1 250 7000\r\n\
    ADD_ATTACK_RADIUS 0 32\r\nATTACK_PAUSE 0 20\r\nATTACK_PAUSE 1 550\r\nWEAPONKIND 0 MECH\r\nWEAPONKIND 1 VISTREL\r\n\
    WEAPON 1 SHOT\r\nDAMAGEDEC 1 300\r\nNOPAUSEDATTACK 0\r\nSLOWRECHARGE\r\nUSETRANSX 0\r\n\
    SEARCH_ENEMY_RADIUS 850\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n\
    PROTECTION 3 PIKA 1 VISTREL 10 MECH 1\r\nZPOINTS 33 33\r\nUNITRADIUS 8\r\nFRIENDLYFIRE\r\n[END]\r\n";

const NRES: &[u8] =
    b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nSTRELA 1 14 HRB\r\nPIKA 11 17 O\r\nIADRO 2 15 HX\r\n\
    VISTREL 12 18 HRB\r\n[UNITKINDS]\r\n";

struct Kinds {
    mus: TypeId,
    bystander: TypeId,
}

fn setup(map: MapGrid) -> (World, Kinds) {
    let mut c = Catalog::new();
    let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    let bystander = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(BYSTANDER.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let w = World::new(Arc::new(map), Arc::new(c), Arc::new(Formations::default()));
    (w, Kinds { mus, bystander })
}

fn unit(w: &World, id: EntityId) -> &acr_sim::Unit {
    &w.units[id.0 as usize]
}

/// Runs until the shooter's reload starts (its shot) or `ticks` pass; returns the tick of
/// the shot.
fn run_until_shot(w: &mut World, shooter: EntityId, ticks: usize) -> Option<usize> {
    for t in 1..=ticks {
        w.step(&[]);
        if unit(w, shooter).delay > 100 {
            return Some(t);
        }
    }
    None
}

/// A friend 200 px ahead on the line of fire: the musketeer fires from where it stands and
/// the friend absorbs the ball unharmed (`FRIENDLYFIRE`).
#[test]
fn a_friend_in_the_line_absorbs_the_ball() {
    let (mut w, k) = setup(MapGrid::open(1));
    let shooter = w.spawn(k.mus, 0, Pos::from_pixels(2000, 2000), 0);
    let friend = w.spawn(k.bystander, 0, Pos::from_pixels(2200, 2000), 0);
    let enemy = w.spawn(k.bystander, 1, Pos::from_pixels(2500, 2000), 128);
    w.step(&[Command::Attack { player: 0, units: vec![shooter], target: enemy }]);
    run_until_shot(&mut w, shooter, 300).expect("the musketeer fired");
    assert_eq!(unit(&w, shooter).pos, Pos::from_pixels(2000, 2000), "fired where it stood");
    for _ in 0..20 {
        w.step(&[]);
    }
    assert_eq!(unit(&w, friend).life, 100, "the friend is unhurt");
    assert_eq!(unit(&w, enemy).life, 100, "the friend took the ball");
}

/// Already standing ready in the musket stance (`#PSTAND1`, one frame: frame 0) with a
/// friend in the line, the musketeer still fires (the engine's 0x51fb8d would drop the
/// attack here, again and again: #194).
#[test]
fn a_ready_shooter_behind_a_friend_fires() {
    let (mut w, k) = setup(MapGrid::open(1));
    let shooter = w.spawn(k.mus, 0, Pos::from_pixels(2000, 2000), 0);
    w.spawn(k.bystander, 0, Pos::from_pixels(2200, 2000), 0);
    let enemy = w.spawn(k.bystander, 1, Pos::from_pixels(2500, 2000), 128);
    {
        let u = &mut w.units[shooter.0 as usize];
        (u.stance, u.local_stance) = (2, 2);
        (u.anim, u.anim_frames, u.frame) = (AnimKind::PStand(1), 1, 0);
    }
    w.step(&[Command::Attack { player: 0, units: vec![shooter], target: enemy }]);
    assert!(!unit(&w, shooter).orders.is_empty(), "the attack stays");
    run_until_shot(&mut w, shooter, 200).expect("the musketeer fired");
    assert_eq!(unit(&w, shooter).pos, Pos::from_pixels(2000, 2000), "without moving");
}

/// A ridge 300 px high between shooter and target blocks the musket's line (0x558eb0: the
/// ground above the line). No place to either side sees past it, so the attack is dropped
/// with a reload of 20 frames (0x51fff1) and the enemy is never shot.
#[test]
fn a_ridge_stops_the_musket() {
    let mut map = MapGrid::open(1);
    let stride = map.heights.stride;
    for row in 0..map.heights.cells.len() / stride {
        for col in 70..=72 {
            map.heights.cells[row * stride + col] = 300;
        }
    }
    let (mut w, k) = setup(map);
    let shooter = w.spawn(k.mus, 0, Pos::from_pixels(2000, 2000), 0);
    let enemy = w.spawn(k.bystander, 1, Pos::from_pixels(2500, 2000), 128);
    w.step(&[Command::Attack { player: 0, units: vec![shooter], target: enemy }]);
    let mut saw_short_reload = false;
    for _ in 0..300 {
        w.step(&[]);
        let u = unit(&w, shooter);
        assert!(u.delay <= 20, "no shot: reload {}", u.delay);
        saw_short_reload |= u.delay == 20 && u.max_delay == 20;
    }
    assert!(saw_short_reload, "the blocked strike set the 20-frame reload");
    assert_eq!(unit(&w, enemy).life, 100);
}

/// Reloaded and standing ready in `#PSTAND1` without orders, the musketeer finds an enemy
/// 490 px away by itself (`SearchVictim` 0x52622b) and fires within the first strike (#194).
#[test]
fn a_ready_shooter_without_orders_finds_and_fires() {
    let (mut w, k) = setup(MapGrid::open(1));
    let shooter = w.spawn(k.mus, 0, Pos::from_pixels(2000, 2000), 0);
    w.spawn(k.bystander, 1, Pos::from_pixels(2490, 2000), 128);
    {
        let u = &mut w.units[shooter.0 as usize];
        (u.stance, u.local_stance) = (2, 2);
        (u.anim, u.anim_frames, u.frame) = (AnimKind::PStand(1), 1, 0);
    }
    let t = run_until_shot(&mut w, shooter, 100).expect("the musketeer fired");
    assert!(t <= 40, "fired at tick {t}");
}
