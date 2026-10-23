//! Marks on the detailed ground (docs/ground.md, phase 2): boot prints of walking men,
//! deeper paired hoof prints of horses, wheel ruts of guns, drag smears of crawlers, the
//! pressed soil under bodies, and craters with a furrow along the flight for cannonballs
//! (each bounce of #258 is an impact) and scorched craters for shells. Render only: it
//! reads the world after each tick and stamps `acr_render::ground_state::GroundState`, which
//! replaces the per-vertex trample darkening and the flat dirt decals in detailed mode.

use std::collections::{HashMap, HashSet};

use acr_render::ground_state::{GroundState, Shape, Stamp, metres};
use acr_sim::World;
use acr_sim::combat::DeathCause;

/// Recovery per tick, as the trampling's (`trample.rs`).
const KEEP: f32 = 0.9997;

/// What a moving unit last left.
struct Walker {
    /// Position at the last tick (world pixels).
    last: [f32; 2],
    /// Distance since the last print (world pixels).
    since: f32,
    /// Which foot (or hoof pair) is next.
    left: bool,
}

/// A step that left a print, for the particles (mud flicks, dust).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footfall {
    pub gait: Gait,
    pub at: [f32; 2],
    /// Heading (unit).
    pub along: [f32; 2],
    /// Distance since the last print per tick (world px per tick).
    pub speed: f32,
    pub unit: u32,
    pub tick: u64,
}

/// Footfalls kept at most between two frames.
const MAX_FOOTFALLS: usize = 1024;

#[derive(Default)]
pub struct GroundMarks {
    pub state: GroundState,
    /// This frame's steps, for the particles (taken by the 3D view).
    pub footfalls: Vec<Footfall>,
    walkers: HashMap<u32, Walker>,
    bodies: HashSet<u32>,
    last_impact: u64,
}

/// The kind of mover, for its prints.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gait {
    Boots,
    Hooves,
    Wheels,
    Crawl,
}

impl Gait {
    /// Distance between prints of one side (world pixels).
    fn stride(self) -> f32 {
        match self {
            Gait::Boots => metres(0.75),
            Gait::Hooves => metres(1.6),
            Gait::Wheels | Gait::Crawl => metres(0.25),
        }
    }
}

/// The prints of one step of `gait` at `at`, heading `along` (unit), foot `left`.
pub fn prints(gait: Gait, at: [f32; 2], from: [f32; 2], along: [f32; 2], left: bool) -> Vec<Stamp> {
    let side = [-along[1], along[0]];
    let off = |p: [f32; 2], s: f32, f: f32| [p[0] + side[0] * s + along[0] * f, p[1] + side[1] * s + along[1] * f];
    let sign = if left { -1.0 } else { 1.0 };
    match gait {
        Gait::Boots => {
            vec![Stamp::print(off(at, sign * metres(0.12), 0.0), along, [metres(0.14), metres(0.055)], -0.012, 0.22)]
        }
        Gait::Hooves => {
            // A pair (fore and hind of one side), round, deeper, a little mud thrown up.
            let a = off(at, sign * metres(0.22), metres(0.35));
            let b = off(at, sign * metres(0.2), -metres(0.4));
            [a, b]
                .into_iter()
                .map(|p| Stamp { mud: 0.25, ..Stamp::print(p, along, [metres(0.075), metres(0.07)], -0.035, 0.45) })
                .collect()
        }
        Gait::Wheels => [-1.0, 1.0]
            .into_iter()
            .map(|s| Stamp {
                shape: Shape::Stroke { to: off(from, s * metres(0.75), 0.0) },
                ..Stamp::print(off(at, s * metres(0.75), 0.0), along, [metres(0.05), metres(0.05)], -0.04, 0.6)
            })
            .collect(),
        Gait::Crawl => vec![Stamp {
            shape: Shape::Stroke { to: from },
            mud: 0.15,
            ..Stamp::print(at, along, [metres(0.2), metres(0.2)], -0.01, 0.5)
        }],
    }
}

/// The crater (and furrow along the flight) of a round of `cause` striking at `at`,
/// flying along `dir` (unit, may be zero).
pub fn crater(cause: DeathCause, at: [f32; 2], dir: [f32; 2]) -> Vec<Stamp> {
    let base = |half: f32, depth: f32, rim: f32, scorch: f32| Stamp {
        at,
        shape: Shape::Crater { rim },
        along: [1.0, 0.0],
        half: [metres(half), metres(half)],
        depth,
        compress: 0.5,
        mud: 0.3,
        scorch,
    };
    match cause {
        DeathCause::Cannonball => {
            let mut out = vec![base(0.45, -0.18, 0.05, 0.0)];
            if dir != [0.0, 0.0] {
                // The ball ploughs on before it rises: a furrow along the flight.
                let to = [at[0] + dir[0] * metres(1.8), at[1] + dir[1] * metres(1.8)];
                out.push(Stamp {
                    shape: Shape::Stroke { to },
                    ..Stamp::print(at, dir, [metres(0.16), metres(0.16)], -0.08, 0.6)
                });
            }
            out
        }
        DeathCause::Explosion => vec![base(1.0, -0.25, 0.08, 0.9)],
        DeathCause::Grapeshot => vec![base(0.12, -0.05, 0.015, 0.0)],
        _ => Vec::new(),
    }
}

