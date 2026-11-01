//! The in-game interface of `dmcr.exe`, rebuilt: resource panel, unit and command icons,
//! minimap, the "M" drop-down, hints, and the overlays drawn over the map (selection
//! brackets, life bars, the move marker, the selection rectangle).
//!
//! The original lays the interface out for the screen size (`RealLx` × `RealLy`). ACR lays
//! it out for the window divided by the menus' scale ([`crate::canvas::Canvas::screen`]),
//! so one side is 1024 or 768 and the other at least as long. The map fills the whole
//! window: `0x676b84` is 1, so the view has no bottom panel (`PanelLy` is unused).
//! See `docs/ui.md` (in-game interface) for the addresses.

pub mod cursor;
pub mod data;
pub mod minimap;
pub mod panels;
pub mod produce;
pub mod touch;
pub mod zoom;

use std::rc::Rc;

use acr_formats::Palette;

use crate::ui::Ctx;
use crate::ui::dialog::Reject;
use crate::ui::draw::{Draw, GAME_PALETTE, Rect};
use crate::ui::fonts::FontId;
use crate::ui::settings::TopBar;
use data::HudData;
use minimap::{Minimap, Placement};
use zoom::{Group, Zoom};

/// Sprite packs of the interface.
pub mod gp {
    pub const RESPANEL: &str = "INTERF2\\RESPANEL.GP";
    pub const ICONS: &str = "ICONS.GP";
    pub const SMICONS: &str = "SMICONS.GP";
    /// `GP "Border"` (lazy id 0x668844): box frames, drop-down rows, tooltip corners.
    pub const BORDER: &str = "BORDER.GP";
    /// `Interface\CursGo`: the marker at a move order's target.
    pub const CURSGO: &str = "INTERFACE\\CURSGO.GP";
}

/// Fonts of the interface (global `RLCFont` objects).
pub mod font {
    use super::FontId;

    pub const YELLOW: FontId = FontId("YellowFont");
    pub const WHITE: FontId = FontId("WhiteFont");
    pub const BLACK: FontId = FontId("BlackFont");
    pub const SMALL_WHITE: FontId = FontId("SmallWhiteFont");
    pub const SMALL_BLACK1: FontId = FontId("SmallBlackFont1");
}

/// A part of the interface an external UI draws instead of the engine (docs/webui.md "The
/// HUD"): the game leaves the claimed sections of `Hud::draw` out of its layers
/// and passes them on ([`claimed`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Claim {
    /// The resource bar, or the touch top bar, with the "M" plaque.
    pub top_bar: bool,
    /// The "M" drop-down.
    pub drop_down: bool,
    /// The tooltip at the mouse.
    pub hints: bool,
    /// The mission's hint lines, the central text and the end of the game.
    pub messages: bool,
    /// The minimap: its map image (terrain, unit dots, fog) with the frame, and the camera
    /// rectangle and the formation mark as a group of their own.
    pub minimap: bool,
    /// The command panel: the type boxes, the command and production icons, the market's plaques.
    pub commands: bool,
    /// The numbers of the selected type (life, attacks, protections, reload) and the unit panel
    /// with the portrait.
    pub stats: bool,
    /// The army list's icons.
    pub army: bool,
    /// The game panel (key I): frame, tabs' pages, scroll bar.
    pub game_panel: bool,
    /// The test editor's window: the placing window and the battle view, wherever it was moved to.
    pub editor: bool,
}

impl Claim {
    pub const ALL: Claim = Claim {
        top_bar: true,
        drop_down: true,
        hints: true,
        messages: true,
        minimap: true,
        commands: true,
        stats: true,
        army: true,
        game_panel: true,
        editor: true,
    };

    pub fn claims(self, layer: Layer) -> bool {
        match layer {
            Layer::TopBar => self.top_bar,
            Layer::DropDown => self.drop_down,
            Layer::Hint => self.hints,
            Layer::Minimap | Layer::MinimapFrame | Layer::MinimapView => self.minimap,
            Layer::Commands => self.commands,
            Layer::Stats | Layer::UnitPanel => self.stats,
            Layer::Army => self.army,
            Layer::GamePanel => self.game_panel,
        }
    }
}

/// The parts of [`Hud::draw`], in drawing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// The map image alone: a new one goes out on its own.
    Minimap,
    /// The player-coloured lines and the border frame around it.
    MinimapFrame,
    /// The camera rectangle and the formation mark over the minimap.
    MinimapView,
    Commands,
    /// The numbers of a single selected type, over the command panel.
    Stats,
    Army,
    /// The unit panel (portrait, values).
    UnitPanel,
    /// The game panel: frame, page and scroll bar.
    GamePanel,
    TopBar,
    DropDown,
    Hint,
}

/// One part of the interface's draws, with the scale of its canvas.
pub struct Section {
    pub layer: Layer,
    pub zoom: Zoom,
    pub draws: Vec<Draw>,
}

/// A group of a claimed part as the external UI gets it: draws in canvas pixels, scaled about the zoom's anchor.
pub struct Claimed {
    pub name: &'static str,
    /// Stacking order over the canvas, low first.
    pub layer: u8,
    pub zoom: Zoom,
    pub draws: Vec<Draw>,
}

/// The groups of a claimed section. The top bar's text is a group of its own, so a changed
/// resource does not resend the frames: strings come after the frames they lie on and no two
/// frames of the bar overlap a string of another, so the split keeps the picture.
pub fn claimed(section: Section) -> Vec<Claimed> {
    let Section { layer, zoom, draws } = section;
    let group = |name, layer, draws| Claimed { name, layer, zoom, draws };
    match layer {
        Layer::Minimap => vec![group("minimap", 0, draws)],
        Layer::MinimapFrame => vec![group("minimap_frame", 1, draws)],
        Layer::MinimapView => vec![group("minimap_view", 2, draws)],
        Layer::Commands => vec![group("commands", 3, draws)],
        Layer::Stats => vec![group("stats", 4, draws)],
        Layer::Army => vec![group("army", 5, draws)],
        Layer::UnitPanel => vec![group("unit_panel", 6, draws)],
        // Beside the unit panel, which it never shares the screen with; no panel, no group.
        Layer::GamePanel if draws.is_empty() => Vec::new(),
        Layer::GamePanel => vec![group("game_panel", 6, draws)],
        Layer::TopBar => {
            let (text, frames): (Vec<Draw>, Vec<Draw>) = draws.into_iter().partition(Draw::is_text);
            vec![group("top_bar", 7, frames), group("top_bar_text", 8, text)]
        }
        Layer::DropDown => vec![group("drop_down", 9, draws)],
        Layer::Hint => vec![group("hint", 10, draws)],
    }
}

/// The messages of the mission over the interface, a claimed group like the others.
pub fn messages_group(draws: Vec<Draw>) -> Claimed {
    Claimed { name: "messages", layer: 11, zoom: Zoom::NONE, draws }
}

/// The test editor's window over the interface, a claimed group like the others (the mission's
/// messages above it).
pub fn editor_group(draws: Vec<Draw>) -> Claimed {
    Claimed { name: "editor", layer: 12, zoom: Zoom::NONE, draws }
}

/// The smallest layout size (`RealLx`, `RealLy`); [`Hud::set_screen`] sets the real one.
pub const SCREEN: [i32; 2] = [1024, 768];

/// `Zone1X`, `Zone1Y`, `PropertyX`, `PropertyY`, `IconLx`, `IconLy`, `AddIconLx`,
/// `AddIconLy`: the defaults of 0x5aa6d2 (`Const.lst` defines none of them).
const ZONE1: [i32; 2] = [6, 6];
const PROPERTY: [i32; 2] = [182, 6];
const ICON: i32 = 40;
const ADD_ICON: i32 = 2;
/// `PanelLy`: the icon rows hang from `RealLy − PanelLy` (0x5aa91c) although no panel is drawn.
const PANEL_LY: i32 = 144;
/// Icons per row of the command grid and visible rows (0x5aa206).
const GRID_COLUMNS: usize = 12;
const GRID_ROWS: usize = 3;
/// The slot of the icon of a single selected type or formation: the original draws the set
/// of selected types (0x28f8190) at (`Zone1X + 6`, the grid's top) in a box of its own
/// beside the stats when it holds exactly one icon (0x5aa136), instead of in the grid.
pub const ZONE1_SLOT: usize = 99;

/// The army icons (`IconSet` 0x28f7658, drawn by 0x5aa36a outside the editor): 5 columns by
/// 7 rows from (6, 31), the top left under the resource bar (0x5aa5ac).
const ARMY_ORIGIN: (i32, i32) = (6, 31);
pub const ARMY_COLUMNS: usize = 5;
pub const ARMY_ROWS: usize = 7;

/// How far the scaled panels reach from their corner (layout pixels), which caps `hud_scale`
/// so they stay on the screen: the command grid's right edge, the army list's height, the
/// minimap's side with its margin.
const COMMANDS_W: i32 = PROPERTY[0] + (ICON + ADD_ICON) * GRID_COLUMNS as i32;
const ARMY_H: i32 = (ICON + ADD_ICON) * ARMY_ROWS as i32;
const MINIMAP_EXTENT: i32 = minimap::SIDE + 17;

/// Width of the resource panel (`Interf2\ResPanel` frame 0, 0x4ecc50).
const RES_PANEL: i32 = 716;
/// Distance of the resource values (right aligned) and their spacing.
const RES_TEXT_X: i32 = 0x77;
const RES_STEP: i32 = 110;
/// The first resource icon in `ResPanel` frame 0 (x; the next ones lie `RES_STEP` apart, each
/// `touch::ICON_W` wide): the touch top bar cuts its slots from the same sprite.
const TOUCH_ICON_X: i32 = 28;
/// The slot of the icon on the Resources plaque: gold.
const TOUCH_FACE_SLOT: usize = 3;

/// Palette indices (palette set 0).
pub mod color {
    /// `GetPaletteColor(255, 200, 64)`: frame of a highlighted icon (0x494f10).
    pub const ICON_SELECTED: [u8; 3] = [255, 200, 64];
    /// `GetPaletteColor(255, 255, 0)`: selection brackets (0x1100a88).
    pub const BRACKETS: [u8; 3] = [255, 255, 0];
    /// `GetPaletteColor(0, 255, 0)`: life bars (0x73473c).
    pub const LIFE: [u8; 3] = [0, 255, 0];
    /// `GetPaletteColor(0, 255, 255)`: the selection rectangle (0x4c4356).
    pub const RECTANGLE: [u8; 3] = [0, 255, 255];
    pub const WHITE: u8 = 0xff;
    /// The reload bar under a unit's life bar (0x4bb5a4, a raw palette index).
    pub const RELOAD: u8 = 0xd0;
    pub const BLACK: u8 = 0;
    /// Background of tooltips (`CBar(..., 185)`, 0x4804bd).
    pub const HINT_BACK: u8 = 185;
    /// First player colour; player `n` is `0xd0 + 4·n` (0x60750b).
    pub const PLAYER: u8 = 0xd0;
}

/// The palette index nearest to an RGB colour (`GetPaletteColor`, 0x46b300).
pub fn nearest(palette: &Palette, rgb: [u8; 3]) -> u8 {
    let dist = |c: [u8; 3]| (0..3).map(|i| (i32::from(c[i]) - i32::from(rgb[i])).pow(2)).sum::<i32>();
    (0..=255u8).min_by_key(|&i| dist(palette.rgb(i))).unwrap_or(0)
}

/// An icon of the unit and command grid (`OneIcon`, drawn by 0x494c96).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Icon {
    /// Frame of `ICONS.GP`.
    pub frame: usize,
    /// Slot in the grid: `column + 12·row`.
    pub slot: usize,
    /// Outline colour (flag 1): the owner's colour index.
    pub outline: Option<u8>,
    /// Life bar (flag 2): life, maximum.
    pub life: Option<(i32, i32)>,
    /// Number in the corner (flag 8).
    pub count: Option<i32>,
    /// Number in the bottom right corner (flag 0x100, `+0x40`, set by 0x494a0d): a
    /// formation's number.
    pub number: Option<i32>,
    /// Minimap pixel marked while the mouse is over the icon (an army icon's formation,
    /// hover callback 0x606500 → 0x4c107f).
    pub mark: Option<[i32; 2]>,
    /// Yellow frame (bit 1 of +0x1a).
    pub selected: bool,
    /// Drawn transparent and not clickable (bit 0 of +0x1a).
    pub disabled: bool,
    /// Drawn through `0\gray.set` (flag 0x200, 0x494dc5): the player cannot afford it.
    pub gray: bool,
    pub hint: Vec<u8>,
    pub action: Option<HudCommand>,
    /// What a right click does (cancelling production, 0x608810).
    pub right: Option<HudCommand>,
    /// The key of a production icon (`[FIXED_PRODUCE]`), upper case.
    pub key: Option<u8>,
}

