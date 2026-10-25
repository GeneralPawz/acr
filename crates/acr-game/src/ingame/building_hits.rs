//! Hit damage on buildings in 3D (#304, `docs/rendering-3d.md` "Hit damage on buildings"):
//! the rounds that struck a building ([`acr_sim::combat::HitRecord`] with `building`) leave
//! scars on its model and arrows stuck in it, and the whole model cracks and blackens as it
//! loses its life. Render only: every scar comes from the hit records and a hash of the hit
//! (victim, tick, its place among that tick's hits), never from the simulation's RNG.
//!
//! A hit is placed by a ray against the model's triangles (its pose at time 0, model frame,
//! metres): through the point the round struck (`World::hit_at`; the building's centre for a
//! scripted hit or one without it, then spread over the face), along the round's flight,
//! level at a hashed height for balls and shot, plunging for arrows. The first face it meets
//! takes the scar ([`Scar`]): a ball's hole, a pock, a shell's scorch or an arrow's puncture.
//! A building keeps [`MAX_SCARS`] scars (pocks go first, then scorches, holes last) and
//! [`MAX_ARROWS`] arrows (the oldest go). A repair takes away the oldest in the share of the
//! damage it mends, all at full life.

use std::collections::HashMap;

use acr_render::math::{self, Mat4, Vec3};
use acr_render::model::ModelData;
use acr_render::model_renderer::{MAX_SCARS, ModelInstance, Scar, ScarKind, Scars};
use acr_sim::World;
use acr_sim::combat::DeathCause;
use acr_sim::world::Unit;

/// Most arrows kept stuck in one building.
pub const MAX_ARROWS: usize = 24;
/// How far an arrow's tip sinks into the wall (metres).
const ARROW_DEPTH: f32 = 0.07;
/// From this share of its life lost on, a building cracks and blackens all over (full at 0).
const BATTERED_FROM: f32 = 0.5;
/// Soot marks over the fires of a burning building (#303), within [`MAX_SCARS`].
const MAX_FIRE_SOOT: usize = 10;

/// A hit not yet placed: the building's model may not be loaded.
#[derive(Debug, Clone, Copy)]
struct Pending {
    pos: (i32, i32),
    dir: (i32, i32),
    cause: DeathCause,
    seed: u32,
}

/// An arrow stuck in a building: its tip and flight direction (model frame, metres).
#[derive(Debug, Clone, Copy)]
struct Arrow {
    tip: Vec3,
    dir: Vec3,
}

#[derive(Debug, Default)]
struct Building {
    pending: Vec<Pending>,
    scars: Vec<Scar>,
    arrows: Vec<Arrow>,
    /// Its life when last drawn (a rise is a repair).
    life: i32,
}

/// A model's triangles in its pose at time 0 (model frame, metres), for placing hits.
struct Mesh {
    tris: Vec<[Vec3; 3]>,
    min: Vec3,
    max: Vec3,
}

/// The hit damage of every building drawn (#304).
#[derive(Default)]
pub struct BuildingHits {
    /// The tick of the last update: going back means another game.
    tick: u64,
    /// The newest hit tick taken and how many building hits of that tick were taken.
    last: u64,
    taken: usize,
    buildings: HashMap<u32, Building>,
    /// By model index.
    meshes: HashMap<usize, Mesh>,
}

impl BuildingHits {
    /// Takes the hits on buildings recorded since the last call and forgets the buildings
    /// that are gone or fell.
    pub fn take(&mut self, world: &World) {
        let now = world.tick.0;
        if now < self.tick {
            *self = Self { meshes: std::mem::take(&mut self.meshes), ..Self::default() };
        }
        self.tick = now;
        // A step records its hits under the tick it began with: count the ones of the
        // newest tick already taken instead of skipping by tick.
        let (mut tick, mut count) = (self.last, 0usize);
        let mut any = false;
        for h in world.hits.iter().filter(|h| h.building && h.tick >= self.last) {
            any = true;
            if h.tick != tick {
                (tick, count) = (h.tick, 0);
            }
            count += 1;
            if h.tick == self.last && count <= self.taken {
                continue;
            }
            if !matches!(
                h.cause,
                DeathCause::Cannonball
                    | DeathCause::Grapeshot
                    | DeathCause::Gunshot
                    | DeathCause::Arrow
                    | DeathCause::Explosion
            ) {
                continue;
            }
            let b = self.buildings.entry(h.victim).or_default();
            if b.pending.len() < 2 * MAX_SCARS {
                let seed = hash(h.victim ^ hash(h.tick as u32 ^ hash(count as u32 ^ 0xb11d)));
                b.pending.push(Pending { pos: h.pos, dir: h.dir, cause: h.cause, seed });
            }
        }
        if any {
            (self.last, self.taken) = (tick, count);
        }
        self.buildings.retain(|&v, _| {
            world.units.get(v as usize).is_some_and(|u| u.id.0 == v && u.building && u.alive && u.death == 0)
        });
    }

