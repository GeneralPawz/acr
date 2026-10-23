//! `decay_stain=on` (settings): a decaying body's fluids soak into the ground under it, the
//! "cadaver decomposition island" of forensic taphonomy. From bloat on, purge fluid runs
//! from the mouth and nose, then through the ruptured skin, into the soil under the head and
//! torso: a stain that grows over active decay, greasy and nearly black-brown with a
//! greenish-grey rim while wet, drying to a dark brown crust; the grass around it yellows and
//! dies in a ring. The patch stays under the bones, and after them: kept with
//! `battlefield_marks=keep` (`kept_marks.rs`), else fading slowly. Not blood: no red.
//!
//! One decal per corpse (a [`Dirt::Seep`] [`BloodPool`], `blood.wgsl` `seep`), drawn under
//! the other decals. Render only: the simulation and its hashes do not change.

use std::collections::HashMap;

use acr_render::model::ramp;
use acr_render::model_renderer::{BloodPool, Dirt, Seep};

use acr_render::view3d::PIXELS_PER_METRE;

/// Decay where the purge fluid starts to leak (bloat, `model::decay_pose` deflates from 0.18).
pub const ONSET: f32 = 0.08;
/// Decay where the stain has its full size: the end of active decay (before
/// `model::HALF_DECAY`, so `corpse_decay=half` shows it whole and wet).
pub const FULL: f32 = 0.40;
/// Decay over which it dries to a crust.
const DRY: [f32; 2] = [0.46, 0.80];
/// Decay over which the grass around it dies.
const RING: [f32; 2] = [0.15, 0.55];
/// Ticks a stain fades over after its corpse went (without `battlefield_marks=keep`): 5
/// minutes of game time.
pub const FADE_TICKS: u64 = 3000;
/// How far the quad runs out ahead (`stretch`): the decal is 1.6 times as long as wide.
const STRETCH: f32 = 0.6;
/// Radius of a man's stain quad (world pixels): the dark core is 0.6 of it across and
/// 0.96 along (about 0.95 × 1.5 m), the dead grass reaches the quad's edge.
const RADIUS: f32 = 0.78 * PIXELS_PER_METRE;
/// The core's centre ahead of the ground point, towards the head (metres): under the chest
/// and head of a body lying towards its facing (the chest is 0.55 m ahead, `gore.rs`).
const AHEAD: f32 = 0.6;
/// The core's centre from the hips towards the neck (metres): under the chest, the head's
/// end reaching past the head.
const HIPS_AHEAD: f32 = 0.42;

/// The stain under a body at ground point `ground` lying towards `(s, c)` (`facing_yaw`'s
/// sine and cosine), `size` times a man's, at corpse decay `decay`; `turf` (0..1) whether
/// grass grows there to die. `None` before the onset.
pub fn stain(ground: [f32; 3], (s, c): (f32, f32), size: f32, decay: f32, turf: f32, seed: f32) -> Option<BloodPool> {
    if decay < ONSET {
        return None;
    }
    let r = RADIUS * size;
    // The core lies AHEAD; the quad starts r behind its centre and runs r(1 + 2·STRETCH)
    // ahead (`blood.wgsl`), so its origin is r·STRETCH behind the core.
    let ahead = AHEAD * size * PIXELS_PER_METRE - r * STRETCH;
    Some(BloodPool {
        centre: [ground[0] + s * ahead, ground[1] + c * ahead, ground[2]],
        radius: r,
        along: [s, c],
        stretch: STRETCH,
        age: ramp(decay, DRY[0], DRY[1]),
        fade: 1.0,
        seed,
        dirt: Dirt::Seep,
        seep: Seep { growth: ramp(decay, ONSET, FULL), ring: turf * ramp(decay, RING[0], RING[1]) },
        ..Default::default()
    })
}

struct Tracked {
    pool: BloodPool,
    /// When its corpse went (fading from then on), or `None` while it lies.
    gone: Option<u64>,
    /// Laid under the body's pose ([`Seeps::place`]): it stays there.
    placed: bool,
}

/// The stains of the corpses, by unit id, and of those gone, fading.
#[derive(Default)]
pub struct Seeps {
    stains: HashMap<u32, Tracked>,
    seen: Vec<u32>,
}

impl Seeps {
    pub fn clear(&mut self) {
        self.stains.clear();
        self.seen.clear();
    }

    pub fn len(&self) -> usize {
        self.stains.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stains.is_empty()
    }

    /// A corpse's stain this frame (call for every corpse drawn, then [`Self::end`]). It
    /// never shrinks or gets wetter again.
    pub fn corpse(&mut self, unit: u32, pool: BloodPool) {
        let t = self.stains.entry(unit).or_insert(Tracked { pool, gone: None, placed: false });
        let old = t.pool;
        let (centre, along) = if t.placed { (old.centre, old.along) } else { (pool.centre, pool.along) };
        t.pool = BloodPool {
            centre,
            along,
            age: old.age.max(pool.age),
            seep: Seep { growth: old.seep.growth.max(pool.seep.growth), ring: old.seep.ring.max(pool.seep.ring) },
            ..pool
        };
        t.gone = None;
        self.seen.push(unit);
    }

