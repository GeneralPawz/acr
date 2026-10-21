//! `acr-viewer <game-dir> [sprite.gp]`

/// The browser build starts through `web::start` instead.
#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next().or_else(|| std::env::var("ACR_GAME_DIR").ok()) else {
        eprintln!("usage: acr-viewer <game-dir> [sprite.gp]   (or set ACR_GAME_DIR)");
        return std::process::ExitCode::from(2);
    };
    match acr_viewer::run(std::path::Path::new(&dir), args.next().as_deref()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
