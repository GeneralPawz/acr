//! Peace time: the countdown at the start of a game (`PeaceTimeLeft`, 0x66795c).
//!
//! The map's `CAEP` chunk carries it in seconds (read by 0x545705; a random map's option
//! digit picks one of `acr_mapgen::PEACE_TIMES`, 0x45dbcf). The engine keeps three values:
//! the seconds left (0x66795c), the full time (0x6e45d4) and the minutes left (0x6e45d8),
//! the latter only for scripts (`GetPeaceTimeLeft`, 0x569882).
//!
//! - Counting (0x4bf51e, also 0x4cbfd3): once the game has run 20 frames, every whole second
//!   that passed is taken off the seconds left, which stop at 1; at 1 the engine sends the
//!   end command (0x4d8258, executed by 0x4d8293), which sets them to 0.
//! - The minutes (0x4a7573): every 30 s the host sends `left / 60` as a network command
//!   (0x4d843f), which 0x4d84b5 stores in 0x6e45d8. Scripts therefore see a value up to
//!   30 s old, and still the last minutes for up to 30 s after the peace ended.
//! - The interface shows the time left and the time played as `h:mm:ss` (0x4bf767).
//! - While it lasts, the AI's diversions (`MakeDiversionLink` 0x424957) and the city's
//!   attack code (0x422d3d, 0x4391a9) do nothing, and units and buildings keep out of the
//!   other nations' territories (0x45f65c, 0x45f767, territories of `CAEP`).

use crate::ORIGINAL_TICKS_PER_SECOND;
use crate::world::World;

/// Ticks between two updates of the minutes (0x4a7562: 30000 ms).
pub const SYNC_TICKS: u64 = 30 * ORIGINAL_TICKS_PER_SECOND as u64;

/// The peace-time state of a game.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Peace {
    /// Seconds left (0x66795c); 0 when there is no peace (any more).
    pub left: i32,
    /// The full peace time in seconds (0x6e45d4).
    pub max: i32,
    /// Minutes left as the scripts see them (0x6e45d8).
    pub minutes: i32,
}

impl Peace {
    /// A peace of `seconds` (0x54572f / 0x45dc4c).
    pub fn new(seconds: i32) -> Self {
        Peace { left: seconds, max: seconds, minutes: seconds / 60 }
    }

    /// Whether the peace still holds (`PeaceTimeLeft != 0`).
    pub fn active(&self) -> bool {
        self.left != 0
    }

    /// Takes `seconds` off the time left (0x4bf51e).
    pub fn count(&mut self, seconds: i32) {
        if self.left > 1 {
            self.left = (self.left - seconds).max(1);
        } else if self.left == 1 {
            // The end command (0x4d8258 -> 0x4d8293).
            self.left = 0;
        }
    }
}

/// Runs the countdown for one tick: a game second every [`ORIGINAL_TICKS_PER_SECOND`] ticks
/// after the first 20, and the minutes every [`SYNC_TICKS`].
pub fn tick(w: &mut World) {
    let t = w.tick.0;
    if t > 20 && t.is_multiple_of(u64::from(ORIGINAL_TICKS_PER_SECOND)) {
        w.peace.count(1);
    }
    if t > 0 && t.is_multiple_of(SYNC_TICKS) {
        w.peace.minutes = w.peace.left / 60;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_to_one_then_ends() {
        let mut p = Peace::new(300);
        assert_eq!((p.left, p.max, p.minutes), (300, 300, 5));
        p.count(1);
        assert_eq!(p.left, 299);
        p.count(500);
        assert_eq!(p.left, 1);
        assert!(p.active());
        p.count(1);
        assert_eq!(p.left, 0);
        assert!(!p.active());
        p.count(1);
        assert_eq!(p.left, 0);
    }
}