    /// The damage of building `u` drawn as `inst` (model `index`, `model`): its scars for
    /// the renderer, and the transforms of its stuck arrows pushed to `arrows`. `fire`: the
    /// points where it burns (model frame, metres) and how hard (0..1,
    /// `ruin_view::Look::sooted`): soot climbs the wall over each. `None` when it shows none.
    pub fn place(
        &mut self,
        u: &Unit,
        index: usize,
        model: &ModelData,
        inst: &ModelInstance,
        fire: &[(Vec3, f32)],
        arrows: &mut Vec<Mat4>,
    ) -> Option<Scars> {
        let lost = 1.0 - u.life.max(0) as f32 / u.max_life.max(1) as f32;
        let batter = ((lost - BATTERED_FROM) / (1.0 - BATTERED_FROM)).clamp(0.0, 1.0);
        if u.death > 0 || (batter <= 0.0 && fire.is_empty() && !self.buildings.contains_key(&u.id.0)) {
            return None;
        }
        let b = self.buildings.entry(u.id.0).or_default();
        // A repair mends the oldest damage first, in the share of the lost life it mends.
        if b.life > 0 && u.life > b.life {
            let before = 1.0 - b.life as f32 / u.max_life.max(1) as f32;
            let keep = if before > 0.0 { (lost / before).clamp(0.0, 1.0) } else { 0.0 };
            let drop = |len: usize| len - (len as f32 * keep).round() as usize;
            b.scars.drain(..drop(b.scars.len()));
            b.arrows.drain(..drop(b.arrows.len()));
        }
        b.life = u.life;
        if !b.pending.is_empty() {
            let mesh = self.meshes.entry(index).or_insert_with(|| Mesh::of(model, inst.clip));
            let inverse = affine_inverse(&inst.transform);
            let centre = (u.pos.x >> 4, u.pos.y >> 4);
            for p in std::mem::take(&mut b.pending) {
                place_hit(b, mesh, &inst.transform, &inverse, centre, p);
            }
        }
        for a in &b.arrows {
            arrows.push(math::mul(&inst.transform, &arrow_matrix(a)));
        }
        if b.scars.is_empty() && batter <= 0.0 && fire.is_empty() {
            return None;
        }
        // Fire soot first (at most `MAX_FIRE_SOOT`), then the newest hits in what is left.
        let fire = &fire[..fire.len().min(MAX_FIRE_SOOT)];
        let keep = MAX_SCARS - fire.len();
        let mut list: Vec<Scar> = b.scars[b.scars.len().saturating_sub(keep)..].to_vec();
        let mut heat = 0.0f32;
        for (k, &(p, strength)) in fire.iter().enumerate() {
            let out = [p[0], 0.0, p[2]];
            let len = math::dot(out, out).sqrt();
            let normal = if len > 1e-4 { math::scale(out, 1.0 / len) } else { [0.0, 1.0, 0.0] };
            let seed = (hash(u.id.0 ^ hash(k as u32 ^ 0x500d)) & 0xffff) as f32 / 65535.0;
            list.push(Scar { point: p, normal, radius: 0.5 + 0.9 * strength, kind: ScarKind::Scorch, seed });
            heat = heat.max(strength);
        }
        Some(Scars { list, batter, soot: (batter * 0.9).max(heat * 0.45) })
    }
}

