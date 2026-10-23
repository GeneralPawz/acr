//! Sound effects and music.
//!
//! The original game plays effects through DirectSound and music through a separate
//! process (`vopl.exe`) that outlives the game. Here both go through one [`Mixer`] owned
//! by the game, so music stops with it.
//!
//! [`wav`] and [`music`] decode, [`Mixer`] mixes (pure, deterministic, testable) and
//! `output` (feature `output`) feeds a mixer to the audio device natively or via Web Audio.

pub mod mixer;
pub mod music;
#[cfg(feature = "output")]
pub mod output;
pub mod wav;

pub use mixer::{Meter, Mixer, VoiceId};
pub use music::{MusicSource, VorbisMusic};
pub use wav::Sound;
