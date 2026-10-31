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
use acr_render::fog::{FogRenderer, FogShades};
use acr_render::fog_world::WorldFogRenderer;
use acr_render::objects_view::ObjectsView;
use acr_render::scene::SceneSprite;
use acr_render::terrain::{Camera, GroundAssets, TerrainData, TerrainRenderer};
use acr_sim::assets::SimAssets;
use acr_sim::world::{AnimKind, EntityId};
use acr_sim::{Command, FixedClock, ORIGINAL_TICKS_PER_SECOND, World};
use winit::keyboard::NamedKey;

use crate::ui::{GameKind, GameSetup};

mod body_arrows;
mod build_view;
mod building_hits;
mod cloth_view;
mod controls;
mod crew_view;
mod damage_view;
mod debris;
mod dip;
mod editor;
pub(crate) use editor::input::{self as editor_input, Input as EditorInput};
mod effects;
mod fire_view;
mod gait_view;
mod gear_view;
mod gore;
mod ground_marks;
mod gun_marks;
mod interface;
mod kept_marks;
mod living_wounds;
mod map_models;
mod maul_view;
mod missile_view;
mod mortar_ui;
mod panels;
mod particle_view;
mod piles;
pub mod probe;
mod ragdoll_view;
mod ruin_view;
mod save;
mod seep;
mod shouts;
mod splatter;
mod strike_view;
mod tears;
pub(crate) mod three_d;
mod trail;
mod trample;
mod water_view;
use crate::animal_sounds::Cry;
use crate::unit_sounds::{self, SoundEvent};
pub use interface::{GameKey, GameMouse};

/// A drag shorter than this (window pixels) is a click.
pub(super) const CLICK_SLOP: f32 = 4.0;

pub struct InGame {
    pub setup: Box<GameSetup>,
    map_path: Option<String>,
    ground_files: Vec<Option<Vec<u8>>>,
    assets: Option<GroundAssets>,
    map: Option<M3dMap>,
    terrain: Option<TerrainData>,
    /// Trampled soil (render only, #223).
    trample: Option<trample::Trample>,
    /// Marks in the detailed ground's state (docs/ground.md, phase 2; render only): in
    /// detailed mode they replace the trample darkening and the dirt decals.
    ground_marks: Option<ground_marks::GroundMarks>,
    /// The trample darkening is cleared while the ground marks are on.
    trample_cleared: bool,
    renderer: Option<TerrainRenderer>,
    /// The fog of war over the map view.
    fog_renderer: Option<FogRenderer>,
    /// The fog of war on the world under the free camera (#309).
    world_fog: Option<WorldFogRenderer>,
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
    /// The tick of the last hit whose sound was chosen ([`crate::hit_sounds`]).
    hit_cursor: u64,
    /// The tick of the last ground impact whose sound was chosen.
    impact_cursor: u64,
    /// Agony of the dying, fearful shouts and the din of battle ([`crate::agony`]).
    agony: crate::agony::Agony,
    /// Units that started a flight leg in panic since the last frame.
    fleeing: Vec<crate::agony::Fleeing>,
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
    /// Object types whose sprites have been requested.
    known_types: HashSet<acr_sim::TypeId>,
    /// The mission script and AI DLLs (`acr-mission`).
    mission: crate::mission::MissionLink,
    /// The log of player inputs for savegames, and a loaded game being replayed.
    save: save::SaveState,
    /// DipServer's dialogs in the side panel.
    diplomacy: crate::diplomacy::DipPanel,
    /// Score and losses for the game info panel.
    stats: panels::Stats,
    /// `ACR_DIP_PAGE` has been looked at.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    dip_page_shown: bool,
    /// Game frames run since the ambient sounds last looked (`take_ambient_ticks`).
    ambient_ticks: u32,
    /// Fires of damaged buildings.
    effects: effects::Effects,
    /// Panic shouts over fleeing units (laid out when the interface draws).
    shouts: RefCell<shouts::Shouts>,
    /// 3D models and the free camera (#163, #164).
    three_d: three_d::ThreeD,
    /// The interface is hidden (the driver's `hud` command): only the world is drawn.
    pub hud_hidden: bool,
    /// The driver's studio holds the simulation (#299): no ticks run, models keep drawing.
    pub studio_hold: bool,
    /// Ctrl + Space, R asked for a performance snapshot (`crate::perf`), not taken yet.
    pub perf_request: bool,
    /// The test editor's panel and state (editor games only).
    editor: Option<editor::Editor>,
    /// The editor's peace has been applied to the new world.
    editor_started: bool,
    /// Where the editor's window stood last (`editor_window` in the settings), for the next editor game.
    editor_window: Option<crate::ui::settings::EditorWindow>,
}

