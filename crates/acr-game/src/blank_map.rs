//! The blank test field of the driver (`docs/testing.md`, "The blank field"). The original
//! editor's new map (`MapAutosave.m3d` right after "new map") is a `3DMP` with every `SURF`
//! height 0, every `TILE` texture 0 (the green field), an empty `TRE1`, no units on the
//! ground and a `SEA2` of zeros. [`build`] makes the same map in code, optionally with one
//! other ground texture, a ramp or a hill for gait tests and a rectangle of water for ships.

use acr_formats::M3dMap;
use acr_formats::m3d::{Grid, MapSize, Water};

use crate::ui::setup::{self, BlankSetup, Ground};

/// `TILES3.BMP` textures of the three grounds: 0 is the editor's green field.
pub const fn texture(ground: Ground) -> u8 {
    match ground {
        Ground::Grass | Ground::White | Ground::Greenscreen | Ground::Sea => 0,
        Ground::Dirt => DIRT,
        Ground::Snow => SNOW,
    }
}

/// The flat colour drawn instead of the ground (`white`, `greenscreen`), as linear RGB.
pub const fn backdrop(ground: Ground) -> Option<[f64; 3]> {
    match ground {
        Ground::White => Some([1.0, 1.0, 1.0]),
        Ground::Greenscreen => Some([0.0, 1.0, 0.0]),
        Ground::Grass | Ground::Dirt | Ground::Snow | Ground::Sea => None,
    }
}

/// Each player's stock on the blank field: 1 000 000 of every resource.
pub const BLANK_STOCK: [u32; 8] = [1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 0, 0];

/// Bare brown earth (`TILES3.BMP` texture 71).
const DIRT: u8 = 71;
/// Snow (texture 103; 110 is plain white).
const SNOW: u8 = 103;

/// `SEA2` depth of open water (deeper than the 130 at which ships float and land units stop).
const DEEP: i32 = 200;

/// The size shift of the menu's map size (normal = `3DMP`, the editor's new map).
pub fn shift(size: setup::MapSize) -> u8 {
    match size {
        setup::MapSize::Normal => 1,
        setup::MapSize::Large => 2,
        setup::MapSize::Huge => 3,
    }
}

/// Side of the playable area in world pixels (15360 for a normal map); its middle is where
/// the driver's scripts put their units.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn playable(size: setup::MapSize) -> i32 {
    MapSize { shift: shift(size) }.playable_size()
}

/// The map of a blank field.
pub fn build(b: &BlankSetup) -> M3dMap {
    let size = MapSize { shift: shift(b.size) };
    let (vw, vh) = (size.vertex_width(), size.vertex_height());
    let mut heights = vec![0i16; vw * vh];
    for (v, h) in heights.iter_mut().enumerate() {
        let (col, row) = ((v % vw) as i32, (v / vw) as i32);
        let (x, y) = (col << 5, (row << 5) - ((col & 1) << 4));
        *h = height(b, x, y).clamp(0.0, f64::from(i16::MAX)).round() as i16;
    }
    // Water on the 32 px grid of the sea (`(240 << s) + 2` cells a side).
    let n = ((240usize) << size.shift) + 2;
    let mut depth = vec![0u8; n * n];
    let mut brightness = vec![0u8; n * n];
    // The sea ground is open water to the edge of the map: ships sail everywhere, land units
    // stop at once (the same layers as the sea battle of the random map setup, #408).
    if b.ground == Ground::Sea {
        let w = acr_mapgen::open_water(size.shift);
        (depth, brightness) = (w.depth.cells, w.brightness.cells);
    } else if let Some((wx, wy, ww, wh)) = b.water {
        let (x0, y0, x1, y1) = (wx >> 5, wy >> 5, (wx + ww) >> 5, (wy + wh) >> 5);
        for cy in y0.max(0)..=y1.min(n as i32 - 1) {
            for cx in x0.max(0)..=x1.min(n as i32 - 1) {
                // A shore of a few cells: shallow at the edge, open water inside.
                let edge = (cx - x0).min(x1 - cx).min(cy - y0).min(y1 - cy);
                let i = cy as usize * n + cx as usize;
                depth[i] = (120 + 16 * edge).min(DEEP) as u8;
                brightness[i] = 0x60;
            }
        }
    }
    M3dMap {
        size: Some(size),
        header: [vw as u32, vh as u32],
        heights: Some(Grid { width: vw, height: vh, cells: heights }),
        textures: Some(Grid { width: vw, height: vh, cells: textures(b, vw, vw * vh) }),
        water: Some(Water {
            depth: Grid { width: n, height: n, cells: depth },
            brightness: Grid { width: n, height: n, cells: brightness },
        }),
        // Plenty of everything: with no food (the editor's new map has none) the players
        // starve: each unit dies with probability 51/32768 per tick (`economy::starve`); a
        // test scene of 120 musketeers lost 26 of them in 400 ticks.
        resources: Some([BLANK_STOCK; acr_formats::m3d::PLAYERS]),
        mission_dll: Some(String::new()),
        ..M3dMap::default()
    }
}

