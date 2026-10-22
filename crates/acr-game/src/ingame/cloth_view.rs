//! Runtime cloth for the coat skirts (docs/clothing.md, "Runtime cloth"): the skirt bones
//! (`skirt_{fl,bl,br,fr}_0{1,2}`, rig.SKIRT_COLUMNS) of men near the camera follow an XPBD
//! cloth ([`acr_physics::Cloth`]) instead of the clip or the ragdoll's hips.
//!
//! The proxy is the skirt rig itself: per column the waist point (pinned to the skinned
//! pose), the knee and the hem; across the skirt the columns are joined (stretch only) but
//! for the opening in front. Targets come from the instance's palette (the clip, the gait
//! or the ragdoll), the legs collide as capsules from the thigh and shin bones, the ground
//! from the terrain. The cloth runs in physics space (metres, `+Z` up, world-fixed), so a
//! man walking or running drags his skirts through the air (inertia), and a man falling
//! has them laid on the ground. Results go back as rotations of the two skirt bones of
//! each column about their heads. Men further away keep the skinned skirts.

use std::collections::HashMap;

use acr_physics::{Capsule, Cloth, ClothConfig, Ground, Vec3};
use acr_render::math::{self, Mat4};
use acr_render::model::{ModelData, Palette, Slot};
use acr_render::terrain::TerrainData;
use acr_render::view3d::PIXELS_PER_METRE;

use super::ragdoll_view::{Drawn, ground_height};

/// Skirt columns in proxy order round the body (front left, back left, back right, front
/// right; the opening between front right and front left), skeleton-space `(x, y)` at the
/// waist (rig.SKIRT_COLUMNS).
const COLUMNS: [(&str, f32, f32); 4] =
    [("fl", 0.125, -0.115), ("bl", 0.12, 0.125), ("br", -0.12, 0.125), ("fr", -0.125, -0.115)];
/// Heights of the waist, knee and hem joints (rig.SKIRT_TOP, SKIRT_KNEE, SKIRT_BOTTOM) and
/// the flare of the knee and the hem points.
const ROWS: [(f32, f32, f32); 3] = [(1.0, 1.0, 1.0), (0.78, 1.08, 1.1), (0.55, 1.16, 1.2)];
/// Leg capsules: bone (skeleton H) and radius with the gaiters (m). Only the shins: the
/// skinned skirt already follows the thigh by its corrective share (`drape._follow_thigh`),
/// and a thigh capsule on four columns pushed the hems through to the wrong side of a
/// leg swinging past them. The shins catch the skirts of a man kneeling, falling or lying.
const LEGS: [(&str, f32); 2] = [("shin_l", 0.055), ("shin_r", 0.055)];
/// Men with live skirts at most, and how far from the camera's focus (world pixels).
const BUDGET: usize = 24;
const REACH_PX: f32 = 900.0;
/// Time constant (s) of the target smoothing.
const SMOOTH_S: f32 = 0.04;
/// The hem's drive back to the drape (1/s²) of a man lying down.
const DRIVE_LYING: f32 = 5.0;
/// A target jumping further than this between frames (m) restarts the cloth.
const TELEPORT_M: f32 = 1.0;

/// The skirt rig of a model: per column the slots of its two bones; the leg slots and
/// their rest segments (skeleton space).
#[derive(Debug, Clone)]
struct SkirtRig {
    skirt: [[usize; 2]; 4],
    legs: [(usize, [f32; 3], [f32; 3], f32); 2],
    /// The thigh slots (left, right): the corrective share.
    thighs: [usize; 2],
}

impl SkirtRig {
    fn new(model: &ModelData) -> Option<SkirtRig> {
        let slot = |name: &str| {
            model.slots.iter().position(|s| matches!(*s, Slot::Joint { node, .. } if model.nodes[node].name == name))
        };
        let mut skirt = [[0; 2]; 4];
        for (c, (col, _, _)) in COLUMNS.iter().enumerate() {
            for (k, s) in skirt[c].iter_mut().enumerate() {
                *s = slot(&format!("skirt_{col}_0{}", k + 1))?;
            }
        }
        let skeleton = acr_physics::skeleton_h();
        let mut legs = [(0, [0.0; 3], [0.0; 3], 0.0); 2];
        for (l, &(bone, r)) in LEGS.iter().enumerate() {
            let b = skeleton.iter().find(|b| b.name == bone)?;
            legs[l] = (slot(bone)?, b.head, b.tail, r);
        }
        Some(SkirtRig { skirt, legs, thighs: [slot("thigh_l")?, slot("thigh_r")?] })
    }
}

