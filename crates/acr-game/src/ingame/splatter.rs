//! Blood sprayed onto the ground behind units hit by bullets, arrows, grapeshot and
//! cannonballs (#219, docs/rendering-3d.md). Render only and stateless: every frame the
//! droplets are derived from `World::hits` (kept by the simulation for
//! `combat::HIT_KEEP` ticks, longer than a droplet lives) and the current tick, seeded from
//! the hit, so they neither flicker nor need state of their own and come back alike after
//! loading a game.
//!
//! The same spray lands on troops standing behind the victim (speckles on the side facing
//! it) and on the walls of buildings in its path (#220).

use acr_render::model::ramp;
use acr_render::model_renderer::{BloodPool, Sprayed};
use acr_render::view3d::PIXELS_PER_METRE;
use acr_sim::combat::{DeathCause, HIT_KEEP, HitRecord};

use crate::ui::settings::Gore;

/// How a weapon sprays blood behind the victim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spray {
    /// Droplets per hit.
    pub count: u32,
    /// How far behind the victim they land (metres).
    pub near: f32,
    pub far: f32,
    /// Droplet radius (metres).
    pub small: f32,
    pub big: f32,
    /// Half the opening angle of the cone (radians).
    pub cone: f32,
}

/// The spray of a cause; `None` for causes that spray nothing.
pub fn spray(cause: DeathCause) -> Option<Spray> {
    let s = |count, near, far, small, big, cone| Some(Spray { count, near, far, small, big, cone });
    match cause {
        DeathCause::Gunshot => s(6, 0.2, 1.8, 0.06, 0.14, 0.30),
        DeathCause::Arrow => s(3, 0.1, 0.8, 0.05, 0.10, 0.25),
        DeathCause::Grapeshot => s(9, 0.2, 2.2, 0.06, 0.15, 0.55),
        DeathCause::Cannonball => s(18, 0.3, 3.5, 0.08, 0.25, 0.45),
        _ => None,
    }
}

/// Ticks after the hit when a droplet has dried, and when it starts to and has faded.
const DRIED: f32 = 400.0;
const FADE: (f32, f32) = (HIT_KEEP as f32 * 0.75, HIT_KEEP as f32 - 10.0);

/// The droplets of `hit` at tick `now`, appended to `out`; `ground(x, y)` is the ground point
/// (world) of world pixel `(x, y)`.
pub fn splatter(
    hit: &HitRecord,
    now: u64,
    gore: Gore,
    ground: &dyn Fn(f32, f32) -> [f32; 3],
    out: &mut Vec<BloodPool>,
) {
    if gore == Gore::Off || hit.building || now < hit.tick {
        return;
    }
    let Some(mut s) = spray(hit.cause) else { return };
    let t = (now - hit.tick) as f32;
    let fade = 1.0 - ramp(t, FADE.0, FADE.1);
    if fade <= 0.0 {
        return;
    }
    if hit.killed {
        s.count += s.count / 2;
    }
    if gore == Gore::Reduced {
        s.count = s.count.div_ceil(2);
        s.small *= 0.7;
        s.big *= 0.7;
        s.far *= 0.8;
    }
    let mut rng = Rng::new(hit.tick, hit.victim);
    let (dx, dy) = (hit.dir.0 as f32, hit.dir.1 as f32);
    let len = (dx * dx + dy * dy).sqrt();
    let base = if len > 0.0 { dy.atan2(dx) } else { rng.next() * std::f32::consts::TAU };
    let age = ramp(t, 40.0, DRIED);
    for _ in 0..s.count {
        let (u1, u2, u3, u4) = (rng.next(), rng.next(), rng.next(), rng.next());
        // More droplets close behind, the far ones smaller and drawn out into streaks.
        let k = u1.powf(1.5);
        let dist = (s.near + (s.far - s.near) * k) * PIXELS_PER_METRE;
        let angle = base + (2.0 * u2 - 1.0) * s.cone;
        let (sin, cos) = angle.sin_cos();
        let at = ground(hit.pos.0 as f32 + cos * dist, hit.pos.1 as f32 + sin * dist);
        out.push(BloodPool {
            centre: at,
            radius: (s.small + (s.big - s.small) * u3) * (1.0 - 0.4 * k) * PIXELS_PER_METRE,
            along: [cos, sin],
            stretch: 0.3 + 1.6 * k + 0.4 * u4,
            age,
            fade,
            seed: rng.next() * 97.0,
            splatter: true,
        });
    }
}

