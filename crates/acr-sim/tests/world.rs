//! World-level behaviour on small synthetic maps.

use std::sync::Arc;

use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;
use acr_sim::world::EntityId;
use acr_sim::{Command, Pos, TypeId, World};

const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";
const OFFICER: &str = "NAME Off\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nOFFICER\r\n[END]\r\n";
const ORDERS: &[u8] = b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n";
const GROUPS: &[u8] = b"1\r\nLINE 0 1 #LINE15\r\n";

fn setup() -> (World, TypeId, TypeId) {
    let mut c = Catalog::new();
    let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
    let off = c.add(ObjectType::from_def("Off(EN)", &UnitDef::parse(OFFICER.as_bytes())));
    let f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(GROUPS));
    (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f)), mus, off)
}

fn px(w: &World, id: EntityId) -> (i32, i32) {
    w.unit(id).unwrap().pos.pixels()
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[]);
    }
}

#[test]
fn infantry_walks_five_pixels_per_tick() {
    let (mut w, mus, _) = setup();
    let id = w.spawn(mus, 0, Pos::from_pixels(1000, 1000), 0);
    w.step(&[Command::Move { player: 0, units: vec![id], x: 2000, y: 1000 }]);
    // First tick: the order link and the decision step move 80/16 = 5 px.
    let start = px(&w, id).0;
    let mut xs = vec![start];
    for _ in 0..10 {
        w.step(&[]);
        xs.push(px(&w, id).0);
    }
    for pair in xs.windows(2) {
        assert_eq!(pair[1] - pair[0], 5, "{xs:?}");
    }
}

#[test]
fn unit_arrives_and_stops() {
    let (mut w, mus, _) = setup();
    let id = w.spawn(mus, 0, Pos::from_pixels(1000, 1000), 0);
    w.step(&[Command::Move { player: 0, units: vec![id], x: 1500, y: 1300 }]);
    run(&mut w, 300);
    let (x, y) = px(&w, id);
    assert!((x - 1500).abs() <= 24 && (y - 1300).abs() <= 24, "at ({x},{y})");
    let u = w.unit(id).unwrap();
    assert!(u.orders.is_empty() && !u.in_motion);
}

#[test]
fn unit_turns_in_place_before_walking() {
    let (mut w, mus, _) = setup();
    let id = w.spawn(mus, 0, Pos::from_pixels(1000, 1000), 0);
    // Target due west: a 128 turn takes 7 ticks of 16 before the unit walks.
    w.step(&[Command::Move { player: 0, units: vec![id], x: 500, y: 1000 }]);
    for _ in 0..6 {
        w.step(&[]);
        assert_eq!(px(&w, id), (1000, 1000));
    }
    run(&mut w, 2);
    assert!(px(&w, id).0 < 1000);
}

#[test]
fn walks_around_a_wall() {
    let (mut w, mus, _) = setup();
    let mut map = (*w.map).clone();
    map.land.set_rect(80, 40, 4, 40); // 16 px cells: a wall at x 1280..1344, y 640..1280
    w.map = Arc::new(map);
    let id = w.spawn(mus, 0, Pos::from_pixels(1000, 960), 0);
    w.step(&[Command::Move { player: 0, units: vec![id], x: 1700, y: 960 }]);
    run(&mut w, 600);
    let (x, y) = px(&w, id);
    assert!((x - 1700).abs() <= 24 && (y - 960).abs() <= 24, "at ({x},{y})");
}

#[test]
fn group_move_spreads_units() {
    let (mut w, mus, _) = setup();
    let ids: Vec<EntityId> =
        (0..20).map(|i| w.spawn(mus, 0, Pos::from_pixels(1000 + 30 * (i % 5), 1000 + 30 * (i / 5)), 0)).collect();
    w.step(&[Command::Move { player: 0, units: ids.clone(), x: 2500, y: 1500 }]);
    run(&mut w, 900);
    let mut cells: Vec<(i32, i32)> = ids.iter().map(|&id| w.unit(id).unwrap().cell).collect();
    for &id in &ids {
        let (x, y) = px(&w, id);
        assert!((x - 2500).abs() < 250 && (y - 1500).abs() < 250, "{id:?} at ({x},{y})");
    }
    cells.sort();
    cells.dedup();
    assert_eq!(cells.len(), ids.len(), "every unit has its own cell");
}

