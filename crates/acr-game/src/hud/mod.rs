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

use std::rc::Rc;

use acr_formats::Palette;

use crate::ui::Ctx;
use crate::ui::draw::{Draw, GAME_PALETTE, Rect};
use crate::ui::fonts::FontId;
use data::HudData;
use minimap::{Minimap, Placement};

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

/// Width of the resource panel (`Interf2\ResPanel` frame 0, 0x4ecc50).
const RES_PANEL: i32 = 716;
/// Distance of the resource values (right aligned) and their spacing.
const RES_TEXT_X: i32 = 0x77;
const RES_STEP: i32 = 110;

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
    /// Queue a unit of this type in the selected building (0x6088c0; Shift: five).
    Produce(usize),
    /// Cancel a queued unit of this type (right click, 0x608810; Shift: five).
    Unproduce(usize),
    /// Start placing a building of this type (`BuildMode`, 0x608a0d).
    Place(usize),
    /// Show page `n` of the building's icons (`PAGE1..3`, 0x608c60).
    Page(u8),
    /// Let the units out of the selected building.
    Leave,
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
    pub stats: Option<Stats>,
    /// Unit dots of the minimap.
    pub dots: Vec<minimap::Dot>,
    /// Camera position and view size in minimap pixels.
    pub camera: [f32; 2],
    pub view: [f32; 2],
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

pub struct Hud {
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
}

impl Default for Hud {
    fn default() -> Self {
        Self {
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
            panel: Panel::None,
            locks: false,
            transparent: false,
            zoom: false,
            unit_panel: panels::UnitPanel::new(),
            game_panel: panels::GamePanel::default(),
        }
    }
}

