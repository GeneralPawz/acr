//! Construction: placing a building (`CreateObject` 0x5155c5), builders (`BuildObj`
//! 0x523c98, `BuildObjLink` 0x524654) and the stages (`NextStage` 0x523666). See
//! `docs/simulation.md`.
//!
//! A placed building is paid at once, starts at stage 0 with 2 life and locks its
//! `BUILDLOCKPOINTS`. Every builder standing at a `BUILDPOINT` adds one stage per cycle of
//! its `#WORK` animation; life follows the stages. At `BUILDSTAGES` the building switches to
//! its `LOCKPOINTS` and works. Builders on a finished, damaged building repair 20 life per cycle.

use std::sync::Arc;

use crate::catalog::{ObjectType, TypeId};
use crate::economy::{base_cell, census, unit_cost};
use crate::fixed::Pos;
use crate::motion::{set_anim, try_to_stand};
use crate::path::{Mover, create_path};
use crate::trig::{get_dir, norma};
use crate::world::{AnimKind, EntityId, FRM_DEC, Order, World};

/// Construction animation (`#BUILDLO_k`, k = stage·4 / stages, at most 3).
fn stage_anim(stage: i32, max: i32) -> AnimKind {
    if stage >= max { AnimKind::Stand } else { AnimKind::BuildLo((stage * 4 / max.max(1)).clamp(0, 3) as u8) }
}

fn lock_cells(t: &ObjectType, pos: Pos, points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let (bx, by) = base_cell(t, pos);
    points.iter().map(|&(x, y)| (bx + x, by + y)).collect()
}

/// Whether `player` may place `kind` with these builders: the type is enabled and in the
/// `[FIXED_PRODUCE]` list (the build menu) of one of them. Without nation data anything
/// may be built.
pub fn can_build(w: &World, player: u8, builders: &[usize], kind: TypeId) -> bool {
    let tech = &w.catalog.tech;
    if !tech.loaded {
        return true;
    }
    w.players[usize::from(player)].enabled.get(usize::from(kind.0)).copied().unwrap_or(false)
        && builders.iter().any(|&i| tech.of(w.units[i].kind).is_some_and(|t| t.produce.contains(&kind)))
}

/// `CreateObject` for a building placed by a player at world pixel `(x, y)`: checks the
/// position and the price, pays, and creates the construction site. `None` if refused.
pub fn place(w: &mut World, player: u8, kind: TypeId, x: i32, y: i32) -> Option<EntityId> {
    let t = w.catalog.get(kind).clone();
    if !t.building {
        return None;
    }
    let (x16, y16) = (x << 4, y << 4);
    let limit = ((240 << w.map.shift) + 2) << 9;
    if x16 < 1024 || y16 < 1024 || x16 > limit || y16 > limit {
        return None;
    }
    w.random.rando();
    let cost = unit_cost(w, player, kind, Some((x, y)));
    if w.players[usize::from(player)].resources.iter().zip(&cost).any(|(s, c)| s < c) {
        return None;
    }
    w.random.rando();
    w.random.rando();
    // Buildings snap so that RealX + 16a and RealY + 32b fall on a 16 px cell corner.
    let (a, b) = (t.location.0 << 4, (2 * t.location.1) << 4);
    let pos = Pos::new(((x16 + a) & !255) - a, ((y16 + b) & !255) - b);
    // Placement check (0x513b2c, simplified): the finished footprint must be free of locks
    // and units.
    let cells = lock_cells(&t, pos, &t.lock_points);
    if cells.iter().any(|&(cx, cy)| w.map.land.check_pt(cx, cy) || w.occupancy.check_pt(cx, cy)) {
        return None;
    }
    w.random.rando();
    let stock = &mut w.players[usize::from(player)].resources;
    for (s, c) in stock.iter_mut().zip(&cost) {
        *s -= c;
    }
    let id = w.spawn(kind, player, pos, 0);
    let u = &mut w.units[id.0 as usize];
    u.stage = 0;
    u.life = 2;
    set_anim(u, &t, stage_anim(0, 1));
    let points = if t.economy.build_lock_points.is_empty() { &t.lock_points } else { &t.economy.build_lock_points };
    let map = Arc::make_mut(&mut w.map);
    for (cx, cy) in lock_cells(&t, pos, points) {
        map.land.set_pt(cx, cy);
    }
    census(w, player);
    Some(id)
}

