//! The static map: terrain heights, the passability bitmaps and the pathfinding areas,
//! built from an `.m3d` map exactly like the engine's map loader does.
//!
//! Passability (`MotionField`, `MFIELDS[media]` at 0x277885c) is one bit per 16 × 16 px
//! cell, one layer for land units and one for ships. `CreateLockMap` (0x44e340) derives it
//! from the terrain slope, the `LOC1` overrides and the `SEA2` water depth (see
//! `docs/simulation.md`). The coarse level are the `TOP1`/`TOPW` areas on 64 px cells.

use std::collections::BTreeMap;

use acr_formats::defs::SpriteResources;
use acr_formats::m3d::{Areas, Grid, M3dMap, Sprite, SpriteGroup};
use acr_formats::sprite_list::SpriteList;

/// Side of a passability cell in world pixels.
pub const LOCK_CELL: i32 = 16;
/// Side of a pathfinding-area cell in world pixels.
pub const AREA_CELL: i32 = 64;

/// Terrain heights (`SURF`) with the engine's height lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Heights {
    /// Vertices per row (513 for a normal map).
    pub stride: usize,
    pub cells: Vec<i16>,
    /// Map size shift (1 = normal).
    pub shift: u8,
}

impl Heights {
    fn v(&self, i: i32) -> i32 {
        usize::try_from(i).ok().and_then(|i| self.cells.get(i)).map_or(0, |&h| i32::from(h))
    }

    /// Ground height at world pixel `(x, y)` (0x4d0530): the vertex grid is triangulated with
    /// odd columns shifted half a cell up, and the height is interpolated in the triangle.
    pub fn at(&self, x: i32, y: i32) -> i32 {
        let max = (240 << self.shift) << 5;
        let x = x.max(0).min(max);
        let y = y.max(32).min(max);
        let s = self.stride as i32;
        let col = x >> 5;
        let xr = x & 31;
        if col & 1 == 1 {
            let t = 15 - (xr >> 1) + y;
            let row = t >> 5;
            if t & 31 > 32 - xr {
                let idx = s * row + col + 1;
                let a = self.v(idx);
                let p = self.v(idx + s - 1);
                let b = self.v(idx + s);
                let dx = x - (col << 5);
                p + ((dx * (((b + a) >> 1) - p)) >> 5) + (((y - (row << 5) - 16) * (b - a)) >> 5)
            } else {
                let idx = s * row + col;
                let c1 = self.v(idx + 1);
                let c0 = self.v(idx);
                let n = self.v(idx + s);
                let dx = x - ((col + 1) << 5);
                (((y - (row << 5)) * (n - c0)) >> 5) - ((dx * (((n + c0) >> 1) - c1)) >> 5) + c1
            }
        } else {
            let t = (xr >> 1) + y;
            let row = t >> 5;
            let idx = s * row + col;
            if t & 31 < xr {
                let c0 = self.v(idx);
                let a = self.v(idx + 1);
                let n1 = self.v(idx + s + 1);
                let dx = x - (col << 5);
                c0 + ((dx * (((n1 + a) >> 1) - c0)) >> 5) + (((y - (row << 5)) * (n1 - a)) >> 5)
            } else {
                let q = self.v(idx + s + 1);
                let c0 = self.v(idx);
                let n0 = self.v(idx + s);
                let dx = x - ((col + 1) << 5);
                (((y - (row << 5) - 16) * (n0 - c0)) >> 5) - ((dx * (((n0 + c0) >> 1) - q)) >> 5) + q
            }
        }
    }
}

/// One passability layer: a bit per 16 px cell, set = blocked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockMap {
    pub width: i32,
    pub height: i32,
    bits: Vec<u64>,
}

