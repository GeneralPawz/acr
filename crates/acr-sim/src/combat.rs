//! Land combat as the engine does it: auto-targeting (`SearchVictim` 0x52622b), attack
//! orders (`AttackObj` 0x51a930, `AttackObjLink` 0x51bcaf), damage (`MakeDamage`
//! 0x4e8990), musket shots as instant line traces (`TraceObjectsInLine` 0x559310), reload
//! and death (object loop 0x5118b3). See `docs/simulation.md`.
//!
//! Attack index `i` is stance `i + 1`; its animation is `#ATTACKi`. A strike starts at the
//! end of an animation and hits when the animation reaches its active frame
//! (`SETACTIVEPOINT`). `ATTACK_PAUSE` counts animation frames: two per tick, or, for
//! `SLOWRECHARGE` units, the frames of each `#ATTACK3` reload animation played.

use acr_formats::defs::nres::weapon_flags::{FIRE, FIRE_SLOW, FORMATION, GARRISON, HIGH, RANDOM_KILL};

use crate::catalog::Adv;
use crate::fixed::Pos;
use crate::motion::{anim_frames, set_anim, try_to_stand};
use crate::path::{Mover, create_path};
use crate::trig::{cos, get_dir, norma, sin};
use crate::world::{AnimKind, EntityId, FRM_DEC, Order, Unit, World};

/// Priority of automatic attacks (SearchVictim) and counter-attacks.
const AUTO_PRIO: u8 = 1;
/// Priority of the victim of a melee strike hitting back.
const STRUCK_PRIO: u8 = 2;
/// A corpse is removed when its death counter passes this.
const CORPSE_LIFETIME: u16 = 2000;

fn height(w: &World, p: Pos) -> i32 {
    w.map.heights.at(p.x >> 4, p.y >> 4)
}

/// `ADC.MinR` and `ADC.MaxR` (0x43b95a): the smallest minimum and largest maximum attack
/// range of the indices that can attack; both 0 if the maximum is below the minimum.
pub fn range_bounds(t: &Adv) -> (i32, i32) {
    let usable = t.attacks.iter().filter(|a| a.usable());
    let min = usable.clone().map(|a| a.radius.0).min().unwrap_or(0);
    let max = usable.map(|a| a.radius.1).max().unwrap_or(0);
    if max < min { (0, 0) } else { (min, max) }
}

/// `ADC.MinR`.
fn min_range(t: &Adv) -> i32 {
    range_bounds(t).0
}

fn is_high(w: &World, t: &Adv, i: usize) -> bool {
    w.catalog.weapon_flags(&t.attacks[i].kind) & HIGH != 0
}

/// `AttackObj` (0x51a930): gives unit `i` an attack order on unit `j`; false if refused.
pub fn attack_obj(w: &mut World, i: usize, j: usize, prio: u8) -> bool {
    attack_obj_ex(w, i, j, prio, Placement::Replace)
}

/// Where a new order goes (`CreateOrder`'s order type): replacing the orders, in front of
/// them (the unit resumes them afterwards), or after them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Replace,
    Front,
    Append,
}

impl Placement {
    /// The engine's order type `0`, `1`, `2`.
    pub fn from_type(t: u8) -> Self {
        match t & 7 {
            1 => Self::Front,
            2 => Self::Append,
            _ => Self::Replace,
        }
    }

    /// Puts `order` into `orders` (the last order runs).
    pub fn push(self, orders: &mut Vec<Order>, order: Order) {
        match self {
            Placement::Replace => {
                orders.clear();
                orders.push(order);
            }
            Placement::Front => orders.push(order),
            Placement::Append => orders.insert(0, order),
        }
    }
}

/// `AttackObj` with the order placement: `front` puts the attack in front of the current
/// orders (order type 1), so the unit resumes them after the kill; otherwise it replaces them.
///
/// The gate, in the engine's order: priority 0xFE becomes 0x90 and ends stand ground; a
/// member of a close-combat formation starts formation melee instead (unless 0x8F); with
/// `NoSearchVictim` an attack below 0x80 is accepted without effect, and one of 0x80 or more
/// clears the flag; an attack below the unit's current priority is refused; below 5 the unit
/// only switches to a strictly closer enemy. The order keeps `prio & 0x7f`; a unit on a
/// patrol-like move (`InPatrol`) gets the attack in front.
pub fn attack_obj_ex(w: &mut World, i: usize, j: usize, prio: u8, placement: Placement) -> bool {
    let (Some(u), Some(e)) = (w.units.get(i), w.units.get(j)) else { return false };
    if i == j || !u.active() || !e.active() || !u.hostile(e) {
        return false;
    }
    let mut prio = prio;
    let mut placement = placement;
    if u.building {
        // Buildings (0x51a930): finished, not researching; a human nation's building that
        // trains units does not shoot; a garrison building needs a weapon; a building not
        // attacking yet gets priority 0x81, and the attack goes in front of its orders.
        let bt = w.catalog.get(u.kind);
        if u.stage < w.max_stage(u) || u.research.is_some() || (!w.ai[usize::from(u.owner) & 7] && !u.queue.is_empty())
        {
            return false;
        }
        let adv = w.adv(u.owner, u.kind);
        if adv.max_inside != 0 && bt.attacks[0].weapon_id.is_none() && bt.attacks[0].weapon.is_none() {
            return false;
        }
        if u.target().is_none() {
            prio = 0x81;
        }
        if !bt.shot_points.is_empty() {
            placement = Placement::Front;
        }
    }
    let u = &w.units[i];
    let t = w.adv(u.owner, u.kind);
    let materials = w.catalog.get(e.kind).materials;
    // CheckAttAbility: some index can attack this material.
    if !t.attacks.iter().any(|a| a.usable() && a.mask & materials != 0) {
        return false;
    }
    // CreateOrder (0x525752) refuses units walking with UnlimitedMotion, except a panicking
    // unit between two flight legs (0x51ae76).
    if u.unlimited && !matches!(u.orders.last(), Some(Order::Panic { .. })) {
        return false;
    }
    let raw = prio;
    let prio = if raw == 0xFE { 0x90 } else { raw };
    if raw == 0xFE {
        w.units[i].stand_ground = false;
    }
    let (u, e) = (&w.units[i], &w.units[j]);
    let ut = w.catalog.get(u.kind);
    if let Some(b) = u.brigade.map(usize::from)
        && prio != 0x8F
        && !ut.role.artillery
        && !e.building
        && let Some(br) = w.brigades.get(b)
    {
        if !br.shooters {
            if raw == 0xFE {
                return true;
            }
            match br.orders.last() {
                Some(crate::formation::BOrder::Bitva(_)) => {}
                Some(_) => {
                    if br.att_enm && norma(u.pos.x - e.pos.x, u.pos.y - e.pos.y) < 0x640 {
                        crate::formation::bitva(w, b);
                    }
                    return true;
                }
                None => {
                    crate::formation::bitva(w, b);
                    return true;
                }
            }
        } else if br.orders.last().is_some_and(|o| o.prio() & 0x7f != 0) {
            return true;
        }
    }
    let u = &w.units[i];
    if u.no_search_victim {
        if prio < 0x80 {
            return true;
        }
        w.units[i].no_search_victim = false;
    }
    let (u, e) = (&w.units[i], &w.units[j]);
    if prio < u.prio && !u.orders.is_empty() {
        return false;
    }
    if prio < 5
        && let Some(cur) = u.target().and_then(|c| w.units.get(c.0 as usize))
    {
        let d1 = norma(e.pos.x - u.pos.x, e.pos.y - u.pos.y);
        let d2 = norma(cur.pos.x - u.pos.x, cur.pos.y - u.pos.y);
        // The engine compares the 1/16 px distance with MinR in pixels (quirk kept).
        if d1 <= min_range(&t) || d2 <= d1 {
            return false;
        }
    }
    let target = e.id;
    let u = &mut w.units[i];
    u.stand_time = 0;
    let prio = prio & 0x7f;
    if let Some(Order::Attack { target: t, prio: p }) = u.orders.last_mut() {
        *t = target;
        *p = prio;
    } else {
        if u.in_patrol && placement == Placement::Replace {
            placement = Placement::Front;
        }
        let was_empty = u.orders.is_empty();
        placement.push(&mut u.orders, Order::Attack { target, prio });
        if was_empty || placement != Placement::Append {
            u.path.delay = 0;
        }
    }
    u.prio = prio;
    true
}

