//! The XPBD solver: rigid capsule bodies, joints with limits and muscle drives, contacts
//! against a ground height field and between capsules, and sleeping.
//!
//! Müller et al. 2020, "Detailed Rigid Body Simulation with Extended Position Based
//! Dynamics": per substep integrate, solve position constraints, derive velocities, solve
//! velocity constraints (friction, restitution, damping).
//!
//! Cost notes (see the bench): the world inverse inertia of every body is computed once
//! per substep and used for all corrections in it; the ground is sampled once per step
//! (a tangent plane per capsule end); joint limits are first checked without trigonometry
//! and only corrected when violated; capsule pairs come from a hashed grid; resting ragdolls
//! go to sleep and cost nothing.
//!
//! Only awake bodies are visited by a step. Frozen ragdolls (corpses) are static collision
//! geometry: their capsules sit in a hashed grid of their own, rebuilt only when a group
//! falls asleep or wakes, and contacts against them are one-sided (infinite mass), so a
//! pile of corpses costs a falling man only the capsules he actually touches. Awake bodies
//! touching or joined form islands; each island's bodies are gathered into compact buffers
//! and its substeps run apart from the others, in parallel on native (rayon).

use crate::math::{Quat, Vec3, closest_segment_params};

/// Index of a body in its [`World`].
pub type BodyId = u32;
/// Index of a joint in its [`World`].
pub type JointId = u32;

const STATIC: u32 = u32::MAX;

/// Smallest principal moment of inertia of a body (kg·m²).
pub const INERTIA_FLOOR: f32 = 0.004;

/// Terrain under the bodies: a height field over `(x, y)`, `+Z` up (skeleton H axes).
pub trait Ground {
    fn height(&self, x: f32, y: f32) -> f32;
    /// Upward unit normal at `(x, y)`; by default from central differences of `height`.
    fn normal(&self, x: f32, y: f32) -> Vec3 {
        const E: f32 = 0.05;
        let dx = self.height(x + E, y) - self.height(x - E, y);
        let dy = self.height(x, y + E) - self.height(x, y - E);
        Vec3::new(-dx, -dy, 2.0 * E).normalize_or_zero()
    }
}

impl<F: Fn(f32, f32) -> f32> Ground for F {
    fn height(&self, x: f32, y: f32) -> f32 {
        self(x, y)
    }
}

/// Symmetric 3×3 matrix (world inverse inertia).
#[derive(Clone, Copy, Debug, Default)]
struct Sym3 {
    xx: f32,
    yy: f32,
    zz: f32,
    xy: f32,
    xz: f32,
    yz: f32,
}

impl Sym3 {
    /// `R · diag(d) · Rᵀ`.
    fn rotated_diag(q: Quat, d: Vec3) -> Sym3 {
        let (a, b, c) = (q.rotate(Vec3::X), q.rotate(Vec3::Y), q.rotate(Vec3::Z));
        Sym3 {
            xx: d.x * a.x * a.x + d.y * b.x * b.x + d.z * c.x * c.x,
            yy: d.x * a.y * a.y + d.y * b.y * b.y + d.z * c.y * c.y,
            zz: d.x * a.z * a.z + d.y * b.z * b.z + d.z * c.z * c.z,
            xy: d.x * a.x * a.y + d.y * b.x * b.y + d.z * c.x * c.y,
            xz: d.x * a.x * a.z + d.y * b.x * b.z + d.z * c.x * c.z,
            yz: d.x * a.y * a.z + d.y * b.y * b.z + d.z * c.y * c.z,
        }
    }
    fn mul(&self, v: Vec3) -> Vec3 {
        Vec3::new(
            self.xx * v.x + self.xy * v.y + self.xz * v.z,
            self.xy * v.x + self.yy * v.y + self.yz * v.z,
            self.xz * v.x + self.yz * v.y + self.zz * v.z,
        )
    }
}

/// A rigid capsule: the segment `pos ± rot·(0, half_len, 0)` swept by `radius`.
#[derive(Clone, Debug)]
pub struct Body {
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub omega: Vec3,
    prev_pos: Vec3,
    prev_rot: Quat,
    pub mass: f32,
    /// Principal moments of inertia in the body frame.
    pub inertia: Vec3,
    inv_mass: f32,
    inv_inertia: Vec3,
    /// World inverse inertia, refreshed once per substep.
    inv_i: Sym3,
    pub radius: f32,
    pub half_len: f32,
    pinned: bool,
    asleep: bool,
    /// Collision and sleeping group (a ragdoll); bodies of one group skip the pairs in
    /// `exclude`.
    group: u32,
    /// Index within the group (below 64).
    group_index: u32,
    /// Bit `i` set: no contact with body `group_index == i` of the same group.
    exclude: u64,
}

impl Body {
    /// A capsule with mass `mass` (kg), inertia of a solid cylinder of the capsule's length.
    pub fn capsule(pos: Vec3, rot: Quat, radius: f32, half_len: f32, mass: f32) -> Body {
        let len = 2.0 * half_len + radius;
        let axial = 0.5 * mass * radius * radius;
        let transverse = mass * (3.0 * radius * radius + len * len) / 12.0;
        // Floor on the inertia: light end bodies (toes, hands) with tiny inertia converge
        // badly against heavy neighbours; the floor is invisible but keeps them stable.
        let floor = INERTIA_FLOOR;
        let inertia = Vec3::new(transverse.max(floor), axial.max(floor), transverse.max(floor));
        let inv_inertia = Vec3::new(1.0 / inertia.x, 1.0 / inertia.y, 1.0 / inertia.z);
        Body {
            pos,
            rot,
            vel: Vec3::ZERO,
            omega: Vec3::ZERO,
            prev_pos: pos,
            prev_rot: rot,
            mass,
            inertia,
            inv_mass: 1.0 / mass,
            inv_inertia,
            inv_i: Sym3::rotated_diag(rot, inv_inertia),
            radius,
            half_len,
            pinned: false,
            asleep: false,
            group: STATIC,
            group_index: 0,
            exclude: 0,
        }
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }
    pub fn is_asleep(&self) -> bool {
        self.asleep
    }

    /// The capsule's segment end points.
    pub fn ends(&self) -> (Vec3, Vec3) {
        let a = self.rot.rotate(Vec3::new(0.0, self.half_len, 0.0));
        (self.pos - a, self.pos + a)
    }

    /// World point of the body-local point `p`.
    pub fn world_point(&self, p: Vec3) -> Vec3 {
        self.pos + self.rot.rotate(p)
    }

    fn refresh_inertia(&mut self) {
        self.inv_i = Sym3::rotated_diag(self.rot, self.inv_inertia);
    }

    /// Generalized inverse mass for a positional correction along unit `n` at offset `r`.
    fn w_pos(&self, r: Vec3, n: Vec3) -> f32 {
        let rn = r.cross(n);
        self.inv_mass + rn.dot(self.inv_i.mul(rn))
    }

    /// Generalized inverse mass for a rotation about unit `n`.
    fn w_rot(&self, n: Vec3) -> f32 {
        n.dot(self.inv_i.mul(n))
    }

