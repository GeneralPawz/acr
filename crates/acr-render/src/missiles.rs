//! Code-built models of the rounds in flight (#308, docs/rendering-3d.md "Projectiles"):
//! arrows, a musket ball's streak, cannonballs, shells, tomahawks, spears and stones. glTF
//! metres, +y up, the flight along +z; vertex colours (linear) on one plain material each.
//! The thin ones are drawn a little thicker than life so they still show at the game's zoom.

use crate::math::{self, Vec3};
use crate::model::{Material, ModelData, Primitive, Tissue, Vertex};

const WOOD: [f32; 4] = [0.30, 0.19, 0.10, 1.0];
const PALE_WOOD: [f32; 4] = [0.45, 0.33, 0.20, 1.0];
const IRON: [f32; 4] = [0.16, 0.16, 0.17, 1.0];
const FEATHER: [f32; 4] = [0.62, 0.58, 0.50, 1.0];
const BALL: [f32; 4] = [0.035, 0.035, 0.04, 1.0];
const STONE: [f32; 4] = [0.36, 0.34, 0.31, 1.0];
const STREAK: [f32; 4] = [0.80, 0.80, 0.77, 1.0];
const FUSE: [f32; 4] = [0.55, 0.42, 0.25, 1.0];

/// Length of an arrow (metres): its tip sits at the origin, the nock at −z.
pub const ARROW_LENGTH: f32 = 0.8;

#[derive(Default)]
struct Mesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl Mesh {
    fn vertex(&mut self, position: Vec3, normal: Vec3, color: [f32; 4]) -> u32 {
        self.vertices.push(Vertex {
            position,
            normal,
            uv: [0.0; 2],
            color,
            joints: [0; 4],
            weights: [1.0, 0.0, 0.0, 0.0],
            morph: [[0.0; 3]; 2],
            body: [-1.0, -1.0, 0.5, 0.0],
            face: [0; 4],
            wind: [0; 4],
        });
        (self.vertices.len() - 1) as u32
    }

    /// A loft along +z through `rings` of (z, radius), `segments` round; ends of radius 0
    /// close to a point. `centre` shifts it in x/y.
    /// Counter-clockwise seen from outside, as glTF.
    fn loft(&mut self, rings: &[(f32, f32)], segments: usize, color: [f32; 4], centre: [f32; 2]) {
        let base = self.vertices.len() as u32;
        for (i, &(z, r)) in rings.iter().enumerate() {
            // The slope of the side for its normal.
            let (z0, r0) = rings[i.saturating_sub(1)];
            let (z1, r1) = rings[(i + 1).min(rings.len() - 1)];
            let dz = (z1 - z0).max(1e-4);
            let tilt = -(r1 - r0) / dz;
            for s in 0..segments {
                let a = s as f32 / segments as f32 * std::f32::consts::TAU;
                let (c, sn) = (a.cos(), a.sin());
                let n = normalize([c, sn, tilt]);
                self.vertex([centre[0] + c * r, centre[1] + sn * r, z], n, color);
            }
        }
        let at = |i: usize, s: usize| base + (i * segments + s % segments) as u32;
        for i in 0..rings.len() - 1 {
            for s in 0..segments {
                self.indices.extend([
                    at(i, s),
                    at(i, s + 1),
                    at(i + 1, s),
                    at(i, s + 1),
                    at(i + 1, s + 1),
                    at(i + 1, s),
                ]);
            }
        }
    }

    /// A sphere of radius `r` about `c`.
    fn sphere(&mut self, c: Vec3, r: f32, rings: usize, segments: usize, color: [f32; 4]) {
        let base = self.vertices.len() as u32;
        for i in 0..=rings {
            let t = i as f32 / rings as f32 * std::f32::consts::PI;
            for s in 0..=segments {
                let a = s as f32 / segments as f32 * std::f32::consts::TAU;
                let n = [t.sin() * a.cos(), t.cos(), t.sin() * a.sin()];
                self.vertex(math::add(c, math::scale(n, r)), n, color);
            }
        }
        let at = |i: usize, s: usize| base + (i * (segments + 1) + s) as u32;
        for i in 0..rings {
            for s in 0..segments {
                self.indices.extend([
                    at(i, s),
                    at(i, s + 1),
                    at(i + 1, s),
                    at(i, s + 1),
                    at(i + 1, s + 1),
                    at(i + 1, s),
                ]);
            }
        }
    }

