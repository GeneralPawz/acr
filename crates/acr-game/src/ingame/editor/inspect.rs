//! The unit inspector (#499; main menu "Inspektor"): a third view of the editor's window for
//! looking at one unit type at a time and playing its moves. Not in the original.
//!
//! A nation and one of its units (foot, riders, artillery) are picked with arrows; the unit
//! stands alone in the middle of the blank field and the camera goes there. Two things can
//! then be played:
//!
//! - **A clip of its 3D model**, looped on the unit whatever the simulation does: the list is
//!   the model's own clips (`ThreeD::inspect`), so every drill it carries can be looked at
//!   without an enemy. Render only; nothing of it reaches the world or a savegame.
//! - **A fight**: an enemy is put in front of the unit (on foot, mounted or on the ground;
//!   standing still or fighting back; near or far) and the unit is ordered to attack it, so
//!   the blow, the reach and the contact are the simulation's.
//!
//! Every change of the scene is one editor input ([`Input::Inspect`]): it removes what the
//! inspector placed before and places the scene anew, so a savegame replays it.

use acr_formats::m3d::MapSize;
use acr_sim::body::{self, Injury, KNEE, LimbId, Outcome, Part};
use acr_sim::{EntityId, Pos, TypeId, World};

use super::input::Input;
use super::touch::TouchControl;
use super::{Act as EditorAct, ENTRIES, Editor, FACINGS, FIRST_EXTRA, Item, Look, OY, Pick, ROW, W, X0, groups, sides};
use crate::ui::dialog::Reject;
use crate::ui::draw::Rect;

/// The life of the enemy (and of the unit when the enemy fights back): the fight lasts.
const LIFE: i32 = 20_000;
/// How far in front of the unit the enemy stands, world pixels: near (a man on foot, a rider,
/// a man on the ground) and far (musket and bow range).
const NEAR: [i32; 3] = [110, 140, 70];
const FAR: i32 = 480;
/// The close-up the first scene of an inspector takes: output pixels per original pixel, and
/// how far above the ground the middle of the window looks (true pixels, a man's chest).
const ZOOM: f32 = 4.0;
pub const LIFT: f32 = 27.0;
/// The scene stands this share of the window's width left of its middle.
pub const ASIDE: f32 = 0.17;
/// Clip lines of the classic window per page.
const PAGE: usize = 10;
/// The enemies, by the inspected unit's nation: a line musketeer and a dragoon of England,
/// or of France against the English.
const FOOT: [&str; 2] = ["EngM18(EN)", "FrnM18(FR)"];
const RIDER: [&str; 2] = ["EngD18(EN)", "FrnD18(FR)"];

/// Who stands in front of the inspected unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Enemy {
    #[default]
    None,
    /// A man on foot who stands still.
    Foot,
    /// A man on foot who fights back.
    FootFights,
    Rider,
    RiderFights,
    /// A man on the ground (a leg lost at the knee: he crawls).
    Down,
}

impl Enemy {
    pub const ALL: [Enemy; 6] =
        [Enemy::None, Enemy::Foot, Enemy::FootFights, Enemy::Rider, Enemy::RiderFights, Enemy::Down];

    pub fn label(self) -> &'static str {
        match self {
            Enemy::None => "keiner",
            Enemy::Foot => "zu Fuss, steht",
            Enemy::FootFights => "zu Fuss, kaempft",
            Enemy::Rider => "Reiter, steht",
            Enemy::RiderFights => "Reiter, kaempft",
            Enemy::Down => "am Boden",
        }
    }

    /// The number in savegames.
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    fn fights(self) -> bool {
        matches!(self, Enemy::FootFights | Enemy::RiderFights)
    }

    /// The index in [`NEAR`].
    fn distance(self) -> usize {
        match self {
            Enemy::Rider | Enemy::RiderFights => 1,
            Enemy::Down => 2,
            _ => 0,
        }
    }
}

/// A click on the inspector's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Switches between the unit list and the inspector.
    Toggle,
    Nation(i32),
    Unit(i32),
    Enemy(i32),
    /// Near or far.
    Range,
    Facing(i32),
    /// Plays clip `n` of the model's list on the unit.
    Clip(usize),
    /// Back to the simulation's own animation.
    Sim,
    /// Places the scene again (the fight starts over).
    Again,
    /// Another page of the clip list (the classic window).
    Page(i32),
}

