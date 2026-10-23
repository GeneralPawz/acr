//! Shell fragments (`docs/damage-model.md`, "Shell fragments"): with
//! [`crate::Rules::hit_locations`] on, an explosive shell (weapon kind `IADROMOR` or
//! `VAFLIA`, [`DeathCause::Explosion`]) bursting on the ground throws [`COUNT`] fragments in
//! random directions on top of the original's round damage. Each has a random mass, mostly
//! small; it flies a random share of [`REACH`] and strikes the first man or animal in its
//! way. The hit goes through the hit-location model with the fragment's energy (by mass,
//! falling with the distance): small fragments wound and bleed for a while
//! ([`crate::body::Body::seep`]), big ones break, sever or kill.
//!
//! Not in the original game: off, nothing here runs, no random number is drawn and the state
//! hash is unchanged. The fragments' paths are kept for the renderer
//! ([`crate::World::fragments`], not hashed, not saved).

use crate::World;
use crate::body::{FractureKind, Outcome};
use crate::combat::{DeathCause, HIT_CAP, HIT_KEEP};
use crate::trig;

/// Fragments a shell throws: at least, at most.
pub const COUNT: (i32, i32) = (14, 26);
/// How far the fragments fly at most (world pixels, about 7 m at 30 px per metre); at least
/// twice the weapon's blast radius.
pub const REACH: i32 = 210;
/// A fragment strikes a body it passes this close to (world pixels from its centre line).
pub const BODY: i32 = 8;
/// Bursts of one tick within this distance (pixels) are one shell (its children explode at
/// the same point).
const SAME_BURST: i32 = 40;

/// One fragment's flight, for the renderer: from the burst to where it struck or fell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fragment {
    pub tick: u64,
    /// The burst (world pixels).
    pub from: (i32, i32),
    /// Where it struck or fell (world pixels).
    pub to: (i32, i32),
    /// 1..=100: a splinter to a large piece of the casing.
    pub mass: u8,
    /// The unit it struck, if any.
    pub victim: Option<u32>,
}

/// Life lost per second and seconds of bleeding of a fragment wound by its outcome: a light
/// wound seeps for a few seconds, a deep one or a break longer, a stump or an open belly (which
/// bleed by [`crate::body::Body::bleeding`] anyway) a little more.
pub fn seep_of(outcome: Outcome) -> (u8, u8) {
    match outcome {
        Outcome::Wound => (1, 6),
        Outcome::DeepWound | Outcome::Fracture(FractureKind::Closed | FractureKind::Dislocated) => (2, 20),
        Outcome::Fracture(FractureKind::Open) | Outcome::Off | Outcome::BellyOpen => (1, 40),
        Outcome::Kill | Outcome::Torn(_) => (0, 0),
    }
}

/// The energy (0..=100) of a fragment of `mass` that struck `along` pixels from the burst
/// on a flight of `range` pixels: by its mass, down to half at the end of its flight.
pub fn energy(mass: u8, along: i32, range: i32) -> u8 {
    let base = 25 + i32::from(mass) * 3 / 4;
    (base * (2 * range - along.clamp(0, range)) / (2 * range).max(1)).clamp(5, 100) as u8
}

/// The life a fragment takes: 1 for a splinter, up to about 50 for a large piece close by.
pub fn damage(mass: u8, energy: u8) -> i32 {
    1 + i32::from(mass) * i32::from(energy) / 200
}

