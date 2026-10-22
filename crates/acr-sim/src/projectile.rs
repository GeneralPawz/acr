//! Projectiles ("AnmObjects", 8192 slots): `Create3DAnmObjectEX` (0x558360), the flight
//! (`ProcessExpl` 0x557fa0, once per tick after the units), explosions
//! (`ExplodeAnmObject` 0x5578f0) and round damage (`MakeRoundDamage` 0x5253ae). See
//! `docs/simulation.md`.
//!
//! Every object is simulated, also smoke, debris and stuck arrows: each draws random numbers
//! and takes a slot, which decides where later objects go and so the processing order.
//! Positions and velocities are pixels << 14.

use crate::fixed::Pos;
use crate::trig::norma;
use crate::weapon::{Propagation, media};
use crate::world::{EntityId, World};

/// The engine's degrees-to-radians factor of `ANGLE` weapons (0x645540, not quite π/180).
pub const DEG_TO_RAD: f64 = 0.017_452_777_777_777_78;

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
    /// The men a low cannonball has ploughed through (only with
    /// [`crate::Rules::hit_locations`]; hashes to nothing while empty).
    pub plough: Plough,
}

/// Bodies a ball can plough through with [`crate::Rules::hit_locations`] on (#256).
pub const PLOUGH_BODIES: u8 = 5;
/// Share of the ball's damage, in percent, that each body takes out of it: the bodies get
/// 100, 80, 60, 40 and 20 % of the damage, then the ball is spent.
pub const PLOUGH_LOSS: i32 = 20;

/// The men a ball low enough to hurt (`DAMAGEHEIGHT`) has already hit, with
/// [`crate::Rules::hit_locations`] on: each is hit once, each costs the ball
/// [`PLOUGH_LOSS`] % of its damage, and after [`PLOUGH_BODIES`] the ball is spent. Not in
/// the original game, where a low ball hurts everything near it every tick at full damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Plough {
    /// Unit indices hit so far (the first `n`).
    pub victims: [u32; PLOUGH_BODIES as usize],
    pub n: u8,
}

impl std::hash::Hash for Plough {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if self.n != 0 {
            (self.victims, self.n).hash(state);
        }
    }
}

impl Plough {
    /// The ball has no energy left to hurt anyone.
    pub fn spent(&self) -> bool {
        self.n >= PLOUGH_BODIES
    }

    fn hit(&self, k: usize) -> bool {
        self.victims[..usize::from(self.n)].contains(&(k as u32))
    }

    /// Records the next body and returns the damage it takes (`None`: already hit, or the
    /// ball is spent).
    fn take(&mut self, k: usize, damage: i32) -> Option<i32> {
        if self.spent() || self.hit(k) {
            return None;
        }
        let dmg = damage * (100 - PLOUGH_LOSS * i32::from(self.n)) / 100;
        self.victims[usize::from(self.n)] = k as u32;
        self.n += 1;
        Some(dmg)
    }
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

/// A corpse thrown by a `DETONATOR` blast (an entry of 0x28b4730): position and velocity in
/// 1/256 px; the height is above the ground of the map, not of the corpse's spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Thrown {
    pub unit: EntityId,
    pub pos: (i32, i32, i32),
    pub vel: (i32, i32, i32),
}

/// At most this many corpses fly at once (0x559e30).
pub const THROWN_MAX: usize = 128;

