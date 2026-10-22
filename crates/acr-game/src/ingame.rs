//! The in-game screen: the started game's map, its objects and the running simulation.
//!
//! The world advances at the original's default speed (10 ticks per second, see
//! `acr_sim::ORIGINAL_TICKS_PER_SECOND`) no matter how fast the screen redraws. Units are
//! drawn from the simulation every frame; the player selects own units with a click or a
//! drag rectangle and orders them with a right click.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use acr_formats::M3dMap;
use acr_render::objects_view::ObjectsView;
use acr_render::scene::SceneSprite;
use acr_render::terrain::{Camera, GroundAssets, TerrainData, TerrainRenderer};
use acr_sim::assets::SimAssets;
use acr_sim::world::{AnimKind, EntityId};
use acr_sim::{Command, FixedClock, ORIGINAL_TICKS_PER_SECOND, World};
use winit::keyboard::NamedKey;

use crate::ui::{GameKind, GameSetup};

mod interface;
mod save;
use crate::unit_sounds::{self, SoundEvent};
pub use interface::{GameKey, GameMouse};

/// A drag shorter than this (window pixels) is a click.
const CLICK_SLOP: f32 = 4.0;

pub struct InGame {
    pub setup: Box<GameSetup>,
    map_path: Option<String>,
    ground_files: Vec<Option<Vec<u8>>>,
    assets: Option<GroundAssets>,
    map: Option<M3dMap>,
    terrain: Option<TerrainData>,
    renderer: Option<TerrainRenderer>,
    uploaded: bool,
    objects: ObjectsView,
    sim_assets: SimAssets,
    world: Option<World>,
    clock: FixedClock,
    /// Commands for the next tick.
    commands: Vec<Command>,
    selected: Vec<EntityId>,
    /// Window position where the left button went down, while it is held.
    drag_from: Option<[f32; 2]>,
    /// Files to request from the platform; drained by [`InGame::take_requests`].
    requests: RefCell<Vec<String>>,
    /// Sounds to play this frame; drained by [`InGame::take_sound_events`].
    sound_events: Vec<SoundEvent>,
    /// Each unit's animation and frame when its sounds were last checked.
    last_frames: HashMap<EntityId, (AnimKind, i32)>,
    camera: Camera,
    centred: bool,
    held: HashSet<NamedKey>,
    cursor: Option<[f32; 2]>,
    failed: Option<String>,
    /// The random map generator while it collects its files.
    mapgen: Option<crate::random_map::RandomMapGen>,
    /// The interface: HUD, cursor, control groups, scrolling.
    interface: interface::State,
    /// `FPSTime` of the options: the game speed.
    frame_time: i32,
    /// The mission script and AI DLLs (`acr-mission`).
    mission: crate::mission::MissionLink,
    /// The log of player inputs for savegames, and a loaded game being replayed.
    save: save::SaveState,
}

impl InGame {
    pub fn new(setup: Box<GameSetup>) -> Self {
        let (map_path, mapgen) = match &setup.kind {
            GameKind::Mission(m) => (Some(m.map.clone()), None),
            GameKind::RandomMap(r) => {
                (None, Some(crate::random_map::RandomMapGen::new(crate::random_map::settings(r, &setup))))
            }
        };
        let failed = None;
        let mission = crate::mission::MissionLink::new(&setup);
        Self {
            setup,
            map_path,
            ground_files: vec![None; GroundAssets::FILES.len()],
            assets: None,
            map: None,
            terrain: None,
            renderer: None,
            uploaded: false,
            objects: ObjectsView::new().with_dynamic_units(),
            sim_assets: SimAssets::default(),
            world: None,
            clock: FixedClock::new(ORIGINAL_TICKS_PER_SECOND, 4),
            commands: Vec::new(),
            selected: Vec::new(),
            drag_from: None,
            requests: RefCell::new(Vec::new()),
            sound_events: Vec::new(),
            last_frames: HashMap::new(),
            camera: Camera { x: 0.0, y: 0.0, scale: 1.0 },
            centred: false,
            held: HashSet::new(),
            cursor: None,
            failed,
            mapgen,
            interface: interface::State::default(),
            frame_time: crate::ui::Settings::default().frame_time,
            mission,
            save: Default::default(),
        }
    }

