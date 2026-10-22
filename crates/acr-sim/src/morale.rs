//! Morale as `Morale.cpp` in the engine does it: kills frighten the victim's comrades
//! (`Fear` 0x601960), who may panic (`TryPanic` 0x6017a0 with the chance `PanicP` 0x601400)
//! and then flee in 128 px legs away from enemies (`PanicLink` 0x600720) until they calm
//! down. The constants come from `MORALE.DAT` (loader 0x600e90). See `docs/simulation.md`.
//!
//! Panic is an order on top of the unit's order stack, and while a flight leg runs the unit
//! moves with `UnlimitedMotion` and takes no orders. Every unit keeps a courage counter
//! (OB+0x53) that kills nearby raise or lower and that drifts back to 0 every 256 ticks.

use std::sync::OnceLock;

use crate::fixed::Pos;
use crate::trig::{cos, get_dir, norma, sin};
use crate::world::{Order, World};

/// `MORALE.DAT`: the factors of the panic chance (globals 0x678408..0x678448).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoraleDef {
    /// `A1`: kills of a unit outside formations, `p·A1/(A1 + kills)`.
    pub a1: i32,
    /// `HF1`, `HF2`: hunger, `p + HF1·p/(dt² + HF2)` after a unit starved.
    pub hf1: i32,
    pub hf2: i32,
    /// `CF1`, `CF2`: a lost command centre, the same form.
    pub cf1: i32,
    pub cf2: i32,
    /// `OFN1`, `DRF1`, `FLF1`: percent when the formation's officer, drummer or flag bearer
    /// is missing.
    pub ofn1: i32,
    pub drf1: i32,
    pub flf1: i32,
    /// `FP1`, `FKF1`: more than `FP1` % of the soldier slots empty multiplies by `FKF1` %.
    pub fp1: i32,
    pub fkf1: i32,
    /// `FB1`: formation size, `p·100/(100·(n−2)/FB1 + 100)`.
    pub fb1: i32,
    /// `FA1`: kills of the formation, `p·FA1/(FA1 + kills)`.
    pub fa1: i32,
    /// `OFA1`, `OFA2`: the officer's kills, `p·OFA1/(OFA1 + kills·OFA2)`.
    pub ofa1: i32,
    pub ofa2: i32,
    /// `CCA`, `CCR0`: near a command centre `p/((CCA−1)·exp(−(d/CCR0)²) + 1)`.
    pub cca: i32,
    pub ccr0: i32,
    /// `STEPS`: before this many legs a unit that calms down stops where it is.
    pub steps: i32,
}

impl Default for MoraleDef {
    /// The built-in values (0x678408..); the shipped file only changes `FKF1` to 500.
    fn default() -> Self {
        Self {
            a1: 1,
            hf1: 5000,
            hf2: 1000,
            cf1: 5000,
            cf2: 1000,
            ofn1: 200,
            drf1: 200,
            flf1: 300,
            fp1: 30,
            fkf1: 200,
            fb1: 15,
            fa1: 5,
            ofa1: 1,
            ofa2: 5,
            cca: 5,
            ccr0: 1000,
            steps: 3,
        }
    }
}

impl MoraleDef {
    /// Reads `MORALE.DAT`: `KEY value` lines, `/` lines are comments (0x600e90). Unknown keys
    /// are skipped (the engine shows a message box).
    pub fn parse(data: &[u8]) -> Self {
        let mut m = Self::default();
        for line in String::from_utf8_lossy(data).lines() {
            if line.starts_with('/') {
                continue;
            }
            let mut it = line.split_whitespace();
            let (Some(k), Some(v)) = (it.next(), it.next().and_then(|v| v.parse::<i32>().ok())) else { continue };
            let field = match k {
                "A1" => &mut m.a1,
                "HF1" => &mut m.hf1,
                "HF2" => &mut m.hf2,
                "CF1" => &mut m.cf1,
                "CF2" => &mut m.cf2,
                "OFN1" => &mut m.ofn1,
                "DRF1" => &mut m.drf1,
                "FLF1" => &mut m.flf1,
                "FP1" => &mut m.fp1,
                "FKF1" => &mut m.fkf1,
                "FB1" => &mut m.fb1,
                "FA1" => &mut m.fa1,
                "OFA1" => &mut m.ofa1,
                "OFA2" => &mut m.ofa2,
                "CCA" => &mut m.cca,
                "CCR0" => &mut m.ccr0,
                "STEPS" => &mut m.steps,
                _ => continue,
            };
            *field = v;
        }
        m
    }
}

