//! The unit inspector (#499; main menu "Inspektor"): a third view of the editor's window for
//! looking at one unit type at a time and playing its moves. Not in the original.
//!
//! It is a model viewer: the game starts on a dark backdrop with 3D models and the free
//! camera on, the picked unit fills the picture ([`Inspector::fit`]), a drag on the picture
//! turns and tilts the camera round it, and the camera follows the unit every frame. A nation
//! and one of its units (foot, riders, artillery) are picked from a list (the touch drawer
//! shows the units' icons). Two things can then be played:
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
use super::{Act as EditorAct, ENTRIES, Editor, FIRST_EXTRA, Item, Look, OY, Pick, ROW, W, X0, groups, sides};
use crate::ui::dialog::Reject;
use crate::ui::draw::Rect;

/// The life of the enemy (and of the unit when the enemy fights back): the fight lasts.
const LIFE: i32 = 20_000;
/// How far in front of the unit the enemy stands, world pixels: near (a man on foot, a rider,
/// a man on the ground) and far (musket and bow range).
const NEAR: [i32; 3] = [110, 140, 70];
const FAR: i32 = 480;
/// How far above the ground the camera looks, true pixels: a man's chest.
pub const LIFT: f32 = 27.0;
/// The heights of a man on foot and of a rider or a gun, original pixels (30 to the metre).
const HEIGHTS: [f32; 2] = [62.0, 92.0];
/// The scene fills these shares of the free picture's height and width.
const FILL: [f32; 2] = [0.62, 0.9];
/// The zoom never leaves this range.
const ZOOMS: (f32, f32) = (1.0, 16.0);
/// A pan moves the scene's middle at most this share of the free picture off its middle, so
/// the unit never leaves the picture.
pub const PAN: f32 = 0.42;
/// A step of the turn buttons, radians (an eighth of a turn).
pub const TURN: f32 = std::f32::consts::FRAC_PI_4;
/// The free camera's first turn and tilt: from the front right, 20 degrees down.
pub const ORBIT: [f32; 2] = [0.0, 0.35];
/// Ticks between two looks at a fight that came to a rest.
const REJOIN: u64 = 16;
/// Clip lines of the classic window per page.
const PAGE: usize = 8;
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
    /// Picks nation `n` and unit `n` of the nation's list (the drawer's lists).
    NationSet(usize),
    Pick(usize),
    /// The next or the previous clip of the model's list ("wie die Simulation" between the
    /// last and the first).
    ClipStep(i32),
    /// Turns the camera round the unit by [`TURN`] steps.
    Turn(i32),
    /// Near or far.
    Range,
    /// The fight lasts (20000 life) or is fatal (the units' own life).
    Fatal,
    /// Blood, wounds and torn bodies on or off (render only).
    Gore,
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
    /// Where the camera looks: the middle of the scene, world pixels.
    pub look: (i32, i32),
    /// The scene's height and width (original pixels) the camera is to fit, once per scene.
    pub fit: Option<(f32, f32)>,
    /// Steps of the turn buttons the camera has not taken yet.
    pub turn: i32,
    /// Where the scene's middle stands off the middle of the free picture, output pixels:
    /// the player's pan. A new scene stands in the middle again.
    pub offset: [f32; 2],
    /// The offset when the pan in progress began.
    pub pan_base: Option<[f32; 2]>,
    /// How many units the nation's list has, as of the last placed scene.
    pub shown: usize,
    /// The fight is fatal: nobody gets extra life.
    fatal: bool,
    /// The gore switch as the player set it (`None`: as the settings have it), and what
    /// the 3D view draws now.
    pub gore: Option<bool>,
    pub gore_on: bool,
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
            look: (0, 0),
            fit: None,
            turn: 0,
            offset: [0.0; 2],
            pan_base: None,
            shown: 0,
            fatal: false,
            gore: None,
            gore_on: true,
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

/// The zoom at which a scene `(height, width)` original pixels large fills [`FILL`] of a free
/// picture `free` output pixels large.
pub fn zoom_for(scene: (f32, f32), free: [f32; 2]) -> f32 {
    (FILL[0] * free[1] / scene.0).min(FILL[1] * free[0] / scene.1).clamp(ZOOMS.0, ZOOMS.1)
}

