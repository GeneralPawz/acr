//! The test editor (main menu "Editor", docs/controls.md, "Editor"): a blank field where
//! units are placed directly, any number at once, loose or already standing in a formation,
//! with fighting switched off while the battle is set up. Not in the original, whose editor
//! places single units only. A click that changes the world becomes an [`Input`]: the game
//! applies it between two ticks and logs it with its tick, so a savegame replays it ([`input`]).
//!
//! The window is the original's "Einheit wählen" (`ED_HDR01`, built by 0x5bb6c0, docs/ui.md,
//! "Editor"): the side panel's column with a title, the eight player colours, a nation combo
//! box and a scrolled grid of unit icons under category headers. ACR's own controls (count,
//! formation, upgrade level, facing, fighting, cannon switches, the player to control) sit in
//! a second box directly under it.

pub(crate) mod input;
mod levels;
pub mod sides;
mod world;

use std::cell::RefCell;
use std::collections::HashMap;

use acr_formats::m3d::SpriteGroup;
use acr_mapgen::edit::{self, Dab, Tool};
use acr_render::terrain::TerrainData;
use acr_sim::catalog::{Medium, MotionStyle};
use acr_sim::economy::WOOD;
use acr_sim::fixed::Pos;
use acr_sim::tech::UpgradeId;
use acr_sim::{TypeId, World};

use crate::hud::{self, SCREEN, panels};
use crate::ui::Ctx;
use crate::ui::dialog::{Dialog, Kind};
use crate::ui::draw::{Draw, GAME_PALETTE, Rect};
use crate::ui::fonts::FontId;
use crate::ui::screens::font as menu_font;

mod battle;
pub mod frame;
mod presets;

use frame::{Drag, Frame, Grab};
use input::Input;

const WHITE: FontId = FontId("SmallWhiteFont");
const YELLOW: FontId = FontId("SmallYellowFont");
/// `Interf2\Combo2` (the global at 0x1100c38, loaded at 0x4eff3b) and `Interf2\VScroll2`.
const COMBO: &str = "INTERF2\\COMBO2.GP";
const VSCROLL: &str = "INTERF2\\VSCROLL2.GP";

/// The `DialogsSystem` origin of the window (0x5bb6c0): `RealLx − 259 − 10`, `InfY1 + 31`.
const OX: i32 = SCREEN[0] - 259 - 10;
const OY: i32 = 48 + 31;
/// The window's box (`addBorder(0, −31, 249, 386, 74, 3)`, 0x47c05b): its right and bottom
/// edge and the divider under the colours and the combo box.
const BOX_Y0: i32 = OY - 31;
const BOX_Y1: i32 = OY + 386;
const DIVIDER: i32 = OY + 74;
/// Corner table 0x668640 of the box above the divider (style 3, 0x48039d); below it the unit
/// panel's paper (0x6686a0), both with fill 0x1c.
const HEADER_CORNERS: [i32; 8] = [17, 18, -1, -1, 12, 14, 23, -1];
const PAPER_FILL: usize = 28;
/// Icon cells: 41 × 41 on a 44 pixel pitch, five to a row from x = 6 (0x5bb9d7); a category
/// header takes 20 pixels, the first one starts at y = 80 (0x5bb8c2).
const CELL: i32 = 41;
const PITCH: i32 = 44;
const COLUMNS: usize = 5;
const FIRST_ROW: i32 = 80;
const HEADER_H: i32 = 20;
/// The scroll bar (`addNewGP_VScrollBar` at (259 − 37, 82), 303 long) and the content height
/// it scrolls to: the content's end less `0x17e` (0x5bba8a).
const BAR_X: i32 = OX + 259 - 37;
const BAR_Y: i32 = OY + 82;
const BAR_LEN: i32 = 303;
const BAR_W: i32 = 16;
const VISIBLE_H: i32 = 0x17e;
/// The palette index of a player's colour is `0xd0 + 4 · player` (0x5bb140); the cell
/// outline is 0x65, 0xff when picked (0x5bb630).
const OUTLINE: u8 = 0x65;
const OUTLINE_PICKED: u8 = 0xff;
/// ACR's controls: the second box under the window.
const X0: i32 = OX + 8;
const W: i32 = 249 - 16;
const ROW: i32 = 16;
const EXTRA_Y: i32 = BOX_Y1 + 6;
/// The nation combo box (10, 36), 224 wide; a right click on it opens the army menu.
const COMBO_BOX: Rect = Rect { x0: OX + 10, y0: OY + 36, x1: OX + 10 + 224, y1: OY + 36 + 22 };
/// The counts offered; a count above one formation's size places several side by side.
pub const COUNTS: [u32; 12] = [1, 5, 10, 15, 36, 72, 120, 196, 392, 588, 784, 980];
/// The combo box's lines: the playable nations in the order of `TEXT\NATIONS.TXT` (name, and
/// the prefix of their type ids), then ACR's own lists: the animals, the cannons and the
/// map's nature (trees, bushes, stones and deposits) and the terrain tools.
pub const ENTRIES: [(&str, &str); 16] = [
    ("Maya", "May"),
    ("Spanien", "Spn"),
    ("England", "Eng"),
    ("Frankreich", "Frn"),
    ("USA", "Usa"),
    ("Inka", "Inc"),
    ("Azteken", "Atc"),
    ("Pueblos", "Pue"),
    ("Delawaren", "Del"),
    ("Huronen", "Hur"),
    ("Irokesen", "Iro"),
    ("Sioux", "Siu"),
    ("Tiere", "Anm"),
    ("Kanonen", "Art"),
    ("Natur", ""),
    ("Gelaende", ""),
];
/// The first of [`ENTRIES`] that is ACR's own (not a nation of the original).
const FIRST_EXTRA: usize = 12;
/// The line of [`ENTRIES`] that lists the map's trees and stones, not types.
const NATUR: usize = 14;
/// The line of [`ENTRIES`] that lists the terrain tools and ground textures.
const GELAENDE: usize = 15;
/// The terrain tools of the Gelaende list (ACR's own: the original editor's ToolBox/ToolOpt at
/// 0x5ba450 are untraced).
const TOOLS: [(&str, GroundTool); 5] = [
    ("Anheben", GroundTool::Raise),
    ("Absenken", GroundTool::Lower),
    ("Ebnen", GroundTool::Flatten),
    ("Wasser", GroundTool::Water),
    ("Land", GroundTool::Land),
];
/// The ground textures of the Gelaende list: name and `TILE` value (docs/ground.md).
const GROUNDS: [(&str, u8); 15] = [
    ("Gras", 0),
    ("Wiese", 1),
    ("Erde", 71),
    ("Schlamm", 29),
    ("Sand", 72),
    ("Fels", 8),
    ("Waldboden", 28),
    ("Schnee", 103),
    ("Feld", 27),
    ("Rissig", 42),
    ("Roterde", 66),
    ("Kies", 14),
    ("Wueste", 120),
    ("Steppe", 93),
    ("Pflaster", 24),
];
/// The brush radii the Pinsel line steps through, world pixels.
pub const BRUSHES: [i32; 5] = [32, 64, 128, 256, 512];
/// The radius a new editor paints with: `BRUSHES[2]`.
const DEFAULT_BRUSH: usize = 2;
/// A dab when the cursor moved `radius / STROKE_STEP` since the last one ...
const STROKE_STEP: i32 = 2;
/// ... or this many seconds passed, at most one per frame.
const STROKE_EVERY: f32 = 0.1;
/// During a stroke the scene, minimap and models are built again at most this often (seconds).
const REFRESH_EVERY: f32 = 0.25;
/// The nation list opens on England, as the first game does for most players.
const DEFAULT_ENTRY: usize = 2;
/// The owner of wild animals: the eighth colour of the window is this neutral player.
pub const NEUTRAL: u8 = 7;
/// Compass names of the eight facings, `dir` = 32 · i (0 east, 64 south).
const FACINGS: [&str; 8] = ["Ost", "Suedost", "Sued", "Suedwest", "West", "Nordwest", "Nord", "Nordost"];
/// Space between formations placed side by side, world pixels.
const FORMATION_GAP: i32 = 64;
/// The categories of a nation's list (the `ED_` headers of `TEXT\EDITOR.TXT`).
const CATEGORIES: [&str; 5] = ["Infanterie", "Kavallerie", "Artillerie", "Marine", "Gebaeude"];

/// A click target of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Opens or closes the nation combo box.
    Combo,
    /// Picks a line of the combo box.
    Entry(usize),
    /// A line of the army menu (a right click on the combo box opens it).
    Army(presets::Act),
    /// A click of the battle view ("Schlacht").
    Battle(battle::Act),
    Pick(Pick),
    /// Picks the owner (one of the eight colours).
    Owner(u8),
    /// Cycles the team number of a colour (a right click on its swatch).
    Team(u8),
    /// Picks the open level drop-down's type at that level ([`levels`]).
    SetLevel(usize),
    /// Scrolls the icons by pixels / by pages (the scroll bar's arrows and track).
    ScrollBy(i32),
    Count(i32),
    Form(i32),
    Facing(i32),
    /// Steps the brush radius ([`BRUSHES`]).
    Brush(i32),
    Peace,
    Reload,
    AutoReload,
    Control,
    Hide,
    /// Takes hold of the title row's handle or the corner grip ([`frame`]).
    Grab(Grab),
}

/// How an item is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// A line of text, highlighted when `hot`.
    Text,
    /// The window's title.
    Title,
    /// A category header of the grid.
    Header,
    /// A box of the window: with a divider below the colours, or plain paper.
    Frame {
        divider: Option<i32>,
    },
    /// A unit icon (`ICONS.GP` frame) in its cell.
    Icon {
        frame: Option<usize>,
    },
    /// A ground texture: its colour (RGB) fills the cell.
    Texture {
        colour: [u8; 3],
    },
    /// A map object's frame of the pack in [`Item::gp`], its ground point `(ox, oy)` at the
    /// bottom centre of the cell, clipped to it; the name stands in the cell without a pack.
    Sprite {
        frame: usize,
        ox: i32,
        oy: i32,
    },
    /// A player colour; `hot` adds the larger outline.
    Swatch {
        owner: u8,
    },
    /// The closed combo box.
    Combo,
    /// A line of the open combo box.
    DropRow,
    /// The scroll bar, drawn as a whole.
    Bar {
        pos: i32,
        max: i32,
    },
    /// The title row's handle (dots), the "_" button and the corner grip (ACR's, [`frame`]).
    Handle,
    Minimise,
    Corner,
}

/// One thing of the window: where it is, what it says, whether it is highlighted.
#[derive(Debug, Clone)]
pub struct Item {
    pub rect: Rect,
    pub text: Vec<u8>,
    /// The name the test driver finds it by.
    pub name: String,
    pub hot: bool,
    pub act: Option<Act>,
    pub look: Look,
    /// The sprite pack of a [`Look::Sprite`] cell.
    pub gp: Option<String>,
}

/// A formation a type can stand in: the `ORD_GROUPS.LST` group, its officer type and the
/// size variants (shape indices).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormChoice {
    pub group: usize,
    pub officer: TypeId,
    pub shapes: Vec<usize>,
}

