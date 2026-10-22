//! Engine-side script state: what `dmcr.exe` keeps for the scripts (`ScenaryInterface
//! SCENINF` at 0x28b7748 and the registries around it), shared by the mission and all AI DLLs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use acr_formats::M3dMap;
use acr_formats::m3d::Areas;
use acr_formats::string_table::Texts;
use acr_sim::world::EntityId;
use acr_sim::{Command, TypeId, World};

use crate::texts::MissionTexts;

/// Number of triggers reachable by `Trigg`/`SetTrigg` (u16[512] at 0x28b78bf; the index is
/// a byte, so the bounds check against 0x1ff never fires).
pub const TRIGGERS: usize = 256;
/// Number of script timers (6-byte records at 0x28b77ff).
pub const TIMERS: usize = 32;
/// Light spots (`LSpot[64]` at 0x28b7cbf).
pub const LIGHT_SPOTS: usize = 64;
/// Ticks between two `ProcessScenary` calls: the simulation step 0x4a61e7 runs the script
/// (0x55a1d2) only when `tmtmt & 7 == 7`; the other seven ticks run the AI cities.
pub const SCRIPT_PERIOD: u64 = 8;
/// Timer and central-text decrement per script call: `8 << SpeedSh` with `SpeedSh` = 1
/// (0x55a1ff), i.e. the same units as `GetGlobalTime` (+2 per tick).
pub const TIMER_STEP: i32 = 8 << acr_sim::world::SPEED_SH;

/// A script timer (`GTimer`, 0x28b77ff + 6·n).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Timer {
    /// Remaining time, in `GetGlobalTime` units.
    pub left: i32,
    /// `Used` (+4): set by `RunTimer` and when the timer runs out.
    pub used: bool,
    /// `First` (+5): set when the timer runs out, cleared by `TimerDoneFirst`/`RunTimer`.
    pub first: bool,
}

impl Timer {
    /// Update after `ProcessScenary` (0x55a246..0x55a295).
    pub fn tick(&mut self) {
        if self.left != 0 {
            self.left -= TIMER_STEP;
            if self.left <= 0 {
                self.left = 0;
                self.first = true;
                self.used = true;
            }
        }
    }

    /// `TimerDone` (0x5635f6).
    pub fn done(&self) -> bool {
        self.left == 0 && self.used
    }
}

/// A named zone of the map (`ActiveZone`, 0x11 bytes at 0x28b83e8; from `ZON1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    /// Centre and radius in world pixels.
    pub x: i32,
    pub y: i32,
    pub r: i32,
    pub name: String,
    /// `RegisterVisibleZone` sets `Dir = 'u'`: drawn on the minimap.
    pub visible: bool,
}

/// A named unit group of the map (`ActiveGroup`, 0x31 bytes at 0x28b83dc; from `ZON1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapGroup {
    pub name: String,
    pub members: Vec<Member>,
}

/// A group member: unit index and the serial it had when it joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Member {
    pub id: u16,
    pub serial: u16,
}

/// Empty member slot (`IDS[i] == 0xFFFF`).
pub const NO_UNIT: u16 = 0xFFFF;

/// A scripted per-group process (`SGP_*`, node of the list at group+0x16). Only the head
/// runs, once per script call.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GroupOrder {
    /// `SGP_ComeIntoBuilding` (`'IBLD'`, handler 0x577945): keep sending members into a building.
    ComeIntoBuilding { building: u16, serial: u16 },
    /// `SGP_MoveToPoint` / `SGP_MakeOneStepTo` (`'MOVE'`, handler 0x57840f): ends after 768
    /// ticks, when nobody moves any more, or 128 ticks after the first arrival.
    Move { t0: i64, half: Option<i64>, flags: i32 },
    /// `SGP_RepairBuilding` (`'REPB'`, handler 0x576fa5): repair until whole or gone.
    Repair { building: u16, serial: u16 },
    /// `SGP_TakeResources` (`'TRES'`, handler 0x5773e1): keep gathering `res` near the
    /// nation's accepting buildings (refreshed every 64 ticks).
    TakeResources { nation: u8, res: u8, last: u64, bases: Vec<(i32, i32)> },
}

/// A script unit group (`UnitsGroup`, 0x22 bytes at 0x28b7760).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Group {
    pub members: Vec<Member>,
    /// `+0x11` StartNMask: given to units produced into the group.
    pub start_nmask: u8,
    /// `+0x16`: scripted processes, head first.
    pub orders: Vec<GroupOrder>,
}

/// A unit a script ordered (`ProduceUnit`): when it is born it joins `group`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Production {
    pub building: EntityId,
    pub kind: TypeId,
    /// Destination group.
    pub group: usize,
    /// Unused (kept for the record layout of older snapshots).
    pub ready: u64,
}

