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

use crate::damage::{self, Damage};
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
    /// Cuts, fractures, a torn trunk, or the piece of a body this instance draws
    /// (`docs/damage-model.md`); the default draws it whole.
    pub damage: Damage,
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
    /// Wounds placed where the hits struck (#277), on the living and the dead: the first
    /// `marks_len` of `marks`.
    pub marks: [Mark; MAX_MARKS],
    pub marks_len: u8,
}

/// Most wounds placed per body ([`Wounds::marks`]).
pub const MAX_MARKS: usize = 8;

/// One wound where a hit struck (#277), in the rest pose (glTF metres): a hole, blood soaking
/// the cloth around it and running down from it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Mark {
    /// Centre on the body's surface (rest pose).
    pub point: [f32; 3],
    /// Radius of the hole (metres).
    pub radius: f32,
    /// A ragged exit wound: torn flesh around a larger hole.
    pub ragged: bool,
    /// How far the blood has run down from it (metres).
    pub run: f32,
    /// How far the cloth around it is soaked, 0..1.
    pub soak: f32,
    /// 0 fresh .. 1 dried.
    pub age: f32,
    pub seed: f32,
}

impl Mark {
    fn gpu(&self) -> [[f32; 4]; 2] {
        let r = if self.ragged { -self.radius } else { self.radius };
        [
            [self.point[0], self.point[1], self.point[2], r],
            [self.run, self.soak.clamp(0.0, 1.0), self.age.clamp(0.0, 1.0), self.seed],
        ]
    }
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
    /// Not blood but churned earth where a round struck the ground.
    pub dirt: Dirt,
    /// The ground's slope under the decal (world z per world x and per world y), so it
    /// lies on the slope instead of sinking into it.
    pub tilt: [f32; 2],
    /// A piece of a smear (a body dragged through blood): a ribbon from `centre` running
    /// `along` instead of a pool. `radius` is its half width where it ends.
    pub smear: Option<Smear>,
}

/// One segment of a smeared blood trail: consecutive segments of one unit join into a
/// ribbon with ragged edges and streaks along the drag (`blood.wgsl`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Smear {
    /// Length from `centre` along `along` (world pixels).
    pub length: f32,
    /// Half width where it starts, relative to `radius` (where it ends), so the ribbon
    /// widens and narrows smoothly from one segment to the next.
    pub start_width: f32,
    /// How much blood: 1 a heavy, dark smear (he was slow) .. 0 a thin, broken one.
    pub density: f32,
}

