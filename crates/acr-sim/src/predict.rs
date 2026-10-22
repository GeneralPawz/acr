//! Looking along a shot before it is fired (`PredictShot` 0x559030 and its friendly-fire
//! wrapper 0x559a40): what the projectile would hit first on its way to the aim point. The
//! musket's strike only starts with a free line, and a unit that is blocked steps aside
//! (see [`crate::combat`] and `docs/simulation.md`).

use crate::trig::norma;
use crate::weapon::{Propagation, Weapon};
use crate::world::World;

/// What is in the way of a shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// 0xFFFF: the ground, or the shooter's own building box.
    Ground,
    /// 0xFFFE: an `ANGLE` weapon whose arc cannot reach the aim point's height.
    Unreachable,
    /// An object: a building whose box is in the way, or the nearest unit on the line of a
    /// `FRIENDLYFIRE` shooter when it is a friend.
    Object(usize),
}

/// The answer of `PredictShot`: what blocks the shot (`None`: the line is free), and the
/// engine's global 0x28a8018, set for a straight shot whose first step covers less than
/// 200 px (`|dx| + |dy| + |dz|`): a close enemy in the way does not stop it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Prediction {
    pub blocked: Option<Blocked>,
    pub close: bool,
}

/// The weapon whose flight is predicted (0x558e20): the weapon itself if it flies straight
/// (`FLY`, `IMMEDIATE`, `ANGLE`), else its first such child, else its first such
/// `[SYNCWEAPON]`. The musket's `SHOTFOGFORPISTOL` (smoke, `STAND`) has the ball `ENDSHOT`
/// as its child.
fn flying(w: &World, wid: u16) -> Option<&Weapon> {
    let ws = &w.catalog.weapons;
    let flies = |id: u16| {
        ws.list
            .get(usize::from(id))
            .filter(|p| matches!(p.propagation, Propagation::Fly | Propagation::Immediate | Propagation::Angle))
    };
    let wp = ws.list.get(usize::from(wid))?;
    flies(wid)
        .or_else(|| wp.children.list.iter().take(wp.children.count.max(0) as usize).find_map(|&c| flies(c)))
        .or_else(|| wp.sync.iter().find_map(|&c| flies(c)))
}

/// One point of the line (0x558eb0, pixels): free when the ground (45 px lower under boxes
/// below 20 px) plus the building box there stays at or below `z`; else a box of another
/// object blocks as that object, the ground or the sender's own box as `Ground` (the own box
/// only if the lowered ground still reaches `z`).
fn point(w: &World, x: i32, y: i32, z: i32, sender: usize) -> Option<Blocked> {
    let bh = w.bar_height(x, y);
    let mut g = w.map.heights.at(x, y);
    if bh < 20 {
        g -= 45;
    }
    if g + bh <= z {
        return None;
    }
    if bh == 0 {
        return Some(Blocked::Ground);
    }
    match w.bar_owner(x, y) {
        Some(o) if o != sender => Some(Blocked::Object(o)),
        _ if g < z => None,
        _ => Some(Blocked::Ground),
    }
}

/// A straight piece of the flight (0x558f30, pixels): `Norma/16 + 1` points from `s`
/// towards `d` (the end itself is not tested), in steps of `(d − s)·2^14 / n`.
fn segment(w: &World, s: (i32, i32, i32), d: (i32, i32, i32), sender: usize) -> Option<Blocked> {
    let (dx, dy, dz) = (d.0 - s.0, d.1 - s.1, d.2 - s.2);
    let n = norma(dx, dy);
    if n == 0 {
        return None;
    }
    let n = i64::from((n >> 4) + 1);
    let step = |v: i32| (i64::from(v) << 14) / n;
    let (sx, sy, sz) = (step(dx), step(dy), step(dz));
    let (mut x, mut y, mut z) = (i64::from(s.0) << 14, i64::from(s.1) << 14, i64::from(s.2) << 14);
    for _ in 0..n {
        if let Some(b) = point(w, (x >> 14) as i32, (y >> 14) as i32, (z >> 14) as i32, sender) {
            return Some(b);
        }
        x += sx;
        y += sy;
        z += sz;
    }
    None
}

