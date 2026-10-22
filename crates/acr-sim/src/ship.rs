//! Ships: their motion (`MOTIONSTYLE SHEEPS` 0x4d5c00, `NEWSHEEPS` 0x4d5810 with
//! `CalculateMotion2` 0x4d4540), transports (docking `ParkWaterNewMonster` 0x54aa9c and
//! `WaterNewMonsterSendToLink` 0x54ac93, boarding `GoToTransport` 0x54d78d, unloading
//! `LeaveTransport` 0x54e000), broadsides (`WaterAttackLink` 0x520ef8), ports and fishing.
//! See `docs/simulation.md`.
//!
//! Ships keep a speed and a velocity per tick, accelerate by 6 up to `2·step·RATE/16`, turn 2
//! per tick while they move and slow down by a quarter per tick without a target. Like land
//! units they reserve their `Lx × Lx` box of 16 px cells in the occupancy grid; they test the
//! box one step ahead against the water lock map and other units.

use crate::catalog::{MotionStyle, ObjectType};
use crate::fixed::Pos;
use crate::map::LockMap;
use crate::motion::{anim_frames, set_anim, try_to_stand};
use crate::rng::SyncRandom;
use crate::trig::{cos, get_dir, norma, sin};
use crate::world::{AnimKind, EntityId, FRM_DEC, Order, Unit, World};

/// A transport's docking point (`DstX/DstY`, OB+0x84): unset at creation (0), away (−1, it
/// sails) or docked at a land point 256 px ahead of its centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Dock {
    #[default]
    Unset,
    Away,
    At(Pos),
}

/// `CalculateMotion2` (0x4d4540; `NEWSHEEPS` 0x4d48b0 quantises the heading to the sprite
/// directions).
fn calculate_motion(u: &mut Unit, t: &ObjectType, rate0: i32) {
    let maxv = (t.step * rate0) >> 3;
    match u.dest {
        Some(dest) => {
            let (dx, dy) = (dest.x - u.pos.x, dest.y - u.pos.y);
            let dis = norma(dx, dy);
            if dis > 64 {
                u.speed = if u.speed < maxv { u.speed + 6 } else { maxv };
                let mut dir1 = get_dir(dx, dy);
                if t.motion_style == MotionStyle::NewShips {
                    let r = t.motion_l.map_or(1, |a| i32::from(a.directions));
                    if r > 1 && r & 1 == 1 {
                        let s = 64 / (r - 1);
                        dir1 = ((((i32::from(dir1 as i8)) + s + 256) / (2 * s)) * (2 * s)) as u8;
                    }
                }
                let dd = dir1.wrapping_sub(u.dir) as i8;
                if i32::from(dd).abs() < 4 {
                    u.dir = dir1;
                    u.vel = (dx * u.speed / dis, dy * u.speed / dis);
                } else {
                    u.dir = if dd > 0 { u.dir.wrapping_add(2) } else { u.dir.wrapping_sub(2) };
                    u.vel = ((u.speed * cos(u.dir)) >> 8, (u.speed * sin(u.dir)) >> 8);
                }
            } else {
                u.dest = None;
            }
        }
        None => {
            if u.speed <= 8 {
                (u.vel, u.speed) = ((0, 0), 0);
            } else {
                u.vel = (u.vel.0 - (u.vel.0 >> 2), u.vel.1 - (u.vel.1 >> 2));
                u.speed -= u.speed >> 2;
            }
        }
    }
}

/// The engine's direction tables of `TryDir` (0x644c74 / 0x644c9c, quirks included).
const DX8: [i32; 8] = [0, 0, 1, 0, 0, -1, -1, -1];
const DY8: [i32; 8] = [-1, -1, 0, 0, 1, 0, 0, -1];

fn try_dir(u: &Unit, water: &LockMap, occ: &LockMap, d: i32) -> bool {
    let d = (d & 7) as usize;
    let (x, y) = (((u.pos.x - u.lx * 128) >> 8) + DX8[d], ((u.pos.y - u.lx * 128) >> 8) + DY8[d]);
    let s = u.lx + (d as i32 & 1);
    !water.check_bar(x, y, s, s) && !occ.check_bar(x, y, s, s)
}