struct Live {
    cloth: Cloth,
    seen: u64,
    /// The targets smoothed over about a tick: units move once per tick and are drawn in
    /// several frames, and the jerk would fling the skirts.
    smooth: [Vec3; 12],
}

/// The skirts of the men near the camera.
#[derive(Default)]
pub struct Skirts {
    rigs: HashMap<usize, Option<SkirtRig>>,
    live: HashMap<u32, Live>,
    frame: u64,
}

struct Terrain<'a>(&'a TerrainData);

impl Ground for Terrain<'_> {
    fn height(&self, x: f32, y: f32) -> f32 {
        ground_height(self.0, x, y)
    }
}

/// Skeleton space → glTF model space: `(x, y, z) → (x, z, −y)`.
fn gltf_of(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], -p[1]]
}

fn apply_rows(m: &[[f32; 4]; 3], p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3])
}

fn apply4(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

fn v3(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}

/// Inverse of an affine matrix (column-major).
fn affine_inverse(m: &Mat4) -> Mat4 {
    let a = |r: usize, c: usize| m[c][r];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    let d = 1.0 / det;
    let inv = [
        [
            (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) * d,
            (a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2)) * d,
            (a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1)) * d,
        ],
        [
            (a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2)) * d,
            (a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0)) * d,
            (a(0, 2) * a(1, 0) - a(0, 0) * a(1, 2)) * d,
        ],
        [
            (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0)) * d,
            (a(0, 1) * a(2, 0) - a(0, 0) * a(2, 1)) * d,
            (a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0)) * d,
        ],
    ];
    let t = [m[3][0], m[3][1], m[3][2]];
    let it: [f32; 3] = std::array::from_fn(|r| -(inv[r][0] * t[0] + inv[r][1] * t[1] + inv[r][2] * t[2]));
    [
        [inv[0][0], inv[1][0], inv[2][0], 0.0],
        [inv[0][1], inv[1][1], inv[2][1], 0.0],
        [inv[0][2], inv[1][2], inv[2][2], 0.0],
        [it[0], it[1], it[2], 1.0],
    ]
}

/// The rotation taking the direction `a` onto `b` (rows), identity if either is degenerate
/// or they are opposite.
fn rotation_between(a: [f32; 3], b: [f32; 3]) -> [[f32; 3]; 3] {
    let (a, b) = (v3(a).normalize_or_zero(), v3(b).normalize_or_zero());
    let v = a.cross(b);
    let c = a.dot(b);
    if a == Vec3::ZERO || b == Vec3::ZERO || c < -0.999 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let k = 1.0 / (1.0 + c);
    [
        [1.0 - k * (v.y * v.y + v.z * v.z), -v.z + k * v.x * v.y, v.y + k * v.x * v.z],
        [v.z + k * v.x * v.y, 1.0 - k * (v.x * v.x + v.z * v.z), -v.x + k * v.y * v.z],
        [-v.y + k * v.x * v.z, v.x + k * v.y * v.z, 1.0 - k * (v.x * v.x + v.y * v.y)],
    ]
}

/// `p ↦ R (p − from) + to` after the palette rows `m`.
fn turned(m: &[[f32; 4]; 3], r: &[[f32; 3]; 3], from: [f32; 3], to: [f32; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|i| {
        let mut row = [0.0; 4];
        for (c, x) in row.iter_mut().enumerate() {
            *x = (0..3).map(|k| r[i][k] * m[k][c]).sum();
        }
        row[3] += to[i] - (0..3).map(|k| r[i][k] * from[k]).sum::<f32>();
        row
    })
}