    /// Kinetic energy (J).
    pub fn kinetic_energy(&self) -> f32 {
        let l = self.rot.conj().rotate(self.omega);
        0.5 * self.mass * self.vel.length_sq() + 0.5 * l.dot(self.inertia.mul_elem(l))
    }
}

/// Muscle tone of a joint: a PD drive towards the target rotation. Stiffness in N·m/rad,
/// damping in N·m·s/rad. Zero stiffness is limp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tone {
    pub stiffness: f32,
    pub damping: f32,
}

impl Tone {
    pub const LIMP: Tone = Tone { stiffness: 0.0, damping: 0.0 };
    /// A healthy soldier holding a pose.
    pub const FULL: Tone = Tone { stiffness: 1500.0, damping: 25.0 };

    /// `self` scaled by `t` (0..1): tone falling off or coming back.
    pub fn scaled(self, t: f32) -> Tone {
        Tone { stiffness: self.stiffness * t, damping: self.damping * t }
    }
}

/// Limits of a joint, in the joint frame: X is the twist axis (along the child bone at
/// rest), Z the hinge axis, Y = Z × X.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JointLimit {
    /// Rotation about Z only, the angle (rad) from X(parent) to X(child) about Z in `[lo, hi]`.
    Hinge { lo: f32, hi: f32 },
    /// Child X within `cone` (rad) of `cone_axis` (unit, in the parent's joint frame), twist
    /// about X in `[twist_lo, twist_hi]` (within ±π).
    Ball { cone_axis: Vec3, cone: f32, twist_lo: f32, twist_hi: f32 },
}

/// Precomputed per joint so that the in-range checks need no trigonometry.
#[derive(Clone, Copy, Debug, Default)]
struct LimitCache {
    /// Joint frame axes in the bodies' frames.
    xa: Vec3,
    za: Vec3,
    xb: Vec3,
    zb: Vec3,
    /// Cone axis in body `a`'s frame.
    cone_a: Vec3,
    cos_cone: f32,
    tan_twist_lo: f32,
    tan_twist_hi: f32,
    /// Hinge: middle of the range and cosine of its half width.
    cos_mid: f32,
    sin_mid: f32,
    cos_half: f32,
}

impl LimitCache {
    fn new(fa: Quat, fb: Quat, limit: JointLimit) -> LimitCache {
        let mut c = LimitCache {
            xa: fa.rotate(Vec3::X),
            za: fa.rotate(Vec3::Z),
            xb: fb.rotate(Vec3::X),
            zb: fb.rotate(Vec3::Z),
            ..LimitCache::default()
        };
        match limit {
            JointLimit::Hinge { lo, hi } => {
                let (mid, half) = (0.5 * (lo + hi), 0.5 * (hi - lo));
                (c.sin_mid, c.cos_mid) = mid.sin_cos();
                c.cos_half = half.cos();
            }
            JointLimit::Ball { cone_axis, cone, twist_lo, twist_hi } => {
                c.cone_a = fa.rotate(cone_axis);
                c.cos_cone = cone.cos();
                c.tan_twist_lo = (0.5 * twist_lo).tan();
                c.tan_twist_hi = (0.5 * twist_hi).tan();
            }
        }
        c
    }
}

/// A joint between a parent body `a` and a child body `b`.
#[derive(Clone, Copy, Debug)]
pub struct Joint {
    pub a: BodyId,
    pub b: BodyId,
    /// Anchor in each body's frame.
    pub ra: Vec3,
    pub rb: Vec3,
    /// Joint frame in each body's frame.
    pub fa: Quat,
    pub fb: Quat,
    /// Read only; change it with [`World::set_joint_limit`].
    pub limit: JointLimit,
    /// `rot_a⁻¹ · rot_b` at rest.
    pub rest_rel: Quat,
    /// Drive target: the child's rotation relative to its rest pose, in the child's frame
    /// (`rot_b = rot_a · rest_rel · target`). Identity = rest pose.
    pub target: Quat,
    pub tone: Tone,
    cache: LimitCache,
}

/// Measured state of a joint (for tests and debugging).
#[derive(Clone, Copy, Debug)]
pub struct JointState {
    /// Distance between the two anchors (m).
    pub separation: f32,
    /// Hinge: the hinge angle. Ball: the angle of the child axis from the cone axis.
    pub angle: f32,
    /// Ball: twist angle; hinge: deviation of the hinge axes.
    pub twist: f32,
    /// Angle between the current and the target relative rotation.
    pub drive_error: f32,
}

/// Solver settings.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Fixed step (s).
    pub dt: f32,
    pub substeps: u32,
    /// Joint passes per substep (contacts get one). 4 × 4 measured best for cost and
    /// accuracy (docs/body-sim.md): fewer substeps cost less per pass than more passes.
    pub joint_iterations: u32,
    pub gravity: Vec3,
    /// Static and dynamic friction with the ground and between capsules.
    pub static_friction: f32,
    pub dynamic_friction: f32,
    /// Passive joint damping (N·m·s/rad, soft tissue), added to every tone.
    pub passive_damping: f32,
    /// Linear and angular air damping (1/s).
    pub air_damping: f32,
    /// A group (ragdoll) whose kinetic energy per kilogram stays below this (J/kg) for
    /// `sleep_time` seconds falls asleep. Zero disables sleeping.
    pub sleep_energy: f32,
    pub sleep_time: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            dt: 1.0 / 60.0,
            substeps: 4,
            joint_iterations: 4,
            gravity: Vec3::new(0.0, 0.0, -9.81),
            static_friction: 0.8,
            dynamic_friction: 0.6,
            passive_damping: 0.5,
            air_damping: 0.05,
            sleep_energy: 0.002,
            sleep_time: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Contact {
    a: u32,
    b: u32,
    /// Normal pointing towards `a` (out of the ground or `b`).
    n: Vec3,
    /// Contact points in the bodies' frames.
    ra: Vec3,
    rb: Vec3,
    lambda_n: f32,
}

/// Ground tangent plane under a capsule end, sampled once per step: `n·x = d`.
#[derive(Clone, Copy, Debug, Default)]
struct Plane {
    n: Vec3,
    d: f32,
}

#[derive(Clone, Copy, Debug)]
struct Group {
    first: u32,
    end: u32,
    calm_time: f32,
    asleep: bool,
    /// Frozen (a corpse): static collision geometry that only an explicit wake moves.
    frozen: bool,
}

/// A hashed grid over `(x, y)` (counting sort into buckets) of some of the world's bodies.
#[derive(Default)]
struct Grid {
    /// Cell size is `1 / inv`.
    inv: f32,
    bucket_start: Vec<u32>,
    sorted: Vec<u32>,
    cell: Vec<u32>,
}

impl Grid {
    fn mask(&self) -> u32 {
        (self.bucket_start.len() - 2) as u32
    }
    fn hash(&self, cx: i32, cy: i32) -> u32 {
        ((cx as u32).wrapping_mul(73_856_093) ^ (cy as u32).wrapping_mul(19_349_663)) & self.mask()
    }
    fn cell_of(&self, p: Vec3) -> (i32, i32) {
        ((p.x * self.inv).floor() as i32, (p.y * self.inv).floor() as i32)
    }

