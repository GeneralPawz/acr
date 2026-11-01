//! The editor's second view, "Schlacht" (#404): two nations' army presets (`presets.rs`)
//! put facing each other across the middle of the map, in one click. Not in the original.
//!
//! The window switches between the unit list and this view with a button in its title row (a
//! combo box line would shift the nation indices the rest of the editor counts by). Seite 1
//! is player 1 and faces east, Seite 2 is player 2 and faces west. The map is square, so
//! which axis the line of battle runs along makes no difference to what fits; it runs
//! west-east, the armies' widths across the north-south axis. The fronts stand 1.5 times the
//! longest range of the armies' main infantry apart (at least [`MIN_GAP`]). A second
//! "Aufstellen" first removes the units the first one placed, so a battle can be re-rolled.
//!
//! "Aufstellen" only places. The swords button then starts the fight: Kampf is switched on and
//! the battle controller (`crate::tactics`) takes over the armies' orders. Each side is either
//! the player's (only its guns are automatic) or the Ki's (the whole controller).

use acr_formats::m3d::MapSize;
use acr_sim::World;
use acr_sim::formation;

use super::input::Input;
use super::presets::{self, Block, Doctrine, Era, Error, Role, Size};
use super::touch::TouchControl;
use super::{Act as EditorAct, ENTRIES, Editor};
use super::{CELL, FIRST_EXTRA, Item, Look, OY, ROW, W, X0, sides};
use crate::tactics::{self, Control, Duty, Style};
use crate::ui::Ctx;
use crate::ui::dialog::Reject;
use crate::ui::draw::Rect;

/// The fronts never stand closer than this, world pixels.
const MIN_GAP: i32 = 400;
/// The facings of the two sides: east and west.
const FACING: [u8; 2] = [0, 128];
/// The `ICONS.GP` icon of the fight button: two sabres on grey.
pub const FIGHT_ICON: &str = "ATAKA2";

/// A click on the battle view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Switches between the unit list and the battle view.
    Toggle,
    /// Another nation for side `0` or `1`, `d` lines down the list.
    Nation(usize, i32),
    /// Another century for the side.
    Era(usize, i32),
    Size(i32),
    /// Who commands side `0` or `1`: the player (guns only) or the Ki.
    Control(usize),
    /// The swords button: starts the fight.
    Fight,
    /// Places both armies.
    Go,
}

/// The battle view's state.
#[derive(Debug, Clone)]
pub struct Battle {
    pub on: bool,
    nation: [usize; 2],
    era: [Option<Era>; 2],
    size: Size,
    /// Whether the Ki commands the side (otherwise the player does, only the guns are automatic).
    pub ai: [bool; 2],
    /// The units the last "Aufstellen" placed.
    placed: Vec<acr_sim::EntityId>,
    /// The bodies of each side's army with the units that stand in them (officers, drummers
    /// and flag bearers too), in the order of the roster.
    pub bodies: [Vec<(Block, Vec<acr_sim::EntityId>)>; 2],
    /// Where each side's army stands: its origin in world pixels.
    pub at: [(i32, i32); 2],
    /// The battle controller, from the swords button until the next "Aufstellen".
    pub tactics: Option<tactics::Commander>,
}

impl Default for Battle {
    fn default() -> Self {
        Self {
            on: false,
            nation: [2, 3],
            era: [None, None],
            size: Size::Medium,
            ai: [false, true],
            placed: Vec::new(),
            bodies: [Vec::new(), Vec::new()],
            at: [(0, 0); 2],
            tactics: None,
        }
    }
}

impl Battle {
    /// The century in use for a side: the chosen one if the nation has it, else the default.
    fn era_of(&self, side: usize) -> Option<Era> {
        let all = presets::eras(self.nation[side]);
        self.era[side].filter(|e| all.contains(e)).or_else(|| presets::default_era(self.nation[side]))
    }
}

/// How far an army reaches around its origin in its own frame (forward and right-hand side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    pub front: i32,
    pub back: i32,
    pub left: i32,
    pub right: i32,
}

