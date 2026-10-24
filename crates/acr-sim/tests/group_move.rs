//! Group moves of many formations: the original's 32-formation cap (0x4ac997) and the
//! optional group-move rule (every formation moves; lines one behind the other, columns
//! side by side, loose units behind them).

use std::sync::Arc;

use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::{Formations, create_placed};
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, Rules, TypeId, World};

fn def(name: &str, extra: &str) -> UnitDef {
    UnitDef::parse(
        format!(
            "NAME {name}\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
             @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n{extra}[END]\r\n"
        )
        .as_bytes(),
    )
}

/// A line 15 wide and one deep, and a column 3 wide and 5 deep.
const ORDERS: &[u8] = b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n\
#COL15 SYM2 9 2 9 3 1 2\r\n C C\r\n\r\n***\r\n***\r\n***\r\n***\r\n***\r\n#END\r\n#EXIT\r\n";
const GROUPS: &[u8] = b"2\r\nLINE 1 0 1 1 #LINE15\r\nCOL 1 0 1 1 #COL15\r\n";

const ON: Rules =
    Rules { hit_locations: false, body_spacing: false, mortars: false, corpse_piles: false, group_move: true };

struct Setup {
    w: World,
    pik: TypeId,
    off: TypeId,
}

fn setup(rules: Rules) -> Setup {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &def("Pik", "")));
    let off = c.add(ObjectType::from_def("Off(EN)", &def("Off", "OFFICER\r\n")));
    let f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    let mut w = World::new(Arc::new(MapGrid::open(2)), Arc::new(c), Arc::new(f));
    w.rules = rules;
    Setup { w, pik, off }
}

