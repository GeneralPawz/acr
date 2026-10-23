//! Savegames (#78).
//!
//! A savegame is not a memory dump: [`World::step`] is deterministic and the commands are
//! its only input, so a save holds how the game was started ([`GameSetup`]), the player's
//! commands per tick, the mission dialogs the player closed and the requests the diplomacy
//! panel sent to DipServer (with their tick), the tick it was saved at and the world's state
//! hash there. Loading starts the same game and
//! fast-forwards to the saved tick ([`FastForward`]), feeding the recorded inputs back in;
//! the mission scripts regenerate their own orders on the way.
//!
//! Binary format (little-endian, integers as LEB128 varints like the replays, signed ones
//! zigzag-encoded, byte strings as length + bytes):
//!
//! ```text
//! "ACRS" | version | name | unix time | local time: year month day hour minute second
//! setup: kind (0 mission: id name map script texts difficulty campaign? [index mission]
//!              1 random map: seed landscape ground resources minerals size climate)
//!        players: count, per player: name nation colour team control (0 human, 1 + difficulty)
//!        local player
//! tick | state hash (u64 LE)
//! commands: length + an `acr_sim::replay::Replay` ("ACRR") holding the command entries
//! dialogs: count, per close: tick delta | answer
//! diplomacy (version 2): count, per input: tick delta | 0 request bytes, or 1 building index
//! camera x | camera y (world pixels) | selection: count + ids | groups: count, per group: ids
//! rules (version 3): bits of the optional rules of the setup (bit 0 hit locations, #225)
//! ```
//!
//! The name, time and setup come first so that the load dialog can list savegames
//! without decoding the rest ([`SaveInfo::peek`]).

use acr_sim::replay::Replay;
use acr_sim::world::EntityId;
use acr_sim::{Command, World};

use crate::ui::setup::{BlankSetup, Control, Difficulty, Ground, MapSize, MissionSetup, PlayerSetup, RandomMapSetup};
use crate::ui::{GameKind, GameSetup};

const MAGIC: &[u8; 4] = b"ACRS";
const VERSION: u64 = 3;

/// An input of the diplomacy panel, given to DipServer between two ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DipInput {
    /// A request of a link or a timer (`DS_SendRequest`).
    Request(Vec<u8>),
    /// A click on a building (`CheckDipBuilding`), by its object index.
    Building(u16),
}

/// The inputs of a running game: player commands, closed mission dialogs and diplomacy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Log {
    /// Ticks that had player commands, in increasing order.
    pub commands: Vec<(u64, Vec<Command>)>,
    /// `(tick, answer)` of every closed mission window (page, question, choice), in order.
    pub dialogs: Vec<(u64, u32)>,
    /// `(tick, input)` of every diplomacy request, in order: it changes the scripts' state
    /// (alliances, purchases) without being a command.
    pub diplomacy: Vec<(u64, DipInput)>,
}

impl Log {
    /// Records the player's commands given to the world for `tick` (script orders are not
    /// recorded: the scripts give them again when the game is replayed).
    pub fn command(&mut self, tick: u64, commands: &[Command]) {
        if !commands.is_empty() {
            self.commands.push((tick, commands.to_vec()));
        }
    }

    /// Records that the player closed a mission window with `answer` while the world stood
    /// at `tick`.
    pub fn dialog(&mut self, tick: u64, answer: u32) {
        self.dialogs.push((tick, answer));
    }

    /// Records a diplomacy input given while the world stood at `tick`.
    pub fn diplomacy(&mut self, tick: u64, input: DipInput) {
        self.diplomacy.push((tick, input));
    }
}

/// Date of a save: seconds since 1970 and the local date and time
/// (year, month, day, hour, minute, second).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaveTime {
    pub unix: u64,
    pub local: [u16; 6],
}

