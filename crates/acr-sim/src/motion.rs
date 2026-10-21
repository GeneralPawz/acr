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

fn set_anim(u: &mut Unit, t: &ObjectType, kind: AnimKind) {
    let info = match kind {
        AnimKind::Stand => t.stand,
        AnimKind::Rest => t.rest,
        AnimKind::Rest1 => t.rest1,
        AnimKind::MotionL => t.motion_l,
        AnimKind::MotionR => t.motion_r,
    };
    u.anim = kind;
    u.anim_frames = info.map_or(1, |a| i32::from(a.frames));
}

/// `NewMonsterSendTo` (0x517470 / 0x517045): sets the steering point at once and gives the
/// unit a `SendTo` order for the lock cell of `pos`. `replace` drops the order stack
/// (player orders); otherwise the order is pushed on top (legs of a routed move).
pub fn send_to(u: &mut Unit, pos: Pos, prio: u8, replace: bool) {
    let cell = Unit::cell_of(pos, u.lx);
    if replace {
        u.orders.clear();
    }
    u.orders.push(Order::SendTo { cell, prio });
    u.dest = Some(pos);
    u.prio = prio;
    u.path.delay = 0;
    u.stand_time = 0;
}

/// `SmartSendTo` (0x52bc95): a move routed over the pathfinding areas to `(x, y)` plus the
/// unit's group offset `(dx, dy)` (world pixels).
pub fn smart_send_to(
    u: &mut Unit,
    areas: Option<&AreaGraph>,
    mut x: i32,
    mut y: i32,
    mut dx: i32,
    mut dy: i32,
    prio: u8,
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
        send_to(u, Pos::from_pixels(x + dx, y + dy), prio, true);
        return;
    };
    if top_zone_of_target(g, &mut x, &mut y).is_none() {
        return;
    }
    u.orders.clear();
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
fn apply_offset(g: &AreaGraph, m: &LockMap, x: i32, y: i32, dx: i32, dy: i32) -> (i32, i32) {
    let n = norma(dx, dy) / 32 + 1;
    for k in (0..=n).rev() {
        let (px, py) = (x + dx * k / n, y + dy * k / n);
        if g.area_at(px >> 6, py >> 6) < 0xFFFE && !m.check_bar((px >> 4) - 2, (py >> 4) - 2, 4, 4) {
            return (px, py);
        }
    }
    (x, y)
}

/// Signal from the motion handler that the unit should clear its way.
struct Push {
    cell: (i32, i32),
}

/// Runs one unit for one tick: its order link, then the motion handler.
pub fn process_unit(world: &mut World, i: usize) {
    let catalog = world.catalog.clone();
    let map = world.map.clone();
    let World { units, occupancy, random, .. } = world;
    let u = &mut units[i];
    let t = catalog.get(u.kind);
    let lay = layer(&map, t);
    match u.orders.last().cloned() {
        Some(Order::SendTo { cell, prio }) => send_to_link(u, t, lay, random, cell, prio),
        Some(Order::SmartSendTo { x, y, dx, dy, prio, area }) => {
            smart_send_to_link(u, map.land_areas.as_ref(), lay, x, y, dx, dy, prio, area)
        }
        None => u.prio = 0,
    }
    if let Some(push) = motion_handler(u, t, lay, occupancy, random) {
        remove_fools(world, i, push.cell);
    }
}

