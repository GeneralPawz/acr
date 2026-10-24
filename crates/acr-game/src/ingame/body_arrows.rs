//! Arrows, bolts and thrown spears stuck in the men they hit (#308, docs/rendering-3d.md
//! "Projectiles"), render only: each hit of an arrow (and of a thrown spear seen flying in)
//! leaves its shaft in the body part it struck (`HitRecord::injury`, else a part hashed from
//! the hit), on that part's bone in the rest pose, pointing back along the flight with a
//! third of it buried. Drawn through the body's palette (clip, crew or ragdoll pose), so it
//! follows him through his clips, his fall and his corpse, and goes with the corpse. The
//! original heals no wounds, so the wounded keep theirs. At most [`PER_MAN`] per man and
//! [`CAP`] in all (the oldest go). The same hit always sticks the same way.

use std::collections::{HashMap, VecDeque};

use acr_render::damage::{Damage, Rig, WHOLE};
use acr_render::math::{self, Mat4, Vec3};
use acr_render::missiles::ARROW_LENGTH;
use acr_render::model::{ModelData, Palette};
use acr_render::model_renderer::ModelInstance;
use acr_render::view3d::{self, PIXELS_PER_METRE};
use acr_sim::body::Part;
use acr_sim::combat::{DeathCause, HitRecord};

/// Most shafts stuck in one man, and in all.
pub const PER_MAN: usize = 6;
pub const CAP: usize = 1536;
/// A spear's model: its tip this far ahead of its origin (`missiles::spear`), and how deep
/// it goes in (metres).
const SPEAR_TIP: f32 = 1.05;
const SPEAR_DEPTH: f32 = 0.3;

/// Where on the body a shaft sits.
#[derive(Debug, Clone, Copy, PartialEq)]
enum At {
    /// Limb (0..4: arm_l, arm_r, leg_l, leg_r) at level 0..1 from its root.
    Limb(usize, f32),
    /// Trunk at level 0 (hips) .. 1 (neck).
    Trunk(f32),
    Head,
}

/// One shaft in a man: the hit it came with, where it sits, its flight in his model frame
/// (glTF axes, unit) and its roll.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stuck {
    unit: u32,
    tick: u64,
    at: At,
    flight: Vec3,
    /// Around the bone from the flight (radians), so a volley's shafts don't line up.
    offset: f32,
    roll: f32,
    spear: bool,
}

#[derive(Default)]
pub struct BodyArrows {
    /// Oldest first.
    list: VecDeque<Stuck>,
    /// The newest hit taken (a step records its hits under the tick it began with).
    seen: Option<u64>,
    /// Per unit: the pose last sampled (model, clip, time) and its palette, so a corpse
    /// holding its last frame is not sampled again every frame.
    poses: HashMap<u32, (usize, Option<usize>, u32, Palette)>,
}