/// `CalculateMotion2Limited` (0x4d4cf0): slides along the nearest free of the eight
/// directions, turning 4 per tick.
fn limited(u: &mut Unit, water: &LockMap, occ: &LockMap) -> bool {
    let dir = get_dir(u.vel.0, u.vel.1);
    let d8 = ((i32::from(dir) + 0x50) >> 5) & 7;
    let dir0 = ((d8 - 2) << 5) as u8;
    let ddd = dir.wrapping_sub(dir0) as i8;
    let new = if try_dir(u, water, occ, d8) {
        dir0
    } else if ddd > 0 {
        if try_dir(u, water, occ, d8 + 1) {
            dir0.wrapping_add(32)
        } else if try_dir(u, water, occ, d8 - 1) {
            dir0.wrapping_sub(32)
        } else if try_dir(u, water, occ, d8 + 2) {
            dir0.wrapping_add(64)
        } else {
            return false;
        }
    } else if try_dir(u, water, occ, d8 - 1) {
        dir0.wrapping_sub(32)
    } else if try_dir(u, water, occ, d8 + 1) {
        dir0.wrapping_add(32)
    } else if try_dir(u, water, occ, d8 - 2) {
        dir0.wrapping_sub(64)
    } else {
        return false;
    };
    let dd = new.wrapping_sub(u.dir) as i8;
    u.dir = if i32::from(dd).abs() < 4 {
        new
    } else if dd > 0 {
        u.dir.wrapping_add(4)
    } else {
        u.dir.wrapping_sub(4)
    };
    u.vel = ((u.speed * cos(u.dir)) >> 8, (u.speed * sin(u.dir)) >> 8);
    true
}

/// What a ship's step asks for afterwards: push other ships out of a box.
struct Push {
    x: i32,
    y: i32,
}

/// One step of a ship's motion handler (`SHEEPS` 0x4d5c00, `NEWSHEEPS` 0x4d5810).
fn handler(
    u: &mut Unit,
    t: &ObjectType,
    water: &LockMap,
    occ: &mut LockMap,
    rnd: &mut SyncRandom,
    rate0: i32,
) -> Option<Push> {
    let new_style = t.motion_style == MotionStyle::NewShips;
    let (mid, old_dir) = (u.frame < u.anim_frames - FRM_DEC, u.dir);
    if new_style {
        u.frame = if mid { u.frame + FRM_DEC } else { 0 };
    } else if mid {
        u.frame += FRM_DEC;
        return None;
    }
    if u.orders.is_empty() {
        u.dest = None;
    }
    calculate_motion(u, t, rate0);
    let dir_changed = u.dir != old_dir;
    if new_style && mid {
        u.dir = old_dir;
    }
    let mut first = true;
    let mut crash = false;
    let mut push = None;
    loop {
        if u.speed == 0 {
            if new_style && u.frame >= u.anim_frames - FRM_DEC {
                let k = match (u.target().is_some(), u.stance) {
                    (true, s) if s > 0 => AnimKind::PStand(s - 1),
                    _ => AnimKind::Stand,
                };
                set_anim(u, t, k);
                u.frame = 0;
            }
            u.stand_time = u.stand_time.wrapping_add(1);
            return push;
        }
        let nsp = if new_style { 1 } else { t.motion_frames().max(1) };
        let (nx, ny) = (u.pos.x + u.vel.0 * nsp, u.pos.y + u.vel.1 * nsp);
        let (cx, cy) = ((nx - u.lx * 128) >> 8, (ny - u.lx * 128) >> 8);
        let (ox, oy) = u.cell;
        occ.clr_bar(ox, oy, u.lx);
        let g = !water.check_bar(cx, cy, u.lx, u.lx);
        let l = !occ.check_bar(cx, cy, u.lx, u.lx);
        if g && l {
            // MoveObject (0x4d5030).
            let save = u.frame;
            u.pos.x += u.vel.0;
            u.pos.y += u.vel.1;
            set_anim(u, t, AnimKind::MotionL);
            u.frame = 0;
            rnd.rando();
            if new_style {
                if (dir_changed && !mid) || u.speed == 0 {
                    set_anim(u, t, AnimKind::Stand);
                    u.frame = 0;
                } else {
                    u.frame = if save >= u.anim_frames { 0 } else { save };
                }
            }
            if !u.unlimited {
                occ.set_bar(cx, cy, u.lx);
            }
            u.cell = (cx, cy);
            u.stand_time = 0;
            u.in_motion = true;
            return push;
        }
        if g {
            if first {
                crash = limited(u, water, occ);
                if crash {
                    first = false;
                    occ.set_bar(ox, oy, u.lx);
                    continue;
                }
            }
            u.stand_time = u.stand_time.wrapping_add(1);
            if !crash {
                // StopObject (0x4d5090).
                try_to_stand(u, t, rnd, false);
                (u.speed, u.vel) = (0, (0, 0));
                if u.stand_time > 15 {
                    push = Some(Push { x: ox, y: oy });
                }
            }
        } else {
            if first {
                crash = limited(u, water, occ);
                if crash {
                    first = false;
                    occ.set_bar(ox, oy, u.lx);
                    continue;
                }
            }
            if !crash {
                if u.stand_time > 15 {
                    push = Some(Push { x: cx, y: cy });
                }
                u.stand_time = u.stand_time.wrapping_add(1);
                u.path.clear();
            }
        }
        if !u.unlimited {
            occ.set_bar(ox, oy, u.lx);
        }
        u.in_motion = false;
        return push;
    }
}

