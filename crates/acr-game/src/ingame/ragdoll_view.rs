//! Physics ragdolls for soldiers hit near the camera (#276, phase 1 #278,
//! `docs/body-sim.md`). Render side only: the simulation decides who is hit and who dies;
//! this shows how the body moves.
//!
//! - A hit ([`acr_sim::combat::HitRecord`]) on a drawn skeleton-H man (not mounted) takes
//!   his current animated pose and velocity into a ragdoll and applies the blow's impulse
//!   at the struck bone. The struck region goes limp.
//! - A survivor's hips follow the animation on a spring (stagger, flinch, knees buckling);
//!   the muscles pull him back to the animation and he is handed back to the clip after
//!   0.5–1.5 s.
//! - A man killed falls: the tone runs out over 0.2–0.6 s with a last reflex (the hands
//!   towards the wound), he settles, sleeps, and his last pose becomes the corpse's pose in
//!   place of `DEATHLIE1`.
//! - Budget: at most `budget` awake ragdolls, from a calibration at startup (the cost of a
//!   ragdoll step against [`FRAME_BUDGET_US`]); nearest to the camera first. Above it the
//!   clips play as before. The budget follows the measured cost (m2-perf): in a dense
//!   pile a step costs about three times the calibration, and a slow frame runs several
//!   steps, so the awake ragdolls allowed are [`FRAME_BUDGET_US`] over the measured cost of
//!   one ragdoll's steps per frame (never above the calibration, never below
//!   [`MIN_AWAKE`]).
//!
//! Axes: physics is right-handed, metres, `x` east, `y` north, `z` up (`world / 30` with `y`
//! negated); the skeleton space of skeleton H turned by the facing is the physics space
//! (see [`physics_of`]); glTF models are `+y` up facing `+z`.

use std::collections::HashMap;

use acr_physics::{
    BoneDesc, BoneKind, Config, Quat, Ragdoll, Region, RigBone, Tone, Vec3, World as Physics, skeleton_h,
};
use acr_render::math::{self, Mat4};
use acr_render::model::{ModelData, Palette, Slot};
use acr_render::terrain::TerrainData;
use acr_render::view3d::{self, PIXELS_PER_METRE};
use acr_sim::body::Cause;
use acr_sim::combat::{DeathCause, HitRecord};
use acr_sim::world::Unit;

/// Frame time given to body physics (µs): the budget of awake ragdolls is this over the
/// measured cost of one ragdoll step.
pub const FRAME_BUDGET_US: f64 = 4000.0;
/// Awake ragdolls always allowed, however slow the steps.
pub const MIN_AWAKE: usize = 12;
/// Ragdolls kept in the physics world: the awake ones plus frozen corpses that still block
/// falling bodies. Beyond it the oldest corpse keeps its pose but leaves the physics.
const POOL: usize = 96;
/// Physics step (s).
const STEP: f32 = 1.0 / 60.0;
/// Hip sway per m/s of the blow's momentum over the body's mass.
const SWAY_GAIN: f32 = 6.0;
/// Impulse (N·s) that knocks a survivor down (a close grapeshot, a glancing cannonball).
const KNOCK_DOWN: f32 = 19.0;
/// Knocked down: falling and lying (s), then getting up (s).
const FALL_S: f32 = 2.2;
const GETUP_S: f32 = 1.5;
/// Centre of mass beyond the support between the feet (m) that makes him step.
const STEP_AT: f32 = 0.12;

/// [`Ragdolls::pool`] key of skeleton-H ragdolls.
const HUMAN: usize = usize::MAX;
/// Impulse (N·s) of an animal's blow that throws a man down: a bear's swipe, a bison's
/// horns.
const MAUL_IMPULSE: [f32; 3] = [28.0, 22.0, 60.0];

/// The bones of skeleton H in the order of [`skeleton_h`].
fn bone_names() -> Vec<&'static str> {
    skeleton_h().iter().map(|b| b.name).collect()
}

/// How a model's palette maps onto the ragdoll: the bone carrying each slot, the bind
/// palette, and each bone's own joint slot.
pub(super) struct Mapping {
    slot_bone: Vec<usize>,
    bind: Vec<Mat4>,
    bone_slot: Vec<usize>,
    /// An animal's own ragdoll (bones, kinds, total mass, kg); `None` for skeleton H.
    generic: Option<Generic>,
}

/// The ragdoll of an animal rig ([`acr_physics::skeleton_generic`]).
pub(super) struct Generic {
    desc: Vec<BoneDesc>,
    kinds: Vec<BoneKind>,
    mass: f32,
}

/// Joints of an animal rig that are details, not body bones (they ride on their nearest
/// body-bone ancestor): digits, claws, tongue, eyes, ears, whiskers and helpers.
const DETAIL_BONES: [&str; 22] = [
    "digit",
    "toe",
    "finger",
    "index",
    "middle",
    "pinky",
    "ring",
    "thumb",
    "claw",
    "tongue",
    "eye",
    "lid",
    "ear",
    "whisker",
    "nose",
    "lip",
    "_end",
    "target",
    "reference",
    "root",
    "blade",
    "teeth",
];

/// A model that gets an animal ragdoll: no skeleton-H rig, an `Anm*` type (riders and
/// horses keep their clips), skinned.
pub(super) fn animal_model(model: &ModelData) -> bool {
    model.rig.is_none()
        && !model.machine
        && model.type_name.as_deref().is_some_and(|t| t.len() > 3 && t[..3].eq_ignore_ascii_case("anm"))
        && model.slots.iter().any(|s| matches!(s, Slot::Joint { .. }))
}

/// The share of an animal's measured radius that is fur (by its `.MD` name): the bison
/// 0.22 (879 kg instead of 1536), the elk 0.1 (386 kg), the bear 0.08 (367 kg); the
/// jaguar's and the alligator's skins are tight.
fn fur_share(type_name: &str) -> f32 {
    let n = type_name.to_ascii_lowercase();
    if n.starts_with("anmbiz") {
        0.22
    } else if n.starts_with("anmoln") {
        0.1
    } else if n.starts_with("anmmed") {
        0.08
    } else {
        0.0
    }
}

fn rows_to_mat(rows: &[[f32; 4]; 3]) -> Mat4 {
    std::array::from_fn(|c| [rows[0][c], rows[1][c], rows[2][c], if c == 3 { 1.0 } else { 0.0 }])
}

impl Mapping {
    /// `None` unless every body bone of skeleton H is a joint of the model (riders' bones
    /// are prefixed: mounted models are left out).
    fn new(model: &ModelData) -> Option<Mapping> {
        Mapping::with_prefix(model, "")
    }

    /// The mapping of the skeleton-H bones named `prefix` + bone (a rider's `rider.`; his
    /// horse's bones ride on the rider's hips, which the strike layer never moves).
    pub(super) fn with_prefix(model: &ModelData, prefix: &str) -> Option<Mapping> {
        let names = bone_names();
        let node_bone =
            |n: usize| names.iter().position(|b| model.nodes[n].name.strip_prefix(prefix).is_some_and(|m| m == *b));
        let mut bone_slot = vec![usize::MAX; names.len()];
        let mut slot_bone = Vec::with_capacity(model.slots.len());
        for (s, slot) in model.slots.iter().enumerate() {
            let node = match *slot {
                Slot::Joint { node, .. } => node,
                Slot::Node(n) => n,
            };
            if let (Slot::Joint { .. }, Some(b)) = (slot, node_bone(node)) {
                bone_slot[b] = s;
            }
            // The nearest ancestor that is a body bone carries it (fingers the hand, sockets
            // their bone); the root and anything above the hips go with the hips.
            let mut n = Some(node);
            let mut bone = 0;
            while let Some(i) = n {
                if let Some(b) = node_bone(i) {
                    bone = b;
                    break;
                }
                n = model.nodes[i].parent;
            }
            slot_bone.push(bone);
        }
        if bone_slot.contains(&usize::MAX) {
            return if animal_model(model) { Mapping::generic(model) } else { None };
        }
        let bind = model.pose(None, 0.0).iter().map(rows_to_mat).collect();
        Some(Mapping { slot_bone, bind, bone_slot, generic: None })
    }

