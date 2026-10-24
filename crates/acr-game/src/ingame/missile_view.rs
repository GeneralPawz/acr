//! The rounds in flight as models (#308, docs/rendering-3d.md "Projectiles"), render only:
//! arrows turned along their flight, a musket ball's short grey streak from the muzzle to
//! where it struck, cannonballs with a faint trail, shells tumbling with a sparking fuse,
//! tomahawks and stones turning over, spears rolling. Arrows that missed stick in the ground
//! at the angle they came down, for as long as corpses lie. The simulation's projectile
//! slots, impacts and hits are only read; variety comes from hashes of the slot and the
//! tick. The 2D sprites of the rounds drawn here are left out (`effects::projectile_sprites`).

use std::collections::VecDeque;

use acr_render::math::{self, Mat4, Vec3};
use acr_render::missiles;
use acr_render::model_renderer::{ModelInstance, ModelRenderer};
use acr_render::particles::{Burst, ParticleKind, ParticleSink, hash, unit};
use acr_render::view3d::{COS_E, PIXELS_PER_METRE};
use acr_sim::World;
use acr_sim::combat::DeathCause;
use acr_sim::projectile::Projectile;
use acr_sim::weapon::Propagation;

/// What a round in flight is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Round {
    Arrow,
    Cannonball,
    Grapeshot,
    Shell,
    Tomahawk,
    Spear,
    Stone,
}

/// The round a projectile draws as, or `None` for smoke, flashes, debris and the rest (which
/// keep their sprites): a flying object (`FLY`, `ANGLE`, `REFLECT`) of a shooter whose attack
/// is a missile weapon (`NRES.DAT [WEAPONS]`).
pub fn round(world: &World, p: &Projectile) -> Option<Round> {
    let wp = world.catalog.weapons.get(p.weapon);
    if !matches!(wp.propagation, Propagation::Fly | Propagation::Angle | Propagation::Reflect) {
        return None;
    }
    let u = world.units.get(p.sender?.0 as usize)?;
    let adv = world.adv(u.owner, u.kind);
    let kind = adv.attacks.get(usize::from(p.att))?.kind.as_str();
    match kind {
        "TAMAGAVK" => Some(Round::Tomahawk),
        "KOPIE" => Some(Round::Spear),
        "KAMEN" => Some(Round::Stone),
        k => match DeathCause::of_weapon(k) {
            DeathCause::Arrow => Some(Round::Arrow),
            DeathCause::Cannonball => Some(Round::Cannonball),
            DeathCause::Grapeshot => Some(Round::Grapeshot),
            DeathCause::Explosion => Some(Round::Shell),
            _ => None,
        },
    }
}

/// Model indices of the code-built missiles (`acr_render::missiles`).
#[derive(Debug, Clone, Copy)]
struct Models {
    arrow: usize,
    streak: usize,
    ball: usize,
    shell: usize,
    tomahawk: usize,
    spear: usize,
    stone: usize,
}

/// An arrow stuck in the ground: its transform and the tick it struck.
#[derive(Debug, Clone, Copy)]
struct Stuck {
    transform: Mat4,
    tick: u64,
}

/// Radius of a drawn cannonball and of a grapeshot ball (metres; a 6-pounder's is 0.047 m,
/// drawn half again as large to show at the game's zoom).
const BALL_RADIUS: f32 = 0.07;
const GRAPE_RADIUS: f32 = 0.03;
/// A musket ball's streak: length and radius (metres), and how fast it is drawn to travel
/// (world px per second: slowed from the ball's ~400 m/s so it is seen at all).
const STREAK_LENGTH: f32 = 2.2;
const STREAK_RADIUS: f32 = 0.022;
const STREAK_SPEED: f32 = 4500.0;
/// Muzzle and chest heights (true world px) the streak runs between.
const MUZZLE: f32 = 1.4 * PIXELS_PER_METRE;
const CHEST: f32 = 1.2 * PIXELS_PER_METRE;
/// How deep a stuck arrow's head goes into the ground (metres), and at most this many lie.
const STUCK_DEPTH: f32 = 0.18;
const STUCK_CAP: usize = 2048;

#[derive(Default)]
pub struct Missiles {
    models: Option<Models>,
    stuck: VecDeque<Stuck>,
    /// Arrows in flight at the last two ticks drawn: point and direction (true world px).
    flying: Vec<(Vec3, Vec3)>,
    flown: Vec<(Vec3, Vec3)>,
    /// The tick of the last update, the newest impact taken, and the real seconds since
    /// the tick changed and the last tick's length (for drawing between ticks).
    tick: u64,
    last_impact: u64,
    since: f32,
    tick_seconds: f32,
}

