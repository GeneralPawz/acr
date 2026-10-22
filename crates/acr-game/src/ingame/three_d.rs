//! 3D models and the free camera in the game (#163, #164, docs/rendering-3d.md): which
//! objects are drawn as models (`models=` setting), their clip, facing and decay, the 3D
//! pass, and the free camera's keys (`camera=free`). The simulation is not touched: all of
//! it is read from the units the sprites are drawn from.

use std::collections::{HashMap, HashSet};

use acr_render::damage::{Damage, Keep, Limb};
use acr_render::model::{self, ModelData};
use acr_render::model_renderer::{BloodPool, DepthTarget, ModelInstance, ModelRenderer, Wounds};
use acr_render::objects_view::ObjectsView;
use acr_render::scene::{Placement, definition_name};
use acr_render::terrain::{Camera, TerrainData, TerrainRenderer};
use acr_render::view3d::{self, Orbit, PIXELS_PER_METRE, View3d};
use acr_sim::World;
use acr_sim::world::{AnimKind, EntityId, Unit};
use winit::keyboard::{Key, NamedKey};

use super::debris::UnitModel;
use super::piles;
use crate::ui::settings::{CameraMode, Gore, Models, Settings};

/// Where models are looked for: `<game folder>\ACR\Models\<name>.glb` (normalised game path),
/// `<name>` the `.MD` `NAME` in lower case (`EngM18` → `engm18.glb`).
pub const MODEL_DIR: &str = "ACR/MODELS/";
/// How long 3D corpses lie by default (minutes of game time, setting `corpse_minutes`).
const CORPSE_MINUTES: u64 = 5;
/// The death counter above which a sprite corpse turns half transparent (0x506ed5).
const CORPSE_FADE: u16 = 100;
/// Turn and tilt speed of the free camera's keys (radians per second).
const TURN_RATE: f32 = 1.6;
const TILT_RATE: f32 = 0.8;

/// Held gear of a unit model (`docs/assets-humans.md`: separate nodes on sockets).
#[derive(Debug, Clone, Default)]
struct Gear {
    /// The model without the musket, rammer and hat.
    unarmed: Option<usize>,
    /// The musket and hat alone, to lie on the ground.
    dropped: Vec<usize>,
}

/// The `gore.glb` pack (#262): gibs, guts and the stump cap.
const GORE: &str = "GORE";

/// Held gear a crawler or a disarmed man no longer carries: long weapons and the hat.
fn held_gear(node: &str) -> bool {
    let n = node.rsplit('.').next().unwrap_or(node);
    ["musket", "rammer", "pike", "tricorn", "hat", "cap"].contains(&n)
}

/// Gear that lies on the ground once dropped (the rammer stays in the musket).
fn lies_on_ground(node: &str) -> bool {
    held_gear(node) && !node.ends_with("rammer")
}

#[derive(Default)]
struct HeldKeys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

pub struct ThreeD {
    /// Severed pieces, gibs and dropped gear (#262 S5).
    debris: super::debris::Debris,
    /// The parts of `gore.glb` (gibs, guts) by node name.
    gibs: super::debris::GibModels,
    /// Per unit model: the model without held gear (crawlers, the disarmed) and the gear
    /// that then lies on the ground.
    gear: HashMap<usize, Gear>,
    /// Blood trails of the bleeding (render only, #254, #262).
    trail: super::trail::Trails,
    /// The last tick drops were added for (frames redraw a tick several times).
    trail_tick: u64,
    /// The tick of the last update: going back means another game, the debris is dropped.
    debris_tick: u64,
    models: Models,
    gore: Gore,
    /// How long 3D corpses lie (ticks, `corpse_minutes`).
    corpse_ticks: u64,
    free: bool,
    pub orbit: Orbit,
    keys: HeldKeys,
    /// Model files by `.MD` `NAME` (upper case) → game path.
    files: HashMap<String, String>,
    /// Requested model files: game path (normalised) → `NAME`.
    requested: HashMap<String, String>,
    /// Parsed models waiting for the GPU.
    parsed: Vec<(String, ModelData)>,
    /// Uploaded models by `NAME`; `None`: the file could not be used.
    loaded: HashMap<String, Option<usize>>,
    renderer: Option<ModelRenderer>,
    depth: Option<DepthTarget>,
    instances: Vec<ModelInstance>,
    /// Blood pools under this frame's corpses and splatter behind hit units.
    pools: Vec<BloodPool>,
    /// Units drawn as models this frame (their sprites are left out).
    shown: HashSet<EntityId>,
    /// Frames and time since the last performance line in the log.
    frames: u32,
    since: Option<web_time::Instant>,
    /// Time spent stacking corpses (µs) and stackings since the last log line.
    pile_us: u128,
    pile_runs: u32,
    /// `ACR_3D_COMPARE` (development): compare models with their sprites.
    compare: Compare,
    started: web_time::Instant,
}

