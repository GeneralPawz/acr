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
//!
//! A ball's or shell's hole knocks out rubble ([`Piece`]): stone blocks or broken boards,
//! by the colour of the struck face (`ruin_view::is_wood`), that drop out of the hole, bounce
//! once and lie on the ground below it for the rest of the battle ([`MAX_PIECES`] in all, the
//! oldest go). A fire arrow (`HitRecord::fire`, `OSTRELA`) scorches the wall where it sticks
//! and burns there in a small fire for [`ARROW_FIRE_SECONDS`], and on as long as the building
//! burns once it has caught (the original lights it after `FIRELIMIT` of them, `combat.rs`
//! `FIRE_SLOW`).

use std::collections::HashMap;

use acr_render::math::{self, Mat4, Vec3};
use acr_render::model::ModelData;
use acr_render::model_renderer::{MAX_SCARS, ModelInstance, Scar, ScarKind, Scars};
use acr_render::view3d::{self, PIXELS_PER_METRE};
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
/// Rubble knocked out of holes, kept for the battle (all buildings).
pub const MAX_PIECES: usize = 600;
/// Fire arrows burning on one building at most (the oldest go out).
const MAX_ARROW_FIRES: usize = 8;
/// Seconds a fire arrow burns where it sticks (on a building that does not burn).
pub const ARROW_FIRE_SECONDS: f32 = 8.0;
/// Ticks per second at normal speed.
const TICKS_PER_SECOND: f32 = 10.0;
/// Gravity (m/s²) and what a bounce keeps of the speed.
const GRAVITY: f32 = 9.81;
const BOUNCE: f32 = 0.3;

/// A hit not yet placed: the building's model may not be loaded.
#[derive(Debug, Clone, Copy)]
struct Pending {
    pos: (i32, i32),
    dir: (i32, i32),
    cause: DeathCause,
    seed: u32,
    /// A fire weapon's round (`HitRecord::fire`).
    fire: bool,
    tick: u64,
}

/// A fire arrow burning where it stuck (model frame, metres) since `tick`.
#[derive(Debug, Clone, Copy)]
struct ArrowFire {
    point: Vec3,
    normal: Vec3,
    tick: u64,
}

/// A piece of rubble knocked out of a hole (#304): where it left the wall and how fast
/// (world pixels, z up; px/s), the ground under it, its size (half extents, metres), its
/// turn and tumble, and the seconds since it fell out.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    from: Vec3,
    vel: Vec3,
    ground: f32,
    half: Vec3,
    yaw: f32,
    /// Quarter turns about its long axis while it falls (it lands on a face).
    turns: u32,
    pub wood: bool,
    age: f32,
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
    fires: Vec<ArrowFire>,
    /// Its life when last drawn (a rise is a repair).
    life: i32,
}