/// A formation of `shape` standing at `(x, y)` facing `dir`; returns its members.
fn formation(s: &mut Setup, shape: &str, x: i32, y: i32, dir: u8) -> Vec<EntityId> {
    let shape = s.w.formations.shape_index(shape).unwrap();
    let n = s.w.formations.shapes[shape].slots();
    let officer = s.w.spawn(s.off, 0, Pos::from_pixels(x, y), dir);
    let soldiers: Vec<EntityId> = (0..n).map(|_| s.w.spawn(s.pik, 0, Pos::from_pixels(x, y), dir)).collect();
    let ids: Vec<usize> = soldiers.iter().map(|id| id.0 as usize).collect();
    create_placed(&mut s.w, (officer.0 as usize, None, None), &ids, shape, (x, y), dir);
    std::iter::once(officer).chain(soldiers).collect()
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

/// The centre of a brigade's target slots (px).
fn slot_centre(w: &World, b: usize) -> (i32, i32) {
    let s = &w.brigades[b].slots[3..];
    let n = s.len() as i32;
    (s.iter().map(|p| p.0).sum::<i32>() / n, s.iter().map(|p| p.1).sum::<i32>() / n)
}

/// 40 lines in a grid (8 across, 5 deep), all selected and sent 1500 px south.
fn forty(rules: Rules) -> (Setup, Vec<EntityId>) {
    let mut s = setup(rules);
    let mut all = Vec::new();
    for k in 0..40 {
        all.extend(formation(&mut s, "LINE15", 600 + (k % 8) * 560, 600 + (k / 8) * 120, 64));
    }
    run(&mut s.w, 5);
    s.w.step(&[Command::Move { player: 0, units: all.clone(), x: 2600, y: 2600 }]);
    (s, all)
}

#[test]
fn the_original_sends_only_32_formations() {
    let (s, _) = forty(Rules::default());
    let moved = (0..40).filter(|&b| slot_centre(&s.w, b).1 > 1500).count();
    assert_eq!(moved, 32, "brigades 32.. keep their places (0x4ac997)");
}

#[test]
fn the_rule_sends_every_formation() {
    let (s, _) = forty(ON);
    let moved = (0..40).filter(|&b| slot_centre(&s.w, b).1 > 1500).count();
    assert_eq!(moved, 40);
    // No two formations share a place.
    for a in 0..40 {
        for b in a + 1..40 {
            let (p, q) = (slot_centre(&s.w, a), slot_centre(&s.w, b));
            assert!((p.0 - q.0).abs() + (p.1 - q.1).abs() > 60, "{a} and {b} at {p:?} {q:?}");
        }
    }
}

#[test]
fn the_rule_is_deterministic_and_off_keeps_the_original() {
    let run_hash = |rules: Rules| {
        let (mut s, _) = forty(rules);
        run(&mut s.w, 200);
        s.w.state_hash()
    };
    assert_eq!(run_hash(ON), run_hash(ON));
    assert_ne!(run_hash(ON), run_hash(Rules::default()));
}

/// Three lines side by side, sent south: under the rule they stand one behind the other,
/// a gap apart, all facing the way.
#[test]
fn three_lines_stand_one_behind_the_other() {
    let mut s = setup(ON);
    let mut all = Vec::new();
    for k in 0..3 {
        all.extend(formation(&mut s, "LINE15", 1000 + k * 700, 1000, 0));
    }
    run(&mut s.w, 5);
    s.w.step(&[Command::Move { player: 0, units: all, x: 1700, y: 2600 }]);
    let mut c: Vec<(i32, i32)> = (0..3).map(|b| slot_centre(&s.w, b)).collect();
    c.sort_by_key(|p| p.1);
    for p in &c {
        assert!((p.0 - 1700).abs() < 20, "on the line of march: {c:?}");
    }
    // A line is 32 px deep; the gap between lines is 96 px.
    assert!((c[1].1 - c[0].1 - 128).abs() <= 4 && (c[2].1 - c[1].1 - 128).abs() <= 4, "{c:?}");
    assert!((c[1].1 - 2600).abs() <= 4, "the block is centred on the target: {c:?}");
    for b in 0..3 {
        let d = s.w.brigades[b].dir;
        assert!((i32::from(d) - 64).abs() <= 16, "facing south, not {d}");
    }
}

/// Columns stand side by side; loose units stand behind the formations.
#[test]
fn columns_side_by_side_and_loose_units_behind() {
    let mut s = setup(ON);
    let mut all = Vec::new();
    for k in 0..3 {
        all.extend(formation(&mut s, "COL15", 1000 + k * 300, 1000, 64));
    }
    for k in 0..30 {
        all.push(s.w.spawn(s.pik, 0, Pos::from_pixels(1100 + (k % 10) * 30, 800 + (k / 10) * 30), 64));
    }
    run(&mut s.w, 5);
    s.w.step(&[Command::Move { player: 0, units: all.clone(), x: 1300, y: 2600 }]);
    let mut c: Vec<(i32, i32)> = (0..3).map(|b| slot_centre(&s.w, b)).collect();
    c.sort_by_key(|p| p.0);
    for p in &c {
        assert!((p.1 - 2600).abs() <= 4, "one row: {c:?}");
    }
    assert!(c[1].0 - c[0].0 >= 96 && c[2].0 - c[1].0 >= 96, "a gap between columns: {c:?}");
    let back = (0..3).flat_map(|b| s.w.brigades[b].slots[3..].iter().map(|p| p.1)).min().unwrap();
    // The loose units walk there and stand behind the rear rank, not in it.
    run(&mut s.w, 3000);
    let mut at: Vec<(i32, i32)> = Vec::new();
    for id in &all[all.len() - 30..] {
        let t = s.w.unit(*id).unwrap().pos.pixels();
        assert!(t.1 < back - 32, "loose unit at {t:?} behind the formations (rear slot y {back})");
        assert!((t.1 - 2600).abs() < 600, "arrived: {t:?}");
        assert!(!at.contains(&t), "two loose units on {t:?}");
        at.push(t);
    }
}

/// One formation with loose units moves as in the original.
#[test]
fn one_formation_moves_as_in_the_original() {
    let go = |rules: Rules| {
        let mut s = setup(rules);
        let mut all = formation(&mut s, "LINE15", 1000, 1000, 0);
        for k in 0..10 {
            all.push(s.w.spawn(s.pik, 0, Pos::from_pixels(900 + k * 30, 900), 64));
        }
        run(&mut s.w, 5);
        s.w.step(&[Command::Move { player: 0, units: all, x: 1700, y: 2000 }]);
        run(&mut s.w, 300);
        s.w.units.iter().map(|u| (u.pos, u.dir)).collect::<Vec<_>>()
    };
    assert_eq!(go(ON), go(Rules::default()));
}
