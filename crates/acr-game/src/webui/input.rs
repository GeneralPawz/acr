//! The UI's messages in the game: raw input enters through the handlers window input uses
//! (`Game::cursor_moved`, `mouse_input`, `mouse_wheel`, `keyboard_input`), so the UI has no
//! hit-testing of its own and the simulation never depends on UI timing.

use acr_ui_protocol::{Button, FromUi, PROTOCOL, Phase};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;

use super::{WebUi, keys};
use crate::Game;
use crate::ui::External;

/// Handles one JSON message from the UI. Malformed messages are logged and dropped.
pub fn on_message(game: &mut Game, event_loop: &ActiveEventLoop, json: &str) {
    match serde_json::from_str::<FromUi>(json) {
        Ok(message) => apply(game, event_loop, message),
        Err(e) => log::warn!("webui: bad message ({e}): {json}"),
    }
}

/// Applies a message the caller already parsed (the driver's `ui_send`).
pub fn apply(game: &mut Game, event_loop: &ActiveEventLoop, message: FromUi) {
    match message {
        FromUi::Ready { protocol, screens, hud, viewport, dpr } => {
            if protocol != PROTOCOL {
                log::error!("webui: the UI speaks protocol {protocol}, the game {PROTOCOL}; the engine draws");
                return;
            }
            let Some(send) = super::sender(game) else {
                log::error!("webui: no way to reach the UI; the engine draws");
                return;
            };
            game.ui.set_external(Some(External { screens: screens.clone(), cursor: true }));
            game.webui = Some(WebUi::new(send, super::HOST, screens, hud, viewport, dpr));
        }
        // Every other message needs a UI that said Ready.
        other => {
            let Some(web) = game.webui.as_mut() else {
                log::warn!("webui: message before Ready");
                return;
            };
            match other {
                FromUi::Viewport { viewport, dpr } => web.viewport = (viewport, dpr),
                FromUi::Resync => web.resync(),
                FromUi::Pointer { phase, x, y, button, mods } => {
                    mods_changed(game, mods);
                    pointer(game, event_loop, phase, [x, y], button.unwrap_or(Button::Left));
                }
                FromUi::Wheel { lines, mods, .. } => {
                    mods_changed(game, mods);
                    game.mouse_wheel(lines, event_loop);
                }
                FromUi::Key { key, code, down, mods, .. } => {
                    mods_changed(game, mods);
                    if let Some(logical) = keys::dom_key(&key, &code) {
                        let state = if down { ElementState::Pressed } else { ElementState::Released };
                        let text = keys::typed(&logical).map(str::to_owned);
                        game.keyboard_input(logical, text.as_deref(), state, event_loop);
                    }
                }
                FromUi::Blur => {
                    // Keys and buttons released elsewhere never arrive.
                    if let crate::Mode::InGame(g) = &mut game.mode {
                        g.release_all();
                    }
                    game.modifiers_changed(ModifiersState::empty());
                }
                FromUi::Ready { .. } => unreachable!("handled above"),
            }
        }
    }
}

fn mods_changed(game: &mut Game, mods: acr_ui_protocol::Mods) {
    let Some(web) = game.webui.as_mut() else { return };
    if web.mods != mods {
        web.mods = mods;
        game.modifiers_changed(keys::modifiers(mods));
    }
}

fn pointer(game: &mut Game, event_loop: &ActiveEventLoop, phase: Phase, at: [f32; 2], button: Button) {
    let position = PhysicalPosition::new(f64::from(at[0]), f64::from(at[1]));
    let button = match button {
        Button::Left => MouseButton::Left,
        Button::Middle => MouseButton::Middle,
        Button::Right => MouseButton::Right,
    };
    match phase {
        Phase::Move => game.cursor_moved(position, event_loop),
        Phase::Down => {
            game.cursor_moved(position, event_loop);
            game.mouse_input(ElementState::Pressed, button, event_loop);
        }
        Phase::Up | Phase::Cancel => {
            game.cursor_moved(position, event_loop);
            game.mouse_input(ElementState::Released, button, event_loop);
        }
    }
}