/// The earth thrown up where a round strikes the ground (a [`BloodPool`] decal in dirt
/// colours).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Dirt {
    /// Blood.
    #[default]
    None,
    /// A cannonball's or shell's crater: a dark pit in a ring of thrown earth.
    Crater,
    /// A musket ball's, arrow's or grapeshot's scuff: a small dusty smudge.
    Scuff,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPool {
    centre: [f32; 4],
    shape: [f32; 4],
    look: [f32; 4],
    tilt: [f32; 4],
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
    /// x: stump cap (wet, ragged outline).
    look: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuInstance {
    rows: [[f32; 4]; 3],
    params: [f32; 4],
    /// First texel of the pose, first texel of the instance data (`model.wgsl`), player, 0.
    extra: [u32; 4],
}

/// The data texels of an instance in the bone texture: wounds, spray and (when damaged)
/// the damage block of [`damage::pack`].
fn instance_texels(inst: &ModelInstance, rig: Option<&damage::Rig>, out: &mut Vec<[f32; 4]>) {
    let [w0, w1] = inst.wounds.gpu();
    let s = &inst.sprayed;
    let damaged = !inst.damage.is_none();
    let marks = &inst.wounds.marks[..usize::from(inst.wounds.marks_len).min(MAX_MARKS)];
    out.extend([
        w0,
        w1,
        [s.point[0], s.point[1], s.point[2], s.radius],
        [s.amount.clamp(0.0, 1.0), s.age.clamp(0.0, 1.0), f32::from(u8::from(damaged)), marks.len() as f32],
    ]);
    if damaged {
        out.extend(damage::pack(&inst.damage, rig));
    }
    // The marks follow the damage block (`model.wgsl`, `marked`).
    for m in marks {
        out.extend(m.gpu());
    }
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
    /// The model placed at every cut ([`damage::procedural_cap`] until [`Self::set_cap`]).
    cap: usize,
    /// Prepared for the next `render`: model, its instances, whether they decay.
    draws: Vec<(usize, Range<u32>, bool)>,
    /// The target does not encode sRGB: the shader does.
    encode_srgb: bool,
    /// Palettes uploaded by the last `prepare`.
    pub pose_count: usize,
    /// Instances drawn by the last `prepare`.
    pub drawn: usize,
    /// Palettes given from outside (a physics ragdoll) for instances by their index in the
    /// next `prepare`'s slice, in place of their clip (`set_custom_poses`).
    custom: HashMap<usize, crate::model::Palette>,
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

/// `look.z` of a decal in `blood.wgsl`: 0 pool, 1 droplet, 2 crater, 3 scuff, 4 smear.
fn pool_kind(p: &BloodPool) -> f32 {
    match p.dirt {
        Dirt::Crater => 2.0,
        Dirt::Scuff => 3.0,
        Dirt::None if p.smear.is_some() => 4.0,
        Dirt::None => f32::from(u8::from(p.splatter)),
    }
}

/// The GPU form of a decal: a smear's quad reaches `length` past its start (`stretch` in
/// pool radii), its start width and density ride in `tilt.z` and `look.w`.
fn gpu_pool(p: &BloodPool) -> GpuPool {
    let (stretch, start, density) = match p.smear {
        Some(s) => (s.length / (2.0 * p.radius), s.start_width, s.density),
        None => (p.stretch, 0.0, 0.0),
    };
    GpuPool {
        centre: [p.centre[0], p.centre[1], p.centre[2], p.radius],
        shape: [p.along[0], p.along[1], stretch, p.seed],
        look: [p.age, p.fade, pool_kind(p), density],
        tilt: [p.tilt[0], p.tilt[1], start, 0.0],
    }
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
            entries: &[uniform(0), texture(1, both, false)],
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
        // 14 of the 16 locations WebGL2 has: wounds, spray and damage live in the bone texture.
        let vertex_attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4,
            4 => Uint32x4, 5 => Float32x4, 6 => Float32x3, 7 => Float32x3, 8 => Float32x4
        ];
        let instance_attributes = wgpu::vertex_attr_array![
            9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4, 13 => Uint32x4
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
        let pool_attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
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
        let mut renderer = Self {
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
            cap: 0,
            draws: Vec::new(),
            encode_srgb: !target.is_srgb(),
            pose_count: 0,
            drawn: 0,
            custom: HashMap::new(),
        };
        renderer.cap = renderer.add_model(device, queue, damage::procedural_cap());
        renderer
    }

    /// Replaces the stump cap model (the modelled cap of `gore.glb`): a unit disc in xy
    /// facing +z, placed at every cut and scaled by the limb's radius.
    pub fn set_cap(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: ModelData) {
        let index = self.add_model(device, queue, data);
        self.models.swap(self.cap, index);
        self.models.pop();
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
                        if d.is_some_and(|d| d.inner) || m.stump { 1.0 } else { 0.0 },
                    ],
                    look: [if m.stump { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
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

    /// Palettes for the next [`ModelRenderer::prepare`]: instance index → palette (the
    /// model's slots in order, as [`ModelData::pose`] gives them), drawn instead of the
    /// instance's clip. Used once.
    pub fn set_custom_poses(&mut self, poses: HashMap<usize, crate::model::Palette>) {
        self.custom = poses;
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
        let mut data: Vec<[f32; 4]> = Vec::new();
        let mut caps: Vec<ModelInstance> = Vec::new();
        self.draws.clear();
        let [w, h] = view.size;
        let cap_model = self.cap;
        let models = &self.models;
        let draws = &mut self.draws;
        let custom = std::mem::take(&mut self.custom);
        let mut push = |i: usize, inst: &ModelInstance, caps: &mut Vec<ModelInstance>| {
            let model = &models[inst.model].data;
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
                return;
            }
            let key = (inst.model, inst.clip, inst.time.to_bits());
            let base = if let Some(palette) = custom.get(&i) {
                let base = texels.len() as u32;
                for rows in palette {
                    texels.extend_from_slice(rows);
                }
                base
            } else {
                *poses.entry(key).or_insert_with(|| {
                    let base = texels.len() as u32;
                    for rows in model.pose(inst.clip, inst.time) {
                        texels.extend_from_slice(&rows);
                    }
                    base
                })
            };
            // Caps at the cuts, carried by the bone at the cut in this pose.
            if let Some(rig) = model.rig.as_ref().filter(|_| !inst.damage.is_none()) {
                for cap in damage::caps(&inst.damage, rig) {
                    let at = (base + cap.slot * 3) as usize;
                    let Some(rows) = texels.get(at..at + 3) else { continue };
                    let bone: Mat4 =
                        std::array::from_fn(|c| [rows[0][c], rows[1][c], rows[2][c], if c == 3 { 1.0 } else { 0.0 }]);
                    caps.push(ModelInstance {
                        model: cap_model,
                        clip: None,
                        time: 0.0,
                        transform: math::mul(&inst.transform, &math::mul(&bone, &cap.rest_matrix())),
                        morphs: [0.0; 2],
                        wounds: Default::default(),
                        sprayed: Default::default(),
                        damage: Damage::default(),
                        ..*inst
                    });
                }
            }
            let index = gpu.len() as u32;
            let decaying = inst.decay > 0.0;
            match draws.last_mut() {
                Some((m, range, d)) if *m == inst.model && *d == decaying => range.end = index + 1,
                _ => draws.push((inst.model, index..index + 1, decaying)),
            }
            let first = data.len() as u32;
            instance_texels(inst, model.rig.as_ref(), &mut data);
            gpu.push(GpuInstance {
                rows: math::affine_rows(&inst.transform),
                params: [inst.decay, inst.morphs[0], inst.morphs[1], if inst.transparent { 1.0 } else { 0.0 }],
                extra: [base, first, u32::from(inst.player), 0],
            });
        };
        for &i in &order {
            push(i, &instances[i], &mut caps);
        }
        let mut none = Vec::new();
        for inst in &caps {
            push(usize::MAX, inst, &mut none);
        }
        // The instance data follows the palettes; `extra.y` becomes absolute.
        let data_base = texels.len() as u32;
        texels.extend_from_slice(&data);
        gpu.iter_mut().for_each(|g| g.extra[1] += data_base);
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
        let gpu: Vec<GpuPool> = pools.iter().filter(|p| p.fade > 0.0 && p.radius > 0.0).map(gpu_pool).collect();
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
