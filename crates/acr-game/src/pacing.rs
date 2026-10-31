//! Frame pacing (#440): how often the window is redrawn. `frame_cap=off|30|60` limits the rate,
//! `power_save=on|off` (unset: follows Android's Game Mode) caps it at 30 fps. The cap skips
//! redraw requests until the next frame time instead of waiting in a loop, so it works with
//! the vsync present mode: the loop sleeps in between and the present still waits for the
//! display's next refresh.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use web_time::Instant;

/// `frame_cap=off|30|60`: no cap (the display's rate under vsync), or at most 30 or 60 frames
/// per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameCap {
    #[default]
    Off,
    Fps30,
    Fps60,
}

impl FrameCap {
    pub fn name(self) -> &'static str {
        match self {
            FrameCap::Off => "off",
            FrameCap::Fps30 => "30",
            FrameCap::Fps60 => "60",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [FrameCap::Off, FrameCap::Fps30, FrameCap::Fps60].into_iter().find(|c| c.name() == value)
    }

    pub fn fps(self) -> Option<u32> {
        match self {
            FrameCap::Off => None,
            FrameCap::Fps30 => Some(30),
            FrameCap::Fps60 => Some(60),
        }
    }
}

/// The cap of the battery saver.
pub const POWER_SAVE_FPS: u32 = 30;

/// The frame rate in effect: the stricter of `frame_cap` and the battery saver's 30 fps. The
/// saver is `power_save` when set, else Android's Game Mode `BATTERY` (`battery_mode`).
pub fn cap_fps(frame_cap: FrameCap, power_save: Option<bool>, battery_mode: bool) -> Option<u32> {
    let saver = power_save.unwrap_or(battery_mode).then_some(POWER_SAVE_FPS);
    match (frame_cap.fps(), saver) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Android's Game Mode is `GAME_MODE_BATTERY` (set by [`set_battery_mode`] on resume).
static BATTERY_MODE: AtomicBool = AtomicBool::new(false);

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn set_battery_mode(on: bool) {
    BATTERY_MODE.store(on, Ordering::Relaxed);
}

pub fn battery_mode() -> bool {
    BATTERY_MODE.load(Ordering::Relaxed)
}

/// A frame is due a little before the interval has passed: under vsync the present then
/// waits for the display's refresh, and a frame that came a hair late would miss it and
/// fall to the next one.
const EARLY: f64 = 0.9;

/// When the next frame is due.
#[derive(Debug, Default)]
pub struct Pacing {
    next: Option<Instant>,
}

impl Pacing {
    /// How long to sleep before the next redraw is requested; `None`: request it now.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.next?.checked_duration_since(now).filter(|d| !d.is_zero())
    }

    /// A frame was drawn at `now`; with a cap the next one is due after its interval.
    pub fn frame_done(&mut self, now: Instant, fps: Option<u32>) {
        self.next = fps.map(|f| now + Duration::from_secs_f64(EARLY / f64::from(f.max(1))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for c in [FrameCap::Off, FrameCap::Fps30, FrameCap::Fps60] {
            assert_eq!(FrameCap::parse(c.name()), Some(c));
        }
        assert_eq!(FrameCap::parse("45"), None);
    }

    #[test]
    fn power_save_caps_at_30_and_follows_game_mode_when_unset() {
        use FrameCap::*;
        assert_eq!(cap_fps(Off, None, false), None);
        assert_eq!(cap_fps(Off, None, true), Some(30));
        assert_eq!(cap_fps(Off, Some(true), false), Some(30));
        // An explicit `power_save=off` wins over the Game Mode.
        assert_eq!(cap_fps(Off, Some(false), true), None);
        assert_eq!(cap_fps(Fps60, None, false), Some(60));
        assert_eq!(cap_fps(Fps60, Some(true), false), Some(30));
        assert_eq!(cap_fps(Fps30, Some(false), true), Some(30));
    }

    #[test]
    fn the_cap_waits_until_the_next_frame_time_without_busy_waiting() {
        let t0 = Instant::now();
        let mut p = Pacing::default();
        assert_eq!(p.wait(t0), None, "nothing drawn yet: draw now");
        p.frame_done(t0, None);
        assert_eq!(p.wait(t0), None, "no cap: never wait");
        p.frame_done(t0, Some(30));
        let w = p.wait(t0 + Duration::from_millis(5)).expect("must wait");
        // 30 fps: a frame every 33.3 ms, due 10 % early (30 ms), 5 ms have passed.
        assert!((w.as_secs_f64() - 0.025).abs() < 1e-6, "{w:?}");
        assert_eq!(p.wait(t0 + Duration::from_millis(31)), None);
        p.frame_done(t0, Some(60));
        assert!(p.wait(t0 + Duration::from_millis(16)).is_none());
    }
}
