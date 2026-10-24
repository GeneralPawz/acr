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
    /// A shell's casing, earth thrown up by a ball, or a man's bones.
    pub kind: Kind,
    /// Thrown by a mortar's shell ([`crate::mortar::fired_by`]): the renderer draws fewer.
    pub mortar: bool,
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

/// What throws the fragments: a shell's casing, the earth and stones a cannonball throws up
/// where it grazes the ground, or the bones of a man a ball tore apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Casing,
    Earth,
    Bone,
}

/// How a burst of fragments is thrown ([`throw`]).
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub kind: Kind,
    /// Pieces: at least, at most.
    pub count: (i32, i32),
    /// How far they fly at most (world pixels).
    pub reach: i32,
    /// The heaviest piece (1..=100).
    pub mass: i32,
    /// Their punch, in percent of a casing fragment's of the same mass.
    pub punch: i32,
    /// Thrown within this many 1/256 turns either side of the direction (128: all around).
    pub cone: i32,
}

/// A shell's casing ([`COUNT`], [`REACH`]).
pub const SHELL: Profile = Profile { kind: Kind::Casing, count: COUNT, reach: REACH, mass: 100, punch: 100, cone: 128 };
/// A cannonball grazing or bouncing on the ground: a few clods, stones and splinters, light
/// and short (about 3.5 m), thrown mostly forwards along the ball's flight.
pub const GRAZE: Profile = Profile { kind: Kind::Earth, count: (4, 9), reach: 105, mass: 30, punch: 70, cone: 80 };
/// A man pulverized or torn by a ball: a few of his bones fly on with it, forwards and to
/// the sides, about 2.5 m.
pub const BONES: Profile = Profile { kind: Kind::Bone, count: (2, 5), reach: 75, mass: 25, punch: 90, cone: 56 };

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
    let profile = Profile { reach: REACH.max(radius * 2), ..SHELL };
    throw(w, s, att, (x, y), 0, &profile, None);
}

/// A cannonball of `sender`'s attack `att` striking the ground at `(x, y)` (pixels) on its
/// flight `dir` (world pixels): the earth it throws up ([`GRAZE`], once per ball and tick).
pub fn graze(w: &mut World, sender: Option<usize>, att: u8, (x, y): (i32, i32), dir: (i32, i32)) {
    if !w.rules.hit_locations {
        return;
    }
    let Some(s) = sender else { return };
    if crate::combat::cause_of_attack(w, s, att) != DeathCause::Cannonball {
        return;
    }
    throw(w, s, att, (x, y), trig::get_dir(dir.0, dir.1), &GRAZE, None);
}

/// After a ball's hit on unit `k` (the hits from index `before` on): if it pulverized or tore
/// him, his bones fly on as fragments ([`BONES`]) at the men behind and beside him.
pub fn ball_hit(w: &mut World, k: usize, before: usize, sender: Option<usize>, att: u8) {
    if !w.rules.hit_locations {
        return;
    }
    let Some(s) = sender else { return };
    let Some(h) = w.hits.iter().skip(before.min(w.hits.len())).rev().find(|h| h.victim == k as u32).copied() else {
        return;
    };
    if h.cause != DeathCause::Cannonball || !h.injury.is_some_and(|i| i.outcome.torn().is_some()) {
        return;
    }
    throw(w, s, att, h.pos, trig::get_dir(h.dir.0, h.dir.1), &BONES, Some(k));
}

/// Throws a burst of `profile`'s fragments from `(x, y)` around the direction `dir`, not at
/// `skip` (the man whose bones they are) or the sender (once per burst point and tick).
fn throw(w: &mut World, s: usize, att: u8, (x, y): (i32, i32), dir: u8, profile: &Profile, skip: Option<usize>) {
    let now = w.tick.0;
    if w.fragments
        .iter()
        .rev()
        .take_while(|f| f.tick == now)
        .any(|f| f.kind == profile.kind && trig::norma(f.from.0 - x, f.from.1 - y) < SAME_BURST)
    {
        return;
    }
    let mortar = crate::mortar::fired_by(w.units.get(s).map(|u| u.kind), &w.catalog);
    let reach = profile.reach;
    // Everyone with a body within reach, by unit index (deterministic order).
    let (cx, cy) = (x >> 7, y >> 7);
    let n = reach / 128 + 1;
    let mut near: Vec<(usize, i32, i32)> = Vec::new();
    for ay in -n..=n {
        for ax in -n..=n {
            for k in w.buckets.all(cx + ax, cy + ay) {
                let k = k as usize;
                if k == s || Some(k) == skip || !w.units[k].living() || !crate::wounds::has_regions(w, k) {
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
    let (lo, hi) = profile.count;
    let count = lo + (((hi - lo + 1) * w.random.rando()) >> 15);
    for _ in 0..count {
        let dir = if profile.cone >= 128 {
            ((w.random.rando() << 8) >> 15) as u8
        } else {
            let off = ((w.random.rando() * (2 * profile.cone + 1)) >> 15) - profile.cone;
            (i32::from(dir) + off) as u8
        };
        // Mostly splinters, now and then a large piece.
        let m = (w.random.rando() * 100) >> 15;
        let mass = (m * m / 100 * profile.mass / 100 + 1).min(100) as u8;
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
                let e = (i32::from(energy(mass, along, range)) * profile.punch / 100).clamp(5, 100) as u8;
                strike(w, k, s, att, mass, e, (dx, dy));
                (w.units[k].pos.pixels(), Some(k as u32))
            }
            None => ((x + ((dx * range) >> 8), y + ((dy * range) >> 8)), None),
        };
        if w.fragments.len() >= HIT_CAP {
            w.fragments.pop_front();
        }
        w.fragments.push_back(Fragment { tick: now, from: (x, y), to, mass, victim, kind: profile.kind, mortar });
    }
}

/// A fragment striking unit `k`: the hit through `MakeDamage` with the fragment's energy and
/// direction, then its wound bleeds ([`seep_of`]).
fn strike(w: &mut World, k: usize, s: usize, att: u8, mass: u8, energy: u8, dir: (i32, i32)) {
    let (outer_energy, outer_dir, outer_cause) = (w.hit_energy, w.hit_dir, w.hit_cause);
    w.hit_energy = Some(energy);
    w.hit_dir = Some(dir);
    // A fragment wounds as a shell's, whatever threw it (a ball's earth, a man's bones).
    w.hit_cause = Some(DeathCause::Explosion);
    let before = w.hits.len();
    crate::combat::make_damage(w, k, damage(mass, energy), Some(s), att);
    (w.hit_energy, w.hit_dir, w.hit_cause) = (outer_energy, outer_dir, outer_cause);
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
    attack_of(w, unit, DeathCause::Explosion)
}

/// The index of `unit`'s first attack of `cause` (the test driver's grazes: `Cannonball`).
pub fn attack_of(w: &World, unit: usize, cause: DeathCause) -> Option<u8> {
    let u = w.units.get(unit)?;
    let a = w.adv(u.owner, u.kind);
    a.attacks.iter().position(|a| DeathCause::of_weapon(&a.kind) == cause).map(|i| i as u8)
}

/// Drops the fragments older than [`HIT_KEEP`] ticks (with the hits).
pub fn forget(w: &mut World) {
    while w.fragments.front().is_some_and(|f| f.tick + HIT_KEEP < w.tick.0) {
        w.fragments.pop_front();
    }
}