    /// Queues the files needed to show and run the game. `paths` are all game paths.
    pub fn start(&mut self, paths: &[String]) {
        let mut queue = self.requests.borrow_mut();
        queue.extend(GroundAssets::FILES.iter().map(|p| (*p).to_owned()));
        queue.extend(self.map_path.clone());
        queue.extend(self.sim_assets.start(paths));
        queue.extend(self.mission.start(paths));
        drop(queue);
        self.generate_map();
        self.objects.start(&|p| self.requests.borrow_mut().push(p.to_owned()));
    }

    /// Sounds started by the game since the last call.
    pub fn take_sound_events(&mut self) -> Vec<SoundEvent> {
        std::mem::take(&mut self.sound_events)
    }

    /// Files to request from the platform; deliver them with [`InGame::on_loaded`].
    pub fn take_requests(&self) -> Vec<String> {
        std::mem::take(&mut self.requests.borrow_mut())
    }

    /// Why the game cannot start, if it cannot.
    pub fn failure(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// True once the map is on the GPU and can be shown.
    pub fn ready(&self) -> bool {
        self.uploaded && !self.loading_save()
    }

    /// Takes a loaded file if it is one of ours. A file may serve several parts (e.g. the
    /// unit definitions are read by both the objects and the simulation).
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> bool {
        if self.mapgen.as_mut().is_some_and(|g| g.on_loaded(path, result)) {
            self.generate_map();
            return true;
        }
        let load = |p: &str| self.requests.borrow_mut().push(p.to_owned());
        let for_objects = match result {
            Ok(bytes) => self.objects.on_loaded(path, bytes, &load),
            Err(_) if self.objects.wants(path) => {
                self.objects.on_failed(path);
                true
            }
            Err(_) => false,
        };
        let (for_mission, more) = self.mission.on_loaded(path, result);
        self.requests.borrow_mut().extend(more);
        let for_sim = self.sim_assets.wants(path);
        if for_sim {
            let more = self.sim_assets.add_file(path, result.as_ref().ok().map(Vec::as_slice));
            self.requests.borrow_mut().extend(more);
        }
        let ground = GroundAssets::FILES.iter().position(|&f| f == path);
        let is_map = self
            .map_path
            .as_deref()
            .is_some_and(|m| acr_formats::normalize_path(m) == acr_formats::normalize_path(path));
        if ground.is_none() && !is_map {
            return for_objects || for_sim || for_mission;
        }
        let bytes = match result {
            Ok(b) => b,
            Err(e) => {
                self.failed = Some(format!("{path}: {e}"));
                return true;
            }
        };
        if let Some(i) = ground {
            self.ground_files[i] = Some(bytes.clone());
            if self.ground_files.iter().all(Option::is_some) {
                let files: Vec<Vec<u8>> = self.ground_files.iter_mut().map(|f| f.take().unwrap_or_default()).collect();
                match GroundAssets::parse(&files) {
                    Ok(a) => self.assets = Some(a),
                    Err(e) => self.failed = Some(format!("terrain assets: {e}")),
                }
            }
        } else {
            match M3dMap::parse(bytes) {
                Ok(map) => {
                    self.map = Some(map);
                    self.objects.reset();
                }
                Err(e) => self.failed = Some(format!("{path}: {e}")),
            }
        }
        if let (Some(assets), Some(map), None) = (&self.assets, &self.map, &self.terrain) {
            match TerrainData::new(map, assets) {
                Ok(t) => self.terrain = Some(t),
                Err(e) => self.failed = Some(format!("{path}: {e}")),
            }
        }
        true
    }

    /// Runs the random map generator once all files it asked for are there (#74).
    fn generate_map(&mut self) {
        let Some(mapgen) = self.mapgen.as_mut().filter(|g| !g.waiting()) else { return };
        let mut requests = Vec::new();
        let result = mapgen.try_generate(&mut requests);
        self.requests.borrow_mut().extend(requests);
        match result {
            Ok(Some(map)) => {
                self.mapgen = None;
                self.map = Some(map);
                self.objects.reset();
                self.build_terrain();
            }
            Ok(None) => {}
            Err(e) => self.failed = Some(format!("random map: {e}")),
        }
    }

    /// Builds the terrain once the map and the ground assets are there.
    fn build_terrain(&mut self) {
        if let (Some(assets), Some(map), None) = (&self.assets, &self.map, &self.terrain) {
            match TerrainData::new(map, assets) {
                Ok(t) => self.terrain = Some(t),
                Err(e) => self.failed = Some(format!("terrain: {e}")),
            }
        }
    }

    /// Builds the world once the map and the simulation data are complete.
    fn build_world(&mut self) {
        let (None, Some(map), true) = (&self.world, &self.map, self.sim_assets.ready()) else { return };
        let data = self.sim_assets.build();
        let (world, report) = World::from_map(map, &data.shapes, Arc::new(data.catalog), Arc::new(data.formations));
        log::info!("world: {} units, {} map objects, {report:?}", world.units.len(), world.objects.len());
        self.world = Some(world);
    }

    /// Creates the renderer and uploads the map when both GPU and data are there.
    pub fn sync_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        if self.renderer.is_none()
            && let Some(assets) = &self.assets
        {
            self.renderer = Some(TerrainRenderer::new(device, queue, format, assets));
        }
        if let (Some(renderer), Some(terrain), false) = (&mut self.renderer, &self.terrain, self.uploaded) {
            renderer.set_map(device, queue, terrain);
            self.uploaded = true;
        }
        self.objects.sync_gpu(device, queue, format);
    }

