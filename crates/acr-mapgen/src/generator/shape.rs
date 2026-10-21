//! The land mask and everything derived from it: water, shore textures, land elevation;
//! plus the inland texture noise and the ground paint layer (`UORG`).

use acr_formats::m3d::Grid;

use super::Gen;
use crate::files::CostTable;

impl Gen<'_> {
    /// Extent of the mask in world pixels and of one mask cell (0x4B8403).
    fn mask_extent(&self) -> (i32, i32) {
        let ext = (0x1E0 << (self.s - 1)) << 5;
        (ext, ext >> 8)
    }

    /// Smooths the mask `30 >> (s - 1)` times with weights 12/16 centre, 1/16 per
    /// neighbour, then copies the border rows and columns from their neighbours (0x45D005).
    pub fn blur_mask(&mut self) {
        let m = &mut self.mask;
        for _ in 0..(30 >> (self.s - 1)) {
            let mut tmp = m.clone();
            for y in 1..255 {
                for x in 1..255 {
                    let i = y * 256 + x;
                    let c = u32::from(m[i]);
                    let sum = c * 12
                        + u32::from(m[i - 1])
                        + u32::from(m[i + 1])
                        + u32::from(m[i - 256])
                        + u32::from(m[i + 256]);
                    tmp[i] = (sum >> 4) as u8;
                }
            }
            m.copy_from_slice(&tmp);
            for i in 0..256 {
                m[i] = m[i + 256];
                m[i + 0xFF00] = m[i + 0xFE00];
                m[i * 256] = m[i * 256 + 1];
                m[i * 256 + 255] = m[i * 256 + 254];
            }
        }
    }

    /// The mask bilinearly interpolated at a world position, ×16 (0x45CB13): 0 deep water,
    /// 4080 inland. Callers subtract 0x800 to get a signed coast distance.
    pub fn sample(&self, x: i32, y: i32) -> i32 {
        let (ext, cell) = self.mask_extent();
        let x = x.max(0).min(ext - 1);
        let y = y.max(0).min(ext - 1);
        let gx = ((x << 8) / ext).min(254);
        let gy = ((y << 8) / ext).min(254);
        let fx = x - ((gx * ext) >> 8);
        let fy = y - ((gy * ext) >> 8);
        let i = (gy * 256 + gx) as usize;
        let m = |k: usize| i32::from(self.mask[k]) << 4;
        let (p00, p10, p01, p11) = (m(i), m(i + 1), m(i + 256), m(i + 257));
        let a = p00 + (p10 - p00) * fx / cell;
        let b = ((p01 - p00) * fy + (p11 + p00 - p10 - p01) * fx * fy / cell) / cell;
        a + b
    }

    /// `SEA2` (0x45D2E1): layer A ("depth") rises from 0 on land into the sea, layer B
    /// ("brightness") is 120 on land and falls quadratically offshore, with noise.
    pub fn water(&mut self) {
        let n = self.sea + 2;
        for y in 0..n {
            for x in 0..n {
                let v = self.sample(x << 5, y << 5) - 0x800;
                let mut a = (0x78 - (v >> 4)).clamp(0, 0xFF);
                let mut b = v / 20 + (self.rng.rand() & 7) + 0x75;
                b = b.clamp(0, 0x78);
                b = 0x78 - (0x78 - b) * (0x78 - b) * 0x78 / (105 * 105);
                b = b.clamp(0, 0x78);
                if self.s == 3 {
                    a = a.clamp(0x43, 0xBE) * 2 - 0x80;
                } else {
                    a = ((a - 0x80) >> (2 - self.s)) + 0x80;
                }
                let i = (y * n + x) as usize;
                self.depth[i] = a as u8;
                self.bright[i] = b as u8;
            }
        }
    }

    /// Water and shore textures by coast distance from `Terrain\cost%d.dat` (0x45D53F).
    /// They carry bit 7 ("taken"), which keeps pieces off the coast until it is cleared at
    /// the end; deep inland stays 0 for the noise textures.
    pub fn shore_textures(&mut self, climate: u8) {
        let Some(cost) = self.source.read(&format!("Terrain\\cost{climate}.dat")).and_then(|d| CostTable::parse(&d))
        else {
            return;
        };
        if cost.step == 0 {
            return;
        }
        let lim = (self.sea << 5) + 64;
        for v in 0..self.vw * self.vh {
            let (x, y) = self.vertex_pos(v);
            if x <= 0 || y <= 0 || x >= lim || y >= lim {
                continue;
            }
            let level = ((self.sample(x, y) - 0x800) / cost.step).max(0) as usize;
            if let Some(list) = cost.levels.get(level) {
                let r = self.rng.below(list.len() as i32) as usize;
                self.tiles[v] = list.get(r).copied().unwrap_or(0).wrapping_add(0x80);
            }
        }
    }

    /// Raises land with the coast distance, up to 64 (0x45DCCD).
    pub fn elevate(&mut self) {
        for v in 0..self.vw * self.vh {
            let (x, y) = self.vertex_pos(v);
            let d = self.sample(x, y) - 0x800;
            if d > 0x20 {
                let mut add = ((((d * d) >> 14) * 29) << 6) / 5800;
                if add > 0x3F {
                    add = 0x40;
                }
                self.heights[v] = self.heights[v].wrapping_add(add as i16);
            }
        }
    }

    /// Inland textures (0x4B7E9B): untouched vertices get `fract[level]` by a fractal noise
    /// level, reserved start areas (0xFF) the same with bit 7.
    pub fn noise_textures(&mut self, fract: &[u8; 8]) {
        let noise = Noise::generate(&mut self.rng);
        let mut v = 0;
        for row in 0..self.vh {
            for col in 0..self.vw {
                // 0x4AA7D2 at (col·256, row·256): the fraction is 0, the corner (x+1, y+1).
                let n = noise.get((col << 3) + 1, (row << 3) + 1);
                let level = (n / 64).clamp(0, 7) as usize;
                match self.tiles[v] {
                    0 => self.tiles[v] = fract[level],
                    0xFF => self.tiles[v] = fract[level].wrapping_add(0x80),
                    _ => {}
                }
                v += 1;
            }
        }
    }

    /// The `UORG` ground paint layer from `Terrain\factures_set%d.dat` (0x468AF7).
    pub fn ground(&mut self) -> (Grid<u8>, Grid<u16>) {
        let s = self.s;
        let (w, h) = (0x100usize << s, 0x80usize << s);
        let shift = s + 8;
        let mut paint = vec![0xFFu8; w * h];
        let mut light = vec![0u8; w * h];
        for row in 4..self.vh - 4 {
            for (v, col) in (row * self.vw + 1..).zip(1..self.vw - 4) {
                let height = i32::from(self.heights[v]);
                if height <= 0x46 {
                    let t = usize::from(self.tiles[v]);
                    let f = self.factures.get(t).copied().unwrap_or([0xFF; 5]);
                    if f[0] != 0xFF {
                        let idx = col as i64 + ((((row >> 1) as i64) - i64::from(height >> 5)) << shift);
                        let pick = (self.rng.rand() & 3) as usize;
                        if (0..(w * h) as i64).contains(&idx) {
                            paint[idx as usize] = f[pick];
                            light[idx as usize] = f[4];
                        }
                    }
                }
            }
        }
        let mut bytes = std::mem::take(&mut self.ground8);
        let mut words = std::mem::take(&mut self.ground16);
        let (mut rx, mut ry) = (0i32, 0i32);
        for i in w..(0x7800usize << (2 * s)).min(w * h - w) {
            if paint[i] == 0xFF {
                continue;
            }
            let lo = (i & 0x7F) as i32;
            let q = (i / w) as i32 / 3;
            let a = if q & 1 != 0 { lo / 3 } else { (lo + 1) / 3 };
            if q & 1 != 0 {
                rx = i32::from(self.random[((a * 19 + q * 17) & 0x1FFF) as usize] & 7);
                ry = i32::from(self.random[((a * 13 + q * 23) & 0x1FFF) as usize] & 7);
            }
            words[i] = ((i32::from(paint[i]) << 6) + rx + ry * 8) as u16;
            let sum =
                u32::from(light[i - 1]) + u32::from(light[i + 1]) + u32::from(light[i - w]) + u32::from(light[i + w]);
            bytes[i] = (sum >> 2) as u8;
        }
        (Grid { width: w, height: h, cells: bytes }, Grid { width: w, height: h, cells: words })
    }
}