/// `TryToPushShipsBy` (0x4d56d0): ships overlapping the box shifted one cell along the
/// pusher's heading get speed 200 along their own heading (docked transports excepted).
fn push_ships(w: &mut World, me: usize, x: i32, y: i32) {
    let (lx, dir) = (w.units[me].lx, w.units[me].dir);
    let (ddx, ddy) = match dir {
        0 => return,
        64 => (0, 1),
        128 => (-1, 0),
        192 => (0, -1),
        1..=63 => (1, 1),
        65..=127 => (-1, 1),
        129..=191 => (-1, -1),
        _ => (1, -1),
    };
    let (ccx, ccy) = ((x + 6) >> 3, (y + 6) >> 3);
    for (bx, by) in [(x + ddx, y), (x, y + ddy)] {
        if (bx, by) == (x, y) {
            continue;
        }
        for dy in -2..=2 {
            for dx in -2..=2 {
                let cell: Vec<u32> = w.buckets.get(ccx + dx, ccy + dy).to_vec();
                for k in cell {
                    let k = k as usize;
                    let u = &w.units[k];
                    let t = w.catalog.get(u.kind);
                    if k == me || !u.living() || t.medium != crate::catalog::Medium::Water {
                        continue;
                    }
                    if t.transport && matches!(u.dock, Dock::At(_)) {
                        continue;
                    }
                    let ul = u.lx;
                    if (2 * u.cell.0 + ul - (2 * bx + lx)).abs() < ul + lx
                        && (2 * u.cell.1 + ul - (2 * by + lx)).abs() < ul + lx
                    {
                        let u = &mut w.units[k];
                        if u.speed < 200 {
                            u.speed = 200;
                            u.vel = ((200 * cos(u.dir)) >> 8, (200 * sin(u.dir)) >> 8);
                        }
                    }
                }
            }
        }
    }
}

/// Runs the motion of ship `i` for one tick (after its order link).
pub fn motion(w: &mut World, i: usize) {
    let catalog = w.catalog.clone();
    let map = w.map.clone();
    let rate0 = w.adv(w.units[i].owner, w.units[i].kind).rate[0];
    let World { units, occupancy, random, .. } = &mut *w;
    let u = &mut units[i];
    let t = catalog.get(u.kind);
    if let Some(p) = handler(u, t, &map.water, occupancy, random, rate0) {
        push_ships(w, i, p.x, p.y);
    }
}

/// `ParkWaterNewMonster` (0x54aa9c), for a move of a transport to `pos`: the dock is away;
/// if the box around the target is open water the move goes on as usual (false); otherwise
/// the nearest free box of the rings around it becomes a docking order (true, also when none
/// is found and nothing is ordered).
pub fn park(u: &mut Unit, water: &LockMap, pos: Pos, prio: u8) -> bool {
    u.dock = Dock::Away;
    let lx = u.lx;
    let (mut xx, mut yy) = ((pos.x >> 8) - (lx >> 1), (pos.y >> 8) - (lx >> 1));
    if !water.check_bar(xx - 1, yy - 1, lx + 2, lx + 2) {
        return false;
    }
    let found = (0..40).find_map(|r| {
        crate::morale::ring(r).iter().find(|&&(dx, dy)| !water.check_bar(xx + dx, yy + dy, lx, lx)).copied()
    });
    let Some((dx, dy)) = found else { return true };
    (xx, yy) = (xx + dx, yy + dy);
    if u.prio > prio {
        return false;
    }
    let prio = prio & 0x7f;
    u.orders.push(Order::WaterSendTo { cell: (xx, yy), prio, turning: false });
    u.prio = prio;
    true
}

/// Every new move of a transport (`NewMonsterSendTo` 0x517045) goes through
/// [`park`] first: a move order whose target has not been checked yet.
pub fn check_park(w: &mut World, i: usize) {
    if !w.catalog.get(w.units[i].kind).transport {
        return;
    }
    let map = w.map.clone();
    let u = &mut w.units[i];
    let Some(Order::SendTo { cell, prio }) = u.orders.last().cloned() else { return };
    if u.park_checked == Some(cell) {
        return;
    }
    u.park_checked = Some(cell);
    let target = u.dest.unwrap_or(Pos::new(cell.0 * 256 + u.lx * 128, cell.1 * 256 + u.lx * 128));
    u.pop_order();
    let p = u.prio;
    u.prio = 0;
    if !park(u, &map.water, target, prio) {
        u.orders.push(Order::SendTo { cell, prio });
        u.prio = prio;
    } else if !matches!(u.orders.last(), Some(Order::WaterSendTo { .. })) {
        u.prio = if u.orders.is_empty() { 0 } else { p };
    }
}

