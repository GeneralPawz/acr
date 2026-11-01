//! Hit locations (#225): the optional rule and that the original rules stay as they were.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::body::{Fracture, FractureKind, LimbId};
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

const ON: Rules = Rules {
    hit_locations: true,
    body_spacing: false,
    mortars: false,
    corpse_piles: false,
    group_move: false,
    cannon_fire: false,
    ball_terrain: false,
    formation_attack: false,
    animal_give_up: false,
    dart_fire: false,
    aimed_fire: false,
};

#[test]
fn off_by_default_the_fight_is_the_original_one() {
    let w = fight(Rules::default(), 1500);
    // The hash of this fight without the optional rule (measured on main at 71552fd, then
    // with the original's look along the line before a shot, PredictShot, then without its
    // friendly-fire look, #194).
    // Update it when the original rules change or the hashed player state gains a field
    // (#437), never for the optional rule.
    assert_eq!(w.state_hash(), 0x4289_426b_25f7_1e80);
    assert!(w.units.iter().all(|u| u.body == Default::default()));
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
    // Survivors carry the injuries of their hits (#262).
    assert!(a.units.iter().any(|u| u.living() && u.body != Default::default()));
    assert!(a.hits.iter().filter(|h| !h.building).all(|h| h.injury.is_some()));
}

/// Pixels walked east in 120 ticks with `legs` flesh wounds on the left leg.
fn walked(legs: u8) -> i32 {
    let (mut w, pik, _) = setup();
    w.rules = ON;
    let p = w.spawn(pik, 0, Pos::from_pixels(1000, 2000), 0);
    w.units[p.0 as usize].body.limbs[LimbId::LegL as usize].wounds = legs;
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
    // 65 % and 30 % of the pace on one wounded leg (`docs/damage-model.md`), no slower.
    assert!(two < one && many <= two && many >= sound / 4, "{two} {many} of {sound}");
}

fn musket_duel(broken_arm: bool) -> (World, EntityId, EntityId) {
    let (mut w, pik, mus) = setup();
    w.rules = ON;
    let shooter = w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 0);
    let target = w.spawn(pik, 1, Pos::from_pixels(2300, 2000), 128);
    if broken_arm {
        let f = Fracture { level: 60, kind: FractureKind::Closed, bend: 10 };
        w.units[shooter.0 as usize].body.limbs[LimbId::ArmL as usize].fracture = Some(f);
    }
    w.step(&[Command::Attack { player: 0, units: vec![shooter], target }]);
    for _ in 0..60 {
        w.step(&[Command::Stop { player: 1, units: vec![target] }]);
    }
    (w, shooter, target)
}

#[test]
fn an_arm_wound_stops_the_musket() {
    // Unwounded, the musketeer fires within 45 ticks (see the combat tests).
    let (w, shooter, _) = musket_duel(false);
    assert!(w.units[shooter.0 as usize].delay > 0 || !w.hits.is_empty(), "the sound musketeer fired");
    // With a broken arm it never fires: it walks up to strike with the bayonet.
    let (w, shooter, target) = musket_duel(true);
    let s = &w.units[shooter.0 as usize];
    assert!(w.projectiles.iter().next().is_none());
    assert!(w.hits.iter().all(|h| h.cause != acr_sim::combat::DeathCause::Gunshot));
    assert!(s.pos.pixels().0 > 2100, "walked towards the target: {:?}", s.pos.pixels());
    assert!(w.units[target.0 as usize].living());
}

/// Lost limbs (#254, #262): a soldier without a leg crawls far slower than a wounded one,
/// slower still with one arm, every stump bleeds life away each second, and he bleeds out
/// with his own cause.
#[test]
fn lost_legs_crawl_and_bleed_out() {
    use acr_sim::combat::DeathCause;
    let crawl = |arms: usize| {
        let (mut w, pik, _) = setup();
        w.rules = ON;
        let p = w.spawn(pik, 0, Pos::from_pixels(1000, 2000), 0);
        let u = &mut w.units[p.0 as usize];
        u.body.limbs[LimbId::LegL as usize].lost = 60;
        if arms < 2 {
            u.body.limbs[LimbId::ArmR as usize].lost = 150;
        }
        u.life = 10_000;
        // Out of the fight he takes no orders: send him as the simulation would.
        acr_sim::motion::send_to(u, Pos::from_pixels(4000, 2000), acr_sim::motion::PLAYER_PRIO, true);
        for _ in 0..120 {
            w.step(&[]);
        }
        w.units[p.0 as usize].pos.pixels().0 - 1000
    };
    let (sound, two, one) = (walked(0), crawl(2), crawl(1));
    assert!(two > 0 && two < sound / 5, "crawled {two} of {sound}");
    assert!(one > 0 && one < two, "{one} {two}");

    let (mut w, pik, _) = setup();
    w.rules = ON;
    let p = w.spawn(pik, 0, Pos::from_pixels(1000, 2000), 0);
    w.units[p.0 as usize].body.limbs[LimbId::LegR as usize].lost = 60;
    let life = w.units[p.0 as usize].life;
    for _ in 0..20 {
        w.step(&[]);
    }
    let after = w.units[p.0 as usize].life;
    assert!(after < life && after >= life - 3 * 6, "life {life} → {after}");
    for _ in 0..2000 {
        w.step(&[]);
    }
    assert!(w.units[p.0 as usize].death > 0, "bled out");
    assert_eq!(w.deaths[&p.0].cause, DeathCause::BledOut);
}

