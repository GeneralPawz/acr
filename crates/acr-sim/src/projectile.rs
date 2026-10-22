//! Projectiles ("AnmObjects", 8192 slots): `Create3DAnmObjectEX` (0x558360), the flight
//! (`ProcessExpl` 0x557fa0, once per tick after the units), explosions
//! (`ExplodeAnmObject` 0x5578f0) and round damage (`MakeRoundDamage` 0x5253ae). See
//! `docs/simulation.md`.
//!
//! Every object is simulated, also smoke, debris and stuck arrows: each draws random numbers
//! and takes a slot, which decides where later objects go and so the processing order.
//! Positions and velocities are pixels << 14.

use crate::trig::norma;
use crate::weapon::{Propagation, media};
use crate::world::{EntityId, World};

/// Projectile slots (`0x28a8518`, 8192).
pub const SLOTS: usize = 0x2000;

/// One object in flight (`AnmObject`, 0x44 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Projectile {
    pub weapon: u16,
    /// Position and velocity (+0x04.., +0x10..), px << 14.
    pub pos: (i32, i32, i32),
    pub vel: (i32, i32, i32),
    /// Vertical acceleration (+0x1c).
    pub az: i32,
    /// Destination (+0x20..), px << 14.
    pub dest: (i32, i32, i32),
    /// +0x2c: visual height of a building's shot point, passed on to children.
    pub extra: i32,
    /// Animation frame (+0x2e) and cycles left (+0x30).
    pub frame: i32,
    pub times: i32,
    /// Damage (+0x36, u16).
    pub damage: i32,
    /// The shooter (+0x38), its attack index (+0x42) and a target object (+0x3e).
    pub sender: Option<EntityId>,
    pub att: u8,
    pub dest_obj: Option<EntityId>,
}

/// A weapon sound (`[SOUND]`) started where an object was created (0x5583a2): `x` in
/// pixels and `y` the screen row `y / 2 − z`. Only for the sound output; never read back
/// by the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponSound {
    pub weapon: u16,
    pub x: i32,
    pub y: i32,
}

/// The projectile pool and the engine globals it uses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Projectiles {
    /// Slots (empty until the first object).
    pub slots: Vec<Option<Projectile>>,
    /// Ring start for the next allocation (0x28b55b6).
    pub ring: u16,
    /// The last explosion media (0x23d5fe8): only round damage resets it, so it is stale
    /// for objects without damage, as in the engine. Starts at 0.
    pub media: i32,
}

impl Projectiles {
    /// Objects in flight.
    pub fn iter(&self) -> impl Iterator<Item = &Projectile> {
        self.slots.iter().flatten()
    }

    /// Slot allocation (0x5583c1): the first free slot from the ring start, else from 0.
    fn alloc(&mut self) -> Option<usize> {
        if self.slots.is_empty() {
            self.slots = vec![None; SLOTS];
        }
        let ring = usize::from(self.ring);
        let s = (ring..SLOTS)
            .find(|&s| self.slots[s].is_none())
            .or_else(|| (0..ring).find(|&s| self.slots[s].is_none()))?;
        self.ring = ((s + 1) & (SLOTS - 1)) as u16;
        Some(s)
    }
}

fn h(w: &World, x: i32, y: i32) -> i32 {
    w.map.heights.at(x, y)
}

fn isqrt(v: i64) -> i64 {
    if v <= 0 { 0 } else { (v as u64).isqrt() as i64 }
}

/// C `div()`: truncation toward zero (what Rust's `/` does).
fn div(a: i64, b: i64) -> i32 {
    if b == 0 { 0 } else { (a / b) as i32 }
}

