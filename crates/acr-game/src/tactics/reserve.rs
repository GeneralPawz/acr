//! The reserve: it follows [`BEHIND`] the line's front and replaces a broken first-line body.
//!
//! A first-line body (`Line`, `Melee` or `Cavalry`) is broken when it has fewer than
//! [`BROKEN_PERCENT`] of its start, when its formation is gone, or when at least
//! [`PANIC_PERCENT`] of it panics. The nearest reserve body of the same arm (by its lateral
//! place in the army) takes its duty, the broken body re-forms and falls back to the reserve's
//! place and becomes the reserve. The duties are swapped in the army itself, which the other
//! behaviours then read.

use acr_sim::{Command, World};

use super::frame::to_world;
use super::picture::{Picture, living};
use super::{Army, Duty, Report};

/// The reserve stands this far behind the line's front, pixels.
const BEHIND: i32 = 350;
/// A body is broken with fewer than this percent of its start...
const BROKEN_PERCENT: u32 = 40;
/// ...or with at least this percent of its men in panic. A reserve body below the first
/// does not replace anyone.
const PANIC_PERCENT: u32 = 30;
/// The reserve closes up in bounds of this many pixels, when it is more than [`SLACK`] short.
const STEP: i32 = 160;
const SLACK: i32 = 24;

/// The reserve of one side.
#[derive(Debug, Clone, Default)]
pub(super) struct Reserves {
    /// Per body: it stood in a formation at the first decision.
    formed: Vec<Option<bool>>,
}

impl Reserves {
    pub(super) fn decide(
        &mut self,
        world: &World,
        army: &mut Army,
        mine: &[Picture],
        (out, report): (&mut Vec<Command>, &mut Report),
    ) {
        let n = army.bodies.len();
        if self.formed.len() != n {
            self.formed = mine.iter().map(|p| Some(p.brigade.is_some())).collect();
        }
        let first = |d: Duty| matches!(d, Duty::Line | Duty::Melee | Duty::Cavalry);
        let broken = |i: usize| {
            let p = &mine[i];
            p.alive * 100 < p.start * BROKEN_PERCENT
                || (p.alive > 0 && self.formed[i] == Some(true) && p.brigade.is_none())
                || (p.alive > 0 && p.panicking * 100 >= p.alive * PANIC_PERCENT)
        };
        let gone: Vec<usize> = (0..n).filter(|&i| first(army.bodies[i].duty) && broken(i)).collect();
        for i in gone {
            let cavalry = army.bodies[i].duty == Duty::Cavalry;
            let pick = (0..n)
                .filter(|&k| army.bodies[k].duty == Duty::Reserve { cavalry } && mine[k].alive > 0 && !broken(k))
                .min_by_key(|&k| ((army.bodies[k].lateral - army.bodies[i].lateral).abs(), k));
            let Some(k) = pick else { continue };
            let spot = mine[k].centroid;
            army.bodies[k].duty = army.bodies[i].duty;
            army.bodies[i].duty = Duty::Reserve { cavalry };
            let units: Vec<_> =
                army.bodies[i].units.iter().copied().filter(|&id| living(world, id).is_some()).collect();
            if mine[i].alive > 0 {
                if let (Some(brigade), Some(group)) = (mine[i].brigade, mine[i].group)
                    && let Ok(group) = u8::try_from(group)
                {
                    out.push(Command::Reform { player: army.owner, brigade, group });
                }
                out.push(Command::MoveFacing { player: army.owner, units, x: spot.0, y: spot.1, dir: army.facing });
            }
            report.reserves += 1;
        }

        // The reserve closes up behind the line's front, never back.
        let front = (0..n)
            .filter(|&i| matches!(army.bodies[i].duty, Duty::Line | Duty::Melee) && mine[i].alive > 0)
            .map(|i| mine[i].f)
            .max();
        let Some(front) = front else { return };
        for i in (0..n).filter(|&i| matches!(army.bodies[i].duty, Duty::Reserve { .. }) && mine[i].alive > 0) {
            let p = &mine[i];
            let gain = front - BEHIND - p.f;
            if gain > SLACK && p.in_order && !broken(i) {
                let (dx, dy) = to_world(army.facing, gain.min(STEP), 0);
                let units = army.bodies[i].units.iter().copied().filter(|&id| living(world, id).is_some()).collect();
                out.push(Command::MoveFacing {
                    player: army.owner,
                    units,
                    x: p.centroid.0 + dx,
                    y: p.centroid.1 + dy,
                    dir: army.facing,
                });
            }
        }
    }
}