impl Skirts {
    /// Steps the skirts of the men near `focus` (world pixels) by `seconds` and writes
    /// their skirt bones into `poses` (the frame's custom palettes by instance; the clip's
    /// pose where an instance has none): the dying and the dead on a ragdoll. Corpses on
    /// the clip keep it.
    pub fn update(
        &mut self,
        seconds: f32,
        drawn: &[Drawn<'_>],
        focus: [f32; 2],
        terrain: &TerrainData,
        poses: &mut HashMap<usize, Palette>,
    ) {
        self.frame += 1;
        let dt = seconds.clamp(0.0, 1.0 / 20.0);
        // Men on their feet keep the skinned skirts for now (`ACR_CLOTH=all` simulates them
        // too): on a walker the four-column proxy still lifts the near skirt (docs/clothing.md).
        let all = std::env::var("ACR_CLOTH").is_ok_and(|v| v == "all");
        let dying = |d: &Drawn<'_>| !d.unit.alive || d.unit.death > 0;
        let mut near: Vec<(f32, usize)> = drawn
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                !d.unit.body.mounted && !d.unit.building && (all || poses.contains_key(&d.index) && dying(d))
            })
            .filter_map(|(i, d)| {
                let (dx, dy) = (d.transform[3][0] - focus[0], d.transform[3][1] - focus[1]);
                let r2 = dx * dx + dy * dy;
                (r2 < REACH_PX * REACH_PX).then_some((r2, i))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let ground = Terrain(terrain);
        let mut done = 0;
        for (_, i) in near {
            if done >= BUDGET {
                break;
            }
            let d = &drawn[i];
            let rig = self.rigs.entry(d.model).or_insert_with(|| SkirtRig::new(d.data)).clone();
            let Some(rig) = rig else { continue };
            let palette = poses.get(&d.index).cloned().unwrap_or_else(|| d.data.pose(d.clip, d.time));
            if let Some(p) = self.skirt(d.unit.id.0, &d.transform, &rig, palette, dt, &ground) {
                poses.insert(d.index, p);
                done += 1;
            }
        }
        let frame = self.frame;
        self.live.retain(|_, l| l.seen + 2 >= frame);
    }

    fn skirt(
        &mut self,
        unit: u32,
        transform: &Mat4,
        rig: &SkirtRig,
        mut palette: Palette,
        dt: f32,
        ground: &dyn Ground,
    ) -> Option<Palette> {
        let k = 1.0 / PIXELS_PER_METRE;
        let physics_of_world: Mat4 =
            [[k, 0.0, 0.0, 0.0], [0.0, -k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let to_physics = math::mul(&physics_of_world, transform);
        let to_model = affine_inverse(&to_physics);
        // Rest points (glTF) and their skinned targets (physics), column by column.
        let rest: Vec<[f32; 3]> = COLUMNS
            .iter()
            .flat_map(|&(_, x, y)| ROWS.iter().map(move |&(z, fx, fy)| gltf_of([x * fx, y * fy, z])))
            .collect();
        // Where the skirt is drawn: the skirt bone blended with the thigh's corrective share
        // (`drape._follow_thigh`), which the cloth has to leave in place.
        let mut targets = [Vec3::ZERO; 12];
        let mut drawn = [[0.0f32; 3]; 12];
        let mut skins = [[0.0f32; 3]; 12];
        for (c, &(_, x, _)) in COLUMNS.iter().enumerate() {
            for r in 0..3 {
                let k = c * 3 + r;
                let slot = rig.skirt[c][if r == 0 { 0 } else { r - 1 }];
                let skin = apply_rows(palette.get(slot)?, rest[k]);
                let leg = if x > 0.0 { rig.thighs[0] } else { rig.thighs[1] };
                let thigh = apply_rows(palette.get(leg)?, rest[k]);
                let s = thigh_share(c, r);
                drawn[k] = std::array::from_fn(|i| s * thigh[i] + (1.0 - s) * skin[i]);
                skins[k] = skin;
                targets[k] = v3(apply4(&to_physics, drawn[k]));
            }
        }
        if targets.iter().any(|t| !t.is_finite()) {
            return None;
        }
        let capsules: Vec<Capsule> = rig
            .legs
            .iter()
            .filter_map(|&(slot, a, b, r)| {
                let rows = palette.get(slot)?;
                let p = |q: [f32; 3]| v3(apply4(&to_physics, apply_rows(rows, gltf_of(q))));
                Some(Capsule { a: p(a), b: p(b), r })
            })
            .collect();
        let frame = self.frame;
        let live = self.live.entry(unit).or_insert_with(|| Live {
            cloth: Cloth::skirt(
                &rest.iter().map(|p| v3(*p)).collect::<Vec<_>>(),
                4,
                3,
                &[false, false, false, true],
                ClothConfig::default(),
            ),
            seen: 0,
            smooth: targets,
        });
        if live.seen + 2 < frame || (live.cloth.pos[0] - targets[0]).length() > TELEPORT_M {
            live.cloth.reset(&targets);
            live.smooth = targets;
        }
        let a = 1.0 - (-dt / SMOOTH_S).exp();
        for (s, t) in live.smooth.iter_mut().zip(&targets) {
            *s += (*t - *s) * a;
        }
        live.seen = frame;
        // Upright (the skirts hang from the waist as draped): a firm shape memory; lying
        // down it lets go and gravity lays the skirts on the ground.
        let waist = (targets[0] + targets[3] + targets[6] + targets[9]) * 0.25;
        let knee = (targets[1] + targets[4] + targets[7] + targets[10]) * 0.25;
        let upright = (waist - knee).normalize_or_zero().z.clamp(0.0, 1.0);
        live.cloth.config.drive[1] = DRIVE_LYING + (ClothConfig::default().drive[1] - DRIVE_LYING) * upright * upright;
        live.cloth.step(dt, &live.smooth, Vec3::ZERO, &capsules, ground);
        // Back to model space: the skirt bone takes the cloth's offset from where the skirt
        // is drawn (the thigh's share keeps its part: the legs never pass through, and the
        // swing shows damped by it); each column's bones turned about their heads.
        let smooth = live.smooth;
        let bone_point = |k: usize, p: Vec3| {
            let x = apply4(&to_model, p.to_array());
            let t = apply4(&to_model, smooth[k].to_array());
            std::array::from_fn::<f32, 3, _>(|i| skins[k][i] + x[i] - t[i])
        };
        for c in 0..4 {
            let [s1, s2] = rig.skirt[c];
            let (top, knee, hem) = (rest[c * 3], rest[c * 3 + 1], rest[c * 3 + 2]);
            let m1 = palette[s1];
            let h = apply_rows(&m1, top);
            let k_anim = apply_rows(&m1, knee);
            let k_sim = bone_point(c * 3 + 1, live.cloth.pos[c * 3 + 1]);
            let r1 = rotation_between(sub(k_anim, h), sub(k_sim, h));
            palette[s1] = turned(&m1, &r1, h, h);
            let m2 = palette[s2];
            let k2 = apply_rows(&m2, knee);
            let b_anim = apply_rows(&m2, hem);
            let b_sim = bone_point(c * 3 + 2, live.cloth.pos[c * 3 + 2]);
            let r2 = rotation_between(sub(b_anim, k2), sub(b_sim, k_sim));
            // The hem bone hangs from where the knee bone now ends.
            let k_new = apply_rows(&palette[s1], knee);
            palette[s2] = turned(&m2, &r2, k2, k_new);
        }
        Some(palette)
    }
}

/// The thigh's corrective share of the skirt at column `c`, row `r` (`drape._follow_thigh`:
/// `t · (0.30 + 0.25 · front)`, `t` 0 at the waist, 1 at the hem).
fn thigh_share(c: usize, r: usize) -> f32 {
    let t = (ROWS[0].0 - ROWS[r].0) / (ROWS[0].0 - ROWS[2].0);
    let front = if COLUMNS[c].2 < 0.0 { 1.0 } else { 0.0 };
    t * (0.30 + 0.25 * front)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    #[test]
    fn rotation_between_turns_a_onto_b() {
        let r = rotation_between([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]);
        let p: [f32; 3] = std::array::from_fn(|i| -r[i][1]);
        assert!((p[0] - 1.0).abs() < 1e-5 && p[1].abs() < 1e-5 && p[2].abs() < 1e-5, "{p:?}");
    }

    #[test]
    fn turned_keeps_the_head_and_moves_the_tail() {
        let id = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let head = [0.1, 1.0, 0.1];
        let tail = [0.1, 0.78, 0.1];
        let want = [0.1, 0.85, 0.3];
        let r = rotation_between(sub(tail, head), sub(want, head));
        let m = turned(&id, &r, head, head);
        let h = apply_rows(&m, head);
        let t = apply_rows(&m, tail);
        assert!(sub(h, head).iter().all(|x| x.abs() < 1e-5));
        let dir = v3(sub(t, h)).normalize_or_zero();
        let w = v3(sub(want, head)).normalize_or_zero();
        assert!(dir.dot(w) > 0.9999, "{t:?}");
    }

    pub(super) struct Flat;
    impl Ground for Flat {
        fn height(&self, _x: f32, _y: f32) -> f32 {
            0.0
        }
    }

    fn rest_rig() -> SkirtRig {
        let skeleton = acr_physics::skeleton_h();
        let leg = |i: usize, name: &str, r: f32| {
            let b = skeleton.iter().find(|b| b.name == name).unwrap();
            (8 + i, b.head, b.tail, r)
        };
        SkirtRig {
            skirt: [[0, 1], [2, 3], [4, 5], [6, 7]],
            legs: [leg(1, "shin_l", 0.055), leg(3, "shin_r", 0.055)],
            thighs: [8, 10],
        }
    }

    #[test]
    fn a_man_at_rest_keeps_his_skirts() {
        let id = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let palette: Palette = vec![id; 12];
        let transform = acr_render::view3d::model_matrix([300.0, 400.0, 0.0], 0.7, 1.0);
        let mut skirts = Skirts::default();
        let rig = rest_rig();
        let mut out = palette.clone();
        for _ in 0..120 {
            skirts.frame += 1;
            out = skirts.skirt(7, &transform, &rig, palette.clone(), 1.0 / 60.0, &Flat).unwrap();
        }
        // The hems stay within a few centimetres of the drape.
        for (c, &(_, x, y)) in COLUMNS.iter().enumerate() {
            let hem = gltf_of([x * ROWS[2].1, y * ROWS[2].2, ROWS[2].0]);
            let moved = apply_rows(&out[rig.skirt[c][1]], hem);
            let d = v3(sub(moved, hem)).length();
            assert!(d < 0.04, "column {c}: hem moved {d} m to {moved:?} from {hem:?}");
        }
    }

    #[test]
    fn affine_inverse_undoes_a_model_matrix() {
        let m: Mat4 = [[0.0, 30.0, 0.0, 0.0], [-30.0, 0.0, 0.0, 0.0], [0.0, 0.0, 30.0, 0.0], [500.0, 900.0, 12.0, 1.0]];
        let p = [0.3, -0.2, 1.1];
        let q = apply4(&affine_inverse(&m), apply4(&m, p));
        assert!(sub(p, q).iter().all(|x| x.abs() < 1e-4), "{q:?}");
    }
}

#[cfg(test)]
mod probe {
    #[test]
    #[ignore]
    fn probe_engm18() {
        let bytes = std::fs::read(std::env::var("PROBE_GLB").unwrap()).unwrap();
        let m = acr_render::model::ModelData::from_glb(&bytes).unwrap();
        let rig = super::SkirtRig::new(&m).unwrap();
        let rest = m.pose(None, 0.0);
        for c in 0..4 {
            for k in 0..2 {
                let s = rig.skirt[c][k];
                println!("skirt {c}{k} slot {s} rest rows {:?}", rest[s]);
            }
        }
        for (i, n) in m.nodes.iter().enumerate() {
            if n.name.starts_with("skirt") || n.name == "hips" || n.name == "root" || n.parent.is_none() {
                println!(
                    "node {i} {} parent {:?} t {:?} r {:?} s {:?}",
                    n.name, n.parent, n.translation, n.rotation, n.scale
                );
            }
        }
        let walk = m.clips.iter().position(|c| c.name.contains("MOTION") || c.name.contains("WALK")).unwrap_or(0);
        println!("clip {}", m.clips[walk].name);
        let mut sk = super::Skirts::default();
        for f in 0..90 {
            let x = 300.0 + 40.0 * f as f32 / 60.0;
            let transform = acr_render::view3d::model_matrix([x, 400.0, 0.0], std::f32::consts::FRAC_PI_2, 1.0);
            let p = m.pose(Some(walk), (f as f32 / 60.0) % 0.5);
            sk.frame += 1;
            let out = sk.skirt(1, &transform, &rig, p.clone(), 1.0 / 60.0, &super::tests::Flat).unwrap();
            if f % 15 == 14 {
                for c in 0..4 {
                    let (_, x, y) = super::COLUMNS[c];
                    let hem = super::gltf_of([x * 1.16, y * 1.2, 0.55]);
                    let a = super::apply_rows(&p[rig.skirt[c][1]], hem);
                    let b = super::apply_rows(&out[rig.skirt[c][1]], hem);
                    println!(
                        "f {f} col {c} clip hem {:?} cloth hem {:?} particle {:?}",
                        a,
                        b,
                        sk.live[&1].cloth.pos[c * 3 + 2]
                    );
                }
            }
        }
    }
}