/// What a click on the interface asks the game to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudCommand {
    /// Keep only the selected units of this type (`ICONS` of the selected types, 0x606360).
    SelectType(usize),
    /// Keep only the selected formation with this index (its icon, 0x606ebd).
    SelectBrigade(u16),
    /// Open the officer's formation `n` (its `ORDERICONS` icon, 0x5f5c68).
    FormGroup(usize),
    /// `DECFORM` (true) / `INCFORM`: the next smaller / larger sizes (0x5f5d49).
    FormSize(bool),
    /// A soldier type of the open formation: form shape `shape` of type `kind` (network
    /// command 0x3a, 0x5f5c77).
    FormNear { shape: u16, kind: u16 },
    /// A formed brigade's formation icons: re-form it as group `n` (0x606680).
    Reform(usize),
    /// `FILLFORM`: fill the formation's empty slots (0x6066b0).
    Fill,
    /// `STANDGR` / `NOSTAND`: stand ground on or off.
    StandGround(bool),
    /// `GO&ATTACK`: moves become attack-moves (`0x28f9e55`, move flag 8) until switched off.
    AttackMode,
    /// The patrol icon (`HPATROL`, 0x606650): the next map click sets the patrol point.
    PatrolMode,
    /// `ATTACKGROUND` (`ATGHINT`, 0x5f5dad): the next map click is the point the selected
    /// cannons bombard.
    AttackGroundMode,
    /// Mortars rule (ACR, #301), "Zielzone setzen": the next click or drag on the map adds
    /// an engagement zone to the selected mortars.
    MortarZoneMode,
    /// ACR, "Zielhilfe": show or hide the guns' and mortars' aim guides on the ground
    /// (`gun_marks.rs`; the `aim_guides` setting).
    AimGuides,
    /// Mortars rule, "Zonen löschen": the selected mortars lose their zones.
    MortarClearZones,
    /// Mortars rule, "Feuer frei" (true) / "Feuer einstellen": shell the zones or stop.
    MortarFire(bool),
    /// `ENABLE_ATTACK` / `DISABLE_ATTACK` (`SetSearchVictim`, 0x4d76cf): hold fire or not.
    HoldFire(bool),
    /// `SHOTMYUNITS` / `KEEPMYUNITS` (network command 0x5e): shoot through own units or not.
    FriendlyFire(bool),
    /// `ARMATTACK` / `ANYATTACK` (network command 0x5f): melee only or not.
    MeleeOnly(bool),
    /// `STOPBUILD` (network command 0x61, click 0x6065d0): stop or resume production.
    StopProduction(bool),
    /// `KARTECH` (0x62, click 0x6065f0): fire canister at once.
    Canister,
    /// `PREKARTECH` (0x63, click 0x606600): load canister.
    LoadCanister,
    /// `FISHING` (0x64, click 0x606610): stop or resume fishing.
    StopFishing(bool),
    /// `ZASEV` (0x65, click 0x606630): stop or resume planting fields.
    StopSowing(bool),
    /// Dissolve the selected formation (`SFREORD`).
    Disband,
    /// `CM_DOGRP` (true) / `CM_UNGRP` (false): group the selected formations or dissolve
    /// their group.
    Group(bool),
    /// Queue a unit of this type in the selected building (0x6088c0; Shift: five).
    Produce(usize),
    /// Cancel a queued unit of this type (right click, 0x608810; Shift: five).
    Unproduce(usize),
    /// Start placing a building of this type (`BuildMode`, 0x608a0d).
    Place(usize),
    /// Show page `n` of the building's icons (`PAGE1..3`, 0x608c60).
    Page(u8),
    /// Let one unit of this type out of each selected building (`0x5f5a66`, command 0x2e).
    Leave(usize),
    /// Research an upgrade in the selected buildings (`PerformUpgrade`).
    Upgrade(u16),
    /// Stop a running research (right click, `StopUpgrade`).
    CancelUpgrade(u16),
    /// Pick the rally point of the selected building on the map (`SET_DEST`).
    Rally,
    /// Market (`ECO::BuildIcons` 0x4a96d9): choose the market slot to sell (0x4a9664).
    MarketSell(usize),
    /// Choose the market slot to buy (0x4a9611).
    MarketBuy(usize),
    /// Change the amount offered (0x4a968a; ×100 with Shift).
    MarketAmount(i32),
    /// `r_Buy`: trade (0x4a961e).
    MarketTrade,
    /// `r_Clear`: amount and result to 0 (0x4a9671).
    MarketClear,
    /// Plant fields around the selected mills (`SPRITEOBJECT` items, command 0x3d).
    PlantFields(usize),
    /// `IDLE_PEASANT` / `IDLE_MINE` (0x5f5d0a): select all idle peasants (false) or all
    /// mines with room for workers (true).
    SelectIdle(bool),
    /// The army icon (`ORDERS_LIST`, click 0x6064a0): show or hide the list of formations
    /// (`0x28f9e54`).
    ArmyList,
    /// A formation of the army list (click 0x6064b0): select its flag bearer and soldiers
    /// (`ImSelBrigade` 0x4e1365 via 0x4d75a6), added to the selection with Shift.
    ArmySelect { brigade: u16, add: bool },
}

/// Numbers of the single selected type's first object (0x607f50).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    /// `KILLUNI` "Treffer %d", drawn even for an unfinished object.
    pub kills: i32,
    /// The object is finished (`Ready`): the rest is drawn only then.
    pub ready: bool,
    /// Life, centred under the icon zone.
    pub life: i32,
    /// Attack icon (`SMICONS.GP` frame) and damage text (`%d+%d` with a bonus), at most two.
    pub attacks: Vec<(usize, Vec<u8>)>,
    /// Shield text, with the formation bonus.
    pub shield: Vec<u8>,
    /// Protection icon and value, at most six; none for buildings.
    pub protections: Vec<(usize, i32)>,
    /// Text lines in `fn10` from (13, `RealLy − 88`), 12 apart: fish, housing, depot capacity,
    /// inside, the mined resource.
    pub lines: Vec<Vec<u8>>,
    /// `SHOWDELAY` types: the summed reload counters and their starting values of the
    /// selected objects, for the reload bar.
    pub reload: Option<(i32, i32)>,
}

/// What the interface shows, collected from the game every frame.
#[derive(Debug, Clone, Default)]
pub struct View {
    pub player: u8,
    /// Resources in `NRES.DAT` order: wood, gold, stone, food, iron, coal.
    pub resources: [u32; 6],
    pub icons: Vec<Icon>,
    /// The army icons at the top left: slot `column + 5·row` (see [`Hud::army_rect`]).
    pub army: Vec<Icon>,
    pub stats: Option<Stats>,
    /// Unit dots of the minimap.
    pub dots: Vec<minimap::Dot>,
    /// Camera position and view size in minimap pixels.
    pub camera: [f32; 2],
    pub view: [f32; 2],
    /// The free camera's ground footprint in minimap pixels (window corners top left, top
    /// right, bottom right, bottom left), drawn instead of the rectangle (#309).
    pub frame: Option<[[i32; 2]; 4]>,
    /// The local player has mission messages: the drop-down offers `UM_HIST` (0x4c6cb0).
    pub history: bool,
    /// Dark 2 × 2 blocks of the minimap (fog of war), `FOG_SIDE²` entries.
    pub fog: Option<Rc<Vec<bool>>>,
    /// The type the unit panel shows (key U).
    pub unit_info: Option<panels::UnitInfo>,
    /// The morale of the selected unit for the unit panel's `THIS_UNIT` page.
    pub morale: Option<panels::MoraleInfo>,
    /// The game panel's page (key I).
    pub game_page: Vec<Draw>,
    /// The market's numbers when a market is selected: the amount offered (once a resource
    /// to sell is chosen) and what it buys (once both are chosen).
    pub market: Option<(Option<i32>, Option<i32>)>,
    /// The game is paused: the touch top bar's pause plaque shows a play glyph.
    pub paused: bool,
}

/// What the game should do after input on the interface.
#[derive(Debug, Clone, PartialEq)]
pub enum HudEvent {
    /// The input was not for the interface.
    None,
    /// The interface took the input.
    Used,
    /// Centre the camera on a point of the minimap (minimap pixels).
    Camera([f32; 2]),
    /// Order the selection to a point of the minimap (minimap pixels).
    Order([f32; 2]),
    Command(HudCommand),
    /// Open the game menu (`UM_MENU`, F12).
    GameMenu,
    /// Restart the mission (`UM_REPLAY`).
    Restart,
    /// Give up (`UM_SURRENDER`).
    Surrender,
    /// Show the mission history (`UM_HIST`).
    History,
    /// Switch the zoomed-out view (`4XSCALE_ON`/`OFF`, 0x5fb4dd).
    Zoom,
    /// The unit panel's Next (1) or Prev (−1) button.
    UnitPanelStep(i32),
    /// The touch top bar's pause plaque: the Pause key.
    TogglePause,
    /// The touch top bar's escape plaque: the Escape key.
    Escape,
}

/// The side panel (`0x1100a74`): none, the game info (key I, 1) or the unit
/// encyclopedia (key U, 2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    None,
    Game,
    Unit,
}

/// Rows of the "M" drop-down (built by 0x4c6840) in a single player game. The tutorial
/// rows (`TUTORIAL`, `UM_CURR`), and the multiplayer, replay and editor rows are not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuEntry {
    /// `UM_MENU` (value 0): the game menu.
    GameMenu,
    /// `UM_INF1`/`UM_INF2` (1): the unit encyclopedia, like key U.
    UnitInfo,
    /// `UM_INF3`/`UM_INF4` (2): the game info, like key I.
    GameInfo,
    /// `UM_MINI` (3): minimap size, like key M.
    Minimap,
    /// `UI_LOCK1`/`UI_LOCK2` (4): the inaccessible areas, like key Q.
    Locks,
    /// `UM_TRANS2`/`UM_TRANS1` (5): transparent buildings, like key O.
    Transparency,
    /// `4XSCALE_ON`/`4XSCALE_OFF` (0x30/0x2f): the zoomed-out view, like key L.
    Zoom,
    /// `UM_SURRENDER` (6).
    Surrender,
    /// `UM_REPLAY` (0x25): restart.
    Restart,
    /// `UM_HIST` (0x26): the mission history, only with messages.
    History,
}

impl MenuEntry {
    /// The text: the alternative rows name what a click does.
    fn text(self, hud: &Hud) -> &'static str {
        match self {
            Self::GameMenu => "UM_MENU",
            Self::UnitInfo if hud.panel == Panel::Unit => "UM_INF2",
            Self::UnitInfo => "UM_INF1",
            Self::GameInfo if hud.panel == Panel::Game => "UM_INF4",
            Self::GameInfo => "UM_INF3",
            Self::Minimap => "UM_MINI",
            Self::Locks if hud.locks => "UI_LOCK2",
            Self::Locks => "UI_LOCK1",
            Self::Transparency if hud.transparent => "UM_TRANS1",
            Self::Transparency => "UM_TRANS2",
            Self::Zoom if hud.zoom => "4XSCALE_OFF",
            Self::Zoom => "4XSCALE_ON",
            Self::Surrender => "UM_SURRENDER",
            Self::Restart => "UM_REPLAY",
            Self::History => "UM_HIST",
        }
    }
}

/// The rows of a single player game, in the order of 0x4c6840.
fn menu_entries(view: &View) -> Vec<MenuEntry> {
    let mut rows = vec![
        MenuEntry::GameMenu,
        MenuEntry::UnitInfo,
        MenuEntry::GameInfo,
        MenuEntry::Minimap,
        MenuEntry::Locks,
        MenuEntry::Transparency,
        MenuEntry::Zoom,
        MenuEntry::Surrender,
        MenuEntry::Restart,
    ];
    if view.history {
        rows.push(MenuEntry::History);
    }
    rows
}

/// The "M" button: `ResPanel` frame 2 at `RealLx − 716 − 65`, clickable strictly inside
/// x + 23..x + 44, 8..26 (0x4c73bb).
fn menu_button(screen: [i32; 2]) -> (i32, i32) {
    (screen[0] - RES_PANEL - 65, 0)
}

/// Width of the "M" plaque and the resource panel together (`RealLx − 781`..`RealLx`).
const TOP_GROUP: i32 = RES_PANEL + 65;

/// How far the top bar (the "M" plaque, resource values and hints) moves from its right
/// anchor (0x4ecc50): with `center` (the `center_top_bar` setting) the group is centred on
/// the screen, otherwise it stays at the original's place.
fn top_bar_dx(screen: [i32; 2], center: bool) -> i32 {
    if center { -(screen[0] - TOP_GROUP) / 2 } else { 0 }
}

pub struct Hud {
    /// The top bar shown: the original's or the touch buttons (`top_bar` in `settings.txt`).
    pub top_bar: TopBar,
    /// Canvas pixels one density-independent pixel covers; sizes the touch buttons.
    density: f32,
    /// The panels' scale (`hud_scale` in `settings.txt`); `None`: the platform's ([`zoom`]).
    pub hud_scale: Option<f32>,
    /// The touch top bar's resource panel is open.
    resources_open: bool,
    /// The top bar is centred on a wide screen (`center_top_bar` in `settings.txt`).
    pub center_top_bar: bool,
    /// The layout size (`RealLx`, `RealLy`), at least [`SCREEN`].
    screen: [i32; 2],
    data: Option<Rc<HudData>>,
    minimap: Option<Minimap>,
    /// Large minimap (`0x668f1d`, starts at 1); M toggles.
    pub large_minimap: bool,
    /// The "M" drop-down is open (`0x1100a79`).
    dropdown: bool,
    mouse: Option<[f32; 2]>,
    /// The left button went down on the interface and is held.
    pressed: bool,
    /// Life bars over all units (Tab, `0x90d411`).
    pub life_bars: bool,
    minimap_dirty: bool,
    last_minimap: Option<MinimapState>,
    /// An external UI draws the camera rectangle over the minimap image, which then has none.
    ui_camera: bool,
    /// The side panel (`0x1100a74`), keys U and I.
    pub panel: Panel,
    /// The inaccessible areas are drawn (`0x90d905`, key Q).
    pub locks: bool,
    /// Buildings are drawn transparent (`0x90d680`, key O).
    pub transparent: bool,
    /// The zoomed-out view is on (`0x28eed70`, key L).
    pub zoom: bool,
    pub unit_panel: panels::UnitPanel,
    pub game_panel: panels::GamePanel,
    /// The army list is open (`0x28f9e54`, toggled by the army icon).
    pub army_list: bool,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            top_bar: TopBar::Classic,
            density: 1.0,
            hud_scale: None,
            resources_open: false,
            center_top_bar: false,
            screen: SCREEN,
            data: None,
            minimap: None,
            large_minimap: true,
            dropdown: false,
            mouse: None,
            pressed: false,
            life_bars: false,
            minimap_dirty: true,
            last_minimap: None,
            ui_camera: false,
            panel: Panel::None,
            locks: false,
            transparent: false,
            zoom: false,
            unit_panel: panels::UnitPanel::new(),
            game_panel: panels::GamePanel::default(),
            army_list: false,
        }
    }
}

