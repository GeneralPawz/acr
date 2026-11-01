//! The Svelte UI's side in the game (tracking issue #316, docs/webui.md).
//!
//! Rust keeps all UI state and logic. [`WebUi`] publishes what changed in the view model after
//! each frame's advance (one message at most, with a growing `rev`) and takes the UI's raw
//! input back ([`input`]); the UI draws the screens it claimed in its `Ready` message and the
//! engine stops drawing exactly those, as long as the UI can describe them ([`view`]).
//!
//! - [`bundle`]: the built UI, embedded in the executable.
//! - [`assets`]: pictures and fonts decoded from the player's files for the UI.
//! - [`font_atlas`]: glyph atlases of the original fonts. [`keys`]: DOM keys as winit keys.
//! - `web`: the browser's exports (wasm only).

pub mod assets;
pub mod bundle;
pub mod font_atlas;
pub mod input;
pub mod keys;
pub mod view;
#[cfg(target_arch = "wasm32")]
pub mod web;

use std::collections::HashMap;

use acr_ui_protocol::{Host, Mode, PROTOCOL, ScreenPatch, Stage, ToUi, ViewPatch, WidgetView, WidgetsPatch};
pub use assets::{AssetBody, AssetError, AssetReply};

use crate::canvas::Canvas;
use crate::ui::{External, Ui};

/// Delivers one JSON message to the UI.
pub type Sender = Box<dyn FnMut(&str)>;

#[cfg(target_arch = "wasm32")]
const HOST: Host = Host::Web;
#[cfg(not(target_arch = "wasm32"))]
const HOST: Host = Host::Native;

/// Messages to the UI that the test driver collects instead of a page (`--ui classic`).
pub type Outbox = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

/// The way to the UI of this platform, once the page connected; under the test driver the
/// collecting outbox.
fn sender(game: &crate::Game) -> Option<Sender> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = game;
        web::sender()
    }
    #[cfg(not(target_arch = "wasm32"))]
    game.ui_outbox.clone().map(|out| Box::new(move |m: &str| out.borrow_mut().push(m.to_owned())) as Sender)
}

/// What the UI was last told, to send only changes.
#[derive(Default)]
struct Sent {
    mode: Option<Mode>,
    loading: Option<Option<f32>>,
    stage: Option<Stage>,
    cursor: Option<Option<acr_ui_protocol::CursorView>>,
    /// Name, stack and flags of the screen.
    meta: Option<String>,
    engine_drawn: Option<bool>,
    widgets: HashMap<String, WidgetView>,
    order: Vec<String>,
}

/// An asset request waiting for files.
struct Waiting {
    path: String,
    reply: AssetReply,
}

/// Screens over the running game that the UI may draw, on hosts whose UI layer is transparent
/// over the game's canvas (the page of the browser build; a native webview that only draws opaque
/// full-screen menus could not, #333, #319). Every other `Screen::in_game` screen stays with the
/// engine even when the UI's registry names it.
pub const OVER_GAME: &[&str] = &["GameMenu", "GameOptions", "SaveGameDialog", "LoadGame"];

pub struct WebUi {
    send: Sender,
    /// The UI layer lies transparently over the game's canvas, so it can draw [`OVER_GAME`].
    transparent: bool,
    /// Screens the UI draws itself.
    claimed: Vec<String>,
    /// Every screen the view model can describe is the UI's (the test driver's classic skin).
    claim_all: bool,
    /// Why the last description failed on an asset (anything but files still loading).
    pub error: Option<String>,
    rev: u64,
    sent: Sent,
    resolver: view::Resolver,
    cache: assets::AssetCache,
    waiting: Vec<Waiting>,
    /// The page size in CSS pixels and the device pixel ratio.
    pub viewport: ([f32; 2], f32),
    /// Last modifier state forwarded, to report changes only.
    pub mods: acr_ui_protocol::Mods,
}

impl WebUi {
    /// A UI that announced `Ready` for `claimed`; answers with `Hello`.
    pub fn new(mut send: Sender, host: Host, claimed: Vec<String>, viewport: [f32; 2], dpr: f32) -> Self {
        let hello = ToUi::Hello { protocol: PROTOCOL, host, asset_base: String::new() };
        send(&serde_json::to_string(&hello).expect("hello serializes"));
        Self {
            send,
            transparent: host == Host::Web,
            claimed,
            claim_all: false,
            error: None,
            rev: 0,
            sent: Sent::default(),
            resolver: view::Resolver::default(),
            cache: assets::AssetCache::default(),
            waiting: Vec::new(),
            viewport: (viewport, dpr),
            mods: Default::default(),
        }
    }

