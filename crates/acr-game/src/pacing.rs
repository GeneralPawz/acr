//! Frame pacing (#440): how often the window is redrawn. `frame_cap=off|30|60` limits the rate,
//! `power_save=on|off` (unset: follows Android's Game Mode) caps it at 30 fps. The cap skips
//! redraw requests until the next frame time instead of waiting in a loop, so it works with
//! the vsync present mode: the loop sleeps in between and the present still waits for the
//! display's next refresh.
//!
//! Render on demand (#440): when a frame looks like the one before for [`QUIET_FRAMES`] frames
//! (a [`Pacing::observe`]d signature of what the frame shows), with no input in between, the
//! rate drops to [`IDLE_FPS`]. Any input ([`Pacing::input`]) or a change in the signature
//! restores the full rate at once. A frame whose signature is unknown (`None`) counts as a
//! change: when unsure, render.

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

/// Frames with an unchanged picture and no input before the rate drops.
pub const QUIET_FRAMES: u32 = 30;

/// The rate while nothing changes.
pub const IDLE_FPS: u32 = 5;

/// Render on demand is on: `power_save` says so, else on Android only (the desktop and the
/// browser keep drawing at the display's rate).
pub fn on_demand(power_save: Option<bool>, android: bool) -> bool {
    power_save.unwrap_or(android)
}

/// The frame budget the performance hints aim at: the interval of the rate in effect (the cap,
/// the idle rate), else the display's refresh rate (`refresh_mhz`, millihertz), else 60 Hz.
pub fn frame_budget(fps: Option<u32>, refresh_mhz: Option<u32>) -> Duration {
    match (fps, refresh_mhz) {
        (Some(f), _) if f > 0 => Duration::from_secs_f64(1.0 / f64::from(f)),
        (_, Some(m)) if m > 0 => Duration::from_secs_f64(1000.0 / f64::from(m)),
        _ => Duration::from_secs_f64(1.0 / 60.0),
    }
}

/// Where the main thread's work per frame goes (Android's performance hints, #440).
pub trait WorkSink {
    /// The work per frame the system should plan for changed.
    fn target(&mut self, budget: Duration);
    /// The thread's work for the frame just drawn, without the wait for the display.
    fn report(&mut self, work: Duration);
}

/// Feeds a [`WorkSink`]: the target when the budget changed, the work every frame.
#[derive(Default)]
pub struct Work {
    budget: Option<Duration>,
}

impl Work {
    pub fn frame(&mut self, sink: &mut dyn WorkSink, budget: Duration, work: Duration) {
        if self.budget != Some(budget) {
            self.budget = Some(budget);
            sink.target(budget);
        }
        sink.report(work);
    }
}

/// A frame is due a little before the interval has passed: under vsync the present then
/// waits for the display's refresh, and a frame that came a hair late would miss it and
/// fall to the next one.
const EARLY: f64 = 0.9;

/// When the next frame is due.
#[derive(Debug, Default)]
pub struct Pacing {
    next: Option<Instant>,
    /// The last frame's signature, and how many frames in a row had it.
    last: Option<u64>,
    quiet: u32,
}

