//! Ground cover of 3D mode (docs/ground.md "Phase 5: ground cover"): grass tufts, flowers,
//! ferns, leaf litter, dry tufts and pebbles on the detailed ground, swaying in the wind.
//!
//! Placement is a jittered grid of [`CELL`] world pixels: each cell's hash (world position
//! only) decides whether it holds a piece, which kind ([`COVER`], by the ground class under
//! it), where in the cell, how big and how turned. Cells are built in chunks of
//! [`CHUNK`]² around the view and cached; a chunk is rebuilt every [`REFRESH`] seconds so
//! the ground state (`ground_state.rs`) thins and flattens it where troops walked
//! (compression), and clears it where it burned (scorch) or a ball dug in (displacement).
//! The cards are drawn by `ground_cover.wgsl` with procedural alpha shapes, no textures.
//! The zoom (and, for the free camera, the distance from its target) fades the cover out by
//! each piece's rank, in the shader. Render only.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::ground_state::GroundState;
use crate::math::Mat4;
use crate::model_renderer::DEPTH_FORMAT;
use crate::particles::{hash, unit};
use crate::view3d::{DEPTH_RANGE, PIXELS_PER_METRE, View3d};

/// World pixels per placement cell (40 cm).
pub const CELL: f32 = 12.0;
/// Cells per chunk side.
pub const CHUNK: i32 = 16;
/// Instances drawn at most.
pub const MAX_INSTANCES: usize = 40_000;
/// Seconds after which a chunk is placed again (for the ground state).
pub const REFRESH: f32 = 1.0;
/// Chunks placed per frame at most: new ones (a jump of the view fills in a frame or two),
/// then stale ones (the ground state's changes come in over a few frames).
const NEW_PER_FRAME: usize = 96;
const STALE_PER_FRAME: usize = 4;
/// The free camera's reach (world pixels from its target, ~110 m): beyond, no cover.
pub const FREE_REACH: f32 = 3300.0;

/// The kinds of ground cover (`color.a` of an [`Instance`]); `Litter` and `Pebble` lie flat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Grass = 0,
    Dry = 1,
    Flower = 2,
    Fern = 3,
    Litter = 4,
    Pebble = 5,
}

impl Kind {
    pub fn flat(self) -> bool {
        matches!(self, Kind::Litter | Kind::Pebble)
    }

    /// Height (upright kinds) or width (flat kinds) range in metres.
    fn size_m(self) -> [f32; 2] {
        match self {
            Kind::Grass => [0.16, 0.30],
            Kind::Dry => [0.22, 0.42],
            Kind::Flower => [0.24, 0.36],
            Kind::Fern => [0.32, 0.55],
            Kind::Litter => [0.30, 0.45],
            Kind::Pebble => [0.14, 0.26],
        }
    }
}

/// What grows on a ground class: the share of cells with a piece and the kinds by weight.
pub struct Cover {
    pub density: f32,
    pub kinds: &'static [(Kind, f32)],
    /// Tint (sRGB) of the grass and dry kinds on this class.
    pub tint: [u8; 3],
}

const NONE: Cover = Cover { density: 0.0, kinds: &[], tint: [0; 3] };

/// Cover per material class (`ground::CLASSES` order): meadow and grass get grass tufts and
/// a few flowers, the forest floor ferns and leaf litter, steppe and fields dry grass, sand,
/// desert and the dry soils sparse dry tufts and pebbles, rock and gravel pebbles, snow none.
pub const COVER: [Cover; 15] = [
    Cover { density: 0.55, kinds: &[(Kind::Grass, 0.95), (Kind::Flower, 0.05)], tint: [70, 90, 36] },
    Cover { density: 0.65, kinds: &[(Kind::Grass, 0.86), (Kind::Flower, 0.14)], tint: [84, 98, 40] },
    Cover { density: 0.10, kinds: &[(Kind::Grass, 0.5), (Kind::Pebble, 0.5)], tint: [92, 94, 48] },
    Cover { density: 0.03, kinds: &[(Kind::Grass, 1.0)], tint: [70, 76, 40] },
    Cover { density: 0.04, kinds: &[(Kind::Dry, 0.6), (Kind::Pebble, 0.4)], tint: [158, 138, 88] },
    Cover { density: 0.16, kinds: &[(Kind::Pebble, 0.65), (Kind::Grass, 0.35)], tint: [82, 92, 46] },
    Cover { density: 0.50, kinds: &[(Kind::Litter, 0.55), (Kind::Fern, 0.3), (Kind::Grass, 0.15)], tint: [60, 80, 34] },
    NONE,
    Cover { density: 0.15, kinds: &[(Kind::Dry, 0.8), (Kind::Grass, 0.2)], tint: [150, 130, 78] },
    Cover { density: 0.05, kinds: &[(Kind::Dry, 0.5), (Kind::Pebble, 0.5)], tint: [150, 128, 82] },
    Cover { density: 0.06, kinds: &[(Kind::Dry, 0.6), (Kind::Pebble, 0.4)], tint: [146, 118, 72] },
    Cover { density: 0.16, kinds: &[(Kind::Pebble, 1.0)], tint: [0; 3] },
    Cover { density: 0.09, kinds: &[(Kind::Dry, 0.45), (Kind::Pebble, 0.55)], tint: [162, 140, 92] },
    Cover { density: 0.50, kinds: &[(Kind::Dry, 0.9), (Kind::Grass, 0.1)], tint: [124, 114, 72] },
    NONE,
];