impl Missiles {
    /// Uploads the missile models once.
    pub fn upload(&mut self, renderer: &mut ModelRenderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.models.is_some() {
            return;
        }
        let mut add = |m| renderer.add_model(device, queue, m);
        self.models = Some(Models {
            arrow: add(missiles::arrow()),
            streak: add(missiles::streak()),
            ball: add(missiles::ball()),
            shell: add(missiles::shell()),
            tomahawk: add(missiles::tomahawk()),
            spear: add(missiles::spear()),
            stone: add(missiles::stone()),
        });
    }

    /// True once the models are up: the rounds' sprites are left out then.
    pub fn ready(&self) -> bool {
        self.models.is_some()
    }

    pub fn clear(&mut self) {
        self.stuck.clear();
        self.flying.clear();
        self.flown.clear();
        self.last_impact = 0;
    }

    /// This frame's rounds, streaks and stuck arrows appended to `out`; trails and fuse
    /// sparks into `sink` once per tick. `ground` gives the true height under a point,
    /// `keep` how long stuck arrows stay (ticks), `seconds` the frame's length.
    pub fn draw(
        &mut self,
        world: &World,
        ground: &dyn Fn(f32, f32) -> f32,
        keep: u64,
        seconds: f32,
        out: &mut Vec<ModelInstance>,
        sink: &mut dyn ParticleSink,
    ) {
        acr_sim::prof_scope!("3d.missiles");
        let Some(models) = self.models else { return };
        let now = world.tick.0;
        if now < self.tick {
            self.clear();
        }
        let new_tick = now != self.tick;
        if new_tick {
            if self.tick != 0 {
                self.tick_seconds = (self.since * 0.5 + self.tick_seconds * 0.5).clamp(0.01, 1.0);
            }
            self.since = 0.0;
            self.flown = std::mem::take(&mut self.flying);
        } else {
            self.since += seconds.clamp(0.0, 0.25);
        }
        self.tick = now;
        // How far into the tick this frame is: rounds move on along their velocity.
        let frac = if self.tick_seconds > 0.0 { (self.since / self.tick_seconds).min(1.0) } else { 0.0 };
        for (slot, p) in world.projectiles.slots.iter().enumerate() {
            let Some(p) = p else { continue };
            let Some(kind) = round(world, p) else { continue };
            let seed = hash(slot as u32 ^ hash(p.weapon as u32 ^ p.sender.map_or(0, |s| s.0)));
            let k = 1.0 / 16384.0;
            let vel = [p.vel.0 as f32 * k, p.vel.1 as f32 * k, p.vel.2 as f32 * k / COS_E];
            let az = p.az as f32 * k / COS_E;
            let at = [
                p.pos.0 as f32 * k + vel[0] * frac,
                p.pos.1 as f32 * k + vel[1] * frac,
                p.pos.2 as f32 * k / COS_E + vel[2] * frac + 0.5 * az * frac * frac,
            ];
            let dir = normalize([vel[0], vel[1], vel[2] + az * frac]);
            // Turning over: per tick of its life, from a phase of its own.
            let turn = (p.frame as f32 + frac) + 6.0 * unit(seed, 1);
            let (model, transform) = match kind {
                Round::Arrow => {
                    if new_tick {
                        self.flying.push((at, dir));
                    }
                    (models.arrow, oriented(at, dir, 0.0, 0.0, [1.0; 3]))
                }
                Round::Cannonball | Round::Grapeshot => {
                    let r = if kind == Round::Cannonball { BALL_RADIUS } else { GRAPE_RADIUS };
                    if new_tick && kind == Round::Cannonball {
                        trail(sink, at, vel, seed ^ now as u32);
                    }
                    (models.ball, oriented(at, dir, 0.0, 0.0, [r; 3]))
                }
                Round::Shell => {
                    let m = oriented(at, dir, 0.0, turn * 0.9, [1.0; 3]);
                    if new_tick {
                        let tip = math::transform_point(&m, [0.0, missiles::SHELL_FUSE, 0.0]);
                        fuse(sink, tip, seed ^ now as u32);
                    }
                    (models.shell, m)
                }
                Round::Tomahawk => (models.tomahawk, oriented(at, dir, 0.0, turn * 1.7, [1.0; 3])),
                Round::Spear => (models.spear, oriented(at, dir, turn * 0.8, 0.0, [1.0; 3])),
                Round::Stone => (models.stone, oriented(at, dir, turn, turn * 1.3, [1.0; 3])),
            };
            out.push(instance(model, transform, false));
        }
        // Arrows that missed (an impact of an arrow) stick where they struck.
        let fresh = world.impacts.iter().filter(|i| i.tick > self.last_impact && i.cause == DeathCause::Arrow);
        for (n, i) in fresh.enumerate() {
            let p = [i.pos.0 as f32, i.pos.1 as f32];
            let g = [p[0], p[1], ground(p[0], p[1])];
            // The arrow seen nearest, else straight along the impact coming down at 40°.
            let near = self
                .flown
                .iter()
                .chain(&self.flying)
                .map(|&(a, d)| ((a[0] - p[0]).abs() + (a[1] - p[1]).abs(), d))
                .filter(|&(dist, _)| dist < 60.0)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let mut d = match near {
                Some((_, d)) => d,
                None => {
                    let (dx, dy) = (i.dir.0 as f32, i.dir.1 as f32);
                    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
                    [dx / l * 0.77, dy / l * 0.77, -0.64]
                }
            };
            // At least a little into the ground, wobbling by a hash of the impact.
            let seed = hash(i.pos.0 as u32 ^ hash(i.pos.1 as u32 ^ hash(i.tick as u32 + n as u32)));
            d[2] = d[2].min(-0.3) - 0.15 * unit(seed, 2);
            d[0] += 0.12 * (unit(seed, 3) - 0.5);
            d[1] += 0.12 * (unit(seed, 4) - 0.5);
            let d = normalize(d);
            let tip = math::add(g, math::scale(d, STUCK_DEPTH * PIXELS_PER_METRE));
            if self.stuck.len() >= STUCK_CAP {
                self.stuck.pop_front();
            }
            self.stuck
                .push_back(Stuck { transform: oriented(tip, d, 6.0 * unit(seed, 5), 0.0, [1.0; 3]), tick: i.tick });
        }
        if let Some(last) = world.impacts.back() {
            self.last_impact = self.last_impact.max(last.tick);
        }
        while self.stuck.front().is_some_and(|s| s.tick + keep < now) {
            self.stuck.pop_front();
        }
        out.extend(self.stuck.iter().map(|s| instance(models.arrow, s.transform, false)));
        // Musket balls are line traces in the simulation (0x5589ec): a streak runs from the
        // shooter to where the ball struck someone (a hit) or the ground (an impact), drawn
        // through the tick of the shot.
        let shots = world
            .hits
            .iter()
            .filter(|h| h.tick == now && h.cause == DeathCause::Gunshot)
            .map(|h| (h.pos, h.dir, CHEST, h.victim))
            .chain(
                world
                    .impacts
                    .iter()
                    .filter(|i| i.tick == now && i.cause == DeathCause::Gunshot)
                    .map(|i| (i.pos, i.dir, 0.0, u32::MAX)),
            );
        for (n, (pos, d, lift, victim)) in shots.enumerate() {
            let len = ((d.0 as f32).powi(2) + (d.1 as f32).powi(2)).sqrt();
            if len < 40.0 {
                continue;
            }
            let end = [pos.0 as f32, pos.1 as f32];
            let start = [end[0] - d.0 as f32, end[1] - d.1 as f32];
            let a = [start[0], start[1], ground(start[0], start[1]) + MUZZLE];
            let b = [end[0], end[1], ground(end[0], end[1]) + lift];
            let way = math::sub(b, a);
            let span = math::dot(way, way).sqrt();
            let seed = hash(victim ^ hash(pos.0 as u32 ^ hash(pos.1 as u32 ^ n as u32)));
            // The head runs from a little out of the muzzle to the end, from a phase of its own.
            let head = ((self.since * STREAK_SPEED + 0.25 * span * unit(seed, 6)) / span).clamp(0.08, 1.0);
            let streak = (STREAK_LENGTH * PIXELS_PER_METRE).min(span * head);
            let at = math::add(a, math::scale(way, head));
            // The model is one unit long and wide: thin, stretched to the streak's length.
            let m = oriented(at, math::scale(way, 1.0 / span), 0.0, 0.0, [1.0; 3]);
            let m = stretch(m, STREAK_RADIUS, streak / PIXELS_PER_METRE);
            out.push(instance(models.streak, m, true));
        }
    }
}

