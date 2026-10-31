//! The native doctrines (`Warband`, `Clans`, `Riders`): they drive the same line and wings as the
//! European ones with other rules, and add the archers.
//!
//! * Warband (formed Mesoamerican armies): the warriors rush (`AttackMove`) an enemy within
//!   [`WARBAND_RUSH`]; the archers (the `Missile` bodies) follow [`BEHIND`] the warriors and halt at
//!   9/10 of their bow range from the nearest enemy foot, standing ground to shoot.
//! * Clans (loose Woodland and Pueblo armies): the warriors rush from [`CLANS_RUSH`]; the
//!   outermost warrior group on each side flanks like a wing, riding [`super::cavalry::FOOT_OUTSIDE`]
//!   outside the enemy's box.
//! * Riders (Plains): the lancer wedges hold until the mounted archers have been shooting for
//!   [`FIRE_TICKS`] ticks from their halt, then charge from wherever they stand; the wings flank
//!   as usual.
//!
//! Where the original draws the native armies' tactics from is not known (its AI cannot drive
//! editor armies); the numbers are the controller's own.

use acr_sim::{Command, EntityId, World};

use super::frame::to_world;
use super::line::{self, Rules, nearest_foe};
use super::picture::living;
use super::{Duty, Report, Style, View};

/// The warriors of a warband rush an enemy this near, pixels.
const WARBAND_RUSH: i32 = 700;
/// The clans' warriors rush from this far, pixels.
const CLANS_RUSH: i32 = 900;
/// Archers keep this far behind the warriors' front, pixels.
const BEHIND: i32 = 150;
/// The length of an archers' bound, pixels.
const STEP: i32 = 160;
/// Archers within this of their halting distance have arrived, pixels.
const SLACK: i32 = 24;
/// The fraction of its bow range an archer body halts at.
const HALT: (i32, i32) = (9, 10);
/// Ticks the mounted archers shoot before the lancers charge.
const FIRE_TICKS: u64 = 200;
/// The lancers charge from anywhere: further than any map is wide, pixels.
const FAR: i32 = 1 << 20;
/// The clans need at least this many warrior groups for the outer ones to flank.
const FLANK_GROUPS: usize = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Archer {
    /// On the way.
    #[default]
    Moving,
    /// Halted at its range; `stood` once stand ground is ordered.
    Halted { stood: bool },
}

/// The native behaviours of one side.
#[derive(Debug, Clone, Default)]
pub(super) struct Natives {
    archers: Vec<Archer>,
    /// The tick the first archers of a rider army halted (or that none were left).
    fired: Option<u64>,
    /// The warrior groups that flank like wings (Clans), chosen at the first decision.
    flank: Option<Vec<usize>>,
}

impl Natives {
    /// What the doctrine changes in the line's rules at `tick`.
    pub(super) fn rules(&self, style: Style, tick: u64) -> Rules {
        match style {
            Style::Warband => Rules { rush: WARBAND_RUSH, hold_melee: false },
            Style::Clans => Rules { rush: CLANS_RUSH, hold_melee: false },
            Style::Riders => {
                let go = self.fired.is_some_and(|t| tick >= t + FIRE_TICKS);
                Rules { rush: if go { FAR } else { line::RUSH }, hold_melee: !go }
            }
            _ => Rules::EUROPEAN,
        }
    }

    /// The warrior groups of `v.me` that flank like wings: for the clans, the outermost group on
    /// each side of the line (never when it has fewer than [`FLANK_GROUPS`]).
    pub(super) fn flank(&mut self, v: &View) -> &[usize] {
        self.flank.get_or_insert_with(|| {
            let groups: Vec<usize> = (0..v.me.bodies.len())
                .filter(|&i| v.me.style == Style::Clans && matches!(v.me.bodies[i].duty, Duty::Line | Duty::Melee))
                .collect();
            if groups.len() < FLANK_GROUPS {
                return Vec::new();
            }
            let lateral = |i: usize| v.me.bodies[i].lateral;
            let lo = groups.iter().copied().min_by_key(|&i| (lateral(i), i));
            let hi = groups.iter().copied().max_by_key(|&i| (lateral(i), std::cmp::Reverse(i)));
            lo.into_iter().chain(hi).collect()
        })
    }

    /// One decision of the archers of `v.me`; they go once the line `marching`.
    pub(super) fn decide(
        &mut self,
        world: &World,
        v: &View,
        marching: bool,
        (out, report): (&mut Vec<Command>, &mut Report),
    ) {
        if !matches!(v.me.style, Style::Warband | Style::Clans | Style::Riders) {
            return;
        }
        let n = v.me.bodies.len();
        self.archers.resize(n, Archer::default());
        let archers: Vec<usize> =
            (0..n).filter(|&i| v.me.bodies[i].duty == Duty::Missile && v.mine[i].alive > 0).collect();
        if v.me.style == Style::Riders && archers.is_empty() && marching {
            self.fired.get_or_insert(v.tick);
        }
        if !marching {
            return;
        }
        // The warriors' front: the archers never pass it by more than they should.
        let lead = (0..n)
            .filter(|&i| matches!(v.me.bodies[i].duty, Duty::Line | Duty::Melee) && v.mine[i].alive > 0)
            .map(|i| v.mine[i].f)
            .max();
        for i in archers {
            let p = &v.mine[i];
            let Some((_, d)) = nearest_foe(v, p.centroid) else { continue };
            let halt = p.reach * HALT.0 / HALT.1;
            let units = || -> Vec<EntityId> {
                v.me.bodies[i].units.iter().copied().filter(|&id| living(world, id).is_some()).collect()
            };
            match self.archers[i] {
                Archer::Moving if d - halt <= SLACK => {
                    self.archers[i] = Archer::Halted { stood: false };
                    report.halts += 1;
                    if v.me.style == Style::Riders {
                        self.fired.get_or_insert(v.tick);
                    }
                }
                Archer::Moving if p.in_order => {
                    let room = match (v.me.style, lead) {
                        (Style::Riders, _) | (_, None) => i32::MAX,
                        (_, Some(f)) => f - BEHIND - p.f,
                    };
                    let step = STEP.min(d - halt).min(room);
                    if step > SLACK {
                        let (dx, dy) = to_world(v.me.facing, step, 0);
                        out.push(Command::MoveFacing {
                            player: v.me.owner,
                            units: units(),
                            x: p.centroid.0 + dx,
                            y: p.centroid.1 + dy,
                            dir: v.me.facing,
                        });
                    }
                }
                Archer::Halted { stood: false } if p.in_order => {
                    out.push(Command::StandGround { player: v.me.owner, units: units(), on: true });
                    self.archers[i] = Archer::Halted { stood: true };
                }
                // The enemy moved out of range: follow it.
                Archer::Halted { stood } if d > p.reach + SLACK => {
                    if stood {
                        out.push(Command::StandGround { player: v.me.owner, units: units(), on: false });
                    }
                    self.archers[i] = Archer::Moving;
                }
                _ => {}
            }
        }
    }
}