impl LockMap {
    pub fn new(width: i32, height: i32) -> Self {
        let n = (width.max(0) as usize * height.max(0) as usize).div_ceil(64);
        Self { width, height, bits: vec![0; n] }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height).then(|| y as usize * self.width as usize + x as usize)
    }

    /// `CheckPt` (0x5335ef): cells outside the map count as blocked.
    pub fn check_pt(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_none_or(|i| self.bits[i >> 6] >> (i & 63) & 1 == 1)
    }

    pub fn set_pt(&mut self, x: i32, y: i32) {
        if let Some(i) = self.index(x, y) {
            self.bits[i >> 6] |= 1 << (i & 63);
        }
    }

    pub fn clr_pt(&mut self, x: i32, y: i32) {
        if let Some(i) = self.index(x, y) {
            self.bits[i >> 6] &= !(1 << (i & 63));
        }
    }

    /// `CheckHLine` (0x5336c5).
    pub fn check_hline(&self, x: i32, y: i32, len: i32) -> bool {
        (0..len).any(|i| self.check_pt(x + i, y))
    }

    /// `CheckVLine` (0x533710): stricter at the edges than `CheckPt` (x or y ≤ 0 is blocked)
    /// and refuses lines longer than 24 cells.
    pub fn check_vline(&self, x: i32, y: i32, len: i32) -> bool {
        if x <= 0 || y <= 0 || y + len > self.height || x >= self.width || len > 24 {
            return true;
        }
        (0..len).any(|i| self.check_pt(x, y + i))
    }

    /// `CheckBar` (0x533808): any blocked cell in the `lx × ly` box.
    pub fn check_bar(&self, x: i32, y: i32, lx: i32, ly: i32) -> bool {
        (0..lx).any(|i| self.check_vline(x + i, y, ly))
    }

    pub fn set_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for yy in y.max(0)..(y + h).min(self.height) {
            for xx in x.max(0)..(x + w).min(self.width) {
                self.set_pt(xx, yy);
            }
        }
    }

    pub fn clr_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for yy in y.max(0)..(y + h).min(self.height) {
            for xx in x.max(0)..(x + w).min(self.width) {
                self.clr_pt(xx, yy);
            }
        }
    }

    /// `BSetBar` (0x533483): an `n × n` square.
    pub fn set_bar(&mut self, x: i32, y: i32, n: i32) {
        self.set_rect(x, y, n, n);
    }

    /// `BClrBar` (0x5334de).
    pub fn clr_bar(&mut self, x: i32, y: i32, n: i32) {
        self.clr_rect(x, y, n, n);
    }

    /// Any blocked cell in the 2 × 2 block at `(x, y)` (used by the 64 px quantisation).
    fn any2(&self, x: i32, y: i32) -> bool {
        self.check_pt(x, y) || self.check_pt(x + 1, y) || self.check_pt(x, y + 1) || self.check_pt(x + 1, y + 1)
    }

    /// Number of blocked cells.
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }
}

/// 4 × 4 corner patterns of the chamfer pass (0x644560..0x64459c), row by row, `*` = blocked.
const P_UL: [&[u8; 4]; 4] = [b"****", b"*** ", b"**  ", b"*   "];
const P_LL: [&[u8; 4]; 4] = [b"*   ", b"**  ", b"*** ", b"****"];
const P_LR: [&[u8; 4]; 4] = [b"   *", b"  **", b" ***", b"****"];
const P_UR: [&[u8; 4]; 4] = [b"****", b" ***", b"  **", b"   *"];

/// Blocking polygon of a sprite type (`.RSR` `[LOCK]`), with the list's hotspot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteLock {
    pub dx: i32,
    pub dy: i32,
    pub polygon: Vec<(i32, i32)>,
    /// `[INTERNAL_SOURCE]` objects (mines) lock after the 64 px quantisation, at full
    /// resolution (0x44e250); the others (rocks) during it.
    pub source: bool,
}

/// `(source type, amount per work cycle, work radius)` of a `[SOURCES]` sprite type.
pub type WorkSource = (u8, i32, i32);

/// Blocking polygons per sprite group and type index, and the deposits' resources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpriteShapes {
    pub groups: BTreeMap<SpriteGroup, Vec<Option<SpriteLock>>>,
    /// `[INTERNAL_SOURCE]` resource index per sprite group and type index.
    pub sources: BTreeMap<SpriteGroup, Vec<Option<usize>>>,
    /// `[SOURCES]`: `(source type, amount per work, radius)` per sprite group and type index.
    pub work: BTreeMap<SpriteGroup, Vec<Option<WorkSource>>>,
    /// `ObjCharacter`s (sources, work and time transforms) per sprite group.
    pub kinds: BTreeMap<SpriteGroup, crate::sprite::SpriteKinds>,
}