    fn local_player(&self) -> u8 {
        self.setup.local_player as u8
    }

    /// Starts on the local player's units, else in the middle of the map.
    fn centre(&mut self, window: [f32; 2]) {
        let (Some(map), Some(terrain)) = (&self.map, &self.terrain) else { return };
        let side = (terrain.width - 1) as f32 * 32.0;
        let own = self.local_player();
        let units: Vec<_> = map.units.iter().filter(|u| u.nation == own).collect();
        let [x, y] = if units.is_empty() {
            [side / 2.0, side / 2.0]
        } else {
            let n = units.len() as f32;
            [
                units.iter().map(|u| (u.x >> 4) as f32).sum::<f32>() / n,
                units.iter().map(|u| (u.y >> 4) as f32).sum::<f32>() / n,
            ]
        };
        self.camera.x = x - window[0] / self.camera.scale / 2.0;
        self.camera.y = y / 2.0 - window[1] / self.camera.scale / 2.0;
        self.centred = true;
    }

    pub fn on_key(&mut self, key: NamedKey, pressed: bool) {
        if pressed {
            self.held.insert(key);
        } else {
            self.held.remove(&key);
        }
    }

    /// World pixel under a window position, ignoring terrain height.
    fn world_at(&self, [x, y]: [f32; 2]) -> (i32, i32) {
        let wx = self.camera.x + x / self.camera.scale;
        let wy = (self.camera.y + y / self.camera.scale) * 2.0;
        (wx as i32, wy as i32)
    }

    /// Window position of a unit's ground point.
    fn screen_of(&self, unit: &acr_sim::Unit) -> [f32; 2] {
        let (x, y) = (unit.pos.x >> 4, unit.pos.y >> 4);
        let h = self.terrain.as_ref().map_or(0, |t| t.height_at(x, y));
        self.camera.project(x, y, h)
    }

    /// The next mission page or question to open over the game.
    pub fn mission_window(&mut self) -> Option<Box<dyn crate::ui::Screen>> {
        self.mission.window()
    }

    /// The mission objectives (F1), if the mission has any.
    pub fn objectives_window(&self) -> Option<Box<dyn crate::ui::Screen>> {
        self.mission.objectives()
    }