    /// Sorts `ids` (positions `pos(id)`) into cells of size `cell`.
    fn build(&mut self, ids: &[u32], cell: f32, pos: impl Fn(u32) -> Vec3) {
        self.inv = 1.0 / cell;
        let buckets = (2 * ids.len()).max(2).next_power_of_two();
        self.bucket_start.clear();
        self.bucket_start.resize(buckets + 1, 0);
        self.cell.clear();
        for &i in ids {
            let (cx, cy) = self.cell_of(pos(i));
            let k = self.hash(cx, cy);
            self.cell.push(k);
            self.bucket_start[k as usize + 1] += 1;
        }
        for k in 1..self.bucket_start.len() {
            self.bucket_start[k] += self.bucket_start[k - 1];
        }
        // Counting sort, using each bucket's start as its cursor, then shifting back.
        self.sorted.clear();
        self.sorted.resize(ids.len(), 0);
        for (n, &i) in ids.iter().enumerate() {
            let k = self.cell[n] as usize;
            self.sorted[self.bucket_start[k] as usize] = i;
            self.bucket_start[k] += 1;
        }
        for k in (1..self.bucket_start.len() - 1).rev() {
            self.bucket_start[k] = self.bucket_start[k - 1];
        }
        self.bucket_start[0] = 0;
    }

    /// Calls `f` with every body in the cells within `r` of `p` (each once).
    fn query(&self, p: Vec3, r: f32, mut f: impl FnMut(u32)) {
        if self.sorted.is_empty() {
            return;
        }
        let (x0, y0) = self.cell_of(p - Vec3::new(r, r, 0.0));
        let (x1, y1) = self.cell_of(p + Vec3::new(r, r, 0.0));
        let span = (x1 - x0 + 1) as i64 * (y1 - y0 + 1) as i64;
        if span > 25 {
            // A body flying fast: every body (rare).
            self.sorted.iter().for_each(|&i| f(i));
            return;
        }
        let mut seen = [u32::MAX; 25];
        let mut ns = 0;
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                let k = self.hash(cx, cy);
                if seen[..ns].contains(&k) {
                    continue;
                }
                seen[ns] = k;
                ns += 1;
                let (s, e) = (self.bucket_start[k as usize] as usize, self.bucket_start[k as usize + 1] as usize);
                self.sorted[s..e].iter().for_each(|&i| f(i));
            }
        }
    }
}

/// Awake bodies touching each other or joined: solved together, apart from other islands
/// (in parallel on native). The island's bodies are gathered into its own buffers.
#[derive(Default)]
struct Island {
    /// World ids of the bodies: the first `dynamic` are awake (written back after the
    /// step), the rest frozen corpses' capsules they touch (read only, infinite mass).
    ids: Vec<u32>,
    dynamic: usize,
    solver: Solver,
}

/// The substep loop over one island's bodies (local indices).
#[derive(Default)]
struct Solver {
    bodies: Vec<Body>,
    joints: Vec<Joint>,
    pairs: Vec<(u32, u32)>,
    /// Per dynamic body.
    planes: Vec<[Plane; 2]>,
    contacts: Vec<Contact>,
}

/// A set of bodies and joints stepped together. Allocates only when bodies or joints are
/// added or a step meets more awake bodies, pairs or islands than any before.
pub struct World {
    pub config: Config,
    bodies: Vec<Body>,
    joints: Vec<Joint>,
    groups: Vec<Group>,
    pairs: Vec<(u32, u32)>,
    planes: Vec<[Plane; 2]>,
    bound: Vec<f32>,
    /// Awake bodies of this step (group order).
    active: Vec<u32>,
    /// Awake bodies and the sleeping ones (corpses), in their own grids. The sleeping grid
    /// is rebuilt only when a group falls asleep or wakes.
    awake_grid: Grid,
    sleep_grid: Grid,
    sleeping: Vec<u32>,
    sleep_bound: f32,
    sleep_dirty: bool,
    /// Islands: union–find over groups, the island of each root, of each awake body.
    uf: Vec<u32>,
    island_of_group: Vec<u32>,
    local: Vec<u32>,
    stamp: Vec<u32>,
    islands: Vec<Island>,
    n_islands: usize,
}

