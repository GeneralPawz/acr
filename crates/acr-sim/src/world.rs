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

/// Stable id of a unit or building: its slot in [`World::units`]. Slots are never reused
/// within a match, so ids stay unique and iteration order is creation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u32);

/// Animation a unit plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimKind {
    Stand,
    Rest,
    Rest1,
    MotionL,
    MotionR,
}

/// An order on a unit's order stack (`LocalOrder` chain); the last one is executed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Order {
    /// `NewMonsterSendTo` (order type 12, link 0x5172c6): walk to a lock cell.
    SendTo { cell: (i32, i32), prio: u8 },
    /// `SmartSendTo` (link 0x52b84a): route over the pathfinding areas to `(x, y) + offset`
    /// (world pixels), pushing `SendTo` legs to area centres.
    SmartSendTo { x: i32, y: i32, dx: i32, dy: i32, prio: u8, area: Option<u16> },
}

/// A unit or building (the engine's `OneObject`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Unit {
    pub id: EntityId,
    pub kind: TypeId,
    pub owner: PlayerId,
    pub building: bool,
    pub alive: bool,
    pub life: i32,
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
}

impl Unit {
    /// Lock cell of a position for a footprint of `lx` cells: `(Real − lx·128) >> 8`.
    pub fn cell_of(pos: Pos, lx: i32) -> (i32, i32) {
        ((pos.x - lx * 128) >> 8, (pos.y - lx * 128) >> 8)
    }

    pub fn idle(&self) -> bool {
        self.orders.is_empty() && !self.in_motion
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Player {
    /// Start resources from `RESU` (resource order not mapped yet).
    pub resources: [u32; 8],
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
}

/// Unit indices per 128 px cell (8 × 8 lock cells).
#[derive(Debug, Clone, Default)]
pub struct Buckets {
    side: i32,
    cells: Vec<Vec<u32>>,
}

impl Buckets {
    fn rebuild(&mut self, side_cells: i32, units: &[Unit]) {
        self.side = (side_cells >> 3).max(1);
        self.cells.resize((self.side * self.side) as usize, Vec::new());
        for c in &mut self.cells {
            c.clear();
        }
        for (i, u) in units.iter().enumerate() {
            if u.alive && !u.building {
                let (x, y) = ((u.cell.0 >> 3).clamp(0, self.side - 1), (u.cell.1 >> 3).clamp(0, self.side - 1));
                self.cells[(y * self.side + x) as usize].push(i as u32);
            }
        }
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
        Self {
            tick: Tick(0),
            catalog,
            formations,
            map,
            players: Default::default(),
            units: Vec::new(),
            objects: Vec::new(),
            brigades: Vec::new(),
            occupancy,
            random: SyncRandom::new(0),
            buckets: Buckets::default(),
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
                p.resources = *r;
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
        (world, report)
    }

    /// Places a new unit or (finished) building.
    pub fn spawn(&mut self, kind: TypeId, owner: PlayerId, pos: Pos, dir: u8) -> EntityId {
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
            life: t.life,
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
        };
        if !unit.building && t.mobile() {
            self.occupancy.set_bar(cell.0, cell.1, lx);
        }
        self.units.push(unit);
        id
    }

    pub fn unit(&self, id: EntityId) -> Option<&Unit> {
        self.units.get(id.0 as usize).filter(|u| u.alive)
    }

    /// Advances the world by one tick: applies the commands in order, then runs every unit
    /// once, player by player (`LongProcesses`, 0x5112aa).
    pub fn step(&mut self, commands: &[Command]) {
        for c in commands {
            self.apply(c);
        }
        crate::formation::run_brigades(self);
        self.buckets.rebuild(self.map.land.width, &self.units);
        for player in 0..PLAYERS as u8 {
            for i in 0..self.units.len() {
                let u = &self.units[i];
                if u.owner != player || !u.alive || u.building || !self.catalog.get(u.kind).mobile() {
                    continue;
                }
                crate::motion::process_unit(self, i);
            }
        }
        self.tick.0 += 1;
    }

    fn owned(&self, player: PlayerId, ids: &[EntityId]) -> Vec<usize> {
        let mut v: Vec<usize> = ids
            .iter()
            .map(|id| id.0 as usize)
            .filter(|&i| self.units.get(i).is_some_and(|u| u.alive && u.owner == player))
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
                let Some(t) = self.unit(*target) else { return };
                let (x, y) = t.pos.pixels();
                let ids = self.owned(*player, units);
                crate::motion::group_send(self, &ids, x, y, None);
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
        }
    }

    /// Hash of the whole dynamic state (units, brigades, occupancy, random index, tick),
    /// identical across platforms; two worlds with equal hashes behave identically.
    pub fn state_hash(&self) -> u64 {
        state_hash(&(self.tick.0, self.random, &self.units, &self.brigades, &self.occupancy, &self.players))
    }
}