/// A model's triangles in its pose at time 0 (model frame, metres), for placing hits.
struct Mesh {
    tris: Vec<[Vec3; 3]>,
    /// Whether each triangle is timber (`ruin_view::is_wood` of its colour).
    wood: Vec<bool>,
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
    /// Rubble on the ground, oldest first.
    pieces: Vec<Piece>,
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
                b.pending.push(Pending { pos: h.pos, dir: h.dir, cause: h.cause, seed, fire: h.fire, tick: h.tick });
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
    /// `ruin_view::Look::sooted`): soot climbs the wall over each. The fire arrows burning
    /// on it go to `flames` (world pixels, strength 0..1). `None` when it shows no scars.
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        &mut self,
        u: &Unit,
        index: usize,
        model: &ModelData,
        inst: &ModelInstance,
        fire: &[(Vec3, f32)],
        arrows: &mut Vec<Mat4>,
        flames: &mut Vec<(Vec3, f32)>,
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
            b.fires.clear();
        }
        b.life = u.life;
        if !b.pending.is_empty() {
            let mesh = self.meshes.entry(index).or_insert_with(|| Mesh::of(model, inst.clip));
            let inverse = affine_inverse(&inst.transform);
            let centre = (u.pos.x >> 4, u.pos.y >> 4);
            for p in std::mem::take(&mut b.pending) {
                place_hit(b, mesh, &inst.transform, &inverse, centre, p, &mut self.pieces);
            }
            if self.pieces.len() > MAX_PIECES {
                self.pieces.drain(..self.pieces.len() - MAX_PIECES);
            }
        }
        for a in &b.arrows {
            arrows.push(math::mul(&inst.transform, &arrow_matrix(a)));
        }
        // Fire arrows flare up, burn and go out; on a building that caught they burn on.
        let now = self.tick;
        let burning = u.burning;
        b.fires.retain(|f| burning || (now.saturating_sub(f.tick) as f32) < ARROW_FIRE_SECONDS * TICKS_PER_SECOND);
        for f in &b.fires {
            let age = now.saturating_sub(f.tick) as f32 / TICKS_PER_SECOND;
            let left = if burning { 1.0 } else { ((ARROW_FIRE_SECONDS - age) / 2.0).clamp(0.0, 1.0) };
            let strength = (0.3 + age / 0.6).min(1.0) * left;
            // Just off the wall, so the flames are not cut by it.
            let at = math::add(f.point, math::scale(f.normal, 0.08));
            flames.push((math::transform_point(&inst.transform, at), strength));
        }
        if b.scars.is_empty() && batter <= 0.0 && fire.is_empty() && b.fires.is_empty() {
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

impl BuildingHits {
    /// Moves the rubble on by `dt` seconds and pushes each piece's model matrix with
    /// whether it is a board (`procedural_rubble(true)`) to `out`.
    pub fn pieces(&mut self, dt: f32, out: &mut Vec<(Mat4, bool)>) {
        let dt = dt.clamp(0.0, 0.1);
        for p in &mut self.pieces {
            if p.age < 10.0 {
                p.age += dt;
            }
            out.push((piece_matrix(p), p.wood));
        }
    }
}

/// Where piece `p` is now (world pixels): out of the hole, falling, one bounce, at rest on
/// one of its faces.
fn piece_matrix(p: &Piece) -> Mat4 {
    let ppm = PIXELS_PER_METRE;
    let g = GRAVITY * ppm;
    // Lying on its broad face after an even number of quarter turns, on its edge after an
    // odd one (about its long axis, x).
    let rest_half = if p.turns.is_multiple_of(2) { p.half[1] } else { p.half[2] };
    let rest = p.ground + rest_half * ppm;
    let z0 = (p.from[2] - rest).max(0.0);
    let vz = p.vel[2];
    let t1 = (vz + (vz * vz + 2.0 * g * z0).sqrt()) / g;
    let vz1 = BOUNCE * (g * t1 - vz).max(0.0);
    let t2 = 2.0 * vz1 / g;
    let s = p.age;
    let (along, z) = if s < t1 {
        (s, rest + z0 + vz * s - 0.5 * g * s * s)
    } else if s < t1 + t2 {
        let u = s - t1;
        (t1 + BOUNCE * u, rest + vz1 * u - 0.5 * g * u * u)
    } else {
        (t1 + BOUNCE * t2, rest)
    };
    let end = t1 + BOUNCE * t2;
    let angle = p.turns as f32 * std::f32::consts::FRAC_PI_2 * (along / end.max(1e-3)).min(1.0);
    let at = [p.from[0] + p.vel[0] * along, p.from[1] + p.vel[1] * along, z];
    let base = view3d::model_matrix(at, p.yaw, 1.0);
    // Tumbling about its long axis (model x), scaled to its size.
    let (sn, cs) = angle.sin_cos();
    let local: Mat4 = [
        [p.half[0], 0.0, 0.0, 0.0],
        [0.0, cs * p.half[1], sn * p.half[1], 0.0],
        [0.0, -sn * p.half[2], cs * p.half[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    math::mul(&base, &local)
}

/// Rubble out of a hole at `point` (`normal` the struck face, both model frame) through the
/// building's `transform`: a shell's burst throws more and further.
fn knock_out(point: Vec3, normal: Vec3, wood: bool, shell: bool, transform: &Mat4, seed: u32, out: &mut Vec<Piece>) {
    let r = |k: u32| (hash(seed ^ k.wrapping_mul(0x2c1b_3c6d)) & 0xffff) as f32 / 65535.0;
    let ppm = PIXELS_PER_METRE;
    let from = math::transform_point(transform, point);
    let n = math::transform_vector(transform, normal);
    let l = (n[0] * n[0] + n[1] * n[1]).sqrt();
    let (nx, ny) = if l > 1e-6 { (n[0] / l, n[1] / l) } else { (1.0, 0.0) };
    let ground = transform[3][2];
    let count = if shell { 6 + (r(0) * 3.0) as u32 } else { 3 + (r(0) * 3.0) as u32 };
    for k in 0..count {
        let q = |j: u32| r(1 + k * 16 + j);
        let a = (q(0) - 0.5) * 1.6;
        let (sa, ca) = a.sin_cos();
        let (dx, dy) = (nx * ca - ny * sa, nx * sa + ny * ca);
        let speed = if shell { 1.5 + 3.0 * q(1) } else { 0.3 + 1.3 * q(1) } * ppm;
        let half = if wood {
            [0.2 + 0.25 * q(2), 0.012 + 0.012 * q(3), 0.05 + 0.04 * q(4)]
        } else {
            let s = 0.06 + 0.08 * q(2);
            [s * (1.0 + 0.5 * q(3)), s * (0.6 + 0.3 * q(4)), s * (0.8 + 0.4 * q(5))]
        };
        let spread = 0.15 * ppm;
        out.push(Piece {
            from: [from[0] + (q(6) - 0.5) * spread, from[1] + (q(7) - 0.5) * spread, from[2] + (q(8) - 0.5) * spread],
            vel: [dx * speed, dy * speed, (if shell { 0.5 + 2.0 * q(9) } else { 0.2 * q(9) }) * ppm],
            ground,
            half,
            yaw: q(10) * std::f32::consts::TAU,
            turns: 1 + (q(11) * 4.0) as u32,
            wood,
            age: 0.0,
        });
    }
}

/// Places one hit on building `b` drawn with `transform` (`inverse` its inverse); `centre`
/// is the building's ground point (world pixels): a hit recorded there struck somewhere on
/// the face towards the shooter. A hole knocks rubble out to `pieces`.
fn place_hit(
    b: &mut Building,
    mesh: &Mesh,
    transform: &Mat4,
    inverse: &Mat4,
    centre: (i32, i32),
    p: Pending,
    pieces: &mut Vec<Piece>,
) {
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
    let Some((point, normal, tri)) = mesh.ray(from, fly) else { return };
    let wood = mesh.wood.get(tri).copied().unwrap_or(true);
    let (kind, radius) = match p.cause {
        DeathCause::Arrow if p.fire => (ScarKind::Scorch, 0.16 + 0.1 * r(5)),
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
    // A shell bursts a hole under its scorch.
    if p.cause == DeathCause::Explosion && b.scars.len() < MAX_SCARS {
        b.scars.push(Scar { point, normal, radius: 0.25 + 0.15 * r(10), kind: ScarKind::Hole, seed: r(11) });
    }
    if matches!(p.cause, DeathCause::Cannonball | DeathCause::Explosion) {
        knock_out(point, normal, wood, p.cause == DeathCause::Explosion, transform, p.seed ^ 0x0c4b, pieces);
    }
    if arrow && p.fire {
        if b.fires.len() >= MAX_ARROW_FIRES {
            b.fires.remove(0);
        }
        b.fires.push(ArrowFire { point, normal, tick: p.tick });
    }
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
        let mut wood = Vec::new();
        // Textures kept encoded until upload are decoded here once (PNG; a KTX2 one is left
        // out and its triangles go by the material and vertex colours).
        let decoded: Vec<Option<acr_render::model::Image>> = model
            .images
            .iter()
            .enumerate()
            .map(|(i, img)| {
                if img.rgba.len() >= (img.width * img.height * 4) as usize {
                    return None;
                }
                let bytes = model.encoded.get(i).cloned().flatten()?;
                acr_render::model::decode_image(&bytes).ok()
            })
            .collect();
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
            let material = &model.materials[prim.material];
            let image =
                material.texture.and_then(|t| decoded.get(t).and_then(Option::as_ref).or_else(|| model.images.get(t)));
            for t in prim.indices.as_chunks::<3>().0 {
                let get = |i: u32| placed.get(i as usize).copied();
                if let (Some(a), Some(b), Some(c)) = (get(t[0]), get(t[1]), get(t[2])) {
                    tris.push([a, b, c]);
                    // Its colour at its middle: the material, the texture and the vertices.
                    let v = t.map(|i| &prim.vertices[i as usize]);
                    let uv =
                        [(v[0].uv[0] + v[1].uv[0] + v[2].uv[0]) / 3.0, (v[0].uv[1] + v[1].uv[1] + v[2].uv[1]) / 3.0];
                    let texel = image.map_or([1.0; 3], |img| super::ruin_view::sample(img, uv));
                    let colour: Vec3 = std::array::from_fn(|c| {
                        texel[c] * material.base_color[c] * (v[0].color[c] + v[1].color[c] + v[2].color[c]) / 3.0
                    });
                    wood.push(super::ruin_view::is_wood(colour));
                }
            }
        }
        if tris.is_empty() {
            (min, max) = ([0.0; 3], [0.0; 3]);
        }
        Self { tris, wood, min, max }
    }

    /// The first face the ray from `from` along `dir` (unit) meets: the point and the face's
    /// normal turned against the ray, and its index (Möller–Trumbore, both sides).
    fn ray(&self, from: Vec3, dir: Vec3) -> Option<(Vec3, Vec3, usize)> {
        let mut best: Option<(f32, Vec3, usize)> = None;
        for (i, [a, b, c]) in self.tris.iter().enumerate() {
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
            if t > 0.0 && best.is_none_or(|(bt, _, _)| t < bt) {
                let n = normalize(cross(e1, e2));
                best = Some((t, if math::dot(n, dir) > 0.0 { math::scale(n, -1.0) } else { n }, i));
            }
        }
        best.map(|(t, n, i)| (math::add(from, math::scale(dir, t)), n, i))
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
            wood: vec![true; 3],
            min: [-1.0, 0.0, 1.0],
            max: [1.0, 2.0, 3.0],
        };
        let (p, n, _) = mesh.ray([0.2, 1.0, -5.0], [0.0, 0.0, 1.0]).unwrap();
        assert!((p[2] - 1.0).abs() < 1e-5 && n[2] < -0.99, "{p:?} {n:?}");
        assert!(mesh.ray([5.0, 1.0, -5.0], [0.0, 0.0, 1.0]).is_none());
    }
}
