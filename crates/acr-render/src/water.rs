//! The water surface of the 3D pass (docs/water.md): a grid at the water level over the
//! visible part of the map, moved by [`WAVES`] Gerstner waves (a seeded spectrum, the same
//! sum on the CPU for ship motion, [`Waves::sample`]) scaled by the `SEA2` depth, with a
//! procedural detail normal, depth colour and absorption over the lakebed, a sky reflection,
//! the sun's glint and shore foam (`water.wgsl`). Ships under way add a Kelvin wake and a bow
//! wave, cannonballs landing in water a ring ([`Disturbances`]). It replaces the original's
//! depth tint (`terrain.wgsl`, `fs_present`) when drawn, for the fixed and the free camera
//! alike. Render only: nothing goes back to the simulation.

use bytemuck::{Pod, Zeroable};

use crate::math::{self, Mat4};
use crate::model_renderer::DEPTH_FORMAT;
use crate::terrain::TerrainData;
use crate::view3d::View3d;

/// Largest wave height (world px) above or below the water level, for culling.
const WAVE_REACH: f32 = 12.0;
/// Gerstner components of the surface (`water.wgsl` loops over as many).
pub const WAVES: usize = 10;
/// Ships with a wake and rings of landing shot the shader takes at most per frame.
pub const MAX_WAKES: usize = 8;
pub const MAX_RINGS: usize = 16;
/// How long a ring of landing shot spreads (seconds).
pub const RING_SECONDS: f32 = 6.0;
/// Gravity in world px per s² (30 px to the metre), as `GRAVITY` in `water.wgsl`.
const GRAVITY: f32 = 294.0;

/// `sea_state=calm|storm`: how wild the sea is. A storm scales the spectrum up (higher,
/// steeper waves with whitecaps) and darkens the water; the weather's rain and wind come with
/// it in the game (docs/water.md "Storm"). Render only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeaState {
    #[default]
    Calm,
    Storm,
}

impl SeaState {
    pub fn name(self) -> &'static str {
        match self {
            SeaState::Calm => "calm",
            SeaState::Storm => "storm",
        }
    }

    /// The setting's value; anything else is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "calm" => SeaState::Calm,
            "storm" => SeaState::Storm,
            _ => return None,
        })
    }

    /// The factor on every wave's amplitude (`globals.grid.w` in `water.wgsl`); also on the
    /// CPU sum ([`Waves::sample`]'s `scale`), so ships and surface agree.
    pub fn gain(self) -> f32 {
        match self {
            SeaState::Calm => 1.0,
            SeaState::Storm => STORM_GAIN,
        }
    }
}

/// The storm's wave amplitude factor: 4 × the calm spectrum.
pub const STORM_GAIN: f32 = 4.0;

/// One Gerstner wave: unit direction, wave number (rad/px), amplitude (px), phase offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wave {
    pub dir: [f32; 2],
    pub k: f32,
    pub amp: f32,
    pub phase: f32,
}

/// The wave spectrum of a map: [`WAVES`] components around one wind direction, wavelengths
/// from 240 px (8 m) down to 26 px, the short ones spread wider, each with a random phase, so
/// no two crests line up into the parallel stripes of a few regular waves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waves(pub [Wave; WAVES]);

/// The surface's height and slope at one point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub height: f32,
    pub slope: [f32; 2],
}

impl Waves {
    pub fn new(seed: u32) -> Self {
        let mut state = seed.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
        let mut rand = move || {
            // xorshift32: a fixed, cheap sequence (render only, never hashed).
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 8) as f32 / (1u32 << 24) as f32
        };
        let wind = rand() * std::f32::consts::TAU;
        let waves = std::array::from_fn(|i| {
            let f = i as f32 / (WAVES - 1) as f32;
            let length = 240.0 * 0.78f32.powi(i as i32) * (0.9 + 0.2 * rand());
            let angle = wind + (rand() * 2.0 - 1.0) * (0.35 + 0.65 * f);
            Wave {
                dir: [angle.cos(), angle.sin()],
                k: std::f32::consts::TAU / length,
                amp: length * 0.0085 * (0.7 + 0.6 * rand()),
                phase: rand() * std::f32::consts::TAU,
            }
        });
        Self(waves)
    }

    /// Height and slope at world point `p` (px) at `time` (s), the waves scaled by `scale`
    /// ([`wave_scale`]): the vertex shader's sum without the horizontal Gerstner shift.
    pub fn sample(&self, p: [f32; 2], time: f32, scale: f32) -> Sample {
        let mut out = Sample::default();
        for w in &self.0 {
            let a = w.amp * scale;
            let phase = w.k * (w.dir[0] * p[0] + w.dir[1] * p[1]) - (GRAVITY * w.k).sqrt() * time + w.phase;
            let (sn, c) = phase.sin_cos();
            out.height += a * sn;
            out.slope[0] += w.dir[0] * w.k * a * c;
            out.slope[1] += w.dir[1] * w.k * a * c;
        }
        out
    }

    /// The uniform form: (dir, k, amp) and the phases four to a vector.
    fn packed(&self) -> ([[f32; 4]; WAVES], [[f32; 4]; WAVES.div_ceil(4)]) {
        let mut phases = [[0.0; 4]; WAVES.div_ceil(4)];
        let waves = std::array::from_fn(|i| {
            let w = self.0[i];
            phases[i / 4][i % 4] = w.phase;
            [w.dir[0], w.dir[1], w.k, w.amp]
        });
        (waves, phases)
    }
}

