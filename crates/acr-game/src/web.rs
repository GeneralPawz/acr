//! Browser entry points, called from `index.html`.

use std::cell::RefCell;
use std::collections::VecDeque;

use acr_app::web::WebAssets;
use wasm_bindgen::prelude::*;
use winit::event_loop::ActiveEventLoop;
use winit::platform::web::EventLoopExtWebSys;

use crate::{Game, Mode};

/// Frame times kept for [`acr_frame_ms`] when the page does not collect them.
const FRAMES_KEPT: usize = 10_000;

thread_local! {
    /// Driver request lines from the page, oldest first.
    static REQUESTS: RefCell<VecDeque<String>> = const { RefCell::new(VecDeque::new()) };
    /// Their answers, one JSON line each.
    static ANSWERS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Milliseconds of each frame's own work (advance and render).
    static FRAMES: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

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

/// Queues one driver request line (the browser's subset, docs/testing.md "Browser"); the game
/// carries it out in a later frame, once it has settled.
#[wasm_bindgen]
pub fn acr_drive(line: String) {
    REQUESTS.with_borrow_mut(|q| q.push_back(line));
}

/// The answers to [`acr_drive`] requests since the last call, one JSON line each.
#[wasm_bindgen]
pub fn acr_answers() -> Vec<String> {
    ANSWERS.with_borrow_mut(std::mem::take)
}

/// The milliseconds of each frame's own work (advance and render) since the last call.
#[wasm_bindgen]
pub fn acr_frame_ms() -> Vec<f32> {
    FRAMES.with_borrow_mut(std::mem::take)
}

/// Carries out the next queued request once the game has settled (skipping the movies).
pub(crate) fn serve_requests(game: &mut Game, event_loop: &ActiveEventLoop) {
    if REQUESTS.with_borrow(VecDeque::is_empty) {
        return;
    }
    if matches!(game.mode, Mode::Intro) {
        game.mode = Mode::Menus;
        return;
    }
    if !crate::drive::web_ready(game) {
        return;
    }
    let Some(line) = REQUESTS.with_borrow_mut(VecDeque::pop_front) else { return };
    let answer = crate::drive::web_request(game, event_loop, &line);
    ANSWERS.with_borrow_mut(|a| a.push(answer.to_string()));
}

/// Notes one frame's work for [`acr_frame_ms`].
pub(crate) fn frame_done(ms: f32) {
    FRAMES.with_borrow_mut(|f| {
        if f.len() >= FRAMES_KEPT {
            f.clear();
        }
        f.push(ms);
    });
}

/// The game's warnings and errors on the browser console (`console.warn`, `console.error`),
/// where the page and the browser test see them.
struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = JsValue::from_str(&format!("{}: {}", record.target(), record.args()));
        if record.level() == log::Level::Error {
            web_sys::console::error_1(&line);
        } else {
            web_sys::console::warn_1(&line);
        }
    }

    fn flush(&self) {}
}

static CONSOLE_LOG: ConsoleLog = ConsoleLog;

fn run(assets: WebAssets) -> Result<(), JsValue> {
    if log::set_logger(&CONSOLE_LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let event_loop = crate::event_loop()?;
    let mut game = Game::new(event_loop.create_proxy(), Box::new(assets));
    game.set_saves(Box::new(acr_app::saves::BrowserSaves::default()));
    game.set_profiles(Box::new(acr_app::saves::BrowserSaves::with_prefix("acr-player:")));
    // The options in `localStorage` entry `acr-settings`.
    game.set_settings_store(Box::new(acr_app::saves::BrowserSaves::with_prefix("acr-")), "localStorage acr-settings");
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
