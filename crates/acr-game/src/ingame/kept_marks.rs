//! `battlefield_marks=keep` (settings): the marks of a battle stay until it ends. The
//! decals of hits and impacts are derived each frame from `World::hits` / `World::impacts`,
//! which the simulation drops after `combat::HIT_KEEP` ticks, and corpse pools go with
//! their corpses; with `keep` each mark is copied here, dried and fully visible, once it
//! has settled, and drawn from here from then on. Render only: the simulation and its
//! hashes do not change.
//!
//! Bounded: a mark landing on a kept mark of the same kind (within [`MERGE`] of its radius)
//! merges into it, growing it a little up to [`GROW_MAX`], instead of adding a decal, and
//! beyond [`CAP`] decals the oldest go. Craters on the detailed ground are stamps in the
//! ground state (`ground_marks.rs`), whose displacement and scorch stay anyway.

use std::collections::{HashMap, VecDeque};

use acr_render::model_renderer::{BloodPool, Dirt};

/// Ticks after a hit or impact when its decals are kept: dried (`splatter.rs` DRIED) and
/// before they start to fade.
pub const SETTLED: u64 = 500;
/// Kept decals at most (about 76 bytes each on the CPU and as a GPU instance).
pub const CAP: usize = 8192;
/// Two marks of one kind merge when their centres lie closer than this share of the kept
/// one's radius.
const MERGE: f32 = 0.6;
/// A merged mark grows to at most this many times its first radius.
const GROW_MAX: f32 = 1.8;
/// Side of the cells of the merge lookup (world pixels, about 1.3 m).
const CELL: f32 = 40.0;

/// The kind of a decal, for merging (blood merges with blood, earth with earth).
fn kind(p: &BloodPool) -> u8 {
    match (p.dirt, p.splatter, p.smear.is_some()) {
        (Dirt::None, false, false) => 0,
        (Dirt::None, true, _) => 1,
        (Dirt::None, false, true) => 2,
        (Dirt::Crater, ..) => 3,
        (Dirt::Scuff, ..) => 4,
    }
}

struct Kept {
    pool: BloodPool,
    /// The radius it was kept with (merges grow it up to [`GROW_MAX`] times this).
    first: f32,
}

#[derive(Default)]
pub struct KeptMarks {
    marks: VecDeque<Kept>,
    /// Serial number of `marks[0]`.
    base: u64,
    /// Newest mark of each kind per cell, by serial number.
    cells: HashMap<(i32, i32, u8), u64>,
    /// Hits and impacts up to this tick are kept here, not drawn from the world.
    pub hits_until: u64,
    pub impacts_until: u64,
    /// The last pool of each corpse drawn, kept when the corpse goes.
    corpses: HashMap<u32, BloodPool>,
    seen: Vec<u32>,
    /// Merges since the battle began (for the log and the bench).
    pub merged: u64,
}

impl KeptMarks {
    /// Forgets everything (a new battle, a loaded game).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn len(&self) -> usize {
        self.marks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// Keeps `pool`, dried and fully visible, merging it into a kept mark of its kind under it.
    pub fn keep(&mut self, mut pool: BloodPool) {
        if pool.radius <= 0.0 {
            return;
        }
        pool.fade = 1.0;
        let k = kind(&pool);
        let cell = ((pool.centre[0] / CELL).floor() as i32, (pool.centre[1] / CELL).floor() as i32, k);
        if let Some(&serial) = self.cells.get(&cell)
            && let Some(i) = serial.checked_sub(self.base).map(|i| i as usize)
            && let Some(kept) = self.marks.get_mut(i)
        {
            let (dx, dy) = (pool.centre[0] - kept.pool.centre[0], pool.centre[1] - kept.pool.centre[1]);
            if (dx * dx + dy * dy).sqrt() < MERGE * kept.pool.radius.max(pool.radius) {
                // Areas add up, to a limit: a much-hit spot is one big, dark mark.
                let r = (kept.pool.radius.powi(2) + 0.5 * pool.radius.powi(2)).sqrt();
                kept.pool.radius = r.min(kept.first.max(pool.radius) * GROW_MAX);
                kept.pool.age = kept.pool.age.max(pool.age);
                self.merged += 1;
                return;
            }
        }
        if self.marks.len() >= CAP {
            self.marks.pop_front();
            self.base += 1;
        }
        let serial = self.base + self.marks.len() as u64;
        self.cells.insert(cell, serial);
        self.marks.push_back(Kept { first: pool.radius, pool });
        if self.cells.len() > 4 * CAP {
            let base = self.base;
            self.cells.retain(|_, s| *s >= base);
        }
    }

