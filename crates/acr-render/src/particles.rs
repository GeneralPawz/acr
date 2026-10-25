//! Particles of the 3D pass (docs/ground.md phase 3, docs/water.md "Splash particles"):
//! earth clods, mud, dust, grass bits, water spray and droplets, wood splinters. CPU
//! simulated (gravity, drag, a ground hit, a bounce and a rest) in a fixed pool of
//! [`MAX_PARTICLES`] with no allocation per frame, drawn as camera-facing instanced quads
//! (`particles.wgsl`): the hard kinds are lit lumpy impostors that write depth, the soft
//! kinds (dust, spray) soft blended puffs. WebGL2 safe: no storage buffers, no compute.
//! Emitters are game-state observers on the render side ([`Burst`], [`ParticleSink`]); the
//! variety comes from hashes of a burst's seed, never from the simulation's RNG.

use bytemuck::{Pod, Zeroable};

use crate::math::{self, Mat4, Vec3};
use crate::model_renderer::DEPTH_FORMAT;
use crate::view3d::{DEPTH_RANGE, View3d};

/// Live particles at most (the pool's size).
pub const MAX_PARTICLES: usize = 4096;
/// Landings kept for the ground marks between two [`Particles::take_landings`].
pub const MAX_LANDINGS: usize = 256;
/// Gravity, world px per s² (30 px to the metre).
pub const GRAVITY: f32 = 294.0;

/// What a particle is: its motion, its rest and its look (`KIND_*` in `particles.wgsl`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ParticleKind {
    /// A lump of earth: tumbles, lands with a small mark, lies a while.
    Clod = 0,
    /// Wet earth: flies like a clod, splats flat, glossy.
    Mud = 1,
    /// A soft puff that drifts, grows and fades.
    Dust = 2,
    /// A torn blade: flutters down, lies briefly.
    Grass = 3,
    /// White water: a soft puff that falls and fades.
    Spray = 4,
    /// A drop of water: falls and is gone where it lands.
    Droplet = 5,
    /// A wood splinter (for later: buildings, fences, felled trees).
    Splinter = 6,
    /// A torn sod: a flat slab of earth with grass on top, tumbling; lies like a clod.
    Sod = 7,
    /// A scrap of flesh or bone from a body torn by a ball (#262): a lumpy impostor like a
    /// clod, tinted by its burst, that lies a while without marking the ground.
    Gore = 8,
    /// A tongue of flame (#256 fires): rises fast, flickers, gone in a fraction of a second.
    /// Emissive, added to what is behind it.
    Flame = 9,
    /// A glowing ember: a spark that drifts up and winks out. Emissive.
    Ember = 10,
    /// Smoke of a fire: a dark (in rain pale) puff that rises, grows a lot and hangs.
    Smoke = 11,
    /// The light a fire throws on the ground and on what stands in it: a faint warm disc
    /// that does not move, re-emitted while the fire burns. Emissive.
    Glow = 12,
}

impl ParticleKind {
    /// Soft kinds blend without writing depth, after the hard ones.
    pub fn soft(self) -> bool {
        matches!(self, Self::Dust | Self::Spray | Self::Smoke) || self.emissive()
    }

    /// Fire light: drawn after the other soft kinds, added rather than laid over.
    pub fn emissive(self) -> bool {
        matches!(self, Self::Flame | Self::Ember | Self::Glow)
    }

    /// Soft kinds that float and never sink into the ground.
    fn floats(self) -> bool {
        matches!(self, Self::Dust | Self::Smoke | Self::Flame | Self::Ember | Self::Glow)
    }

    /// Velocity lost per second (fraction).
    fn drag(self) -> f32 {
        match self {
            Self::Clod => 0.15,
            Self::Sod => 0.2,
            Self::Mud => 0.3,
            Self::Dust => 2.2,
            Self::Grass => 3.0,
            Self::Spray => 1.4,
            Self::Droplet => 0.5,
            Self::Splinter => 0.4,
            Self::Gore => 0.2,
            Self::Flame => 2.5,
            Self::Ember => 1.2,
            Self::Smoke => 1.1,
            Self::Glow => 20.0,
        }
    }

