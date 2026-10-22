//! Hit locations (#225): the optional rule and that the original rules stay as they were.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::wounds::HitRegion;
use acr_sim::{Command, Pos, Rules, TypeId, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 10\r\n\
    ATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nSEARCH_ENEMY_RADIUS 700\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n\
    DET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\nWEAPONKIND 0 PIKA\r\nPROTECTION 1 PIKA 3\r\n\
    UNITRADIUS 8\r\n[END]\r\n";

const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK1 9 0 0 62\r\n@UATTACK1 9 0 62 0\r\n@ATTACK1 9 0 0 67\r\n@ATTACK 9 0 0 52\r\n\
    @ATTACK3 9 0 68 101\r\n@TRANSX3 9 0 0 35\r\n@TRANS3X 9 0 35 0\r\nSETACTIVEPOINT #ATTACK1 13 0 0\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 15\r\nDAMAGE 1 380\r\n\
    ATTACK_RADIUS 0 40 80\r\nDET_RADIUS 0 0 250\r\nATTACK_RADIUS 1 250 850\r\nDET_RADIUS 1 250 7000\r\n\
    ADD_ATTACK_RADIUS 0 32\r\nATTACK_PAUSE 0 20\r\nATTACK_PAUSE 1 550\r\nWEAPONKIND 0 MECH\r\nWEAPONKIND 1 VISTREL\r\n\
    WEAPON 1 SHOT\r\nDAMAGEDEC 1 300\r\nRAZBROS 300\r\nNOPAUSEDATTACK 0\r\nSLOWRECHARGE\r\nUSETRANSX 0\r\n\
    SEARCH_ENEMY_RADIUS 850\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n\
    PROTECTION 3 PIKA 1 VISTREL 10 MECH 1\r\nZPOINTS 33 33\r\nUNITRADIUS 8\r\nFRIENDLYFIRE\r\n[END]\r\n";

const NRES: &[u8] =
    b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nSTRELA 1 14 HRB\r\nPIKA 11 17 O\r\nIADRO 2 15 HX\r\n\
    VISTREL 12 18 HRB\r\n[UNITKINDS]\r\n";

fn setup() -> (World, TypeId, TypeId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), pik, mus)
}

/// Ten pikemen and musketeers on each side, fighting for `ticks` ticks.
fn fight(rules: Rules, ticks: u32) -> World {
    let (mut w, pik, mus) = setup();
    w.rules = rules;
    for k in 0..10 {
        w.spawn(if k % 2 == 0 { pik } else { mus }, 0, Pos::from_pixels(1800 + 30 * k, 2000), 0);
        w.spawn(if k % 2 == 0 { mus } else { pik }, 1, Pos::from_pixels(1800 + 30 * k, 2400), 192);
    }
    for _ in 0..ticks {
        w.step(&[]);
    }
    w
}

const ON: Rules = Rules { hit_locations: true };

#[test]
fn off_by_default_the_fight_is_the_original_one() {
    let w = fight(Rules::default(), 1500);
    // The hash of this fight before hit locations existed (measured on main at 71552fd).
    // Update it when the original rules change, never for the optional rule.
    assert_eq!(w.state_hash(), 0x96cd_7315_0624_4035);
    assert!(w.units.iter().all(|u| u.wounds == Default::default()));
    assert!(w.hits.iter().all(|h| h.region.is_none()));
}

#[test]
fn with_hit_locations_hits_land_on_regions_deterministically() {
    // Hits stay in `World::hits` for 750 ticks: look while the fight goes on.
    let a = fight(ON, 600);
    let b = fight(ON, 600);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.units, b.units);
    assert_ne!(a.state_hash(), fight(Rules::default(), 600).state_hash());
    let regions: Vec<_> = a.hits.iter().filter(|h| !h.building).map(|h| h.region).collect();
    assert!(!regions.is_empty() && regions.iter().all(Option::is_some), "{regions:?}");
    assert!(regions.contains(&Some(HitRegion::Torso)));
    // Survivors carry the wounds of their leg and arm hits.
    assert!(a.units.iter().any(|u| u.wounds.legs + u.wounds.arms > 0));
}

/// Pixels walked east in 120 ticks.
fn walked(legs: u8) -> i32 {
    let (mut w, pik, _) = setup();
    w.rules = ON;
    let p = w.spawn(pik, 0, Pos::from_pixels(1000, 2000), 0);
    w.units[p.0 as usize].wounds.legs = legs;
    w.step(&[Command::Move { player: 0, units: vec![p], x: 4000, y: 2000 }]);
    for _ in 0..120 {
        w.step(&[]);
    }
    w.units[p.0 as usize].pos.pixels().0 - 1000
}

