//! The browser's stand-in for `shot.rs`: no offscreen frames there (the page takes its own
//! screenshots, docs/testing.md "Browser"); only `textures` answers.

use crate::{Game, Mode};

const NO: &str = "not in the browser: the page takes screenshots and frame times itself";

pub fn prime(_game: &mut Game) -> Result<(), String> {
    Err(NO.into())
}

pub fn screenshot(_game: &mut Game, _path: &str, _crop: Option<[u32; 4]>) -> Result<[u32; 2], String> {
    Err(NO.into())
}

pub fn capture(_game: &mut Game, _crop: Option<[u32; 4]>) -> Result<([u32; 2], Vec<u8>), String> {
    Err(NO.into())
}

pub fn write_png(_path: &str, _size: [u32; 2], _rgba: &[u8]) -> Result<(), String> {
    Err(NO.into())
}

pub fn frametimes(_game: &mut Game, _frames: u32) -> Result<serde_json::Value, String> {
    Err(NO.into())
}

pub fn gputime(_game: &mut Game, _frames: u32) -> Result<serde_json::Value, String> {
    Err(NO.into())
}

pub fn perf_frames(
    _game: &mut Game,
    _event_loop: &winit::event_loop::ActiveEventLoop,
    _frames: u32,
    _recorder: bool,
) -> Result<serde_json::Value, String> {
    Err(NO.into())
}

/// The model textures' GPU memory in MB and the streaming counts (`textures`).
pub fn textures(game: &Game) -> serde_json::Value {
    let Mode::InGame(g) = &game.mode else { return serde_json::Value::Null };
    let Some(t) = g.texture_stats() else { return serde_json::Value::Null };
    let mb = |b: u64| (b as f64 / 1e5).round() / 10.0;
    serde_json::json!({
        "loaded_mb": mb(t.loaded_bytes),
        "streamed_mb": mb(t.streamed_bytes),
        "total_mb": mb(t.loaded_bytes + t.streamed_bytes),
        "pending": t.pending,
        "loads": t.loads,
        "evictions": t.evictions,
    })
}
