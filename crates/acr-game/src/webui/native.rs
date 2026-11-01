//! The native host of the Svelte UI on Linux (feature `webview-gtk`, docs/webui.md "Native host").
//!
//! One opaque wry child webview (WebKitGTK) covers the whole window, over the swapchain, and is
//! shown only while the UI draws the current screen ([`super::WebUi::shown`]): a full-screen
//! menu. Everything else, the HUD and the dialogs over the game, the engine draws; the spike
//! (#456) found that a transparent child webview cannot show the frame under it.
//!
//! - The page and its scripts come from the embedded bundle ([`super::bundle`]) through the
//!   custom protocol `acr://localhost/`; assets from `/asset/v1/<path>`, answered by
//!   [`super::assets::resolve`] in the game's thread as `Event::UiAsset`.
//! - Messages from the UI arrive over the IPC handler as `Event::Ui`, the same events the browser
//!   build sends; messages to the UI are `evaluate_script` calls of `window.__acr_ui.recv`.
//! - wry child webviews need X11, so the window runs on XWayland under Wayland; GTK is pumped
//!   from the event loop's `about_to_wait` ([`pump`]).

use std::rc::Rc;

use winit::event_loop::{EventLoop, EventLoopProxy};
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::window::Window;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::http::{Request, Response, StatusCode};
use wry::{Rect, RequestAsyncResponder, WebView, WebViewBuilder};

use super::{AssetBody, Sender, bundle};
use crate::Event;

/// Where the page is served.
const ORIGIN: &str = "acr://localhost";
/// The path prefix of the assets (`ui/src/lib/bridge.ts` `nativeBridge`).
const ASSET_PREFIX: &str = "/asset/v1/";

/// Makes GTK and X11 ready; call before the event loop exists.
pub fn prepare() -> Result<(), String> {
    gtk::gdk::set_allowed_backends("x11");
    gtk::init().map_err(|e| format!("GTK: {e}"))?;
    // wry's X11 child window raises extension errors (170; GLXBadWindow, 168, on XWayland) that
    // winit would treat as fatal; only those above the core codes are dropped.
    winit::platform::x11::register_xlib_error_hook(Box::new(|_display, error| {
        let error = error as *mut x11_dl::xlib::XErrorEvent;
        // SAFETY: winit passes a valid XErrorEvent for the duration of the call.
        let (code, request) = unsafe { ((*error).error_code, (*error).request_code) };
        let ignored = code >= 128;
        log::debug!("webui: X error code={code} request={request} ignored={ignored}");
        ignored
    }));
    Ok(())
}

/// The event loop, on X11 (after [`prepare`]).
pub fn event_loop() -> Result<EventLoop<Event>, String> {
    EventLoop::<Event>::with_user_event().with_x11().build().map_err(|e| e.to_string())
}

/// Runs the pending GTK events (the webview's input, scripts and IPC).
pub fn pump() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
}

/// The webview over the window.
pub struct Host {
    webview: Rc<WebView>,
    visible: bool,
}

impl Host {
    /// Creates the (hidden) webview as a child of `window`; the page loads and announces itself
    /// with `ready` over the IPC handler.
    pub fn new(window: &Window, proxy: EventLoopProxy<Event>) -> Result<Self, String> {
        let ipc = proxy.clone();
        let webview = WebViewBuilder::new()
            .with_background_color((0, 0, 0, 255))
            .with_visible(false)
            .with_bounds(bounds(window))
            .with_asynchronous_custom_protocol("acr".into(), move |_id, request, responder| {
                serve(&proxy, request, responder)
            })
            .with_ipc_handler(move |request| {
                let _ = ipc.send_event(Event::Ui(request.into_body()));
            })
            .with_url(format!("{ORIGIN}/index.html"))
            .build_as_child(window)
            .map_err(|e| format!("webview: {e}"))?;
        // wry maps the child window of a webview that starts hidden anyway, and needs the
        // alternating calls to hide it for real (a comment in `wry::webkitgtk` says so).
        for visible in [true, false] {
            webview.set_visible(visible).map_err(|e| format!("webview: {e}"))?;
        }
        Ok(Self { webview: Rc::new(webview), visible: false })
    }

