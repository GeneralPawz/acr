//! Squares against cavalry.
//!
//! A line that sees enemy cavalry (at least [`RIDERS`] riders) close in reforms into the group
//! `KARE` (`Command::Reform`, `ReformBrigade` 0x4e1cd3, allowed only when the line's group lists
//! it among its buttons). A reform ends stand ground, and standing ground on a `#KARE` formation
//! makes its members face outward (`kare_face_outward` 0x4dfe94), so stand ground is ordered only
//! at a later decision, once the square stands in order. After [`CALM`] decisions without cavalry
//! within [`CALM_RANGE`] the body reforms into the group it came from. Nothing changes while
//! more than [`ENGAGED_PERCENT`] of the body fights at hand. Riders that closed by `gain` pixels
//! since the last decision count from `NEAR_CLOSING + gain` (at most [`LOOKAHEAD`] more), so that
//! dragoons covering 200 px a decision are seen while there is still time to form. A line without
//! the button, and the melee foot (pikes need no square), stand ground and face the riders instead.
//!
//! A body in a square or braced is `held`: the line ([`super::line`]) gives it no orders.

use std::cell::OnceCell;

use acr_sim::trig::get_dir;
use acr_sim::{Command, World};

use super::frame::dist;
use super::picture::living;
use super::{Duty, Report, View};

/// Cavalry counts as a body of at least this many riders.
const RIDERS: u32 = 5;
/// Cavalry this near is a threat, pixels...
const NEAR: i32 = 350;
/// ...and this near when it came nearer by [`CLOSING`] since the last decision, pixels.
const NEAR_CLOSING: i32 = 700;
const CLOSING: i32 = 30;
/// At most this much is added to [`NEAR_CLOSING`] for the distance riders closed by since the
/// last decision, pixels.
const LOOKAHEAD: i32 = 300;
/// No cavalry this near counts as calm, pixels.
const CALM_RANGE: i32 = 1100;
/// Calm decisions in a row before the square dissolves.
const CALM: u8 = 3;
/// A soldier fights at hand when an enemy is this near, pixels.
const HAND: i32 = 100;
/// A body is engaged when more than this percent of its men fight at hand.
const ENGAGED_PERCENT: usize = 30;
/// A braced body faces the riders again when they bear this much (1/256 turns) from its facing.
const REFACE: u8 = 24;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum State {
    #[default]
    Open,
    /// Reformed into a square, which stands ground once in order.
    Square { since: u64, original: usize, stood: bool },
    /// Standing ground, facing `dir`.
    Braced { dir: u8 },
}

#[derive(Debug, Clone, Default)]
struct Body {
    state: State,
    calm: u8,
    /// The distance of each enemy body at the last decision.
    seen: Vec<Option<i32>>,
}

/// The squares of one side.
#[derive(Debug, Clone)]
pub(super) struct Squares {
    /// The group index of `KARE`.
    kare: Option<usize>,
    bodies: Vec<Body>,
}

impl Squares {
    pub(super) fn new(world: &World) -> Self {
        let kare = world.formations.groups.iter().position(|g| g.name.eq_ignore_ascii_case("KARE"));
        Self { kare, bodies: Vec::new() }
    }