    /// A corpse's pool this frame (call for every corpse drawn, then [`Self::end_corpses`]).
    pub fn corpse_pool(&mut self, unit: u32, pool: &BloodPool) {
        // Its biggest, most dried state: the fade at the end must not shrink what stays.
        let entry = self.corpses.entry(unit).or_insert(*pool);
        let fade = entry.fade.max(pool.fade);
        *entry = BloodPool { fade, radius: entry.radius.max(pool.radius), ..*pool };
        self.seen.push(unit);
    }

    /// Keeps the pools of the corpses that were not drawn this frame (gone), laid on
    /// `ground`.
    pub fn end_corpses(&mut self, ground: &dyn Fn(f32, f32) -> [f32; 3]) {
        let mut seen = std::mem::take(&mut self.seen);
        seen.sort_unstable();
        let gone: Vec<u32> = self.corpses.keys().copied().filter(|u| seen.binary_search(u).is_err()).collect();
        for u in gone {
            if let Some(p) = self.corpses.remove(&u) {
                let mut p = [p];
                super::splatter::lay_on_ground(&mut p, ground);
                self.keep(p[0]);
            }
        }
        seen.clear();
        self.seen = seen;
    }

    /// The kept decals, appended to this frame's.
    pub fn draw(&self, out: &mut Vec<BloodPool>) {
        out.extend(self.marks.iter().map(|k| k.pool));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crater(x: f32, y: f32) -> BloodPool {
        BloodPool { centre: [x, y, 0.0], radius: 13.0, fade: 0.3, dirt: Dirt::Crater, ..Default::default() }
    }

    #[test]
    fn kept_marks_merge_and_stay_capped() {
        let mut k = KeptMarks::default();
        k.keep(crater(100.0, 100.0));
        k.keep(crater(102.0, 101.0));
        assert_eq!((k.len(), k.merged), (1, 1));
        let mut out = Vec::new();
        k.draw(&mut out);
        assert_eq!(out[0].fade, 1.0);
        assert!(out[0].radius > 13.0 && out[0].radius <= 13.0 * GROW_MAX);
        for _ in 0..50 {
            k.keep(crater(101.0, 100.0));
        }
        let mut out = Vec::new();
        k.draw(&mut out);
        assert!((out[0].radius - 13.0 * GROW_MAX).abs() < 1e-3, "{}", out[0].radius);
        // Blood does not merge into earth.
        k.keep(BloodPool { dirt: Dirt::None, ..crater(100.0, 100.0) });
        assert_eq!(k.len(), 2);
        for i in 0..CAP + 100 {
            k.keep(crater((i % 1000) as f32 * 50.0, (i / 1000) as f32 * 50.0 + 1000.0));
        }
        assert_eq!(k.len(), CAP);
    }

    #[test]
    fn corpse_pools_stay_when_the_corpse_goes() {
        let flat = |x: f32, y: f32| [x, y, 0.0];
        let mut k = KeptMarks::default();
        let p = BloodPool { centre: [50.0, 50.0, 0.0], radius: 20.0, fade: 1.0, ..Default::default() };
        k.corpse_pool(7, &p);
        k.end_corpses(&flat);
        assert_eq!(k.len(), 0);
        k.corpse_pool(7, &BloodPool { fade: 0.1, ..p });
        k.end_corpses(&flat);
        k.end_corpses(&flat);
        assert_eq!(k.len(), 1);
        let mut out = Vec::new();
        k.draw(&mut out);
        assert_eq!((out[0].fade, out[0].radius), (1.0, 20.0));
    }
}
