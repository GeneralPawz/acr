//! Shell fires and direct hits (#256, rule `hit_locations`): a shell bursting on the ground
//! leaves a fire, soldiers in it catch fire and can burn to death, and a man at the burst is
//! torn apart; without the rule nothing changes.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::combat::DeathCause;
use acr_sim::formation::Formations;
use acr_sim::ground_fire::{BURN_TICKS, GroundFire, RADIUS};
use acr_sim::map::MapGrid;
use acr_sim::projectile::create;
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
    BURST #NUCL1 800 60 NONE NO_GRAVITY STAND 0 -1\r\n\
    [CHILDWEAPON]\r\nNUCL2P 1 1 0 4 SKOKAT SKOKAT SKOKAT SKOKAT\r\nBURST 0 0 -1 0\r\n\
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

fn setup(rules: Rules) -> (World, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let mor = c.add(ObjectType::from_def("Mor(EN)", &UnitDef::parse(MORTAR.as_bytes())));
    let tgt = c.add(ObjectType::from_def("Tgt(EN)", &UnitDef::parse(TARGET.as_bytes())));
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    (w, mor, tgt, sol)
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
};

/// A mortar shells a target; returns the world after `ticks` and the first fire seen.
fn shell(rules: Rules, ticks: u32) -> (World, Option<GroundFire>) {
    let (mut w, mor, tgt, _) = setup(rules);
    let m = w.spawn(mor, 0, Pos::from_pixels(1500, 2000), 0);
    let t = w.spawn(tgt, 1, Pos::from_pixels(2600, 2000), 128);
    w.step(&[Command::Attack { player: 0, units: vec![m], target: t }]);
    let mut first = None;
    for _ in 0..ticks {
        w.step(&[]);
        if first.is_none() {
            first = w.ground_fires.list.first().copied();
        }
    }
    (w, first)
}

#[test]
fn a_shell_burst_leaves_a_fire_that_burns_out() {
    let (w, first) = shell(ON, 300);
    let f = first.expect("the shell left a fire");
    assert_eq!(f.radius, RADIUS);
    // It burst east of the mortar, near the target.
    assert!((2000..=2900).contains(&f.x) && (f.y - 2000).abs() < 200, "{f:?}");
    // The same game gives the same fires.
    let (w2, _) = shell(ON, 300);
    assert_eq!(w.ground_fires, w2.ground_fires);
    assert_eq!(w.state_hash(), w2.state_hash());
    // The first fire is out after BURN_TICKS (the mortar goes on shelling: newer ones burn).
    let mut w = w;
    for _ in 0..BURN_TICKS {
        w.step(&[]);
    }
    assert!(!w.ground_fires.list.contains(&f), "{:?}", w.ground_fires);
    assert!(w.ground_fires.list.iter().all(|g| g.age(w.tick.0) < BURN_TICKS));
}

#[test]
fn without_the_rule_shells_leave_no_fire() {
    let (w, first) = shell(Rules::default(), 300);
    assert!(first.is_none() && w.ground_fires.list.is_empty());
    assert!(w.units.iter().all(|u| !u.burning && u.body.burn == 0));
    // The fires add nothing to the hash while there are none.
    let mut w2 = w.clone();
    w2.ground_fires.list.clear();
    assert_eq!(w.state_hash(), w2.state_hash());
}

#[test]
fn a_man_in_a_fire_burns_to_death() {
    let (mut w, _, _, sol) = setup(ON);
    let s = w.spawn(sol, 1, Pos::from_pixels(2000, 2000), 0);
    let before = w.state_hash();
    w.ground_fires.list.push(GroundFire { x: 2010, y: 2000, radius: RADIUS, start: 0 });
    assert_ne!(before, w.state_hash(), "a fire is part of the state");
    let i = s.0 as usize;
    let mut caught = None;
    for t in 0..200 {
        w.step(&[]);
        if caught.is_none() && w.units[i].burning {
            caught = Some(t);
        }
        if !w.units[i].alive {
            break;
        }
    }
    assert!(caught.is_some_and(|t| t <= 10), "caught fire within a second: {caught:?}");
    assert!(!w.units[i].living(), "60 life at 8 a second: dead in 8 s");
    assert_eq!(w.deaths.get(&s.0).map(|d| d.cause), Some(DeathCause::Burned));
}

