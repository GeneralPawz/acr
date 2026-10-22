//! Sun shadows of the 3D models (docs/rendering-3d.md, "Shadows"): one shadow map seen
//! from the sun, fitted each frame to the models drawn, sampled by the models
//! (`model.wgsl` + `shadow.wgsl`) and laid over the terrain (`terrain.wgsl` + `shadow.wgsl`).
//!
//! The sun is the light the models are shaded by ([`LIGHT`], the game's `SetLight(-15, 14,
//! 30)`): from the west, the south (the viewer's side) and 56° up, so shadows fall to the
//! east and north, right and up on the screen, matching the models' shading.
//!
//! WebGL2: a `Depth32Float` texture sampled through a comparison sampler (GLSL
//! `sampler2DShadow`), a uniform block; no storage buffers.

use bytemuck::{Pod, Zeroable};

use crate::math::{self, Mat4, Vec3};

/// Direction to the light in world coordinates (east, south, up): the game's light vector
/// (`SetLight(-15, 14, 30)`, from the upper left and the viewer's side).
pub const LIGHT: [f32; 3] = [-15.0, 14.0, 30.0];
/// Texels per side of the shadow map.
pub const SIZE: u32 = 2048;
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Smallest side of the area the map covers (world pixels): a lone soldier close up still
/// gets soft edges of a sensible size.
const MIN_EXTENT: f32 = 384.0;
/// How dark a shadow on the terrain is (share of the colour taken away): the ground's
/// colours are lit already, a shadow keeps the ambient share of the models' lighting.
const GROUND_STRENGTH: f32 = 0.42;
/// Width of the soft edge (world pixels, about 6 cm): the PCF kernel spreads over at least
/// this, or one texel when the texels are larger.
const PENUMBRA: f32 = 1.8;

/// The unit vector to the sun.
pub fn sun() -> Vec3 {
    let len = math::dot(LIGHT, LIGHT).sqrt();
    math::scale(LIGHT, 1.0 / len)
}

/// The sun's axes: `right`, `up` across the map and `forward` (the way the light travels).
fn axes() -> [Vec3; 3] {
    let f = math::scale(sun(), -1.0);
    let h = (f[0] * f[0] + f[1] * f[1]).sqrt();
    let right = [f[1] / h, -f[0] / h, 0.0];
    // up = right × forward
    let up = [right[1] * f[2] - right[2] * f[1], right[2] * f[0] - right[0] * f[2], right[0] * f[1] - right[1] * f[0]];
    [right, up, f]
}

/// The area a shadow map covers: light space bounds of the casters and the matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// World → shadow clip space (column-major).
    pub matrix: Mat4,
    /// World pixels per texel.
    pub texel: f32,
    /// Depth range (world pixels along the light).
    pub depth: f32,
}

/// Fits the map to casters (world centre, radius in world pixels) around `focus` within
/// `reach` world pixels: the side is rounded up to a half octave and the corner snapped to
/// whole texels, so the shadows of still models do not swim as others move or the view
/// scrolls. `None` without casters.
pub fn fit(casters: impl IntoIterator<Item = (Vec3, f32)>, focus: Vec3, reach: f32) -> Option<Fit> {
    let [right, up, forward] = axes();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut any = false;
    for (c, r) in casters {
        let d = math::sub(c, focus);
        if (d[0] * d[0] + d[1] * d[1]).sqrt() - r > reach {
            continue;
        }
        any = true;
        for (k, axis) in [right, up, forward].into_iter().enumerate() {
            let v = math::dot(c, axis);
            lo[k] = lo[k].min(v - r);
            hi[k] = hi[k].max(v + r);
        }
    }
    if !any {
        return None;
    }
    let need = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(MIN_EXTENT);
    // Half octaves: 384, 543, 768, 1086, ...
    let extent = 2f32.powf((need.log2() * 2.0).ceil() / 2.0) * (1.0 + 4.0 / SIZE as f32);
    let texel = extent / SIZE as f32;
    let snap = |v: f32| (v / texel).floor() * texel;
    // Centred on the casters, snapped.
    let a0 = snap((lo[0] + hi[0]) / 2.0 - extent / 2.0);
    let b0 = snap((lo[1] + hi[1]) / 2.0 - extent / 2.0);
    // Near: in front of every caster; far: well beyond, the ground behind them is compared
    // clamped to the far plane (`shadow.wgsl`).
    let c0 = lo[2] - 32.0;
    let c1 = hi[2] + 64.0;
    let depth = c1 - c0;
    let row = |axis: Vec3, k: f32, origin: f32, c: f32| -> [f32; 4] {
        [axis[0] * k, axis[1] * k, axis[2] * k, c - k * origin]
    };
    let rows = [
        row(right, 2.0 / extent, a0, -1.0),
        row(up, 2.0 / extent, b0, -1.0),
        row(forward, 1.0 / depth, c0, 0.0),
        [0.0, 0.0, 0.0, 1.0],
    ];
    Some(Fit { matrix: std::array::from_fn(|c| std::array::from_fn(|r| rows[r][c])), texel, depth })
}