/// `Create3DAnmObjectEX` (0x558360): a new object of weapon `wid` from `s` towards `d`
/// (pixels). `vel` is the parent's velocity (only bounces use it). Returns whether an object
/// was created (a hitscan or a steep bounce counts as created).
#[allow(clippy::too_many_arguments)]
pub fn create(
    w: &mut World,
    wid: u16,
    s: (i32, i32, i32),
    d: (i32, i32, i32),
    sender: Option<usize>,
    att: u8,
    dest_obj: Option<usize>,
    vel: (i32, i32, i32),
    extra: i32,
) -> bool {
    let catalog = w.catalog.clone();
    let Some(wp) = catalog.weapons.list.get(usize::from(wid)) else { return false };
    let att = if att >= 12 { 0 } else { att };
    let (xs, ys, mut zs) = s;
    if wp.sound.is_some() {
        // The sound starts first, even if no slot is free.
        w.sounds.push(WeaponSound { weapon: wid, x: xs, y: (ys >> 1) - zs });
    }
    let g = h(w, xs, ys);
    if zs < g {
        zs = g + 1;
    }
    let Some(slot) = w.projectiles.alloc() else { return false };
    w.random.rando(); // ASerial
    let damage = match sender.filter(|_| wp.full_parent) {
        Some(si) => {
            let su = &w.units[si];
            let st = catalog.get(su.kind);
            let adv = w.adv(su.owner, su.kind);
            let max = adv.attacks.get(usize::from(att)).map_or(0, |a| a.damage);
            let cap = adv.max_inside;
            if su.building && !st.shot_points.is_empty() && cap != 0 {
                // Garrisoned building: MinDamage..MaxDamage by the shooters (USAGE STRELOK) inside.
                let n = su
                    .inside
                    .iter()
                    .filter(|e| {
                        w.units.get(e.0 as usize).is_some_and(|x| x.alive && catalog.get(x.kind).role.usage == 21)
                    })
                    .count() as i32;
                let min = st.attacks.get(usize::from(att)).map_or(0, |a| a.min_damage);
                min + (max - min) * n / cap
            } else {
                max
            }
        }
        None => wp.damage,
    };
    let dest_obj =
        dest_obj.filter(|&k| wp.full_parent && w.units.get(k).is_some_and(|u| u.alive)).map(|k| w.units[k].id);
    let az = -8000 * wp.gravity;
    let speed = wp.speed << 1;
    let mut p = Projectile {
        weapon: wid,
        pos: (xs << 14, ys << 14, zs << 14),
        vel: (0, 0, 0),
        az,
        dest: (d.0 << 14, d.1 << 14, d.2 << 14),
        extra,
        frame: 0,
        times: wp.times,
        damage,
        sender: sender.map(|k| w.units[k].id),
        att,
        dest_obj,
    };
    let (dx, dy, dz) = (d.0 - xs, d.1 - ys, d.2 - zs);
    let mut explode_now = false;
    let mut keep = true;
    match wp.propagation {
        Propagation::Stand | Propagation::SlightUp => {
            if wp.propagation == Propagation::SlightUp {
                p.vel.2 = speed << 14;
            }
            explode_now = wp.hot_frame == 0xFF;
        }
        Propagation::Random | Propagation::Random1 => {
            let vx = (w.random.rando() - 0x4000) * speed;
            let vy = (w.random.rando() - 0x4000) * speed;
            let vz = (w.random.rando() * speed) >> 1;
            p.vel = (vx, vy, vz);
        }
        Propagation::Fly => {
            let dist = isqrt(i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy));
            let t = if speed == 0 { 0 } else { dist / i64::from(speed) };
            if t != 0 {
                p.vel = (
                    div(i64::from(dx) << 14, t + 1),
                    div(i64::from(dy) << 14, t + 1),
                    div(i64::from(dz) << 14, t + 1) - (((t as i32 + 2) * az) >> 1),
                );
            } else {
                // Hitscan (0x5589ec): a line trace now.
                let mut hit_at = (d.0, d.1, d.2);
                let hit = crate::combat::trace_line(w, (xs, ys, zs), &mut hit_at, damage, sender, att);
                if !hit
                    && let (Some(si), Some(b)) = (sender, dest_obj.map(|e| e.0 as usize))
                    && w.units[b].building
                    && w.units[b].living()
                    && w.units[b].inside_of.is_none()
                {
                    let r = (norma(hit_at.0 - xs, hit_at.1 - ys) - catalog.get(w.units[b].kind).add_shot_radius).max(0);
                    let su = &w.units[si];
                    let dec = catalog.get(su.kind).attacks.get(usize::from(att)).map_or(0xFFFF, |a| a.damage_dec);
                    let dmg = crate::combat::half_life(damage, r, dec);
                    crate::combat::make_damage(w, b, dmg, Some(si), att);
                }
                w.projectiles.slots[slot] = None;
                return true;
            }
        }
        Propagation::Immediate => {
            p.pos = (d.0 << 14, d.1 << 14, d.2 << 14);
        }
        Propagation::Angle => {
            let dist = isqrt(i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy));
            let tan = libm::tan(f64::from(wp.speed) * 0.017_452_777_777_777_78);
            let hgt = (dist as f64 * tan) as i32;
            let e = d.2 - hgt - zs;
            if e >= 0 || az == 0 {
                // The engine frees the slot without counting it (a bug without effect here).
                w.projectiles.slots[slot] = None;
                return false;
            }
            let t = (libm::sqrt(f64::from(e << 11) / f64::from(az)) * 4.0) as i64;
            if t == 0 {
                w.projectiles.slots[slot] = None;
                return false;
            }
            let (vx, vy) = (div(i64::from(dx) << 14, t), div(i64::from(dy) << 14, t));
            let v = libm::sqrt(f64::from(vx) * f64::from(vx) + f64::from(vy) * f64::from(vy)) as i64;
            p.vel = (vx, vy, (v as f64 * tan) as i32);
        }
        Propagation::Reflect => {
            let (mut a, mut b, mut c) = (vel.0 >> 16, vel.1 >> 16, vel.2 >> 16);
            let mut q = 0;
            if a | b | c != 0 {
                let ga = h(w, xs - 32, ys) - h(w, xs + 32, ys);
                let gb = h(w, xs, ys + 32) - h(w, xs, ys - 32);
                let n = isqrt(i64::from(ga * ga + gb * gb + 0x400)) as i32;
                let k = (2 * (ga * a + gb * b + 32 * c)) / n.max(1);
                let l = isqrt(i64::from(a * a + b * b + c * c)) as i32;
                a -= k * ga / n.max(1);
                b -= k * gb / n.max(1);
                c += (-k * 32) / n.max(1);
                q = if l != 0 { (k * 32) / l } else { 0 };
            }
            p.vel = ((wp.speed * a) << 10, (wp.speed * b) << 10, (wp.speed * c) << 10);
            if q.abs() > 26 {
                explode_now = true;
                keep = false;
            }
        }
    }
    w.projectiles.slots[slot] = Some(p);
    if explode_now {
        explode(w, slot, false);
        if !keep {
            w.projectiles.slots[slot] = None;
        }
    }
    true
}