/// A model that spray can land on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    /// The unit's index (the victim itself is skipped).
    pub unit: u32,
    /// Ground position (world pixels) and the model's turn (`view3d::facing_yaw`).
    pub pos: (f32, f32),
    pub yaw: f32,
    /// Buildings take the spray on their walls, within `radius` (world pixels) of `pos`.
    pub building: bool,
    pub radius: f32,
}

/// How much spray of one hit a unit `along` metres behind the victim and `across` metres
/// off the shot line takes (0..1).
fn share(s: &Spray, along: f32, across: f32) -> f32 {
    if along <= 0.05 || along > s.far {
        return 0.0;
    }
    let half = (along * s.cone.tan()).max(0.25);
    if across > half {
        return 0.0;
    }
    (1.0 - along / s.far) * (1.0 - 0.5 * across / half)
}

/// The blood sprayed onto `targets` by recent `hits` at tick `now`: for each target index
/// the strongest spray that reached it. Units are found through a grid of `CELL` world
/// pixels, so a battle with thousands of hits and models stays cheap.
pub fn sprayed_targets<'a>(
    hits: impl IntoIterator<Item = &'a HitRecord>,
    now: u64,
    gore: Gore,
    targets: &[Target],
) -> Vec<(usize, Sprayed)> {
    const CELL: f32 = 128.0;
    let mut out: std::collections::HashMap<usize, Sprayed> = std::collections::HashMap::new();
    if gore == Gore::Off || targets.is_empty() {
        return Vec::new();
    }
    let key = |x: f32, y: f32| ((x / CELL).floor() as i32, (y / CELL).floor() as i32);
    let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
    let mut buildings = Vec::new();
    for (i, t) in targets.iter().enumerate() {
        if t.building {
            buildings.push(i);
        } else {
            grid.entry(key(t.pos.0, t.pos.1)).or_default().push(i);
        }
    }
    let strength = if gore == Gore::Reduced { 0.5 } else { 1.0 };
    for hit in hits.into_iter().filter(|h| !h.building && now >= h.tick) {
        let Some(s) = spray(hit.cause) else { continue };
        let t = (now - hit.tick) as f32;
        let fade = 1.0 - ramp(t, FADE.0, FADE.1);
        let (dx, dy) = (hit.dir.0 as f32, hit.dir.1 as f32);
        let len = (dx * dx + dy * dy).sqrt();
        if fade <= 0.0 || len == 0.0 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let power = strength * fade * if hit.killed { 1.0 } else { 0.7 };
        let age = ramp(t, 40.0, DRIED);
        let (vx, vy) = (hit.pos.0 as f32, hit.pos.1 as f32);
        let reach = s.far * PIXELS_PER_METRE;
        let mut take = |i: usize, amount: f32, point: [f32; 3], radius: f32| {
            let e = out.entry(i).or_default();
            if amount > e.amount {
                *e = Sprayed { point, radius, amount, age };
            }
        };
        // Troops in the cone behind the victim: speckles on the side facing it.
        let (a, b) = (
            key(vx.min(vx + ux * reach) - CELL, vy.min(vy + uy * reach) - CELL),
            key(vx.max(vx + ux * reach) + CELL, vy.max(vy + uy * reach) + CELL),
        );
        for cx in a.0..=b.0 {
            for cy in a.1..=b.1 {
                for &i in grid.get(&(cx, cy)).into_iter().flatten() {
                    let tg = &targets[i];
                    if tg.unit == hit.victim {
                        continue;
                    }
                    let (rx, ry) = (tg.pos.0 - vx, tg.pos.1 - vy);
                    let along = (rx * ux + ry * uy) / PIXELS_PER_METRE;
                    let across = (rx * uy - ry * ux).abs() / PIXELS_PER_METRE;
                    let amount = power * share(&s, along, across);
                    if amount > 0.02 {
                        // Towards the victim in the model's frame (model x → world (c, −s),
                        // model z → world (s, c)), at chest height.
                        let (sn, cs) = tg.yaw.sin_cos();
                        let (wx, wy) = (-ux, -uy);
                        let (lx, lz) = (wx * cs - wy * sn, wx * sn + wy * cs);
                        take(i, amount, [lx * 0.22, 1.2, lz * 0.22], 0.55);
                    }
                }
            }
        }
        // Buildings whose footprint the spray enters within its reach: a stain on the wall.
        for &i in &buildings {
            let tg = &targets[i];
            let (rx, ry) = (tg.pos.0 - vx, tg.pos.1 - vy);
            let along = rx * ux + ry * uy;
            let off = (rx * uy - ry * ux).abs();
            let r = tg.radius * 0.7;
            if off >= r || along <= 0.0 {
                continue;
            }
            let enter = along - (r * r - off * off).sqrt();
            if !(0.0..=reach).contains(&enter) {
                continue;
            }
            let amount = power * (1.0 - enter / reach);
            let (px, py) = (vx + ux * enter, vy + uy * enter);
            // Buildings are not turned: model x is world x, model z is world y.
            let point = [(px - tg.pos.0) / PIXELS_PER_METRE, 1.3, (py - tg.pos.1) / PIXELS_PER_METRE];
            take(i, amount, point, 0.9);
        }
    }
    let mut v: Vec<(usize, Sprayed)> = out.into_iter().collect();
    v.sort_by_key(|(i, _)| *i);
    v
}

