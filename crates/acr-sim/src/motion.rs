//! Unit motion as the engine does it for land units (`MOTIONSTYLE` default,
//! `FASTROTATE&MOVE`, `MotionHandler1` at 0x50e823), with the order links that steer it.
//!
//! A unit only decides at the end of a walking cycle: it aims at its steering point, turns
//! in place by `ROTATE` (16) per tick if it is more than that off, otherwise looks one
//! whole cycle ahead (`#MOTION_L` frames × step), reserves the 16 px cell it will reach in
//! the occupancy grid and walks there for the cycle. Blocked units stand, try detours of
//! ±16/±32/±48, then re-plan (map obstacle) and push or swap with idle friends.

use crate::catalog::ObjectType;
use crate::fixed::Pos;
use crate::map::{AREA_CELL, AreaGraph, LockMap};
use crate::path::{Mover, create_full_path, create_path, create_pre_path, dist_to, find_best_position};
use crate::rng::SyncRandom;
use crate::trig::{get_dir, norma};
use crate::world::{AnimKind, FRM_DEC, Order, SPEED_SH, Unit, World};

/// Priority of player move orders (`SelSendTo` passes 0x10 to 0x4db0d3).
pub const PLAYER_PRIO: u8 = 0x10;

fn mover(u: &Unit) -> Mover {
    Mover { x: u.cell.0, y: u.cell.1, lx: u.lx }
}

fn layer<'a>(w: &'a crate::map::MapGrid, t: &ObjectType) -> &'a LockMap {
    match t.medium {
        crate::catalog::Medium::Land => &w.land,
        crate::catalog::Medium::Water => &w.water,
    }
}

/// The pathfinding areas of a type's medium (`TOP1` or `TOPW`).
pub(crate) fn areas_of<'a>(map: &'a crate::map::MapGrid, t: &ObjectType) -> Option<&'a AreaGraph> {
    match t.medium {
        crate::catalog::Medium::Land => map.land_areas.as_ref(),
        crate::catalog::Medium::Water => map.water_areas.as_ref(),
    }
}

/// Frames of an animation slot, if the type has it.
pub(crate) fn anim_frames(t: &ObjectType, kind: AnimKind) -> Option<i32> {
    t.anims.get(&kind.name()).map(|a| i32::from(a.frames))
}

/// Switches to an animation slot (1 frame if the type lacks it); the frame is not reset.
pub(crate) fn set_anim(u: &mut Unit, t: &ObjectType, kind: AnimKind) {
    u.anim = kind;
    u.anim_frames = anim_frames(t, kind).unwrap_or(1).max(1);
}

/// The first of `kinds` the type has, or `#STAND`.
fn first_anim(t: &ObjectType, kinds: &[AnimKind]) -> AnimKind {
    kinds.iter().copied().find(|&k| anim_frames(t, k).is_some()).unwrap_or(AnimKind::Stand)
}

/// `NewMonsterSendTo` (0x517470 / 0x517045): sets the steering point at once and gives the
/// unit a `SendTo` order for the lock cell of `pos`. `replace` drops the order stack
/// (player orders); otherwise the order is pushed on top (legs of a routed move).
pub fn send_to(u: &mut Unit, pos: Pos, prio: u8, replace: bool) {
    let cell = Unit::cell_of(pos, u.lx);
    if replace {
        u.orders.clear();
    }
    let prio = prio & 0x7f;
    if prio == 0 {
        u.in_patrol = true;
    }
    u.orders.push(Order::SendTo { cell, prio });
    u.dest = Some(pos);
    u.prio = prio;
    u.path.delay = 0;
    u.stand_time = 0;
}

/// `SmartSendTo` (0x52bc95): a move routed over the pathfinding areas to `(x, y)` plus the
/// unit's group offset `(dx, dy)` (world pixels).
pub fn smart_send_to(u: &mut Unit, areas: Option<&AreaGraph>, x: i32, y: i32, dx: i32, dy: i32, prio: u8) {
    smart_send_to_ex(u, areas, x, y, dx, dy, prio, false, false);
}

/// `SmartSendTo` with the order placement: `front` (order type 1) puts the move in front of
/// the current orders instead of replacing them.
#[allow(clippy::too_many_arguments)]
pub fn smart_send_to_ex(
    u: &mut Unit,
    areas: Option<&AreaGraph>,
    mut x: i32,
    mut y: i32,
    mut dx: i32,
    mut dy: i32,
    prio: u8,
    front: bool,
    stance_walk: bool,
) {
    while norma(dx, dy) > 128 {
        let n = norma(dx, dy);
        let (ndx, ndy) = (dx * 120 / n, dy * 90 / n);
        x += dx - ndx;
        y += dy - ndy;
        (dx, dy) = (ndx, ndy);
    }
    x = x.max(64);
    y = y.max(64);
    if prio < u.prio && !u.orders.is_empty() {
        return;
    }
    let Some(g) = areas.filter(|g| g.count > 0) else {
        // Maps without TOP1 (editor autosaves): walk directly.
        send_to(u, Pos::from_pixels(x + dx, y + dy), prio, !front);
        return;
    };
    if top_zone_of_target(g, &mut x, &mut y).is_none() {
        return;
    }
    if !front {
        u.orders.clear();
    }
    // The order keeps `prio & 0x7f`; priority 0 (attack-move, patrol) marks `InPatrol`
    // (0x52bef4) and walks in stance 1 where the type can (0x52bf0b).
    let prio = prio & 0x7f;
    if prio == 0 {
        u.in_patrol = true;
    }
    u.ground_state = u8::from(prio == 0 && stance_walk);
    if u.target().is_none() {
        u.stance = u.ground_state;
    }
    u.orders.push(Order::SmartSendTo { x, y, dx, dy, prio, area: None });
    u.prio = prio;
}

