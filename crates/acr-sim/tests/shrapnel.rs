//! Shell fragments (rule `hit_locations`, `acr_sim::shrapnel`): a bursting shell throws
//! fragments that strike men around the burst through the hit-location model; without the
//! rule nothing changes.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::weapon::Weapons;
use acr_sim::{Command, Pos, Rules, TypeId, World};

const MORTAR: &str = "NAME Mor\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@MOTION_L 16 0 0 9\r\n\
    @MOTION_R 16 0 10 19\r\n@ATTACK1 16 0 0 19\r\n\
    @PATTACK1 16 0 0 9\r\n@DEATH 16 0 0 9\r\nLIFE 1000\r\nGEOMETRY 1 10 40\r\nDAMAGE 1 340\r\n\
    ATTACK_RADIUS 1 100 2000\r\nDET_RADIUS 1 100 7000\r\nATTACK_PAUSE 1 400\r\nWEAPONKIND 1 IADROMOR\r\n\
    WEAPON 1 PPOINTT\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const TARGET: &str = "NAME Tgt\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@DEATH 9 0 0 9\r\n\
    #DEATHLIE1 9 1 0 9\r\nLIFE 1000\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\nUNITRADIUS 16\r\n[END]\r\n";

const SOLDIER: &str = "NAME Sol
USERLC 0 G SHADOW 0 0
#STAND 9 1 0 0
@DEATH 9 0 0 9
    #DEATHLIE1 9 1 0 9
LIFE 60
GEOMETRY 1 10 40
MATHERIAL 1 BODY
UNITRADIUS 8
[END]
";

const NDS: &[u8] = b"[MEMBERS]\r\n\
    STRELA #STRELA 25 15 NONE HI_GRAVITY FLY 25 -1\r\n\
    POINT #POINT 0 1 NONE HI_GRAVITY1 RANDOM 3 -1\r\n\
    PPOINTT #POINT 0 0 NONE NO_GRAVITY SLIGHTUP 0 1\r\n\
    DIM32_2 #DIM32 1 20 WHITE NO_GRAVITY SLIGHTUP 0 1\r\n\
    NUCL2P #NUCL1 340 35 NONE HI_GRAVITY ANGLE 10 -1\r\n\
    SKOKAT #NUCL1 500 16 NONE HI_GRAVITY REFLECT 32 -1\r\n\
    [CHILDWEAPON]\r\nNUCL2P 1 1 0 4 SKOKAT SKOKAT SKOKAT SKOKAT\r\n\
    [FULLPARENT]\r\nSTRELA\r\n\
    [CUSTOMEXPLOSION]\r\nSTRELA FLASH 1 1 0 1 POINT\r\n\
    [SYNCWEAPON]\r\nPPOINTT 2 DIM32_2 NUCL2P\r\n\
    [SOUND]\r\nSTRELA STRELA1\r\nDIM32_2 PUSVIS1\r\nSKOKAT KERNEL1\r\n\
    [DAMAGEHEIGHT]\r\nSKOKAT 95\r\n[END]\r\n";
const ADS: &[u8] = b"@STRELA 1 0 0 0\r\n@POINT 1 0 0 0\r\n@NUCL1 1 0 0 0\r\n@DIM32 1 0 0 43\r\n";
const NRES: &[u8] = b"[WEAPONS]
#GENSHIELD 13
MECH 21 19 O
IADRO 2 15 HX
IADROMOR 0 7 -
[UNITKINDS]
";

