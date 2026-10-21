//! Map mode: browse the maps (`.m3d`) and scroll over their terrain.
//!
//! Arrows or the window edge scroll, right/middle drag pans, wheel or +/- zoom,
//! PageUp/PageDown previous/next map, Home: centre, O: objects on/off.

use std::collections::HashSet;

use acr_formats::{M3dMap, normalize_path};
use acr_render::terrain::{Camera, GroundAssets, TerrainData, TerrainRenderer};
use winit::keyboard::{Key, NamedKey};

use crate::Loader;
use acr_render::objects_view::ObjectsView as Objects;

/// Scroll speed in output pixels per second.
const SCROLL_SPEED: f32 = 1400.0;
/// Width of the window border that scrolls while the cursor rests on it.
const EDGE: f32 = 3.0;
const ZOOMS: [f32; 9] = [0.125, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0];
const DEFAULT_ZOOM: usize = 4;

/// Finds a map by file stem (`battle5`), path suffix or folder name (`Saratoga`),
/// ignoring case.
fn find_map(maps: &[String], want: &str) -> Option<usize> {
    let want = normalize_path(want).trim_end_matches(".M3D").to_owned();
    let path = |p: &String| normalize_path(p).trim_end_matches(".M3D").to_owned();
    let stem = |p: &String| path(p).rsplit('/').next().unwrap_or_default().to_owned();
    maps.iter()
        .position(|p| stem(p) == want)
        .or_else(|| maps.iter().position(|p| path(p).ends_with(&want)))
        .or_else(|| maps.iter().position(|p| path(p).split('/').any(|part| part == want)))
        .or_else(|| maps.iter().position(|p| path(p).contains(&want)))
}

pub struct MapView {
    maps: Vec<String>,
    current: usize,
    ground_files: Vec<Option<Vec<u8>>>,
    assets: Option<GroundAssets>,
    /// A parsed map waiting for the terrain assets.
    pending: Option<(String, M3dMap)>,
    /// The shown map and whether the renderer has it.
    shown: Option<(String, TerrainData, bool)>,
    /// The shown map's data, for placing its objects.
    map: Option<M3dMap>,
    objects: Objects,
    renderer: Option<TerrainRenderer>,
    camera: Camera,
    zoom: usize,
    /// World position to centre on when the first map appears.
    focus: Option<[f32; 2]>,
    held: HashSet<NamedKey>,
    cursor: Option<[f32; 2]>,
    drag: Option<[f32; 2]>,
}

impl MapView {
    pub fn new(paths: &[String], start: &crate::Start) -> Self {
        let mut maps: Vec<String> =
            paths.iter().filter(|p| p.to_ascii_uppercase().ends_with(".M3D")).cloned().collect();
        maps.sort_by_key(|p| p.to_ascii_uppercase());
        let current = start.map.as_deref().and_then(|want| find_map(&maps, want)).unwrap_or(0);
        let zoom = start.zoom.and_then(|z| ZOOMS.iter().position(|&s| s >= z)).unwrap_or(DEFAULT_ZOOM);
        Self {
            maps,
            current,
            focus: start.at,
            ground_files: vec![None; GroundAssets::FILES.len()],
            assets: None,
            pending: None,
            shown: None,
            map: None,
            objects: Objects::new(),
            renderer: None,
            camera: Camera { x: 0.0, y: 0.0, scale: ZOOMS[zoom] },
            zoom,
            held: HashSet::new(),
            cursor: None,
            drag: None,
        }
    }

    pub fn start(&self, load: &Loader<'_>) {
        self.objects.start(load);
        if self.assets.is_none() {
            for path in GroundAssets::FILES {
                load(path);
            }
        }
        if let Some(path) = self.maps.get(self.current) {
            load(path);
        }
    }

    fn step_map(&mut self, delta: isize, load: &Loader<'_>) {
        if self.maps.is_empty() {
            return;
        }
        self.current = (self.current as isize + delta).rem_euclid(self.maps.len() as isize) as usize;
        load(&self.maps[self.current]);
    }

    /// Takes a loaded file if it is one of ours.
    pub fn on_loaded(&mut self, path: &str, bytes: &[u8], window: [f32; 2], load: &Loader<'_>) {
        self.objects.on_loaded(path, bytes, load);
        self.take(path, bytes, window);
        if let (Some(map), Some((_, terrain, _))) = (&self.map, &self.shown) {
            self.objects.update(map, terrain, load);
        }
    }

    pub fn on_failed(&mut self, path: &str) {
        self.objects.on_failed(path);
    }

    fn take(&mut self, path: &str, bytes: &[u8], window: [f32; 2]) {
        if let Some(i) = GroundAssets::FILES.iter().position(|&f| f == path) {
            self.ground_files[i] = Some(bytes.to_vec());
            if self.ground_files.iter().all(Option::is_some) {
                let files: Vec<Vec<u8>> = self.ground_files.iter_mut().map(|f| f.take().unwrap_or_default()).collect();
                match GroundAssets::parse(&files) {
                    Ok(a) => self.assets = Some(a),
                    Err(e) => log::error!("terrain assets: {e}"),
                }
                self.prepare(window);
            }
            return;
        }
        if self.maps.get(self.current).map(String::as_str) != Some(path) {
            return;
        }
        match M3dMap::parse(bytes) {
            Ok(map) => self.pending = Some((path.to_owned(), map)),
            Err(e) => log::error!("{path}: {e}"),
        }
        self.prepare(window);
    }