/// Spiral search over 64 px cells for the nearest cell with an area (`TopZoneOfTarget`,
/// 0x52b69c; the engine uses 10 precomputed rings, approximated by square rings).
fn nearest_area(g: &AreaGraph, cx: i32, cy: i32) -> Option<(u16, i32, i32)> {
    for r in 0..10i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let a = g.area_at(cx + dx, cy + dy);
                if a < 0xFFFE {
                    return Some((a, cx + dx, cy + dy));
                }
            }
        }
    }
    None
}

fn top_zone_of_target(g: &AreaGraph, x: &mut i32, y: &mut i32) -> Option<u16> {
    let (cx, cy) = (*x >> 6, *y >> 6);
    let a = g.area_at(cx, cy);
    if a < 0xFFFE {
        return Some(a);
    }
    let (a, ax, ay) = nearest_area(g, cx, cy)?;
    *x = AREA_CELL * ax + 32;
    *y = AREA_CELL * ay + 32;
    Some(a)
}

/// `TopLineClear` (0x52c3ff): whether the straight line stays inside areas (64 px cells).
fn top_line_clear(g: &AreaGraph, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let (ddx, ddy) = ((x1 - x0) << 8, (y1 - y0) << 8);
    let n = (norma(ddx, ddy) >> 14) + 1;
    let (sx, sy) = (ddx / n, ddy / n);
    let (mut px, mut py) = (x0 << 8, y0 << 8);
    for _ in 0..n {
        px += sx;
        py += sy;
        let (cx, cy) = (px >> 14, py >> 14);
        if cx <= 0 || cy <= 0 || g.area_at(cx, cy) >= 0xFFFE {
            return false;
        }
    }
    true
}

/// Applies the group offset to a routing point (`ApplyOffset`, 0x52b0cd, simplified): the
/// offset point if its 4 × 4 cell box is free and inside an area, else points closer to
/// the unshifted one.
fn apply_offset(g: &AreaGraph, m: &LockMap, x: i32, y: i32, dx: i32, dy: i32, water: bool) -> (i32, i32) {
    // Land units test a 4 × 4 box, ships a 14 × 14 one (0x52b0cd).
    let (h, s) = if water { (7, 14) } else { (2, 4) };
    let n = norma(dx, dy) / 32 + 1;
    for k in (0..=n).rev() {
        let (px, py) = (x + dx * k / n, y + dy * k / n);
        if g.area_at(px >> 6, py >> 6) < 0xFFFE && !m.check_bar((px >> 4) - h, (py >> 4) - h, s, s) {
            return (px, py);
        }
    }
    (x, y)
}

/// What the motion handler asks the caller to do with access to other units.
enum Signal {
    /// Clear the way into this cell (`RemoveFools`).
    Push((i32, i32)),
    /// An enemy stands in the way: attack it (`CheckMotionThroughEnemyAbility`).
    Engage(usize),
}