/// The market (`ECO` at 0x9d5f28; one set of prices for all nations), indexed by economy
/// index: 0 food, 1 iron, 2 wood, 3 coal, 4 stone, 5 gold. Values from `ECONOMY.TXT`.
#[derive(Debug, Clone, PartialEq)]
pub struct Market {
    pub buy: [f64; 6],
    pub sell: [f64; 6],
    /// `(min, nominal, max)`.
    pub buy_range: [[f64; 3]; 6],
    pub sell_range: [[f64; 3]; 6],
    pub buy_change: [f64; 6],
    pub sell_change: [f64; 6],
}

impl Default for Market {
    fn default() -> Self {
        let buy_range = [
            [20.0, 25.0, 40.0],
            [140.0, 190.0, 240.0],
            [40.0, 50.0, 60.0],
            [140.0, 190.0, 240.0],
            [75.0, 90.0, 105.0],
            [270.0, 300.0, 330.0],
        ];
        let sell_range = [
            [5.0, 15.0, 20.0],
            [80.0, 110.0, 140.0],
            [20.0, 30.0, 40.0],
            [80.0, 110.0, 140.0],
            [30.0, 45.0, 60.0],
            [180.0, 210.0, 240.0],
        ];
        // `CHANGE 100 1.0`: weight / (amount * 100.0f).
        let change = 1.0 / f64::from(100.0f32 * 100.0f32);
        Self {
            buy: buy_range.map(|r| r[1]),
            sell: sell_range.map(|r| r[1]),
            buy_range,
            sell_range,
            buy_change: [change; 6],
            sell_change: [change; 6],
        }
    }
}

/// A light spot (`SetLightSpot`): reveals the fog around a point for the local player.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LightSpot {
    pub x: i32,
    pub y: i32,
    pub kind: i32,
}

/// Settings the scripts can query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptSetup {
    /// Difficulty chosen in the menu (`CITY[n].Difficulty`): 0 easy, 1 normal, 2 hard,
    /// 3 very hard. Missions give it to nations 0..=6.
    pub difficulty: u8,
    /// The human player's nation (`GetMyNation`).
    pub local_player: u8,
}

impl Default for ScriptSetup {
    fn default() -> Self {
        Self { difficulty: 1, local_player: 0 }
    }
}

/// Per-unit data the scripts need that the simulation does not keep.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct UnitExtra {
    /// Alliance mask override (`OneObject+0x9a`); `None` = the nation's mask.
    pub nmask: Option<u8>,
    /// Serial bit 0 flips when the unit changes owner by capture.
    pub serial_flip: bool,
}

