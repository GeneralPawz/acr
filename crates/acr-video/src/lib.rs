//! Bink 1 video (`Video\*.bik`, revision `i`) in pure Rust: container, video and audio
//! decoding, and a [`VideoPlayer`] for the intro movies.
//!
//! The original game plays these through RAD's `binkw32.dll`. Everything here is plain
//! computation without platform dependencies, so it runs natively and in the browser.
//! See `docs/formats/bink.md` for the format and what is (not) supported.

pub mod audio;
mod bits;
pub mod container;
mod huffman;
mod idct;
pub mod player;
mod tables;
pub mod video;

use std::fmt;

pub use audio::AudioDecoder;
pub use container::{AudioTrack, BinkFile, Header, Packet};
pub use player::{BinkAudio, VideoPlayer};
pub use video::VideoDecoder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinkError(pub &'static str);

impl fmt::Display for BinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for BinkError {}
