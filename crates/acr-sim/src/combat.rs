//! Land combat as the engine does it: auto-targeting (`SearchVictim` 0x52622b), attack
//! orders (`AttackObj` 0x51a930, `AttackObjLink` 0x51bcaf), damage (`MakeDamage`
//! 0x4e8990), musket shots as instant line traces (`TraceObjectsInLine` 0x559310), reload
//! and death (object loop 0x5118b3). See `docs/simulation.md`.
//!
//! Attack index `i` is stance `i + 1`; its animation is `#ATTACKi`. A strike starts at the
//! end of an animation and hits when the animation reaches its active frame
//! (`SETACTIVEPOINT`). `ATTACK_PAUSE` counts animation frames: two per tick, or, for
//! `SLOWRECHARGE` units, the frames of each `#ATTACK3` reload animation played.

use acr_formats::defs::nres::weapon_flags::{FORMATION, GARRISON, HIGH, RANDOM_KILL};

use crate::catalog::ObjectType;
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

/// `ADC.MinR`: the smallest attack range of the indices that can attack (0x43b95a).
fn min_range(t: &ObjectType) -> i32 {
    let usable = t.attacks.iter().filter(|a| a.usable());
    let min = usable.clone().map(|a| a.radius.0).min().unwrap_or(0);
    let max = usable.map(|a| a.radius.1).max().unwrap_or(0);
    if max < min { 0 } else { min }
}

fn is_high(w: &World, t: &ObjectType, i: usize) -> bool {
    w.catalog.weapon_flags(&t.attacks[i].kind) & HIGH != 0
}

/// `AttackObj` (0x51a930): gives unit `i` an attack order on unit `j`; false if refused.
pub fn attack_obj(w: &mut World, i: usize, j: usize, prio: u8) -> bool {
    let (Some(u), Some(e)) = (w.units.get(i), w.units.get(j)) else { return false };
    if i == j || !u.active() || u.building || !e.active() || e.owner == u.owner {
        return false;
    }
    let t = w.catalog.get(u.kind);
    let materials = w.catalog.get(e.kind).materials;
    // CheckAttAbility: some index can attack this material.
    if !t.attacks.iter().any(|a| a.usable() && a.mask & materials != 0) {
        return false;
    }
    if prio < u.prio && !u.orders.is_empty() {
        return false;
    }
    if prio < 5
        && let Some(cur) = u.target().and_then(|c| w.units.get(c.0 as usize))
    {
        let d1 = norma(e.pos.x - u.pos.x, e.pos.y - u.pos.y);
        let d2 = norma(cur.pos.x - u.pos.x, cur.pos.y - u.pos.y);
        // The engine compares the 1/16 px distance with MinR in pixels (quirk kept).
        if d1 <= min_range(t) || d2 <= d1 {
            return false;
        }
    }
    let target = e.id;
    let u = &mut w.units[i];
    u.stand_time = 0;
    if let Some(Order::Attack { target: t, prio: p }) = u.orders.last_mut() {
        *t = target;
        *p = prio;
    } else {
        u.orders.clear();
        u.orders.push(Order::Attack { target, prio });
        u.path.delay = 0;
    }
    u.prio = prio;
    true
}

