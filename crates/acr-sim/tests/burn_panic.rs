//! Burning men panic (rule `hit_locations`, `ground_fire::panic`): a man on fire breaks from
//! his formation and runs away from the flames and his comrades out of control for a few
//! seconds, then the flames go out or he falls; without the rule the original's burning
//! flag changes nothing.

use std::sync::Arc;

use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::combat::DeathCause;
use acr_sim::formation::{Formations, Officer, OfficerFormation};
use acr_sim::ground_fire::{FLEE_SECONDS, GroundFire, RADIUS};
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, Rules, World};

fn def(name: &str, extra: &str) -> UnitDef {
    UnitDef::parse(
        format!(
            "NAME {name}\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
             @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 9\r\n#DEATHLIE1 9 1 0 9\r\nLIFE 100\r\n\
             GEOMETRY 1 10 40\r\nMATHERIAL 1 BODY\r\nPSIXOZ 128\r\n{extra}[END]\r\n"
        )
        .as_bytes(),
    )
}

const ORDERS: &[u8] = b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n";
const GROUPS: &[u8] = b"1\r\nLINE 1 0 1 1 #LINE15\r\n";

const ON: Rules =
    Rules { hit_locations: true, body_spacing: false, mortars: false, corpse_piles: false, group_move: false };

/// An officer and 15 pikemen formed in a line; returns the world and the soldiers.
fn formed(rules: Rules) -> (World, Vec<EntityId>) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &def("Pik", "")));
    let off = c.add(ObjectType::from_def("Off(EN)", &def("Off", "OFFICER\r\n")));
    let mut f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    let line = vec![f.shape_index("LINE15").unwrap()];
    f.officers.insert(
        off,
        Officer {
            drummer: None,
            flag: None,
            formations: vec![OfficerFormation { group: 0, shapes: line, kinds: vec![pik] }],
        },
    );
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
    w.rules = rules;
    let officer = w.spawn(off, 0, Pos::from_pixels(2000, 2000), 0);
    let men: Vec<_> =
        (0..15).map(|i| w.spawn(pik, 0, Pos::from_pixels(1900 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0)).collect();
    w.step(&[]);
    let shape = w.formations.shape_index("LINE15").unwrap() as u16;
    w.step(&[Command::FormNear { player: 0, officer, shape, kind: pik }]);
    assert_eq!(w.brigades.len(), 1);
    for _ in 0..200 {
        w.step(&[]);
    }
    (w, men)
}

/// A shell fire under the seventh man of the line; returns the world, his index and the fire.
fn fire_in_the_line(rules: Rules) -> (World, usize, (i32, i32)) {
    let (mut w, men) = formed(rules);
    let i = men[7].0 as usize;
    assert!(w.units[i].brigade.is_some());
    let (x, y) = w.units[i].pos.pixels();
    let (fx, fy) = (x + 10, y + 5);
    w.ground_fires.list.push(GroundFire { x: fx, y: fy, radius: RADIUS, start: w.tick.0 });
    (w, i, (fx, fy))
}

fn dist(w: &World, i: usize, (fx, fy): (i32, i32)) -> i32 {
    let (x, y) = w.units[i].pos.pixels();
    acr_sim::trig::norma(x - fx, y - fy)
}

#[test]
fn a_burning_man_breaks_ranks_and_runs_from_the_fire() {
    let (mut w, i, fire) = fire_in_the_line(ON);
    let id = w.units[i].id;
    let start = dist(&w, i, fire);
    let mut caught = None;
    for t in 0..12 {
        w.step(&[]);
        if w.units[i].body.flee != 0 {
            caught = Some(t);
            break;
        }
    }
    assert!(caught.is_some(), "he panicked within a second");
    let u = &w.units[i];
    assert!(u.burning && u.brigade.is_none(), "on fire and out of the formation");
    assert!(w.brigades[0].members.iter().flatten().all(|&m| m != id));
    // Running out of control: a move order does nothing.
    let mut ran = false;
    for _ in 0..25 {
        w.step(&[Command::Move { player: 0, units: vec![id], x: 1000, y: 3000 }]);
        ran |= w.units[i].panicking();
    }
    assert!(ran, "he ran out of control");
    let away = dist(&w, i, fire);
    assert!(away > start + RADIUS, "he ran away from the fire: {start} -> {away} px");
    // After his flight the flames are out or he fell, burned.
    for _ in 0..(10 * u32::from(FLEE_SECONDS) + 20) {
        w.step(&[]);
    }
    let u = &w.units[i];
    if u.living() {
        assert!(!u.burning && u.body.flee == 0 && !u.panicking(), "he put the flames out and stopped");
    } else {
        assert_eq!(w.deaths.get(&id.0).map(|d| d.cause), Some(DeathCause::Burned));
    }
}

#[test]
fn a_man_set_on_fire_by_a_weapon_runs_from_his_comrades() {
    let (mut w, men) = formed(ON);
    let i = men[7].0 as usize;
    // The original's flag of an `X`/`F` weapon, no fire on the ground.
    w.units[i].burning = true;
    let centre = |w: &World| {
        let others: Vec<_> = men.iter().map(|m| m.0 as usize).filter(|&k| k != i).collect();
        let n = others.len() as i32;
        let (sx, sy) = others.iter().fold((0, 0), |(sx, sy), &k| {
            let (x, y) = w.units[k].pos.pixels();
            (sx + x, sy + y)
        });
        (sx / n, sy / n)
    };
    let start = dist(&w, i, centre(&w));
    for _ in 0..30 {
        w.step(&[]);
    }
    assert!(w.units[i].brigade.is_none());
    let away = dist(&w, i, centre(&w));
    assert!(away > start + 40, "he ran away from his comrades: {start} -> {away} px");
}

#[test]
fn the_flight_is_deterministic() {
    let run = || {
        let (mut w, i, _) = fire_in_the_line(ON);
        for _ in 0..80 {
            w.step(&[]);
        }
        (w.state_hash(), w.units[i].pos, w.units[i].living())
    };
    let a = run();
    assert_eq!(a, run());
    // The panic changes the game against a fire that does not set men running.
    let (mut w, i, _) = fire_in_the_line(ON);
    w.units[i].unlimited = true;
    for _ in 0..80 {
        w.step(&[]);
    }
    assert_ne!(a.0, w.state_hash());
}

#[test]
fn without_the_rule_a_burning_man_keeps_his_place() {
    let (mut w, men) = formed(Rules::default());
    let i = men[7].0 as usize;
    w.units[i].burning = true;
    let pos = w.units[i].pos;
    let mut twin = w.clone();
    twin.units[i].burning = false;
    for _ in 0..60 {
        w.step(&[]);
        twin.step(&[]);
    }
    let u = &w.units[i];
    assert!(u.burning && u.brigade.is_some() && !u.panicking() && u.body.flee == 0);
    assert_eq!(u.pos, pos, "he stays in the line");
    // The flag alone moves no one: everything but the burning flag (and its slow burn) agrees.
    for (a, b) in w.units.iter().zip(&twin.units) {
        assert_eq!(a.pos, b.pos);
    }
}