/// 0..1 from a hit and a salt.
fn hash(tick: u64, unit: u32, salt: u64) -> f32 {
    let mut h = u64::from(unit).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ tick.wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ salt.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Where a hit sticks: the injury's part and level, else (no hit locations) a part from
/// the hit's hash, most often the trunk.
fn at_of(hit: &HitRecord) -> At {
    let h = |salt| hash(hit.tick, hit.victim, salt);
    match hit.injury {
        Some(i) => {
            let level = f32::from(i.level) / 255.0;
            match i.part {
                Part::Limb(l) => At::Limb(l as usize, level.clamp(0.08, 0.85)),
                // The sim counts the trunk from the shoulders, the renderer from the hips.
                Part::Chest | Part::Belly | Part::Pelvis => At::Trunk((1.0 - level).clamp(0.12, 0.9)),
                Part::Head => At::Head,
            }
        }
        None => {
            let r = h(20);
            if r < 0.55 {
                At::Trunk(0.2 + 0.65 * h(21))
            } else if r < 0.92 {
                At::Limb((h(22) * 4.0) as usize % 4, 0.12 + 0.6 * h(23))
            } else {
                At::Head
            }
        }
    }
}

impl BodyArrows {
    pub fn clear(&mut self) {
        self.list.clear();
        self.poses.clear();
        self.seen = None;
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Takes the new arrow hits up to `now`. `facing` gives the victim's `RealDir`;
    /// `flight` the world direction (true height) of an arrow (`false`) or spear (`true`)
    /// seen flying near a point, if any. A thrown hit sticks only as a spear seen in flight.
    pub fn take_hits<'a>(
        &mut self,
        hits: impl Iterator<Item = &'a HitRecord>,
        now: u64,
        facing: &dyn Fn(u32) -> Option<u8>,
        flight: &dyn Fn([f32; 2], bool) -> Option<Vec3>,
    ) {
        if self.seen.is_some_and(|s| s > now) {
            self.clear();
        }
        let seen = self.seen;
        for hit in hits.filter(|h| seen.is_none_or(|s| h.tick > s) && h.tick <= now) {
            self.seen = Some(self.seen.map_or(hit.tick, |s| s.max(hit.tick)));
            if hit.building || !matches!(hit.cause, DeathCause::Arrow | DeathCause::Thrown) {
                continue;
            }
            let Some(dir) = facing(hit.victim) else { continue };
            let at = [hit.pos.0 as f32, hit.pos.1 as f32];
            let spear = hit.cause == DeathCause::Thrown;
            let seen_flying = flight(at, spear);
            if spear && seen_flying.is_none() {
                continue;
            }
            let h = |salt| hash(hit.tick, hit.victim, salt);
            // The flight in the world: as seen, else along the blow coming down at 8..30°.
            let world = seen_flying.unwrap_or_else(|| {
                let (dx, dy) = (hit.dir.0 as f32, hit.dir.1 as f32);
                let l = (dx * dx + dy * dy).sqrt();
                let (dx, dy) = if l > 1e-3 {
                    (dx / l, dy / l)
                } else {
                    let a = h(30) * std::f32::consts::TAU;
                    (a.cos(), a.sin())
                };
                let fall = 0.14 + 0.38 * h(31);
                [dx * fall.cos(), dy * fall.cos(), -fall.sin()]
            });
            // Into his model frame (the facing he had when struck) and turned a little as it
            // struck.
            let m = view3d::model_matrix([0.0; 3], view3d::facing_yaw(dir), 1.0);
            let k = 1.0 / (PIXELS_PER_METRE * PIXELS_PER_METRE);
            let f = [0, 1, 2].map(|c| math::dot([m[c][0], m[c][1], m[c][2]], world) * k);
            let wobble = [h(32) - 0.5, h(33) - 0.5, h(34) - 0.5].map(|w| w * 0.3);
            let flight = normalize(math::add(f, wobble));
            let stuck = Stuck {
                unit: hit.victim,
                tick: hit.tick,
                at: at_of(hit),
                flight,
                offset: (h(35) - 0.5) * 1.4,
                roll: h(36) * std::f32::consts::TAU,
                spear,
            };
            // Six in a man at most: his oldest goes.
            if self.list.iter().filter(|s| s.unit == hit.victim).count() >= PER_MAN
                && let Some(i) = self.list.iter().position(|s| s.unit == hit.victim)
            {
                self.list.remove(i);
            }
            if self.list.len() >= CAP {
                self.list.pop_front();
            }
            self.list.push_back(stuck);
        }
    }

    /// Keeps only the shafts of units `keep` says are still drawn.
    pub fn retain(&mut self, keep: &dyn Fn(u32) -> bool) {
        self.list.retain(|s| keep(s.unit));
        self.poses.retain(|&u, _| keep(u));
    }

    /// This frame's shafts appended to `out`: `drawn` gives a unit's instance index,
    /// `model` the model data, `poses` the custom palettes (crews, ragdolls) by index.
    pub fn draw<'a>(
        &mut self,
        drawn: &HashMap<u32, usize>,
        model: &dyn Fn(usize) -> Option<&'a ModelData>,
        poses: &[&HashMap<usize, Palette>],
        models: (usize, usize),
        out: &mut Vec<ModelInstance>,
    ) {
        if self.list.is_empty() {
            return;
        }
        acr_sim::prof_scope!("3d.body_arrows");
        let mut units: Vec<u32> = self.list.iter().map(|s| s.unit).collect();
        units.sort_unstable();
        units.dedup();
        for unit in units {
            let Some(&index) = drawn.get(&unit) else { continue };
            let Some(&inst) = out.get(index) else { continue };
            let Some(data) = model(inst.model) else { continue };
            let Some(rig) = data.rig.as_ref() else { continue };
            let custom = poses.iter().find_map(|p| p.get(&index));
            let palette = match custom {
                Some(p) => p,
                None => {
                    let key = (inst.model, inst.clip, inst.time.to_bits());
                    let e = self.poses.entry(unit).or_insert_with(|| (usize::MAX, None, 0, Vec::new()));
                    if (e.0, e.1, e.2) != key {
                        *e = (key.0, key.1, key.2, data.pose(inst.clip, inst.time));
                    }
                    &e.3
                }
            };
            for s in self.list.iter().filter(|s| s.unit == unit) {
                let Some((slot, rest)) = place(s, rig, &inst.damage) else { continue };
                let Some(rows) = palette.get(slot as usize) else { continue };
                let bone = from_rows(rows);
                let m = math::mul(&inst.transform, &math::mul(&bone, &rest));
                out.push(arrow(if s.spear { models.1 } else { models.0 }, m));
            }
        }
    }
}

