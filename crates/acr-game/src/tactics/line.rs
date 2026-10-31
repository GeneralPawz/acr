//! Lines and the melee foot behind them: the cannonade, the advance in bounds, the halt and
//! the fire, the second bound and the charge.
//!
//! A formation moves as one body but its members walk to their own slots, so the shape is
//! not kept on the way: the line advances in short bounds ([`STEP`]) and the next bound is
//! ordered only when every line body stands in order again (`line_advance`; after
//! [`PATIENCE`] the line goes on without a straggler). A line halts at 9/10 of its longest
//! ranged attack from the nearest enemy foot body (a column at 10/10) and stands ground, which
//! makes it fire on its own; after [`fire_ticks`] it makes a second bound to 6/10 of its reach,
//! and charges (`AttackMove` at the enemy) when the enemy in front is weak, when it is
//! outnumbered, or after [`CLOSE_TICKS`] at close range. The melee foot (pikes, bayonets) keeps
//! [`MELEE_BEHIND`] behind the line's front and charges with it, or alone when the enemy is
//! within [`RUSH`]. Distances are taken from the bodies' centroids in world pixels
//! ([`dist`]), not through the army frame, whose round trip is only 1 % accurate.

use acr_sim::{Command, EntityId, World};

use super::frame::{dist, to_world};
use super::picture::living;
use super::{Duty, Phase, Report, Style, View};

/// The length of a bound of the advance, pixels.
const STEP: i32 = 160;
/// A body within this of its halting distance has arrived, pixels.
const HALT_SLACK: i32 = 24;
/// The fraction of its reach a line halts at (a column's is [`HALT_COLUMN`]).
const HALT: (i32, i32) = (9, 10);
const HALT_COLUMN: (i32, i32) = (10, 10);
/// The fraction of its reach the second bound ends at.
const CLOSE: (i32, i32) = (6, 10);
/// The infantry holds behind its guns until the enemy front is this much nearer than the
/// reach of the line, pixels...
const CANNONADE_MARGIN: i32 = 600;
/// ...or this many ticks have passed.
const CANNONADE_TICKS: u64 = 600;
/// Ticks the line waits for a body to close up before it goes on without it.
const PATIENCE: u64 = 300;
/// Ticks a line fires at close range before it charges.
const CLOSE_TICKS: u64 = 400;
/// The enemy in front is weak when it has at most this percent of its start.
const WEAK_PERCENT: u32 = 60;
/// A line is superior when its men are at least this fraction of the enemy's in front.
const ODDS: (u32, u32) = (3, 2);
/// The melee foot keeps this far behind the line's front, pixels.
const MELEE_BEHIND: i32 = 120;
/// Melee foot charges an enemy this near, pixels.
const RUSH: i32 = 300;
/// A charge is ordered again when the enemy moved this far from the last aim, pixels.
const RETARGET: i32 = 150;

/// Ticks a line fires from its first halt before the second bound, by doctrine.
const fn fire_ticks(style: Style) -> u64 {
    match style {
        Style::Column => 200,
        Style::PikeShot => 300,
        _ => 400,
    }
}

/// Where a body stands in the attack.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    /// On the way (or still waiting for the order to start).
    #[default]
    Ready,
    /// Halted at 9/10 of its reach, firing.
    Halted,
    /// On its second bound.
    Second,
    /// Arrived at close range, firing.
    Closed,
    /// Charging.
    Charging,
}

#[derive(Debug, Clone, Default)]
struct Mode {
    stage: Stage,
    /// The tick the stage began.
    since: u64,
    /// Stand ground was ordered for this stage.
    stood: bool,
    /// Where the last charge was aimed.
    aim: Option<(i32, i32)>,
}

impl Mode {
    /// A square or a brace had the body: what was ordered before is gone.
    fn release(&mut self) {
        self.stood = false;
        self.aim = None;
    }
}

/// The line and melee foot of one side.
#[derive(Debug, Clone, Default)]
pub(super) struct Line {
    /// The tick of the first decision.
    began: Option<u64>,
    /// The cannonade is over.
    marching: bool,
    /// Ticks the line has waited to close up.
    waited: u64,
    /// Per body of the army.
    modes: Vec<Mode>,
}