/// A small deterministic generator (SplitMix64) seeded from a hit.
struct Rng(u64);

impl Rng {
    fn new(tick: u64, victim: u32) -> Self {
        Self(tick.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(victim).wrapping_mul(0xD1B5_4A32_D192_ED03))
    }

    /// Uniform in `0..1`.
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(cause: DeathCause) -> HitRecord {
        HitRecord { tick: 100, victim: 3, pos: (1000, 1000), dir: (300, 0), cause, killed: false, building: false }
    }

    fn drops(h: &HitRecord, now: u64, gore: Gore) -> Vec<BloodPool> {
        let mut out = Vec::new();
        splatter(h, now, gore, &|x, y| [x, y, 0.0], &mut out);
        out
    }

    fn soldier(unit: u32, x: f32, y: f32, yaw: f32) -> Target {
        Target { unit, pos: (x, y), yaw, building: false, radius: 0.0 }
    }

    #[test]
    fn spray_lands_on_troops_behind_the_victim() {
        // The shot travels east from the victim at (1000, 1000).
        let mut h = hit(DeathCause::Gunshot);
        h.killed = true;
        let behind = soldier(1, 1000.0 + 0.8 * PIXELS_PER_METRE, 1000.0, 0.0);
        let in_front = soldier(2, 1000.0 - 0.8 * PIXELS_PER_METRE, 1000.0, 0.0);
        let aside = soldier(4, 1000.0 + 0.8 * PIXELS_PER_METRE, 1000.0 + 1.5 * PIXELS_PER_METRE, 0.0);
        let victim = soldier(3, 1000.0, 1000.0, 0.0);
        let s = sprayed_targets([&h], 110, Gore::Full, &[behind, in_front, aside, victim]);
        assert_eq!(s.len(), 1, "{s:?}");
        let (i, sp) = s[0];
        assert_eq!(i, 0);
        assert!(sp.amount > 0.3 && sp.age == 0.0);
        // Facing east (yaw 0 → model +z is world +y, +x is world +x): the victim lies west,
        // so the speckles sit on the model's −x side.
        assert!(sp.point[0] < -0.2 && sp.point[2].abs() < 1e-5, "{:?}", sp.point);
        // Reduced gore halves it, off removes it, it dries and fades.
        let r = sprayed_targets([&h], 110, Gore::Reduced, &[behind]);
        assert!((r[0].1.amount - sp.amount / 2.0).abs() < 1e-5);
        assert!(sprayed_targets([&h], 110, Gore::Off, &[behind]).is_empty());
        assert!(sprayed_targets([&h], 600, Gore::Full, &[behind])[0].1.age == 1.0);
        assert!(sprayed_targets([&h], 100 + HIT_KEEP, Gore::Full, &[behind]).is_empty());
    }