#[test]
fn formation_forms_and_moves() {
    let (mut w, mus, off) = setup();
    let officer = w.spawn(off, 0, Pos::from_pixels(2000, 2000), 0);
    let soldiers: Vec<EntityId> =
        (0..15).map(|i| w.spawn(mus, 0, Pos::from_pixels(1900 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0)).collect();
    w.step(&[Command::Form { player: 0, officer, units: soldiers.clone(), kind: "LINE".into() }]);
    run(&mut w, 300);
    assert_eq!(w.brigades.len(), 1);
    let b = &w.brigades[0];
    for (m, slot) in b.members.iter().zip(&b.slots) {
        let Some(id) = m else { continue };
        let (x, y) = px(&w, *id);
        assert!((x - slot.0).abs() <= 24 && (y - slot.1).abs() <= 24, "{id:?} at ({x},{y}), slot {slot:?}");
    }
    // Move east: the line is re-placed around the target, facing east.
    w.step(&[Command::Move { player: 0, units: vec![officer], x: 3000, y: 2000 }]);
    run(&mut w, 600);
    let b = &w.brigades[0];
    assert_eq!(b.dir, 0);
    for (m, slot) in b.members.iter().zip(&b.slots) {
        let Some(id) = m else { continue };
        let (x, y) = px(&w, *id);
        assert!((x - slot.0).abs() <= 24 && (y - slot.1).abs() <= 24, "{id:?} at ({x},{y}), slot {slot:?}");
        assert!((slot.0 - 3000).abs() <= 64, "line centred on the target: {slot:?}");
    }
}

#[test]
fn deterministic_and_command_sensitive() {
    let run_world = |target: i32| {
        let (mut w, mus, _) = setup();
        let ids: Vec<EntityId> =
            (0..30).map(|i| w.spawn(mus, (i % 2) as u8, Pos::from_pixels(1000 + 25 * i, 1200), 0)).collect();
        let even: Vec<EntityId> = ids.iter().copied().step_by(2).collect();
        w.step(&[Command::Move { player: 0, units: even, x: target, y: 2000 }]);
        run(&mut w, 200);
        w.state_hash()
    };
    assert_eq!(run_world(1800), run_world(1800));
    assert_ne!(run_world(1800), run_world(1900));
}

#[test]
fn replay_reproduces_the_match() {
    use acr_sim::replay::Replay;
    let script = |tick: u64, ids: &[EntityId]| -> Vec<Command> {
        match tick {
            0 => vec![Command::Move { player: 0, units: ids[..5].to_vec(), x: 2000, y: 1500 }],
            40 => vec![Command::Move { player: 0, units: ids[5..].to_vec(), x: 1200, y: 2200 }],
            90 => vec![Command::Stop { player: 0, units: ids[..2].to_vec() }],
            _ => Vec::new(),
        }
    };
    let (base, mus, _) = setup();
    let mut base = base;
    let ids: Vec<EntityId> = (0..10).map(|i| base.spawn(mus, 0, Pos::from_pixels(1000 + 30 * i, 1000), 0)).collect();
    let mut w = base.clone();
    let mut rec = Replay::new("test", 0);
    for tick in 0..200 {
        let cmds = script(tick, &ids);
        rec.record(tick, &cmds);
        w.step(&cmds);
    }
    rec.checkpoints.push((200, w.state_hash()));
    let replay = Replay::decode(&rec.encode()).unwrap();
    let mut again = base.clone();
    for tick in 0..200 {
        again.step(replay.commands_at(tick));
    }
    assert_eq!(again.state_hash(), replay.checkpoints[0].1);
}