/// The foot body of the enemy nearest to `from` (any living body if no foot is left) and its
/// distance.
fn nearest_foe(v: &View, from: (i32, i32)) -> Option<(usize, i32)> {
    let pick = |foot: bool| {
        (0..v.theirs.len())
            .filter(|&j| {
                let (p, d) = (&v.theirs[j], v.foe.bodies[j].duty);
                p.alive > 0 && (!foot || (!p.cavalry && d != Duty::Guns))
            })
            .map(|j| (j, dist(from, v.theirs[j].centroid)))
            .min_by_key(|&(j, d)| (d, j))
    };
    pick(true).or_else(|| pick(false))
}

fn units(world: &World, v: &View, i: usize) -> Vec<EntityId> {
    v.me.bodies[i].units.iter().copied().filter(|&id| living(world, id).is_some()).collect()
}

/// The point `step` pixels from `from` toward `to`.
fn toward(from: (i32, i32), to: (i32, i32), step: i32) -> (i32, i32) {
    let d = i64::from(dist(from, to).max(1));
    let s = i64::from(step);
    (from.0 + (i64::from(to.0 - from.0) * s / d) as i32, from.1 + (i64::from(to.1 - from.1) * s / d) as i32)
}

impl Line {
    /// One decision of the line and melee foot of `v.me`. The bodies `held` are in a square or
    /// braced against cavalry ([`super::karree`]) and get no order.
    pub(super) fn decide(
        &mut self,
        world: &World,
        v: &View,
        held: &[bool],
        (out, report): (&mut Vec<Command>, &mut Report),
    ) {
        let n = v.me.bodies.len();
        self.modes.resize_with(n, Mode::default);
        let began = *self.began.get_or_insert(v.tick);
        for i in (0..n).filter(|&i| held[i]) {
            self.modes[i].release();
        }
        let foot =
            |d: Duty| (0..n).filter(move |&i| v.me.bodies[i].duty == d && v.mine[i].alive > 0).collect::<Vec<_>>();
        let (lines, melee) = (foot(Duty::Line), foot(Duty::Melee));
        // The melee foot leads when there is no line.
        let (front, behind) = if lines.is_empty() { (melee, Vec::new()) } else { (lines, melee) };
        if front.is_empty() {
            return;
        }
        let reach = front.iter().map(|&i| v.mine[i].reach).max().unwrap_or(0);
        if !self.marching {
            let gap = front.iter().filter_map(|&i| nearest_foe(v, v.mine[i].centroid)).map(|f| f.1).min();
            if gap.is_none_or(|g| g <= reach + CANNONADE_MARGIN) || v.tick - began >= CANNONADE_TICKS {
                self.marching = true;
            }
        }
        if !self.marching {
            report.phase = report.phase.max(Phase::Cannonade);
            return;
        }
        let style = v.me.style;
        let halt = if style == Style::Column { HALT_COLUMN } else { HALT };
        let stand = |i: usize| Command::StandGround { player: v.me.owner, units: units(world, v, i), on: true };

        // Halts, the second bound and its arrival.
        let mut movers: Vec<(usize, i32)> = Vec::new();
        for &i in front.iter().filter(|&&i| !held[i]) {
            let (p, m) = (&v.mine[i], &mut self.modes[i]);
            let Some((_, d)) = nearest_foe(v, p.centroid) else { continue };
            let halts_at = if p.melee { RUSH } else { p.reach * halt.0 / halt.1 };
            match m.stage {
                Stage::Ready if p.melee => movers.push((i, d - RUSH)),
                Stage::Ready if d - halts_at <= HALT_SLACK => {
                    *m = Mode { stage: Stage::Halted, since: v.tick, ..Mode::default() };
                    report.halts += 1;
                }
                Stage::Ready => movers.push((i, d - halts_at)),
                Stage::Halted if v.tick - m.since >= fire_ticks(style) => {
                    let close = p.reach * CLOSE.0 / CLOSE.1;
                    let (foe, _) = nearest_foe(v, p.centroid).unwrap_or((0, 0));
                    if d - close > HALT_SLACK {
                        let to = toward(p.centroid, v.theirs[foe].centroid, d - close);
                        out.push(Command::MoveFacing {
                            player: v.me.owner,
                            units: units(world, v, i),
                            x: to.0,
                            y: to.1,
                            dir: v.me.facing,
                        });
                        *m = Mode { stage: Stage::Second, since: v.tick, ..Mode::default() };
                    } else {
                        *m = Mode { stage: Stage::Closed, since: v.tick, ..Mode::default() };
                    }
                }
                Stage::Second if v.tick > m.since && p.in_order => {
                    *m = Mode { stage: Stage::Closed, since: v.tick, ..Mode::default() };
                }
                _ => {}
            }
        }

        // Stand ground once a halted body is in its place (it fires on its own).
        for &i in front.iter().filter(|&&i| !held[i]) {
            let (p, m) = (&v.mine[i], &mut self.modes[i]);
            if matches!(m.stage, Stage::Halted | Stage::Closed)
                && !m.stood
                && (p.in_order || v.tick - m.since >= PATIENCE)
            {
                out.push(stand(i));
                m.stood = true;
            }
        }

        // The charge: one body's trigger sends the whole line, and the melee foot with it.
        let triggered = front.iter().any(|&i| {
            let (p, m) = (&v.mine[i], &self.modes[i]);
            let Some((j, d)) = nearest_foe(v, p.centroid) else { return false };
            match m.stage {
                Stage::Charging => true,
                _ if p.melee => d <= RUSH,
                Stage::Ready | Stage::Second => false,
                Stage::Halted | Stage::Closed => {
                    let foe = &v.theirs[j];
                    foe.alive * 100 <= foe.start * WEAK_PERCENT
                        || p.alive * ODDS.1 >= foe.alive * ODDS.0
                        || (m.stage == Stage::Closed && v.tick - m.since >= CLOSE_TICKS)
                }
            }
        });
        for &i in front.iter().chain(&behind).filter(|&&i| !held[i]) {
            let near = nearest_foe(v, v.mine[i].centroid).is_some_and(|f| f.1 <= RUSH);
            let m = &mut self.modes[i];
            if m.stage != Stage::Charging && (triggered || (near && behind.contains(&i))) {
                *m = Mode { stage: Stage::Charging, since: v.tick, ..Mode::default() };
                report.charges += 1;
            }
            if m.stage == Stage::Charging
                && let Some((j, _)) = nearest_foe(v, v.mine[i].centroid)
            {
                let at = v.theirs[j].centroid;
                if m.aim.is_none_or(|a| dist(a, at) >= RETARGET) {
                    out.push(Command::AttackMove { player: v.me.owner, units: units(world, v, i), x: at.0, y: at.1 });
                    m.aim = Some(at);
                }
            }
        }

        // The common bound of what is still on the way, once it stands in order.
        let movers: Vec<(usize, i32)> =
            movers.into_iter().filter(|&(i, _)| self.modes[i].stage == Stage::Ready).collect();
        if let Some(step) = self.line_advance(v, &movers) {
            for &(i, _) in &movers {
                let (dx, dy) = to_world(v.me.facing, step, 0);
                let c = v.mine[i].centroid;
                out.push(Command::MoveFacing {
                    player: v.me.owner,
                    units: units(world, v, i),
                    x: c.0 + dx,
                    y: c.1 + dy,
                    dir: v.me.facing,
                });
            }
        }

        // The melee foot closes up behind the line's front, never back.
        if !behind.is_empty() {
            let line_f = front.iter().map(|&i| v.mine[i].f).max().unwrap_or(0);
            for &i in behind.iter().filter(|&&i| !held[i] && self.modes[i].stage == Stage::Ready) {
                let p = &v.mine[i];
                let gain = line_f - MELEE_BEHIND - p.f;
                if gain > HALT_SLACK && p.in_order {
                    let (dx, dy) = to_world(v.me.facing, gain.min(STEP), 0);
                    out.push(Command::MoveFacing {
                        player: v.me.owner,
                        units: units(world, v, i),
                        x: p.centroid.0 + dx,
                        y: p.centroid.1 + dy,
                        dir: v.me.facing,
                    });
                }
            }
        }

        let stage = front.iter().map(|&i| self.modes[i].stage).max_by_key(|s| *s as u8).unwrap_or_default();
        let phase = match stage {
            Stage::Ready => Phase::Advance,
            Stage::Charging => Phase::Assault,
            _ => Phase::Firefight,
        };
        report.phase = report.phase.max(phase);
    }

    /// The common step of the line's next bound, if it may go: every body still on the way
    /// stands in order (or has been waited for [`PATIENCE`] ticks). The step is [`STEP`] but
    /// ends at the nearest halting distance, so that the line halts where it should.
    fn line_advance(&mut self, v: &View, movers: &[(usize, i32)]) -> Option<i32> {
        if movers.is_empty() {
            return None;
        }
        if !movers.iter().all(|&(i, _)| v.mine[i].in_order) {
            self.waited += super::PERIOD;
            if self.waited < PATIENCE {
                return None;
            }
        }
        self.waited = 0;
        let nearest = movers.iter().map(|m| m.1).min().unwrap_or(STEP);
        Some(STEP.min(nearest).max(HALT_SLACK))
    }
}
