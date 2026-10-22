//! Light XPBD cloth for skinned garments (docs/clothing.md, "Runtime cloth"): the coat
//! skirts of a man near the camera.
//!
//! A low-resolution proxy of particles, each with a skinned target (where the clip or the
//! ragdoll puts it) and a freedom `cloth` (the garment's `_CLOTH`: 0 at the waist seam, which
//! follows the skin exactly, 1 at the hem). Per substep: inertia and gravity, a drive back
//! to the target that weakens with the freedom (the drape's shape memory), XPBD distance
//! constraints (the cloth does not stretch; across the skirt it may bunch, so those are
//! one-sided), bending as distances two apart, collision with capsules (the legs) and the
//! ground, air drag towards the wind.
//!
//! The draped rest shape already hangs nearly straight from the waist, so gravity changes
//! little on a man standing: the skirts keep their drape and swing by inertia when he
//! walks, flap when he runs. On a man lying down the targets stick out from the hips and
//! gravity, against the weak drive at the hem, lays the skirts on the ground.
//!
//! Same conventions as the rest of the crate: metres, `+Z` up, `f32`, no allocation in
//! [`Cloth::step`].

use crate::Ground;
use crate::math::Vec3;

/// A collision capsule (a leg bone): the segment `a`–`b`, radius `r`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
}

impl Capsule {
    fn closest(&self, p: Vec3) -> Vec3 {
        let ab = self.b - self.a;
        let t = ((p - self.a).dot(ab) / ab.length_sq().max(1e-9)).clamp(0.0, 1.0);
        self.a + ab * t
    }

    /// Pushes `p` out of the capsule on the side of `side` (the particle's target: a leg
    /// swinging into the skirt pushes it on, never through to the far side); returns
    /// whether it was inside.
    fn push_out(&self, p: &mut Vec3, side: Vec3) -> bool {
        let q = self.closest(*p);
        let d = *p - q;
        if d.length_sq() >= self.r * self.r {
            return false;
        }
        let s = side - self.closest(side);
        let n = if s.length_sq() > 1e-10 { s.normalize_or_zero() } else { d.normalize_or_zero() };
        let n = if n == Vec3::ZERO { Vec3::Z } else { n };
        // Out along `n`, from the particle's own point on the axis.
        let along = d.dot(n);
        let perp = d - n * along;
        let out = (self.r * self.r - perp.length_sq()).max(0.0).sqrt();
        *p = q + perp + n * out;
        true
    }
}

/// How a cloth behaves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClothConfig {
    pub substeps: u32,
    /// Gravity (m/s²) in the world.
    pub gravity: Vec3,
    /// Drive back to the skinned target (1/s²) of a particle with freedom 0 and 1: the
    /// drape's shape memory (an upright man; the caller weakens it for a man lying down,
    /// whose skirts no longer hang as they were draped).
    pub drive: [f32; 2],
    /// Damping ratio of the drive (relative to the target's motion; 1 = critical): the
    /// skirts swing out and settle instead of ringing with the stride.
    pub drive_damping: f32,
    /// Quadratic air drag towards the wind (1/m: ½ ρ C_d A / m of a skirt panel, heavy
    /// broadcloth about 0.25): a walk barely moves the hem, a run makes it flap.
    pub drag: f32,
    /// Compliance of the stretch and bending constraints (m/N; 0 = rigid).
    pub stretch_compliance: f32,
    pub bend_compliance: f32,
    /// Across the skirt (one-sided): soft, the skinned skirt spreads with the stride.
    pub across_compliance: f32,
    /// How far (m) a fully free particle may stray from its target (a tether against
    /// tunnelling and teleports).
    pub tether: f32,
    /// Kept above the ground (m).
    pub ground_margin: f32,
    /// Friction on the ground and the legs (0..1 of the tangential motion removed).
    pub friction: f32,
}

impl Default for ClothConfig {
    fn default() -> Self {
        Self {
            substeps: 4,
            gravity: Vec3::new(0.0, 0.0, -9.81),
            drive: [400.0, 40.0],
            drive_damping: 0.7,
            drag: 0.25,
            stretch_compliance: 0.0,
            bend_compliance: 2e-3,
            across_compliance: 1e-3,
            tether: 0.8,
            ground_margin: 0.015,
            friction: 0.6,
        }
    }
}

