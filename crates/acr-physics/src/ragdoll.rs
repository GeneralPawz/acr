//! Ragdolls from a skeleton description: one capsule per deforming bone, Dempster masses,
//! anatomical joint limits, muscle tone per joint or region, and the read-back of bone
//! transforms for skinning.

use crate::math::{Quat, Vec3, closest_segment_params, mat4_from_rt};
use crate::world::{Body, BodyId, JointId, JointLimit, Tone, World};

/// Body region, for setting the tone of several joints at once (a wounded arm goes limp).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Torso,
    Head,
    ArmL,
    ArmR,
    LegL,
    LegR,
}

/// How a bone hangs on its parent. Angles in degrees; `flex` is the direction (skeleton
/// space, rest pose) the bone's tail moves when the joint flexes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JointSpec {
    /// The root body (no joint).
    Root,
    /// Knee, elbow (and toes): rotation about one axis, flexion positive, in `[lo, hi]`.
    Hinge { flex: [f32; 3], lo: f32, hi: f32 },
    /// Hip, shoulder, neck, spine: a cone of half-angle `cone` around the rest direction
    /// tilted by `offset` towards `flex`, twist within `±twist`.
    Ball { flex: [f32; 3], cone: f32, offset: f32, twist: f32 },
    /// Wrist, ankle: a small cone around the rest direction and little twist.
    LimitedBall { flex: [f32; 3], cone: f32, twist: f32 },
}

/// One bone of a skeleton description (skeleton space: metres, `+X` left, `-Y` forward,
/// `+Z` up, rest pose).
#[derive(Clone, Copy, Debug)]
pub struct BoneDesc {
    pub name: &'static str,
    pub parent: Option<&'static str>,
    pub head: [f32; 3],
    pub tail: [f32; 3],
    /// Capsule radius (m).
    pub radius: f32,
    /// Share of the total body mass (Dempster).
    pub mass_fraction: f32,
    pub joint: JointSpec,
    pub region: Region,
}

const FWD: [f32; 3] = [0.0, -1.0, 0.0];
const BACK: [f32; 3] = [0.0, 1.0, 0.0];
const UP: [f32; 3] = [0.0, 0.0, 1.0];

