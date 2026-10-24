//! The rounds in flight as models (#308, docs/rendering-3d.md "Projectiles"), render only:
//! arrows turned along their flight, a musket ball's short grey streak from the muzzle to
//! where it struck, cannonballs with a faint trail, shells tumbling with a sparking fuse,
//! tomahawks and stones turning over, spears rolling. Arrows that missed stick in the ground
//! at the angle they came down, for as long as corpses lie. The simulation's projectile
//! slots, impacts and hits are only read; variety comes from hashes of the slot and the
//! tick. The 2D sprites of the rounds drawn here are left out (`effects::projectile_sprites`).

use std::collections::{HashMap, VecDeque};

use acr_render::math::{self, Mat4, Vec3};
use acr_render::missiles;
use acr_render::model_renderer::{ModelInstance, ModelRenderer};
use acr_render::particles::{Burst, ParticleKind, ParticleSink, hash, unit};
use acr_render::view3d::{COS_E, PIXELS_PER_METRE};
use acr_sim::World;
use acr_sim::artillery::GunMode;
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
/// drawn about twice as large to show at the game's zoom).
const BALL_RADIUS: f32 = 0.09;
const GRAPE_RADIUS: f32 = 0.03;
/// A musket ball's streak: length and radius (metres), and how fast it is drawn to travel
/// (world px per second: slowed from the ball's ~400 m/s so it is seen at all).
const STREAK_LENGTH: f32 = 2.2;
const STREAK_RADIUS: f32 = 0.022;
const STREAK_SPEED: f32 = 4500.0;
/// Muzzle and chest heights (true world px) the streak runs between.
const MUZZLE: f32 = 1.4 * PIXELS_PER_METRE;
const CHEST: f32 = 1.2 * PIXELS_PER_METRE;
/// A gun's muzzle (true world px) its arcs start from ([`Missiles::draw`]).
const GUN_MUZZLE: f32 = 0.9 * PIXELS_PER_METRE;
/// How deep a stuck arrow's head goes into the ground (metres), and at most this many lie.
const STUCK_DEPTH: f32 = 0.12;
const STUCK_CAP: usize = 2048;
/// How far a shell turns over per tick (rad).
const SHELL_SPIN: f32 = 0.3;
/// A shell's burst looked ahead for at most this many ticks.
const SHELL_LOOKAHEAD: u32 = 1200;
/// A cannonball's trail: a puff every this many world px, at most this many a frame.
const TRAIL_STEP: f32 = 2.5;
const TRAIL_MAX: u32 = 32;