/// `AttackObjLink`, building branch (0x51c30a), once per tick for a building with an attack
/// order: with its reload over and a garrison inside, the first free shot point whose range
/// and sector hold the target fires its attack index's weapon (one shot per tick); the
/// reload becomes `ATTACK_PAUSE[0]·PersonalDelay/(inside·100)` and the point's own reload
/// starts. A unit target that no free point covers is dropped.
pub fn building_attack_link(w: &mut World, i: usize, target: EntityId, prio: u8) {
    let catalog = w.catalog.clone();
    let j = target.0 as usize;
    w.units[i].prio = prio;
    let valid = w.units.get(j).is_some_and(|e| e.alive && e.death <= 8 && w.units[i].hostile(e));
    if !valid {
        w.units[i].pop_order();
        return;
    }
    if w.units[i].delay != 0 {
        return;
    }
    let u = &w.units[i];
    let t = catalog.get(u.kind);
    if t.shot_points.is_empty() || u.inside.is_empty() {
        w.units[i].pop_order();
        return;
    }
    let first = u.inside.first().and_then(|e| w.units.get(e.0 as usize));
    let mut evro = first.is_some_and(|f| catalog.get(f.kind).evro_shot != 0);
    let adv = w.adv(u.owner, u.kind);
    let (min_r, max_r) = range_bounds(&adv);
    let e = &w.units[j];
    let dist = norma(u.pos.x - e.pos.x, u.pos.y - e.pos.y) >> 4;
    if dist <= min_r || dist > max_r {
        w.units[i].pop_order();
        return;
    }
    let ev = t.evro_shot;
    if ev == 100 {
        evro = false;
    }
    let bx = ((u.pos.x + (t.location.0 << 4)) >> 8) << 4;
    let by = ((u.pos.y + ((2 * t.location.1) << 4)) >> 8) << 4;
    let rz = height(w, u.pos);
    let (ex, ey) = e.pos.pixels();
    let target_building = e.building;
    let mut keep = false;
    for (k, sp) in t.shot_points.iter().enumerate() {
        let (x, y, z) = (bx + sp.x, by + 2 * (sp.y + sp.z), sp.z + rz);
        let extra = sp.extra - sp.y - sp.z;
        let d = norma(x - ex, y - ey);
        let ad = i32::from((get_dir(ex - x, ey - y).wrapping_sub(sp.dir) as i8).unsigned_abs());
        let in_sector = sp.r_min <= d && d <= sp.r_max && ad < sp.sector;
        if w.units[i].shot_timers.get(k).copied().unwrap_or(0) != 0 {
            keep |= target_building && in_sector;
            continue;
        }
        if !in_sector {
            continue;
        }
        let a = i32::from(sp.att);
        if !((!evro && a != ev) || (evro && a == ev)) {
            continue;
        }
        let Some(wid) = t.attacks.get(usize::from(sp.att)).and_then(|at| at.weapon_id) else {
            w.units[i].pop_order();
            return;
        };
        let dest_obj = target_building.then_some(j);
        let c = damage_point(w, j);
        let mut dispersed = c;
        razbros(w, i, &mut dispersed); // drawn, but the engine aims at the undispersed point
        crate::projectile::create(w, wid, (x, y, z), c, Some(i), sp.att, dest_obj, (0, 0, 0), extra);
        let u = &mut w.units[i];
        let n = u.inside.len() as i32;
        u.delay = adv.attacks[0].pause * u.personal_delay / (n * 100);
        u.max_delay = u.delay;
        if let Some(tm) = u.shot_timers.get_mut(k) {
            *tm = sp.reload;
        }
        return;
    }
    if !keep {
        w.units[i].pop_order();
    }
}

/// What an attack aims at: an object, or the ground (`NewAttackPoint`, a zeroed stand-in
/// object at the point, link 0x51bc5b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    Unit(usize),
    /// A point in 1/16 px, and the shots left (0: unlimited).
    Point(Pos, u16),
}

/// `AttackObjLink` (0x51bcaf), once per tick for a unit with an attack order.
pub fn attack_link(w: &mut World, i: usize, target: EntityId, prio: u8) {
    attack_link_aim(w, i, Aim::Unit(target.0 as usize), prio);
}

