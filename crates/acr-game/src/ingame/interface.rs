//! The in-game interface hooked into the game: what the HUD shows of the world, mouse and
//! keyboard input of the game screen, scrolling, and the overlays drawn over the map
//! (selection brackets, life bars, the move marker, the selection rectangle).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use acr_formats::defs::NationList;
use acr_formats::raster::Image;
use acr_sim::world::EntityId;
use acr_sim::{Command, TypeId, Unit};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::{CLICK_SLOP, InGame};
use crate::canvas::Canvas;
use crate::hud::cursor::{self, Cursor};
use crate::hud::data::HudData;
use crate::hud::minimap::{self, Dot, Minimap};
use crate::hud::produce::Nations;
use crate::hud::{self, Hud, HudCommand, HudEvent, Icon, Stats, View, color, line, nearest};
use crate::ui::Ctx;
use crate::ui::dialog::Reject;
use crate::ui::draw::{Draw, GAME_PALETTE, Rect};
use crate::ui::settings::Settings;

/// `Data\Keyboard.dat` (in `Patch02.GS1`): `[CTRL|ALT|SHIFT] KEY : ICON` lines (0x4f0635).
const KEYBOARD: &str = "DATA\\KEYBOARD.DAT";
/// `CursGo`: frames shown at a move order's target, one per game frame (0x587fa9).
const MARKER_FRAMES: u8 = 17;
/// Edge width that scrolls the view (0x4c5910), in canvas pixels.
const EDGE: f32 = 6.0;
/// Speed set by the arrow keys (0x4c5910).
const KEY_SCROLL: i32 = 4;
/// A second left button release within this time and 16 pixels (`|dx| + |dy|`) is a double
/// click (WM_LBUTTONUP, 0x4a2d2a: 600 ms).
const DOUBLE_CLICK: f64 = 0.6;
const DOUBLE_CLICK_PIXELS: f32 = 16.0;
/// Touch placement: a second tap within this time and [`DOUBLE_TAP_DP`] of the first places the
/// building.
const DOUBLE_TAP: Duration = Duration::from_millis(350);
const DOUBLE_TAP_DP: f32 = 40.0;
/// Touch placement: a tap this close to the outline (dp) only starts a double tap and leaves the
/// outline where it is, so placing never shifts the building by the finger's aim.
const ON_OUTLINE_DP: f32 = 64.0;
/// The blinking outline's pulses per second.
const OUTLINE_HZ: f64 = 2.0;
/// A right drag longer than this (map pixels) gives the move a direction (0x4c4dad).
const DRAG_MIN: i32 = 30;
/// Colour tables of the building preview (`ShowGPDarkN` / `ShowGPRedN`, loaded at 0x483feb
/// and 0x4840d1): a free site and a blocked one.
/// The field preview's fills (RGBA): where a field can grow, where it cannot.
const FIELD_FREE: [u8; 4] = [70, 255, 70, 255];
const FIELD_BLOCKED: [u8; 4] = [230, 50, 50, 255];
const PREVIEW_FREE: &str = "0\\AGEW_1W.GRD";
const PREVIEW_BLOCKED: &str = "0\\AGEW_1R.GRD";
/// `NRES.DAT [RESTYPES]` names, the text ids of the resources in cost hints (0x606229).
pub(super) const RESOURCE_NAMES: [&str; 6] = ["Wood", "Gold", "Stone", "Food", "Iron", "Coal"];
/// Shift repeats production orders five times (0x60897d).
const SHIFT_REPEAT: usize = 5;
/// The zoomed-out view draws 8 screen pixels per 32-pixel cell (the world shift 0x90d6b4
/// goes from 5 to 3).
const ZOOM_SCALE: f32 = 0.25;

/// More objects than this and production stops with `UNILIMIT` (0x515579).
const OBJECT_LIMIT: usize = 65000;
/// Icon names of the resources by resource index (`ICONLIST.TXT`).
const MARKET_RESOURCES: [&str; 6] = ["WOOD", "GOLD", "STONE", "FOOD", "IRON", "COAL"];
/// The market's buttons: slot, `ECONOMY.TXT` icon, hint (`COMMENT.TXT`), action.
const MARKET_BUTTONS: &[(usize, &str, &str, HudCommand)] = &[
    (2, "r_Mins10", "ECONOM1", HudCommand::MarketAmount(-10)),
    (3, "r_Plus10", "ECONOM2", HudCommand::MarketAmount(10)),
    (4, "r_Buy", "ECONOM5", HudCommand::MarketTrade),
    (5, "r_Mins1k", "ECONOM3", HudCommand::MarketAmount(-1000)),
    (6, "r_Plus1k", "ECONOM4", HudCommand::MarketAmount(1000)),
    (26, "r_Mins100", "ECONOM6", HudCommand::MarketAmount(-100)),
    (27, "r_Plus100", "ECONOM7", HudCommand::MarketAmount(100)),
    (28, "r_Clear", "ECONOM8", HudCommand::MarketClear),
    (29, "r_Mins10k", "ECONOM9", HudCommand::MarketAmount(-10000)),
    (30, "r_Plus10k", "ECONOMA", HudCommand::MarketAmount(10000)),
];

/// Slot of occupant type `i` (0-based, in type order) on a building's page 3 (0x5f5db9,
/// layout A): `i % 11 + i / 12 * 12 + 1`. Type 11 lands on slot 1 again, as in the original.
fn occupant_slot(i: usize) -> usize {
    i % 11 + i / 12 * 12 + 1
}

/// The unit types inside the `buildings`, ascending by type id, with their counts, minus
/// the units already ordered out (`LeaveMine` orders, 0x5f522e); types left with none are
/// dropped.
fn occupant_counts(world: &acr_sim::World, buildings: &[&Unit]) -> Vec<(TypeId, i32)> {
    let mut counts: std::collections::BTreeMap<TypeId, i32> = std::collections::BTreeMap::new();
    for b in buildings {
        for u in b.inside.iter().filter_map(|id| world.unit(*id)) {
            *counts.entry(u.kind).or_default() += 1;
        }
        for order in &b.orders {
            if let acr_sim::world::Order::LeaveMine { kind, .. } = order
                && let Some(n) = counts.get_mut(kind)
            {
                *n -= 1;
            }
        }
    }
    counts.into_iter().filter(|(_, n)| *n > 0).collect()
}

/// What a key does for the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameKey {
    None,
    /// F12 or Escape: the game menu.
    Menu,
    /// A leader chord of the modern controls (Ctrl + Space, then a letter): the app handles
    /// it as a press of this key.
    Leader(NamedKey),
}

/// What the app should do after mouse input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMouse {
    None,
    Menu,
    Restart,
    /// Open the mission history (`UM_HIST`).
    History,
    /// A press of the Escape key (the touch top bar's escape plaque).
    Escape,
}

/// The gather order of a right click at world pixel `(x, y)` (`DetermineResource`, 0x4d3ef0,
/// called at 0x4c4bf5): the nearest source within 64 px, fields and carcasses included, if
/// one of the units can gather its type.
fn gather_context(world: &acr_sim::World, units: &[&Unit], x: i32, y: i32) -> Option<Context> {
    let kind = acr_sim::gather::determine(world, x, y)?;
    units
        .iter()
        .any(|u| world.catalog.get(u.kind).economy.take_res.get(usize::from(kind)).is_some_and(Option::is_some))
        .then_some(Context::Gather(x, y, kind))
}

/// What a right click on the map orders (and the cursor shown for it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Context {
    Attack(EntityId),
    Repair(EntityId),
    Enter(EntityId),
    /// Into another nation's building (with Ctrl): storm or take it.
    Storm(EntityId),
    /// World pixel and source type.
    Gather(i32, i32, u8),
    Move(i32, i32),
    Rally(i32, i32),
}

impl Context {
    /// The cursor frame (globals 0x66a904..0x66a934).
    fn cursor(self) -> usize {
        match self {
            Context::Attack(_) => cursor::frame::SWORD,
            Context::Repair(_) => cursor::frame::BUILD,
            Context::Enter(_) => cursor::frame::ENTER,
            Context::Storm(_) => cursor::frame::STORM,
            Context::Gather(_, _, kind) => {
                cursor::frame::GATHER.get(usize::from(kind)).map_or(cursor::frame::ARROW, |&f| usize::from(f))
            }
            Context::Move(..) | Context::Rally(..) => cursor::frame::ARROW,
        }
    }
}

/// A move marker: world position and frame.
#[derive(Debug, Clone, Copy)]
struct Marker {
    x: i32,
    y: i32,
    frame: u8,
}

/// What the touch skin asks of the HUD ([`InGame::hud_event`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HudIntent<'a> {
    /// An icon of the command grid (`hud/<slot>`) or the army list (`army/<slot>`).
    Icon { id: &'a str, right: bool },
    /// A point of the minimap, `uv` from its top-left corner.
    Minimap { uv: [f32; 2], right: bool },
    /// A button of the top bar (`bar/menu`, `bar/pause`).
    Bar { id: &'a str },
}

/// Interface state of the game screen.
pub struct State {
    pub hud: Hud,
    cursor: Cursor,
    cursor_loaded: bool,
    /// The parts of the interface an external UI draws, and what it is to draw this frame.
    pub(super) claim: hud::Claim,
    pub(super) claimed: Vec<hud::Claimed>,
    /// The touch skin's pieces of the last frame (`hud::Claim::touch`).
    pub(super) touch_view: Option<hud::touch_skin::TouchView>,
    /// The Pause key (op 0x46) stopped the simulation.
    pub paused: bool,
    /// Control groups 0..9.
    pub(super) groups: Vec<Vec<EntityId>>,
    markers: Vec<Marker>,
    /// Scroll speed per axis (0x4c5910) and the time not yet scrolled.
    scroll: [i32; 2],
    scroll_time: f32,
    pub scroll_speed: i32,
    /// Production lists, availability and officers of the nation files.
    nations: Option<Rc<Nations>>,
    /// A building being placed (`BuildMode` 0x10fc644, `BuildingID` 0x10fca60).
    pub placing: Option<TypeId>,
    /// The next click on the map sets the selected buildings' rally point.
    pub(super) rally_mode: bool,
    /// Shown page of the icons per building type (`+0x183c` of its type).
    pages: HashMap<TypeId, u8>,
    /// Start of the game screen, for the pulsing preview.
    started: web_time::Instant,
    /// Touch mode (#331): a finger is the pointer, so placing a building uses a blinking
    /// outline moved by taps (#439) and a long press on a production icon is endless (#438).
    pub(super) touch: bool,
    /// The HUD hit-tests window pixels (the default). Off: the touch skin draws the HUD and
    /// sends its clicks as [`InGame::hud_event`]s, and a pixel never reaches the HUD.
    pub(super) hud_input: bool,
    /// The outline's window position while placing in touch mode; `None`: the window's middle.
    pub(super) touch_outline: Option<[f32; 2]>,
    /// The last tap while placing (touch clock, window position), for the double tap.
    touch_tap: Option<(Duration, [f32; 2])>,
    /// A finger down on the map while placing in touch mode: where it touched, where the
    /// outline was then, and whether it has moved; moving drags the outline by as much.
    touch_grab: Option<TouchGrab>,
    /// The touch clock at the latest gestures (the app's, or the driver's).
    pub(super) touch_time: Duration,
    keyboard: Option<Vec<(u8, u8, String)>>,
    pub mods: ModifiersState,
    pub(super) view: View,
    pub(super) canvas: Canvas,
    /// The window's scale factor (physical pixels per density-independent pixel).
    pub(super) scale_factor: f32,
    pub(super) window: [f32; 2],
    /// The last left button release on the map (time, canvas point), for double clicks.
    last_click: Option<(web_time::Instant, [f32; 2])>,
    /// The selection when the left button went down (Shift adds the rectangle to it).
    pub(super) select_before: Vec<EntityId>,
    /// The right button went down on the map with a selection: window point and world pixel
    /// (`0x1100b32`, start 0x1100acc / 0x1100ad0).
    pub(super) right_drag: Option<([f32; 2], (i32, i32))>,
    /// `GO&ATTACK`: moves are attack-moves (`0x28f9e55`).
    pub attack_mode: bool,
    /// The patrol icon was pressed: the next left click on the map is the patrol point
    /// (`0x1100b3d`).
    pub patrol_mode: bool,
    /// The officer whose formation icons were built last (0x6781dc): another one closes the
    /// open formation.
    form_officer: Option<EntityId>,
    /// The officer's open formation (0x28ead08): low byte formation + 1 (0 = none), high
    /// byte how many sizes down (`DECFORM`).
    form_menu: u16,
    /// An officer just ordered to form: its formation is selected once formed (0x41f198).
    select_formed: Option<EntityId>,
    /// The attack-ground icon was pressed: the next click on the map is the point to
    /// bombard (`0x28ead0b`).
    pub attack_ground_mode: bool,
    /// Mortars rule (#301): "Zielzone setzen" was pressed (`mortar_ui.rs`).
    pub(super) mortar: super::mortar_ui::ZoneMode,
    /// The guns' and mortars' aim guides are shown (`aim_guides`, the "Zielhilfe" icon).
    aim_guides: bool,
    /// Space cycles through the selection (0x4c6020).
    next_centred: usize,
    /// The market's amount and chosen slots (ECO+0x2a0, +0x2a4, +0x2a8), kept for the game.
    market_amount: i32,
    market_sell: Option<usize>,
    market_buy: Option<usize>,
    /// Own buildings with a queue: its length and the head's progress and stages, to tell
    /// when production finishes (`FINBLD`).
    queues: HashMap<EntityId, (usize, i32, i32)>,
    /// Messages to put on the message list once the texts are at hand.
    hints: Vec<Hint>,
    /// The optional modern controls (#129).
    pub(super) modern: super::controls::Modern,
}

/// A message of the production interface (`AssignHint1`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Hint {
    /// `UNILIMIT`, 50 frames.
    UnitLimit,
    /// `NETRESS`/`NETRES1` with the missing resources, 70 frames.
    Missing(Vec<usize>),
    /// `FINBLD` with the building type's name, 100 frames.
    Finished(TypeId),
    /// `NOT_ENOUGH_PEASANTS`, 10 frames.
    NotEnoughPeasants,
}

impl Default for State {
    fn default() -> Self {
        Self {
            hud: Hud::default(),
            cursor: Cursor::default(),
            cursor_loaded: false,
            claim: hud::Claim::default(),
            claimed: Vec::new(),
            touch_view: None,
            paused: false,
            groups: vec![Vec::new(); 10],
            markers: Vec::new(),
            scroll: [0; 2],
            scroll_time: 0.0,
            scroll_speed: Settings::default().scroll_speed,
            nations: None,
            placing: None,
            rally_mode: false,
            pages: HashMap::new(),
            started: web_time::Instant::now(),
            touch: false,
            hud_input: true,
            touch_outline: None,
            touch_tap: None,
            touch_grab: None,
            touch_time: Duration::ZERO,
            keyboard: None,
            mods: ModifiersState::empty(),
            view: View::default(),
            canvas: Canvas::screen([1024.0, 768.0]),
            scale_factor: 1.0,
            window: [1024.0, 768.0],
            last_click: None,
            select_before: Vec::new(),
            right_drag: None,
            attack_mode: false,
            patrol_mode: false,
            form_officer: None,
            form_menu: 0,
            select_formed: None,
            attack_ground_mode: false,
            mortar: Default::default(),
            aim_guides: true,
            next_centred: 0,
            market_amount: 0,
            market_sell: None,
            market_buy: None,
            queues: HashMap::new(),
            hints: Vec::new(),
            modern: Default::default(),
        }
    }
}

/// Simulation ticks per second for `FPSTime` (`1000 / (2·FPSTime)`, see docs/simulation.md);
/// the uncapped setting 0 runs at 100.
pub fn tick_rate(frame_time: i32) -> u32 {
    (1000 / (2 * frame_time.clamp(5, 100))) as u32
}

/// Map cell size in screen pixels: 32 wide, 16 high (world y is halved on screen).
const CELL: [f32; 2] = [32.0, 16.0];

/// The outline held by a finger while placing in touch mode (#439).
#[derive(Debug, Clone, Copy, PartialEq)]
struct TouchGrab {
    press: [f32; 2],
    outline: [f32; 2],
    dragged: bool,
}

/// Whether a tap at `at` (touch clock `now`) is the second of a double tap after `prev`, within
/// `reach` window pixels.
fn is_double_tap(prev: Option<(Duration, [f32; 2])>, now: Duration, at: [f32; 2], reach: f32) -> bool {
    prev.is_some_and(|(t, p)| now.saturating_sub(t) <= DOUBLE_TAP && (p[0] - at[0]).hypot(p[1] - at[1]) <= reach)
}

/// The touch placement outline's opacity `seconds` into the game: a pulse of [`OUTLINE_HZ`]
/// between 0.25 and 1.
fn outline_alpha(seconds: f64) -> f32 {
    let wave = 0.5 + 0.5 * (std::f64::consts::TAU * OUTLINE_HZ * seconds).sin();
    (0.25 + 0.75 * wave) as f32
}

impl InGame {
    /// Applies the options: game speed (tick rate), scroll speed and the control scheme; the
    /// top bar is the setting's, or the input mode's where the setting is unset.
    pub fn apply_settings(&mut self, settings: &Settings, mode: crate::input_mode::InputMode) {
        self.frame_time = settings.frame_time;
        self.clock.set_rate(tick_rate(settings.frame_time));
        self.interface.scroll_speed = settings.scroll_speed.clamp(2, 10);
        self.apply_controls(settings);
        self.three_d.apply(settings);
        self.interface.aim_guides = settings.aim_guides;
        self.editor_window = settings.editor_window;
        self.interface.hud.center_top_bar = settings.center_top_bar;
        self.interface.hud.hud_scale = settings.hud_scale;
        self.set_input_mode(settings, mode);
    }

    /// Shows the top bar the input mode asks for (an explicit `top_bar` setting wins).
    pub fn set_input_mode(&mut self, settings: &Settings, mode: crate::input_mode::InputMode) {
        self.interface.hud.top_bar = mode.top_bar(settings.top_bar);
        self.interface.touch = mode == crate::input_mode::InputMode::Touch;
    }

    /// The editor window's place as `settings.txt` keeps it (`editor_window`), once a held grip
    /// is let go; `None` outside the editor or while one is held.
    pub fn editor_window(&self) -> Option<Option<crate::ui::settings::EditorWindow>> {
        self.editor.as_ref().filter(|e| !e.dragging()).map(|e| e.frame.setting())
    }

    /// Whether the aim guides are shown: the app writes a switch by the "Zielhilfe" icon back
    /// to `settings.txt` (`aim_guides`), so it holds for the next games too.
    pub fn aim_guides(&self) -> bool {
        self.interface.aim_guides
    }

    pub fn set_modifiers(&mut self, mods: ModifiersState) {
        self.interface.mods = mods;
    }

    /// The touch clock at the gestures about to be applied (the double tap's time).
    pub fn set_touch_time(&mut self, now: Duration) {
        self.interface.touch_time = now;
    }

