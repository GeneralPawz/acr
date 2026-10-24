//! The camera of the 3D path: the original projection as a matrix, and the free camera.
//!
//! World coordinates of the 3D path are world pixels: `x` east, `y` south (as the game
//! stores positions) and `z` up as a **true** height. The game's heights (terrain `SURF`,
//! sprite offsets) are screen pixels, i.e. true heights × cos 30°: a ground point at game
//! height `h` lies at `z = h / cos 30°` (see `docs/assets.md`, "The original camera").
//!
//! The original projection (2:1 dimetric) is an orthographic camera looking north and 30°
//! down: `screen = (x, y / 2 − h)`, with [`Camera`] adding its scroll position and scale. The
//! fixed camera ([`View3d::orbit`] `None`) reproduces it exactly, so models line up with
//! sprites, the HUD, picking and the minimap. Its depth is `−y`: along every view ray `y`
//! falls monotonically, so depth by `y` orders surfaces correctly and makes a sprite (a
//! vertical billboard at its ground row) a constant depth that matches the original's draw
//! order by row. The free camera ([`Orbit`]) turns about the vertical through the point at
//! the window centre and tilts; it stays orthographic and uses its true view depth.
//! `docs/rendering-3d.md` derives the formulas.

use crate::math::{self, Mat4, Vec3};
use crate::terrain::Camera;

/// Elevation of the original camera: 30°, `sin e = 1/2`.
pub const ELEVATION: f32 = std::f32::consts::FRAC_PI_6;
/// `cos 30°`: screen pixels per pixel of true height.
pub const COS_E: f32 = 0.866_025_4;
/// World pixels per metre of the models (`docs/assets.md`).
pub const PIXELS_PER_METRE: f32 = 30.0;
/// Half the depth range in world pixels around the view centre; outside it is clipped.
pub const DEPTH_RANGE: f32 = 32768.0;

/// The free camera's turn about the vertical (`yaw`, radians; positive turns the view
/// clockwise seen from above, i.e. the picture counter-clockwise) and its elevation
/// (`pitch`, radians above the horizon).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
}

impl Orbit {
    /// The original view: no turn, 30° down.
    pub const ORIGINAL: Self = Self { yaw: 0.0, pitch: ELEVATION };
    /// Elevation limits of the free camera.
    pub const MIN_PITCH: f32 = 0.26; // 15°
    pub const MAX_PITCH: f32 = 1.40; // 80°

    pub fn clamped(self) -> Self {
        Self {
            yaw: self.yaw.rem_euclid(std::f32::consts::TAU),
            pitch: self.pitch.clamp(Self::MIN_PITCH, Self::MAX_PITCH),
        }
    }
}

/// The camera of one frame: the game's [`Camera`] (scroll position and scale), the output
/// size and, for the free camera, its [`Orbit`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View3d {
    pub camera: Camera,
    pub size: [f32; 2],
    pub orbit: Option<Orbit>,
}

/// The camera's axes in world coordinates: screen right, screen down, into the screen.
#[derive(Debug, Clone, Copy)]
pub struct Basis {
    pub right: Vec3,
    pub down: Vec3,
    pub forward: Vec3,
}

impl View3d {
    pub fn fixed(camera: Camera, size: [f32; 2]) -> Self {
        Self { camera, size, orbit: None }
    }

    pub fn is_free(&self) -> bool {
        self.orbit.is_some()
    }

    /// The point the free camera turns about: the ground point (`z = 0`) at the window centre
    /// in the original view.
    pub fn target(&self) -> Vec3 {
        let c = &self.camera;
        [c.x + self.size[0] / (2.0 * c.scale), 2.0 * (c.y + self.size[1] / (2.0 * c.scale)), 0.0]
    }

    pub fn basis(&self) -> Basis {
        let Orbit { yaw, pitch } = self.orbit.unwrap_or(Orbit::ORIGINAL);
        let (sy, cy) = yaw.sin_cos();
        let (se, ce) = pitch.sin_cos();
        // Looking north (−y) at yaw 0; the horizontal view direction turns with the yaw.
        let ahead = [sy, -cy];
        Basis {
            right: [cy, sy, 0.0],
            down: [-ahead[0] * se, -ahead[1] * se, -ce],
            forward: [ahead[0] * ce, ahead[1] * ce, -se],
        }
    }

    /// The axis depth is measured along: the true view direction for the free camera, `−y`
    /// for the fixed one (see the module documentation).
    fn depth_axis(&self) -> Vec3 {
        if self.is_free() { self.basis().forward } else { [0.0, -1.0, 0.0] }
    }

