//! The terrain brush of the map editor (#430), an ACR addition: the original editor's
//! terrain tools are untraced, so these are ACR's own. A [`Dab`] is one round stamp on the
//! map (`SURF` heights, `TILE` textures, `SEA2` water); a stroke is a run of dabs.
//!
//! Integer maths only, so the same dabs give the same map on every platform. Vertex
//! `(col, row)` lies at world pixel `(32·col, 32·row − 16·(col & 1))`, the `SEA2` cell
//! `(cx, cy)` at `(32·cx, 32·cy)` (as `docs/ground.md` and the blank field place them).

use acr_formats::M3dMap;
use acr_formats::m3d::Grid;

/// The brush radii the editor offers, in world pixels.
pub const RADII: std::ops::RangeInclusive<i32> = 16..=512;
/// Pixels one Raise or Lower dab moves the ground at its centre.
pub const RAISE: i16 = 8;
/// `SEA2` depth at the edge of painted water (shallow: land units still wade).
pub const SHORE: u8 = 120;
/// `SEA2` depth of open water (deeper than the 130 at which ships float).
pub const DEEP: u8 = 200;
/// Depth gained per 32 px of distance from the edge of a painted pool.
pub const SHORE_STEP: i32 = 16;
/// `SEA2` brightness of painted water (as the blank field's).
const BRIGHT: u8 = 0x60;
/// `TILES3.BMP` has 136 textures.
const TEXTURES: u8 = 136;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// Lifts the ground by up to this many pixels at the centre.
    Raise(i16),
    /// Sinks the ground by up to this many pixels at the centre (never below 0).
    Lower(i16),
    /// Pulls the ground towards this height.
    Flatten(i16),
    /// Paints this ground texture over the disc.
    Texture(u8),
    /// Floods the disc: deep in the middle, a shore at the edge; never makes water shallower.
    Water,
    /// Dries the disc.
    Land,
}

/// One stamp: centre and radius in world pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dab {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
    pub tool: Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    /// The map has no `SURF` or no `TILE`.
    NoTerrain,
    /// Water or Land on a map without `SEA2`.
    NoSea,
    /// The map has `TOP1`/`TOPW` area tables, which an edit would leave stale.
    Areas,
    /// The radius is outside [`RADII`].
    Radius,
    /// The texture is not one of `TILES3.BMP`'s.
    Texture,
}

/// Applies `dab`. Returns the changed box `[x0, y0, x1, y1)` in world pixels, grown by 32
/// px so that the neighbours whose lighting or blending read the change are inside it, or
/// `None` when nothing changed.
pub fn apply(map: &mut M3dMap, dab: &Dab) -> Result<Option<[i32; 4]>, EditError> {
    if !RADII.contains(&dab.radius) {
        return Err(EditError::Radius);
    }
    if matches!(dab.tool, Tool::Texture(t) if t >= TEXTURES) {
        return Err(EditError::Texture);
    }
    if map.land_areas.is_some() || map.water_areas.is_some() {
        return Err(EditError::Areas);
    }
    let mut bounds = Bounds::default();
    match dab.tool {
        Tool::Water | Tool::Land => {
            let water = map.water.as_mut().ok_or(EditError::NoSea)?;
            paint_water(&mut water.depth, &mut water.brightness, dab, &mut bounds);
        }
        _ => {
            let (Some(heights), Some(textures)) = (map.heights.as_mut(), map.textures.as_mut()) else {
                return Err(EditError::NoTerrain);
            };
            paint_ground(heights, textures, dab, &mut bounds);
        }
    }
    Ok(bounds.grown(32))
}

/// The box of what changed, in world pixels.
#[derive(Default)]
struct Bounds(Option<[i32; 4]>);

impl Bounds {
    /// Adds the square `[x, x + size) × [y, y + size)`.
    fn add(&mut self, x: i32, y: i32, size: i32) {
        let b = self.0.get_or_insert([x, y, x + size, y + size]);
        *b = [b[0].min(x), b[1].min(y), b[2].max(x + size), b[3].max(y + size)];
    }

    fn grown(&self, by: i32) -> Option<[i32; 4]> {
        self.0.map(|[x0, y0, x1, y1]| [x0 - by, y0 - by, x1 + by, y1 + by])
    }
}

