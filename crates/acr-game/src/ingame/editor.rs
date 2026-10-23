//! The test editor (main menu "Editor", docs/controls.md, "Editor"): a blank field where
//! units are placed directly, any number at once, loose or already standing in a formation,
//! with fighting switched off while the battle is set up. Not in the original, whose editor
//! places single units only; everything here acts on the local world directly, like the
//! test driver's `spawn`, so an editor game is not a network or replay game.

use acr_sim::catalog::Medium;
use acr_sim::fixed::Pos;
use acr_sim::{TypeId, World};

use crate::ui::Ctx;
use crate::ui::draw::{Draw, Rect};
use crate::ui::fonts::FontId;

const WHITE: FontId = FontId("SmallWhiteFont");
const YELLOW: FontId = FontId("SmallYellowFont");
/// Panel origin and width on the interface canvas, and the row height.
const X0: i32 = 8;
const Y0: i32 = 64;
const W: i32 = 260;
const ROW: i32 = 16;
/// Unit types shown at once in the list.
const LIST_ROWS: usize = 14;
/// The counts offered; a count above one formation's size places several side by side.
pub const COUNTS: [u32; 12] = [1, 5, 10, 15, 36, 72, 120, 196, 392, 588, 784, 980];
/// Nation tabs: label and the type id prefixes they list.
pub const TABS: [(&str, &str); 14] = [
    ("ENG", "Eng"),
    ("FRN", "Frn"),
    ("SPN", "Spn"),
    ("USA", "Usa"),
    ("AZT", "Atc"),
    ("MAY", "May"),
    ("INC", "Inc"),
    ("PUE", "Pue"),
    ("SIU", "Siu"),
    ("DEL", "Del"),
    ("HUR", "Hur"),
    ("IRO", "Iro"),
    ("Tiere", "Anm"),
    ("Kanonen", "Art"),
];
/// Owners offered: the two players and the neutral player 7 (wild animals).
const OWNERS: [(u8, &str); 3] = [(0, "Spieler 1"), (1, "Spieler 2"), (7, "Neutral")];
/// Compass names of the eight facings, `dir` = 32 · i (0 east, 64 south).
const FACINGS: [&str; 8] = ["Ost", "Suedost", "Sued", "Suedwest", "West", "Nordwest", "Nord", "Nordost"];
/// Space between formations placed side by side, world pixels.
const FORMATION_GAP: i32 = 64;
/// Upgrade levels: the button behind a list row and the name in the settings.
const LEVELS: [(&str, &str); 3] = [("B", "Basis"), ("H", "Halb"), ("V", "Voll")];
/// Width of one level button behind a list row.
const LEVEL_W: i32 = 20;

/// A click target of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Tab(usize),
    Pick(TypeId),
    /// Picks the type at an upgrade level (a button behind its list row).
    PickLevel(TypeId, usize),
    Level(i32),
    ScrollUp,
    ScrollDown,
    Count(i32),
    Form(i32),
    Owner,
    Facing(i32),
    Peace,
    Reload,
    AutoReload,
    Control,
    Hide,
}

/// One line of the panel: where it is, what it says, whether it is highlighted.
#[derive(Debug, Clone)]
pub struct Item {
    pub rect: Rect,
    pub text: Vec<u8>,
    pub hot: bool,
    pub act: Option<Act>,
}

