//! Aimed fire (#296, rule `aimed_fire`): a firearm shot beyond 3 m reloads 15 % longer; nearer,
//! and with the rule off, nothing changes (`docs/simulation.md`, "Aimed fire").

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::wounds::{AIMED_MIN, AIMED_PAUSE};
use acr_sim::{Pos, Rules, World};

const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK1 9 0 0 62\r\n@UATTACK1 9 0 62 0\r\n@ATTACK1 9 0 0 67\r\n@ATTACK 9 0 0 52\r\n\
    @ATTACK3 9 0 68 101\r\n@TRANSX3 9 0 0 35\r\n@TRANS3X 9 0 35 0\r\nSETACTIVEPOINT #ATTACK1 13 0 0\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 1000\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 15\r\nDAMAGE 1 380\r\n\
    ATTACK_RADIUS 0 40 80\r\nDET_RADIUS 0 0 250\r\nATTACK_RADIUS 1 20 850\r\nDET_RADIUS 1 20 7000\r\n\
    ADD_ATTACK_RADIUS 0 32\r\nATTACK_PAUSE 0 20\r\nATTACK_PAUSE 1 550\r\nWEAPONKIND 0 MECH\r\nWEAPONKIND 1 VISTREL\r\n\
    WEAPON 1 SHOT\r\nDAMAGEDEC 1 300\r\nRAZBROS 300\r\nNOPAUSEDATTACK 0\r\nSLOWRECHARGE\r\nUSETRANSX 0\r\n\
    SEARCH_ENEMY_RADIUS 850\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n\
    PROTECTION 3 PIKA 1 VISTREL 10 MECH 1\r\nZPOINTS 33 33\r\nUNITRADIUS 8\r\nFRIENDLYFIRE\r\n[END]\r\n";

const NRES: &[u8] = b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nVISTREL 12 18 HRB\r\n[UNITKINDS]\r\n";

fn on() -> Rules {
    Rules { aimed_fire: true, ..Rules::default() }
}

fn world(rules: Rules) -> World {
    let mut c = Catalog::new();
    c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    w
}

/// The reload counter a musketeer sets with his first shot at an enemy `px` pixels away.
fn first_reload(rules: Rules, px: i32) -> i32 {
    let mut w = world(rules);
    let t = w.catalog.find("Mus(EN)").expect("type");
    let m = w.spawn(t, 0, Pos::from_pixels(2000, 2000), 64).0 as usize;
    w.spawn(t, 1, Pos::from_pixels(2000 + px, 2000), 192);
    for _ in 0..400 {
        w.step(&[]);
        if w.units[m].max_delay != 0 {
            return w.units[m].max_delay;
        }
    }
    panic!("no shot");
}

#[test]
fn an_aimed_shot_reloads_longer() {
    let off = first_reload(Rules::default(), AIMED_MIN + 110);
    assert_eq!(first_reload(on(), AIMED_MIN + 110), off * (100 + AIMED_PAUSE) / 100);
}

#[test]
fn a_close_shot_and_rule_off_are_unchanged() {
    let near = first_reload(Rules::default(), AIMED_MIN - 30);
    assert_eq!(first_reload(on(), AIMED_MIN - 30), near);
}

#[test]
fn the_hash_changes_only_with_the_rule_on() {
    let h = |r: Rules| {
        let mut s = DefaultHasher::new();
        r.hash(&mut s);
        s.finish()
    };
    assert_eq!(h(Rules { aimed_fire: false, ..Rules::default() }), h(Rules::default()));
    assert_ne!(h(on()), h(Rules::default()));
}