/// `AttackObjLink` on an object or (for an attack-point order) on the ground: at the active
/// frame of the strike the unit hits or shoots; at the end of an animation it picks the last
/// attack index whose detection range fits, turns, changes stance and starts the strike, or
/// walks towards the target (in the index's walking stance where the type has one). An
/// attack on the ground reaches `ADDRANGE` % as far and reloads `ADDPAUSE` % as long.
pub fn attack_link_aim(w: &mut World, i: usize, aim: Aim, prio: u8) {
    let catalog = w.catalog.clone();
    {
        let u = &mut w.units[i];
        u.prio = prio;
        u.dest = None;
    }
    let point = matches!(aim, Aim::Point(..));
    if let Aim::Unit(j) = aim {
        if hunt(w, i, j) {
            return;
        }
        let valid = w.units.get(j).is_some_and(|e| e.alive && e.death <= 8 && w.units[i].hostile(e));
        if !valid {
            w.units[i].pop_order();
            return;
        }
    }
    // The target's position, aim height, extra range and materials (a point: none, 0xFF).
    let (e_pos, e_dstz, e_add, e_mat, e_bld) = match aim {
        Aim::Unit(j) => {
            let e = &w.units[j];
            let et = catalog.get(e.kind);
            (e.pos, et.zpoints.1, et.add_shot_radius, et.materials, e.building)
        }
        Aim::Point(p, _) => (p, 0, 0, 0xFF, false),
    };
    let (add_range, add_pause) = if point { (catalog.add_range, catalog.add_pause) } else { (100, 100) };
    let u = &w.units[i];
    let t = catalog.get(u.kind);
    let ta = w.adv(u.owner, u.kind);
    let ns = usize::from(u.stance);
    let mut delayed = u.delay != 0;
    let mut can_set_delay = true;
    if delayed {
        if ns > 0 && ta.attacks[ns - 1].no_pause {
            delayed = false;
        }
        can_set_delay = false;
    }
    let need_slow = delayed && t.slow_recharge;
    let (dx, dy) = (e_pos.x - u.pos.x, e_pos.y - u.pos.y);
    let dst = norma(dx, dy) >> 4;
    let my_z = height(w, u.pos) + t.zpoints.0;
    let en_z = height(w, e_pos) + e_dstz;
    let en_dir = get_dir(dx, dy);
    let high_ns = ns > 0 && is_high(w, &ta, ns - 1);
    let (u_frame, u_frames, u_anim) = (u.frame, u.anim_frames, u.anim);

    // (A) The strike animation reached its active frame: hit or shoot.
    if ns > 0 && !delayed {
        let kind = AnimKind::Attack((ns - 1) as u8);
        if let Some(info) = t.anims.get(&kind.name()) {
            let mut af = i32::from(info.active).max(1);
            if FRM_DEC == 2 && af & 1 == 1 {
                af += 1;
            }
            if u_anim == kind && u_frame == af {
                let a = &ta.attacks[ns - 1];
                let mut dst1 = dst - e_add;
                if high_ns {
                    dst1 -= 2 * (my_z - en_z);
                }
                let dr2 = (a.radius.1 + a.add_radius) * add_range / 100;
                let in_range = dst >= a.radius.0 && dst1 <= dr2;
                let (damage, weapon) = (a.damage, a.weapon.is_some());
                let pause = a.pause;
                let u = &mut w.units[i];
                u.dir = en_dir;
                if in_range {
                    if can_set_delay {
                        // delay = MaxDelay = AttPause·PersonalDelay/100·DPAUS/100 (0x51d7e9).
                        u.delay = pause * u.personal_delay / 100 * add_pause / 100;
                        u.max_delay = u.delay;
                    }
                    if weapon {
                        shoot(w, i, aim, ns - 1, my_z, en_z);
                        if t.sector_weapon.is_some_and(|s| usize::from(s.att) == ns - 1) {
                            crate::artillery::sector_damage(w, i);
                        }
                    } else if let Aim::Unit(j) = aim {
                        make_damage(w, j, damage, Some(i), (ns - 1) as u8);
                        if w.units[j].living() {
                            attack_obj(w, j, i, STRUCK_PRIO);
                        }
                    }
                    // NTimes: the shots of an attack on the ground.
                    if let Some(Order::AttackPoint { times, .. }) = w.units[i].orders.last_mut()
                        && *times != 0
                    {
                        *times -= 1;
                        if *times == 0 {
                            w.units[i].pop_order();
                        }
                    }
                }
                return;
            }
        }
    }
    // (B) Otherwise act only at the end of the current animation.
    if u_frame < u_frames - FRM_DEC {
        return;
    }
    if delayed && ns > 0 && u_anim == AnimKind::Attack((ns - 1) as u8) {
        let u = &mut w.units[i];
        if t.slow_recharge && (ns == 4 || high_ns) {
            u.stance = 4;
            u.local_stance = 4;
        } else {
            try_to_stand(u, t, &mut w.random, false);
        }
    }
    // (C) Choose the attack index: the last one whose detection range fits.
    let u = &w.units[i];
    let dstx = dst - 2 * (my_z - en_z);
    let fits = |k: usize| {
        let a = &ta.attacks[k];
        let ok = if is_high(w, &ta, k) {
            dst >= a.detect.0 && dstx <= a.detect.1
        } else {
            a.detect.0 <= dst && dst <= a.detect.1
        };
        ok && a.mask & e_mat != 0
    };
    let mut chosen = (0..ta.attacks.len()).rev().find(|&k| fits(k));
    if u.melee_only && !e_bld {
        // Melee only (OB+0x2a bit2, 0x51bdc6): always the first index; not against buildings.
        chosen = Some(0);
    }
    if u.delay != 0
        && let Some(k) = (0..ta.attacks.len()).rev().find(|&k| ta.attacks[k].no_pause && fits(k))
    {
        chosen = Some(k);
    }
    let Some(k) = chosen else {
        let (ex, ey) = e_pos.pixels();
        let areas = w.map.land_areas.clone();
        let u = &mut w.units[i];
        u.pop_order();
        crate::motion::smart_send_to(u, areas.as_ref(), ex, ey, 0, 0, prio);
        return;
    };
    let high = is_high(w, &ta, k);
    let dst_c = if high { dstx } else { dst };
    if high && need_slow {
        // Musket reload while the order lasts: play #ATTACK3 cycles, each taking its frames
        // off the reload counter.
        let reload = AnimKind::Attack(3);
        let frames = anim_frames(t, reload).unwrap_or(1);
        let u = &mut w.units[i];
        if u.local_stance != 4 {
            u.stance = 4;
            return;
        }
        if u.anim != reload || u.delay > frames {
            u.delay = (u.delay - frames).max(0);
            set_anim(u, t, reload);
            u.frame = 0;
            u.in_motion = false;
            return;
        }
        u.delay = 0;
    }
    // (D) In range: face the enemy and strike; otherwise walk towards it.
    let a = &ta.attacks[k];
    let add = a.radius.1 * (add_range - 100) / 100 + e_add;
    let amove = anim_frames(t, AnimKind::MotionLs(k as u8)).is_some();
    let complex = t.motion_style == crate::catalog::MotionStyle::ComplexRotate;
    let u = &mut w.units[i];
    if dst >= a.radius.0 && dst_c < a.radius.1 + add {
        let mut ddir = en_dir.wrapping_sub(u.dir) as i8;
        let mrot = t.rotate;
        let mrot1 = (mrot + mrot / 2).min(16);
        if complex {
            // A cannon turns in 16ths with its turn animations (0x51f75e). The engine passes
            // the target instead of the cannon to TryToStand here (a random number, kept).
            let q = |d: i32| ((d + 0x208) >> 4) & 15;
            if q(i32::from(en_dir as i8)) == q(i32::from(u.dir)) {
                ddir = 0;
                if u.stance == 2 {
                    (u.stance, u.ground_state) = (0, 0);
                    try_to_stand(u, t, &mut w.random, false);
                    return;
                }
            } else {
                if u.local_stance == 2 {
                    if u.anim == AnimKind::MotionLs(1) {
                        u.dir = u.dir.wrapping_add(16);
                        stand_target(w, aim);
                        return;
                    }
                    if u.anim == AnimKind::MotionRs(1) {
                        stand_target(w, aim);
                        return;
                    }
                    if ddir > 0 {
                        set_anim(u, t, AnimKind::MotionLs(1));
                    } else {
                        set_anim(u, t, AnimKind::MotionRs(1));
                        u.dir = u.dir.wrapping_sub(16);
                    }
                    u.frame = 0;
                    u.in_motion = false;
                } else {
                    (u.stance, u.ground_state) = (2, 2);
                    stand_target(w, aim);
                }
                return;
            }
        }
        if i32::from(ddir).abs() < mrot1 {
            u.dir = en_dir;
            if usize::from(u.local_stance) == k + 1 {
                if !delayed || ta.attacks[k].no_pause {
                    // RASTRATA_NA_VISTREL: shots of these indices cost resources (0x51f9c4).
                    if a.weapon.is_some()
                        && let Some((i1, i2, cost)) = t.shot_cost
                        && (usize::from(i1) == k || usize::from(i2) == k)
                    {
                        let pl = &mut w.players[usize::from(u.owner) & 7];
                        if pl.resources.iter().zip(&cost).any(|(r, c)| r < c) {
                            u.pop_order();
                            return;
                        }
                        for (r, c) in pl.resources.iter_mut().zip(&cost) {
                            *r -= c;
                        }
                    }
                    let kind = AnimKind::Attack(k as u8);
                    set_anim(u, t, kind);
                    u.frame = 0;
                    u.in_motion = false;
                }
            } else {
                u.stance = (k + 1) as u8;
            }
        } else {
            if !amove && i32::from(ddir).abs() > 36 && !complex {
                u.stance = u.ground_state;
                try_to_stand(u, t, &mut w.random, false);
            }
            let r = mrot as u8;
            u.dir = if ddir > 0 { u.dir.wrapping_add(r) } else { u.dir.wrapping_sub(r) };
        }
    } else if u.stand_ground {
        // Standing ground (0x5204aa): a target out of range is dropped, never chased.
        u.pop_order();
    } else if dst < a.radius.0 {
        let need = a.radius.0 - dst;
        too_close(w, i, e_pos, need);
    } else {
        // Walk in the index's walking stance if the type has one (`#MOTION_Lk`), otherwise
        // in the ground stance; then SetDestUnit (0x51b6c0): path to the target's cell.
        if amove {
            if usize::from(u.stance) != k + 1 {
                u.stance = (k + 1) as u8;
                return;
            }
        } else if u.stance != u.ground_state {
            u.stance = u.ground_state;
            try_to_stand(u, t, &mut w.random, false);
            return;
        }
        let m = if t.medium == crate::catalog::Medium::Water { &w.map.water } else { &w.map.land };
        let ecell = Unit::cell_of(e_pos, w.units[i].lx);
        let u = &mut w.units[i];
        let mover = Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
        if let Some((x, y)) = create_path(m, mover, &mut u.path, &mut w.random, ecell.0, ecell.1, t.stop_distance) {
            u.dest = Some(Pos::new(x, y));
        }
    }
}

/// A target inside the minimum range of the chosen attack (0x520689 in `AttackObjLink`): the
/// unit stands for 32 ticks (OB+0x5b), then backs away in up to 11 steps (OB+0x5a). More than
/// 16 px short it turns its back and walks off; closer, it either turns away or steps back
/// facing the target, whichever needs less turning. Without this a musketeer pressed against
/// an enemy (closer than the bayonet's 40 px and the musket's 250 px) stood still for good.
fn too_close(w: &mut World, i: usize, e_pos: Pos, need: i32) {
    let catalog = w.catalog.clone();
    let t = catalog.get(w.units[i].kind);
    let u = &mut w.units[i];
    if u.backoff == 0 {
        if u.backoff_wait < 32 {
            u.backoff_wait += 1;
            try_to_stand(u, t, &mut w.random, false);
            u.dest = None;
            return;
        }
        (u.backoff, u.backoff_wait) = (11, 0);
    }
    let dir = get_dir(e_pos.x - u.pos.x, e_pos.y - u.pos.y);
    let away = dir.wrapping_add(128);
    let mrot = t.rotate;
    let mrot1 = (mrot + mrot / 2).min(16);
    let to_away = away.wrapping_sub(u.dir) as i8;
    let to_face = dir.wrapping_sub(u.dir) as i8;
    // (direction to take, step backwards, turn still needed)
    let (d, back, dd) = if need > 16 || i32::from(to_away).abs() < i32::from(to_face).abs() {
        (away, false, to_away)
    } else {
        (dir, true, to_face)
    };
    if i32::from(dd).abs() < mrot1 {
        u.dir = d;
        crate::motion::step(w, i, d, back);
        let u = &mut w.units[i];
        u.backoff = u.backoff.saturating_sub(1);
    } else {
        // 0x50a20f: turn by `ROTATE` towards it.
        let r = mrot as u8;
        u.dir = if dd > 0 { u.dir.wrapping_add(r) } else { u.dir.wrapping_sub(r) };
    }
}