impl World {
    pub fn new(config: Config) -> World {
        World {
            config,
            bodies: Vec::new(),
            joints: Vec::new(),
            groups: Vec::new(),
            pairs: Vec::new(),
            planes: Vec::new(),
            bound: Vec::new(),
            active: Vec::new(),
            awake_grid: Grid::default(),
            sleep_grid: Grid::default(),
            sleeping: Vec::new(),
            sleep_bound: 0.0,
            sleep_dirty: true,
            uf: Vec::new(),
            island_of_group: Vec::new(),
            local: Vec::new(),
            stamp: Vec::new(),
            islands: Vec::new(),
            n_islands: 0,
        }
    }

    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }
    pub fn body(&self, id: BodyId) -> &Body {
        &self.bodies[id as usize]
    }
    pub fn body_mut(&mut self, id: BodyId) -> &mut Body {
        &mut self.bodies[id as usize]
    }
    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }
    pub fn joint(&self, id: JointId) -> &Joint {
        &self.joints[id as usize]
    }
    /// Direct access; does not wake the ragdoll (see [`World::set_joint_target`]).
    pub fn joint_mut(&mut self, id: JointId) -> &mut Joint {
        &mut self.joints[id as usize]
    }

    /// Sets a joint's drive target and wakes its ragdoll.
    pub fn set_joint_target(&mut self, id: JointId, target: Quat) {
        self.joints[id as usize].target = target;
        self.wake(self.joints[id as usize].a);
    }
    /// Sets a joint's tone and wakes its ragdoll.
    pub fn set_joint_tone(&mut self, id: JointId, tone: Tone) {
        self.joints[id as usize].tone = tone;
        self.wake(self.joints[id as usize].a);
    }
    pub fn set_joint_limit(&mut self, id: JointId, limit: JointLimit) {
        let j = &mut self.joints[id as usize];
        j.limit = limit;
        j.cache = LimitCache::new(j.fa, j.fb, limit);
    }

    /// Adds a free body (its own group).
    pub fn add_body(&mut self, body: Body) -> BodyId {
        let g = self.new_group();
        self.add_body_in_group(body, g, 0, 0)
    }

    pub(crate) fn new_group(&mut self) -> u32 {
        let n = self.bodies.len() as u32;
        self.groups.push(Group { first: n, end: n, calm_time: 0.0, asleep: false, frozen: false });
        self.uf.push(0);
        self.island_of_group.push(0);
        (self.groups.len() - 1) as u32
    }

    /// Bodies of a group are added consecutively, right after [`World::new_group`].
    pub(crate) fn add_body_in_group(&mut self, mut body: Body, group: u32, index: u32, exclude: u64) -> BodyId {
        body.group = group;
        body.group_index = index;
        body.exclude = exclude;
        self.bodies.push(body);
        let n = self.bodies.len();
        assert_eq!(self.groups[group as usize].end as usize, n - 1, "bodies of a group are added consecutively");
        self.groups[group as usize].end = n as u32;
        self.wake_group(group);
        self.planes.push([Plane::default(); 2]);
        self.bound.push(0.0);
        self.local.push(0);
        self.stamp.push(u32::MAX);
        (n - 1) as BodyId
    }

    /// Adds a joint between bodies `a` (parent) and `b` (child) at the world point `anchor`,
    /// with the joint frame `frame` (world, X = twist axis, Z = hinge axis) in the current
    /// pose, which becomes the rest pose of the drive.
    pub fn add_joint(&mut self, a: BodyId, b: BodyId, anchor: Vec3, frame: Quat, limit: JointLimit) -> JointId {
        let (ba, bb) = (&self.bodies[a as usize], &self.bodies[b as usize]);
        let (fa, fb) = (ba.rot.conj() * frame, bb.rot.conj() * frame);
        let joint = Joint {
            a,
            b,
            ra: ba.rot.conj().rotate(anchor - ba.pos),
            rb: bb.rot.conj().rotate(anchor - bb.pos),
            fa,
            fb,
            limit,
            rest_rel: ba.rot.conj() * bb.rot,
            target: Quat::IDENTITY,
            tone: Tone::LIMP,
            cache: LimitCache::new(fa, fb, limit),
        };
        self.joints.push(joint);
        (self.joints.len() - 1) as JointId
    }

    /// Fixes a body in place (kinematic, infinite mass) or releases it.
    pub fn set_pinned(&mut self, id: BodyId, pinned: bool) {
        let b = &mut self.bodies[id as usize];
        b.pinned = pinned;
        if pinned {
            b.inv_mass = 0.0;
            b.inv_inertia = Vec3::ZERO;
            b.vel = Vec3::ZERO;
            b.omega = Vec3::ZERO;
        } else {
            b.inv_mass = 1.0 / b.mass;
            b.inv_inertia = Vec3::new(1.0 / b.inertia.x, 1.0 / b.inertia.y, 1.0 / b.inertia.z);
        }
        b.refresh_inertia();
        self.wake(id);
    }

    /// Places a body (teleport): pose and velocities, as if it had been there since the
    /// last substep. Wakes its ragdoll. A pinned body moved so each step is kinematic.
    pub fn set_body_state(&mut self, id: BodyId, pos: Vec3, rot: Quat, vel: Vec3, omega: Vec3) {
        self.wake(id);
        let b = &mut self.bodies[id as usize];
        b.pos = pos;
        b.rot = rot.normalize();
        b.prev_pos = pos;
        b.prev_rot = b.rot;
        (b.vel, b.omega) = if b.pinned { (Vec3::ZERO, Vec3::ZERO) } else { (vel, omega) };
        b.refresh_inertia();
    }

    /// An instantaneous impulse (N·s) at a world point; wakes the body's ragdoll.
    pub fn apply_impulse(&mut self, id: BodyId, world_point: Vec3, impulse: Vec3) {
        self.wake(id);
        let b = &mut self.bodies[id as usize];
        if b.pinned {
            return;
        }
        b.refresh_inertia();
        b.vel += impulse * b.inv_mass;
        let r = world_point - b.pos;
        b.omega += b.inv_i.mul(r.cross(impulse));
    }

    /// Wakes the ragdoll (group) of a body, a frozen one too.
    pub fn wake(&mut self, id: BodyId) {
        let g = self.bodies[id as usize].group;
        self.wake_group(g);
    }

    fn wake_group(&mut self, g: u32) {
        let grp = &mut self.groups[g as usize];
        grp.calm_time = 0.0;
        if grp.asleep {
            let frozen = grp.frozen;
            grp.asleep = false;
            grp.frozen = false;
            self.sleep_dirty = true;
            for b in &mut self.bodies[grp.first as usize..grp.end as usize] {
                b.asleep = false;
                if frozen && !b.pinned {
                    b.inv_mass = 1.0 / b.mass;
                    b.inv_inertia = Vec3::new(1.0 / b.inertia.x, 1.0 / b.inertia.y, 1.0 / b.inertia.z);
                    b.refresh_inertia();
                }
            }
        }
    }

    /// Puts a group to sleep: it keeps its pose and costs nothing; `frozen` makes it static
    /// collision geometry (infinite mass), otherwise an awake body touching it wakes it.
    fn sleep_group(&mut self, g: u32, frozen: bool) {
        let grp = &mut self.groups[g as usize];
        grp.asleep = true;
        grp.frozen = frozen;
        self.sleep_dirty = true;
        for b in &mut self.bodies[grp.first as usize..grp.end as usize] {
            b.asleep = true;
            b.vel = Vec3::ZERO;
            b.omega = Vec3::ZERO;
            b.prev_pos = b.pos;
            b.prev_rot = b.rot;
            if frozen {
                b.inv_mass = 0.0;
                b.inv_inertia = Vec3::ZERO;
                b.refresh_inertia();
            }
        }
    }

    /// Freezes the ragdoll (group) of a body now: it keeps its pose as static collision
    /// geometry (a corpse): falling bodies land on it as on the ground, it is never solved
    /// and costs nothing, until an explicit wake (an impulse, a new pose, a tone).
    pub fn freeze(&mut self, id: BodyId) {
        let g = self.bodies[id as usize].group;
        self.sleep_group(g, true);
    }

    /// Number of awake (simulated) bodies.
    pub fn awake_bodies(&self) -> usize {
        self.bodies.iter().filter(|b| !b.asleep).count()
    }

    /// Total kinetic plus gravitational potential energy (J), potential relative to `z = 0`.
    pub fn energy(&self) -> f32 {
        let g = self.config.gravity;
        self.bodies.iter().filter(|b| !b.pinned).map(|b| b.kinetic_energy() - b.mass * g.dot(b.pos)).sum()
    }

    /// Advances by one fixed step `config.dt` over `ground`.
    pub fn step<G: Ground + ?Sized>(&mut self, ground: &G) {
        self.broad_phase();
        if self.active.is_empty() {
            return;
        }
        self.sample_ground(ground);
        self.build_islands();
        let config = self.config;
        let islands = &mut self.islands[..self.n_islands];
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rayon::prelude::*;
            // Small work is cheaper on this thread than handed out.
            if islands.len() > 1 && self.active.len() >= 64 {
                islands.par_iter_mut().with_max_len(1).for_each(|i| i.solver.run(&config));
            } else {
                islands.iter_mut().for_each(|i| i.solver.run(&config));
            }
        }
        #[cfg(target_arch = "wasm32")]
        islands.iter_mut().for_each(|i| i.solver.run(&config));
        for isl in &self.islands[..self.n_islands] {
            for (k, &id) in isl.ids[..isl.dynamic].iter().enumerate() {
                self.bodies[id as usize] = isl.solver.bodies[k].clone();
            }
        }
        self.update_sleep();
    }

    fn collect_active(&mut self) {
        self.active.clear();
        for g in &self.groups {
            if !g.asleep {
                self.active.extend(g.first..g.end);
            }
        }
        if self.sleep_dirty {
            self.sleep_dirty = false;
            self.sleeping.clear();
            let mut r = 0.0f32;
            for g in &self.groups {
                if g.asleep {
                    for i in g.first..g.end {
                        let b = &self.bodies[i as usize];
                        r = r.max(b.half_len + b.radius + 0.02);
                        self.sleeping.push(i);
                    }
                }
            }
            self.sleep_bound = r;
            let bodies = &self.bodies;
            self.sleep_grid.build(&self.sleeping, (2.0 * r).max(0.25), |i| bodies[i as usize].pos);
        }
    }

    /// Capsule pairs that may touch within this step: bounding spheres (with the motion of
    /// the step as margin), awake bodies in a hashed grid of their own, sleeping ones in a
    /// grid rebuilt only when that set changes. An awake body touching a sleeping (not
    /// frozen) one wakes it; a frozen corpse's capsules are one-sided obstacles.
    fn broad_phase(&mut self) {
        self.collect_active();
        // An awake body touching a sleeper wakes its group (it joins from this step).
        let mut woke = false;
        for &i in &self.active {
            let a = &self.bodies[i as usize];
            let r = a.half_len + a.radius + a.vel.length() * self.config.dt + 0.02;
            self.bound[i as usize] = r;
        }
        for k in 0..self.active.len() {
            let i = self.active[k] as usize;
            let (p, r) = (self.bodies[i].pos, self.bound[i] + self.sleep_bound);
            let mut wake = u32::MAX;
            let (bodies, groups) = (&self.bodies, &self.groups);
            self.sleep_grid.query(p, r, |j| {
                let b = &bodies[j as usize];
                if wake == u32::MAX && !groups[b.group as usize].frozen && b.asleep {
                    let rr = self.bound[i] + b.half_len + b.radius + 0.02;
                    if (p - b.pos).length_sq() < rr * rr {
                        wake = b.group;
                    }
                }
            });
            if wake != u32::MAX {
                self.wake_group(wake);
                woke = true;
            }
        }
        if woke {
            self.collect_active();
            for &i in &self.active {
                let a = &self.bodies[i as usize];
                self.bound[i as usize] = a.half_len + a.radius + a.vel.length() * self.config.dt + 0.02;
            }
        }
        self.pairs.clear();
        if self.active.is_empty() {
            return;
        }
        let max_r = self.active.iter().map(|&i| self.bound[i as usize]).fold(0.0f32, f32::max);
        // Spheres in non-neighbouring cells cannot touch.
        let bodies = &self.bodies;
        self.awake_grid.build(&self.active, (2.0 * max_r).max(0.25), |i| bodies[i as usize].pos);
        for k in 0..self.active.len() {
            let i = self.active[k];
            let a = &self.bodies[i as usize];
            let (bound, pairs, grid) = (&self.bound, &mut self.pairs, &self.awake_grid);
            grid.query(a.pos, bound[i as usize] + max_r, |j| {
                if j <= i {
                    return;
                }
                let b = &bodies[j as usize];
                if a.pinned && b.pinned {
                    return;
                }
                if a.group == b.group && (a.exclude >> b.group_index) & 1 == 1 {
                    return;
                }
                let r = bound[i as usize] + bound[j as usize];
                if (a.pos - b.pos).length_sq() < r * r {
                    pairs.push((i, j));
                }
            });
            // Frozen corpses (static); sleepers met only now wake for the next step.
            let groups = &self.groups;
            let r0 = bound[i as usize];
            self.sleep_grid.query(a.pos, r0 + self.sleep_bound, |j| {
                let b = &bodies[j as usize];
                if !b.asleep || !groups[b.group as usize].frozen || a.pinned {
                    return;
                }
                let r = r0 + b.half_len + b.radius + 0.02;
                if (a.pos - b.pos).length_sq() < r * r {
                    pairs.push((i, j));
                }
            });
        }
    }

    /// One tangent plane of the ground per capsule end, for the whole step.
    fn sample_ground<G: Ground + ?Sized>(&mut self, ground: &G) {
        let dt = self.config.dt;
        for &i in &self.active {
            let b = &self.bodies[i as usize];
            if b.pinned {
                continue;
            }
            let (e0, e1) = b.ends();
            for (k, e) in [e0, e1].into_iter().enumerate() {
                let gz = ground.height(e.x, e.y);
                // Well above the ground no contact can happen this step: skip the normal.
                let n = if e.z - gz > b.radius + b.vel.length() * dt + 0.1 { Vec3::Z } else { ground.normal(e.x, e.y) };
                self.planes[i as usize][k] = Plane { n, d: n.dot(Vec3::new(e.x, e.y, gz)) };
            }
        }
    }

    fn find(uf: &mut [u32], mut g: u32) -> u32 {
        while uf[g as usize] != g {
            uf[g as usize] = uf[uf[g as usize] as usize];
            g = uf[g as usize];
        }
        g
    }

    /// Splits the awake bodies into islands (groups joined by contacts or joints) and
    /// gathers each island's bodies, joints and pairs into its own buffers.
    fn build_islands(&mut self) {
        let awake = |b: &Body| !b.asleep;
        for (g, grp) in self.groups.iter().enumerate() {
            if !grp.asleep {
                self.uf[g] = g as u32;
            }
        }
        let union = |uf: &mut Vec<u32>, a: u32, b: u32| {
            let (ra, rb) = (Self::find(uf, a), Self::find(uf, b));
            if ra != rb {
                uf[ra.max(rb) as usize] = ra.min(rb);
            }
        };
        for &(i, j) in &self.pairs {
            let (a, b) = (&self.bodies[i as usize], &self.bodies[j as usize]);
            if awake(b) && a.group != b.group {
                union(&mut self.uf, a.group, b.group);
            }
        }
        for j in &self.joints {
            let (a, b) = (&self.bodies[j.a as usize], &self.bodies[j.b as usize]);
            if awake(a) && awake(b) && a.group != b.group {
                union(&mut self.uf, a.group, b.group);
            }
        }
        self.n_islands = 0;
        for g in 0..self.groups.len() {
            if self.groups[g].asleep {
                continue;
            }
            let root = Self::find(&mut self.uf, g as u32);
            if root == g as u32 {
                self.island_of_group[g] = self.n_islands as u32;
                if self.islands.len() == self.n_islands {
                    self.islands.push(Island::default());
                }
                let isl = &mut self.islands[self.n_islands];
                isl.ids.clear();
                isl.solver.bodies.clear();
                isl.solver.joints.clear();
                isl.solver.pairs.clear();
                isl.solver.planes.clear();
                self.n_islands += 1;
            } else {
                self.island_of_group[g] = self.island_of_group[root as usize];
            }
        }
        // Awake bodies (groups in order, roots before their members' later groups).
        for &i in &self.active {
            let b = &self.bodies[i as usize];
            let isl = &mut self.islands[self.island_of_group[b.group as usize] as usize];
            self.local[i as usize] = isl.ids.len() as u32;
            isl.ids.push(i);
            isl.solver.bodies.push(b.clone());
            isl.solver.planes.push(self.planes[i as usize]);
        }
        for isl in &mut self.islands[..self.n_islands] {
            isl.dynamic = isl.ids.len();
        }
        for j in &self.joints {
            let a = &self.bodies[j.a as usize];
            if !awake(a) {
                continue;
            }
            let k = self.island_of_group[a.group as usize];
            let isl = &mut self.islands[k as usize];
            let lb = Self::local_of(&self.bodies, &mut self.local, &mut self.stamp, isl, k, j.b);
            let mut joint = *j;
            joint.a = self.local[j.a as usize];
            joint.b = lb;
            isl.solver.joints.push(joint);
        }
        for &(i, j) in &self.pairs {
            let k = self.island_of_group[self.bodies[i as usize].group as usize];
            let isl = &mut self.islands[k as usize];
            let lj = Self::local_of(&self.bodies, &mut self.local, &mut self.stamp, isl, k, j);
            isl.solver.pairs.push((self.local[i as usize], lj));
        }
        for &i in &self.sleeping {
            self.stamp[i as usize] = u32::MAX;
        }
    }

    /// Local index of body `id` in island `k`: an awake body's own, or a sleeping body
    /// copied in as a static partner (once per island).
    fn local_of(bodies: &[Body], local: &mut [u32], stamp: &mut [u32], isl: &mut Island, k: u32, id: u32) -> u32 {
        let b = &bodies[id as usize];
        if !b.asleep {
            return local[id as usize];
        }
        if stamp[id as usize] != k {
            stamp[id as usize] = k;
            local[id as usize] = isl.ids.len() as u32;
            isl.ids.push(id);
            isl.solver.bodies.push(b.clone());
        }
        local[id as usize]
    }

    /// Groups that stayed calm for `sleep_time` fall asleep.
    fn update_sleep(&mut self) {
        let (limit, time, dt) = (self.config.sleep_energy, self.config.sleep_time, self.config.dt);
        if limit <= 0.0 {
            return;
        }
        for g in 0..self.groups.len() {
            let grp = self.groups[g];
            if grp.asleep || grp.first == grp.end {
                continue;
            }
            let (mut e, mut m) = (0.0, 0.0);
            for b in &self.bodies[grp.first as usize..grp.end as usize] {
                if !b.pinned {
                    e += b.kinetic_energy();
                    m += b.mass;
                }
            }
            let calm = m == 0.0 || e < limit * m;
            let grp = &mut self.groups[g];
            grp.calm_time = if calm { grp.calm_time + dt } else { 0.0 };
            if grp.calm_time >= time {
                self.sleep_group(g as u32, false);
            }
        }
    }
    fn frame_a(&self, j: &Joint) -> Quat {
        self.bodies[j.a as usize].rot * j.fa
    }
    fn frame_b(&self, j: &Joint) -> Quat {
        self.bodies[j.b as usize].rot * j.fb
    }
    /// Measured state of joint `id`.
    pub fn joint_state(&self, id: JointId) -> JointState {
        let j = &self.joints[id as usize];
        let (ba, bb) = (&self.bodies[j.a as usize], &self.bodies[j.b as usize]);
        let separation = ((bb.pos + bb.rot.rotate(j.rb)) - (ba.pos + ba.rot.rotate(j.ra))).length();
        let (qa, qb) = (self.frame_a(j), self.frame_b(j));
        let (angle, twist) = match j.limit {
            JointLimit::Hinge { .. } => {
                let n = qa.rotate(Vec3::Z);
                let (x1, x2) = (qa.rotate(Vec3::X), qb.rotate(Vec3::X));
                (signed_angle(n, x1, x2), n.dot(qb.rotate(Vec3::Z)).clamp(-1.0, 1.0).acos())
            }
            JointLimit::Ball { cone_axis, .. } => {
                let phi = qa.rotate(cone_axis).dot(qb.rotate(Vec3::X)).clamp(-1.0, 1.0).acos();
                (phi, twist_angle(qa, qb).unwrap_or(0.0))
            }
        };
        let want_b = ba.rot * j.rest_rel * j.target;
        let drive_error = (want_b * bb.rot.conj()).angle();
        JointState { separation, angle, twist, drive_error }
    }
}

