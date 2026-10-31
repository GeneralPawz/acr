//! The controller on a synthetic flat map: batteries and the cadence. The world builder here
//! is shared by the behaviours added later.

use std::sync::Arc;

use acr_formats::defs::nres::weapon_kinds;
use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
use acr_sim::catalog::{Catalog, ObjectType};
use acr_sim::formation::{Formations, create_placed};
use acr_sim::map::MapGrid;
use acr_sim::weapon::Weapons;
use acr_sim::{Command, EntityId, Pos, TypeId, World};

use super::frame::{dist, to_army, to_world};
use super::picture;
use super::{Army, Body, Commander, Control, Duty, PERIOD, Style};

const OFFICER: &str = "NAME Off\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nOFFICER\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
/// A musketeer that never searches for enemies: a target for the guns.
const MUSKET: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 9\r\n#DEATHLIE1 9 1 0 9\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    ATTACK_RADIUS 0 0 850\r\nWEAPON 0 POINT\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const HORSE: &str = "NAME Hor\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 9\r\n#DEATHLIE1 9 1 0 9\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    USAGE HARDHORCE\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
/// Mounted shooters with the armed-attack switch (the dragoons).
const DRAGOON: &str = "NAME Dra\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 9\r\n#DEATHLIE1 9 1 0 9\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    USAGE STRELOK\r\nARMATTACK\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
const CANNON: &str = "NAME Can\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 16 1 0 0\r\n@MOTION_L 16 0 0 9\r\n\
    @MOTION_R 16 0 10 19\r\n@MOTION_L0 16 0 0 9\r\n@MOTION_R0 16 0 10 19\r\n@MOTION_L1 16 0 0 19\r\n\
    @MOTION_R1 16 0 0 19\r\n@PATTACK4 16 0 0 19\r\n@UATTACK4 16 0 19 0\r\n@ATTACK4 16 0 0 39\r\n\
    SETACTIVEPOINT #ATTACK4 10 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0 10 0\r\n\
    LIFE 1500\r\nGEOMETRY 1 10 40\r\nMOTIONSTYLE COMPLEXROTATE\r\nUSAGE PUSHKA\r\nARTPODGOTOVKA\r\n\
    ATTACK_RADIUS 4 300 6000\r\nATTACK_PAUSE 4 100\r\nWEAPON 4 PPOINTT\r\nWEAPONKIND 4 IADRO\r\n\
    CANKILL 1 BODY\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
const NDS: &[u8] = b"[MEMBERS]\r\nPOINT #POINT 0 1 NONE HI_GRAVITY1 RANDOM 3 -1\r\n\
    PPOINTT #POINT 0 0 NONE NO_GRAVITY SLIGHTUP 0 1\r\nNUCL2P #NUCL1 340 35 NONE HI_GRAVITY ANGLE 10 -1\r\n\
    [SYNCWEAPON]\r\nPPOINTT 1 NUCL2P\r\n[END]\r\n";
const ADS: &[u8] = b"@POINT 1 0 0 0\r\n@NUCL1 1 0 0 0\r\n";
const NRES: &[u8] = b"[WEAPONS]\r\n#GENSHIELD 13\r\nMECH 21 19 O\r\nIADRO 2 15 HX\r\n[UNITKINDS]\r\n";
const ORDERS: &[u8] = b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n\
#LINE12 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n************\r\n#END\r\n\
#KARE12 SYM4 9 2 9 3\r\n****\r\n*CC*\r\n*C *\r\n****\r\n#END\r\n#EXIT\r\n";
/// Lines of 12 or 15 that can form a square (the button lists the group `KARE`).
const GROUPS: &[u8] = b"2\r\nLINE 2 0 1 2 #LINE12 #LINE15\r\nKARE 2 0 1 1 #KARE12\r\n";
/// Lines without that button.
const GROUPS_NO_KARE: &[u8] = b"2\r\nLINE 0 2 #LINE12 #LINE15\r\nKARE 0 1 #KARE12\r\n";
/// A pikeman: no ranged attack.
const PIKE: &str = "NAME Pik\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
    @MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 9\r\n#DEATHLIE1 9 1 0 9\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
    ATTACK_RADIUS 0 0 100\r\nMATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";

/// The unit types of the synthetic battles and the world they stand in.
pub(super) struct Rig {
    pub w: World,
    pub off: TypeId,
    pub mus: TypeId,
    pub horse: TypeId,
    pub pike: TypeId,
    pub dragoon: TypeId,
    pub gun: TypeId,
}

pub(super) fn rig() -> Rig {
    rig_with(GROUPS)
}

pub(super) fn rig_with(groups: &[u8]) -> Rig {
    let mut c = Catalog::new();
    let mut add = |name: &str, def: &str| c.add(ObjectType::from_def(name, &UnitDef::parse(def.as_bytes())));
    let (off, mus) = (add("Off(EN)", OFFICER), add("Mus(EN)", MUSKET));
    let (horse, gun) = (add("Hor(EN)", HORSE), add("Can(EN)", CANNON));
    let pike = add("Pik(EN)", PIKE);
    let dragoon = add("Dra(EN)", DRAGOON);
    c.set_weapon_kinds(&weapon_kinds(NRES));
    c.set_weapons(Weapons::parse(NDS, Some(ADS), None));
    let f = Formations::new(&OrderList::parse(ORDERS), &OrderGroups::parse(groups));
    Rig { w: World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f)), off, mus, horse, pike, dragoon, gun }
}

