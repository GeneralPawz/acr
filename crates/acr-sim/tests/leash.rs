//! Animals give up a chase (optional rule `Rules::animal_give_up`, #355, `docs/simulation.md`
//! "Animals give up"): off, a bear follows its target for good as in the original; on, it
//! drops the attack once the target is 3 search radii from where the chase began, walks
//! back, and takes the target again only after it left the search radius and came back.

use std::sync::Arc;

use acr_formats::defs::UnitDef;
use acr_formats::defs::nres::weapon_kinds;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::{Pos, Rules, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@PATTACK 9 0 0 35\r\n@UATTACK 9 0 35 0\r\n@ATTACK 9 0 0 20\r\n\
    @DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nDAMAGE 0 10\r\n\
    ATTACK_RADIUS 0 40 100\r\nATTACK_PAUSE 0 1\r\nSEARCH_ENEMY_RADIUS 700\r\nSEARCH_ENEMY_RADIUS_SHIFT 70\r\n\
    DET_RADIUS 0 0 7000\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\nWEAPONKIND 0 PIKA\r\nPROTECTION 1 PIKA 3\r\n\
    UNITRADIUS 8\r\n[END]\r\n";

const NRES: &[u8] = b"[WEAPONS]\r\nPIKA 11 17 O\r\n[UNITKINDS]\r\n";

/// The bear's search radius in pixels (`SEARCH_ENEMY_RADIUS 200` less the `_SHIFT` share).
fn sr(w: &World, b: usize) -> i32 {
    w.catalog.get(w.units[b].kind).search_radius >> 4
}

/// A bear (owner 0) at (2000, 2000) and a man (owner 1, very tough) 150 px east of it.
fn world(give_up: bool) -> (World, usize, usize) {
    let mut c = Catalog::new();
    let bear_def = PIKEMAN
        .replace("NAME Pik", "NAME AnmMed")
        .replace("SEARCH_ENEMY_RADIUS 700", "SEARCH_ENEMY_RADIUS 200")
        .replace("[END]", "ANIMAL\r\nSTANDGROUND\r\n[END]");
    let bear = c.add(ObjectType::from_def("AnmMed", &UnitDef::parse(bear_def.as_bytes())));
    // The man never looks for enemies, so the bear is not drawn into a fight by him.
    let man_def = PIKEMAN.replace("SEARCH_ENEMY_RADIUS 700", "SEARCH_ENEMY_RADIUS 0");
    let man = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(man_def.as_bytes())));
    c.set_weapon_kinds(&weapon_kinds(NRES));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = Rules { animal_give_up: give_up, ..Rules::default() };
    let b = w.spawn(bear, 0, Pos::from_pixels(2000, 2000), 0).0 as usize;
    let m = w.spawn(man, 1, Pos::from_pixels(2150, 2000), 128).0 as usize;
    w.units[b].life = 1_000_000;
    w.units[m].life = 1_000_000;
    (w, b, m)
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

/// Steps until bear `b` attacks man `m` (and the rule has seen it).
fn until_chase(w: &mut World, b: usize, m: usize) {
    for _ in 0..400 {
        if w.units[b].target() == Some(w.units[m].id) {
            // One more tick: the rule records the start of the chase after the search.
            w.step(&[]);
            return;
        }
        w.step(&[]);
    }
    panic!("the bear never attacked");
}

/// Puts man `m` `dx` px east of bear `b`, which faces east.
fn place_beside(w: &mut World, b: usize, m: usize, dx: i32) {
    w.units[b].dir = 0;
    let (x, y) = w.units[b].pos.pixels();
    w.units[m].pos = Pos::from_pixels(x + dx, y);
}

#[test]
fn off_the_chase_goes_on_for_good() {
    let (mut w, b, m) = world(false);
    until_chase(&mut w, b, m);
    w.units[m].pos = Pos::from_pixels(2000 + 5 * sr(&w, b), 2000);
    run(&mut w, 200);
    assert_eq!(w.units[b].target(), Some(w.units[m].id), "still after the man");
    assert!(w.leashes.is_empty());
}

#[test]
fn on_it_gives_up_beyond_the_distance_and_walks_back() {
    let (mut w, b, m) = world(true);
    until_chase(&mut w, b, m);
    let home = w.leashes.map[&(b as u32)].home;
    // Within the distance the chase goes on.
    w.units[m].pos = Pos::from_pixels(2000 + 2 * sr(&w, b), 2000);
    run(&mut w, 3);
    assert_eq!(w.units[b].target(), Some(w.units[m].id));
    // Beyond 3 radii from the start it is dropped, and the bear heads home.
    w.units[m].pos = Pos::from_pixels(2000 + 4 * sr(&w, b), 2000);
    run(&mut w, 2);
    assert_eq!(w.units[b].target(), None, "the attack is dropped");
    let dest = w.units[b].dest.expect("a destination");
    assert!((dest.x - home.0).abs() < 16 * 16 && (dest.y - home.1).abs() < 16 * 16, "it walks back");
    run(&mut w, 400);
    let (x, y) = w.units[b].pos.pixels();
    assert!((x - (home.0 >> 4)).abs() < 30 && (y - (home.1 >> 4)).abs() < 30, "back at the start: {x},{y}");
    assert_eq!(w.units[b].target(), None, "and does not chase the far man");
}

#[test]
fn it_takes_the_target_again_only_after_it_came_near() {
    let (mut w, b, m) = world(true);
    until_chase(&mut w, b, m);
    // The chase began far to the west, so the man beside the bear is beyond the distance.
    let reach = 4 * sr(&w, b) * 16;
    w.leashes.map.get_mut(&(b as u32)).expect("leash").home.0 -= reach;
    run(&mut w, 1);
    assert_eq!(w.units[b].target(), None, "the attack is dropped");
    assert!(w.leashes.shy_of(b, w.units[m].id));
    // He stays in the radius: the bear ignores him.
    run(&mut w, 60);
    assert_ne!(w.units[b].target(), Some(w.units[m].id), "the shy target is ignored in the radius");
    // Out of the radius, then back inside it: the bear attacks again.
    let out = 2 * sr(&w, b);
    place_beside(&mut w, b, m, out);
    run(&mut w, 2);
    assert!(!w.leashes.shy_of(b, w.units[m].id));
    // (Standing still, as the walk home would take a while.)
    w.units[b].orders.clear();
    // Back at the spawn spot, where the bear sees him.
    w.units[b].pos = Pos::from_pixels(2000, 2000);
    w.units[b].dir = 0;
    w.units[m].pos = Pos::from_pixels(2150, 2000);
    let mut again = false;
    for _ in 0..400 {
        w.step(&[]);
        // The test's far-away start point makes it give up at once after taking him, so
        // the sign of a new attack is the walk home it orders.
        again |= !w.units[b].orders.is_empty();
    }
    assert!(again, "the bear took him again");
}