impl Solver {
    /// The substeps of one step: integrate, contacts, joints, velocities.
    fn run(&mut self, config: &Config) {
        let n = config.substeps.max(1);
        let h = config.dt / n as f32;
        // Ground and pair contacts each substep: two per body plus one per pair.
        self.contacts.clear();
        self.contacts.reserve(2 * self.bodies.len() + self.pairs.len());
        for _ in 0..n {
            self.integrate(config, h);
            self.contacts.clear();
            // Contacts first, joints last: a body ends each substep with its joints intact
            // (light end bodies such as toes would otherwise be torn off by the ground).
            self.solve_contacts(config);
            for _ in 0..config.joint_iterations.max(1) {
                self.solve_joints(h);
            }
            self.update_velocities(h);
            self.solve_velocities(config, h);
        }
    }

    fn integrate(&mut self, config: &Config, h: f32) {
        let g = config.gravity;
        let damp = (1.0 - config.air_damping * h).max(0.0);
        for b in &mut self.bodies {
            if b.asleep {
                continue;
            }
            b.prev_pos = b.pos;
            b.prev_rot = b.rot;
            if b.pinned {
                continue;
            }
            b.vel = (b.vel + g * h) * damp;
            b.omega = b.omega * damp;
            b.pos += b.vel * h;
            b.rot = b.rot.add_rotvec(b.omega * h);
            b.refresh_inertia();
        }
    }