    /// The mission has ended and its last message has been shown.
    pub fn mission_over(&self) -> bool {
        self.mission.over()
    }

    /// Escape after the end of the mission: leave now; false while it goes on.
    pub fn mission_escape(&mut self) -> bool {
        self.mission.escape()
    }

    /// Mission messages and dialogs drawn over the map (1024 × 768 canvas).
    pub fn overlay(&self, ctx: &crate::ui::Ctx) -> Vec<crate::ui::draw::Draw> {
        let mut out = Vec::new();
        self.mission.draws(ctx, &mut out);
        self.save_overlay(&mut out);
        out
    }

    /// Left button: press starts a selection, release selects by click or rectangle.
    pub fn on_left(&mut self, pressed: bool) {
        let Some(at) = self.cursor else { return };
        if pressed {
            self.drag_from = Some(at);
            return;
        }
        let Some(from) = self.drag_from.take() else { return };
        let Some(world) = &self.world else { return };
        let own = self.local_player();
        let mine = world.units.iter().filter(|u| u.alive && u.owner == own && !u.building);
        if (from[0] - at[0]).abs() < CLICK_SLOP && (from[1] - at[1]).abs() < CLICK_SLOP {
            self.selected = self.unit_at(at, |u| u.owner == own && !u.building).into_iter().collect();
        } else {
            let (x0, x1) = (from[0].min(at[0]), from[0].max(at[0]));
            let (y0, y1) = (from[1].min(at[1]), from[1].max(at[1]));
            self.selected = mine
                .filter(|u| {
                    let [sx, sy] = self.screen_of(u);
                    (x0..=x1).contains(&sx) && (y0..=y1).contains(&sy)
                })
                .map(|u| u.id)
                .collect();
        }
        log::debug!("selected {} units", self.selected.len());
        if let Some(&first) = self.selected.first() {
            self.unit_sound(first, "SELECT_SOUND");
        }
    }

    /// Right button: attack the enemy under the cursor, else send the selected units to the
    /// point under it.
    pub fn on_right(&mut self) {
        let (Some(at), false) = (self.cursor, self.selected.is_empty()) else { return };
        let (player, units) = (self.local_player(), self.selected.clone());
        let command = match self.unit_at(at, |u| u.owner != player) {
            Some(target) => Command::Attack { player, units, target },
            None => {
                let (x, y) = self.world_at(at);
                Command::Move { player, units, x, y }
            }
        };
        self.commands.push(command);
        self.unit_sound(self.selected[0], "ORDER_SOUND");
    }

    /// Scrolls by held arrow keys and the cursor at the window edge and advances the world;
    /// both depend on time, not on the frame rate.
    pub fn update(&mut self, seconds: f32, window: [f32; 2]) {
        if let (Some(map), Some(terrain)) = (&self.map, &self.terrain) {
            self.objects.update(map, terrain, &|p| self.requests.borrow_mut().push(p.to_owned()));
        }
        self.build_world();
        if !self.centred && self.uploaded {
            self.centre(window);
        }
        let paused = self.interface.paused;
        if let (Some(world), Some(map)) = (&mut self.world, &self.map) {
            let more = self.mission.update(map, world, f64::from(seconds));
            self.requests.borrow_mut().extend(more);
            // Closed mission windows, for savegames.
            for answer in self.mission.take_answers() {
                self.save.log.dialog(world.tick.0, answer);
            }
        }
        // A loaded game replays up to its saved tick first.
        if !self.fast_forward(seconds)
            && let Some(world) = &mut self.world
        {
            let mut ticks = if paused { 0 } else { self.clock.advance((f64::from(seconds) * 1e9) as u64) };
            for t in 0..ticks {
                // The scripts run before each step; a page, a question or the end of the game
                // stops the simulation like the original's modal dialogs.
                let Some(orders) = self.mission.before_step(world) else {
                    ticks = t;
                    break;
                };
                let mut commands = std::mem::take(&mut self.commands);
                self.save.log.command(world.tick.0, &commands);
                commands.extend(orders);
                world.step(&commands);
            }
            self.requests.borrow_mut().extend(self.mission.take_requests());
            if let Some((x, y)) = self.mission.take_camera() {
                let s = self.camera.scale;
                self.camera.x = x as f32 - window[0] / s / 2.0;
                self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
                self.centred = true;
            }
            for _ in 0..ticks {
                self.tick_markers();
            }
            if ticks > 0 {
                self.animation_sounds(window);
            }
        }
        self.scroll_view(seconds, window);
        self.objects.set_dynamic(self.unit_sprites());
    }