/// The inspector's state.
#[derive(Debug, Clone)]
pub struct Inspector {
    pub on: bool,
    nation: usize,
    /// An index in [`kinds`] of the nation.
    unit: usize,
    enemy: Enemy,
    far: bool,
    dir: u8,
    /// The clip played on the unit instead of the simulation's (render only).
    pub clip: Option<String>,
    /// The clips of the unit's 3D model, once it is loaded and drawn (`ThreeD::inspect`).
    pub clips: Vec<String>,
    /// Whether 3D models are drawn at all (the setting `models`).
    pub models: bool,
    page: usize,
    placed: Vec<EntityId>,
    /// The inspected unit and the enemy in front of it.
    pub subject: Option<EntityId>,
    pub target: Option<EntityId>,
    /// The scene has not been placed yet (a fresh inspector game places it by itself).
    pub pending: bool,
}

impl Default for Inspector {
    fn default() -> Self {
        Self {
            on: false,
            nation: super::DEFAULT_ENTRY,
            unit: 0,
            enemy: Enemy::None,
            far: false,
            // South-east: towards the fixed camera.
            dir: 32,
            clip: None,
            clips: Vec::new(),
            models: true,
            page: 0,
            placed: Vec::new(),
            subject: None,
            target: None,
            pending: false,
        }
    }
}

/// The units of a nation the inspector shows: foot, riders and artillery, in the order of
/// the editor's list.
pub fn kinds(world: &World, nation: usize) -> Vec<TypeId> {
    groups(world, nation)
        .into_iter()
        .filter(|g| matches!(g.header, "Infanterie" | "Kavallerie" | "Artillerie"))
        .flat_map(|g| g.kinds)
        .filter_map(|p| match p {
            Pick::Unit(k) => Some(k),
            _ => None,
        })
        .collect()
}

fn find(world: &World, id: &str) -> Option<TypeId> {
    world.catalog.iter().find(|(_, t)| t.id == id).map(|(k, _)| k)
}

/// The enemy's type for a unit with the id `subject`: England's, or France's against the
/// English; the unit's own type when the game has neither.
fn enemy_kind(world: &World, subject: TypeId, enemy: Enemy) -> TypeId {
    let names = if matches!(enemy, Enemy::Rider | Enemy::RiderFights) { RIDER } else { FOOT };
    let english = world.catalog.get(subject).id.to_ascii_lowercase().starts_with("eng");
    let (first, second) = if english { (names[1], names[0]) } else { (names[0], names[1]) };
    find(world, first).or_else(|| find(world, second)).unwrap_or(subject)
}

impl Inspector {
    fn kind(&self, world: &World) -> Option<TypeId> {
        let all = kinds(world, self.nation);
        all.get(self.unit.min(all.len().saturating_sub(1))).copied()
    }

    /// The input that places the scene as the view has it, or none for a nation without units.
    fn input(&self, world: &World) -> Option<Input> {
        let kind = self.kind(world)?;
        Some(Input::Inspect {
            kind: world.catalog.get(kind).id.clone(),
            enemy: self.enemy,
            far: self.far,
            dir: self.dir,
        })
    }

    /// What a fresh inspector game places first.
    pub fn first(&mut self, world: &World) -> Option<Input> {
        self.pending = false;
        self.input(world)
    }
}

impl Editor {
    /// A click on the inspector's view: its own state, and the input that places the scene
    /// anew when the click changes it.
    pub(super) fn inspect_act(&mut self, world: &World, act: Act) -> Option<Input> {
        let i = &mut self.inspect;
        let step = |v: usize, d: i32, n: usize| (v as i32 + d).rem_euclid(n.max(1) as i32) as usize;
        match act {
            Act::Toggle => {
                i.on = !i.on;
                return if i.on && i.subject.is_none() { i.input(world) } else { None };
            }
            Act::Nation(d) => {
                i.nation = step(i.nation, d, FIRST_EXTRA);
                (i.unit, i.clip, i.page) = (0, None, 0);
            }
            Act::Unit(d) => {
                i.unit =
                    step(i.unit.min(kinds(world, i.nation).len().saturating_sub(1)), d, kinds(world, i.nation).len());
                (i.clip, i.page) = (None, 0);
            }
            Act::Enemy(d) => {
                let at = Enemy::ALL.iter().position(|&e| e == i.enemy).unwrap_or(0);
                i.enemy = Enemy::ALL[step(at, d, Enemy::ALL.len())];
                // A fight is the simulation's: the forced clip ends.
                i.clip = None;
            }
            Act::Range => i.far = !i.far,
            Act::Facing(d) => i.dir = i.dir.wrapping_add((d * 32) as u8),
            Act::Clip(n) => {
                i.clip = i.clips.get(n).cloned();
                return None;
            }
            Act::Sim => {
                i.clip = None;
                return None;
            }
            Act::Again => {}
            Act::Page(d) => {
                i.page = step(i.page, d, i.clips.len().div_ceil(PAGE));
                return None;
            }
        }
        i.input(world)
    }