/// What the window needs of the game to draw a type.
pub struct View<'a> {
    /// A type's display name.
    pub names: &'a dyn Fn(TypeId) -> Vec<u8>,
    /// A type's `ICONS.GP` frame (`ICON` of its `.MD`), if it has one.
    pub icon: &'a dyn Fn(TypeId) -> Option<usize>,
    /// An upgrade's text, as the game's upgrade hint shows it.
    pub upgrade_name: &'a dyn Fn(UpgradeId) -> Vec<u8>,
    /// The pack, and the ground point inside the frame, of a map object of a sprite list
    /// (frame = the object's index), once the list is loaded.
    pub sprite: &'a dyn Fn(SpriteGroup, u16) -> Option<(String, i32, i32)>,
    /// The colour of a ground texture as the minimap shows it, once the ground is loaded.
    pub texture_colour: &'a dyn Fn(u8) -> Option<[u8; 3]>,
    /// `ED_HDR01`.
    pub title: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Editor {
    /// Where the window stands and how big it is; folded: only the title row (F2, "_", the title).
    pub frame: Frame,
    /// The handle or corner grip being dragged.
    drag: Option<Drag>,
    /// The combo box line: an index in [`ENTRIES`].
    entry: usize,
    combo_open: bool,
    /// The icons' scroll position in pixels.
    scroll: i32,
    /// What a click on the map places.
    pub pick: Option<Pick>,
    /// The terrain brush radius: an index in [`BRUSHES`].
    brush: usize,
    /// The terrain stroke the held left button makes.
    stroke: Option<Stroke>,
    /// The terrain, objects or minimap changed and the view is built again at the next
    /// [`REFRESH_EVERY`] (or at the end of the stroke).
    pub stale: Option<Stale>,
    /// Seconds since the view was last built again.
    since_refresh: f32,
    count: usize,
    /// 0 = loose, else 1 + the index in [`formations`].
    form: usize,
    /// The player placed units belong to, 0..=7 ([`NEUTRAL`] is wild animals).
    owner: u8,
    facing: [u8; 8],
    /// Training level of the picked type: how many of its upgrades apply, in the nation
    /// file's order ([`levels`]).
    level: usize,
    /// The level drop-down, open after a right click on an icon.
    menu: Option<levels::Menu>,
    /// Who fights whom ([`sides`]); everybody is allied on start.
    pub sides: sides::Mode,
    /// The team number of each owner, used by [`sides::Mode::Teams`].
    team: [u8; 8],
    /// Every cannon is reloaded each tick (a test switch, off on start).
    pub auto_reload: bool,
    /// The army menu and the army the next click on the map places.
    pub army: presets::Menu,
    /// The battle view (a button in the title row switches to it).
    pub battle: battle::Battle,
    /// The camera is to centre on this world point (set by "Aufstellen").
    pub camera_to: Option<(i32, i32)>,
    /// Orders the game issues on its next tick (the battle view's marches).
    pub orders: Vec<acr_sim::command::Command>,
    /// The last placement or problem, shown under the window.
    status: Vec<u8>,
    /// A press placed or hit the window: its release does nothing else.
    pub swallow_release: bool,
    /// The `ICONS.GP` frames of the catalog's types, once the game's data is loaded.
    pub icons: RefCell<Option<HashMap<TypeId, Option<usize>>>>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            frame: frame::DEFAULT,
            drag: None,
            entry: DEFAULT_ENTRY,
            combo_open: false,
            scroll: 0,
            pick: None,
            brush: DEFAULT_BRUSH,
            stroke: None,
            stale: None,
            since_refresh: 0.0,
            count: COUNTS.iter().position(|&c| c == 36).unwrap_or(0),
            form: 0,
            owner: 0,
            // Player 1 faces north (up the screen), player 2 south, the others east.
            facing: [192, 64, 0, 0, 0, 0, 0, 0],
            level: 0,
            menu: None,
            army: presets::Menu::default(),
            sides: sides::Mode::Off,
            team: sides::default_teams(),
            battle: battle::Battle::default(),
            camera_to: None,
            orders: Vec::new(),
            auto_reload: false,
            status: b"Typ waehlen, dann auf die Karte klicken".to_vec(),
            swallow_release: false,
            icons: RefCell::new(None),
        }
    }
}

/// Reloads every cannon (`COMPLEXROTATE` type) of every player at once: its reload counter
/// is cleared, so its next shot starts as soon as it is aimed. Returns the cannons reloaded.
pub fn reload_guns(world: &mut World) -> usize {
    let catalog = world.catalog.clone();
    let mut n = 0;
    for u in world.units.iter_mut().filter(|u| u.alive && !u.building) {
        if catalog.get(u.kind).motion_style == acr_sim::catalog::MotionStyle::ComplexRotate {
            u.delay = 0;
            n += 1;
        }
    }
    n
}

/// What a cell of the list picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// An object type: units, animals, cannons, buildings.
    Unit(TypeId),
    /// A tree, bush, stone or deposit: its sprite group and index in the group's list.
    Nature(SpriteGroup, u16),
    /// A tool of the terrain brush.
    Ground(GroundTool),
}

/// The tools of the terrain brush (ACR's own, the original's are not traced).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundTool {
    Raise,
    Lower,
    Flatten,
    Water,
    Land,
    /// Paints a ground texture (`TILE` value).
    Texture(u8),
}

/// A stroke of the terrain brush: the last dab and what passed since.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stroke {
    /// The last dab; the next ones have its radius and tool (Flatten keeps the level the
    /// press took).
    dab: Dab,
    /// Seconds since the last dab.
    since: f32,
}

/// What the edits left behind for the view to rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stale {
    /// The minimap shows the change (not trees).
    pub minimap: bool,
    /// The ground changed (the 3D models and ground cover follow).
    pub ground: bool,
}

/// A category of an entry's list: its header and what its cells pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub header: &'static str,
    pub kinds: Vec<Pick>,
}

/// The category of a type in a nation's list: buildings, ships, riders, guns
/// (`COMPLEXROTATE`) or foot.
fn category(t: &acr_sim::catalog::ObjectType) -> usize {
    if t.building {
        4
    } else if t.medium == Medium::Water {
        3
    } else if acr_sim::spacing::is_rider(t) {
        1
    } else if t.motion_style == MotionStyle::ComplexRotate {
        2
    } else {
        0
    }
}

/// Types an entry lists, in the order of the nation files (the catalog's): units and
/// animals of the prefix that stand on land, in the categories of the original's lists.
// The original builds these lists at load (0x111ca9c/0x111caa0/0x111caa4, read at 0x5bb8aa);
// where it fills them is unknown, so this groups the catalog by kind (docs/ui.md, "Editor").
pub fn groups(world: &World, entry: usize) -> Vec<Group> {
    if entry == NATUR {
        return nature_groups(world);
    }
    if entry == GELAENDE {
        return ground_groups();
    }
    let prefix = ENTRIES[entry].1.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut by: Vec<Vec<TypeId>> = vec![Vec::new(); CATEGORIES.len()];
    for (id, t) in world.catalog.iter() {
        if t.life <= 0 || !world::of_nation(t, entry, &prefix) || !seen.insert(id) {
            continue;
        }
        by[category(t)].push(id);
    }
    if entry >= FIRST_EXTRA {
        // Animals and cannons are one list under their own name.
        let kinds = by.into_iter().take(3).flatten().map(Pick::Unit).collect();
        return vec![Group { header: ENTRIES[entry].0, kinds }];
    }
    by.into_iter()
        .zip(CATEGORIES)
        .filter(|(kinds, _)| !kinds.is_empty())
        .map(|(kinds, header)| Group { header, kinds: kinds.into_iter().map(Pick::Unit).collect() })
        .collect()
}

/// The Gelaende list: the brush tools and the ground textures.
fn ground_groups() -> Vec<Group> {
    vec![
        Group { header: "Werkzeug", kinds: TOOLS.iter().map(|&(_, t)| Pick::Ground(t)).collect() },
        Group { header: "Boden", kinds: GROUNDS.iter().map(|&(_, t)| Pick::Ground(GroundTool::Texture(t))).collect() },
    ]
}

/// The name of a terrain tool or ground texture.
fn ground_name(tool: GroundTool) -> &'static str {
    match tool {
        GroundTool::Texture(t) => GROUNDS.iter().find(|&&(_, v)| v == t).map_or("Boden", |&(n, _)| n),
        other => TOOLS.iter().find(|&&(_, v)| v == other).map_or("Werkzeug", |&(n, _)| n),
    }
}

/// The Natur list, from the map's sprite lists: trees that are a wood source, other trees
/// (bushes), stones without a source and stones with one (deposits that feed mines).
fn nature_groups(world: &World) -> Vec<Group> {
    let shapes = &world.shapes;
    let (mut trees, mut bushes, mut stones, mut deposits) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (group, kinds) in &shapes.kinds {
        for index in 0..kinds.chars.len() as u16 {
            let pick = Pick::Nature(*group, index);
            match group {
                SpriteGroup::Trees => {
                    let wood = shapes.work_source(*group, index).is_some_and(|(kind, ..)| usize::from(kind) == WOOD);
                    if wood { &mut trees } else { &mut bushes }.push(pick);
                }
                SpriteGroup::Stones => {
                    if shapes.source(*group, index).is_some() { &mut deposits } else { &mut stones }.push(pick);
                }
                _ => {}
            }
        }
    }
    [("Baeume", trees), ("Buesche", bushes), ("Steine", stones), ("Vorkommen", deposits)]
        .into_iter()
        .filter(|(_, kinds)| !kinds.is_empty())
        .map(|(header, kinds)| Group { header, kinds })
        .collect()
}

/// The name of a map object type and its `[AG i]`/`[ST i]` tag, as the driver finds it by.
fn nature_label(world: &World, group: SpriteGroup, index: u16) -> (String, String) {
    let name =
        world.shapes.kinds.get(&group).and_then(|k| k.names.get(usize::from(index))).cloned().unwrap_or_default();
    let code = String::from_utf8_lossy(&group.code()).into_owned();
    let label = format!("{name} [{code} {index}]");
    (name, label)
}

/// What the flow of [`flow`] puts at a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placed {
    /// The header of group `.0` at y.
    Header(usize, i32),
    /// Type `.1` of group `.0` in column `.2` at y.
    Cell(usize, usize, usize, i32),
}

/// The grid's flow (0x5bb8c2): from y = 80 relative to the window's origin, a header per
/// group and its icons `columns` (five in the original) to a row; returns the places and the y
/// after the last row.
pub fn flow(groups: &[Group], columns: usize) -> (Vec<Placed>, i32) {
    let mut out = Vec::new();
    let mut y = FIRST_ROW;
    for (g, group) in groups.iter().enumerate() {
        out.push(Placed::Header(g, y));
        y += HEADER_H;
        for (i, _) in group.kinds.iter().enumerate() {
            out.push(Placed::Cell(g, i, i % columns, y + (i / columns) as i32 * PITCH));
        }
        y += group.kinds.len().div_ceil(columns) as i32 * PITCH;
    }
    (out, y)
}

/// How far the icons scroll: the content's end less the visible height (0x5bba8a).
pub fn scroll_max(end: i32, visible: i32) -> i32 {
    (end - visible).max(0)
}

/// The formations soldiers of `kind` can stand in, as their officers list them.
pub fn formations(world: &World, kind: TypeId) -> Vec<FormChoice> {
    let mut out: Vec<FormChoice> = Vec::new();
    for (&officer, o) in &world.formations.officers {
        for f in o.formations.iter().filter(|f| f.kinds.contains(&kind) && !f.shapes.is_empty()) {
            if !out.iter().any(|c| c.group == f.group) {
                out.push(FormChoice { group: f.group, officer, shapes: f.shapes.clone() });
            }
        }
    }
    out.sort_by_key(|c| c.group);
    out
}