/// Places one hit on building `b` drawn with `transform` (`inverse` its inverse); `centre`
/// is the building's ground point (world pixels): a hit recorded there struck somewhere on
/// the face towards the shooter.
fn place_hit(b: &mut Building, mesh: &Mesh, transform: &Mat4, inverse: &Mat4, centre: (i32, i32), p: Pending) {
    let r = |k: u32| (hash(p.seed ^ k.wrapping_mul(0x9e37_79b9)) & 0xffff) as f32 / 65535.0;
    let height = mesh.max[1] - mesh.min[1];
    if height <= 0.0 || mesh.tris.is_empty() {
        return;
    }
    // The round's flight in the model frame, level (the record has no height).
    let mut d = math::transform_vector(inverse, [p.dir.0 as f32, p.dir.1 as f32, 0.0]);
    d[1] = 0.0;
    let len = math::dot(d, d).sqrt();
    let d = if len > 1e-6 {
        math::scale(d, 1.0 / len)
    } else {
        let a = r(1) * std::f32::consts::TAU;
        [a.cos(), 0.0, a.sin()]
    };
    let arrow = p.cause == DeathCause::Arrow;
    let (y, drop) = if arrow {
        (mesh.min[1] + height * (0.25 + 0.7 * r(2)), -(0.25 + 0.55 * r(3)))
    } else {
        (mesh.min[1] + height * (0.12 + 0.5 * r(2)), 0.0)
    };
    let ground = transform[3][2];
    let mut at = math::transform_point(inverse, [p.pos.0 as f32, p.pos.1 as f32, ground]);
    // Spread across the flight: over the whole face for a hit recorded at the centre.
    let half = 0.5 * (mesh.max[0] - mesh.min[0]).max(mesh.max[2] - mesh.min[2]);
    let spread = if p.pos == centre { 0.8 * half } else { 0.4 };
    let side = [-d[2], 0.0, d[0]];
    at = math::add(at, math::scale(side, spread * (2.0 * r(4) - 1.0)));
    at[1] = y;
    let fly = normalize([d[0], drop, d[2]]);
    let reach = 2.0 * math::dot(math::sub(mesh.max, mesh.min), math::sub(mesh.max, mesh.min)).sqrt() + 1.0;
    let from = math::sub(at, math::scale(fly, reach));
    let Some((point, normal)) = mesh.ray(from, fly) else { return };
    let (kind, radius) = match p.cause {
        DeathCause::Cannonball => (ScarKind::Hole, 0.2 + 0.1 * r(5)),
        DeathCause::Grapeshot => (ScarKind::Pock, 0.06 + 0.03 * r(5)),
        DeathCause::Gunshot => (ScarKind::Pock, 0.035 + 0.02 * r(5)),
        DeathCause::Explosion => (ScarKind::Scorch, 0.7 + 0.4 * r(5)),
        _ => (ScarKind::Puncture, 0.022),
    };
    if b.scars.len() >= MAX_SCARS {
        let rank = |k: ScarKind| match k {
            ScarKind::Pock | ScarKind::Puncture => 0,
            ScarKind::Scorch => 1,
            ScarKind::Hole => 2,
        };
        let low = b.scars.iter().map(|s| rank(s.kind)).min().unwrap_or(0);
        if let Some(i) = b.scars.iter().position(|s| rank(s.kind) == low) {
            b.scars.remove(i);
        }
    }
    b.scars.push(Scar { point, normal, radius, kind, seed: r(6) });
    if arrow {
        if b.arrows.len() >= MAX_ARROWS {
            b.arrows.remove(0);
        }
        // It sticks a little off its flight, as it turned on striking.
        let wobble = [(r(7) - 0.5) * 0.25, (r(8) - 0.5) * 0.25, (r(9) - 0.5) * 0.25];
        b.arrows.push(Arrow { tip: point, dir: normalize(math::add(fly, wobble)) });
    }
}

/// An arrow's model ([`acr_render::damage::procedural_arrow`]: tip at the origin, shaft
/// along −z) placed in the building's model frame.
fn arrow_matrix(a: &Arrow) -> Mat4 {
    let z = a.dir;
    let up = if z[1].abs() > 0.95 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let x = normalize(cross(up, z));
    let y = cross(z, x);
    let t = math::add(a.tip, math::scale(z, ARROW_DEPTH));
    [[x[0], x[1], x[2], 0.0], [y[0], y[1], y[2], 0.0], [z[0], z[1], z[2], 0.0], [t[0], t[1], t[2], 1.0]]
}

impl Mesh {
    fn of(model: &ModelData, clip: Option<usize>) -> Self {
        let palette = model.pose(clip, 0.0);
        let mut tris = Vec::new();
        let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
        for prim in &model.primitives {
            let placed: Vec<Vec3> = prim
                .vertices
                .iter()
                .map(|v| {
                    let mut out = [0.0; 3];
                    for k in 0..4 {
                        let w = v.weights[k];
                        let Some(rows) = palette.get(v.joints[k] as usize).filter(|_| w > 0.0) else { continue };
                        for (c, row) in rows.iter().enumerate() {
                            out[c] += w * (math::dot([row[0], row[1], row[2]], v.position) + row[3]);
                        }
                    }
                    out
                })
                .collect();
            for p in &placed {
                for c in 0..3 {
                    min[c] = min[c].min(p[c]);
                    max[c] = max[c].max(p[c]);
                }
            }
            for t in prim.indices.as_chunks::<3>().0 {
                let get = |i: u32| placed.get(i as usize).copied();
                if let (Some(a), Some(b), Some(c)) = (get(t[0]), get(t[1]), get(t[2])) {
                    tris.push([a, b, c]);
                }
            }
        }
        if tris.is_empty() {
            (min, max) = ([0.0; 3], [0.0; 3]);
        }
        Self { tris, min, max }
    }