impl GroundMarks {
    /// One simulation tick.
    pub fn tick(&mut self, world: &World) {
        self.state.fade(KEEP);
        let sub = 16.0;
        let mut seen = HashSet::new();
        for u in &world.units {
            if u.building || !u.alive || u.inside_of.is_some() {
                continue;
            }
            let at = [u.pos.x as f32 / sub, u.pos.y as f32 / sub];
            let t = world.catalog.get(u.kind);
            let rider = acr_sim::spacing::is_rider(t);
            if u.death > 0 {
                // A body presses the soil once where it fell.
                if self.bodies.insert(u.id.0) {
                    let a = f32::from(u.dir) / 256.0 * std::f32::consts::TAU;
                    let along = [a.cos(), a.sin()];
                    let (len, wide) = if rider { (1.2, 0.5) } else { (0.85, 0.25) };
                    let to = [at[0] + along[0] * metres(len), at[1] + along[1] * metres(len)];
                    self.state.stamp(&Stamp {
                        shape: Shape::Stroke { to },
                        ..Stamp::print(at, along, [metres(wide), metres(wide)], -0.03, 0.7)
                    });
                }
                continue;
            }
            seen.insert(u.id.0);
            let gait = if t.role.artillery {
                Gait::Wheels
            } else if rider {
                Gait::Hooves
            } else if u.body.crawling() {
                Gait::Crawl
            } else {
                Gait::Boots
            };
            let w = self.walkers.entry(u.id.0).or_insert(Walker { last: at, since: 0.0, left: false });
            let (dx, dy) = (at[0] - w.last[0], at[1] - w.last[1]);
            let d = (dx * dx + dy * dy).sqrt();
            if d < 0.01 || d > 200.0 {
                // Standing, or teleported (entered, spawned): no trail.
                w.last = at;
                continue;
            }
            w.since += d;
            let stride = gait.stride();
            if w.since >= stride * 0.5 {
                w.since = 0.0;
                w.left = !w.left;
                let along = [dx / d, dy / d];
                if matches!(gait, Gait::Boots | Gait::Hooves) && self.footfalls.len() < MAX_FOOTFALLS {
                    self.footfalls.push(Footfall { gait, at, along, speed: d, unit: u.id.0, tick: world.tick.0 });
                }
                for s in prints(gait, at, w.last, along, w.left) {
                    self.state.stamp(&s);
                }
                w.last = at;
            } else if matches!(gait, Gait::Wheels | Gait::Crawl) {
                // Continuous marks keep their start until the next stamp.
            } else {
                w.last = at;
            }
        }
        self.walkers.retain(|id, _| seen.contains(id));
        for i in world.impacts.iter().filter(|i| i.tick > self.last_impact) {
            let (dx, dy) = (i.dir.0 as f32, i.dir.1 as f32);
            let len = (dx * dx + dy * dy).sqrt();
            let dir = if len > 0.0 { [dx / len, dy / len] } else { [0.0; 2] };
            for s in crater(i.cause, [i.pos.0 as f32, i.pos.1 as f32], dir) {
                self.state.stamp(&s);
            }
        }
        if let Some(last) = world.impacts.back() {
            self.last_impact = self.last_impact.max(last.tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooves_dig_deeper_and_wider_apart_than_boots() {
        let boots = prints(Gait::Boots, [500.0, 500.0], [480.0, 500.0], [1.0, 0.0], true);
        let hooves = prints(Gait::Hooves, [500.0, 500.0], [480.0, 500.0], [1.0, 0.0], true);
        assert_eq!((boots.len(), hooves.len()), (1, 2));
        assert!(hooves[0].depth < boots[0].depth * 2.0);
        assert!(Gait::Hooves.stride() > 2.0 * Gait::Boots.stride());
        let mut g = GroundState::new();
        hooves.iter().for_each(|s| g.stamp(s));
        assert!(g.texel(hooves[0].at[0], hooves[0].at[1])[3] < 118);
    }

    #[test]
    fn wheels_leave_two_ruts_a_cannonball_a_crater_and_furrow() {
        let ruts = prints(Gait::Wheels, [600.0, 500.0], [500.0, 500.0], [1.0, 0.0], false);
        assert_eq!(ruts.len(), 2);
        assert!((ruts[0].at[1] - ruts[1].at[1]).abs() > metres(1.4));
        let c = crater(DeathCause::Cannonball, [1000.0, 1000.0], [1.0, 0.0]);
        assert_eq!(c.len(), 2);
        let shell = crater(DeathCause::Explosion, [1000.0, 1000.0], [0.0, 0.0]);
        assert!(shell[0].scorch > 0.5);
        assert!(crater(DeathCause::Melee, [0.0, 0.0], [1.0, 0.0]).is_empty());
    }
}