    /// The UI draws every screen it can describe, whatever it announced.
    pub fn claim_everything(&mut self) {
        self.claim_all = true;
    }

    /// The UI wants everything again.
    pub fn resync(&mut self) {
        self.sent = Sent::default();
    }

    /// Queues an asset request; [`WebUi::pump_assets`] answers it.
    pub fn request_asset(&mut self, path: String, reply: AssetReply) {
        self.waiting.push(Waiting { path, reply });
    }

    /// Answers the asset requests whose files have arrived (and asks for the others).
    pub fn pump_assets(&mut self, ui: &Ui) {
        let (lib, fonts) = ui.assets();
        let waiting = std::mem::take(&mut self.waiting);
        for w in waiting {
            match assets::resolve(lib, fonts, &mut self.cache, &w.path) {
                Ok(assets::Resolved::Pending) => self.waiting.push(w),
                Ok(assets::Resolved::Ready(body)) => (w.reply)(Ok(body)),
                Err(e) => {
                    log::warn!("webui asset {}: {e}", w.path);
                    (w.reply)(Err(e));
                }
            }
        }
    }

    /// Resolves one asset path now, as the page's request would (the driver's classic check).
    pub fn check_asset(&mut self, ui: &Ui, path: &str) -> Result<assets::Resolved, AssetError> {
        let (lib, fonts) = ui.assets();
        assets::resolve(lib, fonts, &mut self.cache, path)
    }

    /// Whether the files of any waiting request are still to come (the frame loop keeps asking).
    pub fn has_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Whether the UI draws `screen`: the driver's classic UI draws all it can describe, a page the
    /// ones its registry names. A screen over the game lies on the game's canvas, which the engine
    /// goes on drawing underneath: the UI draws it only on a transparent host and only the
    /// [`OVER_GAME`] ones.
    fn draws(&self, screen: &view::ScreenView) -> bool {
        self.claim_all
            || (self.claimed.contains(&screen.name)
                && (!screen.in_game || (self.transparent && OVER_GAME.contains(&screen.name.as_str()))))
    }

    /// Tells the engine which screens to leave to the UI, and the UI what changed. Call after
    /// the frame's advance, before it is drawn.
    pub fn publish(&mut self, ui: &mut Ui, mode: Mode, loading: Option<f32>, frame: Option<[f32; 2]>) {
        acr_sim::prof_scope!("ui.bridge");
        let mut patch = ViewPatch { rev: self.rev + 1, ..Default::default() };
        let mut changed = false;

        if self.sent.mode != Some(mode) {
            self.sent.mode = Some(mode);
            patch.mode = Some(mode);
            changed = true;
        }
        if self.sent.loading != Some(loading) {
            self.sent.loading = Some(loading);
            patch.loading = Some(loading);
            changed = true;
        }
        if let Some(size) = frame {
            let stage =
                Stage { menu: view::canvas_view(Canvas::fit(size)), game: view::canvas_view(Canvas::screen(size)) };
            if self.sent.stage != Some(stage) {
                self.sent.stage = Some(stage);
                patch.stage = Some(stage);
                changed = true;
            }
        }

        // The screen: described by the UI if it claimed it and the description is complete.
        let described = match view::screen(&mut self.resolver, ui) {
            Ok(Some(screen)) if self.draws(&screen) => {
                self.error = None;
                Some(screen)
            }
            Ok(_) => None,
            Err(view::Failure::Pending) => None,
            Err(e) => {
                log::error!("webui: the engine draws the screen: {e:?}");
                // A screen the protocol cannot express stays with the engine; a broken asset
                // is a defect the driver reports.
                self.error = matches!(e, view::Failure::Asset(_)).then(|| format!("{e:?}"));
                None
            }
        };
        let drawn_here: Vec<String> = described.iter().map(|s| s.name.clone()).collect();
        ui.set_external(Some(External { screens: drawn_here, cursor: true }));
        if let Some(screen) = self.screen_patch(described, ui) {
            patch.screen = Some(screen);
            changed = true;
        }

        match self.resolver.cursor(ui) {
            Ok(cursor) if self.sent.cursor.as_ref() != Some(&cursor) => {
                self.sent.cursor = Some(cursor.clone());
                patch.cursor = Some(cursor);
                changed = true;
            }
            Ok(_) | Err(view::Failure::Pending) => {}
            Err(e) => log::error!("webui: cursor: {e:?}"),
        }

        if changed {
            self.rev += 1;
            (self.send)(&serde_json::to_string(&ToUi::View(patch)).expect("view serializes"));
        }
    }

