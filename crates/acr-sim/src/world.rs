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
    /// Reload counter in animation frames (`delay`, OB+0x8c).
    pub delay: i32,
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
    /// Buildings: five 3-bit stage states (OB+0xaa), raised by `UPSTAGEn` upgrades; state
    /// 2 means stage `n` is done. `[STAGEDEPENDENT]` objects need their stages done.
    pub stages: u16,
    /// Buildings: the upgrade being researched (an order in front of the queue).
    pub research: Option<crate::upgrade::Research>,
    /// Buildings: upgrades researched here (`SingleUpgLevel`, OB+0xe9); `[PRIVATE]`
    /// upgrades need it to equal their `#LEVEL`.
    pub upgrade_level: u8,
    /// Buildings: extra capacity from `SINGLE_INSIDE` (`AddInside`, OB+0xa5).
    pub add_inside: i32,
    /// Buildings: extra housing from `MOREFARMS_SINGLE` (u8, OB+0xf3).
    pub farm_extra: i32,
    /// The building this unit is inside (hidden, but counted and fed).
    pub inside_of: Option<EntityId>,
    /// Walks through locks and other units (leaving a building).
    pub unlimited: bool,
    /// Switched off for lack of gold (`TURNOFF`).
    pub turned_off: bool,
    /// Gathered load (`RAmount`) and its source type (`RType`).
    pub carried: i32,
    pub carry_type: Option<u8>,
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
}

/// Unit indices per 128 px cell (8 × 8 lock cells).
#[derive(Debug, Clone, Default)]
pub struct Buckets {
    side: i32,
    cells: Vec<Vec<u32>>,
    /// Owner and position of every unit at the start of the tick.
    snapshot: Vec<(PlayerId, Pos)>,
}

impl Buckets {
    fn rebuild(&mut self, side_cells: i32, units: &[Unit]) {
        self.side = (side_cells >> 3).max(1);
        self.cells.resize((self.side * self.side) as usize, Vec::new());
        for c in &mut self.cells {
            c.clear();
        }
        self.snapshot.clear();
        self.snapshot.extend(units.iter().map(|u| (u.owner, u.pos)));
        for (i, u) in units.iter().enumerate() {
            if u.active() && !u.building {
                let (x, y) = ((u.cell.0 >> 3).clamp(0, self.side - 1), (u.cell.1 >> 3).clamp(0, self.side - 1));
                self.cells[(y * self.side + x) as usize].push(i as u32);
            }
        }
    }

    /// `CheckMotionThroughEnemyAbility` (0x41df98): an enemy (not a building) in the 128 px
    /// cell of `end` that stands within 32 px of `u`.
    pub(crate) fn enemy_near(&self, u: &Unit, end: Pos) -> Option<usize> {
        let (cx, cy) = (end.x >> 11, end.y >> 11);
        self.get(cx, cy).iter().map(|&j| j as usize).find(|&j| {
            let (owner, pos) = self.snapshot[j];
            owner != u.owner && crate::trig::norma(u.pos.x - pos.x, u.pos.y - pos.y) < 0x200
        })
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
        let players = std::array::from_fn(|_| Player::new(&catalog));
        Self {
            tick: Tick(0),
            catalog,
            formations,
            map,
            players,
            units: Vec::new(),
            objects: Vec::new(),
            brigades: Vec::new(),
            occupancy,
            random: SyncRandom::new(0),
            buckets: Buckets::default(),
            deposits: Vec::new(),
            sources: Arc::new(crate::gather::Sources::default()),
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
        let sources = world
            .objects
            .iter()
            .filter_map(|o| {
                let (kind, per_work, radius) = sprites.work_source(o.group, o.index)?;
                Some(crate::gather::Source { x: o.x, y: o.y, kind, per_work, radius })
            })
            .collect();
        world.sources = Arc::new(crate::gather::Sources::new(sources, world.map.land.width * 16));
        crate::economy::bind_mines(&mut world);
        crate::economy::census_all(&mut world);
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
            stand_time: 0,
            prio: 0,
            orders: Vec::new(),
            path: PathState::default(),
            brigade: None,
            death: 0,
            stance: 0,
            local_stance: 0,
            delay: 0,
            kills: 0,
            attack_bonus: 0,
            defence_bonus: 0,
            stage: adv.build_stages,
            queue: Vec::new(),
            inside: Vec::new(),
            mine: None,
            rally: None,
            stages: 0,
            research: None,
            upgrade_level: 0,
            add_inside: 0,
            farm_extra: 0,
            inside_of: None,
            unlimited: false,
            turned_off: false,
            carried: 0,
            carry_type: None,
        };
        if !unit.building && t.mobile() {
            self.occupancy.set_bar(cell.0, cell.1, lx);
        }
        self.units.push(unit);
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
    }

    /// Advances the world by one tick: applies the commands in order, then runs every unit
    /// once, player by player (`LongProcesses`, 0x5112aa).
    pub fn step(&mut self, commands: &[Command]) {
        for c in commands {
            self.apply(c);
        }
        crate::formation::run_brigades(self);
        self.buckets.rebuild(self.map.land.width, &self.units);
        crate::economy::census(self, (self.tick.0 & 7) as u8);
        crate::economy::upkeep(self);
        crate::combat::search_victims(self);
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
                    crate::economy::building_link(self, i);
                } else if self.catalog.get(u.kind).mobile() {
                    crate::motion::process_unit(self, i);
                }
            }
        }
        crate::combat::object_loop(self);
        self.tick.0 += 1;
    }

    fn owned(&self, player: PlayerId, ids: &[EntityId]) -> Vec<usize> {
        let mut v: Vec<usize> = ids
            .iter()
            .map(|id| id.0 as usize)
            .filter(|&i| self.units.get(i).is_some_and(|u| u.active() && u.owner == player))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    fn apply(&mut self, c: &Command) {
        match c {
            Command::Move { player, units, x, y } => {
                let ids = self.owned(*player, units);
                crate::motion::group_send(self, &ids, *x, *y, None);
            }
            Command::Attack { player, units, target } => {
                let t = target.0 as usize;
                for i in self.owned(*player, units) {
                    crate::combat::attack_obj(self, i, t, crate::motion::PLAYER_PRIO);
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
            Command::Gather { player, units, x, y } => {
                if let Some(kind) = self.sources.determine(*x, *y) {
                    for i in self.owned(*player, units) {
                        crate::gather::take_resource(self, i, *x, *y, kind, 0x80, false);
                    }
                }
            }
        }
    }

    /// Hash of the whole dynamic state (units, brigades, occupancy, random index, tick),
    /// identical across platforms; two worlds with equal hashes behave identically.
    pub fn state_hash(&self) -> u64 {
        state_hash(&(self.tick.0, self.random, &self.units, &self.brigades, &self.occupancy, &self.players))
    }
}