/// The 22 deforming body bones of skeleton H (`assets/blender/lib/acr_assets/rig.py`,
/// `BODY_BONES`) with capsule radii, Dempster mass fractions (sum 1) and joint limits.
pub fn skeleton_h() -> Vec<BoneDesc> {
    use JointSpec::*;
    use Region::*;
    let spine = Ball { flex: FWD, cone: 25.0, offset: 10.0, twist: 20.0 };
    let a_pose = 35f32.to_radians();
    let mut v = vec![
        bone("hips", None, [0.0, 0.0, 0.95], [0.0, 0.0, 1.05], 0.12, 0.142, Root, Torso),
        bone("spine", Some("hips"), [0.0, 0.0, 1.05], [0.0, 0.0, 1.20], 0.12, 0.11, spine, Torso),
        bone("spine1", Some("spine"), [0.0, 0.0, 1.20], [0.0, 0.0, 1.33], 0.12, 0.11, spine, Torso),
        bone("spine2", Some("spine1"), [0.0, 0.0, 1.33], [0.0, 0.0, 1.45], 0.13, 0.105, spine, Torso),
        bone(
            "neck",
            Some("spine2"),
            [0.0, 0.0, 1.45],
            [0.0, -0.01, 1.55],
            0.05,
            0.02,
            Ball { flex: FWD, cone: 40.0, offset: 10.0, twist: 45.0 },
            Head,
        ),
        bone(
            "head",
            Some("neck"),
            [0.0, -0.01, 1.55],
            [0.0, -0.01, 1.75],
            0.10,
            0.061,
            Ball { flex: FWD, cone: 35.0, offset: 5.0, twist: 45.0 },
            Head,
        ),
    ];
    const LEFT: [&str; 8] = ["clavicle_l", "upperarm_l", "forearm_l", "hand_l", "thigh_l", "shin_l", "foot_l", "toe_l"];
    const RIGHT: [&str; 8] =
        ["clavicle_r", "upperarm_r", "forearm_r", "hand_r", "thigh_r", "shin_r", "foot_r", "toe_r"];
    for (side, names) in [(1.0f32, LEFT), (-1.0, RIGHT)] {
        let n = |s: &str| -> &'static str {
            let i = ["clavicle", "upperarm", "forearm", "hand", "thigh", "shin", "foot", "toe"]
                .iter()
                .position(|x| *x == s)
                .unwrap_or(0);
            names[i]
        };
        let (arm, leg) = if side > 0.0 { (ArmL, LegL) } else { (ArmR, LegR) };
        let sh = [0.17 * side, 0.0, 1.43];
        let d = [a_pose.sin() * side, 0.0, -a_pose.cos()];
        let along = |p: [f32; 3], l: f32| [p[0] + d[0] * l, p[1] + d[1] * l, p[2] + d[2] * l];
        let elbow = along(sh, 0.29);
        let wrist = along(elbow, 0.25);
        let hand_end = along(wrist, 0.08);
        let hx = 0.09 * side;
        v.extend([
            bone(
                n("clavicle"),
                Some("spine2"),
                [0.03 * side, -0.01, 1.41],
                sh,
                0.04,
                0.015,
                LimitedBall { flex: UP, cone: 15.0, twist: 10.0 },
                Torso,
            ),
            bone(
                n("upperarm"),
                Some(n("clavicle")),
                sh,
                elbow,
                0.045,
                0.028,
                Ball { flex: FWD, cone: 110.0, offset: 30.0, twist: 70.0 },
                arm,
            ),
            bone(
                n("forearm"),
                Some(n("upperarm")),
                elbow,
                wrist,
                0.04,
                0.016,
                Hinge { flex: FWD, lo: 0.0, hi: 145.0 },
                arm,
            ),
            bone(
                n("hand"),
                Some(n("forearm")),
                wrist,
                hand_end,
                0.035,
                0.006,
                LimitedBall { flex: FWD, cone: 60.0, twist: 30.0 },
                arm,
            ),
            bone(
                n("thigh"),
                Some("hips"),
                [hx, 0.0, 0.93],
                [hx * 1.05, 0.0, 0.50],
                0.07,
                0.100,
                Ball { flex: FWD, cone: 70.0, offset: 45.0, twist: 35.0 },
                leg,
            ),
            bone(
                n("shin"),
                Some(n("thigh")),
                [hx * 1.05, 0.0, 0.50],
                [hx * 1.05, 0.02, 0.085],
                0.05,
                0.0465,
                Hinge { flex: BACK, lo: 0.0, hi: 150.0 },
                leg,
            ),
            bone(
                n("foot"),
                Some(n("shin")),
                [hx * 1.05, 0.02, 0.085],
                [hx * 1.08, -0.11, 0.025],
                0.03,
                0.0115,
                LimitedBall { flex: UP, cone: 30.0, twist: 12.0 },
                leg,
            ),
            bone(
                n("toe"),
                Some(n("foot")),
                [hx * 1.08, -0.11, 0.025],
                [hx * 1.1, -0.18, 0.02],
                0.02,
                0.003,
                Hinge { flex: UP, lo: -30.0, hi: 50.0 },
                leg,
            ),
        ]);
    }
    v
}

/// One bone of an arbitrary rig, for [`skeleton_generic`] (skeleton space: metres, `+X`
/// left, `-Y` forward, `+Z` up, rest pose). Parents come before their children.
#[derive(Clone, Copy, Debug)]
pub struct RigBone {
    pub name: &'static str,
    /// Index of the parent in the list (`None` for the root).
    pub parent: Option<usize>,
    pub head: [f32; 3],
    pub tail: [f32; 3],
    /// Radius of the flesh around the bone (m), e.g. from the skinned vertices; 0: unknown
    /// (a quarter of the bone's length).
    pub radius: f32,
}

/// Density of flesh (kg/m³) for masses by volume, a little under water's: capsules overlap
/// at the joints.
pub const FLESH_DENSITY: f32 = 850.0;

/// What a bone of an arbitrary rig is, by its name and place in the hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoneKind {
    Root,
    /// Spine, chest, hips: stiff.
    Spine,
    Neck,
    Head,
    Jaw,
    Tail,
    /// Shoulder blade, collarbone or a pelvis side bone: the first bone of a limb, barely
    /// moving.
    Girdle,
    /// The upper bone of a leg (shoulder or hip joint).
    Upper,
    /// Middle bones of a leg (elbow, knee, hock): hinge-like.
    Middle,
    /// The last bone of a leg (foot, paw, hoof).
    Foot,
}