/// Strikes fresh pikemen (spawned one after another, so each strike draws on) with `hit`
/// until the limb comes off; the world and the man, or `None` within 40 tries.
fn sever(rules: Rules, limb: LimbId, hit: acr_sim::combat::ScriptedHit) -> Option<(World, usize)> {
    let (mut w, pik, _) = setup();
    w.rules = rules;
    for k in 0..40 {
        let p = w.spawn(pik, 0, Pos::from_pixels(1000 + 40 * k, 2000), 0).0 as usize;
        acr_sim::combat::scripted_hit(&mut w, p, hit);
        if w.units[p].body.limb(limb).severed() && w.units[p].living() {
            return Some((w, p));
        }
    }
    None
}

fn aimed(cause: acr_sim::combat::DeathCause, limb: LimbId, level: u8, energy: u8) -> acr_sim::combat::ScriptedHit {
    acr_sim::combat::ScriptedHit {
        cause,
        mounted: cause == acr_sim::combat::DeathCause::Melee,
        region: None,
        dir: (10, 0),
        energy,
        dmg: 1,
        aim: Some((acr_sim::body::Part::Limb(limb), Some(level))),
        maul: None,
    }
}

/// A spent cannonball (#254) takes the leg off above the knee: the man falls and crawls
/// (the renderer's crawl state is `Body::crawling`), is out of the fight, bleeds the
/// stump's 6 life every second and dies "bled out" after exactly ⌈life / 6⌉ seconds.
#[test]
fn a_severed_leg_crawls_and_bleeds_out_in_whole_seconds() {
    use acr_sim::combat::DeathCause;
    let run = || {
        let (mut w, p) =
            sever(ON, LimbId::LegL, aimed(DeathCause::Cannonball, LimbId::LegL, 60, 30)).expect("a ball severs");
        let b = w.units[p].body;
        assert!(b.limb(LimbId::LegL).lost == 60 && b.crawling() && b.out_of_fight());
        assert_eq!(b.bleeding(), acr_sim::body::BLEED_ABOVE);
        let (life, rate) = (w.units[p].life, b.bleeding());
        let mut seconds = 0;
        let mut last = life;
        for _ in 0..2000 {
            w.step(&[]);
            let u = &w.units[p];
            if !u.living() {
                seconds += 1;
                break;
            }
            if u.life != last {
                assert_eq!(last - u.life, rate, "one stump's loss per second");
                assert!(u.body.crawling());
                last = u.life;
                seconds += 1;
            }
        }
        assert_eq!(seconds, (life + rate - 1) / rate, "bled out after ⌈{life} / {rate}⌉ seconds");
        assert_eq!(w.deaths[&(p as u32)].cause, DeathCause::BledOut);
        w.state_hash()
    };
    assert_eq!(run(), run(), "deterministic");
}

/// A rider's sabre (by chance) takes the arm off: he bleeds slower than from a leg and,
/// one arm short, drops his musket and never fires (he still stands and fights).
#[test]
fn a_severed_arm_bleeds_slower_and_never_fires() {
    use acr_sim::combat::DeathCause;
    let (w, p) = sever(ON, LimbId::ArmR, aimed(DeathCause::Melee, LimbId::ArmR, 60, 80)).expect("a sabre severs");
    let b = w.units[p].body;
    assert!(b.bleeding() > 0 && b.bleeding() < acr_sim::body::BLEED_ABOVE, "{}", b.bleeding());
    assert!(b.disarmed() && !b.crawling());

    let (mut w, pik, mus) = setup();
    w.rules = ON;
    let shooter = w.spawn(mus, 0, Pos::from_pixels(2000, 2000), 0);
    let target = w.spawn(pik, 1, Pos::from_pixels(2300, 2000), 128);
    w.units[shooter.0 as usize].body.limbs[LimbId::ArmL as usize].lost = 150;
    w.units[shooter.0 as usize].life = 10_000;
    w.step(&[Command::Attack { player: 0, units: vec![shooter], target }]);
    for _ in 0..60 {
        w.step(&[Command::Stop { player: 1, units: vec![target] }]);
    }
    assert!(w.projectiles.iter().next().is_none());
    assert!(w.hits.iter().all(|h| h.cause != DeathCause::Gunshot));
}

/// The same scripted strikes with the rule off: no region, no injury, no bleeding, and the
/// state hash of the original rules.
#[test]
fn without_the_rule_a_ball_severs_nothing() {
    use acr_sim::combat::DeathCause;
    let mut hit = aimed(DeathCause::Cannonball, LimbId::LegL, 60, 30);
    hit.aim = None;
    assert!(sever(Rules::default(), LimbId::LegL, hit).is_none());
}
