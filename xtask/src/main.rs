//! Build automation, kept in Rust so the project needs no extra tools.
//!
//! - `cargo xtask web [--release] [--app game|viewer]`    build a browser app into `target/web/<app>`
//! - `cargo xtask ui [--watch]`   build the Svelte UI (`ui/`, Bun) into `ui/dist`; `web`, `serve` and
//!   `dist` do it too for the game unless `--no-ui` (or `ACR_UI_DIST` names a built bundle);
//!   `dist` also embeds it in `AmericanConquest.exe`
//! - `cargo xtask ui-diff A.png B.png [--out D.png] [--min PERCENT]`   pixels of two screenshots within +-8
//! - `cargo xtask serve [--release] [--port N] [--game-dir DIR]`   build, then serve it on
//!   localhost; with `--game-dir`, the archives are served under `/game/` with range
//!   support and `http://127.0.0.1:N/?game=/game/` opens the app without a file picker
//! - `cargo xtask dist [--game-dir DIR] [--install]`   the release in `dist/`: `AmericanConquest.exe`,
//!   the browser build in `web/`, its launcher `AmericanConquest-Web.exe` and ACR's sounds in
//!   `ACR/Sounds/`; `--install` copies them into the game folder
//! - `cargo xtask assets [NAME..] [--game-dir DIR] [--compress none|meshopt|draco] [--check] [--blend]`
//!   rebuild the 3D assets with Blender into `target/assets` (see `docs/assets.md`)
//! - `cargo xtask ground [--src DIR] [--out DIR] [--size N]`   pack the detailed ground's CC0 textures
//!   (`assets/ground/pack.py`, Python with Pillow) into `<out>/ground/` (default `target/assets`;
//!   see `docs/ground.md`)
//! - `cargo xtask e2e [FILE..] [--filter S] [--game-dir DIR] [--exe PATH] [--no-build] [--gpu] [--ui engine|classic] [--list] [-v]`
//!   run the scenario tests in `tests/e2e/` through the game's driver mode (see `docs/testing.md`)
//! - `cargo xtask drive [--game-dir DIR] [--exe PATH] [--no-build] [--gpu] [--ui engine|classic]`   driver requests from
//!   stdin (one JSON object per line), answers on stdout
//! - `cargo xtask jev status | explore --goal TEXT [--steps N] | shot --png FILE --expect TEXT`
//!   cheap decisions with TypeSafe's Jev (key from `~/.config/typesafe/api_key`)

