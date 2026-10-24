//! What a body leaves on the ground (#262 S5, `docs/damage-model.md` §3): severed limbs and
//! the thrown upper half of a body torn in two, gibs of `gore.glb` and red mist where a ball
//! struck point blank, guts spilled from an open belly, and dropped muskets and hats.
//! Render only: placement comes from hashes of the hit (unit and tick), never from the
//! simulation's random numbers; everything ages and goes with the corpses
//! (`corpse_minutes`).

use std::collections::{HashMap, HashSet};

use acr_render::damage::{Damage, Keep, Limb, Rig, Volume, WHOLE};
use acr_render::math::{self, Mat4, Vec3};
use acr_render::model;
use acr_render::model_renderer::{BloodPool, ModelInstance};
use acr_render::view3d::{self, PIXELS_PER_METRE};
use acr_sim::body::{Outcome, Part, Torn};
use acr_sim::combat::{DeathCause, DeathRecord, HitRecord};

use super::splatter::{self, Rng};
use crate::ui::settings::Gore;

/// Ticks a thrown piece flies (the game runs 10 ticks a second).
const FLIGHT: u64 = 7;
/// Energy (0..100) from which a cannonball hit counts as point blank: gibs and mist (the
/// energy from which a trunk hit may pulverize).
const POINT_BLANK: u8 = acr_sim::body::BALL_PULVERIZES;
/// Pieces a blast throws again at most ([`Debris::blast`]).
pub const BLAST_CAP: usize = 24;
/// Pieces, gibs and gear kept at most with `battlefield_marks=keep`.
pub const KEEP_CAP: usize = 1500;
/// Gibs a pulverized body leaves at least, and how many more at most.
const GIBS: (u32, u32) = (4, 4);

/// The models a unit type leaves behind, and its piece of the frame.
#[derive(Debug, Clone, Copy)]
pub struct UnitModel {
    pub model: usize,
    pub player: u8,
}

/// The parts of `gore.glb` by node name (model indices).
pub type GibModels = HashMap<String, usize>;

// A piece carries its wound volumes (#280); items are few and copied by value.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Copy)]
enum What {
    /// A severed piece or upper half: the unit's model lying on its back, its rest point
    /// `centre` (glTF metres) where it lands.
    Piece {
        damage: Damage,
        centre: Vec3,
    },
    Gib,
    /// Dropped gear (musket, hat): lies where the man was.
    Gear,
}

#[derive(Debug, Clone, Copy)]
struct Item {
    what: What,
    model: usize,
    player: u8,
    /// Thrown from (world x, y, z) to the landing point (world x, y; z from the ground).
    from: Vec3,
    to: [f32; 2],
    yaw: f32,
    /// Tumbles this many turns in flight.
    spin: f32,
    tick: u64,
    flight: u64,
    /// The tick its last flight began: the hit, or a shell's blast that threw it again
    /// ([`Debris::blast`]); its decay still runs from `tick`.
    launched: u64,
    /// Radius of its blood pool once landed (metres; 0 for none).
    pool: f32,
    seed: f32,
    /// Size of a gib (1: as modelled).
    scale: f32,
    /// The uniform colour a cloth shred takes (linear RGB).
    coat: Option<[f32; 3]>,
}

/// A throw from the hit: `(rng, near, far, cone, height)` → start and landing point.
type Throw<'a> = dyn FnMut(&mut Rng, f32, f32, f32, f32) -> (Vec3, [f32; 2]) + 'a;

#[derive(Default)]
pub struct Debris {
    items: Vec<Item>,
    /// Hits up to this tick have been looked at.
    seen: Option<u64>,
    /// Units whose gear already lies on the ground.
    dropped: HashSet<u32>,
}

