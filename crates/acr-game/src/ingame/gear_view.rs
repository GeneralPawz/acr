//! Weight for carried gear (docs/assets-humans.md, "Seated gear", "Lag"): the knapsack,
//! quivers, the back shield, the cartridge box, horn and pouch (`gear.lag_bone`: a
//! `gear.<item>` bone each, keyed at rest under the item's main bone) and the slung musket
//! (`socket.sling`) of the men near the camera hang on a damped spring and lag and bob a
//! little behind the body when it walks, runs or stops, then settle.
//!
//! Per item one point mass `PENDULUM_M` below the bone's head (the top of the item's contact
//! side, where the strap holds it) follows the skinned point in physics space (metres,
//! world-fixed): a spring with damping relative to the target, so a steady walk or run
//! leaves no lag, only its steps and the starts and stops do. The offset is kept small
//! (`MAX_SWING_M`, `MAX_BOB_M`) and never towards the body (the item's outward side, from the
//! body's axis to its head, horizontally): the bottom swings out and sideways, never in. It
//! goes back as a turn of the bone about its head (the horizontal part) and a small lift (the
//! vertical part). `ACR_NO_GEAR` switches it off.

use std::collections::HashMap;

use acr_physics::Vec3;
use acr_render::math::{self, Mat4};
use acr_render::model::{ModelData, Palette, Slot};
use acr_render::view3d::PIXELS_PER_METRE;

use super::cloth_view::{affine_inverse, apply_rows, apply4, rotation_between, turned, v3};
use super::ragdoll_view::Drawn;

/// Men with live gear at most, and how far from the camera's focus (world pixels).
const BUDGET: usize = 40;
const REACH_PX: f32 = 900.0;
/// The point mass below the bone's head (m, model space).
const PENDULUM_M: f32 = 0.25;
/// Spring frequency (Hz) and damping ratio: a knapsack settles in about half a second.
const FREQ_HZ: f32 = 2.2;
const DAMPING: f32 = 0.35;
/// Most the point swings sideways and out (m; about 10° at `PENDULUM_M`) and bobs up or down:
/// the lag bone carries `gear.LAG_SHARE` (0.6) of the item, so it shows as about 2.7 cm (6°)
/// and 1 cm.
const MAX_SWING_M: f32 = 0.045;
const MAX_BOB_M: f32 = 0.016;
/// The slung musket turns about its middle: a third of that.
const MUSKET_SHARE: f32 = 0.35;
/// Time constant (s) of the target smoothing (units move once per tick).
const SMOOTH_S: f32 = 0.04;
/// Substep (s) of the spring.
const STEP_S: f32 = 1.0 / 120.0;
/// A target jumping further than this between frames (m) restarts the spring.
const TELEPORT_M: f32 = 1.0;

/// A lagging item of a model: its palette slot, its head at rest (glTF model space), its
/// outward direction (horizontal, model space) and whether it is the slung musket.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Item {
    slot: usize,
    head: [f32; 3],
    out: [f32; 3],
    musket: bool,
}

