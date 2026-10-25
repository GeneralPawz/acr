//! Burning, collapsing and ruined buildings in 3D (#303), render only. A finished building
//! keeps its model while it burns down (`SLOWDEATH`, `combat::slow_death`), with flames and
//! smoke at its fire anchors (`socket.fire.<i>`, `socket.smoke.<i>`) or, for a model without
//! them, at points spread over its roof; they grow with the life it lost and its model chars.
//! When it dies its model sinks, tilts and is squashed into the ground over
//! [`COLLAPSE_SECONDS`], charring black, in dust and debris; then it stays as a ruin (the
//! model's `<name>.ruin` state group where it has one, else the same mesh cut low) that
//! smokes for a while, for as long as the simulation keeps the unit (its `#DEATHLIE`, as the
//! original's ruin sprite). A few roof and wall pieces split off the model ([`chunks`]) break
//! away as it falls in, tumble down with gravity, bounce once and settle in the rubble with a
//! puff of dust ([`chunk_fall`]). Variety comes from the unit id, never the simulation's RNG.

use acr_render::math::{Mat4, Vec3};
use acr_render::model::{ModelData, Vertex};
use acr_render::view3d::{self, PIXELS_PER_METRE};

/// Seconds a destroyed building takes to fall in.
pub const COLLAPSE_SECONDS: f32 = 2.5;
/// The height left of a model without a ruin state (of its standing height).
const RUIN_HEIGHT: f32 = 0.3;
/// The largest tilt while falling in (radians), and what stays of it in the ruin.
const TILT: f32 = 0.14;
const TILT_LEFT: f32 = 0.05;
/// How far the model sinks into the ground (metres).
const SINK: f32 = 0.4;
/// Seconds the ruin keeps flames after the collapse, and smokes.
pub const RUIN_FLAMES: f32 = 20.0;
pub const RUIN_SMOKE: f32 = 90.0;
/// Ticks a building stays as a ruin when the simulation removes it at once (a type without
/// `#DEATH`): as long as a `#DEATHLIE1` lasts (death counter 1200 at two a tick,
/// `combat::object_one`).
pub const RUIN_TICKS: u64 = 600;
/// Points used for a model without fire anchors.
const GENERIC_POINTS: usize = 10;
/// Smoke columns at most (of its smoke anchors, spread evenly): a few large plumes read as
/// well as one per anchor and cost far less in the particles pass.
pub const SMOKE_COLUMNS: usize = 6;
/// The char left on a ruin state (it carries its own soot), reached this many seconds after
/// the collapse from black.
const RUIN_BURN: f32 = 0.3;
const RUIN_COOL: f32 = 4.0;

/// Pieces split off a building model (#303): four from the roof, one from the front and one
/// from the back wall, each the triangles whose centre lies in its box (fractions of the
/// model's bounds: x, y, z from .. to).
const CHUNK_BOXES: [[[f32; 2]; 3]; 6] = [
    [[0.05, 0.4], [0.62, 1.01], [0.05, 0.45]],
    [[0.6, 0.95], [0.62, 1.01], [0.05, 0.45]],
    [[0.05, 0.4], [0.62, 1.01], [0.55, 0.95]],
    [[0.6, 0.95], [0.62, 1.01], [0.55, 0.95]],
    [[0.3, 0.7], [0.3, 0.62], [0.85, 1.01]],
    [[0.3, 0.7], [0.3, 0.62], [-0.01, 0.15]],
];
/// A piece needs this many triangles to be worth drawing.
const CHUNK_MIN_TRIANGLES: usize = 4;
/// Seconds after the death the first piece breaks off, and the spread of the others: the
/// `DESTRUCT` flash has thinned by then (`ThreeD::veils`).
pub const BREAK: f32 = 0.35;
const BREAK_SPREAD: f32 = 0.9;
/// What a bounce keeps of the speed.
const BOUNCE: f32 = 0.3;
/// Gravity (m/s²).
const GRAVITY: f32 = 9.81;
/// Seconds a settled piece lies in the rubble before it sinks away over [`CHUNK_SINK`].
const CHUNK_HOLD: f32 = 25.0;
const CHUNK_SINK: f32 = 4.0;
/// The char of the pieces at most (the shell goes black).
pub const CHUNK_BURN: f32 = 0.4;
/// The longest texture side of a piece (pixels).
const CHUNK_TEXTURE: u32 = 256;
/// Seconds of dust where a piece lands.
pub const LANDING_DUST: f32 = 0.5;

/// A piece of a building model that breaks off when it falls in: its own model (the
/// triangles of one [`CHUNK_BOXES`] box), its centre and radius (model space, metres).
pub struct Chunk {
    pub model: ModelData,
    pub centre: Vec3,
    pub radius: f32,
}