impl SpriteShapes {
    pub fn add(&mut self, group: SpriteGroup, list: &SpriteList, rsr: &SpriteResources) {
        let v = list
            .entries
            .iter()
            .map(|t| {
                let id = t.name.to_ascii_uppercase();
                rsr.locks.get(&id).map(|p| SpriteLock {
                    dx: t.dx,
                    dy: t.dy,
                    polygon: p.clone(),
                    source: rsr.sources.contains_key(&id),
                })
            })
            .collect();
        self.groups.insert(group, v);
        let sources = list
            .entries
            .iter()
            .map(|t| rsr.sources.get(&t.name.to_ascii_uppercase()).and_then(|s| crate::catalog::resource_index(&s.0)))
            .collect();
        self.sources.insert(group, sources);
        let work = list
            .entries
            .iter()
            .map(|t| {
                let (res, amount, radius) = rsr.work_sources.get(&t.name.to_ascii_uppercase())?;
                let kind = crate::catalog::resource_index(res)?;
                Some((kind as u8, *amount, *radius))
            })
            .collect();
        self.work.insert(group, work);
        self.kinds.insert(group, crate::sprite::SpriteKinds::new(list, rsr));
    }

    /// The spacing radius of a map sprite for new sprites (0x4d1aa0): the list radius of
    /// deposits, 0 for everything else.
    pub fn spacing(&self, group: SpriteGroup, index: u16) -> i32 {
        self.kinds
            .get(&group)
            .and_then(|k| k.chars.get(usize::from(index)))
            .filter(|c| c.deposit)
            .map_or(0, |c| c.list_radius)
    }

    /// `(source type, amount per work, radius)` of a gathering source sprite.
    pub fn work_source(&self, group: SpriteGroup, index: u16) -> Option<WorkSource> {
        *self.work.get(&group)?.get(usize::from(index))?
    }

    /// The resource a sprite supplies to mines, if it is a deposit.
    pub fn source(&self, group: SpriteGroup, index: u16) -> Option<usize> {
        *self.sources.get(&group)?.get(usize::from(index))?
    }

    fn get(&self, s: &Sprite) -> Option<&SpriteLock> {
        self.groups.get(&s.group)?.get(usize::from(s.index))?.as_ref()
    }
}

/// Even-odd point-in-polygon test of 0x4cfdc0 (edges with `x1 <= px < x2`, truncating
/// interpolation, counted when the edge lies below the point).
fn inside(poly: &[(i32, i32)], px: i32, py: i32) -> bool {
    let mut n = 0;
    for i in 0..poly.len() {
        let (mut a, mut b) = (poly[i], poly[(i + 1) % poly.len()]);
        if a.0 > b.0 {
            std::mem::swap(&mut a, &mut b);
        }
        if a.0 <= px && px < b.0 {
            let y = a.1 + (b.1 - a.1) * (px - a.0) / (b.0 - a.0);
            if y > py {
                n += 1;
            }
        }
    }
    n & 1 == 1
}

/// Rasterises a sprite's polygon into 16 px cells (0x4cfe90): the picture origin is
/// `(x − dx, y/2 − dy)` with polygon y at half scale; a cell is blocked when its centre
/// `(16cx + 8, 8cy + 4)` lies inside.
fn lock_sprite(m: &mut LockMap, s: &Sprite, l: &SpriteLock) {
    let (x0, y0) = (s.x - l.dx, (s.y >> 1) - l.dy);
    let poly: Vec<(i32, i32)> = l.polygon.iter().map(|&(x, y)| (x0 + x, y0 + y)).collect();
    let (minx, maxx) = (poly.iter().map(|p| p.0).min().unwrap_or(0), poly.iter().map(|p| p.0).max().unwrap_or(0));
    let (miny, maxy) = (poly.iter().map(|p| p.1).min().unwrap_or(0), poly.iter().map(|p| p.1).max().unwrap_or(0));
    for cx in (minx >> 4) - 4..=(maxx >> 4) + 4 {
        for cy in (miny >> 3) - 4..=(maxy >> 3) + 4 {
            if inside(&poly, 16 * cx + 8, 8 * cy + 4) {
                m.set_pt(cx, cy);
            }
        }
    }
}