#[derive(Default)]
pub struct Missiles {
    models: Option<Models>,
    stuck: VecDeque<Stuck>,
    /// Arrows and spears (`true`) in flight at the last two ticks drawn: point and
    /// direction (true world px).
    flying: Vec<(Vec3, Vec3, bool)>,
    flown: Vec<(Vec3, Vec3, bool)>,
    /// Cannonballs drawn last frame by slot: their seed and where (for the trail's way).
    balls: HashMap<usize, (u32, Vec3)>,
    /// Arrows, bolts and spears stuck in the men they hit (#308).
    pub body: super::body_arrows::BodyArrows,
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
        let mut add = |m| renderer.add_small_model(device, queue, m);
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
        self.balls.clear();
        self.body.clear();
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
        let mut drawn = 0;
        let mut balls = HashMap::new();
        for (slot, p) in world.projectiles.slots.iter().enumerate() {
            let Some(p) = p else { continue };
            let Some(kind) = round(world, p) else { continue };
            drawn += 1;
            let seed = hash(slot as u32 ^ hash(p.weapon as u32 ^ p.sender.map_or(0, |s| s.0)));
            let k = 1.0 / 16384.0;
            let vel = [p.vel.0 as f32 * k, p.vel.1 as f32 * k, p.vel.2 as f32 * k / COS_E];
            let az = p.az as f32 * k / COS_E;
            let mut at = [
                p.pos.0 as f32 * k + vel[0] * frac,
                p.pos.1 as f32 * k + vel[1] * frac,
                p.pos.2 as f32 * k / COS_E + vel[2] * frac + 0.5 * az * frac * frac,
            ];
            let angle = world.catalog.weapons.get(p.weapon).propagation == Propagation::Angle;
            let mut dir = normalize([vel[0], vel[1], vel[2] + az * frac]);
            // A shell's high arc is drawn as its own parabola from the muzzle to the burst.
            let arc = if kind == Round::Shell && angle { shell_arc(world, p, frac, ground) } else { None };
            if let Some((a, d)) = arc {
                (at, dir) = (a, d);
            } else if angle {
                // A gun's arc starts a few pixels above the ground (its sprite's offset shows
                // the muzzle height): raised by up to the muzzle's height, down to nothing over
                // the last stretch before it lands.
                let left = ((p.dest.0 as f32 * k - at[0]).powi(2) + (p.dest.1 as f32 * k - at[1]).powi(2)).sqrt();
                at[2] += (left * 0.1).min(GUN_MUZZLE);
            }
            // Turning over: per tick of its life, from a phase of its own.
            let turn = (p.frame as f32 + frac) + 6.0 * unit(seed, 1);
            let (model, transform) = match kind {
                Round::Arrow => {
                    if new_tick {
                        self.flying.push((at, dir, false));
                    }
                    (models.arrow, oriented(at, dir, 0.0, 0.0, [1.0; 3]))
                }
                Round::Cannonball | Round::Grapeshot => {
                    let r = if kind == Round::Cannonball { BALL_RADIUS } else { GRAPE_RADIUS };
                    if kind == Round::Cannonball {
                        // The way since the last frame (at most one tick's): a dense,
                        // continuous trail between the frames, not puffs once per tick.
                        let from = match self.balls.get(&slot) {
                            Some(&(s, last)) if s == seed => {
                                let d = math::sub(at, last);
                                let (step, reach) = (math::dot(d, d), math::dot(vel, vel) * 2.25 + 1.0);
                                if step <= reach { last } else { math::sub(at, vel) }
                            }
                            _ => math::sub(at, math::scale(vel, frac.max(0.2))),
                        };
                        trail(sink, from, at, seed ^ hash(now as u32 ^ (self.since * 1000.0) as u32));
                        balls.insert(slot, (seed, at));
                    }
                    (models.ball, oriented(at, dir, 0.0, 0.0, [r; 3]))
                }
                Round::Shell => {
                    // A slow turn over (a shell barely spins), the fuse sparking every frame.
                    let m = oriented(at, dir, 0.0, turn * SHELL_SPIN, [1.0; 3]);
                    let tip = math::transform_point(&m, [0.0, missiles::SHELL_FUSE, 0.0]);
                    fuse(sink, tip, seed ^ hash(now as u32 ^ (self.since * 1000.0) as u32), new_tick);
                    (models.shell, m)
                }
                Round::Tomahawk => (models.tomahawk, oriented(at, dir, 0.0, turn * 1.7, [1.0; 3])),
                Round::Spear => {
                    if new_tick {
                        self.flying.push((at, dir, true));
                    }
                    (models.spear, oriented(at, dir, turn * 0.8, 0.0, [1.0; 3]))
                }
                Round::Stone => (models.stone, oriented(at, dir, turn, turn * 1.3, [1.0; 3])),
            };
            out.push(instance(model, transform, false));
        }
        self.balls = balls;
        if new_tick && now.is_multiple_of(20) && (drawn > 0 || !self.stuck.is_empty()) {
            log::info!(
                "3d: {drawn} rounds in flight, {} arrows stuck, {} in bodies",
                self.stuck.len(),
                self.body.len()
            );
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
                .filter(|f| !f.2)
                .map(|&(a, d, _)| ((a[0] - p[0]).abs() + (a[1] - p[1]).abs(), d))
                .filter(|&(dist, _)| dist < 60.0)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let d = match near {
                Some((_, d)) => d,
                None => {
                    let (dx, dy) = (i.dir.0 as f32, i.dir.1 as f32);
                    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
                    [dx / l * 0.77, dy / l * 0.77, -0.64]
                }
            };
            // Into the ground at 35..55° (a flat shot's arrow skids and turns up its point
            // into the turf), the bearing wobbling by a hash of the impact.
            let seed = hash(i.pos.0 as u32 ^ hash(i.pos.1 as u32 ^ hash(i.tick as u32 + n as u32)));
            let fall = (-d[2]).atan2((d[0] * d[0] + d[1] * d[1]).sqrt()).max(0.6) + 0.35 * unit(seed, 2);
            let bearing = d[1].atan2(d[0]) + 0.25 * (unit(seed, 3) - 0.5);
            let d = [bearing.cos() * fall.cos(), bearing.sin() * fall.cos(), -fall.sin()];
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

impl Missiles {
    /// Takes the new arrow and spear hits into the bodies (#308), with the flight of the
    /// round seen nearest the victim when there is one.
    pub fn stick(&mut self, world: &World) {
        let now = world.tick.0;
        let facing = |v: u32| world.units.get(v as usize).filter(|u| u.id.0 == v && !u.building).map(|u| u.dir);
        let (flown, flying) = (&self.flown, &self.flying);
        let flight = |p: [f32; 2], spear: bool| {
            flown
                .iter()
                .chain(flying)
                .filter(|f| f.2 == spear)
                .map(|&(a, d, _)| ((a[0] - p[0]).abs() + (a[1] - p[1]).abs(), d))
                .filter(|&(dist, _)| dist < 90.0)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, d)| d)
        };
        self.body.take_hits(world.hits.iter(), now, &facing, &flight);
    }