impl Extent {
    pub fn of(blocks: &[Block]) -> Self {
        let mut e = Extent { front: i32::MIN, back: i32::MAX, left: i32::MAX, right: i32::MIN };
        for b in blocks {
            e.front = e.front.max(b.forward + b.reach.front);
            e.back = e.back.min(b.forward + b.reach.back);
            e.left = e.left.min(b.side + b.reach.left);
            e.right = e.right.max(b.side + b.reach.right);
        }
        e
    }
}

/// Where the two armies' origins stand: side 0 facing east at the left of the middle, side 1
/// facing west at the right, their fronts `gap` apart and each centred across the line, all
/// of both at least [`presets::MAP_MARGIN`] inside a map `side` pixels wide.
pub fn origins(ext: [Extent; 2], gap: i32, side: i32) -> Result<[(i32, i32); 2], Error> {
    let mid = side / 2;
    let across = |e: Extent| (e.left + e.right) / 2;
    let a = (mid - gap / 2 - ext[0].front, mid - across(ext[0]));
    // Facing west the frame is turned half way round: forward and side both flip.
    let b = (mid + gap / 2 + ext[1].front, mid + across(ext[1]));
    let inside = |v: i32| (presets::MAP_MARGIN..=side - presets::MAP_MARGIN).contains(&v);
    let fits = inside(a.0 + ext[0].back)
        && inside(b.0 - ext[1].back)
        && inside(a.1 + ext[0].left)
        && inside(a.1 + ext[0].right)
        && inside(b.1 - ext[1].right)
        && inside(b.1 - ext[1].left);
    if fits { Ok([a, b]) } else { Err(Error::OffMap) }
}

/// The longest range of a side's main infantry, world pixels.
fn range(world: &World, blocks: &[Block]) -> i32 {
    blocks
        .iter()
        .filter(|b| b.role == Role::Front)
        .flat_map(|b| world.catalog.get(b.kind).attacks.iter().map(|a| a.radius.1))
        .max()
        .unwrap_or(0)
}

/// The gap between the fronts: 1.5 times the longer range, at least [`MIN_GAP`].
pub fn gap_of(ranges: [i32; 2]) -> i32 {
    (ranges[0].max(ranges[1]) * 3 / 2).max(MIN_GAP)
}

/// Places both armies of `battle` across the middle of the map; returns the soldiers placed
/// per side. Nothing stays placed when either army does not fit or stands in water.
fn deploy(world: &mut World, battle: &mut Battle) -> Result<[usize; 2], Error> {
    // The previous battle goes first, its units' ids kept (the slots are only freed).
    undo(world, std::mem::take(&mut battle.placed));
    battle.bodies = [Vec::new(), Vec::new()];
    let plans = [0, 1].map(|s| presets::blocks(world, battle.nation[s], battle.size, battle.era_of(s)));
    let [Ok(p0), Ok(p1)] = plans else { return Err(plans.into_iter().find_map(Result::err).expect("an error")) };
    let gap = gap_of([range(world, &p0), range(world, &p1)]);
    let side = MapSize { shift: world.map.shift }.playable_size();
    let at = origins([Extent::of(&p0), Extent::of(&p1)], gap, side)?;
    let before: Vec<bool> = world.units.iter().map(|u| u.alive).collect();
    let mut men = [0; 2];
    for (s, plan) in [p0, p1].into_iter().enumerate() {
        match presets::place_preset(world, battle.nation[s], battle.size, battle.era_of(s), s as u8, at[s], FACING[s]) {
            Ok(ids) => {
                men[s] = ids.iter().map(Vec::len).sum();
                battle.bodies[s] = plan.into_iter().zip(ids).collect();
            }
            Err(e) => {
                undo(world, fresh(world, &before));
                battle.bodies = [Vec::new(), Vec::new()];
                return Err(e);
            }
        }
    }
    battle.at = at;
    battle.placed = fresh(world, &before);
    Ok(men)
}