/// The end of `Die` (0x4e86b9 → 0x55a070): a land unit (not a building) killed while a
/// `DETONATOR` blast deals its damage flies away from the blast point `(bx, by)` with force
/// `d`: at distance `r` (pixels) and `c = r + 20`, `vz = 20·d/c` and `vx = (20·d·dx/r)/c`
/// (likewise `vy`), from 1 px above the ground (0x559e30; not at the blast point itself,
/// not twice, at most [`THROWN_MAX`]). No random numbers.
pub fn throw_corpse(w: &mut World, i: usize) {
    let Some((bx, by, d)) = w.detonation else { return };
    let u = &w.units[i];
    if u.building || w.catalog.get(u.kind).medium != crate::catalog::Medium::Land {
        return;
    }
    let (px, py) = u.pos.pixels();
    let (dx, dy) = (px - bx, py - by);
    let r = isqrt(i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)) as i32;
    if r == 0 || w.thrown.len() >= THROWN_MAX || w.thrown.iter().any(|t| t.unit == u.id) {
        return;
    }
    let c = r + 20;
    let vel = (dx * d * 20 / r / c, dy * d * 20 / r / c, d * 20 / c);
    let z = (h(w, px, py) + 1) << 8;
    let pos = (u.pos.x << 4, u.pos.y << 4, z);
    w.thrown.push(Thrown { unit: u.id, pos, vel });
}

/// The flight of the thrown corpses (0x559f20, at the start of `ProcessExpl`): each moves by
/// its velocity, then falls (`vz −= 1000`); the corpse is put where it flies and is done when
/// the ground there reaches its height. A corpse that is gone is dropped. The cells and the
/// lock map are not touched.
pub fn fly_corpses(w: &mut World) {
    let mut k = 0;
    while k < w.thrown.len() {
        let mut t = w.thrown[k];
        let i = t.unit.0 as usize;
        if !w.units.get(i).is_some_and(|u| u.alive) {
            w.thrown.remove(k);
            continue;
        }
        t.pos = (t.pos.0 + t.vel.0, t.pos.1 + t.vel.1, t.pos.2 + t.vel.2);
        t.vel.2 -= 1000;
        let ground = h(w, t.pos.0 >> 8, t.pos.1 >> 8);
        w.units[i].pos = Pos::new(t.pos.0 >> 4, t.pos.1 >> 4);
        if ground >= t.pos.2 >> 8 {
            w.thrown.remove(k);
        } else {
            w.thrown[k] = t;
            k += 1;
        }
    }
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
        // A bounce goes on with what is left of the ball (only set with the rule on).
        plough: if wp.damage_height != 0 { w.plough_carry.unwrap_or_default() } else { Plough::default() },
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
                    // 0x558b09: from the object's own (starting) point.
                    if wp.detonator != 0 {
                        w.detonation = Some((xs, ys, wp.detonator));
                    }
                    crate::combat::make_damage(w, b, dmg, Some(si), att);
                    w.detonation = None;
                } else if !hit && let Some(si) = sender {
                    // A missed shot strikes the ground at its end (render only).
                    let cause = crate::combat::cause_of_attack(w, si, att);
                    crate::combat::record_impact(w, (hit_at.0, hit_at.1), (dx, dy), cause);
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
            let tan = libm::tan(f64::from(wp.speed) * DEG_TO_RAD);
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
    round_damage_with(w, x, y, r, damage, sender, att, None);
}

/// [`round_damage`], or with a [`Plough`] (the hit-locations rule) each object is hit once
/// and takes what is left of the ball's damage.
#[allow(clippy::too_many_arguments)]
fn round_damage_with(
    w: &mut World,
    x: i32,
    y: i32,
    r: i32,
    damage: i32,
    sender: Option<usize>,
    att: u8,
    mut plough: Option<&mut Plough>,
) {
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
                let dmg = match plough.as_deref_mut() {
                    None => damage,
                    Some(_) if !u.living() || sender == Some(k) => continue,
                    Some(pl) => match pl.take(k, damage) {
                        Some(dmg) => dmg,
                        None => continue,
                    },
                };
                crate::combat::make_damage(w, k, dmg, sender, att);
            }
        }
    }
}