/// The pieces of `model` that break off (#303), each a broken slab: the model's surface
/// inside one of the [`CHUNK_BOXES`], cut along a jagged outline (the box's distance field
/// plus a noise, [`JAG`]; triangles along it are split down to [`FRACTURE_EDGE`]), thickened
/// inwards to a board's or a wall's depth: its back the surface turned over, its broken
/// edge in an inner material ([`Break`]). They stay in the standing model too: they show only once the
/// building, falling in, has left their place.
///
/// `broken` false gives the plain pieces instead (the triangles whose centre lies in the box,
/// no back or edge): a few milliseconds, for when the broken ones are not ready.
pub fn chunks(model: &ModelData, broken: bool) -> Vec<Chunk> {
    let palette = model.pose(None, 0.0);
    let world = |v: &Vertex| -> Vec3 { affine(&palette[v.joints[0] as usize], v.position) };
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in model.primitives.iter().flat_map(|p| &p.vertices) {
        let p = world(v);
        for i in 0..3 {
            (lo[i], hi[i]) = (lo[i].min(p[i]), hi[i].max(p[i]));
        }
    }
    if lo[0] > hi[0] {
        return Vec::new();
    }
    // Small pieces need no full textures: each piece is a model of its own and uploads them.
    let images: Vec<acr_render::model::Image> = model.images.iter().map(|i| shrink(i, CHUNK_TEXTURE)).collect();
    let mut out = Vec::new();
    for (k, b) in CHUNK_BOXES.iter().enumerate() {
        let mid: Vec3 = std::array::from_fn(|i| lo[i] + 0.5 * (b[i][0] + b[i][1]) * (hi[i] - lo[i]));
        let half: Vec3 = std::array::from_fn(|i| 0.5 * (b[i][1] - b[i][0]) * (hi[i] - lo[i]));
        let jag = JAG.min(JAG_SHARE * half.iter().fold(f32::MAX, |m, &h| m.min(h)));
        let seed = k as u32 * 7 + 3;
        // Below 0 inside the piece: the box, its faces broken by two octaves of noise.
        let boxed = |p: Vec3| (0..3).fold(f32::MIN, |m, i| m.max((p[i] - mid[i]).abs() - half[i]));
        let field = |p: Vec3| {
            let n = 0.65 * noise3(p, 1.0 / 0.7, seed) + 0.35 * noise3(p, 1.0 / 0.22, seed + 1);
            boxed(p) + jag * (2.0 * n - 1.0)
        };
        let outline = Outline { boxed: &boxed, field: &field, jag };
        let mut primitives = Vec::new();
        let mut worlds: Vec<Vec<Vec3>> = Vec::new();
        let mut points: Vec<Vec3> = Vec::new();
        let mut kept: Vec<[Vec3; 3]> = Vec::new();
        let (mut colour, mut weight) = ([0.0f32; 3], 0.0f32);
        let mut host: Option<Vertex> = None;
        for p in &model.primitives {
            let material = &model.materials[p.material];
            let texture = material.texture.and_then(|t| images.get(t));
            let mut prim = acr_render::model::Primitive {
                vertices: Vec::new(),
                indices: Vec::new(),
                material: p.material,
                morphs: p.morphs,
                look_morphs: p.look_morphs.iter().map(|(k, _)| (*k, Vec::new())).collect(),
                lod: None,
            };
            let mut at_world = Vec::new();
            // A vertex once each: a corner of the source by its index (shared with the
            // neighbours), a point inside a split triangle by the triangle and its
            // barycentrics (dyadic, so exact).
            let mut map: FastMap<(u32, u32, u32), u32> = FastMap::default();
            for (number, t) in p.indices.as_chunks::<3>().0.iter().enumerate() {
                let corners = t.map(|i| &p.vertices[i as usize]);
                let w = corners.map(world);
                let mut sub = Vec::new();
                let whole = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
                if broken {
                    split(&w, whole, &outline, 0, &mut sub);
                } else if boxed(blend3(&w, &[1.0 / 3.0; 3])) <= 0.0 {
                    sub.push(whole);
                }
                for tri in sub {
                    let mut at = [[0.0f32; 3]; 3];
                    for (c, bary) in tri.iter().enumerate() {
                        let key = match bary.iter().position(|&f| f == 1.0) {
                            Some(c) => (u32::MAX, t[c], 0),
                            None => {
                                (number as u32, (bary[0] * 4096.0).round() as u32, (bary[1] * 4096.0).round() as u32)
                            }
                        };
                        let pos = blend3(&w, bary);
                        at[c] = pos;
                        let next = prim.vertices.len() as u32;
                        let j = *map.entry(key).or_insert_with(|| {
                            let v = lerp_vertex(&corners, bary);
                            points.push(pos);
                            at_world.push(pos);
                            let s = texture.map_or([1.0; 3], |img| sample(img, v.uv));
                            for c in 0..3 {
                                colour[c] += s[c] * material.base_color[c];
                            }
                            weight += 1.0;
                            host.get_or_insert(v);
                            prim.vertices.push(v);
                            for ((_, offsets), (_, all)) in prim.look_morphs.iter_mut().zip(&p.look_morphs) {
                                let o = bary.iter().zip(t).fold([0.0f32; 3], |o, (&f, &i)| {
                                    let a = all.get(i as usize).copied().unwrap_or_default();
                                    [o[0] + f * a[0], o[1] + f * a[1], o[2] + f * a[2]]
                                });
                                offsets.push(o);
                            }
                            next
                        });
                        prim.indices.push(j);
                    }
                    kept.push(at);
                }
            }
            if !prim.indices.is_empty() {
                primitives.push(prim);
                worlds.push(at_world);
            }
        }
        let Some(host) = host else { continue };
        if kept.len() < CHUNK_MIN_TRIANGLES {
            continue;
        }
        let n = points.len() as f32;
        let sum = points.iter().fold([0.0f32; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
        let centre = [sum[0] / n, sum[1] / n, sum[2] / n];
        let radius = points.iter().fold(0.0f32, |r, p| {
            r.max(((p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2) + (p[2] - centre[2]).powi(2)).sqrt())
        });
        // Timber or masonry by the piece's mean colour (warm is wood, as `model.wgsl` `scarred`).
        let mean: Vec3 = std::array::from_fn(|c| colour[c] / weight.max(1e-6));
        let wood = (mean[0] - mean[2]) / mean.iter().fold(0.02f32, |m, &c| m.max(c)) > WOOD_WARMTH;
        let mut piece = model.clone();
        if !broken {
            (piece.primitives, piece.images) = (primitives, images.clone());
            out.push(Chunk { model: piece, centre, radius });
            continue;
        }
        let inner = piece.materials.len();
        let mut m = model.materials[primitives[0].material].clone();
        m.name = if wood { "fracture.wood" } else { "fracture.stone" }.to_owned();
        m.base_color = if wood { WOOD_INNER } else { STONE_INNER };
        (m.texture, m.alpha_cutoff, m.decay, m.stump) = (None, None, None, false);
        (m.surface.normal, m.surface.metal_rough) = (None, None);
        (m.surface.roughness, m.surface.metallic) = (0.95, 0.0);
        piece.materials.push(m);
        let depth = (if wood { THICK_WOOD } else { THICK_STONE }).min(0.35 * radius);
        let broken = Break::new(&kept, &field, wood, depth, k as u32);
        for (prim, world) in primitives.iter_mut().zip(&worlds) {
            broken.add_back(prim, world, &palette);
        }
        if let Some(prim) = broken.rim(host, &palette[host.joints[0] as usize], inner) {
            primitives.push(prim);
        }
        (piece.primitives, piece.images) = (primitives, images.clone());
        out.push(Chunk { model: piece, centre, radius });
    }
    out
}

/// The jagged outline of the pieces (#303): the noise amplitude (metres; at most
/// [`JAG_SHARE`] of the box's smallest half extent), the longest triangle edge along it
/// and how often a triangle is split at most.
const JAG: f32 = 0.35;
const JAG_SHARE: f32 = 0.3;
const FRACTURE_EDGE: f32 = 0.15;
const FRACTURE_DEPTH: u32 = 7;
/// How far inside the outline (metres of the field) an open edge still counts as the break.
const OUTLINE_BAND: f32 = 0.3;
/// The life lost from which a building's broken pieces are cut ahead (on a worker).
pub const HEAVY_DAMAGE: f32 = 0.5;
/// How deep a piece's broken edge is: a board on timber, a wall on stone (metres; at most
/// 35 % of the piece's radius).
const THICK_WOOD: f32 = 0.09;
const THICK_STONE: f32 = 0.32;
/// A piece is timber when its mean colour is this warm ((r − b) / max), as `scarred`.
const WOOD_WARMTH: f32 = 0.19;
/// The inner materials (linear): raw wood, broken stone and mortar (the fresh colours of
/// `model.wgsl` `scarred`).
const WOOD_INNER: [f32; 4] = [0.62, 0.47, 0.27, 1.0];
const STONE_INNER: [f32; 4] = [0.46, 0.44, 0.41, 1.0];

/// A small, fast hasher for the integer keys of the pieces' maps (the standard one is slow
/// for a few hundred thousand points).
#[derive(Default)]
struct Fx(u64);

impl std::hash::Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }
    fn write_u32(&mut self, i: u32) {
        self.write_u64(u64::from(i));
    }
    fn write_i32(&mut self, i: i32) {
        self.write_u64(u64::from(i as u32));
    }
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type FastMap<K, V> = std::collections::HashMap<K, V, std::hash::BuildHasherDefault<Fx>>;

/// A model-space point through a palette entry (rows).
fn affine(r: &[[f32; 4]; 3], p: Vec3) -> Vec3 {
    std::array::from_fn(|i| r[i][0] * p[0] + r[i][1] * p[1] + r[i][2] * p[2] + r[i][3])
}

/// The point at barycentrics `b` of the triangle `w`.
fn blend3(w: &[Vec3; 3], b: &Vec3) -> Vec3 {
    std::array::from_fn(|i| b[0] * w[0][i] + b[1] * w[1][i] + b[2] * w[2][i])
}

/// Splits the triangle `w` (model space; the part given by barycentrics `t`) where the
/// outline `field` crosses it until its edges are shorter than [`FRACTURE_EDGE`], and keeps
/// the parts inside.
fn split(w: &[Vec3; 3], t: [Vec3; 3], o: &Outline, depth: u32, out: &mut Vec<[Vec3; 3]>) {
    let p = t.map(|b| blend3(w, &b));
    let c: Vec3 = std::array::from_fn(|i| (p[0][i] + p[1][i] + p[2][i]) / 3.0);
    let d = |a: Vec3, b: Vec3| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    let reach = p.iter().fold(0.0f32, |r, &q| r.max(d(q, c)));
    // Far from the box's faces the noise cannot move the outline over the triangle: no
    // noise to sample (most of the model).
    let b = (o.boxed)(c);
    if b - reach > o.jag {
        return;
    }
    if b + reach < -o.jag {
        out.push(t);
        return;
    }
    let f = (o.field)(c);
    let edge = d(p[0], p[1]).max(d(p[1], p[2])).max(d(p[2], p[0]));
    // Smaller triangles split only where the outline crosses them (the corners disagree).
    let crossed = || p.iter().any(|&q| ((o.field)(q) < 0.0) != (f < 0.0));
    if edge <= FRACTURE_EDGE || depth >= FRACTURE_DEPTH || (edge <= 4.0 * FRACTURE_EDGE && !crossed()) {
        if f < 0.0 {
            out.push(t);
        }
        return;
    }
    let half = |a: Vec3, b: Vec3| -> Vec3 { std::array::from_fn(|i| 0.5 * (a[i] + b[i])) };
    let (m01, m12, m20) = (half(t[0], t[1]), half(t[1], t[2]), half(t[2], t[0]));
    for s in [[t[0], m01, m20], [m01, t[1], m12], [m20, m12, t[2]], [m01, m12, m20]] {
        split(w, s, o, depth + 1, out);
    }
}

/// A piece's outline: the box's distance field (metres, below 0 inside), the same broken by
/// noise, and the noise's amplitude.
struct Outline<'a> {
    boxed: &'a dyn Fn(Vec3) -> f32,
    field: &'a dyn Fn(Vec3) -> f32,
    jag: f32,
}