    /// Places the inspector's scene ([`Input::Inspect`]): what it placed before goes, the unit
    /// of type `kind` stands in the middle of the map facing `dir`, the enemy in front of it,
    /// and the unit attacks him. Returns the text for the status line.
    pub(super) fn inspect_show(&mut self, world: &mut World, kind: &str, enemy: Enemy, far: bool, dir: u8) -> Vec<u8> {
        // The first scene zooms in; later ones keep the zoom the player chose.
        let first = self.inspect.subject.is_none();
        super::battle::undo(world, std::mem::take(&mut self.inspect.placed));
        (self.inspect.subject, self.inspect.target) = (None, None);
        let Some(kind) = find(world, kind) else { return format!("Unbekannter Typ {kind}").into_bytes() };
        let mid = MapSize { shift: world.map.shift }.playable_size() / 2;
        let subject = world.spawn(kind, 0, Pos::from_pixels(mid, mid), dir);
        let mut placed = vec![subject];
        let mut target = None;
        if enemy != Enemy::None {
            let d = if far && enemy != Enemy::Down { FAR } else { NEAR[enemy.distance()] };
            let (dx, dy) = ((acr_sim::trig::cos(dir) * d) >> 8, (acr_sim::trig::sin(dir) * d) >> 8);
            let t = world.spawn(
                enemy_kind(world, kind, enemy),
                1,
                Pos::from_pixels(mid + dx, mid + dy),
                dir.wrapping_add(128),
            );
            let u = &mut world.units[t.0 as usize];
            (u.life, u.max_life) = (LIFE, LIFE);
            u.no_search_victim = !enemy.fights();
            if enemy == Enemy::Down {
                let leg = Injury {
                    part: Part::Limb(LimbId::LegL),
                    level: KNEE,
                    outcome: Outcome::Off,
                    energy: 0,
                    cause: body::Cause::Cannonball,
                };
                body::apply(&mut u.body, &leg);
            }
            if enemy.fights() {
                let s = &mut world.units[subject.0 as usize];
                (s.life, s.max_life) = (LIFE, LIFE);
            }
            placed.push(t);
            target = Some(t);
        }
        // A fight needs Kampf; the alliances follow the Kampf rule as after every placement.
        if target.is_some() && self.sides == sides::Mode::Off {
            self.sides = sides::Mode::Free;
        }
        self.apply_sides(world);
        if let Some(t) = target {
            acr_sim::combat::attack_obj(world, subject.0 as usize, t.0 as usize, 0xFE);
            if enemy.fights() {
                acr_sim::combat::attack_obj(world, t.0 as usize, subject.0 as usize, 0xFE);
            }
        }
        let i = &mut self.inspect;
        (i.placed, i.subject, i.target) = (placed, Some(subject), target);
        (i.enemy, i.far, i.dir, i.on, i.pending) = (enemy, far, dir, true, false);
        // The view follows a replayed input too.
        if let Some(n) = (0..FIRST_EXTRA).find(|&n| kinds(world, n).contains(&kind)) {
            i.nation = n;
            i.unit = kinds(world, n).iter().position(|&k| k == kind).unwrap_or(0);
        }
        // Between the two when the enemy stands far off.
        let look = target.map_or((mid, mid), |t| {
            let (x, y) = world.units[t.0 as usize].pos.pixels();
            ((mid + x) / 2, (mid + y) / 2)
        });
        self.camera_to = Some(look);
        self.camera_zoom = first.then_some(ZOOM);
        let id = &world.catalog.get(kind).id;
        match enemy {
            Enemy::None => id.clone().into_bytes(),
            e => format!("{id} gegen {}", e.label()).into_bytes(),
        }
    }