/// The damage a low ball does in one place: [`round_damage`] and the target object, or with
/// the hit-locations rule ploughing ([`Plough`]) through each body once, losing energy.
fn ball_damage(
    w: &mut World,
    slot: usize,
    p: &Projectile,
    r: i32,
    sender: Option<usize>,
    target_blast: Option<(i32, i32, i32)>,
) {
    let mut plough = p.plough;
    let ploughing = w.rules.hit_locations && w.catalog.weapons.get(p.weapon).damage_height != 0;
    if ploughing && plough.spent() {
        w.projectiles.media = -1;
        return;
    }
    let mut pl = ploughing.then_some(&mut plough);
    round_damage_with(w, p.pos.0 >> 10, p.pos.1 >> 10, r, p.damage, sender, p.att, pl.as_deref_mut());
    if let Some(b) = p.dest_obj.map(|e| e.0 as usize).filter(|&b| w.units[b].living()) {
        w.projectiles.media = w.catalog.get(w.units[b].kind).expl_media.0;
        let dmg = match pl {
            None => Some(p.damage),
            Some(pl) => pl.take(b, p.damage),
        };
        if let Some(dmg) = dmg {
            // A DETONATOR armed for the target's damage only (`target_blast`).
            if target_blast.is_some() {
                w.detonation = target_blast;
            }
            crate::combat::make_damage(w, b, dmg, sender, p.att);
            if target_blast.is_some() {
                w.detonation = None;
            }
        }
    }
    if ploughing && let Some(Some(q)) = w.projectiles.slots.get_mut(slot) {
        q.plough = plough;
    }
}

/// The flight direction of a round on the ground plane (world pixels, not normalised).
fn flight_dir(p: &Projectile) -> (i32, i32) {
    (p.vel.0 >> 8, p.vel.1 >> 8)
}

/// Notes a damaging round striking the ground for the renderer (`World::impacts`):
/// cannonballs and shells always (they plough and burst whether or not they hit someone),
/// other rounds only when they hit no one (`hit`).
fn note_impact(w: &mut World, p: &Projectile, hit: bool) {
    use crate::combat::DeathCause;
    let Some(s) = p.sender.map(|e| e.0 as usize) else { return };
    let cause = crate::combat::cause_of_attack(w, s, p.att);
    let always = matches!(cause, DeathCause::Cannonball | DeathCause::Explosion);
    if always || (!hit && cause != DeathCause::Unknown && cause != DeathCause::Melee) {
        crate::combat::record_impact(w, (p.pos.0 >> 14, p.pos.1 >> 14), flight_dir(p), cause);
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
    // A DETONATOR blast throws whoever it kills (0x557909), until the end of the explosion.
    if wp.detonator != 0 {
        w.detonation = Some((p.pos.0 >> 14, p.pos.1 >> 14, wp.detonator));
    }
    if p.damage != 0 {
        let last_hit = w.hits.back().copied();
        w.hit_dir = Some(flight_dir(&p));
        ball_damage(w, slot, &p, wp.radius, sender, None);
        w.hit_dir = None;
        if ground {
            note_impact(w, &p, w.hits.back().copied() != last_hit);
            let at = (p.pos.0 >> 14, p.pos.1 >> 14);
            let water = w.rules.hit_locations && ground_media(w, at.0, at.1) == media::WATER;
            crate::ground_fire::shell_burst(w, sender, p.att, at, water);
        }
    }
    if w.projectiles.media == -1 {
        w.projectiles.media = ground_media(w, p.pos.0 >> 14, p.pos.1 >> 14);
    }
    // With the hit-locations rule a bounce carries on what is left of the ball.
    let outer_carry = w.plough_carry;
    w.plough_carry = (w.rules.hit_locations && wp.damage_height != 0)
        .then(|| w.projectiles.slots[slot].map_or(p.plough, |q| q.plough));
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
    w.plough_carry = outer_carry;
    w.detonation = None; // 0x557c6e
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
    if !w.thrown.is_empty() {
        fly_corpses(w);
    }
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
            w.hit_dir = Some(flight_dir(&p));
            // Only the target's damage throws here (0x558212..0x558254).
            let blast = (wp.detonator != 0).then_some((x, y, wp.detonator));
            ball_damage(w, slot, &p, wp.radius, sender, blast);
            w.hit_dir = None;
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