/// The German name of a formation group.
fn group_name(world: &World, group: usize) -> String {
    let name = world.formations.groups.get(group).map_or("?", |g| g.name.as_str());
    match name.to_ascii_uppercase().as_str() {
        "LINE" => "Linie".into(),
        "SQUARE" => "Kolonne".into(),
        "KARE" => "Karree".into(),
        other => other.to_owned(),
    }
}

/// `n` positions of a loose block centred on `(x, y)`, rows across `dir`, `gap` apart.
pub fn grid(n: usize, (x, y): (i32, i32), dir: u8, gap: i32) -> Vec<(i32, i32)> {
    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    let rows = n.div_ceil(cols);
    let a = f64::from(dir) * std::f64::consts::TAU / 256.0;
    // Facing (fx, fy); across it (-fy, fx).
    let (fx, fy) = (a.cos(), a.sin());
    (0..n)
        .map(|i| {
            let (c, r) = ((i % cols) as f64, (i / cols) as f64);
            let u = (c - (cols - 1) as f64 / 2.0) * f64::from(gap);
            let v = ((rows - 1) as f64 / 2.0 - r) * f64::from(gap);
            (x + (u * -fy + v * fx).round() as i32, y + (u * fx + v * fy).round() as i32)
        })
        .collect()
}

impl Editor {
    pub fn count(&self) -> u32 {
        COUNTS[self.count]
    }

    /// The terrain brush radius, world pixels.
    pub fn radius(&self) -> i32 {
        BRUSHES[self.brush]
    }

    pub fn owner(&self) -> u8 {
        self.owner
    }

    pub fn facing(&self) -> u8 {
        self.facing[usize::from(self.owner)]
    }

    /// The window's items in drawing order (hit tests go from the back).
    pub fn layout(&self, world: &World, view: &View, local: u8) -> Vec<Item> {
        // Built at the original's place in the window's size, then moved.
        let (dx, dy) = self.frame.shift();
        let mut out = self.build(world, view, local);
        for i in &mut out {
            i.rect = i.rect.offset(dx, dy);
        }
        // The level drop-down, over everything.
        if let Some(menu) = self.menu {
            let names = levels::names(world, menu.kind, view.upgrade_name);
            let rows = menu.rows(names.len(), names.iter().map(Vec::len).max().unwrap_or(0), self.frame.x1());
            for (n, (rect, text)) in rows.into_iter().zip(names).enumerate() {
                let name = format!("Ausbau {}", String::from_utf8_lossy(&text));
                let hot = self.pick == Some(Pick::Unit(menu.kind)) && self.level == n;
                out.push(Item { rect, text, name, hot, act: Some(Act::SetLevel(n)), look: Look::DropRow, gp: None });
            }
        }
        out
    }

    /// The items at the original's place, without the level drop-down.
    fn build(&self, world: &World, view: &View, local: u8) -> Vec<Item> {
        let f = self.frame.at_default();
        let mut out = Vec::new();
        let item = |rect: Rect, text: Vec<u8>, name: &str, hot: bool, act: Option<Act>, look: Look| Item {
            rect,
            text,
            name: name.to_owned(),
            hot,
            act,
            look,
            gp: None,
        };
        // The handle and "_" button stand in the folded title row too.
        let buttons = |out: &mut Vec<Item>| {
            let grab = Some(Act::Grab(Grab::Move));
            out.push(item(f.handle(), Vec::new(), "Editor verschieben", false, grab, Look::Handle));
            out.push(item(f.minimise(), b"_".to_vec(), "Editor minimieren", false, Some(Act::Hide), Look::Minimise));
        };
        if self.frame.folded {
            let rect = Rect::new(OX, BOX_Y0, f.w, ROW);
            out.push(item(rect, b"[Editor] (F2)".to_vec(), "Editor", false, Some(Act::Hide), Look::Text));
            buttons(&mut out);
            return out;
        }
        let corner = item(f.grip(), Vec::new(), "Editor Groesse", false, Some(Act::Grab(Grab::Resize)), Look::Corner);
        let window = Rect::inclusive(OX, BOX_Y0, f.x1(), f.y1());
        out.push(item(window, Vec::new(), "", false, None, Look::Frame { divider: Some(DIVIDER) }));
        let title = Rect::new(OX + 10, OY - 30, 110, ROW);
        out.push(item(title, view.title.clone(), "Editor einklappen", false, Some(Act::Hide), Look::Title));
        let (label, name) = if self.battle.on { ("Einheiten", "Einheiten") } else { ("Schlacht", "Schlacht") };
        buttons(&mut out);
        let switch = Rect::new(f.x1() - 70, OY - 30, 66, ROW);
        out.push(item(
            switch,
            label.as_bytes().to_vec(),
            name,
            self.battle.on,
            Some(Act::Battle(battle::Act::Toggle)),
            Look::Text,
        ));
        if self.battle.on {
            out.extend(self.battle_items());
            out.push(corner);
            return out;
        }
        // The eight player colours (0x5bb140): 28 apart from x = 10, y = 10.
        for p in 0..8u8 {
            let rect = Rect::new(OX + 10 + 28 * i32::from(p), OY + 10, 22, 16);
            let name = if p == NEUTRAL { "Neutral".to_owned() } else { format!("Spieler {}", p + 1) };
            let hot = self.owner == p;
            // In teams mode the team number shows on the swatch.
            let team = if self.sides == sides::Mode::Teams && p != NEUTRAL {
                self.team[usize::from(p)].to_string()
            } else {
                String::new()
            };
            out.push(item(rect, team.into_bytes(), &name, hot, Some(Act::Owner(p)), Look::Swatch { owner: p }));
        }
        // The grid, scrolled and clipped to the content.
        let groups = groups(world, self.entry);
        let (placed, end) = flow(&groups, f.columns());
        let max = scroll_max(end, f.visible_h());
        let scroll = self.scroll.clamp(0, max);
        for p in &placed {
            match *p {
                Placed::Header(g, y) => {
                    let rect = Rect::new(OX + 10, OY + y - scroll, 200, HEADER_H);
                    let text = groups[g].header.as_bytes().to_vec();
                    out.push(item(rect, text, "", false, None, Look::Header));
                }
                Placed::Cell(g, i, col, y) => {
                    let pick = groups[g].kinds[i];
                    let rect = Rect::new(OX + 5 + PITCH * col as i32, OY + y - 1 - scroll, CELL, CELL);
                    if rect.intersect(&f.content()).is_empty() {
                        continue;
                    }
                    let hot = self.pick == Some(pick);
                    match pick {
                        Pick::Unit(kind) => {
                            let (frame, name) = ((view.icon)(kind), (view.names)(kind));
                            let label = String::from_utf8_lossy(&name).into_owned();
                            out.push(item(rect, name, &label, hot, Some(Act::Pick(pick)), Look::Icon { frame }));
                        }
                        Pick::Nature(group, index) => {
                            let (name, label) = nature_label(world, group, index);
                            let sprite = (view.sprite)(group, index);
                            let look = match &sprite {
                                Some((_, ox, oy)) => Look::Sprite { frame: usize::from(index), ox: *ox, oy: *oy },
                                None => Look::Icon { frame: None },
                            };
                            let mut cell = item(rect, name.into_bytes(), &label, hot, Some(Act::Pick(pick)), look);
                            cell.gp = sprite.map(|(gp, ..)| gp);
                            out.push(cell);
                        }
                        Pick::Ground(tool) => {
                            let name = ground_name(tool);
                            let look = match tool {
                                GroundTool::Texture(t) => match (view.texture_colour)(t) {
                                    Some(colour) => Look::Texture { colour },
                                    None => Look::Icon { frame: None },
                                },
                                _ => Look::Icon { frame: None },
                            };
                            out.push(item(rect, name.as_bytes().to_vec(), name, hot, Some(Act::Pick(pick)), look));
                        }
                    }
                }
            }
        }
        // The combo box.
        out.push(item(
            COMBO_BOX,
            ENTRIES[self.entry].0.as_bytes().to_vec(),
            "Nation",
            false,
            Some(Act::Combo),
            Look::Combo,
        ));
        // The scroll bar: arrows and the track's halves are the click targets.
        if max > 0 {
            let (bar_x, bar_len) = (BAR_X + f.extra_w(), BAR_LEN + f.extra_h());
            let bar = Rect::new(bar_x, BAR_Y, BAR_W, bar_len);
            out.push(item(bar, Vec::new(), "", false, None, Look::Bar { pos: scroll, max }));
            let (up, down) =
                (Rect::new(bar_x, BAR_Y, BAR_W, BAR_W), Rect::new(bar_x, BAR_Y + bar_len - BAR_W, BAR_W, BAR_W));
            out.push(item(up, Vec::new(), "Hoch", false, Some(Act::ScrollBy(-PITCH)), Look::Text));
            out.push(item(down, Vec::new(), "Runter", false, Some(Act::ScrollBy(PITCH)), Look::Text));
            let (mid_y, track_h) = (BAR_Y + BAR_W, bar_len - 2 * BAR_W);
            let page = f.visible_h() - PITCH;
            let above = Rect::new(bar_x, mid_y, BAR_W, track_h / 2);
            let below = Rect::new(bar_x, mid_y + track_h / 2, BAR_W, track_h - track_h / 2);
            out.push(item(above, Vec::new(), "Seite hoch", false, Some(Act::ScrollBy(-page)), Look::Text));
            out.push(item(below, Vec::new(), "Seite runter", false, Some(Act::ScrollBy(page)), Look::Text));
        }
        out.push(corner);
        // ACR's controls in a box under the window, as wide as it.
        let mut y = EXTRA_Y + f.extra_h();
        let chars = f.chars();
        let ew = W + f.extra_w();
        let extras = |rows: i32| Rect::inclusive(OX, f.y1() + 1, f.x1(), f.y1() + 1 + 12 + rows * ROW);
        let brush_row = self.entry == GELAENDE;
        let rows = 13 + i32::from(self.pick.is_some()) + i32::from(brush_row);
        out.push(item(extras(rows), Vec::new(), "", false, None, Look::Frame { divider: None }));
        let line = |out: &mut Vec<Item>, y: &mut i32, text: Vec<u8>, name: &str, hot: bool, act: Option<Act>| {
            out.push(item(Rect::new(X0, *y, ew, ROW), text, name, hot, act, Look::Text));
            *y += ROW;
        };
        let pair = |out: &mut Vec<Item>, y: &mut i32, text: String, name: &str, a: Act, b: Act| {
            let arrow = |x: i32, t: &str, d: &str, act: Act| {
                item(
                    Rect::new(x, *y, 24, ROW),
                    t.as_bytes().to_vec(),
                    &format!("{name} {d}"),
                    false,
                    Some(act),
                    Look::Text,
                )
            };
            out.push(arrow(X0, "<", "-", a));
            out.push(item(Rect::new(X0 + 24, *y, ew - 48, ROW), text.into_bytes(), name, false, None, Look::Text));
            out.push(arrow(X0 + ew - 24, ">", "+", b));
            *y += ROW;
        };
        pair(&mut out, &mut y, format!("Anzahl: {}", self.count()), "Anzahl", Act::Count(-1), Act::Count(1));
        let form = match self.unit() {
            Some(kind) if self.form > 0 => {
                formations(world, kind).get(self.form - 1).map_or("lose".into(), |c| group_name(world, c.group))
            }
            _ => "lose".into(),
        };
        pair(&mut out, &mut y, format!("Formation: {form}"), "Formation", Act::Form(-1), Act::Form(1));
        if brush_row {
            let text = format!("Pinsel: {}", self.radius());
            pair(&mut out, &mut y, text, "Pinsel", Act::Brush(-1), Act::Brush(1));
        }
        let level = self.level_line(world, view);
        line(&mut out, &mut y, level, "Ausbau", false, None);
        let facing = format!("Richtung: {}", FACINGS[usize::from(self.facing() / 32) & 7]);
        pair(&mut out, &mut y, facing, "Richtung", Act::Facing(-1), Act::Facing(1));
        let owner = if self.owner == NEUTRAL { "Neutral".to_owned() } else { format!("Spieler {}", self.owner + 1) };
        line(&mut out, &mut y, format!("Besitzer: {owner}").into_bytes(), "Besitzer", false, None);
        let fight = match self.sides {
            sides::Mode::Off => "Kampf: AUS (klicken: an)",
            sides::Mode::Free => "Kampf: AN, alle gegen alle (klicken: Teams)",
            sides::Mode::Teams => "Kampf: Teams, Rechtsklick auf Farbe (klicken: aus)",
        };
        line(&mut out, &mut y, fight.as_bytes().to_vec(), "Kampf", self.sides != sides::Mode::Off, Some(Act::Peace));
        line(&mut out, &mut y, b"Kanonen sofort laden".to_vec(), "Kanonen sofort laden", false, Some(Act::Reload));
        let auto = if self.auto_reload { "Kanonen: sofort nachladen AN" } else { "Kanonen: sofort nachladen AUS" };
        line(&mut out, &mut y, auto.as_bytes().to_vec(), "Kanonen nachladen", self.auto_reload, Some(Act::AutoReload));
        let me = if local == NEUTRAL { "Neutral".to_owned() } else { format!("Spieler {}", local + 1) };
        line(&mut out, &mut y, format!("Ich steuere: {me}").into_bytes(), "Ich steuere", false, Some(Act::Control));
        if let Some(pick) = self.pick {
            let mut text = b"Typ: ".to_vec();
            match pick {
                Pick::Unit(kind) => text.extend((view.names)(kind)),
                Pick::Nature(group, index) => text.extend(nature_label(world, group, index).1.into_bytes()),
                Pick::Ground(tool) => text.extend(ground_name(tool).as_bytes()),
            }
            text.truncate(chars);
            line(&mut out, &mut y, text, "", false, None);
        }
        let mut status = self.status.clone();
        status.truncate(chars);
        line(&mut out, &mut y, status, "", false, None);
        line(&mut out, &mut y, b"Rechtsklick: Platzieren beenden".to_vec(), "", false, None);
        line(&mut out, &mut y, b"Rechtsklick auf Nation: Armee".to_vec(), "", false, None);
        out.extend(self.army_items());
        // The open combo box's lines, over everything.
        if self.combo_open {
            for (i, (label, _)) in ENTRIES.iter().enumerate() {
                let rect = Rect::new(OX + 10, OY + 36 + 22 + i as i32 * ROW, 224, ROW);
                let name = format!("Nation {label}");
                out.push(item(
                    rect,
                    label.as_bytes().to_vec(),
                    &name,
                    i == self.entry,
                    Some(Act::Entry(i)),
                    Look::DropRow,
                ));
            }
        }
        out
    }

