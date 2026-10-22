//! Warnings and errors of the game, collected for the driver's answers while the log still
//! goes where it would (`acr.log`, `ACR_LOG`).

use std::sync::Mutex;

/// Log lines of level warning or worse since the last [`take`].
static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// More lines than this between two answers are counted, not kept.
const MAX_LINES: usize = 200;
static DROPPED: Mutex<usize> = Mutex::new(0);

struct Capture {
    inner: env_logger::Logger,
}

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn || self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if record.level() <= log::Level::Warn
            && let Ok(mut lines) = CAPTURED.lock()
        {
            if lines.len() < MAX_LINES {
                lines.push(format!("{} {}: {}", record.level(), record.target(), record.args()));
            } else if let Ok(mut n) = DROPPED.lock() {
                *n += 1;
            }
        }
        if self.inner.matches(record) {
            self.inner.log(record);
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Installs `builder`'s logger with the capture in front of it.
pub fn install(mut builder: env_logger::Builder) {
    let inner = builder.build();
    let max = inner.filter().max(log::LevelFilter::Warn);
    if log::set_boxed_logger(Box::new(Capture { inner })).is_ok() {
        log::set_max_level(max);
    }
}

/// The captured lines since the last call, with a note on how many were dropped.
pub fn take() -> Vec<String> {
    let mut lines = CAPTURED.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
    if let Ok(mut n) = DROPPED.lock()
        && *n > 0
    {
        lines.push(format!("... {} more", std::mem::take(&mut *n)));
    }
    lines
}
