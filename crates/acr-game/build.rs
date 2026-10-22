//! Embeds the application icon and version info into the Windows executable.
//!
//! The icon is the original game's `Ac.ico`, kept next to this script so every build,
//! CI releases included, carries it. `ACR_ICON` or `ACR_GAME_DIR/Ac.ico` override it.

use std::path::PathBuf;

/// The original game's icon (`Ac.ico` from the American Conquest folder).
const ORIGINAL_ICON: &[u8] = include_bytes!("Ac.ico");

pub fn main() {
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

/// The file description Windows shows for the crate being built.
fn description() -> &'static str {
    match std::env::var("CARGO_PKG_NAME").as_deref() {
        Ok("acr-serve") => "American Conquest (browser version)",
        _ => "American Conquest",
    }
}