/// What the minimap image was last composed from: placement, dots, camera, fog.
type MinimapState = (Placement, Vec<minimap::Dot>, Option<Rect>, Option<Rc<Vec<bool>>>, Option<[[i32; 2]; 4]>);

/// Name of the minimap image in [`crate::ui::draw::Textures`].
pub const MINIMAP_IMAGE: &str = "hud:minimap";

impl Hud {
    /// The interface data once its files are loaded (requests them otherwise).
    pub fn data(&mut self, ctx: &Ctx) -> Option<Rc<HudData>> {
        if self.data.is_none() {
            let (Some(list), Some(nres)) = (ctx.lib.bytes(HudData::ICON_LIST), ctx.lib.bytes(HudData::NRES)) else {
                return None;
            };
            self.data = Some(Rc::new(HudData::parse(&list, &nres)));
        }
        self.data.clone()
    }

    pub fn set_minimap(&mut self, minimap: Minimap) {
        self.minimap = Some(minimap);
        self.minimap_dirty = true;
    }

    pub fn has_minimap(&self) -> bool {
        self.minimap.is_some()
    }

    /// Lays the interface out for a screen of `screen` canvas pixels (`RealLx`, `RealLy`).
    pub fn set_screen(&mut self, screen: [i32; 2]) {
        let screen = [screen[0].max(SCREEN[0]), screen[1].max(SCREEN[1])];
        if screen != self.screen {
            log::info!("interface laid out for a screen of {} x {}", screen[0], screen[1]);
            self.screen = screen;
            self.minimap_dirty = true;
        }
    }

    pub fn screen(&self) -> [i32; 2] {
        self.screen
    }

    /// Sets how many canvas pixels one density-independent pixel covers (the window's scale
    /// factor over the canvas scale), which sizes the touch top bar.
    pub fn set_density(&mut self, density: f32) {
        self.density = density;
    }

    fn touch(&self) -> bool {
        self.top_bar == TopBar::Touch
    }

    /// The touch top bar's buttons and panel.
    pub fn touch_layout(&self) -> touch::Layout {
        touch::Layout::new(self.screen, self.density)
    }

    /// A press at `p` on the touch top bar: Pause and Escape are events for the game, Menu opens the "M" drop-down (closes it when it
    /// was `was_open`, which the press has just shut), Resources toggles the panel, the open
    /// panel swallows the click; `None` elsewhere.
    fn touch_press(&mut self, p: [f32; 2], was_open: bool) -> Option<HudEvent> {
        let layout = self.touch_layout();
        if self.resources_open && layout.panel.contains(p) {
            return Some(HudEvent::Used);
        }
        if layout.menu.contains(p) {
            self.dropdown = !was_open;
            return Some(HudEvent::Used);
        }
        if layout.pause.contains(p) {
            return Some(HudEvent::TogglePause);
        }
        if layout.escape.contains(p) {
            return Some(HudEvent::Escape);
        }
        if layout.resources.contains(p) {
            self.resources_open = !self.resources_open;
            return Some(HudEvent::Used);
        }
        None
    }

    /// The "M" drop-down is open.
    pub fn dropdown_open(&self) -> bool {
        self.dropdown
    }

    /// The touch top bar's resource panel is open.
    pub fn resources_open(&self) -> bool {
        self.resources_open
    }

    fn top_dx(&self) -> i32 {
        top_bar_dx(self.screen, self.center_top_bar)
    }

    /// [`menu_button`] with the top bar's shift.
    fn menu_button(&self) -> (i32, i32) {
        let (x, y) = menu_button(self.screen);
        (x + self.top_dx(), y)
    }

    /// How far the side panels move right: they are laid out for `RealLx` = 1024 (see
    /// [`panels`]) and anchored to the right edge.
    fn panel_dx(&self) -> i32 {
        self.screen[0] - SCREEN[0]
    }

    /// A canvas point in the side panels' coordinates.
    fn panel_point(&self, p: [f32; 2]) -> [f32; 2] {
        let p = self.corner_zoom(Group::Panel).to_layout(p);
        [p[0] - self.panel_dx() as f32, p[1]]
    }

    /// The scale about the corner a group of panels hangs from (`hud_scale`), kept so that
    /// the group stays on the screen (the minimap at most half of its height).
    fn corner_zoom(&self, group: Group) -> Zoom {
        let scale = zoom::resolve(self.hud_scale, self.density, cfg!(target_os = "android"), ICON);
        let [w, h] = self.screen.map(|v| v as f32);
        match group {
            Group::Commands => self.commands_zoom(PROPERTY[0] - 2),
            Group::Army => {
                let anchor = [ARMY_ORIGIN.0 as f32, ARMY_ORIGIN.1 as f32];
                Zoom { anchor, scale: zoom::fit(scale, h - anchor[1], ARMY_H as f32) }
            }
            Group::Minimap => Zoom { anchor: [w, h], scale: zoom::fit(scale, h / 2.0, MINIMAP_EXTENT as f32) },
            Group::Panel => Zoom { anchor: [w, 0.0], scale: zoom::fit(scale, h, (panels::INF_Y2 + 8) as f32) },
        }
    }

    /// The left edge of what the command panel draws: the single type's box (`Zone1X + 6`)
    /// when there is one, else the commands' frame (`PropertyX − 2`).
    fn commands_left(view: &View) -> i32 {
        if view.icons.iter().any(|i| i.slot == ZONE1_SLOT) { ZONE1[0] + 6 } else { PROPERTY[0] - 2 }
    }

    /// [`Hud::corner_zoom`] of `group`; the command panel is also pulled left so that its
    /// leftmost drawn part sits at the screen's edge.
    pub fn zoom(&self, group: Group, view: &View) -> Zoom {
        match group {
            Group::Commands => self.commands_zoom(Self::commands_left(view)),
            _ => self.corner_zoom(group),
        }
    }

    /// The command panel's scale about the bottom edge with layout x `left` pulled to the
    /// screen's left edge: the anchor `left·s / (s − 1)` maps `left` to 0. Capped so that the
    /// grid's right edge stays on the screen.
    fn commands_zoom(&self, left: i32) -> Zoom {
        let scale = zoom::resolve(self.hud_scale, self.density, cfg!(target_os = "android"), ICON);
        let [w, h] = self.screen.map(|v| v as f32);
        let scale = zoom::fit(scale, w, (COMMANDS_W - left) as f32);
        if scale <= 1.0 {
            return Zoom { anchor: [0.0, h], scale: 1.0 };
        }
        Zoom { anchor: [left as f32 * scale / (scale - 1.0), h], scale }
    }

    /// A canvas point in the layout of `group`.
    fn local(&self, group: Group, view: &View, p: [f32; 2]) -> [f32; 2] {
        self.zoom(group, view).to_layout(p)
    }

    /// The mouse in the layout of `group`.
    fn mouse_in(&self, group: Group, view: &View) -> Option<[f32; 2]> {
        self.mouse.map(|m| self.local(group, view, m))
    }

    /// Command icon `slot` on the canvas, `[x, y, w, h]`.
    pub fn icon_on_canvas(&self, slot: usize, view: &View) -> [f32; 4] {
        self.zoom(Group::Commands, view).rect(Self::icon_rect(self.screen, slot))
    }

    /// Army icon `slot` on the canvas, `[x, y, w, h]`.
    pub fn army_on_canvas(&self, slot: usize) -> [f32; 4] {
        self.corner_zoom(Group::Army).rect(Self::army_rect(slot))
    }

    fn placement(&self, view: &View) -> Placement {
        Placement::new(self.large_minimap, self.screen, view.camera, view.view)
    }

    /// Whether an external UI draws the camera rectangle ([`Claim::minimap`]): the image is then
    /// composed without it, so the camera moving does not recompose the image.
    pub fn set_ui_camera(&mut self, on: bool) {
        if self.ui_camera != on {
            self.ui_camera = on;
            self.minimap_dirty = true;
        }
    }

    /// The minimap image if it changed since the last call.
    pub fn minimap_image(&mut self, ctx: &Ctx, view: &View) -> Option<acr_formats::raster::Image> {
        let map = self.minimap.as_ref()?;
        let palette = ctx.lib.palette(GAME_PALETTE)?;
        let placement = self.placement(view);
        let camera = Rect::new(
            view.camera[0].floor() as i32,
            view.camera[1].floor() as i32,
            (view.view[0] as i32).max(1),
            (view.view[1] as i32).max(1),
        );
        let (camera, frame) = if self.ui_camera { (None, None) } else { (Some(camera), view.frame) };
        let state = (placement, view.dots.clone(), camera, view.fog.clone(), frame);
        if !self.minimap_dirty && self.last_minimap.as_ref() == Some(&state) {
            return None;
        }
        let fog = view.fog.as_deref().map(Vec::as_slice);
        let image = minimap::compose(map, &placement, &view.dots, fog, camera, frame, &palette);
        self.last_minimap = Some(state);
        self.minimap_dirty = false;
        Some(image)
    }

    /// The command grid's top-left corner: `PropertyX`, `RealLy − PanelLy + PropertyY`.
    fn grid_origin(screen: [i32; 2]) -> (i32, i32) {
        (PROPERTY[0], screen[1] - PANEL_LY + PROPERTY[1])
    }

    pub(crate) fn icon_rect(screen: [i32; 2], slot: usize) -> Rect {
        let (x, y) = Self::grid_origin(screen);
        if slot == ZONE1_SLOT {
            return Rect::new(ZONE1[0] + 6, y, ICON, ICON);
        }
        let step = ICON + ADD_ICON;
        Rect::new(x + (slot % GRID_COLUMNS) as i32 * step, y + (slot / GRID_COLUMNS) as i32 * step, ICON, ICON)
    }

    /// An army icon's place: `IconLx + AddIconLx` apart from (6, 31) (`DrawIconSet` 0x4958f7).
    pub(crate) fn army_rect(slot: usize) -> Rect {
        let step = ICON + ADD_ICON;
        let (x, y) = ARMY_ORIGIN;
        Rect::new(x + (slot % ARMY_COLUMNS) as i32 * step, y + (slot / ARMY_COLUMNS) as i32 * step, ICON, ICON)
    }

    /// The army icons that are drawn (`DrawIconSet` shows `nx · ny` of them).
    pub(crate) fn army_icons(view: &View) -> impl Iterator<Item = &Icon> {
        view.army.iter().filter(|i| i.slot < ARMY_COLUMNS * ARMY_ROWS)
    }

    /// Columns used by the visible icons (`GetMaxX`, 0x495b86).
    fn columns(icons: &[Icon]) -> i32 {
        icons.iter().filter(|i| i.slot < GRID_COLUMNS * GRID_ROWS).map(|i| i.slot % GRID_COLUMNS + 1).max().unwrap_or(0)
            as i32
    }

    /// Icons drawn and clickable: the grid's three rows and the single type's icon.
    pub(crate) fn shown(slot: usize) -> bool {
        slot < GRID_COLUMNS * GRID_ROWS || slot == ZONE1_SLOT
    }

    /// The boxes behind the icons and their corner tables. With a single selected type
    /// (0x5aa143): a box from `Zone1X + 6` to `PropertyX − 2` around the type's icon and
    /// stats, open to the right (0x480166, table 0x6685a0), and the commands' box joined to
    /// it (0x480198, table 0x6685c0); without commands the first box closed (0x4800c8).
    /// Otherwise the commands' box alone (0x5aa2f3).
    fn icon_boxes(screen: [i32; 2], icons: &[Icon]) -> Vec<(Rect, [i32; 8])> {
        let commands = Self::icon_box(screen, icons);
        if !icons.iter().any(|i| i.slot == ZONE1_SLOT) {
            return commands.map(|r| (r, BOX_CORNERS)).into_iter().collect();
        }
        let y = screen[1] - PANEL_LY + ZONE1[1];
        let left = Rect { x0: ZONE1[0] + 6, y0: y, x1: PROPERTY[0] - 2, y1: y + ICON * 3 + 5 };
        match commands {
            Some(r) => vec![(left, LEFT_BOX_CORNERS), (r, RIGHT_BOX_CORNERS)],
            None => vec![(left, BOX_CORNERS)],
        }
    }

