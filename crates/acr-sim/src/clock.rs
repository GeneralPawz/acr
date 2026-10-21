//! Converts real elapsed time into a whole number of fixed simulation ticks.

/// Index of a simulation step since the start of a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tick(pub u64);

/// Fixed-timestep accumulator.
///
/// The host calls [`FixedClock::advance`] once per rendered frame with the real time
/// that passed; it returns how many ticks to simulate. Rendering then interpolates
/// between the last two ticks with [`FixedClock::alpha`].
#[derive(Debug, Clone)]
pub struct FixedClock {
    tick_nanos: u64,
    accumulated: u64,
    max_ticks_per_advance: u32,
    now: Tick,
}

impl FixedClock {
    /// `ticks_per_second` sets game speed; `max_ticks_per_advance` bounds catch-up work
    /// after a stall (the game slows down instead of freezing).
    pub fn new(ticks_per_second: u32, max_ticks_per_advance: u32) -> Self {
        assert!(ticks_per_second > 0, "tick rate must be positive");
        Self {
            tick_nanos: 1_000_000_000 / u64::from(ticks_per_second),
            accumulated: 0,
            max_ticks_per_advance,
            now: Tick(0),
        }
    }

    pub fn set_rate(&mut self, ticks_per_second: u32) {
        assert!(ticks_per_second > 0, "tick rate must be positive");
        self.tick_nanos = 1_000_000_000 / u64::from(ticks_per_second);
        self.accumulated = self.accumulated.min(self.tick_nanos);
    }

    /// Adds real time and returns the number of ticks that are now due.
    pub fn advance(&mut self, elapsed_nanos: u64) -> u32 {
        self.accumulated = self.accumulated.saturating_add(elapsed_nanos);
        let due = self.accumulated / self.tick_nanos;
        let run = due.min(u64::from(self.max_ticks_per_advance));
        // Drop time we refuse to catch up on rather than snowballing.
        self.accumulated = if due > run { 0 } else { self.accumulated - run * self.tick_nanos };
        self.now.0 += run;
        run as u32
    }

    /// Progress towards the next tick in `0.0..1.0`, for render interpolation only.
    pub fn alpha(&self) -> f32 {
        self.accumulated as f32 / self.tick_nanos as f32
    }

    pub fn now(&self) -> Tick {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    #[test]
    fn tick_count_is_independent_of_frame_rate() {
        let mut slow = FixedClock::new(20, 100);
        let mut fast = FixedClock::new(20, 100);
        let slow_ticks: u32 = (0..30).map(|_| slow.advance(100 * MS)).sum();
        let fast_ticks: u32 = (0..3000).map(|_| fast.advance(MS)).sum();
        assert_eq!(slow_ticks, 60);
        assert_eq!(fast_ticks, 60);
    }

    #[test]
    fn stalls_are_capped() {
        let mut c = FixedClock::new(20, 5);
        assert_eq!(c.advance(10_000 * MS), 5);
        assert_eq!(c.advance(0), 0);
        assert_eq!(c.now(), Tick(5));
    }

    #[test]
    fn alpha_tracks_partial_tick() {
        let mut c = FixedClock::new(10, 5);
        c.advance(150 * MS);
        assert!((c.alpha() - 0.5).abs() < 1e-6);
    }
}