/// Size of the panic-chance cache (0x603570): 1024 unit slots × 4 fear types.
const CACHE: usize = 4096;

/// The dynamic morale state of a match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct MoraleState {
    /// Command centres per nation (at most 16, 1/16 px), rebuilt every 256 ticks (0x601330).
    pub centers: [Vec<(i32, i32)>; 8],
    /// Panic-chance cache: tick stamp (0x28f21a8), high index bits (0x28f0d88) and the
    /// chance as u16 (0x28eed88), by `ft + (index & 0x3ff)·4`; empty until first used.
    pub stamp: Vec<u32>,
    pub hi: Vec<u8>,
    pub val: Vec<u16>,
    /// The fear type the last `FEARk` upgrade set (global 0x2778840), which the `FEAR`
    /// upgrade wrongly uses.
    pub fear_index: usize,
}

/// The engine's ring tables (0x28a6f10, built at 0x546ac0): ring `r` holds the offsets with
/// `trunc(sqrt(a² + b²)) == r`, ordered by `a`, then `b`.
pub fn ring(r: usize) -> &'static [(i32, i32)] {
    const EMPTY: &[(i32, i32)] = &[];
    static RINGS: OnceLock<Vec<Vec<(i32, i32)>>> = OnceLock::new();
    let rings = RINGS.get_or_init(|| {
        let mut v = vec![Vec::new(); 90];
        for a in -90i32..90 {
            for b in -90i32..90 {
                let d = (a * a + b * b) as u32;
                let r = d.isqrt() as usize;
                if r < v.len() {
                    v[r].push((a, b));
                }
            }
        }
        v
    });
    rings.get(r).map_or(EMPTY, Vec::as_slice)
}

/// The tick counter `tmtmt` while the current step runs.
fn now(w: &World) -> u32 {
    (w.tick.0 + 1) as u32
}

/// `Fear(x, y, nat, R, ft)` (0x601960), `x, y` in pixels: every unit within `R` px of the
/// point in the 128 px cells of rings `0..=R/128+1` gains courage (an enemy of `nat`) or
/// loses it (a unit of `nat`) unless `ft` is 0; units of `nat` within their `FEARRADIUS` of
/// type `ft` may panic (`FearCell` 0x601830).
pub fn fear(w: &mut World, x: i32, y: i32, nat: u8, r: i32, ft: u8) {
    let (cx, cy) = (x >> 7, y >> 7);
    let side = w.buckets.side();
    let catalog = w.catalog.clone();
    for ri in 0..=((r >> 7) + 1) as usize {
        for &(a, b) in ring(ri) {
            let (ax, ay) = (cx + a, cy + b);
            let cell: Vec<u32> = w.buckets.get(ax, ay).to_vec();
            let n = cell.len() as i32;
            let c = (ay + 1) * side + ax + 1;
            for k in cell {
                let k = k as usize;
                let u = &w.units[k];
                if !u.living() {
                    continue;
                }
                let (ux, uy) = u.pos.pixels();
                let d = norma(x - ux, y - uy);
                if d >= r {
                    continue;
                }
                let same = u.owner == nat;
                let radius = i32::from(catalog.get(u.kind).fear.radius[usize::from(ft & 3)]);
                let u = &mut w.units[k];
                if ft != 0 {
                    u.courage = if same { u.courage.wrapping_sub(1) } else { u.courage.wrapping_add(1) };
                }
                if same && d < radius {
                    try_panic(w, k, ft, d + n + x + y + c);
                }
            }
        }
    }
}

/// `TryPanic` (0x6017a0).
fn try_panic(w: &mut World, i: usize, ft: u8, seed: i32) {
    if w.units[i].unlimited {
        return;
    }
    let p = cached_panic_p(w, i, ft);
    if p < 327 {
        return;
    }
    let u = &w.units[i];
    let (ux, uy) = u.pos.pixels();
    let v = (w.random.rando() + i32::from(u.dir) + uy + ux + i32::from(u.stand_time) + seed + i as i32) & 0x7fff;
    if v < p {
        start_panic(w, i);
    }
}