/// The vertex at barycentrics `b` of a triangle.
fn lerp_vertex(c: &[&Vertex; 3], b: &Vec3) -> Vertex {
    let mut v = *c[0];
    let mix = |f: &dyn Fn(&Vertex) -> f32| b[0] * f(c[0]) + b[1] * f(c[1]) + b[2] * f(c[2]);
    for i in 0..3 {
        v.position[i] = mix(&|x| x.position[i]);
        v.normal[i] = mix(&|x| x.normal[i]);
        v.morph[0][i] = mix(&|x| x.morph[0][i]);
        v.morph[1][i] = mix(&|x| x.morph[1][i]);
    }
    for i in 0..2 {
        v.uv[i] = mix(&|x| x.uv[i]);
    }
    for i in 0..4 {
        v.color[i] = mix(&|x| x.color[i]);
        v.body[i] = mix(&|x| x.body[i]);
    }
    v.normal = unit(v.normal);
    v
}

/// The texel (linear RGB) of `img` at `uv`, wrapped.
fn sample(img: &acr_render::model::Image, uv: [f32; 2]) -> Vec3 {
    if img.width == 0 || img.height == 0 {
        return [1.0; 3];
    }
    let x = ((uv[0].rem_euclid(1.0) * img.width as f32) as u32).min(img.width - 1);
    let y = ((uv[1].rem_euclid(1.0) * img.height as f32) as u32).min(img.height - 1);
    let at = ((y * img.width + x) * 4) as usize;
    std::array::from_fn(|c| (f32::from(img.rgba[at + c]) / 255.0).powf(2.2))
}

