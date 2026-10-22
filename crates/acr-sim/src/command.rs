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
    /// Bombard the ground at a point (`ATTACKGROUND` 0x4d870c through `NewAttackPoint`,
    /// priority 0x90): units that may (`ARTPODGOTOVKA`, cannons) fire at it until stopped,
    /// with `ADDRANGE` % of their range and `ADDPAUSE` % of their reload.
    AttackGround { player: PlayerId, units: Vec<EntityId>, x: i32, y: i32 },
    /// Board a transport (`GoToTransport` 0x54d78d, priority 0x90): land units walk to its
    /// landing point and over the water into it. The transport must have docked once.
    Board { player: PlayerId, units: Vec<EntityId>, transport: EntityId },
    /// Let everybody off a transport at the shore ahead of it (`LeaveAll` 0x54df93): one
    /// passenger every 3 ticks; nothing happens away from a shore.
    Unload { player: PlayerId, transport: EntityId },
    /// Patrol between the units' positions and their places around a point (`Patrol`
    /// 0x4ea2c0): back and forth, fighting enemies on the way like an attack-move.
    Patrol { player: PlayerId, units: Vec<EntityId>, x: i32, y: i32 },
    /// A move that ends facing `dir` (0..=255): the interface's right drag (the move command 2
    /// with a direction, 0x4d5f86). Formations re-form facing it, free units are placed
    /// across it.
    MoveFacing { player: PlayerId, units: Vec<EntityId>, x: i32, y: i32, dir: u8 },
    /// A scripted group move with all options of `GroupSend` (0x4ac776): `SelSendTo`,
    /// `SelSendAndKill` (0x4db0d3) and the `SGP_*` moves (`GroupMovePurpose` 0x5785a9).
    /// `dir` ≥ 0x100 faces the direction of travel; `ord` is the order type of the free
    /// units; `abs_dir` gives brigades `dir` itself; `off_x`/`off_y` shift the target along
    /// and across the direction of travel.
    ScriptMove {
        player: PlayerId,
        units: Vec<EntityId>,
        x: i32,
        y: i32,
        dir: u16,
        prio: u8,
        ord: u8,
        abs_dir: bool,
        off_x: i32,
        off_y: i32,
    },
    /// The interface's friendly-fire buttons (`SHOTMYUNITS` / `KEEPMYUNITS`, network command
    /// 0x5e, 0x4d771a): `shoot_own` switches the `FRIENDLYFIRE` avoidance of the units off
    /// (OB+0x2a bit1).
    FriendlyFire { player: PlayerId, units: Vec<EntityId>, shoot_own: bool },
    /// The interface's `ARMATTACK` / `ANYATTACK` buttons (network command 0x5f, 0x4d7765):
    /// `ARMATTACK` units fight in melee only (OB+0x2a bit2).
    MeleeOnly { player: PlayerId, units: Vec<EntityId>, on: bool },
    /// The `STOPBUILD` switch (network command 0x61, 0x4d77b0, executor 0x4e0718):
    /// buildings whose type has `CANSTOPBUILD` stop (or resume) their production (OB+0xf2
    /// bit0, tested at the start of `ProduceObjLink` 0x49eec5).
    StopProduction { player: PlayerId, buildings: Vec<EntityId>, stop: bool },
    /// The `KARTECH` button (0x62, 0x4d77fb, executor 0x4dc8fa): units with canister
    /// (`WEAPONINSECTOR`) clear their orders and fire one volley at the point 256 px ahead.
    Canister { player: PlayerId, units: Vec<EntityId> },
    /// The `PREKARTECH` button (0x63, 0x4d7836, executor 0x4dca2a): units with canister take
    /// its stance (`NewState` = canister attack index + 1), loading canister.
    LoadCanister { player: PlayerId, units: Vec<EntityId> },
    /// The `FISHING` switch (0x64, 0x4d7871, executor 0x4e07f9): fishing boats stop (or
    /// resume) fishing (OB+0xf2 bit0, read by `Fishing` 0x54e606 and `FishingLink` 0x54eb1f).
    StopFishing { player: PlayerId, units: Vec<EntityId>, stop: bool },
    /// The `ZASEV` switch (0x65, 0x4d78bc, executor 0x4e08d2): mills stop (or resume)
    /// planting fields (OB+0xf2 bit3, read by `CreateFields` 0x5780d2); resuming plants at
    /// once (`CreateFields` of the player).
    StopSowing { player: PlayerId, buildings: Vec<EntityId>, stop: bool },
    /// The officer's formation icons (network command 0x3a, 0x4d748f, executor 0x4e174a →
    /// `CreateBrigade` 0x4198b3): the free officer forms `shape` from exactly its slot count
    /// of free soldiers of `kind` within 1024 px, with a free drummer of its type within
    /// 1024 px (unless it needs none) and a flag bearer if one is near.
    FormNear { player: PlayerId, officer: EntityId, shape: u16, kind: TypeId },
    /// A formed brigade's formation icons (network command 0x41, 0x4d79e8, `ReformBrigade`
    /// 0x4e1cd3): its soldiers take the first variant of `ORD_GROUPS.LST` group `group` with
    /// room for them.
    Reform { player: PlayerId, brigade: u16, group: u8 },
    /// The `FILLFORM` icon (network command 0x52, 0x4d8153, `FillBrigade` 0x4193a1): free
    /// soldiers of the brigade's type within 1024 px take its empty slots.
    Fill { player: PlayerId, brigade: u16 },
    /// The `CM_DOGRP` button (network command 0x58, executor `GroupSelectedFormations`
    /// 0x43d91d): the formations of the units leave their groups and are grouped together;
    /// selecting one of them then selects all (see `grouping`).
    GroupFormations { player: PlayerId, units: Vec<EntityId> },
    /// The `CM_UNGRP` button (0x59, `UnGroupSelectedUnits` 0x43d717): the formations of the
    /// units leave their groups.
    UngroupFormations { player: PlayerId, units: Vec<EntityId> },
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
            | Command::Patrol { player, .. }
            | Command::MoveFacing { player, .. }
            | Command::ScriptMove { player, .. }
            | Command::AttackGround { player, .. }
            | Command::FriendlyFire { player, .. }
            | Command::MeleeOnly { player, .. }
            | Command::StopProduction { player, .. }
            | Command::Canister { player, .. }
            | Command::LoadCanister { player, .. }
            | Command::StopFishing { player, .. }
            | Command::StopSowing { player, .. }
            | Command::Board { player, .. }
            | Command::Unload { player, .. }
            | Command::FormNear { player, .. }
            | Command::Reform { player, .. }
            | Command::Fill { player, .. }
            | Command::GroupFormations { player, .. }
            | Command::UngroupFormations { player, .. } => *player,
        }
    }
}