/// A formation a type can stand in: the `ORD_GROUPS.LST` group, its officer type and the
/// size variants (shape indices).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormChoice {
    pub group: usize,
    pub officer: TypeId,
    pub shapes: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Editor {
    /// The panel is shown (F2 or its title toggles).
    pub open: bool,
    tab: usize,
    scroll: usize,
    /// The type placed by a click on the map.
    pub pick: Option<TypeId>,
    count: usize,
    /// 0 = loose, else 1 + the index in [`formations`].
    form: usize,
    owner: usize,
    facing: [u8; 3],
    /// Upgrade level of placed types, an index in [`LEVELS`]: none, the first half of the
    /// type's upgrades or all of them ([`acr_sim::upgrade::of_type`]).
    level: usize,
    /// Fighting is off: every player is everybody's ally (set on start).
    pub peace: bool,
    /// Every cannon is reloaded each tick (a test switch, off on start).
    pub auto_reload: bool,
    /// The last placement or problem, shown under the panel.
    status: Vec<u8>,
    /// A press placed or hit the panel: its release does nothing else.
    pub swallow_release: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            open: true,
            tab: 0,
            scroll: 0,
            pick: None,
            count: COUNTS.iter().position(|&c| c == 36).unwrap_or(0),
            form: 0,
            owner: 0,
            // Player 1 faces north (up the screen), player 2 south, neutral east.
            facing: [192, 64, 0],
            level: 0,
            peace: true,
            auto_reload: false,
            status: b"Typ waehlen, dann auf die Karte klicken".to_vec(),
            swallow_release: false,
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

/// Types a tab lists: units and animals of the prefix that stand on land.
pub fn types(world: &World, tab: usize) -> Vec<TypeId> {
    let prefix = TABS[tab].1.to_ascii_lowercase();
    let mut out: Vec<(String, TypeId)> = world
        .catalog
        .iter()
        .filter(|(_, t)| {
            !t.building && t.life > 0 && t.medium == Medium::Land && t.id.to_ascii_lowercase().starts_with(&prefix)
        })
        .map(|(id, t)| (t.id.clone(), id))
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.1 == b.1);
    out.into_iter().map(|(_, id)| id).collect()
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

/// Every player allied with every other (`peace`), or each only with itself, as the
/// mission scripts set alliances (`sync_alliances`): the nations' masks and every object's.
pub fn set_peace(world: &mut World, peace: bool) {
    for (n, m) in world.nation_mask.iter_mut().enumerate() {
        *m = if peace { 0xFF } else { 1 << n };
    }
    let masks = world.nation_mask;
    for u in &mut world.units {
        u.nmask = masks[usize::from(u.owner) & 7];
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
        OWNERS[self.owner].0
    }

    pub fn facing(&self) -> u8 {
        self.facing[self.owner]
    }

    /// The formation chosen for the picked type, if any.
    pub fn formation(&self, world: &World) -> Option<FormChoice> {
        let kind = self.pick?;
        (self.form > 0).then(|| formations(world, kind).get(self.form - 1).cloned()).flatten()
    }

    /// The panel's lines; `names` gives a type's display name.
    pub fn layout(&self, world: &World, names: &dyn Fn(TypeId) -> Vec<u8>, local: u8) -> Vec<Item> {
        let mut out = Vec::new();
        let mut y = Y0;
        let line = |out: &mut Vec<Item>, y: &mut i32, text: Vec<u8>, hot: bool, act: Option<Act>| {
            out.push(Item { rect: Rect::new(X0, *y, W, ROW), text, hot, act });
            *y += ROW;
        };
        if !self.open {
            line(&mut out, &mut y, b"[Editor] (F2)".to_vec(), false, Some(Act::Hide));
            return out;
        }
        line(&mut out, &mut y, b"EDITOR - einklappen (F2)".to_vec(), true, Some(Act::Hide));
        // Tabs, four to a row.
        for (i, (label, _)) in TABS.iter().enumerate() {
            let (cx, cy) = (X0 + (i % 4) as i32 * (W / 4), y + (i / 4) as i32 * ROW);
            out.push(Item {
                rect: Rect::new(cx, cy, W / 4, ROW),
                text: label.as_bytes().to_vec(),
                hot: i == self.tab,
                act: Some(Act::Tab(i)),
            });
        }
        y += TABS.len().div_ceil(4) as i32 * ROW + 4;
        let list = types(world, self.tab);
        line(&mut out, &mut y, b"  ^ hoch".to_vec(), false, (self.scroll > 0).then_some(Act::ScrollUp));
        let buttons = LEVEL_W * LEVELS.len() as i32;
        for &kind in list.iter().skip(self.scroll).take(LIST_ROWS) {
            let mut text = names(kind);
            text.truncate(34);
            let picked = self.pick == Some(kind);
            out.push(Item { rect: Rect::new(X0, y, W - buttons, ROW), text, hot: picked, act: Some(Act::Pick(kind)) });
            for (l, (label, _)) in LEVELS.iter().enumerate() {
                out.push(Item {
                    rect: Rect::new(X0 + W - buttons + l as i32 * LEVEL_W, y, LEVEL_W, ROW),
                    text: label.as_bytes().to_vec(),
                    hot: picked && self.level == l,
                    act: Some(Act::PickLevel(kind, l)),
                });
            }
            y += ROW;
        }
        for _ in list.len().saturating_sub(self.scroll).min(LIST_ROWS)..LIST_ROWS {
            line(&mut out, &mut y, Vec::new(), false, None);
        }
        let more = self.scroll + LIST_ROWS < list.len();
        line(&mut out, &mut y, b"  v runter".to_vec(), false, more.then_some(Act::ScrollDown));
        y += 4;
        let pair = |out: &mut Vec<Item>, y: &mut i32, text: String, a: Act, b: Act| {
            out.push(Item { rect: Rect::new(X0, *y, 24, ROW), text: b"<".to_vec(), hot: false, act: Some(a) });
            out.push(Item {
                rect: Rect::new(X0 + 24, *y, W - 48, ROW),
                text: text.into_bytes(),
                hot: false,
                act: None,
            });
            out.push(Item { rect: Rect::new(X0 + W - 24, *y, 24, ROW), text: b">".to_vec(), hot: false, act: Some(b) });
            *y += ROW;
        };
        pair(&mut out, &mut y, format!("Anzahl: {}", self.count()), Act::Count(-1), Act::Count(1));
        let form = match self.pick {
            Some(kind) if self.form > 0 => {
                formations(world, kind).get(self.form - 1).map_or("lose".into(), |c| group_name(world, c.group))
            }
            _ => "lose".into(),
        };
        pair(&mut out, &mut y, format!("Formation: {form}"), Act::Form(-1), Act::Form(1));
        let ups = self.pick.map_or(0, |k| acr_sim::upgrade::of_type(&world.catalog, k).len());
        let level = format!("Ausbau: {} ({}/{ups})", LEVELS[self.level].1, level_count(self.level, ups));
        pair(&mut out, &mut y, level, Act::Level(-1), Act::Level(1));
        pair(
            &mut out,
            &mut y,
            format!("Richtung: {}", FACINGS[usize::from(self.facing() / 32) & 7]),
            Act::Facing(-1),
            Act::Facing(1),
        );
        line(&mut out, &mut y, format!("Besitzer: {}", OWNERS[self.owner].1).into_bytes(), false, Some(Act::Owner));
        let peace = if self.peace { "Kampf: AUS (klicken: an)" } else { "Kampf: AN (klicken: aus)" };
        line(&mut out, &mut y, peace.as_bytes().to_vec(), !self.peace, Some(Act::Peace));
        line(&mut out, &mut y, b"Kanonen sofort laden".to_vec(), false, Some(Act::Reload));
        let auto = if self.auto_reload { "Kanonen: sofort nachladen AN" } else { "Kanonen: sofort nachladen AUS" };
        line(&mut out, &mut y, auto.as_bytes().to_vec(), self.auto_reload, Some(Act::AutoReload));
        let me = OWNERS.iter().find(|o| o.0 == local).map_or("?", |o| o.1);
        line(&mut out, &mut y, format!("Ich steuere: {me}").into_bytes(), false, Some(Act::Control));
        y += 4;
        let mut status = self.status.clone();
        status.truncate(40);
        line(&mut out, &mut y, status, false, None);
        if self.pick.is_some() {
            line(&mut out, &mut y, b"Rechtsklick: Platzieren beenden".to_vec(), false, None);
        }
        out
    }

    /// The panel's whole area (clicks there never reach the map).
    pub fn bounds(items: &[Item]) -> Rect {
        items.iter().fold(Rect::new(X0, Y0, 0, 0), |r, i| Rect {
            x0: r.x0.min(i.rect.x0),
            y0: r.y0.min(i.rect.y0),
            x1: r.x1.max(i.rect.x1),
            y1: r.y1.max(i.rect.y1),
        })
    }

    pub fn draws(items: &[Item], out: &mut Vec<Draw>) {
        if items.is_empty() {
            return;
        }
        let b = Self::bounds(items);
        out.push(Draw::fill([0, 0, 0, 170], Rect { x0: b.x0 - 4, y0: b.y0 - 4, x1: b.x1 + 4, y1: b.y1 + 4 }));
        for i in items.iter().filter(|i| !i.text.is_empty()) {
            if i.hot {
                out.push(Draw::fill([90, 70, 20, 200], i.rect));
            }
            let font = if i.act.is_some() { YELLOW } else { WHITE };
            out.push(Draw::game_text(font, &i.text, i.rect.x0 + 3, i.rect.y0 + 2));
        }
    }

    /// A click on panel item `act`. Returns a new local player for [`Act::Control`].
    pub fn apply(&mut self, world: &mut World, act: Act, local: u8) -> Option<u8> {
        match act {
            Act::Tab(t) => {
                self.tab = t;
                self.scroll = 0;
            }
            Act::Pick(k) => {
                self.pick = if self.pick == Some(k) { None } else { Some(k) };
                self.form = self.form.min(formations(world, k).len());
            }
            Act::PickLevel(k, l) => {
                self.pick = Some(k);
                self.level = l;
                self.form = self.form.min(formations(world, k).len());
            }
            Act::Level(d) => self.level = (self.level as i32 + d).rem_euclid(LEVELS.len() as i32) as usize,
            Act::ScrollUp => self.scroll = self.scroll.saturating_sub(LIST_ROWS / 2),
            Act::ScrollDown => {
                let n = types(world, self.tab).len();
                self.scroll = (self.scroll + LIST_ROWS / 2).min(n.saturating_sub(LIST_ROWS));
            }
            Act::Count(d) => self.count = (self.count as i32 + d).clamp(0, COUNTS.len() as i32 - 1) as usize,
            Act::Form(d) => {
                let n = self.pick.map_or(0, |k| formations(world, k).len()) as i32 + 1;
                self.form = (self.form as i32 + d).rem_euclid(n) as usize;
            }
            Act::Facing(d) => {
                let f = &mut self.facing[self.owner];
                *f = f.wrapping_add((d * 32) as u8);
            }
            Act::Owner => self.owner = (self.owner + 1) % OWNERS.len(),
            Act::Peace => {
                self.peace = !self.peace;
                set_peace(world, self.peace);
                self.status = if self.peace { b"Kampf aus".to_vec() } else { b"Kampf an!".to_vec() };
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
            Act::Control => return Some(if local == 0 { 1 } else { 0 }),
            Act::Hide => self.open = !self.open,
        }
        None
    }

    /// Scrolls the list by wheel notches.
    pub fn wheel(&mut self, world: &World, lines: f32) {
        let n = types(world, self.tab).len();
        let s = self.scroll as i32 - (lines.round() as i32) * 2;
        self.scroll = s.clamp(0, n.saturating_sub(LIST_ROWS) as i32) as usize;
    }

    /// Places the picked type at world pixel `at`: [`Editor::count`] units, loose or in
    /// formations of the chosen kind side by side across the facing.
    pub fn place(&mut self, world: &mut World, at: (i32, i32)) {
        let Some(kind) = self.pick else { return };
        let (n, owner, dir) = (self.count() as usize, self.owner(), self.facing());
        // The level holds for the owner's every object of the placed types, as researched
        // upgrades do: set before spawning, so the new men start at full upgraded life.
        let form = self.formation(world);
        let mut kinds = vec![kind];
        if let Some(c) = &form {
            let o = world.formations.officers.get(&c.officer).cloned().unwrap_or_default();
            kinds.extend([Some(c.officer), o.drummer, o.flag].into_iter().flatten());
        }
        let ups = acr_sim::upgrade::of_type(&world.catalog, kind).len();
        for &k in &kinds {
            let n = level_count(self.level, acr_sim::upgrade::of_type(&world.catalog, k).len());
            acr_sim::upgrade::set_type_level(world, owner, k, n);
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
        set_peace(world, self.peace);
        let name = world.catalog.get(kind).id.clone();
        self.status = match placed {
            Ok(k) => format!("{k} x {name}, Ausbau {}/{ups}", level_count(self.level, ups)).into_bytes(),
            Err(e) => e.into_bytes(),
        };
    }
}

/// How many of a type's `ups` upgrades level `level` applies: none, the first half
/// (rounded up) or all.
fn level_count(level: usize, ups: usize) -> usize {
    match level {
        0 => 0,
        1 => ups.div_ceil(2),
        _ => ups,
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

/// The editor's hooks into the game screen.
impl super::InGame {
    /// The panel's lines for the current state, or none outside the editor.
    fn editor_items(&self, ctx: &Ctx) -> Vec<Item> {
        let (Some(e), Some(world)) = (&self.editor, &self.world) else { return Vec::new() };
        let names = |k: TypeId| {
            let t = world.catalog.get(k);
            let mut n = super::interface::unit_name(ctx, &t.md_name);
            if n.is_empty() || n == t.md_name.as_bytes() {
                return t.id.as_bytes().to_vec();
            }
            n.extend_from_slice(format!(" ({})", t.id).as_bytes());
            n
        };
        e.layout(world, &names, self.local_player())
    }

    pub(super) fn editor_overlay(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        Editor::draws(&self.editor_items(ctx), out);
    }

    /// The left button in the editor: the panel, or placing the picked type on the map.
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
        if Editor::bounds(&items).contains(p) {
            let act = items.iter().find(|i| i.rect.contains(p)).and_then(|i| i.act);
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

    /// The right button in the editor: stops placing.
    pub(super) fn editor_right(&mut self, pressed: bool) -> bool {
        match &mut self.editor {
            Some(e) if pressed && e.pick.is_some() => {
                e.pick = None;
                true
            }
            _ => false,
        }
    }

    /// The wheel over the panel scrolls its list.
    pub fn editor_wheel(&mut self, ctx: &Ctx, lines: f32) -> bool {
        let Some(at) = self.cursor else { return false };
        let p = self.canvas_point(at);
        if !Editor::bounds(&self.editor_items(ctx)).contains(p) {
            return false;
        }
        if let (Some(e), Some(world)) = (&mut self.editor, &self.world) {
            e.wheel(world, lines);
        }
        true
    }

    /// F2 folds the panel.
    pub(super) fn editor_key(&mut self, key: &winit::keyboard::Key, pressed: bool) -> bool {
        match (&mut self.editor, key) {
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
    fn levels_apply_none_half_or_all_upgrades() {
        assert_eq!([0, 1, 2].map(|l| level_count(l, 7)), [0, 4, 7]);
        assert_eq!([0, 1, 2].map(|l| level_count(l, 0)), [0, 0, 0]);
        let mut e = Editor::default();
        e.level = (e.level as i32 - 1).rem_euclid(LEVELS.len() as i32) as usize;
        assert_eq!(LEVELS[e.level].1, "Voll", "stepping back from Basis wraps to Voll");
    }
}