/// The panic chance through its cache (0x603570): a value is reused for 64 ticks per unit
/// slot and fear type, kept as u16.
fn cached_panic_p(w: &mut World, i: usize, ft: u8) -> i32 {
    let t = now(w);
    let k = usize::from(ft & 3) + (i & 0x3ff) * 4;
    let hi = (i >> 10) as u8;
    let m = &w.morale;
    if !m.stamp.is_empty() && m.stamp[k] != 0 && m.hi[k] == hi && t.wrapping_sub(m.stamp[k]) < 64 {
        return i32::from(m.val[k]);
    }
    let p = panic_p(w, i, ft);
    let m = &mut w.morale;
    if m.stamp.is_empty() {
        (m.stamp, m.hi, m.val) = (vec![0; CACHE], vec![0; CACHE], vec![0; CACHE]);
    }
    (m.stamp[k], m.val[k], m.hi[k]) = (t, p as u16, hi);
    p
}

/// `PanicP` (0x601400): the chance (out of 32768) that a unit panics from fear of type `ft`.
/// The engine computes it on the x87 at double precision with two float constants; the
/// operations here are the same IEEE operations in the same order.
pub fn panic_p(w: &World, i: usize, ft: u8) -> i32 {
    if ft > 3 {
        return 0;
    }
    let md = &w.catalog.morale;
    let u = &w.units[i];
    let adv = w.adv(u.owner, u.kind);
    let t = now(w);
    let pl = &w.players[usize::from(u.owner) & 7];
    let mut p = f64::from(adv.fear_factor[usize::from(ft)]) * 32.769;
    let rise = |p: f64, since: u32, f1: i32, f2: i32| {
        let dt = t.wrapping_sub(since) as i32;
        if since != 0 && dt > 0 { p + f64::from(f1) * p / f64::from(dt.wrapping_mul(dt).wrapping_add(f2)) } else { p }
    };
    p = rise(p, pl.hungry_time, md.hf1, md.hf2);
    let centers = &w.morale.centers[usize::from(u.owner) & 7];
    if !centers.is_empty() {
        p = f64::from(p as f32);
        let m = centers.iter().map(|&(x, y)| norma(x - u.pos.x, y - u.pos.y) >> 4).fold(10_000_000, i32::min);
        if m < md.ccr0 * 4 {
            let r = f64::from(m) / f64::from(md.ccr0);
            p /= f64::from(md.cca - 1) * libm::exp(-(r * r)) + 1.0;
        }
    }
    let c = i32::from(u.courage);
    if c > 5 {
        p /= f64::from(c + 1);
    }
    if c < -5 {
        p *= f64::from(1 - c);
    }
    p = rise(p, pl.cc_lost_time, md.cf1, md.cf2);
    let pct = f64::from(0.01f32);
    if let Some(b) = u.brigade.and_then(|b| w.brigades.get(usize::from(b))) {
        let valid = |m: Option<crate::world::EntityId>| {
            m.map(|e| e.0 as usize).filter(|&k| w.units.get(k).is_some_and(|x| x.living() && x.brigade == u.brigade))
        };
        let n = b.members.len() as i32;
        if valid(b.members.first().copied().flatten()).is_none() {
            p = p * f64::from(md.ofn1) * pct;
        }
        if valid(b.members.get(1).copied().flatten()).is_none() {
            p = p * f64::from(md.drf1) * pct;
        }
        if valid(b.members.get(2).copied().flatten()).is_none() {
            p = p * f64::from(md.flf1) * pct;
        }
        if n <= 2 {
            return p as i32;
        }
        let cnt = b.members[2..].iter().filter(|&&m| valid(m).is_some()).count() as i32;
        if (n - cnt - 2) * 100 / (n - 2) > md.fp1 {
            p = p * f64::from(md.fkf1) * pct;
        }
        let kills: i32 = b.members.iter().filter_map(|&m| valid(m)).map(|k| i32::from(w.units[k].kills)).sum();
        if kills != 0 {
            p = p * f64::from(md.fa1) / f64::from(md.fa1 + kills);
        }
        p = p * f64::from(100.0f32) / f64::from(100 * (cnt - 2) / md.fb1.max(1) + 100);
        if let Some(o) = valid(b.members.first().copied().flatten()) {
            p = p * f64::from(md.ofa1) / f64::from(md.ofa1 + i32::from(w.units[o].kills) * md.ofa2);
        }
        p as i32
    } else {
        (p * f64::from(md.a1) / f64::from(md.a1 + i32::from(u.kills))) as i32
    }
}

