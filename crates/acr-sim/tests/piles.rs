//! Corpse piles as obstacles (optional rule `Rules::corpse_piles`, #222, `docs/simulation.md`
//! "Corpse piles"): dense piles slow the men who cross them, big piles are walked around,
//! decay clears them; with the rule off nothing changes.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::piles::{BLOCKED, PILE_SHIFT, ROUGH};
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, Rules, TypeId, World};

const MAN: &str = "NAME Man\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";

const ON: Rules = Rules {
    hit_locations: false,
    body_spacing: false,
    mortars: false,
    corpse_piles: true,
    group_move: false,
    cannon_fire: false,
    ball_terrain: false,
    formation_attack: false,
    animal_give_up: false,
    dart_fire: false,
    aimed_fire: false,
};

/// The walker's row (px) and his start and goal columns.
const ROW: i32 = 1008;
const FROM: i32 = 880;
const TO: i32 = 1200;

fn world(rules: Rules) -> (World, TypeId) {
    let mut c = Catalog::new();
    let man = c.add(ObjectType::from_def("Man(EN)", &UnitDef::parse(MAN.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    (w, man)
}

/// Lays `n` corpses in the pile cell `(cx, cy)`, spread over it.
fn pile(w: &mut World, man: TypeId, cx: i32, cy: i32, n: u8) {
    let ids: Vec<EntityId> = (0..i32::from(n))
        .map(|k| {
            let (x, y) = ((cx << PILE_SHIFT) + 4 + (k % 3) * 10, (cy << PILE_SHIFT) + 4 + (k / 3) * 10);
            w.spawn(man, 1, Pos::from_pixels(x, y), 0)
        })
        .collect();
    for id in ids {
        w.kill(id);
    }
}

fn cell_of(w: &World, id: EntityId) -> (i32, i32) {
    let (x, y) = w.units[id.0 as usize].pos.pixels();
    (x >> PILE_SHIFT, y >> PILE_SHIFT)
}

/// Sends a walker along the row and steps until he is there (or `max` ticks): the ticks
/// taken and the pile cells he crossed.
fn walk(w: &mut World, man: TypeId, max: u32) -> (u32, Vec<(i32, i32)>) {
    let id = w.spawn(man, 0, Pos::from_pixels(FROM, ROW), 0);
    w.step(&[Command::Move { player: 0, units: vec![id], x: TO, y: ROW }]);
    let mut cells = Vec::new();
    for t in 1..max {
        w.step(&[]);
        let c = cell_of(w, id);
        if cells.last() != Some(&c) {
            cells.push(c);
        }
        let (x, _) = w.units[id.0 as usize].pos.pixels();
        if x >= TO - 8 && !w.units[id.0 as usize].in_motion {
            return (t, cells);
        }
    }
    (max, cells)
}

/// A band of rough cells (`ROUGH` bodies each) three cells wide across the walker's row.
fn rough_band(w: &mut World, man: TypeId) {
    let row = ROW >> PILE_SHIFT;
    for cx in 31..34 {
        for cy in row - 1..=row + 1 {
            pile(w, man, cx, cy, ROUGH);
        }
    }
}

/// A wall of blocked cells (`BLOCKED` bodies each) five cells high across the row.
fn wall(w: &mut World, man: TypeId) -> Vec<(i32, i32)> {
    let row = ROW >> PILE_SHIFT;
    let cells: Vec<(i32, i32)> = (row - 2..=row + 2).map(|cy| (32, cy)).collect();
    for &(cx, cy) in &cells {
        pile(w, man, cx, cy, BLOCKED);
    }
    cells
}

#[test]
fn a_dense_pile_slows_a_crossing_man() {
    let mut took = Vec::new();
    for rules in [Rules::default(), ON] {
        let (mut w, man) = world(rules);
        rough_band(&mut w, man);
        let (t, _) = walk(&mut w, man, 2000);
        println!("rules {rules:?}: {t} ticks");
        took.push(t);
    }
    assert!(took[0] < 2000 && took[1] < 2000, "{took:?}");
    // Three of the ten cells on the way at half speed: about 30 % longer.
    assert!(took[1] * 10 >= took[0] * 12, "{took:?}");
}

#[test]
fn a_big_pile_is_walked_around() {
    for rules in [Rules::default(), ON] {
        let (mut w, man) = world(rules);
        let blocked = wall(&mut w, man);
        let (t, cells) = walk(&mut w, man, 3000);
        let through = cells.iter().any(|c| blocked.contains(c));
        println!("rules {rules:?}: {t} ticks, through the pile: {through}, {cells:?}");
        assert!(t < 3000, "never arrived");
        assert_eq!(through, rules == Rules::default());
    }
}

#[test]
fn decay_clears_the_piles() {
    let (mut w, man) = world(ON);
    let blocked = wall(&mut w, man);
    rough_band(&mut w, man);
    assert!(!w.piles.is_empty());
    let lock = w.piles.lock().expect("a blocked pile makes the lock map");
    assert!(blocked.iter().all(|&(cx, cy)| lock.check_pt(cx * 2, cy * 2)));
    // The corpses are removed after `CORPSE_LIFETIME` (2000 ticks) and the death animation.
    for _ in 0..2100 {
        w.step(&[]);
    }
    assert!(w.piles.is_empty(), "{:?}", w.piles.counts);
    assert!(w.piles.counts.is_empty());
    let lock = w.piles.lock().unwrap();
    assert!(blocked.iter().all(|&(cx, cy)| !lock.check_bar(cx * 2, cy * 2, 2, 2)));
    // Then the man walks straight through where the pile was.
    let (t, cells) = walk(&mut w, man, 3000);
    assert!(t < 3000);
    assert!(cells.iter().any(|c| blocked.contains(c)), "{cells:?}");
}

#[test]
fn with_the_rule_off_nothing_is_counted() {
    let (mut w, man) = world(Rules::default());
    wall(&mut w, man);
    rough_band(&mut w, man);
    assert!(w.piles.is_empty());
    assert!(w.piles.lock().is_none());
    // The rules' hash part leaves the earlier rules' hashes alone.
    let (mut a, _) = world(Rules::default());
    let (mut b, _) = world(Rules { corpse_piles: false, ..ON });
    a.step(&[]);
    b.step(&[]);
    assert_eq!(a.state_hash(), b.state_hash());
}

/// The whole scene: piles and a walker; the state hash after it.
fn scene(rules: Rules) -> u64 {
    let (mut w, man) = world(rules);
    wall(&mut w, man);
    rough_band(&mut w, man);
    walk(&mut w, man, 600);
    for _ in 0..1600 {
        w.step(&[]);
    }
    w.state_hash()
}

#[test]
fn piles_are_deterministic_and_change_the_game() {
    let on = scene(ON);
    assert_eq!(on, scene(ON), "two runs with the rule differ");
    assert_ne!(on, scene(Rules::default()));
}
