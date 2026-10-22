//! GPU renderer for decoded American Conquest sprites.
//!
//! Sprite layers are uploaded as palette indices into atlas pages; the shader resolves
//! palette and player colours and applies each layer's blend mode. The same code runs on
//! Vulkan/DX12/Metal natively and on WebGPU/WebGL2 in the browser.

pub mod image;
pub mod objects_view;
pub mod scene;
pub mod terrain;

use std::ops::Range;

use acr_formats::raster::{SHADE_ALPHA, SHADOW_LEVEL, TRANSLUCENT_ALPHA};
use acr_formats::{Encoding, Frame, Palette};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub use image::{ImageDraw, ImageRenderer, ImageTexture};

/// Bit of an instance's kind that halves its opacity (`sprite.wgsl`).
const TRANSPARENT: u32 = 0x100;

/// Atlas page edge length. WebGL2 guarantees 2048.
const PAGE_SIZE: u32 = 2048;

/// Blend mode of a layer; must match the constants in `sprite.wgsl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum LayerKind {
    Opaque = 0,
    Player = 1,
    Translucent = 2,
    ShadowLevels = 3,
    Shadow = 4,
}

impl LayerKind {
    /// Blend mode and its parameter, or `None` for layers that draw nothing.
    /// Alphas come from the original blend tables, see `acr_formats::raster`.
    fn of(encoding: Encoding) -> Option<(Self, u32)> {
        Some(match encoding {
            Encoding::DictLz | Encoding::SelfLz(_) => (Self::Opaque, 0),
            Encoding::PlayerColor => (Self::Player, 0),
            Encoding::Raw(level) => (Self::Translucent, u32::from(TRANSLUCENT_ALPHA[usize::from(level - 2)])),
            Encoding::Shade4 | Encoding::Shade8 => (Self::ShadowLevels, 0),
            Encoding::Shadow => (Self::Shadow, u32::from(SHADE_ALPHA[usize::from(SHADOW_LEVEL)])),
            Encoding::Empty | Encoding::Unknown(_) => return None,
        })
    }

    /// The atlas value of a pixel; darkness levels are stored as their alpha.
    fn texel(self, value: u8) -> u8 {
        match self {
            Self::ShadowLevels => SHADE_ALPHA[usize::from(value.min(31))],
            _ => value,
        }
    }
}

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
    src: [u32; 4],
    kind: u32,
    param: u32,
}

#[derive(Debug, Clone, Copy)]
struct LayerSlot {
    page: u32,
    atlas: [u32; 4],
    offset: [i32; 2],
    kind: LayerKind,
    param: u32,
}

/// The layers of a set of frames, uploaded to the GPU.
pub struct SpriteAtlas {
    pages: Vec<wgpu::BindGroup>,
    slots: Vec<LayerSlot>,
    frames: Vec<Range<usize>>,
}