/// Runs one unit for one tick: its order link, then the motion handler.
pub fn process_unit(world: &mut World, i: usize) {
    crate::ship::check_park(world, i);
    match world.units[i].orders.last().cloned() {
        Some(Order::Attack { target, prio }) => {
            if world.catalog.get(world.units[i].kind).guns.is_some() {
                crate::ship::water_attack_link(world, i, target, prio);
            } else {
                crate::combat::attack_link(world, i, target, prio);
            }
        }
        Some(Order::AttackPoint { x, y, prio, times }) => {
            crate::combat::attack_link_aim(world, i, crate::combat::Aim::Point(Pos::from_pixels(x, y), times), prio)
        }
        Some(Order::Build { target, point, prio }) => crate::construction::build_link(world, i, target, point, prio),
        Some(Order::Gather { kind, source, at, prio }) => crate::gather::gather_link(world, i, kind, source, at, prio),
        Some(Order::Panic { .. }) => crate::morale::panic_link(world, i),
        Some(Order::Patrol { .. }) => patrol_link(world, i),
        Some(Order::WaterSendTo { cell, prio, turning }) => {
            crate::ship::water_send_to_link(world, i, cell, prio, turning)
        }
        Some(Order::GoToTransport { transport, land, prio }) => {
            crate::ship::go_to_transport_link(world, i, transport, land, prio);
            if world.units[i].inside_of.is_some() {
                return;
            }
        }
        Some(Order::LeaveTransport { kind, delay }) => crate::ship::leave_transport_link(world, i, kind, delay),
        Some(Order::Fishing) => crate::ship::fishing_link(world, i),
        Some(Order::Turn { dir }) => {
            let step = catalog_rotate(world, i);
            turn_link(&mut world.units[i], step, dir);
        }
        Some(Order::Enter { building, prio }) => {
            crate::economy::enter_link(world, i, building, prio);
            if world.units[i].inside_of.is_some() {
                return;
            }
        }
        _ => {}
    }
    let catalog = world.catalog.clone();
    if catalog.get(world.units[i].kind).motion_style == crate::catalog::MotionStyle::ComplexRotate {
        // Cannons: their own motion handler (0x50f7cb); the order links set the target.
        if let Some(Order::SendTo { cell, prio }) = world.units[i].orders.last().cloned() {
            let map = world.map.clone();
            let World { units, occupancy, random, .. } = &mut *world;
            let u = &mut units[i];
            let t = catalog.get(u.kind);
            send_to_link(u, t, layer(&map, t), random, occupancy, cell, prio);
        } else if let Some(Order::SmartSendTo { x, y, dx, dy, prio, area }) = world.units[i].orders.last().cloned() {
            let map = world.map.clone();
            let u = &mut world.units[i];
            let t = catalog.get(u.kind);
            smart_send_to_link(u, map.land_areas.as_ref(), layer(&map, t), x, y, dx, dy, prio, area);
        } else if let Some(Order::ClearUnlimited) = world.units[i].orders.last() {
            // The order links run before every motion style's handler.
            let map = world.map.clone();
            let World { units, occupancy, .. } = &mut *world;
            clear_unlimited_link(&mut units[i], &map.land, occupancy);
        } else if world.units[i].orders.is_empty() {
            world.units[i].prio = 0;
        }
        crate::artillery::complex_rotate(world, i);
        return;
    }
    let map = world.map.clone();
    let world_adv = world.adv(world.units[i].owner, world.units[i].kind);
    let ship = crate::ship::is_ship(catalog.get(world.units[i].kind));
    let World { units, occupancy, random, buckets, .. } = world;
    let u = &mut units[i];
    let t = catalog.get(u.kind);
    let lay = layer(&map, t);
    let areas =
        if t.medium == crate::catalog::Medium::Water { map.water_areas.as_ref() } else { map.land_areas.as_ref() };
    match u.orders.last().cloned() {
        Some(Order::SendTo { cell, prio }) => send_to_link(u, t, lay, random, occupancy, cell, prio),
        Some(Order::SmartSendTo { x, y, dx, dy, prio, area }) => {
            smart_send_to_link(u, areas, lay, x, y, dx, dy, prio, area);
        }
        Some(Order::Attack { .. })
        | Some(Order::WaterSendTo { .. })
        | Some(Order::GoToTransport { .. })
        | Some(Order::LeaveTransport { .. })
        | Some(Order::Fishing)
        | Some(Order::Turn { .. })
        | Some(Order::AttackPoint { .. })
        | Some(Order::Panic { .. })
        | Some(Order::Patrol { .. })
        | Some(Order::Enter { .. })
        | Some(Order::Build { .. })
        | Some(Order::Gather { .. }) => {}
        Some(Order::ClearUnlimited) => clear_unlimited_link(u, &map.land, occupancy),
        None => u.prio = 0,
    }
    if ship {
        crate::ship::motion(world, i);
        return;
    }
    let rate = if u.stance == 0 { 16 } else { adv_rate(&catalog, u, &world_adv) };
    // Leg wounds (#225, optional rule) slow the walk.
    let rate = u.wounds.walk_rate(rate);
    match motion_handler(u, t, lay, occupancy, random, buckets, rate) {
        Some(Signal::Push(cell)) => remove_fools(world, i, cell),
        Some(Signal::Engage(j)) => {
            crate::combat::attack_obj(world, i, j, 0x8F);
        }
        None => {}
    }
}

/// `ClearUnlimitedLink` (0x527dec): waits a moment if nothing follows; a unit on a locked
/// cell (off a ship) first walks to the nearest free spot on land.
fn clear_unlimited_link(u: &mut Unit, land: &LockMap, occupancy: &mut LockMap) {
    if u.stand_time < 5 && u.orders.len() == 1 {
    } else if land.check_bar(u.cell.0 - 1, u.cell.1 - 1, 3, 3)
        && let Some((x, y)) = crate::ship::find_near_empty(land, u.cell.0, u.cell.1)
    {
        u.unlimited = false;
        send_to(u, Pos::new(x << 8, y << 8), 0x90, false);
        u.unlimited = true;
    } else {
        u.pop_order();
        crate::economy::settle(u, occupancy);
        u.prio = 0;
    }
}

/// The walking rate of the unit's stance (`Rate[NewState − 1]`, 16 = normal).
fn adv_rate(_c: &crate::catalog::Catalog, u: &Unit, adv: &crate::catalog::Adv) -> i32 {
    adv.rate.get(usize::from(u.stance) - 1).copied().unwrap_or(16)
}

