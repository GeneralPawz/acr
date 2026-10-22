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

const CARRIER: &str = "NAME Car\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
    @MOTION_L4 9 0 20 29\r\n@MOTION_R4 9 0 30 39\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nRATE 4 8\r\n[END]\r\n";

#[test]
fn a_stance_walk_goes_at_its_rate() {
    let mut c = Catalog::new();
    let car = c.add(ObjectType::from_def("Car(EN)", &UnitDef::parse(CARRIER.as_bytes())));
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
    let a = w.spawn(car, 0, Pos::from_pixels(2000, 2000), 0);
    let b = w.spawn(car, 0, Pos::from_pixels(2000, 2400), 0);
    // b walks in stance 5 (#MOTION_L4, RATE 4 8): half the speed.
    {
        let u = &mut w.units[b.0 as usize];
        (u.stance, u.local_stance) = (5, 5);
    }
    w.step(&[Command::Move { player: 0, units: vec![a], x: 3000, y: 2000 }]);
    acr_sim::motion::send_to(&mut w.units[b.0 as usize], Pos::from_pixels(3000, 2400), 0x10, true);
    w.units[b.0 as usize].stance = 5;
    run(&mut w, 60);
    let (xa, _) = px(&w, a);
    let (xb, _) = px(&w, b);
    assert!(w.units[b.0 as usize].anim.name().starts_with("#MOTION_"), "{:?}", w.units[b.0 as usize].anim);
    let (da, db) = (xa - 2000, xb - 2000);
    assert!((db * 2 - da).abs() <= 30, "stance 5 walked {db} px, stance 0 {da} px");
}

#[test]
fn a_move_with_a_direction_ends_facing_it() {
    let (mut w, mus, _) = setup();
    let id = w.spawn(mus, 0, Pos::from_pixels(1000, 1000), 0);
    w.step(&[Command::MoveFacing { player: 0, units: vec![id], x: 1200, y: 1000, dir: 64 }]);
    run(&mut w, 200);
    let u = w.unit(id).unwrap();
    assert_eq!(u.dir, 64, "turned after arriving: {:?}", u.orders);
    assert!(u.orders.is_empty());
    assert!((px(&w, id).0 - 1200).abs() < 40);
}

fn script_move(units: Vec<EntityId>, x: i32, y: i32, dir: u16, ord: u8, off: (i32, i32)) -> Command {
    Command::ScriptMove { player: 0, units, x, y, dir, prio: 0x10, ord, abs_dir: false, off_x: off.0, off_y: off.1 }
}

#[test]
fn a_script_move_shifts_its_target_along_the_facing() {
    // GroupSend 0x4accbd: facing east, 100 px ahead and 50 px across.
    let (mut w, mus, _) = setup();
    let id = w.spawn(mus, 0, Pos::from_pixels(1000, 1000), 0);
    w.step(&[script_move(vec![id], 1400, 1000, 0, 0, (100, 50))]);
    run(&mut w, 300);
    let (x, y) = px(&w, id);
    assert!((x - 1500).abs() < 24 && (y - 1050).abs() < 24, "at ({x}, {y})");
    assert_eq!(w.unit(id).unwrap().dir, 0);
}

#[test]
fn script_moves_place_their_orders_by_order_type() {
    // CreateOrder 0x525752: type 2 appends the move (and the turn) behind the current orders,
    // type 1 puts the turn and then the move in front of them. A plain move also ends with a
    // turn to the direction of travel (0x4acc74).
    let (mut w, mus, _) = setup();
    let a = w.spawn(mus, 0, Pos::from_pixels(1000, 1000), 0);
    let b = w.spawn(mus, 0, Pos::from_pixels(1000, 1400), 0);
    w.step(&[Command::Move { player: 0, units: vec![a, b], x: 1000, y: 3000 }]);
    assert_eq!(w.unit(a).unwrap().orders.len(), 2);
    let first = w.unit(a).unwrap().orders.last().cloned().unwrap();
    w.step(&[script_move(vec![a], 3000, 1000, 64, 2, (0, 0)), script_move(vec![b], 3000, 1400, 64, 1, (0, 0))]);
    let ua = w.unit(a).unwrap();
    assert_eq!(ua.orders.len(), 4, "{:?}", ua.orders);
    assert_eq!(ua.orders.last(), Some(&first));
    assert!(matches!(ua.orders[0], acr_sim::world::Order::Turn { dir: 64 }));
    let ub = w.unit(b).unwrap();
    assert_eq!(ub.orders.len(), 4, "{:?}", ub.orders);
    assert!(matches!(ub.orders[2], acr_sim::world::Order::Turn { dir: 64 }));
    assert_ne!(ub.orders.last(), w.unit(a).unwrap().orders.last());
    // Types 3..7 send no free unit.
    let c = w.spawn(mus, 0, Pos::from_pixels(1400, 1000), 0);
    w.step(&[script_move(vec![c], 3000, 1000, 64, 3, (0, 0))]);
    assert!(w.unit(c).unwrap().orders.is_empty());
}

