//! What a body leaves on the ground (#262 S5, `docs/damage-model.md` §3): severed limbs and
//! the thrown upper half of a body torn in two, gibs of `gore.glb` and red mist where a ball
//! struck point blank, guts spilled from an open belly, and dropped muskets and hats.
//! Render only: placement comes from hashes of the hit (unit and tick), never from the
//! simulation's random numbers; everything ages and goes with the corpses
//! (`corpse_minutes`).

use std::collections::{HashMap, HashSet};

use acr_render::damage::{Damage, Keep};
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
/// Energy (0..100) from which a cannonball hit counts as point blank: gibs and mist.
const POINT_BLANK: u8 = 90;
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
    /// Radius of its blood pool once landed (metres; 0 for none).
    pool: f32,
    seed: f32,
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
        unit: &dyn Fn(u32) -> Option<(UnitModel, &'a acr_render::damage::Rig)>,
        death: &dyn Fn(u32) -> Option<DeathRecord>,
        gibs: &GibModels,
        ground: &dyn Fn(f32, f32) -> [f32; 3],
    ) {
        let seen = self.seen.replace(now);
        if gore == Gore::Off {
            return;
        }
        for hit in hits.filter(|h| seen.is_none_or(|s| h.tick > s) && h.tick <= now && !h.building) {
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
            match (injury.part, injury.outcome) {
                (Part::Limb(limb), Outcome::Off) => {
                    let Some((m, rig)) = model else { continue };
                    let limb = acr_render::damage::Limb::ALL[limb as usize];
                    let damage =
                        Damage { keep: Keep::Piece { limb, from: injury.level, to: 255 }, ..Damage::default() };
                    let mid = (f32::from(injury.level) / 255.0 + 1.0) / 2.0;
                    let centre = rig.limbs[limb as usize].at(mid).0;
                    let height = if limb.is_arm() { 1.2 } else { 0.5 };
                    let (from, to) = throw(&mut rng, 0.8, 1.2 + 3.0 * energy, 0.5, height);
                    self.push(What::Piece { damage, centre }, m, from, to, &mut rng, hit.tick, 0.35);
                }
                (_, Outcome::Torn(torn)) => {
                    if let (Torn::InTwo { .. }, Some((m, rig))) = (torn, model) {
                        let record = death(hit.victim).unwrap_or_default();
                        if let Some(upper) = super::damage_view::damage(&record.body, Some(torn))
                            .as_ref()
                            .and_then(super::damage_view::upper)
                        {
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
                        let player = model.map_or(0, |m| m.0.player);
                        self.gibs(n, torn == Torn::Headless, player, hit.tick, &mut rng, gibs, &mut throw);
                    }
                }
                _ if gore == Gore::Full
                    && hit.cause == DeathCause::Cannonball
                    && hit.killed
                    && injury.energy >= POINT_BLANK =>
                {
                    let player = model.map_or(0, |m| m.0.player);
                    self.gibs(GIBS.0, false, player, hit.tick, &mut rng, gibs, &mut throw);
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
            pool,
            seed: rng.next() * 97.0,
        });
    }

    /// `n` gibs scattered along the blow: flesh, bone and cloth (tinted by nation through
    /// the instance's player), skull and jaw for a head.
    #[allow(clippy::too_many_arguments)]
    fn gibs(&mut self, n: u32, head: bool, player: u8, tick: u64, rng: &mut Rng, gibs: &GibModels, throw: &mut Throw) {
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

    /// Forgets debris older than `lifetime` ticks.
    pub fn expire(&mut self, now: u64, lifetime: u64) {
        self.items.retain(|i| now.saturating_sub(i.tick) < lifetime);
    }

    /// This frame's instances and blood pools of the debris.
    pub fn draw(
        &self,
        now: u64,
        lifetime: u64,
        ground: &dyn Fn(f32, f32) -> [f32; 3],
        instances: &mut Vec<ModelInstance>,
        pools: &mut Vec<BloodPool>,
    ) {
        let removed_at = (2 * lifetime).min(u64::from(u16::MAX)) as u16;
        for item in self.items.iter().filter(|i| i.tick <= now) {
            let age = now - item.tick;
            let t = if item.flight == 0 { 1.0 } else { (age as f32 / item.flight as f32).min(1.0) };
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
                What::Gib | What::Gear => {
                    let base = view3d::model_matrix(ground_at, item.yaw, 1.0);
                    (math::mul(&base, &about_x(tumble)), Damage::default())
                }
            };
            instances.push(ModelInstance {
                model: item.model,
                clip: None,
                time: 0.0,
                transform,
                decay,
                morphs: if matches!(item.what, What::Piece { .. }) { morphs } else { [0.0; 2] },
                transparent: false,
                player: item.player,
                wounds: Default::default(),
                sprayed: Default::default(),
                damage,
            });
            if item.pool > 0.0 && t >= 1.0 {
                let grow = model::ramp(age as f32 - item.flight as f32, 0.0, 60.0);
                pools.push(BloodPool {
                    centre: land,
                    radius: item.pool * (0.4 + 0.6 * grow) * PIXELS_PER_METRE,
                    age: model::ramp(age as f32, 60.0, 900.0),
                    fade: 1.0 - model::ramp(age as f32, lifetime as f32 * 0.8, lifetime as f32),
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
        || (hit.cause == DeathCause::Cannonball && hit.killed && injury.energy >= POINT_BLANK);
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
            morphs: [0.0; 2],
            transparent: false,
            player,
            wounds: Default::default(),
            sprayed: Default::default(),
            damage: Damage::default(),
        });
    }
}

/// Held gear cut from its unit model (`ModelData::node_part`) turned to lie on the ground:
/// its longest axis along glTF x, resting on y = 0.
pub fn lying_flat(mut m: model::ModelData) -> model::ModelData {
    let Some(node) = m.mesh_nodes().first().and_then(|name| m.nodes.iter().position(|n| n.name == *name)) else {
        return m;
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
    let rotation = if size[1] >= size[0] && size[1] >= size[2] {
        [0.0, 0.0, -h, h]
    } else if size[2] >= size[0] {
        [0.0, h, 0.0, h]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    let scale = m.nodes[node].scale;
    let turn = math::from_trs([0.0; 3], rotation, scale);
    let bottom = points().map(|p| math::transform_point(&turn, p)[1]).fold(f32::MAX, f32::min);
    let n = &mut m.nodes[node];
    n.rotation = rotation;
    n.translation = [0.0, -bottom.min(1.0), 0.0];
    m
}
