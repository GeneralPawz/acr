//! Build automation, kept in Rust so the project needs no extra tools.
//!
//! - `cargo xtask web [--release] [--app game|viewer]`    build a browser app into `target/web/<app>`
//! - `cargo xtask serve [--release] [--port N] [--game-dir DIR]`   build, then serve it on
//!   localhost; with `--game-dir`, the archives are served under `/game/` with range
//!   support and `http://127.0.0.1:N/?game=/game/` opens the app without a file picker
//! - `cargo xtask dist [--game-dir DIR] [--install]`   the release in `dist/`: `AmericanConquest.exe`,
//!   the browser build in `web/` and its launcher `AmericanConquest-Web.exe`

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const WASM_TARGET: &str = "wasm32-unknown-unknown";
/// Apps with a browser build: `--app game` (default) or `--app viewer`.
const APPS: [&str; 2] = ["game", "viewer"];
/// Name of the game executable (without extension).
const GAME_EXE: &str = "AmericanConquest";
/// Name of the browser version's launcher (without extension), crate `acr-serve`.
const WEB_EXE: &str = "AmericanConquest-Web";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let release = args.iter().any(|a| a == "--release");
    let option = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
    let port = option("--port").and_then(|p| p.parse().ok()).unwrap_or(8080);
    let game_dir = option("--game-dir").map(PathBuf::from);
    let app = option("--app").unwrap_or_else(|| APPS[0].to_owned());
    if !APPS.contains(&app.as_str()) {
        eprintln!("xtask: unknown app {app}, expected one of {APPS:?}");
        return ExitCode::from(2);
    }
    let result = match args.first().map(String::as_str) {
        Some("web") => build_web(&app, release).map(|_| ()),
        Some("serve") => build_web(&app, release).and_then(|dir| serve(&dir, game_dir.as_deref(), port)),
        Some("dist") => dist(game_dir.as_deref(), args.iter().any(|a| a == "--install")),
        _ => {
            eprintln!(
                "usage: cargo xtask web [--release] [--app game|viewer]\n       \
                 cargo xtask serve [--release] [--app game|viewer] [--port N] [--game-dir DIR]\n       \
                 cargo xtask dist [--game-dir DIR] [--install]"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The workspace `cargo xtask` was started in. Found at runtime because a shared
/// `CARGO_TARGET_DIR` may hold an xtask binary built from another checkout.
fn workspace_root() -> PathBuf {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    start
        .ancestors()
        .find(|dir| std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|toml| toml.contains("[workspace]")))
        .map(Path::to_path_buf)
        .unwrap_or(start)
}

/// Cargo's build output directory, honouring `CARGO_TARGET_DIR`.
fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from)
}

/// Compiles the viewer to wasm, generates JS bindings and copies the static page.
fn build_web(app: &str, release: bool) -> Result<PathBuf> {
    let krate = format!("acr-{app}");
    let lib = format!("acr_{app}");
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(cargo);
    build.current_dir(&root).args(["build", "--package", &krate, "--lib", "--target", WASM_TARGET]);
    if release {
        build.arg("--release");
    }
    if !build.status()?.success() {
        return Err("cargo build for wasm failed".into());
    }

    let profile = if release { "release" } else { "debug" };
    let wasm = target_dir(&root).join(WASM_TARGET).join(profile).join(format!("{lib}.wasm"));
    let out = root.join("target").join("web").join(app);
    std::fs::create_dir_all(&out)?;
    wasm_bindgen_cli_support::Bindgen::new()
        .input_path(&wasm)
        .out_name(&lib)
        .web(true)?
        .typescript(false)
        .debug(!release)
        .generate(&out)?;
    std::fs::copy(root.join("crates").join(&krate).join("web/index.html"), out.join("index.html"))?;
    eprintln!("web build ready in {}", out.display());
    Ok(out)
}

/// Builds the release into `dist/`: the game executable, the browser build (`web/`) and its
/// launcher. With a game folder, the original icon is embedded; `install` also copies them
/// into that folder, so both can be started from there with a double-click.
fn dist(game_dir: Option<&Path>, install: bool) -> Result<()> {
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(cargo);
    build.current_dir(&root).args(["build", "--release", "--package", "acr-game", "--bin", GAME_EXE]);
    if let Some(dir) = game_dir {
        if !dir.join("ALL.GSC").is_file() {
            return Err(format!("{} is not an American Conquest folder (no ALL.GSC)", dir.display()).into());
        }
        build.env("ACR_GAME_DIR", dir);
    }
    if !build.status()?.success() {
        return Err("release build failed".into());
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let launcher = Command::new(cargo)
        .current_dir(&root)
        .args(["build", "--release", "--package", "acr-serve", "--bin", WEB_EXE])
        .status()?;
    if !launcher.success() {
        return Err("launcher build failed".into());
    }
    let web = build_web("game", true)?;
    let target = target_dir(&root);
    let exe = format!("{GAME_EXE}{}", std::env::consts::EXE_SUFFIX);
    let built = target.join("release").join(&exe);
    let out = root.join("dist");
    std::fs::create_dir_all(&out)?;
    let web_exe = format!("{WEB_EXE}{}", std::env::consts::EXE_SUFFIX);
    let built_web_exe = target.join("release").join(&web_exe);
    let install_to = |dir: &Path| -> Result<()> {
        std::fs::copy(&built, dir.join(&exe))?;
        std::fs::copy(&built_web_exe, dir.join(&web_exe))?;
        copy_dir(&web, &dir.join("web"))?;
        Ok(())
    };
    install_to(&out)?;
    eprintln!("{}", out.display());
    if install {
        let dir = game_dir.ok_or("--install needs --game-dir")?;
        install_to(dir)?;
        eprintln!("installed {exe}, {web_exe} and web/ to {}", dir.display());
    }
    Ok(())
}

/// Serves a browser build on localhost for testing.
fn serve(dir: &Path, game_dir: Option<&Path>, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("serving {} at http://127.0.0.1:{port}/", dir.display());
    if let Some(game) = game_dir {
        eprintln!("game archives from {} at http://127.0.0.1:{port}/?game=/game/", game.display());
    }
    acr_serve::serve(listener, dir, game_dir)
}

/// Copies the files of `from` (not recursive: a browser build is flat) into `to`.
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let path = entry?.path();
        if path.is_file() {
            std::fs::copy(&path, to.join(path.file_name().ok_or("file name")?))?;
        }
    }
    Ok(())
}
