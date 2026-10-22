//! Desktop entry point: find the original game files, then run.
//!
//! Lookup order: command-line argument, `ACR_GAME_DIR`, the executable's own folder (and
//! its parents), the folder remembered from an earlier run, and finally a folder picker
//! whose answer is remembered. Errors are shown in a message box because release builds
//! have no console; a log is written to `%LOCALAPPDATA%\ACR\acr.log`.

use std::path::{Path, PathBuf};

use acr_app::NativeAssets;

use crate::Game;

/// Marker file of an American Conquest installation.
const MARKER: &str = "ALL.GSC";

pub fn main() -> std::process::ExitCode {
    init_logging();
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("American Conquest")
                .set_description(&e)
                .show();
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(dir) = find_game_dir()? else { return Ok(()) };
    log::info!("game data: {}", dir.display());
    let assets = NativeAssets::open(&dir)?;
    let event_loop = crate::event_loop()?;
    let mut game = Game::new(event_loop.create_proxy(), Box::new(assets));
    game.set_saves(Box::new(acr_app::saves::FolderSaves::new(&dir)));
    game.set_profiles(Box::new(acr_app::saves::FolderSaves::files(&dir, "Players", "txt")));
    // Developer shortcut: skip movies and menus and open a map right away.
    if let Ok(map) = std::env::var("ACR_START_MAP") {
        game.start_map(&map);
    }
    event_loop.run_app(&mut game).map_err(|e| e.to_string())
}

fn is_game_dir(dir: &Path) -> bool {
    dir.join(MARKER).is_file()
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("HOME")).map(|d| PathBuf::from(d).join("ACR"))
}

fn remembered_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("game-dir.txt"))
}

/// Returns `None` when the player cancels the folder picker.
fn find_game_dir() -> Result<Option<PathBuf>, String> {
    let exe_dirs = std::env::current_exe()
        .ok()
        .map(|exe| exe.ancestors().skip(1).take(3).map(Path::to_path_buf).collect::<Vec<_>>())
        .unwrap_or_default();
    let remembered = remembered_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| PathBuf::from(s.trim()));
    let candidates = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .into_iter()
        .chain(std::env::var_os("ACR_GAME_DIR").map(PathBuf::from))
        .chain(exe_dirs)
        .chain(remembered);
    for dir in candidates {
        if is_game_dir(&dir) {
            return Ok(Some(dir));
        }
    }
    loop {
        let Some(dir) = rfd::FileDialog::new().set_title("Select your American Conquest folder").pick_folder() else {
            return Ok(None);
        };
        if is_game_dir(&dir) {
            if let Some(path) = remembered_path() {
                let saved = path
                    .parent()
                    .map(std::fs::create_dir_all)
                    .transpose()
                    .and_then(|_| std::fs::write(&path, dir.to_string_lossy().as_bytes()));
                if let Err(e) = saved {
                    log::warn!("cannot remember the game folder: {e}");
                }
            }
            return Ok(Some(dir));
        }
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("American Conquest")
            .set_description(format!(
                "{} contains no {MARKER}. Please select the folder American Conquest is installed in.",
                dir.display()
            ))
            .show();
    }
}

/// Logs to `%LOCALAPPDATA%\ACR\acr.log`, or to the file `ACR_LOG` names (for runs side by
/// side).
fn init_logging() {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    let path = std::env::var_os("ACR_LOG").map(PathBuf::from).or_else(|| data_dir().map(|d| d.join("acr.log")));
    if let Some(path) = path
        && path.parent().is_none_or(|d| std::fs::create_dir_all(d).is_ok())
        && let Ok(file) = std::fs::File::create(&path)
    {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    builder.init();
}
