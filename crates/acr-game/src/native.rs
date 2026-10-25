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
    let args: Vec<String> = std::env::args().collect();
    let drive = crate::drive::Options::from_args(&args);
    init_logging(drive.is_some());
    // Where the frames' time goes, logged every 5 s (docs/rendering-3d.md).
    acr_sim::prof::enable(std::env::var_os("ACR_PROFILE").is_some());
    if drive.is_some() {
        // The driver answers on stdout; errors go there too, not into a message box.
        return match run(drive) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                log::error!("{e}");
                println!("{}", serde_json::json!({"event": "error", "ok": false, "error": e}));
                std::process::ExitCode::FAILURE
            }
        };
    }
    match run(None) {
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

fn run(drive: Option<crate::drive::Options>) -> Result<(), String> {
    let dir = match &drive {
        // No folder picker under the driver.
        Some(_) => find_game_dir_quietly().ok_or("no American Conquest folder: pass it or set ACR_GAME_DIR")?,
        None => match find_game_dir()? {
            Some(dir) => dir,
            None => return Ok(()),
        },
    };
    log::info!("game data: {}", dir.display());
    let assets = ExtraFiles::new(NativeAssets::open(&dir)?, &dir);
    let event_loop = crate::event_loop()?;
    let mut game = Game::new(event_loop.create_proxy(), Box::new(assets));
    // `ACR_SAVE_DIR` keeps savegames and player profiles of test runs out of the game folder.
    let save_root = std::env::var_os("ACR_SAVE_DIR").map(PathBuf::from).unwrap_or_else(|| dir.clone());
    game.set_saves(Box::new(acr_app::saves::FolderSaves::new(&save_root)));
    game.set_profiles(Box::new(acr_app::saves::FolderSaves::files(&save_root, "Players", "txt")));
    // The options: `settings.txt` next to the remembered game folder, or in `ACR_SETTINGS_DIR`
    // (for runs side by side). The driver uses the defaults unless `ACR_SETTINGS_DIR` is set.
    let settings_dir = std::env::var_os("ACR_SETTINGS_DIR").map(PathBuf::from);
    let settings_dir = if drive.is_some() { settings_dir } else { settings_dir.or_else(data_dir) };
    match settings_dir {
        Some(settings) => {
            let place = settings.join("settings.txt").display().to_string();
            game.set_settings_store(Box::new(acr_app::saves::FolderSaves::files(&settings, "", "txt")), &place);
        }
        None => log::info!("settings: driver without ACR_SETTINGS_DIR, using the defaults (models=Off)"),
    }
    if let Some(options) = drive {
        game.set_driver(options);
    }
    // Developer shortcut: skip movies and menus and open a map right away.
    if let Ok(map) = std::env::var("ACR_START_MAP") {
        game.start_map(&map);
    }
    let ended = event_loop.run_app(&mut game).map_err(|e| e.to_string());
    log::info!("event loop ended: {ended:?}");
    log::logger().flush();
    ended
}

/// ACR's own files from outside the game folder, which take precedence over the game folder's
/// own `ACR\Models` and `ACR\Sounds`:
/// - 3D models (docs/rendering-3d.md): the folder `ACR_MODELS_DIR` names, else `ACR\Models`
///   next to the executable; their `*.glb` files appear as `ACR\Models\<file>`;
/// - hit and pain sounds (docs/simulation.md): the folder `ACR_SOUNDS_DIR` names, else
///   `ACR\Sounds` next to the executable, else (debug builds) the repository's
///   `assets/sounds`; their `*.ogg` and `*.wav` files appear as `ACR\Sounds\<file>`.
struct ExtraFiles {
    inner: NativeAssets,
    dirs: Vec<ExtraDir>,
}

/// A folder whose files stand for the game folder `game` (`ACR\Models`).
struct ExtraDir {
    game: &'static str,
    dir: PathBuf,
    files: Vec<String>,
}

impl ExtraDir {
    fn find(game: &'static str, var: &str, game_dir: &Path, dev: Option<PathBuf>, exts: &[&str]) -> Option<Self> {
        let next_to_exe = std::env::current_exe()
            .ok()
            .and_then(|e| Some(e.parent()?.join(game.replace('\\', "/"))))
            .filter(|d| !d.starts_with(game_dir) && d.is_dir());
        let dir = std::env::var_os(var).map(PathBuf::from).or(next_to_exe).or(dev).filter(|d| d.is_dir())?;
        let files: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| exts.iter().any(|x| n.to_ascii_lowercase().ends_with(x)))
            .collect();
        // The detailed ground's textures (docs/ground.md): `ACR\Models\ground\*.png`.
        let mut files = files;
        if exts.contains(&".glb") {
            files.extend(
                std::fs::read_dir(dir.join("ground"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.to_ascii_lowercase().ends_with(".png"))
                    .map(|n| format!("ground/{n}")),
            );
        }
        log::info!("{game} from {}: {} files", dir.display(), files.len());
        Some(Self { game, dir, files })
    }

    /// The file that the game path `path` stands for.
    fn local(&self, path: &str) -> Option<PathBuf> {
        let norm = acr_formats::normalize_path(path);
        let name = norm.strip_prefix(&acr_formats::normalize_path(self.game))?.strip_prefix('/')?;
        let file = self.files.iter().find(|f| f.eq_ignore_ascii_case(name))?;
        Some(self.dir.join(file))
    }
}

impl ExtraFiles {
    fn new(inner: NativeAssets, game_dir: &Path) -> Self {
        let dev_sounds = cfg!(debug_assertions)
            .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("assets").join("sounds"));
        let dirs = [
            ExtraDir::find(r"ACR\Models", "ACR_MODELS_DIR", game_dir, None, &[".glb"]),
            ExtraDir::find(r"ACR\Sounds", "ACR_SOUNDS_DIR", game_dir, dev_sounds, &[".ogg", ".wav"]),
        ];
        Self { inner, dirs: dirs.into_iter().flatten().collect() }
    }

    fn local(&self, path: &str) -> Option<PathBuf> {
        self.dirs.iter().find_map(|d| d.local(path))
    }
}

