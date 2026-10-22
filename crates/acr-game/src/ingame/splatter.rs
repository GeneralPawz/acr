//! Blood sprayed onto the ground behind units hit by bullets, arrows, grapeshot and
//! cannonballs (#219, docs/rendering-3d.md). Render only and stateless: every frame the
//! droplets are derived from `World::hits` (kept by the simulation for
//! `combat::HIT_KEEP` ticks, longer than a droplet lives) and the current tick, seeded from
//! the hit, so they neither flicker nor need state of their own and come back alike after
//! loading a game.

use acr_render::model::ramp;
use acr_render::model_renderer::BloodPool;
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