    /// A flat quad seen from both sides (fletching, an axe blade).
    fn card(&mut self, corners: [Vec3; 4], color: [f32; 4]) {
        let n = normalize(cross(math::sub(corners[1], corners[0]), math::sub(corners[3], corners[0])));
        for side in [1.0f32, -1.0] {
            let k: Vec<u32> = corners.iter().map(|&p| self.vertex(p, math::scale(n, side), color)).collect();
            if side > 0.0 {
                self.indices.extend([k[0], k[1], k[2], k[0], k[2], k[3]]);
            } else {
                self.indices.extend([k[0], k[2], k[1], k[0], k[3], k[2]]);
            }
        }
    }

    /// An axis-aligned box.
    fn cuboid(&mut self, lo: Vec3, hi: Vec3, color: [f32; 4]) {
        let faces: [(Vec3, [Vec3; 4]); 6] = [
            (
                [1.0, 0.0, 0.0],
                [[hi[0], lo[1], lo[2]], [hi[0], hi[1], lo[2]], [hi[0], hi[1], hi[2]], [hi[0], lo[1], hi[2]]],
            ),
            (
                [-1.0, 0.0, 0.0],
                [[lo[0], lo[1], hi[2]], [lo[0], hi[1], hi[2]], [lo[0], hi[1], lo[2]], [lo[0], lo[1], lo[2]]],
            ),
            (
                [0.0, 1.0, 0.0],
                [[lo[0], hi[1], lo[2]], [lo[0], hi[1], hi[2]], [hi[0], hi[1], hi[2]], [hi[0], hi[1], lo[2]]],
            ),
            (
                [0.0, -1.0, 0.0],
                [[lo[0], lo[1], hi[2]], [lo[0], lo[1], lo[2]], [hi[0], lo[1], lo[2]], [hi[0], lo[1], hi[2]]],
            ),
            (
                [0.0, 0.0, 1.0],
                [[lo[0], lo[1], hi[2]], [hi[0], lo[1], hi[2]], [hi[0], hi[1], hi[2]], [lo[0], hi[1], hi[2]]],
            ),
            (
                [0.0, 0.0, -1.0],
                [[hi[0], lo[1], lo[2]], [lo[0], lo[1], lo[2]], [lo[0], hi[1], lo[2]], [hi[0], hi[1], lo[2]]],
            ),
        ];
        for (n, q) in faces {
            let k: Vec<u32> = q.iter().map(|&p| self.vertex(p, n, color)).collect();
            self.indices.extend([k[0], k[1], k[2], k[0], k[2], k[3]]);
        }
    }

    fn model(self, name: &str) -> ModelData {
        let material = Material {
            name: name.into(),
            base_color: [1.0; 4],
            texture: None,
            nearest: false,
            alpha_cutoff: None,
            decay: None,
            stump: false,
            tissue: Tissue::None,
            tint: Default::default(),
            finish: Default::default(),
            surface: Default::default(),
        };
        ModelData::single(
            Primitive {
                vertices: self.vertices,
                indices: self.indices,
                material: 0,
                morphs: [false; 2],
                look_morphs: Vec::new(),
            },
            material,
        )
    }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: Vec3) -> Vec3 {
    let l = math::dot(v, v).sqrt();
    if l > 0.0 { math::scale(v, 1.0 / l) } else { [0.0, 1.0, 0.0] }
}

/// An arrow (or a crossbow bolt): an iron head with its tip at the origin, the shaft back
/// along −z to the nock, three grey feathers near it.
pub fn arrow() -> ModelData {
    let mut m = Mesh::default();
    let l = ARROW_LENGTH;
    m.loft(&[(-l, 0.0), (-l + 0.01, 0.011), (-0.07, 0.011), (-0.07, 0.0)], 5, PALE_WOOD, [0.0; 2]);
    m.loft(&[(-0.08, 0.0), (-0.075, 0.022), (0.0, 0.0)], 4, IRON, [0.0; 2]);
    for k in 0..3 {
        let a = k as f32 / 3.0 * std::f32::consts::TAU + 0.5;
        let (c, s) = (a.cos(), a.sin());
        let r = |w: f32, z: f32| [c * w, s * w, z];
        m.card([r(0.008, -l + 0.03), r(0.04, -l + 0.06), r(0.03, -l + 0.2), r(0.008, -l + 0.22)], FEATHER);
    }
    m.model("missile_arrow")
}

