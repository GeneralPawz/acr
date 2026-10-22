//! The world: map, players, units and buildings, map objects, and the tick loop.

use std::sync::Arc;

use acr_formats::m3d::{M3dMap, SpriteGroup};

use crate::catalog::{Catalog, Medium, TypeId};
use crate::clock::Tick;
use crate::command::{Command, PlayerId};
use crate::fixed::Pos;
use crate::formation::{Brigade, Formations};
use crate::hash::state_hash;
use crate::map::{LockMap, MapGrid, SpriteShapes};
use crate::path::PathState;
use crate::rng::SyncRandom;

/// Player slots in a match.
pub const PLAYERS: usize = 8;

/// Animation frames the engine advances per tick (`FrmDec`, 0x668f10): 2 in every game
/// (forced at 0x4a789d / 0x53ddf3), with `SpeedSh` = 1.
pub const FRM_DEC: i32 = 2;
/// `SpeedSh` (0x668f14).
pub const SPEED_SH: i32 = 1;

/// Stock every player starts with before the map's `RESU` overrides it: the `NRES.DAT
/// [RESTYPES]` amount (500000) for the six resources (0x4eb641).
pub const START_STOCK: [i32; 8] = [500_000, 500_000, 500_000, 500_000, 500_000, 500_000, 0, 0];

/// Stable id of a unit or building: its slot in [`World::units`]. Slots are never reused
/// within a match, so ids stay unique and iteration order is creation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u32);

/// Animation a unit plays; [`AnimKind::name`] is the `.MD` animation slot to draw.
///
/// Attack-related slots carry the attack index `i` (`#ATTACK` for 0, `#ATTACKi` above);
/// index `i` belongs to stance `i + 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimKind {
    Stand,
    Rest,
    Rest1,
    MotionL,
    MotionR,
    /// `#MOTION_Li` / `#MOTION_Ri`: walking in stance `i + 1` (pikemen with the pike
    /// lowered, loaded gatherers, a driving cannon; a cannon's turns are `#MOTION_L1/R1`).
    MotionLs(u8),
    MotionRs(u8),
    /// `#ATTACKi`: a strike or shot (and `#ATTACK3`, the musketeer's reload).
    Attack(u8),
    /// `#PATTACKi`: getting ready for stance `i + 1`.
    PAttack(u8),
    /// `#UATTACKi`: leaving stance `i + 1`.
    UAttack(u8),
    /// `#PSTANDi`: standing in stance `i + 1`.
    PStand(u8),
    /// `#TRANSX3` / `#TRANS3X`: into and out of the reload stance.
    TransX3,
    Trans3X,
    /// `#DEATH`, then lying dead: `#DEATHLIE1..3`.
    Death,
    DeathLie(u8),
    /// `#WORK`: building or repairing.
    Work,
    /// `#BUILDLO_k`: a building under construction (k = 0..=3).
    BuildLo(u8),
}

impl AnimKind {
    /// The `.MD` animation name, e.g. `#ATTACK1`, `#DEATHLIE1`.
    pub fn name(self) -> String {
        let indexed = |base: &str, i: u8| if i == 0 { base.to_owned() } else { format!("{base}{i}") };
        match self {
            AnimKind::Stand => "#STAND".into(),
            AnimKind::Rest => "#REST".into(),
            AnimKind::Rest1 => "#REST1".into(),
            AnimKind::MotionL => "#MOTION_L".into(),
            AnimKind::MotionR => "#MOTION_R".into(),
            AnimKind::MotionLs(i) => format!("#MOTION_L{i}"),
            AnimKind::MotionRs(i) => format!("#MOTION_R{i}"),
            AnimKind::Attack(i) => indexed("#ATTACK", i),
            AnimKind::PAttack(i) => indexed("#PATTACK", i),
            AnimKind::UAttack(i) => indexed("#UATTACK", i),
            AnimKind::PStand(i) => indexed("#PSTAND", i),
            AnimKind::TransX3 => "#TRANSX3".into(),
            AnimKind::Trans3X => "#TRANS3X".into(),
            AnimKind::Death => "#DEATH".into(),
            AnimKind::DeathLie(n) => format!("#DEATHLIE{n}"),
            AnimKind::Work => "#WORK".into(),
            AnimKind::BuildLo(k) => format!("#BUILDLO_{k}"),
        }
    }
}

/// An order on a unit's order stack (`LocalOrder` chain); the last one is executed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Order {
    /// `NewMonsterSendTo` (order type 12, link 0x5172c6): walk to a lock cell.
    SendTo { cell: (i32, i32), prio: u8 },
    /// `SmartSendTo` (link 0x52b84a): route over the pathfinding areas to `(x, y) + offset`
    /// (world pixels), pushing `SendTo` legs to area centres.
    SmartSendTo { x: i32, y: i32, dx: i32, dy: i32, prio: u8, area: Option<u16> },
    /// `AttackObj` (order type 3, link 0x51bcaf): fight a unit until it is dead.
    Attack { target: EntityId, prio: u8 },
    /// `GoToMine` (order type 0xF2, link 0x5f46f7): walk into a building.
    Enter { building: EntityId, prio: u8 },
    /// `BuildObj` (order type 0x59, link 0x524654): build or repair at a build point (lock cell).
    Build { target: EntityId, point: (i32, i32), prio: u8 },
    /// `TakeResource` (order type 0xEE, link 0x4d2fb0): gather sources of type `kind`;
    /// `at` (world pixels) is where the next source is searched from.
    Gather { kind: u8, source: Option<usize>, at: (i32, i32), prio: u8 },
    /// Panic (order type 0x8e, link 0x600720, priority 0xFF): flight legs away from enemies;
    /// `step` counts the legs, `dir` is the way the unit runs when nothing threatens it.
    Panic { step: u16, dir: u8 },
    /// `NewAttackPoint` (order type 0x4c, link 0x51bc5b): shoot at the ground at `(x, y)`
    /// (world pixels), `times` shots (0: until stopped).
    AttackPoint { x: i32, y: i32, prio: u8, times: u16 },
    /// `Patrol` (link 0x4ea3f0, priority 0): alternately walks to `b` and back to `a` (world
    /// pixels), each leg a priority-0 move put in front, so enemies on the way are fought.
    /// `back` is the leg to walk next.
    Patrol { a: (i32, i32), b: (i32, i32), back: bool },
    /// `WaterNewMonsterSendToLink` (order type 12, link 0x54ac93): a transport sails to the
    /// docking box `cell`, then (`turning`) turns its land side and docks.
    WaterSendTo { cell: (i32, i32), prio: u8, turning: bool },
    /// `GoToTransport` (order type 243, link 0x54dbb0): board a transport through its landing
    /// point `land` (world pixels).
    GoToTransport { transport: EntityId, land: (i32, i32), prio: u8 },
    /// `LeaveTransport` (link 0x54e188), on the transport: let one passenger of `kind` off
    /// after `delay` ticks.
    LeaveTransport { kind: TypeId, delay: u8 },
    /// `FishingLink` (order type 221, link 0x54eb16): a fishing boat fishes around and brings
    /// its catch to the nearest own port.
    Fishing,
    /// Turn to a direction (link 0x41bd0d, created by 0x41bfdb): `ROTATE` per tick.
    Turn { dir: u8 },
    /// `ClearOrderedUnlimitedMotion` (0x527fed): ends the walk through locks out of a
    /// building; the unit takes its cell.
    ClearUnlimited,
}

