//! Fog of war over the map view, as the original draws it after the world and before the
//! interface (0x4bee1e):
//!
//! - 0x485606 spreads the fog grid (128 px cells) over the screen cells (32 × 16 screen
//!   pixels): each fog cell is split 4 × 4 by repeated midpoints of its four corner values;
//! - 0x485a3c turns each screen cell corner into a darkness row (0x484e63, see
//!   [`FogShades`]) and 0x4845ae interpolates the row bilinearly over the cell, replacing
//!   every pixel by `AGEW_1DR.GRD[row][pixel]`. Rows below 64 leave the pixel alone, rows
//!   from 96 on make it black;
//! - the zoomed-out view (0x485d2d) skips the split: one screen cell per fog cell, with the
//!   fog cells' own values at its corners.
//!
//! The terrain is drawn in palette indices but sprites are blended in colour, so the
//! darkness rows are applied as black of the row's fitted opacity
//! ([`acr_formats::grd::BlendTable::darkness`]).

use bytemuck::{Pod, Zeroable};

use crate::terrain::Camera;

/// Darkness rows of `AGEW_1DR.GRD` the overlay can reach (0x484e63 gives at most 158).
pub const ROWS: usize = 160;

/// Darkness rows at the corners of the screen cells in view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FogShades {
    /// Screen cell corner of `rows[0]`, in corners (`spacing` original pixels apart).
    pub origin: [i32; 2],
    /// Original screen pixels between corners: 32 × 16, or 128 × 64 when zoomed out.
    pub spacing: [i32; 2],
    /// Corners per row and rows.
    pub size: [u32; 2],
    pub rows: Vec<u8>,
}

impl FogShades {
    /// The corners covering the view of `camera` on a `window` of output pixels. `cell(gx,
    /// gy)` is the fog value of grid cell `(gx, gy)`, centred on original screen pixel
    /// `(128·(gx − 3), 64·(gy − 3))`; `row` maps a fog value to a darkness row
    /// (`acr_sim::fog::Fog::shade`).
    pub fn new(
        camera: &Camera,
        window: [f32; 2],
        zoomed: bool,
        cell: impl Fn(i32, i32) -> u16,
        row: impl Fn(u16) -> u8,
    ) -> Self {
        let spacing = if zoomed { [128, 64] } else { [32, 16] };
        let (w, h) = (window[0] / camera.scale, window[1] / camera.scale);
        let x0 = (camera.x / spacing[0] as f32).floor() as i32;
        let y0 = (camera.y / spacing[1] as f32).floor() as i32;
        let x1 = ((camera.x + w) / spacing[0] as f32).floor() as i32 + 1;
        let y1 = ((camera.y + h) / spacing[1] as f32).floor() as i32 + 1;
        let size = [(x1 - x0 + 1).max(1) as u32, (y1 - y0 + 1).max(1) as u32];
        let mut rows = Vec::with_capacity((size[0] * size[1]) as usize);
        for n in y0..y0 + size[1] as i32 {
            for m in x0..x0 + size[0] as i32 {
                let v = if zoomed {
                    cell(m + 3, n + 3)
                } else {
                    let (gx, gy) = ((m >> 2) + 3, (n >> 2) + 3);
                    let corners = [cell(gx, gy), cell(gx + 1, gy), cell(gx, gy + 1), cell(gx + 1, gy + 1)];
                    split(corners, (m & 3) as usize, (n & 3) as usize)
                };
                rows.push(row(v));
            }
        }
        Self { origin: [x0, y0], spacing, size, rows }
    }
}

/// Value at point `(i, j)` (0..4 each) of a fog cell split 4 × 4 by 0x485606 from its
/// corners `[top left, top right, bottom left, bottom right]`: edge and centre midpoints
/// first, then the midpoints of those, each rounded down.
pub fn split([a, b, c, d]: [u16; 4], i: usize, j: usize) -> u16 {
    let [a, b, c, d] = [a, b, c, d].map(u32::from);
    let top = (a + b) >> 1;
    let left = (a + c) >> 1;
    let right = (b + d) >> 1;
    let bottom = (c + d) >> 1;
    let mid = (a + d + b + c) >> 2;
    let v = match (i, j) {
        (0, 0) => a,
        (1, 0) => (a + top) >> 1,
        (2, 0) => top,
        (3, 0) => (top + b) >> 1,
        (0, 1) => (a + left) >> 1,
        (1, 1) => (a + left + top + mid) >> 2,
        (2, 1) => (top + mid) >> 1,
        (3, 1) => (top + b + mid + right) >> 2,
        (0, 2) => left,
        (1, 2) => (left + mid) >> 1,
        (2, 2) => mid,
        (3, 2) => (mid + right) >> 1,
        (0, 3) => (left + c) >> 1,
        (1, 3) => (left + mid + c + bottom) >> 2,
        (2, 3) => (mid + bottom) >> 1,
        _ => (mid + right + bottom + d) >> 2,
    };
    v as u16
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    /// Camera x, y, scale; 1 when the target encodes sRGB.
    camera: [f32; 4],
    /// Origin of the corner grid in original pixels, spacing.
    grid: [f32; 4],
    /// Corners per row and rows.
    size: [u32; 4],
    /// Opacity of black per darkness row.
    dark: [[f32; 4]; ROWS / 4],
}

