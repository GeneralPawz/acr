//! A low cannonball ploughing through a file of men (#256): the original hurts everyone
//! near it every tick at full damage; with the hit-locations rule each man is hit once and
//! the ball loses energy with every body.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::combat::DeathCause;
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::projectile::{PLOUGH_BODIES, create};
use acr_sim::weapon::Weapons;
use acr_sim::world::EntityId;
use acr_sim::{Pos, Rules, World};

const GUNNER: &str = "NAME Gun\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@DEATH 16 0 0 9\r\n\
    LIFE 1000\r\nGEOMETRY 1 10 40\r\nDAMAGE 1 340\r\nATTACK_RADIUS 1 100 2000\r\nDET_RADIUS 1 100 7000\r\n\
    ATTACK_PAUSE 1 400\r\nWEAPONKIND 1 IADRO\r\nWEAPON 1 ROLL\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@DEATH 9 0 0 9\r\n\
    #DEATHLIE1 9 1 0 9\r\nLIFE 300\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\nUNITRADIUS 8\r\n[END]\r\n";

/// A ball rolling level 10 px above the ground (no gravity), low enough to hurt
/// (`DAMAGEHEIGHT` 95), with the damage of a bouncing ball (`SKOKAT`, 500 within 16 px).
const NDS: &[u8] = b"[MEMBERS]\r\n\
    ROLL #NUCL1 500 16 NONE NO_GRAVITY FLY 16 -1\r\n\
    [DAMAGEHEIGHT]\r\nROLL 95\r\n[END]\r\n";
const ADS: &[u8] = b"@NUCL1 1 0 0 0\r\n";
const NRES: &[u8] = b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nIADRO 2 15 HX\r\n[UNITKINDS]\r\n";

/// Seven men in a file 20 px apart, a ball fired along it; returns the world after the ball
/// has passed, the men and the gun.
fn file_shot(rules: Rules) -> (World, Vec<EntityId>, EntityId) {
    let mut c = Catalog::new();
    let gun = c.add(ObjectType::from_def("Gun(EN)", &UnitDef::parse(GUNNER.as_bytes())));
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    let g = w.spawn(gun, 0, Pos::from_pixels(1500, 2000), 0);
    let men: Vec<_> = (0..7).map(|k| w.spawn(sol, 1, Pos::from_pixels(2000 + 20 * k, 2000), 128)).collect();
    w.step(&[]);
    let roll = w.catalog.weapons.list.iter().position(|wp| wp.name == "ROLL").unwrap() as u16;
    assert!(create(&mut w, roll, (1900, 2000, 10), (2600, 2000, 10), Some(g.0 as usize), 1, None, (0, 0, 0), 0));
    for _ in 0..40 {
        w.step(&[]);
    }
    assert_eq!(w.projectiles.iter().count(), 0, "the ball has landed");
    (w, men, g)
}

fn life(w: &World, id: EntityId) -> i32 {
    w.units[id.0 as usize].life
}

fn dead(w: &World, id: EntityId) -> bool {
    !w.units[id.0 as usize].living()
}

#[test]
fn originally_a_low_ball_kills_the_whole_file() {
    let (w, men, _) = file_shot(Rules::default());
    assert!(men.iter().all(|&m| dead(&w, m)), "{:?}", men.iter().map(|&m| life(&w, m)).collect::<Vec<_>>());
    assert!(w.deaths.values().all(|d| d.cause == DeathCause::Cannonball));
    // Every man is hurt again on each tick the ball is near him, at full damage.
    let hits = w.hits.iter().filter(|h| men.iter().any(|m| m.0 == h.victim)).count();
    assert!(hits >= men.len(), "{hits} hits");
    assert!(w.hits.iter().all(|h| h.region.is_none()));
}

#[test]
fn with_hit_locations_the_ball_ploughs_through_five_men_losing_energy() {
    let on = Rules { hit_locations: true, body_spacing: false };
    let (w, men, _) = file_shot(on);
    let lives: Vec<_> = men.iter().map(|&m| (life(&w, m), dead(&w, m))).collect();
    // 500, 400 and 300 kill the first three (life 300); the fourth takes 200, but the ball
    // still has 40 % of its energy and tears him apart (`body::BALL_TEARS`); the fifth (100,
    // less the shield of 1) is wounded by the spent ball; then the last two are untouched.
    assert_eq!(PLOUGH_BODIES, 5);
    assert!(lives[..4].iter().all(|&(_, d)| d), "{lives:?}");
    // A direct hit tears each of them apart, whichever region it struck: nobody tumbles
    // over in one piece (#262).
    let torn: Vec<_> = men[..4].iter().map(|m| w.deaths.get(&m.0).and_then(|d| d.torn)).collect();
    assert!(torn.iter().all(Option::is_some), "{torn:?}");
    // The wounded may have lost the limb the ball struck and bleed (#254), so at most that.
    let fifth = lives[4];
    assert!(!fifth.1 && (100..=199).contains(&fifth.0), "{lives:?}");
    assert_eq!(lives[5..], [(300, false), (300, false)], "{lives:?}");
    // Each man is hit by the ball once, along the flight; the bones of the men it tore
    // apart fly on as fragments (`shrapnel::BONES`, a shell's cause).
    let ball = |h: &&acr_sim::combat::HitRecord| h.cause == DeathCause::Cannonball;
    let hit: Vec<u32> = w.hits.iter().filter(ball).map(|h| h.victim).collect();
    let want: Vec<u32> = men[..5].iter().map(|m| m.0).collect();
    assert_eq!(hit, want);
    assert!(w.hits.iter().filter(ball).all(|h| h.region.is_some() && h.dir.0 > 0));
    assert!(w.hits.iter().all(|h| matches!(h.cause, DeathCause::Cannonball | DeathCause::Explosion)));
    assert!(w.fragments.iter().any(|f| f.kind == acr_sim::shrapnel::Kind::Bone));
    // The same shot again plays the same.
    let (w2, _, _) = file_shot(on);
    assert_eq!(w.state_hash(), w2.state_hash());
}

#[test]
fn the_rule_off_keeps_the_original_hash() {
    let (w, _, _) = file_shot(Rules::default());
    // Pinned on the original rules (before #256): update it when the original rules change,
    // never for the optional rule.
    assert_eq!(w.state_hash(), 0x6bbc_90f6_d730_18e6, "hash {:#x}", w.state_hash());
}