    /// The lines of the view: `(text, name, lit, act)`, steppers as `(less, more)`.
    fn inspect_lines(&self, world: &World, name_of: &dyn Fn(TypeId) -> Vec<u8>) -> Vec<Line> {
        let i = &self.inspect;
        let mut out = Vec::new();
        let all = kinds(world, i.nation);
        let stepper = |text: Vec<u8>, id: &'static str, name: &'static str, less: Act, more: Act| Line::Stepper {
            text,
            id,
            name,
            less,
            more,
        };
        out.push(stepper(
            format!("Nation: {}", ENTRIES[i.nation].0).into_bytes(),
            "nation",
            "Inspektor Nation",
            Act::Nation(-1),
            Act::Nation(1),
        ));
        let unit = match i.kind(world) {
            Some(k) => [format!("{}/{} ", i.unit.min(all.len() - 1) + 1, all.len()).into_bytes(), name_of(k)].concat(),
            None => b"keine Einheiten".to_vec(),
        };
        out.push(stepper(unit, "unit", "Inspektor Einheit", Act::Unit(-1), Act::Unit(1)));
        out.push(stepper(
            format!("Gegner: {}", i.enemy.label()).into_bytes(),
            "enemy",
            "Inspektor Gegner",
            Act::Enemy(-1),
            Act::Enemy(1),
        ));
        let range = if i.far { "Abstand: fern (Schuss)" } else { "Abstand: nah (Nahkampf)" };
        out.push(Line::Button {
            text: range.into(),
            id: "range".into(),
            name: "Inspektor Abstand".into(),
            on: i.far,
            act: Act::Range,
        });
        out.push(stepper(
            format!("Richtung: {}", FACINGS[usize::from(i.dir / 32) & 7]).into_bytes(),
            "facing",
            "Inspektor Richtung",
            Act::Facing(-1),
            Act::Facing(1),
        ));
        out.push(Line::Button {
            text: b"Neu aufstellen".to_vec(),
            id: "again".into(),
            name: "Inspektor Neu".into(),
            on: false,
            act: Act::Again,
        });
        let head = if !i.models {
            "3D-Modelle sind aus (Spieleinstellungen)".to_owned()
        } else if i.clips.is_empty() {
            "Kein 3D-Modell fuer diese Einheit".to_owned()
        } else {
            format!("Bewegungen des Modells ({}):", i.clips.len())
        };
        out.push(Line::Text(head.into_bytes()));
        if !i.clips.is_empty() {
            out.push(Line::Button {
                text: b"wie die Simulation".to_vec(),
                id: "sim".into(),
                name: "Inspektor Simulation".into(),
                on: i.clip.is_none(),
                act: Act::Sim,
            });
        }
        for (n, c) in i.clips.iter().enumerate() {
            out.push(Line::Button {
                text: c.clone().into_bytes(),
                id: format!("clip/{n}"),
                name: format!("Bewegung {c}"),
                on: i.clip.as_ref() == Some(c),
                act: Act::Clip(n),
            });
        }
        out
    }

    /// The inspector for the touch drawer (`touch.rs`): every line, the whole clip list.
    pub(super) fn inspect_touch(&self, world: &World, name_of: &dyn Fn(TypeId) -> Vec<u8>) -> Vec<TouchControl> {
        self.inspect_lines(world, name_of)
            .into_iter()
            .map(|l| match l {
                Line::Stepper { text, id, .. } => TouchControl::Stepper {
                    less: format!("editor/inspect/{id}/-"),
                    more: format!("editor/inspect/{id}/+"),
                    text,
                },
                Line::Button { text, id, on, .. } => {
                    TouchControl::Button { id: format!("editor/inspect/{id}"), text, on }
                }
                Line::Text(text) => TouchControl::Line { text },
            })
            .collect()
    }

    /// The inspector's act for the drawer's id below `editor/inspect/`.
    pub(super) fn inspect_intent(&self, tail: &[&str]) -> Result<Act, Reject> {
        let sign = |s: &str| match s {
            "-" => Ok(-1),
            "+" => Ok(1),
            _ => Err(Reject::NoWidget),
        };
        match tail {
            ["nation", d] => Ok(Act::Nation(sign(d)?)),
            ["unit", d] => Ok(Act::Unit(sign(d)?)),
            ["enemy", d] => Ok(Act::Enemy(sign(d)?)),
            ["facing", d] => Ok(Act::Facing(sign(d)?)),
            ["range"] => Ok(Act::Range),
            ["again"] => Ok(Act::Again),
            ["sim"] if !self.inspect.clips.is_empty() => Ok(Act::Sim),
            ["sim"] => Err(Reject::Hidden),
            ["clip", n] => {
                let n: usize = n.parse().map_err(|_| Reject::NoWidget)?;
                if n < self.inspect.clips.len() { Ok(Act::Clip(n)) } else { Err(Reject::Range) }
            }
            _ => Err(Reject::NoWidget),
        }
    }