    /// The box behind the icons (0x5aa2f3): from `PropertyX − 2` to the last used column.
    fn icon_box(screen: [i32; 2], icons: &[Icon]) -> Option<Rect> {
        let n = Self::columns(icons);
        if n == 0 {
            return None;
        }
        let (x, _) = Self::grid_origin(screen);
        let y = screen[1] - PANEL_LY + ZONE1[1];
        let x0 = x - 2;
        Some(Rect { x0, y0: y, x1: x0 + 2 + (ICON + ADD_ICON) * n, y1: y + ICON * 3 + 5 })
    }

    /// The rows as wide as the widest text + 24, centred on the button, from y = 60, 24
    /// pixels apart (0x4c6840).
    fn dropdown_rows(&self, ctx: &Ctx, view: &View) -> (i32, Vec<(MenuEntry, Rect)>) {
        let entries = menu_entries(view);
        let width = entries.iter().map(|e| ctx.fonts.width(ctx.lib, font::WHITE, ctx.texts.get(e.text(self)))).max();
        let width = width.unwrap_or(0) + 24;
        // Classic: centred under the "M" plaque, rows 24 high from y = 60. Touch: under the
        // "M" plaque, rows as high as its target, fitted into the screen.
        let (x, y0, row) = if self.touch() {
            let l = self.touch_layout();
            let gap = l.margin;
            let y0 = l.menu_draw.y1 + touch::SHADOW_H + gap;
            let fit = (self.screen[1] - y0 - gap) / entries.len().max(1) as i32;
            // Left aligned under the plaque, pushed left where it would leave the screen.
            let x = l.menu_draw.x0.min(self.screen[0] - width - gap).max(0);
            (x, y0, l.menu.height().min(fit).max(24))
        } else {
            let (bx, _) = self.menu_button();
            (bx + 33 - width / 2, 60, 24)
        };
        let rows =
            entries.into_iter().enumerate().map(|(i, e)| (e, Rect::new(x, y0 + row * i as i32, width, row))).collect();
        (width, rows)
    }

    /// Key U or `UM_INF1/2`: the unit encyclopedia on or off.
    pub fn toggle_unit_panel(&mut self) {
        self.panel = if self.panel == Panel::Unit { Panel::None } else { Panel::Unit };
        self.dropdown = false;
    }

    /// Key I or `UM_INF3/4`: the game info on or off.
    pub fn toggle_game_panel(&mut self) {
        self.panel = if self.panel == Panel::Game { Panel::None } else { Panel::Game };
        self.dropdown = false;
    }

    /// Key Q or `UI_LOCK1/2`.
    pub fn toggle_locks(&mut self) {
        self.locks = !self.locks;
        self.dropdown = false;
    }

    /// Key O or `UM_TRANS1/2`.
    pub fn toggle_transparency(&mut self) {
        self.transparent = !self.transparent;
        self.dropdown = false;
    }

    /// Whether a canvas point is over a part of the interface that takes mouse input.
    pub fn covers(&self, ctx: &Ctx, view: &View, p: [f32; 2]) -> bool {
        if self.dropdown && self.dropdown_rows(ctx, view).1.iter().any(|(_, r)| r.contains(p)) {
            return true;
        }
        if self.panel != Panel::None && panels::covers(self.panel_point(p)) {
            return true;
        }
        if self.touch() {
            if self.touch_layout().covers(p, self.resources_open) {
                return true;
            }
        } else {
            let (bx, by) = self.menu_button();
            let dx = self.top_dx();
            let x0 = (self.screen[0] - RES_PANEL + dx) as f32;
            // Centred, the bar ends before the screen's edge; anchored, it runs to it.
            let x1 = if dx == 0 { f32::INFINITY } else { x0 + RES_PANEL as f32 };
            if Rect::new(bx, by, 67, 42).contains(p) || (p[1] < 42.0 && p[0] >= x0 && p[0] < x1) {
                return true;
            }
        }
        let commands = self.local(Group::Commands, view, p);
        if Self::icon_boxes(self.screen, &view.icons).iter().any(|(r, _)| r.contains(commands)) {
            return true;
        }
        let army = self.local(Group::Army, view, p);
        if Self::army_icons(view).any(|i| Self::army_rect(i.slot).contains(army)) {
            return true;
        }
        self.minimap.is_some() && self.placement(view).rect.contains(self.local(Group::Minimap, view, p))
    }

    pub fn release(&mut self) {
        self.pressed = false;
        self.unit_panel.release();
        self.game_panel.release();
    }

    /// The portrait of the unit panel when it changed.
    pub fn take_images(&mut self) -> Vec<(&'static str, acr_formats::raster::Image)> {
        self.unit_panel.take_image().map(|i| (panels::PORTRAIT_IMAGE, i)).into_iter().collect()
    }

    /// Left button pressed at canvas point `p`.
    pub fn press(&mut self, ctx: &Ctx, view: &View, p: [f32; 2]) -> HudEvent {
        self.mouse = Some(p);
        let was_open = self.dropdown;
        if self.dropdown {
            self.dropdown = false;
            // The click callback 0x4c6550.
            if let Some(&(entry, _)) = self.dropdown_rows(ctx, view).1.iter().find(|(_, r)| r.contains(p)) {
                return match entry {
                    MenuEntry::GameMenu => HudEvent::GameMenu,
                    MenuEntry::UnitInfo => {
                        self.toggle_unit_panel();
                        HudEvent::Used
                    }
                    MenuEntry::GameInfo => {
                        self.toggle_game_panel();
                        HudEvent::Used
                    }
                    MenuEntry::Minimap => {
                        self.toggle_minimap();
                        HudEvent::Used
                    }
                    MenuEntry::Locks => {
                        self.toggle_locks();
                        HudEvent::Used
                    }
                    MenuEntry::Transparency => {
                        self.toggle_transparency();
                        HudEvent::Used
                    }
                    MenuEntry::Zoom => HudEvent::Zoom,
                    MenuEntry::Surrender => HudEvent::Surrender,
                    MenuEntry::Restart => HudEvent::Restart,
                    MenuEntry::History => HudEvent::History,
                };
            }
        }
        // An open panel swallows the clicks in its column (0x4c1c36).
        let pp = self.panel_point(p);
        if self.panel != Panel::None && panels::covers(pp) {
            match self.panel {
                Panel::Unit => match self.unit_panel.press(pp) {
                    panels::UnitPanelEvent::Close => self.panel = Panel::None,
                    panels::UnitPanelEvent::Step(d) => return HudEvent::UnitPanelStep(d),
                    _ => {}
                },
                _ => {
                    if !self.game_panel.press(pp) {
                        self.panel = Panel::None;
                    }
                }
            }
            return HudEvent::Used;
        }
        if self.touch() {
            if let Some(event) = self.touch_press(p, was_open) {
                return event;
            }
        } else {
            let (bx, _) = self.menu_button();
            let [x, y] = p;
            if x > (bx + 23) as f32 && x < (bx + 44) as f32 && y > 8.0 && y < 26.0 {
                self.dropdown = true;
                return HudEvent::Used;
            }
        }
        let (army, commands, mini) = (
            self.local(Group::Army, view, p),
            self.local(Group::Commands, view, p),
            self.local(Group::Minimap, view, p),
        );
        if let Some(icon) = Self::army_icons(view).find(|i| Self::army_rect(i.slot).contains(army)) {
            self.pressed = true;
            return Self::icon_event(icon, false);
        }
        for icon in &view.icons {
            if Self::icon_rect(self.screen, icon.slot).contains(commands) && Self::shown(icon.slot) {
                self.pressed = true;
                return Self::icon_event(icon, false);
            }
        }
        if self.minimap.is_some()
            && let Some(px) = self.placement(view).pixel_at(mini)
        {
            self.pressed = true;
            return HudEvent::Camera(px);
        }
        if self.covers(ctx, view, p) { HudEvent::Used } else { HudEvent::None }
    }

    /// What a click on `icon` does: its command (left: `action`, right: `right`) unless it is
    /// disabled or has none, which the interface just takes.
    fn icon_event(icon: &Icon, right: bool) -> HudEvent {
        match (if right { &icon.right } else { &icon.action }, icon.disabled) {
            (Some(a), false) => HudEvent::Command(a.clone()),
            _ => HudEvent::Used,
        }
    }

    /// The event of a click on the icon `id` (`hud/<slot>` of the grid, `army/<slot>`) without a
    /// pixel (the touch skin): what [`Hud::press`] or [`Hud::right_press`] returns for it, but
    /// only a command; an icon that does nothing is refused.
    pub fn icon_click(view: &View, id: &str, right: bool) -> Result<HudEvent, Reject> {
        let (group, slot) = id.split_once('/').ok_or(Reject::NoWidget)?;
        let slot: usize = slot.parse().map_err(|_| Reject::NoWidget)?;
        let icon = match group {
            "hud" => view.icons.iter().find(|i| i.slot == slot && Self::shown(slot)),
            "army" if !right => Self::army_icons(view).find(|i| i.slot == slot),
            _ => None,
        }
        .ok_or(Reject::NoWidget)?;
        match Self::icon_event(icon, right) {
            HudEvent::Used if icon.disabled => Err(Reject::Disabled),
            HudEvent::Used => Err(Reject::Kind("an icon with a command")),
            event => Ok(event),
        }
    }

