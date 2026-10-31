//! The battle-tactics controller for the editor's predefined battles (not in the original,
//! whose AI cannot drive editor armies): a pure producer of [`Command`]s from the world and
//! its own small state. It lives here, not in `acr-sim` (which must stay the original) nor in
//! `acr-mission`, and knows no presets: it is given [`Army`]s of [`Body`]s with a [`Duty`].
//!
//! Integer arithmetic only (the engine's tables, [`frame`]), no `HashMap` iteration and no
//! `world.random`, so a replay that runs the controller again gets the same orders. It never
//! mutates the world. Each side decides every [`PERIOD`] ticks (side 1 offset by half of it);
//! asking again within a tick returns the same orders ([`Commander::orders`]).
//!
//! A side under [`Control::Player`] is commanded by the player; only its guns are automatic
//! and a gun the player orders himself is left alone for good. [`Control::Ai`] runs the whole
//! controller. Today that is the batteries ([`guns`]) and the lines and melee foot ([`line`]) with
//! their squares against cavalry ([`karree`]), the cavalry wings ([`cavalry`]), the skirmishers
//! ([`screen`]) and the reserve ([`reserve`]); the native styles add their behaviours in
//! `Commander::decide`.

mod cavalry;
pub mod frame;
mod guns;
mod karree;
mod line;
pub mod picture;
mod reserve;
mod screen;
#[cfg(test)]
mod tests;

use acr_sim::{Command, EntityId, World};

/// Ticks between a side's decisions; side `s` decides when `(tick + 10 * s) % PERIOD == 0`.
pub const PERIOD: u64 = 20;
/// Ticks the sides' decisions are staggered by, per side.
const STAGGER: u64 = 10;

/// What a body is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duty {
    /// Infantry that fires and advances in line.
    Line,
    /// Infantry that fights in melee (pikes, bayonets).
    Melee,
    /// Skirmishers in front of the line.
    Screen,
    /// Shooters behind the line.
    Missile,
    /// A cavalry wing.
    Cavalry,
    /// A battery.
    Guns,
    /// The second line, infantry or cavalry.
    Reserve { cavalry: bool },
}

/// Who gives a side its orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Player,
    Ai,
}

/// How an army fights, by its doctrine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    PikeShot,
    Linear,
    Column,
    Lines19,
    Warband,
    Clans,
    Riders,
}

/// The stage of a side's battle plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Phase {
    #[default]
    Cannonade,
    Advance,
    Firefight,
    Assault,
}

/// A group of units that act together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub duty: Duty,
    pub units: Vec<EntityId>,
    /// Where the body stands across the army, pixels in the army's frame ([`frame`]).
    pub lateral: i32,
}

/// One side: its owner, the way it faces, where its origin stands and its bodies in plan
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Army {
    pub owner: u8,
    pub facing: u8,
    pub origin: (i32, i32),
    pub style: Style,
    pub bodies: Vec<Body>,
}

/// What the controller did and saw, for a side (the driver's `state.battle`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub phase: Phase,
    /// Living units of the army.
    pub alive: u32,
    /// The farthest any gun has ever stood from where it was placed, pixels.
    pub guns_drift_max: i32,
    pub gun_orders: u32,
    pub halts: u32,
    pub karrees: u32,
    pub charges: u32,
    pub rallies: u32,
    pub reserves: u32,
}

/// What one decision of a side looks at.
struct View<'a> {
    tick: u64,
    me: &'a Army,
    foe: &'a Army,
    /// The pictures of the bodies of `me` and `foe`.
    mine: &'a [picture::Picture],
    theirs: &'a [picture::Picture],
}

/// The controller of both sides.
#[derive(Debug, Clone)]
pub struct Commander {
    armies: [Army; 2],
    control: [Control; 2],
    batteries: [guns::Battery; 2],
    squares: [karree::Squares; 2],
    lines: [line::Line; 2],
    wings: [cavalry::Wings; 2],
    screens: [screen::Screens; 2],
    reserves: [reserve::Reserves; 2],
    reports: [Report; 2],
    /// The orders of the last tick asked for, so that asking again repeats them.
    cache: Option<(u64, Vec<Command>)>,
}

impl Commander {
    /// A controller for `armies` as they stand in `world`.
    pub fn new(world: &World, armies: [Army; 2], control: [Control; 2]) -> Self {
        let batteries = [guns::Battery::new(world, &armies[0]), guns::Battery::new(world, &armies[1])];
        Self {
            armies,
            control,
            batteries,
            squares: [karree::Squares::new(world), karree::Squares::new(world)],
            lines: Default::default(),
            wings: Default::default(),
            screens: Default::default(),
            reserves: Default::default(),
            reports: Default::default(),
            cache: None,
        }
    }

    /// Changes who commands each side; what the controller has seen of it stays.
    pub fn set_control(&mut self, control: [Control; 2]) {
        self.control = control;
    }

    /// The orders for the tick `world` is at: the sides whose turn it is decide, in side order.
    /// Never changes the world. The same tick again gives the same orders.
    pub fn orders(&mut self, world: &World) -> Vec<Command> {
        let tick = world.tick.0;
        if let Some((at, cmds)) = &self.cache
            && *at == tick
        {
            return cmds.clone();
        }
        let mut out = Vec::new();
        for side in 0..2 {
            if (tick + STAGGER * side as u64).is_multiple_of(PERIOD) {
                self.decide(world, side, &mut out);
            }
        }
        self.cache = Some((tick, out.clone()));
        out
    }

    /// What the controller did for `side` (0 or 1).
    pub fn report(&self, side: usize) -> &Report {
        &self.reports[side]
    }

    /// One decision of `side`.
    fn decide(&mut self, world: &World, side: usize, out: &mut Vec<Command>) {
        let report = &mut self.reports[side];
        report.alive = self.armies[side]
            .bodies
            .iter()
            .flat_map(|b| &b.units)
            .filter(|&&id| picture::living(world, id).is_some())
            .count() as u32;
        let foe_pics = picture::of(world, &self.armies[1 - side]);
        let hand_over = self.control[side] == Control::Player;
        self.batteries[side].decide(
            world,
            (&self.armies[side], &self.armies[1 - side]),
            &foe_pics,
            hand_over,
            (&mut *out, &mut *report),
        );
        if hand_over {
            return;
        }
        // The reserve fills a broken body's place first: it swaps duties, which the rest reads.
        let mine = picture::of(world, &self.armies[side]);
        self.reserves[side].decide(world, &mut self.armies[side], &mine, (&mut *out, &mut *report));
        let (me, foe) = (&self.armies[side], &self.armies[1 - side]);
        let view = View { tick: world.tick.0, me, foe, mine: &mine, theirs: &foe_pics };
        let held = self.squares[side].decide(world, &view, (&mut *out, &mut *report));
        self.lines[side].decide(world, &view, &held, (&mut *out, &mut *report));
        if let Some(end) = self.lines[side].cannonade_end() {
            self.wings[side].decide(world, &view, end + cavalry::HOLD_AFTER, (&mut *out, &mut *report));
        }
        self.screens[side].decide(world, &view, (out, report));
        // The native styles add their behaviours here.
    }
}