/// `GetBestDirection` (0x54a52f): the mean direction of the blocked boxes 8 cells around.
fn best_direction(water: &LockMap, x: i32, y: i32, lx: i32, cur: u8) -> u8 {
    let (mut sx, mut sy, mut n) = (0, 0, 0);
    for d in (0..=248).step_by(8) {
        let (dx, dy) = (cos(d as u8) >> 5, sin(d as u8) >> 5);
        if water.check_bar(x + dx, y + dy, lx, lx) {
            (sx, sy, n) = (sx + dx, sy + dy, n + 1);
        }
    }
    if n > 0 { get_dir(sx, sy) } else { cur }
}

/// The land point 256 px ahead of a ship, if a 4 × 4 land box there is free.
fn shore_point(w: &World, u: &Unit) -> Option<Pos> {
    let p = Pos::new(u.pos.x + 16 * cos(u.dir), u.pos.y + 16 * sin(u.dir));
    (!w.map.land.check_bar((p.x >> 8) - 2, (p.y >> 8) - 2, 4, 4)).then_some(p)
}

/// `WaterNewMonsterSendToLink` (0x54ac93): sail to the docking box, then turn the side
/// facing the land and dock.
pub fn water_send_to_link(w: &mut World, i: usize, cell: (i32, i32), prio: u8, turning: bool) {
    let catalog = w.catalog.clone();
    let map = w.map.clone();
    let u = &mut w.units[i];
    let t = catalog.get(u.kind);
    u.prio = prio;
    u.stance = 0;
    u.dock = Dock::Unset;
    if turning {
        let d = best_direction(&map.water, cell.0, cell.1, u.lx, u.dir);
        let dd = d.wrapping_sub(u.dir) as i8;
        if i32::from(dd).abs() <= 2 {
            u.dir = d;
            u.pop_order();
            if t.transport
                && let Some(p) = shore_point(w, &w.units[i])
            {
                w.units[i].dock = Dock::At(p);
            }
        } else {
            u.dir = if dd > 0 { u.dir.wrapping_add(2) } else { u.dir.wrapping_sub(2) };
        }
        return;
    }
    let dist = (u.cell.0 - cell.0).abs().max((u.cell.1 - cell.1).abs());
    if dist < 8 && map.water.check_bar(u.cell.0 - 2, u.cell.1 - 2, u.lx + 4, u.lx + 4) {
        u.path.clear();
        u.dest = None;
        if let Some(Order::WaterSendTo { turning, .. }) = u.orders.last_mut() {
            *turning = true;
        }
    } else {
        let mover = crate::path::Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
        if let Some((x, y)) =
            crate::path::create_path(&map.water, mover, &mut u.path, &mut w.random, cell.0, cell.1, t.stop_distance)
        {
            u.dest = Some(Pos::new(x, y));
        }
    }
}

/// The capacity of a transport: `TRANSPORT n` plus `AddInside`.
fn capacity(w: &World, t: usize) -> usize {
    let u = &w.units[t];
    (w.adv(u.owner, u.kind).max_inside + u.add_inside).max(0) as usize
}

/// `GoToTransport` (0x54d78d): a land unit boards transport `t` that has been docked once:
/// it walks to the landing point, then straight over the water to the transport. A
/// formation member breaks up its formation first.
pub fn go_to_transport(w: &mut World, i: usize, t: usize, prio: u8) -> bool {
    let catalog = w.catalog.clone();
    let Some(tr) = w.units.get(t).filter(|x| x.living() && catalog.get(x.kind).transport) else { return false };
    let land = match tr.dock {
        Dock::Unset => return false,
        Dock::Away => Pos::new(-16, -16),
        Dock::At(p) => p,
    };
    if tr.stage < w.max_stage(tr) || tr.inside.len() >= capacity(w, t) {
        return false;
    }
    let u = &w.units[i];
    if !u.active() || u.building || u.unlimited || catalog.get(u.kind).medium != crate::catalog::Medium::Land {
        return false;
    }
    if let Some(b) = u.brigade.map(usize::from) {
        crate::formation::dissolve(w, b);
    }
    let target = w.units[t].id;
    let u = &mut w.units[i];
    if u.prio > prio && !u.orders.is_empty() {
        return false;
    }
    if matches!(u.orders.last(), Some(Order::GoToTransport { transport, .. }) if *transport == target) {
        return true;
    }
    let prio = prio & 0x7f;
    u.orders.clear();
    u.orders.push(Order::GoToTransport { transport: target, land: (land.x >> 4, land.y >> 4), prio });
    u.prio = prio;
    true
}