/// The nation's units by the headers of the editor's list.
pub fn grouped(world: &World, nation: usize) -> Vec<(&'static str, Vec<TypeId>)> {
    groups(world, nation)
        .into_iter()
        .filter(|g| matches!(g.header, "Infanterie" | "Kavallerie" | "Artillerie"))
        .map(|g| {
            let kinds = g.kinds.into_iter().filter_map(|p| if let Pick::Unit(k) = p { Some(k) } else { None });
            (g.header, kinds.collect())
        })
        .collect()
}

impl Inspector {
    pub fn nation(&self) -> usize {
        self.nation
    }

    pub fn kind(&self, world: &World) -> Option<TypeId> {
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
            fatal: self.fatal,
            dir: self.dir,
        })
    }

    /// The middle of the scene as the units stand now: the unit, or the point half way to its
    /// enemy (a fallen one too, while he lies there), world pixels.
    pub fn middle(&self, world: &World) -> Option<(i32, i32)> {
        let at = |id: EntityId| world.units.get(id.0 as usize).map(|u| u.pos.pixels());
        let s = at(self.subject?)?;
        Some(match self.target.and_then(at) {
            Some(t) => ((s.0 + t.0) / 2, (s.1 + t.1) / 2),
            None => s,
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
            Act::NationSet(n) => {
                i.nation = n.min(FIRST_EXTRA - 1);
                (i.unit, i.clip, i.page) = (0, None, 0);
            }
            Act::Pick(n) => {
                i.unit = n.min(kinds(world, i.nation).len().saturating_sub(1));
                (i.clip, i.page) = (None, 0);
            }
            Act::ClipStep(d) => {
                // The list with the simulation's own animation in front.
                let at = i.clip.as_ref().and_then(|c| i.clips.iter().position(|x| x == c)).map_or(0, |n| n + 1);
                let next = step(at, d, i.clips.len() + 1);
                i.clip = next.checked_sub(1).and_then(|n| i.clips.get(n).cloned());
                i.page = next.saturating_sub(1) / PAGE;
                return None;
            }
            Act::Turn(d) => {
                i.turn += d;
                return None;
            }
            Act::Range => i.far = !i.far,
            Act::Fatal => i.fatal = !i.fatal,
            Act::Gore => {
                i.gore = Some(!i.gore_on);
                return None;
            }
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
    pub(super) fn inspect_show(
        &mut self,
        world: &mut World,
        kind: &str,
        enemy: Enemy,
        (far, fatal): (bool, bool),
        dir: u8,
    ) -> Vec<u8> {
        super::battle::undo(world, std::mem::take(&mut self.inspect.placed));
        (self.inspect.subject, self.inspect.target) = (None, None);
        let Some(kind) = find(world, kind) else { return format!("Unbekannter Typ {kind}").into_bytes() };
        let mid = MapSize { shift: world.map.shift }.playable_size() / 2;
        // Nobody is captured here (the rule's widest exemption): a peasant fights his fight.
        world.capture_rule = 3;
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
            if !fatal {
                (u.life, u.max_life) = (LIFE, LIFE);
            }
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
            if enemy.fights() && !fatal {
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
        (i.enemy, i.far, i.fatal, i.dir, i.on, i.pending) = (enemy, far, fatal, dir, true, false);
        // The view follows a replayed input too.
        if let Some(n) = (0..FIRST_EXTRA).find(|&n| kinds(world, n).contains(&kind)) {
            i.nation = n;
            i.unit = kinds(world, n).iter().position(|&k| k == kind).unwrap_or(0);
        }
        i.shown = kinds(world, i.nation).len();
        // The camera looks between the two and fits both.
        let tall = |k: TypeId| {
            let t = world.catalog.get(k);
            HEIGHTS[usize::from(
                acr_sim::spacing::is_rider(t) || t.motion_style == acr_sim::catalog::MotionStyle::ComplexRotate,
            )]
        };
        let (mut high, mut wide) = (tall(kind), tall(kind));
        i.look = (mid, mid);
        if let Some(t) = target {
            let u = &world.units[t.0 as usize];
            let (x, y) = u.pos.pixels();
            i.look = ((mid + x) / 2, (mid + y) / 2);
            high = high.max(tall(u.kind));
            wide += ((x - mid) as f32).hypot((y - mid) as f32);
        }
        i.fit = Some((high, wide));
        i.offset = [0.0; 2];
        let id = &world.catalog.get(kind).id;
        match enemy {
            Enemy::None => id.clone().into_bytes(),
            e => format!("{id} gegen {}", e.label()).into_bytes(),
        }
    }

    /// The lines of the view: `(text, name, lit, act)`, steppers as `(less, more)`.
    fn inspect_lines(&self, world: &World, name_of: &dyn Fn(TypeId) -> Vec<u8>, lists: bool) -> Vec<Line> {
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
        // The drawer picks the nation from its own list.
        if !lists {
            out.push(stepper(
                format!("Nation: {}", ENTRIES[i.nation].0).into_bytes(),
                "nation",
                "Inspektor Nation",
                Act::Nation(-1),
                Act::Nation(1),
            ));
        }
        let unit = match i.kind(world) {
            Some(k) => [format!("{}/{} ", i.unit.min(all.len() - 1) + 1, all.len()).into_bytes(), name_of(k)].concat(),
            None => b"keine Einheiten".to_vec(),
        };
        out.push(stepper(unit, "unit", "Inspektor Einheit", Act::Unit(-1), Act::Unit(1)));
        let playing = i.clip.clone().unwrap_or_else(|| "wie die Simulation".to_owned());
        out.push(stepper(
            format!("Bewegung: {playing}").into_bytes(),
            "clipstep",
            "Inspektor Bewegung",
            Act::ClipStep(-1),
            Act::ClipStep(1),
        ));
        out.push(stepper(b"Ansicht drehen".to_vec(), "turn", "Inspektor Drehen", Act::Turn(-1), Act::Turn(1)));
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
        let fight = if i.fatal { "Kampf: toedlich" } else { "Kampf: endlos (20000 Leben)" };
        out.push(Line::Button {
            text: fight.into(),
            id: "fatal".into(),
            name: "Inspektor Kampf".into(),
            on: i.fatal,
            act: Act::Fatal,
        });
        let gore = if i.gore_on { "Blut und Wunden: an" } else { "Blut und Wunden: aus" };
        out.push(Line::Button {
            text: gore.into(),
            id: "gore".into(),
            name: "Inspektor Blut".into(),
            on: i.gore_on,
            act: Act::Gore,
        });
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
        self.inspect_lines(world, name_of, true)
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
            ["nation", n] if n.parse::<usize>().is_ok() => {
                let n: usize = n.parse().map_err(|_| Reject::NoWidget)?;
                if n < FIRST_EXTRA { Ok(Act::NationSet(n)) } else { Err(Reject::Range) }
            }
            ["pick", n] => {
                let n: usize = n.parse().map_err(|_| Reject::NoWidget)?;
                if n < self.inspect.shown { Ok(Act::Pick(n)) } else { Err(Reject::Range) }
            }
            ["nation", d] => Ok(Act::Nation(sign(d)?)),
            ["unit", d] => Ok(Act::Unit(sign(d)?)),
            ["clipstep", d] => Ok(Act::ClipStep(sign(d)?)),
            ["turn", d] => Ok(Act::Turn(sign(d)?)),
            ["enemy", d] => Ok(Act::Enemy(sign(d)?)),
            ["facing", d] => Ok(Act::Facing(sign(d)?)),
            ["range"] => Ok(Act::Range),
            ["fatal"] => Ok(Act::Fatal),
            ["gore"] => Ok(Act::Gore),
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

    /// The inspector's part of a tick: a fight that came to a rest goes on. The unit attacks
    /// its enemy again when it stands without orders (a blow that made it flinch or step back
    /// ended the attack), and so does an enemy who fights back. Every [`REJOIN`] ticks;
    /// `attack_obj` itself refuses the dead, the allied and men out of the fight.
    pub(super) fn inspect_keep_fighting(&self, world: &mut World) {
        let i = &self.inspect;
        let (Some(s), Some(t)) = (i.subject, i.target) else { return };
        if !world.tick.0.is_multiple_of(REJOIN) {
            return;
        }
        let (s, t) = (s.0 as usize, t.0 as usize);
        let idle = |w: &World, n: usize| w.units.get(n).is_some_and(|u| u.alive && u.death == 0 && u.orders.is_empty());
        if idle(world, s) {
            acr_sim::combat::attack_obj(world, s, t, 0xFE);
        }
        if i.enemy.fights() && idle(world, t) {
            acr_sim::combat::attack_obj(world, t, s, 0xFE);
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
        let lines = self.inspect_lines(world, name_of, false);
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
                    y = y.max(OY + 80 + 2 * ROW + 4) + 2;
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
    fn the_zoom_fits_the_scene_into_the_free_picture() {
        // A man alone in a 1280 x 720 picture: his 62 px fill 62 % of the height.
        assert!((zoom_for((62.0, 62.0), [1280.0, 720.0]) - 7.2).abs() < 0.01);
        // With an enemy 480 px off the width decides.
        let far = zoom_for((92.0, 62.0 + 480.0), [1280.0, 720.0]);
        assert!((far - 0.9 * 1280.0 / 542.0).abs() < 0.01, "{far}");
        assert_eq!(zoom_for((62.0, 62.0), [40_000.0, 40_000.0]), ZOOMS.1);
        assert_eq!(zoom_for((62.0, 9000.0), [100.0, 100.0]), ZOOMS.0);
    }

    /// The clip stepper walks "wie die Simulation", then the model's clips, and round again;
    /// the turn buttons only leave steps for the camera.
    #[test]
    fn the_clip_stepper_goes_round_the_list() {
        let mut e = Editor::default();
        let w = World::new(
            std::sync::Arc::new(acr_sim::MapGrid::open(1)),
            std::sync::Arc::new(acr_sim::catalog::Catalog::new()),
            std::sync::Arc::new(acr_sim::formation::Formations::default()),
        );
        e.inspect.clips = vec!["STAND".into(), "ATTACK".into()];
        let mut seen = Vec::new();
        for _ in 0..4 {
            assert_eq!(e.inspect_act(&w, Act::ClipStep(1)), None);
            seen.push(e.inspect.clip.clone());
        }
        assert_eq!(seen, [Some("STAND".into()), Some("ATTACK".into()), None, Some("STAND".into())]);
        e.inspect_act(&w, Act::ClipStep(-1));
        assert_eq!(e.inspect.clip, None);
        e.inspect_act(&w, Act::ClipStep(-1));
        assert_eq!(e.inspect.clip.as_deref(), Some("ATTACK"));
        assert_eq!(e.inspect_act(&w, Act::Turn(1)), None);
        assert_eq!(e.inspect_act(&w, Act::Turn(1)), None);
        assert_eq!(e.inspect.turn, 2);
    }

    #[test]
    fn the_drawer_ids_come_back_as_acts() {
        let mut e = Editor::default();
        assert_eq!(e.inspect_intent(&["nation", "+"]), Ok(Act::Nation(1)));
        assert_eq!(e.inspect_intent(&["unit", "-"]), Ok(Act::Unit(-1)));
        assert_eq!(e.inspect_intent(&["nation", "3"]), Ok(Act::NationSet(3)));
        assert_eq!(e.inspect_intent(&["nation", "12"]), Err(Reject::Range));
        assert_eq!(e.inspect_intent(&["pick", "0"]), Err(Reject::Range), "nothing placed yet");
        assert_eq!(e.inspect_intent(&["clipstep", "+"]), Ok(Act::ClipStep(1)));
        assert_eq!(e.inspect_intent(&["turn", "-"]), Ok(Act::Turn(-1)));
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
        let Some(Input::Inspect { kind, enemy, far, fatal, dir }) = e.inspect_act(&w, Act::Toggle) else {
            panic!("no input")
        };
        assert_eq!((enemy, far, fatal, dir), (Enemy::None, false, false, 32));
        e.inspect_show(&mut w, &kind, enemy, (far, fatal), dir);
        assert!(e.inspect.on && e.inspect.subject.is_some() && e.inspect.target.is_none());
        assert_eq!(e.inspect.shown, all.len());
        let (high, wide) = e.inspect.fit.expect("the camera fits the first scene");
        assert_eq!((high, wide), (HEIGHTS[0], HEIGHTS[0]), "a man on foot alone");
        assert!(matches!(e.inspect_intent(&["pick", "5"]), Ok(Act::Pick(5))));
        assert_eq!(w.units.iter().filter(|u| u.alive).count(), 1);
        assert_eq!(e.sides, sides::Mode::Off, "no enemy, no Kampf");
        // A line musketeer (the first unit is the peasant, whom an enemy soldier captures)
        // against an enemy on foot who stands still.
        let musketeer = all.iter().position(|&k| w.catalog.get(k).id == "EngM18(EN)").expect("EngM18");
        e.inspect.unit = musketeer;
        let Some(Input::Inspect { kind, enemy, far, fatal, dir }) = e.inspect_act(&w, Act::Enemy(1)) else { panic!() };
        assert_eq!((kind.as_str(), enemy), ("EngM18(EN)", Enemy::Foot));
        e.inspect_show(&mut w, &kind, enemy, (far, fatal), dir);
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
        // A fatal fight: the enemy keeps his own life and falls.
        let Some(Input::Inspect { kind, enemy, far, fatal, dir }) = e.inspect_act(&w, Act::Fatal) else { panic!() };
        assert!(fatal);
        e.inspect_show(&mut w, &kind, enemy, (far, fatal), dir);
        let t = e.inspect.target.unwrap().0 as usize;
        assert!(w.units[t].life < LIFE, "his own life: {}", w.units[t].life);
        for _ in 0..3000 {
            let cmds = e.before_step(&mut w);
            w.step(&cmds);
        }
        assert!(!w.units[t].alive || w.units[t].death > 0, "he fell: life {}", w.units[t].life);
        // The peasant against a dragoon who fights back, fatal: nobody is captured, the fight
        // goes on after the first blows and ends with a death.
        e.inspect.unit = 0;
        let Some(Input::Inspect { kind, far, fatal, dir, .. }) = e.inspect_act(&w, Act::Again) else { panic!() };
        assert_eq!(kind, "EngKri(EN)");
        e.inspect_show(&mut w, &kind, Enemy::RiderFights, (far, fatal), dir);
        let (s, t) = (e.inspect.subject.unwrap().0 as usize, e.inspect.target.unwrap().0 as usize);
        for _ in 0..6000 {
            let cmds = e.before_step(&mut w);
            w.step(&cmds);
        }
        assert_eq!((w.units[s].owner, w.units[t].owner), (0, 1), "not captured");
        let fell = |n: usize| !w.units[n].alive || w.units[n].death > 0;
        assert!(fell(s) || fell(t), "one fell: lives {} and {}", w.units[s].life, w.units[t].life);
        assert_eq!(e.inspect.middle(&w), {
            let (a, b) = (w.units[s].pos.pixels(), w.units[t].pos.pixels());
            Some(((a.0 + b.0) / 2, (a.1 + b.1) / 2))
        });
        // The gore switch is the view's own: no input.
        assert_eq!(e.inspect_act(&w, Act::Gore), None);
        assert_eq!(e.inspect.gore, Some(false));
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

    /// The melee distance table of the Blender pipeline and the clip audit
    /// (`assets/blender/data/melee_distance.json`, `docs/melee.md` "Melee distance"): type id
    /// without the nation → metres, from `acr_sim::melee::strike_distance` against a man.
    fn melee_table(w: &World) -> String {
        use std::collections::BTreeMap;
        let man = find(w, FOOT[0]).map_or(0, |k| w.catalog.get(k).add_shot_radius);
        let m = |px: i32| (f64::from(px) / 30.0 * 100.0).round() / 100.0;
        let mut by_base: BTreeMap<String, BTreeMap<String, [f64; 3]>> = BTreeMap::new();
        for (_, t) in w.catalog.iter().filter(|(_, t)| !t.building) {
            let Some(d) = acr_sim::melee::strike_distance(t, man) else { continue };
            // Not a blow: a priest's or a healer's index without a weapon reaches 10 m and more.
            if !(10..=150).contains(&d.reach) {
                continue;
            }
            let base = t.id.split('(').next().unwrap_or(&t.id).to_string();
            // A strike stance of its own (`melee::STANCES`, the pikemen): the man stands
            // there under the hit-locations rule, within the tolerance.
            let v = match acr_sim::melee::stance_entry(t) {
                Some(s) => {
                    let tol = acr_sim::melee::STRIKE_TOL;
                    [m(s + man), m(s + man - tol), m(s + man + tol)]
                }
                None => [m(d.typical()), m(d.nearest()), m(d.reach)],
            };
            by_base.entry(base).or_default().insert(t.id.clone(), v);
        }
        let mut units = serde_json::Map::new();
        for (base, ids) in by_base {
            let same = ids.values().all(|v| Some(v) == ids.values().next());
            for (id, v) in ids {
                let key = if same { base.clone() } else { id };
                units.insert(key, serde_json::json!({"distance": v[0], "nearest": v[1], "reach": v[2]}));
            }
        }
        let doc = serde_json::json!({
            "about": "Centre-to-centre distance (m, 30 px = 1 m) at which a unit type's melee blow starts at a standing man: reach = ATTACK_RADIUS max + the man's ADDSHOTRADIUS, nearest = one walking cycle inside it, distance = the middle. A type with a strike stance of its own (acr_sim::melee::STANCES, the pikemen) has the stance as its distance, within the tolerance. Generated by the test the_melee_table_matches (ACR_MELEE_TABLE=write), docs/melee.md.",
            "units": units,
        });
        serde_json::to_string_pretty(&doc).unwrap() + "\n"
    }

    /// The committed table is what the rule gives on the real data; `ACR_MELEE_TABLE=write`
    /// rewrites it. With `ACR_MELEE_MEASURED` (the csv of `clip-audit/reach/dist.py`: the
    /// inspector's scene, the enemy 110 px off at direction 32) it prints the rule's walk-up
    /// against the distances measured in the running game.
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn the_melee_table_matches() {
        let w = world();
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/blender/data/melee_distance.json");
        let table = melee_table(&w);
        if std::env::var("ACR_MELEE_TABLE").is_ok_and(|v| v == "write") {
            std::fs::write(&path, &table).unwrap();
        }
        if let Some(csv) = std::env::var_os("ACR_MELEE_MEASURED") {
            let man = find(&w, FOOT[0]).map_or(0, |k| w.catalog.get(k).add_shot_radius);
            for line in std::fs::read_to_string(csv).unwrap().lines().skip(1) {
                let f: Vec<&str> = line.split(',').collect();
                let (Some(k), Ok(seen)) = (find(&w, f[0]), f[1].parse::<f32>()) else { continue };
                let t = w.catalog.get(k);
                let Some(d) = acr_sim::melee::strike_distance(t, man) else {
                    println!("MELEE {} measured {seen:.1} px: no melee index", f[0]);
                    continue;
                };
                // The walk up from 110 px at direction 32, a cycle at a time.
                let (mut x, mut y) = ((acr_sim::trig::cos(32) * NEAR[0]) >> 4, (acr_sim::trig::sin(32) * NEAR[0]) >> 4);
                let nf = t.motion_frames();
                let mut cycles = 0;
                while acr_sim::trig::norma(x, y) >> 4 >= d.reach && cycles < 200 {
                    x -= nf * t.step_dx(32);
                    y -= nf * t.step_dy(32);
                    cycles += 1;
                }
                let walk = ((x * x + y * y) as f32).sqrt() / 16.0;
                println!(
                    "MELEE {} measured {seen:.1} px ({}), rule {walk:.1} px after {cycles} cycles, off {:.1}; reach {} nearest {} stride {} index {}",
                    f[0],
                    f[3],
                    seen - walk,
                    d.reach,
                    d.nearest(),
                    d.stride,
                    d.index
                );
            }
        }
        let committed = std::fs::read_to_string(&path).expect("assets/blender/data/melee_distance.json");
        assert_eq!(committed, table, "regenerate with ACR_MELEE_TABLE=write");
    }

    /// The strike stance (#511) in the inspector's scene on the real data: a man on foot
    /// against a standing man 110 px off at direction 32. Without the rule the first blow
    /// comes from the original's band, with `hit_locations` from the stance. Prints the
    /// distances and the types whose `ATTACK_RADIUS` min is above the stance.
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn the_first_blow_comes_from_the_stance() {
        use acr_sim::melee::{STRIKE_TOL, strike_distance, strike_stance};
        let first = |md: &str, rule: bool| {
            let mut w = world();
            w.rules.hit_locations = rule;
            let id = w.catalog.iter().find(|(_, t)| t.md_name.eq_ignore_ascii_case(md)).map(|(_, t)| t.id.clone());
            let id = id.unwrap_or_else(|| panic!("no type {md}"));
            let mut e = Editor::default();
            e.inspect_show(&mut w, &id, Enemy::Foot, (false, false), 32);
            let t = e.inspect.target.unwrap().0;
            // The blade, not a thrown tomahawk.
            w.units[e.inspect.subject.unwrap().0 as usize].melee_only = true;
            for _ in 0..1500 {
                let cmds = e.before_step(&mut w);
                w.step(&cmds);
                if let Some(h) = w.hits.iter().find(|h| h.victim == t) {
                    return (id, (h.dir.0 as f32).hypot(h.dir.1 as f32));
                }
            }
            panic!("{md} never struck");
        };
        let w = world();
        let man = find(&w, FOOT[0]).map_or(0, |k| w.catalog.get(k).add_shot_radius);
        for md in ["EngPik", "EngM18", "EngOf17", "DelPik", "IroTam"] {
            let (id, off) = first(md, false);
            let (_, on) = first(md, true);
            let k = find(&w, &id).unwrap();
            let want = strike_stance(w.catalog.get(k), man).expect("a stance");
            println!("STANCE {id}: first blow from {off:.1} px without the rule, {on:.1} px with it (stance {want})");
            assert!((on - want as f32).abs() <= STRIKE_TOL as f32 + 1.5, "{id}: {on} px, stance {want}");
        }
        // A pikeman and a line musketeer at each other, 110 px apart: who struck from where.
        let mut w = world();
        w.rules.hit_locations = true;
        let mut e = Editor::default();
        e.inspect_show(&mut w, "EngPik(EN)", Enemy::FootFights, (false, true), 32);
        let (s, t) = (e.inspect.subject.unwrap().0, e.inspect.target.unwrap().0);
        w.units[t as usize].melee_only = true;
        let (mut pike, mut foe) = (Vec::new(), Vec::new());
        let mut seen = 0;
        for _ in 0..1500 {
            let cmds = e.before_step(&mut w);
            w.step(&cmds);
            let n = w.hits.iter().filter(|h| h.victim == s || h.victim == t).count();
            if n > seen {
                seen = n;
                let h = w.hits.iter().rfind(|h| h.victim == s || h.victim == t).unwrap();
                let d = (h.dir.0 as f32).hypot(h.dir.1 as f32).round();
                if h.victim == t { pike.push(d) } else { foe.push(d) }
            }
            if !w.units[s as usize].living() || !w.units[t as usize].living() {
                break;
            }
        }
        println!(
            "STANCE EngPik against an attacking {}: the pike's blows from {pike:?} px, the enemy's from {foe:?} px",
            w.catalog.get(w.units[t as usize].kind).id
        );
        assert!(pike.first().is_some_and(|&d| d > 100.0), "the pike strikes first from beyond the old reach");
        let mut above: Vec<String> = Vec::new();
        for (_, t) in w.catalog.iter().filter(|(_, t)| !t.building) {
            if let Some(d) = strike_distance(t, man)
                && strike_stance(t, man).is_none_or(|s| s != acr_sim::melee::STRIKE_STANCE + man)
            {
                above.push(format!(
                    "{} (min {}, reach {}, stance {:?})",
                    t.md_name,
                    d.min,
                    d.reach,
                    strike_stance(t, man)
                ));
            }
        }
        above.sort();
        above.dedup();
        println!("STANCE not at the default ({}): {}", above.len(), above.join("; "));
    }
}
