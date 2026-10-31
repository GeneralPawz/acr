//! Cannonballs against trees and stones (optional rule `ball_terrain`, with hit locations):
//! a thin tree is felled, a thick one stops a weakened ball, a small stone is knocked aside,
//! a big rock chips and stops the ball; off, the map objects change nothing.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_formats::m3d::SpriteGroup;
use acr_sim::ball_terrain::{Struck, add_object, girth};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::projectile::create;
use acr_sim::weapon::Weapons;
use acr_sim::world::EntityId;
use acr_sim::{Pos, Rules, World};

const GUNNER: &str = "NAME Gun\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@DEATH 16 0 0 9\r\n\
    LIFE 1000\r\nGEOMETRY 1 10 40\r\nDAMAGE 1 340\r\nATTACK_RADIUS 1 100 2000\r\nDET_RADIUS 1 100 7000\r\n\
    ATTACK_PAUSE 1 400\r\nWEAPONKIND 1 IADRO\r\nWEAPON 1 ROLL\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@DEATH 9 0 0 9\r\n\
    #DEATHLIE1 9 1 0 9\r\nLIFE 300\r\nGEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\nUNITRADIUS 8\r\n[END]\r\n";
/// A ball rolling level 10 px above the ground, low enough to hurt (`DAMAGEHEIGHT` 95).
const NDS: &[u8] = b"[MEMBERS]\r\n\
    ROLL #NUCL1 500 16 NONE NO_GRAVITY FLY 16 -1\r\n\
    [DAMAGEHEIGHT]\r\nROLL 95\r\n[END]\r\n";
const ADS: &[u8] = b"@NUCL1 1 0 0 0\r\n";
const NRES: &[u8] = b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nIADRO 2 15 HX\r\n[UNITKINDS]\r\n";

const ON: Rules = Rules {
    hit_locations: true,
    body_spacing: false,
    mortars: false,
    corpse_piles: false,
    group_move: false,
    cannon_fire: false,
    ball_terrain: true,
    animal_give_up: false,
};

/// What stands on the ball's way (y = 2000), from x 1950 eastwards.
#[derive(Clone, Copy)]
enum Thing {
    Tree(i32),
    Man(i32),
    Stone(i32),
    Rock(i32),
}

/// The first `x` from `from` on whose tree girth satisfies `ok`.
fn tree_at(from: i32, ok: impl Fn(u8) -> bool) -> i32 {
    (from..from + 400).find(|&x| ok(girth(x, 2000))).expect("a place with that girth")
}

/// Fires a low ball from x 1900 to 2700 along y 2000 through `things`; returns the world
/// after it has gone, the men and the object indices.
fn shot(rules: Rules, things: &[Thing]) -> (World, Vec<EntityId>, Vec<usize>) {
    let mut c = Catalog::new();
    let gun = c.add(ObjectType::from_def("Gun(EN)", &UnitDef::parse(GUNNER.as_bytes())));
    let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    let g = w.spawn(gun, 0, Pos::from_pixels(1500, 2000), 0);
    let (mut men, mut objs) = (Vec::new(), Vec::new());
    for t in things {
        match *t {
            Thing::Man(x) => men.push(w.spawn(sol, 1, Pos::from_pixels(x, 2000), 128)),
            Thing::Tree(x) => objs.push(add_object(&mut w, SpriteGroup::Trees, 0, x, 2000, true, false)),
            Thing::Stone(x) => objs.push(add_object(&mut w, SpriteGroup::Stones, 0, x, 2004, false, false)),
            Thing::Rock(x) => objs.push(add_object(&mut w, SpriteGroup::Stones, 1, x, 2000, false, true)),
        }
    }
    w.step(&[]);
    let roll = w.catalog.weapons.list.iter().position(|wp| wp.name == "ROLL").unwrap() as u16;
    assert!(create(&mut w, roll, (1900, 2000, 10), (2700, 2000, 10), Some(g.0 as usize), 1, None, (0, 0, 0), 0));
    for _ in 0..60 {
        w.step(&[]);
    }
    assert_eq!(w.projectiles.iter().count(), 0, "the ball is gone");
    (w, men, objs)
}

fn alive(w: &World, id: EntityId) -> bool {
    w.units[id.0 as usize].living()
}

#[test]
fn off_the_trees_and_stones_change_nothing() {
    let thin = tree_at(2000, |g| g <= 30);
    let things = [Thing::Tree(thin), Thing::Stone(2200), Thing::Man(2300)];
    let mut hl = ON;
    hl.ball_terrain = false;
    let (w, men, _) = shot(hl, &things);
    assert!(w.struck.is_empty());
    let (bare, bare_men, _) = shot(hl, &[Thing::Man(2300)]);
    assert_eq!(alive(&w, men[0]), alive(&bare, bare_men[0]));
    assert_eq!(w.state_hash(), bare.state_hash(), "map objects do not count without the rule");
    // Without hit locations the rule does nothing either (it acts on ploughing balls).
    let only = Rules { ball_terrain: true, ..Rules::default() };
    let (w, _, _) = shot(only, &things);
    assert!(w.struck.is_empty());
}

#[test]
fn a_thin_tree_falls_and_a_thick_one_stops_the_weakened_ball() {
    // Thin (girth ≤ 30): felled, the ball keeps ≥ 85 %. Two men take 20 % each, the thick
    // tree (girth ≥ 70) is more than the ball has left: splintered, the ball stops.
    let thin = tree_at(2000, |g| g <= 30);
    let thick = tree_at(2300, |g| g >= 70);
    let things = [Thing::Tree(thin), Thing::Man(2200), Thing::Man(2240), Thing::Tree(thick), Thing::Man(thick + 60)];
    let (w, men, objs) = shot(ON, &things);
    assert!(matches!(w.struck.get(&(objs[0] as u32)), Some(Struck::Felled { dir: 0, .. })), "{:?}", w.struck);
    assert!(matches!(w.struck.get(&(objs[1] as u32)), Some(Struck::Splintered { hits: 1, .. })), "{:?}", w.struck);
    assert!(!alive(&w, men[0]) && !alive(&w, men[1]));
    assert!(alive(&w, men[2]), "the man behind the thick tree is safe");
    // The felled tree is still a wood source.
    assert_eq!(acr_sim::gather::determine(&w, thin, 2000), Some(0));
    // The same shot plays the same, and differs from the rule off.
    let (w2, _, _) = shot(ON, &things);
    assert_eq!(w.state_hash(), w2.state_hash());
    let mut off = ON;
    off.ball_terrain = false;
    let (w3, men3, _) = shot(off, &things);
    assert_ne!(w.state_hash(), w3.state_hash());
    assert!(!alive(&w3, men3[2]), "without the rule the ball reaches him");
}

#[test]
fn a_small_stone_is_knocked_aside_and_a_rock_stops_the_ball() {
    let things = [Thing::Stone(2100), Thing::Rock(2300), Thing::Man(2400)];
    let (w, men, objs) = shot(ON, &things);
    let Some(Struck::Moved { dx, dy, from: (0, 0), .. }) = w.struck.get(&(objs[0] as u32)).copied() else {
        panic!("{:?}", w.struck)
    };
    // 40 px, 45° off the ball's way (east), to the south where it lay.
    assert!(dx > 20 && dy > 20, "{dx} {dy}");
    assert!(matches!(w.struck.get(&(objs[1] as u32)), Some(Struck::Chipped { chips: 1, .. })), "{:?}", w.struck);
    assert!(alive(&w, men[0]), "the rock shields him");
    let (w2, _, _) = shot(ON, &things);
    assert_eq!(w.state_hash(), w2.state_hash());
}
