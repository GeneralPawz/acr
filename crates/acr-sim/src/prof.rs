//! A tiny frame profiler (docs/rendering-3d.md, "Where a battle's frame goes"): named scopes
//! add their wall time to per-name totals while [`enable`]d (the 5 s `ACR_PROFILE` block,
//! the driver's `profile`) or [`record`]ed (the game's always-on performance recorder,
//! docs/testing.md "Performance recorder"); [`drain_frame`] hands one frame's values to the
//! recorder, [`take`] returns and clears the totals. Timing reads the clock only, so the
//! simulation's state (and its hash) never depends on it. Native only: in the browser every
//! scope is a no-op.

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    static ENABLED: AtomicBool = AtomicBool::new(false);
    static RECORDING: AtomicBool = AtomicBool::new(false);

    type Entries = Vec<(&'static str, u64, u32)>;

    thread_local! {
        /// The values since the last [`drain_frame`]; names stay (no allocation per frame).
        static FRAME: RefCell<Entries> = const { RefCell::new(Vec::new()) };
        /// The totals since the last [`take`] (only while [`enabled`]).
        static TOTALS: RefCell<Entries> = const { RefCell::new(Vec::new()) };
    }

    pub fn enable(on: bool) {
        ENABLED.store(on, Ordering::Relaxed);
    }

    pub fn enabled() -> bool {
        ENABLED.load(Ordering::Relaxed)
    }

    /// Times the scopes for the performance recorder (without keeping totals).
    pub fn record(on: bool) {
        RECORDING.store(on, Ordering::Relaxed);
    }

    /// Scopes are timed: [`enabled`] or [`record`]ing.
    pub fn timing() -> bool {
        ENABLED.load(Ordering::Relaxed) || RECORDING.load(Ordering::Relaxed)
    }

    fn add_to(entries: &mut Entries, name: &'static str, nanos: u64, calls: u32) {
        match entries.iter_mut().find(|e| std::ptr::eq(e.0, name) || e.0 == name) {
            Some(e) => {
                e.1 = e.1.wrapping_add(nanos);
                e.2 = e.2.wrapping_add(calls);
            }
            None => entries.push((name, nanos, calls)),
        }
    }

    /// Adds `nanos` (and one call) to `name`.
    pub fn add(name: &'static str, nanos: u64) {
        FRAME.with(|t| add_to(&mut t.borrow_mut(), name, nanos, 1));
    }

    /// Hands this thread's values since the last call to `f` (name, nanoseconds, calls; only
    /// names with calls), adds them to the totals while [`enabled`] and clears them.
    pub fn drain_frame(mut f: impl FnMut(&'static str, u64, u32)) {
        let keep = enabled();
        FRAME.with(|frame| {
            let mut frame = frame.borrow_mut();
            TOTALS.with(|totals| {
                let mut totals = totals.borrow_mut();
                for e in frame.iter_mut().filter(|e| e.2 > 0) {
                    f(e.0, e.1, e.2);
                    if keep {
                        add_to(&mut totals, e.0, e.1, e.2);
                    }
                    (e.1, e.2) = (0, 0);
                }
            })
        });
    }

    /// Times its own life under `name` (dropped at the end of the scope).
    pub struct Scope(Option<(&'static str, Instant)>);

    impl Scope {
        pub fn new(name: &'static str) -> Self {
            Self(timing().then(|| (name, Instant::now())))
        }
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            if let Some((name, start)) = self.0 {
                add(name, start.elapsed().as_nanos() as u64);
            }
        }
    }

    /// The totals since the last call, in first-seen order: name, nanoseconds, calls.
    pub fn take() -> Vec<(&'static str, u64, u32)> {
        if enabled() {
            drain_frame(|_, _, _| {});
        }
        TOTALS.with(|t| std::mem::take(&mut *t.borrow_mut()))
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    pub fn enable(_on: bool) {}
    pub fn enabled() -> bool {
        false
    }
    pub fn record(_on: bool) {}
    pub fn timing() -> bool {
        false
    }
    pub fn add(_name: &'static str, _nanos: u64) {}
    pub fn drain_frame(_f: impl FnMut(&'static str, u64, u32)) {}
    pub struct Scope;
    impl Scope {
        pub fn new(_name: &'static str) -> Self {
            Self
        }
    }
    // Like the native scope, so `drop(scope)` ends it on both platforms.
    impl Drop for Scope {
        fn drop(&mut self) {}
    }
    pub fn take() -> Vec<(&'static str, u64, u32)> {
        Vec::new()
    }
}

pub use imp::*;

/// `totals` per frame of `frames`: `name ms/frame (calls/frame)`, one per line; `ticks` is
/// a count (sim ticks run), not a time.
pub fn report(totals: &[(&'static str, u64, u32)], frames: u32) -> String {
    let f = f64::from(frames.max(1));
    totals
        .iter()
        .map(|&(name, n, calls)| {
            if name == "ticks" {
                format!("{name:<24} {:>7.2} /frame", n as f64 / f)
            } else {
                format!("{name:<24} {:>7.2} ms ({:.1} calls)", n as f64 / 1e6 / f, f64::from(calls) / f)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Times the rest of the enclosing block under `name`.
#[macro_export]
macro_rules! prof_scope {
    ($name:expr) => {
        let _prof_scope = $crate::prof::Scope::new($name);
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn totals_add_up_and_clear() {
        super::enable(true);
        {
            crate::prof_scope!("test.a");
        }
        {
            crate::prof_scope!("test.a");
        }
        let t = super::take();
        let a = t.iter().find(|e| e.0 == "test.a").expect("timed");
        assert_eq!(a.2, 2);
        assert!(super::take().iter().all(|e| e.0 != "test.a"));
    }

    #[test]
    fn a_drained_frame_still_counts_in_the_totals() {
        super::enable(true);
        super::add("test.b", 5);
        let mut seen = Vec::new();
        super::drain_frame(|name, n, calls| seen.push((name, n, calls)));
        assert!(seen.contains(&("test.b", 5, 1)));
        super::drain_frame(|name, _, _| assert_ne!(name, "test.b"));
        assert!(super::take().iter().any(|e| e.0 == "test.b" && e.1 == 5));
    }
}