fn has(name: &str, parts: &[&str]) -> bool {
    parts.iter().any(|p| name.contains(p))
}

const LIMB_WORDS: [&str; 18] = [
    "leg", "arm", "thigh", "shin", "calf", "foot", "hand", "ankle", "collar", "clavic", "shoulder", "knee", "elbow",
    "wrist", "hoof", "paw", "scapula", "pelvis",
];

/// The kind of each bone of `rig` (see [`BoneKind`]).
pub fn bone_kinds(rig: &[RigBone]) -> Vec<BoneKind> {
    let lower: Vec<String> = rig.iter().map(|b| b.name.to_ascii_lowercase()).collect();
    let limb = |i: usize| rig[i].parent.is_some() && has(&lower[i], &LIMB_WORDS);
    let mut kinds = Vec::with_capacity(rig.len());
    for i in 0..rig.len() {
        let n = &lower[i];
        let kind = if rig[i].parent.is_none() {
            BoneKind::Root
        } else if has(n, &["tail"]) {
            BoneKind::Tail
        } else if has(n, &["jaw"]) {
            BoneKind::Jaw
        } else if has(n, &["neck"]) {
            BoneKind::Neck
        } else if has(n, &["head", "skull"]) {
            BoneKind::Head
        } else if limb(i) {
            let parent_limb = rig[i].parent.is_some_and(limb);
            let leaf = !(0..rig.len()).any(|c| rig[c].parent == Some(i) && limb(c));
            let girdle = has(n, &["collar", "clavic", "shoulder", "scapula", "pelvis"]);
            if !parent_limb && girdle {
                BoneKind::Girdle
            } else if leaf {
                BoneKind::Foot
            } else if !parent_limb || rig[i].parent.is_some_and(|p| kinds.get(p) == Some(&BoneKind::Girdle)) {
                BoneKind::Upper
            } else {
                BoneKind::Middle
            }
        } else {
            BoneKind::Spine
        };
        kinds.push(kind);
    }
    kinds
}

