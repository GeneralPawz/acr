//! A tiny frame profiler (docs/rendering-3d.md, "Where a battle's frame goes"): named scopes
//! add their wall time to per-name totals while [`enable`]d; [`take`] returns and clears
//! them. Timing reads the clock only, so the simulation's state (and its hash) never depends
//! on it. Native only: in the browser every scope is a no-op.

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    static ENABLED: AtomicBool = AtomicBool::new(false);

    thread_local! {
        static TOTALS: RefCell<Vec<(&'static str, u64, u32)>> = const { RefCell::new(Vec::new()) };
    }

    pub fn enable(on: bool) {
        ENABLED.store(on, Ordering::Relaxed);
    }

    pub fn enabled() -> bool {
        ENABLED.load(Ordering::Relaxed)
    }

    /// Adds `nanos` (and one call) to `name`.
    pub fn add(name: &'static str, nanos: u64) {
        TOTALS.with(|t| {
            let mut t = t.borrow_mut();
            match t.iter_mut().find(|e| std::ptr::eq(e.0, name) || e.0 == name) {
                Some(e) => {
                    e.1 += nanos;
                    e.2 += 1;
                }
                None => t.push((name, nanos, 1)),
            }
        });
    }

    /// Times its own life under `name` (dropped at the end of the scope).
    pub struct Scope(Option<(&'static str, Instant)>);

    impl Scope {
        pub fn new(name: &'static str) -> Self {
            Self(enabled().then(|| (name, Instant::now())))
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
        TOTALS.with(|t| std::mem::take(&mut *t.borrow_mut()))
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    pub fn enable(_on: bool) {}
    pub fn enabled() -> bool {
        false
    }
    pub fn add(_name: &'static str, _nanos: u64) {}
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
}