    fn gravity(self) -> f32 {
        match self {
            // Warm dust hangs and rises a little.
            Self::Dust => -6.0,
            Self::Spray => GRAVITY * 0.7,
            // Hot gas: flames lick up, sparks and smoke ride the plume.
            Self::Flame => -90.0,
            Self::Ember => -30.0,
            Self::Smoke => -18.0,
            Self::Glow => 0.0,
            _ => GRAVITY,
        }
    }

    /// How much of the wind a particle of this kind takes (0 none, 1 drifts with it).
    fn carried(self) -> f32 {
        match self {
            Self::Smoke => 1.0,
            Self::Ember => 0.6,
            Self::Flame => 0.25,
            _ => 0.0,
        }
    }

    /// Kinds that come to rest on the ground (the others vanish there or float).
    fn lies(self) -> bool {
        matches!(self, Self::Clod | Self::Mud | Self::Grass | Self::Splinter | Self::Sod | Self::Gore)
    }

    /// Seconds a resting particle lies before it is gone (scaled by a hash, 0.5–1.5).
    fn rest_seconds(self) -> f32 {
        match self {
            Self::Clod => 12.0,
            Self::Sod => 16.0,
            Self::Mud => 5.0,
            Self::Grass => 6.0,
            Self::Splinter => 20.0,
            Self::Gore => 20.0,
            _ => 0.0,
        }
    }
}

/// One burst of particles, emitted by game-state observers (render only, never read by
/// acr-sim). Extends the interface of docs/water.md by size, colour and a seed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burst {
    /// World px, z the true height.
    pub at: Vec3,
    /// Main direction (unit), e.g. up for a splash.
    pub dir: Vec3,
    /// Cone half angle, radians.
    pub spread: f32,
    /// Min/max initial speed, px/s.
    pub speed: [f32; 2],
    pub count: u16,
    pub kind: ParticleKind,
    /// Min/max life in flight (seconds); resting kinds lie [`ParticleKind::rest_seconds`] more.
    pub life: [f32; 2],
    /// Min/max radius (world px).
    pub size: [f32; 2],
    /// Base colour (sRGB); each particle varies it a little.
    pub color: [u8; 3],
    /// Start points scatter this far around `at` (world px, horizontal).
    pub scatter: f32,
    /// The hash seed of the burst's variety.
    pub seed: u32,
}

/// Takes bursts (the particle system, or a recorder in tests).
pub trait ParticleSink {
    fn emit(&mut self, burst: &Burst);
}

/// Where a clod or mud fell (world px), for a small mark in the ground state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    pub at: [f32; 2],
    /// Radius of the particle (world px).
    pub size: f32,
    pub kind: ParticleKind,
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    /// Age at which it is gone (raised when it comes to rest).
    life: f32,
    /// Age at which it came to rest (infinite while flying).
    rest: f32,
    size: f32,
    color: [u8; 3],
    kind: ParticleKind,
    seed: u8,
    landed: bool,
    /// Height above the ground at the last step (world px), for the soft kinds' fade.
    clear: f32,
}

/// One instance of `particles.wgsl`: centre, radius, colour and kind, turn, opacity, seed.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default, PartialEq)]
pub struct Instance {
    pub pos: [f32; 3],
    pub size: f32,
    pub color: [u8; 4],
    pub params: [u8; 4],
}

