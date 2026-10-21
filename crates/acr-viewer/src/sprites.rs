//! Sprite mode: browse and animate every `.GP` sprite pack.
//!
//! Left/Right file, PageUp/PageDown 10 files, Up/Down frame, Space play/pause, G grid,
//! +/- zoom, 1-8 player colour.

use acr_formats::pal::SPRITE_PALETTES;
use acr_formats::{Frame, GpFile, Palette, normalize_path, sprite_palette};
use acr_render::{SpriteAtlas, SpriteDraw, SpriteRenderer};
use acr_sim::FixedClock;
use winit::keyboard::{Key, NamedKey};

use crate::Loader;

const ANIMATION_FPS: u32 = 15;
const PREFERRED_START: &str = "ANMBIZC.GP";

struct LoadedSprite {
    path: String,
    frames: Vec<Frame>,
    failed: usize,
    /// Union of all frame bounds, so animations do not jitter.
    bounds: (i32, i32, i32, i32),
    atlas: Option<SpriteAtlas>,
}

pub struct SpriteView {
    /// The sprite palette sets, indexed like [`SPRITE_PALETTES`].
    palettes: [Option<Palette>; 3],
    files: Vec<String>,
    current: usize,
    sprite: Option<LoadedSprite>,
    frame: usize,
    playing: bool,
    grid: bool,
    scale: f32,
    player: u8,
    clock: FixedClock,
}

impl SpriteView {
    pub fn new(paths: &[String], start: Option<&str>) -> Self {
        let files: Vec<String> = paths.iter().filter(|p| p.to_ascii_uppercase().ends_with(".GP")).cloned().collect();
        let want = normalize_path(start.unwrap_or(PREFERRED_START));
        let current = files.iter().position(|f| normalize_path(f).ends_with(&want)).unwrap_or(0);
        Self {
            palettes: [None, None, None],
            files,
            current,
            sprite: None,
            frame: 0,
            playing: true,
            grid: false,
            scale: 2.0,
            player: 0,
            clock: FixedClock::new(ANIMATION_FPS, 4),
        }
    }

    /// Requests the palettes and the current file.
    pub fn start(&self, load: &Loader<'_>) {
        for path in SPRITE_PALETTES {
            load(path);
        }
        self.open_current(load);
    }

    fn open_current(&self, load: &Loader<'_>) {
        if let Some(path) = self.files.get(self.current) {
            load(path);
        }
    }

    fn step_file(&mut self, delta: isize, load: &Loader<'_>) {
        if self.files.is_empty() {
            return;
        }
        self.current = (self.current as isize + delta).rem_euclid(self.files.len() as isize) as usize;
        self.open_current(load);
    }

    /// Takes a loaded file if it is one of ours.
    pub fn on_loaded(&mut self, path: &str, bytes: &[u8], window: [f32; 2]) {
        if let Some(set) = SPRITE_PALETTES.iter().position(|&p| p == path) {
            self.palettes[set] = Palette::parse(bytes);
            return;
        }
        // Ignore stale results when the user skipped ahead.
        if self.files.get(self.current).map(String::as_str) != Some(path) {
            return;
        }
        let file = match GpFile::parse(bytes.to_vec()) {
            Ok(f) => f,
            Err(e) => {
                log::error!("{path}: {e}");
                return;
            }
        };
        let mut failed = 0;
        let frames: Vec<Frame> = (0..file.frame_count())
            .map(|i| {
                file.decode_frame(i).unwrap_or_else(|_| {
                    failed += 1;
                    Frame::default()
                })
            })
            .collect();
        let bounds = frames
            .iter()
            .filter_map(Frame::bounds)
            .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
            .unwrap_or((0, 0, 1, 1));
        self.sprite = Some(LoadedSprite { path: path.to_owned(), frames, failed, bounds, atlas: None });
        self.frame = 0;
        self.fit_scale(window);
    }

    /// Picks the largest integer zoom (up to 4) at which the sprite fits the window.
    pub fn fit_scale(&mut self, window: [f32; 2]) {
        let Some(sprite) = &self.sprite else { return };
        let (x0, y0, x1, y1) = sprite.bounds;
        let fit = (window[0] / (x1 - x0) as f32).min(window[1] / (y1 - y0) as f32);
        self.scale = fit.floor().clamp(1.0, 4.0);
    }