/// `MakeRoundDamage` (0x5253ae), `x, y` in 1/16 px: every object in the 3 × 3 cells of 128 px
/// around the point within `r` (plus its `EXPLMEDIA`/`EXPLRADIUS` size) takes the full
/// damage; resets the explosion media, or sets it to the media of an object hit dead centre.
pub fn round_damage(w: &mut World, x: i32, y: i32, r: i32, damage: i32, sender: Option<usize>, att: u8) {
    let (cx, cy) = (x >> 11, y >> 11);
    w.projectiles.media = -1;
    let catalog = w.catalog.clone();
    for (ax, ay) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
        let cell: Vec<u32> = w.buckets.all(cx + ax, cy + ay).collect();
        for k in cell {
            let k = k as usize;
            let u = &w.units[k];
            if !u.alive {
                continue;
            }
            let t = catalog.get(u.kind);
            let size = (t.expl_media.1 + t.expl_radius) << 4;
            let d = norma(u.pos.x - x, u.pos.y - y);
            if d < r + size {
                if d < size {
                    w.projectiles.media = t.expl_media.0;
                }
                crate::combat::make_damage(w, k, damage, sender, att);
            }
        }
    }
}

/// A living object of the projectile's sender (the engine keeps a raw pointer).
fn sender_of(w: &World, p: &Projectile) -> Option<usize> {
    p.sender.map(|e| e.0 as usize).filter(|&k| w.units.get(k).is_some_and(|u| u.alive))
}

