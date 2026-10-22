//! Minimal `f32` vector and quaternion math (no dependencies; the workspace uses plain
//! arrays at crate boundaries, see `acr-render::math`). Quaternions are `[x, y, z, w]` like
//! glTF.

use core::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    pub const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }
    /// Unit vector, or zero for a (near) zero vector.
    pub fn normalize_or_zero(self) -> Vec3 {
        let l = self.length();
        if l > 1e-12 { self * (1.0 / l) } else { Vec3::ZERO }
    }
    /// Component-wise product.
    pub fn mul_elem(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
    pub fn from_array(a: [f32; 3]) -> Vec3 {
        Vec3::new(a[0], a[1], a[2])
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}
impl SubAssign for Vec3 {
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}

/// Unit quaternion (rotation), Hamilton convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let a = axis.normalize_or_zero();
        let (s, c) = (angle * 0.5).sin_cos();
        Quat { x: a.x * s, y: a.y * s, z: a.z * s, w: c }
    }
    /// Rotation by the rotation vector `v` (axis times angle).
    pub fn from_rotvec(v: Vec3) -> Quat {
        let a = v.length();
        if a < 1e-9 { Quat::IDENTITY } else { Quat::from_axis_angle(v, a) }
    }
    /// Rotation with the given orthonormal basis as its columns (right-handed).
    pub fn from_basis(x: Vec3, y: Vec3, z: Vec3) -> Quat {
        let (m00, m11, m22) = (x.x, y.y, z.z);
        let tr = m00 + m11 + m22;
        let q = if tr > 0.0 {
            let s = (tr + 1.0).sqrt() * 2.0;
            Quat { w: 0.25 * s, x: (y.z - z.y) / s, y: (z.x - x.z) / s, z: (x.y - y.x) / s }
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
            Quat { w: (y.z - z.y) / s, x: 0.25 * s, y: (y.x + x.y) / s, z: (z.x + x.z) / s }
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
            Quat { w: (z.x - x.z) / s, x: (y.x + x.y) / s, y: 0.25 * s, z: (z.y + y.z) / s }
        } else {
            let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
            Quat { w: (x.y - y.x) / s, x: (z.x + x.z) / s, y: (z.y + y.z) / s, z: 0.25 * s }
        };
        q.normalize()
    }
    pub fn conj(self) -> Quat {
        Quat { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }
    pub fn normalize(self) -> Quat {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if l < 1e-12 {
            return Quat::IDENTITY;
        }
        let s = 1.0 / l;
        Quat { x: self.x * s, y: self.y * s, z: self.z * s, w: self.w * s }
    }
    fn vec(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let u = self.vec();
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }
    /// The rotation vector (axis times angle, angle in `[0, π]`).
    pub fn to_rotvec(self) -> Vec3 {
        let q = if self.w < 0.0 { -self } else { self };
        let s = q.vec().length();
        if s < 1e-7 {
            return q.vec() * 2.0;
        }
        q.vec() * (2.0 * s.atan2(q.w) / s)
    }
    /// The rotation angle in `[0, π]`.
    pub fn angle(self) -> f32 {
        self.to_rotvec().length()
    }
    /// `q + ½·[v, 0]·q`, normalized: `q` turned by the small rotation vector `v` (world).
    pub fn add_rotvec(self, v: Vec3) -> Quat {
        let dq = Quat { x: v.x, y: v.y, z: v.z, w: 0.0 } * self;
        Quat { x: self.x + 0.5 * dq.x, y: self.y + 0.5 * dq.y, z: self.z + 0.5 * dq.z, w: self.w + 0.5 * dq.w }
            .normalize()
    }
    /// `q + ½·[v, 0]·q`, renormalized to first order (no square root): for the small
    /// corrections of the solver; `q` must be (nearly) unit.
    pub fn add_rotvec_raw(self, v: Vec3) -> Quat {
        let dq = Quat { x: v.x, y: v.y, z: v.z, w: 0.0 } * self;
        let q = Quat { x: self.x + 0.5 * dq.x, y: self.y + 0.5 * dq.y, z: self.z + 0.5 * dq.z, w: self.w + 0.5 * dq.w };
        let s = 1.5 - 0.5 * (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w);
        if (0.75..=1.25).contains(&s) { Quat { x: q.x * s, y: q.y * s, z: q.z * s, w: q.w * s } } else { q.normalize() }
    }
    pub fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }
    pub fn from_array(a: [f32; 4]) -> Quat {
        Quat { x: a[0], y: a[1], z: a[2], w: a[3] }
    }
}

impl Neg for Quat {
    type Output = Quat;
    fn neg(self) -> Quat {
        Quat { x: -self.x, y: -self.y, z: -self.z, w: -self.w }
    }
}

impl Mul for Quat {
    type Output = Quat;
    fn mul(self, b: Quat) -> Quat {
        let a = self;
        Quat {
            w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
            x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
        }
    }
}

/// Column-major 4×4 matrix of the rigid transform `(t, q)` (the renderer's `Mat4` layout,
/// `m[column][row]`).
pub fn mat4_from_rt(t: Vec3, q: Quat) -> [[f32; 4]; 4] {
    let x = q.rotate(Vec3::X);
    let y = q.rotate(Vec3::Y);
    let z = q.rotate(Vec3::Z);
    [[x.x, x.y, x.z, 0.0], [y.x, y.y, y.z, 0.0], [z.x, z.y, z.z, 0.0], [t.x, t.y, t.z, 1.0]]
}

/// Closest points `(s, t)` (segment parameters in `[0, 1]`) between segments `p1 + s·d1` and
/// `p2 + t·d2` (Ericson, Real-Time Collision Detection 5.1.9).
pub fn closest_segment_params(p1: Vec3, d1: Vec3, p2: Vec3, d2: Vec3) -> (f32, f32) {
    let r = p1 - p2;
    let a = d1.length_sq();
    let e = d2.length_sq();
    let f = d2.dot(r);
    const EPS: f32 = 1e-9;
    if a <= EPS && e <= EPS {
        return (0.0, 0.0);
    }
    if a <= EPS {
        return (0.0, (f / e).clamp(0.0, 1.0));
    }
    let c = d1.dot(r);
    if e <= EPS {
        return ((-c / a).clamp(0.0, 1.0), 0.0);
    }
    let b = d1.dot(d2);
    let denom = a * e - b * b;
    let mut s = if denom > EPS { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
    let mut t = (b * s + f) / e;
    if t < 0.0 {
        t = 0.0;
        s = (-c / a).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - c) / a).clamp(0.0, 1.0);
    }
    (s, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_and_rotvec_round_trip() {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0), 1.2);
        let v = q.to_rotvec();
        assert!((v.length() - 1.2).abs() < 1e-5);
        let r = Quat::from_axis_angle(Vec3::X, core::f32::consts::FRAC_PI_2).rotate(Vec3::Y);
        assert!((r - Vec3::Z).length() < 1e-6);
        let b = Quat::from_basis(Vec3::Y, -Vec3::X, Vec3::Z);
        assert!((b.rotate(Vec3::X) - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn segments_closest() {
        let (s, t) = closest_segment_params(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 1.0),
            Vec3::new(0.0, 2.0, 0.0),
        );
        assert!((s - 0.5).abs() < 1e-6 && (t - 0.5).abs() < 1e-6);
    }
}