impl SaveTime {
    /// As the original's load dialog shows file dates: `%02d:%02d:%02d   %02d.%02d.%d`
    /// (0x5d79c0).
    pub fn text(&self) -> Vec<u8> {
        let [y, mo, d, h, mi, s] = self.local;
        format!("{h:02}:{mi:02}:{s:02}   {d:02}.{mo:02}.{y}").into_bytes()
    }
}

/// A saved game.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveGame {
    /// The name typed in the save dialog (CP1252), also the file name.
    pub name: Vec<u8>,
    pub time: SaveTime,
    pub setup: GameSetup,
    /// The world's tick when saved: the next tick to run.
    pub tick: u64,
    /// `World::state_hash()` at `tick`.
    pub hash: u64,
    pub log: Log,
    /// View position (world pixels of the top left corner).
    pub camera: [i32; 2],
    pub selected: Vec<EntityId>,
    /// Control groups 0..9.
    pub groups: Vec<Vec<EntityId>>,
}

/// What the load dialog shows of a savegame.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveInfo {
    pub name: Vec<u8>,
    pub time: SaveTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveError(pub String);

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad savegame: {}", self.0)
    }
}

impl std::error::Error for SaveError {}

fn err(what: &str) -> SaveError {
    SaveError(what.to_owned())
}

impl SaveInfo {
    /// Reads name and date only; `None` for other files and other versions (the original
    /// lists only files with its magic and current version, 0x5d79c0).
    pub fn peek(data: &[u8]) -> Option<Self> {
        let mut r = Reader { data, pos: 0 };
        let (name, time, _) = r.header().ok()?;
        Some(Self { name, time })
    }
}

impl SaveGame {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.0.extend_from_slice(MAGIC);
        w.u(VERSION);
        w.bytes(&self.name);
        w.u(self.time.unix);
        for v in self.time.local {
            w.u(u64::from(v));
        }
        w.setup(&self.setup);
        w.u(self.tick);
        w.0.extend_from_slice(&self.hash.to_le_bytes());
        let replay = Replay { entries: self.log.commands.clone(), ..Replay::default() };
        w.bytes(&replay.encode());
        w.u(self.log.dialogs.len() as u64);
        let mut last = 0;
        for &(tick, answer) in &self.log.dialogs {
            w.u(tick - last);
            last = tick;
            w.u(u64::from(answer));
        }
        w.u(self.log.diplomacy.len() as u64);
        let mut last = 0;
        for (tick, input) in &self.log.diplomacy {
            w.u(tick - last);
            last = *tick;
            match input {
                DipInput::Request(bytes) => {
                    w.u(0);
                    w.bytes(bytes);
                }
                DipInput::Building(index) => {
                    w.u(1);
                    w.u(u64::from(*index));
                }
            }
        }
        w.i(self.camera[0]);
        w.i(self.camera[1]);
        w.ids(&self.selected);
        w.u(self.groups.len() as u64);
        for g in &self.groups {
            w.ids(g);
        }
        w.u(u64::from(self.setup.rules.hit_locations) | u64::from(self.setup.rules.body_spacing) << 1);
        w.0
    }

    pub fn decode(data: &[u8]) -> Result<Self, SaveError> {
        let mut r = Reader { data, pos: 0 };
        let (name, time, version) = r.header()?;
        let mut setup = r.setup()?;
        let tick = r.u()?;
        let hash = u64::from_le_bytes(r.take(8)?.try_into().expect("8 bytes"));
        let replay = Replay::decode(&r.bytes()?).map_err(|e| SaveError(e.to_string()))?;
        let n = r.len()?;
        let mut dialogs = Vec::with_capacity(n);
        let mut t = 0u64;
        for _ in 0..n {
            t = t.checked_add(r.u()?).ok_or(err("tick overflow"))?;
            dialogs.push((t, u32::try_from(r.u()?).map_err(|_| err("answer"))?));
        }
        let mut diplomacy = Vec::new();
        if version >= 2 {
            let n = r.len()?;
            let mut t = 0u64;
            for _ in 0..n {
                t = t.checked_add(r.u()?).ok_or(err("tick overflow"))?;
                let input = match r.u()? {
                    0 => DipInput::Request(r.bytes()?),
                    1 => DipInput::Building(u16::try_from(r.u()?).map_err(|_| err("building"))?),
                    _ => return Err(err("diplomacy input")),
                };
                diplomacy.push((t, input));
            }
        }
        let camera = [r.i()?, r.i()?];
        let selected = r.ids()?;
        let n = r.len()?;
        let groups = (0..n).map(|_| r.ids()).collect::<Result<Vec<_>, _>>()?;
        if version >= 3 {
            let bits = r.u()?;
            setup.rules = acr_sim::Rules { hit_locations: bits & 1 != 0, body_spacing: bits & 2 != 0 };
        }
        if r.pos != data.len() {
            return Err(err("trailing bytes"));
        }
        let log = Log { commands: replay.entries, dialogs, diplomacy };
        Ok(Self { name, time, setup, tick, hash, log, camera, selected, groups })
    }
}