    /// World → clip space (wgpu: x, y in −1..1, z in 0..1), column-major.
    pub fn world_to_clip(&self) -> Mat4 {
        let b = self.basis();
        let t = self.target();
        let s = self.camera.scale;
        let [w, h] = self.size;
        let d = self.depth_axis();
        let row = |axis: Vec3, k: f32, c: f32| -> [f32; 4] {
            [axis[0] * k, axis[1] * k, axis[2] * k, c - k * math::dot(axis, t)]
        };
        let rows = [
            row(b.right, 2.0 * s / w, 0.0),
            row(b.down, -2.0 * s / h, 0.0),
            row(d, 0.5 / DEPTH_RANGE, 0.5),
            [0.0, 0.0, 0.0, 1.0],
        ];
        std::array::from_fn(|c| std::array::from_fn(|r| rows[r][c]))
    }

    /// Output pixel (x, y) and depth (0..1) of a world point.
    pub fn project(&self, p: Vec3) -> Vec3 {
        let b = self.basis();
        let q = math::sub(p, self.target());
        let s = self.camera.scale;
        [
            self.size[0] / 2.0 + s * math::dot(q, b.right),
            self.size[1] / 2.0 + s * math::dot(q, b.down),
            0.5 + math::dot(q, self.depth_axis()) * 0.5 / DEPTH_RANGE,
        ]
    }

    /// Depth (0..1) of a sprite at ground row `y` (world pixels, `2 · sort row` for parts
    /// sorted by a line) standing at `(x, y, z)`: the fixed camera orders by row only.
    pub fn sprite_depth(&self, p: Vec3) -> f32 {
        self.project(p)[2].clamp(0.0, 1.0)
    }

    /// The view ray through output pixel `px`: a point in the plane through the target and
    /// the (orthographic) view direction.
    pub fn ray(&self, px: [f32; 2]) -> (Vec3, Vec3) {
        let b = self.basis();
        let s = self.camera.scale;
        let origin = math::add(
            self.target(),
            math::add(
                math::scale(b.right, (px[0] - self.size[0] / 2.0) / s),
                math::scale(b.down, (px[1] - self.size[1] / 2.0) / s),
            ),
        );
        (origin, b.forward)
    }

    /// The world point (`x`, `y`) on the ground under output pixel `px`; `height` gives the
    /// ground height in screen pixels (`TerrainData::height_at`). The ray meets the plane
    /// at the height found so far, a few times over, like the game's `world_at` does for
    /// the fixed camera.
    pub fn ground_at(&self, px: [f32; 2], height: impl Fn(i32, i32) -> i32) -> (i32, i32) {
        let (o, d) = self.ray(px);
        let at = |z: f32| {
            let t = (o[2] - z) / -d[2];
            [o[0] + t * d[0], o[1] + t * d[1]]
        };
        let mut p = at(0.0);
        for _ in 0..6 {
            p = at(height(p[0] as i32, p[1] as i32) as f32 / COS_E);
        }
        (p[0] as i32, p[1] as i32)
    }

    /// World rectangle `[x0, y0, x1, y1]` the view can show of ground between true heights
    /// `z0` and `z1` (for culling).
    pub fn ground_bounds(&self, z0: f32, z1: f32) -> [f32; 4] {
        let [w, h] = self.size;
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for px in [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]] {
            let (o, d) = self.ray(px);
            for z in [z0, z1] {
                let t = (o[2] - z) / -d[2];
                let p = [o[0] + t * d[0], o[1] + t * d[1]];
                for i in 0..2 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        }
        [lo[0], lo[1], hi[0], hi[1]]
    }

    /// Moves the camera by `delta` output pixels on the screen: scrolling turns with the
    /// free camera (the game scrolls in its own `camera.x/y`, which are world axes).
    pub fn pan(&self, delta: [f32; 2]) -> Camera {
        let b = self.basis();
        let s = self.camera.scale;
        // A screen step `down` moves the ground point under it by `1 / sin(pitch)` along the
        // horizontal down direction.
        let pitch = self.orbit.unwrap_or(Orbit::ORIGINAL).pitch;
        let down_h = [b.down[0], b.down[1]];
        let len = (down_h[0] * down_h[0] + down_h[1] * down_h[1]).sqrt().max(1e-6);
        let k = 1.0 / pitch.sin() / len;
        let dx = (delta[0] * b.right[0] + delta[1] * down_h[0] * k) / s;
        let dy = (delta[0] * b.right[1] + delta[1] * down_h[1] * k) / s;
        Camera { x: self.camera.x + dx, y: self.camera.y + dy / 2.0, scale: s }
    }
}

