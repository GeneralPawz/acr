//! `AmericanConquest.exe`: double-click to play.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn main() -> std::process::ExitCode {
    acr_game::native::main()
}

/// The browser build starts through `start` / `start_url` instead, the Android app through
/// `android_main` (a NativeActivity library).
#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn main() {}