/// `NewMonsterSendToLink` (0x5172c6).
fn send_to_link(
    u: &mut Unit,
    t: &ObjectType,
    m: &LockMap,
    rnd: &mut SyncRandom,
    occupancy: &mut LockMap,
    cell: (i32, i32),
    prio: u8,
) {
    if u.path.delay != 0 && u.stand_time > 64 {
        u.pop_order();
        crate::economy::settle(u, occupancy);
        return;
    }
    u.prio = prio;
    let mut c = cell;
    if u.unlimited {
        // CreatePath with UnlimitedMotion: straight to the cell centre. The walk through
        // locks ends with the `ClearUnlimited` order queued after the last such leg.
        if dist_to(mover(u), c.0, c.1) <= t.stop_distance >> 8 {
            if u.frame >= u.anim_frames - FRM_DEC {
                try_to_stand(u, t, rnd, false);
                u.pop_order();
            }
        } else {
            u.dest = Some(Pos::new(crate::path::cell_center(c.0, u.lx), crate::path::cell_center(c.1, u.lx)));
        }
        return;
    }
    if m.check_bar(c.0 - 1, c.1 - 1, u.lx + 2, u.lx + 2)
        && find_best_position(m, mover(u), &mut c, 80)
        && let Some(Order::SendTo { cell, .. }) = u.orders.last_mut()
    {
        *cell = c;
    }
    if dist_to(mover(u), c.0, c.1) <= t.stop_distance >> 8 {
        if u.frame >= u.anim_frames - FRM_DEC {
            try_to_stand(u, t, rnd, false);
            u.pop_order();
        }
    } else if let Some((x, y)) = create_path(m, mover(u), &mut u.path, rnd, c.0, c.1, t.stop_distance) {
        u.dest = Some(Pos::new(x, y));
    }
}

/// `SmartSendToLink` (0x52b84a): routes over the areas' next-hop table, pushing `SendTo`
/// legs to area centres (skipping up to three areas in straight sight).
#[allow(clippy::too_many_arguments)]
fn smart_send_to_link(
    u: &mut Unit,
    areas: Option<&AreaGraph>,
    m: &LockMap,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    prio: u8,
    area: Option<u16>,
) {
    let (ux, uy) = u.pos.pixels();
    let Some(g) = areas else {
        u.pop_order();
        send_to(u, Pos::from_pixels(x + dx, y + dy), prio, false);
        return;
    };
    if top_line_clear(g, ux, uy, x + dx, y + dy) {
        u.pop_order();
        send_to(u, Pos::from_pixels(x + dx, y + dy), prio, false);
        return;
    }
    let cur = match area.or_else(|| nearest_area(g, ux >> 6, uy >> 6).map(|a| a.0)) {
        Some(a) => a,
        None => {
            u.pop_order();
            return;
        }
    };
    let (mut tx, mut ty) = (x, y);
    let Some(dst) = top_zone_of_target(g, &mut tx, &mut ty) else {
        u.pop_order();
        return;
    };
    let mut nxt = g.next_hop(cur, dst);
    if nxt == dst || dst == cur {
        u.pop_order();
        let (px, py) = apply_offset(g, m, tx, ty, dx, dy, u.lx > 1);
        send_to(u, Pos::from_pixels(px, py), prio, false);
        return;
    }
    if nxt == 0xFFFF {
        u.pop_order();
        return;
    }
    let center = |a: u16| {
        let (cx, cy) = g.centers[usize::from(a)];
        (AREA_CELL * cx + 32, AREA_CELL * cy + 32)
    };
    for _ in 0..3 {
        let n2 = g.next_hop(nxt, dst);
        if n2 == 0xFFFF {
            break;
        }
        let (cx, cy) = center(n2);
        if top_line_clear(g, ux, uy, cx + dx, cy + dy) {
            nxt = n2;
        } else {
            break;
        }
    }
    let (wx, wy) = center(nxt);
    let (wx, wy) = apply_offset(g, m, wx, wy, dx, dy, u.lx > 1);
    if let Some(Order::SmartSendTo { x, y, area, .. }) = u.orders.last_mut() {
        (*x, *y, *area) = (tx, ty, Some(nxt));
    }
    send_to(u, Pos::from_pixels(wx, wy), prio, false);
}

