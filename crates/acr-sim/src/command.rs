//! Player commands: plain data, applied at the start of a tick in the order given.
//!
//! The host collects the commands of all players for a tick (locally, from a replay or
//! from the network) and passes them to [`crate::World::step`]. Nothing else changes the
//! world, so recording the command stream per tick is a complete replay.

use crate::catalog::TypeId;
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
    /// Attack an object (`AttackObj`, see `combat`).
    Attack { player: PlayerId, units: Vec<EntityId>, target: EntityId },
    /// Lead soldiers into formation `kind` (an `ORDERS.LST` name) under `officer`.
    Form { player: PlayerId, officer: EntityId, units: Vec<EntityId>, kind: String },
    /// Turn a formation (identified by its officer) to face `dir` (0..=255) in place.
    Face { player: PlayerId, officer: EntityId, dir: u8 },
    /// Release the members of the formation led by `officer`.
    Disband { player: PlayerId, officer: EntityId },
    /// Queue a unit in a building; the price is paid at once.
    Produce { player: PlayerId, building: EntityId, kind: TypeId },
    /// Cancel one queued unit of `kind` (full refund).
    Unproduce { player: PlayerId, building: EntityId, kind: TypeId },
    /// Walk into a building (peasants into mines, units into garrisons).
    Enter { player: PlayerId, units: Vec<EntityId>, building: EntityId },
    /// Let everybody out of a building.
    Leave { player: PlayerId, building: EntityId },
    /// Set a building's rally point (world pixels).
    Rally { player: PlayerId, building: EntityId, x: i32, y: i32 },
    /// Place a building at world pixel `(x, y)` (paid at once) and send `units` to build it.
    Build { player: PlayerId, units: Vec<EntityId>, kind: TypeId, x: i32, y: i32 },
    /// Send units to build or repair an existing building.
    Repair { player: PlayerId, units: Vec<EntityId>, building: EntityId },
    /// Send peasants to gather the source nearest to world pixel `(x, y)` (within 64 px).
    Gather { player: PlayerId, units: Vec<EntityId>, x: i32, y: i32 },
}

impl Command {
    pub fn player(&self) -> PlayerId {
        match self {
            Command::Move { player, .. }
            | Command::Stop { player, .. }
            | Command::Attack { player, .. }
            | Command::Form { player, .. }
            | Command::Face { player, .. }
            | Command::Disband { player, .. }
            | Command::Produce { player, .. }
            | Command::Unproduce { player, .. }
            | Command::Enter { player, .. }
            | Command::Leave { player, .. }
            | Command::Rally { player, .. }
            | Command::Build { player, .. }
            | Command::Repair { player, .. }
            | Command::Gather { player, .. } => *player,
        }
    }
}