impl SpriteAtlas {
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

/// One frame to draw with its origin at a screen position, scaled by `scale`.
#[derive(Debug, Clone, Copy)]
pub struct SpriteDraw {
    pub frame: usize,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub player: u8,
    /// Half transparent (`ShowGPTransparent`).
    pub transparent: bool,
}

pub struct SpriteRenderer {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    page_layout: wgpu::BindGroupLayout,
    palette: wgpu::Texture,
    instances: wgpu::Buffer,
    /// (page, instance range) batches prepared for the next `render`.
    batches: Vec<(u32, Range<u32>)>,
}

impl SpriteRenderer {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("sprite.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Palette colours are sRGB; sample them as such when the target encodes sRGB.
        let palette_format =
            if target.is_srgb() { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
        let palette = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("palette"),
            size: wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: palette_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_entry = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let page_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite page"),
            entries: &[texture_entry(0, wgpu::TextureSampleType::Uint)],
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite globals"),
            layout: &globals_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&palette.create_view(&Default::default())),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite"),
            bind_group_layouts: &[Some(&globals_layout), Some(&page_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Uint32x4, 2 => Uint32, 3 => Uint32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite"),
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
        let instances = Self::instance_buffer(device, 1024);
        Self { pipeline, globals, globals_group, page_layout, palette, instances, batches: Vec::new() }
    }

    fn instance_buffer(device: &wgpu::Device, count: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite instances"),
            size: count * size_of::<Instance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn set_palette(&self, queue: &wgpu::Queue, palette: &Palette) {
        let rgba: Vec<u8> = palette.0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
        queue.write_texture(
            self.palette.as_image_copy(),
            &rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256 * 4), rows_per_image: None },
            wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
        );
    }

    /// Packs all layers of `frames` into atlas pages and uploads them.
    pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, frames: &[Frame]) -> SpriteAtlas {
        let mut packer = ShelfPacker::default();
        let mut page_pixels: Vec<Vec<u8>> = Vec::new();
        let mut slots = Vec::new();
        let mut frame_ranges = Vec::with_capacity(frames.len());
        for frame in frames {
            let start = slots.len();
            for layer in &frame.layers {
                let Some((kind, param)) = LayerKind::of(layer.encoding) else { continue };
                let w = layer.runs.iter().map(|r| u32::from(r.x) + u32::from(r.len)).max().unwrap_or(0);
                let h = layer.runs.iter().map(|r| u32::from(r.y) + 1).max().unwrap_or(0);
                if w == 0 || w > PAGE_SIZE || h > PAGE_SIZE {
                    continue;
                }
                let (page, x, y) = packer.place(w, h);
                if page as usize == page_pixels.len() {
                    page_pixels.push(vec![0; (PAGE_SIZE * PAGE_SIZE * 2) as usize]);
                }
                let pixels = &mut page_pixels[page as usize];
                let mut values = layer.pixels.iter();
                for run in &layer.runs {
                    let row = (y + u32::from(run.y)) * PAGE_SIZE;
                    for i in 0..u32::from(run.len) {
                        let at = ((row + x + u32::from(run.x) + i) * 2) as usize;
                        pixels[at] = kind.texel(values.next().copied().unwrap_or(0));
                        pixels[at + 1] = 1;
                    }
                }
                slots.push(LayerSlot {
                    page,
                    atlas: [x, y, w, h],
                    offset: [i32::from(layer.x), i32::from(layer.y)],
                    kind,
                    param,
                });
            }
            frame_ranges.push(start..slots.len());
        }
        let pages = page_pixels
            .iter()
            .map(|pixels| {
                let texture = device.create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some("sprite page"),
                        size: wgpu::Extent3d { width: PAGE_SIZE, height: PAGE_SIZE, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rg8Uint,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    pixels,
                );
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sprite page"),
                    layout: &self.page_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
                    }],
                })
            })
            .collect();
        SpriteAtlas { pages, slots, frames: frame_ranges }
    }

    /// Uploads instance data for `sprites`; call before [`SpriteRenderer::render`].
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        atlas: &SpriteAtlas,
        screen: [f32; 2],
        sprites: &[SpriteDraw],
    ) {
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&Globals { screen, _pad: [0.0; 2] }));
        let mut instances = Vec::new();
        self.batches.clear();
        for sprite in sprites {
            let Some(range) = atlas.frames.get(sprite.frame) else { continue };
            for slot in &atlas.slots[range.clone()] {
                let [ax, ay, w, h] = slot.atlas;
                let x = sprite.x + slot.offset[0] as f32 * sprite.scale;
                let y = sprite.y + slot.offset[1] as f32 * sprite.scale;
                let param = if slot.kind == LayerKind::Player { u32::from(sprite.player % 8) } else { slot.param };
                let index = instances.len() as u32;
                match self.batches.last_mut() {
                    Some((page, r)) if *page == slot.page => r.end = index + 1,
                    _ => self.batches.push((slot.page, index..index + 1)),
                }
                instances.push(Instance {
                    dst: [x, y, w as f32 * sprite.scale, h as f32 * sprite.scale],
                    src: [ax, ay, w, h],
                    kind: slot.kind as u32 | if sprite.transparent { TRANSPARENT } else { 0 },
                    param,
                });
            }
        }
        let needed = (instances.len() * size_of::<Instance>()) as u64;
        if needed > self.instances.size() {
            self.instances = Self::instance_buffer(device, (instances.len() as u64).next_power_of_two());
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>, atlas: &SpriteAtlas) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        for (page, range) in &self.batches {
            pass.set_bind_group(1, &atlas.pages[*page as usize], &[]);
            pass.draw(0..4, range.clone());
        }
    }
}

/// Places rectangles on fixed-size pages, row by row.
#[derive(Default)]
struct ShelfPacker {
    page: u32,
    x: u32,
    y: u32,
    shelf_height: u32,
}

impl ShelfPacker {
    /// Returns `(page, x, y)`; a 1px gap keeps neighbouring layers from bleeding.
    fn place(&mut self, w: u32, h: u32) -> (u32, u32, u32) {
        if self.x + w > PAGE_SIZE {
            self.x = 0;
            self.y += self.shelf_height + 1;
            self.shelf_height = 0;
        }
        if self.y + h > PAGE_SIZE {
            self.page += 1;
            (self.x, self.y, self.shelf_height) = (0, 0, 0);
        }
        let at = (self.page, self.x, self.y);
        self.x += w + 1;
        self.shelf_height = self.shelf_height.max(h);
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packer_wraps_rows_and_pages() {
        let mut p = ShelfPacker::default();
        assert_eq!(p.place(1500, 10), (0, 0, 0));
        assert_eq!(p.place(1500, 20), (0, 0, 11));
        assert_eq!(p.place(10, 2040), (1, 0, 0));
    }
}
