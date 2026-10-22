//! Body spacing (optional rule `Rules::body_spacing`, `docs/body-spacing.md`): men take up
//! space and push each other apart; with the rule off nothing changes.

use std::sync::Arc;
use std::time::Instant;

use acr_formats::defs::UnitDef;
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, Rules, TypeId, World};

const PIKEMAN: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";
const PEASANT: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    LIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\nBUILDSTAGES 20\r\nPRICE 1 FOOD 50\r\n[END]\r\n";
const CENTER: &str = "NAME Cen\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\nBORNPOINTS 2 2 2 2 6\r\n\
    CONCENTRATOR 1 2 3\r\nMFARM 40\r\nBUILDSTAGES 10\r\nPRICE 1 WOOD 100\r\nLOCKPOINTS 9 0 0 1 0 2 0 0 1 1 1 2 1 0 2 1 2 2 2\r\n\
    [END]\r\n";

const ON: Rules = Rules { hit_locations: false, body_spacing: true };
/// Two men's radii, 1/16 px (2 × 0.3 m at 30 px per metre).
const TWO_MEN: i32 = 2 * 144;

fn world(rules: Rules) -> (World, TypeId, TypeId, TypeId) {
    let mut c = Catalog::new();
    let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
    let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(PEASANT.as_bytes())));
    let cen = c.add(ObjectType::from_def("Cen(EN)", &UnitDef::parse(CENTER.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    w.rules = rules;
    (w, pik, kri, cen)
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

fn dist(w: &World, a: EntityId, b: EntityId) -> i32 {
    let (p, q) = (w.units[a.0 as usize].pos, w.units[b.0 as usize].pos);
    let (dx, dy) = (i64::from(p.x - q.x), i64::from(p.y - q.y));
    ((dx * dx + dy * dy) as f64).sqrt() as i32
}

fn min_gap(w: &World, ids: &[EntityId]) -> i32 {
    let mut m = i32::MAX;
    for (k, &a) in ids.iter().enumerate() {
        for &b in &ids[k + 1..] {
            m = m.min(dist(w, a, b));
        }
    }
    m
}

/// Each unit its own move order to the same point (no group placement).
fn all_to(w: &mut World, ids: &[EntityId], x: i32, y: i32) {
    let cmds: Vec<Command> = ids.iter().map(|&id| Command::Move { player: 0, units: vec![id], x, y }).collect();
    w.step(&cmds);
}

#[test]
fn two_men_ordered_to_one_point_end_up_apart() {
    for (rules, apart) in [(Rules::default(), false), (ON, true)] {
        let (mut w, pik, ..) = world(rules);
        let a = w.spawn(pik, 0, Pos::from_pixels(1000, 1000), 0);
        let b = w.spawn(pik, 0, Pos::from_pixels(1000, 1200), 0);
        all_to(&mut w, &[a, b], 1300, 1100);
        run(&mut w, 300);
        let d = dist(&w, a, b);
        println!("rules {rules:?}: {} px apart", d / 16);
        if apart {
            assert!(d >= TWO_MEN - 16, "{d}");
        }
    }
}

/// A 200-man crowd converging on one point; returns the smallest gap and the RMS radius in
/// px, the pairs closer than 16 px and the state hash.
fn crowd(rules: Rules) -> (i32, i32, i32, u64) {
    let (mut w, pik, ..) = world(rules);
    let ids: Vec<EntityId> =
        (0..200).map(|k| w.spawn(pik, 0, Pos::from_pixels(800 + 40 * (k % 14), 800 + 40 * (k / 14)), 0)).collect();
    all_to(&mut w, &ids, 2000, 2000);
    run(&mut w, 1200);
    let n = ids.len() as i64;
    let (sx, sy) = ids.iter().fold((0i64, 0i64), |(x, y), id| {
        let p = w.units[id.0 as usize].pos;
        (x + i64::from(p.x), y + i64::from(p.y))
    });
    let (cx, cy) = (sx / n, sy / n);
    let ms = ids.iter().map(|id| {
        let p = w.units[id.0 as usize].pos;
        let (dx, dy) = (i64::from(p.x) - cx, i64::from(p.y) - cy);
        dx * dx + dy * dy
    });
    let rms = ((ms.sum::<i64>() / n) as f64).sqrt() as i32;
    let mut close = 0;
    for (k, &a) in ids.iter().enumerate() {
        for &b in &ids[k + 1..] {
            close += i32::from(dist(&w, a, b) < 16 * 16);
        }
    }
    (min_gap(&w, &ids) / 16, rms / 16, close, w.state_hash())
}

#[test]
fn a_crowd_fills_an_area_as_big_as_its_bodies() {
    let (gap_off, rms_off, close_off, _) = crowd(Rules::default());
    let (gap, rms, close, _) = crowd(ON);
    println!("off: smallest gap {gap_off} px, RMS radius {rms_off} px, {close_off} pairs under 16 px");
    println!("on: smallest gap {gap} px, RMS radius {rms} px, {close} pairs under 16 px");
    // 200 discs of 9 px: 50 900 px², a disc of radius 127 px (RMS radius 90 px) packed
    // perfectly; densely packed about 100 px. The occupancy grid's 16 px cells keep a
    // pressing crowd near a 16 px pitch, so a few pairs stay a little under 18 px.
    assert!(gap >= 13, "bodies overlap: smallest gap {gap} px");
    assert!(close * 10 < close_off.max(10), "{close} pairs under 16 px (off: {close_off})");
    assert!((80..=180).contains(&rms), "RMS radius {rms} px");
}

#[test]
fn with_the_rule_off_nothing_changes() {
    // The off rule is the default: same state, same hash as before the rule existed (the
    // pinned hashes of the other tests check that); on, the crowd ends elsewhere.
    let off = crowd(Rules { hit_locations: false, body_spacing: false });
    assert_eq!(off, crowd(Rules::default()));
    assert_ne!(off.3, crowd(ON).3);
}

#[test]
fn a_column_marches_through_a_crowd() {
    for rules in [Rules::default(), ON] {
        let (mut w, pik, ..) = world(rules);
        // A crowd of 100 idle men, 20 px apart, between the column and its goal.
        for k in 0..100 {
            w.spawn(pik, 0, Pos::from_pixels(1500 + 20 * (k % 10), 1400 + 20 * (k / 10)), 0);
        }
        let column: Vec<EntityId> = (0..10).map(|k| w.spawn(pik, 0, Pos::from_pixels(1300, 1460 + 8 * k), 0)).collect();
        all_to(&mut w, &column, 1900, 1490);
        run(&mut w, 1500);
        let through = column.iter().filter(|id| w.units[id.0 as usize].pos.pixels().0 > 1750).count();
        println!("rules {rules:?}: {through} of 10 through");
        if rules.body_spacing {
            assert!(through >= 8, "{through} of 10 through");
        }
    }
}

#[test]
fn units_out_of_a_building_are_not_stuck() {
    let (mut w, _, kri, cen) = world(ON);
    let center = w.spawn(cen, 0, Pos::from_pixels(2048, 2048), 0);
    acr_sim::economy::census_all(&mut w);
    w.players[0].resources = [100_000; 8];
    for _ in 0..6 {
        w.step(&[Command::Produce { player: 0, building: center, kind: kri }]);
    }
    run(&mut w, 600);
    let men: Vec<EntityId> = w.units.iter().filter(|u| u.kind == kri).map(|u| u.id).collect();
    assert_eq!(men.len(), 6);
    for &m in &men {
        let u = &w.units[m.0 as usize];
        assert!(!u.unlimited && u.orders.is_empty(), "{:?}", u.orders);
        assert!(!w.map.land.check_bar(u.cell.0, u.cell.1, 1, 1), "on a lock cell {:?}", u.cell);
    }
    let gap = min_gap(&w, &men);
    assert!(gap >= TWO_MEN - 32, "spread out: smallest gap {} px", gap / 16);
}

#[test]
fn cost_of_2000_men_in_one_crowd() {
    let mut cost = Vec::new();
    for rules in [Rules::default(), ON] {
        let (mut w, pik, ..) = world(rules);
        let ids: Vec<EntityId> = (0..2000)
            .map(|k| w.spawn(pik, 0, Pos::from_pixels(1000 + 12 * (k % 50), 1000 + 12 * (k / 50)), 0))
            .collect();
        all_to(&mut w, &ids, 1300, 1240);
        run(&mut w, 20);
        let t = Instant::now();
        run(&mut w, 40);
        cost.push(t.elapsed().as_secs_f64() * 1000.0 / 40.0);
    }
    println!(
        "2000 men: {:.2} ms per tick off, {:.2} ms on, rule {:.2} ms (this build)",
        cost[0],
        cost[1],
        cost[1] - cost[0]
    );
}
