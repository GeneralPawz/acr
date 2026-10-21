//! Readers for the original American Conquest (GSC Game World, 2002) data formats.
//!
//! This crate has no platform dependencies so it builds for native targets and
//! `wasm32-unknown-unknown` alike. Every reader works on `Read + Seek` sources,
//! which in the browser can be backed by user-provided `File` blobs.

pub mod gsc;
pub mod vfs;

pub use gsc::{GscArchive, GscEntry};
pub use vfs::Vfs;

/// Normalizes a game path for lookups: uppercase ASCII, `/` as separator.
///
/// The original engine treats paths case-insensitively and uses `\`.
pub fn normalize_path(path: &str) -> String {
    path.trim_start_matches(['\\', '/'])
        .chars()
        .map(|c| {
            if c == '\\' {
                '/'
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect()
}
