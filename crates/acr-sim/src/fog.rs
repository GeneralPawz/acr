//! Fog of war, as the original draws it: one grid of 128 px cells for the local player
//! (`FogMap` 0x7b7b28). Every tick each living object of the viewer stamps 8000 into a few
//! cells chosen by its `VISION` (0x484f7b), and every frame the grid is blurred and decays
//! (0x483c2a). Nothing in the simulation reads it; it only darkens the screen and the
//! minimap, mutes sounds and answers the scripts' `GetFogSpotVisibility`. It is therefore
//! kept outside [`World`] and its state hash: the host owns one `Fog` for the local player.

use crate::command::PlayerId;
use crate::world::World;

/// Value a stamp writes.
pub const STAMP: u16 = 8000;
/// `GetFogSpotVisibility` (0x56e6a4): a spot is visible at this value or above.
pub const VISIBLE: u16 = 900;
/// The minimap shows black at or below this value (0x485c24).
pub const MINIMAP_DARK: u16 = 1300;

/// Stamp patterns per `VISION` value (0..=8), offsets in world pixels, in the engine's
/// order. Values 7 and 8 keep the original's wrong diagonal point (`y+4`, `y+3`).
fn pattern(v: u8) -> &'static [(i32, i32)] {
    const P: [&[(i32, i32)]; 9] = [
        &[(0, 0)],
        &[(128, 0), (-128, 0), (0, 128), (0, -128)],
        &[(128, 128), (-128, 128), (128, -128), (-128, -128)],
        &[(256, 0), (-256, 0), (0, -256), (0, 256), (128, 128), (-128, 128), (128, -128), (-128, -128)],
        &[(384, 0), (-384, 0), (0, 384), (0, -384), (256, 256), (-256, 256), (256, -256), (-256, -256)],
        &[(512, 0), (-512, 0), (0, 512), (0, -512), (384, 384), (-384, 384), (384, -384), (-384, -384)],
        &[(640, 0), (-640, 0), (0, 640), (0, -640), (512, 512), (-512, 512), (512, -512), (-512, -512)],
        &[(768, 0), (-768, 0), (0, 768), (0, -768), (512, 4), (-512, 512), (512, -512), (-512, -512)],
        &[(896, 0), (-896, 0), (0, 896), (0, -896), (640, 3), (-640, 640), (640, -640), (-640, -640)],
    ];
    P[usize::from(v.min(8))]
}

/// The fog grid of one viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fog {
    width: i32,
    max: (i32, i32),
    /// Blurred area: `(msx >> 2) + 6` cells (`msx` = playable side in 32 px units).
    span: i32,
    cells: Vec<u16>,
    tmp: Vec<u16>,
    /// Script light spots (`SetLightSpot`): `(x, y, r)` in world pixels, 64 slots.
    pub light_spots: [Option<(i32, i32, u8)>; 64],
}

impl Fog {
    /// An all-black grid for a map of size shift `shift` (1 = normal).
    pub fn new(shift: u8) -> Self {
        let width = 0x86 << (shift.max(1) - 1);
        let msx = 240i32 << shift;
        let max = (msx >> 2) + 3;
        Self {
            width,
            max: (max, max),
            span: ((msx >> 2) + 6).min(width - 1),
            cells: vec![0; (width * width) as usize],
            tmp: vec![0; (width * width) as usize],
            light_spots: [None; 64],
        }
    }

    /// Index of the cell at world pixel `(x, y)` (`y` already projected, see [`Fog::reveal`]).
    fn index(&self, x: i32, y: i32) -> usize {
        let cx = (((x + 64) >> 7) + 3).max(1).min(self.max.0);
        let cy = (((y + 64) >> 7) + 3).max(1).min(self.max.1);
        (cy * self.width + cx) as usize
    }

    /// Fog value at a projected world position (0 = black).
    pub fn get(&self, x: i32, y: i32) -> u16 {
        self.cells[self.index(x, y)]
    }

