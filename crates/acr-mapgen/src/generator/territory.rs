//! Start points and territories on the coarse grid (128-pixel cells).
//!
//! Cell values: `slot << 4 | distance` (distance from the start point in 4-cell steps,
//! capped at 15), 0xEE for land nobody reached, 0xFF for water and the border.

use super::Gen;
use crate::files::LandShape;

/// Distance approximation of the engine (0x427210): `(max + |dx| + |dy|) / 2`.
pub fn norm(dx: i32, dy: i32) -> i32 {
    let (a, b) = (dx.unsigned_abs(), dy.unsigned_abs());
    ((a.max(b) + a + b) >> 1) as i32
}

/// Offsets grouped by `⌊√(dx² + dy²)⌋` for radii 0..90 (0x546AC0), in the engine's order
/// (dx outer, dy inner, both from −90 to 90).
pub fn ring_table() -> Vec<Vec<(i8, i8)>> {
    let mut ring = vec![Vec::new(); 90];
    for dx in -90i32..=90 {
        for dy in -90i32..=90 {
            let r = f64::from(dx * dx + dy * dy).sqrt() as usize;
            if r < 90 {
                ring[r].push((dx as i8, dy as i8));
            }
        }
    }
    ring
}

impl Gen<'_> {
    fn coarse_shift(&self) -> u32 {
        self.s + 6
    }

    /// Side of the used part of the coarse grid (0x667958).
    pub fn coarse_used(&self) -> i32 {
        self.sea >> 2
    }

    pub fn terr_at(&self, x: i32, y: i32) -> u8 {
        self.terr[((y << self.coarse_shift()) + x) as usize]
    }

    /// Picks the shape's start points (0x45D7DB): shuffles the group order, then for every
    /// group one of its points. Returns the coarse cells by slot.
    pub fn start_points(&mut self, shape: &LandShape) -> Vec<(i32, i32)> {
        let groups = shape.groups.min(8);
        let mut counts = [0i32; 8];
        for i in 0..shape.points.min(8) {
            if let Some(c) = counts.get_mut(shape.point_group[i] as usize) {
                *c += 1;
            }
        }
        let mut order: [i32; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
        for _ in 0..20 {
            let a = self.rng.below(groups as i32) as usize;
            let b = self.rng.below(groups as i32) as usize;
            if a != b {
                order.swap(a, b);
            }
        }
        let mut cells = vec![(0, 0); groups];
        for (gi, cell) in cells.iter_mut().enumerate() {
            // The engine draws by the count of group `gi` but searches group `order[gi]`.
            let mut r = self.rng.below(counts[gi]);
            for i in 0..shape.points.min(8) {
                if r == -1 {
                    break;
                }
                if i32::from(shape.point_group[i]) == order[gi] {
                    if r == 0 {
                        let (x, y) = shape.positions[i];
                        *cell = ((i32::from(x) * self.sea) >> 10, (i32::from(y) * self.sea) >> 10);
                    }
                    r -= 1;
                }
            }
        }
        cells
    }

    /// Grows one region per start point over the land (0x45E2CE, 0x45E485, 0x45E508) and
    /// lists the land nobody reached.
    pub fn territories(&mut self, cells: &[(i32, i32)]) {
        let n = self.coarse_used();
        let sh = self.coarse_shift();
        let stride = 1usize << sh;
        self.terr.fill(0xFF);
        for y in 1..n {
            let row = (y << sh) as usize;
            self.terr[row + 1..row + n as usize].fill(0xEE);
        }
        let sea_stride = (self.sea + 2) as usize;
        for x in 1..n {
            for y in 1..n {
                let i = (y * 4 + 1) as usize * sea_stride + (x * 4 + 1) as usize;
                if self.depth[i] > 0x80 {
                    self.terr[((y << sh) + x) as usize] = 0xFF;
                }
            }
        }
        self.active = [false; 8];
        for (slot, &(x, y)) in cells.iter().enumerate() {
            if x > 0 && y > 0 && x < n && y < n {
                self.start_cell[slot] = (x, y);
                self.active[slot] = true;
                self.terr[((y << sh) + x) as usize] = (slot << 4) as u8;
            }
        }

        // Region growing.
        let side = 0x3C << self.s;
        let snapshot = self.terr.clone();
        let mut center = [(0i32, 0i32); 16];
        for y in 2..side {
            for x in 2..side {
                let v = snapshot[((y << sh) + x) as usize];
                if v < 0x80 {
                    center[(v >> 4) as usize] = (x, y);
                }
            }
        }
        loop {
            let copy = self.terr.clone();
            let mut changed = false;
            for y in 0..side {
                for x in 0..side {
                    let i = ((y << sh) + x) as usize;
                    if copy[i] != 0xEE {
                        continue;
                    }
                    let mut take = |nb: u8| -> bool {
                        if nb >= 0xEE {
                            return false;
                        }
                        let p = (nb >> 4) as usize;
                        let d = (norm(x - center[p].0, y - center[p].1) >> 2).min(15);
                        self.terr[i] = ((p << 4) as i32 + d) as u8;
                        changed = true;
                        true
                    };
                    if take(copy[i + 1]) {
                        continue;
                    }
                    if x > 0 && take(copy[i - 1]) {
                        continue;
                    }
                    if copy[i + stride] < 0xEE {
                        take(copy[i + stride]);
                        continue;
                    }
                    if y > 0 {
                        take(copy[i - stride]);
                    }
                }
            }
            if !changed {
                break;
            }
        }

        self.unowned.clear();
        let lim = (stride as i32) - 1;
        for x in (1..lim).step_by(2) {
            for y in (1..lim).step_by(2) {
                if self.terr[((y << sh) + x) as usize] == 0xEE && self.unowned.len() < 0x300 {
                    self.unowned.push((x, y));
                }
            }
        }
    }

    /// A random cell near `(x, y)` whose territory value lies in `lo..=hi` (0x45EBFA);
    /// `lo = 0xFF` picks any unowned land cell instead.
    pub fn pick_cell(&mut self, x: i32, y: i32, lo: u8, hi: u8) -> Option<(i32, i32)> {
        if lo == 0xFF {
            if self.unowned.is_empty() {
                return None;
            }
            let i = self.rng.below(self.unowned.len() as i32) as usize;
            return Some(self.unowned[i]);
        }
        let ra = i32::from(lo & 0xF) * 4;
        let rb = i32::from(hi & 0xF) * 4 + 3;
        let n = self.coarse_used();
        for _ in 0..400 {
            let r = (ra + self.rng.below(rb - ra)) as usize;
            let ring = &self.ring[r.min(89)];
            let j = self.rng.below(ring.len() as i32) as usize;
            let (dx, dy) = ring[j];
            let (nx, ny) = (x + i32::from(dx), y + i32::from(dy));
            if nx > 5 && ny > 5 && nx < n - 4 && ny < n - 4 {
                let v = self.terr_at(nx, ny);
                if v >= lo && v <= hi {
                    return Some((nx, ny));
                }
            }
        }
        None
    }
}
