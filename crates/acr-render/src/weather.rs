//! Weather of the 3D path (#224, docs/rendering-3d.md "Weather (#224)"): rain, snow or fog
//! as a look only, no gameplay effect (`weather=off|rain|snow|fog`, off by default).
//!
//! - Drops and flakes are instanced quads without a vertex buffer (`weather.wgsl`
//!   `vs_drop`): each instance hashes its place in a box around the view and falls by the
//!   clock, wrapped into the box, so they stay fixed in the world as the camera moves. Their
//!   number follows the ground the view shows (a constant density per world area, capped).
//! - The ground mist covers the pixels no model drew (stencil 0) before the sprites; models
//!   fade their feet into it by height in `model.wgsl` ([`Look::model`]).
//! - The distance fog and the light grade (a blend-constant factor plus a lift) go over the
//!   whole 3D frame at its end, so terrain, models and sprites shift alike. The HUD (circles,
//!   health bars) draws later and stays as it is.
//! - Wet ground and snow cover are in the ground and model shaders ([`Look::model`]).
//! - The rain's strength ([`rain_intensity`]), the lightning ([`strike`], [`flash`]) and the
//!   gusts ([`gust`], as `model.wgsl` sways the trees) are plain functions of the wind clock,
//!   so the picture and the weather sound (`acr-game` `weather_sound`) agree.

use bytemuck::{Pod, Zeroable};

use crate::math::Mat4;
use crate::model_renderer::DEPTH_FORMAT;
use crate::view3d::View3d;

/// `weather=off|rain|snow|fog`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weather {
    #[default]
    Off,
    Rain,
    Snow,
    Fog,
}

impl Weather {
    pub fn name(self) -> &'static str {
        match self {
            Weather::Off => "off",
            Weather::Rain => "rain",
            Weather::Snow => "snow",
            Weather::Fog => "fog",
        }
    }

    /// The setting's value; anything else is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Weather::Off,
            "rain" => Weather::Rain,
            "snow" => Weather::Snow,
            "fog" => Weather::Fog,
            _ => return None,
        })
    }

    /// How much stronger the trees' wind blows (`ModelRenderer::wind` strength).
    pub fn wind_scale(self) -> f32 {
        match self {
            Weather::Rain => 2.2,
            Weather::Snow => 1.4,
            Weather::Off | Weather::Fog => 1.0,
        }
    }

    pub fn look(self) -> Look {
        match self {
            Weather::Off => Look::default(),
            Weather::Rain => Look {
                grade: [0.70, 0.74, 0.82],
                lift: [0.0, 0.004, 0.012],
                fog: [0.50, 0.53, 0.58],
                fog_strength: 0.22,
                fog_start: 0.0,
                fog_scale: 2500.0,
                mist: 0.06,
                mist_height: 40.0,
                wet: 1.0,
                snow: 0.0,
                density: 1.0 / 260.0,
                height: 520.0,
                speed: 640.0,
                drift: 70.0,
                streak: 24.0,
                opacity: 0.32,
            },
            Weather::Snow => Look {
                grade: [0.94, 0.97, 1.0],
                lift: [0.05, 0.06, 0.08],
                fog: [0.82, 0.85, 0.9],
                fog_strength: 0.28,
                fog_start: 0.0,
                fog_scale: 2600.0,
                mist: 0.05,
                mist_height: 40.0,
                wet: 0.0,
                snow: 1.0,
                density: 1.0 / 520.0,
                height: 420.0,
                speed: 42.0,
                drift: 34.0,
                streak: 0.0,
                opacity: 0.9,
            },
            Weather::Fog => Look {
                grade: [0.9, 0.92, 0.95],
                lift: [0.0; 3],
                fog: [0.70, 0.72, 0.74],
                fog_strength: 0.55,
                fog_start: -500.0,
                fog_scale: 1800.0,
                mist: 0.32,
                mist_height: 45.0,
                wet: 0.3,
                snow: 0.0,
                density: 0.0,
                height: 1.0,
                speed: 0.0,
                drift: 0.0,
                streak: 0.0,
                opacity: 0.0,
            },
        }
    }
}