/// The engine's `TryToStand(EN, 0)` in the cannon's turning (0x51f85a): the target stands
/// (one random number; for a point the stand-in object only takes the number).
fn stand_target(w: &mut World, aim: Aim) {
    match aim {
        Aim::Unit(j) => {
            let catalog = w.catalog.clone();
            let World { units, random, .. } = w;
            let e = &mut units[j];
            try_to_stand(e, catalog.get(e.kind), random, false);
        }
        Aim::Point(..) => {
            w.random.rando();
        }
    }
}

/// Hunting (0x51bf0b in `AttackObjLink`): when the victim of a unit that can gather its
/// carcass (`TRANSFORMTOMEAT` sprite type with a `TAKERESSTAGES` work stance) is dying, the
/// attack ends; the hunter walks to the body (pushed last, so first) and then takes 60
/// gathering orders for the carcass that will appear there (`TakeResource` with order
/// type 1, each drawing a random number while nothing is found).
fn hunt(w: &mut World, i: usize, j: usize) -> bool {
    let Some(e) = w.units.get(j).filter(|e| e.alive && e.death != 0) else { return false };
    let Some(meat) = w.catalog.get(e.kind).economy.meat else { return false };
    let Some(kind) = w.sprite_kinds.1.chars.get(usize::from(meat)).and_then(|c| c.res_type).filter(|&k| k < 16) else {
        return false;
    };
    let t = w.catalog.get(w.units[i].kind);
    if !t.economy.take_res.get(usize::from(kind)).copied().flatten().is_some_and(|r| r.work != 0) {
        return false;
    }
    let (x, y) = e.pos.pixels();
    let target = e.pos;
    w.units[i].pop_order();
    for _ in 0..60 {
        crate::gather::take_resource_ord(w, i, x, y, kind, 0x90, 1);
    }
    let u = &mut w.units[i];
    u.orders.push(Order::SendTo { cell: Unit::cell_of(target, u.lx), prio: 0x10 });
    u.prio = 0x10;
    true
}

/// The muzzle of unit `i` at the current frame of its animation (`Shoot` 0x51ddc3): the
/// `SETACTIVEPOINT` point of its direction plus the frame's sprite offset, mirrored for the
/// east half of 9-direction sets. Returns one point per set, in pixels, with `z` the ground
/// plus `ZPOINTS` source height.
pub fn muzzle_points(w: &World, i: usize) -> Vec<(i32, i32, i32)> {
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    let (px, py) = u.pos.pixels();
    let src_z = t.zpoints.0;
    let z0 = height(w, u.pos) + src_z;
    let Some(m) = t.muzzles.get(&u.anim.name()) else { return vec![(px, py + 2 * src_z, z0)] };
    let r = m.directions;
    let (fdx, fdy) = m.frame_offsets.get(u.frame.max(0) as usize).copied().unwrap_or((0, 0));
    let dir = i32::from(u.dir);
    let mut out = Vec::new();
    for k in 0..i32::from(m.sets) {
        let off = k * r;
        let (j, x0) = if r & 1 == 1 || r == 0 {
            let uocts = if 2 * r - 2 == 0 { 1 } else { 2 * r - 2 };
            let uoc2 = r - 1;
            let usz = 255 / (2 * uocts);
            let udir = (((dir + usz + 64) & 0xFF) * uocts) >> 8;
            if udir < uoc2 {
                let j = uoc2 - udir + off;
                (j, px - (m.points.get(j as usize).map_or(0, |p| p.0) + fdx))
            } else {
                let j = uoc2 - uocts + udir + off;
                (j, px + m.points.get(j as usize).map_or(0, |p| p.0) + fdx)
            }
        } else {
            let j = ((dir + 128 / r - 64) & 0xFF) / r + off;
            (j, px + m.points.get(j as usize).map_or(0, |p| p.0) + fdx)
        };
        let apy = m.points.get(j as usize).map_or(0, |p| p.1);
        out.push((x0, py + 2 * (apy + fdy + src_z), z0));
    }
    out
}

/// `GetDamagePoint` (0x52557e) for a unit target: its position at its `ZPOINTS` aim height,
/// after two random numbers the engine multiplies by 0.
pub(crate) fn damage_point(w: &mut World, j: usize) -> (i32, i32, i32) {
    let e = &w.units[j];
    let t = w.catalog.get(e.kind);
    let (x, y) = e.pos.pixels();
    let c = if t.bars.is_empty() {
        (x, y, height(w, e.pos) + t.zpoints.1)
    } else {
        // A building aims at the centre of a random one of its boxes.
        let k = ((w.random.rando() * t.bars.len() as i32) >> 15) as usize;
        let [bx, by, l, wd, h] = t.bars[k];
        let cx = x + t.location.0 + bx + ((l + wd) >> 1);
        let cy = y + 2 * t.location.1 + 2 * by + ((l - wd) >> 1);
        (cx, cy, w.map.heights.at(cx, cy) + (h >> 1))
    };
    w.random.rando();
    w.random.rando();
    c
}

/// `GetDamagePoint` of an object, or of the stand-in object of an attack on the ground.
fn aim_point(w: &mut World, aim: Aim) -> (i32, i32, i32) {
    match aim {
        Aim::Unit(j) => damage_point(w, j),
        Aim::Point(p, _) => {
            w.random.rando();
            w.random.rando();
            let (x, y) = p.pixels();
            (x, y, height(w, p))
        }
    }
}

/// `CreateRazbros` (0x51b917): disperses the aim point by `RAZBROS`.
pub(crate) fn razbros(w: &mut World, i: usize, c: &mut (i32, i32, i32)) {
    let u = &w.units[i];
    let rz = w.adv(u.owner, u.kind).razbros;
    if rz == 0 {
        return;
    }
    let (xs, ys) = u.pos.pixels();
    let r = norma(xs - c.0, ys - c.1) >> 5;
    c.0 += ((w.random.rando() >> 5) - 512) * r * rz / 32000;
    c.1 += ((w.random.rando() >> 5) - 512) * r * rz / 32000;
}

/// `Shoot` (0x51ddc3): from every muzzle set of the strike animation, the aim point (target
/// position, dispersed by `RAZBROS`) and the attack's weapon (`Create3DAnmObject`).
fn shoot(w: &mut World, i: usize, aim: Aim, att: usize, my_z: i32, en_z: i32) {
    let catalog = w.catalog.clone();
    let wid = catalog.get(w.units[i].kind).attacks.get(att).and_then(|a| a.weapon_id);
    let Some(wid) = wid else {
        // Without the weapon table: the musket's trace, with its random numbers.
        let mut c = aim_point(w, aim);
        c.2 = en_z;
        razbros(w, i, &mut c);
        for _ in 0..4 {
            w.random.rando();
        }
        let (xs, ys) = w.units[i].pos.pixels();
        let damage = w.adv(w.units[i].owner, w.units[i].kind).attacks[att].damage;
        trace_line(w, (xs, ys, my_z), &mut c, damage, Some(i), att as u8);
        return;
    };
    for (x0, y0, z0) in muzzle_points(w, i) {
        let mut c = aim_point(w, aim);
        razbros(w, i, &mut c);
        crate::projectile::create(w, wid, (x0, y0, z0), c, Some(i), att as u8, None, (0, 0, 0), 0);
    }
}

