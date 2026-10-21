//! Desktop entry point: reads game files directly through the VFS.

use std::cell::RefCell;
use std::path::Path;

use acr_formats::Vfs;
use winit::event_loop::EventLoopProxy;

use crate::{App, Assets, UserEvent};

struct NativeAssets(RefCell<Vfs>);

impl Assets for NativeAssets {
    fn sprite_paths(&self) -> Vec<String> {
        sprite_paths(&self.0.borrow())
    }

    fn request(&self, path: &str, proxy: &EventLoopProxy<UserEvent>) {
        let result = self.0.borrow_mut().read(path).map_err(|e| e.to_string());
        let _ = proxy.send_event(UserEvent::Loaded { path: path.to_owned(), result });
    }
}

pub(crate) fn sprite_paths(vfs: &Vfs) -> Vec<String> {
    vfs.list().map(|(p, _)| p).filter(|p| p.to_ascii_uppercase().ends_with(".GP")).map(str::to_owned).collect()
}

/// Opens the installation at `game_dir` and runs the viewer until the window closes.
pub fn run(game_dir: &Path, start: Option<&str>) -> Result<(), String> {
    let vfs = Vfs::open_game_dir(game_dir).map_err(|e| format!("{}: {e}", game_dir.display()))?;
    let event_loop = crate::event_loop()?;
    let mut app = App::new(event_loop.create_proxy(), Box::new(NativeAssets(RefCell::new(vfs))), start);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}