    /// Tells the game the window's scale factor, which sizes the touch top bar.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.interface.scale_factor = scale_factor;
    }

    /// The interface's canvas: the whole window at the menus' scale.
    pub fn screen_canvas(&self) -> Canvas {
        self.interface.canvas
    }

    pub(super) fn canvas_point(&self, p: [f32; 2]) -> [f32; 2] {
        self.interface.canvas.to_virtual(p)
    }

    /// Map shift `s` (1 for normal maps).
    fn shift(&self) -> u32 {
        self.map.as_ref().and_then(|m| m.size).map_or(1, |s| u32::from(s.shift))
    }

    /// Camera position and view size in minimap pixels.
    fn minimap_camera(&self, window: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let k = (1 << self.shift()) as f32;
        let s = self.camera.scale;
        (
            [self.camera.x / (CELL[0] * k), self.camera.y / (CELL[1] * k)],
            [window[0] / s / (CELL[0] * k), window[1] / s / (CELL[1] * k)],
        )
    }

    /// The free camera's ground footprint (the window corners' rays on `z = 0`) in minimap
    /// pixels, `None` with the fixed camera (#309).
    fn minimap_frame(&self, window: [f32; 2]) -> Option<[[i32; 2]; 4]> {
        let view = self.three_d.free_view(self.camera, window)?;
        let k = (1 << self.shift()) as f32;
        // Far corners of a low camera reach the horizon: keep them near the map.
        let side = minimap::SIDE as f32;
        let [w, h] = window;
        Some([[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]].map(|px| {
            let (o, d) = view.ray(px);
            let t = o[2] / -d[2].min(-1e-3);
            let p = [o[0] + t * d[0], o[1] + t * d[1]];
            let m = |v: f32| (v / (CELL[0] * k)).clamp(-2.0 * side, 3.0 * side) as i32;
            [m(p[0]), m(p[1])]
        }))
    }

    /// Centres the camera on a minimap pixel (0x4c5c9a).
    fn centre_on_minimap(&mut self, px: [f32; 2], window: [f32; 2]) {
        let k = (1 << self.shift()) as f32;
        let s = self.camera.scale;
        self.camera.x = px[0] * CELL[0] * k - window[0] / s / 2.0;
        self.camera.y = px[1] * CELL[1] * k - window[1] / s / 2.0;
        self.clamp_camera(window);
    }

    pub(super) fn clamp_camera(&mut self, window: [f32; 2]) {
        if let Some(terrain) = &self.terrain {
            let side = (terrain.width - 1) as f32 * 32.0;
            let s = self.camera.scale;
            self.camera.x = self.camera.x.clamp(-window[0] / s / 2.0, side - window[0] / s / 2.0);
            self.camera.y = self.camera.y.clamp(-window[1] / s / 2.0, side / 2.0 - window[1] / s / 2.0);
        }
    }

    /// World pixel of a minimap pixel.
    fn minimap_world(&self, px: [f32; 2]) -> (i32, i32) {
        let k = (1 << self.shift()) as f32;
        ((px[0] * 32.0 * k) as i32, (px[1] * 32.0 * k) as i32)
    }

    /// The scroll speeds of 0x4c5910, updated once per game frame: arrow keys set ±4, the
    /// cursor at the edge accelerates up to `ScrollSpeed`, otherwise the speed decays by 1;
    /// the view moves by half the speed in map cells per frame.
    ///
    /// The modern controls (#129) add W/A/S/D to the arrow keys, which then scroll at
    /// `ScrollSpeed`, can switch the edge off, and keep the speed on the screen the same at
    /// any zoom.
    pub(super) fn scroll_view(&mut self, seconds: f32, window: [f32; 2]) {
        let rate = 1.0 / self.clock_period();
        self.interface.scroll_time += seconds.min(0.25);
        let frame = 1.0 / rate;
        let edge = EDGE * self.interface.canvas.scale.max(1.0);
        let modern = self.modern();
        let key_speed = if modern { self.interface.scroll_speed } else { KEY_SCROLL };
        let edge_on = self.interface.modern.edge_scroll && self.interface.modern.pan.is_none();
        let keys = self.interface.modern.keys;
        while self.interface.scroll_time >= frame {
            self.interface.scroll_time -= frame;
            for axis in 0..2 {
                let (minus, plus) = if axis == 0 {
                    (NamedKey::ArrowLeft, NamedKey::ArrowRight)
                } else {
                    (NamedKey::ArrowUp, NamedKey::ArrowDown)
                };
                let v = &mut self.interface.scroll[axis];
                let at = self.cursor.filter(|_| self.drag_from.is_none() && edge_on).map(|c| c[axis]);
                if self.held.contains(&minus) || keys[2 * axis] {
                    *v = -key_speed;
                } else if self.held.contains(&plus) || keys[2 * axis + 1] {
                    *v = key_speed;
                } else if at.is_some_and(|c| c < edge) {
                    *v = (*v - 1).max(-self.interface.scroll_speed);
                } else if at.is_some_and(|c| c > window[axis] - edge) {
                    *v = (*v + 1).min(self.interface.scroll_speed);
                } else {
                    *v -= v.signum();
                }
            }
        }
        // Spread each frame's step over the frame so the view moves smoothly.
        let step = seconds.min(0.25) * rate / if modern { self.camera.scale } else { 1.0 };
        self.camera.x += (self.interface.scroll[0] / 2) as f32 * CELL[0] * step;
        self.camera.y += (self.interface.scroll[1] / 2) as f32 * CELL[1] * step;
        self.clamp_camera(window);
    }

    fn clock_period(&self) -> f32 {
        1.0 / tick_rate(self.frame_time) as f32
    }

    /// Advances the move markers by one game frame.
    pub(super) fn tick_markers(&mut self) {
        for m in &mut self.interface.markers {
            m.frame += 1;
        }
        self.interface.markers.retain(|m| m.frame < MARKER_FRAMES);
    }

    fn add_marker(&mut self, x: i32, y: i32) {
        self.interface.markers.push(Marker { x, y, frame: 0 });
    }

    pub(super) fn own(&self) -> u8 {
        self.local_player()
    }

    pub(super) fn selected_units(&self) -> impl Iterator<Item = &Unit> {
        let world = self.world.as_ref();
        self.selected.iter().filter_map(move |&id| world.and_then(|w| w.unit(id)).filter(|u| selectable(u)))
    }

    /// The selected units the panel lists: those outside buildings. The original's panel
    /// builder (0x606710) skips objects whose housed field (OB+0x58) is non-zero; they stay
    /// selected for commands.
    fn panel_units(&self) -> impl Iterator<Item = &Unit> {
        self.selected_units().filter(|u| listed_in_panel(u))
    }

    /// Dead, dying and (hit-locations rule) out-of-the-fight units leave the selection
    /// ([`selectable`]). Called after a tick.
    pub(super) fn prune_selection(&mut self) {
        let Some(world) = &self.world else { return };
        self.selected.retain(|&id| world.unit(id).is_some_and(selectable));
    }

    /// The Delete key (0x4a33a7: pseudo key 0xc8 → 0x4c2c61 → network command 0x21,
    /// `KillSelected` 0x4dc83c): the own selected objects die (finished `SLOWDEATH`
    /// buildings set on fire). No confirmation, as in the original.
    pub(super) fn destruct_selected(&mut self) {
        if self.mission.ended.is_some() {
            return;
        }
        let Some(world) = &self.world else { return };
        if let Some(c) = destruct_command(world, self.own(), &self.selected) {
            self.commands.push(c);
        }
    }

    /// The local player's selected objects, for the state commands.
    fn own_selected_units(&self) -> Vec<EntityId> {
        self.selected_units().filter(|u| u.owner == self.own()).map(|u| u.id).collect()
    }

    /// Reads the nation files once they are loaded.
    fn nations(&mut self, ctx: &Ctx) -> Option<Rc<Nations>> {
        if self.interface.nations.is_none() {
            if !ctx.lib.settled("NATIONS.LST") {
                return None;
            }
            // Only the files of NATIONS.LST, in the order the simulation reads them.
            let paths = ctx.lib.bytes("NATIONS.LST").map(|b| NationList::parse(&b).files()).unwrap_or_default();
            let mut files = Vec::new();
            for p in &paths {
                if !ctx.lib.settled(p) {
                    return None;
                }
                files.extend(ctx.lib.bytes(p));
            }
            self.interface.nations = Some(Rc::new(Nations::parse(files.iter().map(|b| &b[..]))));
        }
        self.interface.nations.clone()
    }

    fn keyboard(&mut self, ctx: &Ctx) -> Option<&Vec<(u8, u8, String)>> {
        if self.interface.keyboard.is_none() && ctx.lib.settled(KEYBOARD) {
            let text = ctx.lib.bytes(KEYBOARD).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            let mut keys = Vec::new();
            for line in text.lines() {
                let line = line.split("//").next().unwrap_or("");
                let Some((left, icon)) = line.split_once(':') else { continue };
                let words: Vec<&str> = left.split_whitespace().collect();
                let Some(key) =
                    words.last().and_then(|k| k.bytes().next()).filter(|_| words.last().unwrap().len() == 1)
                else {
                    continue;
                };
                let mods = words[..words.len() - 1].iter().fold(0u8, |m, w| {
                    m | match w.to_ascii_uppercase().as_str() {
                        "CTRL" => 1,
                        "ALT" => 2,
                        "SHIFT" => 4,
                        _ => 0,
                    }
                });
                keys.push((mods, key.to_ascii_uppercase(), icon.trim().to_owned()));
            }
            self.interface.keyboard = Some(keys);
        }
        self.interface.keyboard.as_ref()
    }

    /// The selection's icons: one per selected type (with count and life), then the
    /// commands (0x606710).
    fn build_view(&mut self, ctx: &Ctx, window: [f32; 2]) -> View {
        let (camera, view_size) = self.minimap_camera(window);
        let player = self.own();
        let frame = self.minimap_frame(window);
        let mut view = View { player, camera, view: view_size, frame, ..View::default() };
        let nations = self.nations(ctx);
        self.panel_view(ctx, &mut view);
        let Some(world) = &self.world else { return view };
        let res = &world.players[usize::from(player).min(world.players.len() - 1)].resources;
        view.resources = std::array::from_fn(|i| res[i].max(0) as u32);
        view.history = self.mission.has_history();
        view.paused = self.interface.paused && self.mission.ended.is_none();
        view.fog = self.fog_mask();
        let shift = self.shift() + 9;
        view.dots = minimap_dots(world, player, &self.selected, shift);
        let Some(data) = self.interface.hud.data(ctx) else { return view };
        // One icon per selected type of the free units, in selection order; its bar sums life
        // and MaxLife (OB+0x78/0x7a, 0x606e12), which building sections raise. Then one per
        // selected formation (0x606d10).
        let mut types: Vec<(acr_sim::TypeId, i32, i32, i32)> = Vec::new();
        let mut brigades: Vec<u16> = Vec::new();
        for u in self.panel_units() {
            if let Some(b) = u.brigade {
                if !brigades.contains(&b) {
                    brigades.push(b);
                }
                continue;
            }
            let max = u.max_life.max(1);
            match types.iter_mut().find(|t| t.0 == u.kind) {
                Some(t) => {
                    t.1 += 1;
                    t.2 += u.life;
                    t.3 += max;
                }
                None => types.push((u.kind, 1, u.life, max)),
            }
        }
        let outline = color::PLAYER.wrapping_add(4 * (player % 8));
        let objects = self.objects.objects();
        // A single formation shows its soldiers' numbers with the formation bonus and the
        // reload of all its members (0x606edd).
        let single = match (&types[..], &brigades[..]) {
            ([(kind, ..)], []) => self.panel_units().find(|u| u.kind == *kind && u.brigade.is_none()),
            ([], [b]) => world
                .brigades
                .get(usize::from(*b))
                .and_then(|br| br.members.iter().skip(3).flatten().find_map(|&m| world.unit(m).filter(|u| u.living()))),
            _ => None,
        };
        if let Some(u) = single {
            let kind = u.kind;
            let t = world.catalog.get(kind);
            let def = objects.definition(&t.id);
            let mut s = stats(world, &data, u, def);
            s.kills = i32::from(u.kills);
            s.ready = !u.building || u.stage >= world.max_stage(u);
            s.life = u.life;
            if t.building {
                s.protections.clear();
            }
            s.lines = self.info_lines(ctx, u);
            // The type's record sums the reload counters of its selected objects (0x606ae0); a
            // formation's those of all its members (0x6073a1).
            if def.is_some_and(|d| d.has("SHOWDELAY")) {
                let sum = |units: &mut dyn Iterator<Item = &Unit>| {
                    units.fold((0, 0), |(d, m), v| (d + v.delay, m + v.max_delay))
                };
                s.reload = Some(match u.brigade.and_then(|b| world.brigades.get(usize::from(b))) {
                    Some(br) => sum(&mut br.members.iter().flatten().filter_map(|&m| world.unit(m))),
                    None => sum(&mut self.panel_units().filter(|v| v.kind == kind)),
                });
            }
            view.stats = Some(s);
        }
        if let ([(kind, ..)], [], Some(nations)) = (&types[..], &brigades[..], &nations) {
            self.production_icons(ctx, &data, nations, *kind, outline, &mut view.icons);
        }
        if view.icons.iter().any(|i| i.action == Some(HudCommand::MarketTrade)) {
            let i = &self.interface;
            let amount = i.market_sell.map(|_| i.market_amount);
            let result = i.market_sell.zip(i.market_buy).map(|(s, b)| world.market.quote(s, b, i.market_amount));
            view.market = Some((amount, result));
        }
        if let ([], [b]) = (&types[..], &brigades[..]) {
            self.brigade_icons(ctx, &data, *b, outline, &mut view.icons);
        }
        self.officer_icons(ctx, &data, outline, &mut view.icons);
        self.state_icons(ctx, &data, outline, &mut view.icons);
        self.attack_ground_icon(ctx, &data, outline, &mut view.icons);
        self.idle_icons(&data, outline, &mut view.icons);
        self.switch_icons(ctx, &data, outline, &mut view.icons);
        self.mortar_icons(&data, outline, &mut view.icons);
        self.aim_guide_icon(&data, outline, &mut view.icons);
        if let ([(kind, ..)], []) = (&types[..], &brigades[..]) {
            self.free_occupant_icons(ctx, &data, outline, *kind, &mut view.icons);
        }
        let Some(world) = &self.world else { return view };
        let objects = self.objects.objects();
        // The type icons, then the formation icons, take the first slots the commands leave
        // free (`AddIcon` without a position, 0x495631). A formation's icon shows its soldier
        // type, how many soldiers it has and their life (0x606d10).
        let grid = (0..36).filter(|s| !view.icons.iter().any(|i| i.slot == *s)).collect::<Vec<_>>();
        // A single type or formation gets its own place beside its stats (0x5aa136).
        let one_entry = types.len().min(12) + brigades.len() == 1;
        let mut free = one_entry.then_some(hud::ZONE1_SLOT).into_iter().chain(grid);
        let icon_of = |kind: acr_sim::TypeId| {
            objects.definition(&world.catalog.get(kind).id).and_then(|d| d.icon()).and_then(|i| data.icon(i))
        };
        for &(kind, count, life, max) in types.iter().take(12) {
            let Some(slot) = free.next() else { break };
            view.icons.push(Icon {
                frame: icon_of(kind).unwrap_or(0),
                slot,
                outline: Some(outline),
                life: Some((life, max)),
                count: Some(count),
                action: Some(HudCommand::SelectType(kind.0 as usize)),
                ..Icon::default()
            });
        }
        for &b in &brigades {
            let Some(br) = world.brigades.get(usize::from(b)) else { continue };
            let Some(slot) = free.next() else { break };
            let soldiers = formation_soldiers(world, b);
            let (life, max) = soldiers.iter().fold((0, 0), |(l, m), u| (l + u.life, m + u.max_life.max(1)));
            view.icons.push(Icon {
                frame: br.kind.and_then(icon_of).unwrap_or(0),
                slot,
                outline: Some(outline),
                life: (max > 0).then_some((life, max)),
                count: (!soldiers.is_empty()).then_some(soldiers.len() as i32),
                // `ARMYSTR` ("Armee") and the formation's number (0x606d91, 0x606e6e).
                number: Some(i32::from(br.number) + 1),
                hint: ctx.texts.get("ARMYSTR").to_vec(),
                action: Some(HudCommand::SelectBrigade(b)),
                ..Icon::default()
            });
        }
        view.army = self.army_icons(&data, self.shift());
        view
    }

    /// The army icons at the top left (0x607d3b, the end of 0x606710). With a formation among
    /// the first 30 slots of the player's brigade list that has a soldier, the `ORDERS_LIST`
    /// icon (`Formation_Kare`) stands at slot 0 and opens or closes the list (0x6064a0). The
    /// open list shows each such formation at slot 5 + its index: its soldier type with the
    /// number of soldiers (members from the fourth on that still exist), its number (the
    /// index + 1) at the bottom right and the yellow frame when one of them is selected. A click
    /// selects it (0x6064b0, Shift adds); the mouse over it marks its members' mean position
    /// on the minimap (0x606500). No outline, life bar or hint.
    fn army_icons(&self, data: &HudData, shift: u32) -> Vec<Icon> {
        let mut icons = Vec::new();
        let Some(world) = &self.world else { return icons };
        let own = self.own();
        let slots = (hud::ARMY_ROWS - 1) * hud::ARMY_COLUMNS;
        let listed: Vec<(usize, &acr_sim::formation::Brigade)> = world
            .brigades
            .iter()
            .enumerate()
            .filter(|(_, br)| br.alive && br.owner == own && usize::from(br.number) < slots)
            .collect();
        let alive = |b: usize, m: EntityId| fighting_member(world, b as u16, m);
        let soldiers = |b: usize, br: &acr_sim::formation::Brigade| -> Vec<EntityId> {
            br.members.iter().skip(3).flatten().copied().filter(|&m| alive(b, m).is_some()).collect()
        };
        if !listed.iter().any(|&(b, br)| !soldiers(b, br).is_empty()) {
            return icons;
        }
        if let Some(frame) = data.orders_list {
            icons.push(Icon { frame, slot: 0, action: Some(HudCommand::ArmyList), ..Icon::default() });
        }
        if !self.interface.hud.army_list {
            return icons;
        }
        let objects = self.objects.objects();
        let add = self.interface.mods.shift_key();
        for (b, br) in listed {
            let members = soldiers(b, br);
            if members.is_empty() {
                continue;
            }
            let frame = br
                .kind
                .and_then(|k| objects.definition(&world.catalog.get(k).id))
                .and_then(|d| d.icon())
                .and_then(|i| data.icon(i))
                .unwrap_or(0);
            // The mean position of all members still in it, as the minimap's unit dots.
            let all: Vec<&Unit> = br.members.iter().flatten().filter_map(|&m| alive(b, m)).collect();
            let n = all.len() as i32;
            let mark = (n > 0).then(|| {
                let sum =
                    all.iter().fold([0, 0], |[x, y], u| [x + (u.pos.x >> (shift + 9)), y + (u.pos.y >> (shift + 9))]);
                [sum[0] / n, sum[1] / n]
            });
            icons.push(Icon {
                frame,
                slot: hud::ARMY_COLUMNS + usize::from(br.number),
                count: Some(members.len() as i32),
                number: Some(i32::from(br.number) + 1),
                selected: members.iter().any(|m| self.selected.contains(m)),
                mark,
                action: Some(HudCommand::ArmySelect { brigade: b as u16, add }),
                ..Icon::default()
            });
        }
        icons
    }

    /// A single selected formation without free units (0x606edd): the formations its group
    /// can turn into (`ORD_GROUPS.LST`, at their `[ORDERICONS]` slots, network command 0x41)
    /// and `FILLFORM` at slot 12 (0x678a80, command 0x52).
    fn brigade_icons(&self, ctx: &Ctx, data: &HudData, b: u16, outline: u8, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let Some(br) = world.brigades.get(usize::from(b)).filter(|br| br.alive && br.owner == self.own()) else {
            return;
        };
        let f = &world.formations;
        let buttons = f.shapes.get(br.shape).and_then(|s| s.group).map_or(&[][..], |g| &f.groups[g].buttons[..]);
        for &g in buttons {
            let Some(icon) = f.groups.get(g).and_then(|g| data.order(&g.name)) else { continue };
            icons.push(Icon {
                frame: icon.frame,
                slot: icon.slot,
                outline: Some(outline),
                hint: ctx.texts.get(&icon.hint).to_vec(),
                action: Some(HudCommand::Reform(g)),
                ..Icon::default()
            });
        }
        if let Some(frame) = data.icon("Add_Formation") {
            icons.push(Icon {
                frame,
                slot: 12,
                outline: Some(outline),
                hint: ctx.texts.get("FILLFRM").to_vec(),
                action: Some(HudCommand::Fill),
                ..Icon::default()
            });
        }
    }

    /// The officer's formations (0x5f6869..0x5f6da3), for a single selected object or a
    /// selection of one type whose first object is an officer:
    /// - each formation of its `[OFFICERS]` entry at its `[ORDERICONS]` slot, enabled while a
    ///   free unit of its drummer type stands within 1024 px of it (or it needs none);
    ///   a click opens the formation (0x5f5c68), a disabled one closes it;
    /// - an open formation shows its soldier types right of its icon, each with the largest
    ///   size with as many free units of the type within 1024 px of the officer, less the
    ///   sizes stepped down; a click forms it (network command 0x3a). With a formation open,
    ///   `DECFORM` and `INCFORM` stand two and one slots left of the first formation's icon
    ///   and step the sizes down and up (0x5f5d49); when no type has a size left, the step
    ///   goes back by one.
    fn officer_icons(&mut self, ctx: &Ctx, data: &HudData, outline: u8, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let objects = self.objects.objects();
        let first = self.selected.first().and_then(|&id| world.unit(id)).filter(|u| u.owner == self.own());
        let same = first.is_some_and(|f| self.selected_units().all(|u| u.kind == f.kind));
        let Some((o, od)) = first.filter(|_| same).and_then(|o| Some((o, world.formations.officers.get(&o.kind)?)))
        else {
            self.interface.form_officer = None;
            return;
        };
        if self.interface.form_officer != Some(o.id) {
            self.interface.form_officer = Some(o.id);
            self.interface.form_menu = 0;
        }
        let near = |kind| acr_sim::formation::count_near(world, o.pos.x, o.pos.y, kind, o.owner);
        let ready = od.drummer.is_none_or(|d| near(d) > 0);
        let mut menu = self.interface.form_menu;
        let order = |f: &acr_sim::formation::OfficerFormation| {
            world.formations.groups.get(f.group).and_then(|g| data.order(&g.name))
        };
        for (i, f) in od.formations.iter().enumerate() {
            let Some(icon) = order(f) else { continue };
            if menu != 0 && i == 0 {
                for (dx, key, name, dec) in
                    [(2, "DECFORM", "Dec_Formation", true), (1, "INCFORM", "Inc_Formation", false)]
                {
                    if let (Some(frame), Some(slot)) = (data.icon(name), icon.slot.checked_sub(dx)) {
                        icons.push(Icon {
                            frame,
                            slot,
                            outline: Some(outline),
                            hint: ctx.texts.get(key).to_vec(),
                            action: Some(HudCommand::FormSize(dec)),
                            ..Icon::default()
                        });
                    }
                }
            }
            if !ready {
                menu = 0;
            }
            icons.push(Icon {
                frame: icon.frame,
                slot: icon.slot,
                outline: Some(outline),
                disabled: !ready,
                hint: ctx.texts.get(&icon.hint).to_vec(),
                action: ready.then_some(HudCommand::FormGroup(i)),
                ..Icon::default()
            });
        }
        if let Some(f) = (menu & 0xff).checked_sub(1).and_then(|i| od.formations.get(usize::from(i))) {
            let mut page = usize::from(menu >> 8);
            let slot = order(f).map_or(0, |i| i.slot);
            let sizes: Vec<usize> = f.shapes.iter().map(|&s| world.formations.shapes[s].slots()).collect();
            let mut any = false;
            for (j, &kind) in f.kinds.iter().enumerate() {
                let n = near(kind);
                let k = sizes.iter().position(|&s| s > n).unwrap_or(sizes.len()).saturating_sub(page);
                let t = world.catalog.get(kind);
                let frame = objects.definition(&t.id).and_then(|d| d.icon()).and_then(|i| data.icon(i)).unwrap_or(0);
                let mut icon = Icon { frame, slot: slot + j + 1, outline: Some(outline), ..Icon::default() };
                if k > 0 {
                    icon.count = Some(sizes[k - 1] as i32);
                    icon.action = Some(HudCommand::FormNear { shape: f.shapes[k - 1] as u16, kind: kind.0 });
                    icon.hint = unit_name(ctx, &t.md_name);
                    any = true;
                } else {
                    icon.disabled = true;
                }
                icons.push(icon);
            }
            if page != 0 && !any {
                page -= 1;
            }
            menu = (menu & 0xff) | (page as u16) << 8;
        }
        self.interface.form_menu = menu;
    }

    /// The commands for the selection's states (0x6074f1..0x607a59), with or without an
    /// officer. Over the selected own objects 0x606889 collects: formations and the types of
    /// the free units, `STANDGROUND` types, stand ground on/off, towers and other
    /// buildings, `WEAPONINSECTOR` types, attacks allowed/held, `FRIENDLYFIRE` and
    /// `ARMATTACK` types with their switches. Then:
    /// - slot 24 `SFREORD` (`FREE_ORDER` icon) with formations and no free units;
    /// - with a `STANDGROUND` type and at most 24 formations and free types: slots 25 and 26
    ///   `STANDGR` / `NOSTAND`, highlighted if some unit stands / does not, and `GO&ATTACK`
    ///   (highlighted in the attack-move mode); with free units and at most 12 rows also the
    ///   patrol icon at slot 14 (`HPATROL`);
    /// - with only towers among the buildings, with the stand-ground icons, or with a
    ///   `WEAPONINSECTOR` type: `ENABLE_ATTACK` / `DISABLE_ATTACK` (`EATTACK` / `DATTACK`);
    /// - `SHOTMYUNITS` / `KEEPMYUNITS` for `FRIENDLYFIRE` types, `ARMATTACK` / `ANYATTACK`
    ///   for `ARMATTACK` types, each highlighted while some unit has that setting.
    ///
    /// The idle icons follow in [`Self::idle_icons`], the first object's switches in
    /// [`Self::switch_icons`]. With at most 12 formations and no free units the grouping
    /// icon follows `SFREORD` (0x60755c, at the `UN?GROUP` slot): `CM_UNGRP` when the
    /// selected formations are exactly a group, else `CM_DOGRP` when every selected unit is
    /// in a formation.
    fn state_icons(&self, ctx: &Ctx, data: &HudData, outline: u8, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let objects = self.objects.objects();
        let own = self.own();
        let units: Vec<&Unit> = self.selected_units().filter(|u| u.owner == own).collect();
        let mut brigades: Vec<u16> = units.iter().filter_map(|u| u.brigade).collect();
        brigades.sort_unstable();
        brigades.dedup();
        let mut free: Vec<TypeId> = units.iter().filter(|u| u.brigade.is_none()).map(|u| u.kind).collect();
        free.sort_unstable_by_key(|k| k.0);
        free.dedup();
        let (rows, free) = (brigades.len() + free.len(), free.len());
        let types =
            |f: &dyn Fn(&acr_sim::catalog::ObjectType) -> bool| units.iter().any(|u| f(world.catalog.get(u.kind)));
        let stand_button = types(&|t| t.role.stand_ground_button);
        let tower = types(&|t| t.role.usage == acr_sim::catalog::usage::TOWER);
        let other_building = units.iter().any(|u| {
            let t = world.catalog.get(u.kind);
            u.building && t.role.usage != acr_sim::catalog::usage::TOWER
        });
        let sector = units
            .iter()
            .any(|u| objects.definition(&world.catalog.get(u.kind).id).is_some_and(|d| d.has("WEAPONINSECTOR")));
        if !brigades.is_empty()
            && free == 0
            && let Some(frame) = data.icon("Stand_Ground_OFF")
        {
            icons.push(Icon {
                frame,
                slot: 24,
                outline: Some(outline),
                hint: ctx.texts.get("SFREORD").to_vec(),
                action: Some(HudCommand::Disband),
                ..Icon::default()
            });
        }
        // Grouping (0x60755c): the `UN?GROUP` icon, without an outline.
        if brigades.len() <= 12 && free == 0 {
            let selected: Vec<EntityId> = self.selected_units().map(|u| u.id).collect();
            let (frame, hint, group) = match grouping(world, own, &selected) {
                Some(false) => (data.ungroup_icon, "CM_UNGRP", false),
                Some(true) => (data.dogroup_icon, "CM_DOGRP", true),
                None => (None, "", false),
            };
            if let Some(frame) = frame {
                icons.push(Icon {
                    frame,
                    slot: data.group_slot,
                    hint: ctx.texts.get(hint).to_vec(),
                    action: Some(HudCommand::Group(group)),
                    ..Icon::default()
                });
            }
        }
        let mut push = |frame: Option<usize>, slot: usize, hint: &str, selected: bool, action: HudCommand| {
            if let Some(frame) = frame {
                icons.push(Icon {
                    frame,
                    slot,
                    outline: Some(outline),
                    selected,
                    hint: ctx.texts.get(hint).to_vec(),
                    action: Some(action),
                    ..Icon::default()
                });
            }
        };
        // `[CONST] NAME x y icon`: the icon at slot `x + 12·y`.
        let placed = |key: &str| data.consts.get(key).map(|c| (c.slot, Some(c.frame)));
        let stand_icons = stand_button && rows > 0 && rows <= 24;
        if stand_icons {
            let on = units.iter().any(|u| u.stand_ground);
            let off = units.iter().any(|u| !u.stand_ground);
            push(data.icon("Stand_Ground_ON"), 25, "STANDGR", on, HudCommand::StandGround(true));
            push(data.icon("Stand_Ground_OFF"), 26, "NOSTAND", off, HudCommand::StandGround(false));
            if let Some((slot, frame)) = placed("GO&ATTACK") {
                push(frame, slot, "GONATT", self.interface.attack_mode, HudCommand::AttackMode);
            }
        }
        if stand_button && free > 0 && rows <= 12 {
            push(data.icon("Patrol"), 14, "HPATROL", self.interface.patrol_mode, HudCommand::PatrolMode);
        }
        if (tower && !other_building) || stand_icons || sector {
            let held = units.iter().any(|u| u.no_search_victim);
            let free_fire = units.iter().any(|u| !u.no_search_victim);
            if let Some((slot, frame)) = placed("ENABLE_ATTACK") {
                push(frame, slot, "EATTACK", free_fire, HudCommand::HoldFire(false));
            }
            if let Some((slot, frame)) = placed("DISABLE_ATTACK") {
                push(frame, slot, "DATTACK", held, HudCommand::HoldFire(true));
            }
        }
        if types(&|t| t.avoid_friendly_fire) {
            let ff = units.iter().filter(|u| world.catalog.get(u.kind).avoid_friendly_fire);
            let (on, off) = ff.fold((false, false), |(a, b), u| (a || u.no_ff_avoid, b || !u.no_ff_avoid));
            if let Some((slot, frame)) = placed("SHOTMYUNITS") {
                push(frame, slot, "SHOTMYUNITS", on, HudCommand::FriendlyFire(true));
            }
            if let Some((slot, frame)) = placed("KEEPMYUNITS") {
                push(frame, slot, "KEEPMYUNITS", off, HudCommand::FriendlyFire(false));
            }
        }
        if types(&|t| t.role.arm_attack) {
            let arm = units.iter().filter(|u| world.catalog.get(u.kind).role.arm_attack);
            let (on, off) = arm.fold((false, false), |(a, b), u| (a || u.melee_only, b || !u.melee_only));
            if let Some((slot, frame)) = placed("ARMATTACK") {
                push(frame, slot, "ARMATTACK", on, HudCommand::MeleeOnly(true));
            }
            if let Some((slot, frame)) = placed("ANYATTACK") {
                push(frame, slot, "ANYATTACK", off, HudCommand::MeleeOnly(false));
            }
        }
    }

    /// The attack-ground icon (0x5f6385): `NRES.DAT`'s `ATTACKGROUND icon slot` (parsed by
    /// 0x4ec18c; `Attack_Ground` at slot 13) with the hint `ATGHINT`, outside missions (no
    /// mission script, SCENINF+0x10 0x28b7758 is 0) when every selected object that is not
    /// dying has a type with `ARTPODGOTOVKA` (NM+0x172c bit 1; the cannons) and there is
    /// one. A click (0x5f5dad) starts the mode `0x28ead0b`: cursor frame 9 (0x66a910,
    /// 0x4c24c3) until the next click on the map, which sends network command 0x57
    /// (0x4d8344, `AttackGround`) at that point ([`Self::attack_ground_at`]); Escape ends it
    /// (0x4a3218).
    ///
    /// The brigade panel (0x607347) offers it too, for formations of such types, which the
    /// game data has none of.
    fn attack_ground_icon(&self, ctx: &Ctx, data: &HudData, outline: u8, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let Some((frame, slot)) = data.attack_ground else { return };
        if self.mission.has_script() {
            return;
        }
        let mut objects = self.selected_units().filter(|u| u.death == 0).peekable();
        if objects.peek().is_none() || !objects.all(|u| world.catalog.get(u.kind).ground_attack) {
            return;
        }
        icons.push(Icon {
            frame,
            slot,
            outline: Some(outline),
            selected: self.interface.attack_ground_mode,
            hint: ctx.texts.get("ATGHINT").to_vec(),
            action: Some(HudCommand::AttackGroundMode),
            ..Icon::default()
        });
    }

    /// ACR's "Zielhilfe" icon (not in the original; docs/ui.md "Gun marks"): with own guns
    /// (`WEAPONINSECTOR` types) or, under the mortars rule, own mortars selected, slot 20
    /// shows the original's `CEN02SP` picture (a cannon with its dotted flight), framed
    /// yellow while the aim guides are shown; a click shows or hides them. Not a command:
    /// replays and the simulation never see it.
    fn aim_guide_icon(&self, data: &HudData, outline: u8, icons: &mut Vec<Icon>) {
        const SLOT: usize = 20;
        let Some(world) = &self.world else { return };
        let own = self.own();
        let guns = self.selected_units().any(|u| {
            let t = world.catalog.get(u.kind);
            u.owner == own && u.death == 0 && (t.sector_weapon.is_some() || (world.rules.mortars && t.mortar))
        });
        let Some(frame) = data.icon("CEN02SP").filter(|_| guns) else { return };
        let on = self.interface.aim_guides;
        icons.retain(|i| i.slot != SLOT);
        icons.push(Icon {
            frame,
            slot: SLOT,
            outline: Some(outline),
            selected: on,
            hint: if on { b"Zielhilfe ausblenden".to_vec() } else { b"Zielhilfe einblenden".to_vec() },
            action: Some(HudCommand::AimGuides),
            ..Icon::default()
        });
    }

    /// The first selected object's idle icons (0x5f63f0): a finished own object whose type has
    /// `SHOWIDLE` or `SHOWIDLEPEASANTS` (type flag 0x172f bit 1) shows `IDLE_PEASANT`, with
    /// `SHOWIDLE` or `SHOWIDLEMINES` (bit 2) `IDLE_MINE`, at their `[CONST]` slots and without
    /// a hint, as in the original.
    fn idle_icons(&self, data: &HudData, outline: u8, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let Some(first) = self.selected.first().and_then(|&id| world.unit(id)) else { return };
        if first.owner != self.own() || (first.building && first.stage < world.max_stage(first)) {
            return;
        }
        let Some(def) = self.objects.objects().definition(&world.catalog.get(first.kind).id) else { return };
        let both = def.has("SHOWIDLE");
        for (key, shown, mines) in [
            ("IDLE_PEASANT", both || def.has("SHOWIDLEPEASANTS"), false),
            ("IDLE_MINE", both || def.has("SHOWIDLEMINES"), true),
        ] {
            if let (true, Some(c)) = (shown, data.consts.get(key)) {
                icons.push(Icon {
                    frame: c.frame,
                    slot: c.slot,
                    outline: Some(outline),
                    action: Some(HudCommand::SelectIdle(mines)),
                    ..Icon::default()
                });
            }
        }
    }

    /// The first selected object's switches (collector 0x606889, icons 0x607add..0x607d3b), at
    /// their `[CONST]` slots:
    /// - `STOPBUILD` for a `CANSTOPBUILD` type with fewer than 12 rows: stopped, the
    ///   `Production_ON` frame (the line's second word) with `STOPBUILD_OFF` resumes; running,
    ///   the `Production_OFF` frame (the first) with `STOPBUILD_ON` stops, see
    ///   [`stopbuild_frame`];
    /// - with one selected type: `KARTECH` (`KARTECH_TXT`) and `PREKARTECH`
    ///   (`PREKARTECH_TXT`) when a selected unit has canister; `FISHING` for a fishing boat
    ///   (fishing: the second frame, `FISHING_OFF_TXT`, stops; else the first,
    ///   `FISHING_ON_TXT`, resumes); `ZASEV` for a finished mill (sowing: `DONTMAKEZASEV`
    ///   stops; else highlighted, `MAKEZASEV` resumes).
    fn switch_icons(&self, ctx: &Ctx, data: &HudData, outline: u8, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let own = self.own();
        let objects: Vec<&Unit> = self.selected_units().filter(|u| u.owner == own).collect();
        let Some(first) = objects.first() else { return };
        let mut kinds: Vec<TypeId> = objects.iter().map(|u| u.kind).collect();
        kinds.sort_unstable_by_key(|k| k.0);
        kinds.dedup();
        let mut brigades: Vec<u16> = objects.iter().filter_map(|u| u.brigade).collect();
        brigades.sort_unstable();
        brigades.dedup();
        let mut free: Vec<TypeId> = objects.iter().filter(|u| u.brigade.is_none()).map(|u| u.kind).collect();
        free.sort_unstable_by_key(|k| k.0);
        free.dedup();
        let rows = brigades.len() + free.len();
        let single = kinds.len() == 1;
        let t = world.catalog.get(first.kind);
        let mut push = |key: &str, frame: usize, hint: &str, selected: bool, action: HudCommand| {
            let Some(s) = data.switches.get(key) else { return };
            if let Some(&frame) = s.frames.get(frame) {
                icons.push(Icon {
                    frame,
                    slot: s.slot,
                    outline: Some(outline),
                    selected,
                    hint: ctx.texts.get(hint).to_vec(),
                    action: Some(action),
                    ..Icon::default()
                });
            }
        };
        if t.can_stop_build && rows < 12 {
            let frame = stopbuild_frame(first.switched_off);
            if first.switched_off {
                push("STOPBUILD", frame, "STOPBUILD_OFF", false, HudCommand::StopProduction(false));
            } else {
                push("STOPBUILD", frame, "STOPBUILD_ON", false, HudCommand::StopProduction(true));
            }
        }
        if !single {
            return;
        }
        if objects.iter().any(|u| world.catalog.get(u.kind).sector_weapon.is_some()) {
            push("KARTECH", 0, "KARTECH_TXT", false, HudCommand::Canister);
            push("PREKARTECH", 0, "PREKARTECH_TXT", false, HudCommand::LoadCanister);
        }
        if t.fishing.0 != 0 {
            if first.switched_off {
                push("FISHING", 0, "FISHING_ON_TXT", false, HudCommand::StopFishing(false));
            } else {
                push("FISHING", 1, "FISHING_OFF_TXT", false, HudCommand::StopFishing(true));
            }
        }
        let ready = !first.building || first.stage >= world.max_stage(first);
        if ready && t.role.usage == acr_sim::catalog::usage::MELNICA {
            if first.no_sowing {
                push("ZASEV", 0, "MAKEZASEV", true, HudCommand::StopSowing(false));
            } else {
                push("ZASEV", 0, "DONTMAKEZASEV", false, HudCommand::StopSowing(true));
            }
        }
    }

    /// The production icons of a single selected type (0x609230): the available items of its
    /// `[FIXED_PRODUCE]` list at `x + 12·y` of the shown page (a later item in the same slot
    /// replaces an earlier one, as `AddIcon` reuses the slot); for buildings also the page
    /// buttons, the rally point and the leave icon.
    fn production_icons(
        &self,
        ctx: &Ctx,
        data: &HudData,
        nations: &Nations,
        kind: TypeId,
        outline: u8,
        icons: &mut Vec<Icon>,
    ) {
        let Some(world) = &self.world else { return };
        let player = self.own();
        let catalog = &world.catalog;
        let t = catalog.get(kind);
        let objects = self.objects.objects();
        let finished = |u: &Unit| !u.building || u.stage >= world.max_stage(u);
        let working: Vec<&Unit> =
            self.selected_units().filter(|u| u.kind == kind && u.owner == player && finished(u)).collect();
        if working.is_empty() {
            return;
        }
        // What the selected objects can make now (enabled, accessible, their stages done):
        // the simulation's tech tree; slots and keys come from `[FIXED_PRODUCE]`.
        let mut makeable: Vec<TypeId> = Vec::new();
        for u in &working {
            for k in acr_sim::economy::producible(world, u.id.0 as usize) {
                if !makeable.contains(&k) {
                    makeable.push(k);
                }
            }
        }
        let stock = &world.players[usize::from(player)].resources;
        let depots = self.depot_counts();
        let page = self.interface.pages.get(&kind).copied().unwrap_or(0);
        let mut pages_used = [false; 3];
        for item in nations.items(&t.id) {
            let p = item.slot / 36;
            let Some(k) = catalog.find(&item.id).filter(|k| makeable.contains(k)) else { continue };
            if p >= 3 {
                continue;
            }
            // Page flags 0x28cff78..7a: set by icons outside column 0.
            pages_used[p] |= !item.slot.is_multiple_of(12);
            let it = catalog.get(k);
            if p != usize::from(page) {
                continue;
            }
            let frame = objects.definition(&it.id).and_then(|d| d.icon()).and_then(|i| data.icon(i)).unwrap_or(0);
            let cost = acr_sim::economy::unit_cost(world, player, k, None);
            // `MAXINDEPO g n`: greyed while the player has `n` per depot of group `g`
            // (0x609abd); the click still works, the simulation does not cancel.
            let limited = objects
                .definition(&it.id)
                .and_then(|d| d.get("MAXINDEPO"))
                .and_then(|c| Some((c.int(0)?, c.int(1)?)))
                .is_some_and(|(g, n)| depots[(g.clamp(0, 3) + 1) as usize] >= depots[0] * n);
            let mut icon = Icon {
                frame,
                slot: item.slot % 36,
                outline: Some(outline),
                gray: limited || stock.iter().zip(&cost).any(|(s, c)| s < c),
                key: item.key,
                hint: cost_hint(unit_name(ctx, &it.md_name), &it.economy.price, &cost, ctx),
                ..Icon::default()
            };
            let sprite = objects.definition(&it.id).is_some_and(|d| d.has("SPRITEOBJECT"));
            if sprite && !objects.definition(&it.id).is_some_and(|d| d.has("WALLOBJECT")) {
                // Fields (`SPRITEOBJECT`): planted around the mill (command 0x3d).
                icon.action = Some(HudCommand::PlantFields(usize::from(k.0)));
            } else if it.building && !t.building {
                icon.action = Some(HudCommand::Place(usize::from(k.0)));
            } else {
                // `GetProduceCount` (0x49e060): an endless order counts 1200, any other 1.
                let queued: i32 = working.iter().flat_map(|b| &b.queue).filter(|q| q.kind == k).map(order_count).sum();
                icon.count = (queued > 0).then_some(queued);
                icon.life = working
                    .iter()
                    .find_map(|b| b.queue.first().filter(|q| q.kind == k).map(|q| (q.progress, q.stages.max(1))));
                icon.action = Some(HudCommand::Produce(usize::from(k.0)));
                icon.right = Some(HudCommand::Unproduce(usize::from(k.0)));
            }
            icons.retain(|i| i.slot != icon.slot);
            icons.push(icon);
        }
        // Upgrades (entries of kind 1 in 0x609230): those the buildings offer now, and the
        // research running in them, at their `#POSITION`.
        let tech = &catalog.tech;
        let mut upgrades: Vec<(acr_sim::tech::UpgradeId, Option<(i32, i32)>)> = Vec::new();
        for u in &working {
            if let Some(r) = &u.research {
                upgrades.retain(|e| e.0 != r.upgrade);
                upgrades.push((r.upgrade, Some((r.stage, r.stages.max(1)))));
            }
            for up in acr_sim::upgrade::available(world, u.id.0 as usize) {
                if !upgrades.iter().any(|e| e.0 == up) {
                    upgrades.push((up, None));
                }
            }
        }
        for (up, progress) in upgrades {
            let def = tech.upgrade(up);
            let (x, y) = def.position;
            if x < 0 || y < 0 {
                continue;
            }
            let slot = (x + 12 * y) as usize;
            let p = slot / 36;
            if p >= 3 {
                continue;
            }
            pages_used[p] |= !slot.is_multiple_of(12);
            if p != usize::from(page) {
                continue;
            }
            let mut icon = Icon {
                frame: data.icon(&def.icon).unwrap_or(0),
                slot: slot % 36,
                outline: Some(outline),
                gray: progress.is_none() && stock.iter().zip(&def.cost).any(|(s, c)| s < c),
                hint: upgrade_hint(ctx, &def.message, &def.cost),
                life: progress,
                ..Icon::default()
            };
            // A stage upgrade cannot start while the first selected object burns (OB+0x26
            // bit 1) or is a building (OB+0x27 bit 4: `BUILDING`, not `WALLOBJECT`, set at
            // 0x516212) with less than 2000 life: `MUSTREPAIR` (0x60988b; the text id has no
            // text, so the id is shown).
            let first = self.selected_units().next();
            let damaged = first.is_some_and(|u| {
                let wall = objects.definition(&world.catalog.get(u.kind).id).is_some_and(|d| d.has("WALLOBJECT"));
                u.burning || (u.building && !wall && u.life < 2000)
            });
            let stage =
                matches!(def.effect, acr_sim::tech::Effect::Building { category } if (24..29).contains(&category));
            if progress.is_some() {
                icon.right = Some(HudCommand::CancelUpgrade(up.0));
            } else if damaged && stage {
                icon.hint = ctx.texts.get("MUSTREPAIR").to_vec();
                icon.gray = true;
            } else {
                icon.action = Some(HudCommand::Upgrade(up.0));
            }
            icons.retain(|i| i.slot != icon.slot);
            icons.push(icon);
        }
        if !t.building {
            return;
        }
        if objects.definition(&t.id).is_some_and(|d| d.has("TORG")) {
            self.market_icons(ctx, data, icons);
        }
        // Occupants (0x5f5db9): a type whose MD defines `PAGE3` lists them on that page
        // (layout A, marking it used); the others get free slots in `build_view` (layout B).
        let def = objects.definition(&t.id);
        if def.is_some_and(|d| d.has("PAGE3")) {
            let inside = occupant_counts(world, &working);
            if !inside.is_empty() {
                pages_used[2] = true;
                if page == 2 {
                    for (i, (k, count)) in inside.into_iter().enumerate() {
                        let icon = self.occupant_icon(ctx, data, outline, k, count, occupant_slot(i));
                        icons.retain(|e| e.slot != icon.slot);
                        icons.push(icon);
                    }
                }
            }
        }
        // Page buttons in column 0 (`PAGE1..3` and `PAGE1HINT..` of the building, 0x609b18).
        if pages_used[1] || pages_used[2] {
            for (p, _) in pages_used.iter().enumerate().filter(|(_, used)| **used) {
                let word = |key: String| def.and_then(|d| d.get(&key)).and_then(|c| c.word(0)).map(str::to_owned);
                let Some(frame) = word(format!("PAGE{}", p + 1)).and_then(|w| data.icon(&w)) else { continue };
                let hint = word(format!("PAGE{}HINT", p + 1)).map(|h| ctx.texts.get(&h).to_vec()).unwrap_or_default();
                icons.retain(|i| i.slot != 12 * p);
                icons.push(Icon {
                    frame,
                    slot: 12 * p,
                    selected: p == usize::from(page),
                    hint,
                    action: Some(HudCommand::Page(p as u8)),
                    ..Icon::default()
                });
            }
        }
        // `[CANSETDEST]` buildings have a rally point.
        let can_rally = !tech.loaded || tech.of(kind).is_some_and(|t| t.can_set_dest);
        if can_rally && let Some(rally) = data.consts.get("SET_DEST") {
            icons.push(Icon {
                frame: rally.frame,
                slot: rally.slot,
                selected: self.interface.rally_mode,
                // `COMMENT.TXT` `SDS_Str` (the key sits beside `GOATTA` and `PATROL` in dmcr.exe);
                // the `NRES.DAT` hint name `Set_concentration_point` has no text.
                hint: ctx.texts.get("SDS_Str").to_vec(),
                action: Some(HudCommand::Rally),
                ..Icon::default()
            });
        }
    }

    /// The icon of `count` occupants of type `kind`; a click lets one out (`0x5f5a66`).
    fn occupant_icon(&self, ctx: &Ctx, data: &HudData, outline: u8, kind: TypeId, count: i32, slot: usize) -> Icon {
        let it = self.world.as_ref().expect("a world while the panel is built").catalog.get(kind);
        let frame =
            self.objects.objects().definition(&it.id).and_then(|d| d.icon()).and_then(|i| data.icon(i)).unwrap_or(0);
        Icon {
            frame,
            slot,
            outline: Some(outline),
            count: Some(count),
            hint: unit_name(ctx, &it.md_name),
            action: Some(HudCommand::Leave(usize::from(kind.0))),
            ..Icon::default()
        }
    }

    /// Layout B of 0x5f5db9: the occupants of a building type without a `PAGE3`, one icon
    /// per type in the first slot nothing else uses.
    fn free_occupant_icons(&self, ctx: &Ctx, data: &HudData, outline: u8, kind: TypeId, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let t = world.catalog.get(kind);
        let player = self.own();
        let has_page3 = self.objects.objects().definition(&t.id).is_some_and(|d| d.has("PAGE3"));
        if !t.building || has_page3 {
            return;
        }
        let working: Vec<&Unit> = self
            .selected_units()
            .filter(|u| u.kind == kind && u.owner == player && u.stage >= world.max_stage(u))
            .collect();
        for (k, count) in occupant_counts(world, &working) {
            let Some(slot) = (0..36).find(|s| !icons.iter().any(|i| i.slot == *s)) else { break };
            icons.push(self.occupant_icon(ctx, data, outline, k, count, slot));
        }
    }

    /// Production orders for the selected buildings (`Produce` 0x4d6226, `UnProduce`
    /// 0x4d6e08), five with Shift, one endless order with Ctrl (bit 0x2000 of the type,
    /// 0x4d6226). A new unit goes to the building with the shortest queue, a cancel to the
    /// one with the most of the type queued (an endless order counting 1200, so first).
    fn queue_orders(&mut self, kind: TypeId, produce: bool) {
        let player = self.own();
        let endless = produce && self.interface.mods.control_key();
        // Per building: the queue length and the counts of its orders of the type in the order
        // `UnProduce` (0x4dbd47) takes them, the waiting ones first, then the head.
        let mut queues: Vec<(EntityId, usize, Vec<i32>)> = self
            .selected_units()
            .filter(|u| u.building && u.owner == player)
            .map(|u| {
                let of_kind =
                    |i: usize, q: &acr_sim::economy::Production| (q.kind == kind).then(|| (i, order_count(q)));
                let mut taken: Vec<_> = u.queue.iter().enumerate().skip(1).filter_map(|(i, q)| of_kind(i, q)).collect();
                taken.extend(u.queue.first().and_then(|q| of_kind(0, q)));
                (u.id, u.queue.len(), taken.into_iter().map(|(_, n)| n).rev().collect())
            })
            .collect();
        let n = if self.interface.mods.shift_key() && !endless { SHIFT_REPEAT } else { 1 };
        for _ in 0..n {
            let pick = if produce {
                queues.iter_mut().min_by_key(|q| q.1)
            } else {
                queues.iter_mut().filter(|q| !q.2.is_empty()).max_by_key(|q| q.2.iter().sum::<i32>())
            };
            let Some(q) = pick else { break };
            let building = q.0;
            if produce {
                q.1 += 1;
                self.commands.push(Command::Produce { player, building, kind, endless });
            } else {
                q.1 -= 1;
                q.2.pop();
                self.commands.push(Command::Unproduce { player, building, kind });
            }
        }
    }

    /// Where the building being placed would stand at a world pixel (0x5142c2): the snapped
    /// position if it is free, else the nearest free one within 10 rings of 16 px cells
    /// (`0x28a6f10`, approximated by square rings); `None` if there is none. A mine's sites
    /// are those on a deposit (`construction::site`), so both the mouse and the touch outline
    /// snap to one and are refused on open ground.
    pub(super) fn placement_site(&self, kind: TypeId, x: i32, y: i32) -> Option<(i32, i32)> {
        let world = self.world.as_ref()?;
        let site = |x: i32, y: i32| acr_sim::construction::site(world, kind, x, y).map(|p| p.pixels());
        if let Some(p) = site(x, y) {
            return Some(p);
        }
        let (bx, by) = (x & !15, y & !15);
        for r in 1..10i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx.abs() == r || dy.abs() == r)
                        && let Some(p) = site(bx + 16 * dx, by + 16 * dy)
                    {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// What a right click at a window point would order the selection to do, with the
    /// cursor shown for it (0x4c4668..0x4c4ce2).
    pub(super) fn context_order(&self, at: [f32; 2]) -> Option<Context> {
        let world = self.world.as_ref()?;
        let own = self.own();
        let units: Vec<&Unit> = self.selected_units().filter(|u| u.owner == own && u.inside_of.is_none()).collect();
        if units.is_empty() {
            return None;
        }
        // Buildings only: the rally point of those that have one (`[CANSETDEST]`).
        if units.iter().all(|u| u.building) {
            let tech = &world.catalog.tech;
            let rally = units.iter().any(|u| !tech.loaded || tech.of(u.kind).is_some_and(|t| t.can_set_dest));
            let (x, y) = self.world_at(at);
            return rally.then_some(Context::Rally(x, y));
        }
        if let Some(target) = self.unit_at(at, |u| u.owner != own) {
            // With Ctrl, units go into another nation's building that takes units: they storm
            // it, or take it when it is empty (the door cursor, 0x4c4808).
            if self.interface.mods.control_key()
                && let Some(b) = world.unit(target).filter(|b| b.building)
                && units.iter().any(|u| !u.building && acr_sim::economy::can_enter(world, u, b))
            {
                return Some(Context::Storm(target));
            }
            return Some(Context::Attack(target));
        }
        if let Some(b) = self.unit_at(at, |u| u.owner == own && u.building) {
            let builders = units.iter().any(|u| world.catalog.get(u.kind).anims.contains_key("#WORK"));
            // The hammer unless Ctrl is held (0x4c49df).
            if builders
                && !self.interface.mods.control_key()
                && acr_sim::construction::needs_work(world, b.0 as usize, own)
            {
                return Some(Context::Repair(b));
            }
            let building = world.unit(b)?;
            if units.iter().any(|u| !u.building && acr_sim::economy::can_enter(world, u, building)) {
                return Some(Context::Enter(b));
            }
        }
        let (x, y) = self.world_at(at);
        Some(gather_context(world, &units, x, y).unwrap_or(Context::Move(x, y)))
    }

    /// Right click on the map: the context order (attack, repair, enter, gather, rally point,
    /// move) for the selection.
    pub(super) fn order_at(&mut self, at: [f32; 2]) {
        let Some(context) = self.context_order(at) else { return };
        let player = self.own();
        let units: Vec<EntityId> =
            self.selected_units().filter(|u| u.owner == player && !u.building).map(|u| u.id).collect();
        let command = match context {
            Context::Attack(target) => Command::Attack { player, units, target },
            Context::Repair(building) => Command::Repair { player, units, building },
            Context::Enter(building) | Context::Storm(building) => Command::Enter { player, units, building },
            Context::Gather(x, y, _) => Command::Gather { player, units, x, y },
            Context::Move(x, y) => {
                self.add_marker(x, y);
                if self.interface.attack_mode {
                    Command::AttackMove { player, units, x, y }
                } else {
                    Command::Move { player, units, x, y }
                }
            }
            Context::Rally(x, y) => {
                self.set_rally(x, y);
                return;
            }
        };
        self.commands.push(command);
        self.unit_sound(self.selected[0], "ORDER_SOUND");
    }

    /// Sets the rally point of the selected buildings (`SET_DEST`).
    fn set_rally(&mut self, x: i32, y: i32) {
        let player = self.own();
        let buildings: Vec<EntityId> =
            self.selected_units().filter(|u| u.owner == player && u.building).map(|u| u.id).collect();
        for building in buildings {
            self.commands.push(Command::Rally { player, building, x, y });
        }
        self.add_marker(x, y);
    }

    /// Runs a command of the interface.
    fn hud_command(&mut self, command: HudCommand) {
        // Modern controls: the army list picks like a file list (plain, Ctrl, Shift).
        if self.modern()
            && let HudCommand::ArmySelect { brigade, .. } = command
        {
            self.army_pick(brigade);
            return;
        }
        let player = self.own();
        let deselect = self.ctrl_deselects();
        let Some(world) = &self.world else { return };
        // Modern controls: Ctrl + click on a type or formation icon takes those units out.
        if deselect && let Some(units) = super::controls::icon_units(world, &self.selected, &command) {
            self.selected.retain(|id| !units.contains(id));
            return;
        }
        match command {
            HudCommand::SelectType(kind) => {
                let keep: Vec<EntityId> = self
                    .selected
                    .iter()
                    .copied()
                    .filter(|&id| world.unit(id).is_some_and(|u| u.kind.0 as usize == kind))
                    .collect();
                self.selected = keep;
            }
            HudCommand::SelectBrigade(b) => {
                // 0x4d737f: the formation's members only.
                self.selected.retain(|&id| world.unit(id).is_some_and(|u| u.brigade == Some(b)));
            }
            HudCommand::FormGroup(i) => self.interface.form_menu = (i + 1) as u16,
            HudCommand::FormSize(dec) => {
                let m = &mut self.interface.form_menu;
                if *m != 0 {
                    if dec {
                        *m += 0x100;
                    } else if *m > 0x100 {
                        *m -= 0x100;
                    }
                }
            }
            HudCommand::FormNear { shape, kind } => {
                let Some(officer) = self.interface.form_officer else { return };
                self.commands.push(Command::FormNear { player, officer, shape, kind: acr_sim::TypeId(kind) });
                // The interface selects the new formation once it exists (0x41f198).
                self.interface.select_formed = Some(officer);
            }
            HudCommand::Reform(group) => {
                let brigade = self.selected_units().find_map(|u| u.brigade);
                if let Some(b) = brigade {
                    self.commands.push(Command::Reform { player, brigade: b, group: group as u8 });
                }
            }
            HudCommand::Fill => {
                let brigade = self.selected_units().find_map(|u| u.brigade);
                if let Some(b) = brigade {
                    self.commands.push(Command::Fill { player, brigade: b });
                }
            }
            HudCommand::ArmyList => self.interface.hud.army_list = !self.interface.hud.army_list,
            HudCommand::ArmySelect { brigade, add } => {
                // `ImSelBrigade` (0x4e1365): without Shift the selection is cleared first; the
                // members from the third on join it.
                if !add {
                    self.selected.clear();
                }
                let Some(br) = world.brigades.get(usize::from(brigade)).filter(|br| br.alive && br.owner == player)
                else {
                    return;
                };
                for &m in br.members.iter().skip(2).flatten() {
                    if fighting_member(world, brigade, m).is_some() && !self.selected.contains(&m) {
                        self.selected.push(m);
                    }
                }
            }
            HudCommand::SelectIdle(mines) => {
                self.selected = idle(world, player, mines);
            }
            HudCommand::Group(group) => {
                // 0x6066f0 / 0x606700: network command 0x58 / 0x59 with the selection.
                let units = self.own_selected_units();
                self.commands.push(if group {
                    Command::GroupFormations { player, units }
                } else {
                    Command::UngroupFormations { player, units }
                });
            }
            HudCommand::Disband => {
                // One member per selected formation (the selection need not hold the officer).
                let mut members: Vec<(u16, EntityId)> = Vec::new();
                for u in self.selected_units() {
                    if let Some(b) = u.brigade.filter(|b| !members.iter().any(|m| m.0 == *b)) {
                        members.push((b, u.id));
                    }
                }
                for (_, officer) in members {
                    self.commands.push(Command::Disband { player, officer });
                }
            }
            HudCommand::Produce(kind) => {
                // 0x6088c0: the object limit, then `CanAfford` with its message.
                if world.units.iter().filter(|u| u.alive).count() > OBJECT_LIMIT {
                    self.interface.hints.push(Hint::UnitLimit);
                    return;
                }
                // Ctrl (0x4d6226) orders an endless unit, which is queued even when it cannot be
                // paid yet; the plain order needs `CanAfford` first.
                if self.interface.mods.control_key() || self.can_afford(TypeId(kind as u16)) {
                    self.queue_orders(TypeId(kind as u16), true);
                }
            }
            HudCommand::Unproduce(kind) => self.queue_orders(TypeId(kind as u16), false),
            HudCommand::Place(kind) => {
                // `CanAfford` (0x514c65) before entering the build mode.
                let kind = TypeId(kind as u16);
                // Walls are placed without a price check.
                let wall =
                    self.objects.objects().definition(&world.catalog.get(kind).id).is_some_and(|d| d.has("WALLOBJECT"));
                if wall || self.can_afford(kind) {
                    self.interface.placing = Some(kind);
                    self.interface.touch_outline = None;
                    self.interface.touch_tap = None;
                    self.interface.touch_grab = None;
                    self.interface.rally_mode = false;
                }
            }
            HudCommand::Page(page) => {
                let kind = self.selected_units().next().map(|u| u.kind);
                if let Some(kind) = kind {
                    self.interface.pages.insert(kind, page);
                }
            }
            HudCommand::Leave(kind) => {
                // Every selected building holding the type lets one unit of it out
                // (0x4de7c8 walks the selected buildings).
                let kind = TypeId(kind as u16);
                let buildings: Vec<EntityId> = self
                    .selected_units()
                    .filter(|u| u.building && u.inside.iter().any(|&i| world.unit(i).is_some_and(|o| o.kind == kind)))
                    .map(|u| u.id)
                    .collect();
                // 0x5f5a66 sends the command five times while Shift is held.
                let n = if self.interface.mods.shift_key() { SHIFT_REPEAT } else { 1 };
                self.commands.extend(leave_commands(player, &buildings, kind, n));
            }
            HudCommand::Rally => {
                self.interface.rally_mode = true;
                self.interface.placing = None;
            }
            HudCommand::Upgrade(up) => {
                let buildings: Vec<EntityId> =
                    self.selected_units().filter(|u| u.building && u.owner == player).map(|u| u.id).collect();
                let upgrade = acr_sim::tech::UpgradeId(up);
                self.commands.push(Command::Upgrade { player, buildings, upgrade });
            }
            HudCommand::CancelUpgrade(up) => {
                let buildings: Vec<EntityId> = self
                    .selected_units()
                    .filter(|u| u.owner == player && u.research.as_ref().is_some_and(|r| r.upgrade.0 == up))
                    .map(|u| u.id)
                    .collect();
                for building in buildings {
                    self.commands.push(Command::CancelUpgrade { player, building });
                }
            }
            HudCommand::MarketSell(slot) => self.interface.market_sell = Some(slot),
            HudCommand::MarketBuy(slot) => self.interface.market_buy = Some(slot),
            HudCommand::MarketAmount(delta) => {
                let delta = if self.interface.mods.shift_key() { delta * 100 } else { delta };
                self.interface.market_amount = (self.interface.market_amount + delta).max(0);
            }
            HudCommand::MarketClear => self.interface.market_amount = 0,
            HudCommand::MarketTrade => {
                let (Some(sell), Some(buy), Some(def)) =
                    (self.interface.market_sell, self.interface.market_buy, world.catalog.market.as_ref())
                else {
                    return;
                };
                let (sell, buy) = (def.res[sell] as u8, def.res[buy] as u8);
                self.commands.push(Command::Trade { player, sell, buy, amount: self.interface.market_amount });
            }
            HudCommand::PlantFields(kind) => {
                let buildings: Vec<EntityId> =
                    self.selected_units().filter(|u| u.building && u.owner == player).map(|u| u.id).collect();
                self.commands.push(Command::PlantFields { player, buildings, kind: TypeId(kind as u16) });
            }
            HudCommand::StandGround(on) => {
                let units = self.selected.clone();
                self.commands.push(Command::StandGround { player, units, on });
            }
            HudCommand::AttackMode => self.interface.attack_mode = !self.interface.attack_mode,
            // 0x606650: the next left click on the map sets the patrol point (0x4c28aa).
            HudCommand::PatrolMode => self.interface.patrol_mode = true,
            HudCommand::AttackGroundMode => self.interface.attack_ground_mode = true,
            HudCommand::AimGuides => self.interface.aim_guides = !self.interface.aim_guides,
            HudCommand::MortarZoneMode | HudCommand::MortarClearZones | HudCommand::MortarFire(_) => {
                self.mortar_command(&command)
            }
            HudCommand::HoldFire(hold) => {
                let units = self.own_selected_units();
                self.commands.push(Command::HoldFire { player, units, hold });
            }
            HudCommand::FriendlyFire(shoot_own) => {
                let units = self.own_selected_units();
                self.commands.push(Command::FriendlyFire { player, units, shoot_own });
            }
            HudCommand::MeleeOnly(on) => {
                let units = self.own_selected_units();
                self.commands.push(Command::MeleeOnly { player, units, on });
            }
            HudCommand::StopProduction(stop) => {
                let buildings = self.own_selected_units();
                self.commands.push(Command::StopProduction { player, buildings, stop });
            }
            HudCommand::Canister => {
                let units = self.own_selected_units();
                self.commands.push(Command::Canister { player, units });
            }
            HudCommand::LoadCanister => {
                let units = self.own_selected_units();
                self.commands.push(Command::LoadCanister { player, units });
            }
            HudCommand::StopFishing(stop) => {
                let units = self.own_selected_units();
                self.commands.push(Command::StopFishing { player, units, stop });
            }
            HudCommand::StopSowing(stop) => {
                let buildings = self.own_selected_units();
                self.commands.push(Command::StopSowing { player, buildings, stop });
            }
        }
    }

    /// The minimap's dark blocks (0x485c24): every 2 × 2 block whose fog value is 1300 or less.
    /// A block covers `64 << shift` map pixels; the fog cell at its first 128 pixels decides.
    fn fog_mask(&self) -> Option<Rc<Vec<bool>>> {
        let fog = self.mission.fog()?;
        let block = 64 << self.shift();
        let n = minimap::FOG_SIDE;
        let mask: Vec<bool> = (0..n * n)
            .map(|i| fog.get((i % n) * block + 64, (i / n) * block + 64) <= acr_sim::fog::MINIMAP_DARK)
            .collect();
        Some(Rc::new(mask))
    }

    /// Puts the production messages on the message list.
    fn post_hints(&mut self, ctx: &Ctx) {
        for hint in std::mem::take(&mut self.interface.hints) {
            let (text, frames) = match hint {
                Hint::UnitLimit => (ctx.texts.get("UNILIMIT").to_vec(), 50),
                Hint::Missing(missing) => {
                    let names: Vec<&[u8]> = missing.iter().map(|&r| ctx.texts.get(RESOURCE_NAMES[r])).collect();
                    let fmt = ctx.texts.get(if missing.len() > 1 { "NETRESS" } else { "NETRES1" });
                    (format_str(fmt, &names.join(&b","[..])), 70)
                }
                Hint::Finished(kind) => {
                    let Some(world) = &self.world else { continue };
                    let name = unit_name(ctx, &world.catalog.get(kind).md_name);
                    (format_str(ctx.texts.get("FINBLD"), &name), 100)
                }
                Hint::NotEnoughPeasants => (ctx.texts.get("NOT_ENOUGH_PEASANTS").to_vec(), 10),
            };
            self.mission.hint(text, frames);
        }
    }

    /// After `CreateBrigade` the new formation's flag bearer and soldiers join the selection
    /// (0x41f198, `SelectBrigade` 0x4e0f80 from member index 2 on). Called after a tick.
    pub(super) fn select_formed(&mut self) {
        let Some(officer) = self.interface.select_formed.take() else { return };
        let Some(world) = &self.world else { return };
        let Some(b) = world.unit(officer).and_then(|u| u.brigade) else { return };
        let members = world.brigades.get(usize::from(b)).map(|br| br.members.clone()).unwrap_or_default();
        for m in members.into_iter().skip(2).flatten() {
            if !self.selected.contains(&m) && world.unit(m).is_some_and(selectable) {
                self.selected.push(m);
            }
        }
    }

    /// `FINBLD` (0x49eeb7): an own building's queue became empty because its last unit was
    /// made (not cancelled). Called after `ticks` ticks.
    pub(super) fn watch_queues(&mut self, ticks: u32) {
        let Some(world) = &self.world else { return };
        let own = self.own();
        let mut now = HashMap::new();
        let mut peasants_missing = false;
        for u in world.units.iter().filter(|u| u.alive && u.building && u.owner == own) {
            // A `PEASANTCONVERTOR` that waits for a peasant to train a non-peasant tells
            // its owner while selected (0x49f16b: the selection mask `+0x22`).
            let catalog = &world.catalog;
            if catalog.get(u.kind).economy.peasant_convertor
                && u.research.is_none()
                && u.queue.first().is_some_and(|q| !catalog.get(q.kind).peasant)
                && !u
                    .inside
                    .iter()
                    .any(|&id| world.units.get(id.0 as usize).is_some_and(|i| catalog.get(i.kind).peasant))
                && self.selected.contains(&u.id)
            {
                peasants_missing = true;
            }
            let before = self.interface.queues.get(&u.id).copied();
            if let Some((_, progress, stages)) = before
                && u.queue.is_empty()
                && progress + 2 * ticks as i32 >= stages
            {
                self.interface.hints.push(Hint::Finished(u.kind));
            }
            if let Some(q) = u.queue.first() {
                now.insert(u.id, (u.queue.len(), q.progress, q.stages));
            }
        }
        self.interface.queues = now;
        if peasants_missing {
            self.interface.hints.push(Hint::NotEnoughPeasants);
        }
    }

    /// `CanAfford` (0x514c65): true if the player can pay for `kind`; otherwise the missing
    /// resources as `NETRESS`/`NETRES1` ("Nicht genügend Ressourcen %s", names joined by
    /// ",") on the message list for 70 frames.
    fn can_afford(&mut self, kind: TypeId) -> bool {
        let Some(world) = &self.world else { return false };
        let player = self.own();
        let cost = acr_sim::economy::unit_cost(world, player, kind, None);
        let stock = &world.players[usize::from(player)].resources;
        let missing: Vec<usize> = (0..RESOURCE_NAMES.len()).filter(|&r| stock[r] < cost[r]).collect();
        if missing.is_empty() {
            return true;
        }
        self.interface.hints.push(Hint::Missing(missing));
        false
    }

    /// The numbers of 0x607f50's info lines: units inside, housing (`FARM`), depot capacity
    /// (`ARTDEPO`), as `fn10` lines.
    fn info_lines(&self, ctx: &Ctx, u: &Unit) -> Vec<Vec<u8>> {
        let Some(world) = &self.world else { return Vec::new() };
        let t = world.catalog.get(u.kind);
        let def = self.objects.objects().definition(&t.id);
        let mut lines = Vec::new();
        let player = &world.players[usize::from(u.owner.min(7))];
        // `FISHING a b` (NM+0x2934): fish on board of the maximum. The simulation does not
        // fish yet, so the boat is always empty.
        if let Some(max) = def.and_then(|d| d.get("FISHING")).and_then(|c| c.int(1)).filter(|&m| m != 0) {
            lines.push(crate::hud::panels::format_ints(ctx.texts.get("FISHSTR"), &[0, max]));
        }
        if def.is_some_and(|d| d.has("FARM")) {
            lines.push(crate::hud::panels::format_int(ctx.texts.get("HABITAN"), player.population));
            lines.push(crate::hud::panels::format_int(ctx.texts.get("MAXHABI"), player.housing));
        }
        if let Some(c) = def.and_then(|d| d.get("ARTDEPO")) {
            let depots = self.depot_counts();
            for i in 0..4 {
                let id = format!("__ARTCAP{i}");
                let text = ctx.texts.get(&id);
                let cap = c.int(i).unwrap_or(0);
                if cap == 0 || text.first() == Some(&b'_') {
                    continue;
                }
                lines.push(crate::hud::panels::format_ints(text, &[depots[i + 1], depots[0] * cap]));
            }
        }
        let max_inside = world.adv(u.owner, u.kind).max_inside;
        if max_inside != 0 {
            lines.push(crate::hud::panels::format_int(ctx.texts.get("INSISTR"), u.inside.len() as i32));
            // `MAXINSI` "%d+%d" with the extra capacity of `SINGLE_INSIDE` (`AddInside`).
            if u.add_inside != 0 {
                lines.push(crate::hud::panels::format_ints(ctx.texts.get("MAXINSI"), &[max_inside, u.add_inside]));
            } else {
                lines.push(crate::hud::panels::format_int(ctx.texts.get("MAXINS1"), max_inside));
            }
            // A working mine names its resource (0x5f7882: the order 0x5f75b3, `RType`).
            if let Some(m) = &u.mine
                && let Some(name) = RESOURCE_NAMES.get(m.resource)
            {
                lines.push(ctx.texts.get(name).to_vec());
            }
        }
        lines
    }

    /// The census of 0x42ec4f for the unit limit: `[0]` the player's finished depots
    /// (`ARTDEPO`), `[g + 1]` its objects of `MAXINDEPO` group `g`.
    fn depot_counts(&self) -> [i32; 5] {
        let mut n = [0; 5];
        let Some(world) = &self.world else { return n };
        let objects = self.objects.objects();
        for u in world.units.iter().filter(|u| u.alive && u.owner == self.own()) {
            let Some(def) = objects.definition(&world.catalog.get(u.kind).id) else { continue };
            if def.has("ARTDEPO") && (!u.building || u.stage >= world.max_stage(u)) {
                n[0] += 1;
            }
            if let Some(g) = def.get("MAXINDEPO").and_then(|c| c.int(0)) {
                n[(g.clamp(0, 3) + 1) as usize] += 1;
            }
        }
        n
    }

    /// The market's icons (`ECO::BuildIcons` 0x4a96d9) at fixed slots: per market slot `i` the
    /// sell icon at `(i >> 1)·12 + (i & 1)` and the buy icon 7 slots right, both highlighted
    /// when chosen; the amount buttons, trade and clear (`ECONOMY.TXT` icon names).
    fn market_icons(&self, ctx: &Ctx, data: &HudData, icons: &mut Vec<Icon>) {
        let Some(world) = &self.world else { return };
        let Some(def) = world.catalog.market.as_ref() else { return };
        for (i, &res) in def.res.iter().enumerate() {
            let Some(frame) = MARKET_RESOURCES.get(res).and_then(|n| data.icon(n)) else { continue };
            let hint = ctx.texts.get(RESOURCE_NAMES.get(res).copied().unwrap_or("")).to_vec();
            let sell = (i >> 1) * 12 + (i & 1);
            for (slot, action, chosen) in [
                (sell, HudCommand::MarketSell(i), self.interface.market_sell == Some(i)),
                (sell + 7, HudCommand::MarketBuy(i), self.interface.market_buy == Some(i)),
            ] {
                icons.retain(|x| x.slot != slot);
                icons.push(Icon {
                    frame,
                    slot,
                    selected: chosen,
                    hint: hint.clone(),
                    action: Some(action),
                    ..Icon::default()
                });
            }
        }
        for (slot, name, hint, action) in MARKET_BUTTONS.iter().cloned() {
            let Some(frame) = data.icon(name) else { continue };
            icons.retain(|x| x.slot != slot);
            icons.push(Icon {
                frame,
                slot,
                hint: ctx.texts.get(hint).to_vec(),
                action: Some(action),
                ..Icon::default()
            });
        }
    }

    /// Draw layers of the game screen: the overlays over the map (world coordinates), the
    /// selection rectangle (window coordinates) and the interface (canvas coordinates);
    /// plus the images that changed (minimap, portrait) and the cursor frame with its hotspot.
    #[allow(clippy::type_complexity)]
    pub fn interface(
        &mut self,
        ctx: &Ctx,
        window: [f32; 2],
        seconds: f32,
    ) -> (Vec<(Canvas, Vec<Draw>)>, Vec<(&'static str, Image)>, Option<(usize, [i32; 2])>) {
        self.interface.canvas = Canvas::screen(window);
        self.interface.window = window;
        self.interface.hud.set_screen(self.interface.canvas.size());
        self.interface.hud.set_density(self.interface.scale_factor / self.interface.canvas.scale);
        self.diplomacy.set_screen(self.interface.hud.screen());
        if !self.interface.hud.has_minimap()
            && let (Some(map), Some(assets)) = (&self.map, &self.assets)
            && let Some(m) = Minimap::new(map, assets)
        {
            self.interface.hud.set_minimap(m);
        }
        if !self.interface.cursor_loaded && ctx.lib.settled(cursor::FILE) {
            if let Some(bytes) = ctx.lib.bytes(cursor::FILE) {
                self.interface.cursor = Cursor::parse(&bytes);
            }
            self.interface.cursor_loaded = true;
        }
        let _ = self.keyboard(ctx);
        self.post_hints(ctx);
        let view = self.build_view(ctx, window);
        let hud_layers = self.interface.hud.draw(ctx, &view);
        self.interface.touch_view =
            if self.interface.claim.touch { self.interface.hud.touch_view(ctx, &view) } else { None };
        if let Some(touch) = &mut self.interface.touch_view {
            touch.speed = 100 - self.frame_time.clamp(0, 100);
        }
        let mut hud_draws = Vec::new();
        self.dip_draw(ctx, &mut hud_draws);
        if self.interface.paused && self.mission.ended.is_none() {
            pause_text(ctx, self.interface.hud.screen(), &mut hud_draws);
        }
        let mut images: Vec<(&'static str, Image)> =
            self.interface.hud.minimap_image(ctx, &view).map(|i| (hud::MINIMAP_IMAGE, i)).into_iter().collect();
        images.extend(self.interface.hud.take_images());
        self.interface.view = view;
        // The cursor (0x4c1f70): the arrow, the sword over an enemy with an own selection.
        let over_hud = self.cursor.is_some_and(|c| self.over_hud(ctx, c));
        let modal = self.interface.placing.is_some() || self.interface.rally_mode;
        let frame = match self.cursor.filter(|_| !over_hud && !modal).and_then(|c| self.context_order(c)) {
            // The patrol point cursor (`0x66a90c`, 0x4c5311).
            _ if self.interface.patrol_mode && !over_hud => cursor::frame::PATROL,
            // The attack-ground cursor (`0x66a910`, 0x4c24c3).
            _ if self.interface.attack_ground_mode && !over_hud => cursor::frame::ATTACK_GROUND,
            // Mortars rule: the zone mode takes the same cursor.
            _ if self.interface.mortar.on && !over_hud => cursor::frame::ATTACK_GROUND,
            Some(context) => context.cursor(),
            None => cursor::frame::ARROW,
        };
        self.interface.cursor.set(frame);
        self.interface.cursor.tick(f64::from(seconds));
        let c = &self.interface.cursor;
        let cursor = Some((c.frame, c.hotspot()));
        let mut layers = vec![(self.world_canvas(), self.world_overlays(ctx))];
        if let Some(rect) = self.selection_rectangle(ctx) {
            layers.push((Canvas::new(1.0, [0.0, 0.0]), rect));
        }
        // The panels at their `hud_scale`, then the dialogs and the pause text at the canvas'.
        let canvas = self.interface.canvas;
        self.interface.claimed.clear();
        for section in hud_layers {
            if self.interface.claim.claims(section.layer) {
                self.interface.claimed.extend(hud::claimed(section));
            } else {
                layers.push((section.zoom.canvas(canvas), section.draws));
            }
        }
        layers.push((canvas, hud_draws));
        (layers, images, cursor)
    }

    /// Whether a window point is over the interface.
    pub(super) fn over_hud(&self, ctx: &Ctx, at: [f32; 2]) -> bool {
        let p = self.canvas_point(at);
        self.interface.hud.covers(ctx, &self.interface.view, p) || self.diplomacy.covers(p)
    }

    /// Window = (world screen position − camera) · scale.
    fn world_canvas(&self) -> Canvas {
        let s = self.camera.scale;
        Canvas::new(s, [-self.camera.x * s, -self.camera.y * s])
    }

    /// Selection brackets, life bars (Tab) and the move markers, in world screen pixels.
    fn world_overlays(&self, ctx: &Ctx) -> Vec<Draw> {
        let mut out = Vec::new();
        let (Some(world), Some(terrain), Some(palette)) = (&self.world, &self.terrain, ctx.lib.palette(GAME_PALETTE))
        else {
            return out;
        };
        let rgba = |i: u8| {
            let [r, g, b] = palette.rgb(i);
            [r, g, b, 255]
        };
        let bracket = rgba(nearest(&palette, color::BRACKETS));
        let green = rgba(nearest(&palette, color::LIFE));
        let white = rgba(color::WHITE);
        let reload = rgba(color::RELOAD);
        let fill = |out: &mut Vec<Draw>, c: [u8; 4], x0: i32, y0: i32, x1: i32, y1: i32| {
            out.push(Draw::fill(c, Rect { x0, y0, x1: x1 + 1, y1: y1 + 1 }).unclipped());
        };
        let objects = self.objects.objects();
        let own = self.own();
        // Corpses get neither brackets nor a life bar (0x4bb3b1 skips objects with `Sdoxlo`).
        for u in world.units.iter().filter(|u| u.living() && u.inside_of.is_none()) {
            let selected = u.owner == own && self.selected.contains(&u.id);
            if !selected && !self.interface.hud.life_bars {
                continue;
            }
            // Tab shows what the player sees: no bar over a stranger in the fog of war.
            if hidden_in_fog(self.mission.fog(), world, u, own) {
                continue;
            }
            let kind = world.catalog.get(u.kind);
            // `RECTANGLE dx dy w h` (defaults 0, 3, 40, 64; 0x4bad90).
            let r = objects.definition(&kind.id).and_then(|d| d.get("RECTANGLE"));
            let [dx, dy, w, h] = [0, 1, 2, 3].map(|i| r.and_then(|c| c.int(i)).unwrap_or([0, 3, 40, 64][i]));
            let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
            // A model is drawn where its body walks between the sim's ticks: the marks follow it.
            let [ox, oy] = self.three_d.drawn_offset(u.id);
            let x0 = x + ox - w / 2 + dx;
            let y0 = ((y + oy) >> 1) - h + dy - terrain.height_at(x, y);
            let (x1, y1) = (x0 + w - 1, y0 + h - 1);
            // Soldiers of a formed formation get no brackets of their own (0x4bae50).
            let in_formation = u.brigade.is_some() && !kind.officer;
            if selected && !u.building && !in_formation {
                for (cx, cy, sx, sy) in [(x0, y0, 1, 1), (x1, y0, -1, 1), (x0, y1, 1, -1), (x1, y1, -1, -1)] {
                    fill(&mut out, bracket, cx.min(cx + 2 * sx), cy, cx.max(cx + 2 * sx), cy);
                    fill(&mut out, bracket, cx, cy.min(cy + 2 * sy), cx, cy.max(cy + 2 * sy));
                }
            }
            if self.interface.hud.life_bars {
                // Life bar (0x4bb390): two green lines above the box, white end caps. Buildings
                // get one too (0x4bb390 and the box 0x4bad90 test no building flag; houses
                // and forts have `RECTANGLE` and, if they shoot, `SHOWDELAY`), measured
                // against their `MaxLife`, which the building stages raise.
                let max = if u.building { u.max_life } else { kind.life }.max(1);
                let l = (w - 12) * u.life.clamp(0, max) / max;
                fill(&mut out, green, x0 + 5, y0 - 12, x0 + l + 7, y0 - 11);
                // `SHOWDELAY` types (NM+0x172a bit 0, 0x4bb51a): the reload bar, two lines of
                // palette index 0xd0 right under it, as long as the part of the reload done:
                // `(w − 12)·(D − delay)/D` with `D = max(MaxDelay or 1, delay)`. The end
                // caps then reach down to it.
                let bottom = if objects.definition(&kind.id).is_some_and(|d| d.has("SHOWDELAY")) {
                    let d = if u.max_delay != 0 { u.max_delay } else { 1 }.max(u.delay);
                    let r = (w - 12) * (d - u.delay) / d;
                    fill(&mut out, reload, x0 + 5, y0 - 10, x0 + r + 7, y0 - 9);
                    y0 - 9
                } else {
                    y0 - 11
                };
                fill(&mut out, white, x0 + 4, y0 - 12, x0 + 4, bottom);
                fill(&mut out, white, x0 + w - 5, y0 - 12, x0 + w - 5, bottom);
                if u.building {
                    // The garrison label (0x4bb6f8, buildings only: OB+0x27 bit 4): an 8 × 8
                    // square in the owner's colour `0xd0 + 4·owner` at (x0 + 5, y0 − 22) and
                    // the count 12 px right of it, its bottom 4 px above the reload bar's
                    // row (y0 − 10), in `SmallWhiteFont` (0x92dc4c).
                    let owner = rgba(color::PLAYER.wrapping_add(4 * u.owner));
                    fill(&mut out, owner, x0 + 5, y0 - 22, x0 + 12, y0 - 15);
                    let seen = world.nation_mask[usize::from(own) & 7] & u.nmask != 0;
                    let capacity = (world.adv(u.owner, u.kind).max_inside & 0xFFFF) + u.add_inside;
                    let text = inside_label(seen, u.inside.len(), capacity);
                    let font = hud::font::SMALL_WHITE;
                    let top = y0 - 14 - ctx.fonts.glyph_height(ctx.lib, font, b'C');
                    out.push(Draw::game_text(font, &text, x0 + 17, top).unclipped());
                }
            }
        }
        if !self.interface.hud.zoom {
            self.formation_marks(world, terrain, &palette, &mut out);
        }
        // Panic shouts (0x600320); the zoomed-out view draws none.
        let lines: [Vec<u8>; 8] = std::array::from_fn(|i| ctx.texts.get(&format!("FEAR_RNDM{i}")).to_vec());
        let measure = |f, t: &[u8]| (ctx.fonts.width(ctx.lib, f, t), ctx.fonts.height(ctx.lib, f));
        let shouts = self.shouts.borrow_mut().update(&lines, measure);
        if !self.interface.hud.zoom {
            out.extend(shouts);
        }
        if self.interface.hud.locks && !self.interface.hud.zoom {
            self.lock_overlay(&palette, &mut out);
        }
        if let Some((_, (sx, sy))) = self.interface.right_drag {
            self.facing_arrow(bracket, terrain.height_at(sx, sy), &mut out);
        }
        // Interface marks: hidden with the interface (the driver's `hud` off).
        // The "Zielhilfe" icon hides the guides; the zone being set still follows the cursor.
        if !self.interface.hud.zoom && !self.hud_hidden {
            self.gun_marks(ctx, world, terrain, &palette, &mut out);
        }
        for m in &self.interface.markers {
            let h = terrain.height_at(m.x, m.y);
            out.push(
                Draw::game_frame(hud::gp::CURSGO, usize::from(m.frame), m.x - 24, (m.y >> 1) - 17 - h, 0).unclipped(),
            );
        }
        // The building being placed (0x4bdd1e): pulsing dark where it fits
        // (`ShowGPDarkN`), red where it does not (`ShowGPRedN`), with
        // N = 5 + (sin(t / 100 ms) + 2)·8 (0x4bde42).
        let outline = if self.interface.touch {
            Some(self.touch_outline_at())
        } else {
            self.cursor.filter(|&c| !self.over_hud(ctx, c))
        };
        if let Some(kind) = self.interface.placing
            && let Some(at) = outline
        {
            let (x, y) = self.world_at(at);
            let ((sx, sy), table) = match self.placement_site(kind, x, y) {
                Some(p) => (p, PREVIEW_FREE),
                None => ((x, y), PREVIEW_BLOCKED),
            };
            let t = self.interface.started.elapsed().as_secs_f64() * 10.0;
            let row = (5.0 + (t.sin() + 2.0) * 8.0) as u8;
            self.field_preview(world, terrain, kind, (sx, sy), &mut out);
            // The packs come from the library: the type may not be on the map yet.
            let def = objects.definition(&world.catalog.get(kind).id);
            let look = def.and_then(|d| {
                let anim = acr_render::scene::idle_animation(d)?;
                let frames = |p: &str| ctx.lib.sprites(p).map(|g| g.frame_count());
                // The preview shows the sections' state-0 frames (0x4bdeaf) and the first
                // `#WORK` frame (0x4be467) too.
                let mut look = acr_render::scene::animation_look(d, anim, 0, 0, Some(0), frames)?;
                look.parts.extend(acr_render::scene::work_part(d, 0, frames));
                Some(look)
            });
            if let Some(look) = look {
                let h = terrain.height_at(sx, sy);
                // Touch: the outline blinks (#439).
                let alpha = if self.interface.touch { outline_alpha(t / 10.0) } else { 1.0 };
                for ((gp, frame), (ox, oy), _, _) in look.parts {
                    let mut d = Draw::game_frame_remap(&gp, frame, sx + ox, (sy >> 1) + oy - h, 0, table, row);
                    d = d.unclipped();
                    d.alpha = alpha;
                    out.push(d);
                }
            }
        }
        out
    }

    /// The area the fields of the building being placed would cover once grown (ACR's
    /// addition; the original shows only the building): plots of about 64 map pixels, each
    /// an inset diamond with a light fill and a firmer outline, green where most of its
    /// field points can grow a field and red where most cannot (buildings, water, trees,
    /// slopes). The points lie only `step` (4) px apart, too dense to draw one by one.
    /// Nothing for a building that grows no fields.
    fn field_preview(
        &self,
        world: &acr_sim::World,
        terrain: &acr_render::terrain::TerrainData,
        kind: TypeId,
        (x, y): (i32, i32),
        out: &mut Vec<Draw>,
    ) {
        let spots = acr_sim::sprite::field_footprint(world, kind, x, y);
        let Some(step) = spots.first().map(|s| s.step.max(1)) else {
            return;
        };
        // The points sit on a diamond lattice: (i + j, i - j) steps from the building.
        let block = (64 / step).max(1);
        let mut plots: std::collections::BTreeMap<(i32, i32), (i32, i32)> = std::collections::BTreeMap::new();
        for s in &spots {
            let (u, v) = ((s.x - x) / step, (s.y - y) / step);
            let (i, j) = ((u + v) / 2, (u - v) / 2);
            let at = ((i + block / 2).div_euclid(block), (j + block / 2).div_euclid(block));
            let e = plots.entry(at).or_default();
            e.0 += i32::from(s.free);
            e.1 += 1;
        }
        let (fill, edge) = if self.interface.touch { (0.12, 0.45) } else { (0.15, 0.5) };
        let span = block * step;
        for ((bi, bj), (free, count)) in plots {
            let color = if free * 2 > count { FIELD_FREE } else { FIELD_BLOCKED };
            let (px, py) = (x + (bi + bj) * span, y + (bi - bj) * span);
            let (cx, cy) = (px, (py >> 1) - terrain.height_at(px, py));
            // The plot is a diamond `2 * span` wide and `span` high on screen (the map's y is
            // doubled); the drawn one is inset, leaving a gap to its neighbours.
            let hw = span * 7 / 8;
            let hh = hw / 2;
            for r in -hh..=hh {
                let w = hw * (hh - r.abs()) / hh.max(1);
                out.push(Draw::fill(color, Rect::new(cx - w, cy + r, 2 * w, 1)).unclipped().with_alpha(fill));
            }
            let corners = [(cx, cy - hh), (cx + hw, cy), (cx, cy + hh), (cx - hw, cy)];
            for i in 0..4 {
                out.push(Draw::line(color, corners[i], corners[(i + 1) % 4]).with_alpha(edge));
            }
        }
    }

    /// The selected formations' marks (0x41c93f, called by the map drawer 0x4bd613 for the
    /// player's brigades with a selected member; not in the zoomed-out view): every soldier
    /// (members from the fourth on that are not dying) gets a diamond 12 pixels wide on the
    /// ground around it, and while the formation keeps its positions (0x41c0a9, after a move
    /// order) a cross 2 pixels wide at the slot it walks to. Both in
    /// `GetPaletteColor(255, 255, 0)`, raw palette index 0xC9 in formation melee (0x41e50c).
    /// The lines follow the ground height at their ends (0x41c75b). The bounding box after
    /// the loop is never drawn (a jump skips it, 0x41cbfc). ACR draws them one window pixel
    /// wide at every zoom ([`Draw::line`]): the original's pixels at zoom 1, crisp beyond.
    fn formation_marks(
        &self,
        world: &acr_sim::World,
        terrain: &acr_render::terrain::TerrainData,
        palette: &acr_formats::Palette,
        out: &mut Vec<Draw>,
    ) {
        let own = self.own();
        let rgba = |i: u8| {
            let [r, g, b] = palette.rgb(i);
            [r, g, b, 255]
        };
        let yellow = rgba(nearest(palette, color::BRACKETS));
        let mut brigades: Vec<u16> = self
            .selected
            .iter()
            .filter_map(|&id| world.unit(id))
            .filter(|u| u.owner == own)
            .filter_map(|u| u.brigade)
            .collect();
        brigades.sort_unstable();
        brigades.dedup();
        let at = |x: i32, y: i32| (x, (y >> 1) - terrain.height_at(x, y));
        for b in brigades {
            let Some(br) = world.brigades.get(usize::from(b)).filter(|br| br.alive && br.members.len() > 3) else {
                continue;
            };
            if br.slots.is_empty() {
                continue;
            }
            let order = br.orders.last();
            let keep = matches!(order, Some(acr_sim::formation::BOrder::Keep));
            let c = if matches!(order, Some(acr_sim::formation::BOrder::Bitva(_))) { rgba(0xc9) } else { yellow };
            for (k, m) in br.members.iter().enumerate().skip(3) {
                let Some(u) = m.and_then(|m| world.unit(m)).filter(|u| u.living() && u.brigade == Some(b)) else {
                    continue;
                };
                if keep && let Some(&(sx, sy)) = br.slots.get(k) {
                    out.push(Draw::line(c, at(sx, sy - 2), at(sx, sy + 2)));
                    out.push(Draw::line(c, at(sx - 2, sy), at(sx + 2, sy)));
                }
                let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
                out.push(Draw::line(c, at(x - 12, y), at(x, y - 12)));
                out.push(Draw::line(c, at(x - 12, y), at(x, y + 12)));
                out.push(Draw::line(c, at(x + 12, y), at(x, y - 12)));
                out.push(Draw::line(c, at(x + 12, y), at(x, y + 12)));
            }
        }
    }

    /// What the selected own guns' fire modes cover on the ground (`gun_marks.rs`): their
    /// current attack or facing, or, while aiming, the ground point under the cursor (the
    /// attack-ground mode) or the enemy under it.
    fn gun_marks(
        &self,
        ctx: &Ctx,
        world: &acr_sim::World,
        terrain: &acr_render::terrain::TerrainData,
        palette: &acr_formats::Palette,
        out: &mut Vec<Draw>,
    ) {
        use super::gun_marks::{self, Aim, Colours};
        let own = self.own();
        let rgba = |i: u8| {
            let [r, g, b] = palette.rgb(i);
            [r, g, b, 255]
        };
        let cs = Colours {
            mark: rgba(nearest(palette, color::BRACKETS)),
            dark: rgba(nearest(palette, [40, 40, 0])),
            grey: rgba(nearest(palette, [150, 150, 150])),
        };
        let cursor = self.cursor.filter(|&c| !self.over_hud(ctx, c));
        let aiming = cursor.and_then(|c| {
            if self.interface.attack_ground_mode {
                let (x, y) = self.world_at(c);
                return Some(Aim { to: Some((x, y)), point: true, building: false });
            }
            let e = world.unit(self.unit_at(c, |u| u.owner != own)?)?;
            Some(Aim { to: Some(e.pos.pixels()), point: false, building: e.building })
        });
        let at = |x: f64, y: f64| {
            let (x, y) = (x as i32, y as i32);
            (x, (y >> 1) - terrain.height_at(x, y))
        };
        let guides = self.interface.aim_guides;
        for u in self.selected_units().filter(|u| guides && u.owner == own && u.inside_of.is_none()) {
            let Some(s) = world.catalog.get(u.kind).sector_weapon else { continue };
            let Some(aim) = aiming.or_else(|| gun_marks::current(world, u)) else { continue };
            if let Some(m) = gun_marks::mark(world, u, aim) {
                gun_marks::draw(&m, &s, cs, &at, out);
            }
        }
        self.mortar_marks(world, cs, &at, cursor, guides, out);
    }

    /// The inaccessible areas (key Q, 0x518306): every blocked 16 × 16 cell of the land
    /// lock map in view gets its two diagonals in colour 0xfe and, towards free neighbours,
    /// its edges in colour 0xff, following the ground height at the corners. Cells outside
    /// the map count as blocked.
    fn lock_overlay(&self, palette: &acr_formats::Palette, out: &mut Vec<Draw>) {
        let (Some(world), Some(terrain)) = (&self.world, &self.terrain) else { return };
        let locks = &world.map.land;
        let rgba = |i: u8| {
            let [r, g, b] = palette.rgb(i);
            [r, g, b, 255]
        };
        let (cross, edge) = (rgba(0xfe), rgba(0xff));
        let s = self.camera.scale;
        let window = self.window_size();
        let (x0, y0) = ((self.camera.x / 16.0).floor() as i32 - 1, (self.camera.y / 8.0).floor() as i32 - 1);
        let (x1, y1) = (x0 + (window[0] / s / 16.0) as i32 + 3, y0 + (window[1] / s / 8.0) as i32 + 3);
        // Hills lift cells into the view from below.
        let y1 = y1 + 64;
        let h = |cx: i32, cy: i32| terrain.height_at(cx * 16, cy * 16);
        for cy in y0.max(-1)..=y1.min(locks.height) {
            for cx in x0.max(-1)..=x1.min(locks.width) {
                if !locks.check_pt(cx, cy) {
                    continue;
                }
                let (sx, sy) = (cx * 16, cy * 8);
                let (h00, h10, h01, h11) = (h(cx, cy), h(cx + 1, cy), h(cx, cy + 1), h(cx + 1, cy + 1));
                line(out, cross, (sx, sy - h00), (sx + 16, sy + 8 - h11));
                line(out, cross, (sx, sy + 8 - h01), (sx + 16, sy - h10));
                if !locks.check_pt(cx + 1, cy) {
                    line(out, edge, (sx + 16, sy - h10), (sx + 16, sy + 8 - h11));
                }
                if !locks.check_pt(cx - 1, cy) {
                    line(out, edge, (sx, sy - h00), (sx, sy + 8 - h01));
                }
                if !locks.check_pt(cx, cy - 1) {
                    line(out, edge, (sx, sy - h00), (sx + 16, sy - h10));
                }
                if !locks.check_pt(cx, cy + 1) {
                    line(out, edge, (sx, sy + 8 - h01), (sx + 16, sy + 8 - h11));
                }
            }
        }
    }

    /// The drag rectangle (0x4c4356), 1 pixel in `GetPaletteColor(0, 255, 255)`.
    fn selection_rectangle(&self, ctx: &Ctx) -> Option<Vec<Draw>> {
        let (Some(from), Some(at)) = (self.drag_from, self.cursor) else { return None };
        let palette = ctx.lib.palette(GAME_PALETTE)?;
        let [red, green, blue] = palette.rgb(nearest(&palette, color::RECTANGLE));
        let r = Rect {
            x0: from[0].min(at[0]) as i32,
            y0: from[1].min(at[1]) as i32,
            x1: from[0].max(at[0]) as i32 + 1,
            y1: from[1].max(at[1]) as i32 + 1,
        };
        let mut out = Vec::new();
        hud::outline(&mut out, r, [red, green, blue, 255]);
        Some(out.into_iter().map(|d| d.unclipped()).collect())
    }

    /// The unit whose box (`RECTANGLE dx dy w h`, as for the selection brackets) contains
    /// the window point, the front-most if several do, among the living units passing `keep`.
    pub(super) fn unit_at(&self, at: [f32; 2], keep: impl Fn(&Unit) -> bool) -> Option<EntityId> {
        let world = self.world.as_ref()?;
        let s = self.camera.scale;
        let (px, py) = ((self.camera.x + at[0] / s) as i32, (self.camera.y + at[1] / s) as i32);
        world
            .units
            .iter()
            .filter(|u| selectable(u) && u.inside_of.is_none() && keep(u))
            .filter(|u| {
                let (x0, y0, w, h) = self.unit_box(u);
                (x0..x0 + w).contains(&px) && (y0..y0 + h).contains(&py)
            })
            .max_by_key(|u| u.pos.y)
            .map(|u| u.id)
    }

    /// A unit's screen box (0x4bad90) in world screen pixels: `RECTANGLE dx dy w h`
    /// (defaults 0, 3, 40, 64) above its ground point.
    pub(super) fn unit_box(&self, u: &Unit) -> (i32, i32, i32, i32) {
        let (Some(world), Some(terrain)) = (&self.world, &self.terrain) else { return (0, 0, 0, 0) };
        let r = self.objects.objects().definition(&world.catalog.get(u.kind).id).and_then(|d| d.get("RECTANGLE"));
        let [dx, dy, w, h] = [0, 1, 2, 3].map(|i| r.and_then(|c| c.int(i)).unwrap_or([0, 3, 40, 64][i]));
        let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
        if self.three_d.orbit().is_some() {
            // The free camera: the box stands at the projected ground point.
            let [sx, sy] = self.screen_of(u);
            let s = self.camera.scale;
            let (gx, gy) = ((self.camera.x + sx / s).round() as i32, (self.camera.y + sy / s).round() as i32);
            return (gx - w / 2 + dx, gy - h + dy, w, h);
        }
        (x - w / 2 + dx, (y >> 1) - h + dy - terrain.height_at(x, y), w, h)
    }

    /// Left button. Returns what the app should do.
    pub fn left_button(&mut self, ctx: &Ctx, pressed: bool) -> GameMouse {
        if self.editor_left(ctx, pressed) {
            return GameMouse::None;
        }
        // The modern controls: Ctrl + left drag on the map pans; without moving it is a
        // plain click.
        if pressed
            && self.modern()
            && self.interface.mods.control_key()
            && self.drag_from.is_none()
            && self.interface.modern.pan.is_none()
            && self.cursor.is_some_and(|c| !self.over_hud(ctx, c))
        {
            self.start_pan(super::controls::PanButton::CtrlLeft);
            return GameMouse::None;
        }
        if !pressed && let Some(click) = self.end_ctrl_pan() {
            if click && self.ctrl_click_deselect() {
                return GameMouse::None;
            }
            if click {
                self.left_click(ctx, true);
                return self.left_click(ctx, false);
            }
            return GameMouse::None;
        }
        self.left_click(ctx, pressed)
    }

    /// A click of the touch skin on the interface without a pixel: the same command or
    /// camera move the press ends in (`hud_input` may be off). Nothing happens when it is
    /// refused.
    pub fn hud_event(&mut self, intent: HudIntent) -> Result<GameMouse, Reject> {
        let window = self.window_size();
        match intent {
            HudIntent::Icon { id, right } => {
                let event = Hud::icon_click(&self.interface.view, id, right)?;
                if right {
                    self.hud_right_event(&event);
                    return Ok(GameMouse::None);
                }
                Ok(self.hud_left_event(event, [window[0] / 2.0, window[1] / 2.0]).unwrap_or(GameMouse::None))
            }
            HudIntent::Bar { id } => {
                let event = match id {
                    "bar/menu" => HudEvent::GameMenu,
                    "bar/pause" => HudEvent::TogglePause,
                    _ => return Err(Reject::NoWidget),
                };
                Ok(self.hud_left_event(event, [window[0] / 2.0, window[1] / 2.0]).unwrap_or(GameMouse::None))
            }
            HudIntent::Minimap { uv, right } => {
                let px = self.interface.hud.minimap_pixel(&self.interface.view, uv)?;
                if right {
                    self.hud_right_event(&HudEvent::Order(px));
                } else {
                    self.centre_on_minimap(px, window);
                }
                Ok(GameMouse::None)
            }
        }
    }

    /// Whether pixels reach the HUD ([`HudIntent`]s are the other way in).
    pub fn set_hud_input(&mut self, on: bool) {
        self.interface.hud_input = on;
    }

    /// What the game does with the interface's answer to a left press at window point `at`;
    /// `None` when the interface did not take it and the map gets it.
    fn hud_left_event(&mut self, event: HudEvent, at: [f32; 2]) -> Option<GameMouse> {
        match event {
            HudEvent::None => return None,
            HudEvent::Camera(px) => {
                let window = self.window_size();
                self.centre_on_minimap(px, window);
            }
            HudEvent::Command(c) => self.hud_command(c),
            HudEvent::GameMenu => return Some(GameMouse::Menu),
            HudEvent::Restart => return Some(GameMouse::Restart),
            HudEvent::History => return Some(GameMouse::History),
            HudEvent::Surrender => self.mission.surrender(),
            HudEvent::TogglePause => self.set_paused(!self.paused()),
            HudEvent::Escape => return Some(GameMouse::Escape),
            HudEvent::Zoom => self.toggle_zoom(at),
            HudEvent::UnitPanelStep(step) => self.step_unit_panel(step),
            HudEvent::Used if self.interface.hud.game_panel.tab == 4 => {
                // The diplomacy tab opens DipServer's panel (`PopupDipDialog` 0x5c3a20).
                self.interface.hud.game_panel.tab = 0;
                self.interface.hud.panel = hud::Panel::None;
                self.dip_toggle();
            }
            _ => {}
        }
        Some(GameMouse::None)
    }

    /// The part of a right press the interface answers with a command or a minimap order;
    /// true if it did.
    fn hud_right_event(&mut self, event: &HudEvent) -> bool {
        let player = self.own();
        match event {
            HudEvent::Order(px) if !self.selected.is_empty() => {
                let (x, y) = self.minimap_world(*px);
                self.commands.push(Command::Move { player, units: self.selected.clone(), x, y });
                self.unit_sound(self.selected[0], "ORDER_SOUND");
            }
            // Touch: a long press on a production icon is the Ctrl+click, an endless order (#438).
            HudEvent::Command(HudCommand::Unproduce(kind)) if self.interface.touch => {
                let mods = self.interface.mods;
                self.interface.mods |= ModifiersState::CONTROL;
                self.hud_command(HudCommand::Produce(*kind));
                self.interface.mods = mods;
            }
            // An icon's right-click command (`UnProduce` 0x4d6e08, cancelling an upgrade).
            HudEvent::Command(c) => self.hud_command(c.clone()),
            _ => return false,
        }
        true
    }

    /// The left button as the original handles it.
    fn left_click(&mut self, ctx: &Ctx, pressed: bool) -> GameMouse {
        let Some(at) = self.cursor else { return GameMouse::None };
        let p = self.canvas_point(at);
        if pressed && self.drag_from.is_none() && self.dip_press(ctx, p) {
            return GameMouse::None;
        }
        if pressed && self.drag_from.is_none() && self.interface.hud_input {
            let view = std::mem::take(&mut self.interface.view);
            let event = self.interface.hud.press(ctx, &view, p);
            self.interface.view = view;
            if let Some(result) = self.hud_left_event(event, at) {
                return result;
            }
        }
        if pressed && self.drag_from.is_none() && self.map_click(at) {
            return GameMouse::None;
        }
        if pressed {
            self.dip_check_building(at);
        }
        if !pressed && self.drag_from.is_none() {
            self.touch_place_release();
            self.mortar_release(at);
            self.dip_release();
            self.interface.hud.release();
            return GameMouse::None;
        }
        if pressed {
            self.interface.select_before = self.selected.clone();
        }
        let before = self.interface.select_before.clone();
        self.on_left(pressed);
        if !pressed {
            self.after_selection(before);
        }
        GameMouse::None
    }

    /// A click on the map in the attack-ground mode, with either button (0x4c28dc tests the
    /// left `0x7c9591` and the right `0x7c9dac`): the mode ends and the selection gets
    /// network command 0x57 at that point (0x4c48c5); the simulation keeps the player's
    /// `ARTPODGOTOVKA` units (0x4d870c).
    fn attack_ground_at(&mut self, at: [f32; 2]) {
        self.interface.attack_ground_mode = false;
        let (x, y) = self.world_at(at);
        let units = self.selected.clone();
        self.commands.push(Command::AttackGround { player: self.own(), units, x, y });
    }

    /// A left click on the map in the build mode (`Build` with the selected units; Shift
    /// keeps the mode) or while picking a rally point. True if it was used.
    fn map_click(&mut self, at: [f32; 2]) -> bool {
        let player = self.own();
        if self.mortar_press(at) {
            return true;
        }
        if self.interface.patrol_mode {
            self.interface.patrol_mode = false;
            let (x, y) = self.world_at(at);
            let units: Vec<EntityId> =
                self.selected_units().filter(|u| u.owner == player && !u.building).map(|u| u.id).collect();
            if let Some(&first) = units.first() {
                self.unit_sound(first, "ORDER_SOUND");
            }
            self.commands.push(Command::Patrol { player, units, x, y });
            return true;
        }
        if self.interface.attack_ground_mode {
            self.attack_ground_at(at);
            return true;
        }
        if self.interface.rally_mode {
            self.interface.rally_mode = false;
            let (x, y) = self.world_at(at);
            self.set_rally(x, y);
            return true;
        }
        let Some(kind) = self.interface.placing else { return false };
        if self.interface.touch {
            self.touch_place_tap(kind, at);
            return true;
        }
        self.place_building(kind, at);
        true
    }

    /// A finger down on the map while placing in touch mode (#439): the second tap of a double
    /// tap (within [`DOUBLE_TAP`] and [`DOUBLE_TAP_DP`] of the first) places the building at the
    /// outline; otherwise the finger grabs the outline: dragging moves it smoothly by as much as
    /// the finger moves, and a release without moving is a tap that puts it there
    /// ([`Self::touch_place_release`]). Nothing else places.
    fn touch_place_tap(&mut self, kind: TypeId, at: [f32; 2]) {
        let reach = DOUBLE_TAP_DP * self.interface.scale_factor;
        if is_double_tap(self.interface.touch_tap, self.interface.touch_time, at, reach) {
            self.interface.touch_tap = None;
            self.interface.touch_grab = None;
            let at = self.touch_outline_at();
            self.place_building(kind, at);
        } else {
            let outline = self.touch_outline_at();
            self.interface.touch_grab = Some(TouchGrab { press: at, outline, dragged: false });
        }
    }

    /// The finger moved while it holds the outline: past the click slop it drags the outline.
    fn touch_place_drag(&mut self, at: [f32; 2]) -> bool {
        let Some(g) = &mut self.interface.touch_grab else { return false };
        let d = [at[0] - g.press[0], at[1] - g.press[1]];
        if !g.dragged && d[0].abs() < CLICK_SLOP && d[1].abs() < CLICK_SLOP {
            return true;
        }
        g.dragged = true;
        self.interface.touch_outline = Some([g.outline[0] + d[0], g.outline[1] + d[1]]);
        true
    }

    /// The finger lifted: a tap (it never moved) counts towards a double tap and, unless it was
    /// on the outline ([`ON_OUTLINE_DP`]), puts the outline there; a drag leaves the outline where
    /// it was dragged.
    fn touch_place_release(&mut self) {
        let Some(g) = self.interface.touch_grab.take() else { return };
        if g.dragged {
            self.interface.touch_tap = None;
        } else {
            self.interface.touch_tap = Some((self.interface.touch_time, g.press));
            let reach = ON_OUTLINE_DP * self.interface.scale_factor;
            if (g.press[0] - g.outline[0]).hypot(g.press[1] - g.outline[1]) > reach {
                self.interface.touch_outline = Some(g.press);
            }
        }
    }

    /// The touch placement outline's window position: the last tap, at first the window's middle.
    pub(super) fn touch_outline_at(&self) -> [f32; 2] {
        self.interface.touch_outline.unwrap_or_else(|| {
            let [w, h] = self.window_size();
            [w / 2.0, h / 2.0]
        })
    }

    /// Orders the building of `kind` at the window position `at` if it fits (Shift keeps the
    /// build mode).
    fn place_building(&mut self, kind: TypeId, at: [f32; 2]) {
        let player = self.own();
        let (x, y) = self.world_at(at);
        if let Some((x, y)) = self.placement_site(kind, x, y) {
            let units: Vec<EntityId> =
                self.selected_units().filter(|u| u.owner == player && !u.building).map(|u| u.id).collect();
            if let Some(&first) = units.first() {
                self.unit_sound(first, "ORDER_SOUND");
            }
            self.commands.push(Command::Build { player, units, kind, x, y });
            if !self.interface.mods.shift_key() {
                self.interface.placing = None;
            }
        }
    }

    /// Shift adds to the selection (0x4da424: never removes); Ctrl does nothing. A double
    /// click (0xF1, 0x4c2ea5) selects the own units of the first selected unit's type on the
    /// screen, of its kind (formation members or free units), added with Shift.
    fn after_selection(&mut self, before: Vec<EntityId>) {
        self.add_with_shift(before.clone());
        let now = web_time::Instant::now();
        let p = self.cursor.map(|c| self.canvas_point(c)).unwrap_or_default();
        let double = self.interface.last_click.is_some_and(|(t, q)| {
            (now - t).as_secs_f64() < DOUBLE_CLICK && (p[0] - q[0]).abs() + (p[1] - q[1]).abs() < DOUBLE_CLICK_PIXELS
        });
        self.interface.last_click = if double { None } else { Some((now, p)) };
        if !double {
            return;
        }
        let Some(world) = &self.world else { return };
        let Some(first) = self.selected.first().and_then(|&id| world.unit(id)) else { return };
        let kind = first.kind;
        let formed = self.selected_units().any(|u| u.brigade.is_some());
        let free = self.selected_units().any(|u| u.brigade.is_none());
        let window = self.window_size();
        let own = self.own();
        self.selected = world
            .units
            .iter()
            .filter(|u| selectable(u) && u.inside_of.is_none() && u.owner == own && u.kind == kind)
            .filter(|u| if u.brigade.is_some() { formed } else { free })
            .filter(|u| {
                let [sx, sy] = self.screen_of(u);
                (0.0..window[0]).contains(&sx) && (0.0..window[1]).contains(&sy)
            })
            .map(|u| u.id)
            .collect();
        self.add_with_shift(before);
    }

    /// With Shift the new selection is added to `before`; then whole formations are
    /// selected ([`Self::correct_brigades`]).
    fn add_with_shift(&mut self, before: Vec<EntityId>) {
        if self.interface.mods.shift_key() {
            let mut all = before;
            for id in std::mem::take(&mut self.selected) {
                if !all.contains(&id) {
                    all.push(id);
                }
            }
            self.selected = all;
        }
        self.correct_brigades();
    }

    /// Selects whole formations and their groups ([`corrected_selection`]).
    pub(super) fn correct_brigades(&mut self) {
        let Some(world) = &self.world else { return };
        let selected = std::mem::take(&mut self.selected);
        self.selected = corrected_selection(world, self.own(), selected);
    }

    /// Mouse moved (window position).
    pub fn mouse_moved(&mut self, at: [f32; 2]) {
        self.cursor = Some(at);
        if self.touch_place_drag(at) {
            return;
        }
        let p = self.canvas_point(at);
        if self.editor.as_mut().is_some_and(|e| e.drag_to(p)) {
            return;
        }
        if self.pan_moved(at) || self.orbit_moved(at) {
            return;
        }
        let p = self.canvas_point(at);
        self.diplomacy.mouse_moved(p);
        // The rectangle selects while it is drawn (0x4d6851 every event).
        if let Some(from) = self.drag_from
            && ((from[0] - at[0]).abs() >= CLICK_SLOP || (from[1] - at[1]).abs() >= CLICK_SLOP)
        {
            self.selected = self.pick(from, at);
            self.add_with_shift(self.interface.select_before.clone());
        }
        if self.drag_from.is_none() && self.interface.hud_input {
            let view = std::mem::take(&mut self.interface.view);
            let event = self.interface.hud.drag(&view, p);
            self.interface.view = view;
            if let HudEvent::Camera(px) = event {
                let window = self.window_size();
                self.centre_on_minimap(px, window);
            }
        }
    }

    /// Right button: an order on the minimap, an attack on an enemy, else a move with the
    /// move marker. On the map the order is given on release: a drag of more than 30 map
    /// pixels moves to the press point facing the drag's direction (0x4c4cea..0x4c5891).
    pub fn right_button(&mut self, ctx: &Ctx, pressed: bool) {
        if self.editor_right(ctx, pressed) || self.orbit_button(ctx, pressed) {
            return;
        }
        if !pressed {
            self.right_release();
            return;
        }
        let Some(at) = self.cursor else { return };
        let p = self.canvas_point(at);
        let event = if self.interface.hud_input {
            let view = std::mem::take(&mut self.interface.view);
            let event = self.interface.hud.right_press(ctx, &view, p);
            self.interface.view = view;
            event
        } else {
            HudEvent::None
        };
        if self.hud_right_event(&event) {
            return;
        }
        let player = self.own();
        match event {
            HudEvent::None if self.interface.attack_ground_mode => self.attack_ground_at(at),
            HudEvent::None if self.cancel_mode() => {}
            HudEvent::None => {
                if self.selected.is_empty() {
                    return;
                }
                // Buildings only: the rally point at once.
                if self.selected_units().filter(|u| u.owner == player).all(|u| u.building) {
                    self.order_at(at);
                    return;
                }
                self.interface.right_drag = Some((at, self.world_at(at)));
            }
            _ => {}
        }
    }

    /// The drag's vector from the press point to the cursor, in map pixels.
    fn drag_vector(&self) -> Option<((i32, i32), i32, i32)> {
        let ((_, start), at) = (self.interface.right_drag?, self.cursor?);
        let (x, y) = self.world_at(at);
        Some((start, x - start.0, y - start.1))
    }

    fn right_release(&mut self) {
        let Some((press, _)) = self.interface.right_drag else { return };
        let vector = self.drag_vector();
        self.interface.right_drag = None;
        let Some(((x, y), dx, dy)) = vector else { return };
        let length = f64::from(dx * dx + dy * dy).sqrt() as i32;
        if length <= DRAG_MIN {
            self.order_at(press);
            return;
        }
        let player = self.own();
        let units: Vec<EntityId> =
            self.selected_units().filter(|u| u.owner == player && !u.building).map(|u| u.id).collect();
        let Some(&first) = units.first() else { return };
        // 32 directions: `(GetDir(dx, dy) + 4) & 0xF8`.
        let dir = acr_sim::trig::get_dir(dx, dy).wrapping_add(4) & 0xF8;
        self.add_marker(x, y);
        self.commands.push(if self.interface.attack_mode {
            Command::AttackMove { player, units, x, y }
        } else {
            Command::MoveFacing { player, units, x, y, dir }
        });
        self.unit_sound(first, "ORDER_SOUND");
    }

    /// The facing arrow while the right button is dragged (0x4c5407): seven yellow lines
    /// around a shaft 40 wide with a head 40 long and 80 wide, from the press point to the
    /// cursor, once the drag is longer than 30 map pixels. Map y is halved on the screen.
    fn facing_arrow(&self, rgba: [u8; 4], h: i32, out: &mut Vec<Draw>) {
        let Some(((sx, sy), a, b)) = self.drag_vector() else { return };
        let l = f64::from(a * a + b * b).sqrt() as i32;
        if l <= DRAG_MIN {
            return;
        }
        let (ux, uy) = (a * 20 / l, b * 20 / l);
        let (p, q) = (uy, -ux);
        let pts = [
            (-p, -q),
            (p, q),
            (a - 2 * ux + p, b - 2 * uy + q),
            (a - 2 * ux + 2 * p, b - 2 * uy + 2 * q),
            (a, b),
            (a - 2 * ux - 2 * p, b - 2 * uy - 2 * q),
            (a - 2 * ux - p, b - 2 * uy - q),
        ];
        let (ox, oy) = (sx, (sy >> 1) - h);
        for i in 0..pts.len() {
            let (p0, p1) = (pts[i], pts[(i + 1) % pts.len()]);
            line(out, rgba, (ox + p0.0, oy + p0.1 / 2), (ox + p1.0, oy + p1.1 / 2));
        }
    }

    /// Leaves the build mode or the rally point mode; false if neither was on.
    pub(super) fn cancel_mode(&mut self) -> bool {
        let on = self.interface.placing.is_some()
            || self.interface.rally_mode
            || self.interface.patrol_mode
            || self.interface.attack_ground_mode
            || self.interface.mortar.on;
        self.interface.mortar = Default::default();
        self.interface.placing = None;
        self.interface.rally_mode = false;
        self.interface.patrol_mode = false;
        self.interface.attack_ground_mode = false;
        on
    }

    pub(super) fn window_size(&self) -> [f32; 2] {
        self.interface.window
    }

    /// A key of the game screen (0x4a30e8 and `Keyboard.dat`).
    pub fn key(&mut self, ctx: &Ctx, key: &Key, pressed: bool) -> GameKey {
        if let Key::Named(named) = key {
            self.on_key(*named, pressed);
        }
        if self.editor_key(key, pressed) || self.three_d.key(key, pressed) {
            return GameKey::None;
        }
        if let Some(used) = self.modern_key(ctx, key, pressed) {
            return used;
        }
        if !pressed {
            return GameKey::None;
        }
        let mods = self.interface.mods;
        match key {
            // Escape first leaves a command mode (0x4a31a3).
            Key::Named(NamedKey::Escape) if self.cancel_mode() => {}
            Key::Named(NamedKey::F12 | NamedKey::Escape) => return GameKey::Menu,
            Key::Named(NamedKey::F8) => self.dip_toggle(),
            Key::Named(NamedKey::Pause) if self.mission.ended.is_none() => {
                self.interface.paused = !self.interface.paused
            }
            Key::Named(NamedKey::Tab) => self.interface.hud.life_bars = !self.interface.hud.life_bars,
            Key::Named(NamedKey::Space) => self.centre_next(),
            Key::Named(NamedKey::Delete) => self.destruct_selected(),
            Key::Character(c) => {
                let Some(ch) = c.chars().next().filter(|ch| ch.is_ascii()) else { return GameKey::None };
                let ch = ch.to_ascii_uppercase() as u8;
                if let Some(n) = (ch as char).to_digit(10) {
                    self.control_group(n as usize, mods);
                } else if ch == b'M' && mods.is_empty() {
                    self.interface.hud.toggle_minimap();
                } else if ch == b'U' && mods.is_empty() {
                    self.interface.hud.toggle_unit_panel();
                } else if ch == b'I' && mods.is_empty() {
                    self.interface.hud.toggle_game_panel();
                } else if ch == b'Q' && mods.is_empty() {
                    self.interface.hud.toggle_locks();
                } else if ch == b'O' && mods.is_empty() {
                    self.interface.hud.toggle_transparency();
                } else if ch == b'L' && mods.is_empty() {
                    if let Some(at) = self.cursor {
                        self.toggle_zoom(at);
                    }
                } else if ch == b'D' && mods.control_key() {
                    // Ctrl+D: slow/fast game mode (op 0x45); the simulation has one mode.
                } else {
                    self.icon_hotkey(ctx, ch, mods);
                }
            }
            _ => {}
        }
        GameKey::None
    }

    /// Ctrl+N stores the selection as group N, N recalls it, Shift+N adds it.
    fn control_group(&mut self, n: usize, mods: ModifiersState) {
        if mods.control_key() {
            self.interface.groups[n] = self.selected.clone();
            return;
        }
        let alive: Vec<EntityId> = self.interface.groups[n]
            .iter()
            .copied()
            .filter(|&id| self.world.as_ref().and_then(|w| w.unit(id)).is_some_and(selectable))
            .collect();
        if mods.shift_key() {
            for id in alive {
                if !self.selected.contains(&id) {
                    self.selected.push(id);
                }
            }
            self.correct_brigades();
        } else {
            self.selected = alive;
            self.correct_brigades();
            self.group_recalled(n);
        }
    }

    /// Key L or `4XSCALE_ON`/`OFF` (0x5fb4dd): the view at a quarter of the size, or back,
    /// centred on the point under the cursor.
    fn toggle_zoom(&mut self, at: [f32; 2]) {
        let (x, y) = self.world_at(at);
        let zoom = !self.interface.hud.zoom;
        self.interface.hud.zoom = zoom;
        // Back from it, the modern controls return to the wheel's zoom level.
        self.camera.scale = if zoom { ZOOM_SCALE } else { self.interface.modern.zoom };
        let window = self.window_size();
        let s = self.camera.scale;
        self.camera.x = x as f32 - window[0] / s / 2.0;
        self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
        self.clamp_camera(window);
    }

    /// Space: the camera jumps to the next selected unit (0x4c6020).
    pub(super) fn centre_next(&mut self) {
        let units: Vec<(i32, i32)> = self.selected_units().map(|u| (u.pos.x >> 4, u.pos.y >> 4)).collect();
        if units.is_empty() {
            return;
        }
        let i = self.interface.next_centred % units.len();
        self.interface.next_centred = i + 1;
        let (x, y) = units[i];
        let window = self.window_size();
        let s = self.camera.scale;
        self.camera.x = x as f32 - window[0] / s / 2.0;
        self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
        self.clamp_camera(window);
    }

    /// Letter keys of `Keyboard.dat`: press the shown icon of that name.
    pub(super) fn icon_hotkey(&mut self, ctx: &Ctx, key: u8, mods: ModifiersState) {
        let flags = u8::from(mods.control_key()) | (u8::from(mods.alt_key()) << 1) | (u8::from(mods.shift_key()) << 2);
        let Some(data) = self.interface.hud.data(ctx) else { return };
        let names: Vec<String> = self
            .keyboard(ctx)
            .map(|k| k.iter().filter(|e| e.0 == flags && e.1 == key).map(|e| e.2.clone()).collect())
            .unwrap_or_default();
        // The keys of the production icons (`[FIXED_PRODUCE]`, without modifiers).
        let produce = self.interface.view.icons.iter().find(|i| flags == 0 && i.key == Some(key) && !i.disabled);
        if let Some(action) = produce.and_then(|i| i.action.clone()) {
            self.hud_command(action);
            return;
        }
        for name in names {
            let Some(frame) = data.icon(&name) else { continue };
            let hit = self
                .interface
                .view
                .icons
                .iter()
                .find(|i| i.frame == frame && !i.disabled)
                .and_then(|i| i.action.clone());
            if let Some(action) = hit {
                self.hud_command(action);
                return;
            }
        }
    }
}

/// Which `STOPBUILD` frame of `NRES.DAT [CONST]` (`off on`: index 0, 1) the button shows. The
/// parser (0x4ec33e, `%s%s%d%d`) stores the first word in 0x66ae60 and the second in 0x66ae5c;
/// the icon code (0x607af1) draws 0x66ae5c while OB+0xf2 bit0 (stopped) is set and 0x66ae60
/// while running. So a running building shows the first word (`Production_OFF`) and a stopped
/// one the second (`Production_ON`), the reverse of the names' order.
fn stopbuild_frame(stopped: bool) -> usize {
    usize::from(stopped)
}

/// What an order counts as in `GetProduceCount` (0x49e060): 1200 if endless, else 1.
fn order_count(q: &acr_sim::economy::Production) -> i32 {
    if q.endless { acr_sim::economy::ENDLESS_COUNT } else { 1 }
}

/// `PAUSETEXT` while the game is paused (0x4c0130): `BigWhiteFont` centred at half the
/// screen height on a paper box (0x480130) 64 pixels wider on each side.
fn pause_text(ctx: &Ctx, screen: [i32; 2], out: &mut Vec<Draw>) {
    let text = ctx.texts.get("PAUSETEXT");
    let w = ctx.fonts.width(ctx.lib, BIG_WHITE, text);
    let (x, y) = ((screen[0] - w) / 2, screen[1] / 2);
    let rect = Rect::inclusive(x - 64, y - 8, x + w + 64, y + 27);
    crate::ui::screens::mission::paper_box(ctx, out, rect, PAPER, hud::BOX_CORNERS, 0);
    out.push(Draw::game_text(BIG_WHITE, text, x - 2, y - 8));
}

/// `BigWhiteFont` and the paper of 0x480130 (`Border` 28..30).
const BIG_WHITE: crate::ui::fonts::FontId = crate::ui::fonts::FontId("BigWhiteFont");
const PAPER: usize = 28;

/// The name of a unit type: the text `NAME.MD` (`GeneralObject::Message`), else its name.
pub(super) fn unit_name(ctx: &Ctx, md_name: &str) -> Vec<u8> {
    let key = format!("{}.MD", md_name.to_ascii_uppercase());
    let text = ctx.texts.get(&key);
    if text == key.as_bytes() { md_name.as_bytes().to_vec() } else { text.to_vec() }
}

/// `sprintf(fmt, s)` for a text with one `%s`.
fn format_str(fmt: &[u8], s: &[u8]) -> Vec<u8> {
    match fmt.windows(2).position(|w| w == b"%s") {
        Some(i) => [&fmt[..i], s, &fmt[i + 2..]].concat(),
        None => fmt.to_vec(),
    }
}

/// A production hint (0x606180, 0x6062a0 for upgrades): the name, " -", then
/// " Resource: price" for every resource of the base price, with the current price.
fn cost_hint(name: Vec<u8>, base: &[i32; 8], cost: &[i32; 8], ctx: &Ctx) -> Vec<u8> {
    let mut out = name;
    out.extend_from_slice(b" -");
    for (r, name) in RESOURCE_NAMES.iter().enumerate() {
        if base[r] != 0 {
            out.push(b' ');
            out.extend_from_slice(ctx.texts.get(name));
            out.extend_from_slice(format!(": {}", cost[r]).as_bytes());
        }
    }
    out
}

/// An upgrade's hint: its message, and on a second line (0x49483d) the price as
/// `Resource:amount` joined by spaces (0x6062a0).
fn upgrade_hint(ctx: &Ctx, message: &str, cost: &[i32; 8]) -> Vec<u8> {
    let mut out = ctx.texts.get(message).to_vec();
    let mut first = true;
    for (r, name) in RESOURCE_NAMES.iter().enumerate().filter(|(r, _)| cost[*r] != 0) {
        out.push(if first { b'/' } else { b' ' });
        first = false;
        out.extend_from_slice(ctx.texts.get(name));
        out.extend_from_slice(format!(":{}", cost[r]).as_bytes());
    }
    out
}

/// The numbers of the first selected object (0x607f50): per attack index with damage
/// (at most two, not of an `I` weapon kind) the damage with its weapon kind's icon, the
/// shield and the protections, from the owner's characteristics (upgrades included).
fn stats(
    world: &acr_sim::World,
    data: &hud::data::HudData,
    u: &Unit,
    def: Option<&acr_formats::defs::UnitDef>,
) -> Stats {
    let t = world.catalog.get(u.kind);
    let adv = world.adv(u.owner, u.kind);
    // `EX_SHOTS` buildings shoot by their garrison: damage scales from the type's base with
    // the share of shooters inside (USAGE 0x15, 0x18, 0x20) in the capacity (0x60804b).
    let mut ratio = 1000;
    if def.is_some_and(|d| d.has("EX_SHOTS")) && adv.max_inside > 0 {
        let shooters = u
            .inside
            .iter()
            .filter_map(|&id| world.units.get(id.0 as usize))
            .filter(|i| matches!(world.catalog.get(i.kind).role.usage, 0x15 | 0x18 | 0x20))
            .count() as i32;
        ratio = shooters * 1000 / adv.max_inside;
    }
    let mut attacks = Vec::new();
    for (i, a) in adv.attacks.iter().enumerate() {
        let kind = data.weapon(&a.kind);
        if a.damage == 0 || kind.is_some_and(|k| data.weapon_hidden[k]) || attacks.len() >= 2 {
            continue;
        }
        let base = t.attacks.get(i).map_or(0, |b| b.damage);
        let damage = base + (a.damage - base) * ratio / 1000;
        // Kills raise the damage of the `SKILLDAMAGEMASK` indices (0x608165).
        let skill = if (t.skill_mask >> i) & 1 != 0 { t.skill_bonus * i32::from(u.kills) / 10 } else { 0 };
        let total = u.attack_bonus + skill;
        // The original prints `skill − bonus` after the minus sign (0x6081c2).
        let text = match total {
            0 => damage.to_string(),
            t if t > 0 => format!("{damage}+{t}"),
            _ => format!("{damage}-{}", skill - u.attack_bonus),
        };
        let icon = kind.and_then(|k| data.weapons.get(k)).map_or(0, |w| w.1);
        attacks.push((icon, text.into_bytes()));
    }
    let shield = match u.defence_bonus {
        0 => adv.shield.to_string(),
        b if b > 0 => format!("{}+{b}", adv.shield),
        b => format!("{}-{}", adv.shield, -b),
    };
    let mut protections: Vec<(usize, usize, i32)> = adv
        .protection
        .iter()
        .filter(|&&(_, v)| v != 0)
        .filter_map(|(k, v)| {
            let i = data.weapon(k)?;
            Some((i, data.weapons[i].2, *v & 0xFF))
        })
        .collect();
    protections.sort_by_key(|p| p.0);
    protections.dedup_by_key(|p| p.0);
    Stats {
        attacks,
        shield: shield.into_bytes(),
        protections: protections.into_iter().map(|(_, icon, v)| (icon, v)).take(6).collect(),
        ..Stats::default()
    }
}

/// The player's idle peasants (0x4e230f: usage 23, no order, not inside a building) or
/// mines with room for more workers (0x4e279e: usage 7, finished, fewer inside than they
/// take), in object order.
fn idle(world: &acr_sim::World, player: u8, mines: bool) -> Vec<EntityId> {
    use acr_sim::catalog::usage;
    world
        .units
        .iter()
        .filter(|u| u.alive && u.owner == player && u.death == 0)
        .filter(|u| {
            let t = world.catalog.get(u.kind);
            if mines {
                t.role.usage == usage::MINE
                    && u.stage >= world.max_stage(u)
                    && (u.inside.len() as i32) < world.adv(u.owner, u.kind).max_inside + u.add_inside
            } else {
                t.role.usage == usage::PEASANT && u.orders.is_empty() && u.inside_of.is_none()
            }
        })
        .map(|u| u.id)
        .collect()
}

/// The distinct formations of the selected objects that are alive and not hidden, in
/// selection order (`FormsIDS` of 0x43dbff / 0x43e0b7).
fn selected_brigades(world: &acr_sim::World, own: u8, selected: &[EntityId]) -> Vec<u16> {
    let mut ids: Vec<u16> = Vec::new();
    for u in selected.iter().filter_map(|&id| world.unit(id)) {
        if let Some(b) = u.brigade.filter(|_| u.active() && u.owner == own)
            && !ids.contains(&b)
            && ids.len() < 256
        {
            ids.push(b);
        }
    }
    ids
}

/// Whether a unit can be selected or counted as a soldier: living (not dying, no corpse) and,
/// under the hit-locations rule (#262), not out of the fight (a crawler or a man without a
/// working arm, who takes no orders, [`acr_sim::body::Body::out_of_fight`]). A man only
/// knocked down gets up again and stays selectable. Without the rule every body is whole,
/// so this is `living()`; the original takes a dying man out of selection and formation at
/// once (`Die` 0x4e77a0 calls `UnRegisterNewUnit` 0x42e2bd).
pub(super) fn selectable(u: &Unit) -> bool {
    u.living() && !u.body.out_of_fight()
}

/// Whether the selection panel lists `u` (0x606710 skips a housed object, OB+0x58 != 0).
fn listed_in_panel(u: &Unit) -> bool {
    u.inside_of.is_none()
}

/// A member of brigade `b` that still fights ([`selectable`]): the soldiers the formation
/// icons count. ACR keeps a dead man's id in his brigade until the corpse is gone, where
/// the original's `Die` empties his slot.
pub(super) fn fighting_member(world: &acr_sim::World, b: u16, m: EntityId) -> Option<&Unit> {
    world.unit(m).filter(|u| selectable(u) && u.brigade == Some(b))
}

/// The soldiers a formation's icon counts (0x606d10): the members from the fourth on that
/// still fight ([`fighting_member`]).
pub(super) fn formation_soldiers(world: &acr_sim::World, b: u16) -> Vec<&Unit> {
    let Some(br) = world.brigades.get(usize::from(b)) else { return Vec::new() };
    br.members.iter().skip(3).flatten().filter_map(|&m| fighting_member(world, b, m)).collect()
}

/// The Delete key's command: the player's own selected objects that can still be selected.
pub(super) fn destruct_command(world: &acr_sim::World, own: u8, selected: &[EntityId]) -> Option<Command> {
    let units: Vec<EntityId> = selected
        .iter()
        .copied()
        .filter(|&id| world.unit(id).is_some_and(|u| selectable(u) && u.owner == own))
        .collect();
    (!units.is_empty()).then_some(Command::Destruct { player: own, units })
}

/// `ImCorrectBrigadesSelection` (0x41f3fb, after a selection 0x4da424 and a recalled group
/// 0x4d6387): a selected member of an own formation selects the formation's soldiers
/// (`ImSelBrigade` 0x4e1365: the members from the third on that are not selected yet,
/// appended); then `CorrectImSelectionInGroups` (0x43dbff) does the same for the formations
/// grouped with the selected ones (`CM_DOGRP`).
pub(super) fn corrected_selection(world: &acr_sim::World, own: u8, mut selected: Vec<EntityId>) -> Vec<EntityId> {
    let select = |selected: &mut Vec<EntityId>, b: u16| {
        let Some(br) = world.brigades.get(usize::from(b)).filter(|br| br.alive && br.owner == own) else { return };
        for &m in br.members.iter().skip(2).flatten() {
            if fighting_member(world, b, m).is_some() && !selected.contains(&m) {
                selected.push(m);
            }
        }
    };
    for b in selected_brigades(world, own, &selected) {
        select(&mut selected, b);
    }
    let groups = &world.formation_groups[usize::from(own)];
    for b in groups.expand(&selected_brigades(world, own, &selected)) {
        select(&mut selected, b);
    }
    selected
}

/// The grouping icon for the selection: `Some(false)` `CM_UNGRP` when its formations are
/// exactly a group (`CheckUnGroupPossibility` 0x43e0b7), else `Some(true)` `CM_DOGRP` when
/// every selected object that is alive and not hidden is in a formation
/// (`CheckGroupPossibility` 0x43dfdb).
fn grouping(world: &acr_sim::World, own: u8, selected: &[EntityId]) -> Option<bool> {
    let ids = selected_brigades(world, own, selected);
    if world.formation_groups[usize::from(own)].is_group(&ids) {
        return Some(false);
    }
    let all_formed =
        selected.iter().filter_map(|&id| world.unit(id)).filter(|u| u.active()).all(|u| u.brigade.is_some());
    all_formed.then_some(true)
}

/// The minimap's unit dots: living objects outside buildings only. Dying men and corpses
/// (`alive` while `death > 0`) are no longer shown (#351).
fn minimap_dots(world: &acr_sim::World, player: u8, selected: &[EntityId], shift: u32) -> Vec<Dot> {
    world
        .units
        .iter()
        .filter(|u| u.living() && u.inside_of.is_none())
        .map(|u| {
            let own_selected = u.owner == player && selected.contains(&u.id);
            let color = if own_selected { minimap::WHITE } else { minimap::PLAYER_COLORS[usize::from(u.owner % 8)] };
            Dot { x: u.pos.x >> shift, y: u.pos.y >> shift, color }
        })
        .collect()
}

/// A unit of another player stands in the fog of war, so its marks stay off.
pub(super) fn hidden_in_fog(fog: Option<&acr_sim::fog::Fog>, world: &acr_sim::World, u: &Unit, own: u8) -> bool {
    u.owner != own && fog.is_some_and(|f| !f.visible(world, u.pos.x >> 4, u.pos.y >> 4))
}

/// The label over a building's bar (0x4bb7ae..0x4bb82a): `inside/capacity` for the player's
/// own side (alliance mask shared, 0x4bb790), `?/capacity` for the others.
fn inside_label(seen: bool, inside: usize, capacity: i32) -> Vec<u8> {
    if seen { format!("{inside}/{capacity}") } else { format!("?/{capacity}") }.into_bytes()
}

/// The typed leave commands of one click on an occupant icon: `n` per building.
fn leave_commands(player: u8, buildings: &[EntityId], kind: TypeId, n: usize) -> Vec<Command> {
    buildings
        .iter()
        .flat_map(|&building| std::iter::repeat_n(Command::Leave { player, building, kind: Some(kind) }, n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occupant_slots_follow_the_original() {
        let slots: Vec<usize> = (0..14).map(occupant_slot).collect();
        assert_eq!(slots, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 1, 14, 15]);
    }

    #[test]
    fn occupants_are_counted_per_type_minus_pending_exits() {
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let mut add = |name: &str| {
            c.add(ObjectType::from_def(
                name,
                &acr_formats::defs::UnitDef::parse(b"NAME X\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n"),
            ))
        };
        let (a, b) = (add("Mus(EN)"), add("Pez(EN)"));
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        let house = w.spawn(a, 0, Pos::from_pixels(950, 900), 0);
        for kind in [b, a, b, b] {
            let u = w.spawn(kind, 0, Pos::from_pixels(900, 900), 0);
            w.units[u.0 as usize].inside_of = Some(house);
            w.units[house.0 as usize].inside.push(u);
        }
        let counts = |w: &acr_sim::World| occupant_counts(w, &[w.unit(house).unwrap()]);
        assert_eq!(counts(&w), [(a, 1), (b, 3)]);
        w.units[house.0 as usize].orders.push(acr_sim::world::Order::LeaveMine { kind: a, delay: 0 });
        w.units[house.0 as usize].orders.push(acr_sim::world::Order::LeaveMine { kind: b, delay: 0 });
        assert_eq!(counts(&w), [(b, 2)]);
    }

    #[test]
    fn the_panel_skips_housed_units() {
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let t = c.add(ObjectType::from_def(
            "Mus(EN)",
            &acr_formats::defs::UnitDef::parse(b"NAME X\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n"),
        ));
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        let out = w.spawn(t, 0, Pos::from_pixels(900, 900), 0);
        let housed = w.spawn(t, 0, Pos::from_pixels(900, 900), 0);
        let house = w.spawn(t, 0, Pos::from_pixels(950, 900), 0);
        w.units[housed.0 as usize].inside_of = Some(house);
        assert!(listed_in_panel(w.unit(out).unwrap()));
        assert!(!listed_in_panel(w.unit(housed).unwrap()));
    }

    #[test]
    fn strangers_in_the_fog_get_no_marks() {
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::fog::Fog;
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let t = c.add(ObjectType::from_def(
            "Mus(EN)",
            &acr_formats::defs::UnitDef::parse(b"NAME X\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n"),
        ));
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        let mine = w.spawn(t, 0, Pos::from_pixels(900, 900), 0);
        let foe = w.spawn(t, 1, Pos::from_pixels(900, 900), 0);
        let (mine, foe) = (w.unit(mine).unwrap().clone(), w.unit(foe).unwrap().clone());
        let mut fog = Fog::new(w.map.shift);
        assert!(hidden_in_fog(Some(&fog), &w, &foe, 0), "dark cell");
        assert!(!hidden_in_fog(Some(&fog), &w, &mine, 0), "own units are always marked");
        fog.open();
        assert!(!hidden_in_fog(Some(&fog), &w, &foe, 0), "seen");
        assert!(!hidden_in_fog(None, &w, &foe, 0), "no fog");
    }

    #[test]
    fn a_right_click_on_a_field_gathers_it() {
        use acr_formats::defs::{NationDef, SpriteResources, UnitDef};
        use acr_formats::m3d::SpriteGroup;
        use acr_formats::sprite_list::SpriteList;
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use acr_sim::sprite::SpriteKinds;
        use std::sync::Arc;
        let peasant = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\n\
            TAKERESSTAGES FOOD 3 3 4\r\n[END]\r\n";
        let swordsman =
            "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\n[END]\r\n";
        let mut c = Catalog::new();
        let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(peasant.as_bytes())));
        let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(swordsman.as_bytes())));
        c.tech.add_nation(&c.clone(), &NationDef::parse(b"[ENABLED]\r\nKri(EN)\r\n[END]\r\n"));
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        let lst = SpriteList::parse("COMPLEX 2\r\nKUK00 159 146 30\r\nKUK01 159 146 30\r\n").unwrap();
        let rsr = SpriteResources::parse(b"[SOURCES]\r\nKUK01 FOOD 3 64\r\n[END]\r\n");
        w.sprite_kinds = Arc::new((SpriteKinds::new(&lst, &rsr), SpriteKinds::default()));
        let field = w.add_sprite(SpriteGroup::Complex, 1, 2600, 2048);
        w.sprites[field].index = 1;
        let peasant = w.spawn(kri, 0, Pos::from_pixels(2000, 2000), 0);
        let soldier = w.spawn(mus, 0, Pos::from_pixels(2000, 2100), 0);
        let (p, s) = (w.unit(peasant).unwrap(), w.unit(soldier).unwrap());
        // A ripe field within 64 px of the click, the sickle cursor.
        let ctx = gather_context(&w, &[p], 2620, 2060);
        assert_eq!(ctx, Some(Context::Gather(2620, 2060, 3)));
        assert_eq!(ctx.unwrap().cursor(), usize::from(cursor::frame::GATHER[3]));
        // Too far from it, or nobody who can gather it.
        assert_eq!(gather_context(&w, &[p], 2800, 2048), None);
        assert_eq!(gather_context(&w, &[s], 2620, 2060), None);
    }

    #[test]
    fn minimap_shows_living_units_only() {
        use acr_formats::defs::UnitDef;
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let def = "NAME X\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n@DEATH 9 0 0 39\r\n#DEATHLIE1 9 1 0 39\r\n\
                   LIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";
        let k = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(def.as_bytes())));
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        let ids: Vec<EntityId> = (0..3).map(|i| w.spawn(k, 0, Pos::from_pixels(1000 + 50 * i, 1000), 0)).collect();
        assert_eq!(minimap_dots(&w, 0, &[], 9).len(), 3);
        w.kill(ids[0]);
        w.step(&[]);
        assert!(w.unit(ids[0]).is_some_and(|u| u.alive && u.death > 0), "the corpse still exists");
        assert_eq!(minimap_dots(&w, 0, &[], 9).len(), 2, "a corpse is no dot");
        w.remove(ids[1]);
        assert_eq!(minimap_dots(&w, 0, &[], 9).len(), 1);
    }

    #[test]
    fn stopbuild_button_shows_the_original_frame_for_each_state() {
        assert_eq!(stopbuild_frame(false), 0, "running: the first word of the line");
        assert_eq!(stopbuild_frame(true), 1, "stopped: the second word of the line");
    }

    #[test]
    fn garrison_label_hides_the_count_of_foreign_buildings() {
        assert_eq!(inside_label(true, 10, 15), b"10/15");
        assert_eq!(inside_label(true, 0, 0), b"0/0");
        assert_eq!(inside_label(false, 10, 15), b"?/15");
    }

    #[test]
    fn shift_click_on_an_occupant_sends_five_leaves_per_building() {
        let (b, kind) = ([EntityId(3), EntityId(4)], TypeId(7));
        assert_eq!(leave_commands(0, &b, kind, 1).len(), 2);
        let five = leave_commands(0, &b, kind, SHIFT_REPEAT);
        assert_eq!(five.len(), 10);
        assert!(five[..5].iter().all(|c| *c == Command::Leave { player: 0, building: EntityId(3), kind: Some(kind) }));
    }

    #[test]
    fn grouped_formations_are_selected_together() {
        use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let def = |officer: &str| {
            format!(
                "NAME X\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n@MOTION_R 9 0 10 19\r\n\
                 LIFE 100\r\nGEOMETRY 1 10 40\r\n{officer}[END]\r\n"
            )
        };
        let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(def("").as_bytes())));
        let off = c.add(ObjectType::from_def("Off(EN)", &UnitDef::parse(def("OFFICER\r\n").as_bytes())));
        let f = Formations::new(
            &OrderList::parse(
                b"#LINE15 SYM2 19 2 9 3 1 2\r\n   C   C   C\r\n      \r\n***************\r\n#END\r\n#EXIT\r\n",
            ),
            &OrderGroups::parse(b"1\r\nLINE 0 1 #LINE15\r\n"),
        );
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
        let mut formations = Vec::new();
        for k in 0..3 {
            let x = 1500 + 600 * k;
            let officer = w.spawn(off, 0, Pos::from_pixels(x, 2000), 0);
            let soldiers: Vec<EntityId> = (0..15)
                .map(|i| w.spawn(mus, 0, Pos::from_pixels(x - 100 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0))
                .collect();
            w.step(&[Command::Form { player: 0, officer, units: soldiers.clone(), kind: "LINE".into() }]);
            formations.push((officer, soldiers));
        }
        let free = w.spawn(mus, 0, Pos::from_pixels(900, 900), 0);
        // One soldier selects its formation's soldiers (the officer only if he was picked).
        let (o0, s0) = &formations[0];
        let sel = corrected_selection(&w, 0, vec![s0[3]]);
        assert_eq!(sel.len(), 15);
        assert!(!sel.contains(o0) && s0.iter().all(|s| sel.contains(s)));
        assert_eq!(corrected_selection(&w, 0, vec![*o0]).len(), 16);
        assert_eq!(corrected_selection(&w, 1, vec![s0[3]]), [s0[3]], "not an own formation");
        // Grouping: offered for formations only; ungrouping once they are a group.
        assert_eq!(grouping(&w, 0, &[*o0, formations[1].0]), Some(true));
        assert_eq!(grouping(&w, 0, &[*o0, free]), None);
        w.step(&[Command::GroupFormations { player: 0, units: vec![*o0, formations[1].0] }]);
        assert_eq!(grouping(&w, 0, &[formations[1].0, *o0]), Some(false));
        assert_eq!(grouping(&w, 0, &[*o0]), Some(true), "part of a group");
        // Selecting one soldier of a grouped formation selects both formations' soldiers.
        let sel = corrected_selection(&w, 0, vec![s0[0]]);
        assert_eq!(sel.len(), 30);
        assert!(formations[1].1.iter().all(|s| sel.contains(s)));
        assert!(!formations[2].1.iter().any(|s| sel.contains(s)));
    }

    #[test]
    fn dead_dying_and_crawling_men_are_neither_selected_nor_counted() {
        use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
        use acr_sim::Pos;
        use acr_sim::body::{KNEE, LimbId};
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let def = |officer: &str| {
            format!(
                "NAME X
USERLC 0 G SHADOW 0 0
#STAND 9 1 0 0
@MOTION_L 9 0 0 9
@MOTION_R 9 0 10 19
                 LIFE 100
GEOMETRY 1 10 40
{officer}[END]
"
            )
        };
        let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(def("").as_bytes())));
        let off = c.add(ObjectType::from_def(
            "Off(EN)",
            &UnitDef::parse(
                def("OFFICER
")
                .as_bytes(),
            ),
        ));
        let f = Formations::new(
            &OrderList::parse(
                b"#LINE15 SYM2 19 2 9 3 1 2
   C   C   C
      
***************
#END
#EXIT
",
            ),
            &OrderGroups::parse(
                b"1
LINE 0 1 #LINE15
",
            ),
        );
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
        let officer = w.spawn(off, 0, Pos::from_pixels(1500, 2000), 0);
        let soldiers: Vec<EntityId> =
            (0..15).map(|i| w.spawn(mus, 0, Pos::from_pixels(1400 + 20 * (i % 5), 2100 + 20 * (i / 5)), 0)).collect();
        w.step(&[Command::Form { player: 0, officer, units: soldiers.clone(), kind: "LINE".into() }]);
        let b = w.unit(soldiers[0]).unwrap().brigade.expect("formed");
        let count = |w: &acr_sim::World| formation_soldiers(w, b).len();
        let start = count(&w);
        assert!(start >= 13, "{start} soldiers counted");
        // A dying man (his body still lies there, `alive`) leaves the count at once.
        let dying = soldiers[5];
        w.kill(dying);
        assert!(w.unit(dying).is_some(), "the body is still there");
        assert_eq!(count(&w), start - 1);
        assert!(!corrected_selection(&w, 0, vec![soldiers[6]]).contains(&dying));
        // A crawler (hit-locations rule) is out of the fight: not counted, not selectable.
        let crawler = soldiers[7];
        w.units[crawler.0 as usize].body.limbs[LimbId::LegL as usize].lost = KNEE;
        assert!(w.units[crawler.0 as usize].body.out_of_fight());
        assert!(!selectable(&w.units[crawler.0 as usize]));
        assert_eq!(count(&w), start - 2);
        assert!(!corrected_selection(&w, 0, vec![soldiers[6]]).contains(&crawler));
        // A man only knocked down gets up again and stays selectable.
        w.units[soldiers[8].0 as usize].body.down = 10;
        assert!(selectable(&w.units[soldiers[8].0 as usize]));
        // Delete: only the own living, fighting men of the selection.
        let selected = vec![dying, crawler, soldiers[9], officer];
        assert_eq!(
            destruct_command(&w, 0, &selected),
            Some(Command::Destruct { player: 0, units: vec![soldiers[9], officer] })
        );
        assert_eq!(destruct_command(&w, 1, &selected), None, "not the player's own");
        w.step(&[destruct_command(&w, 0, &selected).unwrap()]);
        assert!(!w.units[soldiers[9].0 as usize].living() && !w.units[officer.0 as usize].living());
        assert_eq!(count(&w), start - 3);
    }

    #[test]
    fn idle_selects_peasants_without_orders_and_mines_with_room() {
        use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let peasant = c.add(ObjectType::from_def(
            "Kr(EN)",
            &UnitDef::parse(
                b"NAME Kr
USAGE PEASANT
LIFE 10
[END]
",
            ),
        ));
        let mine = c.add(ObjectType::from_def(
            "Mine(EN)",
            &UnitDef::parse(
                b"NAME Mine
BUILDING
USAGE MINE
LIFE 100
PEASANTABSORBER 2
[END]
",
            ),
        ));
        let f = Formations::new(
            &OrderList::parse(
                b"#END
#EXIT
",
            ),
            &OrderGroups::parse(
                b"0
",
            ),
        );
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
        let idle_one = w.spawn(peasant, 0, Pos::from_pixels(500, 500), 0);
        let busy = w.spawn(peasant, 0, Pos::from_pixels(560, 500), 0);
        let enemy = w.spawn(peasant, 1, Pos::from_pixels(620, 500), 0);
        let m = w.spawn(mine, 0, Pos::from_pixels(900, 900), 0);
        w.step(&[Command::Move { player: 0, units: vec![busy], x: 2000, y: 2000 }]);
        assert_eq!(idle(&w, 0, false), [idle_one]);
        assert_eq!(idle(&w, 1, false), [enemy]);
        assert_eq!(idle(&w, 0, true), [m]);
        // A full mine is not idle.
        let full = w.unit(m).unwrap().id;
        let cap = w.adv(0, mine).max_inside;
        w.units[full.0 as usize].inside = (0..cap.max(1) as u32).map(|i| EntityId(100 + i)).collect();
        assert!(idle(&w, 0, true).is_empty());
    }

    /// The pixels a line covers.
    fn pixels(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        line(&mut out, [0; 4], from, to);
        let mut px: Vec<(i32, i32)> = out
            .iter()
            .flat_map(|d| {
                let r = Rect::new(d.x, d.y, d.size[0], d.size[1]);
                (r.y0..r.y1).flat_map(move |y| (r.x0..r.x1).map(move |x| (x, y)))
            })
            .collect();
        px.sort_unstable();
        px
    }

    #[test]
    fn production_messages_and_market_slots() {
        assert_eq!(
            format_str(b"Nicht gen\xfcgend Ressourcen %s", b"Holz,Gold"),
            b"Nicht gen\xfcgend Ressourcen Holz,Gold"
        );
        assert_eq!(format_str(b"%s: fertig", b"Fort"), b"Fort: fertig");
        // Sell icons of market slots 0..5 and the buttons do not collide.
        let mut slots: Vec<usize> =
            (0..6).flat_map(|i| [(i >> 1) * 12 + (i & 1), (i >> 1) * 12 + (i & 1) + 7]).collect();
        slots.extend(MARKET_BUTTONS.iter().map(|b| b.0));
        let n = slots.len();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), n);
    }

    #[test]
    fn lines_cover_every_step_once() {
        // A lock cell's diagonal: 17 pixels from (0, 0) to (16, 8), two per row.
        let px = pixels((0, 0), (16, 8));
        assert_eq!(px.len(), 17);
        assert_eq!((px[0], px[16]), ((0, 0), (16, 8)));
        assert!(px.windows(2).all(|w| w[0] != w[1]));
        // Rising and vertical lines.
        assert_eq!(pixels((0, 8), (16, 0)).len(), 17);
        assert_eq!(pixels((3, 0), (3, 5)), (0..=5).map(|y| (3, y)).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod touch_tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_double_tap_is_two_taps_close_in_time_and_place() {
        let first = Some((ms(1000), [300.0, 200.0]));
        assert!(!is_double_tap(None, ms(1100), [300.0, 200.0], 40.0), "a single tap never places");
        assert!(is_double_tap(first, ms(1350), [310.0, 215.0], 40.0));
        assert!(!is_double_tap(first, ms(1351), [300.0, 200.0], 40.0), "too slow");
        assert!(!is_double_tap(first, ms(1100), [300.0, 241.0], 40.0), "too far");
        assert!(is_double_tap(first, ms(1100), [300.0, 241.0], 80.0), "the reach follows the density");
    }

    #[test]
    fn the_outline_blinks_twice_a_second() {
        assert!((outline_alpha(0.125) - 1.0).abs() < 1e-5, "peak");
        assert!((outline_alpha(0.375) - 0.25).abs() < 1e-5, "trough");
        assert!((outline_alpha(0.5 + 0.125) - outline_alpha(0.125)).abs() < 1e-5, "period 0.5 s");
    }
}