/// The palette slot a shaft rides on and its matrix in the rest pose (metres; the model's
/// tip at the origin, shaft back along −z for an arrow), or `None` where the part is gone.
fn place(s: &Stuck, rig: &Rig, damage: &Damage) -> Option<(u32, Mat4)> {
    let (centre, along, radius, slot) = match s.at {
        At::Limb(l, level) => {
            if damage.limbs[l].cut != WHOLE && f32::from(damage.limbs[l].cut) / 255.0 <= level {
                return None;
            }
            let chain = rig.limbs.get(l)?;
            let (p, d, slot) = chain.at(level);
            (p, d, chain.radius_at(level).max(0.035), slot)
        }
        At::Trunk(level) => {
            if damage.trunk_cut.is_some() {
                return None;
            }
            let (p, d, slot) = rig.trunk.at(level);
            (p, d, rig.trunk.radius_at(level).max(0.09), slot)
        }
        At::Head => {
            if damage.head_gone {
                return None;
            }
            (math::add(rig.neck.0, [0.0, 0.11, 0.0]), [0.0, 1.0, 0.0], rig.head_radius, rig.neck.1)
        }
    };
    // It comes in from the side it flew at, square to the bone, turned around the bone by
    // its offset; the shaft keeps its flight.
    let f = s.flight;
    let mut o = math::sub(f, math::scale(along, math::dot(f, along)));
    if math::dot(o, o) < 1e-4 {
        o = [0.0, 0.0, -1.0];
    }
    let o = rotate_about(normalize(o), along, s.offset);
    let entry = math::sub(centre, math::scale(o, radius));
    let length = if s.spear { 2.05 } else { ARROW_LENGTH };
    // A third in; through a limb or the head the point shows on the far side.
    let depth = match s.at {
        At::Trunk(_) => length / 3.0,
        At::Head => (length / 3.0).min(1.7 * radius),
        At::Limb(..) => (length / 3.0).min(2.0 * radius + 0.05),
    };
    let depth = if s.spear { SPEAR_DEPTH.min(depth) } else { depth };
    let tip = math::add(entry, math::scale(f, depth));
    let z = f;
    let up = if z[1].abs() > 0.95 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let x0 = normalize(cross(up, z));
    let y0 = cross(z, x0);
    let (rs, rc) = s.roll.sin_cos();
    let x = math::add(math::scale(x0, rc), math::scale(y0, rs));
    let y = cross(z, x);
    let origin = if s.spear { math::sub(tip, math::scale(z, SPEAR_TIP)) } else { tip };
    Some((
        slot,
        [
            [x[0], x[1], x[2], 0.0],
            [y[0], y[1], y[2], 0.0],
            [z[0], z[1], z[2], 0.0],
            [origin[0], origin[1], origin[2], 1.0],
        ],
    ))
}

/// A palette entry (affine rows) as a column-major matrix.
fn from_rows(r: &[[f32; 4]; 3]) -> Mat4 {
    [
        [r[0][0], r[1][0], r[2][0], 0.0],
        [r[0][1], r[1][1], r[2][1], 0.0],
        [r[0][2], r[1][2], r[2][2], 0.0],
        [r[0][3], r[1][3], r[2][3], 1.0],
    ]
}

/// `v` turned by `a` radians about the unit axis `k` (Rodrigues).
fn rotate_about(v: Vec3, k: Vec3, a: f32) -> Vec3 {
    let (s, c) = a.sin_cos();
    let kv = cross(k, v);
    let kd = math::dot(k, v) * (1.0 - c);
    [0, 1, 2].map(|i| v[i] * c + kv[i] * s + k[i] * kd)
}

