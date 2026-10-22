//! Hit locations (#225): an optional rule that is not in the original game. With
//! [`Rules::hit_locations`] on, a hit on a soldier or a horseman lands on a body region: a
//! wounded leg slows the unit, a wounded arm stops it from using its firearm or bow. Off (the
//! default) the game plays exactly like the original: no random number is drawn and the
//! state hash stays as it was. See `docs/simulation.md` ("Hit locations").

use std::hash::{Hash, Hasher};

use crate::World;
use crate::catalog::MotionStyle;

/// Optional rules chosen when a game starts (recorded in the game setup and savegames).
/// The default is the original game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rules {
    /// Hits land on body regions with lasting wounds ([`Wounds`]).
    pub hit_locations: bool,
}

/// The body region a hit landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitRegion {
    Torso,
    Legs,
    Arms,
    Head,
}

/// Weights of the regions out of 100, in the order they are tested: torso 45, legs 25,
/// arms 20, head 10.
pub const REGION_WEIGHTS: [(HitRegion, i32); 4] =
    [(HitRegion::Torso, 45), (HitRegion::Legs, 25), (HitRegion::Arms, 20), (HitRegion::Head, 10)];

/// Walking speed in percent after 0, 1, 2 and more leg wounds (×0.6 per wound, at least 30).
pub const LEG_SPEED: [i32; 4] = [100, 60, 36, 30];

/// Lasting wounds of a unit (only with [`Rules::hit_locations`]). Hashed with the unit, but
/// an unwounded unit hashes to nothing, so games without the rule keep their hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wounds {
    /// Leg wounds (each slows, see [`LEG_SPEED`]).
    pub legs: u8,
    /// Arm wounds: one is enough to stop ranged attacks.
    pub arms: u8,
}

impl Hash for Wounds {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if *self != Self::default() {
            (self.legs, self.arms).hash(state);
        }
    }
}

impl Wounds {
    /// Walking rate scaled by the leg wounds (`rate` in 16ths, at least 1).
    pub fn walk_rate(&self, rate: i32) -> i32 {
        if self.legs == 0 {
            return rate;
        }
        (rate * LEG_SPEED[usize::from(self.legs).min(3)] / 100).max(1)
    }

    /// The unit can no longer fire (its arms are wounded).
    pub fn disarmed(&self) -> bool {
        self.arms > 0
    }
}

/// Picks the region of a hit on `victim` when the rule is on and the victim has a body
/// (not a building, a cannon or a ship): one `rando()`, drawn by `MakeDamage` after the
/// damage is known and before the hit is recorded.
pub fn pick_region(w: &mut World, victim: usize) -> Option<HitRegion> {
    if !w.rules.hit_locations {
        return None;
    }
    let u = &w.units[victim];
    let t = w.catalog.get(u.kind);
    if u.building || matches!(t.motion_style, MotionStyle::ComplexRotate | MotionStyle::Ships | MotionStyle::NewShips) {
        return None;
    }
    let mut r = (w.random.rando() * 100) >> 15;
    for (region, weight) in REGION_WEIGHTS {
        if r < weight {
            return Some(region);
        }
        r -= weight;
    }
    Some(HitRegion::Torso)
}

/// Leaves the wound of a hit the victim survived.
pub fn wound(w: &mut World, victim: usize, region: HitRegion) {
    let wounds = &mut w.units[victim].wounds;
    match region {
        HitRegion::Legs => wounds.legs = wounds.legs.saturating_add(1),
        HitRegion::Arms => wounds.arms = wounds.arms.saturating_add(1),
        HitRegion::Torso | HitRegion::Head => {}
    }
}