/// `n` units of `kind` in rows of `row`, `gap` px apart, from `(x, y)` facing `dir`.
pub(super) fn crowd(
    w: &mut World,
    kind: TypeId,
    owner: u8,
    (x, y): (i32, i32),
    n: usize,
    (row, gap): (usize, i32),
    dir: u8,
) -> Vec<EntityId> {
    (0..n)
        .map(|i| w.spawn(kind, owner, Pos::from_pixels(x + gap * (i % row) as i32, y + gap * (i / row) as i32), dir))
        .collect()
}

/// An officer and 12 soldiers of `kind` formed as `shape` at `(x, y)` facing `dir`.
pub(super) fn formed(kind: TypeId, r: &mut Rig, owner: u8, at: (i32, i32), dir: u8, shape: &str) -> Vec<EntityId> {
    let shape = r.w.formations.shape_index(shape).unwrap();
    let officer = r.w.spawn(r.off, owner, Pos::from_pixels(at.0, at.1), dir);
    let soldiers = crowd(&mut r.w, kind, owner, at, 12, (12, 0), dir);
    let ids: Vec<usize> = soldiers.iter().map(|id| id.0 as usize).collect();
    create_placed(&mut r.w, (officer.0 as usize, None, None), &ids, shape, at, dir);
    std::iter::once(officer).chain(soldiers).collect()
}

pub(super) fn body(duty: Duty, units: &[EntityId]) -> Body {
    Body { duty, units: units.to_vec(), lateral: 0 }
}

pub(super) fn army(owner: u8, facing: u8, bodies: Vec<Body>) -> Army {
    Army { owner, facing, origin: (0, 0), style: Style::Linear, bodies }
}

/// Runs the controller and the world for `ticks`, returning every order given.
pub(super) fn run(w: &mut World, cm: &mut Commander, ticks: usize) -> Vec<Command> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        let cmds = cm.orders(w);
        w.step(&cmds);
        all.extend(cmds);
    }
    all
}

/// The target of the attack ordered for `gun`, if any.
fn aimed(cmds: &[Command], gun: EntityId) -> Option<EntityId> {
    cmds.iter().find_map(|c| match c {
        Command::Attack { units, target, .. } if units.contains(&gun) => Some(*target),
        _ => None,
    })
}

#[test]
fn frame_round_trips() {
    for dir in (0..=255u8).step_by(5) {
        for (f, s) in [(0, 0), (1200, -300), (-700, 2500), (4000, 4000), (-3500, -90)] {
            let (dx, dy) = to_world(dir, f, s);
            let (f2, s2) = to_army(dir, dx, dy);
            // The tables' sines and cosines are not exactly unit: the way back is within 1 %.
            let slack = 2 + f.abs().max(s.abs()) / 100;
            assert!((f - f2).abs() <= slack && (s - s2).abs() <= slack, "dir {dir}: ({f}, {s}) -> ({f2}, {s2})");
        }
    }
    // East is +x, south +y; the right-hand side of a south-facing army is west.
    let near = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1;
    assert!(near(to_world(0, 100, 50), (100, 50)));
    assert!(near(to_world(64, 100, 50), (-50, 100)), "{:?}", to_world(64, 100, 50));
    assert_eq!(dist((0, 0), (300, 400)), 500);
}

#[test]
fn decides_only_on_its_ticks_and_repeats_a_tick() {
    let mut r = rig();
    let g0 = crowd(&mut r.w, r.gun, 0, (2000, 4000), 1, (1, 40), 0);
    let g1 = crowd(&mut r.w, r.gun, 1, (6000, 4000), 1, (1, 40), 128);
    let f0 = crowd(&mut r.w, r.mus, 0, (5600, 4000), 4, (2, 20), 128);
    let f1 = crowd(&mut r.w, r.mus, 1, (2400, 4000), 4, (2, 20), 0);
    let armies = [
        army(0, 0, vec![body(Duty::Guns, &g0), body(Duty::Line, &f0)]),
        army(1, 128, vec![body(Duty::Guns, &g1), body(Duty::Line, &f1)]),
    ];
    let mut cm = Commander::new(&r.w, armies, [Control::Ai; 2]);
    let first = cm.orders(&r.w);
    assert_eq!(first.len(), 2, "side 0 decides at tick 0: attack and stand ground");
    assert_eq!(cm.orders(&r.w), first, "the same tick repeats its orders");
    assert_eq!(cm.report(0).gun_orders, 1, "and counts them once");
    assert!(
        first.iter().all(|c| matches!(c, Command::Attack { player: 0, .. } | Command::StandGround { player: 0, .. }))
    );
    r.w.step(&first);
    for t in 1..PERIOD {
        let cmds = cm.orders(&r.w);
        if t == 10 {
            assert_eq!(cmds.len(), 2, "side 1 decides at tick 10");
            assert!(
                cmds.iter()
                    .all(|c| matches!(c, Command::Attack { player: 1, .. } | Command::StandGround { player: 1, .. }))
            );
        } else {
            assert!(cmds.is_empty(), "tick {t}: {cmds:?}");
        }
        r.w.step(&cmds);
    }
    assert!(cm.orders(&r.w).is_empty(), "side 0 again at tick {PERIOD}: nothing changed, nothing ordered");
}

