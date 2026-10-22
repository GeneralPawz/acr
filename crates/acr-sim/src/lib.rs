//! Game simulation: units, economy, combat, AI hooks.
//!
//! Rules every module in this crate follows, so replays and lockstep multiplayer work:
//!
//! - The world only changes in [`Tick`] steps of fixed length. Game speed is how many
//!   ticks run per real second, never a side effect of frame rate or CPU speed (the
//!   original engine's flaw).
//! - No floats in simulation state; use integers or fixed point.
//! - All randomness comes from [`Rng`] or [`SyncRandom`], seeded per match.
//! - Iterate collections in a defined order (no `HashMap` iteration).
//! - No I/O, no clocks, no platform APIs. The host feeds in elapsed time and commands.
//!
//! The model and the original algorithms it reproduces are documented in
//! `docs/simulation.md`.

pub mod assets;
pub mod catalog;
pub mod clock;
pub mod combat;
pub mod command;
pub mod fixed;
pub mod formation;
pub mod hash;
pub mod map;
pub mod motion;
pub mod path;
pub mod replay;
pub mod rng;
pub mod trig;
pub mod world;

pub use catalog::{Catalog, ObjectType, TypeId};
pub use clock::{FixedClock, Tick};
pub use command::{Command, PlayerId};
pub use fixed::Pos;
pub use map::MapGrid;
pub use rng::{Rng, SyncRandom};
pub use world::{EntityId, Unit, World};

/// Simulation ticks per second at the original default game speed: the single-player frame
/// limiter (0x4a7589) waits `2 · FPSTime` ms per frame, `FPSTime` defaults to 50 and one
/// simulation step runs per frame.
pub const ORIGINAL_TICKS_PER_SECOND: u32 = 10;