/// A unit or building (the engine's `OneObject`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Unit {
    pub id: EntityId,
    pub kind: TypeId,
    pub owner: PlayerId,
    /// Alliance mask (`NMask`, OB+0x9a): bit `k` set = friend of nation `k`. Two objects are
    /// enemies when their masks share no bit. New objects take their nation's mask
    /// ([`World::nation_mask`]).
    pub nmask: u8,
    pub building: bool,
    /// The object exists. Dead units stay (as corpses) while `death > 0`; use
    /// [`Unit::living`] for units that can act.
    pub alive: bool,
    pub life: i32,
    /// `MaxLife`: the type's life when created; `LIFE`/`MORELIFE`/`PERSONALLIFE` upgrades and
    /// building stages raise it.
    pub max_life: i32,
    /// `Sdoxlo`: 0 while living, then counts up while dying and lying dead.
    pub death: u16,
    /// `RealX/RealY` (1/16 px).
    pub pos: Pos,
    /// `RealDir`, 0 = east, 64 = south.
    pub dir: u8,
    /// Footprint side in lock cells (`Lx`, 1 for land units).
    pub lx: i32,
    /// Lock cell reserved in the occupancy grid (`x/y`): where the unit stands at the end of
    /// its current walking cycle.
    pub cell: (i32, i32),
    /// Steering point (`DestX/DestY`), `None` = no target.
    pub dest: Option<Pos>,
    /// Per-frame velocity while walking (`RealVx/RealVy`).
    pub vel: (i32, i32),
    pub anim: AnimKind,
    /// Frames of the current animation (`NewAnm->NFrames`).
    pub anim_frames: i32,
    /// Current frame (`NewCurSprite`).
    pub frame: i32,
    pub in_motion: bool,
    pub left_leg: bool,
    /// Detour state while blocked (`MoveStage`, 0..=6 or 255).
    pub move_stage: u8,
    /// Steps left to back away from a target inside the minimum attack range (OB+0x5a).
    pub backoff: u8,
    /// Ticks waited before backing away (OB+0x5b, up to 32).
    pub backoff_wait: u8,
    /// Ticks spent standing (`StandTime`, a wrapping byte like the original).
    pub stand_time: u8,
    /// Priority of the current order (`PrioryLevel`).
    pub prio: u8,
    pub orders: Vec<Order>,
    pub path: PathState,
    /// Brigade (formation) this unit belongs to.
    pub brigade: Option<u16>,
    /// Stance (`NewState`): 0 normal, `i + 1` for attack index `i`.
    pub stance: u8,
    /// Stance the animations have reached (`LocalNewState`).
    pub local_stance: u8,
    /// The stance the unit returns to (`GroundState`, OB+0xe8): 1 on an attack-move for
    /// types that walk in stance 1, the carry stance of a loaded gatherer.
    pub ground_state: u8,
    /// Reload counter in animation frames (`delay`, OB+0x8c).
    pub delay: i32,
    /// The reload the last attack set (`MaxDelay`, OB+0x8f): `delay` counts down from it.
    pub max_delay: i32,
    /// `PersonalDelay` (u8 OB+0xa7, 100): reloads take this percentage of `ATTACK_PAUSE`
    /// (`SINGLE_ATTPAUSE` upgrades change it on the researching building).
    pub personal_delay: i32,
    /// Kills (OB+0x51), for the skill damage bonus.
    pub kills: u16,
    /// Formation attack and defence bonus (OB+0x61 / OB+0x62).
    pub attack_bonus: i32,
    pub defence_bonus: i32,
    /// Buildings: construction stage, finished at the type's `BUILDSTAGES`.
    pub stage: i32,
    /// Buildings: production queue, head first.
    pub queue: Vec<crate::economy::Production>,
    /// Buildings: units inside (mine workers, garrison).
    pub inside: Vec<EntityId>,
    /// Buildings: the deposit a mine works.
    pub mine: Option<crate::economy::Mine>,
    /// Buildings: rally point for produced units (`DstX/DstY`).
    pub rally: Option<Pos>,
    /// Buildings: five 3-bit section states (OB+0xaa, see [`crate::upgrade::section_state`]),
    /// raised by `UPSTAGEn` upgrades; state 2 means section `n` is done. `[STAGEDEPENDENT]`
    /// objects need their sections done. Types with sections start at 0x8000 (0x516150).
    pub stages: u16,
    /// Buildings: the upgrade being researched (an order in front of the queue).
    pub research: Option<crate::upgrade::Research>,
    /// Buildings: upgrades researched here (`SingleUpgLevel`, OB+0xe9), from 2 (0x515ff8);
    /// `[PRIVATE]` upgrades need it to equal their `#LEVEL`. Damage to a building with
    /// sections resets it (0x4e98ce).
    pub upgrade_level: u8,
    /// Buildings: extra capacity from `SINGLE_INSIDE` (`AddInside`, OB+0xa5).
    pub add_inside: i32,
    /// Buildings: the reload timers of the shot points (OB+0xac), −2 per tick.
    pub shot_timers: Vec<i32>,
    /// Buildings: extra housing from `MOREFARMS_SINGLE` (u8, OB+0xf3).
    pub farm_extra: i32,
    /// The building this unit is inside (hidden, but counted and fed).
    pub inside_of: Option<EntityId>,
    /// Buildings: being taken down (`UNCONSTRUCT`).
    pub unconstruct: bool,
    /// Walks through locks and other units (leaving a building).
    pub unlimited: bool,
    /// Switched off for lack of gold (`TURNOFF`).
    pub turned_off: bool,
    /// `NoSearchVictim` (OB+0x25 bit1): no automatic targets; attack orders below priority
    /// 0x80 are accepted without effect, and one of 0x80 or more clears the flag.
    pub no_search_victim: bool,
    /// `StandGround` (OB+0x25 bit4): attacks only what is in range, never chases. Group moves
    /// clear it.
    pub stand_ground: bool,
    /// OB+0x2a bit1: friendly-fire avoidance (`FRIENDLYFIRE`) switched off.
    pub no_ff_avoid: bool,
    /// OB+0x2a bit2: melee only (for `ARMATTACK` units).
    pub melee_only: bool,
    /// AI guard (OB+0xf2 bit2, `SetAI_Guard`): never searches targets and is not found by
    /// others' searches unless it attacks; a guarded building kills units that try to take it.
    pub guard: bool,
    /// OB+0xf2 bit0: production stopped (`STOPBUILD`) or fishing stopped (`FISHING`).
    pub switched_off: bool,
    /// OB+0xf2 bit3: a mill plants no fields (`ZASEV`).
    pub no_sowing: bool,
    /// `InPatrol` (OB+0x2a bit0): it has a move of priority below 5 (attack-move, patrol), so
    /// attacks are put in front of the move and the unit resumes it afterwards.
    pub in_patrol: bool,
    /// Fire: hits of slow-burning weapons so far (+0x3f) and burning (+0x26 bit1).
    pub fire: i32,
    pub burning: bool,
    /// Ports: the water point where ships appear (OB+0x69/+0x6b, lock cells).
    pub port_point: Option<(i32, i32)>,
    /// Ships: speed (OB+0xe4, 1/16 px per tick).
    pub speed: i32,
    /// Transports: the docking point (`DstX/DstY`).
    pub dock: crate::ship::Dock,
    /// Transports: the target cell of the last move checked for docking.
    pub park_checked: Option<(i32, i32)>,
    /// Fishing boats: the catch (OB+0x6f), its fraction (OB+0x6d, 1/128) and the spot.
    pub catch: (i32, i32, Option<(i32, i32)>),
    /// Shots left in the current volley (`ZALP`, OB+0x3d).
    pub volley: i32,
    /// Courage (i8, OB+0x53): kills nearby raise it for enemies of the victim and lower it
    /// for the victim's nation; it drifts back to 0 every 256 ticks.
    pub courage: i8,
    /// Gathered load (`RAmount`) and its source type (`RType`).
    pub carried: i32,
    pub carry_type: Option<u8>,
    /// Lasting wounds (only with [`crate::Rules::hit_locations`]; an unwounded unit adds
    /// nothing to the state hash).
    pub wounds: crate::wounds::Wounds,
}