/// World position of a game ground point: `(x, y)` world pixels at game height `h` (screen
/// pixels). The row is rounded down to even as `y >> 1` does in the sprite projection, so a
/// model's ground point lands on its sprite's anchor pixel.
pub fn ground_point(x: i32, y: i32, h: i32) -> Vec3 {
    [x as f32, ((y >> 1) * 2) as f32, h as f32 / COS_E]
}

/// How far south of its sim position (world pixels, 0.2 m) a unit model stands (#196): the
/// original's unit sprites draw their feet 3–4 screen pixels below the anchor (measured in all
/// 8 directions for EngM18, EngKri, EngD18, EngPik and SpnM18, docs/assets-slice.md), so a
/// model with its feet on the anchor stood that much higher than its sprite. Buildings and
/// map objects stand on their anchor.
pub const UNIT_SOUTH: f32 = 6.0;

/// [`ground_point`] of a unit model: [`UNIT_SOUTH`] further south, at the ground height of the
/// unit's own point.
pub fn unit_ground_point(x: i32, y: i32, h: i32) -> Vec3 {
    let g = ground_point(x, y, h);
    [g[0], g[1] + UNIT_SOUTH, g[2]]
}

/// The yaw of a model facing `RealDir` `dir` (0 east, 64 south): models face south at yaw 0
/// and turn counter-clockwise seen from above (`docs/assets.md`).
pub fn facing_yaw(dir: u8) -> f32 {
    -(f32::from(dir) - 64.0) * std::f32::consts::TAU / 256.0
}