/// `MotionHandler1` (0x50e823).
fn motion_handler(
    u: &mut Unit,
    t: &ObjectType,
    m: &LockMap,
    occupancy: &mut LockMap,
    rnd: &mut SyncRandom,
    buckets: &crate::world::Buckets,
    rate: i32,
) -> Option<Signal> {
    if u.orders.is_empty() {
        u.dest = None;
    }
    u.path.delay = (i32::from(u.path.delay) - FRM_DEC).max(0) as u8;
    if u.frame < u.anim_frames - FRM_DEC {
        // In-cycle step.
        if u.in_motion {
            u.pos.x += u.vel.0 << SPEED_SH;
            u.pos.y += u.vel.1 << SPEED_SH;
        }
        u.frame += FRM_DEC;
        return None;
    }
    let Some(dest) = u.dest else {
        try_to_stand(u, t, rnd, true);
        u.move_stage = 0;
        return None;
    };
    let best = get_dir(dest.x - u.pos.x, dest.y - u.pos.y);
    let ddir = best.wrapping_sub(u.dir) as i8;
    let mrot = t.rotate;
    if i32::from(ddir).abs() > mrot {
        // Stop and turn in place; the animation keeps its last frame.
        let r = mrot as u8;
        u.dir = if ddir > 0 { u.dir.wrapping_add(r) } else { u.dir.wrapping_sub(r) };
        return None;
    }
    let mr = mrot as u8;
    let mut need_push = false;
    let (d, next_stage) = match u.move_stage {
        0 => (best, 1),
        1 => (best.wrapping_add(mr), 2),
        2 => (best.wrapping_sub(mr), 3),
        3 => (best.wrapping_add(2 * mr), 4),
        4 => (best.wrapping_sub(2 * mr), 5),
        5 => (best.wrapping_add(3 * mr), 6),
        6 => (best.wrapping_sub(3 * mr), 255),
        _ => {
            need_push = true;
            (best, 255)
        }
    };
    u.move_stage = next_stage;
    let nst = t.motion_frames() >> SPEED_SH;
    let (dx0, dy0) =
        (nst * (((t.step_dx(d) << SPEED_SH) * rate) >> 4), nst * (((t.step_dy(d) << SPEED_SH) * rate) >> 4));
    let lx = u.lx;
    let end = Pos::new(u.pos.x + dx0, u.pos.y + dy0);
    let (xf, yf) = Unit::cell_of(end, lx);
    if u.unlimited {
        // UnlimitedMotion: no checks, no reservations.
        u.vel = (0, 0);
        try_to_move(u, t, m, rnd, d, rate);
        u.dir = d;
        if u.vel != (0, 0) {
            u.cell = (xf, yf);
        }
        u.move_stage = 0;
        return None;
    }
    occupancy.clr_bar(u.cell.0, u.cell.1, lx);
    let mut on_the_way = false;
    let mut blocked = occupancy.check_bar(xf, yf, lx, lx);
    if !blocked {
        on_the_way = m.check_bar(xf, yf, lx, lx);
        blocked = on_the_way;
    }
    if blocked {
        if on_the_way && need_push {
            if let Some(&(px, py)) = u.path.points.last() {
                let r = create_pre_path(m, mover(u), &mut u.path, rnd, px, py);
                if r.searched {
                    u.path.delay = (32 + (rnd.rando() & 7)) as u8;
                }
            } else {
                let c = Unit::cell_of(dest, lx);
                create_full_path(m, mover(u), &mut u.path, rnd, c.0, c.1);
            }
            u.move_stage = 0;
        }
        try_to_stand(u, t, rnd, false);
        occupancy.set_bar(u.cell.0, u.cell.1, lx);
        return need_push.then_some(Signal::Push((xf, yf)));
    }
    if let Some(j) = buckets.enemy_near(u, end) {
        try_to_stand(u, t, rnd, false);
        occupancy.set_bar(u.cell.0, u.cell.1, lx);
        return Some(Signal::Engage(j));
    }
    u.vel = (0, 0);
    try_to_move(u, t, m, rnd, d, rate);
    u.dir = d;
    if u.vel != (0, 0) {
        u.cell = (xf, yf);
    }
    occupancy.set_bar(u.cell.0, u.cell.1, lx);
    u.move_stage = 0;
    None
}

/// One step towards direction `d` outside the motion handler (0x50e4b2): `back` walks away
/// from `d` while still facing it. A step into a taken or blocked cell stands instead.
pub(crate) fn step(world: &mut World, i: usize, d: u8, back: bool) {
    let catalog = world.catalog.clone();
    let World { units, occupancy, map, random, .. } = world;
    let u = &mut units[i];
    let t = catalog.get(u.kind);
    if u.unlimited {
        return;
    }
    let md = if back { d.wrapping_add(128) } else { d };
    let lx = u.lx;
    let nst = t.motion_frames() >> SPEED_SH;
    let end = Pos::new(u.pos.x + nst * (t.step_dx(md) << SPEED_SH), u.pos.y + nst * (t.step_dy(md) << SPEED_SH));
    let (xf, yf) = Unit::cell_of(end, lx);
    occupancy.clr_bar(u.cell.0, u.cell.1, lx);
    if occupancy.check_bar(xf, yf, lx, lx) || map.land.check_bar(xf, yf, lx, lx) {
        try_to_stand(u, t, random, false);
        occupancy.set_bar(u.cell.0, u.cell.1, lx);
        return;
    }
    u.vel = (0, 0);
    try_to_move(u, t, &map.land, random, md, 16);
    if u.vel != (0, 0) {
        u.cell = (xf, yf);
    }
    u.dir = d;
    occupancy.set_bar(u.cell.0, u.cell.1, lx);
}

