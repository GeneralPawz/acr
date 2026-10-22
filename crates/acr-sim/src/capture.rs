//! Capture (`TryCapture` 0x528868, from the last loop of `LongProcesses` 0x51175d): an
//! object that can be captured (`CAPTURE`: the European peasants, some buildings; and every
//! building site nobody works on) changes sides when an enemy stands close and no friend
//! is near. See `docs/simulation.md`.

use crate::catalog::usage;
use crate::trig::norma;
use crate::world::{EntityId, World};

/// The last loop of `LongProcesses` (0x51175d): every 32nd tick per object (`index & 31 ==
/// tmtmt & 31`), an object that is not dying nor inside, of a `CAPTURE` type or a building
/// site without orders (not `Ready`), tries to be captured.
pub fn run(w: &mut World) {
    let tm = ((w.tick.0 + 1) & 31) as usize;
    for i in (tm..w.units.len()).step_by(32) {
        let u = &w.units[i];
        if !u.active() {
            continue;
        }
        let site = u.building && u.stage < w.max_stage(u);
        if w.catalog.get(u.kind).role.capture || (site && u.orders.is_empty()) {
            try_capture(w, i);
        }
    }
}

/// What the capture rule (`World::capture_rule`) spares: peasants (1), also town centres and
/// mines (2), everything but artillery (3).
fn exempt(w: &World, t: &crate::catalog::ObjectType) -> bool {
    match w.capture_rule {
        1 => t.peasant,
        2 => t.peasant || matches!(t.role.usage, usage::CENTER | usage::MINE),
        3 => !t.role.artillery,
        _ => false,
    }
}

/// The 128 px cell of a linear index of the cell grid (the engine walks the index without
/// wrapping at the row ends).
fn cell(w: &World, idx: i32) -> Option<(i32, i32)> {
    let side = w.buckets.side();
    (idx >= 0 && idx < side * side).then(|| (idx % side, idx / side))
}

/// The first unit of a cell that can capture an object with alliance mask `mask`
/// (0x528072): an enemy, not `CAPTURE`, not `CANTCAPTURE`, not of nation 7.
fn capturer_in(w: &World, (cx, cy): (i32, i32), mask: u8) -> Option<usize> {
    w.buckets.get(cx, cy).iter().map(|&k| k as usize).find(|&k| {
        let o = &w.units[k];
        let r = &w.catalog.get(o.kind).role;
        o.nmask & mask == 0 && !r.capture && !r.cant_capture && o.owner != 7
    })
}

/// The first unit of a cell that defends an object with alliance mask `mask` (0x52817b):
/// a friend, not `CAPTURE`, not dying.
fn defender_in(w: &World, (cx, cy): (i32, i32), mask: u8) -> Option<usize> {
    w.buckets.get(cx, cy).iter().map(|&k| k as usize).find(|&k| {
        let o = &w.units[k];
        o.nmask & mask != 0 && !w.catalog.get(o.kind).role.capture && o.death == 0
    })
}