impl Unit {
    /// Lock cell of a position for a footprint of `lx` cells: `(Real − lx·128) >> 8`.
    pub fn cell_of(pos: Pos, lx: i32) -> (i32, i32) {
        ((pos.x - lx * 128) >> 8, (pos.y - lx * 128) >> 8)
    }

    pub fn idle(&self) -> bool {
        self.orders.is_empty() && !self.in_motion
    }

    /// Exists and is not dead or dying.
    pub fn living(&self) -> bool {
        self.alive && self.death == 0
    }

    /// Living and on the map (not inside a building).
    pub fn active(&self) -> bool {
        self.living() && self.inside_of.is_none()
    }

    /// Whether `other` is an enemy (`NMask & NMask == 0`, as `AttackObj` and `SearchVictim`
    /// test it).
    pub fn hostile(&self, other: &Unit) -> bool {
        self.nmask & other.nmask == 0
    }

    /// `DeleteLastOrder` (0x52582d): drops the current order; `InPatrol` is recomputed from
    /// the orders left (a move of priority below 5).
    pub fn pop_order(&mut self) -> Option<Order> {
        let o = self.orders.pop();
        self.in_patrol = self.orders.iter().any(|o| match o {
            Order::SendTo { prio, .. } | Order::SmartSendTo { prio, .. } => *prio < 5,
            _ => false,
        });
        o
    }

    /// Panicking: running away out of control (`DetectPanic` 0x56d9b7: `UnlimitedMotion` and a
    /// panic order).
    pub fn panicking(&self) -> bool {
        self.unlimited && self.orders.iter().any(|o| matches!(o, Order::Panic { .. }))
    }

    /// The unit's current attack target, if it has an attack order.
    pub fn target(&self) -> Option<EntityId> {
        match self.orders.last() {
            Some(Order::Attack { target, .. }) => Some(*target),
            _ => None,
        }
    }
}

/// Static map object: tree, stone, ... (`TRE1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapObject {
    pub group: SpriteGroup,
    pub index: u16,
    /// World pixels.
    pub x: i32,
    pub y: i32,
}

/// A player's economy (the parts of the engine's `Nation` the simulation uses).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Player {
    /// Stock by resource index (`economy::WOOD` ... `COAL`), from `RESU` at the start.
    pub resources: [i32; 8],
    /// Food accumulator (`Harch`): +3 per unit per tick, 1 food eaten per 1000; 0 = famine.
    pub harch: u16,
    /// Summed `RESCONSUMER` rates and their remainders.
    pub consumption: [i32; 8],
    pub consumption_rest: [i32; 8],
    /// Living non-buildings (`NGidot`) and housing (`NFarms`), from the census.
    pub population: i32,
    pub housing: i32,
    /// Gold below 2 (`GoldBunt`): mercenaries desert, `TURNOFF` units stop.
    pub gold_short: bool,
    /// Objects per type (`UnitAmount`), for prices.
    pub type_count: Vec<u16>,
    /// Finished objects per type (`ReadyAmount`, City+0xfb73), for `[ACCESSCONTROL]`.
    pub ready_count: Vec<u16>,
    /// Availability per type (`GeneralObject::Enabled`): only enabled types can be produced
    /// or built. Recomputed with the census (`RefreshAbility` 0x42ffed).
    pub enabled: Vec<bool>,
    /// `CondEnabled`: the type was enabled by `[ENABLED]` or an `ENABLE` upgrade.
    pub cond_enabled: Vec<bool>,
    /// Upgrade states, by [`crate::tech::UpgradeId`].
    pub upgrades: Vec<crate::tech::UpgradeState>,
    /// Characteristics changed by upgrades (`AdvCharacter`); types without an entry use
    /// [`crate::catalog::ObjectType::adv`].
    pub adv: std::collections::BTreeMap<TypeId, Arc<crate::catalog::Adv>>,
    /// Delivery factors (Nation+0x1a72a..): `WoodEff`, `StoneEff` 100; `FoodEff` 1010, whose
    /// hundreds are the field work multiplier and last two digits the food factor in tenths.
    pub food_eff: i32,
    pub wood_eff: i32,
    pub stone_eff: i32,
    /// `GEOLOGY` researched (deposits shown on the minimap).
    pub geology: bool,
    /// Tick of the last death by hunger (City+0xeb16) and of the last command centre lost
    /// (City+0xeb1a), 0 = never: both raise the panic chance for a while.
    pub hungry_time: u32,
    pub cc_lost_time: u32,
    /// Gathering statistics of the city (City+0xeb2c..): wood, stone and food delivered
    /// since `gather_since` (`GlobalTime`, 2 per tick), and the speeds computed from them
    /// (`amount·25/dt` once `dt > 1000`), read by `GetExtraction`.
    pub gathered: [i32; 3],
    pub gather_since: u32,
    pub gather_speed: [i32; 3],
}

