//! The in-game screen: the started game's map, its objects and the running simulation.
//!
//! The world advances at the original's default speed (10 ticks per second, see
//! `acr_sim::ORIGINAL_TICKS_PER_SECOND`) no matter how fast the screen redraws. Units are
//! drawn from the simulation every frame; the player selects own units with a click or a
//! drag rectangle and orders them with a right click.

use std::cell::RefCell;
use std::collections::HashSet;
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

/// Scroll speed in original pixels per second at scale 1.
const SCROLL_SPEED: f32 = 1400.0;
/// Width of the window border that scrolls while the cursor rests on it.
const EDGE: f32 = 3.0;
/// A drag shorter than this (window pixels) is a click.
const CLICK_SLOP: f32 = 4.0;
/// How far from a unit's ground point (world pixels) a click still hits it.
const PICK_RADIUS: i32 = 24;

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
    camera: Camera,
    centred: bool,
    held: HashSet<NamedKey>,
    cursor: Option<[f32; 2]>,
    failed: Option<String>,
}

impl InGame {
    pub fn new(setup: Box<GameSetup>) -> Self {
        let map_path = match &setup.kind {
            GameKind::Mission(m) => Some(m.map.clone()),
            GameKind::RandomMap(_) => None,
        };
        let failed = map_path.is_none().then(|| "random maps are not generated yet (#74)".to_owned());
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
            camera: Camera { x: 0.0, y: 0.0, scale: 1.0 },
            centred: false,
            held: HashSet::new(),
            cursor: None,
            failed,
        }
    }

    /// Queues the files needed to show and run the game. `paths` are all game paths.
    pub fn start(&mut self, paths: &[String]) {
        let mut queue = self.requests.borrow_mut();
        queue.extend(GroundAssets::FILES.iter().map(|p| (*p).to_owned()));
        queue.extend(self.map_path.clone());
        queue.extend(self.sim_assets.start(paths));
        drop(queue);
        self.objects.start(&|p| self.requests.borrow_mut().push(p.to_owned()));
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
        self.uploaded
    }

    /// Takes a loaded file if it is one of ours. A file may serve several parts (e.g. the
    /// unit definitions are read by both the objects and the simulation).
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> bool {
        let load = |p: &str| self.requests.borrow_mut().push(p.to_owned());
        let for_objects = match result {
            Ok(bytes) => self.objects.on_loaded(path, bytes, &load),
            Err(_) if self.objects.wants(path) => {
                self.objects.on_failed(path);
                true
            }
            Err(_) => false,
        };
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
            return for_objects || for_sim;
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

    pub fn on_cursor(&mut self, at: Option<[f32; 2]>) {
        self.cursor = at;
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
            let (wx, wy) = self.world_at(at);
            let hit = mine
                .map(|u| (u, (u.pos.x >> 4) - wx, (u.pos.y >> 4) - wy))
                .filter(|(_, dx, dy)| dx.abs() <= PICK_RADIUS && dy.abs() <= PICK_RADIUS)
                .min_by_key(|(_, dx, dy)| dx * dx + dy * dy)
                .map(|(u, ..)| u.id);
            self.selected = hit.into_iter().collect();
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
    }

    /// Right button: send the selected units to the point under the cursor.
    pub fn on_right(&mut self) {
        let (Some(at), false) = (self.cursor, self.selected.is_empty()) else { return };
        let (x, y) = self.world_at(at);
        self.commands.push(Command::Move { player: self.local_player(), units: self.selected.clone(), x, y });
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
        if let Some(world) = &mut self.world {
            let ticks = self.clock.advance((f64::from(seconds) * 1e9) as u64);
            for _ in 0..ticks {
                world.step(&std::mem::take(&mut self.commands));
            }
        }
        self.scroll(seconds, window);
        self.objects.set_dynamic(self.unit_sprites());
    }

    fn scroll(&mut self, seconds: f32, window: [f32; 2]) {
        let mut dir = [0.0f32; 2];
        for (key, axis, sign) in [
            (NamedKey::ArrowLeft, 0, -1.0),
            (NamedKey::ArrowRight, 0, 1.0),
            (NamedKey::ArrowUp, 1, -1.0),
            (NamedKey::ArrowDown, 1, 1.0),
        ] {
            if self.held.contains(&key) {
                dir[axis] += sign;
            }
        }
        // No edge scrolling while dragging a selection rectangle.
        if let (Some([x, y]), None) = (self.cursor, self.drag_from) {
            if x < EDGE {
                dir[0] = -1.0;
            } else if x > window[0] - EDGE {
                dir[0] = 1.0;
            }
            if y < EDGE {
                dir[1] = -1.0;
            } else if y > window[1] - EDGE {
                dir[1] = 1.0;
            }
        }
        let step = SCROLL_SPEED * seconds.min(0.1) / self.camera.scale;
        self.camera.x += dir[0] * step;
        self.camera.y += dir[1] * step;
        if let Some(terrain) = &self.terrain {
            // Keep the view over the map.
            let side = (terrain.width - 1) as f32 * 32.0;
            self.camera.x = self.camera.x.clamp(-window[0] / 2.0, side - window[0] / 2.0);
            self.camera.y = self.camera.y.clamp(-window[1] / 2.0, side / 2.0 - window[1] / 2.0);
        }
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
            let anim = match u.anim {
                AnimKind::Stand => "#STAND",
                AnimKind::Rest => "#REST",
                AnimKind::Rest1 => "#REST1",
                AnimKind::MotionL => "#MOTION_L",
                AnimKind::MotionR => "#MOTION_R",
            };
            let step = if u.building { 0 } else { u.frame.max(0) as usize };
            let Some(look) = objects.animation_look(&kind.id, anim, u.dir, step) else { continue };
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
