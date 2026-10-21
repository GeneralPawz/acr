//! The startup movies, played like the original: `Video\1.bik` .. `Video\10.bik` in order,
//! skipping missing files (this version ships 1-3: publisher logo, GSC logo, intro).
//! A click or Esc/Space/Enter skips the current movie.

use std::collections::VecDeque;
use std::sync::Arc;

use acr_audio::MusicSource;
use acr_render::{ImageDraw, ImageRenderer, ImageTexture};
use acr_video::VideoPlayer;
use web_time::Instant;

use crate::canvas::{Canvas, HEIGHT, WIDTH};

struct Playing {
    path: String,
    bytes: Arc<[u8]>,
    player: VideoPlayer,
    /// Set on the first update so loading time does not count.
    started: Option<Instant>,
    /// A new picture waits for upload.
    dirty: bool,
}

#[derive(Default)]
pub struct Intro {
    queue: VecDeque<String>,
    /// Requested and not loaded yet.
    pending: Option<String>,
    current: Option<Playing>,
    texture: Option<ImageTexture>,
}

impl Intro {
    /// The movies among `paths` (the game's files) in playing order.
    pub fn new(paths: &[String]) -> Self {
        let queue = (1..=10)
            .filter_map(|i| {
                let wanted = format!("video\\{i}.bik");
                paths.iter().find(|p| p.to_ascii_lowercase().replace('/', "\\") == wanted).cloned()
            })
            .collect();
        Self { queue, ..Self::default() }
    }

    /// True while a movie is loading or playing.
    pub fn active(&self) -> bool {
        self.current.is_some() || self.pending.is_some() || !self.queue.is_empty()
    }

    /// The next file to load, once nothing is loading or playing.
    pub fn next_request(&mut self) -> Option<String> {
        if self.current.is_some() || self.pending.is_some() {
            return None;
        }
        let path = self.queue.pop_front()?;
        self.pending = Some(path.clone());
        Some(path)
    }

    /// Takes a loaded file if it is the requested movie; returns its soundtrack to play.
    /// `None` with `taken` false means the file was not ours.
    pub fn on_loaded(&mut self, path: &str, bytes: &[u8]) -> (bool, Option<Box<dyn MusicSource>>) {
        if self.pending.as_deref() != Some(path) {
            return (false, None);
        }
        self.pending = None;
        let bytes: Arc<[u8]> = bytes.into();
        match VideoPlayer::new(bytes.clone()) {
            Ok(player) => {
                let h = player.header();
                log::info!("playing {path}: {}x{}, {:.1} s", h.width, h.height, h.duration());
                let audio = player.audio().map(|a| Box::new(a) as Box<dyn MusicSource>);
                self.current = Some(Playing { path: path.to_owned(), bytes, player, started: None, dirty: false });
                (true, audio)
            }
            Err(e) => {
                log::error!("{path}: {e}");
                (true, None)
            }
        }
    }

    /// Starts the current movie again (used when audio becomes available in the browser);
    /// returns its soundtrack.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn restart(&mut self) -> Option<Box<dyn MusicSource>> {
        let current = self.current.as_mut()?;
        current.player = VideoPlayer::new(current.bytes.clone()).ok()?;
        current.started = None;
        current.player.audio().map(|a| Box::new(a) as Box<dyn MusicSource>)
    }

    /// Advances the current movie; returns `true` when it ended.
    pub fn update(&mut self) -> bool {
        let Some(current) = &mut self.current else { return false };
        let started = *current.started.get_or_insert_with(Instant::now);
        let t = started.elapsed().as_secs_f64();
        match current.player.advance_to(t) {
            Ok(changed) => current.dirty |= changed,
            Err(e) => {
                log::error!("{}: {e}", current.path);
                self.skip();
                return true;
            }
        }
        if current.player.finished(t) {
            self.skip();
            return true;
        }
        false
    }

    /// Ends the current movie.
    pub fn skip(&mut self) {
        self.current = None;
        self.texture = None;
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, images: &ImageRenderer) {
        if let Some(current) = &mut self.current
            && current.dirty
        {
            let p = &current.player;
            self.texture = Some(images.upload(device, queue, p.width(), p.height(), p.rgba()));
            current.dirty = false;
        }
    }

    /// The picture scaled to the canvas width, centred vertically.
    pub fn draws(&self, canvas: &Canvas) -> Vec<ImageDraw> {
        self.texture
            .iter()
            .map(|t| {
                let height = (WIDTH * t.height as f32 / t.width as f32).min(HEIGHT);
                ImageDraw::full(t, canvas.to_window([0.0, (HEIGHT - height) / 2.0, WIDTH, height]))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_existing_movies_in_order() {
        let paths: Vec<String> = ["VIDEO\\3.BIK", "Video\\1.bik", "Music\\1.ogg"].map(String::from).to_vec();
        let mut intro = Intro::new(&paths);
        assert!(intro.active());
        assert_eq!(intro.next_request().as_deref(), Some("Video\\1.bik"));
        assert_eq!(intro.next_request(), None, "one at a time");
        let (taken, audio) = intro.on_loaded("Video\\1.bik", b"not a video");
        assert!(taken && audio.is_none());
        assert_eq!(intro.next_request().as_deref(), Some("VIDEO\\3.BIK"));
        intro.on_loaded("VIDEO\\3.BIK", b"");
        assert!(!intro.active());
    }
}
