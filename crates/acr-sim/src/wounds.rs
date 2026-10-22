//! Hit locations (#225): an optional rule that is not in the original game. With
//! [`Rules::hit_locations`] on, a hit on a soldier or a horseman lands on a body region: a
//! wounded leg slows the unit, a wounded arm stops it from using its firearm or bow. Off (the
//! default) the game plays exactly like the original: no random number is drawn and the
//! state hash stays as it was. See `docs/simulation.md` ("Hit locations").
//!
//! Heavy blows take limbs off (#254): a soldier without a leg crawls, every lost limb
//! bleeds, and a bleeding soldier loses life every second until he bleeds out.

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

/// Crawling speed in percent with one and with both legs lost.
pub const CRAWL_SPEED: [i32; 2] = [15, 8];

/// Life lost per bleeding interval for each lost leg and each lost arm.
pub const BLEED_LEG: u8 = 4;
pub const BLEED_ARM: u8 = 2;
/// Ticks between two losses of blood (one second).
pub const BLEED_EVERY: u64 = 10;

/// Chance in percent that a limb hit takes the limb off, by weapon.
pub fn sever_chance(cause: crate::combat::DeathCause) -> i32 {
    use crate::combat::DeathCause as C;
    match cause {
        C::Cannonball | C::Grapeshot => 100,
        C::Explosion => 50,
        C::Melee => 25,
        C::Thrown => 15,
        C::Gunshot => 10,
        _ => 0,
    }
}

/// Lasting wounds of a unit (only with [`Rules::hit_locations`]). Hashed with the unit, but
/// an unwounded unit hashes to nothing, so games without the rule keep their hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wounds {
    /// Leg wounds (each slows, see [`LEG_SPEED`]).
    pub legs: u8,
    /// Arm wounds: one is enough to stop ranged attacks.
    pub arms: u8,
    /// Legs and arms lost (0..=2 each): a soldier without a leg crawls.
    pub legs_lost: u8,
    pub arms_lost: u8,
    /// Seconds left burning after standing in a shell fire (`crate::ground_fire`).
    pub burn: u8,
}

impl Hash for Wounds {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if *self != Self::default() {
            (self.legs, self.arms).hash(state);
            // Only games where a limb was lost hash these (older wounded games keep theirs).
            if self.legs_lost != 0 || self.arms_lost != 0 {
                (self.legs_lost, self.arms_lost).hash(state);
            }
            // Likewise the burning from shell fires.
            if self.burn != 0 {
                self.burn.hash(state);
            }
        }
    }
}

impl Wounds {
    /// Walking rate scaled by the leg wounds (`rate` in 16ths, at least 1).
    pub fn walk_rate(&self, rate: i32) -> i32 {
        if self.legs_lost > 0 {
            return (rate * CRAWL_SPEED[usize::from(self.legs_lost.min(2)) - 1] / 100).max(1);
        }
        if self.legs == 0 {
            return rate;
        }
        (rate * LEG_SPEED[usize::from(self.legs).min(3)] / 100).max(1)
    }

    /// The unit can no longer fire (its arms are wounded or gone).
    pub fn disarmed(&self) -> bool {
        self.arms > 0 || self.arms_lost > 0
    }

    /// The unit lost a leg and crawls.
    pub fn crawling(&self) -> bool {
        self.legs_lost > 0
    }

    /// Life lost per bleeding interval (0: not bleeding).
    pub fn bleeding(&self) -> i32 {
        i32::from(self.legs_lost) * i32::from(BLEED_LEG) + i32::from(self.arms_lost) * i32::from(BLEED_ARM)
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

/// Leaves the wound of a hit the victim survived: a limb hit by a heavy enough blow
/// ([`sever_chance`], one `rando()` for limb hits) takes the limb off, else it wounds.
pub fn wound(w: &mut World, victim: usize, region: HitRegion, cause: crate::combat::DeathCause) {
    let sever = match region {
        HitRegion::Legs | HitRegion::Arms => {
            let chance = sever_chance(cause);
            chance > 0 && ((w.random.rando() * 100) >> 15) < chance
        }
        HitRegion::Torso | HitRegion::Head => false,
    };
    let wounds = &mut w.units[victim].wounds;
    match (region, sever) {
        (HitRegion::Legs, true) if wounds.legs_lost < 2 => wounds.legs_lost += 1,
        (HitRegion::Arms, true) if wounds.arms_lost < 2 => wounds.arms_lost += 1,
        (HitRegion::Legs, _) => wounds.legs = wounds.legs.saturating_add(1),
        (HitRegion::Arms, _) => wounds.arms = wounds.arms.saturating_add(1),
        (HitRegion::Torso | HitRegion::Head, _) => {}
    }
}

/// Once a second the bleeding lose life; a soldier at no life bleeds out and dies
/// (cause [`crate::combat::DeathCause::BledOut`]). Only with [`Rules::hit_locations`].
pub fn bleed(w: &mut World) {
    if !w.rules.hit_locations || !w.tick.0.is_multiple_of(BLEED_EVERY) {
        return;
    }
    for i in 0..w.units.len() {
        let u = &w.units[i];
        if !u.living() {
            continue;
        }
        let loss = u.wounds.bleeding();
        if loss == 0 {
            continue;
        }
        if u.life > loss {
            w.units[i].life -= loss;
        } else {
            w.units[i].life = 0;
            let record = crate::combat::DeathRecord { cause: crate::combat::DeathCause::BledOut, from: None, tick: 0 };
            crate::combat::record_death(w, i, record);
            crate::combat::die(w, i);
        }
    }
}
