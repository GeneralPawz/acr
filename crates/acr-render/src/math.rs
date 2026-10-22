//! The little matrix algebra the 3D path needs: column-major 4 × 4 matrices (`m[column][row]`,
//! the layout WGSL's `mat4x4<f32>` expects), quaternions as `[x, y, z, w]` (glTF order).

pub type Vec3 = [f32; 3];
pub type Mat4 = [[f32; 4]; 4];

pub const IDENTITY: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, s: f32) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// `a · b` (apply `b` first).
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (c, col) in out.iter_mut().enumerate() {
        for (r, v) in col.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

pub fn transform_point(m: &Mat4, p: Vec3) -> Vec3 {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

pub fn transform_vector(m: &Mat4, v: Vec3) -> Vec3 {
    std::array::from_fn(|r| m[0][r] * v[0] + m[1][r] * v[1] + m[2][r] * v[2])
}

/// Translation · rotation · scale, as glTF composes a node's local transform.
pub fn from_trs(t: Vec3, q: [f32; 4], s: Vec3) -> Mat4 {
    let [x, y, z, w] = normalize_quat(q);
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    [
        [(1.0 - 2.0 * (yy + zz)) * s[0], 2.0 * (xy + wz) * s[0], 2.0 * (xz - wy) * s[0], 0.0],
        [2.0 * (xy - wz) * s[1], (1.0 - 2.0 * (xx + zz)) * s[1], 2.0 * (yz + wx) * s[1], 0.0],
        [2.0 * (xz + wy) * s[2], 2.0 * (yz - wx) * s[2], (1.0 - 2.0 * (xx + yy)) * s[2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

pub fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len > 0.0 { q.map(|v| v / len) } else { [0.0, 0.0, 0.0, 1.0] }
}

/// Spherical interpolation along the shorter arc (normalised linear for close quaternions).
pub fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let b = if d < 0.0 {
        d = -d;
        b.map(|v| -v)
    } else {
        b
    };
    let (wa, wb) = if d > 0.9995 {
        (1.0 - t, t)
    } else {
        let theta = d.clamp(-1.0, 1.0).acos();
        let s = theta.sin();
        (((1.0 - t) * theta).sin() / s, (t * theta).sin() / s)
    };
    normalize_quat(std::array::from_fn(|i| a[i] * wa + b[i] * wb))
}

/// The upper 3 × 4 of an affine matrix as rows (`row_i · (p, 1)` = component `i`), the
/// layout of the bone texture and the instance transforms.
pub fn affine_rows(m: &Mat4) -> [[f32; 4]; 3] {
    std::array::from_fn(|r| [m[0][r], m[1][r], m[2][r], m[3][r]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5)
    }

    #[test]
    fn trs_rotates_then_translates() {
        // 90° about +Y turns +X into −Z.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let m = from_trs([1.0, 2.0, 3.0], [0.0, h, 0.0, h], [2.0, 2.0, 2.0]);
        assert!(near(transform_point(&m, [1.0, 0.0, 0.0]), [1.0, 2.0, 1.0]));
        assert!(near(transform_vector(&m, [0.0, 0.0, 1.0]), [2.0, 0.0, 0.0]));
        let both = mul(&m, &from_trs([1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]));
        assert!(near(transform_point(&both, [0.0; 3]), transform_point(&m, [1.0, 0.0, 0.0])));
    }

    #[test]
    fn slerp_halfway() {
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let q = slerp([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, h, h], 0.5);
        let m = from_trs([0.0; 3], q, [1.0; 3]);
        // 45° about +Z.
        assert!(near(transform_vector(&m, [1.0, 0.0, 0.0]), [h, h, 0.0]));
    }
}