/// The numbers of one weather (output-space colours as in an sRGB picture).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// The frame's colour factor (darker, cooler light; at most 1, WebGL2 clamps the blend
    /// constant) and a lift added to it.
    pub grade: [f32; 3],
    pub lift: [f32; 3],
    /// Fog colour, the distance fog's strength (0..1), where it starts (world px ahead of
    /// the window centre) and its distance scale (world px).
    pub fog: [f32; 3],
    pub fog_strength: f32,
    pub fog_start: f32,
    pub fog_scale: f32,
    /// The ground mist (0..1) and the height it thins over on models (world px, 1/e).
    pub mist: f32,
    pub mist_height: f32,
    /// Wet ground and models (0..1): darker albedo, a broad gloss.
    pub wet: f32,
    /// Snow on the ground's upward faces (0..1).
    pub snow: f32,
    /// Drops per world px² of ground shown, the box's height (world px), fall speed and wind
    /// drift (world px/s), the streak length (world px, 0: round flakes), their opacity.
    pub density: f32,
    pub height: f32,
    pub speed: f32,
    pub drift: f32,
    pub streak: f32,
    pub opacity: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            grade: [1.0; 3],
            lift: [0.0; 3],
            fog: [0.0; 3],
            fog_strength: 0.0,
            fog_start: 0.0,
            fog_scale: 1.0,
            mist: 0.0,
            mist_height: 1.0,
            wet: 0.0,
            snow: 0.0,
            density: 0.0,
            height: 1.0,
            speed: 0.0,
            drift: 0.0,
            streak: 0.0,
            opacity: 0.0,
        }
    }
}

impl Look {
    /// `globals.weather` of `model.wgsl` and the ground: wet, snow, mist, mist height.
    pub fn model(&self) -> [f32; 4] {
        [self.wet, self.snow, self.mist, self.mist_height]
    }

    /// `globals.fog` of `model.wgsl`: the fog colour, linear.
    pub fn model_fog(&self) -> [f32; 4] {
        let l = |c: f32| c.powf(2.2);
        [l(self.fog[0]), l(self.fog[1]), l(self.fog[2]), 0.0]
    }
}

/// The lift of a full lightning flash (output space, a cool white).
const FLASH_LIFT: [f32; 3] = [0.10, 0.11, 0.14];

/// The rain's strength at `clock` seconds of the wind clock: 0.4 (a drizzle) to 1 (a
/// downpour), swelling and easing over tens of seconds. It scales the drops drawn and the
/// rain heard.
pub fn rain_intensity(clock: f32) -> f32 {
    use std::f32::consts::TAU;
    let s = 0.55
        + 0.3 * (clock * TAU / 53.0 + 1.3).sin()
        + 0.17 * (clock * TAU / 21.0 + 0.4).sin()
        + 0.08 * (clock * TAU / 8.3).sin();
    0.4 + 0.6 * s.clamp(0.0, 1.0)
}

/// Seconds of one lightning slot: at most one strike in each.
pub const STRIKE_SLOT: f32 = 24.0;

/// A lightning strike in rain: the flash at `at` (wind clock seconds), its thunder `delay`
/// seconds later (sound at ~340 m/s: 0.8 s is 270 m, 6.8 s 2.3 km), `near` 1 close .. 0 far,
/// and where it is heard (`pan`, -1 left .. 1 right).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strike {
    pub at: f32,
    pub delay: f32,
    pub near: f32,
    pub pan: f32,
}

/// The strike of lightning slot `slot` (`at` in `slot * STRIKE_SLOT ..`), if any: two in
/// three slots have one. A hash of the slot, so it never touches the simulation.
pub fn strike(slot: i64) -> Option<Strike> {
    let mut h = (slot as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    let unit = |shift: u32| ((h >> shift) & 0xffff) as f32 / 65536.0;
    if unit(0) > 0.67 {
        return None;
    }
    let delay = 0.8 + 6.0 * unit(16);
    Some(Strike {
        at: slot as f32 * STRIKE_SLOT + unit(32) * (STRIKE_SLOT - 8.0),
        delay,
        near: 1.0 - (delay - 0.8) / 6.0,
        pan: unit(48) * 1.2 - 0.6,
    })
}

/// The lightning flash at `clock` (0..1): two or three flickers over half a second, brighter
/// for a near strike. Only rain has lightning; the caller checks the weather.
pub fn flash(clock: f32) -> f32 {
    let slot = (clock / STRIKE_SLOT).floor() as i64;
    let Some(s) = strike(slot) else { return 0.0 };
    let t = clock - s.at;
    if !(0.0..0.7).contains(&t) {
        return 0.0;
    }
    let pulse = |at: f32, gain: f32| if t >= at { gain * (-(t - at) / 0.05).exp() } else { 0.0 };
    let f = pulse(0.0, 0.7).max(pulse(0.11, 1.0)).max(pulse(0.3, 0.5 * s.near));
    f * (0.35 + 0.65 * s.near)
}

/// The gust strength (0..1) at world point `at` and wind clock `clock` for wind direction
/// `dir`: `gust` of `model.wgsl`, which sways the trees by it.
pub fn gust(at: [f32; 2], dir: [f32; 2], clock: f32) -> f32 {
    let along = at[0] * dir[0] + at[1] * dir[1];
    let across = -at[0] * dir[1] + at[1] * dir[0];
    let a = (along * 0.0052 - clock * 0.78 + 1.6 * (across * 0.0021 + clock * 0.05).sin()).sin();
    let b = (along * 0.003 - clock * 0.33 + across * 0.0013 + 1.7).sin();
    let g = (0.45 + 0.35 * a + 0.3 * b).clamp(0.0, 1.0);
    g * g * (3.0 - 2.0 * g)
}

/// At most this many drops or flakes in a frame.
pub const MAX_DROPS: u32 = 24_000;
/// The box's ground extent is capped (a low free camera sees to the horizon).
const MAX_EXTENT: f32 = 5000.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: Mat4,
    right: [f32; 4],
    down: [f32; 4],
    forward: [f32; 4],
    centre: [f32; 4],
    box_size: [f32; 4],
    box_at: [f32; 4],
    wind: [f32; 4],
    fog: [f32; 4],
    mist: [f32; 4],
    grade: [f32; 4],
}

