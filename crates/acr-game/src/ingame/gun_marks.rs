//! The gun marks: what a selected cannon's fire mode covers on the ground, drawn like the
//! game's own selection marks (the yellow of the selection brackets, `GetPaletteColor(255,
//! 255, 0)`, one-pixel lines following the ground with a dark edge under them; grey for the
//! area too close to fire into). The original shows no gun ranges (`docs/ui.md`, "Gun
//! marks"):
//!
//! - **canister** (`KARTECH`, the automatic volley, men within its reach): the sector of
//!   `WEAPONINSECTOR` ahead of the gun, a dashed outline out to its reach with a dotted arc
//!   at half reach;
//! - **ground fire** (`ATTACKGROUND`): a dotted line along the ball's flight to its first
//!   graze, a ring there, and the dashed lane it skips along with a tick at each likely skip;
//! - **direct fire** (an attack on a building, or on men beyond canister reach): a solid line
//!   to the aim point with a cross bar and a dashed line on to where a miss bounces out;
//! - the **minimum distance** of a ball mode: a hatched grey ring around the gun;
//! - a selected **mortar** (#301, [`mortar`]): the hatched ring of its minimum range and a
//!   dashed circle with a centre cross per engagement zone.
//!
//! Positions are world pixels; `at` maps them to world screen pixels on the ground.

use acr_sim::World;
use acr_sim::artillery::{self, GunMode};
use acr_sim::catalog::SectorWeapon;
use acr_sim::world::{Order, Unit};

use crate::ui::draw::Draw;

/// What a gun aims at: a ground point or an object's position, or (`to: None`) its facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aim {
    pub to: Option<(i32, i32)>,
    /// A ground attack (`ATTACKGROUND`, until stopped) rather than an object.
    pub point: bool,
    pub building: bool,
}

impl Aim {
    /// The gun's facing: the canister volley it fires by itself.
    pub const AHEAD: Self = Self { to: None, point: false, building: false };
}

/// The mark of one gun: the mode, the gun's position, the aim point and the minimum distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub mode: GunMode,
    pub from: (i32, i32),
    pub to: (i32, i32),
    pub min: i32,
}

/// What gun `u` is doing: its current attack, else its facing. `None` for units without
/// canister.
pub fn current(world: &World, u: &Unit) -> Option<Aim> {
    world.catalog.get(u.kind).sector_weapon?;
    Some(match u.orders.last() {
        Some(Order::AttackPoint { x, y, times, .. }) if *times != 1 => {
            Aim { to: Some((*x, *y)), point: true, building: false }
        }
        Some(Order::Attack { target, .. }) => match world.unit(*target) {
            Some(e) => Aim { to: Some(e.pos.pixels()), point: false, building: e.building },
            None => Aim::AHEAD,
        },
        _ => Aim::AHEAD,
    })
}

/// The mark of gun `u` aiming as `aim`: the sim's own choice with the hit-locations rule
/// ([`artillery::gun_shot`]), else the original's (canister within its reach, the ball from
/// its `ATTACK_RADIUS` minimum on).
pub fn mark(world: &World, u: &Unit, aim: Aim) -> Option<Mark> {
    let t = world.catalog.get(u.kind);
    let s = t.sector_weapon?;
    let from = u.pos.pixels();
    let ahead = (from.0 + acr_sim::trig::cos(u.dir), from.1 + acr_sim::trig::sin(u.dir));
    let to = aim.to.unwrap_or(ahead);
    let dst = acr_sim::trig::norma(to.0 - from.0, to.1 - from.1);
    let i = usize::try_from(u.id.0).ok().filter(|&i| world.units.get(i).is_some_and(|v| v.id == u.id))?;
    let ahead_only = aim.to.is_none();
    let times = if aim.point { 0 } else { 1 };
    let (mode, min) = match artillery::gun_shot(world, i, aim.point || ahead_only, times, aim.building, dst) {
        Some((m, _, min)) => (m, min),
        None => {
            let ta = world.adv(u.owner, u.kind);
            let ball = artillery::ball_index(t).and_then(|k| ta.attacks.get(k)).map_or(0, |a| a.radius.0);
            let reach = ta.attacks.get(usize::from(s.att)).map_or(s.r2, |a| a.radius.1);
            if ahead_only || dst <= reach && !aim.building {
                (GunMode::Canister, 0)
            } else if aim.point {
                (GunMode::Ground, ball)
            } else {
                (GunMode::Direct, ball)
            }
        }
    };
    let min = if mode == GunMode::Canister { 0 } else { min };
    Some(Mark { mode, from, to, min })
}