/// `TryToMove` (0x50b021), walking forwards. In stance `s ≠ 0` the unit plays
/// `#MOTION_L{s−1}`/`#MOTION_R{s−1}` and its step is scaled by `RATE[s−1]/16` (0x50b13c);
/// the look-ahead still counts the frames of `#MOTION_L`.
fn try_to_move(u: &mut Unit, t: &ObjectType, m: &LockMap, rnd: &mut SyncRandom, d: u8, rate: i32) {
    rnd.rando();
    if u.local_stance != u.stance {
        try_to_stand(u, t, rnd, false);
        return;
    }
    let nf = t.motion_frames() >> SPEED_SH;
    let (odx, ody) = (((t.step_dx(d) << SPEED_SH) * rate) >> 4, ((t.step_dy(d) << SPEED_SH) * rate) >> 4);
    let end = Pos::new(u.pos.x + nf * odx, u.pos.y + nf * ody);
    let (cx, cy) = Unit::cell_of(end, u.lx);
    if !u.unlimited && m.check_bar(cx, cy, u.lx, u.lx) {
        return; // CheckTerra (0x50a60d) failed
    }
    let kind = if u.stance == 0 {
        match (u.left_leg, t.motion_l.is_some(), t.motion_r.is_some()) {
            (true, true, _) | (false, true, false) => AnimKind::MotionL,
            (_, _, true) => AnimKind::MotionR,
            _ => AnimKind::Stand,
        }
    } else {
        let s = u.stance - 1;
        let (l, r) = (anim_frames(t, AnimKind::MotionLs(s)).is_some(), anim_frames(t, AnimKind::MotionRs(s)).is_some());
        match (u.left_leg, l, r) {
            (true, true, _) | (false, true, false) => AnimKind::MotionLs(s),
            (_, _, true) => AnimKind::MotionRs(s),
            // No walk of this stance: the normal walk (at the stance's rate).
            _ => match (u.left_leg, t.motion_l.is_some(), t.motion_r.is_some()) {
                (true, true, _) | (false, true, false) => AnimKind::MotionL,
                (_, _, true) => AnimKind::MotionR,
                _ => AnimKind::Stand,
            },
        }
    };
    u.dir = d;
    set_anim(u, t, kind);
    u.left_leg = !u.left_leg;
    u.vel = (odx >> SPEED_SH, ody >> SPEED_SH);
    u.pos.x += odx;
    u.pos.y += ody;
    u.in_motion = true;
    u.frame = FRM_DEC - 1;
    u.stand_time = 0;
}

/// `TryToStand` (0x50a8c5): stops, changes stance through the transition animations
/// (`#PATTACKi` up, `#UATTACKi` down, `#TRANSX3`/`#TRANS3X` around the reload stance 4),
/// shows `#PSTANDi` in a stance, and may start an idle animation (`rest`) in stance 0.
pub(crate) fn try_to_stand(u: &mut Unit, t: &ObjectType, rnd: &mut SyncRandom, rest: bool) {
    rnd.rando();
    u.stand_time = u.stand_time.wrapping_add(1);
    u.in_motion = false;
    if u.stance != u.local_stance {
        let (ns, ls) = (u.stance, u.local_stance);
        let bit = |s: u8| s > 0 && t.trans_x & (1 << (s - 1)) != 0;
        let (kind, reached) = if ls == 0 {
            (first_anim(t, &[AnimKind::PAttack(ns - 1), AnimKind::PStand(ns - 1)]), ns)
        } else if ns == 4 && bit(ls) && anim_frames(t, AnimKind::TransX3).is_some() {
            (AnimKind::TransX3, 4)
        } else if ls == 4 && ns != 0 && bit(ns) && anim_frames(t, AnimKind::Trans3X).is_some() {
            (AnimKind::Trans3X, ns)
        } else {
            (first_anim(t, &[AnimKind::UAttack(ls - 1)]), 0)
        };
        u.local_stance = reached;
        set_anim(u, t, kind);
        u.frame = 0;
        return;
    }
    if u.stance != 0 {
        let kind = first_anim(t, &[AnimKind::PStand(u.stance - 1)]);
        set_anim(u, t, kind);
        u.frame = 0;
        return;
    }
    if u.frame < u.anim_frames - FRM_DEC {
        return; // at stance 0 only a finished animation is replaced (0x50ae4e)
    }
    let mut kind = AnimKind::Stand;
    if rest && rnd.rando() < 512 {
        kind = if rnd.rando() < 16384 && t.rest.is_some() {
            AnimKind::Rest
        } else if t.rest1.is_some() {
            AnimKind::Rest1
        } else {
            AnimKind::Stand
        };
    }
    set_anim(u, t, kind);
    u.frame = 0;
}

