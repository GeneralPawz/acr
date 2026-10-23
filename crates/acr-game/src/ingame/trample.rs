//! Trampled soil (#223, docs/rendering-3d.md): where many units walk, the ground darkens
//! into trodden paths and slowly recovers. Render only: wear per terrain vertex grows with
//! every moving unit on it each tick and decays over minutes; the terrain renderer darkens
//! the vertex light levels by it (`TerrainRenderer::set_trample`), in sprite and 3D mode.

use acr_sim::World;

/// Wear a moving unit adds to its vertex per tick: a unit crosses a vertex in about three
/// ticks, so some five walkers start a path and fifteen wear it out; one leaves no trace.
const STEP: f32 = 0.02;
/// Decay per tick: half the wear is gone after about four minutes of game time.
const KEEP: f32 = 0.9997;
/// Ticks between uploads to the renderer (one second).
const UPLOAD_EVERY: u64 = 10;

pub struct Trample {
    width: usize,
    height: usize,
    wear: Vec<f32>,
    last_upload: u64,
    dirty: bool,
}

impl Trample {
    /// A trample map for a terrain of `width` × `height` vertices.
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, wear: vec![0.0; width * height], last_upload: 0, dirty: false }
    }

    /// The terrain vertex nearest world pixel `(x, y)`: vertex (col, row) sits at
    /// (32 col, 32 row − 16 for odd columns).
    pub fn vertex(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 {
            return None;
        }
        let col = ((x + 16) / 32) as usize;
        let row = ((y + if col & 1 == 1 { 16 } else { 0 } + 16) / 32) as usize;
        (col < self.width && row < self.height).then_some(row * self.width + col)
    }

    /// One simulation tick: wear under every moving unit (not ships, buildings or the dead),
    /// decay everywhere.
    pub fn tick(&mut self, world: &World) {
        for w in &mut self.wear {
            *w *= KEEP;
        }
        for u in world.living() {
            if u.building || !u.in_motion || u.inside_of.is_some() {
                continue;
            }
            let (x, y) = u.pos.pixels();
            if let Some(v) = self.vertex(x, y) {
                self.wear[v] = (self.wear[v] + STEP).min(1.0);
            }
        }
        if world.tick.0 >= self.last_upload + UPLOAD_EVERY {
            self.last_upload = world.tick.0;
            self.dirty = true;
        }
    }

    /// Vertices.
    pub fn len(&self) -> usize {
        self.wear.len()
    }

    /// The wear as 0..255 per vertex now, changed or not.
    pub fn force_upload(&mut self) -> Vec<u8> {
        self.dirty = false;
        self.wear.iter().enumerate().map(|(v, &w)| amount(w, v)).collect()
    }

    /// The wear as 0..255 per vertex for the renderer if it changed since the last call:
    /// a ragged edge (per-vertex noise), no trace below a fifth of full wear.
    pub fn take_upload(&mut self) -> Option<Vec<u8>> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        Some(self.wear.iter().enumerate().map(|(v, &w)| amount(w, v)).collect())
    }
}

/// The darkening (0..255) of a vertex with wear `w`, `v` varying the edge.
fn amount(w: f32, v: usize) -> u8 {
    let jitter = ((v as u32).wrapping_mul(2_654_435_761) >> 24) as f32 / 255.0 - 0.5;
    let t = ((w + 0.15 * jitter - 0.2) / 0.6).clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t) * 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertices_follow_the_terrain_grid() {
        let t = Trample::new(513, 513);
        assert_eq!(t.vertex(0, 0), Some(0));
        // Odd columns sit half a row higher (row r at y = 32 r − 16): y = 0 lies halfway
        // and rounds to row 1, y = −10 is out of the map.
        assert_eq!(t.vertex(32, 0), Some(513 + 1));
        assert_eq!(t.vertex(32, 16), Some(513 + 1));
        assert_eq!(t.vertex(30, 40), Some(2 * 513 + 1));
        assert_eq!(t.vertex(64, 32), Some(513 + 2));
        assert_eq!(t.vertex(-5, 0), None);
        assert_eq!(t.vertex(32 * 600, 0), None);
    }

    #[test]
    fn many_walkers_wear_paths_that_recover() {
        // Wear and decay without a world: a hundred passes darken, time heals.
        let mut w = 0.0f32;
        for _ in 0..45 {
            w = (w * KEEP + STEP).min(1.0);
        }
        assert!(amount(w, 0) > 200, "{w}");
        assert_eq!(amount(STEP * 3.0, 0), 0, "a single walker leaves no trace");
        assert!(amount(STEP * 15.0, 0) > 0, "five walkers start a path");
        for _ in 0..6000 {
            w *= KEEP;
        }
        assert!(amount(w, 0) < 30, "recovered {w}");
    }
}