    /// The first face the ray from `from` along `dir` (unit) meets: the point and the face's
    /// normal turned against the ray (Möller–Trumbore, both sides).
    fn ray(&self, from: Vec3, dir: Vec3) -> Option<(Vec3, Vec3)> {
        let mut best: Option<(f32, Vec3)> = None;
        for [a, b, c] in &self.tris {
            let e1 = math::sub(*b, *a);
            let e2 = math::sub(*c, *a);
            let p = cross(dir, e2);
            let det = math::dot(e1, p);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            let s = math::sub(from, *a);
            let u = math::dot(s, p) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = cross(s, e1);
            let v = math::dot(dir, q) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = math::dot(e2, q) * inv;
            if t > 0.0 && best.is_none_or(|(bt, _)| t < bt) {
                let n = normalize(cross(e1, e2));
                best = Some((t, if math::dot(n, dir) > 0.0 { math::scale(n, -1.0) } else { n }));
            }
        }
        best.map(|(t, n)| (math::add(from, math::scale(dir, t)), n))
    }
}

/// The inverse of an affine matrix (columns; the last row 0, 0, 0, 1).
fn affine_inverse(m: &Mat4) -> Mat4 {
    let [a, b, c] = [m[0], m[1], m[2]].map(|v| [v[0], v[1], v[2]]);
    let (x, y, z) = (cross(b, c), cross(c, a), cross(a, b));
    let det = math::dot(a, x);
    let k = if det.abs() > 1e-12 { 1.0 / det } else { 0.0 };
    // Rows of the inverse 3 × 3 are x, y, z over det.
    let r = [math::scale(x, k), math::scale(y, k), math::scale(z, k)];
    let t = [m[3][0], m[3][1], m[3][2]];
    let ti = [-math::dot(r[0], t), -math::dot(r[1], t), -math::dot(r[2], t)];
    [
        [r[0][0], r[1][0], r[2][0], 0.0],
        [r[0][1], r[1][1], r[2][1], 0.0],
        [r[0][2], r[1][2], r[2][2], 0.0],
        [ti[0], ti[1], ti[2], 1.0],
    ]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: Vec3) -> Vec3 {
    let l = math::dot(v, v).sqrt();
    if l > 1e-9 { math::scale(v, 1.0 / l) } else { [0.0, 0.0, 1.0] }
}

fn hash(x: u32) -> u32 {
    let mut h = x.wrapping_mul(0x85eb_ca6b) ^ 0x2545_f491;
    h ^= h >> 15;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^ (h >> 13)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_inverse_undoes_the_model_matrix() {
        let m = acr_render::view3d::model_matrix([100.0, 200.0, 30.0], 0.7, 1.0);
        let inv = affine_inverse(&m);
        let p = [1.5, 2.0, -3.0];
        let back = math::transform_point(&inv, math::transform_point(&m, p));
        assert!((0..3).all(|i| (back[i] - p[i]).abs() < 1e-4), "{back:?}");
    }

    #[test]
    fn a_ray_finds_the_near_face() {
        // A wall in the plane z = 1, two triangles, struck from −z.
        let mesh = Mesh {
            tris: vec![
                [[-1.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 2.0, 1.0]],
                [[-1.0, 0.0, 1.0], [1.0, 2.0, 1.0], [-1.0, 2.0, 1.0]],
                [[-1.0, 0.0, 3.0], [1.0, 0.0, 3.0], [1.0, 2.0, 3.0]],
            ],
            min: [-1.0, 0.0, 1.0],
            max: [1.0, 2.0, 3.0],
        };
        let (p, n) = mesh.ray([0.2, 1.0, -5.0], [0.0, 0.0, 1.0]).unwrap();
        assert!((p[2] - 1.0).abs() < 1e-5 && n[2] < -0.99, "{p:?} {n:?}");
        assert!(mesh.ray([5.0, 1.0, -5.0], [0.0, 0.0, 1.0]).is_none());
    }
}