    fn update_velocities(&mut self, h: f32) {
        let inv_h = 1.0 / h;
        for b in &mut self.bodies {
            if b.pinned || b.asleep {
                continue;
            }
            b.rot = b.rot.normalize();
            b.vel = (b.pos - b.prev_pos) * inv_h;
            let dq = b.rot * b.prev_rot.conj();
            let w = Vec3::new(dq.x, dq.y, dq.z) * (2.0 * inv_h);
            b.omega = if dq.w >= 0.0 { w } else { -w };
        }
    }
    fn solve_joints(&mut self, h: f32) {
        let inv_h2 = 1.0 / (h * h);
        for i in 0..self.joints.len() {
            let joint = self.joints[i];
            let (a, b) = (joint.a, joint.b);
            if self.bodies[a as usize].asleep {
                continue;
            }
            let k = &joint.cache;
            // Anchors coincide (hard); the angular parts follow so the limits have the last
            // word within a pass (the next pass restores the anchors).
            self.solve_anchor(&joint);
            // Muscle drive: soft angular constraint towards the target (compliance 1/k).
            if joint.tone.stiffness > 0.0 {
                let (qa, qb) = (self.bodies[a as usize].rot, self.bodies[b as usize].rot);
                let e = qa * (joint.rest_rel * joint.target) * qb.conj(); // rotation b still has to do
                let v = Vec3::new(e.x, e.y, e.z);
                // Small errors: 2·sin(θ/2)·axis is the rotation vector to within 1 %.
                let err = if v.length_sq() < 0.04 { v * if e.w < 0.0 { -2.0 } else { 2.0 } } else { e.to_rotvec() };
                let alpha = inv_h2 / joint.tone.stiffness;
                apply_rotation(&mut self.bodies, a, b, -err, alpha);
            }
            // Limits (hard), corrected only when violated.
            match joint.limit {
                JointLimit::Hinge { lo, hi } => {
                    let (qa, qb) = (self.bodies[a as usize].rot, self.bodies[b as usize].rot);
                    let (za, zb) = (qa.rotate(k.za), qb.rotate(k.zb));
                    // The angle is measured before the (small) alignment; it is only used to
                    // decide whether the limit needs work.
                    let (x1, x2) = (qa.rotate(k.xa), qb.rotate(k.xb));
                    apply_rotation(&mut self.bodies, a, b, za.cross(zb), 0.0);
                    let n = za;
                    let (s, c) = (x1.cross(x2).dot(n), x1.dot(x2));
                    // cos(φ − mid) ≥ cos(half) ⇔ φ within [lo, hi] (no trigonometry).
                    if c * k.cos_mid + s * k.sin_mid < k.cos_half {
                        let (qa, qb) = (self.bodies[a as usize].rot, self.bodies[b as usize].rot);
                        let (n, x1, x2) = (qa.rotate(k.za), qa.rotate(k.xa), qb.rotate(k.xb));
                        if let Some(c) = limit_angle(n, x1, x2, lo, hi) {
                            apply_rotation(&mut self.bodies, a, b, c, 0.0);
                        }
                    }
                }
                JointLimit::Ball { cone, twist_lo, twist_hi, .. } => {
                    let (qa, qb) = (self.bodies[a as usize].rot, self.bodies[b as usize].rot);
                    let (ca, xb) = (qa.rotate(k.cone_a), qb.rotate(k.xb));
                    if ca.dot(xb) < k.cos_cone {
                        let n = ca.cross(xb).normalize_or_zero();
                        if n != Vec3::ZERO {
                            let target = Quat::from_axis_angle(n, cone).rotate(ca);
                            apply_rotation(&mut self.bodies, a, b, target.cross(xb), 0.0);
                        }
                    }
                    let (qa, qb) = (self.bodies[a as usize].rot * joint.fa, self.bodies[b as usize].rot * joint.fb);
                    // Twist of the swing–twist split about X: 2·atan2(x, w) is within the
                    // limits ⇔ x / w is within tan(limit / 2).
                    let r = qa.conj() * qb;
                    let (x, w) = if r.w < 0.0 { (-r.x, -r.w) } else { (r.x, r.w) };
                    if (x < k.tan_twist_lo * w || x > k.tan_twist_hi * w)
                        && let Some(c) = twist_correction(qa, qb, twist_lo, twist_hi)
                    {
                        apply_rotation(&mut self.bodies, a, b, c, 0.0);
                    }
                }
            }
        }
    }