/// A constraint between two particles: rest length, compliance, one-sided (only resists
/// stretching).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Link {
    i: u32,
    j: u32,
    rest: f32,
    compliance: f32,
    stretch_only: bool,
}

/// A cloth proxy: particles with skinned targets and their constraints.
#[derive(Debug, Clone)]
pub struct Cloth {
    pub pos: Vec<Vec3>,
    pub vel: Vec<Vec3>,
    prev: Vec<Vec3>,
    /// The targets of the last step (their velocity damps the drive).
    last_targets: Vec<Vec3>,
    /// Freedom per particle (`_CLOTH`): 0 pinned to the target, 1 free.
    pub freedom: Vec<f32>,
    links: Vec<Link>,
    pub config: ClothConfig,
}

impl Cloth {
    /// A cloth of `rest` particles (their rest positions) with freedoms `freedom`.
    pub fn new(rest: &[Vec3], freedom: &[f32], config: ClothConfig) -> Cloth {
        assert_eq!(rest.len(), freedom.len());
        Cloth {
            pos: rest.to_vec(),
            vel: vec![Vec3::ZERO; rest.len()],
            prev: rest.to_vec(),
            last_targets: rest.to_vec(),
            freedom: freedom.to_vec(),
            links: Vec::new(),
            config,
        }
    }

    /// A skirt: `columns` × `rows` particles, `rest[c * rows + r]` with row 0 at the waist
    /// (pinned) and the last at the hem; the freedom grows with the row (`_CLOTH` along the
    /// skirt). Columns `c` and `c + 1` are joined (across the skirt, stretch only) unless
    /// `open` says the opening lies between them; down each column the cloth does not
    /// stretch, and bends softly.
    pub fn skirt(rest: &[Vec3], columns: usize, rows: usize, open: &[bool], config: ClothConfig) -> Cloth {
        assert_eq!(rest.len(), columns * rows);
        assert!(rows >= 2);
        let freedom: Vec<f32> = (0..columns * rows).map(|k| (k % rows) as f32 / (rows - 1) as f32).collect();
        let mut c = Cloth::new(rest, &freedom, config);
        let at = |c: usize, r: usize| (c * rows + r) as u32;
        for col in 0..columns {
            for r in 0..rows - 1 {
                c.link(at(col, r), at(col, r + 1), config.stretch_compliance, false);
            }
            for r in 0..rows.saturating_sub(2) {
                c.link(at(col, r), at(col, r + 2), config.bend_compliance, false);
            }
            let next = (col + 1) % columns;
            if next != col && !open.get(col).copied().unwrap_or(false) {
                for r in 1..rows {
                    c.link(at(col, r), at(next, r), config.across_compliance, true);
                }
            }
        }
        c
    }

    /// Adds a distance constraint at the current distance.
    pub fn link(&mut self, i: u32, j: u32, compliance: f32, stretch_only: bool) {
        let rest = (self.pos[i as usize] - self.pos[j as usize]).length();
        self.links.push(Link { i, j, rest, compliance, stretch_only });
    }

    /// Puts every particle on its target at rest (a cloth seen for the first time, or after
    /// a jump).
    pub fn reset(&mut self, targets: &[Vec3]) {
        self.pos.copy_from_slice(targets);
        self.prev.copy_from_slice(targets);
        self.last_targets.copy_from_slice(targets);
        self.vel.iter_mut().for_each(|v| *v = Vec3::ZERO);
    }

