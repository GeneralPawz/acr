//! The ground state of the detailed ground (docs/ground.md, phase 2): what feet, hooves,
//! wheels, bodies and rounds did to the soil, in tiles of [`TILE_TEXELS`]² texels of
//! [`TEXEL_PX`] world pixels (about 17 cm). Tiles are allocated on the first stamp into an
//! atlas of [`ATLAS_SLOTS`]² slots; a page table maps a map tile to its slot. Channels:
//! R compression, G mud / wetness, B scorch, A displacement (0.5 = level,
//! ±[`DISP_RANGE_M`]). Stamps are painted on the CPU (a few hundred texels each) and the
//! changed rectangles uploaded (`TerrainRenderer::upload_ground_state`). Render only.

/// World pixels per state texel (≈ 16.7 cm at 30 px per metre).
pub const TEXEL_PX: f32 = 5.0;
/// Texels per tile side.
pub const TILE_TEXELS: usize = 256;
/// World pixels per tile side.
pub const TILE_PX: f32 = TEXEL_PX * TILE_TEXELS as f32;
/// Slots per atlas side (the atlas is `ATLAS_SLOTS × TILE_TEXELS` square, 2048² = 16 MB).
pub const ATLAS_SLOTS: usize = 8;
/// Page table side in tiles (maps up to 64 × 1280 px = 81 920 px).
pub const PAGE_SIDE: usize = 64;
/// Displacement at the channel's ends, metres.
pub const DISP_RANGE_M: f32 = 0.25;
const PX_PER_M: f32 = 30.0;
const LEVEL: u8 = 128;
/// The smallest half-size a stamp keeps (world pixels): narrower marks would fall between
/// texel centres and vanish.
const MIN_HALF: f32 = TEXEL_PX * 0.75;

/// The shape of a stamp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// An ellipse of half-lengths `half` (along, across `along`), a bowl.
    Ellipse,
    /// A stroke from `at` to `to`, `half[1]` wide on each side (ruts, furrows, drags).
    Stroke { to: [f32; 2] },
    /// A crater of radius `half[0]` with a raised rim of `rim` metres.
    Crater { rim: f32 },
}

/// One mark on the ground (world pixels, metres for depths).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamp {
    pub at: [f32; 2],
    pub shape: Shape,
    /// Unit direction of the long axis.
    pub along: [f32; 2],
    /// Half-lengths in world pixels: along, across.
    pub half: [f32; 2],
    /// Displacement at the centre, metres (negative: a hole).
    pub depth: f32,
    pub compress: f32,
    pub mud: f32,
    pub scorch: f32,
}

impl Stamp {
    /// A plain ellipse print.
    pub fn print(at: [f32; 2], along: [f32; 2], half: [f32; 2], depth: f32, compress: f32) -> Self {
        Self { at, shape: Shape::Ellipse, along, half, depth, compress, mud: 0.0, scorch: 0.0 }
    }
}

struct Slot {
    tile: (usize, usize),
    data: Vec<u8>,
    /// Changed texels: x0, y0, x1, y1 (exclusive), if any.
    dirty: Option<[usize; 4]>,
    /// The stamp counter when last stamped (least recently used goes first).
    used: u64,
}

/// The CPU copy of the ground state and what to upload.
pub struct GroundState {
    pages: Vec<u8>,
    slots: Vec<Slot>,
    pages_dirty: bool,
    stamps: u64,
    fade_next: usize,
    /// Fades so far: varies the rounding of [`GroundState::fade`].
    fades: u32,
}

impl Default for GroundState {
    fn default() -> Self {
        Self {
            pages: vec![0; PAGE_SIDE * PAGE_SIDE],
            slots: Vec::new(),
            pages_dirty: true,
            stamps: 0,
            fade_next: 0,
            fades: 0,
        }
    }
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Accumulates `v` (0..1) into channel byte `c`: `c + v (1 − c)`.
fn accumulate(c: &mut u8, v: f32) {
    let x = f32::from(*c) / 255.0;
    *c = ((x + v.clamp(0.0, 1.0) * (1.0 - x)) * 255.0 + 0.5) as u8;
}

fn disp_byte(metres: f32) -> u8 {
    (f32::from(LEVEL) + metres / DISP_RANGE_M * 127.0).round().clamp(0.0, 255.0) as u8
}

impl GroundState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Slots in use.
    pub fn tiles(&self) -> usize {
        self.slots.len()
    }

