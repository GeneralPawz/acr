//! The in-game screen: the map of the started game, scrolled like the original.
//!
//! For now this shows the mission's map with its objects as placed by the map; the
//! simulation and the panel join as their crates mature.

use std::cell::RefCell;
use std::collections::HashSet;

use acr_formats::M3dMap;
use acr_render::objects_view::ObjectsView;
use acr_render::terrain::{Camera, GroundAssets, TerrainData, TerrainRenderer};
use winit::keyboard::NamedKey;

use crate::ui::{GameKind, GameSetup};

/// Scroll speed in original pixels per second at scale 1.
const SCROLL_SPEED: f32 = 1400.0;
/// Width of the window border that scrolls while the cursor rests on it.
const EDGE: f32 = 3.0;

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
            objects: ObjectsView::new(),
            requests: RefCell::new(Vec::new()),
            camera: Camera { x: 0.0, y: 0.0, scale: 1.0 },
            centred: false,
            held: HashSet::new(),
            cursor: None,
            failed,
        }
    }

    /// Queues the files needed to show the game.
    pub fn start(&self) {
        let mut queue = self.requests.borrow_mut();
        queue.extend(GroundAssets::FILES.iter().map(|p| (*p).to_owned()));
        queue.extend(self.map_path.clone());
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

    /// Takes a loaded file if it is one of ours.
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> bool {
        // Some files (e.g. the game palette) are needed by both the objects and the terrain.
        let load = |p: &str| self.requests.borrow_mut().push(p.to_owned());
        let for_objects = match result {
            Ok(bytes) => self.objects.on_loaded(path, bytes, &load),
            Err(_) if self.objects.wants(path) => {
                self.objects.on_failed(path);
                true
            }
            Err(_) => false,
        };
        let ground = GroundAssets::FILES.iter().position(|&f| f == path);
        let is_map = self
            .map_path
            .as_deref()
            .is_some_and(|m| acr_formats::normalize_path(m) == acr_formats::normalize_path(path));
        if ground.is_none() && !is_map {
            return for_objects;
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

    /// Starts on the local player's units, else in the middle of the map.
    fn centre(&mut self, window: [f32; 2]) {
        let (Some(map), Some(terrain)) = (&self.map, &self.terrain) else { return };
        let side = (terrain.width - 1) as f32 * 32.0;
        let own = self.setup.local_player as u8;
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

    /// Scrolls by held arrow keys and the cursor at the window edge; the speed depends on
    /// time, not on the frame rate.
    pub fn update(&mut self, seconds: f32, window: [f32; 2]) {
        if let (Some(map), Some(terrain)) = (&self.map, &self.terrain) {
            self.objects.update(map, terrain, &|p| self.requests.borrow_mut().push(p.to_owned()));
        }
        if !self.centred && self.uploaded {
            self.centre(window);
        }
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
        if let Some([x, y]) = self.cursor {
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