    /// The minimap pixel at `uv` (`0..=1` from the top-left of the shown minimap), if the
    /// minimap is on; the point [`Hud::press`] makes of the same place.
    pub fn minimap_pixel(&self, view: &View, [u, v]: [f32; 2]) -> Result<[f32; 2], Reject> {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return Err(Reject::Range);
        }
        if self.minimap.is_none() {
            return Err(Reject::Hidden);
        }
        let p = self.placement(view);
        Ok([u * p.rect.width() as f32 + p.offset[0] as f32, v * p.rect.height() as f32 + p.offset[1] as f32])
    }

    /// Mouse dragged with the left button held: the minimap keeps moving the camera.
    pub fn drag(&mut self, view: &View, p: [f32; 2]) -> HudEvent {
        self.mouse = Some(p);
        let pp = self.panel_point(p);
        if self.panel == Panel::Unit {
            self.unit_panel.mouse_move(pp);
        }
        if self.panel == Panel::Game {
            self.game_panel.mouse_move(pp);
        }
        if self.pressed
            && self.minimap.is_some()
            && let Some(px) = self.placement(view).pixel_at(self.local(Group::Minimap, view, p))
        {
            return HudEvent::Camera(px);
        }
        HudEvent::None
    }

    /// Right button at canvas point `p`: an icon's right-click command, or an order to a
    /// point of the minimap.
    pub fn right_press(&mut self, ctx: &Ctx, view: &View, p: [f32; 2]) -> HudEvent {
        if self.panel != Panel::None && panels::covers(self.panel_point(p)) {
            return HudEvent::Used;
        }
        let army = self.local(Group::Army, view, p);
        if Self::army_icons(view).any(|i| Self::army_rect(i.slot).contains(army)) {
            return HudEvent::Used;
        }
        let commands = self.local(Group::Commands, view, p);
        if let Some(icon) =
            view.icons.iter().find(|i| Self::shown(i.slot) && Self::icon_rect(self.screen, i.slot).contains(commands))
        {
            return Self::icon_event(icon, true);
        }
        if self.minimap.is_some()
            && let Some(px) = self.placement(view).pixel_at(self.local(Group::Minimap, view, p))
        {
            return HudEvent::Order(px);
        }
        if self.covers(ctx, view, p) { HudEvent::Used } else { HudEvent::None }
    }

    pub fn toggle_minimap(&mut self) {
        self.large_minimap = !self.large_minimap;
        self.minimap_dirty = true;
    }

    /// The interface in layers in drawing order, each with the [`Zoom`] of its canvas
    /// (`hud_scale`): the minimap, the command panel, the army list, the side panels, then
    /// the top bar and the drop-down at the canvas' scale and the hint at the mouse.
    pub fn draw(&mut self, ctx: &Ctx, view: &View) -> Vec<Section> {
        let Some(palette) = ctx.lib.palette(GAME_PALETTE) else { return Vec::new() };
        let data = self.data(ctx);
        let mut layers = Vec::new();
        // The minimap (0x4c0a40) comes first, then the icons (0x5aa206): the boxes, the
        // icons and the unit stats over them (0x607f50, called at 0x5aa278), so the reload
        // bar at x = 173..176 lies on the boxes' joint; then the resource panel (0x4ecc50).
        if self.minimap.is_some() {
            let (mut image, mut out, mut over) = (Vec::new(), Vec::new(), Vec::new());
            self.draw_minimap(ctx, view, &palette, &mut image, &mut out, &mut over);
            let zoom = self.zoom(Group::Minimap, view);
            layers.push(Section { layer: Layer::Minimap, zoom, draws: image });
            layers.push(Section { layer: Layer::MinimapFrame, zoom, draws: out });
            layers.push(Section { layer: Layer::MinimapView, zoom, draws: over });
        }
        let screen = self.screen;
        let mut out = Vec::new();
        for (r, corners) in Self::icon_boxes(screen, &view.icons) {
            border(ctx, &mut out, r.x0, r.y0, r.x1, r.y1, corners);
        }
        let mouse = self.mouse_in(Group::Commands, view);
        for icon in view.icons.iter().filter(|i| Self::shown(i.slot)) {
            let r = Self::icon_rect(screen, icon.slot);
            let pressed = self.pressed && mouse.is_some_and(|m| r.contains(m));
            draw_icon(ctx, icon, r.x0, r.y0, pressed, &palette, &mut out);
        }
        let mut numbers = Vec::new();
        if let (Some(stats), Some(_)) = (&view.stats, &data) {
            draw_stats(ctx, stats, &palette, screen, &mut numbers);
        }
        if let Some(market) = view.market {
            draw_market(ctx, market, screen, &mut out);
        }
        let zoom = self.zoom(Group::Commands, view);
        layers.push(Section { layer: Layer::Commands, zoom, draws: out });
        layers.push(Section { layer: Layer::Stats, zoom, draws: numbers });
        // The army icons, the last set 0x5a9fa0 draws (0x5aa394).
        let mut out = Vec::new();
        let mouse = self.mouse_in(Group::Army, view);
        for icon in Self::army_icons(view) {
            let r = Self::army_rect(icon.slot);
            let pressed = self.pressed && mouse.is_some_and(|m| r.contains(m));
            draw_icon(ctx, icon, r.x0, r.y0, pressed, &palette, &mut out);
        }
        layers.push(Section { layer: Layer::Army, zoom: self.zoom(Group::Army, view), draws: out });
        let mut panel = Vec::new();
        let mut layer = Layer::GamePanel;
        match (self.panel, &view.unit_info) {
            (Panel::Unit, Some(info)) => {
                layer = Layer::UnitPanel;
                self.unit_panel.draw(ctx, info, view.morale.as_ref(), &palette, &mut panel)
            }
            (Panel::Game, _) => {
                self.game_panel.draw_frame(ctx, &mut panel);
                panel.extend(view.game_page.iter().cloned());
                self.game_panel.draw_widgets(ctx, &mut panel);
            }
            _ => {}
        }
        let dx = self.panel_dx();
        layers.push(Section {
            layer,
            zoom: self.zoom(Group::Panel, view),
            draws: panel.into_iter().map(|d| d.translated(dx, 0)).collect(),
        });
        let mut out = Vec::new();
        if self.touch() {
            self.draw_touch_bar(ctx, view, &mut out);
        } else {
            draw_resources(ctx, view, screen, self.top_dx(), &mut out);
        }
        layers.push(Section { layer: Layer::TopBar, zoom: Zoom::NONE, draws: out });
        if self.dropdown {
            let mut out = Vec::new();
            self.draw_dropdown(ctx, view, &mut out);
            layers.push(Section { layer: Layer::DropDown, zoom: Zoom::NONE, draws: out });
        }
        layers.extend(self.draw_hint(ctx, view, &palette));
        layers
    }

    /// The map image in `image`, its frame in `out`; the camera and the formation mark, which
    /// change on their own, in `over`.
    fn draw_minimap(
        &self,
        ctx: &Ctx,
        view: &View,
        palette: &Palette,
        image: &mut Vec<Draw>,
        out: &mut Vec<Draw>,
        over: &mut Vec<Draw>,
    ) {
        let placement = self.placement(view);
        let r = placement.rect;
        image.push(Draw::image(MINIMAP_IMAGE, r));
        // Two lines in the player's colour around it, then the "Border" frame (0x4c155c).
        let inner = palette_rgba(palette, color::PLAYER + 4 * (view.player % 8));
        let outer = palette_rgba(palette, color::PLAYER + 4 * (view.player % 8) + 1);
        outline(out, Rect { x0: r.x0 - 1, y0: r.y0 - 1, x1: r.x1 + 1, y1: r.y1 + 1 }, inner);
        outline(out, Rect { x0: r.x0 - 2, y0: r.y0 - 2, x1: r.x1 + 2, y1: r.y1 + 2 }, outer);
        border(ctx, out, r.x0 - 2, r.y0 - 2, r.x1 + 1, r.y1 + 1, BOX_CORNERS);
        // The formation of the army icon under the mouse (0x606500 sets 0x66a8e8, 0x4c107f
        // draws): an octagon in `GetPaletteColor(255, 255, 0)` around its members' mean
        // position.
        let mark = self
            .mouse_in(Group::Army, view)
            .and_then(|m| Self::army_icons(view).find(|i| Self::army_rect(i.slot).contains(m)).and_then(|i| i.mark));
        if let Some([px, py]) = mark {
            let x = r.x0 + 1 + px - placement.offset[0];
            let y = r.y0 + 1 + py - placement.offset[1];
            let c = palette_rgba(palette, nearest(palette, color::BRACKETS));
            let mut lines = Vec::new();
            for [x0, y0, x1, y1] in [
                [-5, -2, -5, 2],
                [5, -2, 5, 2],
                [-2, -5, 1, -5],
                [-2, 5, 2, 5],
                [-5, -2, -2, -5],
                [5, -2, 2, -5],
                [-2, 5, -5, 2],
                [2, 5, 5, 2],
            ] {
                line(&mut lines, c, (x + x0, y + y0), (x + x1, y + y1));
            }
            over.extend(lines.into_iter().map(|d| d.clipped(r)));
        }
        if self.ui_camera {
            let mut lines = Vec::new();
            let white = palette_rgba(palette, minimap::WHITE);
            let shift = |[x, y]: [i32; 2]| (r.x0 + x - placement.offset[0], r.y0 + y - placement.offset[1]);
            if let Some(corners) = view.frame {
                for i in 0..4 {
                    line(&mut lines, white, shift(corners[i]), shift(corners[(i + 1) % 4]));
                }
            } else {
                let c = Rect::new(
                    view.camera[0].floor() as i32,
                    view.camera[1].floor() as i32,
                    (view.view[0] as i32).max(1),
                    (view.view[1] as i32).max(1),
                );
                let (x0, y0) = shift([c.x0, c.y0]);
                let (x1, y1) = shift([c.x1, c.y1]);
                outline(&mut lines, Rect { x0, y0, x1, y1 }, white);
            }
            // Behind the formation mark, inside the map image as the engine bakes it.
            over.splice(0..0, lines.into_iter().map(|d| d.clipped(r)));
        }
    }

    fn draw_dropdown(&self, ctx: &Ctx, view: &View, out: &mut Vec<Draw>) {
        let (_, rows) = self.dropdown_rows(ctx, view);
        let (Some((_, first)), Some((_, last))) = (rows.first(), rows.last()) else { return };
        border(ctx, out, first.x0 - 2, first.y0 - 2, last.x1 + 1, last.y1 + 1, BOX_CORNERS);
        for (i, (entry, r)) in rows.iter().enumerate() {
            let hover = self.mouse.is_some_and(|m| r.contains(m));
            // `addGP_TextButtonLimited(.., Border, 74 + i % 3, 77 + i % 3, width, WhiteFont, YellowFont)`.
            let frame = if hover { 77 + i % 3 } else { 74 + i % 3 };
            // The frame repeats along the row (`GP_TextButton` draw 0x46ef12); a taller touch
            // row keeps its 24-pixel frame in the middle.
            let step = ctx.lib.frame_size(gp::BORDER, frame)[0].max(1);
            let fy = r.y0 + (r.height() - 24) / 2;
            if r.height() > 24 {
                out.push(Draw::fill([0, 0, 0, 160], *r));
            }
            for k in 0..(r.width() + step - 1) / step {
                out.push(Draw::game_frame(gp::BORDER, frame, r.x0 + k * step, fy, 0).clipped(Rect {
                    y0: fy,
                    y1: fy + 24,
                    ..*r
                }));
            }
            let text = ctx.texts.get(entry.text(self));
            let f = if hover { font::WHITE } else { font::YELLOW };
            let w = ctx.fonts.width(ctx.lib, f, text);
            out.push(Draw::game_text(f, text, r.x0 + (r.width() - w) / 2, fy + 2));
        }
    }

    /// The touch top bar: the classic bar's "M" plaque and a short bit of the bar with the gold
    /// icon, and under the latter the six slots of the bar stacked while the panel is open.
    fn draw_touch_bar(&self, ctx: &Ctx, view: &View, out: &mut Vec<Draw>) {
        let layout = self.touch_layout();
        // A piece of `ResPanel` `frame` from source column `sx`, `w` wide, drawn at `x` with
        // its body from `y` to `bottom` (the sprite's own top row is empty).
        let piece = |out: &mut Vec<Draw>, frame: usize, sx: i32, x: i32, w: i32, y: i32, bottom: i32| {
            let clip = Rect::new(x, y, w, bottom - y);
            out.push(Draw::game_frame(gp::RESPANEL, frame, x - sx, y - 1, 0).clipped(clip));
        };
        let shadow = touch::SHADOW_H;
        let m = layout.menu_draw;
        piece(out, 2, touch::CAP_X, m.x0, touch::M_W, m.y0, m.y1 + shadow);
        // A slot of the bar: end cap, filler, the icon of `slot`, end cap; `mid` wide between the caps.
        let slot_piece = |out: &mut Vec<Draw>, r: Rect, mid: i32, slot: Option<usize>, bottom: i32| {
            let x = r.x0 + touch::CAP_W;
            piece(out, 0, touch::CAP_X, r.x0, touch::CAP_W, r.y0, bottom);
            piece(out, 0, touch::FILL_X, x, mid.min(touch::FILL_W), r.y0, bottom);
            if mid > touch::FILL_W {
                piece(out, 0, touch::FILL_X, x + touch::FILL_W, mid - touch::FILL_W, r.y0, bottom);
            }
            if let Some(slot) = slot {
                let icon = TOUCH_ICON_X + slot as i32 * RES_STEP;
                piece(out, 0, icon, x + touch::ICON_PAD, touch::ICON_W, r.y0, bottom);
            }
            piece(out, 0, touch::RIGHT_CAP_X, x + mid, touch::CAP_W, r.y0, bottom);
        };
        // The pause and escape plaques: a bare bit of the bar with a glyph of the game's font.
        let pause: &[u8] = if view.paused { b">" } else { b"II" };
        for (r, mid, glyph) in
            [(layout.pause_draw, touch::PAUSE_MID, pause), (layout.escape_draw, touch::ESCAPE_MID, b"O".as_slice())]
        {
            slot_piece(out, r, mid, None, r.y1 + shadow);
            let w = ctx.fonts.width(ctx.lib, font::YELLOW, glyph);
            out.push(Draw::game_text(font::YELLOW, glyph, r.x0 + (r.width() - w) / 2, r.y0 + 6));
        }
        let r = layout.resources_draw;
        slot_piece(out, r, touch::ICON_PAD * 2 + touch::ICON_W, Some(TOUCH_FACE_SLOT), r.y1 + shadow);
        if !self.resources_open {
            return;
        }
        for slot in 0..touch::ROWS {
            let r = layout.row_rect(slot);
            let last = slot + 1 == touch::ROWS;
            slot_piece(out, r, touch::ROW_MID, Some(slot), r.y1 + if last { shadow } else { 0 });
            // The classic bar's values are in `FOOD`, `GOLD` swapped order (0x4ece10).
            let value = view.resources[match slot {
                1 => 3,
                3 => 1,
                s => s,
            }];
            let lot;
            let text: &[u8] = if value >= 10_000_000 {
                ctx.texts.get("RES_LOT_MONEY")
            } else {
                lot = value.to_string();
                lot.as_bytes()
            };
            // Right aligned where the bar puts it (`x0 + 119 + 110·slot` of the frame, y = 7).
            let w = ctx.fonts.width(ctx.lib, font::YELLOW, text);
            let right = r.x0 + touch::CAP_W + touch::ROW_MID - 5;
            out.push(Draw::game_text(font::YELLOW, text, right - w, r.y0 + 6));
        }
    }

    /// Hints of the resource values and icons: a box at the mouse (`DS+6 == −1`, 0x47d6b2),
    /// scaled about the mouse like the panel it hints at.
    fn draw_hint(&self, ctx: &Ctx, view: &View, palette: &Palette) -> Option<Section> {
        let m = self.mouse?;
        let mut hint: Option<&[u8]> = None;
        let mut scaled = true;
        for (i, id) in RES_HINTS.iter().enumerate().filter(|_| !self.touch()) {
            let mut r = res_hint_rect(self.screen, i);
            (r.x0, r.x1) = (r.x0 + self.top_dx(), r.x1 + self.top_dx());
            if r.contains(m) {
                hint = Some(ctx.texts.get(id));
                scaled = false;
            }
        }
        let commands = self.local(Group::Commands, view, m);
        for icon in view.icons.iter().filter(|i| Self::shown(i.slot)) {
            if Self::icon_rect(self.screen, icon.slot).contains(commands) && !icon.hint.is_empty() {
                hint = Some(&icon.hint);
                scaled = true;
            }
        }
        let army = self.local(Group::Army, view, m);
        for icon in Self::army_icons(view) {
            if Self::army_rect(icon.slot).contains(army) && !icon.hint.is_empty() {
                hint = Some(&icon.hint);
                scaled = true;
            }
        }
        if self.panel == Panel::Game
            && let Some(id) = panels::GamePanel::hint(self.panel_point(m))
        {
            hint = Some(ctx.texts.get(id));
            scaled = true;
        }
        if self.panel == Panel::Unit
            && let Some(h) = self.unit_panel.hint()
        {
            hint = Some(h);
            scaled = true;
        }
        let text = hint?;
        let scale = zoom::resolve(self.hud_scale, self.density, cfg!(target_os = "android"), ICON);
        let zoom = Zoom { anchor: m, scale: if scaled { scale } else { 1.0 } };
        // The box keeps within the screen as laid out about the mouse.
        let [sw, sh] = zoom.to_layout([self.screen[0] as f32, self.screen[1] as f32]);
        let mut out = Vec::new();
        tooltip(ctx, palette, text, m, [sw.floor() as i32, sh.floor() as i32], &mut out);
        Some(Section { layer: Layer::Hint, zoom, draws: out })
    }
}

