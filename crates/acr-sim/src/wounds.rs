//! Hit locations (#225): an optional rule that is not in the original game. With
//! [`Rules::hit_locations`] on, a hit on a soldier or a horseman lands on a body region: a
//! wounded leg slows the unit, a wounded arm stops it from using its firearm or bow. Off (the
//! default) the game plays exactly like the original: no random number is drawn and the
//! state hash stays as it was. See `docs/simulation.md` ("Hit locations").
//!
//! What a hit does to the body is the damage model (#262, [`crate::body`],
//! `docs/damage-model.md`): wounds, breaks and limbs lost at any level; a bleeding soldier
//! loses life every second until he bleeds out.

use crate::World;
use crate::catalog::MotionStyle;

/// Optional rules chosen when a game starts (recorded in the game setup and savegames).
/// The default is the original game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rules {
    /// Hits land on body regions with lasting injuries ([`crate::body::Body`]).
    pub hit_locations: bool,
    /// Men, horses and animals have bodies that take up space and push each other apart
    /// ([`crate::spacing`], `docs/body-spacing.md`).
    pub body_spacing: bool,
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

/// Ticks between two losses of blood (one second).
pub const BLEED_EVERY: u64 = 10;

/// Height above the ground (pixels) below which a rolling ball takes the legs.
pub const LOW_BALL: i32 = 20;
/// Chance in percent that a low ball's hit lands on the legs (else the torso).
pub const LOW_BALL_LEGS: i32 = 80;

/// Whether hits on `victim` land on a body region: the hit-locations rule is on and the
/// victim is no building, cannon or ship.
pub fn has_regions(w: &World, victim: usize) -> bool {
    let u = &w.units[victim];
    let t = w.catalog.get(u.kind);
    w.rules.hit_locations
        && !u.building
        && !matches!(t.motion_style, MotionStyle::ComplexRotate | MotionStyle::Ships | MotionStyle::NewShips)
}

/// Picks the region of a hit on `victim` when the rule is on and the victim has a body
/// (not a building, a cannon or a ship): one `rando()`, drawn by `MakeDamage` after the
/// damage is known and before the hit is recorded.
pub fn pick_region(w: &mut World, victim: usize) -> Option<HitRegion> {
    if !has_regions(w, victim) {
        return None;
    }
    let mut r = (w.random.rando() * 100) >> 15;
    if w.hit_height.is_some_and(|h| h < LOW_BALL) {
        // A low rolling ball (#262) takes the legs.
        return Some(if r < LOW_BALL_LEGS { HitRegion::Legs } else { HitRegion::Torso });
    }
    for (region, weight) in REGION_WEIGHTS {
        if r < weight {
            return Some(region);
        }
        r -= weight;
    }
    Some(HitRegion::Torso)
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
        let loss = u.body.bleeding();
        if loss == 0 {
            continue;
        }
        if u.life > loss {
            w.units[i].life -= loss;
        } else {
            w.units[i].life = 0;
            let record = crate::combat::DeathRecord::of(crate::combat::DeathCause::BledOut, None);
            crate::combat::record_death(w, i, record);
            crate::combat::die(w, i);
        }
    }
}