/// `PredictShot` (0x559030): follows weapon `wid`'s flight from `s` to `d` (pixels) through
/// terrain and building boxes, the way `Create3DAnmObject` would launch it. Only straight
/// shots are followed: `FLY` weapons fast enough to arrive in the first tick (the musket ball,
/// a hitscan) and `IMMEDIATE` ones; an `ANGLE` arc only reports a height it cannot reach;
/// slower and other weapons are never blocked. `wid = None` (catalogs without the weapon
/// table) is the musket's instant line. Units are not looked at.
pub fn predict_shot(w: &World, wid: Option<u16>, s: (i32, i32, i32), d: (i32, i32, i32), sender: usize) -> Prediction {
    let free = Prediction::default();
    let wp = match wid {
        Some(id) => match flying(w, id) {
            Some(wp) => Some(wp),
            None => return free,
        },
        None => None,
    };
    let (dx, dy, dz) = (i64::from(d.0 - s.0), i64::from(d.1 - s.1), i64::from(d.2 - s.2));
    let az = -8000 * i64::from(wp.map_or(0, |p| p.gravity));
    let (vx, vy, vz) = match wp.map(|p| p.propagation) {
        Some(Propagation::Angle) => {
            // 0x5590d4: the arc of `Speed` degrees cannot rise to the aim point.
            let wp = wp.expect("matched");
            let dist = isqrt(dx * dx + dy * dy);
            let tan = libm::tan(f64::from(wp.speed) * crate::projectile::DEG_TO_RAD);
            let hgt = (dist as f64 * tan) as i64;
            return if dz - hgt >= 0 { Prediction { blocked: Some(Blocked::Unreachable), close: false } } else { free };
        }
        Some(Propagation::Immediate) => (dx << 14, dy << 14, dz << 14),
        Some(Propagation::Fly) | None => {
            let speed = i64::from(wp.map_or(0, |p| p.speed) << 1);
            let dist = isqrt(dx * dx + dy * dy);
            let t = if speed == 0 { 0 } else { dist / speed };
            if t != 0 {
                return free; // a slow projectile is not followed
            }
            (dx << 14, dy << 14, (dz << 14) - ((2 * az) >> 1))
        }
        _ => return free,
    };
    let close = (vz >> 14).abs() + (vy >> 14).abs() + (vx >> 14).abs() < 200;
    let (mut x, mut y, mut z) = (i64::from(s.0) << 14, i64::from(s.1) << 14, i64::from(s.2) << 14);
    let (tx, ty) = (i64::from(d.0) << 14, i64::from(d.1) << 14);
    let mut vz = vz;
    // The engine flies until within 4 px of the aim point; straight shots get there in one
    // step (the cap only guards against a weapon that never arrives).
    for _ in 0..1024 {
        vz += az;
        let (nx, ny, nz) = (x + vx, y + vy, z + vz);
        let px = |v: i64| (v >> 14) as i32;
        if let Some(b) = segment(w, (px(x), px(y), px(z)), (px(nx), px(ny), px(nz)), sender) {
            return Prediction { blocked: Some(b), close };
        }
        (x, y, z) = (nx, ny, nz);
        if norma64(x - tx, y - ty) <= 0x10000 {
            break;
        }
    }
    Prediction { blocked: None, close }
}

/// The friendly-fire look along a shot (0x559770, pixels): the nearest living unit within
/// its `UNITRADIUS` of the line from `s` through `d` and out to twice the distance (3/4 of
/// the distance from the line counts, doubled for friends closer than 250 px; friends only
/// beyond 150 px, others beyond 60), 0..90 px below the line, in the 128 px cells the line
/// crosses. It is returned only if it is a friend (`nmask`).
pub fn friend_in_line(w: &World, s: (i32, i32, i32), d: (i32, i32, i32), nmask: u8, sender: usize) -> Option<usize> {
    let (xs, ys, zs) = s;
    let (dx, dy, dz) = (d.0 - xs, d.1 - ys, d.2 - zs);
    let len = isqrt(i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy) + i64::from(dz) * i64::from(dz));
    if len == 0 {
        return None;
    }
    let n = (len >> 4) + 1;
    let (mut best, mut min_r) = (None, 10_000);
    let mut last_cell = (-1, -1);
    for st in 0..2 * n {
        let (xx, yy, zz) = (i64::from(dx) * st / n, i64::from(dy) * st / n, i64::from(dz) * st / n);
        let (px, py, pz) = (xs + xx as i32, ys + yy as i32, zs + zz as i32);
        let cell = (px >> 7, py >> 7);
        if cell == last_cell {
            continue;
        }
        last_cell = cell;
        for k in w.buckets.all(cell.0, cell.1) {
            let k = k as usize;
            let o = &w.units[k];
            if k == sender || !o.living() {
                continue;
            }
            let (ox, oy) = o.pos.pixels();
            let r_dist = norma(ox - xs, oy - ys);
            let h = pz - w.map.heights.at(ox, oy);
            let friend = o.nmask & nmask != 0;
            let min_dist = if friend { 150 } else { 60 };
            if h > 0 && h < 90 && r_dist > min_dist && r_dist < min_r {
                let cross = (i64::from(ox - xs) * yy - i64::from(oy - ys) * xx).abs();
                let mut r = cross / len;
                r -= r >> 2;
                if friend && r_dist < 250 {
                    r *= 2;
                }
                if r < i64::from(w.catalog.get(o.kind).unit_radius) {
                    best = Some(k);
                    min_r = r_dist;
                }
            }
        }
    }
    best.filter(|&b| w.units[b].nmask & nmask != 0)
}

/// 0x559a40: `PredictShot` for unit `sender`; a `FRIENDLYFIRE` shooter whose avoidance is
/// on (not switched off with the friendly-fire command) first looks for a friend in the
/// line ([`friend_in_line`]), which then blocks.
pub fn predict_shot_ff(
    w: &World,
    wid: Option<u16>,
    s: (i32, i32, i32),
    d: (i32, i32, i32),
    sender: usize,
) -> Prediction {
    let u = &w.units[sender];
    if w.catalog.get(u.kind).avoid_friendly_fire
        && !u.no_ff_avoid
        && let Some(f) = friend_in_line(w, s, d, u.nmask, sender)
    {
        return Prediction { blocked: Some(Blocked::Object(f)), close: false };
    }
    predict_shot(w, wid, s, d, sender)
}

fn isqrt(v: i64) -> i64 {
    if v <= 0 { 0 } else { (v as u64).isqrt() as i64 }
}

/// `Norma` on 64-bit values (the flight works in px << 14).
fn norma64(dx: i64, dy: i64) -> i64 {
    let (a, b) = (dx.abs(), dy.abs());
    (a.max(b) + a + b) >> 1
}
