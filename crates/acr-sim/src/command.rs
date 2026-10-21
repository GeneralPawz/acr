//! Player commands: plain data, applied at the start of a tick in the order given.
//!
//! The host collects the commands of all players for a tick (locally, from a replay or
//! from the network) and passes them to [`crate::World::step`]. Nothing else changes the
//! world, so recording the command stream per tick is a complete replay.

use crate::world::EntityId;

/// Player slot 0..=7 (the map's nation byte).
pub type PlayerId = u8;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    /// Walk to a point in world pixels. Units of a formation keep it (the formation moves);
    /// other units spread around the point.
    Move { player: PlayerId, units: Vec<EntityId>, x: i32, y: i32 },
    /// Drop the current order and stand.
    Stop { player: PlayerId, units: Vec<EntityId> },
    /// Attack an object (not simulated yet: units walk to it).
    Attack { player: PlayerId, units: Vec<EntityId>, target: EntityId },
    /// Lead soldiers into formation `kind` (an `ORDERS.LST` name) under `officer`.
    Form { player: PlayerId, officer: EntityId, units: Vec<EntityId>, kind: String },
    /// Turn a formation (identified by its officer) to face `dir` (0..=255) in place.
    Face { player: PlayerId, officer: EntityId, dir: u8 },
    /// Release the members of the formation led by `officer`.
    Disband { player: PlayerId, officer: EntityId },
}

impl Command {
    pub fn player(&self) -> PlayerId {
        match self {
            Command::Move { player, .. }
            | Command::Stop { player, .. }
            | Command::Attack { player, .. }
            | Command::Form { player, .. }
            | Command::Face { player, .. }
            | Command::Disband { player, .. } => *player,
        }
    }
}