    /// One decision; returns per body whether it is held (in a square, braced, or just released).
    pub(super) fn decide(
        &mut self,
        world: &World,
        v: &View,
        (out, report): (&mut Vec<Command>, &mut Report),
    ) -> Vec<bool> {
        let n = v.me.bodies.len();
        self.bodies.resize_with(n, Body::default);
        let foes: OnceCell<Vec<(i32, i32)>> = OnceCell::new();
        let engaged = |i: usize| {
            let foes = foes.get_or_init(|| {
                v.foe
                    .bodies
                    .iter()
                    .flat_map(|b| &b.units)
                    .filter_map(|&id| living(world, id).map(|u| u.pos.pixels()))
                    .collect()
            });
            let own: Vec<(i32, i32)> =
                v.me.bodies[i].units.iter().filter_map(|&id| living(world, id).map(|u| u.pos.pixels())).collect();
            let fighting = own.iter().filter(|&&p| foes.iter().any(|&q| dist(p, q) <= HAND)).count();
            fighting * 100 > own.len() * ENGAGED_PERCENT
        };
        let mut held = vec![false; n];
        for (i, held_i) in held.iter_mut().enumerate() {
            let (duty, p) = (v.me.bodies[i].duty, &v.mine[i]);
            let b = &mut self.bodies[i];
            b.seen.resize(v.theirs.len(), None);
            let now: Vec<Option<i32>> = (0..v.theirs.len())
                .map(|j| {
                    let t = &v.theirs[j];
                    (p.alive > 0 && (t.cavalry || v.foe.bodies[j].duty == Duty::Cavalry) && t.alive >= RIDERS)
                        .then(|| dist(p.centroid, t.centroid))
                })
                .collect();
            let before = std::mem::replace(&mut b.seen, now.clone());
            if p.alive == 0 || p.cavalry || !matches!(duty, Duty::Line | Duty::Melee) {
                b.state = State::Open;
                continue;
            }
            // The nearest riders that are a threat now.
            let threat = (0..now.len())
                .filter_map(|j| {
                    let d = now[j]?;
                    // Riders that closed by `gain` since the last decision are a threat from
                    // `gain` farther off: fast ones (dragoons) cover 200 px in a decision.
                    let gain = before[j].map_or(0, |was| was - d);
                    (d <= NEAR || (gain >= CLOSING && d <= NEAR_CLOSING + gain.min(LOOKAHEAD))).then_some((d, j))
                })
                .min();
            let calm_now = now.iter().flatten().all(|&d| d > CALM_RANGE);
            let units: Vec<_> =
                v.me.bodies[i].units.iter().copied().filter(|&id| living(world, id).is_some()).collect();
            let before_state = b.state;
            match b.state {
                State::Open => {
                    if let Some((_, j)) = threat
                        && !engaged(i)
                    {
                        b.calm = 0;
                        let button = self.kare.filter(|&k| {
                            duty == Duty::Line
                                && p.group.is_some_and(|g| world.formations.groups[g].buttons.contains(&k))
                        });
                        match (p.brigade, button, p.group) {
                            (Some(brigade), Some(k), Some(original)) if u8::try_from(k).is_ok() => {
                                out.push(Command::Reform { player: v.me.owner, brigade, group: k as u8 });
                                b.state = State::Square { since: v.tick, original, stood: false };
                                report.karrees += 1;
                            }
                            _ => {
                                let dir = bearing(p.centroid, v.theirs[j].centroid);
                                out.push(Command::StandGround { player: v.me.owner, units: units.clone(), on: true });
                                if p.brigade.is_some() {
                                    out.push(Command::Face { player: v.me.owner, officer: units[0], dir });
                                }
                                b.state = State::Braced { dir };
                            }
                        }
                    }
                }
                State::Square { since, original, stood } => {
                    b.calm = if calm_now { b.calm + 1 } else { 0 };
                    if b.calm >= CALM && !engaged(i) {
                        if let Some(brigade) = p.brigade {
                            out.push(Command::Reform { player: v.me.owner, brigade, group: original as u8 });
                        }
                        b.state = State::Open;
                    } else if !stood && v.tick > since && p.in_order {
                        out.push(Command::StandGround { player: v.me.owner, units, on: true });
                        b.state = State::Square { since, original, stood: true };
                    }
                }
                State::Braced { dir } => {
                    b.calm = if calm_now { b.calm + 1 } else { 0 };
                    if b.calm >= CALM && !engaged(i) {
                        out.push(Command::StandGround { player: v.me.owner, units, on: false });
                        b.state = State::Open;
                    } else if let Some((_, j)) = threat
                        && p.brigade.is_some()
                    {
                        let to = bearing(p.centroid, v.theirs[j].centroid);
                        if (to.wrapping_sub(dir) as i8).unsigned_abs() >= REFACE {
                            out.push(Command::Face { player: v.me.owner, officer: units[0], dir: to });
                            b.state = State::Braced { dir: to };
                        }
                    }
                }
            }
            *held_i = before_state != State::Open || b.state != State::Open;
        }
        held
    }
}

fn bearing(from: (i32, i32), to: (i32, i32)) -> u8 {
    get_dir(to.0 - from.0, to.1 - from.1)
}
