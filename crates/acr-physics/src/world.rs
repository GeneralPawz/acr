//! The XPBD solver: rigid capsule bodies, joints with limits and muscle drives, contacts
//! against a ground height field and between capsules.
//!
//! One position iteration per substep (Müller et al. 2020, "Detailed Rigid Body Simulation
//! with Extended Position Based Dynamics"): integrate, solve position constraints, derive
//! velocities, solve velocity constraints (friction, restitution, damping).

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
    pub radius: f32,
    pub half_len: f32,
    pinned: bool,
    /// Collision group (a ragdoll); bodies of one group skip the pairs in `exclude`.
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
            inv_inertia: Vec3::new(1.0 / inertia.x, 1.0 / inertia.y, 1.0 / inertia.z),
            radius,
            half_len,
            pinned: false,
            group: STATIC,
            group_index: 0,
            exclude: 0,
        }
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
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

    fn inv_inertia_world(&self, v: Vec3) -> Vec3 {
        self.rot.rotate(self.inv_inertia.mul_elem(self.rot.conj().rotate(v)))
    }

    /// Generalized inverse mass for a positional correction along unit `n` at offset `r`.
    fn w_pos(&self, r: Vec3, n: Vec3) -> f32 {
        let rn = r.cross(n);
        self.inv_mass + rn.dot(self.inv_inertia_world(rn))
    }

    /// Generalized inverse mass for a rotation about unit `n`.
    fn w_rot(&self, n: Vec3) -> f32 {
        n.dot(self.inv_inertia_world(n))
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
    /// about X in `[twist_lo, twist_hi]`.
    Ball { cone_axis: Vec3, cone: f32, twist_lo: f32, twist_hi: f32 },
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
    pub limit: JointLimit,
    /// `rot_a⁻¹ · rot_b` at rest.
    pub rest_rel: Quat,
    /// Drive target: the child's rotation relative to its rest pose, in the child's frame
    /// (`rot_b = rot_a · rest_rel · target`). Identity = rest pose.
    pub target: Quat,
    pub tone: Tone,
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
    /// Joint passes per substep (contacts get one): a second pass keeps limits and anchors
    /// together under violent spins.
    pub joint_iterations: u32,
    pub gravity: Vec3,
    /// Static and dynamic friction with the ground and between capsules.
    pub static_friction: f32,
    pub dynamic_friction: f32,
    /// Passive joint damping (N·m·s/rad, soft tissue), added to every tone.
    pub passive_damping: f32,
    /// Linear and angular air damping (1/s).
    pub air_damping: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            dt: 1.0 / 60.0,
            substeps: 8,
            joint_iterations: 2,
            gravity: Vec3::new(0.0, 0.0, -9.81),
            static_friction: 0.8,
            dynamic_friction: 0.6,
            passive_damping: 0.5,
            air_damping: 0.05,
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

/// A set of bodies and joints stepped together. Allocates only when bodies or joints are
/// added; [`World::step`] does not allocate.
pub struct World {
    pub config: Config,
    bodies: Vec<Body>,
    joints: Vec<Joint>,
    contacts: Vec<Contact>,
    pairs: Vec<(u32, u32)>,
    next_group: u32,
}

impl World {
    pub fn new(config: Config) -> World {
        World { config, bodies: Vec::new(), joints: Vec::new(), contacts: Vec::new(), pairs: Vec::new(), next_group: 0 }
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
    pub fn joint_mut(&mut self, id: JointId) -> &mut Joint {
        &mut self.joints[id as usize]
    }

    /// Adds a free body (its own collision group).
    pub fn add_body(&mut self, body: Body) -> BodyId {
        let g = self.new_group();
        self.add_body_in_group(body, g, 0, 0)
    }

    pub(crate) fn new_group(&mut self) -> u32 {
        self.next_group += 1;
        self.next_group - 1
    }

    pub(crate) fn add_body_in_group(&mut self, mut body: Body, group: u32, index: u32, exclude: u64) -> BodyId {
        body.group = group;
        body.group_index = index;
        body.exclude = exclude;
        self.bodies.push(body);
        let n = self.bodies.len();
        // Capacity for the step (no allocation inside it): two ground contacts per body and
        // up to 16 capsule pairs per body.
        self.pairs.reserve((16 * n).saturating_sub(self.pairs.capacity()));
        self.contacts.reserve((18 * n).saturating_sub(self.contacts.capacity()));
        (n - 1) as BodyId
    }

    /// Adds a joint between bodies `a` (parent) and `b` (child) at the world point `anchor`,
    /// with the joint frame `frame` (world, X = twist axis, Z = hinge axis) in the current
    /// pose, which becomes the rest pose of the drive.
    pub fn add_joint(&mut self, a: BodyId, b: BodyId, anchor: Vec3, frame: Quat, limit: JointLimit) -> JointId {
        let (ba, bb) = (&self.bodies[a as usize], &self.bodies[b as usize]);
        let joint = Joint {
            a,
            b,
            ra: ba.rot.conj().rotate(anchor - ba.pos),
            rb: bb.rot.conj().rotate(anchor - bb.pos),
            fa: ba.rot.conj() * frame,
            fb: bb.rot.conj() * frame,
            limit,
            rest_rel: ba.rot.conj() * bb.rot,
            target: Quat::IDENTITY,
            tone: Tone::LIMP,
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
    }

    /// An instantaneous impulse (N·s) at a world point.
    pub fn apply_impulse(&mut self, id: BodyId, world_point: Vec3, impulse: Vec3) {
        let b = &mut self.bodies[id as usize];
        if b.pinned {
            return;
        }
        b.vel += impulse * b.inv_mass;
        let r = world_point - b.pos;
        b.omega += b.inv_inertia_world(r.cross(impulse));
    }

    /// Total kinetic plus gravitational potential energy (J), potential relative to `z = 0`.
    pub fn energy(&self) -> f32 {
        let g = self.config.gravity;
        self.bodies.iter().filter(|b| !b.pinned).map(|b| b.kinetic_energy() - b.mass * g.dot(b.pos)).sum()
    }

    /// Advances by one fixed step `config.dt` over `ground`.
    pub fn step<G: Ground + ?Sized>(&mut self, ground: &G) {
        let n = self.config.substeps.max(1);
        let h = self.config.dt / n as f32;
        self.broad_phase();
        for _ in 0..n {
            self.integrate(h);
            self.contacts.clear();
            // Contacts first, joints last: a body ends each substep with its joints intact
            // (light end bodies such as toes would otherwise be torn off by the ground).
            self.solve_contacts(h, ground);
            for _ in 0..self.config.joint_iterations.max(1) {
                self.solve_joints(h);
            }
            self.update_velocities(h);
            self.solve_velocities(h);
        }
    }

    /// Capsule pairs that may touch within this step (bounding spheres with a margin).
    fn broad_phase(&mut self) {
        self.pairs.clear();
        let cap = self.pairs.capacity();
        let dt = self.config.dt;
        let bodies = &self.bodies;
        'outer: for i in 0..bodies.len() {
            let a = &bodies[i];
            let ra = a.half_len + a.radius + a.vel.length() * dt + 0.02;
            for (j, b) in bodies.iter().enumerate().skip(i + 1) {
                if a.pinned && b.pinned {
                    continue;
                }
                if a.group == b.group && (a.exclude >> b.group_index) & 1 == 1 {
                    continue;
                }
                let rb = b.half_len + b.radius + b.vel.length() * dt + 0.02;
                if (a.pos - b.pos).length_sq() < (ra + rb) * (ra + rb) {
                    if self.pairs.len() == cap {
                        break 'outer; // never allocate in the step
                    }
                    self.pairs.push((i as u32, j as u32));
                }
            }
        }
    }

    fn integrate(&mut self, h: f32) {
        let g = self.config.gravity;
        let damp = (1.0 - self.config.air_damping * h).max(0.0);
        for b in &mut self.bodies {
            b.prev_pos = b.pos;
            b.prev_rot = b.rot;
            if b.pinned {
                continue;
            }
            b.vel = (b.vel + g * h) * damp;
            b.omega = b.omega * damp;
            b.pos += b.vel * h;
            b.rot = b.rot.add_rotvec(b.omega * h);
        }
    }

    fn update_velocities(&mut self, h: f32) {
        let inv_h = 1.0 / h;
        for b in &mut self.bodies {
            if b.pinned {
                continue;
            }
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
            // Anchors coincide (hard), before and after the angular parts.
            self.solve_anchor(&joint);
            // Muscle drive: soft angular constraint towards the target (compliance 1/k).
            if joint.tone.stiffness > 0.0 {
                let (qa, qb) = (self.bodies[a as usize].rot, self.bodies[b as usize].rot);
                let want_b = qa * joint.rest_rel * joint.target;
                let err = (want_b * qb.conj()).to_rotvec(); // rotation b still has to do
                let alpha = inv_h2 / joint.tone.stiffness;
                apply_rotation(&mut self.bodies, a, b, -err, alpha);
            }
            // Limits (hard).
            match joint.limit {
                JointLimit::Hinge { lo, hi } => {
                    let (qa, qb) = (self.frame_a(&joint), self.frame_b(&joint));
                    let (za, zb) = (qa.rotate(Vec3::Z), qb.rotate(Vec3::Z));
                    apply_rotation(&mut self.bodies, a, b, za.cross(zb), 0.0);
                    let (qa, qb) = (self.frame_a(&joint), self.frame_b(&joint));
                    let n = qa.rotate(Vec3::Z);
                    if let Some(c) = limit_angle(n, qa.rotate(Vec3::X), qb.rotate(Vec3::X), lo, hi) {
                        apply_rotation(&mut self.bodies, a, b, c, 0.0);
                    }
                }
                JointLimit::Ball { cone_axis, cone, twist_lo, twist_hi } => {
                    let (qa, qb) = (self.frame_a(&joint), self.frame_b(&joint));
                    let (ca, xb) = (qa.rotate(cone_axis), qb.rotate(Vec3::X));
                    let phi = ca.dot(xb).clamp(-1.0, 1.0).acos();
                    if phi > cone {
                        let n = ca.cross(xb).normalize_or_zero();
                        if n != Vec3::ZERO {
                            let target = Quat::from_axis_angle(n, cone).rotate(ca);
                            apply_rotation(&mut self.bodies, a, b, target.cross(xb), 0.0);
                        }
                    }
                    let (qa, qb) = (self.frame_a(&joint), self.frame_b(&joint));
                    if let Some(c) = twist_correction(qa, qb, twist_lo, twist_hi) {
                        apply_rotation(&mut self.bodies, a, b, c, 0.0);
                    }
                }
            }
            self.solve_anchor(&joint);
        }
    }

    fn solve_anchor(&mut self, joint: &Joint) {
        let (ba, bb) = (&self.bodies[joint.a as usize], &self.bodies[joint.b as usize]);
        let (ra, rb) = (ba.rot.rotate(joint.ra), bb.rot.rotate(joint.rb));
        let corr = (bb.pos + rb) - (ba.pos + ra);
        apply_position(&mut self.bodies, joint.a, joint.b, ra, rb, corr, 0.0);
    }

    fn frame_a(&self, j: &Joint) -> Quat {
        self.bodies[j.a as usize].rot * j.fa
    }
    fn frame_b(&self, j: &Joint) -> Quat {
        self.bodies[j.b as usize].rot * j.fb
    }

    fn solve_contacts<G: Ground + ?Sized>(&mut self, _h: f32, ground: &G) {
        let mu_s = self.config.static_friction;
        // Ground: both capsule end spheres against the height field.
        for i in 0..self.bodies.len() {
            if self.bodies[i].pinned {
                continue;
            }
            for end in 0..2 {
                let b = &self.bodies[i];
                let (e0, e1) = b.ends();
                let e = if end == 0 { e0 } else { e1 };
                let gz = ground.height(e.x, e.y);
                if e.z - gz >= b.radius {
                    continue; // the vertical gap bounds the distance along any upward normal
                }
                let n = ground.normal(e.x, e.y);
                let d = (e.z - gz) * n.z;
                let depth = b.radius - d;
                if depth <= 0.0 {
                    continue;
                }
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

    fn solve_velocities(&mut self, h: f32) {
        let mu_d = self.config.dynamic_friction;
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
        let passive = self.config.passive_damping;
        for j in 0..self.joints.len() {
            let (a, b, d) = {
                let jt = &self.joints[j];
                (jt.a, jt.b, jt.tone.damping + passive)
            };
            let (ba, bb) = (&self.bodies[a as usize], &self.bodies[b as usize]);
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
            let (ia, ib) = (ba.inv_inertia_world(p), bb.inv_inertia_world(p));
            self.bodies[a as usize].omega += ia;
            self.bodies[b as usize].omega -= ib;
        }
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

/// Positional XPBD correction: moves the point `ra` of body `a` by `corr` relative to the
/// point `rb` of body `b` (world offsets). Returns the Lagrange multiplier (N·s²).
fn apply_position(bodies: &mut [Body], a: u32, b: u32, ra: Vec3, rb: Vec3, corr: Vec3, alpha: f32) -> f32 {
    let c = corr.length();
    if c < 1e-9 {
        return 0.0;
    }
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
        let dw = ba.inv_inertia_world(ra.cross(p));
        ba.rot = ba.rot.add_rotvec(dw);
    }
    if b != STATIC {
        let bb = &mut bodies[b as usize];
        bb.pos -= p * bb.inv_mass;
        let dw = bb.inv_inertia_world(rb.cross(p));
        bb.rot = bb.rot.add_rotvec(-dw);
    }
    dl
}

/// Angular XPBD correction: turns body `a` by the rotation vector `corr` relative to `b`.
fn apply_rotation(bodies: &mut [Body], a: u32, b: u32, corr: Vec3, alpha: f32) {
    let theta = corr.length();
    if theta < 1e-9 {
        return;
    }
    let n = corr * (1.0 / theta);
    let (ba, bb) = (&bodies[a as usize], &bodies[b as usize]);
    let w = ba.w_rot(n) + bb.w_rot(n) + alpha;
    if w <= 0.0 {
        return;
    }
    let p = n * (theta / w);
    let (da, db) = (ba.inv_inertia_world(p), bb.inv_inertia_world(p));
    bodies[a as usize].rot = bodies[a as usize].rot.add_rotvec(da);
    bodies[b as usize].rot = bodies[b as usize].rot.add_rotvec(-db);
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
        ba.omega += ba.inv_inertia_world(ra.cross(p));
    }
    if b != STATIC {
        let bb = &mut bodies[b as usize];
        bb.vel -= p * bb.inv_mass;
        bb.omega -= bb.inv_inertia_world(rb.cross(p));
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

fn twist_angle(qa: Quat, qb: Quat) -> Option<f32> {
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
    Some(signed_angle(n, b1, b2))
}

fn twist_correction(qa: Quat, qb: Quat, lo: f32, hi: f32) -> Option<Vec3> {
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
    limit_angle(n, b1, b2, lo, hi)
}