/// `ACR_3D_COMPARE=overlay`: the sprites of units with a model are drawn half transparent
/// over the model; `toggle`: every 2 seconds sprites and models swap (pause the game to
/// compare one moment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compare {
    None,
    Overlay,
    Toggle,
}

impl Compare {
    fn from_env() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        match std::env::var("ACR_3D_COMPARE").as_deref() {
            Ok("overlay") => return Self::Overlay,
            Ok("toggle") => return Self::Toggle,
            _ => {}
        }
        Self::None
    }
}

impl Default for ThreeD {
    fn default() -> Self {
        Self {
            debris: Default::default(),
            gibs: Default::default(),
            gear: HashMap::new(),
            models: Models::Off,
            gore: Gore::Full,
            corpse_ticks: CORPSE_MINUTES * 60 * acr_sim::ORIGINAL_TICKS_PER_SECOND as u64,
            free: false,
            orbit: Orbit::ORIGINAL,
            keys: HeldKeys::default(),
            files: HashMap::new(),
            requested: HashMap::new(),
            parsed: Vec::new(),
            loaded: HashMap::new(),
            renderer: None,
            depth: None,
            instances: Vec::new(),
            pools: Vec::new(),
            trail: super::trail::Trails::default(),
            trail_tick: u64::MAX,
            debris_tick: 0,
            shown: HashSet::new(),
            frames: 0,
            since: None,
            pile_us: 0,
            pile_runs: 0,
            compare: Compare::from_env(),
            started: web_time::Instant::now(),
        }
    }
}

impl ThreeD {
    pub fn apply(&mut self, settings: &Settings) {
        self.models = settings.models;
        self.gore = settings.gore;
        self.corpse_ticks = u64::from(settings.corpse_minutes) * 60 * acr_sim::ORIGINAL_TICKS_PER_SECOND as u64;
        self.free = settings.camera == CameraMode::Free;
        log::info!("3d: models {:?}, camera {:?}, gore {:?}", settings.models, settings.camera, settings.gore);
    }

    /// Finds the model files among the game's paths.
    pub fn start(&mut self, paths: &[String]) {
        for path in paths {
            let norm = acr_formats::normalize_path(path);
            if let Some(file) = norm.strip_prefix(MODEL_DIR).filter(|f| f.ends_with(".GLB") && !f.contains('/')) {
                self.files.insert(file.trim_end_matches(".GLB").to_owned(), path.clone());
            }
        }
        if !self.files.is_empty() {
            let mut names: Vec<&String> = self.files.keys().collect();
            names.sort();
            log::info!("3d: models found for {names:?}");
        }
    }

    /// The 3D pass replaces the plain sprite pass.
    pub fn active(&self) -> bool {
        self.models != Models::Off || self.free
    }

    /// The free camera's view of this frame, `None` with the fixed camera.
    pub fn free_view(&self, camera: Camera, size: [f32; 2]) -> Option<View3d> {
        self.free.then_some(View3d { camera, size, orbit: Some(self.orbit) })
    }