    /// The Ausbau line: the picked type's level, its name and the upgrades applied.
    fn level_line(&self, world: &World, view: &View) -> Vec<u8> {
        let names = self.unit().map_or_else(
            || vec![levels::BASIS.as_bytes().to_vec()],
            |kind| levels::names(world, kind, view.upgrade_name),
        );
        // The upgrade texts are long: the count comes first, the line is cut to the window.
        let mut text = format!("Ausbau {}/{}: ", self.level, names.len() - 1).into_bytes();
        text.extend(names.get(self.level).map_or(&[][..], Vec::as_slice));
        text.truncate(self.frame.chars());
        text
    }

    /// A right click on the icon of `kind` at `cell`: opens its level drop-down.
    pub fn open_levels(&mut self, kind: TypeId, cell: Rect) {
        self.combo_open = false;
        self.menu = Some(levels::Menu { kind, anchor: cell });
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// Closes the level drop-down; true if it was open.
    pub fn close_levels(&mut self) -> bool {
        self.menu.take().is_some()
    }

    /// The window's whole area (clicks there never reach the map).
    pub fn bounds(items: &[Item]) -> Rect {
        let mut frames = items
            .iter()
            .filter(|i| matches!(i.look, Look::Frame { .. }) || matches!(i.act, Some(Act::Hide | Act::Grab(_))));
        let Some(first) = frames.next() else { return Rect::new(0, 0, 0, 0) };
        let first = first.rect;
        frames.fold(first, |r, i| Rect {
            x0: r.x0.min(i.rect.x0),
            y0: r.y0.min(i.rect.y0),
            x1: r.x1.max(i.rect.x1),
            y1: r.y1.max(i.rect.y1),
        })
    }

    pub fn draws(items: &[Item], ctx: &Ctx, content: Rect, out: &mut Vec<Draw>) {
        ctx.lib.want(COMBO);
        ctx.lib.want(VSCROLL);
        let palette = ctx.lib.palette(GAME_PALETTE);
        let colour = |index: u8| {
            palette.as_ref().map_or([128, 128, 128, 255], |p| {
                let [r, g, b] = p.rgb(index);
                [r, g, b, 255]
            })
        };
        for i in items {
            let r = i.rect;
            match i.look {
                Look::Frame { divider } => {
                    let (x0, y0, x1, y1) = (r.x0, r.y0, r.x1 - 1, r.y1 - 1);
                    match divider {
                        Some(d) => {
                            panels::paper(ctx, out, (x0, y0, x1, d), PAPER_FILL, HEADER_CORNERS);
                            panels::paper(ctx, out, (x0, d, x1, y1), PAPER_FILL, panels::PAPER_CORNERS);
                        }
                        None => panels::paper(ctx, out, (x0, y0, x1, y1), PAPER_FILL, panels::PAPER_CORNERS),
                    }
                }
                Look::Title => out.push(Draw::game_text(menu_font::F16Y1, &i.text, r.x0, r.y0)),
                Look::Header => out.push(Draw::game_text(menu_font::F16Y1, &i.text, r.x0, r.y0).clipped(content)),
                Look::Icon { frame } => {
                    let index = if i.hot { OUTLINE_PICKED } else { OUTLINE };
                    let mut cell = Vec::new();
                    match frame {
                        Some(f) => cell.push(Draw::game_frame(hud::gp::ICONS, f, r.x0 + 1, r.y0 + 1, 0)),
                        // No icon: the name stands in its cell, cut at the cell's edge.
                        None => cell.push(
                            Draw::game_text(WHITE, &i.text[..i.text.len().min(8)], r.x0 + 3, r.y0 + 14).clipped(r),
                        ),
                    }
                    hud::outline(&mut cell, r, colour(index));
                    out.extend(cell.into_iter().map(|d| d.clipped(content)));
                }
                Look::Texture { colour: [red, green, blue] } => {
                    let index = if i.hot { OUTLINE_PICKED } else { OUTLINE };
                    let mut cell =
                        vec![Draw::fill([red, green, blue, 255], Rect::new(r.x0 + 1, r.y0 + 1, CELL - 2, CELL - 2))];
                    hud::outline(&mut cell, r, colour(index));
                    out.extend(cell.into_iter().map(|d| d.clipped(content)));
                }
                Look::Sprite { frame, ox, oy } => {
                    let index = if i.hot { OUTLINE_PICKED } else { OUTLINE };
                    let mut cell = Vec::new();
                    match &i.gp {
                        // The ground point at the bottom centre of the cell.
                        Some(gp) => cell.push(
                            Draw::game_frame(gp, frame, r.x0 + CELL / 2 - ox, r.y1 - 4 - oy, 0)
                                .clipped(r.intersect(&content)),
                        ),
                        None => cell.push(Draw::game_text(WHITE, &i.text[..i.text.len().min(8)], r.x0 + 3, r.y0 + 14)),
                    }
                    hud::outline(&mut cell, r, colour(index));
                    out.extend(cell.into_iter().map(|d| d.clipped(content)));
                }
                Look::Swatch { owner } => {
                    let c = colour(hud::color::PLAYER.wrapping_add(4 * owner));
                    out.push(Draw::fill(c, Rect::new(r.x0 + 2, r.y0 + 2, 18, 12)));
                    if i.hot {
                        hud::outline(out, r, c);
                    }
                    if !i.text.is_empty() {
                        out.push(Draw::game_text(WHITE, &i.text, r.x0 + 8, r.y0 + 12));
                    }
                }
                Look::Combo => {
                    let mut d = Dialog::default();
                    let id = d.add_combo_box_dlx(
                        ctx.lib,
                        r.x0,
                        r.y0,
                        r.width(),
                        COMBO,
                        0,
                        9,
                        menu_font::F16Y1,
                        menu_font::F16Y3,
                    );
                    if let Kind::ComboBox(c) = &mut d.widgets[id].kind {
                        c.lines = vec![i.text.clone()];
                    }
                    let mut drawn = Vec::new();
                    d.draw(ctx.lib, ctx.fonts, &mut drawn);
                    out.extend(drawn.into_iter().map(Draw::in_game));
                }
                Look::DropRow => {
                    out.push(Draw::fill(if i.hot { [90, 70, 20, 255] } else { [0, 0, 0, 255] }, r));
                    out.push(Draw::game_text(YELLOW, &i.text, r.x0 + 4, r.y0 + 2));
                }
                Look::Bar { pos, max } => {
                    let mut d = Dialog::default();
                    d.add_scroll_bar(ctx.lib, r.x0, r.y0, r.height(), max, pos, VSCROLL, 0);
                    let mut drawn = Vec::new();
                    d.draw(ctx.lib, ctx.fonts, &mut drawn);
                    out.extend(drawn.into_iter().map(Draw::in_game));
                }
                Look::Handle => {
                    hud::outline(out, r, colour(OUTLINE));
                    // Two columns of three dots to hold.
                    for (a, b) in (0..2).flat_map(|a| (0..3).map(move |b| (a, b))) {
                        out.push(Draw::fill(
                            colour(OUTLINE_PICKED),
                            Rect::new(r.x0 + 6 + 5 * a, r.y0 + 3 + 4 * b, 2, 2),
                        ));
                    }
                }
                Look::Minimise => {
                    hud::outline(out, r, colour(OUTLINE));
                    out.push(Draw::game_text(YELLOW, &i.text, r.x0 + 7, r.y0 + 2));
                }
                Look::Corner => {
                    // A triangle of dots in the corner.
                    for (a, b) in (0..3).flat_map(|a| (0..3 - a).map(move |b| (a, b))) {
                        out.push(Draw::fill(
                            colour(OUTLINE_PICKED),
                            Rect::new(r.x1 - 4 - 4 * a, r.y1 - 4 - 4 * b, 2, 2),
                        ));
                    }
                }
                Look::Text => {
                    if i.text.is_empty() {
                        continue;
                    }
                    if i.hot {
                        out.push(Draw::fill([90, 70, 20, 200], r));
                    }
                    let font = if i.act.is_some() { YELLOW } else { WHITE };
                    out.push(Draw::game_text(font, &i.text, r.x0 + 3, r.y0 + 2));
                }
            }
        }
    }

    /// The nations' alliance masks of the Kampf rule.
    pub fn masks(&self) -> [u8; 8] {
        sides::masks(self.sides, &self.team)
    }

    /// Applies the Kampf rule to the world (on start and after each placement).
    pub fn apply_sides(&self, world: &mut World) {
        sides::apply(world, self.sides, &self.team);
    }

    /// A click on window item `act`: what the window itself does, and the input for the
    /// world if the click changes it.
    pub fn apply(&mut self, world: &World, act: Act, local: u8) -> Out {
        let mut out = Out::default();
        if act != Act::Combo && !matches!(act, Act::Entry(_)) {
            self.combo_open = false;
        }
        let menu = self.menu.take();
        if !matches!(act, Act::Army(_)) {
            self.army.open = false;
        }
        if matches!(act, Act::Pick(_)) {
            self.army.armed = None;
        }
        match act {
            Act::Combo => self.combo_open = !self.combo_open,
            Act::Entry(e) => {
                self.combo_open = false;
                if self.entry != e {
                    self.entry = e;
                    self.scroll = 0;
                }
            }
            Act::Army(a) => self.army_act(a),
            Act::Battle(a) => out.input = self.battle_act(a),
            Act::Pick(p) => {
                self.pick = if self.pick == Some(p) { None } else { Some(p) };
                self.level = 0;
                self.form = match p {
                    Pick::Unit(k) => self.form.min(formations(world, k).len()),
                    _ => 0,
                };
            }
            Act::Owner(p) => self.owner = p,
            Act::SetLevel(l) => {
                if let Some(m) = menu {
                    self.pick = Some(Pick::Unit(m.kind));
                    self.level = l.min(levels::steps(world, m.kind));
                    self.form = self.form.min(formations(world, m.kind).len());
                }
            }
            Act::ScrollBy(d) => self.scroll_by(world, d),
            Act::Brush(d) => self.brush = (self.brush as i32 + d).clamp(0, BRUSHES.len() as i32 - 1) as usize,
            Act::Count(d) => self.count = (self.count as i32 + d).clamp(0, COUNTS.len() as i32 - 1) as usize,
            Act::Form(d) => {
                let n = self.unit().map_or(0, |k| formations(world, k).len()) as i32 + 1;
                self.form = (self.form as i32 + d).rem_euclid(n) as usize;
            }
            Act::Facing(d) => {
                let f = &mut self.facing[usize::from(self.owner)];
                *f = f.wrapping_add((d * 32) as u8);
            }
            Act::Peace => out.input = Some(Input::Sides { mode: self.sides.next(), teams: self.team }),
            Act::Reload => out.input = Some(Input::ReloadGuns),
            Act::AutoReload => out.input = Some(Input::AutoReload(!self.auto_reload)),
            Act::Control => out.local = Some(sides::next_controlled(world, local)),
            Act::Team(p) => {
                let mut teams = self.team;
                let t = &mut teams[usize::from(p) & 7];
                *t = sides::next_team(*t);
                out.input = Some(Input::Sides { mode: self.sides, teams });
            }
            Act::Hide => self.frame = self.frame.toggled(),
            Act::Grab(_) => {}
        }
        out
    }

    /// A fresh editor with its window at `frame` (the saved place).
    pub fn with_frame(frame: Frame) -> Self {
        Self { frame, ..Self::default() }
    }

    /// A press at canvas point `p` took hold of a grip with the right or left button.
    pub fn grab(&mut self, grab: Grab, right: bool, p: [f32; 2]) {
        self.drag = Some(Drag::new(grab, right, (p[0] as i32, p[1] as i32), self.frame));
    }

    /// The cursor is at canvas point `p`: a held grip moves or resizes the window. True while
    /// one is held (the move is used up).
    pub fn drag_to(&mut self, p: [f32; 2]) -> bool {
        let Some(drag) = self.drag else { return false };
        self.frame = drag.at((p[0] as i32, p[1] as i32));
        true
    }

    /// The button went up: lets go of a grip it held. True if it held one.
    pub fn release_grab(&mut self, right: bool) -> bool {
        if self.drag.is_some_and(|d| d.right == right) {
            self.drag = None;
            return true;
        }
        false
    }

    /// A grip is held (the setting is stored once it is let go).
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Scrolls the icons by `d` pixels, within the content.
    fn scroll_by(&mut self, world: &World, d: i32) {
        let (_, end) = flow(&groups(world, self.entry), self.frame.columns());
        self.scroll = (self.scroll + d).clamp(0, scroll_max(end, self.frame.visible_h()));
    }

    /// Scrolls the icons by wheel notches (one row each).
    pub fn wheel(&mut self, world: &World, lines: f32) {
        self.scroll_by(world, -(lines.round() as i32) * PITCH);
    }

    /// The input a left click on the map at world pixel `at` makes: the armed army, or the
    /// picked type ([`Editor::count`] units, loose or in formations of the chosen kind side
    /// by side across the facing). `None` when nothing is armed or picked.
    pub fn click_on_map(&self, world: &World, at: (i32, i32)) -> Option<Input> {
        if let Some(size) = self.army.armed {
            let era = self.army.era_for(self.entry);
            return Some(Input::Preset { nation: self.entry, size, era, owner: self.owner(), at, dir: self.facing() });
        }
        match self.pick? {
            Pick::Unit(kind) => Some(Input::Place {
                kind: world.catalog.get(kind).id.clone(),
                owner: self.owner(),
                at,
                count: self.count(),
                form: self.form as u32,
                dir: self.facing(),
                level: self.level as u32,
            }),
            Pick::Nature(group, index) => Some(Input::Nature { group, index, at }),
            Pick::Ground(tool) => Some(Input::Ground(self.dab(world, tool, at))),
        }
    }

    /// The dab of `tool` at `at` with the brush radius. Flatten takes the level of the
    /// ground there (the press of a stroke fixes it for the whole stroke).
    fn dab(&self, world: &World, tool: GroundTool, (x, y): (i32, i32)) -> Dab {
        let tool = match tool {
            GroundTool::Raise => Tool::Raise(edit::RAISE),
            GroundTool::Lower => Tool::Lower(edit::RAISE),
            GroundTool::Flatten => Tool::Flatten(world.map.heights.at(x, y).clamp(0, i32::from(i16::MAX)) as i16),
            GroundTool::Water => Tool::Water,
            GroundTool::Land => Tool::Land,
            GroundTool::Texture(t) => Tool::Texture(t),
        };
        Dab { x, y, radius: self.radius(), tool }
    }

    /// A press on the map starts a stroke when `input` is a dab.
    pub fn begin_stroke(&mut self, input: &Input) {
        if let Input::Ground(dab) = input {
            self.stroke = Some(Stroke { dab: *dab, since: 0.0 });
        }
    }

    /// Ends the stroke (the button went up); true if there was one.
    pub fn end_stroke(&mut self) -> bool {
        self.stroke.take().is_some()
    }

    /// Advances the stroke by `seconds` with the cursor at world pixel `at`: the next dab, if
    /// the cursor moved half a radius from the last one or [`STROKE_EVERY`] seconds passed.
    /// At most one dab per call (per frame).
    pub fn stroke_step(&mut self, seconds: f32, at: (i32, i32)) -> Option<Input> {
        let stroke = self.stroke.as_mut()?;
        stroke.since += seconds;
        let moved = i64::from(at.0 - stroke.dab.x).pow(2) + i64::from(at.1 - stroke.dab.y).pow(2);
        let step = i64::from(stroke.dab.radius / STROKE_STEP);
        if moved < step * step && stroke.since < STROKE_EVERY {
            return None;
        }
        stroke.since = 0.0;
        stroke.dab.x = at.0;
        stroke.dab.y = at.1;
        Some(Input::Ground(stroke.dab))
    }

    /// Notes that an edit changed what the view shows.
    pub fn mark_stale(&mut self, minimap: bool, ground: bool) {
        let s = self.stale.get_or_insert_with(Stale::default);
        s.minimap |= minimap;
        s.ground |= ground;
    }

    /// Takes what is to be rebuilt: all of it when `now` or the stroke is over or
    /// [`REFRESH_EVERY`] passed since the last time, else nothing.
    pub fn take_stale(&mut self, seconds: f32, now: bool) -> Option<Stale> {
        self.since_refresh += seconds;
        if self.stale.is_some() && (now || self.stroke.is_none() || self.since_refresh >= REFRESH_EVERY) {
            self.since_refresh = 0.0;
            return self.stale.take();
        }
        None
    }

    /// The picked type, if a unit, animal, cannon or building is picked.
    pub fn unit(&self) -> Option<TypeId> {
        match self.pick {
            Some(Pick::Unit(kind)) => Some(kind),
            _ => None,
        }
    }
}

/// What a click on the window did.
#[derive(Debug, Default)]
pub struct Out {
    /// A new local player ([`Act::Control`]).
    pub local: Option<u8>,
    /// The change to the world the click asks for.
    pub input: Option<Input>,
}

/// Spawns formations of `choice` holding `n` soldiers of `kind` (full formations of the
/// largest size, the rest in the smallest size that holds it), side by side across `dir`
/// around `at`, each with its officer, drummer and flag bearer. Returns the soldiers placed.
fn place_formations(
    world: &mut World,
    kind: TypeId,
    choice: &FormChoice,
    n: usize,
    owner: u8,
    at: (i32, i32),
    dir: u8,
) -> Result<usize, String> {
    let slots_of: Vec<(usize, usize)> =
        choice.shapes.iter().map(|&s| (s, world.formations.shapes[s].slots())).collect();
    let slots = |s: usize| slots_of.iter().find(|x| x.0 == s).map_or(0, |x| x.1);
    let mut shapes = choice.shapes.clone();
    shapes.sort_by_key(|&s| slots(s));
    let Some(&largest) = shapes.last() else { return Err("keine Formation".into()) };
    let mut sizes = Vec::new();
    let mut left = n;
    while left > 0 {
        let s = shapes.iter().copied().find(|&s| slots(s) >= left).unwrap_or(largest);
        sizes.push(s);
        left = left.saturating_sub(slots(s));
    }
    // Widths across the facing, from the shapes' slot positions (1/16 px).
    let width = |s: usize| {
        let p = world.formations.shapes[s].positions(0, 0, dir);
        let a = f64::from(dir) * std::f64::consts::TAU / 256.0;
        let across = |&(x, y): &(i32, i32)| f64::from(x) * -a.sin() + f64::from(y) * a.cos();
        let (lo, hi) = p.iter().map(across).fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
        if p.is_empty() { 0 } else { ((hi - lo) / 16.0) as i32 }
    };
    let widths: Vec<i32> = sizes.iter().map(|&s| width(s)).collect();
    let total: i32 = widths.iter().sum::<i32>() + FORMATION_GAP * (widths.len() as i32 - 1);
    let a = f64::from(dir) * std::f64::consts::TAU / 256.0;
    let (ax, ay) = (-a.sin(), a.cos());
    let officer_data = world.formations.officers.get(&choice.officer).cloned().unwrap_or_default();
    let mut offset = -f64::from(total) / 2.0;
    let mut placed = 0;
    for (&shape, &w) in sizes.iter().zip(&widths) {
        let mid = offset + f64::from(w) / 2.0;
        offset += f64::from(w + FORMATION_GAP);
        let c = (at.0 + (mid * ax).round() as i32, at.1 + (mid * ay).round() as i32);
        let p = Pos::from_pixels(c.0, c.1);
        let officer = world.spawn(choice.officer, owner, p, dir).0 as usize;
        let drummer = officer_data.drummer.map(|d| world.spawn(d, owner, p, dir).0 as usize);
        let flag = officer_data.flag.map(|f| world.spawn(f, owner, p, dir).0 as usize);
        let soldiers: Vec<usize> = (0..slots(shape)).map(|_| world.spawn(kind, owner, p, dir).0 as usize).collect();
        placed += soldiers.len();
        acr_sim::formation::create_placed(world, (officer, drummer, flag), &soldiers, shape, c, dir);
    }
    Ok(placed)
}

impl Item {
    /// Where a click hits: icons and headers only inside the clipped content.
    pub fn hit_rect(&self, content: Rect) -> Rect {
        match self.look {
            Look::Icon { .. } | Look::Texture { .. } | Look::Sprite { .. } | Look::Header => {
                self.rect.intersect(&content)
            }
            _ => self.rect,
        }
    }
}

/// The editor's hooks into the game screen.
impl super::InGame {
    /// The `ICONS.GP` frames of the catalog's types, cached in the editor once the `.MD`
    /// definitions and the icon list are loaded.
    fn editor_icons_ready(&self, ctx: &Ctx, e: &Editor, world: &World) {
        use crate::hud::data::HudData;
        if e.icons.borrow().is_some() {
            return;
        }
        ctx.lib.want(HudData::ICON_LIST);
        ctx.lib.want(HudData::NRES);
        let (Some(list), Some(nres)) = (ctx.lib.bytes(HudData::ICON_LIST), ctx.lib.bytes(HudData::NRES)) else {
            return;
        };
        let data = HudData::parse(&list, &nres);
        let objects = self.objects.objects();
        let map: HashMap<TypeId, Option<usize>> = world
            .catalog
            .iter()
            .map(|(id, t)| (id, objects.definition(&t.id).and_then(|d| d.icon()).and_then(|n| data.icon(n))))
            .collect();
        if map.values().any(Option::is_some) {
            *e.icons.borrow_mut() = Some(map);
        }
    }

