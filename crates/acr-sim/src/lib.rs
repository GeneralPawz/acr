//! Game simulation: units, economy, combat, AI hooks.
//!
//! Rules every module in this crate follows, so replays and lockstep multiplayer work:
//!
//! - The world only changes in [`Tick`] steps of fixed length. Game speed is how many
//!   ticks run per real second, never a side effect of frame rate or CPU speed (the
//!   original engine's flaw).
//! - No floats in simulation state; use integers or fixed point.
//! - All randomness comes from [`Rng`], seeded per match.
//! - Iterate collections in a defined order (no `HashMap` iteration).
//! - No I/O, no clocks, no platform APIs. The host feeds in elapsed time and commands.

pub mod clock;
pub mod rng;

pub use clock::{FixedClock, Tick};
pub use rng::Rng;