    /// A skeleton-H man (the gait, the reflexes and the knock-down apply).
    pub(super) fn human(&self) -> bool {
        self.generic.is_none()
    }

    /// The mapping of an animal rig: its body bones (the skin's joints without the details),
    /// their rest segments from the inverse bind matrices, and the flesh radius around each
    /// from the skinned vertices.
    fn generic(model: &ModelData) -> Option<Mapping> {
        let joints: Vec<(usize, usize, Mat4)> = model
            .slots
            .iter()
            .enumerate()
            .filter_map(|(s, slot)| match *slot {
                Slot::Joint { node, inverse_bind } => Some((s, node, inverse_bind)),
                Slot::Node(_) => None,
            })
            .collect();
        let detail = |n: usize| {
            let name = model.nodes[n].name.to_ascii_lowercase();
            DETAIL_BONES.iter().any(|d| name.contains(d)) && !name.contains("forearm")
        };
        let joint_of = |n: usize| joints.iter().find(|j| j.1 == n);
        // The nearest kept ancestor of a node.
        let kept_above = |mut n: usize| {
            while let Some(p) = model.nodes[n].parent {
                if joint_of(p).is_some() && !detail(p) {
                    return Some(p);
                }
                n = p;
            }
            None
        };
        let below = |n: usize, r: usize| {
            let mut a = Some(n);
            while let Some(x) = a {
                if x == r {
                    return true;
                }
                a = model.nodes[x].parent;
            }
            false
        };
        let mut kept: Vec<usize> = joints.iter().map(|j| j.1).filter(|&n| !detail(n)).collect();
        let depth = |mut n: usize| {
            let mut d = 0;
            while let Some(p) = model.nodes[n].parent {
                d += 1;
                n = p;
            }
            d
        };
        // One root: the top bone with the most bones under it.
        let roots: Vec<usize> = kept.iter().copied().filter(|&n| kept_above(n).is_none()).collect();
        let root = *roots.iter().max_by_key(|&&r| kept.iter().filter(|&&n| below(n, r)).count())?;
        kept.retain(|&n| below(n, root));
        kept.sort_by_key(|&n| (depth(n), n));
        if kept.len() < 3 || kept.len() > 64 {
            return None;
        }
        // Rest frames (skeleton space) of all joints.
        let bind_of = |n: usize| joint_of(n).map(|j| math::mul(&GLTF_TO_SKELETON, &affine_inverse(&j.2)));
        let head = |n: usize| bind_of(n).map_or([0.0; 3], |m| [m[3][0], m[3][1], m[3][2]]);
        // Names live as long as the game (one leak per animal model).
        let names: Vec<&'static str> =
            kept.iter().map(|&n| &*Box::leak(model.nodes[n].name.clone().into_boxed_str())).collect();
        let index = |n: usize| kept.iter().position(|&k| k == n);
        let mut rig: Vec<RigBone> = kept
            .iter()
            .enumerate()
            .map(|(i, &n)| RigBone {
                name: names[i],
                parent: kept_above(n).and_then(index),
                head: head(n),
                tail: head(n),
                radius: 0.0,
            })
            .collect();
        let kinds = acr_physics::bone_kinds(&rig);
        let trunk = |k: BoneKind| matches!(k, BoneKind::Root | BoneKind::Spine);
        let limb = |k: BoneKind| matches!(k, BoneKind::Girdle | BoneKind::Upper | BoneKind::Middle | BoneKind::Foot);
        // Tails: the child that continues the chain, else along the joint's own axis as far
        // as the details under it reach.
        let mut lens = vec![0.0f32; rig.len()];
        for i in 0..rig.len() {
            let children: Vec<usize> = (0..rig.len()).filter(|&c| rig[c].parent == Some(i)).collect();
            let next = match kinds[i] {
                k if trunk(k) => children
                    .iter()
                    .find(|&&c| kinds[c] == BoneKind::Spine)
                    .or_else(|| children.iter().find(|&&c| kinds[c] == BoneKind::Neck)),
                BoneKind::Neck => children.iter().find(|&&c| matches!(kinds[c], BoneKind::Neck | BoneKind::Head)),
                BoneKind::Tail => children.iter().find(|&&c| kinds[c] == BoneKind::Tail),
                k if limb(k) => children.iter().find(|&&c| limb(kinds[c])),
                _ => None,
            };
            let h = Vec3::from_array(rig[i].head);
            let tail = match next {
                Some(&c) if (Vec3::from_array(rig[c].head) - h).length() > 0.02 => rig[c].head,
                _ => {
                    let m = bind_of(kept[i]).unwrap_or(math::IDENTITY);
                    let axis = Vec3::new(m[1][0], m[1][1], m[1][2]).normalize_or_zero();
                    let reach = joints
                        .iter()
                        .filter(|j| j.1 != kept[i] && below(j.1, kept[i]))
                        .map(|j| (Vec3::from_array(head(j.1)) - h).dot(axis))
                        .fold(0.0f32, f32::max);
                    let parent_len = rig[i].parent.map_or(0.1, |p| lens[p]);
                    let len = if reach > 0.03 { reach } else { (0.6 * parent_len).max(0.05) };
                    (h + axis * len).to_array()
                }
            };
            lens[i] = (Vec3::from_array(tail) - h).length();
            rig[i].tail = tail;
        }
        // Palette slots: each joint rides on its nearest kept ancestor (or itself).
        let mut bone_slot = vec![usize::MAX; rig.len()];
        let mut slot_bone = Vec::with_capacity(model.slots.len());
        for (s, slot) in model.slots.iter().enumerate() {
            let node = match *slot {
                Slot::Joint { node, .. } => node,
                Slot::Node(n) => n,
            };
            if let (Slot::Joint { .. }, Some(b)) = (slot, index(node)) {
                bone_slot[b] = s;
            }
            let mut n = Some(node);
            let mut bone = 0;
            while let Some(i) = n {
                if let Some(b) = index(i) {
                    bone = b;
                    break;
                }
                n = model.nodes[i].parent;
            }
            slot_bone.push(bone);
        }
        // Flesh radius: the median distance of the bone's vertices (by their heaviest joint)
        // from its segment, a little inside the skin.
        let mut dists: Vec<Vec<f32>> = vec![Vec::new(); rig.len()];
        for p in &model.primitives {
            for v in p.vertices.iter().step_by(3) {
                let k = (0..4).max_by(|&a, &b| v.weights[a].total_cmp(&v.weights[b])).unwrap_or(0);
                if v.weights[k] <= 0.0 {
                    continue;
                }
                let Some(&b) = slot_bone.get(v.joints[k] as usize) else { continue };
                let q = Vec3::new(v.position[0], -v.position[2], v.position[1]);
                let (a, t) = (Vec3::from_array(rig[b].head), Vec3::from_array(rig[b].tail));
                let ab = t - a;
                let u = ((q - a).dot(ab) / ab.dot(ab).max(1e-8)).clamp(0.0, 1.0);
                dists[b].push((q - (a + ab * u)).length());
            }
        }
        // Shaggy coats (the bison's mane and hump, an elk's or a bear's coat) are skin in the
        // mesh: their share of the measured radius is not flesh.
        let flesh = 1.0 - fur_share(model.type_name.as_deref().unwrap_or(""));
        for (b, d) in dists.iter_mut().enumerate() {
            if d.len() >= 8 {
                d.sort_by(f32::total_cmp);
                rig[b].radius = 0.85 * flesh * d[d.len() / 2];
            }
        }
        if bone_slot.contains(&usize::MAX) {
            return None;
        }
        let (desc, mass) = acr_physics::skeleton_generic(&rig);
        let bind = model.pose(None, 0.0).iter().map(rows_to_mat).collect();
        log::info!(
            "3d: animal ragdoll {}: {} bones, {mass:.0} kg",
            model.type_name.as_deref().unwrap_or("?"),
            desc.len()
        );
        Some(Mapping { slot_bone, bind, bone_slot, generic: Some(Generic { desc, kinds, mass }) })
    }
}

