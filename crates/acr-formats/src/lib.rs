//! Readers for the original American Conquest (GSC Game World, 2002) data formats.
//!
//! This crate has no platform dependencies so it builds for native targets and
//! `wasm32-unknown-unknown` alike. Every reader works on `Read + Seek` sources,
//! which in the browser can be backed by user-provided `File` blobs.

pub mod bmp;
pub mod defs;
pub mod font;
pub mod gp;
pub mod grd;
pub mod gsc;
pub mod m3d;
pub mod pal;
pub mod raster;
pub mod rlc;
pub mod text;
pub mod vfs;

pub use bmp::Bitmap;
pub use defs::{NationDef, NdsFile, Piece, PieceLinks, SoundList, UnitDef};
pub use font::{Font, FontColors, named_font};
pub use gp::{Encoding, Frame, GpFile, Layer};
pub use gsc::{GscArchive, GscEntry, GscIndex};
pub use m3d::{M3dError, M3dMap, MapSize};
pub use pal::{Palette, sprite_palette};
pub use text::CodePage;
pub use vfs::{Located, Vfs};

/// Normalizes a game path for lookups: uppercase ASCII, `/` as separator.
///
/// The original engine treats paths case-insensitively and uses `\`.
pub fn normalize_path(path: &str) -> String {
    path.trim_start_matches(['\\', '/']).chars().map(|c| if c == '\\' { '/' } else { c.to_ascii_uppercase() }).collect()
}
