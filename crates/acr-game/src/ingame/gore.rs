//! Blood, wounds, dirt and burns on 3D corpses by the cause of death (#183,
//! docs/rendering-3d.md). Render only: the cause comes from `World::deaths`, the timing from
//! the corpse's death counter, nothing is written back to the simulation.

use acr_render::model::ramp;
use acr_render::model_renderer::{BloodPool, Wounds};
use acr_render::view3d::{self, PIXELS_PER_METRE};
use acr_sim::combat::{DeathCause, DeathRecord};
use acr_sim::world::Unit;

use crate::ui::settings::Gore;

/// How a cause of death marks the body and the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CauseLook {
    /// Wounds on the body and their radius (metres).
    pub wounds: u8,
    pub radius: f32,
    /// Blood on the body at most (0..1).
    pub blood: f32,
    /// Radius of the pool on the ground (metres), 0 for none, and how far it runs out
    /// away from the blow.
    pub pool: f32,
    pub stretch: f32,
    /// Charring (0..1).
    pub burn: f32,
}

pub fn look(cause: DeathCause) -> CauseLook {
    let l = |wounds, radius, blood, pool, stretch, burn| CauseLook { wounds, radius, blood, pool, stretch, burn };
    match cause {
        // Several cuts and thrusts, much blood.
        DeathCause::Melee => l(3, 0.10, 1.0, 0.85, 0.2, 0.0),
        // Entry and exit wound.
        DeathCause::Gunshot => l(2, 0.06, 0.85, 0.55, 0.1, 0.0),
        DeathCause::Arrow => l(1, 0.06, 0.7, 0.45, 0.0, 0.0),
        DeathCause::Thrown => l(1, 0.09, 0.9, 0.6, 0.1, 0.0),
        // One big wound, the body thrown back over a trail of blood.
        DeathCause::Cannonball => l(1, 0.28, 1.0, 1.1, 1.2, 0.0),
        DeathCause::Grapeshot => l(6, 0.06, 1.0, 0.9, 0.4, 0.0),
        DeathCause::Explosion => l(3, 0.12, 0.8, 0.7, 0.3, 0.6),
        DeathCause::Burned => l(0, 0.0, 0.0, 0.0, 0.0, 1.0),
        DeathCause::Hunger => l(0, 0.0, 0.0, 0.0, 0.0, 0.0),
        // Lost limbs: soaked in blood, a large pool where he lay still.
        DeathCause::BledOut => l(2, 0.12, 1.0, 1.0, 0.1, 0.0),
        DeathCause::Unknown => l(1, 0.07, 0.6, 0.5, 0.0, 0.0),
    }
}

/// Charring forced on units by the driver's `burn` (#207), 0..1 per unit id, over the
/// burn amount of `fire_view` ([`Wounds::burn`] takes the largest of them and the cause's).
/// Render only.
#[derive(Debug, Default)]
pub struct Burns {
    units: std::collections::HashMap<u32, f32>,
}

impl Burns {
    /// Sets a unit's charring (the driver's `burn`; 0 forgets it).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn set(&mut self, id: u32, amount: f32) {
        if amount > 0.0 {
            self.units.insert(id, amount.min(1.0));
        } else {
            self.units.remove(&id);
        }
    }

    /// A unit's charring, 0..1.
    pub fn amount(&self, id: u32) -> f32 {
        self.units.get(&id).copied().unwrap_or(0.0)
    }

    pub fn retain(&mut self, keep: impl Fn(u32) -> bool) {
        self.units.retain(|&id, _| keep(id));
    }

    pub fn clear(&mut self) {
        self.units.clear();
    }
}

/// The parts of a unit its corpse's look depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub id: u32,
    /// World pixels.
    pub pos: (i32, i32),
    /// `RealDir`.
    pub dir: u8,
    /// Death counter (`Sdoxlo`), 0 while living.
    pub death: u16,
}

impl From<&Unit> for Body {
    fn from(u: &Unit) -> Self {
        Self { id: u.id.0, pos: u.pos.pixels(), dir: u.dir, death: u.death }
    }
}