    fn solve_anchor(&mut self, joint: &Joint) {
        let (ba, bb) = (&self.bodies[joint.a as usize], &self.bodies[joint.b as usize]);
        let (ra, rb) = (ba.rot.rotate(joint.ra), bb.rot.rotate(joint.rb));
        let corr = (bb.pos + rb) - (ba.pos + ra);
        apply_position(&mut self.bodies, joint.a, joint.b, ra, rb, corr, 0.0);
    }

    fn solve_contacts(&mut self, config: &Config) {
        let mu_s = config.static_friction;
        // Ground: both capsule end spheres against the step's tangent planes.
        for i in 0..self.bodies.len() {
            if self.bodies[i].pinned || self.bodies[i].asleep {
                continue;
            }
            for end in 0..2 {
                let b = &self.bodies[i];
                let p = self.planes[i][end];
                // Cheap reject on the centre: the ends are within half_len of it.
                if p.n.dot(b.pos) - p.d > b.radius + b.half_len {
                    continue;
                }
                let (e0, e1) = b.ends();
                let e = if end == 0 { e0 } else { e1 };
                let depth = b.radius - (p.n.dot(e) - p.d);
                if depth <= 0.0 {
                    continue;
                }
                let n = p.n;
                let r = e - n * b.radius - b.pos;
                let ra = b.rot.conj().rotate(r);
                let lambda_n = apply_position(&mut self.bodies, i as u32, STATIC, r, Vec3::ZERO, n * depth, 0.0);
                self.static_friction(i as u32, STATIC, ra, Vec3::ZERO, n, lambda_n, mu_s);
                if self.contacts.len() < self.contacts.capacity() {
                    self.contacts.push(Contact { a: i as u32, b: STATIC, n, ra, rb: Vec3::ZERO, lambda_n });
                }
            }
        }
        // Capsule pairs.
        for k in 0..self.pairs.len() {
            let (i, j) = self.pairs[k];
            let (a, b) = (&self.bodies[i as usize], &self.bodies[j as usize]);
            let (a0, a1) = a.ends();
            let (b0, b1) = b.ends();
            let (s, t) = closest_segment_params(a0, a1 - a0, b0, b1 - b0);
            let (pa, pb) = (a0 + (a1 - a0) * s, b0 + (b1 - b0) * t);
            let d = pa - pb;
            let dist = d.length();
            let depth = a.radius + b.radius - dist;
            if depth <= 0.0 || dist < 1e-6 {
                continue;
            }
            let n = d * (1.0 / dist);
            let ra = pa - n * a.radius - a.pos;
            let rb = pb + n * b.radius - b.pos;
            let (la, lb) = (a.rot.conj().rotate(ra), b.rot.conj().rotate(rb));
            let lambda_n = apply_position(&mut self.bodies, i, j, ra, rb, n * depth, 0.0);
            self.static_friction(i, j, la, lb, n, lambda_n, mu_s);
            if self.contacts.len() < self.contacts.capacity() {
                self.contacts.push(Contact { a: i, b: j, n, ra: la, rb: lb, lambda_n });
            }
        }
    }

    /// Static friction: undo the tangential slip of the contact points within this substep
    /// while the needed force stays within the friction cone.
    #[allow(clippy::too_many_arguments)]
    fn static_friction(&mut self, a: u32, b: u32, la: Vec3, lb: Vec3, n: Vec3, lambda_n: f32, mu_s: f32) {
        let slip = |bd: &Body, l: Vec3| (bd.pos + bd.rot.rotate(l)) - (bd.prev_pos + bd.prev_rot.rotate(l));
        let ba = &self.bodies[a as usize];
        let ra = ba.rot.rotate(la);
        let mut dp = slip(ba, la);
        let mut rb = Vec3::ZERO;
        if b != STATIC {
            let bb = &self.bodies[b as usize];
            rb = bb.rot.rotate(lb);
            dp -= slip(bb, lb);
        }
        let dt = dp - n * n.dot(dp);
        let c = dt.length();
        if c < 1e-7 {
            return;
        }
        let t = dt * (1.0 / c);
        let w =
            self.bodies[a as usize].w_pos(ra, t) + if b != STATIC { self.bodies[b as usize].w_pos(rb, t) } else { 0.0 };
        if w <= 0.0 || c / w > mu_s * lambda_n {
            return;
        }
        apply_position(&mut self.bodies, a, b, ra, rb, -dt, 0.0);
    }