/// One card of `ground_cover.wgsl`: the ground point, the size (world px), the tint (sRGB)
/// and kind, and turn, seed, flattening (compression), rank (fades out first when high).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default, PartialEq)]
pub struct Instance {
    pub pos: [f32; 3],
    pub size: f32,
    pub color: [u8; 4],
    pub params: [u8; 4],
}

/// The piece of cell `(i, j)`, if any: `class` gives the ground class at a world point
/// (`ground::KEEP` and up for none), `height` the true ground height, `state` the ground
/// state (trampled, burnt, dug).
pub fn place(
    i: i32,
    j: i32,
    class: &dyn Fn(f32, f32) -> u8,
    height: &dyn Fn(f32, f32) -> f32,
    state: Option<&GroundState>,
) -> Option<Instance> {
    let h = hash((i as u32).wrapping_mul(0x8da6_b343) ^ (j as u32).wrapping_mul(0xd816_3841) ^ 0x5bd1_e995);
    let x = (i as f32 + unit(h, 1)) * CELL;
    let y = (j as f32 + unit(h, 2)) * CELL;
    let cover = COVER.get(usize::from(class(x, y)))?;
    let roll = unit(h, 0);
    if roll >= cover.density {
        return None;
    }
    let mut pick = unit(h, 3);
    let mut kind = cover.kinds.last()?.0;
    for &(k, w) in cover.kinds {
        if pick < w {
            kind = k;
            break;
        }
        pick -= w;
    }
    let mut flat = 0.0;
    if let Some(s) = state {
        let [r, _, b, a] = s.texel(x, y);
        // Burnt, or dug by a ball or a wheel: gone.
        if b > 40 || (i32::from(a) - 128).abs() > 14 {
            return None;
        }
        // Trodden: flattened, and thinned where the path is worn.
        let c = f32::from(r) / 255.0;
        if unit(h, 5) < (c - 0.25) * 1.5 {
            return None;
        }
        flat = (c * 2.2).min(1.0);
    }
    let [lo, hi] = kind.size_m();
    let size = (lo + (hi - lo) * unit(h, 4)) * PIXELS_PER_METRE;
    let tint = match kind {
        Kind::Grass | Kind::Dry | Kind::Flower => cover.tint,
        Kind::Fern => [40, 64, 24],
        Kind::Litter => [92, 68, 40],
        Kind::Pebble => {
            if matches!(cover.tint, [r, _, _] if r > 140) {
                [150, 132, 104]
            } else {
                [124, 120, 112]
            }
        }
    };
    let vary = 0.82 + 0.36 * unit(h, 6);
    let warm = unit(h, 7) * 0.12;
    let c = |v: u8, k: f32| (f32::from(v) * k).round().clamp(0.0, 255.0) as u8;
    Some(Instance {
        pos: [x, y, height(x, y)],
        size,
        color: [c(tint[0], vary * (1.0 + warm)), c(tint[1], vary), c(tint[2], vary * (1.0 - warm)), kind as u8],
        params: [(h >> 24) as u8, (h >> 16) as u8, (flat * 255.0) as u8, (roll / cover.density * 255.0) as u8],
    })
}

struct Chunk {
    built: f32,
    upright: Vec<Instance>,
    flat: Vec<Instance>,
}

/// The cached chunks and this frame's instances (upright first, then flat).
#[derive(Default)]
pub struct GroundCover {
    chunks: HashMap<(i32, i32), Chunk>,
    visible: Vec<(i32, i32)>,
    instances: Vec<Instance>,
    upright: usize,
    /// The instance list changed since the last upload.
    dirty: bool,
    /// Chunks (re)built since the start, for the log.
    pub builds: u64,
}