/// `TryCapture` (0x528868), no random numbers:
/// - The game's capture rule (`World::capture_rule`) exempts peasants (1), also town
///   centres and mines (2), or everything but artillery (3).
/// - In the 5 × 5 cells of 128 px around the object, each cell where a nation outside its
///   friends is present offers its first possible capturer ([`capturer_in`]); one within
///   250 px counts, and the last one counted takes the object.
/// - If one did, a non-peasant object's reload becomes 100, and in the 7 × 7 cells each cell
///   with a friend present offers its first defender ([`defender_in`]), whose presence (at
///   any distance) prevents the capture. An AI's artillery piece blows itself up instead
///   when outnumbered (1 defender against at least 1 capturer, 2 against more than 3, 3
///   against more than 7, 4 against more than 10).
/// - Undefended, the object changes sides ([`change_side`]); a peasant captured by an AI
///   whose victim nation plays at difficulty 3 or more dies instead.
pub fn try_capture(w: &mut World, i: usize) {
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    if exempt(w, t) {
        return;
    }
    let side = w.buckets.side();
    let cell0 = (u.pos.y >> 11) * side + (u.pos.x >> 11);
    let mask = u.nmask;
    let (mut found, mut attackers, mut taker) = (false, 0, None);
    let mut idx = cell0 - 2 - 2 * side;
    for _ in 0..5 {
        for _ in 0..5 {
            if let Some(c) = cell(w, idx)
                && w.buckets.presence(c.0, c.1) & !mask != 0
                && let Some(e) = capturer_in(w, c, mask)
            {
                let o = &w.units[e];
                if norma(o.pos.x - u.pos.x, o.pos.y - u.pos.y) < 0xFA0 {
                    found = true;
                    attackers += 1;
                    taker = Some(o.owner);
                }
            }
            idx += 1;
        }
        idx += side - 5;
    }
    if !found {
        return;
    }
    // Walls below a third of their life skip the defence; the data has no walls.
    if !t.peasant {
        let u = &mut w.units[i];
        (u.delay, u.max_delay) = (100, 100);
    }
    let u = &w.units[i];
    let mut defenders = 0;
    let mut idx = cell0 - 3 - 3 * side;
    for _ in 0..7 {
        for _ in 0..7 {
            if let Some(c) = cell(w, idx)
                && w.buckets.presence(c.0, c.1) & mask != 0
                && defender_in(w, c, mask).is_some()
            {
                found = false;
                defenders += 1;
            }
            idx += 1;
        }
        idx += side - 7;
    }
    if w.ai[usize::from(u.owner) & 7] && t.role.artillery {
        let (a, d) = (attackers, defenders);
        if (d != 0 && a >= d && d == 1) || (a > 3 && d == 2) || (a > 7 && d == 3) || (a > 10 && d == 4) {
            crate::combat::die(w, i);
            return;
        }
    }
    let Some(new) = taker.filter(|_| found) else { return };
    let old = usize::from(w.units[i].owner) & 7;
    if w.ai[usize::from(new) & 7] && t.role.usage == usage::PEASANT && w.difficulty[old] >= 3 {
        crate::combat::destruct_building(w, i);
        return;
    }
    change_side(w, i, new);
    let inside = w.units[i].inside.clone();
    if inside.is_empty() {
        return;
    }
    if w.capture_rule == 0 {
        // The garrison goes over too, its orders cleared (0x52922a..0x52949a).
        for e in inside {
            change_side(w, e.0 as usize, new);
            clear_orders(w, e.0 as usize);
        }
    } else {
        crate::combat::destruct_building(w, i);
    }
}

/// `ClearOrders` (0x4efad1).
fn clear_orders(w: &mut World, i: usize) {
    let u = &mut w.units[i];
    u.orders.clear();
    u.path.clear();
    u.prio = 0;
}

/// The change of sides (0x528ea5..0x529206): a peasant (`USAGE PEASANT`) loses its orders,
/// the object leaves its formation, takes the new nation and its alliance mask, and a
/// building keeps its share of construction (`stage · new BUILDSTAGES / old`); it is noted
/// in `World::captured` (the scripts flip its serial). The score moves `VES·5` from the old
/// owner to the new one (the interface's statistics follow the owner).
pub fn change_side(w: &mut World, i: usize, new: u8) {
    if w.catalog.get(w.units[i].kind).role.usage == usage::PEASANT {
        clear_orders(w, i);
    }
    crate::formation::remove_member(w, i);
    let old_stages = w.max_stage(&w.units[i]);
    let u = &mut w.units[i];
    u.owner = new;
    u.nmask = w.nation_mask[usize::from(new) & 7];
    if u.building && old_stages > 0 {
        let new_stages = w.max_stage(&w.units[i]);
        let u = &mut w.units[i];
        u.stage = u.stage * new_stages / old_stages;
    }
    let id: EntityId = w.units[i].id;
    w.captured.push(id);
}

/// Whether an object could be captured now (0x52853e, 0 there): a `CAPTURE` type, or one
/// that is not finished and has no order, that the capture rule does not exempt and with
/// no defender ([`defender_in`]) in the 7 × 7 cells of 128 px around it. A unit with no
/// attack that fits its target walks there only then (`AttackObjLink` 0x51f3df).
pub fn capturable_now(w: &World, j: usize) -> bool {
    let u = &w.units[j];
    let t = w.catalog.get(u.kind);
    let ready = !u.building || u.stage >= w.max_stage(u);
    if !t.role.capture && (ready || !u.orders.is_empty()) {
        return false;
    }
    if exempt(w, t) {
        return false;
    }
    let side = w.buckets.side();
    let cell0 = (u.pos.y >> 11) * side + (u.pos.x >> 11);
    let mask = u.nmask;
    let mut idx = cell0 - 3 - 3 * side;
    for _ in 0..7 {
        for _ in 0..7 {
            if let Some(c) = cell(w, idx)
                && w.buckets.presence(c.0, c.1) & mask != 0
                && defender_in(w, c, mask).is_some()
            {
                return false;
            }
            idx += 1;
        }
        idx += side - 7;
    }
    true
}
