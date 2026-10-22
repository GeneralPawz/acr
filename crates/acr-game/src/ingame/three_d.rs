//! 3D models and the free camera in the game (#163, #164, docs/rendering-3d.md): which
//! objects are drawn as models (`models=` setting), their clip, facing and decay, the 3D
//! pass, and the free camera's keys (`camera=free`). The simulation is not touched: all of
//! it is read from the units the sprites are drawn from.

use std::collections::{HashMap, HashSet};

use acr_render::model::{self, ModelData};
use acr_render::model_renderer::{DepthTarget, ModelInstance, ModelRenderer};
use acr_render::objects_view::ObjectsView;
use acr_render::scene::{Placement, definition_name};
use acr_render::terrain::{Camera, TerrainData, TerrainRenderer};
use acr_render::view3d::{self, Orbit, PIXELS_PER_METRE, View3d};
use acr_sim::World;
use acr_sim::world::{AnimKind, EntityId, Unit};
use winit::keyboard::{Key, NamedKey};

use crate::ui::settings::{CameraMode, Gore, Models, Settings};

/// Where models are looked for: `<game folder>\ACR\Models\<name>.glb` (normalised game path),
/// `<name>` the `.MD` `NAME` in lower case (`EngM18` → `engm18.glb`).
pub const MODEL_DIR: &str = "ACR/MODELS/";
/// The death counter above which a sprite corpse turns half transparent (0x506ed5).
const CORPSE_FADE: u16 = 100;
/// Turn and tilt speed of the free camera's keys (radians per second).
const TURN_RATE: f32 = 1.6;
const TILT_RATE: f32 = 0.8;

#[derive(Default)]
struct HeldKeys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

pub struct ThreeD {
    models: Models,
    gore: Gore,
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
    /// Units drawn as models this frame (their sprites are left out).
    shown: HashSet<EntityId>,
    /// Frames and time since the last performance line in the log.
    frames: u32,
    since: Option<web_time::Instant>,
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
            models: Models::Off,
            gore: Gore::Full,
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
            shown: HashSet::new(),
            frames: 0,
            since: None,
            compare: Compare::from_env(),
            started: web_time::Instant::now(),
        }
    }
}

impl ThreeD {
    pub fn apply(&mut self, settings: &Settings) {
        self.models = settings.models;
        self.gore = settings.gore;
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
                    "3d: {path}: {} triangles, {} clips {:?}",
                    model.triangle_count(),
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
            let index = renderer.add_model(device, queue, model);
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
        self.shown.clear();
        if self.models == Models::Off {
            return;
        }
        let defs = objects.objects();
        let mut names: HashMap<acr_sim::TypeId, String> = HashMap::new();
        for u in world.units.iter().filter(|u| u.alive && u.inside_of.is_none()) {
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
            self.instances.push(instance(u, index, model, terrain, self.gore, removed_at));
            self.shown.insert(u.id);
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

/// A building that stands finished (not under construction, not falling down).
fn finished(world: &World, u: &Unit) -> bool {
    u.stage >= world.max_stage(u) && !matches!(u.anim, AnimKind::BuildLo(_) | AnimKind::Death | AnimKind::DeathLie(_))
}

/// The model instance of a unit: the clip of its sprite animation at its sprite frame, its
/// facing, and as a corpse its decay (or the original's fade with `gore=off`).
fn instance(
    u: &Unit,
    model: usize,
    data: &ModelData,
    terrain: &TerrainData,
    gore: Gore,
    removed_at: u16,
) -> ModelInstance {
    let anim = u.anim.name();
    let clip = data.clip_for_animation(&anim);
    let time = if u.building { 0.0 } else { u.frame.max(0) as f32 / data.fps };
    let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
    let lying = matches!(u.anim, AnimKind::DeathLie(_));
    let (decay, transparent) = match gore {
        Gore::Off => (0.0, u.death > CORPSE_FADE),
        Gore::Full | Gore::Reduced => (model::corpse_decay(u.death, removed_at), false),
    };
    let (morphs, sink) = model::decay_pose(decay, lying);
    let mut ground = view3d::ground_point(x, y, terrain.height_at(x, y));
    ground[2] -= sink * PIXELS_PER_METRE;
    // Buildings have one view, facing south.
    let yaw = if u.building { 0.0 } else { view3d::facing_yaw(u.dir) };
    ModelInstance {
        model,
        clip,
        time,
        transform: view3d::model_matrix(ground, yaw, 1.0),
        decay,
        morphs,
        transparent,
        player: u.owner,
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