    /// The shafts stuck in bodies appended to `out` (see `BodyArrows::draw`).
    pub fn draw_body<'a>(
        &mut self,
        drawn: &HashMap<u32, usize>,
        model: &dyn Fn(usize) -> Option<&'a acr_render::model::ModelData>,
        poses: &[&HashMap<usize, acr_render::model::Palette>],
        out: &mut Vec<ModelInstance>,
    ) {
        let Some(models) = self.models else { return };
        self.body.draw(drawn, model, poses, (models.arrow, models.spear), out);
    }
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
        rise: None,
    }
}

/// A cannonball's faint trail from `from` to `to` (this frame's way): small pale puffs
/// every [`TRAIL_STEP`] px, overlapping, that swell and fade, so it reads as one ribbon
/// thinning out behind the ball (at most [`TRAIL_MAX`] a frame).
fn trail(sink: &mut dyn ParticleSink, from: Vec3, to: Vec3, seed: u32) {
    let way = math::sub(to, from);
    let len = math::dot(way, way).sqrt();
    let n = ((len / TRAIL_STEP).ceil() as u32).clamp(1, TRAIL_MAX);
    for k in 0..n {
        let at = math::add(from, math::scale(way, (k as f32 + 0.5) / n as f32));
        sink.emit(&Burst {
            at,
            dir: [0.0, 0.0, 1.0],
            spread: 1.0,
            speed: [1.0, 3.0],
            count: 1,
            kind: ParticleKind::Dust,
            life: [0.7, 1.0],
            size: [1.4, 2.0],
            color: [200, 198, 192],
            scatter: 0.2,
            seed: seed ^ hash(k),
        });
    }
}