/// The weather's pipelines; draws nothing with [`Weather::Off`].
pub struct WeatherRenderer {
    drops: wgpu::RenderPipeline,
    mist: wgpu::RenderPipeline,
    fog: wgpu::RenderPipeline,
    grade: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    group: wgpu::BindGroup,
    weather: Weather,
    look: Look,
    count: u32,
    /// [`WeatherRenderer::set_storm`]: the rain's strength and the lightning flash.
    storm: [f32; 2],
    /// The target stores sRGB values as they are: the colours stay as given.
    encode_srgb: bool,
}

impl WeatherRenderer {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("weather.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weather"),
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
            label: Some("weather"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let keep_if_zero = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let pipeline = |label: &str, vs: &str, fs: &str, depth: wgpu::DepthStencilState, blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(depth),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let always = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let ground_only = wgpu::DepthStencilState {
            stencil: wgpu::StencilState { front: keep_if_zero, back: keep_if_zero, read_mask: 0xff, write_mask: 0 },
            ..always.clone()
        };
        let premultiplied = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
        let drops =
            pipeline("weather drops", "vs_drop", "fs_drop", crate::model_renderer::depth_test_only(), premultiplied);
        let mist = pipeline("weather mist", "vs_full", "fs_mist", ground_only, premultiplied);
        let fog = pipeline("weather fog", "vs_full", "fs_fog", always.clone(), premultiplied);
        // The frame times the blend constant, plus the shader's lift.
        let grade_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Constant,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let grade = pipeline("weather grade", "vs_full", "fs_grade", always, grade_blend);
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        Self {
            drops,
            mist,
            fog,
            grade,
            globals,
            group,
            weather: Weather::Off,
            look: Look::default(),
            count: 0,
            storm: [1.0, 0.0],
            encode_srgb: !target.is_srgb(),
        }
    }