    /// Lays the stains of the corpses lying this frame under their bodies' poses: `bones`
    /// gives a body's hips and neck (world) where the body physics holds it. The core lies
    /// from the hips to past the head; once laid a stain stays (the body settles).
    pub fn place(&mut self, bones: &dyn Fn(u32) -> Option<([f32; 3], [f32; 3])>) {
        for &u in &self.seen {
            let Some(t) = self.stains.get_mut(&u) else { continue };
            let Some((hips, neck)) = bones(u) else { continue };
            let (dx, dy) = (neck[0] - hips[0], neck[1] - hips[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 0.2 * PIXELS_PER_METRE {
                // Curled up or upright: the facing stays.
                continue;
            }
            let along = [dx / len, dy / len];
            let size = t.pool.radius / RADIUS;
            let back = HIPS_AHEAD * size * PIXELS_PER_METRE - t.pool.radius * STRETCH;
            t.pool.centre = [hips[0] + along[0] * back, hips[1] + along[1] * back, hips[2]];
            t.pool.along = along;
            t.placed = true;
        }
    }

    /// Ends a frame at tick `now`: the stains of corpses not drawn are gone, handed to `keep`
    /// (`battlefield_marks=keep`) or left to fade; all are appended to `out`.
    pub fn end(&mut self, now: u64, keep: Option<&mut dyn FnMut(BloodPool)>, out: &mut Vec<BloodPool>) {
        let mut seen = std::mem::take(&mut self.seen);
        seen.sort_unstable();
        let mut keep = keep;
        self.stains.retain(|u, t| {
            if seen.binary_search(u).is_ok() {
                return true;
            }
            if let Some(k) = keep.as_mut() {
                k(t.pool);
                return false;
            }
            let gone = *t.gone.get_or_insert(now);
            now.saturating_sub(gone) < FADE_TICKS
        });
        for t in self.stains.values() {
            let fade = t.gone.map_or(1.0, |g| 1.0 - now.saturating_sub(g) as f32 / FADE_TICKS as f32);
            out.push(BloodPool { fade, ..t.pool });
        }
        seen.clear();
        self.seen = seen;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_with_the_decay_grows_and_dries() {
        let at = |d| stain([0.0; 3], (0.0, 1.0), 1.0, d, 1.0, 3.0);
        assert!(at(0.05).is_none());
        let (early, wet, dry) = (at(0.1).unwrap(), at(0.4).unwrap(), at(1.0).unwrap());
        assert!(early.seep.growth < 0.2 && wet.seep.growth == 1.0);
        assert_eq!((early.age, wet.age), (0.0, 0.0));
        assert_eq!(dry.age, 1.0);
        assert_eq!(dry.dirt, Dirt::Seep);
        // The half state (`corpse_decay=half`) shows it whole and wet.
        let half = at(acr_render::model::HALF_DECAY).unwrap();
        assert_eq!((half.seep.growth, half.age), (1.0, 0.0));
        // No grass, no dead ring.
        assert_eq!(stain([0.0; 3], (0.0, 1.0), 1.0, 1.0, 0.0, 3.0).unwrap().seep.ring, 0.0);
    }

    #[test]
    fn it_lies_under_the_chest_and_head() {
        // Lying south (+y): the core's centre (quad origin + r·STRETCH) is AHEAD m south.
        let p = stain([100.0, 100.0, 0.0], (0.0, 1.0), 1.0, 0.5, 1.0, 0.0).unwrap();
        let core = p.centre[1] + p.radius * p.stretch;
        assert!((core - (100.0 + AHEAD * PIXELS_PER_METRE)).abs() < 1e-3);
        assert_eq!(p.centre[0], 100.0);
    }

    #[test]
    fn gone_stains_fade_or_are_kept() {
        let p = stain([0.0; 3], (0.0, 1.0), 1.0, 0.9, 1.0, 0.0).unwrap();
        let mut s = Seeps::default();
        let mut out = Vec::new();
        s.corpse(1, p);
        s.end(10, None, &mut out);
        assert_eq!(out.len(), 1);
        // Gone: it fades over FADE_TICKS.
        out.clear();
        s.end(11, None, &mut out);
        out.clear();
        s.end(11 + FADE_TICKS / 2, None, &mut out);
        assert!((out[0].fade - 0.5).abs() < 0.01);
        out.clear();
        s.end(11 + FADE_TICKS, None, &mut out);
        assert!(out.is_empty() && s.is_empty());
        // Kept: handed over at once.
        let mut kept = Vec::new();
        s.corpse(2, p);
        s.end(20, None, &mut out);
        s.end(21, Some(&mut |p| kept.push(p)), &mut out);
        assert_eq!(kept.len(), 1);
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn it_never_shrinks_or_wets_again() {
        let mut s = Seeps::default();
        s.corpse(1, stain([0.0; 3], (0.0, 1.0), 1.0, 0.9, 1.0, 0.0).unwrap());
        s.corpse(1, stain([0.0; 3], (0.0, 1.0), 1.0, 0.2, 1.0, 0.0).unwrap());
        let mut out = Vec::new();
        s.end(1, None, &mut out);
        assert!(out[0].age > 0.9 && out[0].seep.growth == 1.0);
    }
}