/// The game's mission scripts as far as replaying needs them (`MissionLink`).
pub trait Script {
    /// Runs the scripts for the next tick; `None` while the game stands still.
    fn before_step(&mut self, world: &mut World) -> Option<Vec<Command>>;
    fn dialog_open(&self) -> bool;
    /// Closes the open window with the player's answer.
    fn close_dialog(&mut self, answer: u32);
    /// Victory or defeat: the game will not go on.
    fn ended(&self) -> bool;
    /// Gives a recorded diplomacy input to DipServer.
    fn diplomacy(&mut self, world: &mut World, input: &DipInput);
}

/// How far a [`FastForward`] got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Stopped by the time budget; call again.
    Running,
    /// The scripts wait for files; call again later.
    Waiting,
    /// At the saved tick (or the game ended on the way).
    Done,
}

/// Replays a saved game's inputs up to its tick, without drawing or sound. Mission
/// dialogs never show: each is closed the way the player closed it.
#[derive(Debug, Clone)]
pub struct FastForward {
    pub target: u64,
    next_command: usize,
    next_dialog: usize,
    next_dip: usize,
    /// A recorded close did not match an open dialog (or the other way round).
    pub mismatch: bool,
}

impl FastForward {
    pub fn new(target: u64) -> Self {
        Self { target, next_command: 0, next_dialog: 0, next_dip: 0, mismatch: false }
    }

