//! The batteries: which target each gun fires at, ordered the way the original's interface
//! orders it.
//!
//! A gun never picks a target itself ([`acr_sim::artillery::auto_canister`] only fires a
//! volley ahead), and an attack order on a target out of its range makes it drive there. So a
//! target is ordered only when it stands inside `[r1, r2 - RANGE_MARGIN]` of the gun's ranged
//! attacks, and `Attack` (priority 0xFE, which ends stand ground) is always followed by
//! `StandGround` in the same batch: a standing gun drops a target that leaves its range
//! (0x5204aa) instead of following it, and never moves. A gun gets no other order.

use acr_sim::trig::get_dir;
use acr_sim::world::Order;
use acr_sim::{Command, EntityId, World};

use super::frame::{dist, isqrt};
use super::picture::{Picture, living};
use super::{Army, Duty, Report};

/// A target must stand this far inside the gun's longest range, pixels (a ball needs the
/// distance to land).
const RANGE_MARGIN: i32 = 64;
/// Men of a body within this of the aim man count as massed, pixels.
const MASS_RADIUS: i32 = 160;
/// A body whose aim man is this close to the gun is a threat to it, pixels.
const THREAT_RANGE: i32 = 600;
/// A body is closing when its centroid came this much nearer the guns since the last decision.
const CLOSING: i32 = 30;
/// A body shows its flank to the gun when its facing is within this of a right angle to the
/// bearing (1/256 turns).
const ENFILADE_SLACK: i32 = 32;
/// Within this the gun fires canister and its cone widens with the distance (the original
/// picks `KARTECH` below 1500 px, the ball from there to 6000), pixels.
const CANISTER_REACH: i32 = 1500;
/// Own units this close behind the gun do not stand in its line of fire, pixels.
const LINE_NEAR: i32 = 64;
/// Own units up to this far beyond the aim point do (a ball skips on), pixels.
const LINE_BEYOND: i32 = 700;
/// Half the width of the strip along a ball's line in which an own unit is hit, pixels.
const BALL_WIDTH: i32 = 48;
/// A new target replaces the kept one only when it scores more than this fraction of it.
const SWITCH: (i64, i64) = (3, 2);
/// The player's gun that has moved farther than this from where it was placed is his own, pixels.
const HAND_OVER_DRIFT: i32 = 32;
/// The groups that stand in mass: column (`SQUARE`) and square (`KARE`); more of a body is hit.
const MASSED_GROUPS: [&str; 2] = ["SQUARE", "KARE"];

/// One gun and what the controller has ordered it.
#[derive(Debug, Clone)]
struct Gun {
    id: EntityId,
    /// Where the gun stood when the battle began, pixels.
    home: (i32, i32),
    /// The target the controller last ordered.
    aim: Option<EntityId>,
    /// The player gave it an order of his own: the controller leaves it alone for good.
    handed: bool,
}

/// All guns of one side.
#[derive(Debug, Clone, Default)]
pub(super) struct Battery {
    guns: Vec<Gun>,
    /// The distance of each enemy body's centroid to the guns at the last decision.
    seen: Vec<Option<i32>>,
}

impl Battery {
    /// The guns of the army's `Guns` bodies, by body and id.
    pub(super) fn new(world: &World, army: &Army) -> Self {
        let mut guns = Vec::new();
        for body in army.bodies.iter().filter(|b| b.duty == Duty::Guns) {
            let mut ids = body.units.clone();
            ids.sort_by_key(|id| id.0);
            for id in ids {
                let home = world.units.get(id.0 as usize).map_or((0, 0), |u| u.pos.pixels());
                guns.push(Gun { id, home, aim: None, handed: false });
            }
        }
        Self { guns, seen: Vec::new() }
    }