/// The 1024 × 1024 fractal noise of 0x4AA34F (midpoint displacement from a 16-cell grid,
/// normalised to 0..=512 and smoothed once).
pub struct Noise(Vec<i16>);

impl Noise {
    const N: usize = 1024;

    fn get(&self, x: usize, y: usize) -> i32 {
        i32::from(self.0[(y & 0x3FF) * Self::N + (x & 0x3FF)])
    }

    fn set(&mut self, x: usize, y: usize, v: i32) {
        if x < Self::N && y < Self::N {
            self.0[y * Self::N + x] = v as i16;
        }
    }

    pub fn generate(rng: &mut crate::rng::Rng) -> Self {
        let mut n = Self(vec![0; Self::N * Self::N]);
        // 0x4AA2D0: rand·a >> 14 − a, i.e. −a..a.
        let mut rnd = |a: i32| ((rng.rand() * a) >> 14) - a;
        let mut step = Self::N >> 6;
        let mut amp: i16 = 0x800;
        for a in (0..Self::N).step_by(step) {
            for b in (0..Self::N).step_by(step) {
                let v = rnd(amp.into());
                n.set(a, b, v);
            }
        }
        while step > 1 {
            amp >>= 1;
            let h = step / 2;
            for a in (0..Self::N).step_by(step) {
                for b in (0..Self::N).step_by(step) {
                    let r = rnd(amp.into());
                    let v = r + ((n.get(a, b) + n.get(a + step, b)) >> 1);
                    n.set(a + h, b, v);
                    let r = rnd(amp.into());
                    let v = r + ((n.get(a, b + step) + n.get(a + step, b + step)) >> 1);
                    n.set(a + h, b + step, v);
                    let r = rnd(amp.into());
                    let v = r + ((n.get(a, b) + n.get(a, b + step)) >> 1);
                    n.set(a, b + h, v);
                    let r = rnd(amp.into());
                    let v = r + ((n.get(a + step, b) + n.get(a + step, b + step)) >> 1);
                    n.set(a + step, b + h, v);
                    let r = rnd(amp.into());
                    let v = r
                        + ((n.get(a + h, b) + n.get(a + h, b + step) + n.get(a, b + h) + n.get(a + step, b + h)) >> 2);
                    n.set(a + h, b + h, v);
                }
            }
            step >>= 1;
        }
        let (mut lo, mut hi) = (100_000, -100_000);
        for &v in &n.0 {
            lo = lo.min(i32::from(v));
            hi = hi.max(i32::from(v));
        }
        let range = (hi - lo).max(1);
        for v in &mut n.0 {
            *v = (((i32::from(*v) - lo) << 9) / range) as i16;
        }
        for a in 0..Self::N {
            for b in 0..Self::N {
                let (am, bm) = ((a + Self::N - 1) & 0x3FF, (b + Self::N - 1) & 0x3FF);
                let sum = n.get(a + 1, b) + n.get(am, b) + n.get(a, b + 1) + n.get(a, bm) + n.get(a, b);
                n.set(a, b, sum / 5);
            }
        }
        n
    }
}