/// Lays a standing model on its back: glTF y (up) turned to −z, the front up.
const ON_BACK: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn about_x(angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    [[1.0, 0.0, 0.0, 0.0], [0.0, c, s, 0.0], [0.0, -s, c, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

impl Debris {
    /// Pieces, gibs and gear lying (for the log).
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Takes the new hits: throws severed limbs and torn halves, scatters gibs.
    /// `unit(victim)` is the model of a unit drawn in 3D, `death(victim)` its death record.
    #[allow(clippy::too_many_arguments)]
    pub fn take_hits<'a>(
        &mut self,
        hits: impl Iterator<Item = &'a HitRecord>,
        now: u64,
        gore: Gore,
        unit: &dyn Fn(u32) -> Option<(UnitModel, &'a acr_render::damage::Rig, [f32; 3])>,
        death: &dyn Fn(u32) -> Option<DeathRecord>,
        tears: &dyn Fn(&HitRecord, &Rig) -> Vec<Volume>,
        gibs: &GibModels,
        ground: &dyn Fn(f32, f32) -> [f32; 3],
    ) {
        // The newest hit taken (#277): a step records its hits under the tick it began with,
        // which the frame before has already drawn, so `now` would skip them.
        let seen = self.seen;
        if gore == Gore::Off {
            self.seen = Some(now);
            return;
        }
        for hit in hits.filter(|h| seen.is_none_or(|s| h.tick > s) && h.tick <= now && !h.building) {
            self.seen = Some(self.seen.map_or(hit.tick, |s| s.max(hit.tick)));
            let Some(injury) = hit.injury else { continue };
            let mut rng = Rng::new(hit.tick, hit.victim ^ 0x5eed);
            let (dx, dy) = (hit.dir.0 as f32, hit.dir.1 as f32);
            let len = (dx * dx + dy * dy).sqrt();
            let base = if len > 0.0 { dy.atan2(dx) } else { rng.next() * std::f32::consts::TAU };
            let energy = f32::from(injury.energy) / 100.0;
            let start = ground(hit.pos.0 as f32, hit.pos.1 as f32);
            let mut throw = |rng: &mut Rng, near: f32, far: f32, cone: f32, height: f32| {
                let a = base + (rng.next() * 2.0 - 1.0) * cone;
                let d = (near + (far - near) * rng.next()) * PIXELS_PER_METRE;
                let from = [start[0], start[1], start[2] + height * PIXELS_PER_METRE];
                (from, [start[0] + a.cos() * d, start[1] + a.sin() * d])
            };
            let model = unit(hit.victim);
            // Tissue this hit tears out (#280): the pieces end in its ragged boundary, the
            // chunks it took fly as gibs sized by it.
            let mut ripped = Damage::default();
            if gore == Gore::Full
                && let Some((_, rig, _)) = model
            {
                for v in tears(hit, rig) {
                    ripped.push_volume(v);
                }
            }
            let taken = super::tears::size(ripped.volumes());
            if let Some((m, rig, _)) = model {
                if let (Part::Limb(limb), Outcome::Off) = (injury.part, injury.outcome) {
                    ripped.limb_mut(Limb::ALL[limb as usize]).cut = injury.level;
                }
                ripped.sever(rig);
                for limb in Limb::ALL {
                    let cut = ripped.limb(limb).cut;
                    if cut == WHOLE {
                        continue;
                    }
                    let damage = Damage { keep: Keep::Piece { limb, from: cut, to: WHOLE }, ..ripped };
                    let mid = (f32::from(cut) / 255.0 + 1.0) / 2.0;
                    let centre = rig.limbs[limb as usize].at(mid).0;
                    let height = if limb.is_arm() { 1.2 } else { 0.5 };
                    let (from, to) = throw(&mut rng, 0.8, 1.2 + 3.0 * energy, 0.5, height);
                    self.push(What::Piece { damage, centre }, m, from, to, &mut rng, hit.tick, 0.35);
                }
            }
            if gore == Gore::Full && taken > 0.0004 && !matches!(injury.outcome, Outcome::Torn(_)) {
                // Chunks by the tissue taken: a few small bits from a bite, big lumps from a
                // ball's track.
                let n = (taken / 0.0015).round().clamp(1.0, 6.0) as u32;
                let scale = (taken / n as f32 / 0.0008).cbrt().clamp(0.5, 1.6);
                let (player, coat) = model.map_or((0, None), |m| (m.0.player, Some(m.2)));
                let first = self.items.len();
                self.gibs(n, false, player, coat, hit.tick, &mut rng, gibs, &mut throw);
                for item in &mut self.items[first..] {
                    item.scale = scale;
                }
            }
            match (injury.part, injury.outcome) {
                (Part::Limb(_), Outcome::Off) => {}
                (_, Outcome::Torn(torn)) => {
                    if let (Torn::InTwo { .. }, Some((m, rig, _))) = (torn, model) {
                        let record = death(hit.victim).unwrap_or_default();
                        if let Some(upper) = super::damage_view::damage(&record.body, Some(torn))
                            .as_ref()
                            .and_then(super::damage_view::upper)
                        {
                            let upper = Damage { volumes: ripped.volumes, volumes_len: ripped.volumes_len, ..upper };
                            let (from, to) = throw(&mut rng, 1.0, 1.5 + 2.5 * energy, 0.3, 1.1);
                            let what = What::Piece { damage: upper, centre: rig.trunk.at(0.75).0 };
                            self.push(what, m, from, to, &mut rng, hit.tick, 0.6);
                        }
                    }
                    if gore == Gore::Full {
                        let n = match torn {
                            Torn::Pulverized => GIBS.0 + (rng.next() * (GIBS.1 + 1) as f32) as u32,
                            Torn::InTwo { .. } => 3,
                            Torn::Headless => 2,
                        };
                        let (player, coat) = model.map_or((0, None), |m| (m.0.player, Some(m.2)));
                        self.gibs(n, torn == Torn::Headless, player, coat, hit.tick, &mut rng, gibs, &mut throw);
                    }
                }
                _ if gore == Gore::Full
                    && hit.cause == DeathCause::Cannonball
                    && hit.killed
                    && injury.energy >= POINT_BLANK =>
                {
                    let (player, coat) = model.map_or((0, None), |m| (m.0.player, Some(m.2)));
                    self.gibs(GIBS.0, false, player, coat, hit.tick, &mut rng, gibs, &mut throw);
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push(&mut self, what: What, m: UnitModel, from: Vec3, to: [f32; 2], rng: &mut Rng, tick: u64, pool: f32) {
        self.items.push(Item {
            what,
            model: m.model,
            player: m.player,
            from,
            to,
            yaw: rng.next() * std::f32::consts::TAU,
            spin: 0.5 + rng.next() * 1.5,
            tick,
            flight: FLIGHT,
            launched: tick,
            pool,
            seed: rng.next() * 97.0,
            scale: 1.0,
            coat: None,
        });
    }

    /// `n` gibs scattered along the blow: flesh, bone and cloth (tinted by nation through
    /// the instance's player), skull and jaw for a head.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn gibs(
        &mut self,
        n: u32,
        head: bool,
        player: u8,
        coat: Option<[f32; 3]>,
        tick: u64,
        rng: &mut Rng,
        gibs: &GibModels,
        throw: &mut Throw,
    ) {
        const KINDS: [&str; 10] =
            ["flesh_0", "cloth_0", "bone_0", "flesh_1", "flesh_2", "cloth_1", "bone_1", "flesh_3", "organ_0", "bone_2"];
        let offset = (rng.next() * KINDS.len() as f32) as usize;
        for i in 0..n {
            let name = match (head, i) {
                (true, 0) => "skull_fragment",
                (true, 1) => "jaw",
                _ => KINDS[(offset + i as usize) % KINDS.len()],
            };
            let Some(&model) = gibs.get(name) else { continue };
            let (from, to) = throw(rng, 0.3, 4.5, 0.35, 1.0);
            self.push(What::Gib, UnitModel { model, player }, from, to, rng, tick, 0.0);
            if let Some(item) = self.items.last_mut() {
                item.coat = coat;
            }
        }
    }

    /// Leaves `gear` (dropped musket and hat models) of unit `id` lying at `at` (world)
    /// once.
    pub fn drop_gear(&mut self, id: u32, gear: &[usize], player: u8, at: Vec3, now: u64) {
        if !self.dropped.insert(id) {
            return;
        }
        let mut rng = Rng::new(now, id ^ 0x6ea2);
        for (k, &model) in gear.iter().enumerate() {
            let a = rng.next() * std::f32::consts::TAU;
            let d = (0.3 + 0.4 * k as f32 + 0.3 * rng.next()) * PIXELS_PER_METRE;
            let to = [at[0] + a.cos() * d, at[1] + a.sin() * d];
            let m = UnitModel { model, player };
            let mut item_rng = Rng::new(now, id ^ k as u32);
            self.push(What::Gear, m, at, to, &mut item_rng, now, 0.0);
            if let Some(last) = self.items.last_mut() {
                last.flight = 0;
            }
        }
    }

    /// Whether unit `id`'s held gear already lies on the ground.
    pub fn has_dropped(&self, id: u32) -> bool {
        self.dropped.contains(&id)
    }

    /// Lays the held gear of unit `id`, killed at `tick`, where it fell (#206): per item its
    /// model, the model's origin on the ground (world x, y) and its yaw. It lies from the
    /// landing on, rots, sinks and goes with the corpse (both age from the death).
    pub fn lay_gear(&mut self, id: u32, laid: &[(usize, [f32; 2], f32)], player: u8, tick: u64, now: u64) {
        if !self.dropped.insert(id) {
            return;
        }
        for (k, &(model, to, yaw)) in laid.iter().enumerate() {
            let mut rng = Rng::new(tick, id ^ (k as u32).rotate_left(8) ^ 0x51ed);
            self.push(What::Gear, UnitModel { model, player }, [to[0], to[1], 0.0], to, &mut rng, tick, 0.0);
            if let Some(last) = self.items.last_mut() {
                last.yaw = yaw;
                last.flight = 0;
                last.launched = now;
            }
        }
    }

    /// `battlefield_marks=keep`: forgets all but the newest `cap` pieces.
    pub fn keep_newest(&mut self, cap: usize) {
        if self.items.len() > cap {
            let n = self.items.len() - cap;
            self.items.drain(..n);
        }
    }

    /// Forgets debris older than `lifetime` ticks.
    pub fn expire(&mut self, now: u64, lifetime: u64) {
        self.items.retain(|i| now.saturating_sub(i.tick) < lifetime);
    }

    /// A shell burst or a ball's graze at `at` (world pixels): pieces, gibs and gear lying
    /// within `radius` are thrown again, outwards and the farther the nearer they lay (by
    /// `force`, 1 for a shell), at most `cap` of them (a shell [`BLAST_CAP`]), nearest first.
    /// Returns how many flew.
    #[allow(clippy::too_many_arguments)]
    pub fn blast(
        &mut self,
        at: [f32; 2],
        radius: f32,
        cap: usize,
        force: f32,
        now: u64,
        ground: &dyn Fn(f32, f32) -> [f32; 3],
    ) -> usize {
        let mut near: Vec<(f32, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.tick <= now && now >= i.launched + i.flight)
            .filter_map(|(k, i)| {
                let d = ((i.to[0] - at[0]).powi(2) + (i.to[1] - at[1]).powi(2)).sqrt();
                (d < radius).then_some((d, k))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(cap);
        let mut rng = Rng::new(now, (at[0] as u32) ^ (at[1] as u32).rotate_left(16));
        for &(d, k) in &near {
            let item = &mut self.items[k];
            let (dx, dy) = if d > 0.5 { ((item.to[0] - at[0]) / d, (item.to[1] - at[1]) / d) } else { (1.0, 0.0) };
            let push = (1.0 - d / radius) * (0.6 + 0.8 * rng.next()) * 3.0 * force * PIXELS_PER_METRE;
            let angle = (rng.next() - 0.5) * 0.8;
            let (s, c) = angle.sin_cos();
            let (dx, dy) = (dx * c - dy * s, dx * s + dy * c);
            item.from = ground(item.to[0], item.to[1]);
            item.to = [item.to[0] + dx * push, item.to[1] + dy * push];
            item.launched = now;
            item.flight = FLIGHT;
            item.spin += 0.5 + rng.next() * 2.0;
        }
        near.len()
    }

    /// This frame's instances and blood pools of the debris.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        now: u64,
        lifetime: u64,
        keep: bool,
        ground: &dyn Fn(f32, f32) -> [f32; 3],
        shown: &dyn Fn(f32, f32) -> bool,
        instances: &mut Vec<ModelInstance>,
        pools: &mut Vec<BloodPool>,
        stain: Option<&dyn Fn(f32, f32) -> f32>,
    ) {
        let removed_at = (2 * lifetime).min(u64::from(u16::MAX)) as u16;
        // Well off the view (where it flies from and where it lands): nothing to draw.
        let near = |i: &&Item| shown(i.to[0], i.to[1]) || shown(i.from[0], i.from[1]);
        for item in self.items.iter().filter(|i| i.tick <= now).filter(near) {
            let age = now - item.tick;
            let flown = now.saturating_sub(item.launched);
            let t = if item.flight == 0 { 1.0 } else { (flown as f32 / item.flight as f32).min(1.0) };
            let land = ground(item.to[0], item.to[1]);
            let at = if t < 1.0 {
                let lerp = |a: f32, b: f32| a + (b - a) * t;
                // A short arc: up a little and down onto the ground.
                let arc = 4.0 * t * (1.0 - t) * 0.6 * PIXELS_PER_METRE;
                [lerp(item.from[0], land[0]), lerp(item.from[1], land[1]), lerp(item.from[2], land[2]) + arc]
            } else {
                land
            };
            let tumble = (1.0 - t) * item.spin * std::f32::consts::TAU;
            let decay = model::corpse_decay((2 * age).min(u64::from(u16::MAX)) as u16, removed_at);
            let (morphs, sink) = model::decay_pose(decay, true);
            let mut ground_at = at;
            ground_at[2] -= sink * PIXELS_PER_METRE * 0.5;
            let (transform, damage) = match item.what {
                What::Piece { damage, centre } => {
                    // Flat on its back in the bind pose, the piece's middle where it lands.
                    let base = view3d::model_matrix(ground_at, item.yaw, 1.0);
                    let mut m = math::mul(&base, &math::mul(&ON_BACK, &about_x(tumble)));
                    let q = math::transform_point(&m, centre);
                    let lift = 0.06 * PIXELS_PER_METRE;
                    for (k, target) in [ground_at[0], ground_at[1], ground_at[2] + lift].into_iter().enumerate() {
                        m[3][k] += target - q[k];
                    }
                    (m, damage)
                }
                What::Gear if t >= 1.0 => {
                    // Settled flat on the ground's slope (#206).
                    let (s, c) = item.yaw.sin_cos();
                    let d = 0.3 * PIXELS_PER_METRE;
                    let h = |dx: f32, dy: f32| ground(land[0] + dx, land[1] + dy)[2];
                    let pitch = (h(s * d, c * d) - h(-s * d, -c * d)) / (2.0 * d);
                    let roll = (h(-c * d, s * d) - h(c * d, -s * d)) / (2.0 * d);
                    (view3d::model_matrix_tilted(ground_at, item.yaw, item.scale, pitch, roll), Damage::default())
                }
                What::Gib | What::Gear => {
                    let base = view3d::model_matrix(ground_at, item.yaw, item.scale);
                    (math::mul(&base, &about_x(tumble)), Damage::default())
                }
            };
            instances.push(ModelInstance {
                model: item.model,
                clip: None,
                time: 0.0,
                transform,
                decay,
                morphs: if matches!(item.what, What::Piece { .. }) { [morphs[0], morphs[1], 0.0] } else { [0.0; 3] },
                transparent: false,
                player: item.player,
                wounds: Default::default(),
                sprayed: match item.coat {
                    // A cloth shred's colour rides in the spray texel (`model.wgsl`, kind 3).
                    Some(c) => acr_render::model_renderer::Sprayed { point: c, radius: -1.0, amount: 0.0, age: 0.0 },
                    None => Default::default(),
                },
                damage,
                look: None,
                uniform: None,
                drape: None,
                rise: None,
            });
            // A large remain (half a body) stains the ground as it rots (`decay_stain`,
            // `seep.rs`), smaller than a whole body and round; it goes with the remain.
            if let (Some(turf), What::Piece { .. }, true) = (stain, &item.what, t >= 1.0)
                && let Some(mut p) =
                    super::seep::stain(land, item.yaw.sin_cos(), 0.55, decay, turf(land[0], land[1]), item.seed)
            {
                p.stretch = 0.2;
                if !keep {
                    p.fade = 1.0 - model::ramp(age as f32, lifetime as f32 * 0.8, lifetime as f32);
                }
                pools.push(p);
            }
            if item.pool > 0.0 && t >= 1.0 {
                let grow = model::ramp(age as f32 - item.flight as f32, 0.0, 60.0);
                pools.push(BloodPool {
                    centre: land,
                    radius: item.pool * (0.4 + 0.6 * grow) * PIXELS_PER_METRE,
                    age: model::ramp(age as f32, 60.0, 900.0),
                    fade: if keep {
                        1.0
                    } else {
                        1.0 - model::ramp(age as f32, lifetime as f32 * 0.8, lifetime as f32)
                    },
                    seed: item.seed,
                    ..Default::default()
                });
            }
        }
    }
}

/// Fine red mist of a pulverized body or a point-blank ball: extra spray (#239) all around
/// and far along the ball's path, settling as speckles.
pub fn mist(hit: &HitRecord, now: u64, gore: Gore, ground: &dyn Fn(f32, f32) -> [f32; 3], out: &mut Vec<BloodPool>) {
    if gore != Gore::Full {
        return;
    }
    let Some(injury) = hit.injury else { return };
    let burst = matches!(injury.outcome, Outcome::Torn(Torn::Pulverized))
        || (hit.cause == DeathCause::Cannonball && hit.killed && injury.energy >= POINT_BLANK)
        // A ball tearing a limb or a chunk of trunk away (#280).
        || (hit.cause == DeathCause::Cannonball
            && injury.energy >= acr_sim::body::BALL_SEVERS
            && matches!(injury.outcome, Outcome::Off | Outcome::Kill | Outcome::Torn(_)));
    if !burst {
        return;
    }
    for (k, cause) in [(1u32, DeathCause::Explosion), (2, DeathCause::Cannonball), (3, DeathCause::Cannonball)] {
        let spray = HitRecord { victim: hit.victim ^ (k << 24), cause, killed: true, ..*hit };
        splatter::splatter(&spray, now, gore, ground, out);
    }
}

/// Guts spilled from an open belly beside a body lying (or writhing) at `at` (world),
/// facing `yaw`: the heap at the belly and a coil lying out on the ground.
pub fn guts(id: u32, at: Vec3, yaw: f32, decay: f32, player: u8, gibs: &GibModels, out: &mut Vec<ModelInstance>) {
    let mut rng = Rng::new(u64::from(id), 0x6075);
    let (s, c) = yaw.sin_cos();
    // The belly lies ~0.35 m ahead of the ground point along the body; the guts spill to
    // one side of it.
    let side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let place = |ahead: f32, out_m: f32| {
        let (a, o) = (ahead * PIXELS_PER_METRE, out_m * PIXELS_PER_METRE * side);
        [at[0] + s * a - c * o, at[1] + c * a + s * o, at[2]]
    };
    let coil = ["intestine_0", "intestine_1", "intestine_2"][(rng.next() * 3.0) as usize % 3];
    for (name, ahead, out_m) in [("guts_heap", 0.35, 0.22), (coil, 0.3, 0.5)] {
        let Some(&model) = gibs.get(name) else { continue };
        out.push(ModelInstance {
            model,
            clip: None,
            time: 0.0,
            transform: view3d::model_matrix(place(ahead, out_m), yaw + rng.next() * 2.0, 1.0),
            decay,
            morphs: [0.0; 3],
            transparent: false,
            player,
            wounds: Default::default(),
            sprayed: Default::default(),
            damage: Damage::default(),
            look: None,
            uniform: None,
            drape: None,
            rise: None,
        });
    }
}

/// How a piece of held gear lies ([`lying_flat`]): its longest axis in its mesh's own
/// coordinates (a unit axis), and where along glTF x it reaches when lying (metres from its
/// origin: the middle and half the length).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Lying {
    pub axis: [f32; 3],
    pub mid: f32,
    pub half: f32,
}

/// Held gear cut from its unit model (`ModelData::node_part`) turned to lie on the ground:
/// its longest axis along glTF x, resting on y = 0.
pub fn lying_flat(mut m: model::ModelData) -> (model::ModelData, Lying) {
    let Some(node) = m.mesh_nodes().first().and_then(|name| m.nodes.iter().position(|n| n.name == *name)) else {
        return (m, Lying::default());
    };
    let points = || m.primitives.iter().flat_map(|p| p.vertices.iter().map(|v| v.position));
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in points() {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size: [f32; 3] = std::array::from_fn(|k| hi[k] - lo[k]);
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let (rotation, axis) = if size[1] >= size[0] && size[1] >= size[2] {
        ([0.0, 0.0, -h, h], [0.0, 1.0, 0.0])
    } else if size[2] >= size[0] {
        ([0.0, h, 0.0, h], [0.0, 0.0, 1.0])
    } else {
        ([0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0])
    };
    let scale = m.nodes[node].scale;
    let turn = math::from_trs([0.0; 3], rotation, scale);
    let bottom = points().map(|p| math::transform_point(&turn, p)[1]).fold(f32::MAX, f32::min);
    let (lo, hi) = points()
        .map(|p| math::transform_point(&turn, p)[0])
        .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    let n = &mut m.nodes[node];
    n.rotation = rotation;
    n.translation = [0.0, -bottom.min(1.0), 0.0];
    (m, Lying { axis, mid: (lo + hi) / 2.0, half: (hi - lo) / 2.0 })
}

/// The colour of a unit's coat (linear RGB): the commonest colour of its upper sleeves.
pub fn coat(model: &model::ModelData) -> [f32; 3] {
    let mut bins: HashMap<u32, (u32, [f32; 3])> = HashMap::new();
    let lin = |c: u8| (f32::from(c) / 255.0).powf(2.2);
    for prim in &model.primitives {
        let material = &model.materials[prim.material];
        let Some(image) = material.texture.and_then(|t| model.images.get(t)) else { continue };
        if image.width == 0 || image.height == 0 {
            continue;
        }
        // The sleeves above the elbow: the coat's colour, without waistcoat, belts or hands.
        let sleeve =
            |v: &&model::Vertex| (2..=5).contains(&(v.body[0].round() as i32)) && (0.0..0.35).contains(&v.body[1]);
        for v in prim.vertices.iter().filter(sleeve) {
            let x = ((v.uv[0].rem_euclid(1.0) * image.width as f32) as u32).min(image.width - 1);
            let y = ((v.uv[1].rem_euclid(1.0) * image.height as f32) as u32).min(image.height - 1);
            let at = ((y * image.width + x) * 4) as usize;
            let Some(px) = image.rgba.get(at..at + 3) else { continue };
            let rgb = [lin(px[0]), lin(px[1]), lin(px[2])];
            let b = material.base_color;
            let rgb = [rgb[0] * b[0], rgb[1] * b[1], rgb[2] * b[2]];
            let key = rgb.iter().fold(0, |k, &c| k * 8 + (c.sqrt() * 7.99) as u32);
            let e = bins.entry(key).or_insert((0, [0.0; 3]));
            e.0 += 1;
            (0..3).for_each(|k| e.1[k] += rgb[k]);
        }
    }
    // Coloured bins count more than white shirt cuffs.
    let weight = |e: &(u32, [f32; 3])| {
        let c = e.1.map(|c| c / e.0 as f32);
        let (hi, lo) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
        let sat = if hi > 1e-4 { (hi - lo) / hi } else { 0.0 };
        (e.0 as f32 * (0.15 + sat)) as u32
    };
    bins.into_values().max_by_key(weight).map_or([0.35, 0.05, 0.04], |(n, sum)| sum.map(|c| c / n as f32))
}
#[cfg(test)]
mod coat_tests {
    /// The showcase's English line infantry wears red (skipped without the local models).
    #[test]
    fn an_english_coat_is_red() {
        let path = r"C:\Users\verwalter\WIP\ACR-analysis\showcase\models\engm18.glb";
        let Ok(bytes) = std::fs::read(path) else { return };
        let m = acr_render::model::ModelData::from_glb(&bytes).unwrap();
        let c = super::coat(&m);
        assert!(c[0] > 3.0 * c[1] && c[0] > 3.0 * c[2], "{c:?}");
    }
}