/// glTF model space → skeleton (Blender) space: `(x, y, z) → (x, −z, y)`.
const GLTF_TO_SKELETON: Mat4 =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, -1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
/// Skeleton space → glTF.
const SKELETON_TO_GLTF: Mat4 =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Physics → world pixels.
fn world_of_physics() -> Mat4 {
    let k = PIXELS_PER_METRE;
    [[k, 0.0, 0.0, 0.0], [0.0, -k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// World pixels → physics.
fn physics_of_world() -> Mat4 {
    let k = 1.0 / PIXELS_PER_METRE;
    [[k, 0.0, 0.0, 0.0], [0.0, -k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// A world point (pixels) in physics.
pub fn physics_of(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0] / PIXELS_PER_METRE, -p[1] / PIXELS_PER_METRE, p[2] / PIXELS_PER_METRE)
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
    // inv is row-major here: inv[r][c].
    let t = [m[3][0], m[3][1], m[3][2]];
    let it: [f32; 3] = std::array::from_fn(|r| -(inv[r][0] * t[0] + inv[r][1] * t[1] + inv[r][2] * t[2]));
    [
        [inv[0][0], inv[1][0], inv[2][0], 0.0],
        [inv[0][1], inv[1][1], inv[2][1], 0.0],
        [inv[0][2], inv[1][2], inv[2][2], 0.0],
        [it[0], it[1], it[2], 1.0],
    ]
}

/// The skinning matrices (skeleton space → physics) of the body bones in an animated pose.
pub(super) fn animated_skins(map: &Mapping, palette: &Palette, transform: &Mat4) -> Vec<[[f32; 4]; 4]> {
    let to_physics = math::mul(&physics_of_world(), transform);
    map.bone_slot
        .iter()
        .map(|&s| math::mul(&to_physics, &math::mul(&rows_to_mat(&palette[s]), &SKELETON_TO_GLTF)))
        .collect()
}

/// What a survivor does after the blow.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Reaction {
    /// Arms and head: a short jerk.
    Flinch,
    /// Body hits: thrown off balance, a step's worth of sway.
    Stagger,
    /// Legs: the knees give, he sinks and comes up again.
    Buckle,
}

struct Actor {
    slot: usize,
    model: usize,
    /// Physics time of the (first) hit.
    since: f32,
    dying: bool,
    /// Physics time of the death.
    died: f32,
    /// Tone fall-off of a death (s).
    fade: f32,
    reaction: Reaction,
    /// How long until a survivor is handed back to the clip (s).
    recover: f32,
    region: Region,
    /// The struck bone, for the death reflex.
    struck: usize,
    /// The hips' offset from the animation (survivors): position and velocity (m, m/s).
    sway: Vec3,
    sway_v: Vec3,
    sink: f32,
    /// Knocked down (physics time): he crumples, lies, then gets up through a blend.
    down: Option<f32>,
    /// The hips' pose where he lay, the start of the getting up.
    fallen: Option<[[f32; 4]; 4]>,
    /// A recovery step: the foot (bone), from, to, start time.
    step: Option<(usize, Vec3, Vec3, f32)>,
    /// Skinning matrices once he lies still (a corpse no longer needs the physics).
    frozen: Option<Vec<[[f32; 4]; 4]>>,
    /// Physics time the corpse began to relax before it is frozen ([`RELAX_S`]).
    relaxed: Option<f32>,
    /// A skeleton-H man; else an animal (no reflexes, steps or knock-downs).
    human: bool,
    /// The body's mass (kg), for the sway of a blow.
    mass: f32,
}

/// How long a corpse relaxes before it is frozen (s), and the tone pulling it towards the
/// rest pose meanwhile.
const RELAX_S: f32 = 0.7;
const RELAX_TONE: f32 = 0.06;

/// The body physics of the 3D view.
pub struct Ragdolls {
    physics: Physics,
    /// Ragdolls, their user, and the rig they were built for (`HUMAN` or an animal's model).
    pool: Vec<(Ragdoll, Option<u32>, usize)>,
    actors: HashMap<u32, Actor>,
    maps: HashMap<usize, Option<Mapping>>,
    names: Vec<&'static str>,
    rig: acr_physics::gait::Rig,
    /// Awake ragdolls allowed (from the calibration).
    budget: usize,
    /// Measured: µs per awake ragdoll and step, and steps per frame (running means).
    step_cost_us: f64,
    steps_per_frame: f64,
    time: f32,
    stepped: f32,
    last_tick: u64,
    since_tick: f32,
    /// Ground points by unit at the last two ticks, for velocities.
    tracks: HashMap<u32, (u64, [f32; 3], [f32; 3])>,
    /// The newest hit tick taken (a step records its hits under the tick it began with).
    hit_seen: Option<u64>,
    /// This frame's palettes by instance index.
    pub poses: HashMap<usize, Palette>,
    /// Walking and crawling over the clips (#279).
    gaits: super::gait_view::Gaits,
    strikes: super::strike_view::Strikes,
}

impl Default for Ragdolls {
    fn default() -> Self {
        let (cost_us, budget) = calibrate();
        log::info!(
            "3d: ragdoll budget {budget} awake ({cost_us:.1} µs per ragdoll step, {FRAME_BUDGET_US} µs per frame)"
        );
        Ragdolls {
            physics: Physics::new(Config::default()),
            pool: Vec::new(),
            actors: HashMap::new(),
            maps: HashMap::new(),
            names: bone_names(),
            rig: acr_physics::gait::Rig::new(&skeleton_h()),
            budget,
            step_cost_us: cost_us,
            steps_per_frame: 1.0,
            time: 0.0,
            stepped: 0.0,
            last_tick: 0,
            since_tick: 0.0,
            tracks: HashMap::new(),
            hit_seen: None,
            poses: HashMap::new(),
            gaits: Default::default(),
            strikes: Default::default(),
        }
    }
}

/// Cost of a ragdoll step on this machine: 8 limp ragdolls falling on flat ground, 30
/// steps, best of three; and the budget of awake ragdolls it gives.
pub fn calibrate() -> (f64, usize) {
    let skeleton = skeleton_h();
    let mut best = f64::MAX;
    for _ in 0..3 {
        let mut world = Physics::new(Config { sleep_energy: 0.0, ..Config::default() });
        for i in 0..8 {
            let tilt = Quat::from_axis_angle(Vec3::new(1.0, 0.3 * i as f32, 0.0), 0.5);
            Ragdoll::spawn(&mut world, &skeleton, 75.0, Vec3::new(3.0 * i as f32, 0.0, 0.3), tilt);
        }
        let start = web_time::Instant::now();
        for _ in 0..30 {
            world.step(&|_: f32, _: f32| 0.0);
        }
        best = best.min(start.elapsed().as_secs_f64() * 1e6 / (30.0 * 8.0));
    }
    (best, (FRAME_BUDGET_US / best.max(1.0)).floor().min(POOL as f64) as usize)
}

/// Impulse (N·s) of a blow by cause and energy (percent): a musket ball's momentum (30 g at
/// about 300 m/s) when it stops in the body, a share of a cannonball's, the swing of a
/// weapon.
fn impulse_of(cause: DeathCause, energy: u8) -> f32 {
    let e = f32::from(energy.max(20)) / 100.0;
    match cause {
        DeathCause::Gunshot => 9.0 * (0.5 + 0.5 * e),
        DeathCause::Arrow => 4.0,
        DeathCause::Grapeshot => 15.0 * e + 5.0,
        DeathCause::Cannonball => 60.0 + 240.0 * e,
        DeathCause::Explosion => 40.0 + 120.0 * e,
        DeathCause::Melee => 18.0,
        DeathCause::Thrown => 12.0,
        _ => 6.0,
    }
}

/// The bone struck by a hit and its region.
fn struck_bone(hit: &HitRecord, names: &[&str]) -> (usize, Region) {
    use acr_sim::body::{LimbId, Part};
    use acr_sim::wounds::HitRegion;
    let at = |n: &str| names.iter().position(|b| *b == n).unwrap_or(0);
    let side = |l: LimbId| if matches!(l, LimbId::ArmL | LimbId::LegL) { "_l" } else { "_r" };
    match (hit.injury.map(|i| (i.part, i.level)), hit.region) {
        (Some((Part::Head, _)), _) => (at("head"), Region::Head),
        (Some((Part::Chest, _)), _) => (at("spine2"), Region::Torso),
        (Some((Part::Belly, _)), _) => (at("spine"), Region::Torso),
        (Some((Part::Pelvis, _)), _) => (at("hips"), Region::Torso),
        (Some((Part::Limb(l), level)), _) => {
            let (upper, lower) = if l.is_leg() { ("thigh", "shin") } else { ("upperarm", "forearm") };
            let bone = if level > 50 { lower } else { upper };
            let region = match l {
                LimbId::ArmL => Region::ArmL,
                LimbId::ArmR => Region::ArmR,
                LimbId::LegL => Region::LegL,
                LimbId::LegR => Region::LegR,
            };
            (at(&format!("{bone}{}", side(l))), region)
        }
        (None, Some(HitRegion::Head)) => (at("head"), Region::Head),
        (None, Some(HitRegion::Legs)) => (at("thigh_l"), Region::LegL),
        (None, Some(HitRegion::Arms)) => (at("upperarm_r"), Region::ArmR),
        _ => (at("spine1"), Region::Torso),
    }
}

/// The bone of an animal's ragdoll struck by a hit, and its region: the part's bone by the
/// rig's kinds (head, the chest end of the spine, its middle, the root; a leg by its level).
fn struck_generic(hit: &HitRecord, g: &Generic) -> (usize, Region) {
    use acr_sim::body::{LimbId, Part};
    use acr_sim::wounds::HitRegion;
    let n = g.desc.len();
    let spine: Vec<usize> = (0..n).filter(|&b| matches!(g.kinds[b], BoneKind::Root | BoneKind::Spine)).collect();
    let head = (0..n)
        .find(|&b| g.kinds[b] == BoneKind::Head)
        .or_else(|| (0..n).rev().find(|&b| g.kinds[b] == BoneKind::Neck))
        .unwrap_or(0);
    let mid = spine.get(spine.len() / 2).copied().unwrap_or(0);
    let chest = spine.last().copied().unwrap_or(0);
    let limb = |region: Region, level: u8| {
        let bones: Vec<usize> =
            (0..n).filter(|&b| g.desc[b].region == region && g.kinds[b] != BoneKind::Girdle).collect();
        let at = bones.get(usize::from(level) * bones.len() / 256).copied();
        (at.unwrap_or(mid), if at.is_some() { region } else { Region::Torso })
    };
    match (hit.injury.map(|i| (i.part, i.level)), hit.region) {
        (Some((Part::Head, _)), _) | (None, Some(HitRegion::Head)) => (head, Region::Head),
        (Some((Part::Chest, _)), _) => (chest, Region::Torso),
        (Some((Part::Belly, _)), _) => (mid, Region::Torso),
        (Some((Part::Pelvis, _)), _) => (0, Region::Torso),
        (Some((Part::Limb(l), level)), _) => limb(
            match l {
                LimbId::ArmL => Region::ArmL,
                LimbId::ArmR => Region::ArmR,
                LimbId::LegL => Region::LegL,
                LimbId::LegR => Region::LegR,
            },
            level,
        ),
        (None, Some(HitRegion::Legs)) => limb(Region::LegL, 60),
        (None, Some(HitRegion::Arms)) => limb(Region::ArmR, 60),
        _ => (mid, Region::Torso),
    }
}

/// A unit's instance this frame, as [`Ragdolls::update`] needs it.
#[derive(Clone, Copy)]
pub struct Drawn<'a> {
    pub unit: &'a Unit,
    pub index: usize,
    pub model: usize,
    pub data: &'a ModelData,
    pub clip: Option<usize>,
    pub time: f32,
    pub transform: Mat4,
}

impl Ragdolls {
    pub fn clear(&mut self) {
        for (r, user, _) in &mut self.pool {
            *user = None;
            r.freeze(&mut self.physics);
        }
        self.actors.clear();
        self.tracks.clear();
    }

    fn mapping(&mut self, model: usize, data: &ModelData) -> Option<&Mapping> {
        self.maps
            .entry(model)
            .or_insert_with(|| {
                let m = Mapping::new(data);
                log::info!("3d: model {model} ragdoll mapping {}", if m.is_some() { "ok" } else { "missing bones" });
                m
            })
            .as_ref()
    }

    /// Folds this frame's physics time (`us` for `steps` steps of `awake` ragdolls) into the
    /// running means behind [`Self::allowed`].
    fn measure(&mut self, us: f64, steps: usize, awake: usize) {
        const K: f64 = 0.1;
        if us.is_finite() && steps > 0 && awake >= 4 {
            let per = us / (steps * awake) as f64;
            self.step_cost_us += K * (per - self.step_cost_us);
        }
        self.steps_per_frame += K * (steps as f64 - self.steps_per_frame);
    }

    /// Awake ragdolls allowed now: [`FRAME_BUDGET_US`] over the measured cost of one
    /// ragdoll's steps per frame, at most the calibration's and at least [`MIN_AWAKE`].
    fn allowed(&self) -> usize {
        let per_frame = self.step_cost_us.max(1.0) * self.steps_per_frame.max(1.0);
        ((FRAME_BUDGET_US / per_frame) as usize).clamp(MIN_AWAKE.min(self.budget), self.budget)
    }

    /// Awake ragdolls.
    fn awake(&self) -> usize {
        self.actors.values().filter(|a| a.frozen.is_none()).count()
    }

    /// A free ragdoll of the pool built for `model`'s rig, or the oldest frozen corpse's.
    fn take_slot(&mut self, unit: u32, model: usize) -> Option<usize> {
        let generic = self.maps.get(&model).and_then(Option::as_ref).and_then(|m| m.generic.as_ref());
        let key = if generic.is_some() { model } else { HUMAN };
        if let Some(i) = self.pool.iter().position(|(_, u, k)| u.is_none() && *k == key) {
            self.pool[i].1 = Some(unit);
            return Some(i);
        }
        if self.pool.len() < POOL {
            let far = Vec3::new(-1000.0 - 5.0 * self.pool.len() as f32, -1000.0, 0.0);
            let r = match generic {
                Some(g) => Ragdoll::spawn(&mut self.physics, &g.desc, g.mass, far, Quat::IDENTITY),
                None => Ragdoll::spawn(&mut self.physics, &skeleton_h(), 75.0, far, Quat::IDENTITY),
            };
            self.pool.push((r, Some(unit), key));
            return Some(self.pool.len() - 1);
        }
        // The oldest corpse of the same rig leaves the physics; it keeps its pose.
        let pool = &self.pool;
        let (&v, _) = self
            .actors
            .iter()
            .filter(|(_, a)| a.frozen.is_some() && a.slot != usize::MAX && pool[a.slot].2 == key)
            .min_by(|a, b| a.1.died.total_cmp(&b.1.died))?;
        let slot = self.actors.get(&v)?.slot;
        if let Some(a) = self.actors.get_mut(&v) {
            a.slot = usize::MAX;
        }
        self.pool[slot].1 = Some(unit);
        Some(slot)
    }

    /// Advances the body physics to the game time, takes this tick's hits on the drawn
    /// units, and fills [`Ragdolls::poses`] for their instances.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        now: u64,
        seconds: f32,
        hits: &[&HitRecord],
        drawn: &[Drawn<'_>],
        mounted: &[Drawn<'_>],
        focus: [f32; 2],
        terrain: &TerrainData,
        killed: &dyn Fn(u32) -> bool,
    ) {
        self.poses.clear();
        if now < self.last_tick {
            self.clear();
            self.hit_seen = None;
        }
        if now != self.last_tick {
            self.since_tick = 0.0;
            for d in drawn {
                let u = d.unit;
                let p = [d.transform[3][0], d.transform[3][1], d.transform[3][2]];
                let e = self.tracks.entry(u.id.0).or_insert((now, p, p));
                if e.0 != now {
                    *e = (now, e.2, p);
                }
            }
            self.last_tick = now;
        } else {
            self.since_tick = (self.since_tick + seconds).min(1.0 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32);
        }
        // The game's clock (paused with the game), smoothed within a tick.
        let target = now as f32 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32 + self.since_tick;
        if target < self.time || target - self.time > 1.0 {
            self.time = target;
            self.stepped = target;
        }
        self.time = target;

        let by_unit: HashMap<u32, usize> = drawn.iter().enumerate().map(|(i, d)| (d.unit.id.0, i)).collect();
        // New hits, nearest to the camera first.
        let mut fresh: Vec<(&HitRecord, f32)> = hits
            .iter()
            .filter(|h| self.hit_seen.is_none_or(|s| h.tick > s) && h.tick <= now && !h.building)
            .filter_map(|h| {
                let d = &drawn[*by_unit.get(&h.victim)?];
                let (dx, dy) = (d.transform[3][0] - focus[0], d.transform[3][1] - focus[1]);
                Some((*h, dx * dx + dy * dy))
            })
            .collect();
        fresh.sort_by(|a, b| a.1.total_cmp(&b.1));
        if let Some(t) = hits.iter().map(|h| h.tick).filter(|&t| t <= now).max() {
            self.hit_seen = Some(self.hit_seen.map_or(t, |s| s.max(t)));
        }
        for (hit, _) in fresh {
            let d = &drawn[by_unit[&hit.victim]];
            if d.unit.body.mounted || d.unit.building {
                continue;
            }
            let known = self.actors.contains_key(&hit.victim);
            if !known && self.awake() >= self.allowed() {
                continue;
            }
            if self.mapping(d.model, d.data).is_none() {
                continue;
            }
            log::info!("3d: ragdoll for unit {} (hit {:?})", hit.victim, hit.cause);
            self.take_hit(hit, d, now, killed(hit.victim), terrain);
        }
        // Deaths of the hit after their hit (bled out, a second blow elsewhere).
        let time = self.time;
        for (v, a) in &mut self.actors {
            if !a.dying && killed(*v) {
                a.dying = true;
                a.died = time;
            }
        }
        let scope = acr_sim::prof::Scope::new("rag.drive");
        self.drive(drawn, &by_unit, terrain);
        drop(scope);
        let scope = acr_sim::prof::Scope::new("rag.physics");
        // Fixed steps up to the game time.
        let ground = |x: f32, y: f32| ground_height(terrain, x, y);
        let mut n = 0;
        let awake = self.awake();
        let started = web_time::Instant::now();
        while self.stepped + STEP <= self.time && n < 4 {
            self.physics.step(&ground);
            self.stepped += STEP;
            n += 1;
        }
        if n == 4 {
            self.stepped = self.time;
        }
        self.measure(started.elapsed().as_secs_f64() * 1e6, n, awake);
        acr_sim::prof::add("rag.steps(count)", n as u64 * 1_000_000);
        acr_sim::prof::add("rag.awake(count)", self.awake() as u64 * 1_000_000);
        drop(scope);
        let scope = acr_sim::prof::Scope::new("rag.settle_palettes");
        self.settle(&by_unit);
        self.palettes(drawn, &by_unit);
        drop(scope);
        if std::env::var_os("ACR_NO_GAIT").is_none() {
            for d in drawn {
                if d.data.clips.is_empty() {
                    continue;
                }
                if !self.maps.contains_key(&d.model) {
                    self.mapping(d.model, d.data);
                }
            }
            acr_sim::prof_scope!("rag.gaits");
            self.gaits.update(seconds, drawn, &self.maps, focus, terrain, &mut self.poses);
        }
        // Attackers bend their weapon onto their victims (`docs/melee.md`, Animation).
        if std::env::var_os("ACR_NO_STRIKE").is_none() {
            for d in drawn {
                if !d.data.clips.is_empty() && !self.maps.contains_key(&d.model) {
                    self.mapping(d.model, d.data);
                }
            }
            // Riders (bones prefixed `rider.`) take part in the strikes only.
            let all: Vec<Drawn<'_>> = drawn.iter().chain(mounted).copied().collect();
            acr_sim::prof_scope!("rag.strikes");
            self.strikes.update(seconds, &all, hits, &self.maps, focus, &mut self.poses);
        }
    }

    /// Where the bone `name` of `unit`'s ragdoll is (world pixels), while it has one (an
    /// animal mauling him stands over it, `maul_view.rs`).
    pub fn bone_world(&self, unit: u32, name: &str) -> Option<[f32; 3]> {
        let a = self.actors.get(&unit).filter(|a| a.slot != usize::MAX)?;
        let r = &self.pool.get(a.slot)?.0;
        let (p, _) = r.bone_transform(&self.physics, r.bone_index(name)?);
        Some([p[0] * PIXELS_PER_METRE, -p[1] * PIXELS_PER_METRE, p[2] * PIXELS_PER_METRE])
    }

    fn take_hit(&mut self, hit: &HitRecord, d: &Drawn<'_>, now: u64, dies: bool, terrain: &TerrainData) {
        let v = hit.victim;
        let generic = self.maps.get(&d.model).and_then(Option::as_ref).and_then(|m| m.generic.as_ref());
        let (human, mass) = (generic.is_none(), generic.map_or(75.0, |g| g.mass));
        let (bone, region) = match generic {
            Some(g) => struck_generic(hit, g),
            None => struck_bone(hit, &self.names),
        };
        let new = !self.actors.contains_key(&v) || self.actors[&v].slot == usize::MAX;
        if new {
            let Some(slot) = self.take_slot(v, d.model) else { return };
            let Some(map) = self.maps.get(&d.model).and_then(Option::as_ref) else { return };
            let skins = animated_skins(map, &d.data.pose(d.clip, d.time), &d.transform);
            let vel = match self.tracks.get(&v) {
                Some(&(t, a, b)) if t + 1 >= now => {
                    let w = physics_of(b) - physics_of(a);
                    w * acr_sim::ORIGINAL_TICKS_PER_SECOND as f32
                }
                _ => Vec3::ZERO,
            };
            let r = &self.pool[slot].0;
            r.set_pinned_root(&mut self.physics, false);
            for (b, s) in skins.iter().enumerate() {
                r.place(&mut self.physics, b, s, Vec3::new(vel.x, vel.y, 0.0));
            }
            // Out of the ground first: a lying or crawling pose sinks into it a little, and
            // the contacts would throw him up.
            let mut lift = 0.0f32;
            for b in 0..r.len() {
                let body = self.physics.body(r.body(b));
                let (e0, e1) = body.ends();
                for e in [e0, e1] {
                    lift = lift.max(ground_height(terrain, e.x, e.y) + body.radius - e.z);
                }
            }
            if lift > 0.0 {
                for b in 0..r.len() {
                    let id = r.body(b);
                    let body = self.physics.body(id);
                    let (p, q, v) = (body.pos + Vec3::new(0.0, 0.0, lift), body.rot, body.vel);
                    self.physics.set_body_state(id, p, q, v, Vec3::ZERO);
                }
            }
            r.target_current_pose(&mut self.physics);
            r.set_tone_all(&mut self.physics, Tone::FULL);
            let energy = hit.injury.map_or(60, |i| i.energy);
            let reaction = match region {
                _ if !human => Reaction::Stagger,
                Region::LegL | Region::LegR => Reaction::Buckle,
                Region::Torso => Reaction::Stagger,
                _ => Reaction::Flinch,
            };
            let h = (v.wrapping_mul(2_654_435_761) >> 16) as f32 / 65536.0;
            self.actors.insert(
                v,
                Actor {
                    slot,
                    model: d.model,
                    since: self.time,
                    dying: dies,
                    died: self.time,
                    fade: 0.2 + 0.4 * h,
                    reaction,
                    recover: 0.5 + f32::from(energy) / 100.0,
                    region,
                    struck: bone,
                    sway: Vec3::ZERO,
                    sway_v: Vec3::ZERO,
                    sink: 0.0,
                    down: None,
                    fallen: None,
                    step: None,
                    frozen: None,
                    relaxed: None,
                    human,
                    mass,
                },
            );
        }
        let Some(a) = self.actors.get_mut(&v) else { return };
        if dies && !a.dying {
            a.dying = true;
            a.died = self.time;
        }
        a.frozen = None;
        a.region = region;
        a.struck = bone;
        a.since = self.time;
        let r = &self.pool[a.slot].0;
        r.set_tone_region(&mut self.physics, region, Tone::FULL.scaled(0.05));
        // The blow: at the struck bone's surface facing the attacker, along the shot.
        let dir = Vec3::new(hit.dir.0 as f32, -(hit.dir.1 as f32), 0.0).normalize_or_zero();
        let dir = if dir == Vec3::ZERO { Vec3::new(0.0, -1.0, 0.0) } else { dir };
        let body = r.body(bone);
        let b = self.physics.body(body);
        let point = b.pos - dir * (b.radius * 0.8);
        let energy = hit.injury.map_or(60, |i| i.energy);
        // An animal's blow (#276 phase 4): a bear's swipe or a bison's horns throw him down,
        // away from the animal.
        let maul = match hit.injury.map(|i| i.cause) {
            Some(Cause::Claw) => Some(0),
            Some(Cause::Bite) => Some(1),
            Some(Cause::Gore) => Some(2),
            _ => None,
        };
        let j = match maul {
            Some(k) => MAUL_IMPULSE[k] * (0.5 + 0.5 * f32::from(energy) / 100.0),
            None => impulse_of(hit.cause, energy),
        };
        // Thrown down and a little up for a goring.
        let lift = if maul == Some(2) { Vec3::new(0.0, 0.0, 0.35) } else { Vec3::ZERO };
        self.physics.apply_impulse(body, point, (dir + lift) * j);
        // The survivor's hips sway with the blow (the total momentum over his mass).
        a.sway_v += dir * (j / a.mass) * SWAY_GAIN;
        // A hard blow knocks a survivor down: he gets up when the clip takes him back (and the
        // simulation lets him: `Body::down`).
        let thrown = j >= KNOCK_DOWN || d.unit.body.down > 0;
        if !a.dying && a.human && thrown && a.down.is_none() {
            a.down = Some(self.time);
            a.fallen = None;
            a.recover = a.recover.max(FALL_S + GETUP_S + 0.3);
        }
    }

    /// Survivors: hips on a spring around the animation, muscles towards the animation,
    /// tone coming back. The dying: tone running out with a last reflex.
    fn drive(&mut self, drawn: &[Drawn<'_>], by_unit: &HashMap<u32, usize>, terrain: &TerrainData) {
        let dt = (self.time - self.stepped).clamp(0.0, 0.1);
        let hips = 0;
        for (v, a) in &mut self.actors {
            if a.slot == usize::MAX || a.frozen.is_some() {
                continue;
            }
            let r = &self.pool[a.slot].0;
            let t = self.time - a.since;
            if a.dying {
                // Limp for good: leave him alone (so he can fall asleep).
                if self.time - a.died > a.fade + 0.1 {
                    continue;
                }
                let k = (1.0 - (self.time - a.died) / a.fade).clamp(0.0, 1.0);
                r.set_pinned_root(&mut self.physics, false);
                r.set_tone_all(&mut self.physics, Tone::FULL.scaled(0.6 * k));
                r.set_tone_region(&mut self.physics, a.region, Tone::FULL.scaled(0.05 * k));
                // The last reflex: arms drawn in towards the wound.
                if k > 0.0 && a.human {
                    for side in ["_l", "_r"] {
                        let bone = |n: &str| self.names.iter().position(|b| *b == format!("{n}{side}"));
                        if let Some(e) = bone("forearm") {
                            let axis = r.joint_axis(&self.physics, e);
                            r.set_target(&mut self.physics, e, Quat::from_axis_angle(axis, 1.9));
                        }
                        if let Some(sh) = bone("upperarm") {
                            let axis = r.joint_axis(&self.physics, sh);
                            r.set_target(&mut self.physics, sh, Quat::from_axis_angle(axis, 0.7));
                        }
                    }
                }
                continue;
            }
            let Some(&i) = by_unit.get(v) else { continue };
            let d = &drawn[i];
            let Some(map) = self.maps.get(&a.model).and_then(Option::as_ref) else { continue };
            let mut skins = animated_skins(map, &d.data.pose(d.clip, d.time), &d.transform);
            let back = (t / a.recover).clamp(0.0, 1.0);
            if !a.human {
                // An animal staggers on its legs: the struck region limp for a moment, the
                // root on a damped spring around the clip.
                r.set_tone_all(&mut self.physics, Tone::FULL);
                r.set_tone_region(&mut self.physics, a.region, Tone::FULL.scaled(0.05 + 0.95 * back * back));
                let (w, z) = (6.0, 0.45);
                let acc = -(a.sway * (w * w)) - a.sway_v * (2.0 * z * w);
                a.sway_v += acc * dt;
                a.sway += a.sway_v * dt;
                r.target_skins(&mut self.physics, &skins);
                let mut s = skins[hips];
                s[3][0] += a.sway.x;
                s[3][1] += a.sway.y;
                r.set_pinned_root(&mut self.physics, true);
                r.place(&mut self.physics, hips, &s, Vec3::ZERO);
                continue;
            }
            // Held down (mauled): he lies as long as the simulation keeps him down.
            if let Some(down) = a.down
                && d.unit.body.down > 0
                && self.time - down > FALL_S - 0.2
            {
                a.down = Some(self.time - (FALL_S - 0.2));
                a.recover = a.recover.max(t + FALL_S + GETUP_S + 0.3);
            }
            // Knocked down: the legs give, he lies a moment, then gets up through a blend of
            // the hips from where he lay to the clip.
            if let Some(down) = a.down {
                let td = self.time - down;
                r.target_skins(&mut self.physics, &skins);
                if td < FALL_S {
                    r.set_pinned_root(&mut self.physics, false);
                    // Held down by an animal he lies nearly limp instead of struggling up.
                    let tone = if d.unit.body.down > 0 { 0.12 } else { 0.3 };
                    r.set_tone_all(&mut self.physics, Tone::FULL.scaled(tone));
                    // The knees give: thighs and shins fold, he drops to his knees and
                    // over.
                    for leg in [Region::LegL, Region::LegR] {
                        r.set_tone_region(&mut self.physics, leg, Tone::FULL.scaled(0.25));
                    }
                    for (n, flex) in [("thigh_l", 1.3), ("thigh_r", 1.1), ("shin_l", 2.0), ("shin_r", 2.2)] {
                        let b = self.rig.bone(n);
                        let axis = r.joint_axis(&self.physics, b);
                        r.set_target(&mut self.physics, b, Quat::from_axis_angle(axis, flex));
                    }
                    continue;
                }
                let fallen = *a.fallen.get_or_insert_with(|| r.skin_matrix(&self.physics, hips));
                let k = ((td - FALL_S) / GETUP_S).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                r.set_tone_all(&mut self.physics, Tone::FULL.scaled(0.2 + 0.8 * k));
                let s: [[f32; 4]; 4] = std::array::from_fn(|c| {
                    std::array::from_fn(|i| fallen[c][i] + (skins[hips][c][i] - fallen[c][i]) * k)
                });
                r.set_pinned_root(&mut self.physics, true);
                r.place(&mut self.physics, hips, &s, Vec3::ZERO);
                continue;
            }
            r.set_tone_all(&mut self.physics, Tone::FULL);
            r.set_tone_region(&mut self.physics, a.region, Tone::FULL.scaled(0.05 + 0.95 * back * back));
            // Hips: a damped spring around the animation (about 1 Hz), the knees giving
            // way for a leg hit; the trunk leans with the sway (hip strategy).
            let (w, z) = (6.0, 0.45);
            let acc = -(a.sway * (w * w)) - a.sway_v * (2.0 * z * w);
            a.sway_v += acc * dt;
            a.sway += a.sway_v * dt;
            a.sink = match a.reaction {
                Reaction::Buckle => 0.25 * (std::f32::consts::PI * back).sin(),
                Reaction::Stagger => 0.05 * (std::f32::consts::PI * back).sin(),
                Reaction::Flinch => 0.0,
            };
            // Balance: the centre of mass against the support between the feet. Outside it, a
            // recovery step puts the other foot under him (IK), and he stays on it.
            let com = r.center_of_mass(&self.physics);
            let feet = [self.rig.bone("foot_l"), self.rig.bone("foot_r")];
            let fp = feet.map(|f| self.physics.body(r.body(f)).pos);
            let (seg, rel) = (fp[1] - fp[0], com - fp[0]);
            let u = (rel.x * seg.x + rel.y * seg.y) / (seg.x * seg.x + seg.y * seg.y).max(1e-6);
            let cp = fp[0] + seg * u.clamp(0.0, 1.0);
            let off = Vec3::new(com.x - cp.x, com.y - cp.y, 0.0);
            if a.step.is_none() && off.length() > STEP_AT {
                let land = Vec3::new(com.x, com.y, 0.0) + off.normalize_or_zero() * 0.12;
                let far = |f: usize| Vec3::new(fp[f].x - land.x, fp[f].y - land.y, 0.0).length();
                let f = if far(0) > far(1) { 0 } else { 1 };
                let from = self.rig.joint(&skins, feet[f]);
                a.step = Some((f, from, land, self.time));
            }
            if let Some((f, from, to, start)) = a.step {
                let k = ((self.time - start) / 0.3).clamp(0.0, 1.0);
                let mut p = from + (to - from) * k;
                p.z = ground_height(terrain, p.x, p.y) + 0.08 + 0.12 * (std::f32::consts::PI * k).sin();
                let chain = if f == 0 { ["thigh_l", "shin_l", "foot_l"] } else { ["thigh_r", "shin_r", "foot_r"] };
                let [th, sh, ft] = chain.map(|n| self.rig.bone(n));
                self.rig.two_bone_ik(&mut skins, th, sh, ft, p, Vec3::X);
            }
            r.target_skins(&mut self.physics, &skins);
            let mut s = skins[hips];
            s[3][0] += a.sway.x;
            s[3][1] += a.sway.y;
            s[3][2] -= a.sink;
            let lean = Quat::from_axis_angle(Vec3::new(-a.sway.y, a.sway.x, 0.0), a.sway.length() * 1.5);
            let rot = acr_physics::math::mat4_from_rt(Vec3::ZERO, lean);
            let s = math::mul(&rot_about(&rot, [s[3][0], s[3][1], s[3][2]]), &s);
            r.set_pinned_root(&mut self.physics, true);
            r.place(&mut self.physics, hips, &s, Vec3::ZERO);
        }
    }

    /// Survivors whose time is up go back to their clip; corpses lying still are frozen.
    fn settle(&mut self, by_unit: &HashMap<u32, usize>) {
        let time = self.time;
        let mut done = Vec::new();
        for (v, a) in &mut self.actors {
            if a.slot == usize::MAX {
                if !by_unit.contains_key(v) {
                    done.push(*v);
                }
                continue;
            }
            let r = &self.pool[a.slot].0;
            if !a.dying && time - a.since > a.recover + 0.2 {
                done.push(*v);
                continue;
            }
            if a.dying
                && a.frozen.is_none()
                && a.relaxed.is_none()
                && (r.is_asleep(&self.physics) || time - a.died > 8.0)
            {
                // Before it is frozen the corpse relaxes: a faint pull towards the rest pose
                // lowers a limb left propped up (an arm raised by the death reflex).
                a.relaxed = Some(time);
                r.set_pose(&mut self.physics, &vec![Quat::IDENTITY; r.len()]);
                r.set_tone_all(&mut self.physics, Tone::FULL.scaled(RELAX_TONE));
                for b in 0..r.len() {
                    self.physics.wake(r.body(b));
                }
            }
            if a.dying && a.frozen.is_none() && a.relaxed.is_some_and(|t| time - t > RELAX_S) {
                r.set_tone_all(&mut self.physics, Tone::LIMP);
                r.freeze(&mut self.physics);
                a.frozen = Some((0..r.len()).map(|b| r.skin_matrix(&self.physics, b)).collect());
            }
            // The corpse is gone from the game.
            if !by_unit.contains_key(v) && time - a.since > 1.0 {
                done.push(*v);
            }
        }
        for v in done {
            if let Some(a) = self.actors.remove(&v)
                && a.slot != usize::MAX
            {
                self.pool[a.slot].1 = None;
                let r = &self.pool[a.slot].0;
                r.set_pinned_root(&mut self.physics, false);
                r.freeze(&mut self.physics);
            }
        }
    }

    /// Palettes of the ragdolls' instances: `P = T⁻¹ · world · S · glTF→skeleton · bind`.
    fn palettes(&mut self, drawn: &[Drawn<'_>], by_unit: &HashMap<u32, usize>) {
        for (v, a) in &self.actors {
            let Some(&i) = by_unit.get(v) else { continue };
            let d = &drawn[i];
            let Some(map) = self.maps.get(&a.model).and_then(Option::as_ref) else { continue };
            let skins: Vec<[[f32; 4]; 4]> = match (&a.frozen, a.slot) {
                (Some(f), _) => f.clone(),
                (None, usize::MAX) => continue,
                (None, s) => {
                    let r = &self.pool[s].0;
                    (0..r.len()).map(|b| r.skin_matrix(&self.physics, b)).collect()
                }
            };
            self.poses.insert(d.index, palette_of(map, &skins, &d.transform));
        }
    }
}

/// The palette of an instance (transform `transform`) posed by skinning matrices per body
/// bone (physics): `P = T⁻¹ · world · S · glTF→skeleton · bind`.
pub(super) fn palette_of(map: &Mapping, skins: &[[[f32; 4]; 4]], transform: &Mat4) -> Palette {
    let lead = math::mul(&affine_inverse(transform), &world_of_physics());
    let per_bone: Vec<Mat4> = skins.iter().map(|s| math::mul(&lead, &math::mul(s, &GLTF_TO_SKELETON))).collect();
    map.slot_bone.iter().zip(&map.bind).map(|(&b, bind)| math::affine_rows(&math::mul(&per_bone[b], bind))).collect()
}

/// `palette` with each slot moved as its bone moved from `before` to `after` (skins of
/// [`animated_skins`]): unlike [`palette_of`], a slot keeps its own animation relative to its
/// bone (a socket's musket or sabre).
pub(super) fn repose(
    map: &Mapping,
    palette: &Palette,
    before: &[[[f32; 4]; 4]],
    after: &[[[f32; 4]; 4]],
    transform: &Mat4,
) -> Palette {
    let into = math::mul(&physics_of_world(), transform);
    let back = affine_inverse(&into);
    let delta: Vec<Mat4> = before
        .iter()
        .zip(after)
        .map(|(b, a)| math::mul(&back, &math::mul(&math::mul(a, &affine_inverse(b)), &into)))
        .collect();
    map.slot_bone
        .iter()
        .zip(palette)
        .map(|(&b, rows)| math::affine_rows(&math::mul(&delta[b], &rows_to_mat(rows))))
        .collect()
}

/// Where node `node` of the model lies in `globals` (the clip's node transforms), and its
/// local axis `axis` there, both in physics. The held weapons are built along their local
/// +Z in Blender, which the glTF export turns into the node's local +Y.
pub(super) fn node_frame(globals: &[Mat4], node: usize, axis: Vec3, transform: &Mat4) -> (Vec3, Vec3) {
    let m = math::mul(&math::mul(&physics_of_world(), transform), &globals[node]);
    let col = |i: usize| Vec3::new(m[i][0], m[i][1], m[i][2]);
    (col(3), col(0) * axis.x + col(1) * axis.y + col(2) * axis.z)
}

/// Height of the terrain (physics metres) under the physics point `(x, y)`.
pub(super) fn ground_height(terrain: &TerrainData, x: f32, y: f32) -> f32 {
    let (wx, wy) = ((x * PIXELS_PER_METRE) as i32, (-y * PIXELS_PER_METRE) as i32);
    view3d::ground_point(wx, wy, terrain.height_at(wx, wy))[2] / PIXELS_PER_METRE
}

/// The rotation `rot` about the point `p`.
fn rot_about(rot: &[[f32; 4]; 4], p: [f32; 3]) -> Mat4 {
    let mut to = math::IDENTITY;
    to[3] = [p[0], p[1], p[2], 1.0];
    let mut back = math::IDENTITY;
    back[3] = [-p[0], -p[1], -p[2], 1.0];
    math::mul(&to, &math::mul(rot, &back))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_of_a_model_matrix() {
        let m = view3d::model_matrix([100.0, 200.0, 30.0], 0.7, 1.0);
        let p = math::mul(&affine_inverse(&m), &m);
        for (c, col) in p.iter().enumerate() {
            for (r, v) in col.iter().enumerate() {
                let want = if c == r { 1.0 } else { 0.0 };
                assert!((v - want).abs() < 1e-4, "{p:?}");
            }
        }
    }

    #[test]
    fn skeleton_space_turned_by_the_facing_is_physics() {
        // A skeleton point (Blender) through glTF and the model matrix lands where the
        // ragdoll spawned with the same yaw puts it.
        let yaw = 0.9;
        let ground = [300.0, 600.0, 15.0];
        let m = view3d::model_matrix(ground, yaw, 1.0);
        let skel = [0.2, -0.1, 1.4, 1.0];
        let g = math::mul(&SKELETON_TO_GLTF, &{
            let mut t = math::IDENTITY;
            t[3] = skel;
            t
        })[3];
        let w = math::transform_point(&m, [g[0], g[1], g[2]]);
        let p = physics_of(w);
        let q = Quat::from_axis_angle(Vec3::Z, yaw).rotate(Vec3::new(skel[0], skel[1], skel[2])) + physics_of(ground);
        assert!((p - q).length() < 1e-4, "{p:?} vs {q:?}");
    }

    #[test]
    fn struck_bones_of_an_animal() {
        use acr_sim::body::{Injury, LimbId, Outcome, Part};
        let bone = |name, parent, head: [f32; 3], tail: [f32; 3]| RigBone { name, parent, head, tail, radius: 0.1 };
        let rig = vec![
            bone("RigPelvis", None, [0.0, 0.4, 0.8], [0.0, 0.2, 0.8]),
            bone("RigSpine1", Some(0), [0.0, 0.2, 0.8], [0.0, 0.0, 0.8]),
            bone("RigChest", Some(1), [0.0, 0.0, 0.8], [0.0, -0.3, 0.8]),
            bone("RigNeck1", Some(2), [0.0, -0.3, 0.8], [0.0, -0.5, 0.9]),
            bone("RigHead", Some(3), [0.0, -0.5, 0.9], [0.0, -0.8, 0.9]),
            bone("RigLFLegCollarbone", Some(2), [0.1, -0.2, 0.8], [0.15, -0.25, 0.7]),
            bone("RigLFLeg1", Some(5), [0.15, -0.25, 0.7], [0.15, -0.25, 0.4]),
            bone("RigLFLeg2", Some(6), [0.15, -0.25, 0.4], [0.15, -0.25, 0.05]),
            bone("RigLBLeg1", Some(0), [0.15, 0.4, 0.75], [0.15, 0.4, 0.4]),
            bone("RigLBLeg2", Some(8), [0.15, 0.4, 0.4], [0.15, 0.4, 0.05]),
        ];
        let (desc, mass) = acr_physics::skeleton_generic(&rig);
        let g = Generic { kinds: acr_physics::bone_kinds(&rig), desc, mass };
        let hit = |part, level| HitRecord {
            tick: 1,
            victim: 1,
            pos: (0, 0),
            dir: (1, 0),
            cause: DeathCause::Gunshot,
            killed: false,
            building: false,
            region: None,
            injury: Some(Injury { part, level, outcome: Outcome::Wound, energy: 80, cause: Cause::Musket }),
            strike: None,
            critical: None,
        };
        let name = |h: HitRecord| g.desc[struck_generic(&h, &g).0].name;
        assert_eq!(name(hit(Part::Head, 0)), "RigHead");
        assert_eq!(name(hit(Part::Chest, 40)), "RigChest");
        assert_eq!(name(hit(Part::Pelvis, 240)), "RigPelvis");
        assert_eq!(name(hit(Part::Limb(LimbId::ArmL), 30)), "RigLFLeg1");
        assert_eq!(name(hit(Part::Limb(LimbId::ArmL), 220)), "RigLFLeg2");
        assert_eq!(name(hit(Part::Limb(LimbId::LegL), 220)), "RigLBLeg2");
        assert_eq!(struck_generic(&hit(Part::Limb(LimbId::LegL), 10), &g).1, Region::LegL);
        // No right legs in this rig: the blow lands on the trunk.
        assert_eq!(struck_generic(&hit(Part::Limb(LimbId::ArmR), 10), &g).1, Region::Torso);
    }

    #[test]
    fn calibration_gives_a_budget() {
        let (cost, budget) = calibrate();
        assert!(cost > 0.0 && budget > 0, "{cost} {budget}");
    }
}