/// Colours: the selection brackets' yellow, the dark edge under it and the grey of the
/// too-close area.
#[derive(Debug, Clone, Copy)]
pub struct Colours {
    pub mark: [u8; 4],
    pub dark: [u8; 4],
    pub grey: [u8; 4],
}

type At<'a> = &'a dyn Fn(f64, f64) -> (i32, i32);

/// The colour of a line and how many window pixels lower it is drawn (1: the dark edge).
type Ink = ([u8; 4], u8);

/// A ground line from `a` to `b` (world px) in steps of 16 px following the ground, drawn
/// where `pattern(along)` holds (`along` in world px from `a`), one window pixel wide at any
/// zoom ([`Draw::line`]).
fn ground_line(out: &mut Vec<Draw>, (c, drop): Ink, at: At, a: (f64, f64), b: (f64, f64), pattern: fn(f64) -> bool) {
    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let n = (len / 16.0).ceil().max(1.0) as i32;
    let p = |t: f64| at(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    for k in 0..n {
        let (t0, t1) = (f64::from(k) / f64::from(n), f64::from(k + 1) / f64::from(n));
        if pattern(len * (t0 + t1) / 2.0) {
            out.push(Draw::line(c, p(t0), p(t1)).dropped(drop));
        }
    }
}

/// Dashes of 48 px with gaps of 32 (lanes, outlines).
fn dashed(d: f64) -> bool {
    d % 80.0 < 48.0
}

/// Dots of 16 px every 48 (the flight, half reach, hatching).
fn dotted(d: f64) -> bool {
    d % 48.0 < 16.0
}

fn solid(_: f64) -> bool {
    true
}

/// A point `r` px from `o` at angle `a` (radians, x right, y down).
fn polar(o: (f64, f64), a: f64, r: f64) -> (f64, f64) {
    (o.0 + r * a.cos(), o.1 + r * a.sin())
}

/// An arc around `o` of radius `r` from angle `a0` to `a1`.
fn arc(
    out: &mut Vec<Draw>,
    (c, drop): Ink,
    at: At,
    o: (f64, f64),
    r: f64,
    (a0, a1): (f64, f64),
    pattern: fn(f64) -> bool,
) {
    let n = ((a1 - a0).abs() * r / 16.0).ceil().max(2.0) as i32;
    for k in 0..n {
        let t0 = a0 + (a1 - a0) * f64::from(k) / f64::from(n);
        let t1 = a0 + (a1 - a0) * f64::from(k + 1) / f64::from(n);
        if pattern(r * ((t0 + t1) / 2.0 - a0).abs()) {
            let (p, q) = (polar(o, t0, r), polar(o, t1, r));
            out.push(Draw::line(c, at(p.0, p.1), at(q.0, q.1)).dropped(drop));
        }
    }
}

/// A line in the mark's yellow over its dark edge.
fn outlined(out: &mut Vec<Draw>, cs: Colours, at: At, a: (f64, f64), b: (f64, f64), pattern: fn(f64) -> bool) {
    ground_line(out, (cs.dark, 1), at, a, b, pattern);
    ground_line(out, (cs.mark, 0), at, a, b, pattern);
}

/// An arc in the mark's yellow over its dark edge.
fn outlined_arc(
    out: &mut Vec<Draw>,
    cs: Colours,
    at: At,
    o: (f64, f64),
    r: f64,
    a: (f64, f64),
    pattern: fn(f64) -> bool,
) {
    arc(out, (cs.dark, 1), at, o, r, a, pattern);
    arc(out, (cs.mark, 0), at, o, r, a, pattern);
}

/// The grey ring of radius `r` around `o`, dashed, hatched diagonally every 64 px inside.
fn too_close(out: &mut Vec<Draw>, cs: Colours, at: At, o: (f64, f64), r: f64) {
    arc(out, (cs.grey, 0), at, o, r, (0.0, std::f64::consts::TAU), dashed);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let mut off = -r + 32.0;
    while off < r {
        let half = (r * r - off * off).sqrt();
        // Across (1, −1)/√2 by `off`, along (1, 1)/√2 by ±half.
        let m = (o.0 + off * s, o.1 - off * s);
        ground_line(out, (cs.grey, 0), at, (m.0 - half * s, m.1 - half * s), (m.0 + half * s, m.1 + half * s), dotted);
        off += 64.0;
    }
}

/// Draws `m` for a gun of canister sector `s` (`WEAPONINSECTOR`: reach `r1..r2`, half-angle
/// `sector` in 1/256 turns).
pub fn draw(m: &Mark, s: &SectorWeapon, cs: Colours, at: At, out: &mut Vec<Draw>) {
    let o = (f64::from(m.from.0), f64::from(m.from.1));
    let g = (f64::from(m.to.0), f64::from(m.to.1));
    let (dx, dy) = (g.0 - o.0, g.1 - o.1);
    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
    let dir = dy.atan2(dx);
    let u = (dx / dist, dy / dist);
    let n = (-u.1, u.0);
    if m.min > 0 {
        too_close(out, cs, at, o, f64::from(m.min));
    }
    match m.mode {
        GunMode::Canister => {
            let half = f64::from(s.sector) * std::f64::consts::TAU / 256.0;
            let (r1, r2) = (f64::from(s.r1), f64::from(s.r2));
            for a in [dir - half, dir + half] {
                outlined(out, cs, at, polar(o, a, r1), polar(o, a, r2), dashed);
            }
            outlined_arc(out, cs, at, o, r2, (dir - half, dir + half), dashed);
            arc(out, (cs.mark, 0), at, o, r2 / 2.0, (dir - half, dir + half), dotted);
        }
        GunMode::Ground => {
            ground_line(out, (cs.mark, 0), at, o, g, dotted);
            outlined_arc(out, cs, at, g, 24.0, (0.0, std::f64::consts::TAU), solid);
            let lane = f64::from(artillery::GROUND_LANE);
            let w = 32.0;
            for side in [-w, w] {
                let a = (g.0 + n.0 * side, g.1 + n.1 * side);
                outlined(out, cs, at, a, (a.0 + u.0 * lane, a.1 + u.1 * lane), dashed);
            }
            // The skips: each half as long as the one before (a bounce keeps half its
            // speed), so the ticks close up towards the lane's end.
            let (mut d, mut step) = (0.0, lane / 2.0);
            for _ in 0..4 {
                d += step;
                step /= 2.0;
                let c = (g.0 + u.0 * d, g.1 + u.1 * d);
                outlined(out, cs, at, (c.0 - n.0 * w, c.1 - n.1 * w), (c.0 + n.0 * w, c.1 + n.1 * w), solid);
            }
        }
        GunMode::Direct => {
            outlined(out, cs, at, o, g, solid);
            outlined(out, cs, at, (g.0 - n.0 * 20.0, g.1 - n.1 * 20.0), (g.0 + n.0 * 20.0, g.1 + n.1 * 20.0), solid);
            let lane = f64::from(artillery::DIRECT_LANE);
            outlined(out, cs, at, g, (g.0 + u.0 * lane, g.1 + u.1 * lane), dashed);
        }
        GunMode::Mortar => {
            ground_line(out, (cs.mark, 0), at, o, g, dotted);
            outlined_arc(out, cs, at, g, 48.0, (0.0, std::f64::consts::TAU), dashed);
        }
    }
}

/// A mortar's engagement zone (#301): a dashed circle with a small cross at its centre.
pub fn zone(z: &acr_sim::mortar::Zone, cs: Colours, at: At, out: &mut Vec<Draw>) {
    let c = (f64::from(z.x), f64::from(z.y));
    outlined_arc(out, cs, at, c, f64::from(z.r), (0.0, std::f64::consts::TAU), dashed);
    outlined(out, cs, at, (c.0 - 16.0, c.1), (c.0 + 16.0, c.1), solid);
    outlined(out, cs, at, (c.0, c.1 - 16.0), (c.0, c.1 + 16.0), solid);
}

/// A selected mortar's marks (#301): the hatched grey ring of its minimum range `min`
/// around `from` and its zones.
pub fn mortar(from: (i32, i32), min: i32, zones: &[acr_sim::mortar::Zone], cs: Colours, at: At, out: &mut Vec<Draw>) {
    too_close(out, cs, at, (f64::from(from.0), f64::from(from.1)), f64::from(min));
    for z in zones {
        zone(z, cs, at, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_alternate() {
        assert!(dashed(10.0) && !dashed(60.0) && dashed(90.0));
        assert!(dotted(5.0) && !dotted(30.0));
    }

    #[test]
    fn every_mode_draws() {
        let cs = Colours { mark: [255, 255, 0, 255], dark: [0, 0, 0, 255], grey: [128, 128, 128, 255] };
        let s = SectorWeapon { att: 5, r1: 64, r2: 1500, d1: 400, d2: 1, sector: 32, pct: 50 };
        let at = |x: f64, y: f64| (x as i32, (y as i32) >> 1);
        for (mode, min) in [(GunMode::Canister, 0), (GunMode::Ground, 600), (GunMode::Direct, 0)] {
            let mut out = Vec::new();
            draw(&Mark { mode, from: (0, 0), to: (2000, 0), min }, &s, cs, &at, &mut out);
            assert!(out.len() > 20, "{mode:?}");
        }
    }
}