impl Player {
    /// A player at the start of a match: `START_STOCK` and the tech tree's initial
    /// availability.
    pub fn new(catalog: &Catalog) -> Self {
        let tech = &catalog.tech;
        let n = catalog.len();
        let start = |t: usize| !tech.loaded || tech.types.get(t).is_some_and(|t| t.start_enabled);
        Self {
            resources: START_STOCK,
            enabled: (0..n).map(start).collect(),
            cond_enabled: (0..n).map(start).collect(),
            upgrades: tech.upgrades.iter().map(crate::tech::UpgradeState::new).collect(),
            food_eff: 1010,
            wood_eff: 100,
            stone_eff: 100,
            ..Self::default()
        }
    }
}

/// What [`World::from_map`] could not place.
#[derive(Debug, Clone, Default)]
pub struct LoadReport {
    /// Unit ids from the map that are not in the catalog.
    pub unknown_types: Vec<String>,
    /// Units whose nation byte is ≥ 8 (the engine skips them).
    pub skipped: usize,
}

/// The complete simulation state.
#[derive(Debug, Clone)]
pub struct World {
    pub tick: Tick,
    pub catalog: Arc<Catalog>,
    pub formations: Arc<Formations>,
    pub map: Arc<MapGrid>,
    pub players: [Player; PLAYERS],
    /// Alliance mask of each nation (`NATIONS[n].NMask`), given to its new objects; `1 << n`
    /// unless scripts change it (`ChangeFriends`).
    pub nation_mask: [u8; PLAYERS],
    pub units: Vec<Unit>,
    pub objects: Vec<MapObject>,
    pub brigades: Vec<Brigade>,
    /// Unit occupancy (`UnitsField`, 0x1100d78): one bit per 16 px cell.
    pub occupancy: LockMap,
    /// The engine's synchronised random source.
    pub random: SyncRandom,
    /// Units per 128 px cell, rebuilt at the start of every tick (derived, not hashed).
    pub buckets: Buckets,
    /// Resource deposits for mines.
    pub deposits: Vec<crate::economy::Deposit>,
    /// Gathering sources (trees, ...), static.
    pub sources: Arc<crate::gather::Sources>,
    /// The market's current rates, shared by all players.
    pub market: crate::market::Market,
    /// Fields and carcasses (sprites that change), by index.
    pub sprites: Vec<crate::sprite::ResSprite>,
    /// `ObjCharacter`s of the `COMPLEX` (fields) and `MEAT` (carcasses) sprite groups.
    pub sprite_kinds: Arc<(crate::sprite::SpriteKinds, crate::sprite::SpriteKinds)>,
    /// The map's other sprites for the spacing of new ones: `(x, y, radius)` (static).
    pub spacing: Arc<Vec<(i32, i32, i32)>>,
    /// Command centres and the panic-chance cache.
    pub morale: crate::morale::MoraleState,
    /// Projectiles in flight.
    pub projectiles: crate::projectile::Projectiles,
    /// Weapon sounds of the last [`World::step`] (cleared at its start; not part of the
    /// state).
    pub sounds: Vec<crate::projectile::WeaponSound>,
    /// Units produced and buildings finished during the last [`World::step`] (the interface
    /// plays their `BORN_SOUND`, 0x49fab7 / 0x5247c2; cleared at its start; not part of the
    /// state).
    pub born: Vec<EntityId>,
    /// Units that started a flight leg in panic during the last [`World::step`] (the
    /// interface shows their shouts, 0x600bc8; cleared at its start; not part of the state).
    pub shouts: Vec<EntityId>,
    /// The 3D boxes of buildings.
    pub bars: crate::bars::Bars,
    /// The AI planning maps (`CantBuild`, `InfoMap`), built at the first use.
    pub plan: Option<Arc<crate::aiplace::PlanMaps>>,
    /// The building side of the computer players' cities.
    pub cities: [crate::aiplace::City; PLAYERS],
    /// Each player's grouped formations (`City::GroupsSet`).
    pub formation_groups: [crate::grouping::FormationGroups; PLAYERS],
    /// Why each dead unit died, by unit index, for the renderer only (deaths by cause,
    /// wounds and blood). Not part of the state hash: savegames replay their commands and
    /// record it again.
    pub deaths: std::collections::HashMap<u32, crate::combat::DeathRecord>,
    /// Recent hits on units and buildings, oldest first, for the renderer only (blood
    /// splatter, later hit reactions): kept for [`crate::combat::HIT_KEEP`] ticks, at most
    /// [`crate::combat::HIT_CAP`]. Not part of the state hash, not saved.
    pub hits: std::collections::VecDeque<crate::combat::HitRecord>,
    /// Recent ground impacts of rounds (cannonballs, grapeshot, missed shots), oldest first,
    /// for the renderer only (craters, scuffs, impact sounds): kept and capped like `hits`.
    /// Not part of the state hash, not saved.
    pub impacts: std::collections::VecDeque<crate::combat::ImpactRecord>,
    /// The flight direction of the round now doing damage (world pixels, not normalised),
    /// set by the projectiles around their damage so that hits record where the ball really
    /// came from (a ricochet) rather than the attacker. Render only, not hashed.
    pub hit_dir: Option<(i32, i32)>,
    /// What is left of a ploughing ball ([`crate::projectile::Plough`]) while it explodes,
    /// handed to its bounce. Only during `explode` and only with the hit-locations rule;
    /// not hashed.
    pub plough_carry: Option<crate::projectile::Plough>,
    /// Coast places, islands and the computer players' landing setup (derived from the map
    /// and refreshed every tick; not hashed, the landings' orders are in `brigades`).
    pub desant: crate::desant::Desant,
    /// The enemy armies of each nation group (`GlobalEnemyInfo`), refreshed every 256 ticks
    /// (derived from the units, not hashed).
    pub armies: crate::armies::EnemyArmies,
    /// Nations run by an AI (`Nat+0x1a729` bit3): they search targets every 8th tick except
    /// towers, and their training buildings still shoot. Set by the host.
    pub ai: [bool; PLAYERS],
    /// Optional rules of this game (from the game setup; the default is the original game).
    /// Hashed only when not the default.
    pub rules: crate::Rules,
}

