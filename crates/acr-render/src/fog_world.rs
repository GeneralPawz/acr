//! Fog of war of the free camera (#309). The original's overlay ([`crate::fog`]) darkens
//! screen cells, which only fits the original view. Here the fog lies on the world: a
//! full-screen pass reads the 3D pass's depth, rebuilds the world point under each pixel and
//! looks the fog grid up at its `x`, `y`. The caller moves the grid from its projection
//! (`y − 2·h`, where `acr_sim::fog::Fog::reveal` stamps it) onto the ground first, so a
//! model or tree is as dark as the ground under it from every side. The grid's values are
//! interpolated bilinearly between cell centres and turned into the darkness row of 0x484e63
//! per pixel, so the edges stay smooth from any angle. Ground, models, water and particles
//! drawn before the pass are covered; sprites, drawn after it, take the same share of black
//! at their ground point ([`WorldFogRenderer::black_at`]).

use bytemuck::{Pod, Zeroable};

use crate::fog::ROWS;
use crate::math::Mat4;
use crate::view3d::View3d;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    clip_to_world: Mat4,
    screen: [f32; 4],
    size: [u32; 4],
    dark: [[f32; 4]; ROWS / 4],
}

struct Grid {
    size: [u32; 2],
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Draws the fog grid over the world under the free camera.
pub struct WorldFogRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    dark: [[f32; 4]; ROWS / 4],
    srgb: bool,
    grid: Option<Grid>,
    group: Option<wgpu::BindGroup>,
}

