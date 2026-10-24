//! Burning, collapsing and ruined buildings in 3D (#303), render only. A finished building
//! keeps its model while it burns down (`SLOWDEATH`, `combat::slow_death`), with flames and
//! smoke at its fire anchors (`socket.fire.<i>`, `socket.smoke.<i>`) or, for a model without
//! them, at points spread over its roof; they grow with the life it lost and its model chars.
//! When it dies its model sinks, tilts and is squashed into the ground over
//! [`COLLAPSE_SECONDS`], charring black, in dust and debris; then it stays as a ruin (the
//! model's `<name>.ruin` state group where it has one, else the same mesh cut low) that
//! smokes for a while, for as long as the simulation keeps the unit (its `#DEATHLIE`, as the
//! original's ruin sprite). Variety comes from the unit id, never the simulation's RNG.

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
            (0.55 + 0.45 * c, m, flames, smoking, dust, down)
        }
    };
    // Anchors burn in an order picked by the id; more of them as it grows.
    let n = anchors.fire.len();
    let lit = ((flames * n as f32).ceil() as usize).min(n);
    let start = (unit_hash(id, 1) * n as f32) as usize;
    let fire = (0..lit).map(|k| place(&transform, anchors.fire[(start + k * 7) % n])).collect();
    let ns = anchors.smoke.len();
    let smokes = ((smoking * ns as f32).ceil() as usize).min(ns);
    let smoke = (0..smokes).map(|k| place(&transform, anchors.smoke[(start + k * 5) % ns])).collect();
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
        assert!(ruin.burn >= 0.99);
    }
}