/// A ragdoll description of any rig (quadrupeds, the alligator): capsules sized from the
/// bone's length and flesh radius, masses by volume ([`FLESH_DENSITY`]), and joint limits by
/// [`BoneKind`]: the spine stiff and limited, legs hinge-like, the neck and tail flexible.
/// Legs hanging ahead of the root from the chest are the "arms" ([`Region::ArmL`], [`Region::ArmR`]), the others
/// the legs; the side is by `x`. Returns the bones and their total mass (kg).
pub fn skeleton_generic(rig: &[RigBone]) -> (Vec<BoneDesc>, f32) {
    use JointSpec::*;
    const DOWN: [f32; 3] = [0.0, 0.0, -1.0];
    let kinds = bone_kinds(rig);
    let root_y = rig.first().map_or(0.0, |b| b.head[1]);
    // The limb's first bone, for front or back.
    let limb_root = |mut i: usize| {
        while let Some(p) = rig[i].parent {
            if matches!(kinds[p], BoneKind::Root | BoneKind::Spine) {
                break;
            }
            i = p;
        }
        i
    };
    let mut volumes = Vec::with_capacity(rig.len());
    let mut out = Vec::with_capacity(rig.len());
    for (i, b) in rig.iter().enumerate() {
        let (h, t) = (Vec3::from_array(b.head), Vec3::from_array(b.tail));
        let len = (t - h).length().max(0.01);
        let d = (t - h).normalize_or_zero();
        let radius = if b.radius > 0.0 { b.radius.clamp(0.015, 0.45) } else { (0.25 * len).clamp(0.02, 0.2) };
        let pi = core::f32::consts::PI;
        volumes.push(pi * radius * radius * len + 2.0 / 3.0 * pi * radius * radius * radius);
        // Hinges bend vertical bones forwards and back, level bones (splayed legs) up and
        // down; both ways, the rest pose is already bent.
        let bend = if d.z.abs() > 0.6 { FWD } else { DOWN };
        let joint = match kinds[i] {
            BoneKind::Root => Root,
            BoneKind::Spine => Ball { flex: DOWN, cone: 14.0, offset: 0.0, twist: 8.0 },
            BoneKind::Neck => Ball { flex: DOWN, cone: 35.0, offset: 0.0, twist: 30.0 },
            BoneKind::Head => Ball { flex: DOWN, cone: 30.0, offset: 0.0, twist: 25.0 },
            BoneKind::Jaw => Hinge { flex: DOWN, lo: -5.0, hi: 35.0 },
            BoneKind::Tail => Ball { flex: DOWN, cone: 40.0, offset: 0.0, twist: 20.0 },
            BoneKind::Girdle => LimitedBall { flex: FWD, cone: 15.0, twist: 10.0 },
            BoneKind::Upper => Ball { flex: bend, cone: 55.0, offset: 0.0, twist: 20.0 },
            BoneKind::Middle => Hinge { flex: bend, lo: -95.0, hi: 95.0 },
            BoneKind::Foot => LimitedBall { flex: bend, cone: 35.0, twist: 15.0 },
        };
        let region = match kinds[i] {
            BoneKind::Root | BoneKind::Spine | BoneKind::Tail => Region::Torso,
            BoneKind::Neck | BoneKind::Head | BoneKind::Jaw => Region::Head,
            _ => {
                let r = &rig[limb_root(i)];
                let left = (r.head[0] + r.tail[0]) * 0.5 + (b.head[0] + b.tail[0]) * 0.5 > 0.0;
                // Front legs hang from the chest, ahead of the root; hind legs from the root.
                let front = r.parent.is_some_and(|p| kinds[p] != BoneKind::Root) && r.head[1] < root_y;
                match (front, left) {
                    (true, true) => Region::ArmL,
                    (true, false) => Region::ArmR,
                    (false, true) => Region::LegL,
                    (false, false) => Region::LegR,
                }
            }
        };
        out.push(BoneDesc {
            name: b.name,
            parent: b.parent.map(|p| rig[p].name),
            head: b.head,
            tail: b.tail,
            radius,
            mass_fraction: 0.0,
            joint,
            region,
        });
    }
    let total: f32 = volumes.iter().sum();
    for (o, v) in out.iter_mut().zip(&volumes) {
        o.mass_fraction = v / total.max(1e-9);
    }
    (out, total * FLESH_DENSITY)
}

#[allow(clippy::too_many_arguments)]
fn bone(
    name: &'static str,
    parent: Option<&'static str>,
    head: [f32; 3],
    tail: [f32; 3],
    radius: f32,
    mass_fraction: f32,
    joint: JointSpec,
    region: Region,
) -> BoneDesc {
    BoneDesc { name, parent, head, tail, radius, mass_fraction, joint, region }
}

#[derive(Clone, Debug)]
struct RagBone {
    name: &'static str,
    body: BodyId,
    joint: Option<JointId>,
    region: Region,
    /// Rest transform of the body in skeleton space.
    rest_pos: Vec3,
    rest_rot: Quat,
}

/// A ragdoll in a [`World`]: bodies and joints of one skeleton, indexed by bone.
#[derive(Clone, Debug)]
pub struct Ragdoll {
    bones: Vec<RagBone>,
}

/// Rest frame of a capsule along `d`: local `+Y` along the bone.
fn bone_frame(d: Vec3) -> Quat {
    let r = if d.z.abs() < 0.9 { Vec3::Z } else { Vec3::new(0.0, -1.0, 0.0) };
    let x = d.cross(r).normalize_or_zero();
    Quat::from_basis(x, d, x.cross(d))
}