    pub fn orbit(&self) -> Option<Orbit> {
        self.free.then_some(self.orbit)
    }

    /// Whether `unit` is drawn as a model this frame.
    pub fn shows(&self, unit: EntityId) -> bool {
        self.compare != Compare::Overlay && self.models_phase() && self.shown.contains(&unit)
    }

    /// `ACR_3D_COMPARE=overlay`: the unit's sprite is drawn half transparent over its model.
    pub fn ghost(&self, unit: EntityId) -> bool {
        self.compare == Compare::Overlay && self.shown.contains(&unit)
    }

    /// `ACR_3D_COMPARE=toggle`: whether the models are shown at the moment.
    fn models_phase(&self) -> bool {
        self.compare != Compare::Toggle || (self.started.elapsed().as_secs() / 2).is_multiple_of(2)
    }

    /// Takes a model file this layer asked for.
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> bool {
        let Some(name) = self.requested.remove(&acr_formats::normalize_path(path)) else { return false };
        match result.as_ref().map_err(String::clone).and_then(|b| ModelData::from_glb(b)) {
            Ok(model) => {
                log::info!(
                    "3d: {path}: {} triangles, radius {:.1} m, {} clips {:?}",
                    model.triangle_count(),
                    model.radius / PIXELS_PER_METRE,
                    model.clips.len(),
                    model.clips.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()
                );
                self.parsed.push((name, model));
            }
            Err(e) => {
                log::warn!("3d: {path}: {e}");
                self.loaded.insert(name, None);
            }
        }
        true
    }