    /// The screen topic if it changed since the last patch.
    fn screen_patch(&mut self, described: Option<view::ScreenView>, ui: &Ui) -> Option<ScreenPatch> {
        let engine_drawn = described.is_none();
        let (name, stack, in_game, busy, hint, widgets) = match described {
            Some(s) => (s.name, s.stack, s.in_game, s.busy, s.hint, s.widgets),
            None => {
                let top = ui.shown_screen();
                (
                    top.map_or(String::new(), |s| s.name().to_owned()),
                    ui.screens().map(|(s, _)| s.name().to_owned()).collect(),
                    top.is_some_and(|s| s.in_game()),
                    top.is_some_and(|s| s.busy()),
                    None,
                    Vec::new(),
                )
            }
        };
        let meta = format!("{name}|{stack:?}|{in_game}|{busy}|{hint:?}");
        let order: Vec<String> = widgets.iter().map(|w| w.id.clone()).collect();
        let upsert: Vec<WidgetView> =
            widgets.iter().filter(|w| self.sent.widgets.get(&w.id) != Some(*w)).cloned().collect();
        let order_changed = order != self.sent.order;
        let identity_changed = self.sent.engine_drawn != Some(engine_drawn)
            || self.sent.meta.as_ref().map(|m| m.split('|').next()) != Some(Some(name.as_str()));
        if self.sent.meta.as_deref() == Some(meta.as_str())
            && self.sent.engine_drawn == Some(engine_drawn)
            && upsert.is_empty()
            && !order_changed
        {
            return None;
        }
        let widgets_patch = if identity_changed {
            WidgetsPatch::Full { widgets: widgets.clone() }
        } else {
            WidgetsPatch::Changed { upsert, order: order_changed.then(|| order.clone()) }
        };
        self.sent.meta = Some(meta);
        self.sent.engine_drawn = Some(engine_drawn);
        self.sent.order = order;
        self.sent.widgets = widgets.into_iter().map(|w| (w.id.clone(), w)).collect();
        Some(ScreenPatch { name, stack, in_game, busy, engine_drawn, hint, widgets: widgets_patch })
    }
}

