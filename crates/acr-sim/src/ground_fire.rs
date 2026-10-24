//! Shell fires (#256): with [`crate::Rules::hit_locations`] on, an explosive shell (weapon
//! kind `IADROMOR` or `VAFLIA`, [`crate::combat::DeathCause::Explosion`]) bursting on dry
//! ground leaves a small fire there. Soldiers standing in it catch fire: the original's
//! burning flag (`Unit::burning`, also set by `FIRE` weapons) goes on and they burn for a
//! few seconds, losing life every second, and can burn to death
//! ([`crate::combat::DeathCause::Burned`]). Not in the original game: off, nothing here runs,
//! no random number is drawn and the state hash is unchanged. See `docs/simulation.md`
//! ("Hit locations").

use crate::World;
use crate::catalog::MotionStyle;

/// Radius of a shell fire (px, about 1.3 m at 30 px per metre).
pub const RADIUS: i32 = 40;
/// How long a shell fire burns (ticks, 30 s at 10 ticks per second).
pub const BURN_TICKS: u64 = 300;
/// Ticks between two checks of the fires and two losses of life (one second).
pub const EVERY: u64 = 10;
/// Seconds a soldier keeps burning after he last stood in a fire.
pub const BURN_SECONDS: u8 = 5;
/// Life a burning soldier loses every second.
pub const BURN_LOSS: i32 = 8;
/// Seconds a burning man runs in panic before the flames go out or he falls.
pub const FLEE_SECONDS: u8 = 4;
/// Chance (out of 32768, one `rando()`) that his rolling and running put the flames out at
/// the end of his panic; otherwise he falls and burns to death.
pub const OUT_CHANCE: i32 = 16384;

/// One fire on the ground (world pixels). Its age is `tick - start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroundFire {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
    /// The tick it started.
    pub start: u64,
}

impl GroundFire {
    /// Ticks since it started.
    pub fn age(&self, tick: u64) -> u64 {
        tick.saturating_sub(self.start)
    }
}

/// The burning shell fires, oldest first (`World::ground_fires`). Part of the state hash
/// only while there are some.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct GroundFires {
    pub list: Vec<GroundFire>,
}

/// A unit with a body that can burn (not a building, a cannon or a ship).
fn has_body(w: &World, i: usize) -> bool {
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    !u.building && !matches!(t.motion_style, MotionStyle::ComplexRotate | MotionStyle::Ships | MotionStyle::NewShips)
}

/// A round of `sender`'s attack `att` burst on the ground at `(x, y)` (pixels): a shell
/// leaves a fire unless the ground is water or a fire already burns there (a shell's
/// fragments burst at the same point).
pub fn shell_burst(w: &mut World, sender: Option<usize>, att: u8, (x, y): (i32, i32), water: bool) {
    if !w.rules.hit_locations || water {
        return;
    }
    let Some(s) = sender else { return };
    if crate::combat::cause_of_attack(w, s, att) != crate::combat::DeathCause::Explosion {
        return;
    }
    if w.ground_fires.list.iter().any(|f| crate::trig::norma(f.x - x, f.y - y) < f.radius) {
        return;
    }
    w.ground_fires.list.push(GroundFire { x, y, radius: RADIUS, start: w.tick.0 });
}

/// Once a tick (after the bleeding): fires burn out after [`BURN_TICKS`]; once a second
/// every soldier inside a fire catches fire for [`BURN_SECONDS`], and every soldier still
/// burning from one loses [`BURN_LOSS`] life (through `MakeDamage` without a sender, so a
/// death is recorded as burned) and goes out when his time is up. Only with the rule.
pub fn tick(w: &mut World) {
    if !w.rules.hit_locations {
        return;
    }
    let now = w.tick.0;
    w.ground_fires.list.retain(|f| f.age(now) < BURN_TICKS);
    if !now.is_multiple_of(EVERY) {
        return;
    }
    for i in 0..w.units.len() {
        let u = &w.units[i];
        if !u.living() || !has_body(w, i) {
            continue;
        }
        let (x, y) = u.pos.pixels();
        if w.ground_fires.list.iter().any(|f| crate::trig::norma(f.x - x, f.y - y) < f.radius) {
            let u = &mut w.units[i];
            if u.body.burn == 0 {
                // Already burning from a fire weapon: that fire is the original's and stays.
                u.body.burn_kept = u.burning;
            }
            u.burning = true;
            u.body.burn = BURN_SECONDS;
        }
        if w.units[i].body.burn == 0 {
            continue;
        }
        crate::combat::make_damage(w, i, BURN_LOSS, None, 0xFF);
        let u = &mut w.units[i];
        if u.living() {
            u.body.burn -= 1;
            if u.body.burn == 0 {
                u.burning = u.body.burn_kept;
                u.body.burn_kept = false;
            }
        }
    }
    panic(w);
}

/// Burning men panic (once a second, after the burn): a living soldier on fire (a shell
/// fire or an `X`/`F` weapon) who is not running yet breaks from his formation and flees
/// through the original's panic order (`StartPanic` 0x600e20, `PanicLink` 0x600720: legs of
/// 128 px with `UnlimitedMotion`, taking no orders; its second leg frightens his comrades
/// through `Fear` 0x601960, fear type 0), but for [`FLEE_SECONDS`], away from the fires and
/// his comrades (`morale::panic_link`). Then one `rando()`: below [`OUT_CHANCE`] the flames
/// are out (even the original's never-ending flag) and he stops; otherwise he falls, burned
/// to death. Not in the original: there only kills raise fear (`MakeDamage` 0x4e8f92,
/// 0x4e9c4d). Only with the rule.
fn panic(w: &mut World) {
    for i in 0..w.units.len() {
        let u = &w.units[i];
        if !u.living() || !has_body(w, i) {
            continue;
        }
        if u.body.flee == 0 {
            if u.burning && !u.panicking() && !u.unlimited {
                crate::morale::start_panic(w, i);
                if matches!(w.units[i].orders.last(), Some(crate::world::Order::Panic { .. })) {
                    w.units[i].body.flee = FLEE_SECONDS;
                }
            }
            continue;
        }
        let u = &mut w.units[i];
        u.body.flee -= 1;
        if u.burning && u.body.flee > 0 {
            continue;
        }
        // The flight is over (or the shell fire's flames already went out).
        if u.burning && w.random.rando() >= OUT_CHANCE {
            let life = w.units[i].life;
            crate::combat::make_damage(w, i, life, None, 0xFF);
            if !w.units[i].living() {
                continue;
            }
        }
        let World { units, occupancy, .. } = w;
        let u = &mut units[i];
        (u.burning, u.body.burn, u.body.burn_kept, u.body.flee) = (false, 0, false, 0);
        u.orders.clear();
        u.path.clear();
        u.dest = None;
        u.prio = 0;
        crate::economy::settle(u, occupancy);
    }
}