#[test]
fn guns_never_get_a_move_order_and_stay_put() {
    let mut r = rig();
    let guns = crowd(&mut r.w, r.gun, 0, (2000, 4000), 3, (3, 60), 0);
    let foe = crowd(&mut r.w, r.mus, 1, (3500, 3900), 24, (6, 24), 128);
    let armies = [army(0, 0, vec![body(Duty::Guns, &guns)]), army(1, 128, vec![body(Duty::Line, &foe)])];
    let mut cm = Commander::new(&r.w, armies, [Control::Ai; 2]);
    let all = run(&mut r.w, &mut cm, 500);
    assert!(all.iter().any(|c| matches!(c, Command::Attack { .. })), "the guns were ordered to fire");
    for c in &all {
        assert!(matches!(c, Command::Attack { .. } | Command::StandGround { on: true, .. }), "a gun got {c:?}");
    }
    assert!(foe.iter().any(|&id| r.w.units[id.0 as usize].life < 100), "the guns hit");
    assert!(cm.report(0).guns_drift_max <= 8, "drift {}", cm.report(0).guns_drift_max);
    assert_eq!(cm.report(0).alive, 3);
    for &g in &guns {
        assert!(r.w.units[g.0 as usize].stand_ground);
    }
}

#[test]
fn gun_prefers_a_massed_body_in_range() {
    let mut r = rig();
    let gun = crowd(&mut r.w, r.gun, 0, (2000, 4000), 1, (1, 40), 0);
    // Four men a long way apart, a packed crowd, and a packed crowd out of reach.
    let thin = crowd(&mut r.w, r.mus, 1, (3500, 3200), 4, (4, 300), 128);
    let packed = crowd(&mut r.w, r.mus, 1, (3500, 4200), 24, (6, 20), 128);
    let far = crowd(&mut r.w, r.mus, 1, (9000, 4000), 40, (8, 20), 128);
    let foe = army(1, 128, vec![body(Duty::Line, &thin), body(Duty::Line, &packed), body(Duty::Line, &far)]);
    let mut cm = Commander::new(&r.w, [army(0, 0, vec![body(Duty::Guns, &gun)]), foe], [Control::Ai; 2]);
    let cmds = cm.orders(&r.w);
    let target = aimed(&cmds, gun[0]).expect("an attack");
    assert!(packed.contains(&target), "target {target:?}");
    assert!(matches!(cmds.last(), Some(Command::StandGround { on: true, .. })), "stand ground follows");
    let (g, t) = (r.w.units[gun[0].0 as usize].pos.pixels(), r.w.units[target.0 as usize].pos.pixels());
    assert!((300..=6000 - 64).contains(&dist(g, t)), "in range");
}

#[test]
fn gun_skips_a_target_behind_friends() {
    let mut r = rig();
    let gun = crowd(&mut r.w, r.gun, 0, (2000, 4000), 1, (1, 40), 0);
    // The better target stands straight behind a line of the gun's own men; the lesser one off to the side.
    let friends = crowd(&mut r.w, r.mus, 0, (2600, 3960), 8, (1, 12), 0);
    let behind = crowd(&mut r.w, r.mus, 1, (4000, 3980), 30, (6, 20), 128);
    let aside = crowd(&mut r.w, r.mus, 1, (4000, 2800), 10, (5, 20), 128);
    let me = army(0, 0, vec![body(Duty::Guns, &gun), body(Duty::Line, &friends)]);
    let foe = army(1, 128, vec![body(Duty::Line, &behind), body(Duty::Line, &aside)]);
    let mut cm = Commander::new(&r.w, [me, foe], [Control::Ai; 2]);
    let target = aimed(&cm.orders(&r.w), gun[0]).expect("an attack");
    assert!(aside.contains(&target), "the clear body, not {target:?}");
}

#[test]
fn gun_turns_to_closing_cavalry() {
    let mut r = rig();
    let gun = crowd(&mut r.w, r.gun, 0, (2000, 4000), 1, (1, 40), 0);
    let foot = crowd(&mut r.w, r.mus, 1, (4200, 3300), 30, (6, 20), 128);
    let riders = crowd(&mut r.w, r.horse, 1, (4400, 4700), 24, (6, 20), 128);
    let foe = army(1, 128, vec![body(Duty::Line, &foot), body(Duty::Cavalry, &riders)]);
    let mut cm = Commander::new(&r.w, [army(0, 0, vec![body(Duty::Guns, &gun)]), foe], [Control::Ai; 2]);
    let first = aimed(&cm.orders(&r.w), gun[0]).expect("an attack");
    assert!(foot.contains(&first), "the larger crowd first: {first:?}");
    // The horse gallop 600 px nearer before the next decision (the world is not stepped).
    for &id in &riders {
        r.w.units[id.0 as usize].pos.x -= 600 << 4;
    }
    r.w.tick.0 = PERIOD;
    let second = aimed(&cm.orders(&r.w), gun[0]).expect("an attack");
    assert!(riders.contains(&second), "closing cavalry now: {second:?}");
}