    #[test]
    fn spray_stains_a_building_wall_in_its_path() {
        let h = hit(DeathCause::Cannonball);
        // A building 3 m east, 1 m across: the spray enters it from the west.
        let b = Target {
            unit: 9,
            pos: (1000.0 + 3.0 * PIXELS_PER_METRE, 1000.0),
            yaw: 0.0,
            building: true,
            radius: 1.5 * PIXELS_PER_METRE,
        };
        let s = sprayed_targets([&h], 110, Gore::Full, &[b]);
        assert_eq!(s.len(), 1);
        let sp = s[0].1;
        assert!(sp.point[0] < -0.9 && sp.point[0] > -1.1 && sp.point[2].abs() < 1e-4, "{:?}", sp.point);
        // Out of reach (cannonball spray 3.5 m) it stays clean.
        let far = Target { pos: (1000.0 + 6.0 * PIXELS_PER_METRE, 1000.0), ..b };
        assert!(sprayed_targets([&h], 110, Gore::Full, &[far]).is_empty());
    }

    #[test]
    fn droplets_land_in_a_cone_behind_the_victim() {
        let h = hit(DeathCause::Gunshot);
        let d = drops(&h, 110, Gore::Full);
        assert_eq!(d.len(), 6);
        for p in &d {
            // The shot travels east: the droplets lie east of the victim, within the cone.
            let (x, y) = (p.centre[0] - 1000.0, p.centre[1] - 1000.0);
            assert!(x > 0.0 && x <= 1.8 * PIXELS_PER_METRE + 1e-3, "{p:?}");
            assert!(y.abs() <= x * 0.31_f32.tan() + 1e-3, "{p:?}");
            assert!(p.splatter && p.along[0] > 0.9 && p.fade == 1.0);
        }
        // The same every frame; another hit lands elsewhere.
        assert_eq!(d, drops(&h, 110, Gore::Full));
        assert_ne!(d[0].centre, drops(&HitRecord { tick: 101, ..h }, 111, Gore::Full)[0].centre);
    }

    #[test]
    fn causes_gore_and_time() {
        // Melee, fire, hunger and buildings spray nothing; gore=off shows nothing.
        for c in [DeathCause::Melee, DeathCause::Burned, DeathCause::Hunger, DeathCause::Explosion] {
            assert!(drops(&hit(c), 110, Gore::Full).is_empty(), "{c:?}");
        }
        assert!(drops(&HitRecord { building: true, ..hit(DeathCause::Gunshot) }, 110, Gore::Full).is_empty());
        assert!(drops(&hit(DeathCause::Gunshot), 110, Gore::Off).is_empty());
        // A cannonball sprays more and bigger than a musket ball, a kill more than a wound.
        let ball = drops(&hit(DeathCause::Cannonball), 110, Gore::Full);
        let gun = drops(&hit(DeathCause::Gunshot), 110, Gore::Full);
        let max = |v: &[BloodPool]| v.iter().map(|p| p.radius).fold(0.0, f32::max);
        assert!(ball.len() > gun.len() && max(&ball) > max(&gun));
        assert!(drops(&HitRecord { killed: true, ..hit(DeathCause::Gunshot) }, 110, Gore::Full).len() > gun.len());
        // Reduced: fewer and smaller.
        let reduced = drops(&hit(DeathCause::Cannonball), 110, Gore::Reduced);
        assert!(reduced.len() < ball.len() && max(&reduced) < max(&ball));
        // Fresh, dried, faded, gone before the hit is forgotten.
        let at = |t| drops(&hit(DeathCause::Gunshot), 100 + t, Gore::Full);
        assert!(at(0)[0].age == 0.0 && at(500)[0].age == 1.0 && at(500)[0].fade == 1.0);
        assert!(at(650)[0].fade < 1.0);
        assert!(at(HIT_KEEP).is_empty());
    }
}