#[test]
fn a_script_move_turns_a_line_of_formations_with_it() {
    // GroupSend 0x4ace11: the brigades keep their places relative to the group, turned by the
    // new facing minus their mean facing.
    let (mut w, mus, off) = setup();
    let mut officers = Vec::new();
    for k in 0..2 {
        let x = 1500 + 600 * k;
        let officer = w.spawn(off, 0, Pos::from_pixels(x, 2000), 0);
        let soldiers: Vec<EntityId> = (0..15)
            .map(|i| w.spawn(mus, 0, Pos::from_pixels(x - 100 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0))
            .collect();
        w.step(&[Command::Form { player: 0, officer, units: soldiers, kind: "LINE".into() }]);
        officers.push(officer);
    }
    run(&mut w, 200);
    let d0 = w.brigades[0].dir;
    assert_eq!(w.brigades[1].dir, d0);
    let ids: Vec<EntityId> = w.units.iter().filter(|u| u.alive).map(|u| u.id).collect();
    w.step(&[script_move(ids, 2000, 3500, u16::from(d0.wrapping_add(64)), 0, (0, 0))]);
    run(&mut w, 600);
    let c: Vec<(i32, i32)> = (0..2).map(|b| acr_sim::formation::center(&w, b).unwrap()).collect();
    let (dx, dy) = ((c[1].0 - c[0].0) >> 4, (c[1].1 - c[0].1) >> 4);
    assert!(dx.abs() < 100 && dy.abs() > 400, "brigade 1 is ({dx}, {dy}) from brigade 0");
    assert_eq!(w.brigades[0].dir, d0.wrapping_add(64));
}

#[test]
fn formations_group_and_ungroup() {
    let (mut w, mus, off) = setup();
    let mut officers = Vec::new();
    for k in 0..3 {
        let x = 1500 + 600 * k;
        let officer = w.spawn(off, 0, Pos::from_pixels(x, 2000), 0);
        let soldiers: Vec<EntityId> = (0..15)
            .map(|i| w.spawn(mus, 0, Pos::from_pixels(x - 100 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0))
            .collect();
        w.step(&[Command::Form { player: 0, officer, units: soldiers, kind: "LINE".into() }]);
        officers.push(officer);
    }
    assert_eq!(w.brigades.len(), 3);
    let hash = w.state_hash();
    // Another player's units and free units do not count.
    let free = w.spawn(mus, 0, Pos::from_pixels(900, 900), 0);
    w.step(&[Command::GroupFormations { player: 1, units: officers.clone() }]);
    assert!(w.formation_groups[1].is_empty() && w.formation_groups[0].is_empty());
    w.step(&[Command::GroupFormations { player: 0, units: vec![officers[1], free, officers[0]] }]);
    assert_eq!(w.formation_groups[0].sets, vec![vec![1, 0]]);
    assert_ne!(w.state_hash(), hash);
    // Grouping again moves a formation into the new group.
    w.step(&[Command::GroupFormations { player: 0, units: vec![officers[2], officers[0]] }]);
    assert_eq!(w.formation_groups[0].sets, vec![vec![1], vec![2, 0]]);
    assert!(w.formation_groups[0].is_group(&[0, 2]));
    w.step(&[Command::UngroupFormations { player: 0, units: vec![officers[2]] }]);
    assert_eq!(w.formation_groups[0].sets, vec![vec![1], vec![0]]);
    // Group [1] empties: [0] moves down but keeps the count 0 of [1] (as in the original),
    // so it can no longer be found.
    w.step(&[Command::UngroupFormations { player: 0, units: vec![officers[1]] }]);
    assert_eq!(w.formation_groups[0].sets, vec![vec![0]]);
    assert_eq!(w.formation_groups[0].counts, vec![0]);
    assert!(!w.formation_groups[0].is_group(&[0]));
    w.step(&[Command::GroupFormations { player: 0, units: officers.clone() }]);
    assert_eq!(w.formation_groups[0].sets, vec![vec![0], vec![0, 1, 2]]);
    assert!(w.formation_groups[0].is_group(&[2, 1, 0]));
}