/// The asset paths (below `/asset/v1/`) a patch's widgets and cursor refer to: what the page
/// would fetch to draw it. Sprite urls are already paths; a text part needs its font's atlas.
pub fn referenced(patch: &ViewPatch) -> Vec<String> {
    use acr_ui_protocol::Part;
    let mut out = Vec::new();
    let mut part = |p: &Part| match p {
        Part::Sprite { url, .. } => out.push(url.clone()),
        Part::Text { font, set, .. } => {
            out.push(format!("font/{set}/{font}.png"));
            out.push(format!("font/{set}/{font}.json"));
        }
        _ => {}
    };
    if let Some(screen) = &patch.screen {
        let widgets = match &screen.widgets {
            WidgetsPatch::Full { widgets } => widgets,
            WidgetsPatch::Changed { upsert, .. } => upsert,
        };
        widgets.iter().flat_map(|w| &w.parts).for_each(&mut part);
    }
    if let Some(Some(cursor)) = &patch.cursor {
        out.push(cursor.url.clone());
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn ui_with_menu() -> Ui {
        Ui::new(&[])
    }

    #[test]
    fn hello_goes_out_first_and_unbuilt_screens_stay_with_the_engine() {
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = log.clone();
        let mut web = WebUi::new(
            Box::new(move |m| sink.borrow_mut().push(m.to_owned())),
            Host::Web,
            vec!["MainMenu".into()],
            [1024.0, 768.0],
            1.0,
        );
        assert!(log.borrow()[0].starts_with(r#"{"t":"hello","protocol":1,"host":"web""#));
        let mut ui = ui_with_menu();
        web.publish(&mut ui, Mode::Menus, None, Some([1024.0, 768.0]));
        web.publish(&mut ui, Mode::Menus, None, Some([1024.0, 768.0]));
        let log = log.borrow();
        // One patch: the second publish found nothing changed.
        assert_eq!(log.len(), 2, "{log:?}");
        let ToUi::View(patch) = serde_json::from_str(&log[1]).unwrap() else { panic!() };
        assert_eq!((patch.rev, patch.mode), (1, Some(Mode::Menus)));
        assert_eq!(patch.stage.unwrap().menu.scale, 1.0);
        // Nothing is built yet: no screen to describe.
        let screen = patch.screen.unwrap();
        assert!(screen.engine_drawn && screen.name.is_empty());
    }

    #[test]
    fn a_patch_refers_to_its_sprites_fonts_and_cursor() {
        use acr_ui_protocol::{CursorView, Part, WidgetKind};
        let widget = |parts| WidgetView {
            id: "S/0".into(),
            kind: WidgetKind::Button,
            rect: [0; 4],
            enabled: true,
            hover: false,
            parts,
        };
        let sprite = |url: &str| Part::Sprite { url: url.into(), x: 0, y: 0, w: 1, h: 1, alpha: 1.0, clip: None };
        let text = Part::Text { font: "fonG60w".into(), set: 1, text: "a".into(), x: 0, y: 0, fade: 0, clip: None };
        let patch = ViewPatch {
            rev: 1,
            screen: Some(ScreenPatch {
                name: "S".into(),
                stack: vec![],
                in_game: false,
                busy: false,
                engine_drawn: false,
                hint: None,
                widgets: WidgetsPatch::Full {
                    widgets: vec![widget(vec![sprite("bmp/a"), text.clone(), sprite("bmp/a")])],
                },
            }),
            cursor: Some(Some(CursorView { url: "frame/menu/0/C.GP/0".into(), hotspot: [0, 0] })),
            ..Default::default()
        };
        assert_eq!(referenced(&patch), ["bmp/a", "font/1/fonG60w.json", "font/1/fonG60w.png", "frame/menu/0/C.GP/0"]);
        assert!(referenced(&ViewPatch::default()).is_empty());
    }

    #[test]
    fn screens_over_the_game_are_the_uis_only_on_a_transparent_host_and_when_listed() {
        let screen = |name: &str, in_game| view::ScreenView {
            name: name.into(),
            stack: vec![],
            in_game,
            busy: false,
            hint: None,
            widgets: vec![],
        };
        let claimed = || vec!["LoadGame".to_owned(), "GameMenu".to_owned(), "Elsewhere".to_owned()];
        let mut web = WebUi::new(Box::new(|_| {}), Host::Web, claimed(), [1.0; 2], 1.0);
        // The main menu's load dialog and the game menu's, but not an unlisted screen over the game.
        assert!(web.draws(&screen("LoadGame", false)) && web.draws(&screen("LoadGame", true)));
        assert!(web.draws(&screen("GameMenu", true)));
        assert!(!web.draws(&screen("Elsewhere", true)) && web.draws(&screen("Elsewhere", false)));
        // Named screens only: the registry is the UI's say.
        assert!(!web.draws(&screen("HistoryWindow", true)));
        // A host that cannot draw over the canvas keeps them with the engine.
        let native = WebUi::new(Box::new(|_| {}), Host::Native, claimed(), [1.0; 2], 1.0);
        assert!(native.draws(&screen("LoadGame", false)) && !native.draws(&screen("LoadGame", true)));
        assert!(!native.draws(&screen("GameMenu", true)));
        web.claim_everything();
        assert!(web.draws(&screen("Elsewhere", true)));
    }

    #[test]
    fn claiming_everything_describes_screens_the_ui_did_not_name() {
        let mut web = WebUi::new(Box::new(|_| {}), Host::Web, vec![], [1.0; 2], 1.0);
        assert!(!web.claim_all);
        web.claim_everything();
        assert!(web.claim_all && web.error.is_none());
    }

    #[test]
    fn resync_sends_everything_again_with_a_higher_rev() {
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = log.clone();
        let mut web =
            WebUi::new(Box::new(move |m| sink.borrow_mut().push(m.to_owned())), Host::Web, vec![], [1.0; 2], 1.0);
        let mut ui = ui_with_menu();
        web.publish(&mut ui, Mode::Intro, None, None);
        web.resync();
        web.publish(&mut ui, Mode::Intro, None, None);
        let revs: Vec<u64> = log
            .borrow()
            .iter()
            .skip(1)
            .map(|m| match serde_json::from_str(m).unwrap() {
                ToUi::View(p) => p.rev,
                _ => 0,
            })
            .collect();
        assert_eq!(revs, vec![1, 2]);
    }
}