/// The uniform block of `shadow.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    matrix: Mat4,
    /// x: on (1), y: PCF step (uv), z: depth bias (light depth units), w: ground strength.
    params: [f32; 4],
    /// xyz: fade centre (world), w: fade reach (world pixels; shadows fade out over the last
    /// fifth).
    fade: [f32; 4],
    /// x: normal offset (world pixels), y: one texel (uv).
    extra: [f32; 4],
}

/// The shadow map, its comparison sampler and the uniform the receivers read (group 2 of the
/// model and terrain-shadow pipelines).
pub struct ShadowMap {
    pub view: wgpu::TextureView,
    pub layout: wgpu::BindGroupLayout,
    pub group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    /// The map was drawn this frame: receivers sample it.
    pub active: bool,
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow uniform"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow receive"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | fragment,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: fragment,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: fragment,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow receive"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        Self { view, layout, group, uniform, active: false }
    }

    /// Uploads this frame's fit (`None`: shadows off or nothing casts; the receivers then
    /// skip the map).
    pub fn upload(&mut self, queue: &wgpu::Queue, fit: Option<&Fit>, focus: Vec3, reach: f32) {
        self.active = fit.is_some();
        let u = match fit {
            Some(f) => {
                let uv = 1.0 / SIZE as f32;
                Uniform {
                    matrix: f.matrix,
                    params: [1.0, (PENUMBRA / f.texel).max(1.0) * uv * 0.5, 1.5 / f.depth, GROUND_STRENGTH],
                    fade: [focus[0], focus[1], focus[2], reach],
                    extra: [f.texel * 1.5, uv, 0.0, 0.0],
                }
            }
            None => Uniform { matrix: [[0.0; 4]; 4], params: [0.0; 4], fade: [0.0; 4], extra: [0.0; 4] },
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
    }

    /// Opens the depth-only pass into the map.
    pub fn begin<'e>(&self, encoder: &'e mut wgpu::CommandEncoder) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow map"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }
}

/// The depth state of the shadow-map pipeline: slope-scaled bias against acne on surfaces
/// the light grazes.
pub fn caster_depth() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: Default::default(),
        bias: wgpu::DepthBiasState { constant: 0, slope_scale: 1.5, clamp: 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadows_fall_east_and_north() {
        // A point 60 px up over the origin and the ground point 60 px · (horizontal / up)
        // east-north-east of it share their place in the map.
        let f = fit([([0.0, 0.0, 30.0], 40.0)], [0.0; 3], 1e4).unwrap();
        let s = sun();
        let k = 60.0 / s[2];
        let head = math::transform_point(&f.matrix, [0.0, 0.0, 60.0]);
        let ground = math::transform_point(&f.matrix, [-s[0] * k, -s[1] * k, 0.0]);
        assert!((head[0] - ground[0]).abs() < 1e-4 && (head[1] - ground[1]).abs() < 1e-4);
        assert!(head[2] < ground[2], "the head is nearer the sun");
        // East (+x) and north (−y).
        assert!(-s[0] * k > 0.0 && -s[1] * k < 0.0);
    }

    #[test]
    fn fit_is_snapped_and_stable() {
        let a = fit([([100.0, 200.0, 0.0], 40.0), ([400.0, 300.0, 0.0], 40.0)], [0.0; 3], 1e4).unwrap();
        let b = fit([([100.3, 200.2, 0.0], 40.0), ([400.0, 300.0, 0.0], 40.0)], [0.0; 3], 1e4).unwrap();
        assert_eq!(a.texel, b.texel);
        // The same world point lands on the same texel position.
        let p = [250.0, 250.0, 0.0];
        let (pa, pb) = (math::transform_point(&a.matrix, p), math::transform_point(&b.matrix, p));
        let texels = |v: f32| (v * 0.5 + 0.5) * SIZE as f32;
        let frac = |v: f32| texels(v) - texels(v).floor();
        assert!((frac(pa[0]) - frac(pb[0])).abs() < 1e-2);
        assert!(fit([([1e5, 0.0, 0.0], 40.0)], [0.0; 3], 1e3).is_none());
    }
}
