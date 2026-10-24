//! Cannonballs against trees and stones (optional rule [`crate::Rules::ball_terrain`],
//! together with [`crate::Rules::hit_locations`]; `docs/simulation.md`, "Balls against trees
//! and stones"). Not in the original game: `ProcessExpl` (0x557fa0) tests a round only against
//! the ground (0x4d0530) and the building boxes (0x443b06 / 0x443d37), never against a map
//! sprite, so a ball flies through woods and rocks.
//!
//! With the rule on, a ploughing ball ([`crate::projectile::Plough`]: a cannonball's aimed
//! strike or a low bounce) that passes a tree trunk or a stone within [`TRUNK_TOP`] /
//! [`STONE_TOP`] px of the ground meets it:
//! - a tree (a `TREELIST` object that is a wood source) has a girth of 20..80 % (a hash of
//!   its place, never the RNG); a ball with at least that much energy left fells it away
//!   from the impact and loses half the girth; a weaker ball splinters it and stops;
//! - a small stone (its ground cell free) is knocked [`STONE_PUSH`] px aside, the ball loses
//!   [`STONE_LOSS`]; a big rock (its ground cell locked, a `[LOCK]` sprite) chips and stops
//!   the ball.
//!
//! No random number is drawn. The state is [`World::struck`], hashed only while not empty.
//! Trees never block paths or sight in the engine (`docs/simulation.md` "Passability"), and
//! a felled tree stays a wood source (its trunk lies there), so paths and gathering do not
//! change; small stones never block either, and big rocks stay where they are.

use std::collections::BTreeMap;

use acr_formats::m3d::SpriteGroup;

use crate::projectile::Projectile;
use crate::trig::norma;
use crate::world::World;

/// A trunk is met below this height above the ground (px; a ball higher passes the crown).
pub const TRUNK_TOP: i32 = 100;
/// A stone is met below this height above the ground (px).
pub const STONE_TOP: i32 = 40;
/// Reach of a trunk around its point (trunk radius plus the ball's, px).
pub const TRUNK_REACH: i32 = 10;
/// Reach of a small stone and of a big rock (px).
pub const STONE_REACH: i32 = 12;
pub const ROCK_REACH: i32 = 24;
/// How far a small stone is knocked (px), 45° off the ball's way.
pub const STONE_PUSH: i32 = 40;
/// Energy (percent of the ball's) a knocked stone takes out of the ball.
pub const STONE_LOSS: u8 = 20;

/// What a ball did to a map object ([`World::objects`] index in [`World::struck`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Struck {
    /// The tree is down: it fell towards `dir` (0 = east, 64 = south) at `tick`. Its stump
    /// and trunk stay (still a wood source).
    Felled { dir: u8, tick: u64 },
    /// A thick tree took `hits` balls that it stopped; the last at `tick` from `dir`.
    Splintered { hits: u8, dir: u8, tick: u64 },
    /// A small stone lies at `(dx, dy)` px from its map place, knocked there from `from` at
    /// `tick`.
    Moved { dx: i32, dy: i32, from: (i32, i32), tick: u64 },
    /// A big rock took `chips` balls; the last at `tick` from `dir`.
    Chipped { chips: u8, dir: u8, tick: u64 },
}

impl Struck {
    /// The tick of the last strike.
    pub fn tick(&self) -> u64 {
        match *self {
            Struck::Felled { tick, .. }
            | Struck::Splintered { tick, .. }
            | Struck::Moved { tick, .. }
            | Struck::Chipped { tick, .. } => tick,
        }
    }
}

/// Struck map objects by [`World::objects`] index.
pub type StruckMap = BTreeMap<u32, Struck>;

/// A tree's girth: the energy (percent) a ball needs to fell it, 20..=80, from its place.
pub fn girth(x: i32, y: i32) -> u8 {
    let h = (x.wrapping_mul(-1_640_531_535) ^ y.wrapping_mul(805_459_861)) as u32;
    20 + ((h >> 11) % 61) as u8
}

/// Where map object `k` is now (world px): its place plus a knock.
pub fn place(w: &World, k: usize) -> (i32, i32) {
    let o = &w.objects[k];
    match w.struck.get(&(k as u32)) {
        Some(Struck::Moved { dx, dy, .. }) => (o.x + dx, o.y + dy),
        _ => (o.x, o.y),
    }
}

/// A `TREELIST` object that is a wood source (a tree, not a bush, flag or ruin).
fn is_tree(w: &World, x: i32, y: i32) -> bool {
    w.sources.nearest(x, y).is_some_and(|(d, kind)| d == 0 && usize::from(kind) == crate::economy::WOOD)
}

/// A big rock: a stone whose ground cell (or one next to it) is locked.
fn is_rock(w: &World, x: i32, y: i32) -> bool {
    let (cx, cy) = (x >> 4, y >> 4);
    [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(a, b)| {
        let (px, py) = (cx + a, cy + b);
        px >= 0 && py >= 0 && w.map.land.check_pt(px, py)
    })
}

/// The flight direction of a ball (0 = east, 64 = south).
fn dir_of(p: &Projectile) -> u8 {
    crate::trig::get_dir(p.vel.0 >> 10, p.vel.1 >> 10)
}