/// Value noise 0..1 at `p` (metres) with `freq` cells a metre, from `seed`.
fn noise3(p: Vec3, freq: f32, seed: u32) -> f32 {
    let q = p.map(|x| x * freq);
    let cell = q.map(f32::floor);
    let f: Vec3 = std::array::from_fn(|i| {
        let t = q[i] - cell[i];
        t * t * (3.0 - 2.0 * t)
    });
    let at = |dx: i32, dy: i32, dz: i32| {
        let h = ((cell[0] as i32 + dx) as u32)
            ^ ((cell[1] as i32 + dy) as u32).wrapping_mul(0x8da6_b343)
            ^ ((cell[2] as i32 + dz) as u32).wrapping_mul(0xd816_3841);
        acr_render::particles::unit(h, seed)
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let plane = |dz: i32| lerp(lerp(at(0, 0, dz), at(1, 0, dz), f[0]), lerp(at(0, 1, dz), at(1, 1, dz), f[0]), f[1]);
    lerp(plane(0), plane(1), f[2])
}

fn unit(a: Vec3) -> Vec3 {
    let l = a.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    a.map(|x| x / l)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A point of the piece (model space) as a key: millimetres.
fn point_key(p: Vec3) -> [i32; 3] {
    p.map(|x| (x * 1000.0).round() as i32)
}

/// The break of a piece (#303), from its kept surface (model space): the smoothed normals,
/// the outline (the edges only one kept triangle uses, in its winding, with its normal) and
/// the outward direction at each outline point. [`Break::back`] gives the point about
/// `depth` under the surface where the piece's back lies; on the outline a hash shifts it in
/// and out, so the broken edge between them ([`Break::rim`]) is jagged.
struct Break {
    normals: FastMap<[i32; 3], Vec3>,
    outline: Vec<(Vec3, Vec3, Vec3)>,
    outward: FastMap<[i32; 3], Vec3>,
    wood: bool,
    depth: f32,
    seed: u32,
}

impl Break {
    fn new(tris: &[[Vec3; 3]], field: &dyn Fn(Vec3) -> f32, wood: bool, depth: f32, seed: u32) -> Break {
        let mut normals: FastMap<[i32; 3], Vec3> = FastMap::default();
        // Per edge: how many triangles use it, its ends in the first one's winding, that normal.
        type Use = (usize, Vec3, Vec3, Vec3);
        let mut edges: FastMap<([i32; 3], [i32; 3]), Use> = FastMap::default();
        for t in tris {
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            if n.iter().all(|x| x.abs() < 1e-12) {
                continue;
            }
            for (c, &p) in t.iter().enumerate() {
                let e = normals.entry(point_key(p)).or_insert([0.0; 3]);
                *e = std::array::from_fn(|i| e[i] + n[i]);
                let q = t[(c + 1) % 3];
                let (a, b) = (point_key(p), point_key(q));
                let id = if a < b { (a, b) } else { (b, a) };
                edges.entry(id).and_modify(|e| e.0 += 1).or_insert((1, p, q, unit(n)));
            }
        }
        // Only along the cut: generated meshes have open seams all over, which do not break.
        let mut outline: Vec<(Vec3, Vec3, Vec3)> = edges
            .values()
            .filter(|e| e.0 == 1 && field(std::array::from_fn(|i| 0.5 * (e.1[i] + e.2[i]))) > -OUTLINE_BAND)
            .map(|&(_, a, b, n)| (a, b, n))
            .collect();
        // The same order on every run (the map's is random).
        outline.sort_by_key(|&(a, b, _)| (point_key(a), point_key(b)));
        let mut outward: FastMap<[i32; 3], Vec3> = FastMap::default();
        for &(a, b, n) in &outline {
            let o = unit(cross(sub(b, a), n));
            for p in [a, b] {
                let e = outward.entry(point_key(p)).or_insert([0.0; 3]);
                *e = std::array::from_fn(|i| e[i] + o[i]);
            }
        }
        Break { normals, outline, outward, wood, depth, seed }
    }

    fn hash(&self, k: [i32; 3], j: u32) -> f32 {
        let h = (k[0] as u32).wrapping_mul(0x9e37_79b9)
            ^ (k[1] as u32).wrapping_mul(0x85eb_ca6b)
            ^ (k[2] as u32).wrapping_mul(0xc2b2_ae35);
        acr_render::particles::unit(h ^ self.seed.wrapping_mul(0x27d4_eb2f), j)
    }

    fn normal(&self, p: Vec3) -> Vec3 {
        unit(self.normals.get(&point_key(p)).copied().unwrap_or([0.0, 1.0, 0.0]))
    }

    /// The back point under the surface point `p`.
    fn back(&self, p: Vec3) -> Vec3 {
        let (n, k) = (self.normal(p), point_key(p));
        let shift = self.outward.get(&k).map_or([0.0; 3], |o| {
            let s = (self.hash(k, 0) - 0.45) * self.depth * if self.wood { 1.2 } else { 1.6 };
            unit(*o).map(|x| x * s)
        });
        let deeper = self.depth * (0.8 + 0.4 * self.hash(k, 1));
        std::array::from_fn(|i| p[i] - n[i] * deeper + shift[i])
    }

    /// Turns the kept part of a primitive over (`world`: its vertices in model space) as the
    /// piece's back: the same material, pushed in, the winding and normals reversed, darker.
    fn add_back(&self, prim: &mut acr_render::model::Primitive, world: &[Vec3], palette: &acr_render::model::Palette) {
        // The back is seen through the break only: a coarser mesh than the front does, with
        // just the vertices it uses. The generated meshes' open seams keep the simplified
        // levels near the full count, so the shadow level (sloppy, 3 %, it ignores them) is
        // taken where it is far smaller.
        let levels = acr_render::lod::levels(&prim.vertices, &prim.indices);
        let back = [acr_render::lod::SHADOW, 2, 1]
            .iter()
            .map(|&l| &levels[l])
            .find(|l| !l.is_empty() && l.len() * 4 <= prim.indices.len())
            .unwrap_or(&levels[1])
            .clone();
        let back = if back.is_empty() { prim.indices.clone() } else { back };
        let mut map: FastMap<u32, u32> = FastMap::default();
        let mut tri = [0u32; 3];
        for t in back.as_chunks::<3>().0 {
            for (c, &i) in t.iter().enumerate() {
                tri[c] = *map.entry(i).or_insert_with(|| {
                    let mut v = prim.vertices[i as usize];
                    let r = &palette[v.joints[0] as usize];
                    if let Some(inv) = invert(r) {
                        v.position = to_node(r, &inv, self.back(world[i as usize]));
                    }
                    v.normal = v.normal.map(|x| -x);
                    v.color = [v.color[0] * 0.55, v.color[1] * 0.55, v.color[2] * 0.55, v.color[3]];
                    v.morph = [[0.0; 3]; 2];
                    prim.vertices.push(v);
                    for offsets in prim.look_morphs.iter_mut().map(|(_, o)| o) {
                        let o = offsets.get(i as usize).copied().unwrap_or_default();
                        offsets.push(o);
                    }
                    prim.vertices.len() as u32 - 1
                });
            }
            prim.indices.extend_from_slice(&[tri[0], tri[2], tri[1]]);
        }
    }

    /// The broken edge: a strip from each outline edge down to the back in the inner
    /// `material`, shaded by the vertex colour (pale at the surface, darker deep in, stone
    /// mottled with mortar); timber also throws long thin splinters off it, seen from both
    /// sides. The vertices sit in the node of `host` (`matrix`).
    fn rim(&self, host: Vertex, matrix: &[[f32; 4]; 3], material: usize) -> Option<acr_render::model::Primitive> {
        let inverse = invert(matrix)?;
        let (wood, depth) = (self.wood, self.depth);
        let mut prim = acr_render::model::Primitive {
            vertices: Vec::new(),
            indices: Vec::new(),
            material,
            morphs: [false; 2],
            look_morphs: Vec::new(),
            lod: None,
        };
        let mut push = |p: Vec3, n: Vec3, shade: f32| -> u32 {
            let mut v = host;
            v.position = to_node(matrix, &inverse, p);
            // The shader turns normals by the palette's 3 × 3 part.
            v.normal =
                unit(std::array::from_fn(|i| inverse[i][0] * n[0] + inverse[i][1] * n[1] + inverse[i][2] * n[2]));
            v.uv = [0.5, 0.5];
            v.color = [shade, shade, shade, 1.0];
            v.morph = [[0.0; 3]; 2];
            prim.vertices.push(v);
            prim.vertices.len() as u32 - 1
        };
        let stone = |p: Vec3| {
            let h = self.hash(point_key(p), 2);
            // Pale mortar among the broken stones.
            if h > 0.8 { 1.3 } else { 0.7 + 0.35 * h }
        };
        let mut indices = Vec::new();
        for &(a, b, n) in &self.outline {
            let (a2, b2) = (self.back(a), self.back(b));
            let side = unit(cross(sub(b, a), n));
            let (sa, sb, sa2, sb2) =
                if wood { (1.1, 1.1, 0.75, 0.75) } else { (stone(a), stone(b), stone(a2), stone(b2)) };
            let ia = push(a, side, sa);
            let ib = push(b, side, sb);
            let ib2 = push(b2, side, sb2);
            let ia2 = push(a2, side, sa2);
            indices.extend_from_slice(&[ia, ia2, ib2, ia, ib2, ib]);
            let ke = point_key(std::array::from_fn(|i| 0.5 * (a[i] + b[i])));
            if wood && self.hash(ke, 7) < 0.45 {
                let len = 0.08 + 0.22 * self.hash(ke, 8);
                let f = 0.3 + 0.4 * self.hash(ke, 9);
                let bend = (self.hash(ke, 10) - 0.5) * 0.8;
                let s0: Vec3 = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * (f - 0.2) - n[i] * depth * 0.3);
                let s1: Vec3 = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * (f + 0.2) - n[i] * depth * 0.6);
                let tip: Vec3 = std::array::from_fn(|i| {
                    a[i] + (b[i] - a[i]) * f + side[i] * len + n[i] * (bend * len - depth * 0.45)
                });
                let i0 = push(s0, n, 1.0);
                let i1 = push(s1, n, 1.0);
                let i2 = push(tip, n, 1.25);
                indices.extend_from_slice(&[i0, i1, i2, i0, i2, i1]);
            }
        }
        if indices.is_empty() {
            return None;
        }
        prim.indices = indices;
        Some(prim)
    }
}