fn paint_ground(heights: &mut Grid<i16>, textures: &mut Grid<u8>, dab: &Dab, bounds: &mut Bounds) {
    let r = i64::from(dab.radius);
    let r2 = r * r;
    let (w, h) = (heights.width.min(textures.width), heights.height.min(textures.height));
    let cols = (dab.x - dab.radius).div_euclid(32).max(0)..=(dab.x + dab.radius).div_euclid(32) + 1;
    for col in cols {
        if col as usize >= w {
            break;
        }
        let shift = 16 * (col & 1);
        let rows = (dab.y - dab.radius + shift).div_euclid(32).max(0)..=(dab.y + dab.radius + shift).div_euclid(32) + 1;
        for row in rows {
            if row as usize >= h {
                break;
            }
            let (vx, vy) = (32 * col, 32 * row - shift);
            let (dx, dy) = (i64::from(vx - dab.x), i64::from(vy - dab.y));
            let d2 = dx * dx + dy * dy;
            if d2 > r2 {
                continue;
            }
            let weight = (r2 - d2) * 256 / r2;
            let i = row as usize * heights.width + col as usize;
            let before = heights.cells[i];
            let after = match dab.tool {
                Tool::Raise(a) => i64::from(before) + i64::from(a) * weight / 256,
                Tool::Lower(a) => i64::from(before) - i64::from(a) * weight / 256,
                Tool::Flatten(level) => i64::from(before) + (i64::from(level) - i64::from(before)) * weight / 256,
                Tool::Texture(t) => {
                    let j = row as usize * textures.width + col as usize;
                    if textures.cells[j] != t {
                        textures.cells[j] = t;
                        bounds.add(vx, vy, 1);
                    }
                    continue;
                }
                Tool::Water | Tool::Land => unreachable!("water tools take the sea path"),
            };
            let after = after.clamp(0, i64::from(i16::MAX)) as i16;
            if after != before {
                heights.cells[i] = after;
                bounds.add(vx, vy, 1);
            }
        }
    }
}

