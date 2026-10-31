//! The cavalry wings: they hold until [`HOLD_AFTER`] ticks after the cannonade, ride round the
//! enemy's box on their own side, charge the best target, fall back and re-form after losses
//! and intercept enemy cavalry that threatens the own infantry and guns.
//!
//! The flank route has two waypoints in the army's frame: WP1 [`AHEAD`] short of the enemy's
//! nearest man and [`OUTSIDE`] beyond the side of its box, WP2 level with the middle of the box
//! on the same line. Both are clamped to the map ([`MAP_MARGIN`], the editor's margin); when
//! less than [`ROOM`] is left between the clamped route and the box the way is blocked and the
//! wing goes for the guns and skirmishers in front instead.
//!
//! A charge is `AttackMove` at the best target of the wing, scored by [`score`]: guns without
//! foot to protect them x4, skirmishers x3, a line hit in its flank or rear x3, other bodies
//! x1; a square or pikes x0 (never charged). The score is divided by the distance. Types with
//! `ARMATTACK` get `MeleeOnly(on)` first: the dragoons (`EngD18`, `FrnD18`: `USAGE STRELOK`,
//! sabre 40-80 and carbine 200-850, `ARMATTACK`) charge with the sabre and fire again only when
//! the wing rides home; the wing counts as riders (a [`Duty::Cavalry`] body) though the
//! engine's usage says foot shooter. A wing that lost [`RALLY_PERCENT`] of what it charged with
//! rides home, re-forms and rests [`REST`] ticks; below [`SPENT_PERCENT`] of its start it is
//! spent and the reserve ([`super::reserve`]) takes its place.

use acr_formats::m3d::MapSize;
use acr_sim::trig::get_dir;
use acr_sim::{Command, EntityId, World};

use super::frame::{dist, to_army, to_world};
use super::picture::{Picture, living};
use super::{Duty, Report, View};