/// A model-space point into the frame of the palette entry `r` (`inv`: its 3 × 3 inverse).
fn to_node(r: &[[f32; 4]; 3], inv: &[[f32; 3]; 3], p: Vec3) -> Vec3 {
    let q = [p[0] - r[0][3], p[1] - r[1][3], p[2] - r[2][3]];
    std::array::from_fn(|i| inv[i][0] * q[0] + inv[i][1] * q[1] + inv[i][2] * q[2])
}

/// The inverse of a palette entry's 3 × 3 part (rows), if it has one.
fn invert(r: &[[f32; 4]; 3]) -> Option<[[f32; 3]; 3]> {
    let m = |i: usize, j: usize| r[i][j];
    let det = m(0, 0) * (m(1, 1) * m(2, 2) - m(1, 2) * m(2, 1)) - m(0, 1) * (m(1, 0) * m(2, 2) - m(1, 2) * m(2, 0))
        + m(0, 2) * (m(1, 0) * m(2, 1) - m(1, 1) * m(2, 0));
    if det.abs() < 1e-12 {
        return None;
    }
    // The adjugate over the determinant: entry (i, j) is the cofactor of (j, i).
    Some(std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            let (a, b) = ((j + 1) % 3, (j + 2) % 3);
            let (x, y) = ((i + 1) % 3, (i + 2) % 3);
            (m(a, x) * m(b, y) - m(a, y) * m(b, x)) / det
        })
    }))
}

/// `image` halved (2 × 2 means) until its longer side is at most `max`.
fn shrink(image: &acr_render::model::Image, max: u32) -> acr_render::model::Image {
    let mut img = image.clone();
    while img.width.max(img.height) > max && img.width >= 2 && img.height >= 2 {
        let (w, h) = (img.width / 2, img.height / 2);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let at = |dx: u32, dy: u32| {
                        u32::from(img.rgba[(((2 * y + dy) * img.width + 2 * x + dx) * 4 + c) as usize])
                    };
                    rgba[((y * w + x) * 4 + c) as usize] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                }
            }
        }
        img = acr_render::model::Image { width: w, height: h, rgba };
    }
    img
}

