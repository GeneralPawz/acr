//! The water surface of the 3D pass (docs/water.md, phase 1): a grid at the water level over
//! the visible part of the map, moved by Gerstner waves scaled by the `SEA2` depth, with depth
//! colour and absorption over the lakebed, a sky reflection, the sun's glint and shore foam
//! (`water.wgsl`). It replaces the original's depth tint (`terrain.wgsl`, `fs_present`) when
//! drawn, for the fixed and the free camera alike. Render only: nothing goes back to the
//! simulation.

use bytemuck::{Pod, Zeroable};

use crate::math::{self, Mat4};
use crate::model_renderer::DEPTH_FORMAT;
use crate::terrain::TerrainData;
use crate::view3d::View3d;

/// Largest wave height (world px) above or below the water level, for culling.
const WAVE_REACH: f32 = 12.0;
/// The free camera's grid reaches this far (world px) from the view's target at most.
const FREE_REACH: f32 = 7000.0;
/// Grid cells per side: about this many world px per cell, between the limits.
const CELL_PX: f32 = 6.0;
const MIN_CELLS: u32 = 16;
const MAX_CELLS: u32 = 192;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view: Mat4,
    rect: [f32; 4],
    eye: [f32; 4],
    sun: [f32; 4],
    grid: [f32; 4],
}

struct WaterMap {
    /// Which depth grid was uploaded (its size and address).
    key: (usize, usize, usize),
    /// World size of the grid (px).
    extent: [f32; 2],
    group: wgpu::BindGroup,
}

pub struct WaterRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    sampler: wgpu::Sampler,
    map: Option<WaterMap>,
    /// Vertices of this frame's grid (0: nothing to draw).
    vertices: u32,
}

impl WaterRenderer {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("water.wgsl"));
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: both,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: both,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: both,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_water"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            // Depth-tested against the terrain and the models (a hull below the waterline is
            // hidden), not written: the sprites draw over the water as over the ground.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_water"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("water depth"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { pipeline, layout, globals, sampler, map: None, vertices: 0 }
    }

    /// Uploads the map's depth grid when it changed (another map).
    fn set_map(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &TerrainData) -> bool {
        let Some((w, h, cells)) = data.water() else {
            self.map = None;
            return false;
        };
        let key = (w, h, cells.as_ptr() as usize);
        if self.map.as_ref().is_some_and(|m| m.key == key) {
            return true;
        }
        if !cells.iter().any(|&d| d > 127) {
            self.map = None;
            return false;
        }
        use wgpu::util::DeviceExt;
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("water depth"),
                size: wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            cells,
        );
        let view = texture.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.map = Some(WaterMap { key, extent: [w as f32 * 32.0, h as f32 * 32.0], group });
        true
    }

    /// Prepares this frame's surface for `view`: `seconds` drives the waves, `detailed` the
    /// waves, glint and foam (`water=detailed`; else a flat surface, for the free camera
    /// under `water=original`). False when there is no water to draw.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &TerrainData,
        view: &View3d,
        seconds: f32,
        detailed: bool,
    ) -> bool {
        self.vertices = 0;
        if !self.set_map(device, queue, data) {
            return false;
        }
        let map = self.map.as_ref().expect("set above");
        let [mut x0, mut y0, mut x1, mut y1] = view.ground_bounds(-WAVE_REACH, WAVE_REACH);
        if view.is_free() {
            let t = view.target();
            x0 = x0.max(t[0] - FREE_REACH);
            y0 = y0.max(t[1] - FREE_REACH);
            x1 = x1.min(t[0] + FREE_REACH);
            y1 = y1.min(t[1] + FREE_REACH);
        }
        let margin = 2.0 * WAVE_REACH;
        let rect = [
            (x0 - margin).max(0.0),
            (y0 - margin).max(0.0),
            (x1 + margin).min(map.extent[0]),
            (y1 + margin).min(map.extent[1]),
        ];
        if rect[2] <= rect[0] || rect[3] <= rect[1] {
            return false;
        }
        let side = (rect[2] - rect[0]).max(rect[3] - rect[1]);
        let cells = ((side / CELL_PX) as u32).clamp(MIN_CELLS, MAX_CELLS);
        let forward = view.basis().forward;
        let mut eye = math::scale(forward, -1.0);
        if eye[2] < 0.0 {
            eye = forward;
        }
        let sun = crate::shadow::sun();
        // Waves repeat; the clock wraps hourly to keep the phases precise in f32.
        let time = seconds % 3600.0;
        let globals = Globals {
            view: view.world_to_clip(),
            rect,
            eye: [eye[0], eye[1], eye[2], time],
            sun: [sun[0], sun[1], sun[2], if detailed { 1.0 } else { 0.0 }],
            grid: [map.extent[0] / 32.0, map.extent[1] / 32.0, cells as f32, 0.0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.vertices = cells * cells * 6;
        true
    }

    /// Draws the surface prepared by [`Self::prepare`] into the 3D pass, after the models.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(map) = &self.map else { return };
        if self.vertices == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &map.group, &[]);
        pass.draw(0..self.vertices, 0..1);
    }
}