/// Ticks the wings hold after the cannonade ended.
pub(super) const HOLD_AFTER: u64 = 200;
/// How far from the map's edge a unit stays, pixels (the editor's `MAP_MARGIN`).
pub(super) const MAP_MARGIN: i32 = 320;
/// WP1 lies this far short of the enemy's nearest man, pixels...
const AHEAD: i32 = 300;
/// ...and WP1 and WP2 this far outside the side of its box.
const OUTSIDE: i32 = 400;
/// Foot that flanks like a wing (the outer warrior groups of the clans) rides this far outside
/// the box instead, pixels.
pub(super) const FOOT_OUTSIDE: i32 = 600;
/// A route with less room than this left outside the box (after clamping) is blocked, pixels.
const ROOM: i32 = 150;
/// A wing has reached a waypoint when its nearest man is within this, pixels ([`lead`]).
const ARRIVED: i32 = 200;
/// Ticks after which a wing on its flank route charges wherever it is.
const FLANK_TICKS: u64 = 1500;
/// Guns are protected by foot (pikes or a square) within this of them, pixels.
const PROTECT: i32 = 250;
/// A line is hit in the flank or rear when the wing comes from this much off its facing (1/256
/// turns).
const FLANK_ANGLE: u8 = 48;
/// A charge is ordered again when the target moved this far from the last aim, pixels.
const RETARGET: i32 = 150;
/// A new target replaces the kept one only when it scores more than this fraction of it.
const SWITCH: (i64, i64) = (3, 2);
/// A wing rallies when it has fewer than this percent of the riders it charged with...
const RALLY_PERCENT: u32 = 60;
/// ...and is spent when it has fewer than this percent of its start.
const SPENT_PERCENT: u32 = 30;
/// Ticks a wing rests after it re-formed.
const REST: u64 = 200;
/// A wing that rides home re-forms within this of its home, pixels, or after [`HOME_TICKS`].
const HOME: i32 = 200;
const HOME_TICKS: u64 = 600;
/// Enemy cavalry counts as a body of at least this many riders.
const RIDERS: u32 = 5;
/// Enemy cavalry this near the own infantry or guns, closing, is intercepted, pixels...
const INTERCEPT: i32 = 1000;
/// ...closing is a gain of this much since the last decision...
const CLOSING: i32 = 30;
/// ...and an interception ends when the riders are this far off, pixels.
const INTERCEPT_END: i32 = 1500;
/// Only a wing with at least this percent of its start intercepts.
const INTERCEPT_PERCENT: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aim {
    /// The best target by [`score`]; `frontal` limits it to the guns and skirmishers.
    Best { frontal: bool },
    /// Enemy body `j`, the riders it intercepts.
    Fixed(usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum State {
    /// Waiting at home.
    #[default]
    Hold,
    /// On the flank route to its `leg`th waypoint.
    Flank { route: [(i32, i32); 2], leg: u8, since: u64 },
    /// Charging, with `start_alive` riders at the start.
    Charge { aim: Aim, start_alive: u32 },
    /// Riding home to re-form.
    Rally { since: u64 },
    /// Re-formed, resting until the tick.
    Rest { until: u64 },
    /// Broken: no more orders.
    Spent,
}

#[derive(Debug, Clone)]
struct Wing {
    state: State,
    /// Where the wing stood at the first decision.
    home: Option<(i32, i32)>,
    /// The enemy body it charges and where the last `AttackMove` was aimed.
    target: Option<usize>,
    aim: Option<(i32, i32)>,
    /// How far outside the enemy's box the route runs ([`OUTSIDE`], or [`FOOT_OUTSIDE`] for foot).
    outside: i32,
    /// Foot that found no way to flank and no frontal target after its release: the line has it.
    gave_up: bool,
}

impl Default for Wing {
    fn default() -> Self {
        Self { state: State::Hold, home: None, target: None, aim: None, outside: OUTSIDE, gave_up: false }
    }
}

/// The cavalry wings of one side.
#[derive(Debug, Clone, Default)]
pub(super) struct Wings {
    wings: Vec<Wing>,
    /// The distance of each enemy body to the own foot at the last decision.
    seen: Vec<Option<i32>>,
}

/// Whether the enemy body is a square (the group `KARE`).
fn square(world: &World, p: &Picture) -> bool {
    p.group.is_some_and(|g| world.formations.groups[g].name.eq_ignore_ascii_case("KARE"))
}

/// Riders as the foe's plan has them: cavalry by usage, or a wing (the dragoons).
fn riders(v: &View, j: usize) -> bool {
    let t = &v.theirs[j];
    t.alive >= RIDERS && (t.cavalry || v.foe.bodies[j].duty == Duty::Cavalry)
}

/// What a charge at enemy body `j` is worth to riders at `from`, zero for none; `frontal`
/// limits the choice to the guns and the skirmishers. Scaled by the distance.
fn score(world: &World, v: &View, j: usize, from: (i32, i32), frontal: bool) -> Option<i64> {
    let t = &v.theirs[j];
    if t.alive == 0 || square(world, t) {
        return None;
    }
    let weight = match v.foe.bodies[j].duty {
        Duty::Melee => return None,
        Duty::Guns => {
            let guarded = (0..v.theirs.len()).any(|k| {
                let o = &v.theirs[k];
                k != j
                    && o.alive > 0
                    && (v.foe.bodies[k].duty == Duty::Melee || square(world, o))
                    && dist(o.centroid, t.centroid) <= PROTECT
            });
            if guarded { 1 } else { 4 }
        }
        Duty::Screen => 3,
        _ if frontal => return None,
        _ => {
            let approach = get_dir(from.0 - t.centroid.0, from.1 - t.centroid.1);
            let off = (approach.wrapping_sub(t.facing) as i8).unsigned_abs();
            if off >= FLANK_ANGLE && !riders(v, j) { 3 } else { 1 }
        }
    };
    Some(weight * 100_000 / i64::from(dist(from, t.centroid) + 100))
}

/// The best target for riders at `from`; the `kept` one stays unless another scores clearly more.
pub(super) fn best(world: &World, v: &View, from: (i32, i32), frontal: bool, kept: Option<usize>) -> Option<usize> {
    let top = (0..v.theirs.len())
        .filter_map(|j| score(world, v, j, from, frontal).map(|s| (s, j)))
        .max_by_key(|&(s, j)| (s, std::cmp::Reverse(j)))?;
    match kept.and_then(|k| score(world, v, k, from, frontal)) {
        Some(was) if top.0 * SWITCH.1 <= was * SWITCH.0 => kept,
        _ => Some(top.1),
    }
}

/// The flank route of a wing at army-frame side `s` round the enemy's box, in world pixels,
/// and whether the map leaves too little room for it. `None` without a living enemy.
fn route(world: &World, v: &View, s: i32, outside: i32) -> Option<([(i32, i32); 2], bool)> {
    let (mut f_min, mut f_max, mut s_min, mut s_max) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for id in v.foe.bodies.iter().flat_map(|b| &b.units) {
        let Some(u) = living(world, *id) else { continue };
        let (x, y) = u.pos.pixels();
        let (f, s) = to_army(v.me.facing, x - v.me.origin.0, y - v.me.origin.1);
        (f_min, f_max, s_min, s_max) = (f_min.min(f), f_max.max(f), s_min.min(s), s_max.max(s));
    }
    if f_min > f_max {
        return None;
    }
    let side = if s < (s_min + s_max) / 2 { -1 } else { 1 };
    let (edge, box_edge) = if side < 0 { (s_min - outside, s_min) } else { (s_max + outside, s_max) };
    let limit = MapSize { shift: world.map.shift }.playable_size() - MAP_MARGIN;
    let mut blocked = false;
    let mut point = |f: i32| {
        let (dx, dy) = to_world(v.me.facing, f, edge);
        let p = ((v.me.origin.0 + dx).clamp(MAP_MARGIN, limit), (v.me.origin.1 + dy).clamp(MAP_MARGIN, limit));
        let (_, s) = to_army(v.me.facing, p.0 - v.me.origin.0, p.1 - v.me.origin.1);
        blocked |= side * (s - box_edge) < ROOM;
        p
    };
    let wp1 = point(f_min - AHEAD);
    let wp2 = point((f_min + f_max) / 2);
    Some(([wp1, wp2], blocked))
}

impl Wings {
    /// Whether the wings still command body `i`: a flanking foot body they gave up on is the
    /// line's again.
    pub(super) fn keeps(&self, i: usize) -> bool {
        !self.wings.get(i).is_some_and(|w| w.gave_up)
    }

    /// One decision of the wings of `v.me` and of the foot bodies `flank` that flank like wings;
    /// they hold until the tick `release`.
    pub(super) fn decide(
        &mut self,
        world: &World,
        v: &View,
        (release, flank): (u64, &[usize]),
        (out, report): (&mut Vec<Command>, &mut Report),
    ) {
        let n = v.me.bodies.len();
        self.wings.resize_with(n, Wing::default);
        let is_wing = |i: usize| v.me.bodies[i].duty == Duty::Cavalry || flank.contains(&i);
        if !(0..n).any(is_wing) {
            return;
        }
        for i in (0..n).filter(|&i| is_wing(i)) {
            let w = &mut self.wings[i];
            w.home.get_or_insert(v.mine[i].centroid);
            if v.me.bodies[i].duty != Duty::Cavalry {
                w.outside = FOOT_OUTSIDE;
            }
        }
        let sent = self.intercept(v);
        let mut ctx = Ctx { world, v, release, out, report };
        for i in sent {
            self.wings[i].melee(&mut ctx, i, true);
            ctx.report.charges += 1;
        }
        for i in (0..n).filter(|&i| is_wing(i) && v.mine[i].alive > 0) {
            if !self.wings[i].gave_up {
                self.wings[i].step(&mut ctx, i);
            }
        }
    }

    /// Sends the nearest idle wing against each enemy rider body that closes on the own foot or
    /// guns.
    fn intercept(&mut self, v: &View) -> Vec<usize> {
        let mut sent = Vec::new();
        let guarded: Vec<usize> = (0..v.me.bodies.len())
            .filter(|&i| v.mine[i].alive > 0 && !matches!(v.me.bodies[i].duty, Duty::Cavalry))
            .collect();
        self.seen.resize(v.theirs.len(), None);
        let now: Vec<Option<i32>> = (0..v.theirs.len())
            .map(|j| {
                riders(v, j)
                    .then(|| guarded.iter().map(|&i| dist(v.mine[i].centroid, v.theirs[j].centroid)).min())
                    .flatten()
            })
            .collect();
        let before = std::mem::replace(&mut self.seen, now.clone());
        for j in 0..now.len() {
            let Some(d) = now[j] else { continue };
            if d > INTERCEPT || !before[j].is_some_and(|was| was - d >= CLOSING) {
                continue;
            }
            if self.wings.iter().any(|w| matches!(w.state, State::Charge { aim: Aim::Fixed(k), .. } if k == j)) {
                continue;
            }
            let idle = |i: usize| {
                let (w, p) = (&self.wings[i], &v.mine[i]);
                v.me.bodies[i].duty == Duty::Cavalry
                    && p.alive * 100 >= p.start * INTERCEPT_PERCENT
                    && matches!(w.state, State::Hold | State::Rest { .. })
            };
            let pick = (0..self.wings.len())
                .filter(|&i| idle(i))
                .min_by_key(|&i| (dist(v.mine[i].centroid, v.theirs[j].centroid), i));
            if let Some(i) = pick {
                self.wings[i].state = State::Charge { aim: Aim::Fixed(j), start_alive: v.mine[i].alive };
                self.wings[i].aim = None;
                self.wings[i].target = Some(j);
                sent.push(i);
            }
        }
        sent
    }
}

struct Ctx<'a, 'b> {
    world: &'a World,
    v: &'a View<'a>,
    release: u64,
    out: &'b mut Vec<Command>,
    report: &'b mut Report,
}

impl Wing {
    fn step(&mut self, c: &mut Ctx, i: usize) {
        let p = &c.v.mine[i];
        match self.state {
            State::Spent => {}
            State::Hold => self.dispatch(c, i),
            State::Rest { until } => {
                if c.v.tick >= until {
                    self.state = State::Hold;
                    self.dispatch(c, i);
                }
            }
            State::Flank { route, leg, since } => {
                if c.v.tick - since >= FLANK_TICKS {
                    self.enter_charge(c, i, Aim::Best { frontal: false });
                } else if lead(c, i, route[usize::from(leg)]) <= ARRIVED {
                    if leg == 0 {
                        let to = route[1];
                        let foe = c.v.theirs.iter().find(|t| t.alive > 0).map_or(to, |t| t.centroid);
                        self.go(c, i, to, get_dir(foe.0 - to.0, foe.1 - to.1));
                        self.state = State::Flank { route, leg: 1, since };
                    } else {
                        self.enter_charge(c, i, Aim::Best { frontal: false });
                    }
                }
            }
            State::Charge { .. } => self.charge(c, i),
            State::Rally { since } => {
                let home = self.home.unwrap_or(p.centroid);
                if lead(c, i, home) <= HOME || c.v.tick - since >= HOME_TICKS {
                    if let (Some(brigade), Some(group)) = (p.brigade, p.group)
                        && let Ok(group) = u8::try_from(group)
                    {
                        c.out.push(Command::Reform { player: c.v.me.owner, brigade, group });
                    }
                    self.state = if p.alive * 100 < p.start * SPENT_PERCENT {
                        State::Spent
                    } else {
                        State::Rest { until: c.v.tick + REST }
                    };
                }
            }
        }
    }

    /// One step of a charge: rally when too many fell, else aim at the target.
    fn charge(&mut self, c: &mut Ctx, i: usize) {
        let p = &c.v.mine[i];
        let State::Charge { aim, start_alive } = self.state else { return };
        if p.alive * 100 < start_alive * RALLY_PERCENT {
            self.rally(c, i);
            return;
        }
        let at = match aim {
            Aim::Fixed(j) => {
                let t = &c.v.theirs[j];
                let guarded = (0..c.v.me.bodies.len())
                    .filter(|&k| c.v.mine[k].alive > 0 && c.v.me.bodies[k].duty != Duty::Cavalry);
                let near = guarded.map(|k| dist(c.v.mine[k].centroid, t.centroid)).min();
                (t.alive > 0 && near.is_some_and(|d| d <= INTERCEPT_END)).then_some((j, t.centroid))
            }
            Aim::Best { frontal } => {
                best(c.world, c.v, p.centroid, frontal, self.target).map(|j| (j, c.v.theirs[j].centroid))
            }
        };
        let Some((j, at)) = at else {
            // Nothing left to charge: hold where it stands and look again.
            self.melee(c, i, false);
            self.state = State::Hold;
            self.target = None;
            return;
        };
        self.target = Some(j);
        if self.aim.is_none_or(|a| dist(a, at) >= RETARGET) {
            c.out.push(Command::AttackMove { player: c.v.me.owner, units: units(c, i), x: at.0, y: at.1 });
            self.aim = Some(at);
        }
    }

    /// A wing that may go: onto its flank route, or straight at the guns and skirmishers in
    /// front when the route is blocked.
    fn dispatch(&mut self, c: &mut Ctx, i: usize) {
        if c.v.tick < c.release {
            return;
        }
        match route(c.world, c.v, c.v.mine[i].s, self.outside) {
            Some((route, false)) => {
                let wp1 = route[0];
                self.go(c, i, wp1, c.v.me.facing);
                self.state = State::Flank { route, leg: 0, since: c.v.tick };
            }
            Some((_, true)) if best(c.world, c.v, c.v.mine[i].centroid, true, None).is_some() => {
                self.enter_charge(c, i, Aim::Best { frontal: true });
            }
            _ => self.gave_up = c.v.me.bodies[i].duty != Duty::Cavalry,
        }
    }

    fn go(&mut self, c: &mut Ctx, i: usize, to: (i32, i32), dir: u8) {
        c.out.push(Command::MoveFacing { player: c.v.me.owner, units: units(c, i), x: to.0, y: to.1, dir });
    }

    fn enter_charge(&mut self, c: &mut Ctx, i: usize, aim: Aim) {
        self.melee(c, i, true);
        self.state = State::Charge { aim, start_alive: c.v.mine[i].alive };
        self.aim = None;
        c.report.charges += 1;
        self.charge(c, i);
    }

    /// Falls back: sabres off, home, re-formed once there.
    fn rally(&mut self, c: &mut Ctx, i: usize) {
        self.melee(c, i, false);
        let home = self.home.unwrap_or(c.v.mine[i].centroid);
        self.go(c, i, home, c.v.me.facing);
        self.state = State::Rally { since: c.v.tick };
        self.target = None;
        self.aim = None;
        c.report.rallies += 1;
    }

    /// `MeleeOnly` for the soldiers of the types that have the switch (`ARMATTACK`).
    fn melee(&self, c: &mut Ctx, i: usize, on: bool) {
        let arm: Vec<EntityId> = units(c, i)
            .into_iter()
            .filter(|id| c.world.catalog.get(c.world.units[id.0 as usize].kind).role.arm_attack)
            .collect();
        if !arm.is_empty() {
            c.out.push(Command::MeleeOnly { player: c.v.me.owner, units: arm, on });
        }
    }
}

/// How far the nearest living man of body `i` is from `to`, pixels: a wing's formation is
/// hundreds of pixels deep, so its centroid trails the point it was sent to by half of that.
fn lead(c: &Ctx, i: usize, to: (i32, i32)) -> i32 {
    c.v.me.bodies[i]
        .units
        .iter()
        .filter_map(|&id| living(c.world, id))
        .map(|u| dist(u.pos.pixels(), to))
        .min()
        .unwrap_or(i32::MAX)
}

fn units(c: &Ctx, i: usize) -> Vec<EntityId> {
    c.v.me.bodies[i].units.iter().copied().filter(|&id| living(c.world, id).is_some()).collect()
}