/// What the minimap image was last composed from: placement, dots, camera, fog.
type MinimapState = (Placement, Vec<minimap::Dot>, Option<Rect>, Option<Rc<Vec<bool>>>);

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

    /// How far the side panels move right: they are laid out for `RealLx` = 1024 (see
    /// [`panels`]) and anchored to the right edge.
    fn panel_dx(&self) -> i32 {
        self.screen[0] - SCREEN[0]
    }

    /// A canvas point in the side panels' coordinates.
    fn panel_point(&self, p: [f32; 2]) -> [f32; 2] {
        [p[0] - self.panel_dx() as f32, p[1]]
    }

    fn placement(&self, view: &View) -> Placement {
        Placement::new(self.large_minimap, self.screen, view.camera, view.view)
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
        let state = (placement, view.dots.clone(), Some(camera), view.fog.clone());
        if !self.minimap_dirty && self.last_minimap.as_ref() == Some(&state) {
            return None;
        }
        let fog = view.fog.as_deref().map(Vec::as_slice);
        let image = minimap::compose(map, &placement, &view.dots, fog, Some(camera), &palette);
        self.last_minimap = Some(state);
        self.minimap_dirty = false;
        Some(image)
    }

    /// The command grid's top-left corner: `PropertyX`, `RealLy − PanelLy + PropertyY`.
    fn grid_origin(screen: [i32; 2]) -> (i32, i32) {
        (PROPERTY[0], screen[1] - PANEL_LY + PROPERTY[1])
    }

    fn icon_rect(screen: [i32; 2], slot: usize) -> Rect {
        let (x, y) = Self::grid_origin(screen);
        let step = ICON + ADD_ICON;
        Rect::new(x + (slot % GRID_COLUMNS) as i32 * step, y + (slot / GRID_COLUMNS) as i32 * step, ICON, ICON)
    }

    /// Columns used by the visible icons (`GetMaxX`, 0x495b86).
    fn columns(icons: &[Icon]) -> i32 {
        icons.iter().filter(|i| i.slot < GRID_COLUMNS * GRID_ROWS).map(|i| i.slot % GRID_COLUMNS + 1).max().unwrap_or(0)
            as i32
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
        let (bx, _) = menu_button(self.screen);
        let x = bx + 33 - width / 2;
        let rows =
            entries.into_iter().enumerate().map(|(i, e)| (e, Rect::new(x, 60 + 24 * i as i32, width, 24))).collect();
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
        let (bx, by) = menu_button(self.screen);
        if Rect::new(bx, by, 67, 42).contains(p) || (p[1] < 42.0 && p[0] >= (self.screen[0] - RES_PANEL) as f32) {
            return true;
        }
        if Self::icon_box(self.screen, &view.icons).is_some_and(|r| r.contains(p)) {
            return true;
        }
        self.minimap.is_some() && self.placement(view).rect.contains(p)
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
        let (bx, _) = menu_button(self.screen);
        let [x, y] = p;
        if x > (bx + 23) as f32 && x < (bx + 44) as f32 && y > 8.0 && y < 26.0 {
            self.dropdown = true;
            return HudEvent::Used;
        }
        for icon in &view.icons {
            if Self::icon_rect(self.screen, icon.slot).contains(p) && icon.slot < GRID_COLUMNS * GRID_ROWS {
                self.pressed = true;
                return match (&icon.action, icon.disabled) {
                    (Some(a), false) => HudEvent::Command(a.clone()),
                    _ => HudEvent::Used,
                };
            }
        }
        if self.minimap.is_some()
            && let Some(px) = self.placement(view).pixel_at(p)
        {
            self.pressed = true;
            return HudEvent::Camera(px);
        }
        if self.covers(ctx, view, p) { HudEvent::Used } else { HudEvent::None }
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
            && let Some(px) = self.placement(view).pixel_at(p)
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
        if let Some(icon) = view
            .icons
            .iter()
            .find(|i| i.slot < GRID_COLUMNS * GRID_ROWS && Self::icon_rect(self.screen, i.slot).contains(p))
        {
            return match (&icon.right, icon.disabled) {
                (Some(a), false) => HudEvent::Command(a.clone()),
                _ => HudEvent::Used,
            };
        }
        if self.minimap.is_some()
            && let Some(px) = self.placement(view).pixel_at(p)
        {
            return HudEvent::Order(px);
        }
        if self.covers(ctx, view, p) { HudEvent::Used } else { HudEvent::None }
    }

    pub fn toggle_minimap(&mut self) {
        self.large_minimap = !self.large_minimap;
        self.minimap_dirty = true;
    }

    /// The interface in canvas coordinates, in drawing order.
    pub fn draw(&mut self, ctx: &Ctx, view: &View, out: &mut Vec<Draw>) {
        let Some(palette) = ctx.lib.palette(GAME_PALETTE) else { return };
        let data = self.data(ctx);
        // The minimap (0x4c0a40) comes first, then the unit stats (0x606710), the icons
        // (0x5aa206) and the resource panel (0x4ecc50).
        if self.minimap.is_some() {
            self.draw_minimap(ctx, view, &palette, out);
        }
        let screen = self.screen;
        if let (Some(stats), Some(_)) = (&view.stats, &data) {
            draw_stats(ctx, stats, &palette, screen, out);
        }
        if let Some(r) = Self::icon_box(screen, &view.icons) {
            border(ctx, out, r.x0, r.y0, r.x1, r.y1, BOX_CORNERS);
        }
        let mouse = self.mouse;
        for icon in view.icons.iter().filter(|i| i.slot < GRID_COLUMNS * GRID_ROWS) {
            let r = Self::icon_rect(screen, icon.slot);
            let pressed = self.pressed && mouse.is_some_and(|m| r.contains(m));
            draw_icon(icon, r.x0, r.y0, pressed, &palette, out);
        }
        let mut panel = Vec::new();
        match (self.panel, &view.unit_info) {
            (Panel::Unit, Some(info)) => self.unit_panel.draw(ctx, info, view.morale.as_ref(), &palette, &mut panel),
            (Panel::Game, _) => {
                self.game_panel.draw_frame(ctx, &mut panel);
                panel.extend(view.game_page.iter().cloned());
                self.game_panel.draw_widgets(ctx, &mut panel);
            }
            _ => {}
        }
        let dx = self.panel_dx();
        out.extend(panel.into_iter().map(|d| d.translated(dx, 0)));
        if let Some(market) = view.market {
            draw_market(ctx, market, screen, out);
        }
        draw_resources(ctx, view, screen, out);
        if self.dropdown {
            self.draw_dropdown(ctx, view, out);
        }
        self.draw_hint(ctx, view, &palette, out);
    }

    fn draw_minimap(&self, ctx: &Ctx, view: &View, palette: &Palette, out: &mut Vec<Draw>) {
        let r = self.placement(view).rect;
        out.push(Draw::image(MINIMAP_IMAGE, r));
        // Two lines in the player's colour around it, then the "Border" frame (0x4c155c).
        let inner = palette_rgba(palette, color::PLAYER + 4 * (view.player % 8));
        let outer = palette_rgba(palette, color::PLAYER + 4 * (view.player % 8) + 1);
        outline(out, Rect { x0: r.x0 - 1, y0: r.y0 - 1, x1: r.x1 + 1, y1: r.y1 + 1 }, inner);
        outline(out, Rect { x0: r.x0 - 2, y0: r.y0 - 2, x1: r.x1 + 2, y1: r.y1 + 2 }, outer);
        border(ctx, out, r.x0 - 2, r.y0 - 2, r.x1 + 1, r.y1 + 1, BOX_CORNERS);
    }

    fn draw_dropdown(&self, ctx: &Ctx, view: &View, out: &mut Vec<Draw>) {
        let (_, rows) = self.dropdown_rows(ctx, view);
        let (Some((_, first)), Some((_, last))) = (rows.first(), rows.last()) else { return };
        border(ctx, out, first.x0 - 2, first.y0 - 2, last.x1 + 1, last.y1 + 1, BOX_CORNERS);
        for (i, (entry, r)) in rows.iter().enumerate() {
            let hover = self.mouse.is_some_and(|m| r.contains(m));
            // `addGP_TextButtonLimited(.., Border, 74 + i % 3, 77 + i % 3, width, WhiteFont, YellowFont)`.
            let frame = if hover { 77 + i % 3 } else { 74 + i % 3 };
            // The frame repeats along the row (`GP_TextButton` draw 0x46ef12).
            let step = ctx.lib.frame_size(gp::BORDER, frame)[0].max(1);
            for k in 0..(r.width() + step - 1) / step {
                out.push(Draw::game_frame(gp::BORDER, frame, r.x0 + k * step, r.y0, 0).clipped(*r));
            }
            let text = ctx.texts.get(entry.text(self));
            let f = if hover { font::WHITE } else { font::YELLOW };
            let w = ctx.fonts.width(ctx.lib, f, text);
            out.push(Draw::game_text(f, text, r.x0 + (r.width() - w) / 2, r.y0 + 2));
        }
    }

    /// Hints of the resource values and icons: a box at the mouse (`DS+6 == −1`, 0x47d6b2).
    fn draw_hint(&self, ctx: &Ctx, view: &View, palette: &Palette, out: &mut Vec<Draw>) {
        let Some(m) = self.mouse else { return };
        let mut hint: Option<&[u8]> = None;
        for (i, id) in RES_HINTS.iter().enumerate() {
            if res_hint_rect(self.screen, i).contains(m) {
                hint = Some(ctx.texts.get(id));
            }
        }
        for icon in view.icons.iter().filter(|i| i.slot < GRID_COLUMNS * GRID_ROWS) {
            if Self::icon_rect(self.screen, icon.slot).contains(m) && !icon.hint.is_empty() {
                hint = Some(&icon.hint);
            }
        }
        if self.panel == Panel::Game
            && let Some(id) = panels::GamePanel::hint(self.panel_point(m))
        {
            hint = Some(ctx.texts.get(id));
        }
        if self.panel == Panel::Unit
            && let Some(h) = self.unit_panel.hint()
        {
            hint = Some(h);
        }
        let Some(text) = hint else { return };
        tooltip(ctx, palette, text, m, self.screen, out);
    }
}