/// The units alive now that were not before.
fn fresh(world: &World, before: &[bool]) -> Vec<acr_sim::EntityId> {
    world
        .units
        .iter()
        .enumerate()
        .filter(|(i, u)| u.alive && !before.get(*i).copied().unwrap_or(false))
        .map(|(_, u)| u.id)
        .collect()
}

fn undo(world: &mut World, ids: Vec<acr_sim::EntityId>) {
    for id in ids {
        if let Some(b) = world.units.get(id.0 as usize).and_then(|u| u.brigade) {
            formation::dissolve(world, usize::from(b));
        }
        world.remove(id);
    }
}

/// The controller's armies for the bodies "Aufstellen" placed, or `None` without any.
fn armies(world: &World, battle: &Battle) -> Option<[tactics::Army; 2]> {
    if battle.bodies.iter().all(Vec::is_empty) {
        return None;
    }
    let make = |s: usize| {
        let doctrine = presets::roster(battle.nation[s], battle.era_of(s)).ok()?.doctrine;
        let blocks = &battle.bodies[s];
        // The rows behind the first one with main infantry are the second line.
        let front_row = blocks.iter().filter(|(b, _)| b.role == Role::Front).map(|(b, _)| b.row).min();
        let bodies = blocks
            .iter()
            .map(|(b, ids)| tactics::Body {
                duty: duty_of(world, s as u8, b, front_row),
                units: ids.clone(),
                lateral: b.side,
            })
            .collect();
        Some(tactics::Army {
            owner: s as u8,
            facing: FACING[s],
            origin: battle.at[s],
            style: style_of(doctrine),
            bodies,
        })
    };
    Some([make(0)?, make(1)?])
}

/// The doctrine's style in the controller.
fn style_of(d: Doctrine) -> Style {
    match d {
        Doctrine::PikeShot => Style::PikeShot,
        Doctrine::Linear => Style::Linear,
        Doctrine::Column => Style::Column,
        Doctrine::Lines19 => Style::Lines19,
        Doctrine::Warband => Style::Warband,
        Doctrine::Clans => Style::Clans,
        Doctrine::Riders => Style::Riders,
    }
}

/// What a roster role does in the battle: guns, skirmishers, shooters behind the line and
/// cavalry wings keep their duty; the main infantry fights as a line, or in melee when its
/// weapons reach no further than [`tactics::picture::MELEE_REACH`]; militia and everything
/// but guns, skirmishers and shooters in a row behind the first one with main infantry
/// (`front_row`) is the reserve.
fn duty_of(world: &World, owner: u8, b: &Block, front_row: Option<u8>) -> Duty {
    let behind = front_row.is_some_and(|f| b.row > f);
    match b.role {
        Role::Guns => Duty::Guns,
        Role::Screen => Duty::Screen,
        Role::Back => Duty::Missile,
        Role::Militia => Duty::Reserve { cavalry: false },
        Role::Wing if behind => Duty::Reserve { cavalry: true },
        Role::Wing => Duty::Cavalry,
        Role::Front | Role::Pike | Role::Elite if behind => Duty::Reserve { cavalry: false },
        Role::Front | Role::Pike | Role::Elite => {
            let adv = world.adv(owner, b.kind);
            let reach = adv.attacks.iter().filter(|a| a.weapon.is_some()).map(|a| a.radius.1).max().unwrap_or(0);
            if reach > tactics::picture::MELEE_REACH { Duty::Line } else { Duty::Melee }
        }
    }
}

