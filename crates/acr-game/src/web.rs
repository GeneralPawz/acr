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
    // The options in `localStorage` entry `acr-settings`.
    game.set_settings_store(Box::new(acr_app::saves::BrowserSaves::with_prefix("acr-")));
    event_loop.spawn_app(game);
    Ok(())
}

/// Keeps the browser's own reactions away from the game's canvas: the wheel neither scrolls
/// nor (with Ctrl) zooms the page, and the middle button starts no auto-scrolling. winit
/// already cancels most canvas events; these listeners make sure of it (`passive: false`).
pub fn guard_canvas(window: &winit::window::Window) {
    use winit::platform::web::WindowExtWebSys;
    let Some(canvas) = window.canvas() else { return };
    let options = web_sys::AddEventListenerOptions::new();
    options.set_passive(false);
    let cancel = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| e.prevent_default());
    let middle = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(|e: web_sys::MouseEvent| {
        if e.button() == 1 {
            e.prevent_default();
        }
    });
    let _ = canvas.add_event_listener_with_callback_and_add_event_listener_options(
        "wheel",
        cancel.as_ref().unchecked_ref(),
        &options,
    );
    for event in ["mousedown", "auxclick"] {
        let _ = canvas.add_event_listener_with_callback_and_add_event_listener_options(
            event,
            middle.as_ref().unchecked_ref(),
            &options,
        );
    }
    // The canvas lives as long as the page.
    cancel.forget();
    middle.forget();
}