    /// Advances by `dt` seconds towards the skinned `targets`; `wind` (m/s) in the world.
    #[allow(clippy::needless_range_loop)] // positions, velocities, freedoms and targets per particle
    pub fn step(&mut self, dt: f32, targets: &[Vec3], wind: Vec3, capsules: &[Capsule], ground: &dyn Ground) {
        let cfg = self.config;
        let n = cfg.substeps.max(1);
        let h = dt / n as f32;
        if h <= 0.0 {
            return;
        }
        for sub in 0..n {
            // The pinned particles move to their targets over the substeps, not in one jump.
            let w = (sub + 1) as f32 / n as f32;
            for k in 0..self.pos.len() {
                let f = self.freedom[k];
                if f <= 0.0 {
                    self.prev[k] = self.pos[k];
                    self.pos[k] = self.last_targets[k] + (targets[k] - self.last_targets[k]) * w;
                    continue;
                }
                let mut v = self.vel[k] + cfg.gravity * (h * f);
                let rel = wind - v;
                v += rel * ((cfg.drag * rel.length() * h).min(1.0) * f);
                // The drive back to the skinned shape (implicit, so it never overshoots),
                // damped towards the target's own motion.
                let kd = cfg.drive[0] + (cfg.drive[1] - cfg.drive[0]) * f;
                let tv = (targets[k] - self.last_targets[k]) * (1.0 / dt);
                v += (tv - v) * (1.0 - (-2.0 * cfg.drive_damping * kd.sqrt() * h).exp());
                let pull = (targets[k] - self.pos[k]) * (kd * h * h / (1.0 + kd * h * h));
                self.prev[k] = self.pos[k];
                self.pos[k] += v * h + pull;
            }
            let h2 = h * h;
            for l in &self.links {
                let (i, j) = (l.i as usize, l.j as usize);
                let wi = if self.freedom[i] > 0.0 { 1.0 } else { 0.0 };
                let wj = if self.freedom[j] > 0.0 { 1.0 } else { 0.0 };
                let w = wi + wj;
                if w == 0.0 {
                    continue;
                }
                let d = self.pos[i] - self.pos[j];
                let len = d.length();
                if len < 1e-6 {
                    continue;
                }
                let c = len - l.rest;
                if l.stretch_only && c <= 0.0 {
                    continue;
                }
                let alpha = l.compliance / h2;
                let dl = -c / (w + alpha);
                let nrm = d * (1.0 / len);
                self.pos[i] += nrm * (dl * wi);
                self.pos[j] -= nrm * (dl * wj);
            }
            for k in 0..self.pos.len() {
                let f = self.freedom[k];
                if f <= 0.0 {
                    continue;
                }
                // Tether: never further from the target than the freedom allows.
                let off = self.pos[k] - targets[k];
                let lim = cfg.tether * f;
                if off.length_sq() > lim * lim {
                    self.pos[k] = targets[k] + off.normalize_or_zero() * lim;
                }
                let mut touched = false;
                for cap in capsules {
                    touched |= cap.push_out(&mut self.pos[k], targets[k]);
                }
                let floor = ground.height(self.pos[k].x, self.pos[k].y) + cfg.ground_margin;
                if self.pos[k].z < floor {
                    self.pos[k].z = floor;
                    touched = true;
                }
                let mut v = (self.pos[k] - self.prev[k]) * (1.0 / h);
                if touched {
                    v = v * (1.0 - cfg.friction);
                }
                self.vel[k] = v;
            }
        }
        self.last_targets.copy_from_slice(targets);
    }

