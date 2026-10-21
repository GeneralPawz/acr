//! Browser entry points, called from `index.html`.
//!
//! The URL hash picks what to show first: `#ANMBIZC.GP` or `#map=Battle`.

use acr_app::web::WebAssets;
use wasm_bindgen::prelude::*;
use winit::platform::web::EventLoopExtWebSys;

use crate::{App, Start};

/// Starts the viewer with the files of the player's American Conquest folder.
#[wasm_bindgen]
pub async fn start(files: js_sys::Array) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    run(WebAssets::from_files(files).await?)
}

/// Starts the viewer with archives served from `base` (see `cargo xtask serve --game-dir`).
#[wasm_bindgen]
pub async fn start_url(base: String, patches: Vec<String>) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    run(WebAssets::from_url(&base, &patches).await?)
}

fn run(assets: WebAssets) -> Result<(), JsValue> {
    let hash = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .map(|h| h.trim_start_matches('#').to_owned())
        .filter(|h| !h.is_empty());
    let start = match hash {
        Some(h) => match h.strip_prefix("map=") {
            Some(map) => Start { map: Some(map.to_owned()), ..Start::default() },
            None => Start { sprite: Some(h), ..Start::default() },
        },
        None => Start::default(),
    };
    let event_loop = crate::event_loop()?;
    let app = App::new(event_loop.create_proxy(), Box::new(assets), start);
    event_loop.spawn_app(app);
    Ok(())
}