/// The static map of a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapGrid {
    pub shift: u8,
    pub heights: Heights,
    /// Passability for land units (`MFIELDS[0]`).
    pub land: LockMap,
    /// Passability for ships (`MFIELDS[1]`).
    pub water: LockMap,
    /// `TOP1`: land pathfinding areas, if the map has them.
    pub land_areas: Option<AreaGraph>,
}

/// `TOP1`/`TOPW` in the form the router uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaGraph {
    pub count: usize,
    /// Area centres in area cells.
    pub centers: Vec<(i32, i32)>,
    /// `next[from * count + to]`, 0xFFFF = unreachable.
    pub next: Vec<u16>,
    /// `LinksDist[from * count + to]` (0x28a72bc): path length between areas.
    pub dist: Vec<u16>,
    pub cells: Grid<u16>,
}

impl AreaGraph {
    pub fn from_areas(a: &Areas) -> Self {
        Self {
            count: a.areas.len(),
            centers: a.areas.iter().map(|r| (i32::from(r.x), i32::from(r.y))).collect(),
            next: a.next_hop.clone(),
            dist: a.distance.clone(),
            cells: a.cells.clone(),
        }
    }

    /// Area of a 64 px cell (≥ 0xFFFE = none).
    pub fn area_at(&self, cx: i32, cy: i32) -> u16 {
        if cx < 0 || cy < 0 {
            return 0xFFFF;
        }
        self.cells.get(cx as usize, cy as usize).unwrap_or(0xFFFF)
    }

    pub fn next_hop(&self, from: u16, to: u16) -> u16 {
        let (f, t) = (usize::from(from), usize::from(to));
        if f >= self.count || t >= self.count {
            return 0xFFFF;
        }
        self.next.get(f * self.count + t).copied().unwrap_or(0xFFFF)
    }
}

impl MapGrid {
    /// Builds the map like `CreateLockMap` (0x44e340) followed by the resource-sprite pass
    /// (0x44e250). Buildings are locked by the world when it places them.
    pub fn from_m3d(map: &M3dMap, sprites: &SpriteShapes) -> Self {
        let shift = map.size.map_or(1, |s| s.shift);
        let side = 512 << shift; // lock cells per side (map side / 16 px)
        let heights = match &map.heights {
            Some(g) => Heights { stride: g.width, cells: g.cells.clone(), shift },
            None => Heights { stride: (256 << shift) + 1, cells: Vec::new(), shift },
        };
        let mut grid = MapGrid {
            shift,
            heights,
            land: LockMap::new(side, side),
            water: LockMap::new(side, side),
            land_areas: map.land_areas.as_ref().map(AreaGraph::from_areas),
        };
        grid.lock_by_terrain(map, sprites);
        if let Some(w) = &map.water {
            grid.water_lock(&w.depth);
        }
        grid.lock_borders();
        for s in &map.sprites {
            if let Some(l) = sprites.get(s).filter(|l| l.source) {
                lock_sprite(&mut grid.land, s, l);
            }
        }
        grid
    }

    /// A flat land map without areas, with the engine's border locks (for tests and tools).
    pub fn open(shift: u8) -> Self {
        let side = 512 << shift;
        let stride = (256usize << shift) + 1;
        let mut grid = MapGrid {
            shift,
            heights: Heights { stride, cells: vec![0; stride * (256 << shift)], shift },
            land: LockMap::new(side, side),
            water: LockMap::new(side, side),
            land_areas: None,
        };
        grid.water.set_rect(0, 0, side, side);
        grid.lock_borders();
        grid
    }

    /// Playable side in 32 px units (`[0xbe0720]`, 480 for a normal map).
    fn half_cells(&self) -> i32 {
        240 << self.shift
    }

