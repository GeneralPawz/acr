//! Desktop entry point: reads game files directly through the VFS.

use std::path::Path;

use acr_app::NativeAssets;

use crate::{App, Start};

/// Opens the installation at `game_dir` and runs the viewer until the window closes.
pub fn run(game_dir: &Path, start: Start) -> Result<(), String> {
    let assets = NativeAssets::open(game_dir)?;
    let event_loop = crate::event_loop()?;
    let mut app = App::new(event_loop.create_proxy(), Box::new(assets), start);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}
