//! `acr-viewer <game-dir> [sprite.gp | --map NAME [--at X,Y] [--zoom Z]]`

/// The browser build starts through `web::start` instead.
#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let usage = || {
        eprintln!(
            "usage: acr-viewer <game-dir> [sprite.gp | --map NAME [--at X,Y] [--zoom Z]]   (or set ACR_GAME_DIR)"
        );
        std::process::ExitCode::from(2)
    };
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut start = acr_viewer::Start::default();
    let mut option = |name: &str| {
        let i = args.iter().position(|a| a == name)?;
        let value = args.get(i + 1).cloned();
        args.drain(i..(i + 2).min(args.len()));
        Some(value)
    };
    let (map, at, zoom) = (option("--map"), option("--at"), option("--zoom"));
    if [&map, &at, &zoom].iter().any(|o| matches!(o, Some(None))) {
        return usage();
    }
    start.map = map.flatten();
    start.at = at.flatten().and_then(|s| {
        let (x, y) = s.split_once(',')?;
        Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
    });
    start.zoom = zoom.flatten().and_then(|z| z.parse().ok());
    let mut args = args.into_iter();
    let Some(dir) = args.next().or_else(|| std::env::var("ACR_GAME_DIR").ok()) else { return usage() };
    start.sprite = args.next();
    match acr_viewer::run(std::path::Path::new(&dir), start) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
