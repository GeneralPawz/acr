//! Browser entry points, called from `index.html`.

use acr_app::web::WebAssets;
use wasm_bindgen::prelude::*;
use winit::platform::web::EventLoopExtWebSys;

use crate::Game;

/// Starts the game with the files of the player's American Conquest folder.
#[wasm_bindgen]
pub async fn start(files: js_sys::Array) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    run(WebAssets::from_files(files).await?)
}

/// Starts the game with archives served from `base` (see `cargo xtask serve --game-dir`).
#[wasm_bindgen]
pub async fn start_url(base: String, patches: Vec<String>) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    run(WebAssets::from_url(&base, &patches).await?)
}

fn run(assets: WebAssets) -> Result<(), JsValue> {
    let event_loop = crate::event_loop()?;
    let mut game = Game::new(event_loop.create_proxy(), Box::new(assets));
    game.set_saves(Box::new(acr_app::saves::BrowserSaves::default()));
    game.set_profiles(Box::new(acr_app::saves::BrowserSaves::with_prefix("acr-player:")));
    event_loop.spawn_app(game);
    Ok(())
}