/// How much the waves move at `units` of `SEA2` depth: calm in shallows, full on open water
/// (`wave_scale` in `water.wgsl`).
pub fn wave_scale(units: f32) -> f32 {
    let t = (units / 40.0).clamp(0.0, 1.0);
    0.15 + 0.85 * t * t * (3.0 - 2.0 * t)
}

/// Depth units (0 at the waterline) of the `SEA2` grid `(w, h, cells)` at world `(x, y)`,
/// nearest cell (`depth_units` in the shader filters; ship motion needs no more).
pub fn depth_units(water: (usize, usize, &[u8]), x: f32, y: f32) -> f32 {
    let (w, h, cells) = water;
    let cx = ((x / 32.0) as isize).clamp(0, w as isize - 1) as usize;
    let cy = ((y / 32.0) as isize).clamp(0, h as isize - 1) as usize;
    f32::from(cells[cy * w + cx]) - 127.0
}

/// The seed of a map's wave spectrum (its depth grid's size: the same map, the same waves).
pub fn seed(water: (usize, usize, &[u8])) -> u32 {
    (water.0 as u32).wrapping_mul(31) ^ (water.1 as u32).wrapping_mul(0x2545_F491)
}

/// A ship under way, for its wake and bow wave.
#[derive(Clone, Copy, Debug)]
pub struct Wake {
    /// The hull's centre (world px).
    pub at: [f32; 2],
    /// Unit heading.
    pub dir: [f32; 2],
    /// Speed (world px per second).
    pub speed: f32,
    /// Half the hull's length (px).
    pub half_length: f32,
}

/// A cannonball (or anything) that fell into the water.
#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub at: [f32; 2],
    /// When it landed, on the clock passed to [`WaterRenderer::prepare`] (seconds).
    pub time: f32,
    /// 1 for a cannonball.
    pub strength: f32,
}

/// What disturbs the surface this frame (render only, gathered by the game from its state).
#[derive(Clone, Debug, Default)]
pub struct Disturbances {
    pub wakes: Vec<Wake>,
    pub rings: Vec<Ring>,
}
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
    waves: [[f32; 4]; WAVES],
    phases: [[f32; 4]; WAVES.div_ceil(4)],
    // Per ship: x, y, dir x, dir y, then speed, half length, unused, unused.
    wakes: [[f32; 4]; 2 * MAX_WAKES],
    // x, y, landing time, strength (0: none).
    rings: [[f32; 4]; MAX_RINGS],
    // Wakes in use, rings in use, unused, unused.
    counts: [f32; 4],
}