fn normalize(v: Vec3) -> Vec3 {
    let len = math::dot(v, v).sqrt();
    if len > 0.0 { math::scale(v, 1.0 / len) } else { [0.0, 0.0, 1.0] }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A 32-bit hash (lowbias32).
pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// A hash in 0..1.
pub fn unit(seed: u32, k: u32) -> f32 {
    (hash(seed ^ hash(k.wrapping_mul(0x9e37_79b9))) >> 8) as f32 / (1u32 << 24) as f32
}

/// Seconds over which a new particle comes up to the wind's speed (a plume rises
/// straight off the fire, then bends).
const WIND_TAKE: f32 = 2.0;

/// The pool: fixed capacity, swap-removed, and this frame's instances (hard first).
pub struct Particles {
    pool: Vec<Particle>,
    landings: Vec<Landing>,
    instances: Vec<Instance>,
    /// Instances before this index are hard, the rest soft.
    soft_from: usize,
    /// Bursts cut short because the pool was full (since the start).
    pub dropped: u64,
    /// The wind over the field (world px a second, x y): smoke drifts with it, more as it
    /// rises and ages (#303); embers and flames lean a little. Zero leaves them as before.
    pub wind: [f32; 2],
}

impl Default for Particles {
    fn default() -> Self {
        Self {
            pool: Vec::with_capacity(MAX_PARTICLES),
            landings: Vec::with_capacity(MAX_LANDINGS),
            instances: Vec::with_capacity(MAX_PARTICLES),
            soft_from: 0,
            dropped: 0,
            wind: [0.0; 2],
        }
    }
}

impl ParticleSink for Particles {
    fn emit(&mut self, b: &Burst) {
        let dir = normalize(b.dir);
        // A basis across the main direction.
        let helper = if dir[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let side = normalize(cross(dir, helper));
        let other = cross(dir, side);
        for i in 0..u32::from(b.count) {
            if self.pool.len() == MAX_PARTICLES {
                self.dropped += u64::from(b.count) - u64::from(i);
                return;
            }
            let r = |k: u32| unit(b.seed, i * 8 + k);
            let theta = b.spread * r(0).sqrt();
            let phi = std::f32::consts::TAU * r(1);
            let (st, ct) = theta.sin_cos();
            let (sp, cp) = phi.sin_cos();
            let d = [
                dir[0] * ct + (side[0] * cp + other[0] * sp) * st,
                dir[1] * ct + (side[1] * cp + other[1] * sp) * st,
                dir[2] * ct + (side[2] * cp + other[2] * sp) * st,
            ];
            let speed = b.speed[0] + (b.speed[1] - b.speed[0]) * r(2);
            let (sa, ca) = (std::f32::consts::TAU * r(3)).sin_cos();
            let reach = b.scatter * r(4).sqrt();
            // Lighter or darker by up to 15 %.
            let shade = 0.85 + 0.3 * r(5);
            let color = b.color.map(|c| (f32::from(c) * shade).min(255.0) as u8);
            self.pool.push(Particle {
                pos: [b.at[0] + ca * reach, b.at[1] + sa * reach, b.at[2]],
                vel: math::scale(d, speed),
                age: 0.0,
                life: b.life[0] + (b.life[1] - b.life[0]) * r(6),
                rest: f32::INFINITY,
                size: b.size[0] + (b.size[1] - b.size[0]) * r(7) * r(7),
                color,
                kind: b.kind,
                seed: (hash(b.seed.wrapping_add(i)) & 0xff) as u8,
                landed: false,
                clear: 255.0,
            });
        }
    }
}

impl Particles {
    pub fn live(&self) -> usize {
        self.pool.len()
    }

    pub fn clear(&mut self) {
        self.pool.clear();
        self.landings.clear();
        self.instances.clear();
        self.soft_from = 0;
    }

    /// Moves every particle by `dt` seconds over the ground `ground(x, y)` (true height of
    /// the surface: terrain or water level).
    pub fn step(&mut self, dt: f32, ground: &dyn Fn(f32, f32) -> f32) {
        let dt = dt.clamp(0.0, 0.1);
        let mut i = 0;
        while i < self.pool.len() {
            let p = &mut self.pool[i];
            p.age += dt;
            if p.age >= p.life {
                self.pool.swap_remove(i);
                continue;
            }
            i += 1;
            if p.rest.is_finite() {
                continue;
            }
            let kind = p.kind;
            p.vel[2] -= kind.gravity() * dt;
            let keep = (1.0 - kind.drag() * dt).max(0.0);
            p.vel = math::scale(p.vel, keep);
            p.pos = math::add(p.pos, math::scale(p.vel, dt));
            let carried = kind.carried() * (p.age / WIND_TAKE).min(1.0) * dt;
            if carried > 0.0 {
                p.pos[0] += self.wind[0] * carried;
                p.pos[1] += self.wind[1] * carried;
            }
            if kind.floats() {
                // Dust, smoke and fire float: they never sink into the ground (the glow lies
                // on it).
                let floor = ground(p.pos[0], p.pos[1]);
                let lift = if kind == ParticleKind::Glow { 1.0 } else { p.size * 0.5 };
                p.pos[2] = p.pos[2].max(floor + lift);
                p.clear = p.pos[2] - floor;
                continue;
            }
            let g = ground(p.pos[0], p.pos[1]);
            p.clear = p.pos[2] - g;
            let lift = if kind.lies() { p.size * 0.35 } else { 0.0 };
            if p.pos[2] > g + lift || p.vel[2] > 0.0 {
                continue;
            }
            if !kind.lies() {
                // Water is gone where it lands.
                p.life = p.age;
                continue;
            }
            p.pos[2] = g + lift;
            if !p.landed {
                p.landed = true;
                if matches!(kind, ParticleKind::Clod | ParticleKind::Mud | ParticleKind::Sod)
                    && self.landings.len() < MAX_LANDINGS
                {
                    self.landings.push(Landing { at: [p.pos[0], p.pos[1]], size: p.size, kind });
                }
            }
            // A bounce, a skid, then rest (mud and grass stick).
            let bounce = match kind {
                ParticleKind::Clod | ParticleKind::Gore => 0.3,
                ParticleKind::Sod => 0.15,
                _ => 0.0,
            };
            if -p.vel[2] * bounce > 25.0 {
                p.vel = [p.vel[0] * 0.5, p.vel[1] * 0.5, -p.vel[2] * bounce];
            } else {
                p.rest = p.age;
                let k = 0.5 + f32::from(p.seed) / 255.0;
                p.life = p.age + kind.rest_seconds() * k;
                p.vel = [0.0; 3];
            }
        }
    }

    /// Where clods and mud fell since the last call.
    pub fn take_landings(&mut self) -> std::vec::Drain<'_, Landing> {
        self.landings.drain(..)
    }

    /// Fills this frame's instances (hard kinds first, fire light last), returning them.
    pub fn instances(&mut self) -> &[Instance] {
        self.instances.clear();
        for group in 0..3u8 {
            let soft = group > 0;
            if group == 1 {
                self.soft_from = self.instances.len();
            }
            for p in self.pool.iter().filter(|p| p.kind.soft() == soft && (group == 2) == p.kind.emissive()) {
                let t = p.age / p.life.max(1e-3);
                let (size, alpha) = match p.kind {
                    // Grows as it spreads, fades in quickly and out slowly.
                    ParticleKind::Dust => (p.size * (1.0 + 1.6 * t.sqrt()), (t * 8.0).min(1.0) * (1.0 - t)),
                    ParticleKind::Spray => (p.size * (1.0 + 0.8 * t), (1.0 - t) * (1.0 - t)),
                    ParticleKind::Smoke => (p.size * (1.0 + 3.0 * t.sqrt()), (t * 6.0).min(1.0) * (1.0 - t)),
                    // A tongue swells, then shrinks to its tip as it burns out.
                    ParticleKind::Flame => {
                        (p.size * (0.55 + 0.9 * (t * std::f32::consts::PI).sin()), (1.0 - t * t).max(0.0))
                    }
                    // Sparks twinkle.
                    ParticleKind::Ember => {
                        let wink = 0.6 + 0.4 * (p.age * 23.0 + f32::from(p.seed)).sin();
                        (p.size, (1.0 - t) * wink)
                    }
                    // Fades in and out, so two overlapping glows give a steady flicker.
                    ParticleKind::Glow => (p.size, (t * std::f32::consts::PI).sin()),
                    // Hard kinds shrink away in the last second instead of fading.
                    _ => (p.size * ((p.life - p.age) / 1.0).min(1.0), 1.0),
                };
                let spin = if p.rest.is_finite() { p.rest } else { p.age };
                // Soft puffs turn slowly (their noise rolls), hard ones tumble.
                let rate = if soft { 4.0 + f32::from(p.seed) / 32.0 } else { 60.0 + f32::from(p.seed) };
                let turn = (f32::from(p.seed) + spin * rate) as u32 & 0xff;
                self.instances.push(Instance {
                    pos: p.pos,
                    size,
                    color: [p.color[0], p.color[1], p.color[2], p.kind as u8],
                    // The last byte: resting for the hard kinds, the height above the ground
                    // (world px) for the soft ones (their fade at the ground).
                    params: [
                        turn as u8,
                        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
                        p.seed,
                        if soft { p.clear.clamp(0.0, 255.0) as u8 } else { u8::from(p.rest.is_finite()) },
                    ],
                });
            }
        }
        &self.instances
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view: Mat4,
    right: [f32; 4],
    up: [f32; 4],
    // xyz towards the camera, w 1 when the shader encodes sRGB.
    toward: [f32; 4],
    // xyz the sun, w the depth (clip) per world px towards the camera.
    sun: [f32; 4],
}

/// Draws [`Particles::instances`] in the 3D pass.
pub struct ParticleRenderer {
    hard: wgpu::RenderPipeline,
    soft: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    group: wgpu::BindGroup,
    counts: (u32, u32),
    /// The target stores sRGB values as they are: the shader encodes.
    encode_srgb: bool,
}

impl ParticleRenderer {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("particles.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles"),
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
            label: Some("particles"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Unorm8x4, 3 => Uint8x4];
        let pipeline = |soft: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if soft { "particles soft" } else { "particles hard" }),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_particle"),
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
                    depth_write_enabled: Some(!soft),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_particle"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let (hard, soft) = (pipeline(false), pipeline(true));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle instances"),
            size: (MAX_PARTICLES * size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particles"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        Self { hard, soft, globals, instances, group, counts: (0, 0), encode_srgb: !target.is_srgb() }
    }

    /// Uploads this frame's particles for `view`.
    pub fn prepare(&mut self, queue: &wgpu::Queue, view: &View3d, particles: &mut Particles) {
        let list = particles.instances();
        let n = list.len().min(MAX_PARTICLES);
        self.counts = (particles.soft_from.min(n) as u32, n as u32);
        if n == 0 {
            return;
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&particles.instances[..n]));
        let b = view.basis();
        let up = math::scale(b.down, -1.0);
        let toward = math::scale(b.forward, -1.0);
        let sun = crate::shadow::sun();
        let globals = Globals {
            view: view.world_to_clip(),
            right: [b.right[0], b.right[1], b.right[2], 0.0],
            up: [up[0], up[1], up[2], 0.0],
            toward: [toward[0], toward[1], toward[2], if self.encode_srgb { 1.0 } else { 0.0 }],
            sun: [sun[0], sun[1], sun[2], 0.5 / DEPTH_RANGE],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }

    /// Draws the hard particles, then the soft ones, into the 3D pass.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (soft_from, n) = self.counts;
        if n == 0 {
            return;
        }
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        if soft_from > 0 {
            pass.set_pipeline(&self.hard);
            pass.draw(0..6, 0..soft_from);
        }
        if n > soft_from {
            pass.set_pipeline(&self.soft);
            pass.draw(0..6, soft_from..n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fountain(seed: u32, count: u16, kind: ParticleKind) -> Burst {
        Burst {
            at: [1000.0, 1000.0, 0.0],
            dir: [0.5, 0.0, 1.0],
            spread: 0.6,
            speed: [150.0, 300.0],
            count,
            kind,
            life: [3.0, 5.0],
            size: [2.0, 6.0],
            color: [90, 70, 50],
            scatter: 8.0,
            seed,
        }
    }

    #[test]
    fn clods_fly_land_mark_and_rest() {
        let mut p = Particles::default();
        p.emit(&fountain(7, 50, ParticleKind::Clod));
        assert_eq!(p.live(), 50);
        for _ in 0..240 {
            p.step(1.0 / 60.0, &|_, _| 0.0);
        }
        let landed: Vec<_> = p.take_landings().collect();
        assert_eq!(landed.len(), 50);
        // Thrown forward along +x.
        assert!(landed.iter().map(|l| l.at[0]).sum::<f32>() / 50.0 > 1050.0);
        assert!(p.pool.iter().all(|q| q.rest.is_finite() && q.pos[2] < q.size));
        // Gone after their rest.
        for _ in 0..1500 {
            p.step(1.0 / 60.0, &|_, _| 0.0);
        }
        assert_eq!(p.live(), 0);
    }

    #[test]
    fn smoke_drifts_with_the_wind() {
        let mean_x = |wind: [f32; 2], kind| {
            let mut p = Particles { wind, ..Particles::default() };
            p.emit(&Burst { dir: [0.0, 0.0, 1.0], ..fountain(5, 20, kind) });
            for _ in 0..120 {
                p.step(1.0 / 60.0, &|_, _| 0.0);
            }
            p.pool.iter().map(|q| q.pos[0]).sum::<f32>() / p.pool.len().max(1) as f32
        };
        let still = mean_x([0.0; 2], ParticleKind::Smoke);
        let blown = mean_x([50.0, 0.0], ParticleKind::Smoke);
        // Two seconds at a speed that comes up over the first two: about half of 100 px.
        assert!(blown - still > 35.0 && blown - still < 60.0, "{still} {blown}");
        // Dust stays where it was.
        assert_eq!(mean_x([50.0, 0.0], ParticleKind::Dust), mean_x([0.0; 2], ParticleKind::Dust));
    }

    #[test]
    fn the_pool_is_capped_and_bursts_repeat() {
        let mut p = Particles::default();
        for s in 0..50 {
            p.emit(&fountain(s, 100, ParticleKind::Dust));
        }
        assert_eq!(p.live(), MAX_PARTICLES);
        assert_eq!(p.dropped, 5000 - MAX_PARTICLES as u64);
        let (mut a, mut b) = (Particles::default(), Particles::default());
        a.emit(&fountain(3, 20, ParticleKind::Mud));
        b.emit(&fountain(3, 20, ParticleKind::Mud));
        assert_eq!(a.instances().to_vec(), b.instances().to_vec());
        let mut c = Particles::default();
        c.emit(&fountain(4, 20, ParticleKind::Mud));
        assert_ne!(a.instances().to_vec(), c.instances().to_vec());
    }

    #[test]
    fn soft_after_hard_and_water_vanishes() {
        let mut p = Particles::default();
        p.emit(&fountain(1, 5, ParticleKind::Dust));
        p.emit(&fountain(2, 5, ParticleKind::Clod));
        let list = p.instances().to_vec();
        assert_eq!(p.soft_from, 5);
        assert!(list[..5].iter().all(|i| i.color[3] == ParticleKind::Clod as u8));
        let mut w = Particles::default();
        w.emit(&fountain(1, 30, ParticleKind::Droplet));
        for _ in 0..120 {
            w.step(1.0 / 60.0, &|_, _| 0.0);
        }
        assert_eq!(w.live(), 0);
        assert_eq!(w.take_landings().count(), 0);
    }

    /// Release cost of 4000 live particles (`cargo test --release -p acr-render particle_cost
    /// -- --ignored --nocapture`); the budget is 0.5 ms per frame.
    #[test]
    #[ignore]
    fn particle_cost() {
        let mut p = Particles::default();
        for s in 0..40 {
            p.emit(&Burst {
                life: [30.0, 40.0],
                ..fountain(s, 100, if s % 2 == 0 { ParticleKind::Clod } else { ParticleKind::Dust })
            });
        }
        assert_eq!(p.live(), 4000);
        let ground = |x: f32, y: f32| (x * 0.01).sin() * 4.0 + (y * 0.013).cos() * 3.0;
        let frames = 300;
        let start = std::time::Instant::now();
        let mut n = 0;
        for _ in 0..frames {
            p.step(1.0 / 60.0, &ground);
            n += p.instances().len();
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
        println!("particles: {} live, {ms:.3} ms per frame (step + instances), {n}", p.live());
    }
}