impl Pacing {
    /// How long to sleep before the next redraw is requested; `None`: request it now.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.next?.checked_duration_since(now).filter(|d| !d.is_zero())
    }

    /// A frame was drawn at `now`; with a cap, or while nothing changes and `on_demand` is on,
    /// the next one is due after its interval.
    pub fn frame_done(&mut self, now: Instant, fps: Option<u32>, on_demand: bool) {
        self.next = self.rate(fps, on_demand).map(|f| now + Duration::from_secs_f64(EARLY / f64::from(f.max(1))));
    }

    /// The frame rate in effect: the cap `fps`, lowered to [`IDLE_FPS`] while idle.
    pub fn rate(&self, fps: Option<u32>, on_demand: bool) -> Option<u32> {
        match (fps, on_demand && self.idle()) {
            (Some(f), true) => Some(f.min(IDLE_FPS)),
            (None, true) => Some(IDLE_FPS),
            (f, false) => f,
        }
    }

    /// What the frame just drawn shows, as a hash; `None`: not known, so not quiet.
    pub fn observe(&mut self, signature: Option<u64>) {
        self.quiet = match signature {
            Some(s) if self.last == Some(s) => self.quiet.saturating_add(1),
            _ => 0,
        };
        self.last = signature;
    }

    /// An input arrived: the full rate again, and the next frame at once if the rate was low.
    pub fn input(&mut self) {
        if self.idle() {
            self.next = None;
        }
        self.quiet = 0;
    }

    /// Nothing has changed for [`QUIET_FRAMES`] frames.
    pub fn idle(&self) -> bool {
        self.quiet >= QUIET_FRAMES
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
        p.frame_done(t0, None, false);
        assert_eq!(p.wait(t0), None, "no cap: never wait");
        p.frame_done(t0, Some(30), false);
        let w = p.wait(t0 + Duration::from_millis(5)).expect("must wait");
        // 30 fps: a frame every 33.3 ms, due 10 % early (30 ms), 5 ms have passed.
        assert!((w.as_secs_f64() - 0.025).abs() < 1e-6, "{w:?}");
        assert_eq!(p.wait(t0 + Duration::from_millis(31)), None);
        p.frame_done(t0, Some(60), false);
        assert!(p.wait(t0 + Duration::from_millis(16)).is_none());
    }

    /// Feeds `n` frames with signature `s` and returns the wait after the last one.
    fn frames(p: &mut Pacing, t: Instant, n: u32, s: Option<u64>, fps: Option<u32>) -> Option<Duration> {
        for _ in 0..n {
            p.observe(s);
            p.frame_done(t, fps, true);
        }
        p.wait(t)
    }

    #[test]
    fn an_unchanged_picture_drops_to_idle_after_the_quiet_frames() {
        let t0 = Instant::now();
        let mut p = Pacing::default();
        assert_eq!(frames(&mut p, t0, QUIET_FRAMES, Some(7), None), None, "the first frame is a change");
        assert!(!p.idle());
        let w = frames(&mut p, t0, 1, Some(7), None).expect("idle: waits");
        assert!(p.idle());
        // 5 fps: 200 ms, due 10 % early.
        assert!((w.as_secs_f64() - 0.18).abs() < 1e-6, "{w:?}");
        // A cap below the idle rate stays: 2 fps is never raised to 5.
        assert!(frames(&mut p, t0, 1, Some(7), Some(2)).unwrap() > w);
    }

    #[test]
    fn a_change_an_unknown_picture_or_an_input_restores_the_rate_at_once() {
        let t0 = Instant::now();
        let mut p = Pacing::default();
        assert!(frames(&mut p, t0, QUIET_FRAMES + 2, Some(7), None).is_some());
        // The picture changed.
        assert_eq!(frames(&mut p, t0, 1, Some(8), None), None);
        assert!(!p.idle());
        // Not known: render.
        frames(&mut p, t0, QUIET_FRAMES + 2, Some(8), None);
        assert!(p.idle());
        assert_eq!(frames(&mut p, t0, 1, None, None), None);
        // Input: the waiting frame is requested now, not after the idle interval.
        frames(&mut p, t0, QUIET_FRAMES + 2, Some(9), None);
        assert!(p.wait(t0).is_some());
        p.input();
        assert_eq!(p.wait(t0), None);
        assert!(!p.idle());
    }

    #[test]
    fn input_keeps_a_cap() {
        let t0 = Instant::now();
        let mut p = Pacing::default();
        p.frame_done(t0, Some(30), true);
        p.input();
        assert!(p.wait(t0).is_some(), "not idle: the cap still holds");
    }

    #[test]
    fn the_budget_is_the_rate_in_effect_else_the_display() {
        let ms = |d: Duration| (d.as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
        assert_eq!(ms(frame_budget(Some(30), Some(120_000))), 33.33);
        assert_eq!(ms(frame_budget(None, Some(120_000))), 8.33);
        assert_eq!(ms(frame_budget(None, None)), 16.67);
        assert_eq!(ms(frame_budget(Some(IDLE_FPS), None)), 200.0);
        assert_eq!(ms(frame_budget(None, Some(0))), 16.67);
    }

    #[test]
    fn work_reports_every_frame_and_the_target_when_it_changes() {
        #[derive(Default)]
        struct Log(Vec<String>);
        impl WorkSink for Log {
            fn target(&mut self, b: Duration) {
                self.0.push(format!("target {}", b.as_millis()));
            }
            fn report(&mut self, w: Duration) {
                self.0.push(format!("work {}", w.as_millis()));
            }
        }
        let (mut sink, mut work) = (Log::default(), Work::default());
        let ms = Duration::from_millis;
        work.frame(&mut sink, ms(16), ms(3));
        work.frame(&mut sink, ms(16), ms(4));
        work.frame(&mut sink, ms(200), ms(2));
        assert_eq!(sink.0, ["target 16", "work 3", "work 4", "target 200", "work 2"]);
    }

    #[test]
    fn on_demand_follows_power_save_else_android() {
        assert!(on_demand(None, true));
        assert!(!on_demand(None, false));
        assert!(on_demand(Some(true), false));
        assert!(!on_demand(Some(false), true));
    }
}