    /// The slot of map tile `(tx, ty)`, allocated (or recycled, least recently stamped)
    /// on first use.
    fn slot(&mut self, tx: usize, ty: usize) -> usize {
        let page = self.pages[ty * PAGE_SIDE + tx];
        if page > 0 {
            return usize::from(page - 1);
        }
        let data = || {
            let mut d = vec![0u8; TILE_TEXELS * TILE_TEXELS * 4];
            d.as_chunks_mut::<4>().0.iter_mut().for_each(|t| t[3] = LEVEL);
            d
        };
        let i = if self.slots.len() < ATLAS_SLOTS * ATLAS_SLOTS {
            self.slots.push(Slot { tile: (tx, ty), data: data(), dirty: None, used: 0 });
            self.slots.len() - 1
        } else {
            let i = (0..self.slots.len()).min_by_key(|&i| self.slots[i].used).expect("slots");
            let (ox, oy) = self.slots[i].tile;
            self.pages[oy * PAGE_SIDE + ox] = 0;
            self.slots[i] = Slot { tile: (tx, ty), data: data(), dirty: None, used: 0 };
            i
        };
        self.slots[i].dirty = Some([0, 0, TILE_TEXELS, TILE_TEXELS]);
        self.pages[ty * PAGE_SIDE + tx] = i as u8 + 1;
        self.pages_dirty = true;
        i
    }

    /// The texel `(R, G, B, A)` at world pixel `(x, y)` (level ground where nothing is).
    pub fn texel(&self, x: f32, y: f32) -> [u8; 4] {
        let (gx, gy) = ((x / TEXEL_PX).floor(), (y / TEXEL_PX).floor());
        if gx < 0.0 || gy < 0.0 {
            return [0, 0, 0, LEVEL];
        }
        let (gx, gy) = (gx as usize, gy as usize);
        let (tx, ty) = (gx / TILE_TEXELS, gy / TILE_TEXELS);
        if tx >= PAGE_SIDE || ty >= PAGE_SIDE || self.pages[ty * PAGE_SIDE + tx] == 0 {
            return [0, 0, 0, LEVEL];
        }
        let s = &self.slots[usize::from(self.pages[ty * PAGE_SIDE + tx] - 1)];
        let i = ((gy % TILE_TEXELS) * TILE_TEXELS + gx % TILE_TEXELS) * 4;
        [s.data[i], s.data[i + 1], s.data[i + 2], s.data[i + 3]]
    }