    pub fn sync_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        if !self.active() {
            return;
        }
        let renderer = self.renderer.get_or_insert_with(|| ModelRenderer::new(device, queue, format));
        self.depth.get_or_insert_with(|| DepthTarget::new(device, format));
        for (name, model) in self.parsed.drain(..) {
            if name == GORE {
                // The pack is split into its nodes; the torn cap replaces the procedural one
                // (one cap for all cuts: `stump_cap_clean` is not used yet).
                for node in model.mesh_nodes() {
                    let Some(part) = model.node_part(node) else { continue };
                    if node == "stump_cap" {
                        renderer.set_cap(device, queue, part);
                    } else {
                        self.gibs.insert(node.to_owned(), renderer.add_model(device, queue, part));
                    }
                }
                self.loaded.insert(name, Some(usize::MAX));
                continue;
            }
            let mut gear = Gear::default();
            if model.rig.is_some() {
                let unarmed = model.without_nodes(held_gear);
                if unarmed.primitives.len() < model.primitives.len() {
                    for node in model.mesh_nodes().into_iter().filter(|n| lies_on_ground(n)) {
                        if let Some(part) = model.node_part(node) {
                            gear.dropped.push(renderer.add_model(device, queue, super::debris::lying_flat(part)));
                        }
                    }
                    gear.unarmed = Some(renderer.add_model(device, queue, unarmed));
                }
            }
            let index = renderer.add_model(device, queue, model);
            self.gear.insert(index, gear);
            self.loaded.insert(name, Some(index));
        }
    }

    /// A key while the game runs; true when the free camera took it: `,` and `.` turn,
    /// Page Up / Page Down tilt, End returns to the original angle.
    pub fn key(&mut self, key: &Key, pressed: bool) -> bool {
        if !self.free {
            return false;
        }
        match key {
            Key::Character(c) if c.as_str() == "," => self.keys.left = pressed,
            Key::Character(c) if c.as_str() == "." => self.keys.right = pressed,
            Key::Named(NamedKey::PageUp) => self.keys.up = pressed,
            Key::Named(NamedKey::PageDown) => self.keys.down = pressed,
            Key::Named(NamedKey::End) => {
                if pressed {
                    self.orbit = Orbit::ORIGINAL;
                }
            }
            _ => return false,
        }
        true
    }

    /// Turns and tilts the free camera by the held keys.
    pub fn tick(&mut self, seconds: f32) {
        if !self.free {
            return;
        }
        let dt = seconds.min(0.25);
        let k = &self.keys;
        let turn = f32::from(u8::from(k.right)) - f32::from(u8::from(k.left));
        let tilt = f32::from(u8::from(k.up)) - f32::from(u8::from(k.down));
        if turn != 0.0 || tilt != 0.0 {
            self.orbit =
                Orbit { yaw: self.orbit.yaw + turn * TURN_RATE * dt, pitch: self.orbit.pitch + tilt * TILT_RATE * dt }
                    .clamped();
        }
    }

    pub fn release_keys(&mut self) {
        self.keys = HeldKeys::default();
    }

    /// Collects this frame's model instances from the simulation; requests model files of
    /// types that appear.
    pub fn update(&mut self, world: &World, terrain: &TerrainData, objects: &ObjectsView, request: &dyn Fn(&str)) {
        self.instances.clear();
        self.pools.clear();
        self.shown.clear();
        if self.models == Models::Off {
            return;
        }
        let defs = objects.objects();
        let mut names: HashMap<acr_sim::TypeId, String> = HashMap::new();
        // Corpses drawn as models: instance index, sunk ground point, yaw, footprint.
        let mut corpses: Vec<(usize, [f32; 3], f32)> = Vec::new();
        let mut footprints: Vec<piles::Footprint> = Vec::new();
        let mut targets: Vec<super::splatter::Target> = Vec::new();
        let mut pieces: Vec<(usize, Damage)> = Vec::new();
        // Guts beside open bellies, drawn after the units (the indices above are per unit).
        let mut extra: Vec<ModelInstance> = Vec::new();
        // Models of the units drawn, for the pieces their hits throw.
        let mut unit_models: HashMap<u32, (UnitModel, usize)> = HashMap::new();
        let now = world.tick.0;
        if self.debris_tick > now {
            // A loaded game or a restart: the debris belongs to another battle.
            self.debris.clear();
        }
        self.debris_tick = now;
        self.request_gore(request);
        // Corpses the simulation removed (after about a minute, as the original) stay in 3D
        // until their own lifetime ends (`corpse_minutes`), so battlefields fill and piles
        // grow; their units keep position, facing and lying pose.
        let lingering = |u: &Unit| {
            self.gore != Gore::Off
                && !u.alive
                && !u.building
                && u.death >= 1000
                && world.deaths.get(&u.id.0).is_some_and(|r| r.tick > 0 && now - r.tick < self.corpse_ticks)
        };
        for u in world.units.iter().filter(|u| (u.alive || lingering(u)) && u.inside_of.is_none()) {
            if u.building && (self.models != Models::All || !finished(world, u)) {
                continue;
            }
            let name = names.entry(u.kind).or_insert_with(|| definition_name(&world.catalog.get(u.kind).id));
            let Some(path) = self.files.get(name.as_str()) else { continue };
            let index = match self.loaded.get(name.as_str()) {
                Some(Some(i)) => *i,
                Some(None) => continue,
                None => {
                    let key = acr_formats::normalize_path(path);
                    if !self.requested.contains_key(&key) && !self.parsed.iter().any(|(n, _)| n == name) {
                        self.requested.insert(key, name.clone());
                        request(path);
                    }
                    continue;
                }
            };
            let Some(model) = self.renderer.as_ref().and_then(|r| r.model(index)) else { continue };
            let removed_at = match defs.definition(&world.catalog.get(u.kind).id) {
                Some(d) if d.animation("#DEATHLIE2").is_some() => 2000,
                _ => 1200,
            };
            let record = world.deaths.get(&u.id.0).copied().unwrap_or_default();
            // 3D corpses age on their own clock: two counter steps per tick from the death
            // (as a lying corpse counts), removed after `corpse_minutes`.
            let (death, removed_at) = if u.death > 0 && !u.building && self.gore != Gore::Off && record.tick > 0 {
                let age = now.saturating_sub(record.tick);
                ((2 * age).min(u64::from(u16::MAX)) as u16, (2 * self.corpse_ticks).min(u64::from(u16::MAX)) as u16)
            } else {
                (u.death, removed_at)
            };
            let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
            let ground = view3d::ground_point(x, y, terrain.height_at(x, y));
            let body = super::gore::Body { death, ..u.into() };
            let (wounds, pool) = super::gore::corpse(body, record, self.gore, removed_at, ground);
            self.pools.extend(pool);
            let (mut inst, at) = instance(u, death, index, model, terrain, self.gore, removed_at, wounds);
            let living = u.alive && u.death == 0;
            // What is left of the body (#262): cuts, fractures, a torn trunk, an open belly.
            let damage = match (self.gore, model.rig.is_some(), living) {
                (Gore::Off, ..) | (_, false, _) => Some(Damage::default()),
                (_, true, true) => super::damage_view::damage(&u.body, None),
                (_, true, false) => super::damage_view::damage(&record.body, record.torn),
            };
            let Some(damage) = damage else {
                // Pulverized: nothing whole to draw, only gibs and mist (and no sprite).
                self.shown.insert(u.id);
                continue;
            };
            inst.damage = damage;
            if model.rig.is_some() {
                unit_models.insert(u.id.0, (UnitModel { model: index, player: u.owner }, index));
            }
            // Lost legs (#254, #262): he lies face down and drags himself along, head first.
            if living && u.body.crawling() {
                match model.clip(super::damage_view::crawl_clip(&u.body)) {
                    Some(clip) => {
                        inst.clip = Some(clip);
                        inst.time = now as f32 / acr_sim::ORIGINAL_TICKS_PER_SECOND as f32;
                    }
                    None => {
                        inst.clip = model.clip_for_animation("DEATHLIE1");
                        inst.time = 0.0;
                        inst.morphs[1] = 1.0;
                    }
                }
            }
            // A crawler or a man who can no longer hold his musket lets it fall with his hat.
            if living
                && (u.body.crawling() || u.body.dropped)
                && let Some(gear) = self.gear.get(&index)
                && let Some(unarmed) = gear.unarmed
            {
                inst.model = unarmed;
                self.debris.drop_gear(u.id.0, &gear.dropped, u.owner, at, now);
            }
            if damage.belly_open && self.gore == Gore::Full {
                let yaw = view3d::facing_yaw(u.dir);
                super::debris::guts(u.id.0, at, yaw, inst.decay, u.owner, &self.gibs, &mut extra);
            }
            // The bleeding leave drops of blood as they go, crawlers a smear.
            if u.alive
                && u.death == 0
                && u.body.bleeding() > 0
                && self.gore != Gore::Off
                && now.is_multiple_of(super::trail::EVERY)
                && now != self.trail_tick
            {
                self.trail.sample(u.id.0, at, u.body.crawling(), now);
            }
            if u.death > 0 && !u.building {
                corpses.push((self.instances.len(), at, view3d::facing_yaw(u.dir)));
                footprints.push(footprint(u, at, &inst, model.radius));
            }
            targets.push(super::splatter::Target {
                unit: u.id.0,
                pos: (at[0], at[1]),
                yaw: if u.building { 0.0 } else { view3d::facing_yaw(u.dir) },
                building: u.building,
                radius: model.radius,
            });
            // `ACR_DEBUG_DAMAGE` overrides the game's damage (a debug aid).
            if let Some(damage) = model.rig.as_ref().and_then(|_| debug_damage(u.id.0)) {
                inst.damage = damage;
                pieces.push((self.instances.len(), damage));
            }
            self.instances.push(inst);
            self.shown.insert(u.id);
        }
        // Blood sprayed onto troops and buildings behind hit units (#220).
        for (i, sprayed) in super::splatter::sprayed_targets(&world.hits, world.tick.0, self.gore, &targets) {
            self.instances[i].sprayed = sprayed;
        }
        // Blood trails of the bleeding (#254, #262): drops and smears that dry and fade.
        if now.is_multiple_of(super::trail::EVERY) {
            self.trail_tick = now;
        }
        self.trail.expire(now);
        self.trail.pools(now, &mut self.pools);
        // Corpses that overlap lie on each other (#221).
        let start = web_time::Instant::now();
        let rests = piles::stack(&footprints);
        self.pile_us += start.elapsed().as_micros();
        self.pile_runs += 1;
        if self.pile_runs == 500 {
            log::info!(
                "3d: {} corpses piled in {} µs; debris {}",
                footprints.len(),
                self.pile_us / 500,
                self.debris.len()
            );
            self.pile_us = 0;
            self.pile_runs = 0;
        }
        for ((i, at, yaw), rest) in corpses.into_iter().zip(rests) {
            if rest != piles::Rest::default() {
                let ground = [at[0], at[1], at[2] + rest.lift];
                self.instances[i].transform = view3d::model_matrix_tilted(ground, yaw, 1.0, rest.pitch, rest.roll);
            }
        }
        // Appended last: the indices above refer to one instance per unit.
        for (i, damage) in pieces {
            let body = self.instances[i];
            self.instances.extend(debug_pieces(&body, damage));
        }
        self.instances.extend(extra);
        let ground = |x: f32, y: f32| {
            let (x, y) = (x as i32, y as i32);
            view3d::ground_point(x, y, terrain.height_at(x, y))
        };
        // Severed limbs and halves thrown by the blow, gibs (#262 S5).
        let renderer = self.renderer.as_ref();
        let unit = |v: u32| {
            let &(m, index) = unit_models.get(&v)?;
            Some((m, renderer?.model(index)?.rig.as_ref()?))
        };
        let death = |v: u32| world.deaths.get(&v).copied();
        self.debris.take_hits(world.hits.iter(), now, self.gore, &unit, &death, &self.gibs, &ground);
        self.debris.expire(now, self.corpse_ticks);
        self.debris.draw(now, self.corpse_ticks, &ground, &mut self.instances, &mut self.pools);
        for hit in &world.hits {
            super::splatter::splatter(hit, world.tick.0, self.gore, &ground, &mut self.pools);
            super::debris::mist(hit, world.tick.0, self.gore, &ground, &mut self.pools);
        }
        // Earth churned up where rounds struck the ground (#226).
        for impact in &world.impacts {
            super::splatter::impact(impact, world.tick.0, &ground, &mut self.pools);
        }
        // Every decal lies on the slope under it rather than flat at one height (#226).
        super::splatter::lay_on_ground(&mut self.pools, &ground);
    }

    /// Asks for `gore.glb` (gibs, guts, the stump cap) once gore is shown.
    fn request_gore(&mut self, request: &dyn Fn(&str)) {
        if self.gore == Gore::Off || self.loaded.contains_key(GORE) || self.parsed.iter().any(|(n, _)| n == GORE) {
            return;
        }
        let Some(path) = self.files.get(GORE) else { return };
        let key = acr_formats::normalize_path(path);
        if let std::collections::hash_map::Entry::Vacant(e) = self.requested.entry(key) {
            e.insert(GORE.to_owned());
            request(path);
        }
    }

    /// Draws the frame's 3D pass over the terrain already in `view`: the terrain's depth,
    /// the models, then the sprites depth-tested against the models.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: [f32; 2],
        camera: &Camera,
        ground: Option<&TerrainRenderer>,
        terrain: Option<&TerrainData>,
        objects: &mut ObjectsView,
    ) {
        let models_phase = self.models_phase();
        let (Some(renderer), Some(depth), Some(terrain)) = (&mut self.renderer, &mut self.depth, terrain) else {
            objects.render(device, queue, encoder, view, size, camera);
            return;
        };
        let view3d = View3d { camera: *camera, size, orbit: self.free.then_some(self.orbit) };
        let shown = if models_phase { &self.instances[..] } else { &[] };
        renderer.prepare(device, queue, &view3d, shown, self.gore == Gore::Full);
        renderer.prepare_pools(device, queue, if models_phase { &self.pools[..] } else { &[] });
        let sprites = objects.prepare(
            device,
            queue,
            size,
            camera,
            Some(Placement { view: &view3d, terrain, on_top: self.compare == Compare::Overlay }),
        );
        let mut pass = depth.begin(device, encoder, view, size);
        if let Some(ground) = ground {
            ground.render_depth(&mut pass);
        }
        renderer.render(&mut pass);
        depth.reset_ground(&mut pass);
        if sprites {
            objects.draw(&mut pass, true);
        }
        drop(pass);
        self.frames += 1;
        let now = web_time::Instant::now();
        let since = *self.since.get_or_insert(now);
        let elapsed = (now - since).as_secs_f32();
        if elapsed >= 5.0 {
            log::info!(
                "3d: {:.1} fps, {} models drawn ({} poses) of {}, camera {:?}",
                self.frames as f32 / elapsed,
                renderer.drawn,
                renderer.pose_count,
                self.instances.len(),
                view3d.orbit
            );
            self.frames = 0;
            self.since = Some(now);
        }
    }
}