/// `RESHINT0..5`: hints of the resource values (viewports of 0x5aab4b).
const RES_HINTS: [&str; 6] = ["RESHINT0", "RESHINT1", "RESHINT2", "RESHINT3", "RESHINT4", "RESHINT5"];

fn res_hint_rect(screen: [i32; 2], i: usize) -> Rect {
    Rect::new(screen[0] - 0x2b3 + RES_STEP * i as i32, 7, 105, 20)
}

/// Corner table 0x668580: TL, TR, BL, BR, left, right, top, bottom frames of `Border`.
pub const BOX_CORNERS: [i32; 8] = [0, 2, 6, 8, 12, 14, 9, 11];
/// Corner table 0x6685a0: the single type's box, open to the right (−1: not drawn).
const LEFT_BOX_CORNERS: [i32; 8] = [0, -1, 6, -1, 12, -1, 9, 11];
/// Corner table 0x6685c0: the commands' box joined to it, with the joint's corners and edge.
const RIGHT_BOX_CORNERS: [i32; 8] = [1, 2, 7, 8, 13, 14, 9, 11];

fn palette_rgba(palette: &Palette, index: u8) -> [u8; 4] {
    let [r, g, b] = palette.rgb(index);
    [r, g, b, 255]
}

/// `Hline`/`Vline` rectangle outline (`Rect`, 0x40121a) on the edges of `r`.
pub fn outline(out: &mut Vec<Draw>, r: Rect, rgba: [u8; 4]) {
    out.push(Draw::fill(rgba, Rect::new(r.x0, r.y0, r.width(), 1)));
    out.push(Draw::fill(rgba, Rect::new(r.x0, r.y1 - 1, r.width(), 1)));
    out.push(Draw::fill(rgba, Rect::new(r.x0, r.y0, 1, r.height())));
    out.push(Draw::fill(rgba, Rect::new(r.x1 - 1, r.y0, 1, r.height())));
}

/// A box drawn with the frames of `Border` (0x47f48a): edges tiled every 64 pixels and
/// centred on the box lines, corners on top.
pub fn border(ctx: &Ctx, out: &mut Vec<Draw>, x0: i32, y0: i32, x1: i32, y1: i32, corners: [i32; 8]) {
    let gp = gp::BORDER;
    let height = |f: i32| if f < 0 { 0 } else { ctx.lib.frame_size(gp, f as usize)[1] };
    let hh = height(corners[0]) / 2;
    let mut frame = |f: i32, x: i32, y: i32, clip: Rect| {
        if f >= 0 {
            out.push(Draw::game_frame(gp, f as usize, x, y, 0).clipped(clip));
        }
    };
    let n = ((x1 - x0 + 1 - 64) / 64).max(0);
    let top = Rect { x0: x0 + 32, y0: y0 - hh, x1: x1 - 32, y1: y1 + hh + 1 };
    for i in 0..=n {
        frame(corners[6], x0 + 64 * i + 32, y0 - hh, top);
        frame(corners[7], x0 + 64 * i + 32, y1 - 32, top);
    }
    let m = ((y1 - y0 + 1 - height(corners[0])) / 64).max(0);
    let side = Rect { x0: x0 - 32, y0: y0 + hh, x1: x1 + 33, y1: y1 - hh };
    for i in 0..=m {
        frame(corners[4], x0 - 32, y0 + 64 * i + hh, side);
        frame(corners[5], x1 - 32, y0 + 64 * i + hh, side);
    }
    let (mx, my) = ((x0 + x1) / 2, (y0 + y1) / 2);
    let all = Rect { x0: -10_000, y0: -10_000, x1: 10_000, y1: 10_000 };
    frame(corners[0], x0 - 32, y0 - hh, Rect { x1: mx, y1: my, ..all });
    frame(corners[1], x1 - 32, y0 - hh, Rect { x0: mx, y1: my, ..all });
    frame(corners[2], x0 - 32, y1 - hh, Rect { x1: mx, y0: my, ..all });
    frame(corners[3], x1 - 32, y1 - hh, Rect { x0: mx, y0: my, ..all });
}

/// The resource panel (0x4ecc50): frame 1 tiled every 256 pixels from the left, frame 0 at
/// `RealLx − 716`, the "M" button frame 2 left of it; values right aligned in `YellowFont`,
/// 110 pixels apart, with food and gold swapped into the icons' order.
///
/// `dx` moves the "M" plaque, frame 0 and the values ([`top_bar_dx`]); the filler then runs
/// the whole width, as frame 0 no longer reaches the edge.
fn draw_resources(ctx: &Ctx, view: &View, screen: [i32; 2], dx: i32, out: &mut Vec<Draw>) {
    let x0 = screen[0] - RES_PANEL;
    let tiles = if dx == 0 { x0 / 256 } else { screen[0] / 256 };
    let x0 = x0 + dx;
    for i in 0..=tiles {
        out.push(Draw::game_frame(gp::RESPANEL, 1, i * 256, 0, 0));
    }
    out.push(Draw::game_frame(gp::RESPANEL, 0, x0, 0, 0));
    let (bx, by) = menu_button(screen);
    out.push(Draw::game_frame(gp::RESPANEL, 2, bx + dx, by, 0));
    for (i, &value) in view.resources.iter().enumerate() {
        // `FOOD` and `GOLD` change places (0x4ece10).
        let slot = match i {
            1 => 3,
            3 => 1,
            s => s,
        } as i32;
        let lot;
        let text: &[u8] = if value >= 10_000_000 {
            ctx.texts.get("RES_LOT_MONEY")
        } else {
            lot = value.to_string();
            lot.as_bytes()
        };
        let w = ctx.fonts.width(ctx.lib, font::YELLOW, text);
        out.push(Draw::game_text(font::YELLOW, text, x0 + RES_TEXT_X + slot * RES_STEP - w, 7));
    }
}

/// The numbers of a single selected type (0x607f50): kills, then for a finished object
/// its life, up to two attacks in the top row, the shield below, up to six protections in
/// three columns, and the text lines.
fn draw_stats(ctx: &Ctx, stats: &Stats, palette: &Palette, screen: [i32; 2], out: &mut Vec<Draw>) {
    let x = ICON + ZONE1[0] + 6;
    // Everything hangs from `RealLy − PanelLy` (0x28ccc54).
    let panel_y = screen[1] - PANEL_LY;
    let top = ZONE1[1] + panel_y - 1;
    if stats.kills != 0 {
        let t = panels::format_int(ctx.texts.get("KILLUNI"), stats.kills);
        out.push(Draw::game_text(font::SMALL_WHITE, &t, 16, panel_y + ZONE1[1] - 20));
    }
    if !stats.ready {
        return;
    }
    let life = stats.life.to_string();
    let w = ctx.fonts.width(ctx.lib, font::SMALL_WHITE, life.as_bytes());
    out.push(Draw::game_text(font::SMALL_WHITE, life.as_bytes(), 12 + (ICON - w) / 2, panel_y + 46));
    for (i, line) in stats.lines.iter().enumerate() {
        out.push(Draw::game_text(FN10, line, 13, panel_y + 56 + 12 * i as i32));
    }
    for (n, (icon, damage)) in stats.attacks.iter().take(2).enumerate() {
        let tx = x + 19 + 55 * n as i32;
        out.push(Draw::game_frame(gp::SMICONS, *icon, tx - 18, top + 1, 0));
        out.push(Draw::game_text(font::SMALL_WHITE, damage, tx, top + 5));
    }
    let y = top + 21;
    out.push(Draw::game_frame(gp::SMICONS, SHIELD_ICON, x + 1, y + 1, 0));
    out.push(Draw::game_text(font::SMALL_WHITE, &stats.shield, x + 21, y + 5));
    let base = ZONE1[1] + panel_y + ICON + 10;
    for (i, &(icon, value)) in stats.protections.iter().take(6).enumerate() {
        let col = (i >> 1) as i32;
        let row = (i & 1) as i32;
        let x = ZONE1[0] + 4 + 55 * col;
        let y = base + row + 18 * row;
        out.push(Draw::game_frame(gp::SMICONS, icon, x + 3, y + 1, 0));
        out.push(Draw::game_text(font::SMALL_WHITE, value.to_string().as_bytes(), x + 22, y + 5));
    }
    // The reload bar (0x6086ea, `SHOWDELAY`): four green lines at `Zone1LX + Zone1X − 5`..
    // `− 2`, from the bottom of the zone up to the part of the reload done.
    if let Some((delay, max)) = stats.reload {
        let h = ZONE1_SIZE[1] + 25;
        let done = if max != 0 { h * delay / max } else { 0 };
        if done < h {
            let [r, g, b] = palette.rgb(nearest(palette, color::LIFE));
            let y0 = panel_y + ZONE1[1] + 1 + done;
            let y1 = panel_y + ZONE1[1] + h - 2;
            let x = ZONE1_SIZE[0] + ZONE1[0] - 5;
            out.push(Draw::fill([r, g, b, 255], Rect { x0: x, y0, x1: x + 4, y1: y1 + 1 }));
        }
    }
}

/// `Zone1LX`/`Zone1LY` (defaults 172, 100; 0x5aa6e6).
const ZONE1_SIZE: [i32; 2] = [172, 100];

/// `fn10` (`FontM12`), the info lines of 0x607f50.
const FN10: crate::ui::fonts::FontId = crate::ui::fonts::FontId("fn10");

/// The market's plaques and numbers (0x4a9c70, 0x4a96d9): `Border` 88 and 89 above the
/// grid with `ECOSELL` and `ECOBUY` in `SmallYellowFont`; the amount between −10 and +10,
/// the result between −1000 and +1000, centred in `WhiteFont`.
fn draw_market(ctx: &Ctx, (amount, result): (Option<i32>, Option<i32>), screen: [i32; 2], out: &mut Vec<Draw>) {
    let (x0, y0) = Hud::grid_origin(screen);
    out.push(Draw::game_frame(gp::BORDER, 88, x0 - 2, y0 - 40, 0));
    out.push(Draw::game_frame(gp::BORDER, 89, x0 + 293, y0 - 40, 0));
    let centred = |out: &mut Vec<Draw>, f, t: &[u8], x: i32, y: i32| {
        let w = ctx.fonts.width(ctx.lib, f, t);
        out.push(Draw::game_text(f, t, x - w / 2, y));
    };
    let small_yellow = panels::FONT_SMALL_YELLOW;
    centred(out, small_yellow, ctx.texts.get("ECOSELL"), x0 + 2 + ICON, y0 - 35);
    centred(out, small_yellow, ctx.texts.get("ECOBUY"), x0 + 16 + 8 * ICON, y0 - 35);
    let y = y0 + ICON + 16;
    if let Some(a) = amount {
        centred(out, font::WHITE, a.to_string().as_bytes(), x0 + 3 * (ICON + ADD_ICON), y);
    }
    if let Some(r) = result {
        centred(out, font::WHITE, r.to_string().as_bytes(), x0 + 6 * (ICON + ADD_ICON), y);
    }
}

/// The gray colour table of unaffordable icons (`0\gray.set`, loaded at 0x483e38).
pub const GRAY_SET: &str = "0\\GRAY.SET";

/// `#GENSHIELD` of `NRES.DAT` (0x66ad84).
const SHIELD_ICON: usize = 13;

/// A one pixel line (`DrawLine` 0x4012ff) as runs of fills.
pub fn line(out: &mut Vec<Draw>, rgba: [u8; 4], (x0, y0): (i32, i32), (x1, y1): (i32, i32)) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let n = dx.abs().max(dy.abs());
    // The pixel of step k, rounded to the nearest.
    let at = |k: i32| {
        if n == 0 {
            return (x0, y0);
        }
        let round = |d: i32| (2 * d * k + n * d.signum()) / (2 * n);
        (x0 + round(dx), y0 + round(dy))
    };
    let horizontal = dx.abs() >= dy.abs();
    // Consecutive pixels in the same row (column) become one fill.
    let mut start = at(0);
    let mut last = start;
    for k in 1..=n + 1 {
        let p = if k <= n { Some(at(k)) } else { None };
        let same = p.is_some_and(|p| if horizontal { p.1 == start.1 } else { p.0 == start.0 });
        if !same {
            let r = Rect {
                x0: start.0.min(last.0),
                y0: start.1.min(last.1),
                x1: start.0.max(last.0) + 1,
                y1: start.1.max(last.1) + 1,
            };
            out.push(Draw::fill(rgba, r).unclipped());
            if let Some(p) = p {
                start = p;
            }
        }
        if let Some(p) = p {
            last = p;
        }
    }
}

/// The count a button shows for an endless production order (`GetProduceCount` 0x49e060).
const ENDLESS_COUNT: i32 = 1200;

/// The segments of the infinity sign at `(x, y)` (0x494ad5: twelve calls of 0x443ea2),
/// 14 pixels wide and 6 high.
const INFINITY: [[i32; 4]; 12] = [
    [2, 0, 4, 0],
    [4, 0, 10, 6],
    [10, 6, 12, 6],
    [12, 6, 14, 4],
    [14, 4, 14, 2],
    [14, 2, 12, 0],
    [12, 0, 10, 0],
    [10, 0, 4, 6],
    [4, 6, 2, 6],
    [2, 6, 0, 4],
    [0, 4, 0, 2],
    [0, 2, 2, 0],
];