/// A model at `at` (world px, true height) with its glTF +z along `fwd` (unit), +y as
/// near world up as it allows, rolled by `roll` about +z and then turned end over end by
/// `pitch` about its x; `scale` per glTF axis (metres per model unit).
fn oriented(at: Vec3, fwd: Vec3, roll: f32, pitch: f32, scale: [f32; 3]) -> Mat4 {
    let z_up = [0.0, 0.0, 1.0];
    let mut up = math::sub(z_up, math::scale(fwd, math::dot(z_up, fwd)));
    if math::dot(up, up) < 1e-6 {
        up = [0.0, -1.0, 0.0];
    }
    let up = normalize(up);
    // glTF x = z × y, as `view3d::model_matrix`.
    let side = cross(fwd, up);
    let (rs, rc) = roll.sin_cos();
    let y = math::add(math::scale(up, rc), math::scale(side, rs));
    let (ps, pc) = pitch.sin_cos();
    let z = math::add(math::scale(fwd, pc), math::scale(y, ps));
    let y = math::sub(math::scale(y, pc), math::scale(fwd, ps));
    let x = cross(z, y);
    let k = PIXELS_PER_METRE;
    let col = |v: Vec3, s: f32| [v[0] * k * s, v[1] * k * s, v[2] * k * s, 0.0];
    [col(x, scale[0]), col(y, scale[1]), col(z, scale[2]), [at[0], at[1], at[2], 1.0]]
}