#[test]
fn player_side_only_commands_guns() {
    let mut r = rig();
    let guns = crowd(&mut r.w, r.gun, 0, (2000, 4000), 2, (2, 60), 0);
    let line = crowd(&mut r.w, r.mus, 0, (1500, 3800), 12, (6, 20), 0);
    let foe = crowd(&mut r.w, r.mus, 1, (3400, 3900), 12, (6, 20), 128);
    let armies = [
        army(0, 0, vec![body(Duty::Guns, &guns), body(Duty::Line, &line)]),
        army(1, 128, vec![body(Duty::Line, &foe)]),
    ];
    let mut cm = Commander::new(&r.w, armies, [Control::Player, Control::Ai]);
    let all = run(&mut r.w, &mut cm, 200);
    assert!(!all.is_empty());
    for c in &all {
        match c {
            Command::Attack { player: 0, units, .. } | Command::StandGround { player: 0, units, .. } => {
                assert!(units.iter().all(|u| guns.contains(u)), "{c:?}");
            }
            other => panic!("{other:?}"),
        }
    }
    // The same side under the Ai gives the same orders to its guns, and none to the line yet.
    cm.set_control([Control::Ai, Control::Ai]);
    assert!(
        run(&mut r.w, &mut cm, 100).iter().all(|c| !matches!(c, Command::Move { .. } | Command::AttackMove { .. }))
    );
}

#[test]
fn a_gun_ordered_by_the_player_is_left_alone() {
    let mut r = rig();
    let guns = crowd(&mut r.w, r.gun, 0, (2000, 4000), 2, (2, 200), 0);
    let foe = crowd(&mut r.w, r.mus, 1, (3400, 3900), 30, (6, 20), 128);
    let armies = [army(0, 0, vec![body(Duty::Guns, &guns)]), army(1, 128, vec![body(Duty::Line, &foe)])];
    let mut cm = Commander::new(&r.w, armies, [Control::Player, Control::Ai]);
    let first = run(&mut r.w, &mut cm, 1);
    assert!(aimed(&first, guns[0]).is_some() && aimed(&first, guns[1]).is_some(), "both fire at first");
    // The player sends the first gun to a new place.
    r.w.step(&[Command::Move { player: 0, units: vec![guns[0]], x: 2000, y: 3500 }]);
    let later = run(&mut r.w, &mut cm, 300);
    assert!(
        later.iter().all(|c| match c {
            Command::Attack { units, .. } | Command::StandGround { units, .. } => !units.contains(&guns[0]),
            _ => true,
        }),
        "the player's gun got an order"
    );
    assert!(r.w.units[guns[0].0 as usize].pos.pixels().1 < 3700, "it went where it was sent");
    assert!(!r.w.units[guns[1].0 as usize].pos.pixels().1.is_negative());
}

#[test]
fn a_formation_is_in_order_until_it_scatters() {
    let mut r = rig();
    let shape = r.w.formations.shape_index("LINE15").unwrap();
    let officer = r.w.spawn(r.off, 0, Pos::from_pixels(2000, 2000), 0);
    let soldiers = crowd(&mut r.w, r.mus, 0, (2000, 2000), 15, (15, 0), 0);
    let ids: Vec<usize> = soldiers.iter().map(|id| id.0 as usize).collect();
    create_placed(&mut r.w, (officer.0 as usize, None, None), &ids, shape, (2000, 2000), 0);
    let units: Vec<EntityId> = std::iter::once(officer).chain(soldiers.iter().copied()).collect();
    let a = army(0, 0, vec![body(Duty::Line, &units)]);
    let p = &picture::of(&r.w, &a)[0];
    assert!(p.in_order && p.brigade.is_some() && p.alive == 16 && p.start == 16);
    assert!(!p.melee && !p.cavalry && p.reach == 850, "musketeers: reach {}", p.reach);
    for &s in soldiers.iter().take(8) {
        r.w.units[s.0 as usize].pos.x += 400 << 4;
    }
    assert!(!picture::of(&r.w, &a)[0].in_order, "half the soldiers far from their slots");
}

/// The centroid of the units of `ids`, pixels.
fn centroid(w: &World, ids: &[EntityId]) -> (i32, i32) {
    let n = ids.len() as i32;
    let (x, y) = ids.iter().fold((0, 0), |a, id| {
        let p = w.units[id.0 as usize].pos.pixels();
        (a.0 + p.0, a.1 + p.1)
    });
    (x / n, y / n)
}

fn moves(cmds: &[Command]) -> usize {
    cmds.iter().filter(|c| matches!(c, Command::MoveFacing { .. })).count()
}

/// Two lines of 12 musketeers at x = 1600, one behind the other, facing a crowd at x = 2900.
fn two_lines() -> (Rig, [Vec<EntityId>; 2], Vec<EntityId>) {
    let mut r = rig();
    let a = formed(r.mus, &mut r, 0, (1600, 3900), 0, "LINE12");
    let b = formed(r.mus, &mut r, 0, (1600, 4300), 0, "LINE12");
    let foe = crowd(&mut r.w, r.mus, 1, (2900, 4000), 12, (6, 20), 128);
    (r, [a, b], foe)
}

fn two_line_armies(lines: &[Vec<EntityId>; 2], foe: &[EntityId], style: Style) -> [Army; 2] {
    let mut me = army(0, 0, vec![body(Duty::Line, &lines[0]), body(Duty::Line, &lines[1])]);
    me.style = style;
    [me, army(1, 128, vec![body(Duty::Line, foe)])]
}

