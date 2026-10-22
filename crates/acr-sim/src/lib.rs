//! Game simulation: units, economy, combat, AI hooks.
//!
//! Rules every module in this crate follows, so replays and lockstep multiplayer work:
//!
//! - The world only changes in [`Tick`] steps of fixed length. Game speed is how many
//!   ticks run per real second, never a side effect of frame rate or CPU speed (the
//!   original engine's flaw).
//! - No floats in simulation state; use integers or fixed point. The one exception is the
//!   market (`market`), whose rates the original keeps as doubles; it uses only IEEE
//!   additions, multiplications and divisions, which are exact and identical everywhere.
//! - All randomness comes from [`Rng`] or [`SyncRandom`], seeded per match.
//! - Iterate collections in a defined order (no `HashMap` iteration).
//! - No I/O, no clocks, no platform APIs. The host feeds in elapsed time and commands.
//!
//! The model and the original algorithms it reproduces are documented in
//! `docs/simulation.md`.

pub mod aiplace;
pub mod armies;
pub mod artillery;
pub mod assets;
pub mod bars;
pub mod body;
pub mod capture;
pub mod catalog;
pub mod clock;
pub mod combat;
pub mod command;
pub mod construction;
pub mod desant;
pub mod economy;
pub mod fixed;
pub mod fog;
pub mod formation;
pub mod gather;
pub mod ground_fire;
pub mod grouping;
pub mod hash;
pub mod map;
pub mod market;
pub mod morale;
pub mod motion;
pub mod path;
pub mod predict;
pub mod projectile;
pub mod replay;
pub mod rng;
pub mod ship;
pub mod sprite;
pub mod states;
pub mod tech;
pub mod trig;
pub mod upgrade;
pub mod weapon;
pub mod world;
pub mod wounds;

pub use catalog::{Catalog, ObjectType, TypeId};
pub use clock::{FixedClock, Tick};
pub use command::{Command, PlayerId};
pub use fixed::Pos;
pub use map::MapGrid;
pub use rng::{Rng, SyncRandom};
pub use world::{EntityId, Unit, World};
pub use wounds::Rules;

/// Simulation ticks per second at the original default game speed: the single-player frame
/// limiter (0x4a7589) waits `2 · FPSTime` ms per frame, `FPSTime` defaults to 50 and one
/// simulation step runs per frame.
pub const ORIGINAL_TICKS_PER_SECOND: u32 = 10;
