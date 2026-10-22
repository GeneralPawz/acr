//! Draws [`ModelData`] models on the GPU: skinned in the vertex shader from a float texture
//! of palettes (WebGL2-friendly), instanced per model type, depth-tested against the
//! terrain and each other in a pass shared with the sprites ([`DepthTarget`]).
//!
//! One frame: [`ModelRenderer::prepare`] samples each distinct pose once (instances of the
//! same model, clip and frame share it), uploads palettes and instances; then, in the pass
//! [`DepthTarget::begin`] opens, the terrain writes its depth
//! (`TerrainRenderer::render_depth`), [`ModelRenderer::render`] draws the models (marking
//! their pixels in the stencil), [`DepthTarget::reset_ground`] moves everything else back to
//! the far plane, and the sprites draw with a depth test against the models only.
//!
//! Corpses carry [`Wounds`] (blood, dirt and burns by the cause of death) and leave
//! [`BloodPool`]s: flat noisy decals on the ground, drawn before the models.

use std::collections::HashMap;
use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::math::{self, Mat4};
use crate::model::{ModelData, Vertex};
use crate::view3d::View3d;

/// Depth and stencil of the 3D pass (supported everywhere, WebGL2 included).
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
/// Texels per row of the bone texture.
const BONE_WIDTH: u32 = 1024;
/// Direction to the light in world coordinates (east, south, up): the game's light vector
/// (`SetLight(-15, 14, 30)`, from the upper left and the viewer's side).
const LIGHT: [f32; 3] = [-15.0, 14.0, 30.0];
/// Share of unlit colour.
const AMBIENT: f32 = 0.5;

/// One model to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelInstance {
    /// Index returned by [`ModelRenderer::add_model`].
    pub model: usize,
    /// Clip index ([`ModelData::clip`]); `None` is the bind pose.
    pub clip: Option<usize>,
    /// Clip time in seconds.
    pub time: f32,
    /// Model (glTF metres) → world ([`crate::view3d::model_matrix`]).
    pub transform: Mat4,
    /// Decay 0..1 (`docs/assets-humans.md`).
    pub decay: f32,
    /// Weights of the morph targets `deflate` and `lie_flat`.
    pub morphs: [f32; 2],
    /// Every other pixel only (the original's half transparent corpses).
    pub transparent: bool,
    pub player: u8,
    /// Wounds, blood, dirt and burns of a corpse (all zero for the living).
    pub wounds: Wounds,
    /// Blood sprayed onto it from someone hit in front of it (troops and buildings).
    pub sprayed: Sprayed,
}

/// Blood sprayed onto a model from a hit nearby: speckles around a point of its rest pose
/// (glTF metres), fresh red drying to brown.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sprayed {
    pub point: [f32; 3],
    /// Radius of the speckled area (metres).
    pub radius: f32,
    /// 0..1.
    pub amount: f32,
    /// 0 fresh .. 1 dried.
    pub age: f32,
}

/// Blood, dirt and burns on a body (deaths by cause, `docs/rendering-3d.md`). Wounds sit in
/// the model's rest pose (glTF metres, +y up, facing +z), so they stay on the body whatever
/// pose it lies in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Wounds {
    /// Direction to the blow in the model's frame (glTF x, z); zero: from the front.
    pub from: [f32; 2],
    /// Blood on the body, 0..1.
    pub blood: f32,
    /// Dirt, 0..1.
    pub dirt: f32,
    /// Charring, 0..1.
    pub burn: f32,
    /// Number of wounds (0..6): the first faces the blow; with two the second is the exit.
    pub count: u8,
    /// Wound radius in metres.
    pub radius: f32,
    /// Varies the places of the further wounds per corpse.
    pub seed: f32,
}

impl Wounds {
    fn gpu(&self) -> [[f32; 4]; 2] {
        [
            [self.from[0], self.from[1], self.blood, self.dirt],
            [self.burn, f32::from(self.count.min(6)), self.radius, self.seed],
        ]
    }
}