#[test]
fn the_line_waits_for_a_body_out_of_order() {
    let (mut r, lines, foe) = two_lines();
    let mut cm = Commander::new(&r.w, two_line_armies(&lines, &foe, Style::Linear), [Control::Ai, Control::Player]);
    // Both bodies in order: the first bound goes to both.
    let first = run(&mut r.w, &mut cm, 1);
    assert_eq!(moves(&first), 2, "{first:?}");
    // Hold the second body scattered (a formation walks its soldiers back by itself): nobody
    // moves while it is out of order...
    let (mut r, lines, foe) = two_lines();
    let mut cm = Commander::new(&r.w, two_line_armies(&lines, &foe, Style::Linear), [Control::Ai, Control::Player]);
    let scattered = |r: &mut Rig, cm: &mut Commander, ticks: usize| {
        let mut all = Vec::new();
        for _ in 0..ticks {
            for &s in lines[1].iter().skip(1).take(8) {
                r.w.units[s.0 as usize].pos.x = 2600 << 4;
            }
            let cmds = cm.orders(&r.w);
            r.w.step(&cmds);
            all.extend(cmds);
        }
        all
    };
    let early = scattered(&mut r, &mut cm, 200);
    assert_eq!(moves(&early), 0, "{early:?}");
    // ...until the patience is out; then the line goes on without the straggler.
    let late = scattered(&mut r, &mut cm, 300);
    assert!(moves(&late) > 0);
}

/// The first tick at which `pred` holds of the orders given, running up to `ticks`.
fn first_order(r: &mut Rig, cm: &mut Commander, ticks: u64, pred: impl Fn(&Command) -> bool) -> Option<u64> {
    for _ in 0..ticks {
        let cmds = cm.orders(&r.w);
        let hit = cmds.iter().any(&pred);
        r.w.step(&cmds);
        if hit {
            return Some(r.w.tick.0 - 1);
        }
    }
    None
}

#[test]
fn the_line_halts_at_nine_tenths_of_musket_range() {
    let mut r = rig();
    let line = formed(r.mus, &mut r, 0, (1500, 4000), 0, "LINE12");
    let foe = crowd(&mut r.w, r.mus, 1, (3000, 4000), 12, (6, 20), 128);
    let armies = [army(0, 0, vec![body(Duty::Line, &line)]), army(1, 128, vec![body(Duty::Line, &foe)])];
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let at = first_order(&mut r, &mut cm, 3000, |c| matches!(c, Command::StandGround { on: true, .. }));
    assert!(at.is_some(), "the line never stood ground");
    assert_eq!(cm.report(0).halts, 1);
    let d = dist(centroid(&r.w, &line), centroid(&r.w, &foe));
    assert!((680..=808).contains(&d), "halted {d} px from the foe (reach 850)");
    r.w.step(&[]);
    assert!(line.iter().skip(1).all(|id| r.w.units[id.0 as usize].stand_ground));
    // It stays there.
    run(&mut r.w, &mut cm, 100);
    assert!((dist(centroid(&r.w, &line), centroid(&r.w, &foe)) - d).abs() <= 40);
}

#[test]
fn columns_charge_after_a_short_fire() {
    let charge_at = |style| {
        let mut r = rig();
        let line = formed(r.mus, &mut r, 0, (1900, 4000), 0, "LINE12");
        let foe = crowd(&mut r.w, r.mus, 1, (3000, 4000), 12, (6, 20), 128);
        let mut me = army(0, 0, vec![body(Duty::Line, &line)]);
        me.style = style;
        let mut cm =
            Commander::new(&r.w, [me, army(1, 128, vec![body(Duty::Line, &foe)])], [Control::Ai, Control::Player]);
        let at = first_order(&mut r, &mut cm, 4000, |c| matches!(c, Command::AttackMove { .. }));
        assert_eq!(cm.report(0).charges, u32::from(at.is_some()));
        at
    };
    let (column, linear) = (charge_at(Style::Column), charge_at(Style::Linear));
    let (column, linear) = (column.expect("a column charges"), linear.expect("a line charges"));
    assert!(column + 100 <= linear, "column {column}, line {linear}");
}

#[test]
fn pikes_stay_behind_until_the_charge() {
    let mut r = rig();
    let line = formed(r.mus, &mut r, 0, (1900, 4000), 0, "LINE12");
    let pikes = formed(r.pike, &mut r, 0, (1500, 4000), 0, "LINE12");
    let foe = crowd(&mut r.w, r.mus, 1, (3000, 4000), 12, (6, 20), 128);
    let me = army(0, 0, vec![body(Duty::Line, &line), body(Duty::Melee, &pikes)]);
    let mut cm = Commander::new(&r.w, [me, army(1, 128, vec![body(Duty::Line, &foe)])], [Control::Ai, Control::Player]);
    let mut charged = None;
    for _ in 0..4000 {
        let cmds = cm.orders(&r.w);
        if charged.is_none()
            && cmds.iter().any(|c| matches!(c, Command::AttackMove { units, .. } if units.contains(&pikes[1])))
        {
            charged = Some(r.w.tick.0);
            assert!(cmds.iter().any(|c| matches!(c, Command::AttackMove { units, .. } if units.contains(&line[1]))));
        }
        r.w.step(&cmds);
        if charged.is_none() && r.w.tick.0.is_multiple_of(10) {
            let gap = centroid(&r.w, &line).0 - centroid(&r.w, &pikes).0;
            assert!(gap >= 60, "tick {}: the pikes are {gap} px behind the line", r.w.tick.0);
        }
    }
    assert!(charged.is_some(), "the pikes charged with the line");
    assert!(centroid(&r.w, &pikes).0 > 1500, "and they moved up behind it");
}