/// `StartPanic` (0x600e20): pushes the panic order (priority 0xFF) and takes the unit out
/// of its formation; a formation left with fewer than 5 others breaks up (0x600d50).
pub fn start_panic(w: &mut World, i: usize) {
    let u = &w.units[i];
    if matches!(u.orders.last(), Some(Order::Panic { .. })) || u.unlimited {
        return;
    }
    let dir = u.dir;
    if let Some(b) = u.brigade.map(usize::from) {
        let id = u.id;
        let br = &mut w.brigades[b];
        for m in br.members.iter_mut() {
            if *m == Some(id) {
                *m = None;
            }
        }
        let others = br
            .members
            .iter()
            .flatten()
            .filter(|e| w.units.get(e.0 as usize).is_some_and(|x| x.living() && x.brigade == Some(b as u16)))
            .count();
        let u = &mut w.units[i];
        (u.brigade, u.attack_bonus, u.defence_bonus) = (None, 0, 0);
        if others < 5 {
            crate::formation::dissolve(w, b);
        }
    }
    let u = &mut w.units[i];
    u.orders.push(Order::Panic { step: 0, dir });
    u.prio = 0xFF;
}

/// `Danger` (0x600670), in 16 px units: `1000/(d + 50)` for every enemy unit in the 128 px
/// cells of rings 0..=6 around `(tx, ty)`, `20000/(d + 50)` for enemy buildings with a
/// garrison.
fn danger(w: &World, tx: i32, ty: i32, mask: u8) -> i32 {
    let (cx, cy) = (tx >> 3, ty >> 3);
    let mut sum = 0;
    for r in 0..=6 {
        for &(a, b) in ring(r) {
            for &k in w.buckets.get(cx + a, cy + b) {
                let e = &w.units[k as usize];
                if e.nmask & mask == 0 {
                    sum += 1000 / (norma(tx - (e.pos.x >> 8), ty - (e.pos.y >> 8)) + 50);
                }
            }
        }
    }
    for e in w.units.iter().filter(|e| e.building && e.living() && !e.inside.is_empty() && e.nmask & mask == 0) {
        let (ex, ey) = (e.pos.x >> 8, e.pos.y >> 8);
        if ((ex >> 3) - cx).pow(2) + ((ey >> 3) - cy).pow(2) < 49 {
            sum += 20000 / (norma(tx - ex, ty - ey) + 50);
        }
    }
    sum
}