/// `m` with its x and y axes set to `radius` and its z to `length` (metres, unit-sized model).
fn stretch(mut m: Mat4, radius: f32, length: f32) -> Mat4 {
    for (c, s) in [(0, radius), (1, radius), (2, length)] {
        let v = [m[c][0], m[c][1], m[c][2]];
        let l = math::dot(v, v).sqrt().max(1e-6);
        for (i, x) in v.iter().enumerate() {
            m[c][i] = x / l * s * PIXELS_PER_METRE;
        }
    }
    m
}

fn instance(model: usize, transform: Mat4, transparent: bool) -> ModelInstance {
    ModelInstance {
        model,
        clip: None,
        time: 0.0,
        transform,
        decay: 0.0,
        morphs: [0.0; 3],
        transparent,
        player: 0,
        wounds: Default::default(),
        sprayed: Default::default(),
        damage: Default::default(),
        look: None,
        uniform: None,
        drape: None,
    }
}

/// A cannonball's faint trail: a few pale puffs along the last tick's way.
fn trail(sink: &mut dyn ParticleSink, at: Vec3, vel: Vec3, seed: u32) {
    for k in 0..3u32 {
        let back = math::sub(at, math::scale(vel, k as f32 / 3.0));
        sink.emit(&Burst {
            at: back,
            dir: [0.0, 0.0, 1.0],
            spread: 1.0,
            speed: [2.0, 6.0],
            count: 1,
            kind: ParticleKind::Dust,
            life: [0.5, 0.9],
            size: [1.5, 2.5],
            color: [176, 174, 168],
            scatter: 0.5,
            seed: seed ^ hash(k),
        });
    }
}

/// A burning fuse: sparks thrown off its tip and a wisp of smoke.
fn fuse(sink: &mut dyn ParticleSink, tip: Vec3, seed: u32) {
    sink.emit(&Burst {
        at: tip,
        dir: [0.0, 0.0, 1.0],
        spread: 1.4,
        speed: [20.0, 60.0],
        count: 4,
        kind: ParticleKind::Ember,
        life: [0.15, 0.35],
        size: [0.4, 0.8],
        color: [255, 196, 96],
        scatter: 0.3,
        seed,
    });
    sink.emit(&Burst {
        at: tip,
        dir: [0.0, 0.0, 1.0],
        spread: 0.8,
        speed: [3.0, 8.0],
        count: 1,
        kind: ParticleKind::Dust,
        life: [0.5, 0.8],
        size: [1.0, 2.0],
        color: [150, 148, 144],
        scatter: 0.2,
        seed: seed ^ 0x5e11,
    });
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: Vec3) -> Vec3 {
    let l = math::dot(v, v).sqrt();
    if l > 1e-6 { math::scale(v, 1.0 / l) } else { [0.0, 1.0, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oriented_matches_the_model_matrix_when_level() {
        // Facing south (+y) level: the same as `model_matrix` at yaw 0.
        let m = oriented([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], 0.0, 0.0, [1.0; 3]);
        let r = acr_render::view3d::model_matrix([1.0, 2.0, 3.0], 0.0, 1.0);
        for c in 0..4 {
            for i in 0..4 {
                assert!((m[c][i] - r[c][i]).abs() < 1e-4, "{c} {i}: {m:?} vs {r:?}");
            }
        }
        // Coming down: the tip (+z) points below the horizon.
        let d = normalize([1.0, 0.0, -1.0]);
        let m = oriented([0.0; 3], d, 0.0, 0.0, [1.0; 3]);
        assert!(math::transform_point(&m, [0.0, 0.0, 1.0])[2] < 0.0);
    }
}