mod assets;
mod drive;
mod e2e;
mod jev;
mod ui;

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
    if let Err(e) = check_own_build(env!("CARGO_MANIFEST_DIR"), std::env::var_os("CARGO_MANIFEST_DIR")) {
        eprintln!("xtask: {e}");
        return ExitCode::from(2);
    }
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
        Some("ui") => ui::ui(&workspace_root(), &args[1..]),
        Some("ui-diff") => ui::ui_diff(&args[1..]),
        Some("e2e") => drive::run_e2e(&args[1..], game_dir.clone()),
        Some("drive") => drive::drive(&args[1..], game_dir.clone()),
        Some("jev") => drive::run_jev(&args[1..], game_dir.clone()),
        Some("ground") => {
            let root = workspace_root();
            let src = option("--src").unwrap_or_else(|| r"D:\ACR-models".to_owned());
            let out = option("--out").map_or_else(|| root.join("target").join("assets"), PathBuf::from);
            let size = option("--size").unwrap_or_else(|| "1024".to_owned());
            let status = std::process::Command::new("python")
                .arg(root.join("assets").join("ground").join("pack.py"))
                .args([src.as_str(), &out.to_string_lossy(), size.as_str()])
                .status();
            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => Err(format!("pack.py: {s}").into()),
                Err(e) => Err(format!("python: {e}").into()),
            }
        }
        Some("assets") => {
            let mut names = Vec::new();
            let mut it = args.iter().skip(1);
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--game-dir" | "--compress" => {
                        it.next();
                    }
                    flag if flag.starts_with("--") => {}
                    name => names.push(name.to_owned()),
                }
            }
            let options = assets::Options {
                names,
                game_dir: game_dir.clone(),
                compress: option("--compress").unwrap_or_else(|| "none".to_owned()),
                check: args.iter().any(|a| a == "--check"),
                blend: args.iter().any(|a| a == "--blend"),
            };
            assets::assets(&workspace_root(), &options)
        }
        _ => {
            eprintln!(
                "usage: cargo xtask web [--release] [--app game|viewer]\n       \
                 cargo xtask serve [--release] [--app game|viewer] [--port N] [--game-dir DIR]\n       \
                 cargo xtask dist [--game-dir DIR] [--install]
                        cargo xtask ui [--watch]
                        cargo xtask ui-diff A.png B.png [--out D.png] [--min PERCENT]
                        cargo xtask assets [NAME..] [--game-dir DIR] [--compress none|meshopt|draco] [--check] [--blend]
                        cargo xtask e2e [FILE..] [--filter S] [--game-dir DIR] [--exe PATH] [--no-build] [--gpu] [--ui engine|classic] [--list] [-v]
                        cargo xtask drive [--game-dir DIR] [--exe PATH] [--no-build] [--gpu] [--ui engine|classic]
                        cargo xtask jev status | explore --goal TEXT [--steps N] | shot --png FILE --expect TEXT"
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

/// Refuses a binary built from another checkout (#208). With a `CARGO_TARGET_DIR` shared
/// between worktrees, cargo may keep its fingerprint fresh while another worktree replaced
/// `xtask`, so `cargo xtask` would run that branch's code. `cargo run` passes the manifest
/// directory it ran for; it must be the one this binary was compiled from. Started without
/// cargo (no variable), the binary is taken as it is.
fn check_own_build(built_from: &str, run_for: Option<std::ffi::OsString>) -> Result<()> {
    let Some(run_for) = run_for else { return Ok(()) };
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    if same(Path::new(built_from), Path::new(&run_for)) {
        return Ok(());
    }
    Err(format!(
        "this xtask binary was built from {built_from}, not from {} (a shared CARGO_TARGET_DIR?); \
         use a target directory per worktree, or `touch xtask/src/main.rs` to rebuild it here",
        Path::new(&run_for).display()
    )
    .into())
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
    // The Svelte UI is the game's only (the viewer has none).
    let ui_out = out.join("ui");
    if ui_out.exists() {
        std::fs::remove_dir_all(&ui_out)?;
    }
    if app == "game" {
        let no_ui = std::env::args().any(|a| a == "--no-ui");
        if let Some(dist) = ui::bundle(&root, no_ui)? {
            copy_dir(&dist, &ui_out)?;
        }
    }
    eprintln!("web build ready in {}", out.display());
    Ok(out)
}

/// Builds the release into `dist/`: the game executable (with the Svelte UI embedded), the browser build (`web/`) and its
/// launcher. With a game folder, the original icon is embedded in both; `install` also copies them
/// into that folder, so both can be started from there with a double-click.
fn dist(game_dir: Option<&Path>, install: bool) -> Result<()> {
    let root = workspace_root();
    // The executable embeds the Svelte UI (build.rs reads `ACR_UI_DIST`); without Bun it is left out.
    let ui_dist = ui::bundle(&root, std::env::args().any(|a| a == "--no-ui"))?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(cargo);
    build.current_dir(&root).args(["build", "--release", "--package", "acr-game", "--bin", GAME_EXE]);
    if let Some(dist) = &ui_dist {
        build.env("ACR_UI_DIST", dist);
    }
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
    let mut launcher = Command::new(cargo);
    launcher.current_dir(&root).args(["build", "--release", "--package", "acr-serve", "--bin", WEB_EXE]);
    if let Some(dir) = game_dir {
        launcher.env("ACR_GAME_DIR", dir);
    }
    let launcher = launcher.status()?;
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
        // ACR's own hit and pain sounds, looked up as game files `ACR\Sounds\<file>`.
        copy_dir(&root.join("assets").join("sounds"), &dir.join("ACR").join("Sounds"))?;
        Ok(())
    };
    install_to(&out)?;
    eprintln!("{}", out.display());
    if install {
        let dir = game_dir.ok_or("--install needs --game-dir")?;
        install_to(dir)?;
        eprintln!("installed {exe}, {web_exe}, web/ and ACR/Sounds/ to {}", dir.display());
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

#[cfg(test)]
mod tests {
    use super::check_own_build;

    #[test]
    fn own_build_runs() {
        let here = env!("CARGO_MANIFEST_DIR");
        assert!(check_own_build(here, Some(here.into())).is_ok());
        assert!(check_own_build(here, None).is_ok());
    }

    #[test]
    fn other_checkout_is_refused() {
        let other = std::env::temp_dir();
        let e = check_own_build(env!("CARGO_MANIFEST_DIR"), Some(other.into_os_string())).unwrap_err();
        assert!(e.to_string().contains("shared CARGO_TARGET_DIR"));
    }
}