/// `RESHINT0..5`: hints of the resource values (viewports of 0x5aab4b).
const RES_HINTS: [&str; 6] = ["RESHINT0", "RESHINT1", "RESHINT2", "RESHINT3", "RESHINT4", "RESHINT5"];

fn res_hint_rect(screen: [i32; 2], i: usize) -> Rect {
    Rect::new(screen[0] - 0x2b3 + RES_STEP * i as i32, 7, 105, 20)
}

/// Corner table 0x668580: TL, TR, BL, BR, left, right, top, bottom frames of `Border`.
pub const BOX_CORNERS: [i32; 8] = [0, 2, 6, 8, 12, 14, 9, 11];

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
fn draw_resources(ctx: &Ctx, view: &View, screen: [i32; 2], out: &mut Vec<Draw>) {
    let x0 = screen[0] - RES_PANEL;
    for i in 0..=(x0 / 256) {
        out.push(Draw::game_frame(gp::RESPANEL, 1, i * 256, 0, 0));
    }
    out.push(Draw::game_frame(gp::RESPANEL, 0, x0, 0, 0));
    let (bx, by) = menu_button(screen);
    out.push(Draw::game_frame(gp::RESPANEL, 2, bx, by, 0));
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

/// One icon (0x494c96): the picture (one pixel down-right while pressed), the outline in
/// the owner's colour, the yellow frame when highlighted, the life bar, the count.
fn draw_icon(icon: &Icon, x: i32, y: i32, pressed: bool, palette: &Palette, out: &mut Vec<Draw>) {
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
    if let Some(n) = icon.count {
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
        assert_eq!(res_hint_rect(SCREEN, 0), Rect::new(333, 7, 105, 20));
        let hud = Hud::default();
        assert_eq!((hud.panel_dx(), hud.panel_point([800.0, 100.0])), (0, [800.0, 100.0]));
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
}
