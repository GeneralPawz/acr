//! The test editor (main menu "Editor", docs/controls.md, "Editor"): a blank field where
//! units are placed directly, any number at once, loose or already standing in a formation,
//! with fighting switched off while the battle is set up. Not in the original, whose editor
//! places single units only; everything here acts on the local world directly, like the
//! test driver's `spawn`, so an editor game is not a network or replay game.
//!
//! The window is the original's "Einheit wählen" (`ED_HDR01`, built by 0x5bb6c0, docs/ui.md,
//! "Editor"): the side panel's column with a title, the eight player colours, a nation combo
//! box and a scrolled grid of unit icons under category headers. ACR's own controls (count,
//! formation, upgrade level, facing, fighting, cannon switches, the player to control) sit in
//! a second box directly under it.

mod levels;
pub mod sides;

use std::cell::RefCell;
use std::collections::HashMap;

use acr_sim::catalog::{Medium, MotionStyle};
use acr_sim::fixed::Pos;
use acr_sim::tech::UpgradeId;
use acr_sim::{TypeId, World};

use crate::hud::{self, SCREEN, panels};
use crate::ui::Ctx;
use crate::ui::dialog::{Dialog, Kind};
use crate::ui::draw::{Draw, GAME_PALETTE, Rect};
use crate::ui::fonts::FontId;
use crate::ui::screens::font as menu_font;

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
const BOX_X1: i32 = OX + 249;
const BOX_Y0: i32 = OY - 31;
const BOX_Y1: i32 = OY + 386;
const DIVIDER: i32 = OY + 74;
/// Corner table 0x668640 of the box above the divider (style 3, 0x48039d); below it the unit
/// panel's paper (0x6686a0), both with fill 0x1c.
const HEADER_CORNERS: [i32; 8] = [17, 18, -1, -1, 12, 14, 23, -1];
const PAPER_FILL: usize = 28;
/// The clipped content (`addClipper(DS.x + 4, DS.y + 75, RealLx, DS.y + 386)`).
const CONTENT: Rect = Rect { x0: OX + 4, y0: OY + 75, x1: BOX_X1, y1: BOX_Y1 };
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
/// The counts offered; a count above one formation's size places several side by side.
pub const COUNTS: [u32; 12] = [1, 5, 10, 15, 36, 72, 120, 196, 392, 588, 784, 980];
/// The combo box's lines: the playable nations in the order of `TEXT\NATIONS.TXT` (name, and
/// the prefix of their type ids), then ACR's two extra lists, the animals and the cannons.
pub const ENTRIES: [(&str, &str); 14] = [
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
];
/// The first of [`ENTRIES`] that is ACR's own (not a nation of the original).
const FIRST_EXTRA: usize = 12;
/// The nation list opens on England, as the first game does for most players.
const DEFAULT_ENTRY: usize = 2;
/// The owner of wild animals: the eighth colour of the window is this neutral player.
pub const NEUTRAL: u8 = 7;
/// Compass names of the eight facings, `dir` = 32 · i (0 east, 64 south).
const FACINGS: [&str; 8] = ["Ost", "Suedost", "Sued", "Suedwest", "West", "Nordwest", "Nord", "Nordost"];
/// Space between formations placed side by side, world pixels.
const FORMATION_GAP: i32 = 64;
/// The categories of a nation's list (the `ED_` headers of `TEXT\EDITOR.TXT`); the original
/// also has marine and buildings, which this editor cannot place yet.
const CATEGORIES: [&str; 3] = ["Infanterie", "Kavallerie", "Artillerie"];