fn arrow(model: usize, transform: Mat4) -> ModelInstance {
    ModelInstance {
        model,
        clip: None,
        time: 0.0,
        transform,
        decay: 0.0,
        morphs: [0.0; 3],
        transparent: false,
        player: 0,
        wounds: Default::default(),
        sprayed: Default::default(),
        damage: Default::default(),
        look: None,
        uniform: None,
        drape: None,
        rise: None,
    }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: Vec3) -> Vec3 {
    let l = math::dot(v, v).sqrt();
    if l > 1e-6 { math::scale(v, 1.0 / l) } else { [0.0, 0.0, 1.0] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acr_sim::body::{Injury, LimbId, Outcome};

    fn hit(tick: u64, victim: u32, part: Part) -> HitRecord {
        HitRecord {
            tick,
            victim,
            pos: (1000, 1000),
            dir: (100, 0),
            cause: DeathCause::Arrow,
            killed: false,
            building: false,
            region: None,
            injury: Some(Injury {
                part,
                level: 120,
                outcome: Outcome::Wound,
                energy: 60,
                cause: acr_sim::body::Cause::Musket,
            }),
            strike: None,
            critical: None,
        }
    }

    #[test]
    fn arrows_stick_six_to_a_man_and_the_same_hit_sticks_the_same_way() {
        let mut a = BodyArrows::default();
        let hits: Vec<_> = (1..10).map(|t| hit(t, 4, Part::Limb(LimbId::LegL))).collect();
        a.take_hits(hits.iter(), 10, &|_| Some(0), &|_, _| None);
        assert_eq!(a.len(), PER_MAN);
        assert_eq!(a.list.front().unwrap().tick, 4);
        let mut b = BodyArrows::default();
        b.take_hits(hits.iter(), 10, &|_| Some(0), &|_, _| None);
        assert_eq!(a.list, b.list);
        // Taken once; a thrown hit without a spear seen flying leaves nothing.
        a.take_hits(hits.iter(), 11, &|_| Some(0), &|_, _| None);
        assert_eq!(a.len(), PER_MAN);
        let thrown = HitRecord { cause: DeathCause::Thrown, tick: 12, ..hits[0] };
        a.take_hits([thrown].iter(), 12, &|_| Some(0), &|_, _| None);
        assert_eq!(a.len(), PER_MAN);
        a.retain(&|u| u != 4);
        assert_eq!(a.len(), 0);
    }

    #[test]
    fn the_shaft_points_along_the_flight_and_its_tip_is_inside() {
        let mut a = BodyArrows::default();
        a.take_hits([hit(1, 2, Part::Chest)].iter(), 1, &|_| Some(0), &|_, _| None);
        let s = a.list[0];
        // A rough trunk: hips to neck up y, 0.15 m thick.
        let rig = Rig {
            limbs: std::array::from_fn(|_| acr_render::damage::Chain {
                points: vec![(0.0, [0.2, 1.4, 0.0], 1), (1.0, [0.2, 0.8, 0.0], 2)],
                radius: [0.05; acr_render::damage::RADIUS_SAMPLES],
            }),
            trunk: acr_render::damage::Chain {
                points: vec![(0.0, [0.0, 0.95, 0.0], 3), (1.0, [0.0, 1.5, 0.0], 4)],
                radius: [0.15; acr_render::damage::RADIUS_SAMPLES],
            },
            neck: ([0.0, 1.5, 0.0], 5),
            head_radius: 0.1,
        };
        let (slot, m) = place(&s, &rig, &Damage::default()).unwrap();
        assert_eq!(slot, 3);
        let tip = [m[3][0], m[3][1], m[3][2]];
        let r = (tip[0] * tip[0] + tip[2] * tip[2]).sqrt();
        assert!(r < 0.15, "tip {tip:?}");
        let nock = math::transform_point(&m, [0.0, 0.0, -ARROW_LENGTH]);
        let rn = (nock[0] * nock[0] + nock[2] * nock[2]).sqrt();
        assert!(rn > 0.3, "nock {nock:?}");
        // Cut through the trunk: nothing to stick in.
        let cut = Damage { trunk_cut: Some(100), ..Damage::default() };
        assert!(place(&s, &rig, &cut).is_none());
    }
}