    /// `LockByTerrain` (0x44d800).
    fn lock_by_terrain(&mut self, map: &M3dMap, sprites: &SpriteShapes) {
        let n = self.half_cells() / 2; // 64 px cells per side
        let land = &mut self.land;
        // (a) Slope: a 64 px cell is blocked when a corner deviates ≥ 23 from the corners' mean.
        for cx in 0..n {
            for cy in (0..n).rev() {
                let h = [
                    self.heights.at(64 * cx, 64 * cy),
                    self.heights.at(64 * cx + 64, 64 * cy),
                    self.heights.at(64 * cx, 64 * cy + 64),
                    self.heights.at(64 * cx + 64, 64 * cy + 64),
                ];
                let avg = h.iter().sum::<i32>() >> 2;
                if h.iter().any(|&v| (v - avg).abs() >= 23) {
                    land.set_bar(4 * cx, 4 * cy, 4);
                } else {
                    land.clr_bar(4 * cx, 4 * cy, 4);
                }
            }
        }
        // (b) Blocking sprites (rocks); mines follow after the quantisation.
        for s in &map.sprites {
            if let Some(l) = sprites.get(s).filter(|l| !l.source) {
                lock_sprite(land, s, l);
            }
        }
        // (c) LOC1 overrides on 64 px cells: first unlock, then lock.
        if let Some(locks) = &map.locks {
            for &(x, y) in &locks.unlocked {
                land.clr_bar(4 * i32::from(x), 4 * i32::from(y), 4);
            }
            for &(x, y) in &locks.locked {
                land.set_bar(4 * i32::from(x), 4 * i32::from(y), 4);
            }
        }
        // (e) Quantise to 64 px: blocked iff ≥ 3 of the four 32 px quadrants have a lock.
        for ax in 1..n - 1 {
            for ay in 1..n - 1 {
                let (x, y) = (4 * ax, 4 * ay);
                let q = [land.any2(x, y), land.any2(x + 2, y), land.any2(x, y + 2), land.any2(x + 2, y + 2)];
                if q.iter().filter(|&&b| b).count() >= 3 {
                    land.set_bar(x, y, 4);
                } else {
                    land.clr_bar(x, y, 4);
                }
            }
        }
        // (f) Chamfer free blocks next to two adjacent blocked sides; (g) the same for blocked blocks.
        for blocked_pass in [false, true] {
            let copy = land.clone();
            for ax in 1..n - 1 {
                for ay in 1..n - 1 {
                    let (x, y) = (4 * ax, 4 * ay);
                    if copy.any2(x + 1, y + 1) != blocked_pass {
                        continue;
                    }
                    let m = u8::from(copy.any2(x + 1, y - 3))
                        | u8::from(copy.any2(x + 5, y + 1)) << 1
                        | u8::from(copy.any2(x + 1, y + 5)) << 2
                        | u8::from(copy.any2(x - 3, y + 1)) << 3;
                    let pattern = match m {
                        3 => Some(P_UR),
                        6 => Some(P_LR),
                        9 => Some(P_UL),
                        12 => Some(P_LL),
                        7 | 11 | 13 | 14 if !blocked_pass => {
                            land.set_bar(x, y, 4);
                            None
                        }
                        _ => None,
                    };
                    if let Some(p) = pattern {
                        for (row, line) in p.iter().enumerate() {
                            for (col, &c) in line.iter().enumerate() {
                                let (px, py) = (x + col as i32, y + row as i32);
                                if c == b'*' { land.set_pt(px, py) } else { land.clr_pt(px, py) }
                            }
                        }
                    }
                }
            }
        }
    }