impl GroundCover {
    /// This frame's instances and how many of them are upright (first).
    pub fn instances(&self) -> (&[Instance], usize) {
        (&self.instances, self.upright)
    }

    pub fn clear(&mut self) {
        if !self.instances.is_empty() || !self.chunks.is_empty() {
            self.chunks.clear();
            self.visible.clear();
            self.instances.clear();
            self.upright = 0;
            self.dirty = true;
        }
    }

    /// Places the chunks `view` shows, at `seconds` (any monotonic clock).
    pub fn update(
        &mut self,
        view: &View3d,
        seconds: f32,
        class: &dyn Fn(f32, f32) -> u8,
        height: &dyn Fn(f32, f32) -> f32,
        state: Option<&GroundState>,
    ) {
        let side = CELL * CHUNK as f32;
        let [mut x0, mut y0, mut x1, mut y1] = view.ground_bounds(-100.0, 900.0);
        if view.is_free() {
            let t = view.target();
            x0 = x0.max(t[0] - FREE_REACH);
            y0 = y0.max(t[1] - FREE_REACH);
            x1 = x1.min(t[0] + FREE_REACH);
            y1 = y1.min(t[1] + FREE_REACH);
        }
        let [w, h] = view.size;
        let margin = side * view.camera.scale;
        let mut visible = Vec::new();
        let (cx0, cy0) = ((x0 / side).floor().max(0.0) as i32, (y0 / side).floor().max(0.0) as i32);
        let (cx1, cy1) = ((x1 / side).floor() as i32, (y1 / side).floor() as i32);
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                let (mx, my) = ((cx as f32 + 0.5) * side, (cy as f32 + 0.5) * side);
                let p = view.project([mx, my, height(mx, my)]);
                if p[0] > -margin && p[0] < w + margin && p[1] > -margin && p[1] < h + margin {
                    visible.push((cx, cy));
                }
            }
        }
        let mut changed = visible != self.visible;
        // New chunks first, then the stalest.
        let mut todo: Vec<(f32, (i32, i32))> = visible
            .iter()
            .filter_map(|&k| match self.chunks.get(&k) {
                None => Some((f32::MIN, k)),
                Some(c) if seconds - c.built >= REFRESH || seconds < c.built => Some((c.built, k)),
                _ => None,
            })
            .collect();
        todo.sort_by(|a, b| a.0.total_cmp(&b.0));
        let new = todo.iter().take_while(|t| t.0 == f32::MIN).count().min(NEW_PER_FRAME);
        for &(_, (cx, cy)) in todo.iter().take(new.max(STALE_PER_FRAME)) {
            let mut chunk = Chunk { built: seconds, upright: Vec::new(), flat: Vec::new() };
            for j in cy * CHUNK..(cy + 1) * CHUNK {
                for i in cx * CHUNK..(cx + 1) * CHUNK {
                    if let Some(p) = place(i, j, class, height, state) {
                        if p.color[3] >= Kind::Litter as u8 {
                            chunk.flat.push(p);
                        } else {
                            chunk.upright.push(p);
                        }
                    }
                }
            }
            let same = self.chunks.get(&(cx, cy)).is_some_and(|c| c.upright == chunk.upright && c.flat == chunk.flat);
            changed |= !same;
            self.chunks.insert((cx, cy), chunk);
            self.builds += 1;
        }
        // Forget chunks far out of view.
        if self.chunks.len() > visible.len() * 3 + 64 {
            let keep: std::collections::HashSet<_> = visible.iter().copied().collect();
            self.chunks.retain(|k, _| keep.contains(k));
        }
        self.visible = visible;
        if changed {
            self.instances.clear();
            for k in &self.visible {
                if let Some(c) = self.chunks.get(k) {
                    self.instances.extend_from_slice(&c.upright);
                }
            }
            self.instances.truncate(MAX_INSTANCES);
            self.upright = self.instances.len();
            for k in &self.visible {
                if let Some(c) = self.chunks.get(k) {
                    self.instances.extend_from_slice(&c.flat);
                }
            }
            self.instances.truncate(MAX_INSTANCES);
            self.dirty = true;
        }
    }
}