/// The engine-side state of the script API.
#[derive(Debug, Clone)]
pub struct ScriptState {
    pub setup: ScriptSetup,
    pub texts: MissionTexts,
    /// Raw trigger words as stored: `!value`, so a zeroed array reads as all ones and every
    /// trigger is 0xFF at mission start (the scripts rely on it).
    pub triggers: [u16; TRIGGERS],
    pub timers: [Timer; TIMERS],
    pub light_spots: [LightSpot; LIGHT_SPOTS],
    /// The local player's fog of war while the scripts run (the host puts it here for each
    /// tick, see `Scripts::set_fog`); `None` reads as everything visible, like fog off.
    pub fog: Option<acr_sim::fog::Fog>,
    /// `TextDisable[52]`: hidden `{A}`..`{z}` sections of `#BRIEFING`.
    pub text_disable: [bool; 52],
    /// `Victory`, `LooseGame` flags; once set the game stops.
    pub victory: bool,
    pub defeat: bool,
    /// Central text and its remaining time (`CTextTime`, -16 per script call).
    pub central_text: Option<(Vec<u8>, i32)>,
    pub zones: Vec<Zone>,
    pub map_groups: Vec<MapGroup>,
    /// `ZGRP`: zone groups, each the zones with one name.
    pub zone_groups: Vec<Vec<usize>>,
    /// `UGRP`: script unit groups.
    pub groups: Vec<Group>,
    /// Registered variables `(machine, address, size)`: saved with the game.
    pub vars: Vec<(u8, u32, u32)>,
    /// Upgrade names in registration order (until the simulation has upgrades).
    pub upgrades: Vec<String>,
    /// `AssignTBL[8]` (0x6714f0): nation remap applied to many `byte Nation` arguments.
    pub assign: [u8; 8],
    /// Alliance masks of the nations (`Nation+0x11ed1`), bit k = nation k is a friend.
    pub nmask: [u8; 8],
    /// `CITY[n].Difficulty`.
    pub difficulty: [u8; 8],
    /// The AI landing's groups per nation (`CITY+0xf4e`, `+0xf50`): start and destination
    /// group (see [`ScriptState::sync_desant`]).
    pub desant: [(Option<usize>, Option<usize>); 8],
    /// Player names set by `SetPlayerName`.
    pub player_names: [Option<Vec<u8>>; 8],
    /// Nations whose AI was switched on by `StartAI`: `(nation, DLL path)`.
    pub ai_started: Vec<(u8, String)>,
    pub extra: BTreeMap<u32, UnitExtra>,
    /// Per-nation selection (`Selm`/`SerN`/`NSL`).
    pub selection: [Vec<Member>; 8],
    /// `SCENINF+0x54`: API errors (bad handles, unknown names).
    pub errors: u32,
    pub error_log: Vec<String>,
    /// Calls per API function (for coverage reports).
    pub calls: BTreeMap<&'static str, u64>,
    /// Calls to functions that have no entry at all.
    pub unknown_calls: BTreeMap<String, u64>,
    /// Orders given while loading (DllMain, InitAI), delivered with the first tick.
    pub pending_commands: Vec<Command>,
    /// The game's interface texts (`GetTextByID`; `Comment.txt`, `Text/*.txt`, ...).
    pub strings: Texts,
    /// Answers to questions for the script call being (re)run, in order.
    pub answers: VecDeque<u32>,
    /// Questions answered in the current call (to know which answer is next).
    pub answers_used: usize,
    /// AI parameters given by `StartAI`: land type per nation (`GetLandType`).
    pub land_type: [u8; 8],
    /// The map's `TOP1` areas ("top zones").
    pub topo: Option<Areas>,
    /// Script productions waiting for their unit to be born.
    pub production: Vec<Production>,
    /// Units already looked at for productions.
    pub seen_units: usize,
    /// Build-menu overrides `(nation, is upgrade, index) -> +1 enabled / -1 disabled`.
    pub availability: BTreeMap<(u8, bool, u16), i8>,
    pub market: Market,
    /// The AI cities (projects of the computer players).
    pub cities: [crate::aicity::City; 8],
    /// Unit indices mirrored into each machine (`GetOBJ`), refreshed around AI calls.
    pub mirrors: BTreeMap<u8, BTreeSet<u16>>,
    /// Game files the scripts may read (`RReset`/`Gopen`; DipServer's `Dip\*`), by normalized
    /// path; files they write live here too.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Open file handles: path and read/write position.
    pub open_files: BTreeMap<u32, (String, usize)>,
    pub next_file: u32,
    /// `SendDipCommand` data waiting for `PerformDipCommand`.
    pub dip_commands: Vec<Vec<u8>>,
    /// Log every API call into `trace_log` (`acr mission-run --trace`).
    pub trace: bool,
    pub trace_log: Vec<String>,
    /// Guest copies handed out (`GetTextByID` strings, link lists), per machine and key.
    pub text_cache: BTreeMap<(u8, Vec<u8>), u32>,
}

impl ScriptState {
    pub fn new(setup: ScriptSetup, texts: MissionTexts) -> Self {
        let mut difficulty = [0; 8];
        difficulty[..7].fill(setup.difficulty);
        Self {
            setup,
            texts,
            triggers: [0; TRIGGERS],
            timers: [Timer::default(); TIMERS],
            light_spots: [LightSpot::default(); LIGHT_SPOTS],
            fog: None,
            text_disable: [false; 52],
            victory: false,
            defeat: false,
            central_text: None,
            zones: Vec::new(),
            map_groups: Vec::new(),
            zone_groups: Vec::new(),
            groups: Vec::new(),
            vars: Vec::new(),
            upgrades: Vec::new(),
            assign: [0, 1, 2, 3, 4, 5, 6, 7],
            nmask: std::array::from_fn(|n| 1 << n),
            difficulty,
            desant: [(None, None); 8],
            player_names: Default::default(),
            ai_started: Vec::new(),
            extra: BTreeMap::new(),
            selection: Default::default(),
            errors: 0,
            error_log: Vec::new(),
            calls: BTreeMap::new(),
            unknown_calls: BTreeMap::new(),
            pending_commands: Vec::new(),
            strings: Texts::default(),
            answers: VecDeque::new(),
            answers_used: 0,
            land_type: [0; 8],
            topo: None,
            production: Vec::new(),
            seen_units: 0,
            availability: BTreeMap::new(),
            market: Market::default(),
            cities: Default::default(),
            mirrors: BTreeMap::new(),
            files: BTreeMap::new(),
            open_files: BTreeMap::new(),
            next_file: 0x4000,
            dip_commands: Vec::new(),
            trace: false,
            trace_log: Vec::new(),
            text_cache: BTreeMap::new(),
        }
    }