/// `AttackObjLink` (0x51bcaf), once per tick for a unit with an attack order.
pub fn attack_link(w: &mut World, i: usize, target: EntityId, prio: u8) {
    let catalog = w.catalog.clone();
    let j = target.0 as usize;
    {
        let u = &mut w.units[i];
        u.prio = prio;
        u.dest = None;
    }
    let valid = w.units.get(j).is_some_and(|e| e.alive && e.death <= 8 && e.owner != w.units[i].owner);
    if !valid {
        w.units[i].orders.pop();
        return;
    }
    let (u, e) = (&w.units[i], &w.units[j]);
    let t = catalog.get(u.kind);
    let et = catalog.get(e.kind);
    let ns = usize::from(u.stance);
    let mut delayed = u.delay != 0;
    let mut can_set_delay = true;
    if delayed {
        if ns > 0 && t.attacks[ns - 1].no_pause {
            delayed = false;
        }
        can_set_delay = false;
    }
    let need_slow = delayed && t.slow_recharge;
    let (dx, dy) = (e.pos.x - u.pos.x, e.pos.y - u.pos.y);
    let dst = norma(dx, dy) >> 4;
    let my_z = height(w, u.pos) + t.zpoints.0;
    let en_z = height(w, e.pos) + et.zpoints.1;
    let en_dir = get_dir(dx, dy);
    let e_pos = e.pos;
    let high_ns = ns > 0 && is_high(w, t, ns - 1);
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
                let a = &t.attacks[ns - 1];
                let mut dst1 = dst - et.add_shot_radius;
                if high_ns {
                    dst1 -= 2 * (my_z - en_z);
                }
                let in_range = dst >= a.radius.0 && dst1 <= a.radius.1 + a.add_radius;
                let (damage, weapon) = (a.damage, a.weapon.is_some());
                let pause = a.pause;
                let u = &mut w.units[i];
                u.dir = en_dir;
                if in_range {
                    if can_set_delay {
                        u.delay = pause;
                    }
                    if weapon {
                        shoot(w, i, j, ns - 1, my_z, en_z);
                    } else {
                        make_damage(w, j, damage, Some(i), (ns - 1) as u8);
                        if w.units[j].living() {
                            attack_obj(w, j, i, STRUCK_PRIO);
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
        let a = &t.attacks[k];
        let ok = if is_high(w, t, k) {
            dst >= a.detect.0 && dstx <= a.detect.1
        } else {
            a.detect.0 <= dst && dst <= a.detect.1
        };
        ok && a.mask & et.materials != 0
    };
    let mut chosen = (0..t.attacks.len()).rev().find(|&k| fits(k));
    if u.delay != 0
        && let Some(k) = (0..t.attacks.len()).rev().find(|&k| t.attacks[k].no_pause && fits(k))
    {
        chosen = Some(k);
    }
    let Some(k) = chosen else {
        let (ex, ey) = e_pos.pixels();
        let areas = w.map.land_areas.clone();
        let u = &mut w.units[i];
        u.orders.pop();
        crate::motion::smart_send_to(u, areas.as_ref(), ex, ey, 0, 0, prio);
        return;
    };
    let high = is_high(w, t, k);
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
    let a = &t.attacks[k];
    let u = &mut w.units[i];
    if dst >= a.radius.0 && dst_c < a.radius.1 + et.add_shot_radius {
        let ddir = en_dir.wrapping_sub(u.dir) as i8;
        let mrot = t.rotate;
        let mrot1 = (mrot + mrot / 2).min(16);
        if i32::from(ddir).abs() < mrot1 {
            u.dir = en_dir;
            if usize::from(u.local_stance) == k + 1 {
                if !delayed || t.attacks[k].no_pause {
                    let kind = AnimKind::Attack(k as u8);
                    set_anim(u, t, kind);
                    u.frame = 0;
                    u.in_motion = false;
                }
            } else {
                u.stance = (k + 1) as u8;
            }
        } else {
            let r = mrot as u8;
            u.dir = if ddir > 0 { u.dir.wrapping_add(r) } else { u.dir.wrapping_sub(r) };
        }
    } else if dst >= a.radius.0 {
        // SetDestUnit (0x51b6c0): path to the enemy's cell.
        u.stance = 0;
        let m = if t.medium == crate::catalog::Medium::Water { &w.map.water } else { &w.map.land };
        let ecell = Unit::cell_of(w.units[j].pos, w.units[i].lx);
        let u = &mut w.units[i];
        let mover = Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
        if let Some((x, y)) = create_path(m, mover, &mut u.path, &mut w.random, ecell.0, ecell.1, t.stop_distance) {
            u.dest = Some(Pos::new(x, y));
        }
    }
}

/// `Shoot` + `TraceObjectsInLine` (0x51ddc3, 0x559310): a musket ball is an instant trace
/// from the shooter towards the (dispersed) aim point, out to twice its distance; the
/// closest unit within `UNITRADIUS` of the line takes the damage, halved every `DAMAGEDEC` px.
fn shoot(w: &mut World, i: usize, j: usize, att: usize, my_z: i32, en_z: i32) {
    let catalog = w.catalog.clone();
    let (u, e) = (&w.units[i], &w.units[j]);
    let t = catalog.get(u.kind);
    let (xs, ys) = u.pos.pixels();
    let (mut cx, mut cy) = e.pos.pixels();
    let owner = u.owner;
    // GetDamagePoint draws two numbers without using them.
    w.random.rando();
    w.random.rando();
    if t.razbros != 0 {
        let r = norma(xs - cx, ys - cy) >> 5;
        cx += ((w.random.rando() >> 5) - 512) * r * t.razbros / 32000;
        cy += ((w.random.rando() >> 5) - 512) * r * t.razbros / 32000;
    }
    // Muzzle smoke and the child projectile: four more draws.
    for _ in 0..4 {
        w.random.rando();
    }
    let (dx, dy, dz) = (cx - xs, cy - ys, en_z - my_z);
    let len = isqrt(i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy) + i64::from(dz) * i64::from(dz));
    if len == 0 {
        return;
    }
    let n = (len >> 4) + 1;
    let (mut best, mut min_r) = (None, 10_000);
    let mut last_cell = (-1, -1);
    for s in 0..2 * n {
        if min_r == 0 {
            break;
        }
        let (xx, yy, zz) = (dx * s / n, dy * s / n, dz * s / n);
        let (px, py, pz) = (xs + xx, ys + yy, my_z + zz);
        let cell = (px >> 7, py >> 7);
        if cell == last_cell {
            continue;
        }
        last_cell = cell;
        for &k in w.buckets.get(cell.0, cell.1) {
            let k = k as usize;
            let o = &w.units[k];
            if k == i || !o.living() || o.building {
                continue;
            }
            let (ox, oy) = o.pos.pixels();
            let r_dist = norma(ox - xs, oy - ys);
            let h = pz - height(w, o.pos);
            let friend = o.owner == owner;
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
    let Some(b) = best else { return };
    let (bx, by) = w.units[b].pos.pixels();
    let a = &t.attacks[att];
    let damage = half_life(a.damage, norma(bx - xs, by - ys), a.damage_dec);
    if t.avoid_friendly_fire && w.units[b].owner == owner {
        return; // the friend absorbs the ball unharmed
    }
    make_damage(w, b, damage, Some(i), att as u8);
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
    let mut dmg = dmg;
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
    dmg -= vt.shield;
    if let Some(s) = sender {
        let st = catalog.get(w.units[s].kind);
        let kind = st.attacks.get(usize::from(att)).map_or("MECH", |a| a.kind.as_str());
        let wf = catalog.weapon_flags(kind);
        if wf & GARRISON != 0 && vt.building {
            return; // only a garrison would be hurt
        }
        if wf & RANDOM_KILL != 0 && w.random.rando() < 1310 && !vt.building && !vt.no25 {
            die(w, victim);
            w.units[s].kills = w.units[s].kills.saturating_add(1);
            return;
        }
        dmg -= vt.protection.iter().find(|p| p.0 == kind).map_or(0, |p| p.1);
        if wf & FORMATION != 0 && w.units[s].attack_bonus != 0 {
            dmg += formation_scaled(w, s, victim, w.units[s].attack_bonus);
        }
        if w.units[victim].defence_bonus != 0 {
            dmg -= formation_scaled(w, victim, s, w.units[victim].defence_bonus);
        }
    }
    let dmg = dmg.max(1);
    if w.units[victim].life > dmg {
        w.units[victim].life -= dmg;
        if let Some(s) = sender {
            attack_obj(w, victim, s, AUTO_PRIO);
        }
    } else {
        w.units[victim].life = 0;
        if let Some(s) = sender {
            w.units[s].kills = w.units[s].kills.saturating_add(1);
        }
        die(w, victim);
    }
}

/// `Die` (0x4e77a0): the unit turns a little, frees its cell and starts dying.
pub fn die(w: &mut World, i: usize) {
    let u = &mut w.units[i];
    if u.death != 0 || !u.alive {
        return;
    }
    let turn = (w.random.rando() >> 8) - 64;
    w.random.rando();
    u.dir = u.dir.wrapping_add(turn as u8);
    u.death = 1;
    u.life = 0;
    u.orders.clear();
    u.path.clear();
    u.in_motion = false;
    u.dest = None;
    if !u.building {
        w.occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
    }
}

/// `SearchVictim` (0x52622b) for every unit: on about one tick in four (`rando() > 0x2000`
/// skips) a unit scans the 128 px cells of a circle `SEARCH_ENEMY_RADIUS` wide centred
/// `_SHIFT` ahead of it and attacks the nearest enemy found (priority 1).
pub fn search_victims(w: &mut World) {
    let catalog = w.catalog.clone();
    for i in 0..w.units.len() {
        let u = &w.units[i];
        if !u.active() || u.building {
            continue;
        }
        let t = catalog.get(u.kind);
        if t.search_radius == 0 {
            continue;
        }
        if w.random.rando() > 0x2000 {
            continue;
        }
        let (sr, ss) = (t.search_radius, t.search_shift);
        let c = Pos::new(u.pos.x + ((cos(u.dir) * ss) >> 8), u.pos.y + ((sin(u.dir) * ss) >> 8));
        let min_r = min_range(t) << 4;
        let kill_mask = t.attacks.iter().filter(|a| a.usable()).fold(0, |m, a| m | a.mask);
        let rx = (sr >> 11) + 1;
        let (ccx, ccy) = (c.x >> 11, c.y >> 11);
        let mut best = (i32::MAX, None);
        for cy in ccy - rx..=ccy + rx {
            for cx in ccx - rx..=ccx + rx {
                let found = w.buckets.get(cx, cy).iter().map(|&k| k as usize).find(|&k| {
                    let o = &w.units[k];
                    o.active() && o.owner != u.owner && catalog.get(o.kind).materials & kill_mask != 0
                });
                if let Some(k) = found {
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
            u.death = u.death.saturating_add(1);
            continue;
        }
        crate::economy::starve(w, i);
        let u = &mut w.units[i];
        if u.death != 0 {
            continue;
        }
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