/// Unit indices per 128 px cell (8 × 8 lock cells).
#[derive(Debug, Clone, Default)]
pub struct Buckets {
    side: i32,
    cells: Vec<Vec<u32>>,
    /// Buildings per 128 px cell (by position), for round damage.
    buildings: Vec<Vec<u32>>,
    /// Nations present in each cell (`NPresence`, a bit per owner).
    presence: Vec<u8>,
    /// Alliance mask and position of every unit at the start of the tick.
    snapshot: Vec<(u8, Pos)>,
}

impl Buckets {
    fn rebuild(&mut self, side_cells: i32, units: &[Unit]) {
        self.side = (side_cells >> 3).max(1);
        self.cells.resize((self.side * self.side) as usize, Vec::new());
        for c in &mut self.cells {
            c.clear();
        }
        self.buildings.resize((self.side * self.side) as usize, Vec::new());
        for c in &mut self.buildings {
            c.clear();
        }
        self.presence.clear();
        self.presence.resize((self.side * self.side) as usize, 0);
        self.snapshot.clear();
        self.snapshot.extend(units.iter().map(|u| (u.nmask, u.pos)));
        for (i, u) in units.iter().enumerate() {
            if u.active() && !u.building {
                let (x, y) = ((u.cell.0 >> 3).clamp(0, self.side - 1), (u.cell.1 >> 3).clamp(0, self.side - 1));
                self.cells[(y * self.side + x) as usize].push(i as u32);
                self.presence[(y * self.side + x) as usize] |= 1 << (u.owner & 7);
            } else if u.living() && u.building {
                let (x, y) = ((u.pos.x >> 11).clamp(0, self.side - 1), (u.pos.y >> 11).clamp(0, self.side - 1));
                self.buildings[(y * self.side + x) as usize].push(i as u32);
                self.presence[(y * self.side + x) as usize] |= 1 << (u.owner & 7);
            }
        }
    }

    /// `CheckMotionThroughEnemyAbility` (0x41df98): an enemy (not a building) in the 128 px
    /// cell of `end` that stands within 32 px of `u`.
    pub(crate) fn enemy_near(&self, u: &Unit, end: Pos) -> Option<usize> {
        let (cx, cy) = (end.x >> 11, end.y >> 11);
        self.get(cx, cy).iter().map(|&j| j as usize).find(|&j| {
            let (mask, pos) = self.snapshot[j];
            mask & u.nmask == 0 && crate::trig::norma(u.pos.x - pos.x, u.pos.y - pos.y) < 0x200
        })
    }

    /// Cells per row (128 px cells).
    pub fn side(&self) -> i32 {
        self.side
    }

    /// Units, then buildings, of a cell.
    pub fn all(&self, x: i32, y: i32) -> impl Iterator<Item = u32> + '_ {
        self.get(x, y).iter().chain(self.buildings_in(x, y)).copied()
    }

    /// The nations present in a cell (`NPresence`): a bit per owner.
    pub fn presence(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.side || y >= self.side {
            return 0;
        }
        self.presence[(y * self.side + x) as usize]
    }

    /// Buildings of a cell (by position).
    pub fn buildings_in(&self, x: i32, y: i32) -> &[u32] {
        if x < 0 || y < 0 || x >= self.side || y >= self.side {
            return &[];
        }
        &self.buildings[(y * self.side + x) as usize]
    }

    pub fn get(&self, x: i32, y: i32) -> &[u32] {
        if x < 0 || y < 0 || x >= self.side || y >= self.side {
            return &[];
        }
        &self.cells[(y * self.side + x) as usize]
    }
}

impl World {
    /// An empty world on a map.
    pub fn new(map: Arc<MapGrid>, catalog: Arc<Catalog>, formations: Arc<Formations>) -> Self {
        let occupancy = LockMap::new(map.land.width, map.land.height);
        let bars = crate::bars::Bars::new(map.land.width * 16);
        let players = std::array::from_fn(|_| Player::new(&catalog));
        let market = catalog.market.as_ref().map(crate::market::Market::new).unwrap_or_default();
        Self {
            tick: Tick(0),
            catalog,
            formations,
            map,
            players,
            nation_mask: std::array::from_fn(|n| 1 << n),
            units: Vec::new(),
            objects: Vec::new(),
            brigades: Vec::new(),
            occupancy,
            random: SyncRandom::new(0),
            buckets: Buckets::default(),
            deposits: Vec::new(),
            sources: Arc::new(crate::gather::Sources::default()),
            market,
            sprites: Vec::new(),
            sprite_kinds: Arc::default(),
            spacing: Arc::default(),
            morale: Default::default(),
            projectiles: Default::default(),
            sounds: Vec::new(),
            born: Vec::new(),
            shouts: Vec::new(),
            bars,
            plan: None,
            cities: Default::default(),
            formation_groups: Default::default(),
            deaths: Default::default(),
            hits: Default::default(),
            impacts: Default::default(),
            hit_dir: None,
            plough_carry: None,
            desant: Default::default(),
            armies: Default::default(),
            ai: [false; PLAYERS],
            rules: Default::default(),
        }
    }

    /// Creates the world of a map: units and buildings from `UNI3`, map objects, resources.
    pub fn from_map(
        m3d: &M3dMap,
        sprites: &SpriteShapes,
        catalog: Arc<Catalog>,
        formations: Arc<Formations>,
    ) -> (Self, LoadReport) {
        let mut grid = MapGrid::from_m3d(m3d, sprites);
        for u in &m3d.units {
            let Some(kind) = catalog.find(&u.name).filter(|_| usize::from(u.nation) < PLAYERS) else { continue };
            for (x, y) in catalog.get(kind).lock_cells(Pos::new(u.x, u.y)) {
                grid.land.set_pt(x, y);
            }
        }
        let map = Arc::new(grid);
        let mut world = World::new(map, catalog, formations);
        let mut report = LoadReport::default();
        if let Some(res) = &m3d.resources {
            for (p, r) in world.players.iter_mut().zip(res) {
                p.resources = r.map(|v| v as i32);
            }
        }
        for u in &m3d.units {
            if usize::from(u.nation) >= PLAYERS {
                report.skipped += 1;
                continue;
            }
            match world.catalog.find(&u.name) {
                Some(kind) => {
                    world.spawn(kind, u.nation, Pos::new(u.x, u.y), u.direction);
                }
                None => report.unknown_types.push(u.name.clone()),
            }
        }
        world.objects = m3d
            .sprites
            .iter()
            .filter(|s| !matches!(s.group, SpriteGroup::Unknown(_)))
            .map(|s| MapObject { group: s.group, index: s.index, x: s.x, y: s.y })
            .collect();
        world.deposits = crate::economy::deposits(&world.objects, sprites);
        let kind_of = |g: SpriteGroup| sprites.kinds.get(&g).cloned().unwrap_or_default();
        world.sprite_kinds = Arc::new((kind_of(SpriteGroup::Complex), kind_of(SpriteGroup::Meat)));
        let dynamic = |g: SpriteGroup| matches!(g, SpriteGroup::Complex | SpriteGroup::Meat);
        for o in world.objects.clone().iter().filter(|o| dynamic(o.group)) {
            world.add_sprite(o.group, o.index, o.x, o.y);
        }
        world.spacing = Arc::new(
            world
                .objects
                .iter()
                .filter(|o| !dynamic(o.group))
                .map(|o| (o.x, o.y, sprites.spacing(o.group, o.index)))
                .collect(),
        );
        let sources = world
            .objects
            .iter()
            .filter(|o| !dynamic(o.group))
            .filter_map(|o| {
                let (kind, per_work, radius) = sprites.work_source(o.group, o.index)?;
                Some(crate::gather::Source { x: o.x, y: o.y, kind, per_work, radius })
            })
            .collect();
        world.sources = Arc::new(crate::gather::Sources::new(sources, world.map.land.width * 16));
        crate::economy::bind_mines(&mut world);
        crate::economy::census_all(&mut world);
        crate::desant::create_cost_places(&mut world);
        (world, report)
    }