/// A line of 12 musketeers at x = 1800 and 12 riders 300 px in front of it.
fn line_and_riders(r: &mut Rig) -> (Vec<EntityId>, Vec<EntityId>, [Army; 2]) {
    let line = formed(r.mus, r, 0, (1800, 4000), 0, "LINE12");
    let riders = crowd(&mut r.w, r.horse, 1, (2050, 3950), 12, (6, 20), 128);
    let armies = [army(0, 0, vec![body(Duty::Line, &line)]), army(1, 128, vec![body(Duty::Cavalry, &riders)])];
    (line, riders, armies)
}

fn shape_of(w: &World, unit: EntityId) -> String {
    let b = w.units[unit.0 as usize].brigade.unwrap();
    w.formations.shapes[w.brigades[usize::from(b)].shape].name.clone()
}

#[test]
fn a_line_forms_karree_when_cavalry_closes() {
    let mut r = rig();
    let (line, _riders, armies) = line_and_riders(&mut r);
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let (mut reformed, mut stood) = (None, None);
    for _ in 0..400 {
        let cmds = cm.orders(&r.w);
        let reform = cmds.iter().any(|c| matches!(c, Command::Reform { group: 1, .. }));
        let stand = cmds.iter().any(|c| matches!(c, Command::StandGround { on: true, .. }));
        assert!(!(reform && stand), "stand ground in the batch of the reform: {cmds:?}");
        if reform {
            reformed.get_or_insert(r.w.tick.0);
        }
        if stand && stood.is_none() {
            stood = Some(r.w.tick.0);
            assert!(shape_of(&r.w, line[1]).starts_with("#KARE"), "the square stands before it stands ground");
            assert!(picture::of(&r.w, &cm.armies[0])[0].in_order);
        }
        r.w.step(&cmds);
    }
    let (reformed, stood) = (reformed.expect("a reform to KARE"), stood.expect("stand ground"));
    assert!(reformed < stood, "reform {reformed}, stand ground {stood}");
    assert_eq!(cm.report(0).karrees, 1);
    assert!(line.iter().skip(1).all(|id| r.w.units[id.0 as usize].stand_ground));
}

#[test]
fn and_returns_to_line_when_it_is_gone() {
    let mut r = rig();
    let (line, riders, armies) = line_and_riders(&mut r);
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    run(&mut r.w, &mut cm, 200);
    assert!(shape_of(&r.w, line[1]).starts_with("#KARE"));
    // The riders leave: three calm decisions later the square is a line again.
    for &id in &riders {
        r.w.units[id.0 as usize].pos.x += 3000 << 4;
    }
    let later = run(&mut r.w, &mut cm, 40);
    assert!(!later.iter().any(|c| matches!(c, Command::Reform { .. })), "not yet: {later:?}");
    let later = run(&mut r.w, &mut cm, 100);
    assert_eq!(later.iter().filter(|c| matches!(c, Command::Reform { group: 0, .. })).count(), 1, "{later:?}");
    assert_eq!(shape_of(&r.w, line[1]), "#LINE12");
    assert_eq!(cm.report(0).karrees, 1);
}

#[test]
fn no_karree_without_the_button_faces_instead() {
    let mut r = rig_with(GROUPS_NO_KARE);
    let (line, _riders, armies) = line_and_riders(&mut r);
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let all = run(&mut r.w, &mut cm, 100);
    assert!(!all.iter().any(|c| matches!(c, Command::Reform { .. })), "{all:?}");
    assert!(all.iter().any(|c| matches!(c, Command::StandGround { on: true, .. })));
    // The riders stand east of the line, which already faces east: the face order points east too.
    assert!(all.iter().any(|c| matches!(c, Command::Face { officer, .. } if *officer == line[0])), "{all:?}");
    assert_eq!(cm.report(0).karrees, 0);
    assert_eq!(shape_of(&r.w, line[1]), "#LINE12");
}

/// A foe line of 12 musketeers (6 wide, 100 px apart) with its corner at `at`, facing the west.
fn foe_line(r: &mut Rig, at: (i32, i32)) -> Vec<EntityId> {
    crowd(&mut r.w, r.mus, 1, at, 12, (6, 100), 128)
}