/// `GoToTransportLink` (0x54dbb0).
pub fn go_to_transport_link(w: &mut World, i: usize, t: EntityId, land: (i32, i32), prio: u8) {
    let catalog = w.catalog.clone();
    let ti = t.0 as usize;
    w.units[i].prio = prio;
    w.units[i].stance = w.units[i].ground_state;
    let tr = &w.units[ti];
    let Dock::At(dock) = tr.dock else {
        let u = &mut w.units[i];
        u.pop_order();
        if u.unlimited {
            crate::combat::die(w, i);
        }
        return;
    };
    if !tr.living() {
        let u = &mut w.units[i];
        u.pop_order();
        if u.unlimited {
            crate::combat::die(w, i);
        }
        return;
    }
    let t_pos = tr.pos;
    let stop = catalog.get(w.units[i].kind).stop_distance;
    if w.units[ti].inside.len() < capacity(w, ti) {
        let u = &w.units[i];
        if norma(u.pos.x - t_pos.x, u.pos.y - t_pos.y) < stop + 512 && u.unlimited {
            // Aboard: hidden inside the transport.
            let id = u.id;
            let u = &mut w.units[i];
            u.orders.clear();
            u.prio = 0;
            u.unlimited = false;
            u.inside_of = Some(t);
            u.dest = None;
            w.units[ti].inside.push(id);
            return;
        }
        if norma(u.pos.x - dock.x, u.pos.y - dock.y) < stop {
            // At the landing point: straight over the water to the transport.
            let World { units, occupancy, .. } = &mut *w;
            let u = &mut units[i];
            if !u.unlimited {
                occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
            }
            u.unlimited = false;
            crate::motion::send_to(u, t_pos, 16, false);
            u.unlimited = true;
            return;
        }
        let map = w.map.clone();
        let u = &mut w.units[i];
        let c = Unit::cell_of(dock, u.lx);
        let mover = crate::path::Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
        if let Some((x, y)) = crate::path::create_path(&map.land, mover, &mut u.path, &mut w.random, c.0, c.1, stop) {
            u.dest = Some(Pos::new(x, y));
        }
    } else if w.units[i].unlimited {
        // Full: back to the shore.
        let (xx, yy) = (land.0 << 4, land.1 << 4);
        let u = &mut w.units[i];
        u.orders.clear();
        u.unlimited = false;
        let mid = Pos::new((xx + t_pos.x) >> 1, (yy + t_pos.y) >> 1);
        let ry = w.random.rando();
        let rx = w.random.rando();
        let u = &mut w.units[i];
        u.orders.push(Order::SendTo { cell: Unit::cell_of(Pos::new(xx + (rx >> 6), yy + (ry >> 6)), u.lx), prio: 16 });
        u.orders.push(Order::ClearUnlimited);
        crate::motion::send_to(u, mid, 16, false);
        u.unlimited = true;
    } else {
        w.units[i].pop_order();
    }
}

/// `LeaveTransport` (0x54e000): at a shore (a free land box 256 px ahead) the transport is
/// docked there and lets one passenger of `kind` off after 3 ticks; away from a shore its
/// dock is away.
pub fn leave_transport(w: &mut World, t: usize, kind: crate::TypeId) {
    let u = &w.units[t];
    if u.inside.is_empty() || u.dock == Dock::Unset {
        return;
    }
    let Some(p) = shore_point(w, u) else {
        w.units[t].dock = Dock::Away;
        return;
    };
    w.units[t].dock = Dock::At(p);
    if !w.units[t].inside.iter().any(|e| w.units[e.0 as usize].kind == kind) {
        return;
    }
    w.units[t].orders.push(Order::LeaveTransport { kind, delay: 3 });
}

/// `LeaveAll` (0x54df93): one unloading order per passenger.
pub fn leave_all(w: &mut World, t: usize) {
    let kinds: Vec<crate::TypeId> = w.units[t].inside.iter().map(|e| w.units[e.0 as usize].kind).collect();
    for k in kinds {
        leave_transport(w, t, k);
    }
}