    /// Turns a parsed map into terrain data once the assets are there.
    fn prepare(&mut self, window: [f32; 2]) {
        let Some(assets) = &self.assets else { return };
        let Some((path, map)) = self.pending.take() else { return };
        match TerrainData::new(&map, assets) {
            Ok(terrain) => {
                self.shown = Some((path, terrain, false));
                self.map = Some(map);
                self.objects.reset();
                self.centre(window);
            }
            Err(e) => log::error!("{path}: {e}"),
        }
    }

    fn centre(&mut self, window: [f32; 2]) {
        let Some((_, terrain, _)) = &self.shown else { return };
        let side = (terrain.width - 1) as f32 * 32.0;
        let [x, y] = self.focus.take().unwrap_or([side / 2.0, side / 2.0]);
        self.camera.x = x - window[0] / self.camera.scale / 2.0;
        self.camera.y = y / 2.0 - window[1] / self.camera.scale / 2.0;
    }

    /// Creates the renderer and uploads the map when the GPU is ready.
    pub fn sync_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        if self.renderer.is_none()
            && let Some(assets) = &self.assets
        {
            self.renderer = Some(TerrainRenderer::new(device, queue, format, assets));
        }
        if let (Some(renderer), Some((_, terrain, uploaded))) = (&mut self.renderer, &mut self.shown)
            && !*uploaded
        {
            renderer.set_map(device, queue, terrain);
            *uploaded = true;
        }
        self.objects.sync_gpu(device, queue, format);
    }

    pub fn title(&self) -> String {
        match &self.shown {
            Some((path, ..)) => format!(
                "{path} [{}/{}]  x {:.0}  y {:.0}  zoom {}  objects {}",
                self.current + 1,
                self.maps.len(),
                self.camera.x,
                self.camera.y * 2.0,
                self.camera.scale,
                self.objects.count()
            ),
            None if self.maps.is_empty() => "ACR viewer - no maps found".to_owned(),
            None => "ACR viewer - loading map".to_owned(),
        }
    }

    pub fn on_key(&mut self, key: &Key, pressed: bool, load: &Loader<'_>, window: [f32; 2]) {
        if let Key::Named(named) = key {
            if pressed {
                self.held.insert(*named);
            } else {
                self.held.remove(named);
            }
        }
        if !pressed {
            return;
        }
        match key {
            Key::Named(NamedKey::PageDown) => self.step_map(1, load),
            Key::Named(NamedKey::PageUp) => self.step_map(-1, load),
            Key::Named(NamedKey::Home) => self.centre(window),
            Key::Character(c) => match c.as_str() {
                "+" | "=" => self.zoom_by(1, [window[0] / 2.0, window[1] / 2.0]),
                "-" => self.zoom_by(-1, [window[0] / 2.0, window[1] / 2.0]),
                "o" | "O" => self.objects.shown = !self.objects.shown,
                _ => {}
            },
            _ => {}
        }
    }

    /// Steps through [`ZOOMS`], keeping the point under `anchor` (output pixels) in place.
    pub fn zoom_by(&mut self, steps: i32, anchor: [f32; 2]) {
        let next = (self.zoom as i32 + steps).clamp(0, ZOOMS.len() as i32 - 1) as usize;
        let (old, new) = (ZOOMS[self.zoom], ZOOMS[next]);
        self.camera.x += anchor[0] / old - anchor[0] / new;
        self.camera.y += anchor[1] / old - anchor[1] / new;
        self.zoom = next;
        self.camera.scale = new;
    }

    pub fn on_cursor(&mut self, at: Option<[f32; 2]>) {
        if let (Some(from), Some(to)) = (self.drag, at) {
            self.camera.x -= (to[0] - from[0]) / self.camera.scale;
            self.camera.y -= (to[1] - from[1]) / self.camera.scale;
            self.drag = Some(to);
        }
        self.cursor = at;
    }

    pub fn cursor(&self) -> Option<[f32; 2]> {
        self.cursor
    }

    pub fn on_drag(&mut self, active: bool) {
        self.drag = if active { self.cursor } else { None };
    }

    /// Scrolls by the held keys and the cursor at the window edge, `seconds` after the
    /// last update: the speed depends on time, not on the frame rate.
    pub fn update(&mut self, seconds: f32, window: [f32; 2]) {
        let mut dir = [0.0f32; 2];
        let held = |k| self.held.contains(&k);
        if held(NamedKey::ArrowLeft) {
            dir[0] -= 1.0;
        }
        if held(NamedKey::ArrowRight) {
            dir[0] += 1.0;
        }
        if held(NamedKey::ArrowUp) {
            dir[1] -= 1.0;
        }
        if held(NamedKey::ArrowDown) {
            dir[1] += 1.0;
        }
        if let (Some([x, y]), None) = (self.cursor, self.drag) {
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