/// A round of `sender`'s attack `att` burst on the ground at `(x, y)` (pixels) with the
/// weapon's blast `radius`: a shell throws its fragments (once per shell and tick).
pub fn burst(w: &mut World, sender: Option<usize>, att: u8, (x, y): (i32, i32), radius: i32) {
    if !w.rules.hit_locations {
        return;
    }
    let Some(s) = sender else { return };
    if crate::combat::cause_of_attack(w, s, att) != DeathCause::Explosion {
        return;
    }
    let now = w.tick.0;
    if w.fragments
        .iter()
        .rev()
        .take_while(|f| f.tick == now)
        .any(|f| trig::norma(f.from.0 - x, f.from.1 - y) < SAME_BURST)
    {
        return;
    }
    let reach = REACH.max(radius * 2);
    // Everyone with a body within reach, by unit index (deterministic order).
    let (cx, cy) = (x >> 7, y >> 7);
    let n = reach / 128 + 1;
    let mut near: Vec<(usize, i32, i32)> = Vec::new();
    for ay in -n..=n {
        for ax in -n..=n {
            for k in w.buckets.all(cx + ax, cy + ay) {
                let k = k as usize;
                if k == s || !w.units[k].living() || !crate::wounds::has_regions(w, k) {
                    continue;
                }
                let (ux, uy) = w.units[k].pos.pixels();
                if trig::norma(ux - x, uy - y) <= reach {
                    near.push((k, ux - x, uy - y));
                }
            }
        }
    }
    near.sort_unstable_by_key(|n| n.0);
    near.dedup_by_key(|n| n.0);
    let count = COUNT.0 + (((COUNT.1 - COUNT.0 + 1) * w.random.rando()) >> 15);
    for _ in 0..count {
        let dir = ((w.random.rando() << 8) >> 15) as u8;
        // Mostly splinters, now and then a large piece.
        let m = (w.random.rando() * 100) >> 15;
        let mass = (m * m / 100 + 1) as u8;
        let range = reach * (40 + ((w.random.rando() * 61) >> 15)) / 100;
        let (dx, dy) = (trig::cos(dir), trig::sin(dir));
        // The first body in its way.
        let mut first: Option<(usize, i32)> = None;
        for &(k, vx, vy) in &near {
            let along = (vx * dx + vy * dy) >> 8;
            if along <= 0 || along > range || first.is_some_and(|f| f.1 <= along) {
                continue;
            }
            if ((vx * dy - vy * dx) >> 8).abs() < BODY && w.units[k].living() {
                first = Some((k, along));
            }
        }
        let (to, victim) = match first {
            Some((k, along)) => {
                strike(w, k, s, att, mass, energy(mass, along, range), (dx, dy));
                (w.units[k].pos.pixels(), Some(k as u32))
            }
            None => ((x + ((dx * range) >> 8), y + ((dy * range) >> 8)), None),
        };
        if w.fragments.len() >= HIT_CAP {
            w.fragments.pop_front();
        }
        w.fragments.push_back(Fragment { tick: now, from: (x, y), to, mass, victim });
    }
}

/// A fragment striking unit `k`: the hit through `MakeDamage` with the fragment's energy and
/// direction, then its wound bleeds ([`seep_of`]).
fn strike(w: &mut World, k: usize, s: usize, att: u8, mass: u8, energy: u8, dir: (i32, i32)) {
    let (outer_energy, outer_dir) = (w.hit_energy, w.hit_dir);
    w.hit_energy = Some(energy);
    w.hit_dir = Some(dir);
    let before = w.hits.len();
    crate::combat::make_damage(w, k, damage(mass, energy), Some(s), att);
    (w.hit_energy, w.hit_dir) = (outer_energy, outer_dir);
    if !w.units[k].living() {
        return;
    }
    let injury =
        w.hits.iter().skip(before.min(w.hits.len())).rev().find(|h| h.victim == k as u32).and_then(|h| h.injury);
    // `hits` is capped: a full list drops from the front, so look at the newest one too.
    let injury =
        injury.or_else(|| w.hits.back().filter(|h| h.victim == k as u32 && h.tick == w.tick.0).and_then(|h| h.injury));
    if let Some(i) = injury {
        let (seep, secs) = seep_of(i.outcome);
        let b = &mut w.units[k].body;
        b.seep = b.seep.saturating_add(seep).min(8);
        b.seep_s = b.seep_s.max(secs);
    }
}

/// The index of `unit`'s first shell attack (`IADROMOR`, `VAFLIA`), if it has one (the test
/// driver's shell bursts).
pub fn shell_attack(w: &World, unit: usize) -> Option<u8> {
    let u = w.units.get(unit)?;
    let a = w.adv(u.owner, u.kind);
    a.attacks.iter().position(|a| DeathCause::of_weapon(&a.kind) == DeathCause::Explosion).map(|i| i as u8)
}

/// Drops the fragments older than [`HIT_KEEP`] ticks (with the hits).
pub fn forget(w: &mut World) {
    while w.fragments.front().is_some_and(|f| f.tick + HIT_KEEP < w.tick.0) {
        w.fragments.pop_front();
    }
}