impl InGame {
    pub fn new(setup: Box<GameSetup>) -> Self {
        let (map_path, mapgen) = match &setup.kind {
            GameKind::Mission(m) => (Some(m.map.clone()), None),
            GameKind::RandomMap(r) => {
                (None, Some(crate::random_map::RandomMapGen::new(crate::random_map::settings(r, &setup))))
            }
            GameKind::Blank(_) | GameKind::Acr(_) => (None, None),
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
            trample: None,
            ground_marks: None,
            trample_cleared: false,
            renderer: None,
            fog_renderer: None,
            world_fog: None,
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
            hit_cursor: 0,
            impact_cursor: 0,
            agony: crate::agony::Agony::new(crate::agony::Mix::from_env()),
            fleeing: Vec::new(),
            camera: Camera { x: 0.0, y: 0.0, scale: 1.0 },
            centred: false,
            held: HashSet::new(),
            cursor: None,
            failed,
            mapgen,
            interface: interface::State::default(),
            frame_time: crate::ui::Settings::default().frame_time,
            known_types: HashSet::new(),
            mission,
            save: Default::default(),
            diplomacy: crate::diplomacy::DipPanel::default(),
            stats: Default::default(),
            dip_page_shown: false,
            ambient_ticks: 0,
            effects: Default::default(),
            shouts: Default::default(),
            three_d: Default::default(),
            hud_hidden: false,
            perf_request: false,
            studio_hold: false,
            editor: None,
            editor_started: false,
            editor_window: None,
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
        self.three_d.start(paths);
        self.generate_map();
        match &self.setup.kind {
            GameKind::Blank(b) => {
                self.map = Some(crate::blank_map::build(b));
                self.objects.reset();
                self.editor = b
                    .editor
                    .then(|| editor::Editor::with_frame(editor::frame::Frame::from_setting(self.editor_window)));
            }
            GameKind::Acr(m) => {
                self.map = Some(m.map());
                self.objects.reset();
            }
            _ => {}
        }
        self.objects.start(&|p| self.requests.borrow_mut().push(p.to_owned()));
        self.objects.start_effects(&|p| self.requests.borrow_mut().push(p.to_owned()));
    }

    /// The flat colour drawn instead of the ground, if any (the blank field's `white` and
    /// `greenscreen`). Nothing grows on it: the 3D view drops its ground cover then.
    fn backdrop(&self) -> Option<wgpu::Color> {
        match &self.setup.kind {
            GameKind::Blank(b) => crate::blank_map::backdrop(b.ground).map(|[r, g, b]| wgpu::Color { r, g, b, a: 1.0 }),
            _ => None,
        }
    }

    /// The driver's `ground_cover`: forces the ground cover on or off for this game (`None`:
    /// the setting, none under a backdrop).
    pub fn set_ground_cover(&mut self, on: Option<bool>) {
        self.three_d.cover_forced = on;
    }

    /// Turns the decay stains on or off for this game (the driver's `decay_stain`).
    pub fn set_decay_stain(&mut self, on: bool) {
        self.three_d.set_decay_stain(on);
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

    /// The game is paused (the Pause key, 0x1100a78); a finished game is never paused.
    pub fn paused(&self) -> bool {
        self.interface.paused && self.mission.ended.is_none()
    }

    /// Pauses or resumes, as the Pause key does (the touch gestures set it, not toggle it).
    pub fn set_paused(&mut self, paused: bool) {
        if self.mission.ended.is_none() {
            self.interface.paused = paused;
        }
    }

    /// Takes a loaded file if it is one of ours. A file may serve several parts (e.g. the
    /// unit definitions are read by both the objects and the simulation).
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> bool {
        if self.mapgen.as_mut().is_some_and(|g| g.on_loaded(path, result)) {
            self.generate_map();
            return true;
        }
        if self.three_d.on_loaded(path, result) {
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
        let mut data = self.sim_assets.build();
        // Mortars rule: the `ArtMrt` types come after the original ones (#301).
        if self.setup.rules.mortars {
            acr_sim::mortar::append_types(&mut data.catalog);
        }
        let (mut world, report) = World::from_map(map, &data.shapes, Arc::new(data.catalog), Arc::new(data.formations));
        world.rules = self.setup.rules;
        log::info!("world: {} units, {} map objects, {report:?}", world.units.len(), world.objects.len());
        let catalog = &world.catalog;
        self.objects.set_names((0..catalog.len()).map(|i| {
            let t = catalog.get(acr_sim::TypeId(i as u16));
            (t.id.as_str(), t.md_name.as_str())
        }));
        self.world = Some(world);
    }

    /// Whether the detailed ground's marks are drawn (3D mode, `ground=detailed`, textures
    /// loaded).
    fn ground_marks_on(&self) -> bool {
        self.three_d.active()
            && self.three_d.ground_detailed
            && self.renderer.as_ref().is_some_and(TerrainRenderer::has_materials)
    }

    /// Creates the renderer and uploads the map when both GPU and data are there.
    pub fn sync_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        if self.renderer.is_none()
            && let Some(assets) = &self.assets
        {
            self.renderer = Some(TerrainRenderer::new(device, queue, format, assets));
            self.fog_renderer = Some(FogRenderer::new(device, format, &assets.fog_darkness()));
            self.world_fog = Some(WorldFogRenderer::new(device, format, &assets.fog_darkness()));
        }
        if let (Some(renderer), Some(terrain), false) = (&mut self.renderer, &self.terrain, self.uploaded) {
            renderer.set_map(device, queue, terrain);
            self.uploaded = true;
        }
        // An edit of the terrain: the changed vertices go to the GPU.
        if self.uploaded
            && let (Some(renderer), Some(terrain)) = (&mut self.renderer, &mut self.terrain)
            && let Some(rect) = terrain.take_dirty()
        {
            renderer.update_map(queue, terrain, rect);
        }
        let marks_on = self.ground_marks_on();
        self.three_d.ground_marks = marks_on;
        if let (Some(renderer), Some(marks), true) = (&self.renderer, &mut self.ground_marks, marks_on) {
            renderer.upload_ground_state(queue, &mut marks.state);
        }
        if let (Some(renderer), Some(trample)) = (&self.renderer, &mut self.trample) {
            if marks_on {
                // The ground state's compression replaces the per-vertex darkening.
                if !std::mem::replace(&mut self.trample_cleared, true) {
                    renderer.set_trample(queue, &vec![0; trample.len()]);
                }
            } else if let Some(amounts) = trample
                .take_upload()
                .or_else(|| std::mem::take(&mut self.trample_cleared).then(|| trample.force_upload()))
            {
                renderer.set_trample(queue, &amounts);
            }
        }
        self.objects.sync_gpu(device, queue, format);
        self.three_d.sync_gpu(device, queue, format);
    }

    /// Without a GPU (the test driver): the game counts as shown once its terrain is built.
    pub fn sync_headless(&mut self) {
        if self.terrain.is_some() {
            self.uploaded = true;
        }
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

    /// World pixel of the ground under a window position.
    fn world_at(&self, [x, y]: [f32; 2]) -> (i32, i32) {
        if let Some(view) = self.three_d.free_view(self.camera, self.window_size()) {
            return view.ground_at([x, y], |x, y| self.terrain.as_ref().map_or(0, |t| t.height_at(x, y)));
        }
        let wx = (self.camera.x + x / self.camera.scale) as i32;
        let sy = self.camera.y + y / self.camera.scale;
        // The ground there is lifted by its height: a few steps find the point under the
        // cursor (screen y = world y / 2 − height).
        let mut wy = (sy * 2.0) as i32;
        if let Some(t) = &self.terrain {
            for _ in 0..4 {
                wy = ((sy + t.height_at(wx, wy) as f32) * 2.0) as i32;
            }
        }
        (wx, wy)
    }

    /// Window position of a unit's ground point.
    fn screen_of(&self, unit: &acr_sim::Unit) -> [f32; 2] {
        let (x, y) = (unit.pos.x >> 4, unit.pos.y >> 4);
        let h = self.terrain.as_ref().map_or(0, |t| t.height_at(x, y));
        if let Some(view) = self.three_d.free_view(self.camera, self.window_size()) {
            let p = view.project(acr_render::view3d::ground_point(x, y, h));
            return [p[0], p[1]];
        }
        self.camera.project(x, y, h)
    }

    /// The next mission page or question to open over the game.
    pub fn mission_window(&mut self) -> Option<Box<dyn crate::ui::Screen>> {
        self.mission.window()
    }

    /// Game frames run since the last call, for the ambient sounds.
    pub fn take_ambient_ticks(&mut self) -> u32 {
        std::mem::take(&mut self.ambient_ticks)
    }

    /// The weather to hear (`weather_sound`): the weather drawn, off without the 3D pass
    /// (it draws only there).
    /// `window` is the window size in pixels.
    pub fn weather_heard(&self, window: [f32; 2]) -> crate::weather_sound::Heard {
        if !self.three_d.active() {
            return Default::default();
        }
        let s = self.camera.scale;
        // The window centre on the ground (world y is twice the screen's).
        let at = [self.camera.x + window[0] / s * 0.5, (self.camera.y + window[1] / s * 0.5) * 2.0];
        let (intensity, _) = self.three_d.storm();
        crate::weather_sound::Heard {
            weather: self.three_d.weather(),
            clock: self.three_d.wind_clock(),
            intensity,
            wind: self.three_d.wind_at(at),
            near: if self.interface.hud.zoom { 0.0 } else { crate::weather_sound::nearness(s) },
        }
    }

    /// What the ambient sounds need: the view in original pixels and the map's water.
    pub fn ambient_view(&self, ticks: u32, window: [f32; 2]) -> Option<crate::unit_sounds::AmbientView<'_>> {
        let (map, terrain) = (self.map.as_ref()?, self.terrain.as_ref()?);
        let s = self.camera.scale;
        let side = terrain.width as i32 - 1;
        Some(crate::unit_sounds::AmbientView {
            ticks,
            view: [self.camera.x, self.camera.y, window[0] / s, window[1] / s],
            map: [side, side],
            depth: map.water.as_ref().map(|w| &w.depth),
            fog: self.mission.fog(),
        })
    }

    /// The mission objectives (F1), if the mission has any.
    pub fn objectives_window(&self) -> Option<Box<dyn crate::ui::Screen>> {
        self.mission.objectives()
    }

    /// The mission history (`UM_HIST`, 0x562bea), if any page or question was shown.
    pub fn history_window(&self) -> Option<Box<dyn crate::ui::Screen>> {
        self.mission.history()
    }

    /// The mission has ended and its last message has been shown.
    pub fn mission_over(&self) -> bool {
        self.mission.over()
    }

    /// How the mission ended: `Some(true)` won, `Some(false)` lost, `None` not over.
    pub fn mission_result(&self) -> Option<bool> {
        self.mission.ended
    }

    /// Escape after the end of the mission: leave now; false while it goes on.
    pub fn mission_escape(&mut self) -> bool {
        self.mission.escape()
    }

    /// Mission messages and dialogs drawn over the map (the interface's canvas,
    /// [`InGame::screen_canvas`]).
    pub fn overlay(&self, ctx: &crate::ui::Ctx) -> Vec<crate::ui::draw::Draw> {
        let mut out = Vec::new();
        self.mission.draws(ctx, self.interface.hud.screen(), &mut out);
        self.save_overlay(&mut out);
        self.editor_overlay(ctx, &mut out);
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
        self.selected = self.pick(from, at);
        log::debug!("selected {} units", self.selected.len());
        if let Some(&first) = self.selected.first() {
            self.unit_sound(first, "SELECT_SOUND");
        }
    }

    /// The own objects a click or a rectangle picks (0x5179d6): a click takes the object
    /// under the cursor (a unit in front of a building wins), a rectangle every object
    /// whose box meets it, but buildings only if no unit is caught. Nothing hit selects
    /// nothing. A formation member picked brings its formation's flag bearer and soldiers
    /// (0x41f3fb after the selection command 0x4da424, `SelectBrigade` 0x4e1365 takes the
    /// members from index 2 on).
    pub(super) fn pick(&self, from: [f32; 2], at: [f32; 2]) -> Vec<EntityId> {
        let Some(world) = &self.world else { return Vec::new() };
        let mut picked = self.pick_objects(from, at);
        let mut brigades: Vec<u16> = Vec::new();
        for u in picked.iter().filter_map(|&id| world.unit(id)) {
            if let Some(b) = u.brigade.filter(|b| !brigades.contains(b)) {
                brigades.push(b);
            }
        }
        for b in brigades {
            let Some(br) = world.brigades.get(usize::from(b)) else { continue };
            for &m in br.members.iter().skip(2).flatten() {
                if !picked.contains(&m) && interface::fighting_member(world, b, m).is_some() {
                    picked.push(m);
                }
            }
        }
        picked
    }

    fn pick_objects(&self, from: [f32; 2], at: [f32; 2]) -> Vec<EntityId> {
        let Some(world) = &self.world else { return Vec::new() };
        let own = self.local_player();
        if (from[0] - at[0]).abs() < CLICK_SLOP && (from[1] - at[1]).abs() < CLICK_SLOP {
            let unit = self.unit_at(at, |u| u.owner == own && !u.building);
            return unit.or_else(|| self.unit_at(at, |u| u.owner == own && u.building)).into_iter().collect();
        }
        let s = self.camera.scale;
        let to_world = |p: [f32; 2]| [self.camera.x + p[0] / s, self.camera.y + p[1] / s];
        let (a, b) = (to_world(from), to_world(at));
        let (x0, x1) = (a[0].min(b[0]) as i32, a[0].max(b[0]) as i32);
        let (y0, y1) = (a[1].min(b[1]) as i32, a[1].max(b[1]) as i32);
        let caught: Vec<&acr_sim::Unit> = world
            .units
            .iter()
            .filter(|u| interface::selectable(u) && u.inside_of.is_none() && u.owner == own)
            .filter(|u| {
                let (bx, by, w, h) = self.unit_box(u);
                bx <= x1 && bx + w > x0 && by <= y1 && by + h > y0
            })
            .collect();
        let units = caught.iter().any(|u| !u.building);
        caught.into_iter().filter(|u| !units || !u.building).map(|u| u.id).collect()
    }

    /// Scrolls by held arrow keys and the cursor at the window edge and advances the world;
    /// both depend on time, not on the frame rate.
    pub fn update(&mut self, seconds: f32, window: [f32; 2]) {
        if let (Some(map), Some(terrain)) = (&self.map, &self.terrain) {
            self.objects.update(map, terrain, &|p| self.requests.borrow_mut().push(p.to_owned()));
        }
        self.build_world();
        self.editor_stroke(seconds);
        if !self.editor_started
            && let (Some(e), Some(world)) = (&self.editor, &mut self.world)
        {
            e.apply_sides(world);
            self.editor_started = true;
        }
        // The scripts copy their alliance masks to the world every tick: give them the editor's.
        if let Some(e) = &self.editor {
            self.mission.set_alliances(e.masks());
        }
        if !self.centred && self.uploaded {
            self.centre(window);
        }
        // A finished game cannot be paused (0x1100a78).
        let paused = (self.interface.paused || self.studio_hold) && self.mission.ended.is_none();
        if let (Some(world), Some(map)) = (&mut self.world, &self.map) {
            let more = self.mission.update(map, world, f64::from(seconds));
            self.requests.borrow_mut().extend(more);
            // Closed mission windows, for savegames.
            for answer in self.mission.take_answers() {
                self.save.log.dialog(world.tick.0, answer);
            }
        }
        let marks_on = self.ground_marks_on();
        // A loaded game replays up to its saved tick first.
        if !self.fast_forward(seconds)
            && let Some(world) = &mut self.world
        {
            let mut ticks = if paused { 0 } else { self.clock.advance((f64::from(seconds) * 1e9) as u64) };
            let mut weapon_sounds = Vec::new();
            let mut born = Vec::new();
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
                // The editor's part: the battle controller's orders (not logged, a replay makes
                // them again) and the test switch of cannons always loaded.
                if let Some(e) = &mut self.editor {
                    commands.extend(e.before_step(world));
                }
                world.step(&commands);
                let _after = acr_sim::prof::Scope::new("game.after_tick");
                weapon_sounds.append(&mut world.sounds);
                born.append(&mut world.born);
                for &id in &world.shouts {
                    let Some(u) = world.unit(id) else { continue };
                    let kind = &world.catalog.get(u.kind).id;
                    if self.objects.objects().definition(kind).is_some_and(|d| d.has("NIKAKIXMAM")) {
                        continue;
                    }
                    let group = u.brigade.map_or((0xff, u.id.0), |b| (u.owner, u32::from(b)));
                    let pos = ((u.pos.x >> 4) as f32, (u.pos.y >> 5) as f32);
                    self.fleeing.push(crate::agony::Fleeing { tick: world.tick.0, group, pos });
                }
                if let Some(terrain) = &self.terrain {
                    shouts::collect(
                        world,
                        terrain,
                        &self.objects,
                        self.mission.fog(),
                        self.setup.local_player as u8,
                        self.interface.hud.life_bars,
                        &mut self.shouts.borrow_mut(),
                    );
                }
                // The fog follows the world (once per tick, also read by the scripts).
                self.mission.update_fog(world);
                if let Some(terrain) = &self.terrain {
                    self.trample
                        .get_or_insert_with(|| trample::Trample::new(terrain.width, terrain.height))
                        .tick(world);
                }
                if marks_on {
                    self.ground_marks.get_or_insert_with(Default::default).tick(world);
                }
            }
            acr_sim::prof::add("ticks", u64::from(ticks));
            self.requests.borrow_mut().extend(self.mission.take_requests());
            self.update_diplomacy(f64::from(seconds));
            if let Some((x, y)) = self.mission.take_camera() {
                let s = self.camera.scale;
                self.camera.x = x as f32 - window[0] / s / 2.0;
                self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
                self.centred = true;
            }
            for _ in 0..ticks {
                self.tick_markers();
            }
            self.shouts.borrow_mut().advance(ticks);
            if ticks > 0 {
                self.update_stats();
                self.watch_queues(ticks);
                self.select_formed();
                self.prune_selection();
            }
            if ticks > 0 {
                self.animation_sounds(window);
                self.effect_sounds(ticks, &weapon_sounds, window);
                self.born_sounds(&born, window);
                self.hit_sounds(window);
                self.agony_sounds(window);
            }
            self.ambient_ticks += ticks;
        }
        self.glide_zoom(seconds, window);
        let before = self.camera;
        self.scroll_view(seconds, window);
        self.turn_scrolling(before, window);
        self.want_new_types();
        self.update_models(seconds, window);
        self.objects.set_dynamic(self.unit_sprites());
    }

    /// The interface sound `keyword` (`SELECT_SOUND`, `ORDER_SOUND`) of a unit's type,
    /// played centred like the original's interface sounds.
    fn unit_sound(&mut self, unit: EntityId, keyword: &str) {
        let Some(world) = &self.world else { return };
        let Some(u) = world.unit(unit) else { return };
        let Some(def) = self.objects.objects().definition(&world.catalog.get(u.kind).id) else { return };
        if let Some(name) = def.all(keyword).next().and_then(|c| c.word(0)) {
            self.sound_events.push(SoundEvent { name: name.to_owned(), pan: 0.0, volume: 1.0, at: None });
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
            let before = self.last_frames.insert(u.id, now);
            if before == Some(now) {
                continue;
            }
            let (x, y) = ((u.pos.x >> 4) as f32, (u.pos.y >> 5) as f32);
            let Some((volume, pan)) = unit_sounds::placed(x, y, view, self.mission.fog()) else {
                continue;
            };
            let id = &world.catalog.get(u.kind).id;
            let Some(def) = self.objects.objects().definition(id) else { continue };
            // An animal growls as it starts a strike (its `.MD` has no sounds).
            if matches!(u.anim, AnimKind::Attack(_))
                && before.is_none_or(|b| b.0 != u.anim)
                && def.has("NIKAKIXMAM")
                && let Some(v) = crate::animal_sounds::voice(id)
            {
                log::debug!("animal sound {} of {id} (pan {pan:.2}, volume {volume:.2})", v.attack);
                events.push(SoundEvent { name: v.attack.to_owned(), pan, volume, at: None });
            }
            let anim = anim_name(u.anim);
            for c in def.all("SOUND") {
                let (Some(frame), Some(on), Some(name)) = (c.int(0), c.word(1), c.word(2)) else { continue };
                // Animation names may carry a variant digit (`#MOTION_L0`).
                let base = on.trim_end_matches(|ch: char| ch.is_ascii_digit());
                if frame == u.frame && base.eq_ignore_ascii_case(&anim) {
                    // Marching feet and death cries count towards their crowd ([`crate::crowd_sounds`]).
                    events.push(SoundEvent { name: name.to_owned(), pan, volume, at: Some(x) });
                }
            }
        }
        self.sound_events.extend(events);
    }

    /// The weapon sounds of the last ticks, and the fires of damaged buildings in view
    /// (advanced once per tick) with their animation sounds.
    /// `BORN_SOUND` of produced units (0x49fab7, any player) and of the local player's
    /// finished buildings (0x5247c2), at the object like other world sounds: the mill's
    /// creaking wheel, the mines' picks, a soldier's answer.
    fn born_sounds(&mut self, born: &[EntityId], window: [f32; 2]) {
        let Some(world) = &self.world else { return };
        let scale = self.camera.scale;
        let view = [self.camera.x, self.camera.y, window[0] / scale, window[1] / scale];
        let own = self.local_player();
        for &id in born {
            let Some(u) = world.unit(id) else { continue };
            if u.building && u.owner != own {
                continue;
            }
            let Some(def) = self.objects.objects().definition(&world.catalog.get(u.kind).id) else { continue };
            let Some(name) = def.all("BORN_SOUND").next().and_then(|c| c.word(0)) else { continue };
            let (x, y) = ((u.pos.x >> 4) as f32, (u.pos.y >> 5) as f32);
            if let Some((volume, pan)) = unit_sounds::placed(x, y, view, self.mission.fog()) {
                log::trace!("born sound {name} (pan {pan:.2}, volume {volume:.2})");
                self.sound_events.push(SoundEvent { name: name.to_owned(), pan, volume, at: None });
            }
        }
    }

    /// Impact and pain sounds of the hits since the last call.
    fn hit_sounds(&mut self, window: [f32; 2]) {
        let Some(world) = &self.world else { return };
        // A loaded game may start at an earlier tick, with its own hits.
        if self.hit_cursor > world.tick.0 {
            self.hit_cursor = 0;
        }
        if self.impact_cursor > world.tick.0 {
            self.impact_cursor = 0;
        }
        let scale = self.camera.scale;
        let view = [self.camera.x, self.camera.y, window[0] / scale, window[1] / scale];
        let objects = self.objects.objects();
        // Men cry out with men's voices, animals with their own (never the men's).
        let who = |victim: u32| {
            let Some(u) = world.units.get(victim as usize) else { return Cry::Silent };
            let id = &world.catalog.get(u.kind).id;
            Cry::of(id, objects.definition(id).is_some_and(|d| d.has("NIKAKIXMAM")))
        };
        let events = crate::hit_sounds::hit_sounds(&world.hits, who, self.hit_cursor, view, self.mission.fog());
        self.hit_cursor = world.hits.back().map_or(self.hit_cursor, |h| h.tick.max(self.hit_cursor));
        let ground = crate::hit_sounds::ground_sounds(&world.impacts, self.impact_cursor, view, self.mission.fog());
        self.impact_cursor = world.impacts.back().map_or(self.impact_cursor, |i| i.tick.max(self.impact_cursor));
        let events: Vec<_> = events.into_iter().chain(ground).collect();
        for e in &events {
            log::debug!("hit sound {} (pan {:.2}, volume {:.2})", e.name, e.pan, e.volume);
        }
        self.sound_events.extend(events);
    }

    /// The agony of the dying, the fearful shouts of the fleeing and the din of battle.
    fn agony_sounds(&mut self, window: [f32; 2]) {
        let fleeing = std::mem::take(&mut self.fleeing);
        let Some(world) = &self.world else { return };
        let scale = self.camera.scale;
        let view = [self.camera.x, self.camera.y, window[0] / scale, window[1] / scale];
        let objects = self.objects.objects();
        let human = |victim: u32| {
            world.units.get(victim as usize).is_some_and(|u| {
                let id = &world.catalog.get(u.kind).id;
                !u.building && Cry::of(id, objects.definition(id).is_some_and(|d| d.has("NIKAKIXMAM"))) == Cry::Human
            })
        };
        let now = world.tick.0;
        let alive = |victim: u32| world.units.get(victim as usize).is_some_and(|u| u.living());
        // Living men who burn, or whose wounds (hit-locations rule) are not healed (#253).
        let suffering: Vec<_> = world
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                u.living()
                    && !u.building
                    && (u.burning || (u.life < u.max_life && u.body != acr_sim::body::Body::default()))
            })
            .filter(|&(i, _)| human(i as u32))
            .map(|(i, u)| crate::agony::Suffering {
                victim: i as u32,
                pos: ((u.pos.x >> 4) as f32, (u.pos.y >> 5) as f32),
                severity: if u.life < u.max_life { crate::agony::severity(&u.body) } else { 0 },
                burning: u.burning,
            })
            .collect();
        let events = self.agony.update(&world.hits, &fleeing, &suffering, human, alive, now, view, self.mission.fog());
        for e in &events {
            log::debug!("agony sound {} at tick {now} (pan {:.2}, volume {:.2})", e.name, e.pan, e.volume);
        }
        self.sound_events.extend(events);
    }

    fn effect_sounds(&mut self, ticks: u32, shots: &[acr_sim::projectile::WeaponSound], window: [f32; 2]) {
        let Some(world) = &self.world else { return };
        let scale = self.camera.scale;
        let view = [self.camera.x, self.camera.y, window[0] / scale, window[1] / scale];
        let fog = self.mission.fog();
        let mut events = effects::weapon_sounds(world, shots, view, fog);
        // Buildings whose fires may show: the view plus a building's size.
        let rect = [
            view[0] as i32 - 512,
            (view[1] as i32 - 512) * 2,
            (view[0] + view[2]) as i32 + 512,
            (view[1] + view[3]) as i32 * 2 + 2048,
        ];
        for _ in 0..ticks.min(8) {
            for (name, x, y) in self.effects.tick(world, &self.objects, rect) {
                if let Some((volume, pan)) = unit_sounds::placed(x as f32, y as f32, view, fog) {
                    events.push(SoundEvent { name, pan, volume, at: None });
                }
            }
        }
        for e in &events {
            log::trace!("effect sound {} (pan {:.2}, volume {:.2})", e.name, e.pan, e.volume);
        }
        self.sound_events.extend(events);
    }

    /// Loads the sprites of object types that appear during the game (produced units,
    /// placed buildings). A building is asked for as soon as it is being placed, so its
    /// site can be drawn right after the click.
    fn want_new_types(&mut self) {
        let (Some(world), true) = (&self.world, self.objects.ready()) else { return };
        let mut new = Vec::new();
        let placing = self.interface.placing.into_iter();
        for kind in placing.chain(world.units.iter().filter(|u| u.alive).map(|u| u.kind)) {
            if self.known_types.insert(kind) {
                new.push(world.catalog.get(kind).id.clone());
            }
        }
        if !new.is_empty() {
            let requests = &self.requests;
            self.objects.want_units(new.iter().map(String::as_str), &|p| requests.borrow_mut().push(p.to_owned()));
        }
    }

    /// The simulation's units as sprites in their current animation frame.
    fn unit_sprites(&self) -> Vec<SceneSprite> {
        let (Some(world), Some(terrain), true) = (&self.world, &self.terrain, self.objects.ready()) else {
            return Vec::new();
        };
        let objects = self.objects.objects();
        let transparent = self.interface.hud.transparent;
        // The free camera turned by `yaw`: sprites face the camera as they would the
        // original one, by the direction relative to it (#309). Buildings keep theirs.
        let yaw = self.three_d.orbit().map_or(0.0, |o| o.yaw);
        let turn = (yaw * 256.0 / std::f32::consts::TAU).round() as i32 as u8;
        let mut out = Vec::with_capacity(world.units.len());
        effects::field_sprites(world, &self.objects, self.three_d.active(), &mut out);
        // Units inside buildings are not drawn (`Hidden`).
        for u in world.units.iter().filter(|u| u.alive && u.inside_of.is_none() && !self.three_d.shows(u.id)) {
            let kind = world.catalog.get(u.kind);
            let anim = anim_name(u.anim);
            let step = if u.building { 0 } else { u.frame.max(0) as usize };
            // A finished building (`Ready`, not dying) adds its `ADDSECTION` frames (0x5099a8).
            let finished = u.building
                && u.stage >= world.max_stage(u)
                && !matches!(u.anim, AnimKind::BuildLo(_) | AnimKind::Death | AnimKind::DeathLie(_));
            let sections = finished.then_some(u.stages);
            let dir = if u.building { u.dir } else { u.dir.wrapping_sub(turn) };
            let Some(mut look) = objects.animation_look(&kind.id, &anim, dir, step, sections) else { continue };
            // On top: the `#WORK` frame of a finished building (0x50927a).
            if finished {
                look.parts.extend(objects.work_part(&kind.id, self.effects.work_step(u.id)));
            }
            let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
            let h = terrain.height_at(x, y);
            // The transparency mode (key O) draws buildings with `ShowGPTransparent`; a
            // corpse whose death counter passed 100 is drawn with `ShowGPTransparentLayers`
            // (0x506ed5, options 2 → 0x48e180): it fades soon after it fell.
            let transparent = (transparent && u.building) || u.death > CORPSE_FADE || self.three_d.ghost(u.id);
            for (frame, (ox, oy), layer, row) in look.parts {
                let sort = (y >> 1) + row;
                out.push(SceneSprite {
                    frame,
                    layer,
                    x,
                    y,
                    dx: ox,
                    dy: oy - h,
                    sort,
                    player: u.owner,
                    transparent,
                    mirrored: look.mirrored,
                    white: false,
                });
            }
        }
        let veiled = |p: &acr_sim::projectile::Projectile| self.three_d.veils(world, p);
        effects::projectile_sprites(world, &self.objects, self.three_d.draws_rounds(), yaw, &veiled, &mut out);
        self.effects.fire_sprites(world, &self.objects, &|id| self.three_d.shows(id), &mut out);
        effects::ground_fire_sprites(world, &self.objects, &mut out);
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
        let backdrop = self.backdrop();
        if let Some(renderer) = &mut self.renderer {
            renderer.zoomed = self.interface.hud.zoom;
            renderer.orbit = self.three_d.orbit();
            renderer.water_surface = self.three_d.draws_water(self.interface.hud.zoom);
            renderer.detailed = self.three_d.active() && self.three_d.ground_detailed;
            if let Some(images) = self.three_d.take_ground() {
                renderer.set_materials(device, queue, &images);
            }
            renderer.backdrop = backdrop;
            renderer.render(device, queue, encoder, view, size, &self.camera);
        }
        self.objects.zoomed = self.interface.hud.zoom;
        if self.three_d.active() {
            let ground = self.renderer.as_ref();
            let terrain = self.terrain.as_ref();
            let fog = self.mission.drawn_fog().map(acr_sim::fog::Fog::cells);
            let world_fog = self.world_fog.as_mut().map(|f| (f, fog));
            self.three_d.render(
                device,
                queue,
                encoder,
                view,
                size,
                &self.camera,
                ground,
                terrain,
                &mut self.objects,
                world_fog,
            );
        } else {
            self.objects.render(device, queue, encoder, view, size, &self.camera);
        }
        // The free camera's fog lies on the world instead (#309).
        let on_world = self.three_d.active() && self.world_fog.as_ref().is_some_and(WorldFogRenderer::active);
        let shades = if on_world { None } else { self.fog_shades(size) };
        if let Some(fog) = &mut self.fog_renderer {
            fog.prepare(device, queue, &self.camera, shades.as_ref());
        }
    }

    /// Draws the fog of war prepared by [`InGame::render`] into `pass`, after the world
    /// overlays and before the interface.
    /// This frame's counts for the performance recorder (`crate::perf`): the 3D ones, the
    /// living units and corpses, the camera and the game speed.
    pub fn perf_counts(&self, c: &mut crate::perf::Counts) {
        self.three_d.perf_counts(c);
        if let Some(world) = &self.world {
            let alive = world.units.iter().filter(|u| u.alive).count() as u32;
            c.units = alive;
            c.corpses = world.units.len() as u32 - alive;
        }
        c.zoom = self.camera.scale;
        c.ticks_per_second = interface::tick_rate(self.frame_time) as u16;
        c.in_game = self.ready();
    }

    /// A line on the message list for `frames` frames (the performance recorder's toast).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn perf_hint(&mut self, text: &str) {
        let bytes = acr_formats::CodePage::Cp1252.encode(text).unwrap_or_else(|| text.as_bytes().to_vec());
        self.mission.hint(bytes, 150);
    }

    pub fn render_fog(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(fog) = &self.fog_renderer {
            fog.render(pass);
        }
    }

    /// The fog over the view (0x4bee1e): drawn while fog is on (`0x775e18`, always in a
    /// game: only the editor's F key, the cheats and a defeat in multiplayer clear it), by
    /// 0x485606/0x485a3c, or 0x485d2d in the zoomed-out view.
    fn fog_shades(&self, window: [f32; 2]) -> Option<FogShades> {
        let fog = self.mission.drawn_fog()?;
        let (width, cells) = fog.cells();
        let cell = |gx: i32, gy: i32| {
            if (0..width).contains(&gx) && (0..width).contains(&gy) { cells[(gy * width + gx) as usize] } else { 0 }
        };
        Some(FogShades::new(&self.camera, window, self.interface.hud.zoom, cell, acr_sim::fog::Fog::shade))
    }
}

/// The death counter (`Sdoxlo`) above which a corpse is drawn half transparent (0x506ed5).
const CORPSE_FADE: u16 = 100;

/// The `.MD` animation a simulation animation plays.
fn anim_name(kind: AnimKind) -> String {
    kind.name()
}