/// Where piece `k` (centre `centre`, radius `radius`, model space) of building `id` at
/// `ground` is `t` seconds after the death: `None` before it breaks off and once it sank
/// away; else its model matrix and the seconds since it first struck the ground (negative
/// before). It leaves the falling model where it is at its break time, flies outwards and
/// tumbles, bounces once and lies still.
pub fn chunk_fall(ground: Vec3, id: u32, k: usize, centre: Vec3, radius: f32, t: f32) -> Option<(Mat4, f32)> {
    let h = |j: u32| unit_hash(id.wrapping_mul(31).wrapping_add(k as u32), 10 + j);
    let tb = BREAK + BREAK_SPREAD * h(0);
    if t < tb {
        return None;
    }
    let ppm = PIXELS_PER_METRE;
    let mb = collapse_matrix(ground, id, collapse(tb), RUIN_HEIGHT);
    let p0 = place(&mb, centre);
    // Outwards from the middle, turned a little.
    let mid = place(&mb, [0.0, centre[1], 0.0]);
    let (dx, dy) = (p0[0] - mid[0], p0[1] - mid[1]);
    let a = if dx * dx + dy * dy > 1e-4 { dy.atan2(dx) } else { std::f32::consts::TAU * h(1) } + (h(2) - 0.5) * 0.8;
    let speed = (1.0 + 1.6 * h(3)) * ppm;
    let (vx, vy) = (a.cos() * speed, a.sin() * speed);
    let vz = (0.4 + 1.2 * h(4)) * ppm;
    let g = GRAVITY * ppm;
    let rest = ground[2] + 0.35 * radius * ppm;
    let z0 = (p0[2] - rest).max(0.0);
    // First impact, then one bounce.
    let t1 = (vz + (vz * vz + 2.0 * g * z0).sqrt()) / g;
    let vz1 = BOUNCE * (g * t1 - vz);
    let t2 = 2.0 * vz1 / g;
    let s = t - tb;
    let (along, z) = if s < t1 {
        (s, rest + z0 + vz * s - 0.5 * g * s * s)
    } else if s < t1 + t2 {
        let u = s - t1;
        (t1 + BOUNCE * u, rest + vz1 * u - 0.5 * g * u * u)
    } else {
        (t1 + BOUNCE * t2, rest)
    };
    let after = s - t1;
    let sink = ((after - CHUNK_HOLD) / CHUNK_SINK).clamp(0.0, 1.0);
    if sink >= 1.0 {
        return None;
    }
    let at = [p0[0] + vx * along, p0[1] + vy * along, z - sink * 1.2 * radius * ppm];
    // It rolls over the way it flies.
    let axis = {
        let (x, y) = (-a.sin(), a.cos());
        let tilt = (h(5) - 0.5) * 0.6;
        let l = (1.0 + tilt * tilt).sqrt();
        [x / l, y / l, tilt / l]
    };
    let angle = (2.0 + 3.0 * h(6)) * along;
    let cols: [Vec3; 3] = std::array::from_fn(|c| rotate([mb[c][0], mb[c][1], mb[c][2]], axis, angle));
    let off: Vec3 = std::array::from_fn(|i| cols[0][i] * centre[0] + cols[1][i] * centre[1] + cols[2][i] * centre[2]);
    let mut m = mb;
    for (c, col) in cols.iter().enumerate() {
        m[c][..3].copy_from_slice(col);
    }
    m[3] = [at[0] - off[0], at[1] - off[1], at[2] - off[2], 1.0];
    Some((m, after))
}

/// Where a building model burns, in model space (glTF metres, y up).
#[derive(Clone, Debug, Default)]
pub struct Anchors {
    pub fire: Vec<Vec3>,
    pub smoke: Vec<Vec3>,
    /// The model's footprint half extents (x, z) and height (metres), for dust and debris.
    pub half: [f32; 2],
    pub top: f32,
}

/// The fire anchors of `model`: its `socket.fire.*` / `socket.smoke.*` nodes, else points on
/// its upper part (vertices above 60 % of its height, taken at even strides).
pub fn anchors(model: &ModelData) -> Anchors {
    let globals = model.globals(None, 0.0);
    let at = |i: usize| [globals[i][3][0], globals[i][3][1], globals[i][3][2]];
    let named = |prefix: &str| -> Vec<Vec3> {
        model.nodes.iter().enumerate().filter(|(_, n)| n.name.starts_with(prefix)).map(|(i, _)| at(i)).collect()
    };
    let palette = model.pose(None, 0.0);
    let mut points: Vec<Vec3> = Vec::new();
    for p in &model.primitives {
        for v in &p.vertices {
            let r = &palette[v.joints[0] as usize];
            points.push(std::array::from_fn(|i| {
                r[i][0] * v.position[0] + r[i][1] * v.position[1] + r[i][2] * v.position[2] + r[i][3]
            }));
        }
    }
    let top = points.iter().fold(0.0f32, |t, p| t.max(p[1]));
    let half = points.iter().fold([0.0f32; 2], |h, p| [h[0].max(p[0].abs()), h[1].max(p[2].abs())]);
    let mut fire = named("socket.fire.");
    let mut smoke = named("socket.smoke.");
    if fire.is_empty() {
        let high: Vec<Vec3> = points.iter().copied().filter(|p| p[1] > 0.6 * top).collect();
        let step = (high.len() / GENERIC_POINTS).max(1);
        fire = high.iter().step_by(step).take(GENERIC_POINTS).copied().collect();
    }
    if smoke.is_empty() {
        smoke = fire.clone();
    }
    Anchors { fire, smoke, half, top }
}

/// A hash of the unit id to 0..1 (`k` picks another one).
fn unit_hash(id: u32, k: u32) -> f32 {
    acr_render::particles::unit(acr_render::particles::hash(id ^ 0x9e37_79b9), k)
}

/// How far the collapse is (0 standing .. 1 down), `seconds` after the death.
pub fn collapse(seconds: f32) -> f32 {
    let t = (seconds / COLLAPSE_SECONDS).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The model matrix of a building at `ground` falling in (`c` 0..1, [`collapse`]): squashed
/// down to `low` of its height (1 for a ruin state, drawn as built), tilted about a
/// horizontal axis picked by its id, and sunk into the ground.
pub fn collapse_matrix(ground: Vec3, id: u32, c: f32, low: f32) -> Mat4 {
    let mut m = view3d::model_matrix(ground, 0.0, 1.0);
    let squash = 1.0 - (1.0 - low) * c;
    // glTF y is the height: scale its column.
    for v in &mut m[1][..3] {
        *v *= squash;
    }
    let tilt = if low < 1.0 { TILT * (std::f32::consts::PI * c).sin() + TILT_LEFT * c } else { 0.0 };
    if tilt > 0.0 {
        let a = std::f32::consts::TAU * unit_hash(id, 0);
        let axis = [a.cos(), a.sin(), 0.0];
        for col in m.iter_mut().take(3) {
            let v = rotate([col[0], col[1], col[2]], axis, tilt);
            col[..3].copy_from_slice(&v);
        }
    }
    let sink = if low < 1.0 { SINK * c } else { 0.0 };
    m[3][2] -= sink * PIXELS_PER_METRE;
    m
}

/// `v` turned about the unit vector `k` by `angle` (Rodrigues).
fn rotate(v: Vec3, k: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    let cross = [k[1] * v[2] - k[2] * v[1], k[2] * v[0] - k[0] * v[2], k[0] * v[1] - k[1] * v[0]];
    let dot = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
    std::array::from_fn(|i| v[i] * c + cross[i] * s + k[i] * dot * (1.0 - c))
}

/// A model-space point through a model matrix (world pixels).
pub fn place(m: &Mat4, p: Vec3) -> Vec3 {
    std::array::from_fn(|i| m[0][i] * p[0] + m[1][i] * p[1] + m[2][i] * p[2] + m[3][i])
}

/// One burning or fallen building this frame, for [`super::fire_view::FireView::buildings`].
#[derive(Clone, Debug, Default)]
pub struct Burning {
    pub unit: u32,
    /// Its [`Stage`] of burning.
    pub stage: Stage,
    /// Flames at these world points (pixels), with this strength (0..1) and size (1 a
    /// roof fire's flames).
    pub fire: Vec<Vec3>,
    pub flames: f32,
    pub size: f32,
    /// Smoke at these points and strength.
    pub smoke: Vec<Vec3>,
    pub smoking: f32,
    /// Dust and debris of the collapse (0..1), over the footprint centred at `ground` with
    /// half extents `half` and height `top` (pixels).
    pub dust: f32,
    pub ground: Vec3,
    pub half: [f32; 2],
    pub top: f32,
    /// Where pieces struck the ground in the last [`LANDING_DUST`] seconds (pixels).
    pub landings: Vec<Vec3>,
    /// Seconds since it fell in, if it did: the ground round it chars (scorch).
    pub fallen: Option<f32>,
}

/// How far a building burns (#303), by the life it lost, at the original's thresholds:
/// its fire points show below three quarters of its life (`effects::burning_points`,
/// 0x505e5f) and more catch fire as it loses more.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    #[default]
    None,
    /// Small fires at a few of its windows and its roof (from 25 % of its life lost).
    Small,
    /// The roof burns (from 50 %): flames over its top.
    Roof,
    /// A full blaze (from 75 %, and while it falls in).
    Blaze,
}