    /// Paints one stamp.
    pub fn stamp(&mut self, s: &Stamp) {
        self.stamps += 1;
        let [ax, ay] = s.along;
        let len = (ax * ax + ay * ay).sqrt();
        let (ax, ay) = if len > 1e-6 { (ax / len, ay / len) } else { (1.0, 0.0) };
        let reach = match s.shape {
            Shape::Ellipse => s.half[0].max(s.half[1]).max(MIN_HALF),
            Shape::Stroke { .. } => s.half[1].max(MIN_HALF),
            Shape::Crater { .. } => s.half[0].max(MIN_HALF) * 1.6,
        };
        let (mut x0, mut y0, mut x1, mut y1) = (s.at[0] - reach, s.at[1] - reach, s.at[0] + reach, s.at[1] + reach);
        if let Shape::Stroke { to } = s.shape {
            x0 = x0.min(to[0] - reach);
            y0 = y0.min(to[1] - reach);
            x1 = x1.max(to[0] + reach);
            y1 = y1.max(to[1] + reach);
        }
        let limit = (PAGE_SIDE * TILE_TEXELS) as f32 - 1.0;
        let gx0 = (x0 / TEXEL_PX).floor().clamp(0.0, limit) as usize;
        let gy0 = (y0 / TEXEL_PX).floor().clamp(0.0, limit) as usize;
        let gx1 = (x1 / TEXEL_PX).ceil().clamp(0.0, limit) as usize;
        let gy1 = (y1 / TEXEL_PX).ceil().clamp(0.0, limit) as usize;
        if gx1 <= gx0 || gy1 <= gy0 || x1 < 0.0 || y1 < 0.0 {
            return;
        }
        let stamps = self.stamps;
        for gy in gy0..gy1 {
            for gx in gx0..gx1 {
                let px = (gx as f32 + 0.5) * TEXEL_PX - s.at[0];
                let py = (gy as f32 + 0.5) * TEXEL_PX - s.at[1];
                // Coverage `w` (0..1) and displacement (metres).
                let (w, disp, scorch) = match s.shape {
                    Shape::Ellipse => {
                        let u = (px * ax + py * ay) / s.half[0].max(MIN_HALF);
                        let v = (-px * ay + py * ax) / s.half[1].max(MIN_HALF);
                        let r = (u * u + v * v).sqrt();
                        let w = 1.0 - smooth(0.7, 1.3, r);
                        (w, s.depth * w, s.scorch * w)
                    }
                    Shape::Stroke { to } => {
                        let (dx, dy) = (to[0] - s.at[0], to[1] - s.at[1]);
                        let l2 = dx * dx + dy * dy;
                        let t = if l2 > 0.0 { ((px * dx + py * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                        let (qx, qy) = (px - t * dx, py - t * dy);
                        let r = (qx * qx + qy * qy).sqrt() / s.half[1].max(MIN_HALF);
                        let w = 1.0 - smooth(0.4, 1.0, r);
                        (w, s.depth * w, s.scorch * w)
                    }
                    Shape::Crater { rim } => {
                        let r = (px * px + py * py).sqrt() / s.half[0].max(MIN_HALF);
                        let bowl = if r < 1.0 { s.depth * (1.0 - r * r) } else { 0.0 };
                        let lip = rim * (-((r - 1.05) / 0.28).powi(2)).exp();
                        let w = 1.0 - smooth(0.9, 1.5, r);
                        (w, bowl + lip, s.scorch * (1.0 - smooth(0.6, 1.6, r)))
                    }
                };
                if w <= 0.0 && disp.abs() < 1e-4 && scorch <= 0.0 {
                    continue;
                }
                let (tx, ty) = (gx / TILE_TEXELS, gy / TILE_TEXELS);
                let i = self.slot(tx, ty);
                let slot = &mut self.slots[i];
                slot.used = stamps;
                let (lx, ly) = (gx % TILE_TEXELS, gy % TILE_TEXELS);
                let t = &mut slot.data[(ly * TILE_TEXELS + lx) * 4..][..4];
                accumulate(&mut t[0], s.compress * w);
                accumulate(&mut t[1], s.mud * w);
                t[2] = t[2].max((scorch.clamp(0.0, 1.0) * 255.0) as u8);
                let d = disp_byte(disp);
                // Holes take the deeper, rims the higher: marks never dig without end.
                if disp < 0.0 {
                    t[3] = t[3].min(d);
                } else if disp > 0.0 {
                    t[3] = t[3].max(d);
                }
                slot.dirty = Some(match slot.dirty {
                    None => [lx, ly, lx + 1, ly + 1],
                    Some([a, b, c, e]) => [a.min(lx), b.min(ly), c.max(lx + 1), e.max(ly + 1)],
                });
            }
        }
    }

    /// Recovery, called once per tick: one slot fades `ticks_per_slot` ticks' worth per
    /// call (compression and mud by `keep` per tick, shallow prints level out; craters,
    /// ruts and scorch stay), round robin.
    pub fn fade(&mut self, keep: f32) {
        if self.slots.is_empty() {
            return;
        }
        let n = self.slots.len();
        let i = self.fade_next % n;
        self.fade_next = (i + 1) % n;
        let k = keep.powi(n as i32);
        self.fades = self.fades.wrapping_add(1);
        let seed = self.fades.wrapping_mul(0x9e37_79b9);
        let slot = &mut self.slots[i];
        for (j, t) in slot.data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            // Dithered rounding: truncating took a whole step per call, so a slot alone
            // lost 1/255 a tick and every path was gone in seconds whatever `keep` said.
            let d = ((j as u32).wrapping_mul(0x85eb_ca6b) ^ seed) >> 8;
            let d = d as f32 / (1u32 << 24) as f32;
            t[0] = (f32::from(t[0]) * k + d) as u8;
            t[1] = (f32::from(t[1]) * k + d) as u8;
            // Prints shallower than 4 cm level out where the soil has recovered.
            if t[0] < 40 && t[3] < LEVEL && t[3] > LEVEL - 20 {
                t[3] += 1;
            }
        }
        slot.dirty = Some([0, 0, TILE_TEXELS, TILE_TEXELS]);
    }

    /// The page table (`PAGE_SIDE²` bytes: slot + 1, 0 = none) if it changed.
    pub fn take_pages(&mut self) -> Option<&[u8]> {
        std::mem::take(&mut self.pages_dirty).then_some(&self.pages[..])
    }

    /// The changed rectangles: (atlas texel x, y, width, height, RGBA rows), clearing them.
    pub fn take_dirty(&mut self) -> Vec<(u32, u32, u32, u32, Vec<u8>)> {
        let mut out = Vec::new();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let Some([x0, y0, x1, y1]) = slot.dirty.take() else { continue };
            let mut rows = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
            for y in y0..y1 {
                rows.extend_from_slice(&slot.data[(y * TILE_TEXELS + x0) * 4..(y * TILE_TEXELS + x1) * 4]);
            }
            let (sx, sy) = ((i % ATLAS_SLOTS) * TILE_TEXELS, (i / ATLAS_SLOTS) * TILE_TEXELS);
            out.push(((sx + x0) as u32, (sy + y0) as u32, (x1 - x0) as u32, (y1 - y0) as u32, rows));
        }
        out
    }
}

/// Metres to world pixels.
pub fn metres(m: f32) -> f32 {
    m * PX_PER_M
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_print_dents_and_compresses() {
        let mut g = GroundState::new();
        g.stamp(&Stamp::print([1000.0, 1000.0], [1.0, 0.0], [metres(0.14), metres(0.06)], -0.01, 0.3));
        let t = g.texel(1000.0, 1000.0);
        assert!(t[0] > 40 && t[3] < LEVEL, "{t:?}");
        assert_eq!(g.texel(1100.0, 1000.0), [0, 0, 0, LEVEL]);
        assert_eq!(g.tiles(), 1);
    }

    #[test]
    fn crater_has_a_rim_and_holes_do_not_deepen() {
        let mut g = GroundState::new();
        let c = Stamp {
            at: [2000.0, 2000.0],
            shape: Shape::Crater { rim: 0.06 },
            along: [1.0, 0.0],
            half: [metres(0.6), metres(0.6)],
            depth: -0.2,
            compress: 0.5,
            mud: 0.0,
            scorch: 0.0,
        };
        g.stamp(&c);
        let bottom = g.texel(2000.0, 2000.0)[3];
        assert!(bottom < LEVEL - 90, "{bottom}");
        assert!(g.texel(2000.0 + metres(0.63), 2000.0)[3] > LEVEL + 15);
        g.stamp(&c);
        assert_eq!(g.texel(2000.0, 2000.0)[3], bottom);
    }

    #[test]
    fn strokes_cross_tiles_and_upload_rectangles() {
        let mut g = GroundState::new();
        let s = Stamp {
            shape: Shape::Stroke { to: [1400.0, 600.0] },
            ..Stamp::print([1200.0, 600.0], [1.0, 0.0], [4.0, 4.0], -0.04, 0.5)
        };
        g.stamp(&s);
        assert_eq!(g.tiles(), 2);
        assert!(g.take_pages().is_some());
        assert!(g.take_pages().is_none());
        let dirty = g.take_dirty();
        assert_eq!(dirty.len(), 2);
        assert!(dirty.iter().all(|d| d.4.len() == (d.2 * d.3 * 4) as usize));
        assert!(g.take_dirty().is_empty());
    }

    #[test]
    fn compression_recovers_craters_stay() {
        let mut g = GroundState::new();
        g.stamp(&Stamp::print([100.0, 100.0], [1.0, 0.0], [20.0, 20.0], -0.3, 1.0));
        for _ in 0..3000 {
            g.fade(0.9997);
        }
        let t = g.texel(100.0, 100.0);
        assert!(t[0] < 120, "{t:?}");
        assert!(t[3] < 10, "a deep hole stays: {t:?}");
    }

    #[test]
    fn compression_fades_at_its_rate() {
        // One slot fades every tick: 600 ticks at 0.9997 keep 84 % (truncating left 0).
        let mut g = GroundState::new();
        g.stamp(&Stamp::print([100.0, 100.0], [1.0, 0.0], [20.0, 20.0], -0.01, 0.6));
        let r0 = f32::from(g.texel(100.0, 100.0)[0]);
        for _ in 0..600 {
            g.fade(0.9997);
        }
        let r = f32::from(g.texel(100.0, 100.0)[0]);
        let want = r0 * 0.9997f32.powi(600);
        assert!((r - want).abs() < 8.0, "{r0} -> {r}, want {want}");
    }

    #[test]
    fn full_atlas_recycles_the_oldest_tile() {
        let mut g = GroundState::new();
        for i in 0..=ATLAS_SLOTS * ATLAS_SLOTS {
            let (tx, ty) = (i % PAGE_SIDE, i / PAGE_SIDE);
            let at = [(tx as f32 + 0.5) * TILE_PX, (ty as f32 + 0.5) * TILE_PX];
            g.stamp(&Stamp::print(at, [1.0, 0.0], [10.0, 10.0], -0.01, 0.5));
        }
        assert_eq!(g.tiles(), ATLAS_SLOTS * ATLAS_SLOTS);
        assert_eq!(g.texel(0.5 * TILE_PX, 0.5 * TILE_PX)[0], 0, "the first tile was recycled");
    }
}
