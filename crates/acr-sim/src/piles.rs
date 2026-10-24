//! Corpse piles as obstacles (#222): an optional rule that is not in the original game. With
//! [`crate::Rules::corpse_piles`] on, the sim counts the corpses per 32 px cell (about one
//! square metre at 30 px per metre). A cell with [`ROUGH`] or more bodies is rough ground:
//! a man standing on it walks at half his rate. A cell with [`BLOCKED`] or more bodies is an
//! obstacle: it is set in a copy of the land lock map that the walkers' path search and
//! steps use, so men path around it like around a rock. A man who already stands on a
//! blocked pile walks on the plain land map, so he can climb out.
//!
//! The count follows the corpses' own life: a body counts from `Die` (0x4e77a0) until the
//! object loop (0x5118b3) removes it (`CORPSE_LIFETIME`, a carcass, a type without a lying
//! animation) or a script erases it; a body thrown by a `DETONATOR` blast counts where it
//! lands. Updates are incremental (death, removal, landing), nothing scans the units.
//!
//! Off (the default) nothing is counted, no lock map is made and no random number is drawn,
//! so the state hash stays the original one. See `docs/simulation.md` ("Corpse piles").

use std::collections::BTreeMap;

use crate::World;
use crate::fixed::Pos;
use crate::map::LockMap;

/// Pile cells are `1 << PILE_SHIFT` px wide (32 px, two lock cells).
pub const PILE_SHIFT: i32 = 5;
/// Bodies in one pile cell from which crossing men walk at half rate.
pub const ROUGH: u8 = 4;
/// Bodies in one pile cell from which it blocks the walkers' path.
pub const BLOCKED: u8 = 7;

/// The corpse count per pile cell and the cell each counted body lies in.
#[derive(Debug, Clone, Default)]
pub struct Piles {
    /// Bodies per pile cell (cells without bodies are absent).
    pub counts: BTreeMap<(i32, i32), u8>,
    /// The pile cell of each counted corpse (unit index).
    pub bodies: BTreeMap<u32, (i32, i32)>,
    /// The land lock map with the blocked piles set; made when the first pile blocks.
    /// Derived from `counts` and the map, so not hashed.
    lock: Option<LockMap>,
}

impl std::hash::Hash for Piles {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (&self.counts, &self.bodies).hash(state);
    }
}

impl Piles {
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// The bodies in the pile cell of `pos`.
    pub fn count_at(&self, pos: Pos) -> u8 {
        if self.counts.is_empty() {
            return 0;
        }
        let (x, y) = pos.pixels();
        self.counts.get(&(x >> PILE_SHIFT, y >> PILE_SHIFT)).copied().unwrap_or(0)
    }

    /// The walking rate of a man at `pos`: halved on rough ground.
    pub fn walk_rate(&self, pos: Pos, rate: i32) -> i32 {
        if self.count_at(pos) >= ROUGH { rate / 2 } else { rate }
    }

    /// The land lock map with the blocked piles, while some pile blocks or blocked.
    pub fn lock(&self) -> Option<&LockMap> {
        self.lock.as_ref()
    }
}

/// Counts unit `i`, which just died, in the pile under it (rule on; men, riders and animals
/// on land, not buildings, guns or ships).
pub fn add(w: &mut World, i: usize) {
    if !w.rules.corpse_piles {
        return;
    }
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    if u.building
        || u.inside_of.is_some()
        || t.medium != crate::catalog::Medium::Land
        || t.motion_style == crate::catalog::MotionStyle::ComplexRotate
    {
        return;
    }
    let (x, y) = u.pos.pixels();
    let cell = (x >> PILE_SHIFT, y >> PILE_SHIFT);
    if let Some(old) = w.piles.bodies.insert(i as u32, cell) {
        bump(w, old, -1);
    }
    bump(w, cell, 1);
}

/// Unit `i`'s corpse is gone: it leaves its pile.
pub fn forget(w: &mut World, i: usize) {
    if w.piles.bodies.is_empty() {
        return;
    }
    if let Some(cell) = w.piles.bodies.remove(&(i as u32)) {
        bump(w, cell, -1);
    }
}

/// Unit `i`'s corpse landed somewhere else (a `DETONATOR` throw): it moves to that pile.
pub fn moved(w: &mut World, i: usize) {
    if w.piles.bodies.contains_key(&(i as u32)) {
        add(w, i);
    }
}

/// Changes the count of `cell` by `delta` and keeps the blocking lock map in step.
fn bump(w: &mut World, cell: (i32, i32), delta: i32) {
    let old = w.piles.counts.get(&cell).copied().unwrap_or(0);
    let new = (i32::from(old) + delta).clamp(0, 255) as u8;
    if new == 0 {
        w.piles.counts.remove(&cell);
    } else {
        w.piles.counts.insert(cell, new);
    }
    let (was, is) = (old >= BLOCKED, new >= BLOCKED);
    if was == is {
        return;
    }
    let land = &w.map.land;
    let lock = w.piles.lock.get_or_insert_with(|| land.clone());
    let (lx, ly) = (cell.0 * 2, cell.1 * 2);
    for (x, y) in [(lx, ly), (lx + 1, ly), (lx, ly + 1), (lx + 1, ly + 1)] {
        if is {
            lock.set_pt(x, y);
        } else if !land.check_pt(x, y) {
            lock.clr_pt(x, y);
        }
    }
}