/// `NextStage` (0x523666): one stage (or 20 life of repair) for a building. Life follows
/// the stages up to `MaxLife`; the stage count is the owner's (`BUILD` upgrades change it).
pub fn next_stage(w: &mut World, b: usize) {
    let t = w.catalog.get(w.units[b].kind).clone();
    let max = w.max_stage(&w.units[b]).max(1);
    let u = &mut w.units[b];
    if !u.building || u.life >= u.max_life && u.stage >= max {
        return;
    }
    if u.stage < max {
        u.stage += 1;
        u.life = u.stage * u.max_life / max;
        set_anim(u, &t, stage_anim(u.stage, max));
        u.frame = 0;
        if u.stage == max {
            let pos = u.pos;
            let map = Arc::make_mut(&mut w.map);
            for (cx, cy) in lock_cells(&t, pos, &t.economy.build_lock_points) {
                map.land.clr_pt(cx, cy);
            }
            for (cx, cy) in lock_cells(&t, pos, &t.lock_points) {
                map.land.set_pt(cx, cy);
            }
            let owner = w.units[b].owner;
            census(w, owner);
            // A finished mill plants its first product (fields) around it (0x5239ec).
            if t.economy.mill
                && let Some(&field) = w.catalog.tech.of(t_kind(w, b)).and_then(|tt| tt.produce.first())
            {
                let pos = w.units[b].pos;
                crate::sprite::plant_fields(w, owner, pos.x, pos.y, field);
            }
        }
    } else {
        u.life = (u.life + 20).min(u.max_life);
    }
}

fn t_kind(w: &World, b: usize) -> TypeId {
    w.units[b].kind
}

/// The free `BUILDPOINT` nearest to `from` (`FindPoint` 0x5233e4 with flags 7).
fn find_point(w: &World, b: usize, from: (i32, i32)) -> Option<(i32, i32)> {
    let bu = &w.units[b];
    let t = w.catalog.get(bu.kind);
    let (bx, by) = base_cell(t, bu.pos);
    t.economy
        .build_points
        .iter()
        .map(|&(x, y)| (bx + x, by + y))
        .filter(|&(cx, cy)| !w.map.land.check_bar(cx - 1, cy - 1, 3, 3))
        .min_by_key(|&(cx, cy)| norma(cx - from.0, cy - from.1))
}

fn needs_work(w: &World, b: usize, owner: u8) -> bool {
    let bu = &w.units[b];
    bu.living() && bu.building && bu.owner == owner && (bu.stage < w.max_stage(bu) || bu.life < bu.max_life)
}

/// `BuildObj` (0x523c98): sends a unit with a `#WORK` animation to build or repair.
pub fn build_obj(w: &mut World, i: usize, b: usize, prio: u8) -> bool {
    let u = &w.units[i];
    if !u.active() || crate::motion::anim_frames(w.catalog.get(u.kind), AnimKind::Work).is_none() {
        return false;
    }
    if !needs_work(w, b, u.owner) || prio < u.prio && !u.orders.is_empty() {
        return false;
    }
    let Some(point) = find_point(w, b, u.cell) else { return false };
    let target = w.units[b].id;
    let u = &mut w.units[i];
    u.orders.clear();
    u.orders.push(Order::Build { target, point, prio });
    u.prio = prio;
    u.path.delay = 0;
    true
}

/// `BuildObjLink` (0x524654), once per tick for a builder.
pub fn build_link(w: &mut World, i: usize, target: EntityId, point: (i32, i32), prio: u8) {
    w.units[i].prio = prio;
    if w.units[i].frame < w.units[i].anim_frames - FRM_DEC {
        return;
    }
    let b = target.0 as usize;
    let owner = w.units[i].owner;
    if w.units.get(b).is_none() || !needs_work(w, b, owner) {
        w.units[i].orders.pop();
        w.units[i].path.clear();
        return;
    }
    let catalog = w.catalog.clone();
    let t = catalog.get(w.units[i].kind);
    let u = &w.units[i];
    let d = norma(u.cell.0 - point.0, u.cell.1 - point.1);
    if d < 7 && w.map.land.check_pt(point.0, point.1) {
        match find_point(w, b, u.cell) {
            Some(p) => {
                if let Some(Order::Build { point, .. }) = w.units[i].orders.last_mut() {
                    *point = p;
                }
            }
            None => {
                w.units[i].orders.pop();
            }
        }
        return;
    }
    if d <= 1 {
        let bpos = w.units[b].pos;
        let u = &mut w.units[i];
        u.dest = None;
        u.path.clear();
        let dir = get_dir(bpos.x - u.pos.x, bpos.y - u.pos.y);
        let dd = dir.wrapping_sub(u.dir) as i8;
        if i32::from(dd).abs() < 16 {
            u.dir = dir;
            set_anim(u, t, AnimKind::Work);
            u.frame = 0;
            u.in_motion = false;
            next_stage(w, b);
        } else {
            u.dir = if dd > 0 { u.dir.wrapping_add(16) } else { u.dir.wrapping_sub(16) };
            try_to_stand(u, t, &mut w.random, false);
        }
        return;
    }
    let u = &mut w.units[i];
    let mover = Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
    if let Some((x, y)) = create_path(&w.map.land, mover, &mut u.path, &mut w.random, point.0, point.1, t.stop_distance)
    {
        u.dest = Some(Pos::new(x, y));
    }
}