impl Ragdoll {
    /// Builds the ragdoll of `skeleton` (parents before children, at most 64 bones) with
    /// `total_mass` (kg), placed at `pos` turned by `rot` (skeleton space → world). All
    /// joints start limp with the rest pose as target.
    pub fn spawn(world: &mut World, skeleton: &[BoneDesc], total_mass: f32, pos: Vec3, rot: Quat) -> Ragdoll {
        assert!(skeleton.len() <= 64, "a ragdoll has at most 64 bodies");
        let index = |name: &str| skeleton.iter().position(|b| b.name == name);
        let parents: Vec<Option<usize>> =
            skeleton.iter().map(|b| b.parent.map(|p| index(p).expect("parent bone is listed"))).collect();
        let seg = |b: &BoneDesc| (Vec3::from_array(b.head), Vec3::from_array(b.tail));
        // Pairs that never collide: parent and child, and capsules overlapping at rest.
        let mut exclude = vec![0u64; skeleton.len()];
        for i in 0..skeleton.len() {
            for j in i + 1..skeleton.len() {
                let ((a0, a1), (b0, b1)) = (seg(&skeleton[i]), seg(&skeleton[j]));
                let (s, t) = closest_segment_params(a0, a1 - a0, b0, b1 - b0);
                let dist = ((a0 + (a1 - a0) * s) - (b0 + (b1 - b0) * t)).length();
                let related = parents[i] == Some(j) || parents[j] == Some(i);
                if related || dist < skeleton[i].radius + skeleton[j].radius + 0.01 {
                    exclude[i] |= 1 << j;
                    exclude[j] |= 1 << i;
                }
            }
        }
        let group = world.new_group();
        let place = |p: Vec3| pos + rot.rotate(p);
        let mut bones: Vec<RagBone> = Vec::with_capacity(skeleton.len());
        for (i, b) in skeleton.iter().enumerate() {
            let (h, t) = seg(b);
            let d = (t - h).normalize_or_zero();
            let rest_rot = bone_frame(d);
            let rest_pos = (h + t) * 0.5;
            let body = Body::capsule(
                place(rest_pos),
                rot * rest_rot,
                b.radius,
                0.5 * (t - h).length(),
                (b.mass_fraction * total_mass).max(0.05),
            );
            let id = world.add_body_in_group(body, group, i as u32, exclude[i]);
            let joint = parents[i].map(|p| {
                let (frame, limit) = joint_frame(d, &b.joint);
                world.add_joint(bones[p].body, id, place(h), rot * frame, limit)
            });
            bones.push(RagBone { name: b.name, body: id, joint, region: b.region, rest_pos, rest_rot });
        }
        Ragdoll { bones }
    }