    /// One decision: appends the orders for `me`'s guns against `foe`, whose bodies stand as
    /// `foe_pics`. With `hand_over` a gun the player ordered himself is left alone.
    pub(super) fn decide(
        &mut self,
        world: &World,
        (me, foe): (&Army, &Army),
        foe_pics: &[Picture],
        hand_over: bool,
        (out, report): (&mut Vec<Command>, &mut Report),
    ) {
        if hand_over {
            for g in &mut self.guns {
                g.handed |= living(world, g.id).is_some_and(|u| taken(u, g));
            }
        }
        let ids: Vec<EntityId> = self.guns.iter().map(|g| g.id).collect();
        let live: Vec<usize> = (0..self.guns.len()).filter(|&i| living(world, self.guns[i].id).is_some()).collect();
        let pos = |id: EntityId| living(world, id).map(|u| u.pos.pixels());
        report.guns_drift_max = live
            .iter()
            .filter_map(|&i| pos(self.guns[i].id).map(|p| dist(p, self.guns[i].home)))
            .fold(report.guns_drift_max, i32::max);
        let own: Vec<usize> = live.iter().copied().filter(|&i| !self.guns[i].handed).collect();
        if own.is_empty() {
            return;
        }
        let n = own.len() as i64;
        let sum = own
            .iter()
            .filter_map(|&i| pos(self.guns[i].id))
            .fold((0i64, 0i64), |a, p| (a.0 + i64::from(p.0), a.1 + i64::from(p.1)));
        let centre = ((sum.0 / n) as i32, (sum.1 / n) as i32);
        let now: Vec<Option<i32>> = foe_pics.iter().map(|p| (p.alive > 0).then(|| dist(p.centroid, centre))).collect();
        let eyes = Eyes { world, me, foe, foe_pics, guns: &ids, seen: &self.seen, now: &now };
        for i in own {
            let g = &mut self.guns[i];
            let Some(target) = eyes.choose(g) else {
                g.aim = None;
                continue;
            };
            let u = &world.units[g.id.0 as usize];
            if g.aim != Some(target) || u.target() != Some(target) {
                out.push(Command::Attack { player: me.owner, units: vec![g.id], target });
                out.push(Command::StandGround { player: me.owner, units: vec![g.id], on: true });
                report.gun_orders += 1;
            }
            g.aim = Some(target);
        }
        self.seen = now;
    }
}

/// Whether the player has taken the gun: an order other than the controller's attack, or a
/// position away from where it was placed.
fn taken(u: &acr_sim::Unit, g: &Gun) -> bool {
    let foreign = match u.orders.as_slice() {
        [] => false,
        [Order::Attack { target, .. }] => g.aim != Some(*target),
        _ => true,
    };
    foreign || dist(u.pos.pixels(), g.home) > HAND_OVER_DRIFT
}

/// What one decision of a battery looks at.
struct Eyes<'a> {
    world: &'a World,
    me: &'a Army,
    foe: &'a Army,
    foe_pics: &'a [Picture],
    /// All guns of the side (they never stand in each other's line of fire).
    guns: &'a [EntityId],
    /// The enemy bodies' distances to the guns at the last decision and now.
    seen: &'a [Option<i32>],
    now: &'a [Option<i32>],
}

/// A target for one gun.
#[derive(Debug, Clone, Copy)]
struct Aim {
    man: EntityId,
    score: i64,
}