/// `ExplodeAnmObject` (0x5578f0): round damage (and damage to the target object), then the
/// children for the media hit and the sync weapons. `ground` is set for terrain contact.
pub fn explode(w: &mut World, slot: usize, ground: bool) {
    let Some(p) = w.projectiles.slots.get(slot).copied().flatten() else { return };
    let catalog = w.catalog.clone();
    let wp = catalog.weapons.get(p.weapon);
    let sender = sender_of(w, &p);
    if p.damage != 0 {
        round_damage(w, p.pos.0 >> 10, p.pos.1 >> 10, wp.radius, p.damage, sender, p.att);
        if let Some(b) = p.dest_obj.map(|e| e.0 as usize).filter(|&b| w.units[b].living()) {
            w.projectiles.media = catalog.get(w.units[b].kind).expl_media.0;
            crate::combat::make_damage(w, b, p.damage, sender, p.att);
        }
    }
    if w.projectiles.media == -1 {
        w.projectiles.media = ground_media(w, p.pos.0 >> 14, p.pos.1 >> 14);
    }
    let m = w.projectiles.media;
    let block = wp.custom.iter().rev().find(|(cm, _)| *cm == m).map(|(_, c)| c);
    let default = block.is_none();
    let block = block.unwrap_or(&wp.children);
    let n = block.min + (((block.max - block.min) * w.random.rando()) >> 15);
    let s = (p.pos.0 >> 14, p.pos.1 >> 14, p.pos.2 >> 14);
    let d = (p.dest.0 >> 14, p.dest.1 >> 14, p.dest.2 >> 14);
    let dest_obj = p.dest_obj.map(|e| e.0 as usize);
    for _ in 0..n {
        let mut tries = 20;
        let mut child;
        loop {
            child = block.list.get(((w.random.rando() * block.count) >> 15) as usize).copied();
            tries -= 1;
            let ok = ground || child.is_none_or(|c| catalog.weapons.get(c).propagation != Propagation::Reflect);
            if ok || tries == 0 {
                break;
            }
        }
        if let Some(c) = child {
            create(w, c, s, d, sender, p.att, dest_obj, p.vel, p.extra);
        }
    }
    if !wp.sync.is_empty() && default {
        for &sy in &wp.sync {
            create(w, sy, s, d, sender, p.att, dest_obj, p.vel, p.extra);
        }
    }
}

/// The media of the ground at a point (0x5578f0): water where the four `SEA2` cells around
/// the point (rows shifted by the height, as the engine's screen grid) are deep, else the
/// texture's media (`TEXTURES.LST`).
fn ground_media(w: &World, x: i32, y: i32) -> i32 {
    let map = &w.map;
    if let Some(depth) = &map.water_depth {
        let (cx, cy) = (x >> 5, (y - 2 * h(w, x, y)) >> 5);
        let (wd, hd) = (depth.width as i32, depth.height as i32);
        if cx >= 1 && cy >= 1 && cx < wd - 1 && cy < hd - 1 {
            let v = |a: i32, b: i32| i32::from(depth.get(a as usize, b as usize).unwrap_or(0));
            let sum = v(cx, cy) + v(cx + 1, cy) + v(cx, cy + 1) + v(cx + 1, cy + 1);
            if sum & !3 > 0x200 {
                return media::WATER;
            }
        }
    }
    let Some(tex) = &map.textures else { return media::DEFAULT };
    let tx = (x + 16).div_euclid(32).clamp(0, tex.width as i32 - 1);
    let ty = (y + if tx & 1 == 1 { 0 } else { 16 }).div_euclid(32).clamp(0, tex.height as i32 - 1);
    let t = tex.get(tx as usize, ty as usize).unwrap_or(0);
    w.catalog.weapons.texture_media.get(usize::from(t)).copied().unwrap_or(media::DEFAULT)
}