    /// The way to the page: one `evaluate_script` per message.
    pub fn sender(&self) -> Sender {
        let webview = self.webview.clone();
        Box::new(move |json| {
            // The JSON text goes in as a JS string literal; the page parses it.
            let literal = serde_json::to_string(json).expect("a string serializes");
            if let Err(e) = webview.evaluate_script(&format!("window.__acr_ui&&window.__acr_ui.recv({literal})")) {
                log::warn!("webui: evaluate_script failed: {e}");
            }
        })
    }

    /// Whether the webview is shown now.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Shows or hides the webview; input follows it (the page while shown, else the window).
    pub fn show(&mut self, show: bool) {
        if show == self.visible {
            return;
        }
        self.visible = show;
        log::info!("webui: the webview is {}", if show { "shown" } else { "hidden" });
        let moved = self
            .webview
            .set_visible(show)
            .and_then(|()| if show { self.webview.focus() } else { self.webview.focus_parent() });
        if let Err(e) = moved {
            log::warn!("webui: cannot {} the webview: {e}", if show { "show" } else { "hide" });
        }
    }

    /// The window changed size or scale: the webview covers it again.
    pub fn resize(&self, window: &Window) {
        if let Err(e) = self.webview.set_bounds(bounds(window)) {
            log::warn!("webui: cannot resize the webview: {e}");
        }
    }
}

/// The whole window, in logical pixels.
fn bounds(window: &Window) -> Rect {
    let size = window.inner_size().to_logical::<u32>(window.scale_factor());
    Rect { position: LogicalPosition::new(0, 0).into(), size: LogicalSize::new(size.width, size.height).into() }
}

fn serve(proxy: &EventLoopProxy<Event>, request: Request<Vec<u8>>, responder: RequestAsyncResponder) {
    let path = request.uri().path();
    if let Some(asset) = path.strip_prefix(ASSET_PREFIX) {
        let reply: super::AssetReply = Box::new(move |result| {
            responder.respond(match result {
                Ok(body) => {
                    let mime = body.mime();
                    let bytes = match body {
                        AssetBody::Png(bytes) => bytes,
                        AssetBody::Json(text) => text.into_bytes(),
                    };
                    response(StatusCode::OK, mime, bytes, None)
                }
                Err(e) => response(
                    StatusCode::from_u16(e.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                    "text/plain",
                    Vec::new(),
                    Some(&e.to_string()),
                ),
            });
        });
        if let Err(closed) = proxy.send_event(Event::UiAsset { path: asset.to_owned(), reply }) {
            // The event loop is gone: the game is closing.
            if let Event::UiAsset { reply, .. } = closed.0 {
                reply(Err(super::AssetError::NotFound("the game is closing".into())));
            }
        }
        return;
    }
    let name = match path.trim_start_matches('/') {
        "" => "index.html",
        other => other,
    };
    responder.respond(match bundle::file(name) {
        Some(bytes) => response(StatusCode::OK, mime(name), bytes.to_vec(), None),
        None => response(StatusCode::NOT_FOUND, "text/plain", Vec::new(), Some(&format!("not in the bundle: {name}"))),
    });
}

fn response(
    status: StatusCode,
    mime: &str,
    body: Vec<u8>,
    error: Option<&str>,
) -> Response<std::borrow::Cow<'static, [u8]>> {
    let mut builder =
        Response::builder().status(status).header("Content-Type", mime).header("Cache-Control", "no-cache");
    if let Some(error) = error {
        // Header values are visible ASCII: the message may not be.
        let ascii: String = error.chars().map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' }).collect();
        builder = builder.header("x-acr-error", ascii);
    }
    builder.body(body.into()).expect("a valid response")
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_files_get_their_type() {
        assert_eq!(mime("index.html"), "text/html");
        assert_eq!(mime("acr-ui.js"), "text/javascript");
        assert_eq!(mime("acr-ui.css"), "text/css");
        assert_eq!(mime("other.bin"), "application/octet-stream");
    }

    #[test]
    fn an_error_travels_as_status_and_ascii_header() {
        let r = response(StatusCode::NOT_FOUND, "text/plain", Vec::new(), Some("fehlt: \u{e4}"));
        assert_eq!(r.status(), 404);
        assert_eq!(r.headers()["x-acr-error"], "fehlt: ?");
    }
}
