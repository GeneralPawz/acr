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

/// The lock cells the finished building of `kind` at `pos` occupies.
pub fn lock_footprint(w: &World, kind: TypeId, pos: Pos) -> Vec<(i32, i32)> {
    let t = w.catalog.get(kind);
    lock_cells(t, pos, &t.lock_points)
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

/// The map area a building may be placed in (world pixels).
fn on_map(w: &World, x: i32, y: i32) -> bool {
    let (x16, y16) = (x << 4, y << 4);
    let limit = ((240 << w.map.shift) + 2) << 9;
    x16 >= 1024 && y16 >= 1024 && x16 <= limit && y16 <= limit
}

/// Buildings snap so that `RealX + 16a` and `RealY + 32b` fall on a 16 px cell corner.
fn snap(t: &ObjectType, x: i32, y: i32) -> Pos {
    let (a, b) = (t.location.0 << 4, (2 * t.location.1) << 4);
    Pos::new((((x << 4) + a) & !255) - a, (((y << 4) + b) & !255) - b)
}

/// Placement check (0x513b2c, simplified): the finished footprint must be free of locks, and
/// no enemy object may stand within `(BRadius + 15)·16 px` of the check-point centre
/// (`CheckMonstersInArea` 0x5133bc tests only units not allied with the player); the
/// player's own units are moved out when the site is created.
fn footprint_free(w: &World, player: u8, t: &ObjectType, pos: Pos) -> bool {
    let cells = lock_cells(t, pos, &t.lock_points);
    if cells.iter().any(|&(cx, cy)| w.map.land.check_pt(cx, cy)) {
        return false;
    }
    let (bx, by) = base_cell(t, pos);
    let (cmx, cmy, br) = t.check_center;
    let (xc, yc, r) = (((bx + cmx) << 8) + 128, ((by + cmy) << 8) + 128, (br + 15) << 8);
    let mask = w.nation_mask[usize::from(player) & 7];
    !w.units.iter().any(|u| u.living() && u.nmask & mask == 0 && u.owner != 7 && norma(u.pos.x - xc, u.pos.y - yc) < r)
}

/// Where a building of `kind` placed at world pixel `(x, y)` would stand, if the position
/// passes [`place`]'s checks (the price aside). For the placement preview.
pub fn site(w: &World, kind: TypeId, x: i32, y: i32) -> Option<Pos> {
    let t = w.catalog.get(kind);
    if !t.building || !on_map(w, x, y) {
        return None;
    }
    let pos = snap(t, x, y);
    (!lock_cells(t, pos, &t.lock_points).iter().any(|&(cx, cy)| w.map.land.check_pt(cx, cy))).then_some(pos)
}

/// `RESSUBST` (CreateObject 0x515b62): a mine placed within 64 px of a deposit of another
/// resource becomes the mine of that resource — the types follow the gold mine in every
/// nation: iron +1, coal +2, stone +3, food +4, wood +5. The first deposit near enough
/// decides; the price paid stays the original's.
fn res_subst(w: &World, t: &crate::catalog::ObjectType, kind: TypeId, pos: Pos) -> TypeId {
    if !t.res_subst {
        return kind;
    }
    let (px, py) = pos.pixels();
    let mask = t.economy.producer.map_or(0, |p| p.mask);
    for d in &w.deposits {
        if d.resource >= 8 || crate::trig::norma(d.x - px, d.y - py) >= 64 {
            continue;
        }
        if u32::from(mask) == 1 << d.resource {
            return kind;
        }
        let off = match d.resource {
            crate::economy::IRON => 1,
            crate::economy::COAL => 2,
            crate::economy::STONE => 3,
            crate::economy::FOOD => 4,
            crate::economy::WOOD => 5,
            _ => 0,
        };
        let k = TypeId(kind.0 + off);
        return if usize::from(k.0) < w.catalog.len() { k } else { kind };
    }
    kind
}

/// `CreateObject` for a building placed by a player at world pixel `(x, y)`: checks the
/// position and the price, pays, and creates the construction site. `None` if refused.
pub fn place(w: &mut World, player: u8, kind: TypeId, x: i32, y: i32) -> Option<EntityId> {
    let t = w.catalog.get(kind).clone();
    if !t.building {
        return None;
    }
    if !on_map(w, x, y) {
        return None;
    }
    w.random.rando();
    let cost = unit_cost(w, player, kind, Some((x, y)));
    if w.players[usize::from(player)].resources.iter().zip(&cost).any(|(s, c)| s < c) {
        return None;
    }
    w.random.rando();
    w.random.rando();
    let pos = snap(&t, x, y);
    if !footprint_free(w, player, &t, pos) {
        return None;
    }
    w.random.rando();
    let stock = &mut w.players[usize::from(player)].resources;
    for (s, c) in stock.iter_mut().zip(&cost) {
        *s -= c;
    }
    let kind = res_subst(w, &t, kind, pos);
    Some(found_site(w, player, kind, &t, pos))
}

/// The editor's placing (no price, no builders): `kind` at world pixel `(x, y)` under the
/// checks of [`place`], finished with its `LOCKPOINTS` (`stage` = `None`) or as a site
/// advanced to that construction stage. `None` if the position is refused.
pub fn place_free(w: &mut World, player: u8, kind: TypeId, x: i32, y: i32, stage: Option<i32>) -> Option<EntityId> {
    let t = w.catalog.get(kind).clone();
    if !t.building || !on_map(w, x, y) {
        return None;
    }
    let pos = snap(&t, x, y);
    if !footprint_free(w, player, &t, pos) {
        return None;
    }
    let id = found_site(w, player, kind, &t, pos);
    let max = w.max_stage(&w.units[id.0 as usize]).max(1);
    for _ in 0..stage.map_or(max, |s| s.clamp(0, max)) {
        next_stage(w, id.0 as usize);
    }
    Some(id)
}

/// Creates the construction site of `kind` at `pos`: stage 0, 2 life, its `BUILDLOCKPOINTS`.
fn found_site(w: &mut World, player: u8, kind: TypeId, t: &ObjectType, pos: Pos) -> EntityId {
    let id = w.spawn(kind, player, pos, 0);
    let u = &mut w.units[id.0 as usize];
    u.stage = 0;
    u.life = 2;
    set_anim(u, t, stage_anim(0, 1));
    let points = if t.economy.build_lock_points.is_empty() { &t.lock_points } else { &t.economy.build_lock_points };
    let map = Arc::make_mut(&mut w.map);
    for (cx, cy) in lock_cells(t, pos, points) {
        map.land.set_pt(cx, cy);
    }
    if !t.economy.build_lock_points.is_empty() {
        // 0x51692c: units standing on the new locks are moved out.
        let (bx, by) = base_cell(t, pos);
        let (cmx, cmy, br) = t.check_center;
        push_out(w, ((bx + cmx) << 8) | 0x80, ((by + cmy) << 8) | 0x80, (br + 2) << 8);
    }
    // RenewInfoMap (0x43f763) for the AI planning maps.
    let (px, py) = pos.pixels();
    crate::aiplace::renew(w, px, py);
    census(w, player);
    id
}

/// The land lock cells building `i` holds now: a construction site its `BUILDLOCKPOINTS`
/// (the `LOCKPOINTS` when it has none), a finished building its `LOCKPOINTS` and the
/// `STAGELOCKPOINTS` of its done sections (as `upgrade::upstage` sets them).
pub(crate) fn current_locks(w: &World, i: usize) -> Vec<(i32, i32)> {
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    if u.stage < w.max_stage(u) {
        let points = if t.economy.build_lock_points.is_empty() { &t.lock_points } else { &t.economy.build_lock_points };
        return lock_cells(t, u.pos, points);
    }
    let mut cells = lock_cells(t, u.pos, &t.lock_points);
    for &(k, _) in &t.economy.sections {
        if crate::upgrade::section_state(u.stages, k) == crate::upgrade::section::DONE {
            cells.extend(lock_cells(t, u.pos, &t.economy.stage_lock_points[usize::from(k)]));
        }
    }
    cells
}

/// 0x51374e: every unit within `r` of `(x, y)` (1/16 px) whose 3 × 3 lock cells around its
/// cell are not free jumps to the centre of the first cell on the rings 0..59 around it with
/// a free 3 × 3 box; its destination and path are dropped, its orders stay.
pub(crate) fn push_out(w: &mut World, x: i32, y: i32, r: i32) {
    let land = w.map.land.clone();
    for i in 0..w.units.len() {
        let u = &w.units[i];
        if !u.living() || u.building || u.inside_of.is_some() || crate::trig::norma(u.pos.x - x, u.pos.y - y) >= r {
            continue;
        }
        let (cx, cy) = u.cell;
        if !land.check_bar(cx - 1, cy - 1, 3, 3) {
            continue;
        }
        let spot = (0..60).find_map(|k| {
            crate::morale::ring(k)
                .iter()
                .map(|&(dx, dy)| (cx + dx, cy + dy))
                .find(|&(nx, ny)| !land.check_bar(nx - 1, ny - 1, 3, 3))
        });
        let Some((nx, ny)) = spot else { continue };
        let World { units, occupancy, .. } = &mut *w;
        let u = &mut units[i];
        if !u.unlimited {
            occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
            occupancy.set_bar(nx, ny, u.lx);
        }
        u.pos = Pos::new((nx << 8) + 0x80, (ny << 8) + 0x80);
        u.cell = (nx, ny);
        u.dest = None;
        u.path.clear();
    }
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
            // The builder's link plays the finished building's `BORN_SOUND` (0x5247c2).
            w.born.push(w.units[b].id);
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
        if u.life >= u.max_life {
            // A full repair puts a fire out (0x523b63).
            (u.burning, u.fire) = (false, 0);
        }
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

/// Whether building `b` of `owner` is unfinished or damaged (builders have work there).
pub fn needs_work(w: &World, b: usize, owner: u8) -> bool {
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
        w.units[i].pop_order();
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
                w.units[i].pop_order();
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
    let mover = Mover::of(u, t);
    if let Some((x, y)) = create_path(&w.map.land, mover, &mut u.path, &mut w.random, point.0, point.1, t.stop_distance)
    {
        u.dest = Some(Pos::new(x, y));
    }
}