#[test]
fn a_man_who_walks_out_goes_out() {
    let (mut w, _, _, sol) = setup(ON);
    let s = w.spawn(sol, 1, Pos::from_pixels(2000, 2000), 0);
    w.ground_fires.list.push(GroundFire { x: 2000, y: 2000, radius: RADIUS, start: 0 });
    let i = s.0 as usize;
    while !w.units[i].burning {
        w.step(&[]);
    }
    // Out of the fire: he burns on for a few seconds, then goes out alive.
    w.ground_fires.list.clear();
    for _ in 0..100 {
        w.step(&[]);
    }
    let u = &w.units[i];
    assert!(u.living() && !u.burning && u.body.burn == 0);
    assert!(u.life < 60 && u.life > 0, "life {}", u.life);
}

#[test]
fn a_fire_weapon_flame_outlasts_the_shell_fire() {
    let (mut w, _, _, sol) = setup(ON);
    let s = w.spawn(sol, 1, Pos::from_pixels(2000, 2000), 0);
    let i = s.0 as usize;
    // Set on fire by an `X`/`F` weapon before (the original's flag, which never goes out).
    w.units[i].burning = true;
    // A man who cannot panic (walking with `UnlimitedMotion`, as out of a building): a man
    // who can runs and his flight ends the flames (`tests/burn_panic.rs`).
    w.units[i].unlimited = true;
    w.ground_fires.list.push(GroundFire { x: 2000, y: 2000, radius: RADIUS, start: 0 });
    while w.units[i].body.burn == 0 {
        w.step(&[]);
    }
    w.ground_fires.list.clear();
    for _ in 0..100 {
        w.step(&[]);
    }
    let u = &w.units[i];
    assert!(u.living() && u.body.burn == 0 && !u.body.burn_kept, "life {}", u.life);
    assert!(u.burning, "the weapon's fire stays on");
}

/// A shell bursting at (2000, 2000) among three men 3, 12 and 40 px away; returns the world
/// and the men.
fn burst_among(rules: Rules) -> (World, [usize; 3]) {
    let (mut w, mor, _, sol) = setup(rules);
    let m = w.spawn(mor, 0, Pos::from_pixels(1000, 2000), 0);
    let men = [(2003, 2000), (2000, 2012), (2040, 2000)].map(|(x, y)| w.spawn(sol, 1, Pos::from_pixels(x, y), 0));
    w.step(&[]);
    let burst = w.catalog.weapons.list.iter().position(|wp| wp.name == "BURST").unwrap() as u16;
    assert!(create(&mut w, burst, (2000, 2000, 0), (2000, 2000, 0), Some(m.0 as usize), 1, None, (0, 0, 0), 0));
    (w, men.map(|e| e.0 as usize))
}

#[test]
fn a_shell_at_a_mans_feet_leaves_a_mist_closer_ones_are_torn_in_two() {
    use acr_sim::body::{Outcome, SHELL_DIRECT, SHELL_MIST, Torn};
    let (w, men) = burst_among(ON);
    for (k, &i) in men.iter().enumerate() {
        let (x, y) = w.units[i].pos.pixels();
        let d = acr_sim::trig::norma(x - 2000, y - 2000);
        assert!(!w.units[i].living(), "man {k} at {d} px died");
        let hit = w.hits.iter().find(|h| h.victim == i as u32 && h.cause == DeathCause::Explosion).unwrap();
        let outcome = hit.injury.unwrap().outcome;
        let torn = w.deaths.get(&(i as u32)).and_then(|d| d.torn);
        match k {
            0 => {
                assert!(d < SHELL_MIST, "{d}");
                assert_eq!(outcome, Outcome::Torn(Torn::Pulverized));
                assert_eq!(torn, Some(Torn::Pulverized));
            }
            1 => {
                assert!((SHELL_MIST..SHELL_DIRECT).contains(&d), "{d}");
                assert!(matches!(outcome, Outcome::Torn(Torn::InTwo { .. })), "{outcome:?}");
                assert!(matches!(torn, Some(Torn::InTwo { .. })), "{torn:?}");
            }
            _ => assert!(!matches!(outcome, Outcome::Torn(_)), "{outcome:?}"),
        }
    }
    let (w2, _) = burst_among(ON);
    assert_eq!(w.state_hash(), w2.state_hash());
    // Without the rule: the original blast, no bodies torn.
    let (off, men) = burst_among(Rules::default());
    assert!(men.iter().all(|&i| !off.units[i].living()));
    assert!(off.hits.iter().all(|h| h.injury.is_none()));
    assert!(off.deaths.values().all(|d| d.torn.is_none()));
}