/// A pool of blood on the ground under a corpse, or (`splatter`) a droplet sprayed onto the
/// ground behind a hit unit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BloodPool {
    /// Centre on the ground (world coordinates).
    pub centre: [f32; 3],
    /// Radius (world pixels).
    pub radius: f32,
    /// Direction the pool runs out to (world x, y; unit or zero) and how far (0: round, 1:
    /// twice as long).
    pub along: [f32; 2],
    pub stretch: f32,
    /// 0 fresh red .. 1 dried and soaked in.
    pub age: f32,
    /// 1 visible .. 0 gone.
    pub fade: f32,
    /// Varies the outline per pool.
    pub seed: f32,
    /// A droplet: a crisp round head with a tail running out along `along`, instead of a
    /// noisy pool.
    pub splatter: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPool {
    centre: [f32; 4],
    shape: [f32; 4],
    look: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: Mat4,
    light: [f32; 4],
    forward: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialUniform {
    base: [f32; 4],
    tint: [f32; 4],
    windows: [f32; 4],
    edge: [f32; 4],
    flags: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuInstance {
    rows: [[f32; 4]; 3],
    params: [f32; 4],
    extra: [u32; 4],
    wounds: [[f32; 4]; 2],
    spray: [f32; 4],
}

struct GpuPrimitive {
    indices: Range<u32>,
    material: usize,
    /// Hidden inside the body until it decays (the bone layer): drawn for decaying
    /// instances only, a third of a human's triangles.
    inner: bool,
}

struct GpuModel {
    data: ModelData,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    primitives: Vec<GpuPrimitive>,
    materials: Vec<wgpu::BindGroup>,
}

pub struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    pool_pipeline: wgpu::RenderPipeline,
    pools: wgpu::Buffer,
    pool_count: u32,
    globals: wgpu::Buffer,
    globals_layout: wgpu::BindGroupLayout,
    globals_group: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    bones: wgpu::Texture,
    instances: wgpu::Buffer,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    white: wgpu::TextureView,
    models: Vec<GpuModel>,
    /// Prepared for the next `render`: model, its instances, whether they decay.
    draws: Vec<(usize, Range<u32>, bool)>,
    /// The target does not encode sRGB: the shader does.
    encode_srgb: bool,
    /// Palettes uploaded by the last `prepare`.
    pub pose_count: usize,
    /// Instances drawn by the last `prepare`.
    pub drawn: usize,
}

fn bone_texture(device: &wgpu::Device, rows: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("model bones"),
        size: wgpu::Extent3d { width: BONE_WIDTH, height: rows, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn pool_buffer(device: &wgpu::Device, count: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blood pools"),
        size: count.max(1) * size_of::<GpuPool>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn instance_buffer(device: &wgpu::Device, count: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model instances"),
        size: count.max(1) * size_of::<GpuInstance>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl ModelRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("model.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: both,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding, visibility, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model globals"),
            entries: &[uniform(0), texture(1, wgpu::ShaderStages::VERTEX, false)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model material"),
            entries: &[
                uniform(0),
                texture(1, wgpu::ShaderStages::FRAGMENT, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4,
            4 => Uint32x4, 5 => Float32x4, 6 => Float32x3, 7 => Float32x3
        ];
        let instance_attributes = wgpu::vertex_attr_array![
            8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Uint32x4,
            13 => Float32x4, 14 => Float32x4, 15 => Float32x4
        ];
        let stencil_mark = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Replace,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attributes,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<GpuInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &instance_attributes,
                    }),
                ],
            },
            // Clothes are thin shells seen from both sides (holes when they decay): no culling.
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState {
                    front: stencil_mark,
                    back: stencil_mark,
                    read_mask: 0xff,
                    write_mask: 0xff,
                },
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Blood pools: decals over the terrain's depth, blended, writing neither depth nor
        // stencil (the models and sprites cover them).
        let pool_shader = device.create_shader_module(wgpu::include_wgsl!("blood.wgsl"));
        let pool_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blood pools"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let pool_attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let pool_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blood pools"),
            layout: Some(&pool_layout),
            vertex: wgpu::VertexState {
                module: &pool_shader,
                entry_point: Some("vs_pool"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<GpuPool>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &pool_attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &pool_shader,
                entry_point: Some("fs_pool"),
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
        let bones = bone_texture(device, 1);
        let globals_group = Self::globals_group(device, &globals_layout, &globals, &bones);
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("model texture"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let white = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("model white"),
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &[255; 4],
            )
            .create_view(&Default::default());
        Self {
            pipeline,
            pool_pipeline,
            pools: pool_buffer(device, 64),
            pool_count: 0,
            globals,
            globals_layout,
            globals_group,
            material_layout,
            bones,
            instances: instance_buffer(device, 256),
            linear: sampler(wgpu::FilterMode::Linear),
            nearest: sampler(wgpu::FilterMode::Nearest),
            white,
            models: Vec::new(),
            draws: Vec::new(),
            encode_srgb: !target.is_srgb(),
            pose_count: 0,
            drawn: 0,
        }
    }

    fn globals_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        bones: &wgpu::Texture,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model globals"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&bones.create_view(&Default::default())),
                },
            ],
        })
    }

    /// Uploads a model; returns its index for [`ModelInstance::model`].
    pub fn add_model(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: ModelData) -> usize {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut primitives = Vec::new();
        for p in &data.primitives {
            // Indices are offset here: WebGL2 has no base vertex.
            let base = vertices.len() as u32;
            let start = indices.len() as u32;
            vertices.extend_from_slice(&p.vertices);
            indices.extend(p.indices.iter().map(|i| i + base));
            let inner = data.materials.get(p.material).and_then(|m| m.decay).is_some_and(|d| d.inner);
            primitives.push(GpuPrimitive { indices: start..indices.len() as u32, material: p.material, inner });
        }
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let images: Vec<wgpu::TextureView> = data
            .images
            .iter()
            .map(|img| {
                device
                    .create_texture_with_data(
                        queue,
                        &wgpu::TextureDescriptor {
                            label: Some("model texture"),
                            size: wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8UnormSrgb,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        },
                        wgpu::util::TextureDataOrder::LayerMajor,
                        &img.rgba,
                    )
                    .create_view(&Default::default())
            })
            .collect();
        let materials = data
            .materials
            .iter()
            .map(|m| {
                let d = m.decay;
                let uniform = MaterialUniform {
                    base: m.base_color,
                    tint: d.map_or([0.0; 4], |d| [d.tint[0], d.tint[1], d.tint[2], d.tint_strength]),
                    windows: d.map_or([0.0, 1.0, 2.0, 2.0], |d| {
                        let [s0, s1] = d.dissolve.unwrap_or([2.0, 2.0]);
                        [d.discolour[0], d.discolour[1], s0, s1]
                    }),
                    edge: d.map_or([0.0; 4], |d| [d.edge_rgb[0], d.edge_rgb[1], d.edge_rgb[2], d.edge]),
                    flags: [
                        if d.is_some() { 1.0 } else { 0.0 },
                        if d.is_some() { 0.0 } else { 1.0 },
                        m.alpha_cutoff.unwrap_or(0.0),
                        // The bone layer takes no blood, dirt or burns.
                        if d.is_some_and(|d| d.inner) { 1.0 } else { 0.0 },
                    ],
                };
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("model material"),
                    contents: bytemuck::bytes_of(&uniform),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let view = m.texture.and_then(|t| images.get(t)).unwrap_or(&self.white);
                let sampler = if m.nearest { &self.nearest } else { &self.linear };
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("model material"),
                    layout: &self.material_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
                    ],
                })
            })
            .collect();
        self.models.push(GpuModel { data, vertices: vertex_buffer, indices: index_buffer, primitives, materials });
        self.models.len() - 1
    }

    pub fn model(&self, index: usize) -> Option<&ModelData> {
        self.models.get(index).map(|m| &m.data)
    }

    /// Samples the poses, uploads palettes and instances for `instances` seen through
    /// `view`; `dissolve` lets decaying layers dissolve (the gore setting).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &View3d,
        instances: &[ModelInstance],
        dissolve: bool,
    ) {
        let mut order: Vec<usize> = (0..instances.len()).filter(|&i| instances[i].model < self.models.len()).collect();
        // By model, the decaying ones last (they also draw the inner layers).
        order.sort_by_key(|&i| (instances[i].model, instances[i].decay > 0.0));
        let mut texels: Vec<[f32; 4]> = Vec::new();
        let mut poses: HashMap<(usize, Option<usize>, u32), u32> = HashMap::new();
        let mut gpu: Vec<GpuInstance> = Vec::with_capacity(order.len());
        self.draws.clear();
        let [w, h] = view.size;
        for i in order {
            let inst = &instances[i];
            let model = &self.models[inst.model].data;
            // Culling by the rest pose's radius (lying corpses reach about as far).
            let origin = [inst.transform[3][0], inst.transform[3][1], inst.transform[3][2]];
            let stretch = math::dot(
                [inst.transform[0][0], inst.transform[0][1], inst.transform[0][2]],
                [inst.transform[0][0], inst.transform[0][1], inst.transform[0][2]],
            )
            .sqrt()
                / crate::view3d::PIXELS_PER_METRE;
            let r = (model.radius * stretch * 1.5 + 8.0) * view.camera.scale;
            let p = view.project(origin);
            if p[0] < -r || p[0] > w + r || p[1] < -r || p[1] > h + r || !(0.0..=1.0).contains(&p[2]) {
                continue;
            }
            let key = (inst.model, inst.clip, inst.time.to_bits());
            let base = *poses.entry(key).or_insert_with(|| {
                let base = texels.len() as u32;
                for rows in model.pose(inst.clip, inst.time) {
                    texels.extend_from_slice(&rows);
                }
                base
            });
            let index = gpu.len() as u32;
            let decaying = inst.decay > 0.0;
            match self.draws.last_mut() {
                Some((m, range, d)) if *m == inst.model && *d == decaying => range.end = index + 1,
                _ => self.draws.push((inst.model, index..index + 1, decaying)),
            }
            gpu.push(GpuInstance {
                rows: math::affine_rows(&inst.transform),
                params: [inst.decay, inst.morphs[0], inst.morphs[1], if inst.transparent { 1.0 } else { 0.0 }],
                extra: [
                    base,
                    u32::from(inst.player),
                    (inst.sprayed.amount.clamp(0.0, 1.0) * 1000.0) as u32,
                    (inst.sprayed.age.clamp(0.0, 1.0) * 1000.0) as u32,
                ],
                wounds: inst.wounds.gpu(),
                spray: [inst.sprayed.point[0], inst.sprayed.point[1], inst.sprayed.point[2], inst.sprayed.radius],
            });
        }
        self.pose_count = poses.len();
        self.drawn = gpu.len();

        let rows = (texels.len() as u32).div_ceil(BONE_WIDTH).max(1);
        if rows > self.bones.height() {
            let rows = rows.next_power_of_two().min(device.limits().max_texture_dimension_2d);
            self.bones = bone_texture(device, rows);
            self.globals_group = Self::globals_group(device, &self.globals_layout, &self.globals, &self.bones);
        }
        let rows = rows.min(self.bones.height());
        texels.resize((rows * BONE_WIDTH) as usize, [0.0; 4]);
        queue.write_texture(
            self.bones.as_image_copy(),
            bytemuck::cast_slice(&texels),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(BONE_WIDTH * 16), rows_per_image: None },
            wgpu::Extent3d { width: BONE_WIDTH, height: rows, depth_or_array_layers: 1 },
        );
        let needed = (gpu.len() * size_of::<GpuInstance>()) as u64;
        if needed > self.instances.size() {
            self.instances = instance_buffer(device, (gpu.len() as u64).next_power_of_two());
        }
        if !gpu.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&gpu));
        }
        let len = math::dot(LIGHT, LIGHT).sqrt();
        let forward = view.basis().forward;
        let globals = Globals {
            view_proj: view.world_to_clip(),
            light: [LIGHT[0] / len, LIGHT[1] / len, LIGHT[2] / len, AMBIENT],
            forward: [forward[0], forward[1], forward[2], 0.0],
            params: [
                if self.encode_srgb { 1.0 } else { 0.0 },
                if dissolve { 1.0 } else { 0.0 },
                BONE_WIDTH as f32,
                0.0,
            ],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }

    /// Uploads the blood pools of the next `render` (after [`Self::prepare`], which sets the
    /// view they are drawn through).
    pub fn prepare_pools(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pools: &[BloodPool]) {
        let gpu: Vec<GpuPool> = pools
            .iter()
            .filter(|p| p.fade > 0.0 && p.radius > 0.0)
            .map(|p| GpuPool {
                centre: [p.centre[0], p.centre[1], p.centre[2], p.radius],
                shape: [p.along[0], p.along[1], p.stretch, p.seed],
                look: [p.age, p.fade, f32::from(u8::from(p.splatter)), 0.0],
            })
            .collect();
        self.pool_count = gpu.len() as u32;
        if gpu.is_empty() {
            return;
        }
        let needed = (gpu.len() * size_of::<GpuPool>()) as u64;
        if needed > self.pools.size() {
            self.pools = pool_buffer(device, (gpu.len() as u64).next_power_of_two());
        }
        queue.write_buffer(&self.pools, 0, bytemuck::cast_slice(&gpu));
    }

    /// Draws the blood pools, then the prepared instances, into a pass opened by
    /// [`DepthTarget::begin`].
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.pool_count > 0 {
            pass.set_pipeline(&self.pool_pipeline);
            pass.set_bind_group(0, &self.globals_group, &[]);
            pass.set_vertex_buffer(0, self.pools.slice(..));
            pass.draw(0..4, 0..self.pool_count);
        }
        if self.draws.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_stencil_reference(1);
        let stride = size_of::<GpuInstance>() as u64;
        for (model, range, decaying) in &self.draws {
            let m = &self.models[*model];
            pass.set_vertex_buffer(0, m.vertices.slice(..));
            // The instances are bound from their first one: no base instance on WebGL2.
            pass.set_vertex_buffer(
                1,
                self.instances.slice(u64::from(range.start) * stride..u64::from(range.end) * stride),
            );
            pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
            for p in m.primitives.iter().filter(|p| *decaying || !p.inner) {
                pass.set_bind_group(1, &m.materials[p.material], &[]);
                pass.draw_indexed(p.indices.clone(), 0, 0..range.end - range.start);
            }
        }
    }
}