impl Eyes<'_> {
    /// The ranged attacks' span `(r1, r2)` of a gun, pixels.
    fn range(&self, id: EntityId) -> Option<(i32, i32)> {
        let u = &self.world.units[id.0 as usize];
        let adv = self.world.adv(u.owner, u.kind);
        let ranged = adv.attacks.iter().filter(|a| a.weapon.is_some());
        let lo = ranged.clone().map(|a| a.radius.0).min()?;
        let hi = ranged.map(|a| a.radius.1).max()?;
        Some((lo, hi - RANGE_MARGIN))
    }

    /// The target for gun `g`: the kept one unless it became invalid or a new one scores more
    /// than [`SWITCH`] times as much.
    fn choose(&self, g: &Gun) -> Option<EntityId> {
        let from = living(self.world, g.id)?.pos.pixels();
        let (lo, hi) = self.range(g.id)?;
        let valid = |man: EntityId, body: usize| -> Option<Aim> {
            let at = living(self.world, man)?.pos.pixels();
            let d = dist(from, at);
            if d < lo || d > hi || self.blocked(from, at, d) {
                return None;
            }
            Some(Aim { man, score: self.score(body, from, at, d) })
        };
        let mut best: Option<Aim> = None;
        for p in self.foe_pics.iter().filter(|p| p.alive > 0) {
            // The living man nearest the gun inside its range, the lowest id among equals.
            let near = self.foe.bodies[p.body]
                .units
                .iter()
                .filter_map(|&id| living(self.world, id).map(|u| (dist(from, u.pos.pixels()), id)))
                .filter(|&(d, _)| (lo..=hi).contains(&d))
                .min_by_key(|&(d, id)| (d, id.0));
            if let Some(a) = near.and_then(|(_, man)| valid(man, p.body))
                && best.is_none_or(|b| a.score > b.score)
            {
                best = Some(a);
            }
        }
        let kept = g.aim.and_then(|man| {
            let body = self.foe.bodies.iter().position(|b| b.units.contains(&man))?;
            valid(man, body)
        });
        match (kept, best) {
            (Some(k), Some(b)) if b.score * SWITCH.1 > k.score * SWITCH.0 => Some(b.man),
            (Some(k), _) => Some(k.man),
            (None, b) => b.map(|b| b.man),
        }
    }

    /// How much firing at `body`'s man at `at`, `d` px from the gun at `from`, is worth:
    /// the men massed around the man at `at`, scaled by what the shot does to such a body.
    fn score(&self, body: usize, from: (i32, i32), at: (i32, i32), d: i32) -> i64 {
        let p = &self.foe_pics[body];
        let massed = self.foe.bodies[body]
            .units
            .iter()
            .filter_map(|&id| living(self.world, id))
            .filter(|u| dist(u.pos.pixels(), at) <= MASS_RADIUS)
            .count() as i64;
        let (mut num, mut den) = (1i64, 1i64);
        // Columns and squares: a ball goes through more of them.
        let group = p.group.and_then(|g| self.world.formations.groups.get(g)).map(|g| g.name.as_str());
        if group.is_some_and(|n| MASSED_GROUPS.iter().any(|m| n.eq_ignore_ascii_case(m))) {
            (num, den) = (num * 3, den * 2);
        }
        // Enfilade: the gun sees the body's flank, so the ball runs along the line.
        let bearing = get_dir(at.0 - from.0, at.1 - from.1);
        let turn = i32::from(p.facing.wrapping_sub(bearing) as i8).abs();
        if (turn - 64).abs() <= ENFILADE_SLACK {
            num *= 2;
        }
        let closing = matches!(
            (self.seen.get(body).copied().flatten(), self.now[body]),
            (Some(before), Some(now)) if before - now >= CLOSING
        );
        if p.cavalry && closing {
            num *= 2;
        }
        if d <= THREAT_RANGE {
            num *= 3;
        }
        if self.foe.bodies[body].duty == Duty::Guns {
            den *= 2;
        }
        massed * 256 * num / den
    }

    /// Whether an own unit stands in the line of fire from `from` to `to` (`d` px): from
    /// [`LINE_NEAR`] to [`LINE_BEYOND`] past the aim point, within a ball's strip or a
    /// canister's cone (a quarter of the distance to either side).
    fn blocked(&self, from: (i32, i32), to: (i32, i32), d: i32) -> bool {
        let v = (i64::from(to.0 - from.0), i64::from(to.1 - from.1));
        let len = isqrt(v.0 * v.0 + v.1 * v.1);
        if len == 0 {
            return false;
        }
        let canister = d < CANISTER_REACH;
        self.world.units.iter().any(|u| {
            if !u.alive || u.death != 0 || u.building || u.owner != self.me.owner || self.guns.contains(&u.id) {
                return false;
            }
            let (x, y) = u.pos.pixels();
            let w = (i64::from(x - from.0), i64::from(y - from.1));
            let along = (w.0 * v.0 + w.1 * v.1) / len;
            let across = (w.0 * v.1 - w.1 * v.0).abs() / len;
            let width = if canister { along / 4 } else { i64::from(BALL_WIDTH) };
            (i64::from(LINE_NEAR)..=len + i64::from(LINE_BEYOND)).contains(&along) && across <= width
        })
    }
}