/// Life lost (share) where each [`Stage`] begins.
const STAGE_FROM: [f32; 3] = [0.25, 0.5, 0.75];
/// Flame strength and size per [`Stage`] (none, small, roof, blaze).
const STAGE_FLAMES: [f32; 4] = [0.0, 0.5, 0.8, 1.0];
const STAGE_SIZE: [f32; 4] = [0.0, 0.55, 1.0, 1.45];
/// Charring of a building that burns down, at its death (the collapse takes it to black).
const BURN_STANDING: f32 = 0.55;

/// The [`Stage`] of a building that lost `lost` (share) of its life.
pub fn stage(lost: f32) -> Stage {
    match STAGE_FROM.iter().filter(|&&f| lost >= f).count() {
        0 => Stage::None,
        1 => Stage::Small,
        2 => Stage::Roof,
        _ => Stage::Blaze,
    }
}

/// Points on a building's roof (model space): `n` x `n` over the inner footprint at height
/// `y`, for a roof fire and a blaze.
fn roof_points(anchors: &Anchors, n: usize, y: f32) -> Vec<Vec3> {
    let (hx, hz) = (anchors.half[0] * 0.6, anchors.half[1] * 0.6);
    let step = |i: usize| if n > 1 { 2.0 * i as f32 / (n - 1) as f32 - 1.0 } else { 0.0 };
    (0..n * n).map(|k| [hx * step(k % n), y, hz * step(k / n)]).collect()
}

/// What a building shows: its burn tint (0..1) and its fires, and the matrix it is drawn
/// with. `lost`: the share of life lost (alive); `dead`: seconds since it died, if it did;
/// `ruin`: it has a ruin state (drawn once the collapse is done).
pub struct Look {
    pub burn: f32,
    pub transform: Mat4,
    pub fire: Burning,
    pub show_ruin: bool,
    /// The fire anchors that burn while it stands (model space, metres) and how hard
    /// (0..1): soot climbs the walls over them (`building_hits`).
    pub sooted: Vec<(Vec3, f32)>,
}

