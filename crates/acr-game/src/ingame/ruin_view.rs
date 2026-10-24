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
use acr_render::model::ModelData;
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
const BREAK: f32 = 0.35;
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

/// The pieces of `model` that break off (#303): the triangles whose centre lies in each of
/// the [`CHUNK_BOXES`], with the vertices they use. They stay in the standing model too: they
/// show only once the building, falling in, has left their place.
pub fn chunks(model: &ModelData) -> Vec<Chunk> {
    let palette = model.pose(None, 0.0);
    let world = |v: &acr_render::model::Vertex| -> Vec3 {
        let r = &palette[v.joints[0] as usize];
        std::array::from_fn(|i| r[i][0] * v.position[0] + r[i][1] * v.position[1] + r[i][2] * v.position[2] + r[i][3])
    };
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
    for b in &CHUNK_BOXES {
        let inside = |p: Vec3| {
            (0..3).all(|i| {
                let f = (p[i] - lo[i]) / (hi[i] - lo[i]).max(1e-4);
                f >= b[i][0] && f <= b[i][1]
            })
        };
        let mut primitives = Vec::new();
        let mut triangles = 0usize;
        let mut points: Vec<Vec3> = Vec::new();
        for p in &model.primitives {
            let mut map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
            let mut prim = acr_render::model::Primitive {
                vertices: Vec::new(),
                indices: Vec::new(),
                material: p.material,
                morphs: p.morphs,
                look_morphs: p.look_morphs.iter().map(|(k, _)| (*k, Vec::new())).collect(),
                lod: None,
            };
            for t in p.indices.as_chunks::<3>().0 {
                let c = t.iter().fold([0.0f32; 3], |c, &i| {
                    let q = world(&p.vertices[i as usize]);
                    [c[0] + q[0] / 3.0, c[1] + q[1] / 3.0, c[2] + q[2] / 3.0]
                });
                if !inside(c) {
                    continue;
                }
                triangles += 1;
                for &i in t {
                    let next = prim.vertices.len() as u32;
                    let j = *map.entry(i).or_insert_with(|| {
                        let v = &p.vertices[i as usize];
                        points.push(world(v));
                        prim.vertices.push(*v);
                        for ((_, offsets), (_, all)) in prim.look_morphs.iter_mut().zip(&p.look_morphs) {
                            offsets.push(all.get(i as usize).copied().unwrap_or_default());
                        }
                        next
                    });
                    prim.indices.push(j);
                }
            }
            if !prim.indices.is_empty() {
                primitives.push(prim);
            }
        }
        if triangles < CHUNK_MIN_TRIANGLES {
            continue;
        }
        let n = points.len() as f32;
        let sum = points.iter().fold([0.0f32; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
        let centre = [sum[0] / n, sum[1] / n, sum[2] / n];
        let radius = points.iter().fold(0.0f32, |r, p| {
            r.max(((p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2) + (p[2] - centre[2]).powi(2)).sqrt())
        });
        let mut piece = model.clone();
        (piece.primitives, piece.images) = (primitives, images.clone());
        out.push(Chunk { model: piece, centre, radius });
    }
    out
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
    /// Flames at these world points (pixels), with this strength (0..1).
    pub fire: Vec<Vec3>,
    pub flames: f32,
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
}

/// What a building shows: its burn tint (0..1) and its fires, and the matrix it is drawn
/// with. `lost`: the share of life lost (alive); `dead`: seconds since it died, if it did;
/// `ruin`: it has a ruin state (drawn once the collapse is done).
pub struct Look {
    pub burn: f32,
    pub transform: Mat4,
    pub fire: Burning,
    pub show_ruin: bool,
}

/// The [`Look`] of building `id` at `ground` with `anchors`.
pub fn look(id: u32, ground: Vec3, anchors: &Anchors, lost: f32, dead: Option<f32>, ruin: bool) -> Look {
    let ramp = |x: f32, a: f32, b: f32| ((x - a) / (b - a)).clamp(0.0, 1.0);
    let (burn, transform, flames, smoking, dust, show_ruin) = match dead {
        None => {
            // Burning down: flames from a quarter of its life lost on (the original's
            // sprite fires show from about there), all of them near the end.
            let flames = ramp(lost, 0.2, 0.85);
            let m = view3d::model_matrix(ground, 0.0, 1.0);
            (0.55 * ramp(lost, 0.3, 1.0), m, flames, ramp(lost, 0.1, 0.6), 0.0, false)
        }
        Some(t) => {
            let c = collapse(t);
            let down = ruin && c >= 1.0;
            let low = if down { 1.0 } else { RUIN_HEIGHT };
            let m = collapse_matrix(ground, id, c, low);
            let after = (t - COLLAPSE_SECONDS).max(0.0);
            let flames = if c < 1.0 { 1.0 } else { (1.0 - after / RUIN_FLAMES).max(0.0) * 0.6 };
            let smoking = (1.0 - after / RUIN_SMOKE).max(0.0);
            let dust = (1.0 - t / (COLLAPSE_SECONDS + 1.0)).max(0.0);
            let burn =
                if down { RUIN_BURN + (1.0 - RUIN_BURN) * (1.0 - after / RUIN_COOL).max(0.0) } else { 0.55 + 0.45 * c };
            (burn, m, flames, smoking, dust, down)
        }
    };
    // Anchors burn in an order picked by the id; more of them as it grows.
    let n = anchors.fire.len();
    let lit = ((flames * n as f32).ceil() as usize).min(n);
    let start = (unit_hash(id, 1) * n as f32) as usize;
    let fire = (0..lit).map(|k| place(&transform, anchors.fire[(start + k * 7) % n])).collect();
    let ns = anchors.smoke.len();
    let columns = ns.min(SMOKE_COLUMNS);
    let smokes = ((smoking * columns as f32).ceil() as usize).min(columns);
    let smoke = (0..smokes).map(|k| place(&transform, anchors.smoke[(start + k * ns / columns.max(1)) % ns])).collect();
    let k = PIXELS_PER_METRE;
    let fire = Burning {
        unit: id,
        fire,
        flames,
        smoke,
        smoking,
        dust,
        ground,
        half: [anchors.half[0] * k, anchors.half[1] * k],
        top: anchors.top * k,
        landings: Vec::new(),
    };
    Look { burn, transform, fire, show_ruin }
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
        assert_eq!(burning.fire.fire.len(), 28);
    }
}