/// The depth and stencil buffer of the 3D pass and the pipeline that resets the ground.
pub struct DepthTarget {
    size: (u32, u32),
    view: Option<wgpu::TextureView>,
    reset: wgpu::RenderPipeline,
}

impl DepthTarget {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("model.wgsl"));
        let keep_if_zero = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth reset"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let reset = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth reset"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_far"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState { front: keep_if_zero, back: keep_if_zero, read_mask: 0xff, write_mask: 0 },
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_none"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { size: (0, 0), view: None, reset }
    }

    /// Opens the 3D pass over `color` (kept) with a cleared depth and stencil buffer of the
    /// output size.
    pub fn begin<'e>(
        &mut self,
        device: &wgpu::Device,
        encoder: &'e mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        size: [f32; 2],
    ) -> wgpu::RenderPass<'e> {
        let size = ((size[0] as u32).max(1), (size[1] as u32).max(1));
        if self.size != size || self.view.is_none() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.view = Some(texture.create_view(&Default::default()));
            self.size = size;
        }
        let depth = self.view.as_ref().expect("created above");
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3d"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Moves every pixel no model drew to the far plane: sprites then draw over the terrain
    /// (as in the original) but behind models nearer than them.
    pub fn reset_ground(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.reset);
        pass.set_stencil_reference(0);
        pass.draw(0..3, 0..1);
    }
}

/// The depth state of pipelines drawn in the 3D pass after the models: test, no writes.
pub fn depth_test_only() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: Default::default(),
        bias: Default::default(),
    }
}