    /// The inspector's lines inside the classic window: the clip list by pages of [`PAGE`].
    pub(super) fn inspect_items(&self, world: &World, name_of: &dyn Fn(TypeId) -> Vec<u8>) -> Vec<Item> {
        let mut out = Vec::new();
        let mut y = OY + 4;
        let item = |rect: Rect, text: Vec<u8>, name: String, hot: bool, act: Option<Act>| Item {
            rect,
            text,
            name,
            hot,
            act: act.map(EditorAct::Inspect),
            look: Look::Text,
            gp: None,
        };
        let lines = self.inspect_lines(world, name_of);
        let clips = lines.iter().filter(|l| matches!(l, Line::Button { act: Act::Clip(_), .. })).count();
        let pages = clips.div_ceil(PAGE).max(1);
        let page = self.inspect.page.min(pages - 1);
        for l in lines {
            match l {
                Line::Stepper { mut text, name, less, more, .. } => {
                    text.truncate(34);
                    out.push(item(Rect::new(X0, y, 24, ROW), b"<".to_vec(), format!("{name} -"), false, Some(less)));
                    out.push(item(Rect::new(X0 + 24, y, W - 48, ROW), text, name.to_owned(), false, None));
                    out.push(item(
                        Rect::new(X0 + W - 24, y, 24, ROW),
                        b">".to_vec(),
                        format!("{name} +"),
                        false,
                        Some(more),
                    ));
                }
                Line::Button { act: Act::Clip(n), .. } if n / PAGE != page => continue,
                Line::Button { mut text, name, on, act, .. } => {
                    // The window's font has no underscore.
                    text.iter_mut().filter(|b| **b == b'_').for_each(|b| *b = b' ');
                    out.push(item(Rect::new(X0, y, W, ROW), text, name, on, Some(act)));
                }
                Line::Text(text) => {
                    // The window's divider lies under the first four lines; the list stands below.
                    y = y.max(OY + 80 + 2 * ROW + 4);
                    out.push(item(Rect::new(X0, y, W, ROW), text, String::new(), false, None));
                }
            }
            y += ROW;
            if y == OY + 4 + 4 * ROW {
                y = OY + 80;
            }
        }
        if pages > 1 {
            let text = format!("Seite {}/{pages}", page + 1).into_bytes();
            out.push(item(
                Rect::new(X0, y, 24, ROW),
                b"<".to_vec(),
                "Inspektor Seite -".into(),
                false,
                Some(Act::Page(-1)),
            ));
            out.push(item(Rect::new(X0 + 24, y, W - 48, ROW), text, "Inspektor Seite".into(), false, None));
            out.push(item(
                Rect::new(X0 + W - 24, y, 24, ROW),
                b">".to_vec(),
                "Inspektor Seite +".into(),
                false,
                Some(Act::Page(1)),
            ));
            y += ROW;
        }
        let mut status = self.status.clone();
        status.truncate(44);
        out.push(item(Rect::new(X0, y + 4, W, ROW), status, String::new(), false, None));
        out
    }
}