/// `ACR_DEBUG_DAMAGE` (native, `docs/rendering-3d.md`): forces damage on the 3D humans to
/// show the renderer without the game logic. A spec of [`Damage::parse`] for all, or `demo`
/// for a preset per unit (by id): cut arm, cut thigh, fractures, torn in two, headless with
/// the belly open, hand and foot off.
fn debug_damage(unit: u32) -> Option<Damage> {
    static SPEC: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let spec = SPEC.get_or_init(|| std::env::var("ACR_DEBUG_DAMAGE").ok()).as_deref()?;
    const DEMO: [&str; 6] = [
        "cut:arm_l:160",
        "cut:leg_r:70",
        "frac:leg_l:170:55,frac:arm_r:150:-70",
        "trunk:110",
        "head,belly",
        "cut:arm_r:215,cut:leg_l:228",
    ];
    let spec = if spec == "demo" { DEMO[unit as usize % DEMO.len()] } else { spec };
    Damage::parse(spec).ok()
}

/// The severed pieces of a body under `ACR_DEBUG_DAMAGE`, laid beside it (game placement is
/// #262 S5).
fn debug_pieces(body: &ModelInstance, damage: Damage) -> Vec<ModelInstance> {
    let beside = |x: f32, z: f32| {
        let mut m = acr_render::math::IDENTITY;
        m[3] = [x, 0.0, z, 1.0];
        acr_render::math::mul(&body.transform, &m)
    };
    let mut out = Vec::new();
    if !matches!(damage.keep, Keep::Body) {
        return out;
    }
    for limb in Limb::ALL {
        if let Some(piece) = damage.piece(limb) {
            let x = if matches!(limb, Limb::ArmL | Limb::LegL) { 0.7 } else { -0.7 };
            out.push(ModelInstance { damage: piece, transform: beside(x, 0.5), ..*body });
        }
    }
    if damage.trunk_cut.is_some() {
        out.push(ModelInstance {
            damage: Damage { keep: Keep::Upper, ..damage },
            transform: beside(0.9, 0.0),
            ..*body
        });
    }
    out
}