    /// The window's items for the current state, or none outside the editor.
    fn editor_items(&self, ctx: &Ctx) -> Vec<Item> {
        let (Some(e), Some(world)) = (&self.editor, &self.world) else { return Vec::new() };
        self.editor_icons_ready(ctx, e, world);
        let names = |k: TypeId| {
            let t = world.catalog.get(k);
            let mut n = super::interface::unit_name(ctx, &t.md_name);
            if n.is_empty() || n == t.md_name.as_bytes() {
                return t.id.as_bytes().to_vec();
            }
            n.extend_from_slice(format!(" ({})", t.id).as_bytes());
            n
        };
        let icon = |k: TypeId| e.icons.borrow().as_ref().and_then(|m| m.get(&k).copied().flatten());
        // A text the game does not have comes back as its own id: no text then.
        let upgrade_name = |u: acr_sim::tech::UpgradeId| {
            let id = &world.catalog.tech.upgrade(u).message;
            let text = ctx.texts.get(id);
            if text == id.as_bytes() { Vec::new() } else { text.to_vec() }
        };
        let sprite = |group: SpriteGroup, index: u16| {
            let entry = self.objects.objects().list(group)?.entries.get(usize::from(index))?;
            Some((self.objects.objects().list(group)?.gp_path(), entry.dx, entry.dy))
        };
        let texture_colour = |t: u8| {
            let assets = self.assets.as_ref()?;
            Some(assets.palette.0[usize::from(assets.minimap_color(t, 3))])
        };
        let view = View {
            names: &names,
            icon: &icon,
            upgrade_name: &upgrade_name,
            sprite: &sprite,
            texture_colour: &texture_colour,
            title: ctx.texts.get("ED_HDR01").to_vec(),
        };
        e.layout(world, &view, self.local_player())
    }

