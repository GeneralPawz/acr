//! The in-game mouse cursor: `CURSOR_00.GP` (cursor pack = palette set + 5 after
//! `LoadFog(0)` at game start, 0x5ab133), hotspots and animation chains from
//! `curs_xy.dat` (one `dx dy next` line per frame), drawn at mouse − hotspot (0x4970b0).

/// The cursor pack of the game screen.
pub const GP: &str = "CURSOR_00.GP";
pub const FILE: &str = "CURS_XY.DAT";

/// Frames of `CURSOR_00.GP` (the globals 0x66a904.. and 0x66a924).
pub mod frame {
    pub const ARROW: usize = 0;
    /// Attack: an enemy under the cursor with an own selection.
    pub const SWORD: usize = 1;
    /// Build or repair an own building (0x66a920).
    pub const BUILD: usize = 3;
    /// Go into an own building (0x66a918).
    pub const ENTER: usize = 11;
    /// Pick the patrol point (0x66a90c).
    pub const PATROL: usize = 13;
    /// Pick the point to bombard (0x66a910).
    pub const ATTACK_GROUND: usize = 9;
    /// Gathering, by source type (0x66a934): axe for wood, pick for minerals, sickle for
    /// fields, sword for meat; 0 is the arrow.
    pub const GATHER: [u8; 16] = [6, 5, 5, 7, 5, 5, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
}

/// The animation steps every 60 ms (`RefreshView`, 0x47e075).
const STEP: f64 = 0.06;

#[derive(Debug, Clone)]
pub struct Cursor {
    /// Hotspot and next frame per frame.
    table: Vec<([i32; 2], usize)>,
    pub frame: usize,
    time: f64,
}

impl Default for Cursor {
    fn default() -> Self {
        // Without the file: no hotspots, frames 0..15 static, 16..31 cycling (0x497036).
        let table = (0..32)
            .map(|i| {
                (
                    [0, 0],
                    if i < 16 {
                        i
                    } else if i == 31 {
                        16
                    } else {
                        i + 1
                    },
                )
            })
            .collect();
        Self { table, frame: 0, time: 0.0 }
    }
}

impl Cursor {
    pub fn parse(data: &[u8]) -> Self {
        let text = String::from_utf8_lossy(data);
        let mut table = Vec::new();
        for line in text.lines().take(256) {
            let v: Vec<i32> = line.split_whitespace().filter_map(|w| w.parse().ok()).collect();
            if let [dx, dy, next, ..] = v[..] {
                table.push(([dx, dy], next.max(0) as usize));
            }
        }
        if table.is_empty() {
            return Self::default();
        }
        Self { table, frame: 0, time: 0.0 }
    }

    pub fn hotspot(&self) -> [i32; 2] {
        self.table.get(self.frame).map_or([0, 0], |e| e.0)
    }

    /// `SetCurPtr(n)` (0x5c3b90): a frame of `n`'s animation chain keeps animating,
    /// anything else switches to `n`.
    pub fn set(&mut self, n: usize) {
        let mut f = n;
        for _ in 0..30 {
            if f == self.frame {
                return;
            }
            f = self.table.get(f).map_or(f, |e| e.1);
        }
        self.frame = n;
    }

    pub fn tick(&mut self, seconds: f64) {
        self.time += seconds;
        while self.time > STEP {
            self.time -= STEP;
            self.frame = self.table.get(self.frame).map_or(self.frame, |e| e.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotspots_and_animation() {
        let mut c = Cursor::parse(b"0 0 0\r\n5 5 1\r\n7 17  2\r\n");
        assert_eq!(c.hotspot(), [0, 0]);
        c.set(1);
        assert_eq!(c.hotspot(), [5, 5]);
        let mut c = Cursor::default();
        c.set(16);
        c.tick(0.13);
        assert_eq!(c.frame, 18);
        c.set(16);
        assert_eq!(c.frame, 18, "a frame of the chain keeps animating");
        c.set(0);
        assert_eq!(c.frame, 0);
    }
}