    /// Largest stretch of a non-bunching constraint over its rest length (a test aid).
    pub fn max_stretch(&self) -> f32 {
        self.links
            .iter()
            .filter(|l| l.compliance == 0.0)
            .map(|l| (self.pos[l.i as usize] - self.pos[l.j as usize]).length() / l.rest.max(1e-6) - 1.0)
            .fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat(f32);
    impl Ground for Flat {
        fn height(&self, _x: f32, _y: f32) -> f32 {
            self.0
        }
    }

    /// Four columns round the hips (front left, back left, back right, front right) of three
    /// rows (waist, knee, hem), open in front, as the coat skirts of skeleton H.
    fn skirt_rest() -> Vec<Vec3> {
        let cols = [(0.125, -0.115), (0.12, 0.125), (-0.12, 0.125), (-0.125, -0.115)];
        let mut v = Vec::new();
        for (x, y) in cols {
            v.push(Vec3::new(x, y, 1.0));
            v.push(Vec3::new(x * 1.08, y * 1.1, 0.78));
            v.push(Vec3::new(x * 1.16, y * 1.2, 0.55));
        }
        v
    }

    fn skirt() -> Cloth {
        Cloth::skirt(&skirt_rest(), 4, 3, &[false, false, false, true], ClothConfig::default())
    }

    fn legs(dx: f32) -> Vec<Capsule> {
        vec![
            Capsule { a: Vec3::new(0.09 + dx, 0.0, 0.95), b: Vec3::new(0.09 + dx, 0.0, 0.5), r: 0.08 },
            Capsule { a: Vec3::new(-0.09 + dx, 0.0, 0.95), b: Vec3::new(-0.09 + dx, 0.0, 0.5), r: 0.08 },
        ]
    }

    #[test]
    fn a_man_standing_keeps_his_drape() {
        let rest = skirt_rest();
        let mut c = skirt();
        for _ in 0..120 {
            c.step(1.0 / 60.0, &rest, Vec3::ZERO, &legs(0.0), &Flat(0.0));
        }
        // The hem swings a little towards the vertical, no more.
        for (p, r) in c.pos.iter().zip(&rest) {
            assert!((*p - *r).length() < 0.04, "{p:?} strayed from {r:?}");
        }
    }

    /// The largest trail (m) of the front hem behind its target, the body speeding up to
    /// `speed` (m/s, forward −Y) over half a second and keeping it.
    fn trail(speed: f32) -> (f32, Cloth) {
        let rest = skirt_rest();
        let mut c = skirt();
        let (mut lag, mut y) = (0.0f32, 0.0);
        for f in 0..150 {
            let v = speed * (f as f32 / 30.0).min(1.0);
            y -= v / 60.0;
            let targets: Vec<Vec3> = rest.iter().map(|p| *p + Vec3::new(0.0, y, 0.0)).collect();
            c.step(1.0 / 60.0, &targets, Vec3::ZERO, &[], &Flat(0.0));
            lag = lag.max(c.pos[2].y - targets[2].y);
        }
        (lag, c)
    }

    #[test]
    fn walking_swings_the_skirt_a_little_running_makes_it_flap() {
        let (walk, c) = trail(1.3);
        assert!(walk > 0.005 && walk < 0.1, "walking: the hem trails {walk} m");
        assert!(c.max_stretch() < 0.05, "stretch {}", c.max_stretch());
        let (run, c) = trail(4.5);
        assert!(run > 2.0 * walk && run < 0.3, "running: the hem trails {run} m (walking {walk})");
        assert!(c.max_stretch() < 0.05, "stretch {}", c.max_stretch());
    }

    #[test]
    fn a_fallen_mans_skirt_drapes_to_the_ground() {
        // Lying on his back (turned −90° about X, his hips 0.15 m up): the skirt targets
        // stick out level towards his feet, the front ones in the air; gravity lays them on
        // the ground.
        let rest = skirt_rest();
        let lie = |p: Vec3| Vec3::new(p.x, p.z, 0.15 - p.y);
        let targets: Vec<Vec3> = rest.iter().map(|p| lie(*p)).collect();
        let mut c = skirt();
        c.config.drive[1] = 6.0;
        c.reset(&targets);
        for _ in 0..240 {
            c.step(1.0 / 60.0, &targets, Vec3::ZERO, &[], &Flat(0.0));
        }
        for k in [2, 5, 8, 11] {
            let p = c.pos[k];
            assert!(p.z < targets[k].z - 0.05 || p.z < 0.05, "hem {k} did not sag: {p:?} target {:?}", targets[k]);
            assert!(p.z >= 0.0, "hem {k} under the ground: {p:?}");
        }
        for k in [0, 3, 6, 9] {
            assert!((c.pos[k] - targets[k]).length() < 1e-5, "the waist follows the skin");
        }
    }

    #[test]
    fn the_legs_push_the_skirt_out() {
        let rest = skirt_rest();
        let mut c = skirt();
        // A leg stepping forward into the front skirt.
        let caps = [Capsule { a: Vec3::new(0.12, -0.05, 0.95), b: Vec3::new(0.14, -0.25, 0.55), r: 0.09 }];
        for _ in 0..60 {
            c.step(1.0 / 60.0, &rest, Vec3::ZERO, &caps, &Flat(0.0));
        }
        for p in &c.pos {
            let ab = caps[0].b - caps[0].a;
            let t = ((*p - caps[0].a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0);
            let d = (*p - (caps[0].a + ab * t)).length();
            assert!(d >= caps[0].r - 1e-3 || p.z > 0.99, "particle {p:?} inside the leg ({d})");
        }
    }

    #[test]
    fn wind_lifts_the_hem() {
        let rest = skirt_rest();
        let mut c = skirt();
        for _ in 0..120 {
            c.step(1.0 / 60.0, &rest, Vec3::new(0.0, 6.0, 0.0), &[], &Flat(0.0));
        }
        assert!(c.pos[2].y > rest[2].y + 0.01, "the front hem blown back: {:?}", c.pos[2]);
        assert!(c.pos.iter().all(|p| p.is_finite()));
    }
}