fn paint_water(depth: &mut Grid<u8>, brightness: &mut Grid<u8>, dab: &Dab, bounds: &mut Bounds) {
    let r = i64::from(dab.radius);
    let (w, h) = (depth.width.min(brightness.width), depth.height.min(brightness.height));
    let cols = (dab.x - dab.radius).div_euclid(32).max(0)..=(dab.x + dab.radius).div_euclid(32) + 1;
    for cx in cols {
        if cx as usize >= w {
            break;
        }
        let rows = (dab.y - dab.radius).div_euclid(32).max(0)..=(dab.y + dab.radius).div_euclid(32) + 1;
        for cy in rows {
            if cy as usize >= h {
                break;
            }
            let (dx, dy) = (i64::from(32 * cx - dab.x), i64::from(32 * cy - dab.y));
            let d2 = dx * dx + dy * dy;
            if d2 > r * r {
                continue;
            }
            let (i, j) = (cy as usize * depth.width + cx as usize, cy as usize * brightness.width + cx as usize);
            let (d, b) = match dab.tool {
                Tool::Water => {
                    let edge = (r - d2.isqrt()) as i32 / 32;
                    let d = (i32::from(SHORE) + SHORE_STEP * edge).min(i32::from(DEEP)) as u8;
                    (depth.cells[i].max(d), BRIGHT)
                }
                _ => (0, 0),
            };
            if (depth.cells[i], brightness.cells[j]) != (d, b) {
                depth.cells[i] = d;
                brightness.cells[j] = b;
                bounds.add(32 * cx, 32 * cy, 32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acr_formats::m3d::{MapSize, Water};

    /// A flat map of size shift 1 (513 × 512 vertices, 482 × 482 sea cells).
    fn flat() -> M3dMap {
        let size = MapSize { shift: 1 };
        let (vw, vh) = (size.vertex_width(), size.vertex_height());
        let n = (240usize << 1) + 2;
        M3dMap {
            size: Some(size),
            header: [vw as u32, vh as u32],
            heights: Some(Grid { width: vw, height: vh, cells: vec![0; vw * vh] }),
            textures: Some(Grid { width: vw, height: vh, cells: vec![0; vw * vh] }),
            water: Some(Water {
                depth: Grid { width: n, height: n, cells: vec![0; n * n] },
                brightness: Grid { width: n, height: n, cells: vec![0; n * n] },
            }),
            ..M3dMap::default()
        }
    }

    fn dab(x: i32, y: i32, radius: i32, tool: Tool) -> Dab {
        Dab { x, y, radius, tool }
    }

    fn height(map: &M3dMap, col: usize, row: usize) -> i16 {
        map.heights.as_ref().unwrap().get(col, row).unwrap()
    }

    #[test]
    fn raise_peaks_at_the_centre_and_fades() {
        let mut map = flat();
        // Vertex (20, 20) is at (640, 640): even column, no stagger.
        apply(&mut map, &dab(640, 640, 128, Tool::Raise(RAISE))).unwrap();
        assert_eq!(height(&map, 20, 20), RAISE);
        // One vertex away: d = 32, w = (128² − 32²)·256 / 128² = 240, 8·240/256 = 7.
        assert_eq!(height(&map, 21, 20), 7);
        assert_eq!(height(&map, 20, 21), 7);
        // At and beyond the radius nothing moves.
        assert_eq!(height(&map, 24, 20), 0);
        assert_eq!(height(&map, 25, 20), 0);
        assert_eq!(height(&map, 20, 15), 0);
    }

    #[test]
    fn lower_stops_at_zero() {
        let mut map = flat();
        apply(&mut map, &dab(640, 640, 64, Tool::Raise(3))).unwrap();
        for _ in 0..5 {
            apply(&mut map, &dab(640, 640, 64, Tool::Lower(RAISE))).unwrap();
        }
        assert!(map.heights.as_ref().unwrap().cells.iter().all(|&h| h == 0));
        // Nothing left to lower: no change, no box.
        assert_eq!(apply(&mut map, &dab(640, 640, 64, Tool::Lower(RAISE))), Ok(None));
    }

    #[test]
    fn flatten_reaches_the_level_at_the_centre() {
        let mut map = flat();
        apply(&mut map, &dab(640, 640, 96, Tool::Flatten(100))).unwrap();
        assert_eq!(height(&map, 20, 20), 100);
        let edge = height(&map, 22, 20);
        assert!(0 < edge && edge < 100, "{edge}");
        assert_eq!(height(&map, 23, 20), 0);
        // Flatten down again from a hill.
        apply(&mut map, &dab(640, 640, 96, Tool::Flatten(0))).unwrap();
        assert_eq!(height(&map, 20, 20), 0);
    }

    #[test]
    fn texture_paints_the_disc_only() {
        let mut map = flat();
        apply(&mut map, &dab(640, 640, 64, Tool::Texture(71))).unwrap();
        let t = map.textures.as_ref().unwrap();
        assert_eq!(t.get(20, 20), Some(71));
        assert_eq!(t.get(22, 20), Some(71));
        assert_eq!(t.get(23, 20), Some(0));
        // Odd columns are staggered up by 16 px: (21, 21) is at (672, 656), d² = 32² + 16².
        assert_eq!(t.get(21, 21), Some(71));
        assert_eq!(t.get(21, 23), Some(0));
        let painted = t.cells.iter().filter(|&&c| c == 71).count();
        assert!((10..=20).contains(&painted), "{painted}");
        assert!(map.heights.as_ref().unwrap().cells.iter().all(|&h| h == 0));
    }

    #[test]
    fn water_floats_ships_inside_with_a_shore() {
        let mut map = flat();
        apply(&mut map, &dab(3200, 3200, 160, Tool::Water)).unwrap();
        let w = map.water.as_ref().unwrap();
        // Centre (cell 100, 100): edge = 160/32 = 5, 120 + 80 = 200.
        assert_eq!((w.depth.get(100, 100), w.brightness.get(100, 100)), (Some(DEEP), Some(0x60)));
        // Ships float from 130: the middle, not the rim.
        assert!(w.depth.get(100, 100).unwrap() > 130);
        assert_eq!(w.depth.get(105, 100), Some(SHORE));
        assert_eq!(w.depth.get(106, 100), Some(0));
        // Never shallower than what is there.
        apply(&mut map, &dab(3200, 3200, 160, Tool::Water)).unwrap();
        apply(&mut map, &dab(3200 + 128, 3200, 64, Tool::Water)).unwrap();
        assert_eq!(map.water.as_ref().unwrap().depth.get(100, 100), Some(DEEP));
        // Heights stay as they were.
        assert!(map.heights.as_ref().unwrap().cells.iter().all(|&h| h == 0));
    }

    #[test]
    fn land_clears_the_depth() {
        let mut map = flat();
        apply(&mut map, &dab(3200, 3200, 160, Tool::Water)).unwrap();
        apply(&mut map, &dab(3200, 3200, 64, Tool::Land)).unwrap();
        let w = map.water.as_ref().unwrap();
        assert_eq!((w.depth.get(100, 100), w.brightness.get(100, 100)), (Some(0), Some(0)));
        assert_eq!(w.depth.get(102, 100), Some(0));
        // Outside the dry disc the pool stays.
        assert!(w.depth.get(104, 100).unwrap() > 0);
        assert_eq!(apply(&mut map, &dab(3200, 3200, 64, Tool::Land)), Ok(None));
    }

    #[test]
    fn dirty_box_covers_every_change() {
        let tools = [Tool::Raise(RAISE), Tool::Flatten(40), Tool::Texture(5), Tool::Water];
        for tool in tools {
            // Odd and even columns, a map corner, and near the far edge (the sea ends at 15392).
            for (x, y) in [(640, 640), (672, 656), (5, 9), (15300, 15300)] {
                let mut map = flat();
                let before = map.clone();
                let b = apply(&mut map, &dab(x, y, 100, tool)).unwrap().expect("changed");
                let inside = |px: i32, py: i32| b[0] <= px && px < b[2] && b[1] <= py && py < b[3];
                let (h0, h1) = (before.heights.as_ref().unwrap(), map.heights.as_ref().unwrap());
                let (t0, t1) = (before.textures.as_ref().unwrap(), map.textures.as_ref().unwrap());
                for col in 0..h0.width {
                    for row in 0..h0.height {
                        let i = row * h0.width + col;
                        if h0.cells[i] != h1.cells[i] || t0.cells[i] != t1.cells[i] {
                            assert!(inside(32 * col as i32, 32 * row as i32 - 16 * (col as i32 & 1)));
                        }
                    }
                }
                let (w0, w1) = (&before.water.as_ref().unwrap().depth, &map.water.as_ref().unwrap().depth);
                for (i, (a, c)) in w0.cells.iter().zip(&w1.cells).enumerate() {
                    if a != c {
                        let (cx, cy) = ((i % w0.width) as i32, (i / w0.width) as i32);
                        assert!(inside(32 * cx, 32 * cy) && inside(32 * cx + 31, 32 * cy + 31));
                    }
                }
            }
        }
    }

    #[test]
    fn refuses_areas_missing_sea_bad_radius_bad_texture() {
        let raise = |radius| dab(640, 640, radius, Tool::Raise(RAISE));
        let mut map = flat();
        assert_eq!(apply(&mut map, &raise(15)), Err(EditError::Radius));
        assert_eq!(apply(&mut map, &raise(513)), Err(EditError::Radius));
        assert!(apply(&mut map, &raise(512)).is_ok() && apply(&mut map, &raise(16)).is_ok());
        assert_eq!(apply(&mut map, &dab(640, 640, 64, Tool::Texture(136))), Err(EditError::Texture));
        assert!(apply(&mut map, &dab(640, 640, 64, Tool::Texture(135))).is_ok());

        let mut no_sea = flat();
        no_sea.water = None;
        assert_eq!(apply(&mut no_sea, &dab(640, 640, 64, Tool::Water)), Err(EditError::NoSea));
        assert_eq!(apply(&mut no_sea, &dab(640, 640, 64, Tool::Land)), Err(EditError::NoSea));
        assert!(apply(&mut no_sea, &raise(64)).is_ok());

        let mut no_terrain = flat();
        no_terrain.textures = None;
        assert_eq!(apply(&mut no_terrain, &raise(64)), Err(EditError::NoTerrain));
        assert!(apply(&mut no_terrain, &dab(640, 640, 64, Tool::Water)).is_ok());

        let mut areas = flat();
        areas.land_areas = Some(acr_formats::m3d::Areas {
            areas: Vec::new(),
            next_hop: Vec::new(),
            distance: Vec::new(),
            cells: Grid { width: 0, height: 0, cells: Vec::new() },
        });
        assert_eq!(apply(&mut areas, &raise(64)), Err(EditError::Areas));
        assert_eq!(apply(&mut areas, &dab(640, 640, 64, Tool::Water)), Err(EditError::Areas));
        // Refused before anything is touched.
        assert!(areas.heights.as_ref().unwrap().cells.iter().all(|&h| h == 0));
    }
}