/// The lagging items of `model`: joints named `gear.*` or `socket.sling` (with any rig
/// prefix).
fn items(model: &ModelData) -> Vec<Item> {
    model
        .slots
        .iter()
        .enumerate()
        .filter_map(|(slot, s)| {
            let Slot::Joint { node, inverse_bind } = *s else { return None };
            let name = &model.nodes[node].name;
            let musket = name.ends_with("socket.sling");
            if !musket && !name.split(':').next_back().is_some_and(|n| n.contains("gear.")) {
                return None;
            }
            let bind = affine_inverse(&inverse_bind);
            let head = [bind[3][0], bind[3][1], bind[3][2]];
            // glTF: +Y up; the body's axis through x = z = 0.
            let out = Vec3::new(head[0], 0.0, head[2]).normalize_or_zero();
            let out = if out == Vec3::ZERO { Vec3::new(0.0, 0.0, -1.0) } else { out };
            Some(Item { slot, head, out: out.to_array(), musket })
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct Bob {
    x: Vec3,
    v: Vec3,
    smooth: Vec3,
    seen: u64,
}

/// The gear of the men near the camera.
#[derive(Default)]
pub struct Gear {
    rigs: HashMap<usize, Vec<Item>>,
    live: HashMap<(u32, usize), Bob>,
    frame: u64,
    /// Items stepped in the last frame and the time it took (µs), for `cost`.
    pub stepped: usize,
    pub micros: u64,
}

fn rows3(m: &[[f32; 4]; 3], d: [f32; 3]) -> Vec3 {
    Vec3::new(
        m[0][0] * d[0] + m[0][1] * d[1] + m[0][2] * d[2],
        m[1][0] * d[0] + m[1][1] * d[1] + m[1][2] * d[2],
        m[2][0] * d[0] + m[2][1] * d[1] + m[2][2] * d[2],
    )
}

fn lin4(m: &Mat4, d: Vec3) -> Vec3 {
    Vec3::new(
        m[0][0] * d.x + m[1][0] * d.y + m[2][0] * d.z,
        m[0][1] * d.x + m[1][1] * d.y + m[2][1] * d.z,
        m[0][2] * d.x + m[1][2] * d.y + m[2][2] * d.z,
    )
}

impl Gear {
    /// Steps the gear of the living men near `focus` (world pixels) by `seconds` and writes
    /// their gear bones into `poses` (the frame's custom palettes by instance; the clip's
    /// pose where an instance has none). The dying keep theirs.
    pub fn update(
        &mut self,
        seconds: f32,
        drawn: &[&[Drawn<'_>]],
        focus: [f32; 2],
        poses: &mut HashMap<usize, Palette>,
    ) {
        let started = web_time::Instant::now();
        self.frame += 1;
        self.stepped = 0;
        let dt = seconds.clamp(0.0, 1.0 / 20.0);
        let mut near: Vec<(f32, &Drawn<'_>)> = drawn
            .iter()
            .flat_map(|list| list.iter())
            .filter(|d| d.unit.alive && d.unit.death == 0 && !d.unit.building)
            .filter_map(|d| {
                let (dx, dy) = (d.transform[3][0] - focus[0], d.transform[3][1] - focus[1]);
                let r2 = dx * dx + dy * dy;
                (r2 < REACH_PX * REACH_PX).then_some((r2, d))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut done = 0;
        for (_, d) in near {
            if done >= BUDGET {
                break;
            }
            let items = self.rigs.entry(d.model).or_insert_with(|| items(d.data));
            if items.is_empty() {
                continue;
            }
            let items = items.clone();
            let mut palette = poses.get(&d.index).cloned().unwrap_or_else(|| d.data.pose(d.clip, d.time));
            for (k, it) in items.iter().enumerate() {
                self.step(d.unit.id.0, k, it, &d.transform, &mut palette, dt);
            }
            self.stepped += items.len();
            poses.insert(d.index, palette);
            done += 1;
        }
        let frame = self.frame;
        self.live.retain(|_, b| b.seen + 2 >= frame);
        self.micros = started.elapsed().as_micros() as u64;
    }

    fn step(&mut self, unit: u32, k: usize, it: &Item, transform: &Mat4, palette: &mut Palette, dt: f32) {
        let Some(&m) = palette.get(it.slot) else { return };
        let up = [0.0, 1.0, 0.0];
        let bob_rest = [it.head[0], it.head[1] - PENDULUM_M, it.head[2]];
        let head = apply_rows(&m, it.head);
        let bob = apply_rows(&m, bob_rest);
        // A hidden item (the slung musket's socket scaled to 0) keeps its pose.
        let out_m = rows3(&m, it.out);
        if out_m.length() < 1e-3 {
            return;
        }
        let k_ = 1.0 / PIXELS_PER_METRE;
        let physics_of_world: Mat4 =
            [[k_, 0.0, 0.0, 0.0], [0.0, -k_, 0.0, 0.0], [0.0, 0.0, k_, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let to_physics = math::mul(&physics_of_world, transform);
        let target = v3(apply4(&to_physics, bob));
        if !target.is_finite() {
            return;
        }
        let out = lin4(&to_physics, out_m).normalize_or_zero();
        let vert = lin4(&to_physics, rows3(&m, up)).normalize_or_zero();
        let frame = self.frame;
        let b = self.live.entry((unit, k)).or_insert(Bob { x: target, v: Vec3::ZERO, smooth: target, seen: frame });
        if b.seen + 2 < frame || (b.x - target).length() > TELEPORT_M {
            *b = Bob { x: target, v: Vec3::ZERO, smooth: target, seen: frame };
        }
        b.seen = frame;
        let before = b.smooth;
        let a = 1.0 - (-dt / SMOOTH_S).exp();
        b.smooth += (target - b.smooth) * a;
        let w = std::f32::consts::TAU * FREQ_HZ;
        let n = (dt / STEP_S).ceil().max(1.0);
        let h = dt / n;
        let vt = if dt > 0.0 { (b.smooth - before) * (1.0 / dt) } else { Vec3::ZERO };
        let share = if it.musket { MUSKET_SHARE } else { 1.0 };
        for i in 0..n as usize {
            let t = before + (b.smooth - before) * ((i + 1) as f32 / n);
            let acc = (t - b.x) * (w * w) + (vt - b.v) * (2.0 * DAMPING * w);
            b.v += acc * h;
            b.x += b.v * h;
            // Never towards the body; small.
            let mut o = b.x - t;
            let into = o.dot(out);
            if into < 0.0 {
                o -= out * into;
                let vi = (b.v - vt).dot(out);
                if vi < 0.0 {
                    b.v -= out * vi;
                }
            }
            let up_o = o.dot(vert).clamp(-MAX_BOB_M, MAX_BOB_M) * share;
            let side = o - vert * o.dot(vert);
            let (len, max) = (side.length(), MAX_SWING_M * share);
            let side = if len > max { side * (max / len) } else { side };
            b.x = t + side + vert * up_o;
        }
        // Back to model space: lift by the vertical part, turn about the head by the rest.
        let o = b.x - b.smooth;
        let to_model = affine_inverse(&to_physics);
        let lift = lin4(&to_model, vert * o.dot(vert));
        let side = lin4(&to_model, o - vert * o.dot(vert));
        let swung = [bob[0] + side.x, bob[1] + side.y, bob[2] + side.z];
        let r = rotation_between(sub(bob, head), sub(swung, head));
        let to = [head[0] + lift.x, head[1] + lift.y, head[2] + lift.z];
        palette[it.slot] = turned(&m, &r, head, to);
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: [[f32; 4]; 3] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

    fn pack() -> Item {
        // On the back (glTF −Z), the head at the top of the pack.
        Item { slot: 0, head: [0.0, 1.45, -0.2], out: [0.0, 0.0, -1.0], musket: false }
    }

    fn run(gear: &mut Gear, xs: impl Iterator<Item = f32>) -> Vec<[f32; 3]> {
        let it = pack();
        let bottom = [0.0, 1.1, -0.2];
        xs.map(|x| {
            gear.frame += 1;
            let transform = acr_render::view3d::model_matrix([x, 400.0, 0.0], std::f32::consts::FRAC_PI_2, 1.0);
            let mut p = vec![ID];
            gear.step(1, 0, &it, &transform, &mut p, 1.0 / 60.0);
            apply_rows(&p[0], bottom)
        })
        .collect()
    }

    #[test]
    fn at_rest_the_pack_stays_put() {
        let mut g = Gear::default();
        let out = run(&mut g, (0..120).map(|_| 300.0));
        let last = out.last().unwrap();
        assert!(sub(*last, [0.0, 1.1, -0.2]).iter().all(|d| d.abs() < 1e-3), "{last:?}");
    }

    #[test]
    fn a_stop_swings_it_a_little_and_it_settles() {
        let mut g = Gear::default();
        // Running at 4 m/s along +x (world), then stopping dead.
        let v = 4.0 * PIXELS_PER_METRE / 60.0;
        let xs = (0..60).map(move |f| 300.0 + v * f as f32).chain((0..120).map(move |_| 300.0 + v * 59.0));
        let out = run(&mut g, xs);
        let rest = [0.0, 1.1, -0.2];
        let most = out.iter().map(|p| v3(sub(*p, rest)).length()).fold(0.0, f32::max);
        assert!(most > 0.005, "it lags: {most}");
        // The bone (0.35 m below the head here; the mesh takes 0.6 of it).
        assert!(most < 0.07, "it stays small: {most}");
        // Never into the body (glTF +Z from the back at z = −0.2), allowing the turn's arc.
        assert!(out.iter().all(|p| p[2] < -0.2 + 1e-3), "{out:?}");
        let last = out.last().unwrap();
        assert!(v3(sub(*last, rest)).length() < 2e-3, "settles: {last:?}");
    }

    #[test]
    #[ignore]
    fn cost_per_item() {
        let mut g = Gear::default();
        let it = pack();
        let transform = acr_render::view3d::model_matrix([300.0, 400.0, 0.0], 0.3, 1.0);
        let mut p = vec![ID];
        let n = 100_000;
        let t = std::time::Instant::now();
        for f in 0..n {
            g.frame += 1;
            g.step(f % 64, 0, &it, &transform, &mut p, 1.0 / 60.0);
        }
        println!("{:.3} µs per item step", t.elapsed().as_secs_f64() * 1e6 / n as f64);
    }
}