#[test]
fn a_leg_wound_slows_the_walk() {
    let (sound, one, two, many) = (walked(0), walked(1), walked(2), walked(5));
    assert!(sound > 200, "walked {sound}");
    assert!(one < sound * 7 / 10 && one > sound / 2, "{one} of {sound}");
    assert!(two < one && many < two && many >= sound / 4, "{two} {many} of {sound}");
}

fn musket_duel(arms: u8) -> (World, EntityId, EntityId) {
    let (mut w, pik, mus) = setup();
    w.rules = ON;
    let shooter = w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 0);
    let target = w.spawn(pik, 1, Pos::from_pixels(2300, 2000), 128);
    w.units[shooter.0 as usize].wounds.arms = arms;
    w.step(&[Command::Attack { player: 0, units: vec![shooter], target }]);
    for _ in 0..60 {
        w.step(&[Command::Stop { player: 1, units: vec![target] }]);
    }
    (w, shooter, target)
}

#[test]
fn an_arm_wound_stops_the_musket() {
    // Unwounded, the musketeer fires within 45 ticks (see the combat tests).
    let (w, shooter, _) = musket_duel(0);
    assert!(w.units[shooter.0 as usize].delay > 0 || !w.hits.is_empty(), "the sound musketeer fired");
    // With a wounded arm it never fires: it walks up to strike with the bayonet.
    let (w, shooter, target) = musket_duel(1);
    let s = &w.units[shooter.0 as usize];
    assert!(w.projectiles.iter().next().is_none());
    assert!(w.hits.iter().all(|h| h.cause != acr_sim::combat::DeathCause::Gunshot));
    assert!(s.pos.pixels().0 > 2100, "walked towards the target: {:?}", s.pos.pixels());
    assert!(w.units[target.0 as usize].living());
}

/// Lost limbs (#254): a soldier without a leg crawls far slower than a wounded one, every
/// lost limb bleeds a little life away each second, and he bleeds out with his own cause.
#[test]
fn lost_legs_crawl_and_bleed_out() {
    use acr_sim::combat::DeathCause;
    let crawl = |lost: u8| {
        let (mut w, pik, _) = setup();
        w.rules = ON;
        let p = w.spawn(pik, 0, Pos::from_pixels(1000, 2000), 0);
        w.units[p.0 as usize].wounds.legs_lost = lost;
        w.units[p.0 as usize].life = 10_000;
        w.step(&[Command::Move { player: 0, units: vec![p], x: 4000, y: 2000 }]);
        for _ in 0..120 {
            w.step(&[]);
        }
        w.units[p.0 as usize].pos.pixels().0 - 1000
    };
    let (sound, one, both) = (walked(0), crawl(1), crawl(2));
    assert!(one > 0 && one < sound / 5, "crawled {one} of {sound}");
    assert!(both < one, "{both} {one}");

    let (mut w, pik, _) = setup();
    w.rules = ON;
    let p = w.spawn(pik, 0, Pos::from_pixels(1000, 2000), 0);
    w.units[p.0 as usize].wounds.legs_lost = 1;
    let life = w.units[p.0 as usize].life;
    for _ in 0..20 {
        w.step(&[]);
    }
    let after = w.units[p.0 as usize].life;
    assert!(after < life && after >= life - 3 * 4, "life {life} → {after}");
    for _ in 0..2000 {
        w.step(&[]);
    }
    assert!(w.units[p.0 as usize].death > 0, "bled out");
    assert_eq!(w.deaths[&p.0].cause, DeathCause::BledOut);
}

/// Heavy blows take limbs off: cannonballs always, a musket ball rarely.
#[test]
fn heavy_blows_sever_limbs() {
    use acr_sim::combat::DeathCause;
    let (mut w, pik, _) = setup();
    w.rules = ON;
    let p = w.spawn(pik, 0, Pos::from_pixels(1000, 2000), 0);
    acr_sim::wounds::wound(&mut w, p.0 as usize, HitRegion::Legs, DeathCause::Cannonball);
    assert_eq!(w.units[p.0 as usize].wounds.legs_lost, 1);
    let mut severed = 0;
    for _ in 0..200 {
        let before = w.units[p.0 as usize].wounds.arms_lost;
        acr_sim::wounds::wound(&mut w, p.0 as usize, HitRegion::Arms, DeathCause::Gunshot);
        severed += usize::from(w.units[p.0 as usize].wounds.arms_lost > before);
        w.units[p.0 as usize].wounds.arms_lost = 0;
    }
    assert!((5..=40).contains(&severed), "gunshot severed {severed} of 200");
}
