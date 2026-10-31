//! The lock derivation of `MapGrid::from_m3d` on a synthetic map: slopes, a rock, a mine,
//! `LOC1` overrides and a lake, pinned by a hash of both bitmaps.

use acr_formats::m3d::{Grid, Locks, M3dMap, MapSize, Sprite, SpriteGroup, Water};
use acr_sim::MapGrid;
use acr_sim::map::{LockMap, SpriteLock, SpriteShapes};

const ROCK: u16 = 0;
const MINE: u16 = 1;

/// Rock and mine polygons (picture pixels at half scale, hotspot (0, 0)).
fn shapes() -> SpriteShapes {
    let square = |w, h, source| SpriteLock { dx: 0, dy: 0, polygon: vec![(0, 0), (w, 0), (w, h), (0, h)], source };
    let mut s = SpriteShapes::default();
    s.groups.insert(SpriteGroup::Stones, vec![Some(square(96, 64, false)), Some(square(64, 40, true))]);
    s
}

fn sprite(index: u16, x: i32, y: i32) -> Sprite {
    Sprite { group: SpriteGroup::Stones, x, y, index, extra: None }
}

/// A rolling map with a steep ridge, a rock, a mine, `LOC1` overrides and a lake.
fn map() -> M3dMap {
    let size = MapSize { shift: 1 };
    let (w, h) = (size.vertex_width(), size.vertex_height());
    let mut heights = Grid { width: w, height: h, cells: vec![0i16; w * h] };
    for row in 0..h {
        for col in 0..w {
            let ridge = if (100..108).contains(&col) { 40 * (col as i16 - 99) } else { 0 };
            heights.cells[row * w + col] = 30 + ((col * 7 + row * 3) % 11) as i16 + ridge;
        }
    }
    let side = (size.playable_size() / 32) as usize + 2;
    let mut depth = Grid { width: side, height: side, cells: vec![0u8; side * side] };
    for y in 200..300 {
        for x in 60..160 {
            let edge = (x - 60).min(159 - x).min(y - 200).min(299 - y);
            depth.cells[y * side + x] = (110 + 6 * edge).min(220) as u8;
        }
    }
    let brightness = Grid { width: side, height: side, cells: vec![0u8; side * side] };
    M3dMap {
        size: Some(size),
        heights: Some(heights),
        sprites: vec![sprite(ROCK, 3000, 6000), sprite(MINE, 8000, 3000), sprite(ROCK, 12000, 12000)],
        locks: Some(Locks { locked: vec![(30, 130), (31, 130), (32, 130)], unlocked: vec![(100, 100), (3, 3)] }),
        water: Some(Water { depth, brightness }),
        ..M3dMap::default()
    }
}

/// FNV-1a over the bits of a layer, row by row.
fn hash(layer: &LockMap) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for y in 0..layer.height {
        for x in 0..layer.width {
            h = (h ^ u64::from(layer.check_pt(x, y))).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

#[test]
fn from_m3d_is_unchanged() {
    let g = MapGrid::from_m3d(&map(), &shapes());
    let (land, water) = (hash(&g.land), hash(&g.water));
    assert_eq!((g.land.count(), g.water.count()), (LAND_COUNT, WATER_COUNT));
    assert_eq!((land, water), (LAND_HASH, WATER_HASH), "land {land:#x}, water {water:#x}");
}

const LAND_COUNT: usize = 187426;
const WATER_COUNT: usize = 1014720;
const LAND_HASH: u64 = 0xdc9f_f524_0db3_7355;
const WATER_HASH: u64 = 0xfa36_5874_160d_ffe5;

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self, n: u32) -> i32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as i32 % n as i32
    }
}

#[test]
fn region_rederive_equals_full_derivation() {
    let shapes = shapes();
    let mut map = map();
    let mut grid = MapGrid::from_m3d(&map, &shapes);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let size = map.size.expect("size");
    let (w, side) = (size.vertex_width() as i32, size.playable_size() + 64);
    for step in 0..50 {
        // Centres cover the borders, the top edge, the ridge, the lake and the rest.
        let (cx, cy) = match step % 5 {
            0 => (rng.next(side as u32), rng.next(1800)),
            1 => (3200 + rng.next(400), rng.next(side as u32)),
            2 => (1900 + rng.next(3000), 6400 + rng.next(3200)),
            3 => (rng.next(600), rng.next(side as u32)),
            _ => (rng.next(side as u32), rng.next(side as u32)),
        };
        let r = 64 + rng.next(500);
        let area = [cx - r, cy - r, cx + r, cy + r];
        let heights = map.heights.as_mut().expect("heights");
        let plateau = rng.next(3) == 0;
        let level = rng.next(400) as i16;
        for row in 0..heights.height as i32 {
            for col in 0..w {
                let (x, y) = (32 * col, 32 * row - 16 * (col & 1));
                if x >= area[0] && x < area[2] && y >= area[1] && y < area[3] {
                    heights.cells[(row * w + col) as usize] = if plateau { level } else { rng.next(400) as i16 };
                }
            }
        }
        let depth = &mut map.water.as_mut().expect("water").depth;
        let dv = [0, 0, 120, 131, 200, 255][rng.next(6) as usize] as u8;
        for y in (area[1] >> 5).max(0)..=(area[3] >> 5) {
            for x in (area[0] >> 5).max(0)..=(area[2] >> 5) {
                if let Some(c) =
                    depth.cells.get_mut(y as usize * depth.width + x as usize).filter(|_| x < depth.width as i32)
                {
                    *c = if rng.next(4) == 0 { dv.saturating_sub(rng.next(60) as u8) } else { dv };
                }
            }
        }
        if step % 3 == 0 {
            map.sprites.push(sprite(rng.next(2) as u16, cx.clamp(64, side - 64), cy.clamp(64, side - 64)));
        }
        // Sprites reach about 400 px around their position.
        let area = [area[0].min(cx - 400), area[1].min(cy - 400), area[2].max(cx + 400), area[3].max(cy + 400)];
        grid.copy_terrain(&map, area);
        grid.rederive(&map, &shapes, area);
        let full = MapGrid::from_m3d(&map, &shapes);
        assert!(grid.land == full.land, "land differs after edit {step} at {area:?}");
        assert!(grid.water == full.water, "water differs after edit {step} at {area:?}");
        assert!(grid.heights == full.heights && grid.water_depth == full.water_depth);
    }
}