/// `LeaveTransportLink` (0x54e188): after the delay one passenger of the type appears at the
/// transport's centre, walks straight to the 3/4 point towards the landing point, takes a
/// free spot on land and walks to the landing point ± 16 px.
pub fn leave_transport_link(w: &mut World, t: usize, kind: crate::TypeId, delay: u8) {
    if delay > 0 {
        if let Some(Order::LeaveTransport { delay, .. }) = w.units[t].orders.last_mut() {
            *delay -= 1;
        }
        return;
    }
    let tr = &w.units[t];
    let Dock::At(dock) = tr.dock else {
        w.units[t].pop_order();
        return;
    };
    let Some(k) = tr.inside.iter().position(|e| w.units[e.0 as usize].kind == kind) else {
        w.units[t].pop_order();
        return;
    };
    let t_pos = tr.pos;
    let p = w.units[t].inside.remove(k).0 as usize;
    let ry = w.random.rando() & 511;
    let rx = w.random.rando() & 511;
    let u = &mut w.units[p];
    u.inside_of = None;
    u.pos = t_pos;
    u.cell = Unit::cell_of(t_pos, u.lx);
    u.orders.clear();
    u.orders
        .push(Order::SendTo { cell: Unit::cell_of(Pos::new(dock.x + rx - 256, dock.y + ry - 256), u.lx), prio: 16 });
    u.orders.push(Order::ClearUnlimited);
    let three = Pos::new((3 * dock.x + t_pos.x) >> 2, (3 * dock.y + t_pos.y) >> 2);
    crate::motion::send_to(u, three, 16, false);
    u.unlimited = true;
    w.units[t].pop_order();
}

/// `FINDNEAREMPTY` (0x527d02): the first offset of the rings 1..30 around a lock cell with a
/// free 9 × 9 land box.
pub fn find_near_empty(land: &LockMap, x: i32, y: i32) -> Option<(i32, i32)> {
    (1..30).find_map(|r| {
        crate::morale::ring(r)
            .iter()
            .find(|&&(dx, dy)| !land.check_bar(x + dx - 4, y + dy - 4, 9, 9))
            .map(|&(dx, dy)| (x + dx, y + dy))
    })
}

/// `FindPortPlace` (0x532cdb): the water point of a port at `pos` — the free 21 × 21 water
/// box nearest to it on the square rings 5..44 (lock cells), taken as soon as one lies closer
/// than the ring plus 3.
pub fn port_point(water: &LockMap, pos: Pos) -> Option<(i32, i32)> {
    let (mx, my) = (pos.x >> 8, pos.y >> 8);
    let mut best: Option<(i32, (i32, i32))> = None;
    for r in 5..45 {
        let mut ring = Vec::new();
        for y in my - r..=my + r {
            ring.push((mx + r, y));
        }
        for y in my - r..=my + r {
            ring.push((mx - r, y));
        }
        for x in mx - r + 1..mx + r {
            ring.push((x, my - r));
        }
        for x in mx - r + 1..mx + r {
            ring.push((x, my + r));
        }
        for (x, y) in ring {
            if !water.check_bar(x - 10, y - 10, 21, 21) {
                let d = norma(x - mx, y - my);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, (x, y)));
                }
            }
        }
        if best.is_some_and(|b| b.0 < r + 3) {
            break;
        }
    }
    best.map(|b| b.1)
}

/// `ClearBirthPlace` (0x49e479): idle ships in the 5 × 5 cells of 128 px around a port's water
/// point (lock cells) sail 300..700 px away to a free 16 × 16 box (40 tries of two random
/// numbers each, then 40 more testing only the water).
pub fn clear_birth_place(w: &mut World, px: i32, py: i32) {
    let (cx, cy) = (px >> 3, py >> 3);
    let map = w.map.clone();
    for dy in -2..=2 {
        for dx in -2..=2 {
            let cell: Vec<u32> = w.buckets.get(cx + dx, cy + dy).to_vec();
            for k in cell {
                let k = k as usize;
                let u = &w.units[k];
                if !u.living() || w.catalog.get(u.kind).medium != crate::catalog::Medium::Water || u.vel != (0, 0) {
                    continue;
                }
                let mut sent = false;
                for pass in 0..2 {
                    for _ in 0..40 {
                        let ddx = (w.random.rando() & 0x3ff) - 0x200;
                        let ddy = (w.random.rando() & 0x3ff) - 0x200;
                        let n = norma(ddx, ddy);
                        let (bx, by) = (px + (ddx >> 4) - 8, py + (ddy >> 4) - 8);
                        if 300 < n
                            && n < 700
                            && !map.water.check_bar(bx, by, 16, 16)
                            && (pass == 1 || !w.occupancy.check_bar(bx, by, 16, 16))
                        {
                            let (ux, uy) = w.units[k].pos.pixels();
                            let areas = map.water_areas.as_ref();
                            crate::motion::smart_send_to(&mut w.units[k], areas, ux + ddx, uy + ddy, 0, 0, 0x80);
                            sent = true;
                            break;
                        }
                    }
                    if sent {
                        break;
                    }
                }
            }
        }
    }
}