fn moves_to(cmds: &[Command], units: &[EntityId]) -> Vec<(i32, i32)> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::MoveFacing { units: u, x, y, .. } if u[0] == units[0] => Some((*x, *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_flank_route_stays_outside_the_enemy_box() {
    let mut r = rig();
    let foe = foe_line(&mut r, (3000, 6900));
    let (north, south) = (
        crowd(&mut r.w, r.horse, 0, (1000, 6400), 8, (4, 20), 0),
        crowd(&mut r.w, r.horse, 0, (1000, 7600), 8, (4, 20), 0),
    );
    let mut me = army(0, 0, vec![body(Duty::Cavalry, &north), body(Duty::Cavalry, &south)]);
    me.origin = (1000, 7000);
    let armies = [me, army(1, 128, vec![body(Duty::Line, &foe)])];
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let all = run(&mut r.w, &mut cm, 900);
    // The box: x 3000..3500, y 6900..7000.
    let n = moves_to(&all, &north);
    assert_eq!(n[0].1.signum(), 1);
    assert!(n[0].1 < 6900 - 300 && n[0].0 > 2500 && n[0].0 < 2800, "north WP1 {:?}", n[0]);
    assert!(n[0].1 > 6300, "{:?}", n[0]);
    let s = moves_to(&all, &south);
    assert!(s[0].1 > 7000 + 300 && s[0].0 > 2500 && s[0].0 < 2800, "south WP1 {:?}", s[0]);
    assert_eq!(cm.report(0).charges, 0, "the wings are still riding round");
}

/// A wing at the top edge of the map, a line of the foe and a lone gun, none of them guarded.
fn edge_scene(r: &mut Rig, kind: TypeId) -> (Vec<EntityId>, Vec<EntityId>, [Army; 2]) {
    let wing = formed(kind, r, 0, (1200, 350), 0, "LINE12");
    let line = foe_line(r, (3000, 420));
    let guns = vec![r.w.spawn(r.gun, 1, Pos::from_pixels(3600, 440), 128)];
    let mut me = army(0, 0, vec![body(Duty::Cavalry, &wing)]);
    me.origin = (1000, 500);
    let armies = [me, army(1, 128, vec![body(Duty::Line, &line), body(Duty::Guns, &guns)])];
    (wing, guns, armies)
}

#[test]
fn the_flank_route_is_blocked_at_the_map_edge() {
    let mut r = rig();
    let (wing, guns, armies) = {
        let k = r.horse;
        edge_scene(&mut r, k)
    };
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let all = run(&mut r.w, &mut cm, 900);
    // No ride round: the wing goes straight for the lone guns, not the line.
    assert!(moves_to(&all, &wing).is_empty(), "{:?}", moves_to(&all, &wing));
    let (x, y) = all
        .iter()
        .find_map(|c| match c {
            Command::AttackMove { x, y, .. } => Some((*x, *y)),
            _ => None,
        })
        .expect("a charge");
    let gun = r.w.units[guns[0].0 as usize].pos.pixels();
    assert!(dist((x, y), gun) < 60, "aimed at {:?}, guns at {gun:?}", (x, y));
    assert_eq!(cm.report(0).charges, 1);
    // Horses have no armed-attack switch.
    assert!(!all.iter().any(|c| matches!(c, Command::MeleeOnly { .. })));
}

fn view_of<'a>(armies: &'a [Army; 2], mine: &'a [picture::Picture], theirs: &'a [picture::Picture]) -> super::View<'a> {
    super::View { tick: 0, me: &armies[0], foe: &armies[1], mine, theirs }
}

#[test]
fn the_wing_charges_guns_before_a_karree() {
    let mut r = rig();
    let wing = crowd(&mut r.w, r.horse, 0, (1000, 4000), 12, (6, 20), 0);
    let square = formed(r.mus, &mut r, 1, (3000, 3800), 128, "KARE12");
    let line = foe_line(&mut r, (3000, 4400));
    let guns = vec![r.w.spawn(r.gun, 1, Pos::from_pixels(3200, 4000), 128)];
    let armies = [
        army(0, 0, vec![body(Duty::Cavalry, &wing)]),
        army(1, 128, vec![body(Duty::Line, &square), body(Duty::Line, &line), body(Duty::Guns, &guns)]),
    ];
    let (mine, theirs) = (picture::of(&r.w, &armies[0]), picture::of(&r.w, &armies[1]));
    let v = view_of(&armies, &mine, &theirs);
    // The square stands 200 px from the guns: it protects them, but is never the choice.
    let at = mine[0].centroid;
    let pick = super::cavalry::best(&r.w, &v, at, false, None).unwrap();
    assert_ne!(pick, 0, "a square is charged");
    // Moved away from the square, the guns come first.
    r.w.units[guns[0].0 as usize].pos = Pos::from_pixels(3200, 5200);
    let theirs = picture::of(&r.w, &armies[1]);
    let v = view_of(&armies, &mine, &theirs);
    assert_eq!(super::cavalry::best(&r.w, &v, at, false, None), Some(2));
}

#[test]
fn a_wing_rallies_and_reforms_after_losses() {
    let mut r = rig();
    let (wing, _guns, armies) = {
        let k = r.dragoon;
        edge_scene(&mut r, k)
    };
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let mut all = Vec::new();
    for _ in 0..1000 {
        all.extend(run(&mut r.w, &mut cm, 1));
        if cm.report(0).charges == 1 {
            break;
        }
    }
    assert_eq!(cm.report(0).charges, 1);
    // The dragoons charge with the sabre.
    assert!(all.iter().any(|c| matches!(c, Command::MeleeOnly { on: true, units, .. } if units.contains(&wing[1]))));
    // Eight of twelve fall.
    for id in &wing[1..9] {
        r.w.units[id.0 as usize].alive = false;
    }
    let later = run(&mut r.w, &mut cm, 800);
    assert_eq!(cm.report(0).rallies, 1);
    assert!(later.iter().any(|c| matches!(c, Command::MeleeOnly { on: false, .. })));
    assert!(moves_to(&later, &wing).iter().any(|&p| dist(p, (1200, 350)) < 100), "{:?}", moves_to(&later, &wing));
    assert!(later.iter().any(|c| matches!(c, Command::Reform { group: 0, .. })), "{later:?}");
}