/// The [`Look`] of building `id` at `ground` with `anchors`.
pub fn look(id: u32, ground: Vec3, anchors: &Anchors, lost: f32, dead: Option<f32>, ruin: bool) -> Look {
    let ramp = |x: f32, a: f32, b: f32| ((x - a) / (b - a)).clamp(0.0, 1.0);
    // The share of its fire points alight: the original's n·(T − L)/T with T = 3/4 of its
    // life.
    let alight = ramp(lost, STAGE_FROM[0], 1.0);
    let (burn, transform, stage, lit, smoking, dust, show_ruin, fallen) = match dead {
        None => {
            let m = view3d::model_matrix(ground, 0.0, 1.0);
            // Chars as it burns: a little from the first small fires, most in the blaze.
            let burn = BURN_STANDING * alight.powf(0.8);
            (burn, m, stage(lost), alight, ramp(lost, 0.1, 0.6), 0.0, false, None)
        }
        Some(t) => {
            let c = collapse(t);
            let down = ruin && c >= 1.0;
            let low = if down { 1.0 } else { RUIN_HEIGHT };
            let m = collapse_matrix(ground, id, c, low);
            let after = (t - COLLAPSE_SECONDS).max(0.0);
            let lit = if c < 1.0 { 1.0 } else { (1.0 - after / RUIN_FLAMES).max(0.0) * 0.6 };
            // A blaze while it falls in, the roof's last flames in the rubble, then embers.
            let stage = if c < 1.0 {
                Stage::Blaze
            } else if lit > 0.3 {
                Stage::Roof
            } else if lit > 0.0 {
                Stage::Small
            } else {
                Stage::None
            };
            let smoking = (1.0 - after / RUIN_SMOKE).max(0.0);
            let dust = (1.0 - t / (COLLAPSE_SECONDS + 1.0)).max(0.0);
            let burn = if down {
                RUIN_BURN + (1.0 - RUIN_BURN) * (1.0 - after / RUIN_COOL).max(0.0)
            } else {
                BURN_STANDING + (1.0 - BURN_STANDING) * c
            };
            (burn, m, stage, lit, smoking, dust, down, Some(t))
        }
    };
    let s = stage as usize;
    let flames = if stage == Stage::None { 0.0 } else { STAGE_FLAMES[s] };
    // Anchors burn in an order picked by the id; more of them as it grows.
    let n = anchors.fire.len();
    let lit_n = if stage == Stage::None || n == 0 { 0 } else { ((lit * n as f32).ceil() as usize).clamp(1, n) };
    let start = (unit_hash(id, 1) * n as f32) as usize;
    let chosen: Vec<Vec3> = (0..lit_n).map(|k| anchors.fire[(start + k * 7) % n]).collect();
    // The first to catch burn longest: their soot is darkest.
    let sooted = if dead.is_none() {
        let n = n.max(1) as f32;
        chosen.iter().enumerate().map(|(k, &p)| (p, (alight * 1.5 - k as f32 / n).clamp(0.2, 1.0))).collect()
    } else {
        Vec::new()
    };
    let mut fire: Vec<Vec3> = chosen.iter().map(|&p| place(&transform, p)).collect();
    // The roof catches: flames over its top, all of it in a blaze; in the rubble low.
    let roof = match stage {
        Stage::Roof => 2,
        Stage::Blaze => 3,
        _ => 0,
    };
    let y = if show_ruin { anchors.top * 0.25 } else { anchors.top * 0.92 };
    fire.extend(roof_points(anchors, roof, y).into_iter().map(|p| place(&transform, p)));
    let ns = anchors.smoke.len();
    let columns = ns.min(SMOKE_COLUMNS);
    let smokes = ((smoking * columns as f32).ceil() as usize).min(columns);
    let smoke = (0..smokes).map(|k| place(&transform, anchors.smoke[(start + k * ns / columns.max(1)) % ns])).collect();
    let k = PIXELS_PER_METRE;
    let fire = Burning {
        unit: id,
        stage,
        fire,
        flames,
        size: STAGE_SIZE[s],
        smoke,
        smoking,
        dust,
        ground,
        half: [anchors.half[0] * k, anchors.half[1] * k],
        top: anchors.top * k,
        landings: Vec::new(),
        fallen,
    };
    Look { burn, transform, fire, show_ruin, sooted }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_squashes_tilts_and_sinks() {
        let g = [100.0, 200.0, 10.0];
        let up = [0.0, 10.0, 0.0];
        let stand = place(&collapse_matrix(g, 3, 0.0, RUIN_HEIGHT), up);
        assert!((stand[2] - (10.0 + 10.0 * PIXELS_PER_METRE)).abs() < 1e-3);
        let down = place(&collapse_matrix(g, 3, 1.0, RUIN_HEIGHT), up);
        // Low (about 30 %), sunk, and leaning a little.
        assert!(down[2] < 10.0 + 3.5 * PIXELS_PER_METRE && down[2] > 10.0, "{down:?}");
        assert!((down[0] - g[0]).abs() + (down[1] - g[1]).abs() > 1.0);
        // A ruin state is drawn as built.
        let ruin = place(&collapse_matrix(g, 3, 1.0, 1.0), up);
        assert!((ruin[2] - stand[2]).abs() < 1e-3);
    }

    #[test]
    fn fires_grow_with_lost_life() {
        let a = Anchors { fire: vec![[0.0, 5.0, 0.0]; 10], smoke: vec![[0.0, 5.0, 0.0]; 10], half: [5.0; 2], top: 6.0 };
        let n = |lost| look(1, [0.0; 3], &a, lost, None, false).fire.fire.len();
        assert_eq!(n(0.0), 0);
        assert!(n(0.5) > 0 && n(0.5) < n(0.9));
        let ruin = look(1, [0.0; 3], &a, 1.0, Some(COLLAPSE_SECONDS + RUIN_SMOKE + 1.0), true);
        assert!(ruin.fire.fire.is_empty() && ruin.fire.smoke.is_empty() && ruin.show_ruin);
        // A ruin state carries its own soot: black when it appears, lighter as it cools.
        assert!((ruin.burn - RUIN_BURN).abs() < 1e-3);
        let fresh = look(1, [0.0; 3], &a, 1.0, Some(COLLAPSE_SECONDS + 0.1), true);
        assert!(fresh.show_ruin && fresh.burn > 0.9);
        let shell = look(1, [0.0; 3], &a, 1.0, Some(COLLAPSE_SECONDS + RUIN_SMOKE + 1.0), false);
        assert!(shell.burn >= 0.99);
    }

    #[test]
    fn pieces_fall_bounce_and_settle() {
        let g = [100.0, 200.0, 10.0];
        let c = [1.0, 4.0, 1.0];
        assert!(chunk_fall(g, 7, 0, c, 0.8, 0.0).is_none(), "still part of the building");
        let at = |t| chunk_fall(g, 7, 0, c, 0.8, t).map(|(m, landed)| (place(&m, c), landed));
        let (high, _) = at(BREAK + BREAK_SPREAD + 0.01).unwrap();
        let (low, landed) = at(6.0).unwrap();
        assert!(landed > 0.0 && low[2] < high[2], "{high:?} {low:?}");
        // Still once settled, it lies at the same place; then sinks away.
        let (later, _) = at(8.0).unwrap();
        assert!((later[0] - low[0]).abs() + (later[1] - low[1]).abs() + (later[2] - low[2]).abs() < 1e-3);
        assert!(at(6.0 + CHUNK_HOLD + CHUNK_SINK + 1.0).is_none());
        // Deterministic from the id, and the pieces differ.
        assert_eq!(at(3.0), at(3.0));
        assert_ne!(chunk_fall(g, 7, 1, c, 0.8, 3.0).map(|m| m.0), chunk_fall(g, 7, 0, c, 0.8, 3.0).map(|m| m.0));
    }

    #[test]
    fn smoke_rises_in_a_few_columns() {
        let a = Anchors { fire: vec![[0.0, 5.0, 0.0]; 28], smoke: vec![[0.0, 5.0, 0.0]; 28], half: [5.0; 2], top: 6.0 };
        let burning = look(1, [0.0; 3], &a, 0.9, None, false);
        assert_eq!(burning.fire.smoke.len(), SMOKE_COLUMNS);
        // In a blaze at 90 % lost the original's 28 * (T - L) / T anchors (25, T = 3/4 of
        // its life) burn, and the roof's 3 x 3.
        assert_eq!(burning.fire.fire.len(), 25 + 9);
    }

    #[test]
    fn burning_goes_through_stages() {
        assert_eq!(stage(0.2), Stage::None);
        assert_eq!(stage(0.3), Stage::Small);
        assert_eq!(stage(0.6), Stage::Roof);
        assert_eq!(stage(0.8), Stage::Blaze);
        let a = Anchors { fire: vec![[0.0, 5.0, 0.0]; 10], smoke: vec![[0.0, 5.0, 0.0]; 10], half: [5.0; 2], top: 6.0 };
        let at = |lost| look(1, [0.0; 3], &a, lost, None, false);
        let (small, roof, blaze) = (at(0.3), at(0.6), at(0.9));
        assert!(small.fire.size < roof.fire.size && roof.fire.size < blaze.fire.size);
        assert!(small.fire.fire.len() < roof.fire.fire.len() && roof.fire.fire.len() < blaze.fire.fire.len());
        // Charring grows with it, soot climbs over the fires that burn.
        assert!(small.burn < roof.burn && roof.burn < blaze.burn && blaze.burn <= BURN_STANDING);
        assert!(!small.sooted.is_empty() && small.sooted.len() < blaze.sooted.len());
        assert!(at(0.1).fire.fire.is_empty() && at(0.1).burn == 0.0);
        // Falling in is a blaze, and the ground round it chars.
        let falling = look(1, [0.0; 3], &a, 1.0, Some(0.5), false);
        assert_eq!(falling.fire.stage, Stage::Blaze);
        assert!(falling.sooted.is_empty() && falling.fire.fallen.is_some());
    }
}