/// The vertex textures: the field's ground, then the patches in order.
fn textures(b: &BlankSetup, vw: usize, len: usize) -> Vec<u8> {
    let mut out = vec![texture(b.ground); len];
    for (v, t) in out.iter_mut().enumerate() {
        let (col, row) = ((v % vw) as i32, (v / vw) as i32);
        let (x, y) = (col << 5, (row << 5) - ((col & 1) << 4));
        for &(px, py, r, tex) in &b.patches {
            let (dx, dy) = (i64::from(x - px), i64::from(y - py));
            if dx * dx + dy * dy <= i64::from(r) * i64::from(r) {
                *t = tex;
            }
        }
    }
    out
}

/// Ground height (world pixels) at `(x, y)`: the ramp plus the hill plus the bumps.
fn height(b: &BlankSetup, x: i32, y: i32) -> f64 {
    let mut h = 0.0;
    if b.rough > 0 {
        let k = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)).wrapping_mul(0x2C1B_3C6D) as u32 >> 16;
        h += f64::from(b.rough) * f64::from(k) / 32767.5;
    }
    if let Some((tenths, x0, x1)) = b.slope {
        let run = f64::from(x.clamp(x0.min(x1), x1.max(x0)) - x0.min(x1));
        h += run * (f64::from(tenths) / 10.0).to_radians().tan();
    }
    if let Some((hx, hy, r, top)) = b.hill {
        let d = f64::from(x - hx).hypot(f64::from(y - hy));
        if r > 0 && d < f64::from(r) {
            h += f64::from(top) * 0.5 * (1.0 + (std::f64::consts::PI * d / f64::from(r)).cos());
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_map_is_flat_green_and_empty() {
        let map = build(&BlankSetup::default());
        assert_eq!(map.size, Some(MapSize { shift: 1 }));
        assert_eq!(map.header, [513, 512]);
        assert!(map.heights.as_ref().unwrap().cells.iter().all(|&h| h == 0));
        assert!(map.textures.as_ref().unwrap().cells.iter().all(|&t| t == 0));
        assert!(map.sprites.is_empty() && map.units.is_empty());
        assert!(map.water.as_ref().unwrap().depth.cells.iter().all(|&d| d == 0));
        // Food, so nobody starves in a test scene.
        assert!(map.resources.unwrap().iter().all(|r| r[3] > 0));
        assert_eq!(playable(setup::MapSize::Normal), 15360);
    }

    #[test]
    fn slope_hill_and_water() {
        let b = BlankSetup {
            size: setup::MapSize::Normal,
            ground: Ground::Snow,
            slope: Some((50, 1000, 2000)),
            hill: Some((5000, 5000, 400, 100)),
            water: Some((8000, 8000, 640, 640)),
            patches: vec![(5000, 5000, 100, 71)],
            rough: 0,
            editor: false,
        };
        let map = build(&b);
        let heights = map.heights.as_ref().unwrap();
        // 5 degrees over 1000 px: 87 px at the top of the ramp, flat behind it.
        assert_eq!(heights.get(1000 / 32, 10), Some(0));
        assert_eq!(heights.get(2000 / 32 + 2, 10), Some(87));
        // The hill stands on the plateau behind the ramp.
        assert_eq!(heights.get(5000 / 32, 5000 / 32), Some(87 + 100));
        // Snow everywhere but the dirt patch of radius 100 at (5000, 5000).
        let textures = map.textures.as_ref().unwrap();
        assert_eq!(textures.get(5000 / 32, 5000 / 32), Some(71));
        assert_eq!(textures.get(5200 / 32, 5000 / 32), Some(SNOW));
        assert_eq!(
            textures.cells.iter().filter(|&&t| t != SNOW).count(),
            textures.cells.iter().filter(|&&t| t == 71).count()
        );
        let depth = &map.water.as_ref().unwrap().depth;
        assert_eq!(depth.get(260, 260), Some(DEEP as u8));
        assert_eq!(depth.get(250, 260), Some(120));
        assert_eq!(depth.get(240, 260), Some(0));
    }

    #[test]
    fn the_sea_ground_is_open_water_to_the_edge() {
        let map = build(&BlankSetup { ground: Ground::Sea, ..Default::default() });
        let depth = &map.water.as_ref().unwrap().depth;
        assert!(depth.cells.iter().all(|&d| d == DEEP as u8));
        assert!(map.units.is_empty());
        assert!(map.heights.as_ref().unwrap().cells.iter().all(|&h| h == 0));
    }
}