/// `ProcessFishing` (0x54f1b0): every 51 ticks (at 37) idle units with a `FishAmount` start
/// fishing (`Fishing` 0x54e5ee, priority 0).
pub fn process_fishing(w: &mut World) {
    if w.tick.0 % 51 != 37 {
        return;
    }
    for i in 0..w.units.len() {
        let u = &w.units[i];
        if !u.active() || u.building || !u.orders.is_empty() || u.switched_off || w.adv(u.owner, u.kind).fish.1 == 0 {
            continue;
        }
        let u = &mut w.units[i];
        u.orders.push(Order::Fishing);
        u.prio = 0;
    }
}

/// `FishingLink` (0x54eb16): with a full catch to the nearest own port's water point, where
/// within 400 px (on ticks `& 15 == 4`) the catch becomes food and the boat rests 60 ticks;
/// otherwise the catch grows by `FishSpeed/128` per tick and the boat sails between random
/// spots within 32 lock cells.
pub fn fishing_link(w: &mut World, i: usize) {
    if w.units[i].switched_off {
        w.units[i].pop_order();
        return;
    }
    let (owner, kind) = (w.units[i].owner, w.units[i].kind);
    let (speed, amount) = w.adv(owner, kind).fish;
    let map = w.map.clone();
    w.units[i].prio = 0;
    let u = &w.units[i];
    if u.catch.0 >= amount {
        let (x, y) = (u.pos.x >> 8, u.pos.y >> 8);
        let port = w
            .units
            .iter()
            .filter(|p| p.owner == owner && p.active() && w.catalog.get(p.kind).port)
            .filter_map(|p| p.port_point)
            .min_by_key(|&(px, py)| norma(px - x, py - y));
        let Some((px, py)) = port else { return };
        let u = &mut w.units[i];
        if norma((px << 8) - u.pos.x, (py << 8) - u.pos.y) < 400 * 16 {
            if w.tick.0 & 15 == 4 {
                let c = u.catch.0;
                u.catch.0 = 0;
                u.delay = 60;
                u.max_delay = 60;
                let p = &mut w.players[usize::from(owner) & 7];
                p.resources[crate::economy::FOOD] += c;
                p.gathered[2] += c;
            }
        } else if u.dest.is_none() {
            go_water(w, i, &map, (px, py));
        }
        return;
    }
    let u = &mut w.units[i];
    u.catch.1 += speed << crate::world::SPEED_SH;
    u.catch.0 += u.catch.1 >> 7;
    u.catch.1 &= 127;
    if u.delay != 0 {
        return;
    }
    let (x, y) = (u.pos.x >> 8, u.pos.y >> 8);
    match u.catch.2 {
        Some((sx, sy)) if norma(sx - x, sy - y) >= 10 => {
            if u.dest.is_none() {
                go_water(w, i, &map, (sx, sy));
            }
        }
        _ => {
            w.units[i].catch.2 = None;
            for _ in 0..20 {
                let x1 = x + (w.random.rando() & 63) - 32;
                let y1 = y + (w.random.rando() & 63) - 32;
                if !map.water.check_bar(x1 - 4, y1 + 6, 8, 8) {
                    w.units[i].catch.2 = Some((x1, y1));
                    go_water(w, i, &map, (x1, y1));
                    break;
                }
            }
        }
    }
}

/// A path over the water to lock cell `(x, y)` for the motion handler.
fn go_water(w: &mut World, i: usize, map: &crate::map::MapGrid, (x, y): (i32, i32)) {
    let t = w.catalog.get(w.units[i].kind);
    let stop = t.stop_distance;
    let u = &mut w.units[i];
    let mover = crate::path::Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
    if let Some((px, py)) = crate::path::create_path(&map.water, mover, &mut u.path, &mut w.random, x, y, stop) {
        u.dest = Some(Pos::new(px, py));
    }
}

/// Whether a type moves like a ship (`SHEEPS`, `NEWSHEEPS`).
pub fn is_ship(t: &ObjectType) -> bool {
    matches!(t.motion_style, MotionStyle::Ships | MotionStyle::NewShips)
}

/// The frames of a ship's walk (1 for the big ships).
pub fn walk_frames(t: &ObjectType) -> i32 {
    anim_frames(t, AnimKind::MotionL).unwrap_or(1)
}