fn setup(rules: Rules) -> (World, TypeId, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let mor = c.add(ObjectType::from_def("Mor(EN)", &UnitDef::parse(MORTAR.as_bytes())));
    let tgt = c.add(ObjectType::from_def("Tgt(EN)", &UnitDef::parse(TARGET.as_bytes())));
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    let can = c.add(ObjectType::from_def("Can(EN)", &UnitDef::parse(CANNON.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    (w, mor, tgt, sol, can)
}

const ON: Rules = Rules { hit_locations: true, body_spacing: false, mortars: false, corpse_piles: false };

/// A mortar shells a target with a ring of soldiers around it; the world after `ticks`.
fn shell(rules: Rules, ticks: u32) -> World {
    let (mut w, mor, tgt, sol, _) = setup(rules);
    let m = w.spawn(mor, 0, Pos::from_pixels(1500, 2000), 0);
    let t = w.spawn(tgt, 1, Pos::from_pixels(2600, 2000), 128);
    for k in 0..48 {
        let (x, y) = (2450 + (k % 8) * 40, 1880 + (k / 8) * 45);
        w.spawn(sol, 1, Pos::from_pixels(x, y), 128);
    }
    w.step(&[Command::Attack { player: 0, units: vec![m], target: t }]);
    for _ in 0..ticks {
        w.step(&[]);
    }
    w
}

#[test]
fn a_shell_throws_fragments_that_wound_and_bleed() {
    let w = shell(ON, 200);
    assert!(w.fragments.len() >= acr_sim::shrapnel::COUNT.0 as usize, "{}", w.fragments.len());
    let struck: Vec<_> = w.fragments.iter().filter(|f| f.victim.is_some()).collect();
    assert!(!struck.is_empty(), "no fragment struck anyone");
    // Fragments fly within reach and both splinters and large pieces come.
    for f in &w.fragments {
        let d = f64::from(f.to.0 - f.from.0).hypot(f64::from(f.to.1 - f.from.1));
        assert!(d <= f64::from(acr_sim::shrapnel::REACH) + 4.0, "{f:?}");
    }
    assert!(w.fragments.iter().any(|f| f.mass < 10) && w.fragments.iter().any(|f| f.mass > 30));
    // The same game gives the same fragments.
    let w2 = shell(ON, 200);
    assert_eq!(w.fragments, w2.fragments);
    assert_eq!(w.state_hash(), w2.state_hash());
}

#[test]
fn fragment_wounds_seep_and_clot() {
    use acr_sim::body::{FractureKind, Outcome};
    use acr_sim::shrapnel::seep_of;
    assert_eq!(seep_of(Outcome::Wound), (1, 6));
    assert!(seep_of(Outcome::DeepWound).1 > seep_of(Outcome::Wound).1);
    assert!(seep_of(Outcome::Fracture(FractureKind::Open)).1 > seep_of(Outcome::DeepWound).1);
    // Energy falls with the distance, a big piece hits harder than a splinter.
    use acr_sim::shrapnel::{damage, energy};
    assert!(energy(100, 10, 200) > energy(100, 190, 200));
    assert!(energy(100, 50, 200) > energy(2, 50, 200));
    assert!(damage(100, energy(100, 20, 200)) >= 30 && damage(2, energy(2, 150, 200)) <= 2);
    // A man bleeding from fragments loses life every second until it clots.
    let mut b = acr_sim::body::Body { seep: 2, seep_s: 3, ..Default::default() };
    assert_eq!(b.bleeding(), 2);
    b.seep_s = 0;
    assert_eq!(b.bleeding(), 0);
}

#[test]
fn small_fragments_rarely_sever() {
    use acr_sim::body::{Cause, Outcome, limb_outcome};
    // The blast itself (energy 100) keeps its table; a weak fragment never takes a limb off.
    assert_eq!(limb_outcome(Cause::Shell, 100, true, 100, 90).0, Outcome::Off);
    assert_eq!(limb_outcome(Cause::Shell, 100, true, 100, 40).0, Outcome::Wound);
    for roll in 0..100 {
        assert_ne!(limb_outcome(Cause::Shell, 30, true, 100, roll).0, Outcome::Off);
    }
    let wounds = |e| (0..100).filter(|&r| limb_outcome(Cause::Shell, e, false, 50, r).0 == Outcome::Wound).count();
    assert!(wounds(20) > wounds(100));
}

#[test]
fn without_the_rule_no_fragments_fly() {
    let w = shell(Rules::default(), 200);
    assert!(w.fragments.is_empty());
    assert!(w.units.iter().all(|u| u.body.seep == 0 && u.body.seep_s == 0));
}

const CANNON: &str = "NAME Can\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@MOTION_L 16 0 0 9\r\n\
    @MOTION_R 16 0 10 19\r\n@ATTACK1 16 0 0 19\r\n\
    @PATTACK1 16 0 0 9\r\n@DEATH 16 0 0 9\r\nLIFE 1000\r\nGEOMETRY 1 10 40\r\nDAMAGE 1 340\r\n\
    ATTACK_RADIUS 1 100 2000\r\nDET_RADIUS 1 100 7000\r\nATTACK_PAUSE 1 400\r\nWEAPONKIND 1 IADRO\r\n\
    WEAPON 1 NUCL2P\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";

/// A cannon fires solid shot at a target in a block of soldiers; the world after `ticks`.
fn cannonade(rules: Rules, ticks: u32) -> World {
    let (mut w, _, tgt, sol, can) = setup(rules);
    let m = w.spawn(can, 0, Pos::from_pixels(1500, 2000), 0);
    let t = w.spawn(tgt, 1, Pos::from_pixels(2600, 2000), 128);
    for k in 0..48 {
        let (x, y) = (2450 + (k % 8) * 40, 1880 + (k / 8) * 45);
        w.spawn(sol, 1, Pos::from_pixels(x, y), 128);
    }
    w.step(&[Command::Attack { player: 0, units: vec![m], target: t }]);
    for _ in 0..ticks {
        w.step(&[]);
    }
    w
}

#[test]
fn a_grazing_ball_throws_up_earth_that_wounds() {
    use acr_sim::combat::DeathCause;
    use acr_sim::shrapnel::{GRAZE, Kind};
    let w = cannonade(ON, 300);
    let earth: Vec<_> = w.fragments.iter().filter(|f| f.kind == Kind::Earth).collect();
    assert!(earth.len() >= GRAZE.count.0 as usize, "{}", earth.len());
    assert!(w.fragments.iter().all(|f| f.kind != Kind::Casing), "a ball throws no casing");
    for f in &earth {
        let d = f64::from(f.to.0 - f.from.0).hypot(f64::from(f.to.1 - f.from.1));
        assert!(d <= f64::from(GRAZE.reach) + 4.0 && f.mass <= GRAZE.mass as u8 + 1, "{f:?}");
    }
    // A man a torn victim's bones or the earth struck took a shell's (fragment's) wound.
    let bones = w.fragments.iter().filter(|f| f.kind == Kind::Bone).count();
    let torn = w.deaths.values().filter(|d| d.torn.is_some() && d.cause == DeathCause::Cannonball).count();
    eprintln!(
        "earth {} (struck {}), bones {bones}, torn {torn}",
        earth.len(),
        earth.iter().filter(|f| f.victim.is_some()).count()
    );
    assert_eq!(bones > 0, torn > 0, "bones fly from every torn man");
    let w2 = cannonade(ON, 300);
    assert_eq!(w.fragments, w2.fragments);
    assert_eq!(w.state_hash(), w2.state_hash());
}

#[test]
fn without_the_rule_a_ball_throws_nothing() {
    let w = cannonade(Rules::default(), 300);
    assert!(w.fragments.is_empty());
    assert!(w.units.iter().all(|u| u.body.seep == 0 && u.body.seep_s == 0));
}