fn infinity(out: &mut Vec<Draw>, (x, y): (i32, i32), rgba: [u8; 4]) {
    for [x0, y0, x1, y1] in INFINITY {
        line(out, rgba, (x + x0, y + y0), (x + x1, y + y1));
    }
}

/// One icon (0x494c96): the picture (one pixel down-right while pressed), the outline in
/// the owner's colour, the yellow frame when highlighted, the life bar, the number at the
/// bottom right, the count.
fn draw_icon(ctx: &Ctx, icon: &Icon, x: i32, y: i32, pressed: bool, palette: &Palette, out: &mut Vec<Draw>) {
    let (x, y) = if pressed { (x + 1, y + 1) } else { (x, y) };
    let mut picture = if icon.gray {
        Draw::game_frame_remap(gp::ICONS, icon.frame, x, y, 0, GRAY_SET, 0)
    } else {
        Draw::game_frame(gp::ICONS, icon.frame, x, y, 0)
    };
    if icon.disabled {
        picture = picture.with_alpha(0.5);
    }
    out.push(picture);
    let rgba = |i: u8| palette_rgba(palette, i);
    if let Some(c) = icon.outline {
        outline(out, Rect::new(x, y, ICON + 1, ICON + 1), rgba(c));
        if icon.life.is_some_and(|(l, _)| l > 0) {
            out.push(Draw::fill(rgba(c), Rect { x0: x + 1, y0: y + ICON - 3, x1: x + ICON, y1: y + ICON - 2 }));
        }
    }
    if icon.selected {
        let c = nearest(palette, color::ICON_SELECTED);
        outline(out, Rect::new(x - 1, y - 1, ICON + 3, ICON + 3), rgba(c));
    }
    if let Some((life, max)) = icon.life
        && max > 0
        && life > 0
    {
        let w = (ICON - 2) * life / max;
        for dy in [2, 1] {
            out.push(Draw::fill(
                rgba(color::WHITE),
                Rect { x0: x + 1, y0: y + ICON - dy, x1: x + w + 2, y1: y + ICON - dy + 1 },
            ));
            if life < max {
                out.push(Draw::fill(
                    rgba(color::BLACK),
                    Rect { x0: x + w + 2, y0: y + ICON - dy, x1: x + ICON, y1: y + ICON - dy + 1 },
                ));
            }
        }
    }
    // Flag 0x100 (0x4951a5): right aligned 4 pixels inside the right edge, on the bottom
    // edge by the height of 'W', `BlackFont` at ±1 under `WhiteFont`.
    if let Some(n) = icon.number {
        let text = n.to_string();
        let w = ctx.fonts.width(ctx.lib, font::WHITE, text.as_bytes());
        let h = ctx.fonts.glyph_height(ctx.lib, font::WHITE, b'W');
        let (tx, ty) = (x + ICON - 4 - w, y + ICON - h);
        out.push(Draw::game_text(font::BLACK, text.as_bytes(), tx + 1, ty + 1));
        out.push(Draw::game_text(font::BLACK, text.as_bytes(), tx - 1, ty - 1));
        out.push(Draw::game_text(font::WHITE, text.as_bytes(), tx, ty));
    }
    if let Some(n) = icon.count.filter(|&n| n >= ENDLESS_COUNT) {
        // Flag 8 with count >= 1200 (0x495294): the infinity sign as line segments (0x494ad5),
        // black shifted by (2, 2) under colour 0xae, and the number of endless orders if more
        // than one, right aligned at the top (BlackFont at (+1, +2) and (-1, 0), then
        // YellowFont; the original's second font 0x90d69c is not named in the data).
        infinity(out, (x + 3 + 2, y + 3 + 2), rgba(0));
        infinity(out, (x + 3 + 1, y + 3 + 1), rgba(0xae));
        if n / ENDLESS_COUNT > 1 {
            let text = (n / ENDLESS_COUNT).to_string();
            let w = ctx.fonts.width(ctx.lib, font::YELLOW, text.as_bytes());
            let tx = x + ICON - 2 - w;
            out.push(Draw::game_text(font::BLACK, text.as_bytes(), tx + 1, y + 2));
            out.push(Draw::game_text(font::BLACK, text.as_bytes(), tx - 1, y));
            out.push(Draw::game_text(font::YELLOW, text.as_bytes(), tx, y + 1));
        }
    } else if let Some(n) = icon.count {
        let text = n.to_string();
        out.push(Draw::game_text(font::BLACK, text.as_bytes(), x + 2, y + 2));
        out.push(Draw::game_text(font::BLACK, text.as_bytes(), x, y));
        out.push(Draw::game_text(font::WHITE, text.as_bytes(), x + 1, y + 1));
    }
}