    pub fn len(&self) -> usize {
        self.bones.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }
    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }
    pub fn bone_name(&self, bone: usize) -> &'static str {
        self.bones[bone].name
    }
    pub fn body(&self, bone: usize) -> BodyId {
        self.bones[bone].body
    }
    /// The joint to the bone's parent (`None` for the root).
    pub fn joint(&self, bone: usize) -> Option<JointId> {
        self.bones[bone].joint
    }
    pub fn region(&self, bone: usize) -> Region {
        self.bones[bone].region
    }

    /// Target of the bone's muscles: its rotation relative to the parent as a delta from the
    /// rest pose, in the bone's own (body) frame. Identity = rest pose.
    pub fn set_target(&self, world: &mut World, bone: usize, rotation: Quat) {
        if let Some(j) = self.bones[bone].joint {
            world.set_joint_target(j, rotation);
        }
    }
    /// Targets for all bones at once (in bone order; the root's entry is ignored).
    pub fn set_pose(&self, world: &mut World, rotations: &[Quat]) {
        for (b, q) in rotations.iter().enumerate().take(self.bones.len()) {
            self.set_target(world, b, *q);
        }
    }
    /// The hinge axis (or the joint frame's Z) in the bone's frame: a hinge target is
    /// `Quat::from_axis_angle(axis, flexion)`.
    pub fn joint_axis(&self, world: &World, bone: usize) -> Vec3 {
        match self.bones[bone].joint {
            Some(j) => world.joint(j).fb.rotate(Vec3::Z),
            None => Vec3::Z,
        }
    }
    pub fn set_tone(&self, world: &mut World, bone: usize, tone: Tone) {
        if let Some(j) = self.bones[bone].joint {
            world.set_joint_tone(j, tone);
        }
    }
    pub fn set_tone_region(&self, world: &mut World, region: Region, tone: Tone) {
        for b in 0..self.bones.len() {
            if self.bones[b].region == region {
                self.set_tone(world, b, tone);
            }
        }
    }
    pub fn set_tone_all(&self, world: &mut World, tone: Tone) {
        for b in 0..self.bones.len() {
            self.set_tone(world, b, tone);
        }
    }

    /// World transform of the bone's body: position and rotation `[x, y, z, w]`.
    pub fn bone_transform(&self, world: &World, bone: usize) -> ([f32; 3], [f32; 4]) {
        let b = world.body(self.bones[bone].body);
        (b.pos.to_array(), b.rot.to_array())
    }

    /// Skinning matrix of the bone (column-major): maps a skeleton-space rest vertex to its
    /// current world position, i.e. `current · rest⁻¹`.
    pub fn skin_matrix(&self, world: &World, bone: usize) -> [[f32; 4]; 4] {
        let rb = &self.bones[bone];
        let b = world.body(rb.body);
        let q = b.rot * rb.rest_rot.conj();
        mat4_from_rt(b.pos - q.rotate(rb.rest_pos), q)
    }

    /// Places the bone's body by a skinning matrix (column-major, `current · rest⁻¹` as
    /// [`Ragdoll::skin_matrix`] gives it, e.g. from an animation) moving at `vel` (m/s).
    pub fn place(&self, world: &mut World, bone: usize, skin: &[[f32; 4]; 4], vel: Vec3) {
        let col = |c: usize| Vec3::new(skin[c][0], skin[c][1], skin[c][2]);
        let x = col(0).normalize_or_zero();
        let z = x.cross(col(1)).normalize_or_zero();
        let q = Quat::from_basis(x, z.cross(x), z);
        let rb = &self.bones[bone];
        let p = col(3) + q.rotate(rb.rest_pos);
        world.set_body_state(rb.body, p, q * rb.rest_rot, vel, Vec3::ZERO);
    }

    /// Pins the root body (kinematic, moved by [`Ragdoll::place`]) or frees it; no change,
    /// no wake.
    pub fn set_pinned_root(&self, world: &mut World, pinned: bool) {
        let id = self.bones[0].body;
        if world.body(id).is_pinned() != pinned {
            world.set_pinned(id, pinned);
        }
    }

    /// Makes the current pose the muscles' target (every joint's relative rotation now).
    pub fn target_current_pose(&self, world: &mut World) {
        for rb in &self.bones {
            let Some(j) = rb.joint else { continue };
            let jt = world.joint(j);
            let (qa, qb) = (world.body(jt.a).rot, world.body(jt.b).rot);
            let target = (jt.rest_rel.conj() * qa.conj() * qb).normalize();
            world.set_joint_target(j, target);
        }
    }

    /// Makes a pose given as skinning matrices per bone (as for [`Ragdoll::place`]) the
    /// muscles' target, without moving the bodies.
    pub fn target_skins(&self, world: &mut World, skins: &[[[f32; 4]; 4]]) {
        let rot = |b: usize| {
            let m = &skins[b];
            let col = |c: usize| Vec3::new(m[c][0], m[c][1], m[c][2]);
            let x = col(0).normalize_or_zero();
            let z = x.cross(col(1)).normalize_or_zero();
            Quat::from_basis(x, z.cross(x), z) * self.bones[b].rest_rot
        };
        for b in 0..self.bones.len().min(skins.len()) {
            let Some(j) = self.bones[b].joint else { continue };
            let jt = world.joint(j);
            let Some(pa) = self.bones.iter().position(|r| r.body == jt.a) else { continue };
            let target = (jt.rest_rel.conj() * rot(pa).conj() * rot(b)).normalize();
            world.set_joint_target(j, target);
        }
    }

    /// The muscles' targets of all joints (bone order; the root's entry is identity).
    pub fn targets(&self, world: &World, out: &mut [Quat]) {
        for (o, rb) in out.iter_mut().zip(&self.bones) {
            *o = rb.joint.map_or(Quat::IDENTITY, |j| world.joint(j).target);
        }
    }

    /// A blow of `impulse` (N·s) at `point` on `bone`, of which the struck body takes at most
    /// `max_dv` (m/s) worth; the rest goes to the trunk (the root, or the first spine bone
    /// while the root is pinned) at its centre. A light limb struck by a melee weapon is
    /// pushed, not whipped into a spin that throws the whole body; the momentum is the same.
    pub fn apply_blow(&self, world: &mut World, bone: usize, point: Vec3, impulse: Vec3, max_dv: f32) {
        let id = self.bones[bone].body;
        let j = impulse.length();
        let own = j.min(world.body(id).mass * max_dv);
        let share = if j > 0.0 { own / j } else { 1.0 };
        world.apply_impulse(id, point, impulse * share);
        if share < 1.0 {
            let trunk = if world.body(self.bones[0].body).is_pinned() { 1.min(self.bones.len() - 1) } else { 0 };
            let tid = self.bones[trunk].body;
            let centre = world.body(tid).pos;
            world.apply_impulse(tid, centre, impulse * (1.0 - share));
        }
    }

    /// Every body moving at `vel` without spin. A pinned root moved by [`Ragdoll::place`]
    /// jumps once per frame and leaves velocity spikes in the bodies hanging on it; a
    /// survivor's ragdoll that is let go (he dies) starts from his ground velocity instead.
    pub fn set_velocity(&self, world: &mut World, vel: Vec3) {
        for rb in &self.bones {
            let b = world.body(rb.body);
            let (p, q) = (b.pos, b.rot);
            world.set_body_state(rb.body, p, q, vel, Vec3::ZERO);
        }
    }

    /// Freezes the ragdoll in its current pose: a static corpse that costs nothing and
    /// still blocks others (one-sided, like the ground), until an explicit wake (an
    /// impulse, a new pose or tone).
    pub fn freeze(&self, world: &mut World) {
        world.freeze(self.bones[0].body);
    }
    pub fn is_asleep(&self, world: &World) -> bool {
        world.body(self.bones[0].body).is_asleep()
    }
    /// All skinning matrices (bone order) into `out`, e.g. to keep a frozen corpse's pose.
    pub fn pose(&self, world: &World, out: &mut [[[f32; 4]; 4]]) {
        for (b, m) in out.iter_mut().enumerate().take(self.bones.len()) {
            *m = self.skin_matrix(world, b);
        }
    }

    /// Lowest point of all capsules (m, world Z).
    pub fn min_z(&self, world: &World) -> f32 {
        self.bones
            .iter()
            .map(|rb| {
                let b = world.body(rb.body);
                let (e0, e1) = b.ends();
                e0.z.min(e1.z) - b.radius
            })
            .fold(f32::INFINITY, f32::min)
    }

    /// Centre of mass (world).
    pub fn center_of_mass(&self, world: &World) -> Vec3 {
        let (mut m, mut p) = (0.0, Vec3::ZERO);
        for rb in &self.bones {
            let b = world.body(rb.body);
            m += b.mass;
            p += b.pos * b.mass;
        }
        p * (1.0 / m)
    }
}