/// How much of the cover the zoom shows: none below 0.55, all from 1.3.
pub fn zoom_fade(scale: f32) -> f32 {
    let t = ((scale - 0.55) / (1.3 - 0.55)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view: Mat4,
    // xy direction, z strength, w clock (the models' `globals.wind`).
    wind: [f32; 4],
    // xyz the sun, w 1 when the shader encodes sRGB.
    sun: [f32; 4],
    // x the zoom fade, yz the free camera's target, w its reach (0: the fixed camera).
    fade: [f32; 4],
    // x clip depth per world px, y the camera scale, z snow (0..1).
    misc: [f32; 4],
}

/// Draws [`GroundCover::instances`] in the 3D pass.
pub struct GroundCoverRenderer {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    group: wgpu::BindGroup,
    counts: (u32, u32),
    encode_srgb: bool,
}

impl GroundCoverRenderer {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("ground_cover.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ground cover"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ground cover"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Unorm8x4, 3 => Uint8x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ground cover"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_cover"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_cover"),
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
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ground cover globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ground cover instances"),
            size: (MAX_INSTANCES * size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ground cover"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        Self { pipeline, globals, instances, group, counts: (0, 0), encode_srgb: !target.is_srgb() }
    }

    /// Uploads the cover (when it changed) and this frame's globals; `wind` as the models'
    /// `globals.wind`. `None` draws nothing.
    pub fn prepare(&mut self, queue: &wgpu::Queue, view: &View3d, cover: Option<&mut GroundCover>, wind: [f32; 4]) {
        let Some(cover) = cover else {
            self.counts = (0, 0);
            return;
        };
        let n = cover.instances.len().min(MAX_INSTANCES);
        self.counts = (cover.upright.min(n) as u32, n as u32);
        if std::mem::take(&mut cover.dirty) && n > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&cover.instances[..n]));
        }
        let sun = crate::shadow::sun();
        let t = view.target();
        let globals = Globals {
            view: view.world_to_clip(),
            wind,
            sun: [sun[0], sun[1], sun[2], if self.encode_srgb { 1.0 } else { 0.0 }],
            fade: [zoom_fade(view.camera.scale), t[0], t[1], if view.is_free() { FREE_REACH } else { 0.0 }],
            misc: [0.5 / DEPTH_RANGE, view.camera.scale, 0.0, 0.0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }

    /// Draws the upright cards (three crossed quads each), then the flat ones.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (upright, n) = self.counts;
        if n == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        if upright > 0 {
            pass.draw(0..18, 0..upright);
        }
        if n > upright {
            pass.draw(0..6, upright..n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground_state::Stamp;

    fn cells(class: u8, state: Option<&GroundState>) -> Vec<Instance> {
        let mut out = Vec::new();
        for j in 0..64 {
            for i in 0..64 {
                out.extend(place(i, j, &|_, _| class, &|_, _| 7.0, state));
            }
        }
        out
    }

    #[test]
    fn cover_follows_the_class_and_is_deterministic() {
        let meadow = cells(1, None);
        let n = meadow.len() as f32 / 4096.0;
        assert!((0.6..0.7).contains(&n), "meadow share {n}");
        assert!(meadow.iter().all(|p| p.pos[2] == 7.0 && p.color[3] <= Kind::Flower as u8));
        assert!(meadow.iter().any(|p| p.color[3] == Kind::Flower as u8));
        assert_eq!(meadow, cells(1, None));
        let forest = cells(6, None);
        assert!(forest.iter().any(|p| p.color[3] == Kind::Fern as u8));
        assert!(forest.iter().any(|p| p.color[3] == Kind::Litter as u8));
        assert!(cells(7, None).is_empty(), "snow");
        assert!(cells(crate::ground::KEEP, None).is_empty());
        let desert = cells(12, None);
        assert!(desert.len() * 4 < meadow.len());
    }

    #[test]
    fn trodden_and_burnt_ground_thins_the_cover() {
        let mut s = GroundState::new();
        for k in 0..30 {
            let at = [100.0 + 20.0 * k as f32, 300.0];
            s.stamp(&Stamp::print(at, [1.0, 0.0], [30.0, 30.0], 0.0, 1.0));
        }
        let path = |p: &&Instance| (280.0..320.0).contains(&p.pos[1]) && (130.0..650.0).contains(&p.pos[0]);
        let before = cells(1, None).iter().filter(path).count();
        let after: Vec<_> = cells(1, Some(&s));
        let left: Vec<_> = after.iter().filter(path).collect();
        assert!(left.len() * 2 < before, "{} of {before}", left.len());
        assert!(left.iter().all(|p| p.params[2] > 200), "flattened");
    }
}