/// The wounds and the blood pool of a corpse at ground point `ground` (world), removed at
/// death counter `removed_at`.
pub fn corpse(
    u: Body,
    record: DeathRecord,
    gore: Gore,
    removed_at: u16,
    ground: [f32; 3],
) -> (Wounds, Option<BloodPool>) {
    if u.death == 0 || gore == Gore::Off {
        return (Wounds::default(), None);
    }
    let mut look = look(record.cause);
    if gore == Gore::Reduced {
        look.wounds = look.wounds.min(1);
        look.blood *= 0.5;
        look.pool *= 0.5;
        look.stretch *= 0.5;
    }
    let d = f32::from(u.death);
    let (x, y) = u.pos;
    let yaw = view3d::facing_yaw(u.dir);
    let (s, c) = yaw.sin_cos();
    // The blow from the attacker, in world pixels and in the model's frame (model x → world
    // (c, −s), model z → world (s, c), see `view3d::model_matrix`).
    let blow = record.from.map(|(fx, fy)| [(fx - x) as f32, (fy - y) as f32]).filter(|v| v[0] != 0.0 || v[1] != 0.0);
    let from = blow.map_or([0.0; 2], |v| {
        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
        [(v[0] * c - v[1] * s) / len, (v[0] * s + v[1] * c) / len]
    });
    let seed = (u.id as f32 * 0.618_034).fract() * 97.0;
    let wounds = Wounds {
        from,
        blood: look.blood * ramp(d, 0.0, 50.0),
        dirt: 0.8 * ramp(d, 60.0, 900.0),
        burn: look.burn * ramp(d, 0.0, 30.0),
        count: look.wounds,
        radius: look.radius,
        seed,
        ..Default::default()
    };
    if look.pool <= 0.0 {
        return (wounds, None);
    }
    // Under the chest of a body lying face down towards its facing, running out away from
    // the blow.
    let chest = 0.55 * PIXELS_PER_METRE;
    let along = blow.map_or([0.0; 2], |v| {
        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
        [-v[0] / len, -v[1] / len]
    });
    let pool = BloodPool {
        centre: [ground[0] + s * chest, ground[1] + c * chest, ground[2]],
        radius: look.pool * PIXELS_PER_METRE * (0.35 + 0.65 * ramp(d, 5.0, 140.0)),
        along,
        stretch: look.stretch * ramp(d, 5.0, 140.0),
        age: ramp(d, 120.0, 900.0),
        fade: 1.0 - ramp(d, f32::from(removed_at) * 0.85, f32::from(removed_at)),
        seed,
        ..Default::default()
    };
    (wounds, Some(pool))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpse_at(death: u16, dir: u8) -> Body {
        Body { id: 7, pos: (1000, 1000), dir, death }
    }

    #[test]
    fn forced_burns() {
        let mut b = Burns::default();
        b.set(4, 0.5);
        b.set(3, 2.0);
        assert_eq!((b.amount(3), b.amount(4)), (1.0, 0.5));
        b.retain(|id| id == 4);
        b.set(4, 0.0);
        assert_eq!((b.amount(3), b.amount(4)), (0.0, 0.0));
    }

    #[test]
    fn the_living_and_gore_off_show_nothing() {
        let record = DeathRecord { cause: DeathCause::Melee, from: Some((1100, 1000)), tick: 0, ..Default::default() };
        assert_eq!(corpse(corpse_at(0, 64), record, Gore::Full, 1200, [0.0; 3]), (Wounds::default(), None));
        assert_eq!(corpse(corpse_at(300, 64), record, Gore::Off, 1200, [0.0; 3]), (Wounds::default(), None));
    }

    #[test]
    fn the_blow_is_turned_into_the_models_frame() {
        // Facing south (64) the model's +z is world +y; a blow from the east is model +x.
        let record =
            DeathRecord { cause: DeathCause::Gunshot, from: Some((1100, 1000)), tick: 0, ..Default::default() };
        let (w, pool) = corpse(corpse_at(300, 64), record, Gore::Full, 1200, [1000.0, 1000.0, 0.0]);
        assert!((w.from[0] - 1.0).abs() < 1e-5 && w.from[1].abs() < 1e-5, "{:?}", w.from);
        assert_eq!(w.count, 2);
        // The pool lies south of the ground point (under the chest) and runs west.
        let pool = pool.expect("a pool");
        assert!(pool.centre[1] > 1000.0 && (pool.centre[0] - 1000.0).abs() < 1e-3, "{:?}", pool.centre);
        assert!((pool.along[0] + 1.0).abs() < 1e-5);
        // Facing east (0) a blow from the south is the model's right side (−x).
        let record =
            DeathRecord { cause: DeathCause::Gunshot, from: Some((1000, 1100)), tick: 0, ..Default::default() };
        let (w, _) = corpse(corpse_at(300, 0), record, Gore::Full, 1200, [0.0; 3]);
        assert!((w.from[0] + 1.0).abs() < 1e-5 && w.from[1].abs() < 1e-5, "{:?}", w.from);
    }

    #[test]
    fn blood_spreads_dries_and_fades() {
        let record = DeathRecord { cause: DeathCause::Melee, from: None, tick: 0, ..Default::default() };
        let at = |d| corpse(corpse_at(d, 64), record, Gore::Full, 1200, [0.0; 3]);
        let (early, p0) = at(2);
        let (late, p1) = at(600);
        let (p0, p1) = (p0.unwrap(), p1.unwrap());
        assert!(early.blood < late.blood && p0.radius < p1.radius && p0.age < p1.age);
        assert_eq!(at(1200).1.unwrap().fade, 0.0);
        // Hunger and fire leave no pool; fire chars.
        let hunger = corpse(
            corpse_at(300, 64),
            DeathRecord { cause: DeathCause::Hunger, from: None, tick: 0, ..Default::default() },
            Gore::Full,
            1200,
            [0.0; 3],
        );
        assert!(hunger.1.is_none() && hunger.0.blood == 0.0);
        let burned = corpse(
            corpse_at(300, 64),
            DeathRecord { cause: DeathCause::Burned, from: None, tick: 0, ..Default::default() },
            Gore::Full,
            1200,
            [0.0; 3],
        );
        assert!(burned.1.is_none() && burned.0.burn == 1.0);
        // Reduced: one wound, half the blood.
        let (r, rp) = corpse(corpse_at(600, 64), record, Gore::Reduced, 1200, [0.0; 3]);
        assert_eq!(r.count, 1);
        assert!((r.blood - late.blood / 2.0).abs() < 1e-5 && rp.unwrap().radius < p1.radius);
    }
}
