//! The main menu: the original background with its six entries.
//!
//! The entry labels are painted into the background bitmap; the click areas below are
//! measured from it on the 1024x768 canvas.

use acr_formats::Bitmap;
use acr_render::{ImageDraw, ImageRenderer, ImageTexture};

use crate::Action;
use crate::canvas::{Canvas, HEIGHT, WIDTH};

const BACKGROUND: &str = "INTERF2\\BACKGROUND_MAIN_MENU.BMP";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    SinglePlayer,
    Multiplayer,
    LoadGame,
    Options,
    Intro,
    Exit,
}

/// Entries with their label centre line (y) on the canvas.
const ENTRIES: [(Entry, f32); 6] = [
    (Entry::SinglePlayer, 494.0),
    (Entry::Multiplayer, 531.0),
    (Entry::LoadGame, 567.0),
    (Entry::Options, 603.0),
    (Entry::Intro, 638.0),
    (Entry::Exit, 673.0),
];
const ENTRY_LEFT: f32 = 415.0;
const ENTRY_WIDTH: f32 = 200.0;
const ENTRY_HALF_HEIGHT: f32 = 17.0;

#[derive(Default)]
pub struct MainMenu {
    background: Option<Bitmap>,
    texture: Option<ImageTexture>,
}

impl MainMenu {
    pub fn assets() -> [&'static str; 1] {
        [BACKGROUND]
    }

    pub fn on_loaded(&mut self, path: &str, bytes: Vec<u8>) {
        if path != BACKGROUND {
            return;
        }
        match Bitmap::parse(&bytes) {
            Ok(b) => {
                self.background = Some(b);
                self.texture = None;
            }
            Err(e) => log::error!("{path}: {e}"),
        }
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, images: &ImageRenderer) {
        if let (Some(b), None) = (&self.background, &self.texture) {
            self.texture = Some(images.upload(device, queue, b.width, b.height, &b.to_rgba()));
        }
    }

    pub fn draws(&self, canvas: &Canvas) -> Vec<ImageDraw> {
        self.texture.iter().map(|t| ImageDraw::full(t, canvas.to_window([0.0, 0.0, WIDTH, HEIGHT]))).collect()
    }

    pub fn hit(&self, p: [f32; 2]) -> Option<Entry> {
        if !(ENTRY_LEFT..ENTRY_LEFT + ENTRY_WIDTH).contains(&p[0]) {
            return None;
        }
        ENTRIES.iter().find(|(_, y)| (p[1] - y).abs() <= ENTRY_HALF_HEIGHT).map(|(e, _)| *e)
    }

    pub fn click(&mut self, p: [f32; 2]) -> Action {
        match self.hit(p) {
            Some(Entry::Exit) => Action::Quit,
            Some(Entry::Intro) => Action::Intro,
            Some(entry) => {
                log::info!("{entry:?} is not implemented yet");
                Action::None
            }
            None => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_entries_on_their_lines() {
        let m = MainMenu::default();
        assert_eq!(m.hit([512.0, 494.0]), Some(Entry::SinglePlayer));
        assert_eq!(m.hit([512.0, 680.0]), Some(Entry::Exit));
        assert_eq!(m.hit([200.0, 494.0]), None);
        assert_eq!(m.hit([512.0, 300.0]), None);
    }
}