/// A hint box at the mouse (0x47d6b2 with `DS+6 == −1`): `SmallBlackFont1` on colour 185
/// with the `Border` corners, at (mouse + 20, mouse + 32), kept on the screen.
fn tooltip(ctx: &Ctx, palette: &Palette, text: &[u8], m: [f32; 2], screen: [i32; 2], out: &mut Vec<Draw>) {
    const WRAP: i32 = 100;
    let f = font::SMALL_BLACK1;
    let line_h = ctx.fonts.height(ctx.lib, f);
    // Lines break at '/' and '\', and at the first space past the wrap width.
    let mut lines: Vec<Vec<u8>> = vec![Vec::new()];
    let mut width = 0;
    for &c in text {
        let line = lines.last_mut().unwrap();
        if c == b'/' || c == b'\\' || (c == b' ' && width > WRAP) {
            lines.push(Vec::new());
            width = 0;
            continue;
        }
        line.push(c);
        width += ctx.fonts.char_width(ctx.lib, f, c);
    }
    let max_w = lines.iter().map(|l| ctx.fonts.width(ctx.lib, f, l)).max().unwrap_or(0).max(80);
    let h = lines.len() as i32 * line_h;
    let mut x = m[0] as i32 + 20;
    let mut y = m[1] as i32 + 32;
    x = x.max(10);
    if x + max_w + 20 > screen[0] {
        x = screen[0] - max_w - 20;
    }
    if y + h + 16 > screen[1] {
        y = screen[1] - h - 16;
    }
    let (x1, y1) = (x + max_w + 14, y + h + 12);
    out.push(Draw::fill(palette_rgba(palette, color::HINT_BACK), Rect { x0: x, y0: y, x1: x1 + 1, y1: y1 + 1 }));
    let corners = [58, 59, 60, 61, 62, 63, 64, 65];
    let all = Rect { x0: -10_000, y0: -10_000, x1: 10_000, y1: 10_000 };
    for (i, &(cx, cy)) in [(x, y), (x1, y), (x, y1), (x1, y1)].iter().enumerate() {
        out.push(Draw::game_frame(gp::BORDER, corners[i], cx - 32, cy - 32, 0).clipped(all));
    }
    for i in 0..=((x1 - x) >> 6) {
        let clip = Rect { x0: x, x1: x1 + 1, ..all };
        out.push(Draw::game_frame(gp::BORDER, 64, x + 64 * i, y - 32, 0).clipped(clip));
        out.push(Draw::game_frame(gp::BORDER, 65, x + 64 * i, y1 - 32, 0).clipped(clip));
    }
    for i in 0..=((y1 - y) >> 6) {
        let clip = Rect { y0: y, y1: y1 + 1, ..all };
        out.push(Draw::game_frame(gp::BORDER, 62, x - 32, y + 64 * i, 0).clipped(clip));
        out.push(Draw::game_frame(gp::BORDER, 63, x1 - 32, y + 64 * i, 0).clipped(clip));
    }
    out.push(Draw::game_frame(gp::BORDER, 66, (x + x1) / 2 - 32, y - 32, 0));
    out.push(Draw::game_frame(gp::BORDER, 67, (x + x1) / 2 - 32, y1 - 32, 0));
    for (i, line) in lines.iter().enumerate() {
        out.push(Draw::game_text(f, line, x + 7, y + 5 + line_h * i as i32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_layout_at_1024x768() {
        assert_eq!(Hud::icon_rect(SCREEN, 0), Rect::new(182, 630, 40, 40));
        assert_eq!(Hud::icon_rect(SCREEN, 27), Rect::new(182 + 3 * 42, 630 + 2 * 42, 40, 40));
        let icon = |slot| Icon { slot, ..Icon::default() };
        assert_eq!(Hud::icon_box(SCREEN, &[]), None);
        assert_eq!(Hud::icon_box(SCREEN, &[icon(0), icon(8 + 24)]), Some(Rect { x0: 180, y0: 630, x1: 560, y1: 755 }));
        assert_eq!(menu_button(SCREEN), (243, 0));
        // The army icons: 5 columns from (6, 31) under the resource bar, 42 apart.
        assert_eq!(Hud::army_rect(0), Rect::new(6, 31, 40, 40));
        assert_eq!(Hud::army_rect(5), Rect::new(6, 73, 40, 40));
        assert_eq!(Hud::army_rect(7), Rect::new(90, 73, 40, 40));
        assert_eq!(res_hint_rect(SCREEN, 0), Rect::new(333, 7, 105, 20));
        let hud = Hud::default();
        assert_eq!((hud.panel_dx(), hud.panel_point([800.0, 100.0])), (0, [800.0, 100.0]));
    }

    /// A single selected type (0x5aa136): its icon at (`Zone1X + 6`, 630) in a box open to the
    /// right, the commands in a box joined to it; the reload bar (x 173..176, drawn after the
    /// boxes) lies in the first box, left of the joint at `PropertyX − 2`.
    #[test]
    fn single_type_gets_its_own_box() {
        let icon = |slot| Icon { slot, ..Icon::default() };
        assert_eq!(Hud::icon_rect(SCREEN, ZONE1_SLOT), Rect::new(12, 630, 40, 40));
        assert!(Hud::shown(ZONE1_SLOT) && Hud::shown(35) && !Hud::shown(36));
        assert_eq!(Hud::columns(&[icon(ZONE1_SLOT)]), 0);
        let left = Rect { x0: 12, y0: 630, x1: 180, y1: 755 };
        assert_eq!(Hud::icon_boxes(SCREEN, &[icon(ZONE1_SLOT)]), vec![(left, BOX_CORNERS)]);
        assert_eq!(
            Hud::icon_boxes(SCREEN, &[icon(ZONE1_SLOT), icon(1)]),
            vec![(left, LEFT_BOX_CORNERS), (Rect { x0: 180, y0: 630, x1: 266, y1: 755 }, RIGHT_BOX_CORNERS)]
        );
        // Several types: the commands' box alone, the types in the grid.
        assert_eq!(
            Hud::icon_boxes(SCREEN, &[icon(0), icon(1)]),
            vec![(Rect { x0: 180, y0: 630, x1: 266, y1: 755 }, BOX_CORNERS)]
        );
        let bar_x = ZONE1_SIZE[0] + ZONE1[0] - 5;
        assert_eq!(bar_x, 173);
        assert!(bar_x + 3 < left.x1);
    }

    /// `center_top_bar`: the plaque, values, hint rects and click region move so that the
    /// group (781 wide) is centred; off, they stay at the original's right anchor.
    #[test]
    fn top_bar_centering_offsets() {
        for (w, dx, left) in [(1024, -121, 122), (1920, -569, 570), (2560, -889, 890)] {
            let screen = [w, 1080];
            assert_eq!(top_bar_dx(screen, false), 0);
            assert_eq!(top_bar_dx(screen, true), dx);
            let mut hud = Hud::default();
            hud.set_screen(screen);
            assert_eq!(hud.menu_button(), menu_button(screen));
            hud.center_top_bar = true;
            assert_eq!(hud.menu_button(), (left, 0));
            // The group spans left..left + 781: the margins differ by at most a pixel.
            assert!((left - (w - left - TOP_GROUP)).abs() <= 1);
            assert_eq!(hud.top_dx(), dx);
        }
    }

    /// `top_bar=touch`: Menu opens the game menu, Resources toggles the panel (a second tap
    /// closes it), the open panel takes its clicks, and the old bar's area is map.
    #[test]
    fn touch_bar_presses() {
        let mut hud = Hud { top_bar: TopBar::Touch, ..Hud::default() };
        hud.set_screen([1690, 768]);
        hud.set_density(3.5 / 1.875);
        let l = hud.touch_layout();
        let centre = |r: Rect| [((r.x0 + r.x1) / 2) as f32, ((r.y0 + r.y1) / 2) as f32];
        assert!(!hud.resources_open());
        assert!(matches!(hud.touch_press(centre(l.menu), false), Some(HudEvent::Used)));
        assert!(hud.dropdown, "Menu opens the M drop-down");
        // The press that follows shuts it first; a second tap on Menu leaves it shut.
        hud.dropdown = false;
        assert!(matches!(hud.touch_press(centre(l.menu), true), Some(HudEvent::Used)));
        assert!(!hud.dropdown);
        assert!(hud.touch_press(centre(l.panel), false).is_none(), "the closed panel is map");
        assert!(matches!(hud.touch_press(centre(l.resources), false), Some(HudEvent::Used)));
        assert!(hud.resources_open());
        assert!(matches!(hud.touch_press(centre(l.panel), false), Some(HudEvent::Used)));
        assert!(hud.resources_open(), "a tap on the panel keeps it open");
        assert!(matches!(hud.touch_press(centre(l.resources), false), Some(HudEvent::Used)));
        assert!(!hud.resources_open());
        assert!(matches!(hud.touch_press(centre(l.pause), false), Some(HudEvent::TogglePause)));
        assert!(matches!(hud.touch_press(centre(l.escape), false), Some(HudEvent::Escape)));
        // Where the classic bar and its "M" plaque were: map.
        for p in [[1000.0, 20.0], [600.0, 30.0], [hud.screen()[0] as f32 - 700.0, 10.0]] {
            assert!(hud.touch_press(p, false).is_none(), "{p:?}");
        }
        assert!(hud.touch_layout().covers(centre(l.menu), false) && !hud.touch_layout().covers([1000.0, 20.0], true));
    }

    /// The bar along the top edge, the panels at the right edge, the icons and the minimap
    /// at the bottom (`RealLx`, `RealLy` of a wide and a tall window).
    #[test]
    fn layout_follows_the_screen() {
        let icon = |slot| Icon { slot, ..Icon::default() };
        let wide = [1366, 768];
        assert_eq!(menu_button(wide), (1366 - 781, 0));
        assert_eq!(res_hint_rect(wide, 0), Rect::new(1366 - 691, 7, 105, 20));
        assert_eq!(Hud::icon_rect(wide, 0), Rect::new(182, 630, 40, 40));
        let mut hud = Hud::default();
        hud.set_screen(wide);
        assert_eq!((hud.screen(), hud.panel_dx()), (wide, 342));
        assert_eq!(hud.panel_point([1366.0 - 100.0, 50.0]), [924.0, 50.0]);
        assert_eq!(hud.placement(&View::default()).rect, Rect::new(1366 - 257, 512, 240, 240));

        let tall = [1024, 1055];
        assert_eq!(menu_button(tall), (243, 0));
        assert_eq!(Hud::icon_rect(tall, 0), Rect::new(182, 1055 - 138, 40, 40));
        assert_eq!(Hud::icon_box(tall, &[icon(0)]).map(|r| (r.y0, r.y1)), Some((1055 - 138, 1055 - 13)));
        hud.set_screen(tall);
        assert_eq!(hud.panel_dx(), 0);
        assert_eq!(hud.placement(&View::default()).rect, Rect::new(767, 1055 - 256, 240, 240));
        // Never smaller than 1024 × 768.
        hud.set_screen([800, 600]);
        assert_eq!(hud.screen(), SCREEN);
    }

    fn centre(r: [f32; 4]) -> [f32; 2] {
        [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
    }

    fn icon(slot: usize) -> Icon {
        Icon { slot, action: Some(HudCommand::SelectType(slot)), ..Icon::default() }
    }

    fn phone() -> Hud {
        let mut hud = Hud::default();
        hud.set_screen([1690, 768]);
        hud.set_density(3.5 / 1.875);
        hud
    }

    /// `hud_scale`: at scale 1 nothing moves; scaled, the commands' frame is pulled to the
    /// screen's left edge (anchor `L·s / (s − 1)`) and grows up from the bottom, and a press
    /// is mapped back through the same transform.
    #[test]
    fn presses_follow_the_scaled_panel() {
        let ui = crate::ui::Ui::new(&[]);
        let ctx = ui.ctx();
        // A mixed selection: the types in the grid, no single type's box.
        let mut view = View { icons: vec![icon(0), icon(13)], ..View::default() };
        let mut hud = phone();
        assert_eq!(hud.icon_on_canvas(0, &view), [182.0, 630.0, 40.0, 40.0]);
        assert_eq!(hud.press(&ctx, &view, [202.0, 650.0]), HudEvent::Command(HudCommand::SelectType(0)));

        hud.hud_scale = Some(2.0);
        let z = hud.zoom(Group::Commands, &view);
        assert_eq!((z.scale, z.anchor), (2.0, [360.0, 768.0]));
        // The frame (layout x 180) is at the screen's edge, the first icon 4 pixels in.
        assert_eq!(hud.icon_on_canvas(0, &view), [4.0, 492.0, 80.0, 80.0]);
        assert_eq!(hud.icon_on_canvas(13, &view), [88.0, 576.0, 80.0, 80.0]);
        for slot in [0, 13] {
            let hit = hud.press(&ctx, &view, centre(hud.icon_on_canvas(slot, &view)));
            assert_eq!(hit, HudEvent::Command(HudCommand::SelectType(slot)));
        }
        // The old place is map now.
        assert_eq!(hud.press(&ctx, &view, [202.0, 650.0]), HudEvent::None);
        assert!(hud.covers(&ctx, &view, centre(hud.icon_on_canvas(0, &view))));
        assert!(!hud.covers(&ctx, &view, [202.0, 650.0]));
        // The frame's left edge itself is covered.
        assert!(hud.covers(&ctx, &view, [1.0, 700.0]));
        view.icons[0].right = Some(HudCommand::SelectType(99));
        let right = hud.right_press(&ctx, &view, centre(hud.icon_on_canvas(0, &view)));
        assert_eq!(right, HudEvent::Command(HudCommand::SelectType(99)));

        // A single unit: its portrait box (layout x 12) goes to the edge instead.
        let single = View { icons: vec![icon(ZONE1_SLOT), icon(0)], ..View::default() };
        let z = hud.zoom(Group::Commands, &single);
        assert_eq!(z.anchor, [24.0, 768.0]);
        assert_eq!(hud.icon_on_canvas(ZONE1_SLOT, &single), [0.0, 492.0, 80.0, 80.0]);
        let hit = hud.press(&ctx, &single, centre(hud.icon_on_canvas(ZONE1_SLOT, &single)));
        assert_eq!(hit, HudEvent::Command(HudCommand::SelectType(ZONE1_SLOT)));
        let hit = hud.press(&ctx, &single, centre(hud.icon_on_canvas(0, &single)));
        assert_eq!(hit, HudEvent::Command(HudCommand::SelectType(0)));

        // The minimap grows up and left from the bottom-right corner, capped at half the height.
        hud.set_minimap(Minimap { pixels: vec![0; (minimap::SIDE * minimap::SIDE) as usize] });
        let z = hud.zoom(Group::Minimap, &view);
        assert_eq!(z.anchor, [1690.0, 768.0]);
        assert!((z.scale - 384.0 / 257.0).abs() < 1e-4, "{}", z.scale);
        let [x, y, w, h] = z.rect(hud.placement(&view).rect);
        match hud.press(&ctx, &view, [x + w / 2.0, y + h / 2.0]) {
            HudEvent::Camera([px, py]) => assert!((px - 120.0).abs() < 1.0 && (py - 120.0).abs() < 1.0, "{px} {py}"),
            e => panic!("{e:?}"),
        }
    }

    /// The phone default (a 3168 x 1440 window at scale factor 3.5, canvas scale 1.875): the
    /// command icons are 6 mm, the panel sits in the lower-left corner for a mixed selection
    /// and for a single unit, and the grid fits the width; the desktop default is 1.
    #[test]
    fn the_phone_default_sits_in_the_corner() {
        let mut hud = phone();
        let density = 3.5 / 1.875;
        hud.hud_scale = Some(zoom::resolve(None, density, true, ICON));
        let mixed = View { icons: vec![icon(0), icon(1)], ..View::default() };
        let single = View { icons: vec![icon(ZONE1_SLOT)], ..View::default() };
        for view in [&mixed, &single] {
            let z = hud.zoom(Group::Commands, view);
            assert!((z.scale - 1.764).abs() < 0.01, "{}", z.scale);
            let [x, y, w, _] = hud.icon_on_canvas(0, view);
            let mm = w / density / (160.0 / 25.4);
            assert!((mm - 6.0).abs() < 0.1, "{mm} mm");
            assert!(x > 0.0 && x < 400.0 && y > 400.0, "{x} {y}");
            let [x, _, w, _] = hud.icon_on_canvas(GRID_COLUMNS - 1, view);
            assert!(x + w <= 1690.0, "{}", x + w);
        }
        // The leftmost drawn part is at the edge: the frame, or the single type's box.
        let [x, ..] = hud.zoom(Group::Commands, &mixed).rect(Rect::new(180, 700, 1, 1));
        assert!(x.abs() < 1e-3, "{x}");
        let [x, ..] = hud.zoom(Group::Commands, &single).rect(Rect::new(12, 700, 1, 1));
        assert!(x.abs() < 1e-3, "{x}");
        // The unset desktop default leaves the layout alone.
        hud.hud_scale = None;
        assert_eq!(hud.zoom(Group::Commands, &mixed), Zoom { anchor: [0.0, 768.0], scale: 1.0 });
        assert_eq!(hud.icon_on_canvas(0, &mixed), [182.0, 630.0, 40.0, 40.0]);
    }

    /// The touch skin's clicks give the event a press at the icon (or the minimap point) gives.
    #[test]
    fn an_icon_click_without_a_pixel_is_the_press_event() {
        let ui = crate::ui::Ui::new(&[]);
        let ctx = ui.ctx();
        let mut hud = phone();
        let mut disabled = icon(3);
        disabled.disabled = true;
        let mut silent = icon(4);
        silent.action = None;
        let mut view = View { icons: vec![icon(0), icon(2), disabled, silent], army: vec![icon(1)], ..View::default() };
        view.icons[1].right = Some(HudCommand::SelectType(99));
        for (id, slot) in [("hud/0", 0), ("hud/2", 2)] {
            let at = centre(hud.icon_on_canvas(slot, &view));
            assert_eq!(Hud::icon_click(&view, id, false), Ok(hud.press(&ctx, &view, at)), "{id}");
        }
        let at = centre(hud.icon_on_canvas(2, &view));
        assert_eq!(Hud::icon_click(&view, "hud/2", true), Ok(hud.right_press(&ctx, &view, at)));
        let at = centre(hud.army_on_canvas(1));
        assert_eq!(Hud::icon_click(&view, "army/1", false), Ok(hud.press(&ctx, &view, at)));
        // What a press only takes is refused, as is an unknown or malformed icon.
        assert_eq!(Hud::icon_click(&view, "hud/3", false), Err(Reject::Disabled));
        assert_eq!(Hud::icon_click(&view, "hud/4", false), Err(Reject::Kind("an icon with a command")));
        assert_eq!(Hud::icon_click(&view, "hud/0", true), Err(Reject::Kind("an icon with a command")));
        assert_eq!(Hud::icon_click(&view, "hud/9", false), Err(Reject::NoWidget));
        assert_eq!(Hud::icon_click(&view, "army/1", true), Err(Reject::NoWidget));
        assert_eq!(Hud::icon_click(&view, "hud/x", false), Err(Reject::NoWidget));
        assert_eq!(Hud::icon_click(&view, "editor/1", false), Err(Reject::NoWidget));

        // The minimap: a point of it is the pixel a press there gives.
        assert_eq!(hud.minimap_pixel(&view, [0.5, 0.5]), Err(Reject::Hidden));
        hud.set_minimap(Minimap { pixels: vec![0; (minimap::SIDE * minimap::SIDE) as usize] });
        let z = hud.zoom(Group::Minimap, &view);
        let [x, y, w, h] = z.rect(hud.placement(&view).rect);
        for uv in [[0.5, 0.5], [0.25, 0.75]] {
            let HudEvent::Camera(px) = hud.press(&ctx, &view, [x + w * uv[0], y + h * uv[1]]) else { panic!() };
            let got = hud.minimap_pixel(&view, uv).unwrap();
            assert!((px[0] - got[0]).abs() < 0.5 && (px[1] - got[1]).abs() < 0.5, "{uv:?}: {px:?} {got:?}");
        }
        assert_eq!(hud.minimap_pixel(&view, [1.5, 0.5]), Err(Reject::Range));
    }

    #[test]
    fn a_claimed_top_bar_splits_its_text_from_its_frames() {
        let fill = Draw::fill([1, 2, 3, 255], Rect::new(0, 0, 4, 4));
        let text = Draw::game_text(font::YELLOW, b"7", 1, 2);
        let section =
            Section { layer: Layer::TopBar, zoom: Zoom::NONE, draws: vec![fill.clone(), text.clone(), fill.clone()] };
        let groups = claimed(section);
        let names: Vec<_> = groups.iter().map(|g| (g.name, g.layer, g.draws.len())).collect();
        assert_eq!(names, [("top_bar", 7, 2), ("top_bar_text", 8, 1)]);
        let map = claimed(Section { layer: Layer::Minimap, zoom: Zoom::NONE, draws: vec![fill.clone()] });
        assert_eq!(map.iter().map(|g| (g.name, g.layer)).collect::<Vec<_>>(), [("minimap", 0)]);
        assert!(claimed(Section { layer: Layer::GamePanel, zoom: Zoom::NONE, draws: Vec::new() }).is_empty());
        let panel = claimed(Section { layer: Layer::GamePanel, zoom: Zoom::NONE, draws: vec![fill] });
        assert_eq!(panel.iter().map(|g| (g.name, g.layer)).collect::<Vec<_>>(), [("game_panel", 6)]);
        assert!(Claim::ALL.claims(Layer::Hint) && !Claim::default().claims(Layer::TopBar));
        assert!(Claim::ALL.claims(Layer::Commands) && Claim::ALL.claims(Layer::GamePanel));
        assert!(!Claim::default().claims(Layer::Army) && !Claim::default().claims(Layer::GamePanel));
    }
}