impl WorldFogRenderer {
    /// `darkness[row]`: the opacity of black that row of `AGEW_1DR.GRD` amounts to.
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat, darkness: &[f32; ROWS]) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("fog_world.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world fog globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world fog"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture(1, wgpu::TextureSampleType::Uint),
                // The depth aspect as unfilterable float: `textureLoad` of a depth texture has no GLSL.
                texture(2, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world fog"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world fog"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut dark = [[0.0; 4]; ROWS / 4];
        for (i, d) in darkness.iter().enumerate() {
            dark[i / 4][i % 4] = d.clamp(0.0, 1.0);
        }
        Self { pipeline, layout, globals, dark, srgb: target.is_srgb(), grid: None, group: None }
    }

    /// Uploads the fog grid on the ground (`width` cells per row, cell `(cx, cy)` centred on
    /// world `(128·(cx − 3), 128·(cy − 3))`) and binds
    /// `depth` (the depth aspect of the 3D pass's depth buffer) for the next
    /// [`WorldFogRenderer::render`]; `None` draws no fog.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &View3d,
        grid: Option<(i32, &[u16])>,
        depth: &wgpu::TextureView,
    ) {
        self.group = None;
        let Some((width, cells)) = grid.filter(|(w, c)| *w > 0 && !c.is_empty() && c.len() % *w as usize == 0) else {
            return;
        };
        let size = [width as u32, (cells.len() / width as usize) as u32];
        if self.grid.as_ref().is_none_or(|g| g.size != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("world fog cells"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R16Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            self.grid = Some(Grid { size, texture, view });
        }
        let grid = self.grid.as_ref().expect("created above");
        queue.write_texture(
            grid.texture.as_image_copy(),
            bytemuck::cast_slice(cells),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 2), rows_per_image: None },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        let globals = Globals {
            clip_to_world: affine_inverse(&view.world_to_clip()),
            screen: [view.size[0], view.size[1], if self.srgb { 1.0 } else { 0.0 }, 0.0],
            size: [size[0], size[1], 0, 0],
            dark: self.dark,
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world fog"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&grid.view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth) },
            ],
        }));
    }

    /// The share of black the pass puts over world point (`x`, `y`): what a sprite drawn
    /// after it takes (`SpriteDraw::fog`).
    pub fn black_at(&self, (width, cells): (i32, &[u16]), x: f32, y: f32) -> f32 {
        if width <= 0 {
            return 0.0;
        }
        let rows = cells.len() as i32 / width;
        let cell = |cx: i32, cy: i32| {
            if (0..width).contains(&cx) && (0..rows).contains(&cy) {
                f32::from(cells[(cy * width + cx) as usize])
            } else {
                0.0
            }
        };
        let q = [x / 128.0 + 3.0, y / 128.0 + 3.0];
        let (cx, cy) = (q[0].floor() as i32, q[1].floor() as i32);
        let f = [q[0] - q[0].floor(), q[1] - q[1].floor()];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let top = lerp(cell(cx, cy), cell(cx + 1, cy), f[0]);
        let bottom = lerp(cell(cx, cy + 1), cell(cx + 1, cy + 1), f[0]);
        let row = shade(lerp(top, bottom, f[1])).min(ROWS - 1);
        let a = self.dark[row / 4][row % 4];
        // The sprite shader darkens its output, linear on an sRGB target (as fog_world.wgsl).
        if self.srgb { 1.0 - (1.0 - a).powf(2.2) } else { a }
    }

    /// Draws nothing until the next [`WorldFogRenderer::prepare`].
    pub fn clear(&mut self) {
        self.group = None;
    }

    /// True when [`WorldFogRenderer::prepare`] found a grid to draw.
    pub fn active(&self) -> bool {
        self.group.is_some()
    }

    /// Darkens `color` by the fog in a pass of its own (the depth buffer is read, so it must
    /// not be attached).
    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, color: &wgpu::TextureView) {
        let Some(group) = &self.group else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world fog"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// 0x484e63 (`acr_sim::fog::Fog::shade`) on an interpolated fog value: the darkness row.
fn shade(v: f32) -> usize {
    let i = 1500.0 - v;
    if i <= 0.0 {
        5
    } else if i <= 300.0 {
        (5.0 + i * 153.0 / 300.0) as usize
    } else {
        158
    }
}

/// Inverse of an affine column-major matrix (last row `0 0 0 1`).
fn affine_inverse(m: &Mat4) -> Mat4 {
    // a(r, c) of the 3 × 3 part.
    let a = |r: usize, c: usize| m[c][r];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    let d = if det != 0.0 { 1.0 / det } else { 0.0 };
    let mut inv = [[0.0f32; 3]; 3];
    for (r, row) in inv.iter_mut().enumerate() {
        for (c, v) in row.iter_mut().enumerate() {
            // The cofactor of a(c, r): the adjugate is the transposed cofactor matrix.
            let (r1, r2) = ((c + 1) % 3, (c + 2) % 3);
            let (c1, c2) = ((r + 1) % 3, (r + 2) % 3);
            *v = (a(r1, c1) * a(r2, c2) - a(r1, c2) * a(r2, c1)) * d;
        }
    }
    let t = [m[3][0], m[3][1], m[3][2]];
    let mut out = [[0.0f32; 4]; 4];
    for r in 0..3 {
        for c in 0..3 {
            out[c][r] = inv[r][c];
        }
        out[3][r] = -(inv[r][0] * t[0] + inv[r][1] * t[1] + inv[r][2] * t[2]);
    }
    out[3][3] = 1.0;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::transform_point;
    use crate::terrain::Camera;
    use crate::view3d::Orbit;

    #[test]
    fn clip_to_world_undoes_the_projection() {
        let camera = Camera { x: 3000.0, y: 1500.0, scale: 1.5 };
        for orbit in [None, Some(Orbit { yaw: 1.1, pitch: 0.7 })] {
            let view = View3d { camera, size: [1600.0, 900.0], orbit };
            let m = view.world_to_clip();
            let inv = affine_inverse(&m);
            let p = [3500.0, 3600.0, 42.0];
            let back = transform_point(&inv, transform_point(&m, p));
            for i in 0..3 {
                assert!((back[i] - p[i]).abs() < 0.05, "{orbit:?}: {back:?} vs {p:?}");
            }
        }
    }

    #[test]
    fn shade_follows_0x484e63() {
        // `acr_sim::fog::Fog::shade` at the same values.
        for (v, row) in [(0.0, 158), (1199.0, 158), (1200.0, 158), (1350.0, 81), (1499.0, 5), (1500.0, 5), (8000.0, 5)]
        {
            assert_eq!(shade(v), row, "{v}");
        }
    }
}