/// `WaterAttackLink` (0x520ef8), for ships with guns (`MULTIWEAPON`): with the target in the
/// range of the last index whose detection range holds it, the ship turns the nearer
/// broadside towards it (2 per tick) and, within 8 of it, fires a random gun once its reload
/// is over (`ZALP` shots per volley); out of range it sails closer unless it stands ground.
pub fn water_attack_link(w: &mut World, i: usize, target: EntityId, prio: u8) {
    let catalog = w.catalog.clone();
    let j = target.0 as usize;
    let u = &mut w.units[i];
    u.prio = prio;
    u.dest = None;
    let t = catalog.get(u.kind);
    let Some((gz, guns)) = t.guns.clone() else {
        u.pop_order();
        return;
    };
    let valid = w.units.get(j).is_some_and(|e| e.living() && w.units[i].hostile(e));
    if !valid || guns.is_empty() {
        w.units[i].pop_order();
        return;
    }
    // nTimes (0x520f23): the shots left, read once per link.
    let times = match w.units[i].orders.last() {
        Some(Order::Attack { times, .. }) => i32::from(*times),
        _ => 0,
    };
    let (u, e) = (&w.units[i], &w.units[j]);
    let adv = w.adv(u.owner, u.kind);
    let dst = norma(e.pos.x - u.pos.x, e.pos.y - u.pos.y) >> 4;
    let Some(am) =
        (0..adv.attacks.len()).rev().find(|&k| adv.attacks[k].detect.0 <= dst && dst <= adv.attacks[k].detect.1)
    else {
        w.units[i].pop_order();
        return;
    };
    let a = &adv.attacks[am];
    let e_pos = e.pos;
    if a.radius.0 <= dst && dst <= a.radius.1 {
        let u = &mut w.units[i];
        u.path.clear();
        let endir = get_dir(e_pos.x - u.pos.x, e_pos.y - u.pos.y);
        let (lb, rb) = (u.dir.wrapping_add(64), u.dir.wrapping_sub(64));
        let (dl, dr) = (lb.wrapping_sub(endir) as i8, rb.wrapping_sub(endir) as i8);
        let left = i32::from(dl).abs() <= i32::from(dr).abs();
        let (dside, aim) = if left {
            (dl, i32::from((endir.wrapping_sub(lb) as i8).unsigned_abs()))
        } else {
            (dr, i32::from(dr.unsigned_abs()))
        };
        if aim >= 8 {
            u.dir = if dside > 0 { u.dir.wrapping_sub(2) } else { u.dir.wrapping_add(2) };
            return;
        }
        if u.delay != 0 {
            return;
        }
        if let Some((i1, i2, cost)) = t.shot_cost
            && (usize::from(i1) == am || usize::from(i2) == am)
        {
            let pl = &mut w.players[usize::from(w.units[i].owner) & 7];
            if pl.resources.iter().zip(&cost).any(|(r, c)| r < c) {
                w.units[i].pop_order();
                return;
            }
            for (r, c) in pl.resources.iter_mut().zip(&cost) {
                *r -= c;
            }
        }
        let k = ((guns.len() as i32 * w.random.rando()) >> 15) as usize;
        let (vx, vy) = guns[k.min(guns.len() - 1)];
        let u = &mut w.units[i];
        let an = u.dir.wrapping_add(128);
        let (c, s) = (cos(an), sin(an));
        let (sx, sy) = if left {
            ((vx * c + vy * s) >> 8, (vx * s - vy * c) >> 8)
        } else {
            ((vx * c - vy * s) >> 8, (vx * s + vy * c) >> 8)
        };
        if t.zalp != 0 && u.volley != 0 {
            u.volley -= 1;
        } else {
            u.volley = t.zalp;
            u.delay = t.attacks[am].pause * u.personal_delay / 100;
            u.max_delay = u.delay;
        }
        if let Some(wid) = a.weapon_id {
            let mut cpt = crate::combat::damage_point(w, j);
            crate::combat::razbros(w, i, &mut cpt);
            let (px, py) = w.units[i].pos.pixels();
            crate::projectile::create(w, wid, (px + sx, py + sy, gz), cpt, Some(i), am as u8, None, (0, 0, 0), 0);
        }
        // 0x521edd: the last counted shot ends the attack, others count down.
        let u = &mut w.units[i];
        if times == 1 {
            u.pop_order();
        } else if times > 1
            && let Some(Order::Attack { times: n, .. }) = u.orders.last_mut()
        {
            *n = n.wrapping_sub(1);
        }
    } else if dst > t.attacks[am].radius.1 {
        let u = &w.units[i];
        // A counted attack does not sail after its target (0x521f34).
        if times > 0 || u.stand_ground {
            w.units[i].pop_order();
            return;
        }
        let map = w.map.clone();
        let u = &mut w.units[i];
        let c = Unit::cell_of(e_pos, u.lx);
        let mover = crate::path::Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
        if let Some((x, y)) =
            crate::path::create_path(&map.water, mover, &mut u.path, &mut w.random, c.0, c.1, t.stop_distance)
        {
            u.dest = Some(Pos::new(x, y));
        }
    } else {
        w.units[i].pop_order();
    }
}
