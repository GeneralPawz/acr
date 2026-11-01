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

use acr_ui_protocol::{
    Host, HudGroup, HudPatch, Mode, PROTOCOL, ScreenPatch, Stage, ToUi, ViewPatch, WidgetView, WidgetsPatch,
};
pub use assets::{AssetBody, AssetError, AssetReply};

use crate::canvas::Canvas;
use crate::hud::{Claim, Claimed};
use crate::ui::draw::Draw;
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
    /// The groups of the in-game interface, with the draws they were made from; `None` until the
    /// first patch of a connection (or a resync), which then carries the topic even if empty.
    hud: Option<HashMap<String, SentGroup>>,
}

/// A group of the interface as the UI has it: the draws it was made from (at its last full send),
/// the shift since, and what was sent.
struct SentGroup {
    draws: Vec<Draw>,
    /// The versions of the dynamic images the draws show.
    images: Vec<u32>,
    offset: [i32; 2],
    group: HudGroup,
}

/// The shift that takes `old` to `new`, if `new` is `old` moved as a whole (a tooltip that follows
/// the mouse): the same sprites, sizes and opacity, every position and every clip window of its own
/// moved alike (a clip that covers everything stays).
fn shift(old: &[Draw], new: &[Draw]) -> Option<[i32; 2]> {
    if old.len() != new.len() {
        return None;
    }
    let (Some(a), Some(b)) = (old.first(), new.first()) else { return Some([0, 0]) };
    let d = [b.x - a.x, b.y - a.y];
    let same = old.iter().zip(new).all(|(o, n)| {
        o.sprite == n.sprite
            && o.size == n.size
            && o.alpha == n.alpha
            && [n.x - o.x, n.y - o.y] == d
            && (o.clip == n.clip || o.clone().translated(d[0], d[1]).clip == n.clip)
    });
    same.then_some(d)
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
pub const OVER_GAME: &[&str] =
    &["GameMenu", "GameOptions", "SaveGameDialog", "LoadGame", "MessageWindow", "ChoiceWindow", "HistoryWindow"];

/// The parts of the in-game interface the UI can claim (`Ready::hud`), over the same transparent
/// canvas as [`OVER_GAME`].
pub const HUD_PARTS: &[&str] =
    &["TopBar", "DropDown", "Hints", "Messages", "Minimap", "CommandPanel", "UnitStats", "ArmyList", "GamePanel"];

/// The minimap image changes with every unit that moves; the UI gets a new one at most this often
/// (the camera rectangle is a group of its own and follows every frame).
const MINIMAP_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

pub struct WebUi {
    send: Sender,
    /// The UI layer lies transparently over the game's canvas, so it can draw [`OVER_GAME`].
    transparent: bool,
    /// Screens the UI draws itself.
    claimed: Vec<String>,
    /// Parts of the in-game interface the UI draws itself ([`HUD_PARTS`]).
    hud: Vec<String>,
    /// Every screen the view model can describe is the UI's (the test driver's classic skin).
    claim_all: bool,
    /// Dynamic images (minimap, portrait) the engine made and the UI has not been given yet.
    pending_images: HashMap<&'static str, acr_formats::raster::Image>,
    minimap_at: Option<web_time::Instant>,
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
    pub fn new(
        mut send: Sender,
        host: Host,
        claimed: Vec<String>,
        hud: Vec<String>,
        viewport: [f32; 2],
        dpr: f32,
    ) -> Self {
        let hello = ToUi::Hello { protocol: PROTOCOL, host, asset_base: String::new() };
        send(&serde_json::to_string(&hello).expect("hello serializes"));
        Self {
            send,
            transparent: host == Host::Web,
            claimed,
            hud,
            claim_all: false,
            pending_images: HashMap::new(),
            minimap_at: None,
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

    /// The parts of the interface the engine leaves to the UI: the ones the UI named, on a host
    /// whose UI layer is transparent over the canvas, at a whole canvas `scale` (the driver's
    /// classic skin takes all). At a fractional scale the engine's straight-alpha filtering
    /// darkens sprite and text edges that the browser composites correctly, so the two differ on
    /// every edge (#464); until that is fixed the engine keeps the HUD there.
    pub fn hud_claim(&self, scale: f32) -> Claim {
        let whole = scale.fract() == 0.0;
        let has = |part: &str| self.claim_all || (self.transparent && whole && self.hud.iter().any(|p| p == part));
        Claim {
            top_bar: has("TopBar"),
            drop_down: has("DropDown"),
            hints: has("Hints"),
            messages: has("Messages"),
            minimap: has("Minimap"),
            commands: has("CommandPanel"),
            stats: has("UnitStats"),
            army: has("ArmyList"),
            game_panel: has("GamePanel"),
        }
    }

    /// An image the engine drew into a texture (the minimap, the portrait), for the UI to fetch as
    /// `image/<name>/<version>`. The minimap gets a new version at most four times a second
    /// ([`MINIMAP_EVERY`]; the driver's classic skin takes every one), the others at once.
    pub fn set_image(&mut self, name: &'static str, image: acr_formats::raster::Image) {
        self.pending_images.insert(name, image);
    }

    fn adopt_images(&mut self) {
        let now = web_time::Instant::now();
        let due = self.claim_all || self.minimap_at.is_none_or(|t| now.duration_since(t) >= MINIMAP_EVERY);
        for name in self.pending_images.keys().copied().collect::<Vec<_>>() {
            let slow = name == crate::hud::MINIMAP_IMAGE;
            if slow && !due {
                continue;
            }
            if slow {
                self.minimap_at = Some(now);
            }
            let image = self.pending_images.remove(name).expect("listed above");
            let version = self.resolver.image_versions.entry(name).or_insert(0);
            *version += 1;
            self.cache.add_image(name, *version, image);
        }
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
    pub fn publish(
        &mut self,
        ui: &mut Ui,
        mode: Mode,
        loading: Option<f32>,
        frame: Option<[f32; 2]>,
        hud: Vec<Claimed>,
    ) {
        acr_sim::prof_scope!("ui.bridge");
        let mut patch = ViewPatch { rev: self.rev + 1, ..Default::default() };
        let mut changed = false;
        self.adopt_images();

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
                patch.stage = Some(Box::new(stage));
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

        match self.hud_patch(ui, hud) {
            Ok(Some(hud)) => {
                patch.hud = Some(Box::new(hud));
                changed = true;
            }
            // Files still loading: the groups stay as they were.
            Ok(None) | Err(view::Failure::Pending) => {}
            Err(e) => {
                log::error!("webui: the interface: {e:?}");
                if matches!(e, view::Failure::Asset(_)) {
                    self.error = Some(format!("{e:?}"));
                }
            }
        }

        if changed {
            self.rev += 1;
            (self.send)(&serde_json::to_string(&ToUi::View(patch)).expect("view serializes"));
        }
    }

    /// The interface topic: the groups whose draws changed, and the ones that are gone. Draws are
    /// compared before they are resolved to parts, which is the costly step.
    fn hud_patch(&mut self, ui: &Ui, groups: Vec<Claimed>) -> Result<Option<HudPatch>, view::Failure> {
        let (lib, fonts) = ui.assets();
        let first = self.sent.hud.is_none();
        let sent = self.sent.hud.get_or_insert_with(HashMap::new);
        let mut patch = HudPatch::default();
        let mut updates = Vec::new();
        let mut names = Vec::with_capacity(groups.len());
        let mut moves = Vec::new();
        for g in groups {
            names.push(g.name);
            let images = self.resolver.image_stamp(&g.draws);
            if let Some(old) = sent.get(g.name).filter(|o| o.group.scale == g.zoom.scale && o.images == images)
                && let Some(d) = shift(&old.draws, &g.draws)
                && (g.zoom.scale == 1.0
                    || (0..2).all(|i| (g.zoom.anchor[i] - old.group.anchor[i] - d[i] as f32).abs() < 1e-3))
            {
                if d != old.offset {
                    moves.push((g.name, d));
                }
                continue;
            }
            let parts = self.resolver.parts(lib, fonts, &g.draws)?;
            let group = HudGroup {
                name: g.name.to_owned(),
                layer: g.layer,
                anchor: g.zoom.anchor,
                scale: g.zoom.scale,
                offset: [0, 0],
                parts,
            };
            patch.groups.push(group.clone());
            updates.push((g.name, g.draws, images, group));
        }
        // Nothing is recorded unless every group could be made: a retry sends them all.
        for (name, draws, images, group) in updates {
            sent.insert(name.to_owned(), SentGroup { draws, images, offset: [0, 0], group });
        }
        for (name, offset) in moves {
            if let Some(old) = sent.get_mut(name) {
                old.offset = offset;
                patch.moved.push(acr_ui_protocol::HudMove { name: name.to_owned(), offset });
            }
        }
        patch.removed = sent.keys().filter(|k| !names.contains(&k.as_str())).cloned().collect();
        patch.removed.sort();
        for name in &patch.removed {
            sent.remove(name);
        }
        Ok((first || !patch.groups.is_empty() || !patch.moved.is_empty() || !patch.removed.is_empty()).then_some(patch))
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

/// Drops the dynamic images (`image/<name>/<version>`) that a newer version of the same name
/// replaced: the page drops those patches' pictures unseen, and the cache keeps only the last few.
pub fn newest_images(paths: &mut Vec<String>) {
    let version = |p: &str| {
        p.strip_prefix("image/")
            .and_then(|r| r.rsplit_once('/'))
            .map(|(n, v)| (n.to_owned(), v.parse::<u32>().unwrap_or(0)))
    };
    let mut newest: HashMap<String, u32> = HashMap::new();
    for (n, v) in paths.iter().filter_map(|p| version(p)) {
        let e = newest.entry(n).or_default();
        *e = (*e).max(v);
    }
    paths.retain(|p| version(p).is_none_or(|(n, v)| newest[&n] == v));
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
    if let Some(hud) = &patch.hud {
        hud.groups.iter().flat_map(|g| &g.parts).for_each(&mut part);
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
            vec![],
            [1024.0, 768.0],
            1.0,
        );
        assert!(log.borrow()[0].starts_with(r#"{"t":"hello","protocol":1,"host":"web""#));
        let mut ui = ui_with_menu();
        web.publish(&mut ui, Mode::Menus, None, Some([1024.0, 768.0]), Vec::new());
        web.publish(&mut ui, Mode::Menus, None, Some([1024.0, 768.0]), Vec::new());
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
        let mut web = WebUi::new(Box::new(|_| {}), Host::Web, claimed(), vec![], [1.0; 2], 1.0);
        // The main menu's load dialog and the game menu's, but not an unlisted screen over the game.
        assert!(web.draws(&screen("LoadGame", false)) && web.draws(&screen("LoadGame", true)));
        assert!(web.draws(&screen("GameMenu", true)));
        assert!(!web.draws(&screen("Elsewhere", true)) && web.draws(&screen("Elsewhere", false)));
        // Named screens only: the registry is the UI's say.
        assert!(!web.draws(&screen("HistoryWindow", true)));
        // A host that cannot draw over the canvas keeps them with the engine.
        let native = WebUi::new(Box::new(|_| {}), Host::Native, claimed(), vec![], [1.0; 2], 1.0);
        assert!(native.draws(&screen("LoadGame", false)) && !native.draws(&screen("LoadGame", true)));
        assert!(!native.draws(&screen("GameMenu", true)));
        web.claim_everything();
        assert!(web.draws(&screen("Elsewhere", true)));
    }

    #[test]
    fn claiming_everything_describes_screens_the_ui_did_not_name() {
        let mut web = WebUi::new(Box::new(|_| {}), Host::Web, vec![], vec![], [1.0; 2], 1.0);
        assert!(!web.claim_all);
        web.claim_everything();
        assert!(web.claim_all && web.error.is_none());
    }

    #[test]
    fn the_interface_goes_out_by_group_and_only_what_changed() {
        use crate::hud::font;
        use crate::hud::zoom::Zoom;
        let mut web = WebUi::new(Box::new(|_| {}), Host::Web, vec![], vec!["TopBar".into()], [1.0; 2], 1.0);
        let ui = ui_with_menu();
        let group = |name, layer, draws| Claimed { name, layer, zoom: Zoom::NONE, draws };
        let bar = || group("top_bar", 0, vec![Draw::fill([1, 2, 3, 255], crate::ui::draw::Rect::new(0, 0, 9, 9))]);
        let text = |s: &[u8]| group("top_bar_text", 1, vec![Draw::game_text(font::YELLOW, s, 5, 7)]);
        // The first patch carries the topic whatever it holds.
        let p = web.hud_patch(&ui, Vec::new()).unwrap().unwrap();
        assert!(p.groups.is_empty() && p.removed.is_empty());
        assert!(web.hud_patch(&ui, Vec::new()).unwrap().is_none());
        let p = web.hud_patch(&ui, vec![bar(), text(b"10")]).unwrap().unwrap();
        assert_eq!(p.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["top_bar", "top_bar_text"]);
        // A changed number resends the number only.
        assert!(web.hud_patch(&ui, vec![bar(), text(b"10")]).unwrap().is_none());
        let p = web.hud_patch(&ui, vec![bar(), text(b"11")]).unwrap().unwrap();
        assert_eq!(p.groups.len(), 1);
        assert_eq!(p.groups[0].name, "top_bar_text");
        assert!(matches!(&p.groups[0].parts[0], acr_ui_protocol::Part::Text { text, x: 5, y: 7, .. } if text == "11"));
        // A group that is gone is removed; a resync sends the topic again, with everything.
        let p = web.hud_patch(&ui, vec![bar()]).unwrap().unwrap();
        assert_eq!((p.groups.len(), p.removed.as_slice()), (0, ["top_bar_text".to_owned()].as_slice()));
        web.resync();
        let p = web.hud_patch(&ui, vec![bar()]).unwrap().unwrap();
        assert_eq!(p.groups.len(), 1);
    }

    #[test]
    fn a_new_image_resends_a_group_with_the_same_draws() {
        use crate::hud::MINIMAP_IMAGE;
        use crate::hud::zoom::Zoom;
        use crate::ui::draw::Rect;
        let mut web = WebUi::new(Box::new(|_| {}), Host::Web, vec![], vec![], [1.0; 2], 1.0);
        let ui = ui_with_menu();
        let map = || Claimed {
            name: "minimap",
            layer: 0,
            zoom: Zoom::NONE,
            draws: vec![Draw::image(MINIMAP_IMAGE, Rect::new(767, 512, 240, 240))],
        };
        let image = |v: u8| acr_formats::raster::Image { width: 1, height: 1, rgba: vec![v, 0, 0, 255] };
        // No image yet: the group waits.
        assert!(matches!(web.hud_patch(&ui, vec![map()]), Err(view::Failure::Pending)));
        web.set_image(MINIMAP_IMAGE, image(1));
        web.adopt_images();
        let p = web.hud_patch(&ui, vec![map()]).unwrap().unwrap();
        let acr_ui_protocol::Part::Sprite { url, w, h, .. } = &p.groups[0].parts[0] else { panic!() };
        assert_eq!((url.as_str(), *w, *h), ("image/hud%3Aminimap/1", 240, 240));
        assert!(web.hud_patch(&ui, vec![map()]).unwrap().is_none());
        // The images come at most four times a second: the next one waits.
        web.set_image(MINIMAP_IMAGE, image(2));
        web.adopt_images();
        assert!(web.hud_patch(&ui, vec![map()]).unwrap().is_none());
        web.minimap_at = web.minimap_at.map(|t| t - MINIMAP_EVERY);
        web.adopt_images();
        let p = web.hud_patch(&ui, vec![map()]).unwrap().unwrap();
        assert!(
            matches!(&p.groups[0].parts[0], acr_ui_protocol::Part::Sprite { url, .. } if url == "image/hud%3Aminimap/2")
        );
        // The page can fetch both versions, and nothing else.
        let (lib, fonts) = ui.assets();
        for (path, ok) in
            [("image/hud%3Aminimap/1", true), ("image/hud%3Aminimap/2", true), ("image/hud%3Aminimap/3", false)]
        {
            let got = assets::resolve(lib, fonts, &mut web.cache, path);
            assert_eq!(matches!(got, Ok(assets::Resolved::Ready(_))), ok, "{path}");
        }
    }

    #[test]
    fn a_group_that_only_moved_goes_out_as_a_shift() {
        use crate::hud::font;
        use crate::hud::zoom::Zoom;
        use crate::ui::draw::Rect;
        let mut web = WebUi::new(Box::new(|_| {}), Host::Web, vec![], vec![], [1.0; 2], 1.0);
        let ui = ui_with_menu();
        // A tooltip at the mouse: a fill, a frame clipped to its box, a string; the box's clip window moves with it.
        let hint = |m: i32, scale: f32| Claimed {
            name: "hint",
            layer: 3,
            zoom: Zoom { anchor: [m as f32, 5.0], scale },
            draws: vec![
                Draw::fill([1, 2, 3, 255], Rect::new(m, 20, 9, 9)),
                Draw::fill([4, 5, 6, 255], Rect::new(m, 20, 9, 1)).clipped(Rect::new(m, 0, 9, 99)),
                Draw::game_text(font::YELLOW, b"EISEN", m + 2, 22),
            ],
        };
        let p = web.hud_patch(&ui, vec![hint(10, 1.0)]).unwrap().unwrap();
        assert_eq!((p.groups.len(), p.moved.len()), (1, 0));
        let p = web.hud_patch(&ui, vec![hint(17, 1.0)]).unwrap().unwrap();
        assert_eq!((p.groups.len(), p.moved.len()), (0, 1));
        assert_eq!((p.moved[0].name.as_str(), p.moved[0].offset), ("hint", [7, 0]));
        assert!(web.hud_patch(&ui, vec![hint(17, 1.0)]).unwrap().is_none());
        // Back at the start: the shift is zero again, which the UI must be told.
        let p = web.hud_patch(&ui, vec![hint(10, 1.0)]).unwrap().unwrap();
        assert_eq!(p.moved[0].offset, [0, 0]);
        // A different scale is a new group; an anchor that did not move with the draws too.
        let p = web.hud_patch(&ui, vec![hint(10, 2.0)]).unwrap().unwrap();
        assert_eq!(p.groups.len(), 1);
        let mut off = hint(14, 2.0);
        off.zoom.anchor = [99.0, 5.0];
        let p = web.hud_patch(&ui, vec![off]).unwrap().unwrap();
        assert_eq!(p.groups.len(), 1);
        // Changed content is not a shift.
        let mut other = hint(14, 2.0);
        other.draws[2] = Draw::game_text(font::YELLOW, b"STEIN", 16, 22);
        let p = web.hud_patch(&ui, vec![other]).unwrap().unwrap();
        assert_eq!(p.groups.len(), 1);
    }

    #[test]
    fn the_interface_is_the_uis_only_where_it_named_it_on_a_transparent_host() {
        let hud = || vec!["TopBar".to_owned(), "Hints".to_owned(), "Minimap".to_owned(), "ArmyList".to_owned()];
        let web = WebUi::new(Box::new(|_| {}), Host::Web, vec![], hud(), [1.0; 2], 1.0);
        let some = Claim { top_bar: true, hints: true, minimap: true, army: true, ..Claim::default() };
        assert_eq!(web.hud_claim(1.0), some);
        assert_eq!(web.hud_claim(2.0), some);
        // A fractional canvas scale (1920 x 1080: 1.40625) leaves the HUD to the engine (#464).
        assert_eq!(web.hud_claim(1.40625), Claim::default());
        let native = WebUi::new(Box::new(|_| {}), Host::Native, vec![], hud(), [1.0; 2], 1.0);
        assert_eq!(native.hud_claim(1.0), Claim::default());
        let mut all = WebUi::new(Box::new(|_| {}), Host::Native, vec![], vec![], [1.0; 2], 1.0);
        all.claim_everything();
        assert_eq!(all.hud_claim(1.40625), Claim::ALL);
    }

    #[test]
    fn resync_sends_everything_again_with_a_higher_rev() {
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = log.clone();
        let mut web = WebUi::new(
            Box::new(move |m| sink.borrow_mut().push(m.to_owned())),
            Host::Web,
            vec![],
            vec![],
            [1.0; 2],
            1.0,
        );
        let mut ui = ui_with_menu();
        web.publish(&mut ui, Mode::Intro, None, None, Vec::new());
        web.resync();
        web.publish(&mut ui, Mode::Intro, None, None, Vec::new());
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
