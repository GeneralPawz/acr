//! Full-colour images (menu backgrounds, previews, loading screens) drawn as scaled quads.

use std::ops::Range;
use std::rc::Rc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    screen: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    dst: [f32; 4],
    src: [f32; 4],
    color: [f32; 4],
}

/// An uploaded RGBA image.
#[derive(Clone)]
pub struct ImageTexture {
    pub width: u32,
    pub height: u32,
    group: Rc<wgpu::BindGroup>,
}

/// One image (or part of it) drawn into a screen rectangle.
#[derive(Clone)]
pub struct ImageDraw {
    pub texture: ImageTexture,
    /// Screen pixels: x, y, width, height.
    pub dst: [f32; 4],
    /// Source pixels inside the texture: x, y, width, height.
    pub src: [f32; 4],
    /// Multiplied with every texel, e.g. `[1.0, 1.0, 1.0, 0.5]` for half transparency.
    pub color: [f32; 4],
}

impl ImageDraw {
    /// The whole texture stretched over `dst`.
    pub fn full(texture: &ImageTexture, dst: [f32; 4]) -> Self {
        let src = [0.0, 0.0, texture.width as f32, texture.height as f32];
        Self { texture: texture.clone(), dst, src, color: [1.0; 4] }
    }
}

pub struct ImageRenderer {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    image_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    srgb: bool,
    instances: wgpu::Buffer,
    batches: Vec<(Rc<wgpu::BindGroup>, Range<u32>)>,
}

impl ImageRenderer {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("image.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image"),
            bind_group_layouts: &[Some(&globals_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
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
        let instances = Self::instance_buffer(device, 64);
        Self {
            pipeline,
            globals,
            globals_group,
            image_layout,
            sampler,
            srgb: target.is_srgb(),
            instances,
            batches: Vec::new(),
        }
    }

    fn instance_buffer(device: &wgpu::Device, count: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image instances"),
            size: count * size_of::<Instance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Uploads straight-alpha RGBA pixels (sRGB colour values, top row first).
    pub fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> ImageTexture {
        let format = if self.srgb { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("image"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
                },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        ImageTexture { width, height, group: Rc::new(group) }
    }

    /// Uploads instance data for `draws`; call before [`ImageRenderer::render`].
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, screen: [f32; 2], draws: &[ImageDraw]) {
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&Globals { screen, _pad: [0.0; 2] }));
        self.batches.clear();
        let instances: Vec<Instance> = draws
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let i = i as u32;
                match self.batches.last_mut() {
                    Some((group, r)) if Rc::ptr_eq(group, &d.texture.group) => r.end = i + 1,
                    _ => self.batches.push((d.texture.group.clone(), i..i + 1)),
                }
                let (w, h) = (d.texture.width as f32, d.texture.height as f32);
                Instance { dst: d.dst, src: [d.src[0] / w, d.src[1] / h, d.src[2] / w, d.src[3] / h], color: d.color }
            })
            .collect();
        if (instances.len() * size_of::<Instance>()) as u64 > self.instances.size() {
            self.instances = Self::instance_buffer(device, (instances.len() as u64).next_power_of_two());
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.render_range(pass, 0..u32::MAX);
    }

    /// Draws the prepared draws with indices in `range` (positions in the list given to
    /// [`ImageRenderer::prepare`]), so that other passes can go between two parts.
    pub fn render_range(&self, pass: &mut wgpu::RenderPass<'_>, range: Range<u32>) {
        let mut set = false;
        for (group, r) in &self.batches {
            let part = r.start.max(range.start)..r.end.min(range.end);
            if part.is_empty() {
                continue;
            }
            if !set {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.globals_group, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                set = true;
            }
            pass.set_bind_group(1, group.as_ref(), &[]);
            pass.draw(0..4, part);
        }
    }
}