#[test]
fn a_resting_wing_intercepts_cavalry() {
    let mut r = rig();
    let line = formed(r.mus, &mut r, 0, (1800, 4000), 0, "LINE12");
    let wing = crowd(&mut r.w, r.horse, 0, (1600, 4600), 8, (4, 20), 0);
    let riders = crowd(&mut r.w, r.horse, 1, (3000, 4000), 12, (6, 20), 128);
    let armies = [
        army(0, 0, vec![body(Duty::Line, &line), body(Duty::Cavalry, &wing)]),
        army(1, 128, vec![body(Duty::Cavalry, &riders)]),
    ];
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let first = run(&mut r.w, &mut cm, 1);
    assert!(!first.iter().any(|c| matches!(c, Command::AttackMove { units, .. } if units.contains(&wing[0]))));
    for id in &riders {
        r.w.units[id.0 as usize].pos.x -= 600 << 4;
    }
    let mut all = run(&mut r.w, &mut cm, 20);
    all.extend(run(&mut r.w, &mut cm, 1));
    let hit = all.iter().find_map(|c| match c {
        Command::AttackMove { units, x, y, .. } if units.contains(&wing[0]) => Some((*x, *y)),
        _ => None,
    });
    let at = hit.expect("the wing rides at the riders");
    assert!(dist(at, centroid(&r.w, &riders)) < 100, "{at:?}");
    assert_eq!(cm.report(0).charges, 1);
}

#[test]
fn skirmishers_fall_back_behind_the_flank_when_infantry_closes() {
    let mut r = rig();
    let line = formed(r.mus, &mut r, 0, (1800, 4000), 0, "LINE12");
    let screen = crowd(&mut r.w, r.mus, 0, (2400, 3900), 8, (4, 20), 0);
    // Formed enemy infantry 800 px off: the skirmishers fire on.
    let foe = foe_line(&mut r, (3200, 3900));
    let armies = [
        army(0, 0, vec![body(Duty::Line, &line), body(Duty::Screen, &screen)]),
        army(1, 128, vec![body(Duty::Line, &foe)]),
    ];
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    let first = run(&mut r.w, &mut cm, 1);
    assert!(moves_to(&first, &screen).is_empty());
    // It closes to 450 px.
    for id in &foe {
        r.w.units[id.0 as usize].pos.x -= 600 << 4;
    }
    let mut all = run(&mut r.w, &mut cm, 20);
    all.extend(run(&mut r.w, &mut cm, 1));
    let to = moves_to(&all, &screen);
    assert_eq!(to.len(), 1, "{all:?}");
    let ys: Vec<i32> = line.iter().map(|id| r.w.units[id.0 as usize].pos.pixels().1).collect();
    let (lo, hi) = (*ys.iter().min().unwrap(), *ys.iter().max().unwrap());
    let front = line.iter().map(|id| r.w.units[id.0 as usize].pos.pixels().0).max().unwrap();
    // 250 behind the front, 150 outside the nearer (northern) flank.
    assert!((to[0].0 - (front - 250)).abs() < 30, "{to:?} front {front}");
    assert!((to[0].1 - (lo - 150)).abs() < 30 || (to[0].1 - (hi + 150)).abs() < 30, "{to:?} y {lo}..{hi}");
    // They stand ground at the spot.
    let later = run(&mut r.w, &mut cm, 600);
    assert!(later.iter().any(|c| matches!(c, Command::StandGround { units, on: true, .. } if units == &screen)));
}

#[test]
fn the_reserve_takes_a_broken_bodys_place() {
    let mut r = rig();
    let first_line = formed(r.mus, &mut r, 0, (1800, 4000), 0, "LINE12");
    let reserve = formed(r.mus, &mut r, 0, (1300, 4000), 0, "LINE12");
    let foe = foe_line(&mut r, (4000, 3900));
    let armies = [
        army(0, 0, vec![body(Duty::Line, &first_line), body(Duty::Reserve { cavalry: false }, &reserve)]),
        army(1, 128, vec![body(Duty::Line, &foe)]),
    ];
    let mut cm = Commander::new(&r.w, armies, [Control::Ai, Control::Player]);
    // The reserve closes up to 350 px behind the line's front.
    let first = run(&mut r.w, &mut cm, 1);
    let to = moves_to(&first, &reserve);
    assert_eq!(to.len(), 1, "{first:?}");
    let front = first_line.iter().map(|id| r.w.units[id.0 as usize].pos.pixels().0).max().unwrap();
    let here = r.w.units[reserve[1].0 as usize].pos.pixels().0;
    assert!(to[0].0 > here && to[0].0 <= front - 350 + 30, "{to:?} front {front}");
    assert_eq!(cm.armies[0].bodies[1].duty, Duty::Reserve { cavalry: false });
    // The first line is shot to pieces: nine of thirteen fall.
    for id in &first_line[1..10] {
        r.w.units[id.0 as usize].alive = false;
    }
    let later = run(&mut r.w, &mut cm, 40);
    assert_eq!(cm.report(0).reserves, 1);
    assert_eq!(cm.armies[0].bodies[1].duty, Duty::Line);
    assert_eq!(cm.armies[0].bodies[0].duty, Duty::Reserve { cavalry: false });
    assert!(moves_to(&later, &first_line).len() == 1, "the broken body falls back: {later:?}");
    assert!(later.iter().any(|c| matches!(c, Command::Reform { .. })));
}
