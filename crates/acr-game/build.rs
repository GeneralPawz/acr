//! Embeds the application icon and version info into the Windows executable.
//!
//! The original game's icon (`Ac.ico`) is used when available, from `ACR_ICON` or
//! `ACR_GAME_DIR/Ac.ico`; it is never committed to the repository. Without it a generated
//! stand-in icon is embedded.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=ACR_ICON");
    println!("cargo:rerun-if-env-changed=ACR_GAME_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let original = std::env::var_os("ACR_ICON")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ACR_GAME_DIR").map(|d| PathBuf::from(d).join("Ac.ico")))
        .filter(|p| p.is_file());
    let icon = match original {
        Some(path) => {
            println!("cargo:rerun-if-changed={}", path.display());
            path
        }
        None => {
            let path = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("acr.ico");
            std::fs::write(&path, fallback_icon()).expect("write fallback icon");
            path
        }
    };
    let mut res = winresource::WindowsResource::new();
    // Resource id 1 is what the game loads at runtime for the window and taskbar icon.
    res.set_icon_with_id(icon.to_str().expect("icon path is UTF-8"), "1")
        .set("ProductName", "American Conquest (ACR)")
        .set("FileDescription", "American Conquest")
        .set("LegalCopyright", "Engine: ACR contributors. Game data: GSC Game World.");
    if let Err(e) = res.compile() {
        // A missing resource compiler must not break the build; the exe just has no icon.
        println!("cargo:warning=could not embed the icon: {e}");
    }
}

/// A 32x32 icon: a gold compass star on dark green, as a 32-bit BMP-in-ICO.
fn fallback_icon() -> Vec<u8> {
    const N: i32 = 32;
    let mut pixels = Vec::with_capacity((N * N * 4) as usize);
    // ICO bitmaps are stored bottom-up.
    for y in (0..N).rev() {
        for x in 0..N {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let r = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx);
            // Eight-pointed star: radius depends on the angle.
            let star = 6.0 + 9.0 * (4.0 * angle).cos().abs().powf(6.0);
            let bgra = if r <= star {
                [40, 170, 215, 255]
            } else if r <= 15.5 {
                [30, 58, 38, 255]
            } else {
                [0, 0, 0, 0]
            };
            pixels.extend(bgra);
        }
    }
    let mut ico = Vec::new();
    ico.extend([0, 0, 1, 0, 1, 0]); // reserved, type icon, 1 image
    let image_len = 40 + pixels.len() + (N * N / 8) as usize;
    ico.extend([N as u8, N as u8, 0, 0, 1, 0, 32, 0]);
    ico.extend((image_len as u32).to_le_bytes());
    ico.extend(22u32.to_le_bytes());
    ico.extend(40u32.to_le_bytes());
    ico.extend(N.to_le_bytes());
    ico.extend((2 * N).to_le_bytes()); // XOR + AND mask height
    ico.extend(1u16.to_le_bytes());
    ico.extend(32u16.to_le_bytes());
    ico.extend([0u8; 24]);
    ico.extend(pixels);
    ico.extend(vec![0u8; (N * N / 8) as usize]); // AND mask unused with alpha
    ico
}