/// `RemoveFools` (0x50e2f3, simplified): an idle friend standing in the wanted cell swaps
/// places with the mover if it has been idle a while, otherwise it is sent to the nearest
/// free cell. Enemies are left alone (combat is not simulated yet).
fn remove_fools(world: &mut World, mover_idx: usize, cell: (i32, i32)) {
    let (owner, lx, me) = (world.units[mover_idx].owner, world.units[mover_idx].lx, world.units[mover_idx].id);
    // Candidates from the 3 x 3 block of 128 px buckets around the cell (like the engine's
    // MCount cells); the lowest index wins.
    let (bx, by) = (cell.0 >> 3, cell.1 >> 3);
    let mut found: Option<usize> = None;
    for y in by - 1..=by + 1 {
        for x in bx - 1..=bx + 1 {
            for &j in world.buckets.get(x, y) {
                let j = j as usize;
                let o = &world.units[j];
                if o.alive
                    && !o.building
                    && o.id != me
                    && !o.in_motion
                    && o.owner == owner
                    && o.orders.is_empty()
                    && (o.cell.0 - cell.0).abs() < lx.max(o.lx)
                    && (o.cell.1 - cell.1).abs() < lx.max(o.lx)
                    && found.is_none_or(|f| j < f)
                {
                    found = Some(j);
                }
            }
        }
    }
    let Some(j) = found else { return };
    if world.units[j].death != 0 {
        return;
    }
    let finished = world.units[j].frame >= world.units[j].anim_frames - FRM_DEC;
    if world.units[j].stand_time > 20 && world.units[j].lx == lx && finished {
        let (a, b) = (mover_idx.min(j), mover_idx.max(j));
        let (lo, hi) = world.units.split_at_mut(b);
        let (p, q) = (&mut lo[a], &mut hi[0]);
        std::mem::swap(&mut p.pos, &mut q.pos);
        std::mem::swap(&mut p.cell, &mut q.cell);
        std::mem::swap(&mut p.dir, &mut q.dir);
        return;
    }
    let map = world.map.clone();
    let t = world.catalog.clone();
    let m = layer(&map, t.get(world.units[j].kind));
    let (cx, cy) = world.units[j].cell;
    for r in 1..8i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if !m.check_bar(x, y, 1, 1) && !world.occupancy.check_bar(x, y, 1, 1) {
                    let pos = Pos::new(crate::path::cell_center(x, 1), crate::path::cell_center(y, 1));
                    send_to(&mut world.units[j], pos, 0, true);
                    return;
                }
            }
        }
    }
}