    fn stamp(&mut self, x: i32, y: i32, vision: u8) {
        for &(dx, dy) in pattern(vision) {
            let i = self.index(x + dx, y + dy);
            self.cells[i] = STAMP;
        }
    }

    /// Per tick: every living object of `viewer` stamps its `VISION` pattern at its screen
    /// position (`y − 2·height`, so hills shift the spot north). The original does the same
    /// for allies outside missions; ACR has no alliances yet, so only `viewer`'s own units.
    pub fn reveal(&mut self, world: &World, viewer: PlayerId) {
        for u in world.units_of(viewer) {
            let t = world.catalog.get(u.kind);
            if t.vision > 8 {
                continue;
            }
            let (x, y) = u.pos.pixels();
            let y = y - 2 * world.map.heights.at(x, y);
            self.stamp(x, y, t.vision);
        }
    }

    /// Per rendered frame: stamps the script light spots, then blurs and decays the grid
    /// (each cell becomes `(s >> 2) − (s >> 10)` of the sum `s` of its 4 neighbours).
    pub fn blur(&mut self, world: &World) {
        for (x, y, r) in self.light_spots.into_iter().flatten() {
            let y = y - 2 * world.map.heights.at(x, y);
            self.stamp(x, y, r.min(8));
        }
        self.tmp.copy_from_slice(&self.cells);
        let w = self.width as usize;
        let span = self.span as usize;
        for y in 1..span {
            for x in 1..span {
                let i = y * w + x;
                let s = u32::from(self.tmp[i - w])
                    + u32::from(self.tmp[i + w])
                    + u32::from(self.tmp[i - 1])
                    + u32::from(self.tmp[i + 1]);
                self.cells[i] = ((s >> 2) - (s >> 10)) as u16;
            }
        }
    }

    /// `GetFogSpotVisibility` for a world position (the projection is applied here).
    pub fn visible(&self, world: &World, x: i32, y: i32) -> bool {
        self.get(x, y - 2 * world.map.heights.at(x, y)) >= VISIBLE
    }

    /// Darkness of the overlay for a fog value (0x484e63): 5 = clear, 158 = black.
    pub fn shade(v: u16) -> u8 {
        if v >= 1500 {
            return 5;
        }
        let i = 1500 - i32::from(v);
        if i <= 300 { (5 + i * 153 / 300) as u8 } else { 158 }
    }

    /// Grid side in cells and the raw values, for drawing (cell `(cx, cy)` is centred on
    /// world `(128·(cx − 3), 128·(cy − 3))` in projected coordinates).
    pub fn cells(&self) -> (i32, &[u16]) {
        (self.width, &self.cells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_ramps_between_1200_and_1500() {
        assert_eq!(Fog::shade(8000), 5);
        assert_eq!(Fog::shade(1500), 5);
        assert_eq!(Fog::shade(1350), 81);
        assert_eq!(Fog::shade(1200), 158);
        assert_eq!(Fog::shade(0), 158);
    }

    #[test]
    fn stamps_spread_and_fade() {
        let mut f = Fog::new(1);
        f.stamp(1000, 1000, 1);
        assert_eq!(f.get(1128, 1000), STAMP);
        assert_eq!(f.get(1000, 1000), 0, "vision 1 does not stamp the centre");
        let w = f.width as usize;
        let span = f.span as usize;
        // Blur by hand (no world needed without light spots).
        for _ in 0..3 {
            f.tmp.copy_from_slice(&f.cells);
            for y in 1..span {
                for x in 1..span {
                    let i = y * w + x;
                    let s = u32::from(f.tmp[i - w])
                        + u32::from(f.tmp[i + w])
                        + u32::from(f.tmp[i - 1])
                        + u32::from(f.tmp[i + 1]);
                    f.cells[i] = ((s >> 2) - (s >> 10)) as u16;
                }
            }
        }
        assert!(f.get(1000, 1000) > 0, "the inside fills");
        assert!(f.get(1128, 1000) < STAMP, "stamps fade");
    }
}