    /// Sets up this frame's weather for `view`: `seconds` is the render clock, `wind` the
    /// trees' wind direction (world x, y, unit).
    pub fn prepare(&mut self, queue: &wgpu::Queue, view: &View3d, weather: Weather, seconds: f32, wind: [f32; 2]) {
        self.weather = weather;
        self.look = weather.look();
        self.count = 0;
        // A lightning flash lifts the light towards white for a moment.
        let flash = self.storm[1];
        if flash > 0.0 {
            let look = &mut self.look;
            for ((grade, lift), add) in look.grade.iter_mut().zip(&mut look.lift).zip(FLASH_LIFT) {
                *grade += (1.0 - *grade) * flash * 0.6;
                *lift += add * flash;
            }
        }
        if weather == Weather::Off {
            return;
        }
        let look = self.look;
        let b = view.basis();
        let centre = view.target();
        // The ground the view shows, from the ground up to the box's top.
        let bounds = view.ground_bounds(0.0, look.height);
        let extent = [(bounds[2] - bounds[0]).min(MAX_EXTENT) + 64.0, (bounds[3] - bounds[1]).min(MAX_EXTENT) + 64.0];
        let mid = [(bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5];
        // Near the window centre when the extent is capped.
        let reach = [(extent[0] * 0.5).max(1.0), (extent[1] * 0.5).max(1.0)];
        let at = [
            mid[0].clamp(centre[0] - reach[0], centre[0] + reach[0]),
            mid[1].clamp(centre[1] - reach[1], centre[1] + reach[1]),
        ];
        if look.density > 0.0 {
            let rain = if weather == Weather::Rain { self.storm[0] } else { 1.0 };
            self.count = ((extent[0] * extent[1] * look.density * rain) as u32).min(MAX_DROPS);
        }
        // Colours into the target's space: blending is linear on an sRGB target.
        let space = |c: f32| if self.encode_srgb { c } else { c.powf(2.2) };
        let rgb = |c: [f32; 3]| [space(c[0]), space(c[1]), space(c[2])];
        let fog = rgb(look.fog);
        let lift = rgb(look.lift);
        let globals = Globals {
            view_proj: view.world_to_clip(),
            right: [b.right[0], b.right[1], b.right[2], view.camera.scale],
            down: [b.down[0], b.down[1], b.down[2], if self.encode_srgb { 1.0 } else { 0.0 }],
            forward: [b.forward[0], b.forward[1], b.forward[2], seconds % 3600.0],
            centre: [centre[0], centre[1], centre[2], weather as u32 as f32],
            box_size: [extent[0], extent[1], look.height, look.speed],
            box_at: [at[0], at[1], look.streak, look.opacity],
            wind: [wind[0] * look.drift, wind[1] * look.drift, view.size[0], view.size[1]],
            fog: [fog[0], fog[1], fog[2], look.fog_strength],
            mist: [look.mist, 0.0, look.fog_start, look.fog_scale],
            grade: [lift[0], lift[1], lift[2], 0.0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }

    /// The rain's strength (0..1, [`rain_intensity`]: scales the drops) and the lightning
    /// flash (0..1, [`flash`]) for the next [`WeatherRenderer::prepare`].
    pub fn set_storm(&mut self, intensity: f32, flash: f32) {
        self.storm = [intensity.clamp(0.0, 1.0), flash.clamp(0.0, 1.0)];
    }

    /// The ground mist, where no model drew: after the models, before the depth reset.
    pub fn render_mist(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.weather == Weather::Off || self.look.mist <= 0.0 {
            return;
        }
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_pipeline(&self.mist);
        pass.set_stencil_reference(0);
        pass.draw(0..3, 0..1);
    }

    /// Drops or flakes, the distance fog and the light grade: last in the 3D pass.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.weather == Weather::Off {
            return;
        }
        pass.set_bind_group(0, &self.group, &[]);
        if self.count > 0 {
            pass.set_pipeline(&self.drops);
            pass.draw(0..6, 0..self.count);
        }
        if self.look.fog_strength > 0.0 {
            pass.set_pipeline(&self.fog);
            pass.draw(0..3, 0..1);
        }
        let space = |c: f32| if self.encode_srgb { c } else { c.powf(2.2) };
        let g = self.look.grade.map(|c| f64::from(space(c)));
        pass.set_pipeline(&self.grade);
        pass.set_blend_constant(wgpu::Color { r: g[0], g: g[1], b: g[2], a: 1.0 });
        pass.draw(0..3, 0..1);
    }

    /// Drops drawn in the last frame.
    pub fn count(&self) -> u32 {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_names_round_trip() {
        for w in [Weather::Off, Weather::Rain, Weather::Snow, Weather::Fog] {
            assert_eq!(Weather::parse(w.name()), Some(w));
        }
        assert_eq!(Weather::parse("hail"), None);
        assert_eq!(Weather::Off.look(), Look::default());
    }

    #[test]
    fn storm_follows_the_clock() {
        let strikes: Vec<Strike> = (0..150).filter_map(strike).collect();
        assert!(strikes.len() > 70 && strikes.len() < 130, "{}", strikes.len());
        for s in &strikes {
            assert!((0.8..=6.8).contains(&s.delay) && (0.0..=1.0).contains(&s.near));
            // Each strike flashes, and the flash is over before its slot ends.
            assert!(flash(s.at + 0.12) > 0.25, "{s:?}");
            assert!(s.at + 0.7 < ((s.at / STRIKE_SLOT).floor() + 1.0) * STRIKE_SLOT);
        }
        for t in 0..3600 {
            let i = rain_intensity(t as f32);
            assert!((0.4..=1.0).contains(&i));
        }
        let g: Vec<f32> = (0..600).map(|t| gust([7680.0, 7680.0], [0.94, 0.34], t as f32 * 0.1)).collect();
        assert!(g.iter().cloned().fold(0.0, f32::max) > 0.8 && g.iter().cloned().fold(1.0, f32::min) < 0.3);
    }
}