/// `TraceObjectsInLine` (0x559310): an instant shot from `s` along the line to `d` (pixels),
/// out to twice its length. The unit nearest to the shooter within `UNITRADIUS` of the line
/// (doubled for friends closer than 250 px; friends only beyond 150 px, others beyond 60)
/// and 0..90 px below the line takes `damage`, halved every `DAMAGEDEC` px of the sender's
/// attack. A friend of a `FRIENDLYFIRE` shooter absorbs the ball. Returns whether something
/// was hit; `d` becomes the position of what was hit.
pub fn trace_line(
    w: &mut World,
    s: (i32, i32, i32),
    d: &mut (i32, i32, i32),
    damage: i32,
    sender: Option<usize>,
    att: u8,
) -> bool {
    let catalog = w.catalog.clone();
    let (xs, ys, zs) = s;
    let (mask, no_ff) = match sender.map(|k| &w.units[k]) {
        Some(u) => (u.nmask, catalog.get(u.kind).avoid_friendly_fire && !u.no_ff_avoid),
        None => (0, false),
    };
    let (dx, dy, dz) = (d.0 - xs, d.1 - ys, d.2 - zs);
    let len = isqrt(i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy) + i64::from(dz) * i64::from(dz));
    if len == 0 {
        return false;
    }
    let n = (len >> 4) + 1;
    let (mut best, mut min_r) = (None, 10_000);
    let mut last_cell = (-1, -1);
    for st in 0..2 * n {
        if min_r == 0 {
            break;
        }
        let (xx, yy, zz) = (dx * st / n, dy * st / n, dz * st / n);
        let (px, py, pz) = (xs + xx, ys + yy, zs + zz);
        let bh = w.bar_height(px, py);
        if bh != 0
            && w.map.heights.at(px, py) + bh > pz
            && let Some(o) = w.bar_owner(px, py).filter(|&o| Some(o) != sender)
        {
            best = Some(o);
            min_r = 0;
        }
        let cell = (px >> 7, py >> 7);
        if cell == last_cell {
            continue;
        }
        last_cell = cell;
        let list: Vec<u32> = w.buckets.all(cell.0, cell.1).collect();
        for k in list {
            let k = k as usize;
            let o = &w.units[k];
            if Some(k) == sender || !o.living() {
                continue;
            }
            let (ox, oy) = o.pos.pixels();
            let r_dist = norma(ox - xs, oy - ys);
            let h = pz - height(w, o.pos);
            let friend = o.nmask & mask != 0;
            let min_dist = if friend { 150 } else { 60 };
            if h > 0 && h < 90 && r_dist > min_dist && r_dist < min_r {
                let cross = (i64::from(ox - xs) * i64::from(yy) - i64::from(oy - ys) * i64::from(xx)).abs();
                let mut r = (cross / i64::from(len)) as i32;
                if friend && r_dist < 250 {
                    r *= 2;
                }
                if r < catalog.get(o.kind).unit_radius {
                    best = Some(k);
                    min_r = r_dist;
                }
            }
        }
    }
    let Some(b) = best else { return false };
    let bu = &w.units[b];
    if bu.building && bu.inside.is_empty() {
        return true; // a building without garrison absorbs the ball
    }
    let (bx, by) = bu.pos.pixels();
    (d.0, d.1) = (bx, by);
    let dmg = match sender {
        Some(k) if att < 12 => {
            let dec = catalog.get(w.units[k].kind).attacks.get(usize::from(att)).map_or(0xFFFF, |a| a.damage_dec);
            half_life(damage, norma(bx - xs, by - ys), dec)
        }
        _ => damage,
    };
    if no_ff && w.units[b].nmask & mask != 0 {
        return true; // the friend absorbs the ball unharmed
    }
    make_damage(w, b, dmg, sender, att);
    true
}

/// `trunc(d · 2^(−dist/dec))`, the shot falloff (the engine evaluates
/// `exp(−0.693147·dist/dec)` on the x87 FPU; this integer version can differ by 1).
pub fn half_life(d: i32, dist: i32, dec: i32) -> i32 {
    if dec <= 0 || dist <= 0 {
        return d;
    }
    // 2^(−k/16) in Q16, k = 0..=16.
    const T: [i64; 17] = [
        65536, 62757, 60097, 57548, 55109, 52773, 50535, 48393, 46341, 44376, 42495, 40693, 38968, 37316, 35734, 34219,
        32768,
    ];
    let q = (i64::from(dist) << 16) / i64::from(dec);
    let (whole, frac) = (q >> 16, q & 0xFFFF);
    if whole >= 31 {
        return 0;
    }
    let (k, r) = ((frac >> 12) as usize, frac & 0xFFF);
    let f = T[k] + (((T[k + 1] - T[k]) * r) >> 12);
    (((i64::from(d) * f) >> 16) >> whole) as i32
}

fn isqrt(v: i64) -> i32 {
    if v <= 0 {
        return 0;
    }
    // Newton's method on integers.
    let mut x = v;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    x as i32
}

/// Formation bonus scaled by the angle between `from → to` and the brigade's facing
/// (0x4e9002): full within 40, half below 70, none beyond; `#KARE` shapes always full.
fn formation_scaled(w: &World, unit: usize, other: usize, v: i32) -> i32 {
    let u = &w.units[unit];
    let Some(b) = u.brigade.and_then(|b| w.brigades.get(usize::from(b))) else { return v };
    if w.formations.shapes[b.shape].kare {
        return v;
    }
    let o = &w.units[other];
    let dd = i32::from((get_dir(o.pos.x - u.pos.x, o.pos.y - u.pos.y).wrapping_sub(b.dir) as i8).unsigned_abs());
    if dd <= 40 {
        v
    } else if dd < 70 {
        v >> 1
    } else {
        0
    }
}

/// `MakeDamage` (0x4e8990): applies `dmg` from `sender`'s attack index `att` to `victim`.
pub fn make_damage(w: &mut World, victim: usize, dmg: i32, sender: Option<usize>, att: u8) {
    if sender == Some(victim) || !w.units[victim].living() {
        return;
    }
    let catalog = w.catalog.clone();
    let vt = catalog.get(w.units[victim].kind);
    let va = w.adv(w.units[victim].owner, w.units[victim].kind);
    let mut dmg = dmg;
    let mut record = DeathRecord {
        cause: if w.units[victim].burning { DeathCause::Burned } else { DeathCause::Unknown },
        from: None,
        tick: 0,
    };
    if let Some(s) = sender {
        let st = catalog.get(w.units[s].kind);
        if st.skill_bonus != 0 && att < 32 && st.skill_mask & (1 << att) != 0 {
            let bonus = (i32::from(w.units[s].kills) * st.skill_bonus / 10).min(19 * dmg);
            dmg += bonus;
        }
    }
    if dmg <= 0 {
        return;
    }
    if att != 0xFF {
        dmg -= va.shield;
    }
    if let Some(s) = sender {
        let st = catalog.get(w.units[s].kind);
        let sa = w.adv(w.units[s].owner, w.units[s].kind);
        if !st.shot_points.is_empty() && att == 0 && sa.max_inside != 0 {
            // A garrisoned building's index 0 (0x4e8c43): MinDamage..MaxDamage by the shooters
            // inside (STRELOK, HORSE-STRELOK, ARCHER), replacing the shot's damage.
            let n = w.units[s]
                .inside
                .iter()
                .filter(|e| {
                    w.units.get(e.0 as usize).is_some_and(|x| matches!(catalog.get(x.kind).role.usage, 21 | 24 | 32))
                })
                .count() as i32;
            let (min, max) = (st.attacks[0].min_damage, sa.attacks[0].damage);
            dmg = min + (max - min) * n / sa.max_inside - va.shield;
        }
        let kind = sa.attacks.get(usize::from(att)).map_or("MECH", |a| a.kind.as_str());
        let wf = catalog.weapon_flags(kind);
        record = DeathRecord { cause: DeathCause::of_weapon(kind), from: Some(w.units[s].pos.pixels()), tick: 0 };
        if wf & GARRISON != 0 && vt.building {
            if !w.units[victim].inside.is_empty() {
                garrison_damage(w, victim, dmg, s, att);
            } else if w.units[s].building && w.units[s].target() == Some(w.units[victim].id) {
                // EnemySN += 0x100: the order is invalid and goes.
                w.units[s].pop_order();
            }
            return; // the building itself takes no damage from such weapons
        }
        if wf & RANDOM_KILL != 0 && w.random.rando() < 1310 && !vt.building && !vt.no25 {
            frighten(w, victim, s, att);
            record_hit(w, victim, s, record.cause, true);
            record_death(w, victim, record);
            die(w, victim);
            w.units[s].kills = w.units[s].kills.saturating_add(1);
            return;
        }
        dmg -= va.protection.iter().find(|p| p.0 == kind).map_or(0, |p| p.1);
        if wf & FIRE != 0 {
            // X: set on fire at once (+0x3f = FIRELIMIT + 1).
            let v = &mut w.units[victim];
            (v.fire, v.burning) = (vt.fire_limit + 1, true);
        } else if wf & FIRE_SLOW != 0 {
            let v = &mut w.units[victim];
            if v.fire < vt.fire_limit {
                v.fire += 1;
            } else {
                v.burning = true;
            }
        }
        if wf & FORMATION != 0 && w.units[s].attack_bonus != 0 {
            dmg += formation_scaled(w, s, victim, w.units[s].attack_bonus);
        }
        if w.units[victim].defence_bonus != 0 {
            dmg -= formation_scaled(w, victim, s, w.units[victim].defence_bonus);
        }
    }
    let dmg = dmg.max(1);
    if let Some(s) = sender {
        record_hit(w, victim, s, record.cause, w.units[victim].life <= dmg);
    }
    if w.units[victim].life > dmg {
        w.units[victim].life -= dmg;
        if !vt.economy.sections.is_empty() {
            section_damage(w, victim);
        }
        // The victim hits back (0x4e9cfb), except cannons (MotionStyle 3).
        if let Some(s) = sender
            && vt.motion_style != crate::catalog::MotionStyle::ComplexRotate
        {
            attack_obj(w, victim, s, AUTO_PRIO);
        }
    } else {
        w.units[victim].life = 0;
        if let Some(s) = sender {
            w.units[s].kills = w.units[s].kills.saturating_add(1);
            frighten(w, victim, s, att);
        }
        record_death(w, victim, record);
        die(w, victim);
    }
}