/// A click target of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Opens or closes the nation combo box.
    Combo,
    /// Picks a line of the combo box.
    Entry(usize),
    Pick(TypeId),
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
    Peace,
    Reload,
    AutoReload,
    Control,
    Hide,
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
    Frame { divider: Option<i32> },
    /// A unit icon (`ICONS.GP` frame) in its cell.
    Icon { frame: Option<usize> },
    /// A player colour; `hot` adds the larger outline.
    Swatch { owner: u8 },
    /// The closed combo box.
    Combo,
    /// A line of the open combo box.
    DropRow,
    /// The scroll bar, drawn as a whole.
    Bar { pos: i32, max: i32 },
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
    /// `ED_HDR01`.
    pub title: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Editor {
    /// The window is shown (F2 or its title toggles).
    pub open: bool,
    /// The combo box line: an index in [`ENTRIES`].
    entry: usize,
    combo_open: bool,
    /// The icons' scroll position in pixels.
    scroll: i32,
    /// The type placed by a click on the map.
    pub pick: Option<TypeId>,
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
            open: true,
            entry: DEFAULT_ENTRY,
            combo_open: false,
            scroll: 0,
            pick: None,
            count: COUNTS.iter().position(|&c| c == 36).unwrap_or(0),
            form: 0,
            owner: 0,
            // Player 1 faces north (up the screen), player 2 south, the others east.
            facing: [192, 64, 0, 0, 0, 0, 0, 0],
            level: 0,
            menu: None,
            sides: sides::Mode::Off,
            team: sides::default_teams(),
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

/// A category of an entry's list: its header and types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub header: &'static str,
    pub kinds: Vec<TypeId>,
}