/// `ProcessExpl` (0x557fa0), once per tick after the units: every object in slot order
/// leaves its trail, moves (`vz += az`, then the position), explodes near its destination
/// (aimed objects), on the ground or on a building, hurts every tick while low
/// (`DAMAGEHEIGHT`), explodes at its hot frame and dies at the end of its last animation
/// cycle.
pub fn process(w: &mut World) {
    if w.projectiles.slots.is_empty() {
        return;
    }
    let catalog = w.catalog.clone();
    for slot in 0..SLOTS {
        let Some(mut p) = w.projectiles.slots[slot] else { continue };
        let wp = catalog.weapons.get(p.weapon);
        // (a) Trail objects, stepping back from the position before the move.
        if !wp.trail.is_empty() && wp.trail_count > 0 {
            let n = wp.trail_count;
            let dv = (p.vel.0 / n, p.vel.1 / n, p.vel.2 / n);
            let mut q = p.pos;
            let d = (p.dest.0 >> 14, p.dest.1 >> 14, p.dest.2 >> 14);
            for _ in 0..n {
                let k = ((w.random.rando() * wp.trail.len() as i32) >> 15) as usize;
                if let Some(&c) = wp.trail.get(k) {
                    create(w, c, (q.0 >> 14, q.1 >> 14, q.2 >> 14), d, None, 0, None, (0, 0, 0), 0);
                }
                q = (q.0 - dv.0, q.1 - dv.1, q.2 - dv.2);
            }
        }
        // (b) Integration.
        p.vel.2 += p.az;
        p.pos = (p.pos.0 + p.vel.0, p.pos.1 + p.vel.1, p.pos.2 + p.vel.2);
        w.projectiles.slots[slot] = Some(p);
        // (c) Aimed objects explode within 16 px (L1) of the destination.
        if matches!(wp.propagation, Propagation::Fly | Propagation::Angle)
            && (p.pos.2 - p.dest.2).abs() + (p.pos.1 - p.dest.1).abs() + (p.pos.0 - p.dest.0).abs() < 0x40000
        {
            explode(w, slot, true);
            w.projectiles.slots[slot] = None;
            continue;
        }
        // (d) Contact with the ground or a building.
        let (x, y, z) = (p.pos.0 >> 14, p.pos.1 >> 14, p.pos.2 >> 14);
        let mut ground = h(w, x, y);
        let mut bar = 0;
        if matches!(
            wp.propagation,
            Propagation::Random | Propagation::Fly | Propagation::Immediate | Propagation::Angle
        ) {
            bar = w.bar_height(x, y);
            ground += bar;
        }
        let sender = sender_of(w, &p);
        if wp.damage_height != 0 && ground > z - wp.damage_height && p.damage != 0 {
            round_damage(w, p.pos.0 >> 10, p.pos.1 >> 10, wp.radius, p.damage, sender, p.att);
            if let Some(b) = p.dest_obj.map(|e| e.0 as usize).filter(|&b| w.units[b].living()) {
                w.projectiles.media = catalog.get(w.units[b].kind).expl_media.0;
                crate::combat::make_damage(w, b, p.damage, sender, p.att);
            }
        }
        if ground > z {
            if bar != 0 {
                let owner = w.bar_owner(x, y);
                if !(owner.is_some() && owner == p.sender.map(|e| e.0 as usize)) {
                    if let Some(o) = owner
                        && let Some(Some(q)) = w.projectiles.slots.get_mut(slot)
                    {
                        q.dest_obj = Some(w.units[o].id);
                    }
                    explode(w, slot, false);
                    w.projectiles.slots[slot] = None;
                    continue;
                }
            } else {
                explode(w, slot, true);
                w.projectiles.slots[slot] = None;
                continue;
            }
        }
        // (e) Hot frame and animation cycles.
        if p.times == 1 && p.frame == wp.hot_frame {
            explode(w, slot, false);
        }
        let Some(Some(q)) = w.projectiles.slots.get_mut(slot) else { continue };
        if q.frame >= wp.frames - crate::world::FRM_DEC {
            if q.times == 1 {
                w.projectiles.slots[slot] = None;
                continue;
            }
            if q.times > 0 {
                q.times -= 1;
            }
            q.frame = -1;
        }
        q.frame += 1;
    }
}