/// Integer square root, as `(int)sqrt(n)` for the sizes involved.
fn isqrt(n: i32) -> i32 {
    let mut r = 0;
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// Group move (`GroupSend` 0x4ac776 with the planner `Place` 0x4ab607 and `Send` 0x4ac182):
/// brigade members move with their brigade, free units get slots of a grid `W` wide (across
/// the movement direction) and `D` deep centred on the target, matched to units by sorting
/// on the two axes so paths do not cross; every unit then routes on its own.
pub fn group_send(world: &mut World, ids: &[usize], x: i32, y: i32, dir: Option<u8>, prio: u8) {
    let mut brigades: Vec<u16> = ids.iter().filter_map(|&i| world.units[i].brigade).collect();
    brigades.sort_unstable();
    brigades.dedup();
    let free: Vec<usize> = ids.iter().copied().filter(|&i| world.units[i].brigade.is_none()).collect();
    let (x16, y16) = (x << 4, y << 4);
    if !brigades.is_empty() {
        let centers: Vec<(i32, i32)> =
            brigades.iter().map(|&b| crate::formation::center(world, usize::from(b)).unwrap_or((x16, y16))).collect();
        let n = centers.len() as i64;
        let gx = (centers.iter().map(|c| i64::from(c.0)).sum::<i64>() / n) as i32;
        let gy = (centers.iter().map(|c| i64::from(c.1)).sum::<i64>() / n) as i32;
        for (&b, c) in brigades.iter().zip(&centers) {
            // An attack-move (priority & 0x7f == 0) lets a formation fight on the way (AttEnm).
            world.brigades[usize::from(b)].att_enm = prio & 0x7f == 0;
            crate::formation::send(world, usize::from(b), x16 + c.0 - gx, y16 + c.1 - gy, dir);
            // Every brigade sent stops standing ground, with its normal bonuses (0x4ad0b5).
            crate::formation::cancel_stand_ground(world, usize::from(b), false);
        }
    }
    if free.is_empty() {
        return;
    }
    let n = free.len() as i64;
    let gx = (free.iter().map(|&i| i64::from(world.units[i].pos.x)).sum::<i64>() / n) as i32;
    let gy = (free.iter().map(|&i| i64::from(world.units[i].pos.y)).sum::<i64>() / n) as i32;
    let (vx, vy) = match dir {
        Some(d) => (crate::trig::cos(d) << 4, crate::trig::sin(d) << 4),
        None => (gx - x16, gy - y16),
    };
    let slots = place(world, &free, x16, y16, vx, vy);
    let map = world.map.clone();
    for (i, (sx, sy)) in slots {
        // Planner.Send (0x4ac20d) ends stand ground of every free unit it sends.
        world.units[i].stand_ground = false;
        let sw = stance_walker(world, i);
        let areas = areas_of(&map, world.catalog.get(world.units[i].kind));
        smart_send_to_ex(&mut world.units[i], areas, x, y, (sx - x16) >> 4, (sy - y16) >> 4, prio, false, sw);
        // With a direction, the unit turns to it after the move (0x4ac2d3, appended).
        if let Some(d) = dir
            && !world.catalog.get(world.units[i].kind).transport
        {
            turn_to(world, i, d);
        }
    }
}

/// 0x41bfdb with order type 2: a turn to `dir` appended to the orders (not for transports,
/// `NEWSHEEPS` ships or buildings).
pub fn turn_to(world: &mut World, i: usize, dir: u8) {
    let t = world.catalog.get(world.units[i].kind);
    if t.transport || t.motion_style == crate::catalog::MotionStyle::NewShips || t.building {
        return;
    }
    world.units[i].orders.insert(0, Order::Turn { dir });
}

fn catalog_rotate(world: &World, i: usize) -> i32 {
    world.catalog.get(world.units[i].kind).rotate
}

/// The turn link (0x41bd0d, units without `COMPLEXROTATE`): within `ROTATE` of the target
/// direction the unit faces it and the order ends, otherwise it turns by `ROTATE`.
fn turn_link(u: &mut Unit, step: i32, dir: u8) {
    // The finished move left no destination (`DestX = -1`).
    u.dest = None;
    let dd = u.dir.wrapping_sub(dir) as i8;
    if i32::from(dd).abs() <= step {
        u.dir = dir;
        u.pop_order();
    } else if dd > 0 {
        u.dir = u.dir.wrapping_sub(step as u8);
    } else {
        u.dir = u.dir.wrapping_add(step as u8);
    }
}

/// `Patrol` (planner 0x4ac45d, order 0x4ea2c0): each free unit patrols between where it
/// stands and its place in a group move to `(x, y)`; it keeps its stand-ground state.
pub fn patrol(world: &mut World, ids: &[usize], x: i32, y: i32) {
    let free: Vec<usize> = ids.iter().copied().filter(|&i| world.units[i].brigade.is_none()).collect();
    if free.is_empty() {
        return;
    }
    let n = free.len() as i64;
    let (x16, y16) = (x << 4, y << 4);
    let gx = (free.iter().map(|&i| i64::from(world.units[i].pos.x)).sum::<i64>() / n) as i32;
    let gy = (free.iter().map(|&i| i64::from(world.units[i].pos.y)).sum::<i64>() / n) as i32;
    for (i, (sx, sy)) in place(world, &free, x16, y16, gx - x16, gy - y16) {
        let u = &mut world.units[i];
        u.orders.clear();
        u.path.clear();
        u.orders.push(Order::Patrol { a: u.pos.pixels(), b: (sx >> 4, sy >> 4), back: true });
        u.prio = 0;
    }
}

/// `PatrolLink` (0x4ea3f0): the next leg, a priority-0 move put in front of the patrol.
fn patrol_link(world: &mut World, i: usize) {
    let map = world.map.clone();
    let areas = areas_of(&map, world.catalog.get(world.units[i].kind));
    let sw = stance_walker(world, i);
    let u = &mut world.units[i];
    let Some(Order::Patrol { a, b, back }) = u.orders.last_mut() else { return };
    let (tx, ty) = if *back { *a } else { *b };
    *back = !*back;
    u.prio = 0;
    smart_send_to_ex(u, areas, tx, ty, 0, 0, 0, true, sw);
}

/// Whether a unit walks in stance 1 on attack-moves: it has `#MOTION_L0` and no `FASTUNIT`.
fn stance_walker(world: &World, i: usize) -> bool {
    let t = world.catalog.get(world.units[i].kind);
    !t.fast_unit && anim_frames(t, AnimKind::MotionLs(0)).is_some()
}

/// The planner `Place` (0x4ab607): returns `(unit index, slot position in 1/16 px)`.
fn place(world: &World, ids: &[usize], x: i32, y: i32, vx: i32, vy: i32) -> Vec<(usize, (i32, i32))> {
    let (mut vx, vy) = (vx >> 4, vy >> 4);
    if vx == 0 && vy == 0 {
        vx = 1;
    }
    let mut units: Vec<usize> = ids.iter().copied().filter(|&i| world.units[i].living()).collect();
    let n = units.len() as i32;
    if n == 0 {
        return Vec::new();
    }
    let s = isqrt(n);
    let (mut w, mut d) = (2 * s, s >> 1);
    if n < 4 {
        (d, w) = (1, n);
    }
    for _ in 0..2 {
        if d * w < n {
            if d * w + d >= n {
                w += 1;
            } else if d * w + w >= n {
                d += 1;
            } else {
                w += 1;
                d += 1;
            }
        }
    }
    let (px, py) = (vy, -vx);
    let key = |i: usize, a: i32, b: i32| {
        let p = world.units[i].pos;
        i64::from(p.x >> 5) * i64::from(a >> 4) + i64::from(p.y >> 5) * i64::from(b >> 4)
    };
    units.sort_by_key(|&i| key(i, px, py));
    for chunk in units.chunks_mut(d.max(1) as usize) {
        chunk.sort_by_key(|&i| key(i, -py, px));
    }
    let catalog = &world.catalog;
    let first = world.units[units[0]].kind;
    let mixed = units.iter().any(|&i| world.units[i].kind != first);
    let mut r2 = units.iter().map(|&i| catalog.get(world.units[i].kind).radius2 << 4).max().unwrap_or(0);
    if mixed {
        r2 = r2.min(256);
    }
    let sp = 4 * r2;
    let nn = norma(px, py).max(1);
    let (sx, sy) = (px * sp / nn, py * sp / nn);
    let cx0 = ((w - 1) * sx - (d - 1) * sy) >> 1;
    let cy0 = ((d - 1) * sx + (w - 1) * sy) >> 1;
    let mut out = Vec::with_capacity(units.len());
    let mut k = 0;
    for i in 0..w {
        for j in 0..d {
            if let Some(&u) = units.get(k) {
                out.push((u, (x + i * sx - j * sy - cx0, y + j * sx + i * sy - cy0)));
                k += 1;
            }
        }
    }
    out
}