/// Model (glTF: metres, +x east, +y up, +z south) → world, for a model standing at
/// `ground` (world coordinates), turned by `yaw` and scaled by `scale` (1 for the asset
/// convention of 30 px per metre).
pub fn model_matrix(ground: Vec3, yaw: f32, scale: f32) -> Mat4 {
    let (s, c) = yaw.sin_cos();
    let k = PIXELS_PER_METRE * scale;
    // Blender (x, −z_gltf, y_gltf) turned about +z by `yaw`, then to world (x, −y, z).
    // glTF x → world (c, −s, 0); glTF y → world z; glTF z → Blender −y → world (s, c, 0).
    [[c * k, -s * k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [s * k, c * k, 0.0, 0.0], [ground[0], ground[1], ground[2], 1.0]]
}

/// [`model_matrix`] for a model tilted to rest on a slope (a corpse on a pile): `pitch` is
/// the height change per world pixel along the model's facing (glTF +z), `roll` towards its
/// right side (glTF −x). Lengths are kept: the axes are turned, not sheared.
pub fn model_matrix_tilted(ground: Vec3, yaw: f32, scale: f32, pitch: f32, roll: f32) -> Mat4 {
    if pitch == 0.0 && roll == 0.0 {
        return model_matrix(ground, yaw, scale);
    }
    let (s, c) = yaw.sin_cos();
    let k = PIXELS_PER_METRE * scale;
    let norm = |v: Vec3| math::scale(v, 1.0 / math::dot(v, v).sqrt());
    // Along the facing (glTF z) and to the model's right (glTF −x), raised by the slopes.
    let along = norm([s, c, pitch]);
    let right = [-c, s, roll];
    let right = norm(math::sub(right, math::scale(along, math::dot(right, along))));
    // The world mapping mirrors (y points south): glTF y = x × z = along × right.
    let up = [
        along[1] * right[2] - along[2] * right[1],
        along[2] * right[0] - along[0] * right[2],
        along[0] * right[1] - along[1] * right[0],
    ];
    let x = math::scale(right, -k);
    let (y, z) = (math::scale(up, k), math::scale(along, k));
    [[x[0], x[1], x[2], 0.0], [y[0], y[1], y[2], 0.0], [z[0], z[1], z[2], 0.0], [ground[0], ground[1], ground[2], 1.0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tilted_model_keeps_its_size_and_follows_the_slope() {
        let flat = model_matrix([1.0, 2.0, 3.0], 0.7, 1.0);
        assert_eq!(model_matrix_tilted([1.0, 2.0, 3.0], 0.7, 1.0, 0.0, 0.0), flat);
        // A tiny tilt is almost the flat matrix (same axes, same handedness).
        let near = model_matrix_tilted([1.0, 2.0, 3.0], 0.7, 1.0, 1e-6, 1e-6);
        for (a, b) in near.iter().flatten().zip(flat.iter().flatten()) {
            assert!((a - b).abs() < 1e-3, "{near:?} {flat:?}");
        }
        let m = model_matrix_tilted([0.0; 3], 0.7, 1.0, 0.3, -0.2);
        let at = |p: Vec3| math::transform_point(&m, p);
        let head = at([0.0, 0.0, 1.0]);
        assert!((math::dot(head, head).sqrt() - PIXELS_PER_METRE).abs() < 1e-3);
        // Rising along the facing by the pitch, falling to the right (−x) by the roll.
        let h = (head[0] * head[0] + head[1] * head[1]).sqrt();
        assert!((head[2] / h - 0.3).abs() < 1e-4, "{head:?}");
        assert!(at([-1.0, 0.0, 0.0])[2] < 0.0);
        let up = at([0.0, 1.0, 0.0]);
        assert!(up[2] > 0.9 * PIXELS_PER_METRE && math::dot(up, head).abs() < 1e-2, "{up:?}");
    }

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn clip(m: &Mat4, p: Vec3, size: [f32; 2]) -> Vec3 {
        let c = math::transform_point(m, p);
        [(c[0] + 1.0) * size[0] / 2.0, (1.0 - c[1]) * size[1] / 2.0, c[2]]
    }

    #[test]
    fn fixed_camera_reproduces_the_sprite_projection() {
        // The sprite path places a ground point (x, y) at height h at `Camera::project`.
        for (camera, size) in [
            (Camera { x: 1234.0, y: 567.0, scale: 1.0 }, [1024.0, 768.0]),
            (Camera { x: 0.0, y: 0.0, scale: 1.0 }, [800.0, 600.0]),
            (Camera { x: 300.5, y: -20.25, scale: 1.5 }, [1280.0, 960.0]),
            // The zoomed-out view (key L) is the same projection at a quarter of the size.
            (Camera { x: 4000.0, y: 2000.0, scale: 0.25 }, [1024.0, 768.0]),
        ] {
            let view = View3d::fixed(camera, size);
            let m = view.world_to_clip();
            for (x, y, h) in [(1300, 1200, 0), (1301, 1201, 37), (2000, 3000, -12), (1500, 1500, 400), (0, 0, 0)] {
                let want = camera.project(x, y, h);
                let p = ground_point(x, y, h);
                let got = view.project(p);
                let via_matrix = clip(&m, p, size);
                for (g, what) in [(got, "project"), (via_matrix, "matrix")] {
                    assert!(
                        close(g[0], want[0], 1e-3) && close(g[1], want[1], 1e-3),
                        "{what} ({x}, {y}, {h}) at {camera:?}: {g:?} vs {want:?}"
                    );
                }
                assert!(close(via_matrix[2], got[2], 1e-6));
            }
        }
    }

    #[test]
    fn unit_models_stand_three_pixels_below_the_anchor() {
        // #196: at zoom 1 a unit's feet land 3 screen pixels below its sprite anchor, on any
        // ground height; the left-right position stays.
        let camera = Camera { x: 1234.0, y: 567.0, scale: 1.0 };
        let view = View3d::fixed(camera, [1024.0, 768.0]);
        for (x, y, h) in [(1300, 1200, 0), (1301, 1201, 37), (2000, 3000, -12)] {
            let anchor = camera.project(x, y, h);
            let feet = view.project(unit_ground_point(x, y, h));
            assert!(close(feet[0], anchor[0], 1e-3), "{feet:?} {anchor:?}");
            assert!(close(feet[1] - anchor[1], UNIT_SOUTH / 2.0, 1e-3), "{feet:?} {anchor:?}");
            assert_eq!(unit_ground_point(x, y, h)[2], ground_point(x, y, h)[2]);
        }
    }

    #[test]
    fn model_points_follow_the_asset_camera_formula() {
        // docs/assets.md: canvas = anchor + (q · right, −q · up) / s with q = Rz(yaw) · p in
        // Blender metres, right = (1, 0, 0), up = (0, sin e, cos e).
        let camera = Camera { x: 100.0, y: 50.0, scale: 1.0 };
        let view = View3d::fixed(camera, [640.0, 480.0]);
        let (gx, gy, gh) = (400, 300, 20);
        let anchor = camera.project(gx, gy, gh);
        for dir in [0u8, 37, 64, 128, 192, 255] {
            let m = model_matrix(ground_point(gx, gy, gh), facing_yaw(dir), 1.0);
            for gltf in [[0.3, 1.7, -0.2], [-0.25, 0.0, 0.4], [0.0, 1.0, 0.0]] {
                // glTF (x, y, z) is Blender (x, −z, y).
                let b = [gltf[0], -gltf[2], gltf[1]];
                let (s, c) = facing_yaw(dir).sin_cos();
                let q = [c * b[0] - s * b[1], s * b[0] + c * b[1], b[2]];
                let want = [anchor[0] + q[0] * 30.0, anchor[1] - (q[1] * 0.5 + q[2] * COS_E) * 30.0];
                let got = view.project(math::transform_point(&m, gltf));
                assert!(close(got[0], want[0], 1e-3) && close(got[1], want[1], 1e-3), "{dir} {gltf:?}");
            }
        }
        // Facing: the model's front (+z) points south at RealDir 64, west at 128.
        let front = |dir| math::transform_vector(&model_matrix([0.0; 3], facing_yaw(dir), 1.0), [0.0, 0.0, 1.0]);
        assert!(close(front(64)[1], 30.0, 1e-4));
        assert!(close(front(128)[0], -30.0, 1e-4));
        assert!(close(front(0)[0], 30.0, 1e-4));
        assert!(close(front(192)[1], -30.0, 1e-4));
    }

    #[test]
    fn fixed_depth_orders_by_row_and_along_rays() {
        let view = View3d::fixed(Camera { x: 0.0, y: 0.0, scale: 1.0 }, [800.0, 600.0]);
        // South is nearer, whatever the height.
        assert!(view.project([10.0, 500.0, 0.0])[2] < view.project([10.0, 400.0, 300.0])[2]);
        // Along a view ray depth grows (farther).
        let (o, d) = view.ray([100.0, 100.0]);
        let a = view.project(o);
        let b = view.project(math::add(o, math::scale(d, 50.0)));
        assert!(b[2] > a[2]);
        assert!(close(b[0], a[0], 1e-3) && close(b[1], a[1], 1e-3), "a ray is one pixel");
    }

    #[test]
    fn free_camera_at_the_original_angle_is_the_fixed_one() {
        let camera = Camera { x: 777.0, y: 333.0, scale: 1.25 };
        let fixed = View3d::fixed(camera, [1024.0, 768.0]);
        let free = View3d { orbit: Some(Orbit::ORIGINAL), ..fixed };
        for p in [[800.0, 900.0, 10.0], [1200.0, 700.0, 0.0]] {
            let (a, b) = (fixed.project(p), free.project(p));
            assert!(close(a[0], b[0], 1e-3) && close(a[1], b[1], 1e-3));
        }
        // Turned by 180°, the target stays and the point mirrors about it.
        let half = View3d { orbit: Some(Orbit { yaw: std::f32::consts::PI, pitch: ELEVATION }), ..fixed };
        let t = fixed.target();
        let p = [t[0] + 100.0, t[1] + 40.0, 0.0];
        let (a, b) = (fixed.project(p), half.project(p));
        assert!(close(a[0] - 512.0, 512.0 - b[0], 1e-2) && close(a[1] - 384.0, 384.0 - b[1], 1e-2));
    }

    #[test]
    fn ground_picking_inverts_the_projection() {
        let height = |x: i32, y: i32| (x + y) / 40;
        for orbit in [None, Some(Orbit { yaw: 0.7, pitch: 0.9 }), Some(Orbit { yaw: 4.0, pitch: 0.3 })] {
            let view = View3d { camera: Camera { x: 500.0, y: 300.0, scale: 1.0 }, size: [800.0, 600.0], orbit };
            for (x, y) in [(900, 1000), (700, 1300)] {
                let px = view.project([x as f32, y as f32, height(x, y) as f32 / COS_E]);
                let (gx, gy) = view.ground_at([px[0], px[1]], height);
                // Integer heights quantise the answer: one step is cot(pitch) · 1.15 px.
                let slack = orbit.map_or(2, |o| (1.2 / o.pitch.tan()).ceil() as i32 + 1);
                assert!((gx - x).abs() <= slack && (gy - y).abs() <= slack, "{orbit:?}: ({x}, {y}) -> ({gx}, {gy})");
            }
        }
    }

    #[test]
    fn panning_moves_along_the_screen() {
        let camera = Camera { x: 1000.0, y: 1000.0, scale: 1.0 };
        let size = [800.0, 600.0];
        for orbit in [None, Some(Orbit { yaw: 1.1, pitch: 0.8 })] {
            let view = View3d { camera, size, orbit };
            let t = view.target();
            let moved = View3d { camera: view.pan([30.0, -20.0]), ..view };
            // The old target is now 30 px left and 20 px below the centre.
            let p = moved.project(t);
            assert!(close(p[0], 400.0 - 30.0, 1e-2) && close(p[1], 300.0 + 20.0, 1e-2), "{orbit:?}: {p:?}");
        }
    }
}