    /// Takes the map's named zones and groups (`ZON1`) and its areas (`TOP1`). The loader binds
    /// every group member position to the unit standing there (0x543bef); [`World::from_map`]
    /// creates the units at their `UNI3` positions, so the unit is found by its position.
    pub fn bind_map(&mut self, map: &M3dMap, world: &World) {
        self.topo = map.land_areas.clone();
        self.seen_units = world.units.len();
        self.zones = map
            .zones
            .iter()
            .map(|z| Zone { x: z.x, y: z.y, r: z.radius, name: z.name.clone(), visible: false })
            .collect();
        let mut at: BTreeMap<(i32, i32), u32> = BTreeMap::new();
        for u in &world.units {
            at.entry((u.pos.x, u.pos.y)).or_insert(u.id.0);
        }
        self.map_groups = map
            .groups
            .iter()
            .map(|g| MapGroup {
                name: g.name.clone(),
                members: g
                    .members
                    .iter()
                    .filter_map(|p| at.get(p))
                    .map(|&id| Member { id: id as u16, serial: self.serial(EntityId(id)) })
                    .collect(),
            })
            .collect();
    }

    /// The serial number of a unit (`OneObject+0x71`). Slots are never reused in ACR, so the
    /// serial only has to change on capture (bit 0, as in 0x5f4cfe).
    pub fn serial(&self, id: EntityId) -> u16 {
        let flip = self.extra.get(&id.0).is_some_and(|e| e.serial_flip);
        ((id.0 as u16) << 1) ^ u16::from(flip)
    }

    /// The alliance mask of a unit (`OneObject+0x9a`).
    pub fn unit_nmask(&self, id: EntityId, owner: u8) -> u8 {
        self.extra.get(&id.0).and_then(|e| e.nmask).unwrap_or(self.nmask[usize::from(owner & 7)])
    }

    /// Hands the alliance masks to the simulation, which tells friend from foe by them
    /// (`NMask` of the nations and of every object).
    pub fn sync_alliances(&self, world: &mut World) {
        world.nation_mask = self.nmask;
        for u in &mut world.units {
            u.nmask = self.unit_nmask(u.id, u.owner);
        }
    }

    /// Exchanges the AI landing's groups with the simulation (`acr_sim::desant`): the units
    /// the last step took from a start group leave it, which is cleaned first as the engine
    /// does before taking (0x56d0cc: alive or hidden units with their serial); the units
    /// that landed join the destination group (0x56dc5f: appended unless there); then the
    /// start groups' units go to the simulation for the next step. `mission`: a mission
    /// script runs.
    pub fn sync_desant(&mut self, world: &mut World, mission: bool) {
        world.desant.mission = mission;
        for n in 0..8 {
            let (start, dest) = self.desant[n];
            let taken = std::mem::take(&mut world.desant.cities[n].taken);
            let landed = std::mem::take(&mut world.desant.cities[n].landed);
            let valid = |st: &Self, m: Member| {
                m.id != NO_UNIT
                    && world.units.get(usize::from(m.id)).is_some_and(|u| u.alive && u.living())
                    && st.serial(EntityId(u32::from(m.id))) == m.serial
            };
            if let Some(g) = start
                && !taken.is_empty()
            {
                let kept: Vec<Member> = self.groups[g]
                    .members
                    .iter()
                    .copied()
                    .filter(|&m| valid(self, m) && !taken.contains(&EntityId(u32::from(m.id))))
                    .collect();
                self.groups[g].members = kept;
            }
            if let Some(g) = dest {
                for id in landed {
                    if !self.groups[g].members.iter().any(|m| u32::from(m.id) == id.0) {
                        let serial = self.serial(id);
                        self.groups[g].members.push(Member { id: id.0 as u16, serial });
                    }
                }
            }
            world.desant.cities[n].start = start.map(|g| {
                self.groups[g].members.iter().filter(|&&m| valid(self, m)).map(|m| EntityId(u32::from(m.id))).collect()
            });
        }
    }

    /// Opens a file of [`ScriptState::files`] and returns its handle.
    pub fn open_file(&mut self, path: String) -> u32 {
        self.next_file += 1;
        self.open_files.insert(self.next_file, (path, 0));
        self.next_file
    }

    /// Reports a script error like `IntErr` (0x55b581; silent in the original unless an
    /// `ERR.LOG` file exists) and counts it.
    pub fn error(&mut self, message: impl Into<String>) {
        self.errors += 1;
        if self.error_log.len() < 256 {
            self.error_log.push(message.into());
        }
    }

    /// Per-call bookkeeping after `ProcessScenary` (0x55a1d2): central text time and timers.
    pub fn after_script(&mut self) {
        if let Some((_, t)) = &mut self.central_text {
            *t -= TIMER_STEP;
            if *t <= 0 {
                self.central_text = None;
            }
        }
        for t in &mut self.timers {
            t.tick();
        }
    }
}