/// A line of the view, before it becomes a window item or a drawer control.
enum Line {
    Stepper { text: Vec<u8>, id: &'static str, name: &'static str, less: Act, more: Act },
    Button { text: Vec<u8>, id: String, name: String, on: bool, act: Act },
    Text(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingame::editor::presets::tests::world;

    fn plain(k: TypeId) -> Vec<u8> {
        format!("{k:?}").into_bytes()
    }

    #[test]
    fn enemies_keep_their_codes() {
        for (n, e) in Enemy::ALL.into_iter().enumerate() {
            assert_eq!(usize::from(e.code()), n);
            assert_eq!(Enemy::from_code(e.code()), Some(e));
        }
        assert_eq!(Enemy::from_code(6), None);
    }

    #[test]
    fn the_drawer_ids_come_back_as_acts() {
        let mut e = Editor::default();
        assert_eq!(e.inspect_intent(&["nation", "+"]), Ok(Act::Nation(1)));
        assert_eq!(e.inspect_intent(&["unit", "-"]), Ok(Act::Unit(-1)));
        assert_eq!(e.inspect_intent(&["enemy", "+"]), Ok(Act::Enemy(1)));
        assert_eq!(e.inspect_intent(&["range"]), Ok(Act::Range));
        assert_eq!(e.inspect_intent(&["again"]), Ok(Act::Again));
        assert_eq!(e.inspect_intent(&["sim"]), Err(Reject::Hidden), "no model, no list");
        assert_eq!(e.inspect_intent(&["clip", "0"]), Err(Reject::Range));
        e.inspect.clips = vec!["STAND".into(), "ATTACK".into()];
        assert_eq!(e.inspect_intent(&["clip", "1"]), Ok(Act::Clip(1)));
        assert_eq!(e.inspect_intent(&["clip", "2"]), Err(Reject::Range));
        assert_eq!(e.inspect_intent(&["sim"]), Ok(Act::Sim));
        assert_eq!(e.inspect_intent(&["nope"]), Err(Reject::NoWidget));
    }

    /// England's units on the real data: the first one stands in the middle, an enemy on
    /// foot is placed in front of it and attacked without hitting back, the next scene
    /// replaces the first, and a clip is only the view's state.
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn the_scene_is_placed_attacked_and_replaced() {
        let mut w = world();
        let mut e = Editor::default();
        let all = kinds(&w, 2);
        assert!(all.len() > 5, "England's units: {}", all.len());
        assert!(all.iter().all(|&k| !w.catalog.get(k).building));
        let Some(Input::Inspect { kind, enemy, far, dir }) = e.inspect_act(&w, Act::Toggle) else { panic!("no input") };
        assert_eq!((enemy, far, dir), (Enemy::None, false, 32));
        e.inspect_show(&mut w, &kind, enemy, far, dir);
        assert!(e.inspect.on && e.inspect.subject.is_some() && e.inspect.target.is_none());
        assert_eq!(w.units.iter().filter(|u| u.alive).count(), 1);
        assert_eq!(e.sides, sides::Mode::Off, "no enemy, no Kampf");
        // A line musketeer (the first unit is the peasant, whom an enemy soldier captures)
        // against an enemy on foot who stands still.
        let musketeer = all.iter().position(|&k| w.catalog.get(k).id == "EngM18(EN)").expect("EngM18");
        e.inspect.unit = musketeer;
        let Some(Input::Inspect { kind, enemy, far, dir }) = e.inspect_act(&w, Act::Enemy(1)) else { panic!() };
        assert_eq!((kind.as_str(), enemy), ("EngM18(EN)", Enemy::Foot));
        e.inspect_show(&mut w, &kind, enemy, far, dir);
        assert_eq!(w.units.iter().filter(|u| u.alive).count(), 2, "the first scene is gone");
        let (s, t) = (e.inspect.subject.unwrap().0 as usize, e.inspect.target.unwrap().0 as usize);
        assert_eq!((w.units[s].owner, w.units[t].owner), (0, 1));
        assert_eq!(e.sides, sides::Mode::Free);
        let life = w.units[s].life;
        for _ in 0..400 {
            let cmds = e.before_step(&mut w);
            w.step(&cmds);
        }
        assert!(w.units[t].alive && w.units[t].life < LIFE, "he was struck: {}", w.units[t].life);
        assert_eq!(w.units[s].life, life, "and did not hit back");
        // A clip is the view's own; the lists name it.
        e.inspect.clips = (0..25).map(|n| format!("CLIP{n}")).collect();
        assert_eq!(e.inspect_act(&w, Act::Clip(12)), None);
        assert_eq!(e.inspect.clip.as_deref(), Some("CLIP12"));
        let touch = e.inspect_touch(&w, &plain);
        assert!(
            touch
                .iter()
                .any(|c| matches!(c, TouchControl::Button { id, on: true, .. } if id == "editor/inspect/clip/12"))
        );
        assert_eq!(
            touch.iter().filter(|c| matches!(c, TouchControl::Button { id, .. } if id.contains("/clip/"))).count(),
            25
        );
        let items = e.inspect_items(&w, &plain);
        assert_eq!(items.iter().filter(|i| i.name.starts_with("Bewegung ")).count(), PAGE);
        assert!(items.iter().any(|i| i.name == "Inspektor Seite +"));
        assert!(
            items.iter().all(|i| i.rect.x1 <= X0 + W && i.rect.y1 <= super::super::BOX_Y1 + 2 * ROW),
            "inside the window"
        );
        // The next unit: a new input, the clip forgotten.
        assert!(matches!(e.inspect_act(&w, Act::Unit(1)), Some(Input::Inspect { .. })));
        assert_eq!(e.inspect.clip, None);
    }
}