/// A musket ball's streak: a spindle from −1 to 0 along z, widest near its head, of unit
/// radius there; drawn stretched to the distance flown and thin.
pub fn streak() -> ModelData {
    let mut m = Mesh::default();
    m.loft(&[(-1.0, 0.0), (-0.6, 0.35), (-0.12, 1.0), (-0.03, 0.8), (0.0, 0.0)], 6, STREAK, [0.0; 2]);
    m.model("missile_streak")
}

/// A ball of unit radius (cannonball, grapeshot; scaled per round).
pub fn ball() -> ModelData {
    let mut m = Mesh::default();
    m.sphere([0.0; 3], 1.0, 6, 10, BALL);
    m.model("missile_ball")
}

/// A mortar shell of 0.11 m radius with its wooden fuse plug sticking out along +y, the
/// fuse's tip at (0, 0.15, 0).
pub fn shell() -> ModelData {
    let mut m = Mesh::default();
    m.sphere([0.0; 3], 0.11, 7, 12, BALL);
    // The plug along +y: a loft along z, turned by swapping y and z.
    let start = m.vertices.len();
    m.loft(&[(0.09, 0.0), (0.09, 0.022), (0.15, 0.016), (0.15, 0.0)], 6, FUSE, [0.0; 2]);
    for v in &mut m.vertices[start..] {
        v.position = [v.position[0], v.position[2], -v.position[1]];
        v.normal = [v.normal[0], v.normal[2], -v.normal[1]];
    }
    m.model("missile_shell")
}

/// Height of a shell's fuse tip above its centre (metres).
pub const SHELL_FUSE: f32 = 0.15;

/// A tomahawk: a handle along z (−0.25..0.2), the iron head at its +z end, its blade
/// along +y, turning about its middle.
pub fn tomahawk() -> ModelData {
    let mut m = Mesh::default();
    m.loft(&[(-0.25, 0.0), (-0.245, 0.016), (0.2, 0.014), (0.21, 0.0)], 5, WOOD, [0.0; 2]);
    m.cuboid([-0.012, -0.03, 0.15], [0.012, 0.05, 0.2], IRON);
    m.card([[0.0, 0.05, 0.13], [0.0, 0.12, 0.11], [0.0, 0.12, 0.24], [0.0, 0.05, 0.22]], IRON);
    m.model("missile_tomahawk")
}

/// A spear: a shaft from −1 to 0.8 along z with an iron head to 1.05.
pub fn spear() -> ModelData {
    let mut m = Mesh::default();
    m.loft(&[(-1.0, 0.0), (-0.99, 0.016), (0.8, 0.014), (0.8, 0.0)], 5, WOOD, [0.0; 2]);
    m.loft(&[(0.78, 0.0), (0.8, 0.02), (0.88, 0.032), (1.05, 0.0)], 4, IRON, [0.0; 2]);
    m.model("missile_spear")
}

/// A thrown stone of about 0.06 m radius, a little flattened.
pub fn stone() -> ModelData {
    let mut m = Mesh::default();
    m.sphere([0.0; 3], 0.06, 4, 6, STONE);
    for v in &mut m.vertices {
        v.position[1] *= 0.7;
    }
    m.model("missile_stone")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missiles_are_small_closed_meshes() {
        for (name, m) in [
            ("arrow", arrow()),
            ("streak", streak()),
            ("ball", ball()),
            ("shell", shell()),
            ("tomahawk", tomahawk()),
            ("spear", spear()),
            ("stone", stone()),
        ] {
            let p = &m.primitives[0];
            assert!(!p.indices.is_empty() && p.indices.len() % 3 == 0, "{name}");
            assert!(p.indices.iter().all(|&i| (i as usize) < p.vertices.len()), "{name}");
            assert!(p.indices.len() / 3 < 400, "{name}: {} triangles", p.indices.len() / 3);
        }
        // The arrow's tip at the origin, the nock behind it.
        let zs: Vec<f32> = arrow().primitives[0].vertices.iter().map(|v| v.position[2]).collect();
        assert!(zs.iter().all(|z| (-ARROW_LENGTH - 1e-6..=1e-6).contains(z)));
    }
}