    pub(super) fn editor_overlay(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        if let Some(e) = &self.editor {
            Editor::draws(&self.editor_items(ctx), ctx, e.frame.content(), out);
        }
    }

    /// The window's clickable items for the test driver (`ui`, `click`), in window pixels.
    pub fn editor_icons(&self, ctx: &Ctx) -> Vec<super::probe::HudIcon> {
        let canvas = self.interface.canvas;
        let content = self.editor.as_ref().map_or(Rect::SCREEN, |e| e.frame.content());
        self.editor_items(ctx)
            .iter()
            .enumerate()
            .filter(|(_, i)| i.act.is_some())
            .map(|(n, i)| {
                let r = i.hit_rect(content);
                super::probe::HudIcon {
                    id: format!("editor/{n}"),
                    rect: canvas.to_window([r.x0 as f32, r.y0 as f32, r.width() as f32, r.height() as f32]),
                    hint: i.name.clone().into_bytes(),
                    disabled: false,
                    selected: i.hot,
                    gray: false,
                    count: None,
                    number: None,
                    clickable: !r.is_empty(),
                    right_clickable: matches!(i.act, Some(Act::Pick(Pick::Unit(_)))) || i.look == Look::Combo,
                }
            })
            .collect()
    }

    /// A click on the window: its own effect, and the input for the world, if any, applied
    /// and logged. Returns a new local player.
    fn editor_act(&mut self, act: Act, local: u8) -> Option<u8> {
        let (Some(e), Some(world)) = (&mut self.editor, &self.world) else { return None };
        let out = e.apply(world, act, local);
        if let Some(input) = out.input {
            self.editor_input(input);
        }
        out.local
    }

    /// Applies an editor input between two ticks and logs it with the tick, for savegames.
    pub(super) fn editor_input(&mut self, input: Input) {
        let (Some(e), Some(world), Some(map)) = (&mut self.editor, &mut self.world, &mut self.map) else { return };
        self.save.log.editor(world.tick.0, input.clone());
        let applied = input::apply(world, map, e, &input);
        e.status = applied.status;
        if let Some(area) = applied.ground {
            // The render data follows each dab; the GPU copy goes at the next `sync_gpu`.
            if let (Some(terrain), Some(assets)) = (&mut self.terrain, &self.assets)
                && let Err(why) = terrain.update(map, assets, area)
            {
                self.failed = Some(format!("terrain: {why}"));
            }
            e.mark_stale(true, true);
        }
        if applied.objects {
            // A tree is no dot of the minimap.
            let tree = matches!(input, Input::Nature { group: SpriteGroup::Trees, .. });
            e.mark_stale(!tree, false);
        }
        self.refresh_edited_view(0.0);
    }

    /// Builds the view of the edited map again, as far as it is stale: the scene (objects stand
    /// on the new ground), the minimap and the 3D map models and ground cover. During a stroke
    /// at most every [`REFRESH_EVERY`] seconds, otherwise at once. The water draws from the
    /// terrain data's revision and the GPU copy goes with `sync_gpu`.
    pub(super) fn refresh_edited_view(&mut self, seconds: f32) {
        let Some(e) = &mut self.editor else { return };
        let Some(stale) = e.take_stale(seconds, false) else { return };
        let (Some(map), Some(terrain)) = (&self.map, &self.terrain) else { return };
        self.objects.rebuild_static(map, terrain);
        if stale.minimap {
            self.rebuild_minimap();
        }
        if stale.ground {
            self.three_d.terrain_edited();
        }
    }

    fn rebuild_minimap(&mut self) {
        if let Some(map) = &self.map
            && let Some(assets) = &self.assets
            && let Some(m) = crate::hud::minimap::Minimap::new(map, assets)
        {
            self.interface.hud.set_minimap(m);
        }
    }

    /// The map was edited behind the view's back (a loaded game's replay): the terrain, the
    /// GPU copy, the scene, the minimap and the models are made from the map again.
    pub(super) fn rebuild_after_edits(&mut self) {
        if let (Some(assets), Some(map)) = (&self.assets, &self.map) {
            match TerrainData::new(map, assets) {
                Ok(t) => {
                    self.terrain = Some(t);
                    self.uploaded = false;
                }
                Err(e) => self.failed = Some(format!("terrain: {e}")),
            }
        }
        if let (Some(map), Some(terrain)) = (&self.map, &self.terrain) {
            self.objects.rebuild_static(map, terrain);
        }
        self.rebuild_minimap();
        self.three_d.terrain_edited();
    }

    /// Advances a terrain stroke by a frame: the next dab where the cursor is, if due, and the
    /// throttled rebuild of the view.
    pub(super) fn editor_stroke(&mut self, seconds: f32) {
        let at = self.cursor.map(|c| self.world_at(c));
        let input = match (&mut self.editor, at) {
            (Some(e), Some(at)) => e.stroke_step(seconds, at),
            _ => None,
        };
        if let Some(input) = input {
            self.editor_input(input);
        }
        self.refresh_edited_view(seconds);
    }

    /// The left button in the editor: the window, or placing the picked type on the map.
    /// True when the press (or its release) is used up.
    pub(super) fn editor_left(&mut self, ctx: &Ctx, pressed: bool) -> bool {
        if self.editor.is_none() {
            return false;
        }
        if !pressed {
            let e = self.editor.as_mut().expect("editor");
            let stroked = e.end_stroke();
            e.release_grab(false);
            let swallow = std::mem::take(&mut e.swallow_release);
            if stroked {
                // The stroke's last changes show at once.
                self.refresh_edited_view(0.0);
            }
            return swallow;
        }
        let Some(at) = self.cursor else { return false };
        let p = self.canvas_point(at);
        let items = self.editor_items(ctx);
        let content = self.editor.as_ref().map_or(Rect::SCREEN, |e| e.frame.content());
        // A press outside the level drop-down closes it; one on the map does no more.
        let on_menu = items.iter().any(|i| i.look == Look::DropRow && i.rect.contains(p));
        let closed = self.editor.as_mut().is_some_and(|e| !on_menu && e.close_levels());
        if closed && !Editor::bounds(&items).contains(p) {
            self.editor.as_mut().expect("editor").swallow_release = true;
            return true;
        }
        if Editor::bounds(&items).contains(p) || items.iter().any(|i| i.look == Look::DropRow && i.rect.contains(p)) {
            let act =
                items.iter().rev().find(|i| i.act.is_some() && i.hit_rect(content).contains(p)).and_then(|i| i.act);
            if let Some(Act::Grab(grab)) = act {
                self.editor.as_mut().expect("editor").grab(grab, false, p);
            }
            let local = self.local_player();
            if let Some(act) = act
                && let Some(next) = self.editor_act(act, local)
            {
                self.setup.local_player = usize::from(next);
                self.selected.clear();
            }
            let e = self.editor.as_mut().expect("editor");
            e.swallow_release = true;
            let orders = std::mem::take(&mut e.orders);
            let camera = e.camera_to.take();
            self.commands.extend(orders);
            if let Some((x, y)) = camera {
                self.set_camera(x, y, 0.0, None, None);
            }
            return true;
        }
        let armed = self.editor.as_ref().is_some_and(|e| e.pick.is_some() || e.army.armed.is_some());
        if armed && !self.over_hud(ctx, at) {
            let target = self.world_at(at);
            let input = match (&self.editor, &self.world) {
                (Some(e), Some(world)) => e.click_on_map(world, target),
                _ => None,
            };
            if let Some(e) = &mut self.editor {
                e.swallow_release = true;
            }
            if let Some(input) = input {
                self.editor_input(input.clone());
                if let Some(e) = &mut self.editor {
                    e.begin_stroke(&input);
                }
            }
            return true;
        }
        false
    }

