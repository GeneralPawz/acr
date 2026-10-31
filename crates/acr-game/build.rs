//! Embeds the Svelte UI bundle and, on Windows, the application icon and version info.
//!
//! The bundle (docs/webui.md) is the built `ui/dist` or the folder `ACR_UI_DIST` names; it is
//! generated into `ui_bundle.rs` as `FILES` and `PRESENT` (see `webui/bundle.rs`). A missing
//! `ui/dist` is a `cargo:warning`, never an error: the game then uses the engine's UI.
//!
//! The icon is the original game's `Ac.ico`, kept next to this script so every build,
//! CI releases included, carries it. `ACR_ICON` or `ACR_GAME_DIR/Ac.ico` override it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The original game's icon (`Ac.ico` from the American Conquest folder).
const ORIGINAL_ICON: &[u8] = include_bytes!("Ac.ico");

pub fn main() {
    bundle();
    println!("cargo:rerun-if-env-changed=ACR_ICON");
    println!("cargo:rerun-if-env-changed=ACR_GAME_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let local = std::env::var_os("ACR_ICON")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ACR_GAME_DIR").map(|d| PathBuf::from(d).join("Ac.ico")))
        .filter(|p| p.is_file());
    let icon = match local {
        Some(path) => {
            println!("cargo:rerun-if-changed={}", path.display());
            path
        }
        None => {
            let path = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("Ac.ico");
            std::fs::write(&path, ORIGINAL_ICON).expect("write icon");
            path
        }
    };
    let mut res = winresource::WindowsResource::new();
    // Resource id 1 is what the game loads at runtime for the window and taskbar icon.
    res.set_icon_with_id(icon.to_str().expect("icon path is UTF-8"), "1")
        .set("ProductName", "American Conquest (ACR)")
        .set("FileDescription", description())
        .set("LegalCopyright", "Engine: ACR contributors. Game data: GSC Game World.");
    if let Err(e) = res.compile() {
        // A missing resource compiler must not break the build; the exe just has no icon.
        println!("cargo:warning=could not embed the icon: {e}");
    }
}

/// Writes `ui_bundle.rs`: every file of the built UI, as (path with '/', bytes) sorted by path.
fn bundle() {
    println!("cargo:rerun-if-env-changed=ACR_UI_DIST");
    let explicit = std::env::var_os("ACR_UI_DIST").map(PathBuf::from);
    let dist = explicit
        .clone()
        .unwrap_or_else(|| Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../ui/dist"));
    // The browser build serves the bundle from `web/ui/` (cargo xtask web), so the wasm library
    // never carries it.
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32");
    let mut files = Vec::new();
    if !wasm {
        // Watched even while missing, so building the UI later refreshes the embedded copy.
        println!("cargo:rerun-if-changed={}", dist.display());
        if dist.join("acr-ui.js").is_file() {
            collect(&dist, &dist, &mut files);
            files.sort();
        } else if let Some(dir) = explicit {
            panic!("ACR_UI_DIST {} has no acr-ui.js", dir.display());
        } else {
            println!(
                "cargo:warning=no Svelte UI bundle ({} has no acr-ui.js): the game uses the engine's UI; \
                 build it with `cargo xtask ui` (needs Bun) or set ACR_UI_DIST",
                dist.display()
            );
        }
    }
    let mut code = String::new();
    writeln!(code, "/// Files of the UI bundle: path below the bundle folder (with `/`), bytes.").unwrap();
    writeln!(code, "pub const FILES: &[(&str, &[u8])] = &[").unwrap();
    for (name, path) in &files {
        writeln!(code, "    ({name:?}, include_bytes!({:?})),", path.to_str().expect("UI path is UTF-8")).unwrap();
    }
    writeln!(code, "];").unwrap();
    writeln!(code, "/// Whether a bundle was embedded.").unwrap();
    writeln!(code, "pub const PRESENT: bool = {};", !files.is_empty()).unwrap();
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("ui_bundle.rs");
    std::fs::write(out, code).expect("write ui_bundle.rs");
}

/// Adds the files below `dir` as (path relative to `root`, absolute path).
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    for entry in std::fs::read_dir(dir).expect("read the UI bundle folder") {
        let path = entry.expect("read the UI bundle folder").path();
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            let rel: Vec<_> = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_str().expect("UI path is UTF-8"))
                .collect();
            files.push((rel.join("/"), path.canonicalize().expect("canonical UI path")));
        }
    }
}

/// The file description Windows shows for the crate being built.
fn description() -> &'static str {
    match std::env::var("CARGO_PKG_NAME").as_deref() {
        Ok("acr-serve") => "American Conquest (browser version)",
        _ => "American Conquest",
    }
}