    /// The interface sound `keyword` (`SELECT_SOUND`, `ORDER_SOUND`) of a unit's type,
    /// played centred like the original's interface sounds.
    fn unit_sound(&mut self, unit: EntityId, keyword: &str) {
        let Some(world) = &self.world else { return };
        let Some(u) = world.unit(unit) else { return };
        let Some(def) = self.objects.objects().definition(&world.catalog.get(u.kind).id) else { return };
        if let Some(name) = def.all(keyword).next().and_then(|c| c.word(0)) {
            self.sound_events.push(SoundEvent { name: name.to_owned(), pan: 0.0, volume: 1.0 });
        }
    }

    /// `SOUND frame #ANIM name` entries of units in earshot whose animation reached `frame`.
    fn animation_sounds(&mut self, window: [f32; 2]) {
        let Some(world) = &self.world else { return };
        let scale = self.camera.scale;
        let view = [self.camera.x, self.camera.y, window[0] / scale, window[1] / scale];
        let mut events = Vec::new();
        for u in world.units.iter().filter(|u| u.alive && !u.building) {
            let now = (u.anim, u.frame);
            if self.last_frames.insert(u.id, now) == Some(now) {
                continue;
            }
            let Some((volume, pan)) = unit_sounds::placed((u.pos.x >> 4) as f32, (u.pos.y >> 5) as f32, view) else {
                continue;
            };
            let Some(def) = self.objects.objects().definition(&world.catalog.get(u.kind).id) else { continue };
            let anim = anim_name(u.anim);
            for c in def.all("SOUND") {
                let (Some(frame), Some(on), Some(name)) = (c.int(0), c.word(1), c.word(2)) else { continue };
                // Animation names may carry a variant digit (`#MOTION_L0`).
                let base = on.trim_end_matches(|ch: char| ch.is_ascii_digit());
                if frame == u.frame && base.eq_ignore_ascii_case(&anim) {
                    events.push(SoundEvent { name: name.to_owned(), pan, volume });
                }
            }
        }
        self.sound_events.extend(events);
    }

    /// The simulation's units as sprites in their current animation frame.
    fn unit_sprites(&self) -> Vec<SceneSprite> {
        let (Some(world), Some(terrain), true) = (&self.world, &self.terrain, self.objects.ready()) else {
            return Vec::new();
        };
        let objects = self.objects.objects();
        let mut out = Vec::with_capacity(world.units.len());
        for u in world.units.iter().filter(|u| u.alive) {
            let kind = world.catalog.get(u.kind);
            let anim = anim_name(u.anim);
            let step = if u.building { 0 } else { u.frame.max(0) as usize };
            let Some(look) = objects.animation_look(&kind.id, &anim, u.dir, step) else { continue };
            let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
            let h = terrain.height_at(x, y);
            for (frame, (ox, oy), layer, row) in look.parts {
                let sort = (y >> 1) + row;
                out.push(SceneSprite { frame, layer, x, y, dx: ox, dy: oy - h, sort, player: u.owner });
            }
        }
        out
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: [f32; 2],
    ) {
        if let Some(renderer) = &mut self.renderer {
            renderer.render(device, queue, encoder, view, size, &self.camera);
        }
        self.objects.render(device, queue, encoder, view, size, &self.camera);
    }
}

/// The `.MD` animation a simulation animation plays.
fn anim_name(kind: AnimKind) -> String {
    kind.name()
}