    /// The right button in the editor: on an icon, opens its level drop-down; on the nation
    /// combo box it opens the army menu; elsewhere closes the drop-down or menu, or stops placing.
    pub(super) fn editor_right(&mut self, ctx: &Ctx, pressed: bool) -> bool {
        let Some(e) = &mut self.editor else { return false };
        if !pressed {
            return e.release_grab(true);
        }
        let (dx, dy) = e.frame.shift();
        let content = e.frame.content();
        let combo = COMBO_BOX.offset(dx, dy);
        let over_combo = self.cursor.is_some_and(|c| combo.contains(self.canvas_point(c)));
        let local = self.local_player();
        if over_combo && self.editor.as_ref().is_some_and(|e| !e.frame.folded) {
            self.editor_act(Act::Army(presets::Act::Toggle), local);
            return true;
        }
        let hit = self.cursor.map(|at| self.canvas_point(at)).and_then(|p| {
            let items = self.editor_items(ctx);
            items.into_iter().rev().find(|i| i.act.is_some() && i.hit_rect(content).contains(p))
        });
        // The handle or corner grip held with the right button (a touch long press) drags too.
        if let Some(Item { act: Some(Act::Grab(grab)), .. }) = hit
            && let Some(at) = self.cursor
        {
            let p = self.canvas_point(at);
            if let Some(e) = &mut self.editor {
                e.grab(grab, true, p);
                e.swallow_release = true;
            }
            return true;
        }
        // A colour's swatch cycles its team while teams fight.
        if let Some(Item { look: Look::Swatch { owner }, .. }) = hit
            && owner != NEUTRAL
            && self.editor.as_ref().is_some_and(|e| e.sides == sides::Mode::Teams)
        {
            self.editor_act(Act::Team(owner), 0);
            return true;
        }
        let icon = match hit {
            Some(Item { act: Some(Act::Pick(Pick::Unit(k))), rect, .. }) => Some((k, rect)),
            _ => None,
        };
        let e = self.editor.as_mut().expect("editor");
        if let Some((kind, cell)) = icon {
            e.open_levels(kind, cell);
            return true;
        }
        if e.close_levels() || std::mem::take(&mut e.army.open) {
            return true;
        }
        if e.pick.is_some() || e.army.armed.is_some() {
            e.pick = None;
            e.army.armed = None;
            return true;
        }
        false
    }

    /// The wheel over the window scrolls its icons.
    pub fn editor_wheel(&mut self, ctx: &Ctx, lines: f32) -> bool {
        let Some(at) = self.cursor else { return false };
        let p = self.canvas_point(at);
        if !Editor::bounds(&self.editor_items(ctx)).contains(p) {
            return false;
        }
        if let (Some(e), Some(world)) = (&mut self.editor, &self.world) {
            e.close_levels();
            e.wheel(world, lines);
        }
        true
    }

    /// F2 folds the window; Escape closes the level drop-down.
    pub(super) fn editor_key(&mut self, key: &winit::keyboard::Key, pressed: bool) -> bool {
        match (&mut self.editor, key) {
            (Some(e), winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)) if e.menu_open() => {
                if pressed {
                    e.close_levels();
                }
                true
            }
            (Some(e), winit::keyboard::Key::Named(winit::keyboard::NamedKey::F2)) => {
                if pressed {
                    e.frame = e.frame.toggled();
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use acr_sim::map::SpriteShapes;
    use acr_sim::sprite::{ObjChar, SpriteKinds};

    use super::*;

    fn kinds(names: &[&str]) -> SpriteKinds {
        let chars = names
            .iter()
            .map(|_| ObjChar {
                res_type: None,
                per_work: 0,
                radius: 32,
                work_amount: 0,
                work_next: None,
                time_amount: 0,
                time_next: None,
                deposit: false,
                list_radius: 0,
            })
            .collect();
        SpriteKinds { chars, names: names.iter().map(|n| (*n).to_owned()).collect() }
    }

    /// A world with two trees (a wood source and a bush) and three stones (a rock, a rock and
    /// a deposit), and no game files.
    pub(in crate::ingame::editor) fn nature_world() -> World {
        use acr_sim::formation::Formations;
        let mut shapes = SpriteShapes::default();
        shapes.kinds.insert(SpriteGroup::Trees, kinds(&["AG1", "AG2"]));
        shapes.kinds.insert(SpriteGroup::Stones, kinds(&["ST1", "ST2", "ST3"]));
        shapes.work.insert(SpriteGroup::Trees, vec![Some((WOOD as u8, 4, 30)), None]);
        shapes.sources.insert(SpriteGroup::Stones, vec![None, None, Some(1)]);
        let mut world = World::new(
            std::sync::Arc::new(acr_sim::MapGrid::open(1)),
            std::sync::Arc::new(acr_sim::catalog::Catalog::new()),
            std::sync::Arc::new(Formations::default()),
        );
        world.shapes = std::sync::Arc::new(shapes);
        world
    }

    #[test]
    fn natur_lists_trees_bushes_stones_deposits() {
        let world = nature_world();
        let groups = groups(&world, NATUR);
        let pick = |g, i| Pick::Nature(g, i);
        assert_eq!(
            groups,
            vec![
                Group { header: "Baeume", kinds: vec![pick(SpriteGroup::Trees, 0)] },
                Group { header: "Buesche", kinds: vec![pick(SpriteGroup::Trees, 1)] },
                Group { header: "Steine", kinds: vec![pick(SpriteGroup::Stones, 0), pick(SpriteGroup::Stones, 1)] },
                Group { header: "Vorkommen", kinds: vec![pick(SpriteGroup::Stones, 2)] },
            ]
        );
        // A world without sprite lists has an empty list.
        let mut bare = nature_world();
        bare.shapes = Default::default();
        assert!(super::groups(&bare, NATUR).is_empty());

        // The cells carry the sprite of the pack, or the name where there is none.
        let sprite = |g: SpriteGroup, i: u16| (i < 2).then(|| (format!("{g:?}.GP"), 5 + i32::from(i), 9));
        let cells = |e: &Editor, sprite: &dyn Fn(SpriteGroup, u16) -> Option<(String, i32, i32)>| -> Vec<Item> {
            let view = View {
                names: &|_| Vec::new(),
                icon: &|_| None,
                upgrade_name: &|_| Vec::new(),
                sprite,
                texture_colour: &|_| None,
                title: Vec::new(),
            };
            e.layout(&world, &view, 0).into_iter().filter(|i| matches!(i.act, Some(Act::Pick(_)))).collect()
        };
        let mut e = Editor { entry: NATUR, ..Editor::default() };
        let items = cells(&e, &sprite);
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].name, "AG1 [AG 0]");
        assert_eq!(items[4].name, "ST3 [ST 2]");
        assert_eq!(items[1].look, Look::Sprite { frame: 1, ox: 6, oy: 9 });
        assert_eq!(items[1].gp.as_deref(), Some("Trees.GP"));
        assert_eq!(items[4].look, Look::Icon { frame: None }, "no sprite: the name");
        assert_eq!(items[4].gp, None);
        // Picking a cell, then the map: the input names the type and the place.
        e.apply(&world, Act::Pick(Pick::Nature(SpriteGroup::Stones, 2)), 0);
        assert_eq!(
            e.click_on_map(&world, (700, 800)),
            Some(Input::Nature { group: SpriteGroup::Stones, index: 2, at: (700, 800) })
        );
        assert_eq!(e.unit(), None);
    }

    #[test]
    fn entries_end_with_natur_and_gelaende() {
        assert_eq!(ENTRIES[ENTRIES.len() - 2], ("Natur", ""));
        assert_eq!(ENTRIES[ENTRIES.len() - 1], ("Gelaende", ""));
        assert_eq!((NATUR, GELAENDE), (ENTRIES.len() - 2, ENTRIES.len() - 1));
    }

    #[test]
    fn gelaende_lists_the_tools_and_the_textures() {
        let world = nature_world();
        let groups = groups(&world, GELAENDE);
        assert_eq!(
            groups.iter().map(|g| (g.header, g.kinds.len())).collect::<Vec<_>>(),
            [("Werkzeug", 5), ("Boden", 15)]
        );
        assert_eq!(groups[0].kinds[0], Pick::Ground(GroundTool::Raise));
        assert_eq!(groups[1].kinds[2], Pick::Ground(GroundTool::Texture(71)));
        assert_eq!(ground_name(GroundTool::Texture(71)), "Erde");
        assert_eq!(ground_name(GroundTool::Flatten), "Ebnen");

        let cells = |e: &Editor, colour: bool| {
            let view = View {
                names: &|_| Vec::new(),
                icon: &|_| None,
                upgrade_name: &|_| Vec::new(),
                sprite: &|_, _| None,
                texture_colour: &|t| colour.then_some([t, 0, 0]),
                title: Vec::new(),
            };
            e.layout(&world, &view, 0)
        };
        let mut e = Editor { entry: GELAENDE, ..Editor::default() };
        let items = cells(&e, true);
        let find = |name: &str| items.iter().find(|i| i.name == name).expect(name).clone();
        assert_eq!(find("Anheben").look, Look::Icon { frame: None });
        assert_eq!(find("Erde").look, Look::Texture { colour: [71, 0, 0] });
        assert_eq!(find("Erde").act, Some(Act::Pick(Pick::Ground(GroundTool::Texture(71)))));
        assert_eq!(find("Pinsel").text, b"Pinsel: 128");
        assert_eq!(
            cells(&e, false).iter().find(|i| i.name == "Erde").map(|i| i.look),
            Some(Look::Icon { frame: None })
        );

        // The brush steps through its radii and stops at the ends; other lists have no Pinsel line.
        for (d, radius) in [(1, 256), (1, 512), (1, 512), (-1, 256)] {
            e.apply(&world, Act::Brush(d), 0);
            assert_eq!(e.radius(), radius);
        }
        e.entry = NATUR;
        assert!(cells(&e, true).iter().all(|i| i.name != "Pinsel"));
    }

    #[test]
    fn a_ground_click_makes_a_dab_of_the_brush_and_flatten_takes_the_level_there() {
        let mut world = nature_world();
        std::sync::Arc::make_mut(&mut world.map).heights.cells.fill(40);
        let mut e = Editor::default();
        e.apply(&world, Act::Brush(-1), 0);
        e.apply(&world, Act::Pick(Pick::Ground(GroundTool::Raise)), 0);
        assert_eq!(
            e.click_on_map(&world, (900, 1000)),
            Some(Input::Ground(Dab { x: 900, y: 1000, radius: 64, tool: Tool::Raise(edit::RAISE) }))
        );
        e.apply(&world, Act::Pick(Pick::Ground(GroundTool::Flatten)), 0);
        let Some(Input::Ground(Dab { tool: Tool::Flatten(level), .. })) = e.click_on_map(&world, (900, 1000)) else {
            panic!("a Flatten dab")
        };
        assert_eq!(level, 40);
        e.apply(&world, Act::Pick(Pick::Ground(GroundTool::Texture(71))), 0);
        assert!(matches!(e.click_on_map(&world, (1, 2)), Some(Input::Ground(Dab { tool: Tool::Texture(71), .. }))));
        assert_eq!(e.unit(), None);
    }

    #[test]
    fn a_stroke_dabs_when_the_cursor_moved_half_a_radius_or_a_tenth_of_a_second_passed() {
        let mut world = nature_world();
        std::sync::Arc::make_mut(&mut world.map).heights.cells.fill(40);
        let mut e = Editor::default();
        e.apply(&world, Act::Pick(Pick::Ground(GroundTool::Flatten)), 0);
        let press = e.click_on_map(&world, (1000, 1000)).expect("a dab");
        e.begin_stroke(&press);
        let dab = |x, y| Some(Input::Ground(Dab { x, y, radius: 128, tool: Tool::Flatten(40) }));
        assert_eq!(e.stroke_step(0.016, (1000, 1000)), None, "still, too soon");
        assert_eq!(e.stroke_step(0.016, (1050, 1000)), None, "50 px of the 64 needed");
        assert_eq!(e.stroke_step(0.016, (1064, 1000)), dab(1064, 1000), "half a radius");
        assert_eq!(e.stroke_step(0.016, (1064, 1000)), None, "the clock starts again");
        // The level is the press's, whatever the ground is now.
        std::sync::Arc::make_mut(&mut world.map).heights.cells.fill(90);
        for _ in 0..5 {
            assert_eq!(e.stroke_step(0.016, (1064, 1000)), None);
        }
        assert_eq!(e.stroke_step(0.03, (1064, 1000)), dab(1064, 1000), "a tenth of a second, standing still");
        assert!(e.end_stroke());
        assert_eq!(e.stroke_step(1.0, (0, 0)), None, "no stroke after the release");
        assert!(!e.end_stroke());
        // Only a dab starts a stroke.
        e.begin_stroke(&Input::ReloadGuns);
        assert_eq!(e.stroke_step(1.0, (0, 0)), None);
    }

    #[test]
    fn the_view_is_rebuilt_at_once_outside_a_stroke_and_every_quarter_second_in_one() {
        let mut e = Editor::default();
        assert_eq!(e.take_stale(1.0, false), None, "nothing stale");
        e.mark_stale(false, true);
        assert_eq!(e.take_stale(0.0, false), Some(Stale { minimap: false, ground: true }), "no stroke: at once");
        e.begin_stroke(&Input::Ground(Dab { x: 0, y: 0, radius: 64, tool: Tool::Land }));
        e.mark_stale(true, false);
        assert_eq!(e.take_stale(0.1, false), None, "a stroke waits");
        e.mark_stale(false, true);
        assert_eq!(e.take_stale(0.1, false), None);
        assert_eq!(e.take_stale(0.1, false), Some(Stale { minimap: true, ground: true }), "a quarter second on");
        e.mark_stale(true, true);
        assert_eq!(e.take_stale(0.0, false), None, "just rebuilt");
        e.end_stroke();
        assert!(e.take_stale(0.0, false).is_some(), "the end of the stroke shows the rest");
    }

    #[test]
    fn a_grid_is_centred_and_spaced() {
        let p = grid(9, (1000, 1000), 0, 40);
        assert_eq!(p.len(), 9);
        let (sx, sy) = p.iter().fold((0, 0), |(a, b), q| (a + q.0, b + q.1));
        assert_eq!((sx / 9, sy / 9), (1000, 1000));
        assert!(p.windows(2).any(|w| (w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs() == 40));
    }

    #[test]
    fn counts_step_and_clamp() {
        let mut e = Editor::default();
        assert_eq!(e.count(), 36);
        e.count = 0;
        e.count = (e.count as i32 - 1).clamp(0, COUNTS.len() as i32 - 1) as usize;
        assert_eq!(e.count(), 1);
        assert!(COUNTS.contains(&196));
    }

    #[test]
    fn the_grid_flows_in_rows_of_five_under_headers() {
        let g = |n: u16| Group { header: "x", kinds: (0..n).map(|i| Pick::Unit(TypeId(i))).collect() };
        let (placed, end) = flow(&[g(6), g(1)], COLUMNS);
        // Header at 80, icons from 100 (two rows), the next header 2 rows on, its icon 20 later.
        assert_eq!(placed[0], Placed::Header(0, 80));
        assert_eq!(placed[1], Placed::Cell(0, 0, 0, 100));
        assert_eq!(placed[5], Placed::Cell(0, 4, 4, 100));
        assert_eq!(placed[6], Placed::Cell(0, 5, 0, 144));
        assert_eq!(placed[7], Placed::Header(1, 188));
        assert_eq!(placed[8], Placed::Cell(1, 0, 0, 208));
        assert_eq!(end, 252);
        assert_eq!(flow(&[], COLUMNS).1, FIRST_ROW);
    }

    #[test]
    fn the_scroll_range_is_the_content_beyond_the_visible_height() {
        assert_eq!(scroll_max(0x17e, VISIBLE_H), 0);
        assert_eq!(scroll_max(0x17e + 44, VISIBLE_H), 44);
        assert_eq!(scroll_max(100, VISIBLE_H), 0);
    }

    #[test]
    fn the_window_sits_in_the_side_panel_column() {
        // `RealLx − 269`, `InfY1 + 31` (0x5bb735, 0x5bb743) and the box of 0x5bb766.
        assert_eq!((OX, OY), (755, 79));
        assert_eq!((frame::DEFAULT.x1(), BOX_Y0, BOX_Y1, DIVIDER), (1004, 48, 465, 153));
        assert_eq!(ENTRIES[FIRST_EXTRA].0, "Tiere");
        assert!(ENTRIES.len() == FIRST_EXTRA + 4 && DEFAULT_ENTRY < FIRST_EXTRA);
    }

    /// The window's items on the nature list (six cells, in a world without icons).
    fn natur_items(e: &Editor) -> Vec<Item> {
        let view = View {
            names: &|_| Vec::new(),
            icon: &|_| None,
            upgrade_name: &|_| Vec::new(),
            sprite: &|_, _| None,
            texture_colour: &|_| None,
            title: b"Einheit".to_vec(),
        };
        e.layout(&nature_world(), &view, 0)
    }

    fn named<'a>(items: &'a [Item], name: &str) -> &'a Item {
        items.iter().find(|i| i.name == name).unwrap_or_else(|| panic!("no item {name}"))
    }

