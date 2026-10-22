//! Player commands: plain data, applied at the start of a tick in the order given.
//!
//! The host collects the commands of all players for a tick (locally, from a replay or
//! from the network) and passes them to [`crate::World::step`]. Nothing else changes the
//! world, so recording the command stream per tick is a complete replay.

use crate::catalog::TypeId;
use crate::tech::UpgradeId;
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
    /// Research an upgrade (`PerformUpgrade` 0x54f25b through the command 0x4dc386): every
    /// listed building that offers it, is finished and idle, and whose owner can pay tries in
    /// turn; a nation-wide upgrade starts in the first one only (it is no longer offered
    /// meanwhile), a `[PRIVATE]` one in each. The price is paid at the start; the building
    /// stops producing (and mining) until it is done. `upgrade::available` lists what a
    /// building offers.
    Upgrade { player: PlayerId, buildings: Vec<EntityId>, upgrade: UpgradeId },
    /// Cancel the research in a building (`StopUpgrade` 0x4e19ad): full refund, the upgrade
    /// is offered again.
    CancelUpgrade { player: PlayerId, building: EntityId },
    /// Sell `amount` of resource `sell` (at most the stock) for resource `buy` at the market
    /// (`PerformOperation` 0x4a9d80). Resources are indices (`economy::WOOD` …). The
    /// player gets `trunc(amount · sell rate / buy rate)`; `market::Market::quote` tells how
    /// much in advance. The rates are shared by all players and move with every trade.
    Trade { player: PlayerId, sell: u8, buy: u8, amount: i32 },
    /// Plant fields of type `kind` (a `SPRITEOBJECT` such as `FIELD(EN)`) around the listed
    /// mills (the command 0x4e0d88 through 0x51522f): every listed building that is a mill
    /// (`USAGE MELNICA`) with a build list gets a diamond of fields; each field costs its
    /// price. A finished mill does this once by itself with its first product.
    PlantFields { player: PlayerId, buildings: Vec<EntityId>, kind: TypeId },
    /// Switch stand ground on or off (`MakeStandGround` / `CancelStandGround` 0x4e09c7 /
    /// 0x4e0e70): a standing unit attacks only what is in range and never chases. A member
    /// of a formation switches the whole formation. Moves end it.
    StandGround { player: PlayerId, units: Vec<EntityId>, on: bool },
    /// Hold fire (`NoSearchVictim`, the interface's `SetSearchVictim` 0x4e0330): the units
    /// stop attacking and pick no targets on their own until an order of priority 0x80 or
    /// more (a player's attack order) or `hold: false`. A formation member switches the
    /// whole formation.
    HoldFire { player: PlayerId, units: Vec<EntityId>, hold: bool },
    /// Attack-move to a point (`SelSendAndKill`, a group move of priority 0x80): units fight
    /// the enemies they meet on the way and then go on to their places.
    AttackMove { player: PlayerId, units: Vec<EntityId>, x: i32, y: i32 },
    /// Patrol between the units' positions and their places around a point (`Patrol`
    /// 0x4ea2c0): back and forth, fighting enemies on the way like an attack-move.
    Patrol { player: PlayerId, units: Vec<EntityId>, x: i32, y: i32 },
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
            | Command::Gather { player, .. }
            | Command::Upgrade { player, .. }
            | Command::CancelUpgrade { player, .. }
            | Command::Trade { player, .. }
            | Command::PlantFields { player, .. }
            | Command::StandGround { player, .. }
            | Command::HoldFire { player, .. }
            | Command::AttackMove { player, .. }
            | Command::Patrol { player, .. } => *player,
        }
    }
}