/// The sections of a building that lost life and lives on (`MakeDamage` 0x4e93fe..0x4e9925,
/// for types with `ADDSECTION`). The threshold starts at the type's life (for the owner)
/// and grows by the life of each section that was done, in section order. While life is at
/// or below it, a section being built drops back to not built (0x4e94b3; its research goes
/// on and marks it again on the next tick) and a done one becomes a ruin (0x4e94f1) with
/// its explosions ([`section_ruin`]). Life and maximum life stay: a ruin keeps its share of
/// the maximum, so builders can repair it. The building's upgrade count (OB+0xe9) is
/// overwritten on the way: 2 + the section if life is above the threshold, else 2, so 2
/// after the last section unless life exceeds the whole threshold. The object is read
/// anew for every section: the explosions may hurt it again on the way.
fn section_damage(w: &mut World, b: usize) {
    use crate::upgrade::{section, section_state, with_section};
    let catalog = w.catalog.clone();
    let sections = &catalog.get(w.units[b].kind).economy.sections;
    let mut threshold = w.adv(w.units[b].owner, w.units[b].kind).life;
    for s in 0..5u8 {
        let state = section_state(w.units[b].stages, s);
        if w.units[b].life <= threshold {
            let u = &mut w.units[b];
            match state {
                section::BUILDING => u.stages = with_section(u.stages, s, section::NONE),
                section::DONE => {
                    u.stages = with_section(u.stages, s, section::RUIN);
                    section_ruin(w, b, s);
                }
                _ => {}
            }
            w.units[b].upgrade_level = 2;
        } else {
            w.units[b].upgrade_level = s + 2;
        }
        if state == section::DONE {
            threshold += sections.iter().find(|x| x.0 == s).map_or(0, |x| x.1);
        }
    }
}

/// A section that became a ruin (0x4e952f..0x4e98bd), if it has `EXPLPOINTS`: at each of
/// its cells (relative to the lock base cell 0x516fde) one random `DESTRUCT` weapon
/// (`rando()·n >> 15`) explodes when a second `rando()` is below the `DESTRUCT`
/// probability; both numbers are drawn for every cell, also without `DESTRUCT`. The
/// explosions are lifted by `h = done y + 80 − 8·(mean cell y)` (picture rows: 2h further
/// down the map, h higher). Then the engine frees the `STAGELOCKPOINTS` cells of the stage
/// numbered by its loop counter, which is the cell count at that point: stages 0..4 for
/// up to 4 cells. With more it reads past the five lists (NM+0x16ce + 2·count); for the
/// only file with `EXPLPOINTS` (PORTKRE, 298 cells, not in `NMLIST.TXT`) that is the middle
/// of `OneStepDX[50]`, which is 0, so nothing is freed there.
fn section_ruin(w: &mut World, b: usize, s: u8) {
    let catalog = w.catalog.clone();
    let t = catalog.get(w.units[b].kind);
    let ruin = &t.economy.section_ruin[usize::from(s)];
    let n = ruin.expl_points.len() as i32;
    if n == 0 {
        return;
    }
    // The mean x is computed too but not used.
    let ay = ruin.expl_points.iter().map(|p| p.1).sum::<i32>() / n;
    let h = ruin.done_y + 0x50 - (ay << 3);
    let (cx, cy) = crate::economy::base_cell(t, w.units[b].pos);
    let count = t.destruct.ids.len() as i32;
    for &(px, py) in &ruin.expl_points {
        let x = (cx + px) << 4;
        let y = ((cy + py) << 4) + 2 * h;
        let k = ((w.random.rando() * count) >> 15) as usize;
        if w.random.rando() < t.destruct.prob
            && let Some(Some(wid)) = t.destruct.ids.get(k)
        {
            let z = w.map.heights.at(x, y) + h;
            crate::projectile::create(w, *wid, (x, y, z), (x, y, 1000), None, 0, None, (0, 0, 0), 0);
        }
    }
    if let Some(cells) = t.economy.stage_lock_points.get(n as usize) {
        let map = std::sync::Arc::make_mut(&mut w.map);
        for &(x, y) in cells {
            map.land.clr_pt(cx + x, cy + y);
        }
    }
}

/// 0x4e8870: a shot at a garrisoned building may hit one of the units inside. Misses with
/// `rando()·101 >> 15 < PROMAX[attacker's PROMAXCTG]`; a unit's melee index 0 does nothing.
fn garrison_damage(w: &mut World, b: usize, dmg: i32, s: usize, att: u8) {
    let st = w.catalog.get(w.units[s].kind);
    if att == 0 && st.attacks[1].weapon.is_some() && !w.units[s].building {
        return;
    }
    let promax = w.catalog.get(w.units[b].kind).promax[usize::from(st.promax_ctg)];
    let r = (w.random.rando() * 101) >> 15;
    if r < promax {
        return;
    }
    let n = w.units[b].inside.len() as i32;
    let j = ((w.random.rando() * n) >> 15) as usize;
    if let Some(e) = w.units[b].inside.get(j).copied() {
        make_damage(w, e.0 as usize, dmg, Some(s), att);
    }
}

/// The fear a kill raises around the victim (MakeDamage 0x4e8f92 / 0x4e9c4d, before `Die`):
/// the killer's `FEARTYPE` of the attack index, within 256 px.
fn frighten(w: &mut World, victim: usize, sender: usize, att: u8) {
    let ft = w.catalog.get(w.units[sender].kind).fear.types.get(usize::from(att)).copied().unwrap_or(0xFF);
    if ft != 0xFF {
        let (x, y) = w.units[victim].pos.pixels();
        crate::morale::fear(w, x, y, w.units[victim].owner, 256, ft);
    }
}

/// Why a unit died, kept for the renderer only (`World::deaths`): the original draws every
/// death alike, the 3D models die by cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeathCause {
    /// Killed by a script, a lost transport and the like.
    #[default]
    Unknown,
    Melee,
    Gunshot,
    Arrow,
    /// Tomahawk, spear or stone.
    Thrown,
    Cannonball,
    Grapeshot,
    /// Mortar shells and other bursting rounds.
    Explosion,
    /// Died burning.
    Burned,
    Hunger,
}

impl DeathCause {
    /// The cause of a hit by a weapon kind of `NRES.DAT [WEAPONS]`.
    pub fn of_weapon(kind: &str) -> Self {
        match kind {
            "MECH" | "PIKA" => Self::Melee,
            "VISTREL" | "VISTREL2" => Self::Gunshot,
            "OSTRELA" => Self::Arrow,
            k if k.starts_with("STRELA") => Self::Arrow,
            "TAMAGAVK" | "KOPIE" | "KAMEN" => Self::Thrown,
            "IADRO" => Self::Cannonball,
            "KARTECH" => Self::Grapeshot,
            "IADROMOR" | "VAFLIA" => Self::Explosion,
            _ => Self::Unknown,
        }
    }
}

/// How a unit died: the cause and where the blow came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeathRecord {
    pub cause: DeathCause,
    /// The attacker's position at the blow (world pixels).
    pub from: Option<(i32, i32)>,
    /// The tick the unit died: the renderer keeps 3D corpses longer than the simulation
    /// does and times their decay from it.
    pub tick: u64,
}

/// How long hits stay in `World::hits` (ticks): long enough for the renderer to draw the
/// blood they splatter until it has dried and faded.
pub const HIT_KEEP: u64 = 750;
/// At most this many hits are kept (the oldest go first).
pub const HIT_CAP: usize = 4096;