    /// Steps the world while `more()` allows (checked before every tick).
    pub fn run(
        &mut self,
        log: &Log,
        world: &mut World,
        script: &mut dyn Script,
        mut more: impl FnMut() -> bool,
    ) -> Progress {
        loop {
            let tick = world.tick.0;
            // Diplomacy inputs came between ticks, before the next one ran.
            while let Some((t, input)) = log.diplomacy.get(self.next_dip).filter(|(t, _)| *t <= tick) {
                self.mismatch |= *t != tick;
                self.next_dip += 1;
                script.diplomacy(world, input);
            }
            // Closes recorded at the saved tick itself still apply: a question answered
            // just before saving (its tick is re-run with the answer next).
            let pending = log.dialogs.get(self.next_dialog).is_some_and(|&(t, _)| t <= tick);
            if tick >= self.target && !pending {
                return Progress::Done;
            }
            if script.dialog_open() {
                match log.dialogs.get(self.next_dialog) {
                    Some(&(t, answer)) if t <= tick => {
                        self.mismatch |= t != tick;
                        self.next_dialog += 1;
                        script.close_dialog(answer);
                    }
                    _ => {
                        log::warn!("savegame: unexpected mission window at tick {tick}");
                        self.mismatch = true;
                        script.close_dialog(1);
                    }
                }
                continue;
            }
            if tick >= self.target {
                // A close recorded for a question that has not come up yet: run the
                // scripts once more to ask it.
                if script.before_step(world).is_some() || !script.dialog_open() {
                    log::warn!("savegame: no question at tick {tick} for a recorded answer");
                    self.mismatch = true;
                    self.next_dialog = log.dialogs.len();
                }
                continue;
            }
            if !more() {
                return Progress::Running;
            }
            let Some(orders) = script.before_step(world) else {
                if script.dialog_open() {
                    continue;
                }
                return if script.ended() { Progress::Done } else { Progress::Waiting };
            };
            while log.commands.get(self.next_command).is_some_and(|(t, _)| *t < tick) {
                self.next_command += 1;
                self.mismatch = true;
            }
            let mut commands = match log.commands.get(self.next_command) {
                Some((t, c)) if *t == tick => {
                    self.next_command += 1;
                    c.clone()
                }
                _ => Vec::new(),
            };
            commands.extend(orders);
            world.step(&commands);
        }
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn u(&mut self, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.0.push(b);
                return;
            }
            self.0.push(b | 0x80);
        }
    }

    fn i(&mut self, v: i32) {
        self.u(u64::from(((v << 1) ^ (v >> 31)) as u32));
    }

    fn bytes(&mut self, b: &[u8]) {
        self.u(b.len() as u64);
        self.0.extend_from_slice(b);
    }

    fn ids(&mut self, ids: &[EntityId]) {
        self.u(ids.len() as u64);
        for id in ids {
            self.u(u64::from(id.0));
        }
    }

    fn setup(&mut self, s: &GameSetup) {
        match &s.kind {
            GameKind::Mission(m) => {
                self.u(0);
                self.bytes(m.id.as_bytes());
                self.bytes(&m.name);
                self.bytes(m.map.as_bytes());
                self.bytes(m.script.as_bytes());
                self.bytes(m.texts.as_bytes());
                self.u(m.difficulty.index() as u64);
                match m.campaign {
                    Some((c, i)) => {
                        self.u(1);
                        self.u(c as u64);
                        self.u(i as u64);
                    }
                    None => self.u(0),
                }
            }
            GameKind::RandomMap(r) => {
                self.u(1);
                self.u(u64::from(r.seed));
                for v in [r.landscape, r.ground, r.resources, r.minerals] {
                    self.u(v as u64);
                }
                self.u(r.size as u64);
                self.u(r.climate as u64);
            }
            GameKind::Blank(b) => {
                self.u(2);
                self.u(b.size as u64);
                self.u(b.ground as u64);
                let parts = [
                    b.slope.map(|(a, b, c)| [a, b, c, 0]),
                    b.hill.map(<[i32; 4]>::from),
                    b.water.map(<[i32; 4]>::from),
                ];
                for part in parts {
                    match part {
                        Some(values) => {
                            self.u(1);
                            for v in values {
                                self.i(v);
                            }
                        }
                        None => self.u(0),
                    }
                }
            }
        }
        self.u(s.players.len() as u64);
        for p in &s.players {
            self.bytes(&p.name);
            self.u(p.nation as u64);
            self.u(u64::from(p.color));
            self.u(u64::from(p.team));
            self.u(match p.control {
                Control::Human => 0,
                Control::Computer(d) => 1 + d.index() as u64,
            });
        }
        self.u(s.local_player as u64);
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SaveError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or(err("truncated"))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn byte(&mut self) -> Result<u8, SaveError> {
        Ok(self.take(1)?[0])
    }

    fn u(&mut self) -> Result<u64, SaveError> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(err("varint too long"))
    }

    fn small(&mut self) -> Result<usize, SaveError> {
        usize::try_from(self.u()?).ok().filter(|&v| v < 1 << 20).ok_or(err("value out of range"))
    }

    fn len(&mut self) -> Result<usize, SaveError> {
        let n = self.u()?;
        // Every element takes at least one byte.
        usize::try_from(n).ok().filter(|&n| n <= self.data.len() - self.pos).ok_or(err("bad length"))
    }

    fn i(&mut self) -> Result<i32, SaveError> {
        let v = u32::try_from(self.u()?).map_err(|_| err("integer"))?;
        Ok(((v >> 1) as i32) ^ -((v & 1) as i32))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, SaveError> {
        let n = self.len()?;
        Ok(self.take(n)?.to_vec())
    }

    fn str(&mut self) -> Result<String, SaveError> {
        String::from_utf8(self.bytes()?).map_err(|_| err("text"))
    }

    fn ids(&mut self) -> Result<Vec<EntityId>, SaveError> {
        let n = self.len()?;
        (0..n).map(|_| u32::try_from(self.u()?).map(EntityId).map_err(|_| err("entity id"))).collect()
    }

    fn header(&mut self) -> Result<(Vec<u8>, SaveTime, u64), SaveError> {
        if self.take(4)? != MAGIC {
            return Err(err("not a savegame"));
        }
        let version = self.u()?;
        if !(1..=VERSION).contains(&version) {
            return Err(err("unsupported version"));
        }
        let name = self.bytes()?;
        let unix = self.u()?;
        let mut local = [0u16; 6];
        for v in &mut local {
            *v = u16::try_from(self.u()?).map_err(|_| err("date"))?;
        }
        Ok((name, SaveTime { unix, local }, version))
    }

    fn setup(&mut self) -> Result<GameSetup, SaveError> {
        let kind = match self.u()? {
            0 => {
                let id = self.str()?;
                let name = self.bytes()?;
                let map = self.str()?;
                let script = self.str()?;
                let texts = self.str()?;
                let difficulty = Difficulty::from_index(self.small()?);
                let campaign = match self.u()? {
                    0 => None,
                    _ => Some((self.small()?, self.small()?)),
                };
                GameKind::Mission(MissionSetup { id, name, map, script, texts, difficulty, campaign })
            }
            1 => {
                let seed = u32::try_from(self.u()?).map_err(|_| err("seed"))?;
                let [landscape, ground, resources, minerals] =
                    [self.small()?, self.small()?, self.small()?, self.small()?];
                let size = match self.u()? {
                    0 => MapSize::Normal,
                    1 => MapSize::Large,
                    _ => MapSize::Huge,
                };
                let climate = self.small()?;
                GameKind::RandomMap(RandomMapSetup { seed, landscape, ground, resources, minerals, size, climate })
            }
            2 => {
                let size = match self.u()? {
                    0 => MapSize::Normal,
                    1 => MapSize::Large,
                    _ => MapSize::Huge,
                };
                let ground = match self.u()? {
                    0 => Ground::Grass,
                    1 => Ground::Dirt,
                    2 => Ground::Snow,
                    3 => Ground::White,
                    _ => Ground::Greenscreen,
                };
                let mut parts = [None; 3];
                for part in &mut parts {
                    if self.u()? != 0 {
                        *part = Some([self.i()?, self.i()?, self.i()?, self.i()?]);
                    }
                }
                let [slope, hill, water] = parts;
                GameKind::Blank(BlankSetup {
                    size,
                    ground,
                    slope: slope.map(|[a, b, c, _]| (a, b, c)),
                    hill: hill.map(<(i32, i32, i32, i32)>::from),
                    water: water.map(<(i32, i32, i32, i32)>::from),
                    patches: Vec::new(),
                    editor: false,
                })
            }
            _ => return Err(err("game kind")),
        };
        let n = self.len()?;
        let mut players = Vec::with_capacity(n);
        for _ in 0..n {
            let name = self.bytes()?;
            let nation = self.small()?;
            let color = u8::try_from(self.u()?).map_err(|_| err("colour"))?;
            let team = u8::try_from(self.u()?).map_err(|_| err("team"))?;
            let control = match self.small()? {
                0 => Control::Human,
                d => Control::Computer(Difficulty::from_index(d - 1)),
            };
            players.push(PlayerSetup { name, nation, color, team, control });
        }
        let local_player = self.small()?;
        // The rules come at the end of the file (version 3).
        Ok(GameSetup { kind, players, local_player, rules: acr_sim::Rules::default() })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
    use acr_sim::catalog::{Catalog, ObjectType};
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;
    use acr_sim::{Pos, TypeId};

    use super::*;

    const MUSKETEER: &str = "NAME Mus\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
        @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";

    fn world() -> (World, TypeId) {
        let mut c = Catalog::new();
        let mus = c.add(ObjectType::from_def("Mus(EN)", &UnitDef::parse(MUSKETEER.as_bytes())));
        let f = Formations::new(&OrderList::parse(b"#END\r\n#EXIT\r\n"), &OrderGroups::parse(b"0\r\n"));
        (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f)), mus)
    }

    /// A stand-in for a mission script: asks a question at tick 30 (re-running the tick
    /// with the answer like `Scripts::answer`), shows a page at tick 60 and orders a unit
    /// depending on the answer.
    struct Quiz {
        unit: EntityId,
        answer: Option<bool>,
        asked: bool,
        page_shown: bool,
        dialog: Option<bool>,
        /// A diplomacy input arrived: the scripts send a unit away.
        allied: bool,
    }

    impl Script for Quiz {
        fn before_step(&mut self, world: &mut World) -> Option<Vec<Command>> {
            if self.dialog.is_some() {
                return None;
            }
            let tick = world.tick.0;
            if tick == 30 && !self.asked {
                self.asked = true;
                self.dialog = Some(true);
                return None;
            }
            if tick == 60 && !self.page_shown {
                self.page_shown = true;
                self.dialog = Some(false);
            }
            if tick == 31 {
                let x = if self.answer == Some(true) { 2400 } else { 600 };
                return Some(vec![Command::Move { player: 1, units: vec![self.unit], x, y: 900 }]);
            }
            if tick == 90 && self.allied {
                return Some(vec![Command::Move { player: 1, units: vec![self.unit], x: 300, y: 300 }]);
            }
            Some(Vec::new())
        }

        fn dialog_open(&self) -> bool {
            self.dialog.is_some()
        }

        fn close_dialog(&mut self, answer: u32) {
            if self.dialog.take() == Some(true) {
                self.answer = Some(answer == 1);
            }
        }

        fn ended(&self) -> bool {
            false
        }

        fn diplomacy(&mut self, _: &mut World, input: &DipInput) {
            self.allied |= *input == DipInput::Building(7);
        }
    }

    fn setup() -> GameSetup {
        GameSetup {
            kind: GameKind::RandomMap(RandomMapSetup { seed: 12345, size: MapSize::Large, ..Default::default() }),
            players: vec![
                PlayerSetup { name: b"Gr\xfcn".to_vec(), nation: 3, color: 1, team: 0, control: Control::Human },
                PlayerSetup {
                    name: b"CPU".to_vec(),
                    nation: 7,
                    color: 2,
                    team: 1,
                    control: Control::Computer(Difficulty::Hard),
                },
            ],
            local_player: 0,
            rules: Default::default(),
        }
    }

    /// Plays like the game does: player commands at some ticks, the dialogs closed a few
    /// frames after they open.
    fn play(ticks: u64) -> (World, Quiz, Log, Vec<EntityId>) {
        let (mut w, mus) = world();
        let ids: Vec<EntityId> =
            (0..6).map(|i| w.spawn(mus, (i % 2) as u8, Pos::from_pixels(1000 + 30 * i, 1000), 0)).collect();
        let mut quiz =
            Quiz { unit: ids[1], answer: None, asked: false, page_shown: false, dialog: None, allied: false };
        let mut log = Log::default();
        let mut waited = 0;
        while w.tick.0 < ticks {
            let Some(orders) = quiz.before_step(&mut w) else {
                waited += 1;
                if waited == 3 {
                    waited = 0;
                    let answer = u32::from(quiz.dialog == Some(true));
                    log.dialog(w.tick.0, answer);
                    quiz.close_dialog(answer);
                }
                continue;
            };
            if w.tick.0 == 80 && !quiz.allied {
                // The diplomacy panel sends its requests between ticks.
                log.diplomacy(80, DipInput::Request(vec![1, 2, 3]));
                quiz.diplomacy(&mut w, &DipInput::Request(vec![1, 2, 3]));
                log.diplomacy(80, DipInput::Building(7));
                quiz.diplomacy(&mut w, &DipInput::Building(7));
            }
            let mine = match w.tick.0 {
                5 => vec![Command::Move { player: 0, units: vec![ids[0], ids[2]], x: 1800, y: 1500 }],
                45 => vec![Command::Stop { player: 0, units: vec![ids[2]] }],
                70 => vec![Command::Move { player: 0, units: vec![ids[4]], x: 700, y: 1300 }],
                _ => Vec::new(),
            };
            log.command(w.tick.0, &mine);
            let mut commands = mine;
            commands.extend(orders);
            w.step(&commands);
        }
        (w, quiz, log, ids)
    }

    #[test]
    fn save_and_load_reach_the_same_state() {
        for saved_at in [20, 30, 61, 80, 81, 120] {
            let (w, _, log, ids) = play(saved_at);
            let save = SaveGame {
                name: b"Test \xe4".to_vec(),
                time: SaveTime { unix: 1_700_000_000, local: [2026, 10, 22, 3, 4, 5] },
                setup: setup(),
                tick: w.tick.0,
                hash: w.state_hash(),
                log,
                camera: [-12, 3400],
                selected: vec![ids[0], ids[2]],
                groups: vec![vec![], vec![ids[4]]],
            };
            let bytes = save.encode();
            let info = SaveInfo::peek(&bytes).unwrap();
            assert_eq!(info.name, save.name);
            assert_eq!(info.time.text(), b"03:04:05   22.10.2026");
            let back = SaveGame::decode(&bytes).unwrap();
            assert_eq!(back, save);

            let (mut again, mus) = world();
            for i in 0..6 {
                again.spawn(mus, (i % 2) as u8, Pos::from_pixels(1000 + 30 * i, 1000), 0);
            }
            let mut quiz =
                Quiz { unit: ids[1], answer: None, asked: false, page_shown: false, dialog: None, allied: false };
            let mut ff = FastForward::new(back.tick);
            // A small budget: the replay is spread over several calls.
            let mut calls = 0;
            loop {
                let mut n = 0;
                calls += 1;
                match ff.run(&back.log, &mut again, &mut quiz, || {
                    n += 1;
                    n <= 7
                }) {
                    Progress::Done => break,
                    Progress::Running => {}
                    Progress::Waiting => panic!("nothing to wait for"),
                }
            }
            assert!(calls > 1 || saved_at <= 20);
            assert!(!ff.mismatch, "saved at {saved_at}");
            assert_eq!(again.tick.0, back.tick);
            assert_eq!(again.state_hash(), back.hash, "saved at {saved_at}");
        }
        // The recorded answer matters: answered the other way the game ends elsewhere.
        let (w, _, mut log, ids) = play(120);
        log.dialogs[0].1 = 1 - log.dialogs[0].1;
        let (mut again, mus) = world();
        for i in 0..6 {
            again.spawn(mus, (i % 2) as u8, Pos::from_pixels(1000 + 30 * i, 1000), 0);
        }
        let mut quiz =
            Quiz { unit: ids[1], answer: None, asked: false, page_shown: false, dialog: None, allied: false };
        assert_eq!(FastForward::new(120).run(&log, &mut again, &mut quiz, || true), Progress::Done);
        assert_ne!(again.state_hash(), w.state_hash());
        // Without the diplomacy inputs the scripts do not send the unit away either.
        let (w, _, mut log, ids) = play(120);
        log.diplomacy.clear();
        let (mut again, mus) = world();
        for i in 0..6 {
            again.spawn(mus, (i % 2) as u8, Pos::from_pixels(1000 + 30 * i, 1000), 0);
        }
        let mut quiz =
            Quiz { unit: ids[1], answer: None, asked: false, page_shown: false, dialog: None, allied: false };
        assert_eq!(FastForward::new(120).run(&log, &mut again, &mut quiz, || true), Progress::Done);
        assert_ne!(again.state_hash(), w.state_hash());
    }

    #[test]
    fn reads_version_1_saves_without_diplomacy() {
        let (w, _, mut log, _) = play(40);
        log.diplomacy.clear();
        let save = SaveGame {
            name: b"alt".to_vec(),
            time: SaveTime::default(),
            setup: setup(),
            tick: w.tick.0,
            hash: w.state_hash(),
            log,
            camera: [0, 0],
            selected: Vec::new(),
            groups: Vec::new(),
        };
        let mut bytes = save.encode();
        // Version 1 had no diplomacy section and no rules: drop the version, the empty count
        // and the rules.
        assert_eq!(bytes[4], 3);
        bytes[4] = 1;
        assert_eq!(bytes.pop(), Some(0));
        let cut = bytes.len() - 4;
        assert_eq!(bytes[cut..], [0, 0, 0, 0]);
        bytes.remove(cut);
        assert_eq!(SaveGame::decode(&bytes).unwrap(), save);
    }

    #[test]
    fn keeps_the_optional_rules() {
        let mut save = SaveGame {
            name: b"r".to_vec(),
            time: SaveTime::default(),
            setup: setup(),
            tick: 0,
            hash: 0,
            log: Log::default(),
            camera: [0, 0],
            selected: vec![],
            groups: vec![],
        };
        save.setup.rules.hit_locations = true;
        let mut bytes = save.encode();
        assert_eq!(SaveGame::decode(&bytes).unwrap(), save);
        // A version 2 save has no rules: the original ones.
        bytes[4] = 2;
        assert_eq!(bytes.pop(), Some(1));
        assert_eq!(SaveGame::decode(&bytes).unwrap().setup.rules, acr_sim::Rules::default());
    }

    #[test]
    fn keeps_a_blank_field() {
        let mut save = SaveGame {
            name: b"b".to_vec(),
            time: SaveTime::default(),
            setup: setup(),
            tick: 0,
            hash: 0,
            log: Log::default(),
            camera: [0, 0],
            selected: vec![],
            groups: vec![],
        };
        save.setup.kind = GameKind::Blank(BlankSetup {
            size: MapSize::Large,
            ground: Ground::Snow,
            slope: Some((50, 7000, 8000)),
            hill: None,
            water: Some((-1, 2, 3, 4)),
            patches: Vec::new(),
            editor: false,
        });
        assert_eq!(SaveGame::decode(&save.encode()).unwrap(), save);
    }

    #[test]
    fn rejects_damaged_data() {
        let save = SaveGame {
            name: b"x".to_vec(),
            time: SaveTime::default(),
            setup: setup(),
            tick: 0,
            hash: 0,
            log: Log::default(),
            camera: [0, 0],
            selected: vec![],
            groups: vec![],
        };
        let bytes = save.encode();
        assert!(SaveGame::decode(&bytes[..bytes.len() - 1]).is_err());
        assert!(SaveGame::decode(b"KASF\x01\x00\x00\x00").is_err());
        assert!(SaveInfo::peek(b"KASF").is_none());
        let mut long = bytes.clone();
        long.push(0);
        assert!(SaveGame::decode(&long).is_err());
    }
}