/// The category of a land type in a nation's list: riders, guns (`COMPLEXROTATE`) or foot.
fn category(t: &acr_sim::catalog::ObjectType) -> usize {
    if acr_sim::spacing::is_rider(t) {
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
    let prefix = ENTRIES[entry].1.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut by: Vec<Vec<TypeId>> = vec![Vec::new(); CATEGORIES.len()];
    for (id, t) in world.catalog.iter() {
        if t.building
            || t.life <= 0
            || t.medium != Medium::Land
            || !t.id.to_ascii_lowercase().starts_with(&prefix)
            || !seen.insert(id)
        {
            continue;
        }
        by[category(t)].push(id);
    }
    if entry >= FIRST_EXTRA {
        // Animals and cannons are one list under their own name.
        let kinds = by.into_iter().flatten().collect();
        return vec![Group { header: ENTRIES[entry].0, kinds }];
    }
    by.into_iter()
        .zip(CATEGORIES)
        .filter(|(kinds, _)| !kinds.is_empty())
        .map(|(kinds, header)| Group { header, kinds })
        .collect()
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
/// group and its icons five to a row; returns the places and the y after the last row.
pub fn flow(groups: &[Group]) -> (Vec<Placed>, i32) {
    let mut out = Vec::new();
    let mut y = FIRST_ROW;
    for (g, group) in groups.iter().enumerate() {
        out.push(Placed::Header(g, y));
        y += HEADER_H;
        for (i, _) in group.kinds.iter().enumerate() {
            out.push(Placed::Cell(g, i, i % COLUMNS, y + (i / COLUMNS) as i32 * PITCH));
        }
        y += group.kinds.len().div_ceil(COLUMNS) as i32 * PITCH;
    }
    (out, y)
}

/// How far the icons scroll: the content's end less the visible height (0x5bba8a).
pub fn scroll_max(end: i32) -> i32 {
    (end - VISIBLE_H).max(0)
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

    pub fn owner(&self) -> u8 {
        self.owner
    }

    pub fn facing(&self) -> u8 {
        self.facing[usize::from(self.owner)]
    }

    /// The formation chosen for the picked type, if any.
    pub fn formation(&self, world: &World) -> Option<FormChoice> {
        let kind = self.pick?;
        (self.form > 0).then(|| formations(world, kind).get(self.form - 1).cloned()).flatten()
    }

    /// The window's items in drawing order (hit tests go from the back).
    pub fn layout(&self, world: &World, view: &View, local: u8) -> Vec<Item> {
        let mut out = Vec::new();
        let item = |rect: Rect, text: Vec<u8>, name: &str, hot: bool, act: Option<Act>, look: Look| Item {
            rect,
            text,
            name: name.to_owned(),
            hot,
            act,
            look,
        };
        if !self.open {
            let rect = Rect::new(OX, BOX_Y0, 249, ROW);
            out.push(item(rect, b"[Editor] (F2)".to_vec(), "Editor", false, Some(Act::Hide), Look::Text));
            return out;
        }
        let window = Rect::inclusive(OX, BOX_Y0, BOX_X1, BOX_Y1);
        out.push(item(window, Vec::new(), "", false, None, Look::Frame { divider: Some(DIVIDER) }));
        let title = Rect::new(OX + 10, OY - 30, 200, ROW);
        out.push(item(title, view.title.clone(), "Editor einklappen", false, Some(Act::Hide), Look::Title));
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
        let (placed, end) = flow(&groups);
        let max = scroll_max(end);
        let scroll = self.scroll.clamp(0, max);
        for p in &placed {
            match *p {
                Placed::Header(g, y) => {
                    let rect = Rect::new(OX + 10, OY + y - scroll, 200, HEADER_H);
                    let text = groups[g].header.as_bytes().to_vec();
                    out.push(item(rect, text, "", false, None, Look::Header));
                }
                Placed::Cell(g, i, col, y) => {
                    let kind = groups[g].kinds[i];
                    let rect = Rect::new(OX + 5 + PITCH * col as i32, OY + y - 1 - scroll, CELL, CELL);
                    if rect.intersect(&CONTENT).is_empty() {
                        continue;
                    }
                    let (frame, name) = ((view.icon)(kind), (view.names)(kind));
                    let label = String::from_utf8_lossy(&name).into_owned();
                    let hot = self.pick == Some(kind);
                    out.push(item(rect, name, &label, hot, Some(Act::Pick(kind)), Look::Icon { frame }));
                }
            }
        }
        // The combo box (10, 36), 224 wide.
        let combo = Rect::new(OX + 10, OY + 36, 224, 22);
        out.push(item(
            combo,
            ENTRIES[self.entry].0.as_bytes().to_vec(),
            "Nation",
            false,
            Some(Act::Combo),
            Look::Combo,
        ));
        // The scroll bar: arrows and the track's halves are the click targets.
        if max > 0 {
            let bar = Rect::new(BAR_X, BAR_Y, BAR_W, BAR_LEN);
            out.push(item(bar, Vec::new(), "", false, None, Look::Bar { pos: scroll, max }));
            let (up, down) =
                (Rect::new(BAR_X, BAR_Y, BAR_W, BAR_W), Rect::new(BAR_X, BAR_Y + BAR_LEN - BAR_W, BAR_W, BAR_W));
            out.push(item(up, Vec::new(), "Hoch", false, Some(Act::ScrollBy(-PITCH)), Look::Text));
            out.push(item(down, Vec::new(), "Runter", false, Some(Act::ScrollBy(PITCH)), Look::Text));
            let (mid_y, track_h) = (BAR_Y + BAR_W, BAR_LEN - 2 * BAR_W);
            let page = VISIBLE_H - PITCH;
            let above = Rect::new(BAR_X, mid_y, BAR_W, track_h / 2);
            let below = Rect::new(BAR_X, mid_y + track_h / 2, BAR_W, track_h - track_h / 2);
            out.push(item(above, Vec::new(), "Seite hoch", false, Some(Act::ScrollBy(-page)), Look::Text));
            out.push(item(below, Vec::new(), "Seite runter", false, Some(Act::ScrollBy(page)), Look::Text));
        }
        // ACR's controls in a box under the window.
        let mut y = EXTRA_Y;
        let extras = |rows: i32| Rect::inclusive(OX, BOX_Y1 + 1, BOX_X1, BOX_Y1 + 1 + 12 + rows * ROW);
        let rows = 12 + i32::from(self.pick.is_some());
        out.push(item(extras(rows), Vec::new(), "", false, None, Look::Frame { divider: None }));
        let line = |out: &mut Vec<Item>, y: &mut i32, text: Vec<u8>, name: &str, hot: bool, act: Option<Act>| {
            out.push(item(Rect::new(X0, *y, W, ROW), text, name, hot, act, Look::Text));
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
            out.push(item(Rect::new(X0 + 24, *y, W - 48, ROW), text.into_bytes(), name, false, None, Look::Text));
            out.push(arrow(X0 + W - 24, ">", "+", b));
            *y += ROW;
        };
        pair(&mut out, &mut y, format!("Anzahl: {}", self.count()), "Anzahl", Act::Count(-1), Act::Count(1));
        let form = match self.pick {
            Some(kind) if self.form > 0 => {
                formations(world, kind).get(self.form - 1).map_or("lose".into(), |c| group_name(world, c.group))
            }
            _ => "lose".into(),
        };
        pair(&mut out, &mut y, format!("Formation: {form}"), "Formation", Act::Form(-1), Act::Form(1));
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
        if let Some(kind) = self.pick {
            let mut text = b"Typ: ".to_vec();
            text.extend((view.names)(kind));
            text.truncate(44);
            line(&mut out, &mut y, text, "", false, None);
        }
        let mut status = self.status.clone();
        status.truncate(44);
        line(&mut out, &mut y, status, "", false, None);
        line(&mut out, &mut y, b"Rechtsklick: Platzieren beenden".to_vec(), "", false, None);
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
        // The level drop-down, over everything.
        if let Some(menu) = self.menu {
            let names = levels::names(world, menu.kind, view.upgrade_name);
            let rows = menu.rows(names.len(), names.iter().map(Vec::len).max().unwrap_or(0));
            for (n, (rect, text)) in rows.into_iter().zip(names).enumerate() {
                let name = format!("Ausbau {}", String::from_utf8_lossy(&text));
                let hot = self.pick == Some(menu.kind) && self.level == n;
                out.push(item(rect, text, &name, hot, Some(Act::SetLevel(n)), Look::DropRow));
            }
        }
        out
    }

    /// The Ausbau line: the picked type's level, its name and the upgrades applied.
    fn level_line(&self, world: &World, view: &View) -> Vec<u8> {
        let names = self.pick.map_or_else(
            || vec![levels::BASIS.as_bytes().to_vec()],
            |kind| levels::names(world, kind, view.upgrade_name),
        );
        // The upgrade texts are long: the count comes first, the line is cut to the window.
        let mut text = format!("Ausbau {}/{}: ", self.level, names.len() - 1).into_bytes();
        text.extend(names.get(self.level).map_or(&[][..], Vec::as_slice));
        text.truncate(44);
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
        let mut frames = items.iter().filter(|i| matches!(i.look, Look::Frame { .. }) || i.act == Some(Act::Hide));
        let Some(first) = frames.next() else { return Rect::new(0, 0, 0, 0) };
        let first = first.rect;
        frames.fold(first, |r, i| Rect {
            x0: r.x0.min(i.rect.x0),
            y0: r.y0.min(i.rect.y0),
            x1: r.x1.max(i.rect.x1),
            y1: r.y1.max(i.rect.y1),
        })
    }

    pub fn draws(items: &[Item], ctx: &Ctx, out: &mut Vec<Draw>) {
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
                Look::Header => out.push(Draw::game_text(menu_font::F16Y1, &i.text, r.x0, r.y0).clipped(CONTENT)),
                Look::Icon { frame } => {
                    let index = if i.hot { OUTLINE_PICKED } else { OUTLINE };
                    let mut cell = Vec::new();
                    match frame {
                        Some(f) => cell.push(Draw::game_frame(hud::gp::ICONS, f, r.x0 + 1, r.y0 + 1, 0)),
                        // No icon: the name stands in its cell.
                        None => cell.push(Draw::game_text(WHITE, &i.text[..i.text.len().min(8)], r.x0 + 3, r.y0 + 14)),
                    }
                    hud::outline(&mut cell, r, colour(index));
                    out.extend(cell.into_iter().map(|d| d.clipped(CONTENT)));
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

    /// A click on window item `act`. Returns a new local player for [`Act::Control`].
    pub fn apply(&mut self, world: &mut World, act: Act, local: u8) -> Option<u8> {
        if act != Act::Combo && !matches!(act, Act::Entry(_)) {
            self.combo_open = false;
        }
        let menu = self.menu.take();
        match act {
            Act::Combo => self.combo_open = !self.combo_open,
            Act::Entry(e) => {
                self.combo_open = false;
                if self.entry != e {
                    self.entry = e;
                    self.scroll = 0;
                }
            }
            Act::Pick(k) => {
                self.pick = if self.pick == Some(k) { None } else { Some(k) };
                self.level = 0;
                self.form = self.form.min(formations(world, k).len());
            }
            Act::Owner(p) => self.owner = p,
            Act::SetLevel(l) => {
                if let Some(m) = menu {
                    self.pick = Some(m.kind);
                    self.level = l.min(levels::steps(world, m.kind));
                    self.form = self.form.min(formations(world, m.kind).len());
                }
            }
            Act::ScrollBy(d) => self.scroll_by(world, d),
            Act::Count(d) => self.count = (self.count as i32 + d).clamp(0, COUNTS.len() as i32 - 1) as usize,
            Act::Form(d) => {
                let n = self.pick.map_or(0, |k| formations(world, k).len()) as i32 + 1;
                self.form = (self.form as i32 + d).rem_euclid(n) as usize;
            }
            Act::Facing(d) => {
                let f = &mut self.facing[usize::from(self.owner)];
                *f = f.wrapping_add((d * 32) as u8);
            }
            Act::Peace => {
                self.sides = self.sides.next();
                sides::apply(world, self.sides, &self.team);
                self.status = match self.sides {
                    sides::Mode::Off => b"Kampf aus".to_vec(),
                    sides::Mode::Free => b"Kampf an: alle gegen alle".to_vec(),
                    sides::Mode::Teams => b"Kampf an: Teams".to_vec(),
                };
            }
            Act::Reload => {
                let n = reload_guns(world);
                self.status = format!("{n} Kanonen geladen").into_bytes();
            }
            Act::AutoReload => {
                self.auto_reload = !self.auto_reload;
                self.status = if self.auto_reload {
                    b"Kanonen laden sofort nach".to_vec()
                } else {
                    b"Kanonen laden normal".to_vec()
                };
            }
            Act::Control => return Some(sides::next_controlled(world, local)),
            Act::Team(p) => {
                let t = &mut self.team[usize::from(p) & 7];
                *t = sides::next_team(*t);
                sides::apply(world, self.sides, &self.team);
            }
            Act::Hide => self.open = !self.open,
        }
        None
    }

    /// Scrolls the icons by `d` pixels, within the content.
    fn scroll_by(&mut self, world: &World, d: i32) {
        let (_, end) = flow(&groups(world, self.entry));
        self.scroll = (self.scroll + d).clamp(0, scroll_max(end));
    }

    /// Scrolls the icons by wheel notches (one row each).
    pub fn wheel(&mut self, world: &World, lines: f32) {
        self.scroll_by(world, -(lines.round() as i32) * PITCH);
    }

    /// Places the picked type at world pixel `at`: [`Editor::count`] units, loose or in
    /// formations of the chosen kind side by side across the facing.
    pub fn place(&mut self, world: &mut World, at: (i32, i32)) {
        let Some(kind) = self.pick else { return };
        if !world.sea_allows(kind, Pos::from_pixels(at.0, at.1)) {
            self.status = b"Nicht auf Wasser".to_vec();
            return;
        }
        let (n, owner, dir) = (self.count() as usize, self.owner(), self.facing());
        // The level holds for the owner's every object of the placed types, as researched
        // upgrades do: set before spawning, so the new men start at full upgraded life.
        let form = self.formation(world);
        let mut kinds = vec![kind];
        if let Some(c) = &form {
            let o = world.formations.officers.get(&c.officer).cloned().unwrap_or_default();
            kinds.extend([Some(c.officer), o.drummer, o.flag].into_iter().flatten());
        }
        let ups = levels::steps(world, kind);
        let level = levels::apply(world, owner, kind, self.level);
        // The officer, drummer and flag bearer stand at the same level, as far as they have it.
        for &k in kinds.iter().skip(1) {
            levels::apply(world, owner, k, level);
        }
        let placed = match form {
            Some(choice) => place_formations(world, kind, &choice, n, owner, at, dir),
            None => {
                let t = world.catalog.get(kind);
                let gap = (t.radius2 * 4).max(32);
                for p in grid(n, at, dir, gap) {
                    world.spawn(kind, owner, Pos::from_pixels(p.0, p.1), dir);
                }
                Ok(n)
            }
        };
        sides::apply(world, self.sides, &self.team);
        let name = world.catalog.get(kind).id.clone();
        self.status = match placed {
            Ok(k) => format!("{k} x {name}, Ausbau {level}/{ups}").into_bytes(),
            Err(e) => e.into_bytes(),
        };
    }
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

impl Look {
    fn is_icon(self) -> bool {
        matches!(self, Look::Icon { .. })
    }
}

impl Item {
    /// Where a click hits: icons and headers only inside the clipped content.
    pub fn hit_rect(&self) -> Rect {
        match self.look {
            Look::Icon { .. } | Look::Header => self.rect.intersect(&CONTENT),
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
        let view =
            View { names: &names, icon: &icon, upgrade_name: &upgrade_name, title: ctx.texts.get("ED_HDR01").to_vec() };
        e.layout(world, &view, self.local_player())
    }

    pub(super) fn editor_overlay(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        Editor::draws(&self.editor_items(ctx), ctx, out);
    }

    /// The window's clickable items for the test driver (`ui`, `click`), in window pixels.
    pub fn editor_icons(&self, ctx: &Ctx) -> Vec<super::probe::HudIcon> {
        let canvas = self.interface.canvas;
        self.editor_items(ctx)
            .iter()
            .enumerate()
            .filter(|(_, i)| i.act.is_some())
            .map(|(n, i)| {
                let r = i.hit_rect();
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
                    right_clickable: i.look.is_icon(),
                }
            })
            .collect()
    }

    /// The left button in the editor: the window, or placing the picked type on the map.
    /// True when the press (or its release) is used up.
    pub(super) fn editor_left(&mut self, ctx: &Ctx, pressed: bool) -> bool {
        if self.editor.is_none() {
            return false;
        }
        if !pressed {
            let e = self.editor.as_mut().expect("editor");
            return std::mem::take(&mut e.swallow_release);
        }
        let Some(at) = self.cursor else { return false };
        let p = self.canvas_point(at);
        let items = self.editor_items(ctx);
        // A press outside the level drop-down closes it; one on the map does no more.
        let on_menu = items.iter().any(|i| i.look == Look::DropRow && i.rect.contains(p));
        let closed = self.editor.as_mut().is_some_and(|e| !on_menu && e.close_levels());
        if closed && !Editor::bounds(&items).contains(p) {
            self.editor.as_mut().expect("editor").swallow_release = true;
            return true;
        }
        if Editor::bounds(&items).contains(p) || items.iter().any(|i| i.look == Look::DropRow && i.rect.contains(p)) {
            let act = items.iter().rev().find(|i| i.act.is_some() && i.hit_rect().contains(p)).and_then(|i| i.act);
            let local = self.local_player();
            if let (Some(act), Some(e), Some(world)) = (act, &mut self.editor, &mut self.world)
                && let Some(next) = e.apply(world, act, local)
            {
                self.setup.local_player = usize::from(next);
                self.selected.clear();
            }
            self.editor.as_mut().expect("editor").swallow_release = true;
            return true;
        }
        if self.editor.as_ref().is_some_and(|e| e.pick.is_some()) && !self.over_hud(ctx, at) {
            let target = self.world_at(at);
            if let (Some(e), Some(world)) = (&mut self.editor, &mut self.world) {
                e.place(world, target);
                e.swallow_release = true;
            }
            return true;
        }
        false
    }

    /// The right button in the editor: on an icon, opens its level drop-down; elsewhere
    /// closes the drop-down or stops placing.
    pub(super) fn editor_right(&mut self, ctx: &Ctx, pressed: bool) -> bool {
        if !pressed || self.editor.is_none() {
            return false;
        }
        let hit = self.cursor.map(|at| self.canvas_point(at)).and_then(|p| {
            let items = self.editor_items(ctx);
            items.into_iter().rev().find(|i| i.act.is_some() && i.hit_rect().contains(p))
        });
        // A colour's swatch cycles its team while teams fight.
        if let Some(Item { look: Look::Swatch { owner }, .. }) = hit
            && owner != NEUTRAL
            && let (Some(e), Some(world)) = (&mut self.editor, &mut self.world)
            && e.sides == sides::Mode::Teams
        {
            e.apply(world, Act::Team(owner), 0);
            return true;
        }
        let icon = match hit {
            Some(Item { look: Look::Icon { .. }, act: Some(Act::Pick(k)), rect, .. }) => Some((k, rect)),
            _ => None,
        };
        let e = self.editor.as_mut().expect("editor");
        if let Some((kind, cell)) = icon {
            e.open_levels(kind, cell);
            return true;
        }
        if e.close_levels() {
            return true;
        }
        if e.pick.is_some() {
            e.pick = None;
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
                    e.open = !e.open;
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let g = |n: u16| Group { header: "x", kinds: (0..n).map(TypeId).collect() };
        let (placed, end) = flow(&[g(6), g(1)]);
        // Header at 80, icons from 100 (two rows), the next header 2 rows on, its icon 20 later.
        assert_eq!(placed[0], Placed::Header(0, 80));
        assert_eq!(placed[1], Placed::Cell(0, 0, 0, 100));
        assert_eq!(placed[5], Placed::Cell(0, 4, 4, 100));
        assert_eq!(placed[6], Placed::Cell(0, 5, 0, 144));
        assert_eq!(placed[7], Placed::Header(1, 188));
        assert_eq!(placed[8], Placed::Cell(1, 0, 0, 208));
        assert_eq!(end, 252);
        assert_eq!(flow(&[]).1, FIRST_ROW);
    }

    #[test]
    fn the_scroll_range_is_the_content_beyond_the_visible_height() {
        assert_eq!(scroll_max(0x17e), 0);
        assert_eq!(scroll_max(0x17e + 44), 44);
        assert_eq!(scroll_max(100), 0);
    }

    #[test]
    fn the_window_sits_in_the_side_panel_column() {
        // `RealLx − 269`, `InfY1 + 31` (0x5bb735, 0x5bb743) and the box of 0x5bb766.
        assert_eq!((OX, OY), (755, 79));
        assert_eq!((BOX_X1, BOX_Y0, BOX_Y1, DIVIDER), (1004, 48, 465, 153));
        assert_eq!(ENTRIES[FIRST_EXTRA].0, "Tiere");
        assert!(ENTRIES.len() == FIRST_EXTRA + 2 && DEFAULT_ENTRY < FIRST_EXTRA);
    }
}