/// The ball in `slot` has just moved by its velocity: what it met on that way, the nearest
/// first. Returns whether the ball stopped (its slot is then empty).
pub fn meet(w: &mut World, slot: usize) -> bool {
    let Some(p) = w.projectiles.slots[slot] else { return false };
    if p.plough.spent() || w.objects.is_empty() {
        return false;
    }
    let (x1, y1, z) = (p.pos.0 >> 14, p.pos.1 >> 14, p.pos.2 >> 14);
    let (x0, y0) = ((p.pos.0 - p.vel.0) >> 14, (p.pos.1 - p.vel.1) >> 14);
    let above = z - w.map.heights.at(x1, y1);
    if above >= TRUNK_TOP {
        return false;
    }
    let (sx, sy) = (i64::from(x1 - x0), i64::from(y1 - y0));
    let len2 = sx * sx + sy * sy;
    let reach_max = ROCK_REACH;
    let (lx, hx) = (x0.min(x1) - reach_max, x0.max(x1) + reach_max);
    let (ly, hy) = (y0.min(y1) - reach_max, y0.max(y1) + reach_max);
    // The nearest object along the way: (fraction of the way in 1/1024, index).
    let mut best: Option<(i64, usize)> = None;
    for k in 0..w.objects.len() {
        let group = w.objects[k].group;
        if !matches!(group, SpriteGroup::Trees | SpriteGroup::Stones) {
            continue;
        }
        let (ox, oy) = place(w, k);
        if ox < lx || ox > hx || oy < ly || oy > hy {
            continue;
        }
        if matches!(w.struck.get(&(k as u32)), Some(Struck::Felled { .. })) {
            continue;
        }
        let reach = match group {
            SpriteGroup::Trees if is_tree(w, w.objects[k].x, w.objects[k].y) => TRUNK_REACH,
            SpriteGroup::Stones if above < STONE_TOP => {
                if is_rock(w, ox, oy) {
                    ROCK_REACH
                } else {
                    STONE_REACH
                }
            }
            _ => continue,
        };
        let (dx, dy) = (i64::from(ox - x0), i64::from(oy - y0));
        let t = if len2 == 0 { 0 } else { ((dx * sx + dy * sy) * 1024 / len2).clamp(0, 1024) };
        let (cx, cy) = (x0 + (sx * t / 1024) as i32, y0 + (sy * t / 1024) as i32);
        if norma(ox - cx, oy - cy) > reach {
            continue;
        }
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, k));
        }
    }
    let Some((_, k)) = best else { return false };
    let dir = dir_of(&p);
    let tick = w.tick.0;
    let energy = p.plough.energy();
    let o = w.objects[k].clone();
    let key = k as u32;
    let (loss, stop) = if o.group == SpriteGroup::Trees {
        let g = girth(o.x, o.y);
        if energy >= g {
            w.struck.insert(key, Struck::Felled { dir, tick });
            (g / 2, false)
        } else {
            let hits = match w.struck.get(&key) {
                Some(Struck::Splintered { hits, .. }) => hits.saturating_add(1),
                _ => 1,
            };
            w.struck.insert(key, Struck::Splintered { hits, dir, tick });
            (0, true)
        }
    } else {
        let (ox, oy) = place(w, k);
        if is_rock(w, ox, oy) || energy < STONE_LOSS {
            let chips = match w.struck.get(&key) {
                Some(Struck::Chipped { chips, .. }) => chips.saturating_add(1),
                _ => 1,
            };
            if is_rock(w, ox, oy) {
                w.struck.insert(key, Struck::Chipped { chips, dir, tick });
            }
            (0, true)
        } else {
            // Knocked 45° off the ball's way, to the side the ball passed it on.
            let (bx, by) = (x1 - x0, y1 - y0);
            let side = i64::from(bx) * i64::from(oy - y0) - i64::from(by) * i64::from(ox - x0);
            let d = if side >= 0 { dir.wrapping_add(32) } else { dir.wrapping_sub(32) };
            let (mx, my) = ((crate::trig::cos(d) * STONE_PUSH) >> 8, (crate::trig::sin(d) * STONE_PUSH) >> 8);
            w.struck
                .insert(key, Struck::Moved { dx: ox + mx - o.x, dy: oy + my - o.y, from: (ox - o.x, oy - o.y), tick });
            (STONE_LOSS, false)
        }
    };
    if stop {
        w.projectiles.slots[slot] = None;
        return true;
    }
    if let Some(Some(q)) = w.projectiles.slots.get_mut(slot) {
        q.plough.lost = q.plough.lost.saturating_add(loss);
        // The way is slowed by the same share.
        let keep = 100 - i32::from(loss.min(100));
        q.vel = (q.vel.0 / 100 * keep, q.vel.1 / 100 * keep, q.vel.2);
    }
    false
}

/// Puts a tree or stone on the map after it was made (the test field, unit tests): a
/// `TREELIST` object `wood` becomes a wood source as the map's trees are (4 per work cycle
/// within 30 px, `docs/simulation.md` "Gathering"); a `rock` locks the 3 × 3 land cells
/// around it as a `[LOCK]` rock does. Returns the object's index.
pub fn add_object(w: &mut World, group: SpriteGroup, index: u16, x: i32, y: i32, wood: bool, rock: bool) -> usize {
    w.objects.push(crate::world::MapObject { group, index, x, y });
    if wood {
        let mut list = w.sources.list.clone();
        list.push(crate::gather::Source { x, y, kind: crate::economy::WOOD as u8, per_work: 4, radius: 30 });
        w.sources = std::sync::Arc::new(crate::gather::Sources::new(list, w.map.land.width * 16));
    }
    if rock {
        let map = std::sync::Arc::make_mut(&mut w.map);
        for b in -1..=1 {
            for a in -1..=1 {
                let (cx, cy) = ((x >> 4) + a, (y >> 4) + b);
                if cx >= 0 && cy >= 0 {
                    map.land.set_pt(cx, cy);
                }
            }
        }
    }
    w.objects.len() - 1
}