/// A building that stands finished (not under construction, not falling down).
fn finished(world: &World, u: &Unit) -> bool {
    u.stage >= world.max_stage(u) && !matches!(u.anim, AnimKind::BuildLo(_) | AnimKind::Death | AnimKind::DeathLie(_))
}

/// The model instance of a unit: the clip of its sprite animation at its sprite frame, its
/// facing, and as a corpse its decay (or the original's fade with `gore=off`); and its
/// ground point (sunk with the decay).
#[allow(clippy::too_many_arguments)]
fn instance(
    u: &Unit,
    death: u16,
    model: usize,
    data: &ModelData,
    terrain: &TerrainData,
    gore: Gore,
    removed_at: u16,
    wounds: Wounds,
) -> (ModelInstance, [f32; 3]) {
    let anim = u.anim.name();
    let clip = data.clip_for_animation(&anim);
    let time = if u.building { 0.0 } else { u.frame.max(0) as f32 / data.fps };
    let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
    let lying = matches!(u.anim, AnimKind::DeathLie(_));
    let (decay, transparent) = match gore {
        Gore::Off => (0.0, u.death > CORPSE_FADE),
        Gore::Full | Gore::Reduced => (model::corpse_decay(death, removed_at), false),
    };
    let (morphs, sink) = model::decay_pose(decay, lying);
    let mut ground = view3d::ground_point(x, y, terrain.height_at(x, y));
    ground[2] -= sink * PIXELS_PER_METRE;
    // Buildings have one view, facing south.
    let yaw = if u.building { 0.0 } else { view3d::facing_yaw(u.dir) };
    let instance = ModelInstance {
        model,
        clip,
        time,
        transform: view3d::model_matrix(ground, yaw, 1.0),
        decay,
        morphs,
        transparent,
        player: u.owner,
        wounds,
        sprayed: Default::default(),
        damage: Default::default(),
    };
    (instance, ground)
}