    /// Pushes the sprite and the palette it was drawn for to the GPU once both exist.
    pub fn sync_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, renderer: &SpriteRenderer) {
        let path = self.sprite.as_ref().map_or("", |s| s.path.as_str());
        let set = SPRITE_PALETTES.iter().position(|&p| p == sprite_palette(path)).unwrap_or(0);
        if let Some(palette) = &self.palettes[set] {
            renderer.set_palette(queue, palette);
        }
        if let Some(sprite) = &mut self.sprite
            && sprite.atlas.is_none()
        {
            sprite.atlas = Some(renderer.upload(device, queue, &sprite.frames));
        }
    }

    pub fn title(&self) -> String {
        match &self.sprite {
            Some(s) => format!(
                "{} [{}/{}]  frame {}/{}{}  x{}  player {}",
                s.path,
                self.current + 1,
                self.files.len(),
                self.frame + 1,
                s.frames.len(),
                if s.failed > 0 { format!(" ({} undecodable)", s.failed) } else { String::new() },
                self.scale,
                self.player + 1,
            ),
            None => "ACR viewer - loading sprites".to_owned(),
        }
    }

    pub fn on_key(&mut self, key: &Key, load: &Loader<'_>) {
        let frames = self.sprite.as_ref().map_or(0, |s| s.frames.len()).max(1);
        match key {
            Key::Named(NamedKey::ArrowRight) => self.step_file(1, load),
            Key::Named(NamedKey::ArrowLeft) => self.step_file(-1, load),
            Key::Named(NamedKey::PageDown) => self.step_file(10, load),
            Key::Named(NamedKey::PageUp) => self.step_file(-10, load),
            Key::Named(NamedKey::ArrowDown) => self.frame = (self.frame + 1) % frames,
            Key::Named(NamedKey::ArrowUp) => self.frame = (self.frame + frames - 1) % frames,
            Key::Named(NamedKey::Space) => self.playing = !self.playing,
            Key::Character(c) => match c.as_str() {
                "g" | "G" => self.grid = !self.grid,
                "+" | "=" => self.scale = (self.scale + 1.0).min(8.0),
                "-" => self.scale = (self.scale - 1.0).max(1.0),
                d if d.len() == 1 && ("1"..="8").contains(&d) => self.player = d.as_bytes()[0] - b'1',
                _ => {}
            },
            _ => {}
        }
    }

    /// Advances the animation by `nanos`; returns whether the frame changed.
    pub fn tick(&mut self, nanos: u64) -> bool {
        let ticks = self.clock.advance(nanos);
        if self.playing
            && !self.grid
            && ticks > 0
            && let Some(s) = &self.sprite
        {
            self.frame = (self.frame + ticks as usize) % s.frames.len().max(1);
            return true;
        }
        false
    }

    pub fn atlas(&self) -> Option<&SpriteAtlas> {
        self.sprite.as_ref().and_then(|s| s.atlas.as_ref())
    }

    pub fn draws(&self, width: f32, height: f32) -> Vec<SpriteDraw> {
        let Some(sprite) = &self.sprite else { return Vec::new() };
        let (x0, y0, x1, y1) = sprite.bounds;
        let (cell_w, cell_h) = ((x1 - x0) as f32 * self.scale, (y1 - y0) as f32 * self.scale);
        let draw = |frame, left: f32, top: f32| SpriteDraw {
            frame,
            x: left - x0 as f32 * self.scale,
            y: top - y0 as f32 * self.scale,
            scale: self.scale,
            player: self.player,
        };
        if !self.grid {
            return vec![draw(self.frame, ((width - cell_w) / 2.0).floor(), ((height - cell_h) / 2.0).floor())];
        }
        let cols = ((width / cell_w) as usize).max(1);
        let rows = ((height / cell_h) as usize).max(1);
        (self.frame..sprite.frames.len())
            .take(cols * rows)
            .enumerate()
            .map(|(i, f)| draw(f, (i % cols) as f32 * cell_w, (i / cols) as f32 * cell_h))
            .collect()
    }
}