/// `PanicLink` (0x600720), while the panic order is on top: each call starts a flight leg of
/// 128 px to the least dangerous free spot (a move with `UnlimitedMotion`), or ends the panic
/// with chance `PSIXOZ/1024`: in place before leg `STEPS`, afterwards at the nearest own
/// command centre. The second leg frightens the unit's comrades (fear type 0).
pub fn panic_link(w: &mut World, i: usize) {
    let Some(&Order::Panic { step, dir }) = w.units[i].orders.last() else { return };
    let catalog = w.catalog.clone();
    let t = catalog.get(w.units[i].kind);
    {
        let World { units, occupancy, .. } = w;
        let u = &mut units[i];
        u.dest = None;
        crate::economy::settle(u, occupancy);
    }
    let step = step.wrapping_add(1);
    if let Some(Order::Panic { step: s, .. }) = w.units[i].orders.last_mut() {
        *s = step;
    }
    if step == 2 {
        let (x, y) = w.units[i].pos.pixels();
        fear(w, x, y, w.units[i].owner, 256, 0);
    }
    if w.random.rando() < t.fear.psixoz << 5 {
        if t.peasant || i32::from(step) <= catalog.morale.steps {
            w.units[i].pop_order();
            return;
        }
        let u = &mut w.units[i];
        u.orders.clear();
        u.path.clear();
        u.prio = 0;
        let (owner, pos) = (u.owner, u.pos);
        let mut best: Option<(i32, Pos)> = None;
        for c in w.units.iter().filter(|c| c.living() && c.owner == owner && catalog.get(c.kind).command_center) {
            let d = norma(c.pos.x - pos.x, c.pos.y - pos.y);
            if best.is_none_or(|b| d < b.0) {
                let p = match c.rally {
                    Some(r) if r.x > 0 && (norma(r.x - c.pos.x, r.y - c.pos.y) & !0xF) < 0x4000 => r,
                    _ => c.pos,
                };
                best = Some((d, p));
            }
        }
        let Some((_, p)) = best else { return };
        let ry = w.random.rando();
        let rx = w.random.rando();
        let (cx, cy) = p.pixels();
        let areas = w.map.land_areas.clone();
        crate::motion::smart_send_to(
            &mut w.units[i],
            areas.as_ref(),
            cx + (rx & 0x7f) - 64,
            cy + (ry & 0x7f) - 64,
            0,
            0,
            0x90,
        );
        return;
    }
    let (x, y) = w.units[i].cell;
    let mask = w.units[i].nmask;
    let (mut best, mut max_d, mut target) = (1_000_000, 0, None);
    for &(a, b) in ring(4) {
        let (tx, ty) = (x + 2 * a, y + 2 * b);
        if w.map.land.check_bar(tx - 1, ty - 1, 3, 3) {
            continue;
        }
        let d = danger(w, tx, ty, mask);
        let v = d + (w.random.rando() & 7);
        if v < best {
            (best, target) = (v, Some((tx, ty)));
        }
        max_d = max_d.max(d);
    }
    let mut new_dir = dir;
    if max_d == 0 {
        let (tx, ty) = (x + (cos(dir) >> 5), y + (sin(dir) >> 5));
        if !w.map.land.check_bar(tx - 1, ty - 1, 3, 3) {
            target = Some((tx, ty));
        } else if let Some((bx, by)) = target {
            new_dir = get_dir(bx - x, by - y);
        }
    } else if let Some((bx, by)) = target {
        new_dir = get_dir(bx - x, by - y);
    }
    let Some((bx, by)) = target else {
        w.units[i].pop_order();
        return;
    };
    let World { units, occupancy, shouts, .. } = w;
    let u = &mut units[i];
    if let Some(Order::Panic { dir, .. }) = u.orders.last_mut() {
        *dir = new_dir;
    }
    u.prio = 0;
    crate::motion::send_to(u, Pos::new(bx << 8, by << 8), 0x80, false);
    occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
    u.unlimited = true;
    // Every leg the unit shouts one of `FEAR_RNDM0..7` (0x600bc8, shown by the interface).
    shouts.push(u.id);
}

/// 0x601a20, at the start of a step whose counter before the increment ends in 0x3d (once
/// every 256 ticks): courage drifts one step towards 0 and the command centre table is
/// rebuilt (0x601330).
pub fn decay(w: &mut World) {
    if w.tick.0 & 0xFF != 0x3d {
        return;
    }
    for u in w.units.iter_mut().filter(|u| u.alive) {
        u.courage -= u.courage.signum();
    }
    let catalog = w.catalog.clone();
    let mut centers: [Vec<(i32, i32)>; 8] = Default::default();
    for u in w.units.iter().filter(|u| u.living() && catalog.get(u.kind).command_center) {
        let c = &mut centers[usize::from(u.owner) & 7];
        if c.len() < 16 {
            c.push((u.pos.x, u.pos.y));
        }
    }
    w.morale.centers = centers;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_are_euclidean() {
        assert_eq!(ring(0), [(0, 0)]);
        assert_eq!(ring(1).len(), 8);
        assert_eq!(ring(1)[0], (-1, -1));
        assert!(ring(3).iter().all(|&(a, b)| (9..16).contains(&(a * a + b * b))));
    }

    #[test]
    fn parses_the_shipped_file() {
        let m = MoraleDef::parse(b"//skill\r\nA1 1\r\nFKF1 500\r\n//x\r\nSTEPS 3");
        assert_eq!((m.a1, m.fkf1, m.steps, m.ofa2), (1, 500, 3, 5));
    }
}
