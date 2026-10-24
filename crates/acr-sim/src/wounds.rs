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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rules {
    /// Hits land on body regions with lasting injuries ([`crate::body::Body`]).
    pub hit_locations: bool,
    /// Men, horses and animals have bodies that take up space and push each other apart
    /// ([`crate::spacing`], `docs/body-spacing.md`).
    pub body_spacing: bool,
    /// Mortars (`ArtMrt`) with engagement zones ([`crate::mortar`], `docs/simulation.md`,
    /// "Mortar").
    pub mortars: bool,
    /// Corpse piles are obstacles: dense piles slow the men who cross them, big ones are
    /// walked around ([`crate::piles`], `docs/simulation.md`, "Corpse piles").
    pub corpse_piles: bool,
    /// Group moves send every selected formation (the original takes at most 32,
    /// 0x4ac997) and give several formations their own places around the target: lines
    /// one behind the other, columns side by side, loose units behind them
    /// ([`crate::motion::group_send_ex`], `docs/simulation.md`, "Group moves").
    pub group_move: bool,
    /// Round shot sets men on fire (#226 follow-up): with [`Rules::hit_locations`] on, a man
    /// (not a building, cannon or ship) hit by an `IADRO` round (the `X` flag of `NRES.DAT`)
    /// catches fire, then burns, panics and may burn to death ([`crate::ground_fire`]). Off
    /// (the default) round shot lights no man under hit locations; without hit locations
    /// the original's flag is set as before either way.
    pub cannon_fire: bool,
    /// Cannonballs break trees and knock stones aside (with [`Rules::hit_locations`] on):
    /// [`crate::ball_terrain`], `docs/simulation.md`, "Balls against trees and stones".
    pub ball_terrain: bool,
}

impl std::hash::Hash for Rules {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Later rules count only when on, so the hashes of games with the earlier ones stay.
        (self.hit_locations, self.body_spacing).hash(state);
        if self.mortars {
            self.mortars.hash(state);
        }
        if self.corpse_piles {
            (3u8, self.corpse_piles).hash(state);
        }
        if self.group_move {
            (4u8, self.group_move).hash(state);
        }
        if self.cannon_fire {
            (5u8, self.cannon_fire).hash(state);
        }
        if self.ball_terrain {
            (6u8, self.ball_terrain).hash(state);
        }
    }
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
        let mut loss = u.body.bleeding();
        // A vital hit (`crate::melee`): the life left runs out with the seconds left.
        let (timed, life) = (u.body.bleed_out, u.life);
        // Shell fragment wounds (`crate::shrapnel`) bleed for their seconds, then clot.
        if w.units[i].body.seep_s > 0 {
            let b = &mut w.units[i].body;
            b.seep_s -= 1;
            if b.seep_s == 0 {
                b.seep = 0;
            }
        }
        if timed > 0 {
            w.units[i].body.bleed_out -= 1;
            loss = if timed == 1 { i32::MAX } else { loss.saturating_add((life / i32::from(timed)).max(1)) };
        }
        if loss == 0 {
            continue;
        }
        if w.units[i].life > loss {
            w.units[i].life -= loss;
        } else {
            w.units[i].life = 0;
            let record = crate::combat::DeathRecord::of(crate::combat::DeathCause::BledOut, None);
            crate::combat::record_death(w, i, record);
            crate::combat::die(w, i);
        }
    }
}
