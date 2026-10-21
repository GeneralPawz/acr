//! `AmericanConquest.exe`: double-click to play.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    acr_game::native::main()
}

/// The browser build starts through `start` / `start_url` instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