/// A human's body lying on the ground (metres): length, width, thickness, and how far its
/// centre lies ahead of the ground point (towards the head; the chest is 0.55 m ahead,
/// `gore.rs`).
const BODY: [f32; 4] = [1.7, 0.5, 0.25, 0.1];
/// The rest-pose radius of a musketeer with his musket (`engm18.glb`, world pixels). Larger
/// models (horses) get a larger footprint, up to 1.8 times: a mounted model's radius grows
/// less than its lying body (2.6 m for `engd18.glb`, a horse lies ~2.4 m long).
const HUMAN_RADIUS: f32 = 2.2 * PIXELS_PER_METRE;

/// The footprint of a corpse for stacking (#221): thinner as it deflates and sinks.
fn footprint(u: &Unit, ground: [f32; 3], inst: &ModelInstance, radius: f32) -> piles::Footprint {
    let size = (1.0 + 2.5 * (radius / HUMAN_RADIUS - 1.0)).clamp(1.0, 1.8) * PIXELS_PER_METRE;
    let (s, c) = view3d::facing_yaw(u.dir).sin_cos();
    let [length, width, thick, ahead] = BODY.map(|v| v * size);
    // Deflating takes about two thirds of the body; the sink already lowered `ground`.
    let thickness = thick * (1.0 - 0.65 * inst.morphs[0]);
    piles::Footprint {
        centre: [ground[0] + s * ahead, ground[1] + c * ahead, ground[2]],
        along: [s, c],
        half_length: length / 2.0,
        half_width: width / 2.0,
        thickness,
        // Died earlier (larger death counter) lies lower.
        order: (u32::from(u16::MAX - u.death), u.id.0),
    }
}

impl super::InGame {
    /// With the free camera turned, scrolling (arrow keys, the window edge) moves along the
    /// screen instead of the world axes the game scrolls in.
    pub(super) fn turn_scrolling(&mut self, before: Camera, window: [f32; 2]) {
        let Some(view) = self.three_d.free_view(before, window) else { return };
        if self.camera.scale != before.scale {
            return;
        }
        let s = before.scale;
        let delta = [(self.camera.x - before.x) * s, (self.camera.y - before.y) * s];
        if delta != [0.0, 0.0] {
            self.camera = view.pan(delta);
        }
    }

    /// Turns the free camera and collects the models of this frame.
    pub(super) fn update_models(&mut self, seconds: f32) {
        self.three_d.tick(seconds);
        let (Some(world), Some(terrain), true) = (&self.world, &self.terrain, self.objects.ready()) else { return };
        let requests = &self.requests;
        self.three_d.update(world, terrain, &self.objects, &|p| requests.borrow_mut().push(p.to_owned()));
    }
}