/// `NewMonsterSendToLink` (0x5172c6).
fn send_to_link(u: &mut Unit, t: &ObjectType, m: &LockMap, rnd: &mut SyncRandom, cell: (i32, i32), prio: u8) {
    if u.path.delay != 0 && u.stand_time > 64 {
        u.orders.pop();
        return;
    }
    u.prio = prio;
    let mut c = cell;
    if m.check_bar(c.0 - 1, c.1 - 1, u.lx + 2, u.lx + 2)
        && find_best_position(m, mover(u), &mut c, 80)
        && let Some(Order::SendTo { cell, .. }) = u.orders.last_mut()
    {
        *cell = c;
    }
    if dist_to(mover(u), c.0, c.1) <= t.stop_distance >> 8 {
        if u.frame >= u.anim_frames - FRM_DEC {
            try_to_stand(u, t, rnd, false);
            u.orders.pop();
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
        u.orders.pop();
        send_to(u, Pos::from_pixels(x + dx, y + dy), prio, false);
        return;
    };
    if top_line_clear(g, ux, uy, x + dx, y + dy) {
        u.orders.pop();
        send_to(u, Pos::from_pixels(x + dx, y + dy), prio, false);
        return;
    }
    let cur = match area.or_else(|| nearest_area(g, ux >> 6, uy >> 6).map(|a| a.0)) {
        Some(a) => a,
        None => {
            u.orders.pop();
            return;
        }
    };
    let (mut tx, mut ty) = (x, y);
    let Some(dst) = top_zone_of_target(g, &mut tx, &mut ty) else {
        u.orders.pop();
        return;
    };
    let mut nxt = g.next_hop(cur, dst);
    if nxt == dst || dst == cur {
        u.orders.pop();
        let (px, py) = apply_offset(g, m, tx, ty, dx, dy);
        send_to(u, Pos::from_pixels(px, py), prio, false);
        return;
    }
    if nxt == 0xFFFF {
        u.orders.pop();
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
    let (wx, wy) = apply_offset(g, m, wx, wy, dx, dy);
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
) -> Option<Push> {
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
    let (dx0, dy0) = (nst * (t.step_dx(d) << SPEED_SH), nst * (t.step_dy(d) << SPEED_SH));
    let lx = u.lx;
    occupancy.clr_bar(u.cell.0, u.cell.1, lx);
    let end = Pos::new(u.pos.x + dx0, u.pos.y + dy0);
    let (xf, yf) = Unit::cell_of(end, lx);
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
        return need_push.then_some(Push { cell: (xf, yf) });
    }
    u.vel = (0, 0);
    try_to_move(u, t, m, rnd, d);
    u.dir = d;
    if u.vel != (0, 0) {
        u.cell = (xf, yf);
    }
    occupancy.set_bar(u.cell.0, u.cell.1, lx);
    u.move_stage = 0;
    None
}

/// `TryToMove` (0x50b021) for a unit in the normal stance, walking forwards.
fn try_to_move(u: &mut Unit, t: &ObjectType, m: &LockMap, rnd: &mut SyncRandom, d: u8) {
    rnd.rando();
    let nf = t.motion_frames() >> SPEED_SH;
    let (odx, ody) = (t.step_dx(d) << SPEED_SH, t.step_dy(d) << SPEED_SH);
    let end = Pos::new(u.pos.x + nf * odx, u.pos.y + nf * ody);
    let (cx, cy) = Unit::cell_of(end, u.lx);
    if m.check_bar(cx, cy, u.lx, u.lx) {
        return; // CheckTerra (0x50a60d) failed
    }
    let kind = match (u.left_leg, t.motion_l.is_some(), t.motion_r.is_some()) {
        (true, true, _) | (false, true, false) => AnimKind::MotionL,
        (_, _, true) => AnimKind::MotionR,
        _ => AnimKind::Stand,
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

/// `TryToStand` (0x50a8c5) in the normal stance; `rest` may start an idle animation.
fn try_to_stand(u: &mut Unit, t: &ObjectType, rnd: &mut SyncRandom, rest: bool) {
    rnd.rando();
    u.stand_time = u.stand_time.wrapping_add(1);
    u.in_motion = false;
    if u.frame < u.anim_frames - FRM_DEC {
        return; // only a finished animation is replaced
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
pub fn group_send(world: &mut World, ids: &[usize], x: i32, y: i32, dir: Option<u8>) {
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
            crate::formation::send(world, usize::from(b), x16 + c.0 - gx, y16 + c.1 - gy, dir);
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
    let areas = world.map.land_areas.clone();
    for (i, (sx, sy)) in slots {
        smart_send_to(&mut world.units[i], areas.as_ref(), x, y, (sx - x16) >> 4, (sy - y16) >> 4, PLAYER_PRIO);
    }
}

/// The planner `Place` (0x4ab607): returns `(unit index, slot position in 1/16 px)`.
fn place(world: &World, ids: &[usize], x: i32, y: i32, vx: i32, vy: i32) -> Vec<(usize, (i32, i32))> {
    let (mut vx, vy) = (vx >> 4, vy >> 4);
    if vx == 0 && vy == 0 {
        vx = 1;
    }
    let mut units: Vec<usize> = ids.iter().copied().filter(|&i| world.units[i].alive).collect();
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