    /// `WaterLock` (0x53c632): the `SEA2` depth (32 px cells) is sampled at the four 16 px
    /// sub-cells with weights 9/3/3/1; deep water blocks land units and frees ships.
    fn water_lock(&mut self, depth: &Grid<u8>) {
        let d = |x: i32, y: i32| -> i32 {
            if x < 0 || y < 0 {
                return 0;
            }
            i32::from(depth.get(x as usize, y as usize).unwrap_or(0))
        };
        let f = |p: i32, q: i32, r: i32, s: i32| (3 * (3 * p + q + r) + s) >> 4;
        let n = self.land.width / 2;
        for y in 0..n - 1 {
            for x in 0..n - 1 {
                let (a, b, c, e) = (d(x, y), d(x + 1, y), d(x, y + 1), d(x + 1, y + 1));
                let sub = [(0, 0, f(a, b, c, e)), (1, 0, f(b, e, a, c)), (0, 1, f(c, e, a, b)), (1, 1, f(e, c, b, a))];
                for (i, j, v) in sub {
                    let (px, py) = (2 * x + i, 2 * y + j);
                    if v > 130 {
                        self.water.clr_pt(px, py);
                    } else {
                        self.water.set_pt(px, py);
                    }
                    if v > 128 {
                        self.land.set_pt(px, py);
                    }
                }
            }
        }
    }

    /// Map borders and the top edge, where high terrain would be drawn above the screen top.
    fn lock_borders(&mut self) {
        let half = self.half_cells();
        let (w2, h2) = (half * 2, half * 2);
        let shift = i32::from(self.shift);
        for layer in [&mut self.land, &mut self.water] {
            layer.set_rect(0, 0, w2, 4);
            layer.set_rect(0, h2, w2, 32 << shift);
            layer.set_rect(0, 4, 4, h2 - 4);
            layer.set_rect(w2, 0, 32 << shift, 512 << shift);
        }
        for ax in 0..half / 2 {
            for ay in 1..25 {
                if ay * 32 - self.heights.at(ax * 64 + 32, ay * 64) < 16 {
                    self.land.set_bar(4 * ax, 4 * ay, 4);
                    self.water.set_bar(4 * ax, 4 * ay, 4);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(shift: u8, h: i16) -> Heights {
        let stride = (256usize << shift) + 1;
        Heights { stride, cells: vec![h; stride * (256 << shift)], shift }
    }

    #[test]
    fn flat_terrain_has_flat_heights() {
        let h = flat(1, 40);
        for (x, y) in [(0, 0), (100, 77), (33, 1000), (5000, 4321)] {
            assert_eq!(h.at(x, y), 40);
        }
    }

    #[test]
    fn height_interpolates_on_a_ramp() {
        // Height = 2 * column index: rises 2 per 32 px eastwards.
        let mut h = flat(1, 0);
        for (i, c) in h.cells.iter_mut().enumerate() {
            *c = (2 * (i % h.stride)) as i16;
        }
        assert_eq!(h.at(64, 300), 4);
        assert_eq!(h.at(80, 300), 5);
    }

    #[test]
    fn sprite_polygon_locks_cells_whose_centre_is_inside() {
        let mut m = LockMap::new(64, 64);
        // Square picture polygon 0..64 x 0..32 (half-scale y), hotspot (0, 0), sprite at (160, 320):
        // origin (160, 160) -> x 160..224, y 160..192 in half-scale units = 16 px cells 10..13, 20..23.
        let l = SpriteLock { dx: 0, dy: 0, polygon: vec![(0, 0), (64, 0), (64, 32), (0, 32)], source: false };
        let s = Sprite { group: SpriteGroup::Stones, x: 160, y: 320, index: 0, extra: None };
        lock_sprite(&mut m, &s, &l);
        assert_eq!(m.count(), 16);
        assert!(m.check_pt(10, 20) && m.check_pt(13, 23) && !m.check_pt(14, 20) && !m.check_pt(10, 24));
    }

    #[test]
    fn lock_map_edges() {
        let mut m = LockMap::new(64, 64);
        assert!(m.check_pt(-1, 0) && m.check_pt(64, 0));
        assert!(!m.check_pt(0, 0));
        assert!(m.check_vline(0, 5, 1), "x = 0 counts as blocked for line checks");
        assert!(!m.check_bar(5, 5, 2, 2));
        m.set_bar(6, 6, 1);
        assert!(m.check_bar(5, 5, 2, 2));
        assert!(m.check_vline(10, 10, 25));
    }
}