    /// Places a new unit or (finished) building.
    pub fn spawn(&mut self, kind: TypeId, owner: PlayerId, pos: Pos, dir: u8) -> EntityId {
        let adv = self.adv(owner, kind);
        let t = self.catalog.get(kind);
        let id = EntityId(u32::try_from(self.units.len()).expect("too many units"));
        let lx = match t.medium {
            Medium::Land => 1,
            Medium::Water => ((t.radius2 << 4) + 3) >> 7,
        }
        .max(1);
        let cell = Unit::cell_of(pos, lx);
        let stand = t.stand.map_or(1, |a| i32::from(a.frames));
        let unit = Unit {
            id,
            kind,
            owner,
            nmask: self.nation_mask[usize::from(owner) & 7],
            building: t.building,
            alive: true,
            life: adv.life,
            max_life: adv.life,
            pos,
            dir,
            lx,
            cell,
            dest: None,
            vel: (0, 0),
            anim: AnimKind::Stand,
            anim_frames: stand,
            frame: 0,
            in_motion: false,
            left_leg: false,
            move_stage: 0,
            backoff: 0,
            backoff_wait: 0,
            stand_time: 0,
            prio: 0,
            orders: Vec::new(),
            path: PathState::default(),
            brigade: None,
            death: 0,
            stance: 0,
            local_stance: 0,
            ground_state: 0,
            delay: 0,
            max_delay: 0,
            personal_delay: 100,
            kills: 0,
            attack_bonus: 0,
            defence_bonus: 0,
            stage: adv.build_stages,
            queue: Vec::new(),
            inside: Vec::new(),
            mine: None,
            rally: None,
            stages: if t.economy.sections.is_empty() { 0 } else { 0x8000 },
            research: None,
            upgrade_level: 2,
            add_inside: 0,
            shot_timers: vec![0; t.shot_points.len()],
            farm_extra: 0,
            inside_of: None,
            unconstruct: false,
            unlimited: false,
            turned_off: false,
            no_search_victim: t.role.auto_no_attack,
            stand_ground: t.role.auto_stand_ground,
            no_ff_avoid: false,
            melee_only: false,
            guard: false,
            switched_off: false,
            no_sowing: false,
            in_patrol: false,
            port_point: if t.port { crate::ship::port_point(&self.map.water, pos) } else { None },
            speed: 0,
            dock: crate::ship::Dock::Unset,
            park_checked: None,
            volley: 0,
            catch: (0, 0, None),
            fire: 0,
            burning: false,
            courage: 0,
            carried: 0,
            carry_type: None,
            wounds: Default::default(),
        };
        if !unit.building && t.mobile() {
            self.occupancy.set_bar(cell.0, cell.1, lx);
        }
        // Add3DBar (CreateObject 0x516b98): the boxes, at full height from the start.
        let (bx, by) = ((pos.x >> 4) + t.location.0, (pos.y >> 4) + 2 * t.location.1);
        for &[x, y, l, wd, h] in &t.bars {
            let (x0, y0) = (bx + x, by + 2 * y);
            self.bars.add(x0, y0, x0 + l + wd, y0 + l - wd, h, id.0);
        }
        self.units.push(unit);
        // `CreateNewMonsterAt` (0x5155c5) registers new units with a computer player's city.
        crate::desant::register_new_unit(self, id.0 as usize);
        id
    }

    /// The characteristics of `kind` for `player`, upgrades included.
    pub fn adv(&self, player: PlayerId, kind: TypeId) -> Arc<crate::catalog::Adv> {
        match self.players[usize::from(player) & 7].adv.get(&kind) {
            Some(a) => a.clone(),
            None => self.catalog.get(kind).adv.clone(),
        }
    }

    /// Construction stages of a unit's type for its owner: a building is finished at this
    /// stage.
    pub fn max_stage(&self, u: &Unit) -> i32 {
        match self.players[usize::from(u.owner) & 7].adv.get(&u.kind) {
            Some(a) => a.build_stages,
            None => self.catalog.get(u.kind).economy.build_stages,
        }
    }

    /// A unit that exists (living or dead), by id.
    pub fn unit(&self, id: EntityId) -> Option<&Unit> {
        self.units.get(id.0 as usize).filter(|u| u.alive)
    }