/// A shell's place and heading (true world px, unit) on a ballistic parabola from the
/// muzzle to where the simulation will burst it, `frac` of the tick on (render only).
///
/// The simulation's `ANGLE` flight (`projectile::create`) starts at the sprite's muzzle a
/// few px above the ground and is integrated with `vz += az` each tick; it bursts within
/// 16 px of `dest` or below the ground. Its z does come down to the ground there, but drawn
/// as it is (`/ COS_E`, plus the up to 0.9 m raise of the flat gun arcs) it peaked about 15 %
/// higher than the elevation gives and was still 1.5 m up at the last tick before the burst.
/// Here the shell's ticks flown `k` come from its climb (`vz0 = |v| tan(elevation)`, less
/// `az` per tick), the ticks left `r` from running the simulation's own steps on to the
/// burst, and the height is the parabola through the muzzle (`GUN_MUZZLE` above the ground
/// at the start) and the ground at the burst whose launch angle is the elevation: it peaks a
/// quarter of the range times its tangent high and lands on the burst at its tick. The
/// horizontal place stays the simulation's (constant speed), so this is a true ballistic arc.
fn shell_arc(world: &World, p: &Projectile, frac: f32, ground: &dyn Fn(f32, f32) -> f32) -> Option<(Vec3, Vec3)> {
    if p.az >= 0 {
        return None;
    }
    let wp = world.catalog.weapons.get(p.weapon);
    let deg = GunMode::from_u8(p.shot.mode).and_then(|m| m.elevation()).unwrap_or(wp.speed);
    let tan = f64::from(deg).to_radians().tan();
    let (vx, vy, vz, az) = (i64::from(p.vel.0), i64::from(p.vel.1), i64::from(p.vel.2), i64::from(p.az));
    let v = ((vx * vx + vy * vy) as f64).sqrt().floor();
    let vz0 = (v * tan).trunc();
    let flown = ((vz0 - vz as f64) / -(az as f64)).round().max(0.0) as i64;
    // The simulation's steps run on to the burst (`projectile::process` (b)–(d)).
    let dest = (i64::from(p.dest.0), i64::from(p.dest.1), i64::from(p.dest.2));
    let (mut x, mut y, mut z, mut vz) = (i64::from(p.pos.0), i64::from(p.pos.1), i64::from(p.pos.2), vz);
    let mut left = 0i64;
    while left < i64::from(SHELL_LOOKAHEAD) {
        left += 1;
        vz += az;
        (x, y, z) = (x + vx, y + vy, z + vz);
        let near = (z - dest.2).abs() + (y - dest.1).abs() + (x - dest.0).abs() < 0x40000;
        let below = i64::from(world.map.heights.at((x >> 14) as i32, (y >> 14) as i32)) > z >> 14;
        if near || below {
            break;
        }
    }
    let k = 1.0 / 16384.0;
    let step = [vx as f32 * k, vy as f32 * k];
    let now = [p.pos.0 as f32 * k, p.pos.1 as f32 * k];
    let start = [now[0] - step[0] * flown as f32, now[1] - step[1] * flown as f32];
    let burst = [now[0] + step[0] * left as f32, now[1] + step[1] * left as f32];
    let span = [burst[0] - start[0], burst[1] - start[1]];
    let range = (span[0] * span[0] + span[1] * span[1]).sqrt();
    if range < 1.0 {
        return None;
    }
    let u = ((flown as f32 + frac) / (flown + left) as f32).clamp(0.0, 1.0);
    let h0 = ground(start[0], start[1]) + GUN_MUZZLE;
    let h1 = ground(burst[0], burst[1]);
    let peak = range * tan as f32 / 4.0;
    let h = h0 + (h1 - h0) * u + 4.0 * peak * u * (1.0 - u);
    let climb = (h1 - h0) + 4.0 * peak * (1.0 - 2.0 * u);
    let at = [start[0] + span[0] * u, start[1] + span[1] * u, h];
    Some((at, normalize([span[0], span[1], climb])))
}

/// A burning fuse: sparks thrown off its tip every frame and a wisp of smoke once a tick.
fn fuse(sink: &mut dyn ParticleSink, tip: Vec3, seed: u32, smoke: bool) {
    sink.emit(&Burst {
        at: tip,
        dir: [0.0, 0.0, 1.0],
        spread: 1.4,
        speed: [20.0, 60.0],
        count: 1,
        kind: ParticleKind::Ember,
        life: [0.15, 0.35],
        size: [0.4, 0.8],
        color: [255, 196, 96],
        scatter: 0.3,
        seed,
    });
    if !smoke {
        return;
    }
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