    #[test]
    fn a_moved_window_moves_every_item_and_its_hit_areas() {
        let mut e = Editor { entry: NATUR, ..Editor::default() };
        let before = natur_items(&e);
        e.frame = Frame { x: 100, y: 200, ..frame::DEFAULT };
        let after = natur_items(&e);
        assert_eq!(before.len(), after.len());
        let (dx, dy) = (100 - 755, 200 - 48);
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(b.rect.offset(dx, dy), a.rect, "{}", b.name);
        }
        // A click maps to the moved rectangles only.
        let content = e.frame.content();
        let cell = after.iter().find(|i| matches!(i.act, Some(Act::Pick(_)))).unwrap();
        let at = [cell.rect.x0 as f32 + 5.0, cell.rect.y0 as f32 + 5.0];
        assert!(cell.hit_rect(content).contains(at));
        assert!(!before.iter().filter(|i| i.act.is_some()).any(|i| i.hit_rect(frame::DEFAULT.content()).contains(at)));
        assert_eq!(Editor::bounds(&after), before_bounds(&before).offset(dx, dy));
    }

    fn before_bounds(items: &[Item]) -> Rect {
        Editor::bounds(items)
    }

    #[test]
    fn a_wider_window_has_more_columns_and_a_taller_one_a_longer_bar() {
        let g = |n: u16| Group { header: "x", kinds: (0..n).map(|i| Pick::Unit(TypeId(i))).collect() };
        let wide = Frame { x: 300, w: 249 + 44, ..frame::DEFAULT };
        assert_eq!(wide.columns(), 6);
        let (placed, end) = flow(&[g(7)], wide.columns());
        // Six to a row, the seventh below; five columns need two rows too, but sixteen do not.
        assert_eq!((placed[6], placed[7]), (Placed::Cell(0, 5, 5, 100), Placed::Cell(0, 6, 0, 144)));
        assert_eq!(end, 188);
        assert_eq!(flow(&[g(12)], wide.columns()).1, 80 + 20 + 2 * 44);
        assert_eq!(flow(&[g(12)], 5).1, 80 + 20 + 3 * 44);
        // The extras box follows the size (the scroll bar: tests/e2e/editor_window_move.json).
        let mut e = Editor {
            entry: 0,
            frame: Frame { x: 300, w: 249 + 88, h: 417 + 60, ..frame::DEFAULT },
            ..Editor::default()
        };
        let items = natur_items(&e);
        let extras = items.iter().find(|i| matches!(i.look, Look::Frame { divider: None })).unwrap();
        assert_eq!((extras.rect.x0, extras.rect.x1), (300, 300 + 337 + 1));
        assert_eq!(extras.rect.y0, 48 + 417 + 60 + 1);
        assert_eq!(named(&items, "Anzahl +").rect.x1, 300 + 8 + (337 - 16));
        e.frame.h = 417;
        assert!(natur_items(&e).iter().all(|i| i.rect.y1 <= 768));
    }

    #[test]
    fn the_minimise_button_folds_to_the_title_row_and_back() {
        let mut e = Editor::default();
        let world = nature_world();
        let open = natur_items(&e);
        assert!(open.iter().any(|i| i.look == Look::Corner));
        let out = e.apply(&world, named(&open, "Editor minimieren").act.unwrap(), 0);
        assert!(out.input.is_none() && e.frame.folded);
        let folded = natur_items(&e);
        let names: Vec<_> = folded.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Editor", "Editor verschieben", "Editor minimieren"]);
        assert!(Editor::bounds(&folded).height() <= ROW + 1);
        // The same button, at the same place, opens it again.
        assert_eq!(named(&folded, "Editor minimieren").rect, named(&open, "Editor minimieren").rect);
        e.apply(&world, Act::Hide, 0);
        assert!(!e.frame.folded);
    }

    #[test]
    fn dragging_the_handle_or_the_grip_moves_or_resizes_with_either_button() {
        let mut e = Editor::default();
        let items = natur_items(&e);
        let handle = named(&items, "Editor verschieben");
        assert_eq!(handle.act, Some(Act::Grab(Grab::Move)));
        let start = [handle.rect.x0 as f32 + 3.0, handle.rect.y0 as f32 + 3.0];
        e.grab(Grab::Move, false, start);
        assert!(e.drag_to([start[0] - 300.0, start[1] + 40.0]));
        assert_eq!((e.frame.x, e.frame.y), (455, 88));
        // A right button's release does not let go of a left drag.
        assert!(!e.release_grab(true) && e.dragging());
        assert!(e.release_grab(false) && !e.dragging());
        assert!(!e.drag_to([0.0, 0.0]));
        // The grip, taken with the right button (a touch long press), widens it by a column.
        let grip = named(&natur_items(&e), "Editor Groesse").rect;
        let at = [grip.x0 as f32 + 2.0, grip.y0 as f32 + 2.0];
        e.grab(Grab::Resize, true, at);
        e.drag_to([at[0] + 44.0, at[1]]);
        assert_eq!((e.frame.w, e.frame.columns()), (293, 6));
        assert!(e.release_grab(true));
    }
}