impl Editor {
    /// A click on the battle view; "Aufstellen" returns the input that places the armies.
    pub(super) fn battle_act(&mut self, act: Act) -> Option<Input> {
        let b = &mut self.battle;
        let step = |v: usize, d: i32| (v as i32 + d).rem_euclid(FIRST_EXTRA as i32) as usize;
        match act {
            Act::Toggle => b.on = !b.on,
            Act::Nation(s, d) => {
                b.nation[s] = step(b.nation[s], d);
                b.era[s] = None;
            }
            Act::Era(s, d) => {
                let all = presets::eras(b.nation[s]);
                if let Some(i) = all.iter().position(|&e| Some(e) == b.era_of(s)) {
                    b.era[s] = Some(all[(i as i32 + d).rem_euclid(all.len() as i32) as usize]);
                }
            }
            Act::Size(d) => {
                let i = Size::ALL.iter().position(|&s| s == b.size).unwrap_or(0) as i32;
                b.size = Size::ALL[(i + d).rem_euclid(Size::ALL.len() as i32) as usize];
            }
            Act::Control(s) => {
                b.ai[s] = !b.ai[s];
                // While the fight runs the change is an input (logged, replayed).
                if b.tactics.is_some() {
                    return Some(Input::Fight { ai: b.ai });
                }
            }
            Act::Fight => return Some(Input::Fight { ai: b.ai }),
            Act::Go => {
                return Some(Input::Battle {
                    nation: b.nation,
                    era: [b.era_of(0), b.era_of(1)],
                    size: b.size,
                    fight: false,
                });
            }
        }
        None
    }

    /// Places both armies for [`Input::Battle`]: the sides as given, `fight` (an old savegame's
    /// "Kampf sofort") switching Kampf on, and the camera on the middle. The controller of an
    /// earlier fight ends. Returns the text for the status line.
    pub(super) fn go_to_battle(
        &mut self,
        world: &mut World,
        nation: [usize; 2],
        era: [Option<Era>; 2],
        size: Size,
        fight: bool,
    ) -> Vec<u8> {
        let b = &mut self.battle;
        (b.nation, b.era, b.size) = (nation, era, size);
        b.tactics = None;
        let result = deploy(world, b);
        if fight && self.sides == sides::Mode::Off {
            self.sides = sides::Mode::Free;
        }
        self.apply_sides(world);
        match result {
            Ok([a, c]) => {
                let side = MapSize { shift: world.map.shift }.playable_size();
                self.camera_to = Some((side / 2, side / 2));
                format!("Schlacht: {a} gegen {c} Mann").into_bytes()
            }
            Err(e) => e.to_string().into_bytes(),
        }
    }

    /// The swords button ([`Input::Fight`]): Kampf on, and the battle controller commands the
    /// armies placed (or, while it runs, `ai` changes who commands each side).
    pub(super) fn start_fight(&mut self, world: &mut World, ai: [bool; 2]) -> Vec<u8> {
        let b = &mut self.battle;
        b.ai = ai;
        let control = ai.map(|ai| if ai { Control::Ai } else { Control::Player });
        if let Some(t) = &mut b.tactics {
            t.set_control(control);
            return b"Steuerung geaendert".to_vec();
        }
        let Some(armies) = armies(world, b) else { return b"Erst aufstellen".to_vec() };
        b.tactics = Some(tactics::Commander::new(world, armies, control));
        if self.sides == sides::Mode::Off {
            self.sides = sides::Mode::Free;
        }
        self.apply_sides(world);
        b"Kampf".to_vec()
    }

    /// The battle view for the touch drawer (`touch.rs`), in the order of the classic lines.
    pub(super) fn battle_touch(&self, ctx: &Ctx) -> Vec<TouchControl> {
        let b = &self.battle;
        let mut out = Vec::new();
        let stepper = |name: String, text: String| TouchControl::Stepper {
            less: format!("editor/{name}/-"),
            more: format!("editor/{name}/+"),
            text: text.into_bytes(),
        };
        for s in 0..2 {
            out.push(stepper(format!("battle/nation/{s}"), format!("Seite {}: {}", s + 1, ENTRIES[b.nation[s]].0)));
            let era = b.era_of(s).map_or("ein Heer", Era::label).to_owned();
            if presets::eras(b.nation[s]).len() > 1 {
                out.push(stepper(format!("battle/era/{s}"), era));
            } else {
                out.push(TouchControl::Line { text: era.into_bytes() });
            }
        }
        out.push(stepper("battle/size".into(), format!("Groesse: {}", b.size.label())));
        for s in 0..2 {
            let who = if b.ai[s] { "KI" } else { "Spieler" };
            let text = format!("Seite {}: {who}", s + 1).into_bytes();
            out.push(TouchControl::Button { id: format!("editor/battle/control/{s}"), text, on: b.ai[s] });
        }
        out.push(TouchControl::Button { id: "editor/battle/go".into(), text: b"Aufstellen".to_vec(), on: false });
        let fight = Item {
            rect: Rect::new(0, 0, CELL, CELL),
            text: b"Kampf".to_vec(),
            name: "Kampf".into(),
            hot: b.tactics.is_some(),
            act: None,
            look: Look::Icon { frame: self.fight_icon.get() },
            gp: None,
        };
        out.push(TouchControl::Icon {
            id: "editor/battle/fight".into(),
            text: b"Kampf".to_vec(),
            on: b.tactics.is_some(),
            draws: super::touch::cell_draws(&fight, ctx),
        });
        out
    }