    /// Living units and buildings, in id order.
    pub fn living(&self) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(|u| u.living())
    }

    /// Living units and buildings of a player, in id order.
    pub fn units_of(&self, player: PlayerId) -> impl Iterator<Item = &Unit> {
        self.living().filter(move |u| u.owner == player)
    }

    /// Living units and buildings within `r` world pixels (octagonal `Norma` distance, like
    /// the engine's zones) of `(x, y)`, in id order.
    pub fn units_in_circle(&self, x: i32, y: i32, r: i32) -> impl Iterator<Item = &Unit> {
        self.living().filter(move |u| {
            let (ux, uy) = u.pos.pixels();
            crate::trig::norma(ux - x, uy - y) <= r
        })
    }

    /// Kills a unit as if it had been struck down (death animation, corpse).
    pub fn kill(&mut self, id: EntityId) {
        if self.unit(id).is_some_and(Unit::living) {
            crate::combat::die(self, id.0 as usize);
        }
    }

    /// Removes a unit at once, without a corpse (scripts' `RemoveUnits`, `EraseGroup`).
    pub fn remove(&mut self, id: EntityId) {
        let Some(u) = self.units.get_mut(id.0 as usize).filter(|u| u.alive) else { return };
        if u.living() && !u.building {
            self.occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
        }
        u.alive = false;
        u.orders.clear();
        self.bars.remove(id.0);
    }

    /// Advances the world by one tick: applies the commands in order, then runs every unit
    /// once, player by player (`LongProcesses`, 0x5112aa).
    pub fn step(&mut self, commands: &[Command]) {
        self.sounds.clear();
        self.born.clear();
        crate::combat::forget_hits(self);
        self.shouts.clear();
        crate::morale::decay(self);
        crate::wounds::bleed(self);
        for c in commands {
            self.apply(c);
        }
        crate::desant::tick(self);
        crate::economy::city_tick(self);
        crate::armies::tick(self);
        if let Some(def) = &self.catalog.market {
            self.market.process(def);
        }
        crate::formation::run_brigades(self);
        self.buckets.rebuild(self.map.land.width, &self.units);
        crate::economy::census(self, (self.tick.0 & 7) as u8);
        crate::economy::upkeep(self);
        crate::combat::search_victims(self);
        crate::ship::process_fishing(self);
        if self.tick.0 & 255 == 0 {
            crate::economy::bind_mines(self);
        }
        for player in 0..PLAYERS as u8 {
            for i in 0..self.units.len() {
                let u = &self.units[i];
                if u.owner != player || !u.active() {
                    continue;
                }
                if u.building {
                    // The attack order goes in front of production (0x51c30a).
                    if let Some(Order::Attack { target, prio }) = u.orders.last().cloned() {
                        crate::combat::building_attack_link(self, i, target, prio);
                    } else {
                        crate::economy::building_link(self, i);
                    }
                    // 0x5115a6: buildings count their reload and shot timers down here (and
                    // the reload once more in the object loop).
                    let u = &mut self.units[i];
                    u.delay = (u.delay - FRM_DEC).max(0);
                    for t in &mut u.shot_timers {
                        *t = if *t > FRM_DEC { *t - FRM_DEC } else { 0 };
                    }
                } else if self.catalog.get(u.kind).mobile() {
                    crate::motion::process_unit(self, i);
                }
            }
        }
        crate::combat::object_loop(self);
        crate::sprite::time_step(self);
        crate::projectile::process(self);
        self.tick.0 += 1;
    }

    fn owned(&self, player: PlayerId, ids: &[EntityId]) -> Vec<usize> {
        let mut v: Vec<usize> = ids
            .iter()
            .map(|id| id.0 as usize)
            .filter(|&i| self.units.get(i).is_some_and(|u| u.active() && u.owner == player && !u.unlimited))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    fn apply(&mut self, c: &Command) {
        match c {
            Command::Move { player, units, x, y } => {
                let ids = self.owned(*player, units);
                crate::motion::group_send(self, &ids, *x, *y, None, crate::motion::PLAYER_PRIO);
            }
            Command::MoveFacing { player, units, x, y, dir } => {
                let ids = self.owned(*player, units);
                crate::motion::group_send(self, &ids, *x, *y, Some(*dir), crate::motion::PLAYER_PRIO);
            }
            Command::Attack { player, units, target } => {
                let t = target.0 as usize;
                // The interface's attack order has priority 0xFE (0x43ce14): 0x90 that also ends
                // stand ground.
                for i in self.owned(*player, units) {
                    crate::combat::attack_obj(self, i, t, 0xFE);
                }
            }
            Command::Stop { player, units } => {
                for i in self.owned(*player, units) {
                    let u = &mut self.units[i];
                    u.orders.clear();
                    u.path.clear();
                }
            }
            Command::Form { player, officer, units, kind } => {
                let ids = self.owned(*player, units);
                if let Some(&o) = self.owned(*player, &[*officer]).first() {
                    crate::formation::form(self, o, &ids, kind);
                }
            }
            Command::Face { player, officer, dir } => {
                if let Some(&o) = self.owned(*player, &[*officer]).first() {
                    crate::formation::face(self, o, *dir);
                }
            }
            Command::Disband { player, officer } => {
                if let Some(&o) = self.owned(*player, &[*officer]).first() {
                    crate::formation::disband(self, o);
                }
            }
            Command::Produce { player, building, kind } => {
                if let Some(&b) = self.owned(*player, &[*building]).first()
                    && usize::from(kind.0) < self.catalog.len()
                    && crate::economy::can_produce(self, b, *kind)
                {
                    crate::economy::produce(self, b, *kind);
                }
            }
            Command::Unproduce { player, building, kind } => {
                if let Some(&b) = self.owned(*player, &[*building]).first() {
                    crate::economy::unproduce(self, b, *kind);
                }
            }
            Command::Enter { player, units, building } => {
                let b = building.0 as usize;
                if self.units.get(b).is_some_and(|u| u.owner == *player) {
                    for i in self.owned(*player, units) {
                        crate::economy::enter(self, i, b, crate::motion::PLAYER_PRIO, false);
                    }
                }
            }
            Command::Leave { player, building } => {
                if let Some(&b) = self.owned(*player, &[*building]).first() {
                    crate::economy::leave(self, b);
                }
            }
            Command::Rally { player, building, x, y } => {
                if let Some(&b) = self.owned(*player, &[*building]).first()
                    && (!self.catalog.tech.loaded
                        || self.catalog.tech.of(self.units[b].kind).is_some_and(|t| t.can_set_dest))
                {
                    self.units[b].rally = Some(Pos::from_pixels(*x, *y));
                }
            }
            Command::Build { player, units, kind, x, y } => {
                let builders = self.owned(*player, units);
                if usize::from(kind.0) < self.catalog.len()
                    && crate::construction::can_build(self, *player, &builders, *kind)
                    && let Some(b) = crate::construction::place(self, *player, *kind, *x, *y)
                {
                    for i in builders {
                        crate::construction::build_obj(self, i, b.0 as usize, crate::motion::PLAYER_PRIO);
                    }
                }
            }
            Command::Repair { player, units, building } => {
                let b = building.0 as usize;
                if self.units.get(b).is_some_and(|u| u.owner == *player) {
                    for i in self.owned(*player, units) {
                        crate::construction::build_obj(self, i, b, crate::motion::PLAYER_PRIO);
                    }
                }
            }
            Command::Upgrade { player, buildings, upgrade } => {
                for b in self.owned(*player, buildings) {
                    crate::upgrade::start(self, b, *upgrade);
                }
            }
            Command::CancelUpgrade { player, building } => {
                if let Some(&b) = self.owned(*player, &[*building]).first() {
                    crate::upgrade::cancel(self, b);
                }
            }
            Command::Trade { player, sell, buy, amount } => {
                if usize::from(*player) < PLAYERS {
                    crate::market::trade(self, *player, usize::from(*sell), usize::from(*buy), *amount);
                }
            }
            Command::PlantFields { player, buildings, kind } => {
                if usize::from(kind.0) < self.catalog.len() {
                    for b in self.owned(*player, buildings) {
                        let u = &self.units[b];
                        let t = self.catalog.get(u.kind);
                        let listed = !self.catalog.tech.loaded
                            || self.catalog.tech.of(u.kind).is_some_and(|tt| !tt.produce.is_empty());
                        if t.economy.mill && listed && u.stage >= self.max_stage(u) {
                            let pos = u.pos;
                            crate::sprite::plant_fields(self, *player, pos.x, pos.y, *kind);
                        }
                    }
                }
            }
            Command::StandGround { player, units, on } => {
                let ids: Vec<EntityId> = self.owned(*player, units).into_iter().map(|i| self.units[i].id).collect();
                crate::states::set_stand_ground(self, &ids, *on);
            }
            Command::HoldFire { player, units, hold } => {
                let ids: Vec<EntityId> = self.owned(*player, units).into_iter().map(|i| self.units[i].id).collect();
                crate::states::hold_fire(self, &ids, *hold);
            }
            Command::FriendlyFire { player, units, shoot_own } => {
                for i in self.owned(*player, units) {
                    if self.catalog.get(self.units[i].kind).avoid_friendly_fire {
                        self.units[i].no_ff_avoid = *shoot_own;
                    }
                }
            }
            Command::MeleeOnly { player, units, on } => {
                for i in self.owned(*player, units) {
                    if self.catalog.get(self.units[i].kind).role.arm_attack {
                        self.units[i].melee_only = *on;
                    }
                }
            }
            Command::StopProduction { player, buildings, stop } => {
                for i in self.owned(*player, buildings) {
                    if self.catalog.get(self.units[i].kind).can_stop_build {
                        self.units[i].switched_off = *stop;
                    }
                }
            }
            Command::StopFishing { player, units, stop } => {
                for i in self.owned(*player, units) {
                    if self.catalog.get(self.units[i].kind).fishing.0 != 0 {
                        self.units[i].switched_off = *stop;
                    }
                }
            }
            Command::FormNear { player, officer, shape, kind } => {
                if let Some(&o) = self.owned(*player, &[*officer]).first() {
                    crate::formation::form_near(self, o, usize::from(*shape), *kind);
                }
            }
            Command::Reform { player, brigade, group } => {
                let b = usize::from(*brigade);
                if self.brigades.get(b).is_some_and(|br| br.alive && br.owner == *player) {
                    crate::formation::reform(self, b, usize::from(*group));
                }
            }
            Command::Fill { player, brigade } => {
                let b = usize::from(*brigade);
                if self.brigades.get(b).is_some_and(|br| br.alive && br.owner == *player) {
                    crate::formation::fill(self, b);
                }
            }
            Command::StopSowing { player, buildings, stop } => {
                for i in self.owned(*player, buildings) {
                    if self.catalog.get(self.units[i].kind).role.usage == crate::catalog::usage::MELNICA {
                        self.units[i].no_sowing = *stop;
                    }
                }
                if !*stop {
                    crate::economy::create_fields(self, *player);
                }
            }
            Command::Canister { player, units } => {
                for i in self.owned(*player, units) {
                    crate::artillery::fire_canister(self, i);
                }
            }
            Command::LoadCanister { player, units } => {
                for i in self.owned(*player, units) {
                    if let Some(s) = self.catalog.get(self.units[i].kind).sector_weapon {
                        self.units[i].stance = s.att.wrapping_add(1);
                    }
                }
            }
            Command::AttackMove { player, units, x, y } => {
                let ids = self.owned(*player, units);
                crate::motion::group_send(self, &ids, *x, *y, None, 0x80);
            }
            Command::AttackGround { player, units, x, y } => {
                for i in self.owned(*player, units) {
                    if self.catalog.get(self.units[i].kind).ground_attack {
                        crate::artillery::attack_point(self, i, *x, *y, 0x90, crate::combat::Placement::Replace, 0);
                    }
                }
            }
            Command::Board { player, units, transport } => {
                let t = transport.0 as usize;
                if self.units.get(t).is_some_and(|u| u.owner == *player) {
                    for i in self.owned(*player, units) {
                        crate::ship::go_to_transport(self, i, t, 0x90);
                    }
                }
            }
            Command::Unload { player, transport } => {
                if let Some(&t) = self.owned(*player, &[*transport]).first() {
                    crate::ship::leave_all(self, t);
                }
            }
            Command::GroupFormations { player, units } => crate::grouping::group(self, *player, units),
            Command::UngroupFormations { player, units } => crate::grouping::ungroup(self, *player, units),
            Command::Patrol { player, units, x, y } => {
                let ids = self.owned(*player, units);
                crate::motion::patrol(self, &ids, *x, *y);
            }
            Command::Gather { player, units, x, y } => {
                if let Some(kind) = crate::gather::determine(self, *x, *y) {
                    for i in self.owned(*player, units) {
                        crate::gather::take_resource(self, i, *x, *y, kind, 0x80, false);
                    }
                }
            }
        }
    }

    /// `Bar3DHeight` (0x443b06): the height of the highest building box above the ground at
    /// a point (pixels), 0 where there is none.
    pub fn bar_height(&self, x: i32, y: i32) -> i32 {
        self.bars.at(x, y, |o| self.units.get(o as usize).is_some_and(|u| u.alive)).0
    }

    /// `Bar3DOwner` (0x443d37): the building whose box that is.
    pub fn bar_owner(&self, x: i32, y: i32) -> Option<usize> {
        self.bars.at(x, y, |o| self.units.get(o as usize).is_some_and(|u| u.alive)).1.map(|o| o as usize)
    }

    /// Hash of the whole dynamic state (units, brigades, occupancy, random index, tick),
    /// identical across platforms; two worlds with equal hashes behave identically.
    pub fn state_hash(&self) -> u64 {
        let hash = state_hash(&(
            self.tick.0,
            self.random,
            &self.units,
            &self.brigades,
            &self.occupancy,
            &self.players,
            self.nation_mask,
            &self.market,
            &self.sprites,
            &self.morale,
            &self.projectiles,
            (&self.bars, self.ai),
        ));
        // Formation groups count only once there are some, so the hashes of games without
        // them stay as they were.
        let hash = if self.formation_groups.iter().all(|g| g.is_empty()) {
            hash
        } else {
            state_hash(&(hash, &self.formation_groups))
        };
        // Likewise the optional rules, only when some are on.
        if self.rules == crate::Rules::default() { hash } else { state_hash(&(hash, self.rules)) }
    }
}