struct Grid {
    size: [u32; 2],
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
}

/// Draws [`FogShades`] as black of the rows' opacity over what is already in the target.
pub struct FogRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    dark: [[f32; 4]; ROWS / 4],
    srgb: bool,
    grid: Option<Grid>,
    active: bool,
}

impl FogRenderer {
    /// `darkness[row]`: the opacity of black that row of `AGEW_1DR.GRD` amounts to.
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat, darkness: &[f32; ROWS]) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("fog.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fog globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fog"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fog"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fog"),
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
        Self { pipeline, layout, globals, dark, srgb: target.is_srgb(), grid: None, active: false }
    }

    /// Uploads the rows for the next [`FogRenderer::render`]; `None` draws no fog.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, camera: &Camera, shades: Option<&FogShades>) {
        self.active = false;
        let Some(s) = shades.filter(|s| s.rows.len() == (s.size[0] * s.size[1]) as usize && !s.rows.is_empty()) else {
            return;
        };
        if self.grid.as_ref().is_none_or(|g| g.size != s.size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("fog rows"),
                size: wgpu::Extent3d { width: s.size[0], height: s.size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fog"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
                    },
                ],
            });
            self.grid = Some(Grid { size: s.size, texture, group });
        }
        let grid = self.grid.as_ref().expect("created above");
        queue.write_texture(
            grid.texture.as_image_copy(),
            &s.rows,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s.size[0]), rows_per_image: None },
            wgpu::Extent3d { width: s.size[0], height: s.size[1], depth_or_array_layers: 1 },
        );
        let globals = Globals {
            camera: [camera.x, camera.y, camera.scale, if self.srgb { 1.0 } else { 0.0 }],
            grid: [
                (s.origin[0] * s.spacing[0]) as f32,
                (s.origin[1] * s.spacing[1]) as f32,
                s.spacing[0] as f32,
                s.spacing[1] as f32,
            ],
            size: [s.size[0], s.size[1], 0, 0],
            dark: self.dark,
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.active = true;
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (true, Some(grid)) = (self.active, &self.grid) else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &grid.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_keeps_corners_and_halves_edges() {
        let c = [1000, 2000, 3000, 4000];
        assert_eq!(split(c, 0, 0), 1000);
        assert_eq!(split(c, 2, 0), 1500);
        assert_eq!(split(c, 0, 2), 2000);
        assert_eq!(split(c, 2, 2), 2500);
        assert_eq!(split(c, 1, 0), 1250);
        assert_eq!(split(c, 3, 3), (2500 + 3000 + 3500 + 4000) / 4);
        // Rounded down at each step.
        assert_eq!(split([0, 3, 0, 0], 1, 0), 0);
        assert_eq!(split([0, 3, 0, 0], 3, 0), 2);
    }

    #[test]
    fn shades_cover_the_view() {
        let camera = Camera { x: 100.0, y: 40.0, scale: 1.0 };
        // Only fog cell (4, 4) (screen (128, 64)) is lit.
        let cell = |gx, gy| if (gx, gy) == (4, 4) { 8000 } else { 0 };
        let s = FogShades::new(&camera, [320.0, 160.0], false, cell, |v| if v >= 1500 { 5 } else { 158 });
        assert_eq!(s.origin, [3, 2]);
        assert_eq!(s.size, [12, 12]);
        let at = |x: i32, y: i32| s.rows[((y / 16 - 2) * 12 + x / 32 - 3) as usize];
        assert_eq!(at(128, 64), 5, "the lit cell's centre");
        assert_eq!(at(256, 64), 158, "the next cell's centre");
        assert_eq!(at(96, 64), 5, "a quarter cell to the left: 8000 * 3/4");
        let zoomed = FogShades::new(&camera, [320.0, 160.0], true, cell, |v| if v >= 1500 { 5 } else { 158 });
        assert_eq!(zoomed.spacing, [128, 64]);
        assert_eq!(zoomed.rows[(zoomed.size[0] + 1) as usize], 5, "corner (1, 1) is cell (4, 4)");
    }
}
