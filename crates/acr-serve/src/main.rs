//! `AmericanConquest-Web`: starts the browser version from the release without commands.
//!
//! It serves the browser build in the `web` folder next to it on a free localhost port and
//! opens the default browser. If it finds an American Conquest installation, the page loads
//! the game files from it; otherwise the page asks for the game folder (folder picker).
//! The game folder is, in this order: the first argument, `ACR_GAME_DIR`, the launcher's
//! own folder or its parent, the folder the desktop game last used
//! (`%LOCALAPPDATA%\ACR\game-dir.txt`). Closing the window stops the server.
//! `ACR_NO_BROWSER` keeps it from opening the browser (tests).

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let Some(web) = exe_dir.as_deref().and_then(web_dir) else {
        eprintln!("No browser build (web\\index.html) found next to AmericanConquest-Web.");
        wait_for_enter();
        return ExitCode::FAILURE;
    };
    let game = game_dir(exe_dir.as_deref());
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Cannot start the local server: {e}");
            wait_for_enter();
            return ExitCode::FAILURE;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let url = match &game {
        Some(_) => format!("http://127.0.0.1:{port}/?game=/game/"),
        None => format!("http://127.0.0.1:{port}/"),
    };
    println!("American Conquest im Browser: {url}");
    match &game {
        Some(dir) => println!("Spieldaten: {}", dir.display()),
        None => println!("Kein Spielordner gefunden: bitte im Browser den American-Conquest-Ordner waehlen."),
    }
    println!("Dieses Fenster schliessen, um den Server zu beenden.");
    if std::env::var_os("ACR_NO_BROWSER").is_none() {
        open_browser(&url);
    }
    match acr_serve::serve(listener, &web, game.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Server error: {e}");
            wait_for_enter();
            ExitCode::FAILURE
        }
    }
}

/// `web/` next to the launcher, or the launcher's own folder if the page is there.
fn web_dir(exe_dir: &Path) -> Option<PathBuf> {
    [exe_dir.join("web"), exe_dir.to_path_buf()].into_iter().find(|d| d.join("index.html").is_file())
}

fn is_game_dir(dir: &Path) -> bool {
    dir.join("ALL.GSC").is_file()
}

fn game_dir(exe_dir: Option<&Path>) -> Option<PathBuf> {
    let remembered = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(|d| PathBuf::from(d).join("ACR").join("game-dir.txt"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| PathBuf::from(s.trim()));
    let candidates = [
        std::env::args_os().nth(1).map(PathBuf::from),
        std::env::var_os("ACR_GAME_DIR").map(PathBuf::from),
        exe_dir.map(Path::to_path_buf),
        exe_dir.and_then(Path::parent).map(Path::to_path_buf),
        remembered,
    ];
    candidates.into_iter().flatten().find(|d| is_game_dir(d))
}

fn open_browser(url: &str) {
    let result = if cfg!(windows) {
        Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        eprintln!("Cannot open the browser ({e}); open {url} yourself.");
    }
}

/// Keeps the console open so that a double-clicked launcher's error stays readable.
fn wait_for_enter() {
    eprintln!("Enter druecken zum Beenden.");
    let _ = std::io::stdin().read_line(&mut String::new());
}
