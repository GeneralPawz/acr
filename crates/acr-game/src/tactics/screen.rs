//! The skirmishers: they fire from in front of the line (they search for targets on their
//! own and get no order) and fall back, once, when formed enemy infantry comes within
//! [`FOOT`], enemy riders within [`RIDERS_RANGE`] or the own line has caught up within
//! [`OVERRUN`]. They fall back to [`BEHIND`] the line's front and [`OUTSIDE`] its nearer flank,
//! out of its way and out of its line of fire, and stand ground there.

use acr_formats::m3d::MapSize;
use acr_sim::{Command, World};

use super::cavalry::MAP_MARGIN;
use super::frame::{dist, to_army, to_world};
use super::picture::living;
use super::{Duty, Report, View};

/// Formed enemy infantry this near drives the skirmishers back, pixels.
const FOOT: i32 = 500;
/// Enemy riders this near do, pixels.
const RIDERS_RANGE: i32 = 900;
/// Enemy riders count as a body of at least this many.
const RIDERS: u32 = 5;
/// The own line this near is on top of them, pixels.
const OVERRUN: i32 = 200;
/// They fall back to this far behind the line's front, pixels...
const BEHIND: i32 = 250;
/// ...and this far outside its nearer flank.
const OUTSIDE: i32 = 150;
/// They have arrived within this of the spot, pixels, or after [`BACK_TICKS`].
const ARRIVED: i32 = 120;
const BACK_TICKS: u64 = 400;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum State {
    /// In front, firing.
    #[default]
    Front,
    /// Falling back to the spot.
    Back { since: u64, to: (i32, i32) },
    /// At the spot, standing ground.
    Held,
}

/// The skirmishers of one side.
#[derive(Debug, Clone, Default)]
pub(super) struct Screens {
    states: Vec<State>,
}

/// The extent of the line (foot bodies with the duty `Line` or `Melee`) in the army's frame:
/// the front `f` and the `s` range of its living men.
fn line_box(world: &World, v: &View) -> Option<(i32, i32, i32)> {
    let (mut f_max, mut s_min, mut s_max) = (i32::MIN, i32::MAX, i32::MIN);
    for b in v.me.bodies.iter().filter(|b| matches!(b.duty, Duty::Line | Duty::Melee)) {
        for id in &b.units {
            let Some(u) = living(world, *id) else { continue };
            let (x, y) = u.pos.pixels();
            let (f, s) = to_army(v.me.facing, x - v.me.origin.0, y - v.me.origin.1);
            (f_max, s_min, s_max) = (f_max.max(f), s_min.min(s), s_max.max(s));
        }
    }
    (s_min <= s_max).then_some((f_max, s_min, s_max))
}

impl Screens {
    pub(super) fn decide(&mut self, world: &World, v: &View, (out, _): (&mut Vec<Command>, &mut Report)) {
        let n = v.me.bodies.len();
        self.states.resize(n, State::default());
        let limit = MapSize { shift: world.map.shift }.playable_size() - MAP_MARGIN;
        for i in (0..n).filter(|&i| v.me.bodies[i].duty == Duty::Screen && v.mine[i].alive > 0) {
            let p = &v.mine[i];
            let units: Vec<_> =
                v.me.bodies[i].units.iter().copied().filter(|&id| living(world, id).is_some()).collect();
            match self.states[i] {
                State::Front => {
                    let Some((f_max, s_min, s_max)) = line_box(world, v) else { continue };
                    let pressed = (0..v.theirs.len()).any(|j| {
                        let (t, duty) = (&v.theirs[j], v.foe.bodies[j].duty);
                        let d = dist(p.centroid, t.centroid);
                        let riders = t.cavalry || duty == Duty::Cavalry;
                        t.alive > 0
                            && if riders {
                                t.alive >= RIDERS && d <= RIDERS_RANGE
                            } else {
                                !matches!(duty, Duty::Screen | Duty::Guns) && d <= FOOT
                            }
                    }) || (0..n).any(|k| {
                        matches!(v.me.bodies[k].duty, Duty::Line | Duty::Melee)
                            && v.mine[k].alive > 0
                            && dist(p.centroid, v.mine[k].centroid) <= OVERRUN
                    });
                    if !pressed {
                        continue;
                    }
                    let s = if p.s < (s_min + s_max) / 2 { s_min - OUTSIDE } else { s_max + OUTSIDE };
                    let (dx, dy) = to_world(v.me.facing, f_max - BEHIND, s);
                    let to =
                        ((v.me.origin.0 + dx).clamp(MAP_MARGIN, limit), (v.me.origin.1 + dy).clamp(MAP_MARGIN, limit));
                    out.push(Command::MoveFacing { player: v.me.owner, units, x: to.0, y: to.1, dir: v.me.facing });
                    self.states[i] = State::Back { since: v.tick, to };
                }
                State::Back { since, to } => {
                    if dist(p.centroid, to) <= ARRIVED || v.tick - since >= BACK_TICKS {
                        out.push(Command::StandGround { player: v.me.owner, units, on: true });
                        self.states[i] = State::Held;
                    }
                }
                State::Held => {}
            }
        }
    }
}