/// A hit on a unit or building (`World::hits`), for the renderer only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitRecord {
    /// The tick of the hit.
    pub tick: u64,
    /// The unit hit (index).
    pub victim: u32,
    /// Where the victim stood (world pixels).
    pub pos: (i32, i32),
    /// The direction of the blow, from the attacker to the victim (world pixels, not
    /// normalised; zero when they stand on the same point).
    pub dir: (i32, i32),
    /// The weapon's cause ([`DeathCause::of_weapon`]).
    pub cause: DeathCause,
    /// Whether the hit killed.
    pub killed: bool,
    /// Whether the victim is a building.
    pub building: bool,
}

/// Notes a hit of `sender` on `victim` (render only, not hashed).
fn record_hit(w: &mut World, victim: usize, sender: usize, cause: DeathCause, killed: bool) {
    let (vx, vy) = w.units[victim].pos.pixels();
    let (sx, sy) = w.units[sender].pos.pixels();
    if w.hits.len() >= HIT_CAP {
        w.hits.pop_front();
    }
    w.hits.push_back(HitRecord {
        tick: w.tick.0,
        victim: victim as u32,
        pos: (vx, vy),
        dir: (vx - sx, vy - sy),
        cause,
        killed,
        building: w.units[victim].building,
    });
}

/// Drops the hits older than [`HIT_KEEP`] ticks (at the start of every step).
pub fn forget_hits(w: &mut World) {
    while w.hits.front().is_some_and(|h| h.tick + HIT_KEEP < w.tick.0) {
        w.hits.pop_front();
    }
}

/// Notes why a living unit is about to die; the first record of a death stays.
pub fn record_death(w: &mut World, i: usize, record: DeathRecord) {
    if w.units.get(i).is_some_and(Unit::living) {
        w.deaths.entry(i as u32).or_insert(record);
    }
}

/// `Die` (0x4e77a0): the unit turns a little, frees its cell and starts dying.
pub fn die(w: &mut World, i: usize) {
    let u = &w.units[i];
    if u.death != 0 || !u.alive {
        return;
    }
    let tick = w.tick.0;
    w.deaths.entry(i as u32).or_default().tick = tick;
    if w.catalog.get(u.kind).command_center {
        // City+0xeb1a (0x4e7a1f): a lost command centre frightens the nation for a while.
        w.players[usize::from(u.owner) & 7].cc_lost_time = (w.tick.0 + 1) as u32;
    }
    // Buildings and artillery skip the turn and its random number (0x4e78e1).
    let turn = if u.building || w.catalog.get(u.kind).role.artillery { 0 } else { (w.random.rando() >> 8) - 64 };
    w.random.rando();
    let u = &mut w.units[i];
    u.dir = u.dir.wrapping_add(turn as u8);
    u.death = 1;
    u.life = 0;
    u.orders.clear();
    u.path.clear();
    u.in_motion = false;
    u.dest = None;
    u.burning = false;
    if !u.building {
        w.occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
    }
    w.bars.remove(i as u32);
    // A building's garrison dies with it (0x4e7b1d) and is gone at once.
    let inside = std::mem::take(&mut w.units[i].inside);
    for e in inside {
        let k = e.0 as usize;
        if w.units.get(k).is_some_and(|x| x.living()) {
            w.units[k].inside_of = None;
            die(w, k);
            w.units[k].alive = false;
        }
    }
    if w.units[i].building {
        destroyed_building_explodes(w, i);
    }
}

/// The end of `Die` for a building (0x4e84c7..0x4e8678): unless its `delay` (OB+0x8c) is
/// 1500 or more (the player's own destroy order sets 6000, 0x4dcbb4) or it was not built
/// half way (`stage < BUILDSTAGES / 2`), each of its `CHECKPOINTS` cells (relative to the
/// lock base cell 0x516fde) draws two `rando()`: the first picks one of its `DESTRUCT`
/// weapons (`r·n >> 15`), the second below the `DESTRUCT` probability creates it at the
/// cell, 16 px above the ground, towards `(x, y, 1000)` with no sender (0x558d60). Types
/// without `DESTRUCT` draw nothing.
///
/// Wall pieces (OB+0x29 bit 0) take another way (0x4e7efb..0x4e8247): the pieces in the 3×3
/// wall cells around one that falls go with it, each with 10 explosions at random offsets
/// of up to 32 px (the loaded data has no wall types; not reproduced).
fn destroyed_building_explodes(w: &mut World, b: usize) {
    let catalog = w.catalog.clone();
    let t = catalog.get(w.units[b].kind);
    let u = &w.units[b];
    let n = t.destruct.ids.len() as i32;
    if u.delay >= 1500 || u.stage < w.max_stage(u) >> 1 || n == 0 {
        return;
    }
    let (cx, cy) = crate::economy::base_cell(t, u.pos);
    for &(px, py) in &t.check_points {
        let (x, y) = ((cx + px) << 4, (cy + py) << 4);
        let k = ((w.random.rando() * n) >> 15) as usize;
        if w.random.rando() < t.destruct.prob
            && let Some(Some(wid)) = t.destruct.ids.get(k)
        {
            let z = w.map.heights.at(x, y) + 16;
            crate::projectile::create(w, *wid, (x, y, z), (x, y, 1000), None, 0, None, (0, 0, 0), 0);
        }
    }
}

/// `CheckVisibility` (0x556af4): no building box higher than 18 px (not the searcher's own)
/// stands between `from` and `to` (1/16 px), sampled every 32 px without the ends.
pub fn visible_between(w: &World, from: Pos, to: Pos, me: usize) -> bool {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let n = (norma(4 * dx, 4 * dy) >> 11) + 1;
    (1..n).all(|k| {
        let (x, y) = ((from.x + dx * k / n) >> 4, (from.y + dy * k / n) >> 4);
        w.bar_height(x, y) <= 18 || w.bar_owner(x, y) == Some(me)
    })
}

/// The first enemy in a 128 px cell that the searcher can hurt (`SearchEnemyInCell`
/// 0x525ac1): guards only while they attack.
fn enemy_in_cell(w: &World, u: &Unit, kill: u8, cx: i32, cy: i32) -> Option<usize> {
    let ok = |k: &&u32| {
        let o = &w.units[**k as usize];
        o.active() && u.hostile(o) && w.catalog.get(o.kind).materials & kill != 0 && (!o.guard || o.target().is_some())
    };
    let b = &w.buckets;
    // NPresence: only cells where a nation outside the searcher's friends is present.
    if b.presence(cx, cy) & !u.nmask == 0 {
        return None;
    }
    b.get(cx, cy).iter().find(ok).or_else(|| b.buildings_in(cx, cy).iter().find(ok)).map(|&k| k as usize)
}

/// The fire damage of the object loop's first pass (0x5112e3): a burning building loses
/// `max(1, MaxLife >> 11)` every tick, a burning unit 1 every 32 ticks while above a tenth of
/// its life.
fn burn(w: &mut World, i: usize) {
    let u = &w.units[i];
    if !u.burning || u.life <= 0 {
        return;
    }
    if u.building {
        let d = (u.max_life >> 11).max(1);
        make_damage(w, i, d, None, 0xFF);
    } else if (w.tick.0 + 1) & 31 == 0 && u.life > u.max_life / 10 {
        make_damage(w, i, 1, None, 0xFF);
    }
}