    fn solve_velocities(&mut self, config: &Config, h: f32) {
        let mu_d = config.dynamic_friction;
        for k in 0..self.contacts.len() {
            let c = self.contacts[k];
            let ba = &self.bodies[c.a as usize];
            let ra = ba.rot.rotate(c.ra);
            let mut v = ba.vel + ba.omega.cross(ra);
            let mut rb = Vec3::ZERO;
            if c.b != STATIC {
                let bb = &self.bodies[c.b as usize];
                rb = bb.rot.rotate(c.rb);
                v -= bb.vel + bb.omega.cross(rb);
            }
            let vn = c.n.dot(v);
            let vt = v - c.n * vn;
            let vt_len = vt.length();
            let mut dv = Vec3::ZERO;
            if vt_len > 1e-6 {
                dv -= vt * ((mu_d * c.lambda_n / h).min(vt_len) / vt_len);
            }
            if vn < 0.0 {
                dv -= c.n * vn; // restitution 0: bodies do not bounce
            }
            apply_velocity(&mut self.bodies, c.a, c.b, ra, rb, dv);
        }
        // Joint damping (muscle tone plus passive tissue).
        let passive = config.passive_damping;
        for j in 0..self.joints.len() {
            let (a, b, d) = {
                let jt = &self.joints[j];
                (jt.a, jt.b, jt.tone.damping + passive)
            };
            let (ba, bb) = (&self.bodies[a as usize], &self.bodies[b as usize]);
            if ba.asleep {
                continue;
            }
            let rel = bb.omega - ba.omega;
            let l = rel.length();
            if l < 1e-7 {
                continue;
            }
            let n = rel * (1.0 / l);
            let w = ba.w_rot(n) + bb.w_rot(n);
            if w <= 0.0 {
                continue;
            }
            let p = rel * (d * h).min(1.0 / w);
            let (ia, ib) = (ba.inv_i.mul(p), bb.inv_i.mul(p));
            self.bodies[a as usize].omega += ia;
            self.bodies[b as usize].omega -= ib;
        }
    }
}

/// Positional XPBD correction: moves the point `ra` of body `a` by `corr` relative to the
/// point `rb` of body `b` (world offsets). Returns the Lagrange multiplier (N·s²).
fn apply_position(bodies: &mut [Body], a: u32, b: u32, ra: Vec3, rb: Vec3, corr: Vec3, alpha: f32) -> f32 {
    let c2 = corr.length_sq();
    if c2 < 1e-18 {
        return 0.0;
    }
    let c = c2.sqrt();
    let n = corr * (1.0 / c);
    let wa = if a != STATIC { bodies[a as usize].w_pos(ra, n) } else { 0.0 };
    let wb = if b != STATIC { bodies[b as usize].w_pos(rb, n) } else { 0.0 };
    let w = wa + wb + alpha;
    if w <= 0.0 {
        return 0.0;
    }
    let dl = c / w;
    let p = n * dl;
    if a != STATIC {
        let ba = &mut bodies[a as usize];
        ba.pos += p * ba.inv_mass;
        let dw = ba.inv_i.mul(ra.cross(p));
        ba.rot = ba.rot.add_rotvec_raw(dw);
    }
    if b != STATIC {
        let bb = &mut bodies[b as usize];
        bb.pos -= p * bb.inv_mass;
        let dw = bb.inv_i.mul(rb.cross(p));
        bb.rot = bb.rot.add_rotvec_raw(-dw);
    }
    dl
}

/// Angular XPBD correction: turns body `a` by the rotation vector `corr` relative to `b`.
fn apply_rotation(bodies: &mut [Body], a: u32, b: u32, corr: Vec3, alpha: f32) {
    let t2 = corr.length_sq();
    if t2 < 1e-18 {
        return;
    }
    let theta = t2.sqrt();
    let n = corr * (1.0 / theta);
    let (ba, bb) = (&bodies[a as usize], &bodies[b as usize]);
    let w = ba.w_rot(n) + bb.w_rot(n) + alpha;
    if w <= 0.0 {
        return;
    }
    let p = n * (theta / w);
    let (da, db) = (ba.inv_i.mul(p), bb.inv_i.mul(p));
    bodies[a as usize].rot = bodies[a as usize].rot.add_rotvec_raw(da);
    bodies[b as usize].rot = bodies[b as usize].rot.add_rotvec_raw(-db);
}

/// Velocity change `dv` of point `ra` of `a` relative to point `rb` of `b`, as an impulse.
fn apply_velocity(bodies: &mut [Body], a: u32, b: u32, ra: Vec3, rb: Vec3, dv: Vec3) {
    let l = dv.length();
    if l < 1e-9 {
        return;
    }
    let n = dv * (1.0 / l);
    let wa = if a != STATIC { bodies[a as usize].w_pos(ra, n) } else { 0.0 };
    let wb = if b != STATIC { bodies[b as usize].w_pos(rb, n) } else { 0.0 };
    if wa + wb <= 0.0 {
        return;
    }
    let p = dv * (1.0 / (wa + wb));
    if a != STATIC {
        let ba = &mut bodies[a as usize];
        ba.vel += p * ba.inv_mass;
        ba.omega += ba.inv_i.mul(ra.cross(p));
    }
    if b != STATIC {
        let bb = &mut bodies[b as usize];
        bb.vel -= p * bb.inv_mass;
        bb.omega -= bb.inv_i.mul(rb.cross(p));
    }
}

/// Signed angle from `n1` to `n2` about the unit axis `n` (both perpendicular to `n`).
fn signed_angle(n: Vec3, n1: Vec3, n2: Vec3) -> f32 {
    n1.cross(n2).dot(n).atan2(n1.dot(n2))
}

/// Correction (rotation of the parent) keeping the angle from `n1` to `n2` about `n` within
/// `[lo, hi]` (Müller et al. 2020, algorithm 3).
fn limit_angle(n: Vec3, n1: Vec3, n2: Vec3, lo: f32, hi: f32) -> Option<Vec3> {
    let phi = signed_angle(n, n1, n2);
    if phi >= lo && phi <= hi {
        return None;
    }
    let target = Quat::from_axis_angle(n, phi.clamp(lo, hi)).rotate(n1);
    Some(target.cross(n2))
}

fn twist_axes(qa: Quat, qb: Quat) -> Option<(Vec3, Vec3, Vec3)> {
    let n = (qa.rotate(Vec3::X) + qb.rotate(Vec3::X)).normalize_or_zero();
    if n == Vec3::ZERO {
        return None;
    }
    let (ya, yb) = (qa.rotate(Vec3::Y), qb.rotate(Vec3::Y));
    let b1 = (ya - n * n.dot(ya)).normalize_or_zero();
    let b2 = (yb - n * n.dot(yb)).normalize_or_zero();
    if b1 == Vec3::ZERO || b2 == Vec3::ZERO {
        return None;
    }
    Some((n, b1, b2))
}

fn twist_angle(qa: Quat, qb: Quat) -> Option<f32> {
    twist_axes(qa, qb).map(|(n, b1, b2)| signed_angle(n, b1, b2))
}

fn twist_correction(qa: Quat, qb: Quat, lo: f32, hi: f32) -> Option<Vec3> {
    twist_axes(qa, qb).and_then(|(n, b1, b2)| limit_angle(n, b1, b2, lo, hi))
}