impl acr_app::Assets for ExtraFiles {
    fn paths(&self) -> Vec<String> {
        let mut paths = self.inner.paths();
        for d in &self.dirs {
            paths.extend(d.files.iter().map(|f| format!(r"{}\{f}", d.game)));
        }
        paths.sort();
        paths.dedup();
        paths
    }

    fn request(&self, path: &str, done: acr_app::LoadCallback) {
        match self.local(path) {
            Some(file) => done(std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))),
            None => self.inner.request(path, done),
        }
    }
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

/// The places to look for the game, in order: the first argument that is not a flag,
/// `ACR_GAME_DIR`, the executable's folder and its parents, the remembered folder.
fn candidates() -> Vec<PathBuf> {
    let exe_dirs = std::env::current_exe()
        .ok()
        .map(|exe| exe.ancestors().skip(1).take(3).map(Path::to_path_buf).collect::<Vec<_>>())
        .unwrap_or_default();
    let remembered = remembered_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| PathBuf::from(s.trim()));
    let args: Vec<String> = std::env::args().skip(1).collect();
    crate::drive::strip_args(&args)
        .into_iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .into_iter()
        .chain(std::env::var_os("ACR_GAME_DIR").map(PathBuf::from))
        .chain(exe_dirs)
        .chain(remembered)
        .collect()
}

/// The game folder without asking.
fn find_game_dir_quietly() -> Option<PathBuf> {
    candidates().into_iter().find(|d| is_game_dir(d))
}

/// Returns `None` when the player cancels the folder picker.
fn find_game_dir() -> Result<Option<PathBuf>, String> {
    if let Some(dir) = find_game_dir_quietly() {
        return Ok(Some(dir));
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
/// side). The driver logs to stderr unless `ACR_LOG` is set, so test runs keep the player's
/// log (and a panic in it).
fn init_logging(drive: bool) {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    let path = std::env::var_os("ACR_LOG")
        .map(PathBuf::from)
        .or_else(|| if drive { None } else { data_dir().map(|d| d.join("acr.log")) });
    if let Some(path) = path
        && path.parent().is_none_or(|d| std::fs::create_dir_all(d).is_ok())
        && let Ok(file) = std::fs::File::create(&path)
    {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    } else if drive {
        // Never on stdout, which carries the driver's answers.
        builder.target(env_logger::Target::Stderr);
    }
    if drive {
        crate::drive::install_logger(builder);
    } else {
        builder.init();
    }
    // A panic goes into the log with its backtrace: the window has no console.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!(
            "{info}
{}",
            std::backtrace::Backtrace::force_capture()
        );
        log::logger().flush();
        default(info);
    }));
}
