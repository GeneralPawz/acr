//! The browser's side of the bridge: exports the page calls (docs/webui.md "The bridge").
//!
//! - `acr_ui_connect(callback)`: the UI's receiver of JSON messages.
//! - `acr_ui_send(json)`: a message from the UI; it reaches the game as `Event::Ui`.
//! - `acr_ui_asset(path)`: a `Promise` of the bytes of an asset (`webui::assets`).
//! - `acr_audio_unlock()`: starts the audio inside the user gesture.

use std::cell::RefCell;

use wasm_bindgen::prelude::*;
use winit::event_loop::EventLoopProxy;

use super::Sender;
use crate::Event;

thread_local! {
    static PROXY: RefCell<Option<EventLoopProxy<Event>>> = const { RefCell::new(None) };
    static CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

/// Lets the exports reach the game: a clone of its proxy.
pub fn set_proxy(proxy: EventLoopProxy<Event>) {
    PROXY.with_borrow_mut(|p| *p = Some(proxy));
}

/// The sender into the callback of `acr_ui_connect`; `None` before the page connected.
pub fn sender() -> Option<Sender> {
    let callback = CALLBACK.with_borrow(Clone::clone)?;
    Some(Box::new(move |json| {
        if let Err(e) = callback.call1(&JsValue::NULL, &JsValue::from_str(json)) {
            log::warn!("webui: the UI callback failed: {e:?}");
        }
    }))
}

fn send(event: Event) {
    PROXY.with_borrow(|p| {
        if let Some(p) = p {
            let _ = p.send_event(event);
        }
    });
}

/// The UI receives the game's messages (JSON strings) through `callback`.
#[wasm_bindgen]
pub fn acr_ui_connect(callback: js_sys::Function) {
    CALLBACK.with_borrow_mut(|c| *c = Some(callback));
}

/// A message from the UI (JSON).
#[wasm_bindgen]
pub fn acr_ui_send(json: String) {
    send(Event::Ui(json));
}

/// The bytes of an asset, as a `Promise<Uint8Array>` that rejects with the error.
#[wasm_bindgen]
pub fn acr_ui_asset(path: String) -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, reject| {
        let reply: super::AssetReply = Box::new(move |result| {
            let _ = match result {
                Ok(body) => {
                    let bytes = match body {
                        super::AssetBody::Png(bytes) => bytes,
                        super::AssetBody::Json(text) => text.into_bytes(),
                    };
                    resolve.call1(&JsValue::NULL, &js_sys::Uint8Array::from(bytes.as_slice()))
                }
                Err(e) => reject.call1(&JsValue::NULL, &JsValue::from_str(&format!("{} {e}", e.status()))),
            };
        });
        send(Event::UiAsset { path: path.clone(), reply });
    })
}

/// Starts the audio; the page calls it inside the pointer or key event of the gesture.
#[wasm_bindgen]
pub fn acr_audio_unlock() {
    send(Event::AudioUnlock);
}