/// The first pass of `LongProcesses` (0x5112e3) over every object: fire damage, then
/// `SearchVictim` (0x52622b). Human nations search every tick (peasants every 16th), AI
/// nations every 8th tick (towers every tick). A garrisoned building picks a random enemy
/// from one random unit per enemy cell of its square; others take the nearest enemy to the
/// search centre (`SEARCH_ENEMY_RADIUS`, centred `_SHIFT` ahead) after a `rando() > 0x2000`
/// skip, or, with a search square wider than 10 cells, from 6 random cells. The target must
/// be visible past building boxes; the attack has priority 1.
pub fn search_victims(w: &mut World) {
    let catalog = w.catalog.clone();
    let tm = (w.tick.0 + 1) as usize;
    for i in 0..w.units.len() {
        burn(w, i);
        let u = &w.units[i];
        if !u.active() || u.building && u.stage < w.max_stage(u) {
            continue;
        }
        let t = catalog.get(u.kind);
        // NoSearchVictim and AI guards never search (0x52624b, 0x526263).
        if t.search_radius == 0 || u.no_search_victim || u.guard {
            continue;
        }
        let ai = w.ai[usize::from(u.owner) & 7];
        let scheduled = if ai {
            t.role.usage == crate::catalog::usage::TOWER || (i & 7) == (tm & 7)
        } else {
            t.role.usage != crate::catalog::usage::PEASANT || (i & 15) == (tm & 15)
        };
        if !scheduled || (ai && t.role.archer) {
            continue;
        }
        if t.sector_weapon.is_some() {
            // WEAPONINSECTOR units (cannons) only fire canister on their own (0x526349).
            crate::artillery::auto_canister(w, i);
            continue;
        }
        let (sr, ss) = (t.search_radius, t.search_shift);
        let rx = (sr >> 11) + 1;
        let garrison = u.building && !t.shot_points.is_empty() && !u.inside.is_empty();
        // The 3-in-4 skip comes before any work (the setup above it draws nothing).
        if !garrison && rx <= 10 && w.random.rando() > 0x2000 {
            continue;
        }
        let u = &w.units[i];
        let c = if ss != 0 {
            Pos::new(u.pos.x + ((cos(u.dir) * ss) >> 8), u.pos.y + ((sin(u.dir) * ss) >> 8))
        } else {
            u.pos
        };
        let ta = w.adv(u.owner, u.kind);
        let min_r = min_range(&ta) << 4;
        let kill_mask = ta.attacks.iter().filter(|a| a.usable()).fold(0, |m, a| m | a.mask);
        let (ccx, ccy) = (c.x >> 11, c.y >> 11);
        let side = 2 * rx + 1;
        let mut best = (10_000_000, None);
        let nearest = |w: &World, best: &mut (i32, Option<usize>), cx: i32, cy: i32| {
            if let Some(k) = enemy_in_cell(w, &w.units[i], kill_mask, cx, cy) {
                let o = &w.units[k];
                let d = norma(o.pos.x - c.x, o.pos.y - c.y);
                if d < best.0 && d > min_r {
                    *best = (d, Some(k));
                }
            }
        };
        if garrison {
            // The garrison path (0x5271a4): one random enemy per cell (0x525caa), then a
            // random one of those.
            let mut list = Vec::new();
            for cy in ccy - rx..=ccy + rx {
                for cx in ccx - rx..=ccx + rx {
                    if w.buckets.presence(cx, cy) & !w.units[i].nmask == 0 {
                        continue;
                    }
                    let me = &w.units[i];
                    let ok = |k: &usize| {
                        let o = &w.units[*k];
                        o.living()
                            && o.inside_of.is_none()
                            && me.hostile(o)
                            && catalog.get(o.kind).materials & kill_mask != 0
                    };
                    let n = w.buckets.all(cx, cy).map(|k| k as usize).filter(ok).take(64).count();
                    if n == 0 {
                        continue;
                    }
                    let pick = ((w.random.rando() * n as i32) >> 15) as usize;
                    let me = &w.units[i];
                    let ok = |k: &usize| {
                        let o = &w.units[*k];
                        o.living()
                            && o.inside_of.is_none()
                            && me.hostile(o)
                            && catalog.get(o.kind).materials & kill_mask != 0
                    };
                    let k = w.buckets.all(cx, cy).map(|k| k as usize).filter(ok).nth(pick).expect("counted");
                    let o = &w.units[k];
                    if norma(o.pos.x - c.x, o.pos.y - c.y) > min_r && list.len() < 1024 {
                        list.push(k);
                    }
                }
            }
            if !list.is_empty() {
                best = (100, Some(list[((w.random.rando() * list.len() as i32) >> 15) as usize]));
            }
        } else if rx <= 10 {
            for cy in ccy - rx..=ccy + rx {
                for cx in ccx - rx..=ccx + rx {
                    nearest(w, &mut best, cx, cy);
                }
            }
        } else {
            if (ta.attacks.iter().filter(|a| a.usable()).map(|a| a.radius.0).min().unwrap_or(0)) < 140
                && !(u.stand_time > 16 && w.random.rando() > 6000)
            {
                for cy in ccy - 1..=ccy + 1 {
                    for cx in ccx - 1..=ccx + 1 {
                        nearest(w, &mut best, cx, cy);
                    }
                }
            }
            if best.1.is_none() {
                let n = w.buckets.side();
                for _ in 0..6 {
                    let dx = ((w.random.rando() * side) >> 15) - rx;
                    let dy = ((w.random.rando() * side) >> 15) - rx;
                    let (cx, cy) = (ccx + dx, ccy + dy);
                    if cx < 0 || cy < 0 || cx >= n || cy >= n {
                        continue;
                    }
                    w.random.rando();
                    if w.buckets.presence(cx, cy) & !w.units[i].nmask == 0 {
                        continue;
                    }
                    w.random.rando();
                    let Some(k) = enemy_in_cell(w, &w.units[i], kill_mask, cx, cy) else { continue };
                    w.random.rando();
                    let o = &w.units[k];
                    let d = norma(o.pos.x - c.x, o.pos.y - c.y);
                    if d < best.0 && d > min_r {
                        best = (d, Some(k));
                    }
                }
            }
        }
        if let (d, Some(k)) = best
            && d < sr
            && visible_between(w, w.units[k].pos, c, i)
        {
            attack_obj(w, i, k, AUTO_PRIO);
        }
    }
}

/// The object loop (0x5118b3) after all units moved: reload counters, idle reloading of
/// `SLOWRECHARGE` units, and the death animation and corpse removal.
pub fn object_loop(w: &mut World) {
    let catalog = w.catalog.clone();
    for i in 0..w.units.len() {
        let u = &mut w.units[i];
        if !u.alive {
            continue;
        }
        let t = catalog.get(u.kind);
        if u.death != 0 {
            if u.death == 1 {
                if anim_frames(t, AnimKind::Death).is_none() {
                    u.alive = false;
                    continue;
                }
                set_anim(u, t, AnimKind::Death);
                u.frame = 0;
            }
            if u.anim == AnimKind::Death {
                u.frame += FRM_DEC;
                if u.frame >= u.anim_frames - FRM_DEC {
                    if t.economy.meat.is_some() {
                        // The body becomes a carcass sprite (0x511f4b).
                        if crate::sprite::leave_carcass(w, i) {
                            w.units[i].alive = false;
                            continue;
                        }
                    }
                    let u = &mut w.units[i];
                    if anim_frames(t, AnimKind::DeathLie(1)).is_some() {
                        set_anim(u, t, AnimKind::DeathLie(1));
                        u.frame = 0;
                    } else {
                        u.alive = false;
                    }
                }
            } else {
                u.death = u.death.saturating_add(1);
                let next = match u.death {
                    1201.. if u.anim == AnimKind::DeathLie(1) => Some(2),
                    1601.. if u.anim == AnimKind::DeathLie(2) => Some(3),
                    _ => None,
                };
                if let Some(n) = next {
                    if anim_frames(t, AnimKind::DeathLie(n)).is_some() {
                        set_anim(u, t, AnimKind::DeathLie(n));
                        u.frame = 0;
                    } else {
                        u.alive = false;
                    }
                }
                if u.death > CORPSE_LIFETIME {
                    u.alive = false;
                }
            }
            let u = &mut w.units[i];
            u.death = u.death.saturating_add(1);
            continue;
        }
        crate::economy::starve(w, i);
        let u = &mut w.units[i];
        if u.death != 0 {
            continue;
        }
        if u.building && u.life < u.max_life / 2 {
            // Smoke of a damaged building (0x511be2): a random number even without FOGGING.
            w.random.rando();
        }
        let u = &mut w.units[i];
        if u.delay > 0 {
            if !t.slow_recharge {
                u.delay = (u.delay - FRM_DEC).max(0);
            } else if u.stand_time > 20 && u.orders.is_empty() && (u.frame >> 1) == ((u.anim_frames - 1) >> 1) {
                let reload = AnimKind::Attack(3);
                if u.stance != 4 {
                    u.stance = 4;
                    try_to_stand(u, t, &mut w.random, false);
                } else if u.anim != reload {
                    u.delay = (u.delay - anim_frames(t, reload).unwrap_or(1)).max(0);
                    set_anim(u, t, reload);
                    u.frame = 0;
                    u.in_motion = false;
                }
            }
        } else if u.stance == 4 {
            u.stance = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_life_halves_per_distance() {
        assert_eq!(half_life(380, 0, 300), 380);
        assert_eq!(half_life(380, 300, 300), 190);
        assert_eq!(half_life(380, 600, 300), 95);
        assert!((half_life(380, 850, 300) - 53).abs() <= 1);
        assert_eq!(half_life(380, 100, 0xFFFF), 379);
    }
}