    /// The battle view's act for the drawer's id below `editor/battle/`.
    pub(super) fn battle_intent(&self, tail: &[&str]) -> Result<Act, Reject> {
        let side = |s: &str| match s {
            "0" => Ok(0),
            "1" => Ok(1),
            _ => Err(Reject::NoWidget),
        };
        let sign = |s: &str| match s {
            "-" => Ok(-1),
            "+" => Ok(1),
            _ => Err(Reject::NoWidget),
        };
        match tail {
            ["nation", s, d] => Ok(Act::Nation(side(s)?, sign(d)?)),
            ["era", s, d] => {
                let s = side(s)?;
                if presets::eras(self.battle.nation[s]).len() < 2 {
                    return Err(Reject::Hidden);
                }
                Ok(Act::Era(s, sign(d)?))
            }
            ["size", d] => Ok(Act::Size(sign(d)?)),
            ["control", s] => Ok(Act::Control(side(s)?)),
            ["go"] => Ok(Act::Go),
            ["fight"] => Ok(Act::Fight),
            _ => Err(Reject::NoWidget),
        }
    }

    /// The battle view's lines inside the window.
    pub(super) fn battle_items(&self) -> Vec<Item> {
        let b = &self.battle;
        let mut out = Vec::new();
        let mut y = OY + 4;
        let text = |y: i32, text: String, name: &str, hot: bool, act: Option<Act>| Item {
            rect: Rect::new(X0, y, W, ROW),
            text: text.into_bytes(),
            name: name.into(),
            hot,
            act: act.map(EditorAct::Battle),
            look: Look::Text,
            gp: None,
        };
        // A value with an arrow to each side, like the settings under the unit list.
        let pair = |out: &mut Vec<Item>, y: i32, value: String, name: &str, less: Act, more: Act| {
            let arrow = |x: i32, t: &str, d: &str, act: Act| Item {
                rect: Rect::new(x, y, 24, ROW),
                text: t.as_bytes().to_vec(),
                name: format!("{name} {d}"),
                hot: false,
                act: Some(EditorAct::Battle(act)),
                look: Look::Text,
                gp: None,
            };
            out.push(arrow(X0, "<", "-", less));
            out.push(Item { rect: Rect::new(X0 + 24, y, W - 48, ROW), ..text(y, value, name, false, None) });
            out.push(arrow(X0 + W - 24, ">", "+", more));
        };
        for s in 0..2 {
            let n = s + 1;
            let nation = format!("Seite {n}: {}", ENTRIES[b.nation[s]].0);
            pair(&mut out, y, nation, &format!("Seite {n} Nation"), Act::Nation(s, -1), Act::Nation(s, 1));
            y += ROW;
            let era = b.era_of(s).map_or("ein Heer", Era::label).to_owned();
            if presets::eras(b.nation[s]).len() > 1 {
                pair(&mut out, y, era, &format!("Seite {n} Jahrhundert"), Act::Era(s, -1), Act::Era(s, 1));
            } else {
                out.push(Item { rect: Rect::new(X0 + 24, y, W - 48, ROW), ..text(y, era, "", false, None) });
            }
            y += ROW;
        }
        // The window's divider lies under the two sides; the rest stands below it.
        y = OY + 80;
        pair(&mut out, y, format!("Groesse: {}", b.size.label()), "Groesse", Act::Size(-1), Act::Size(1));
        y += ROW;
        for s in 0..2 {
            let n = s + 1;
            let who = if b.ai[s] { "KI" } else { "Spieler" };
            out.push(text(
                y,
                format!("Seite {n}: {who}"),
                &format!("Seite {n} Steuerung"),
                false,
                Some(Act::Control(s)),
            ));
            y += ROW;
        }
        y += 6;
        out.push(text(y, "Aufstellen".into(), "Aufstellen", false, Some(Act::Go)));
        y += ROW + 6;
        // The swords: starts the fight; lit while it runs.
        let cell = Rect::new(X0 + 4, y, CELL, CELL);
        out.push(Item {
            rect: cell,
            text: b"Kampf".to_vec(),
            name: "Kampf".into(),
            hot: b.tactics.is_some(),
            act: Some(EditorAct::Battle(Act::Fight)),
            look: Look::Icon { frame: self.fight_icon.get() },
            gp: None,
        });
        y += CELL + 6;
        let mut status = self.status.clone();
        status.truncate(44);
        out.push(Item { text: status, ..text(y, String::new(), "", false, None) });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: i32 = 15360;

    /// A world without units or types: enough for the view's own state.
    fn empty() -> World {
        use acr_sim::catalog::Catalog;
        use acr_sim::formation::Formations;
        use std::sync::Arc;
        World::new(Arc::new(acr_sim::MapGrid::open(1)), Arc::new(Catalog::new()), Arc::new(Formations::default()))
    }

    fn ext(front: i32, back: i32, half: i32) -> Extent {
        Extent { front, back, left: -half, right: half }
    }

    #[test]
    fn the_fronts_stand_a_gap_apart_facing_each_other_across_the_middle() {
        let e = [ext(300, -900, 2000), ext(250, -1200, 3500)];
        let [a, b] = origins(e, 1200, SIDE).unwrap();
        // Facing east and west: the front edges.
        let (fa, fb) = (a.0 + e[0].front, b.0 - e[1].front);
        assert_eq!(fb - fa, 1200);
        assert_eq!((fa + fb) / 2, SIDE / 2);
        // Each is centred across the line.
        assert_eq!(a.1, SIDE / 2);
        assert_eq!(b.1, SIDE / 2);
        assert_eq!(FACING[1].wrapping_sub(FACING[0]), 128);
    }

    #[test]
    fn the_widest_army_still_fits_the_map() {
        // A Gross army is 4000 to 8700 px wide.
        let wide = ext(400, -1500, 4350);
        let [a, b] = origins([wide, wide], gap_of([700, 700]), SIDE).unwrap();
        assert!(a.0 - 1500 >= presets::MAP_MARGIN && b.0 + 1500 <= SIDE - presets::MAP_MARGIN);
        assert!(a.1 - 4350 >= presets::MAP_MARGIN && a.1 + 4350 <= SIDE - presets::MAP_MARGIN);
    }

    #[test]
    fn an_army_wider_than_the_map_is_refused() {
        assert_eq!(origins([ext(10, -10, 7600), ext(10, -10, 100)], 400, SIDE), Err(Error::OffMap));
    }

    #[test]
    fn the_gap_is_one_and_a_half_ranges_but_never_small() {
        assert_eq!(gap_of([640, 800]), 1200);
        assert_eq!(gap_of([0, 50]), MIN_GAP);
    }

    /// Every roster against itself in every size, on the 15360 px map: both armies inside the
    /// map on their own half, the fronts at least the gap apart, and a second "Aufstellen"
    /// replaces the first one's units (the same count stands, none of the old ones).
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn every_roster_deploys_inside_the_map_with_the_fronts_apart() {
        let base = presets::tests::world();
        let side = MapSize { shift: base.map.shift }.playable_size();
        let inside = |v: i32| (presets::MAP_MARGIN..=side - presets::MAP_MARGIN).contains(&v);
        for r in presets::ROSTERS {
            for size in Size::ALL {
                let mut w = base.clone();
                let mut battle = Battle { nation: [r.nation; 2], era: [r.era; 2], size, ..Battle::default() };
                let name = format!("{} {:?} {size:?}", ENTRIES[r.nation].0, r.era);
                let men = deploy(&mut w, &mut battle).expect(&name);
                assert!(men[0] > 0 && men[0] == men[1], "{name}: {men:?}");
                // Every body kept its units: alive, the owner's, soldiers and commanders.
                for (s, bodies) in battle.bodies.iter().enumerate() {
                    assert_eq!(bodies.len(), presets::blocks(&w, r.nation, size, r.era).unwrap().len(), "{name}");
                    assert_eq!(bodies.iter().map(|(_, ids)| ids.len()).sum::<usize>(), men[s], "{name}");
                    for (b, ids) in bodies {
                        let alive = |id: &acr_sim::EntityId| {
                            let u = &w.units[id.0 as usize];
                            u.alive && usize::from(u.owner) == s
                        };
                        assert!(!ids.is_empty() && ids.iter().all(alive), "{name}: {:?}", b.role);
                        let soldiers = ids.iter().filter(|id| w.units[id.0 as usize].kind == b.kind).count();
                        assert_eq!(soldiers, b.men, "{name}: {:?}", b.role);
                        // Formations bring an officer (and drummer, flag bearer); loose men none.
                        assert_eq!(ids.len() > b.men, b.form.is_some(), "{name}: {:?} commanders", b.role);
                    }
                }
                let first = battle.placed.clone();
                let x_of = |w: &World, owner: u8| -> Vec<i32> {
                    w.units.iter().filter(|u| u.alive && u.owner == owner).map(|u| u.pos.x >> 4).collect()
                };
                for owner in 0..2u8 {
                    let ys: Vec<_> =
                        w.units.iter().filter(|u| u.alive && u.owner == owner).map(|u| u.pos.y >> 4).collect();
                    assert!(x_of(&w, owner).into_iter().chain(ys).all(inside), "{name}: player {owner} off the map");
                }
                let (a, b) = (x_of(&w, 0), x_of(&w, 1));
                assert!(a.iter().all(|&x| x < side / 2) && b.iter().all(|&x| x > side / 2), "{name}: halves");
                let reach = range(&w, &presets::blocks(&w, r.nation, size, r.era).unwrap());
                let want = gap_of([reach; 2]);
                let gap = a.iter().max().zip(b.iter().min()).map(|(a, b)| b - a).unwrap();
                assert!(gap >= want, "{name}: fronts {gap} apart, wanted {want}");
                // Again: the first battle's units are gone, the count is the same.
                let again = deploy(&mut w, &mut battle).expect(&name);
                assert_eq!(again, men, "{name}");
                assert_eq!(w.units.iter().filter(|u| u.alive).count(), first.len(), "{name}: re-roll");
            }
        }
    }

    #[test]
    fn the_swords_ask_for_a_fight_and_the_switches_send_one_only_while_it_runs() {
        let mut e = Editor::default();
        assert_eq!(e.battle.ai, [false, true], "Seite 1 is the player's, Seite 2 the Ki's");
        assert_eq!(e.battle_act(Act::Fight), Some(Input::Fight { ai: [false, true] }));
        // Before the fight a switch is only the view's state.
        assert_eq!(e.battle_act(Act::Control(0)), None);
        assert_eq!(e.battle.ai, [true, true]);
        // "Aufstellen" never fights.
        assert!(matches!(e.battle_act(Act::Go), Some(Input::Battle { fight: false, .. })));
        // While it runs, a switch is an input that is logged.
        let w = empty();
        let army = |owner| tactics::Army { owner, facing: 0, origin: (0, 0), style: Style::Linear, bodies: Vec::new() };
        e.battle.tactics = Some(tactics::Commander::new(&w, [army(0), army(1)], [Control::Ai; 2]));
        assert_eq!(e.battle_act(Act::Control(1)), Some(Input::Fight { ai: [true, false] }));
        let items = e.battle_items();
        let swords = items.iter().find(|i| i.name == "Kampf").expect("the swords");
        assert!(
            swords.hot && matches!(swords.look, Look::Icon { frame: None }),
            "lit, and a name without the icon list"
        );
        assert_eq!((swords.rect.width(), swords.rect.height()), (41, 41));
        for name in ["Seite 1 Steuerung", "Seite 2 Steuerung", "Aufstellen"] {
            assert!(items.iter().any(|i| i.name == name), "{name}");
        }
        assert!(items.iter().all(|i| i.name != "Kampf sofort"), "replaced by the swords");
    }

    #[test]
    fn a_fight_without_armies_says_so() {
        let mut e = Editor::default();
        let mut w = empty();
        assert_eq!(e.start_fight(&mut w, [false, true]), b"Erst aufstellen");
        assert!(e.battle.tactics.is_none() && e.sides == sides::Mode::Off);
    }

    /// England against France, Mittel: "Aufstellen" places and the swords start the fight;
    /// Kampf goes on, the guns of both sides are ordered (the Ki's side with the rest of its
    /// army still holding), nothing but their attack and stand ground is ordered, and no gun
    /// moves. An old savegame's "Kampf sofort" only switches Kampf on.
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn the_swords_start_the_controller_over_the_placed_armies() {
        use acr_sim::Command;
        let base = presets::tests::world();
        let mut e = Editor::default();
        let mut w = base.clone();
        e.go_to_battle(&mut w, [2, 3], [None, None], Size::Medium, true);
        assert_eq!(e.sides, sides::Mode::Free, "fight: true switches Kampf on");
        assert!(e.battle.tactics.is_none(), "and starts nothing");
        let mut e = Editor::default();
        let mut w = base.clone();
        e.go_to_battle(&mut w, [2, 3], [None, None], Size::Medium, false);
        assert_eq!(e.sides, sides::Mode::Off);
        assert_eq!(e.start_fight(&mut w, [false, true]), b"Kampf");
        assert_eq!(e.sides, sides::Mode::Free);
        let a = armies(&w, &e.battle).expect("armies");
        for (s, army) in a.iter().enumerate() {
            let duties = |d: Duty| army.bodies.iter().filter(|b| b.duty == d).count();
            assert_eq!(duties(Duty::Guns), e.battle.bodies[s].iter().filter(|(b, _)| b.role == Role::Guns).count());
            assert!(duties(Duty::Guns) > 0 && duties(Duty::Line) > 0 && duties(Duty::Cavalry) > 0, "side {s}");
            assert!(army.bodies.iter().all(|b| !b.units.is_empty()));
        }
        let guns: Vec<_> =
            a.iter().flat_map(|x| &x.bodies).filter(|b| b.duty == Duty::Guns).flat_map(|b| &b.units).collect();
        for _ in 0..600 {
            let cmds = e.before_step(&mut w);
            for c in &cmds {
                match c {
                    Command::Attack { units, .. } | Command::StandGround { units, .. } => {
                        assert!(units.iter().all(|u| guns.contains(&u)), "{c:?}");
                    }
                    other => panic!("{other:?}"),
                }
            }
            w.step(&cmds);
        }
        let t = e.battle.tactics.as_ref().expect("running");
        for s in 0..2 {
            let r = t.report(s);
            assert!(r.gun_orders > 0 && r.guns_drift_max <= 8, "side {s}: {r:?}");
        }
        // The same fight on a copy of the world gives the same state.
        let (mut e2, mut w2) = (Editor::default(), base.clone());
        e2.go_to_battle(&mut w2, [2, 3], [None, None], Size::Medium, false);
        e2.start_fight(&mut w2, [false, true]);
        for _ in 0..600 {
            let cmds = e2.before_step(&mut w2);
            w2.step(&cmds);
        }
        assert_eq!(w.state_hash(), w2.state_hash());
    }
}
