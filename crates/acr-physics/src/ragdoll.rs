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

    /// Freezes the ragdoll in its current pose: a static corpse that costs nothing and
    /// still blocks others, until an impulse or an awake body touching it wakes it.
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