struct WaterMap {
    /// Which depth grid was uploaded (its size and address).
    key: (usize, usize, usize, u64),
    /// The map's wave spectrum ([`seed`]).
    waves: Waves,
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
        let key = (w, h, cells.as_ptr() as usize, data.water_revision());
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
        let waves = Waves::new(seed((w, h, cells)));
        self.map = Some(WaterMap { key, waves, extent: [w as f32 * 32.0, h as f32 * 32.0], group });
        true
    }

    /// Prepares this frame's surface for `view`: `seconds` drives the waves, `detailed` the
    /// waves, glint, foam, wakes and rings (`water=detailed`; else a flat surface, for the
    /// free camera under `water=original`). False when there is no water to draw.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &TerrainData,
        view: &View3d,
        seconds: f32,
        detailed: bool,
        sea: SeaState,
        disturbances: &Disturbances,
    ) -> bool {
        self.vertices = 0;
        if !self.set_map(device, queue, data) {
            return false;
        }
        let map = self.map.as_ref().expect("set above");
        let [mut x0, mut y0, mut x1, mut y1] = view.ground_bounds(-WAVE_REACH * sea.gain(), WAVE_REACH * sea.gain());
        if view.is_free() {
            let t = view.target();
            x0 = x0.max(t[0] - FREE_REACH);
            y0 = y0.max(t[1] - FREE_REACH);
            x1 = x1.min(t[0] + FREE_REACH);
            y1 = y1.min(t[1] + FREE_REACH);
        }
        let margin = 2.0 * WAVE_REACH * sea.gain();
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
        let (waves, phases) = map.waves.packed();
        let mut wakes = [[0.0; 4]; 2 * MAX_WAKES];
        let mut n_wakes = 0;
        let near = |p: [f32; 2], reach: f32| {
            p[0] > rect[0] - reach && p[0] < rect[2] + reach && p[1] > rect[1] - reach && p[1] < rect[3] + reach
        };
        for w in disturbances.wakes.iter().filter(|w| near(w.at, 1200.0)) {
            if n_wakes == MAX_WAKES {
                break;
            }
            wakes[2 * n_wakes] = [w.at[0], w.at[1], w.dir[0], w.dir[1]];
            wakes[2 * n_wakes + 1] = [w.speed, w.half_length, 0.0, 0.0];
            n_wakes += 1;
        }
        let mut rings = [[0.0; 4]; MAX_RINGS];
        let mut n_rings = 0;
        // The newest first (the list grows at its end), on the waves' wrapped clock.
        let live = |r: &&Ring| (0.0..RING_SECONDS).contains(&(seconds - r.time)) && near(r.at, 400.0);
        for r in disturbances.rings.iter().rev().filter(live).take(MAX_RINGS) {
            rings[n_rings] = [r.at[0], r.at[1], time - (seconds - r.time), r.strength];
            n_rings += 1;
        }
        let globals = Globals {
            view: view.world_to_clip(),
            rect,
            eye: [eye[0], eye[1], eye[2], time],
            sun: [sun[0], sun[1], sun[2], if detailed { 1.0 } else { 0.0 }],
            grid: [map.extent[0] / 32.0, map.extent[1] / 32.0, cells as f32, sea.gain()],
            waves,
            phases,
            wakes,
            rings,
            counts: [n_wakes as f32, n_rings as f32, 0.0, 0.0],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_is_seeded_and_bounded() {
        let a = Waves::new(7);
        assert_eq!(a, Waves::new(7));
        assert_ne!(a, Waves::new(8));
        let reach: f32 = a.0.iter().map(|w| w.amp).sum();
        assert!(reach < WAVE_REACH, "waves reach {reach} px beyond the culling margin");
        // The directions spread: not all within 20 degrees of the first.
        let spread = a.0.iter().map(|w| w.dir[0] * a.0[0].dir[0] + w.dir[1] * a.0[0].dir[1]).fold(1.0f32, f32::min);
        assert!(spread < 0.94, "{spread}");
    }

    #[test]
    fn slope_matches_the_height() {
        let w = Waves::new(3);
        let (p, t, e) = ([1234.0, 567.0], 12.5, 0.05);
        let s = w.sample(p, t, 1.0);
        let h = |x: f32, y: f32| w.sample([x, y], t, 1.0).height;
        let dx = (h(p[0] + e, p[1]) - h(p[0] - e, p[1])) / (2.0 * e);
        let dy = (h(p[0], p[1] + e) - h(p[0], p[1] - e)) / (2.0 * e);
        assert!((dx - s.slope[0]).abs() < 2e-2 && (dy - s.slope[1]).abs() < 2e-2, "{dx} {dy} vs {:?}", s.slope);
    }

    #[test]
    fn storm_scales_the_sum() {
        let w = Waves::new(5);
        let (p, t) = ([800.0, 300.0], 3.0);
        let (calm, storm) = (w.sample(p, t, SeaState::Calm.gain()), w.sample(p, t, SeaState::Storm.gain()));
        assert!((storm.height - calm.height * STORM_GAIN).abs() < 1e-3);
        assert!((storm.slope[0] - calm.slope[0] * STORM_GAIN).abs() < 1e-3);
        assert_eq!(SeaState::parse("storm"), Some(SeaState::Storm));
        assert_eq!(SeaState::parse("gale"), None);
    }

    #[test]
    fn shallows_are_calm() {
        assert!((wave_scale(0.0) - 0.15).abs() < 1e-6);
        assert!((wave_scale(80.0) - 1.0).abs() < 1e-6);
    }
}