/// Joint frame (world, rest) and limit of a bone along `d`.
fn joint_frame(d: Vec3, spec: &JointSpec) -> (Quat, JointLimit) {
    let flex = match *spec {
        JointSpec::Root => [0.0, -1.0, 0.0],
        JointSpec::Hinge { flex, .. } | JointSpec::Ball { flex, .. } | JointSpec::LimitedBall { flex, .. } => flex,
    };
    let f = Vec3::from_array(flex);
    let mut z = d.cross(f).normalize_or_zero();
    if z == Vec3::ZERO {
        z = d.cross(Vec3::X).normalize_or_zero();
    }
    let frame = Quat::from_basis(d, z.cross(d), z);
    let rad = |deg: f32| deg.to_radians();
    let limit = match *spec {
        JointSpec::Hinge { lo, hi, .. } => JointLimit::Hinge { lo: rad(lo), hi: rad(hi) },
        JointSpec::Ball { cone, offset, twist, .. } => JointLimit::Ball {
            cone_axis: Vec3::new(rad(offset).cos(), rad(offset).sin(), 0.0),
            cone: rad(cone),
            twist_lo: -rad(twist),
            twist_hi: rad(twist),
        },
        JointSpec::LimitedBall { cone, twist, .. } => {
            JointLimit::Ball { cone_axis: Vec3::X, cone: rad(cone), twist_lo: -rad(twist), twist_hi: rad(twist) }
        }
        JointSpec::Root => {
            JointLimit::Ball { cone_axis: Vec3::X, cone: core::f32::consts::PI, twist_lo: -4.0, twist_hi: 4.0 }
        }
    };
    (frame, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_h_masses_sum_to_one() {
        let s = skeleton_h();
        assert_eq!(s.len(), 22);
        let sum: f32 = s.iter().map(|b| b.mass_fraction).sum();
        assert!((sum - 1.0).abs() < 1e-4, "{sum}");
    }
}
