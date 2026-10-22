//! Replays: the command stream of a match, tick by tick, with state hashes to detect
//! divergence. Because [`World::step`](crate::World::step) is deterministic and commands
//! are its only input, replaying the commands on the same map and data reproduces the
//! match exactly.
//!
//! Binary format (little-endian, integers as LEB128 varints, signed ones zigzag-encoded):
//!
//! ```text
//! "ACRR" | version | map path (len + UTF-8) | random index
//! entries: count, then per entry: tick delta | command count | commands
//! checkpoints: count, then per checkpoint: tick | hash (u64 LE)
//! ```
//!
//! A command is a tag byte followed by its fields (see [`Command`]).

use crate::catalog::TypeId;
use crate::command::Command;
use crate::tech::UpgradeId;
use crate::world::EntityId;

const MAGIC: &[u8; 4] = b"ACRR";
const VERSION: u64 = 1;

/// A recorded match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replay {
    /// Game path of the map (`Missions\England\01\eng01.m3d`).
    pub map: String,
    /// Initial index of the synchronised random table.
    pub random_index: u16,
    /// Ticks that had commands, in increasing order.
    pub entries: Vec<(u64, Vec<Command>)>,
    /// `(tick, World::state_hash())` after that tick, for divergence checks.
    pub checkpoints: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError(pub &'static str);

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad replay: {}", self.0)
    }
}

impl std::error::Error for ReplayError {}

impl Replay {
    pub fn new(map: &str, random_index: u16) -> Self {
        Self { map: map.to_owned(), random_index, ..Self::default() }
    }

    /// Records the commands given to the world for `tick` (empty lists are skipped).
    pub fn record(&mut self, tick: u64, commands: &[Command]) {
        if !commands.is_empty() {
            self.entries.push((tick, commands.to_vec()));
        }
    }

    /// The commands recorded for `tick`.
    pub fn commands_at(&self, tick: u64) -> &[Command] {
        match self.entries.binary_search_by_key(&tick, |e| e.0) {
            Ok(i) => &self.entries[i].1,
            Err(_) => &[],
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.0.extend_from_slice(MAGIC);
        w.u(VERSION);
        w.str(&self.map);
        w.u(u64::from(self.random_index));
        w.u(self.entries.len() as u64);
        let mut last = 0;
        for (tick, cmds) in &self.entries {
            w.u(tick - last);
            last = *tick;
            w.u(cmds.len() as u64);
            for c in cmds {
                w.command(c);
            }
        }
        w.u(self.checkpoints.len() as u64);
        for &(tick, hash) in &self.checkpoints {
            w.u(tick);
            w.0.extend_from_slice(&hash.to_le_bytes());
        }
        w.0
    }

    pub fn decode(data: &[u8]) -> Result<Self, ReplayError> {
        let mut r = Reader { data, pos: 0 };
        if r.take(4)? != MAGIC {
            return Err(ReplayError("not a replay"));
        }
        if r.u()? != VERSION {
            return Err(ReplayError("unsupported version"));
        }
        let map = r.str()?;
        let random_index = u16::try_from(r.u()?).map_err(|_| ReplayError("random index"))?;
        let n = r.len()?;
        let mut entries = Vec::with_capacity(n.min(1 << 16));
        let mut tick = 0u64;
        for _ in 0..n {
            tick = tick.checked_add(r.u()?).ok_or(ReplayError("tick overflow"))?;
            let m = r.len()?;
            let cmds = (0..m).map(|_| r.command()).collect::<Result<Vec<_>, _>>()?;
            entries.push((tick, cmds));
        }
        let n = r.len()?;
        let mut checkpoints = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let tick = r.u()?;
            let hash = u64::from_le_bytes(r.take(8)?.try_into().expect("8 bytes"));
            checkpoints.push((tick, hash));
        }
        if r.pos != data.len() {
            return Err(ReplayError("trailing bytes"));
        }
        Ok(Self { map, random_index, entries, checkpoints })
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

    fn str(&mut self, s: &str) {
        self.u(s.len() as u64);
        self.0.extend_from_slice(s.as_bytes());
    }

    fn ids(&mut self, ids: &[EntityId]) {
        self.u(ids.len() as u64);
        for id in ids {
            self.u(u64::from(id.0));
        }
    }

    fn command(&mut self, c: &Command) {
        match c {
            Command::Move { player, units, x, y } => {
                self.0.extend_from_slice(&[0, *player]);
                self.ids(units);
                self.i(*x);
                self.i(*y);
            }
            Command::Stop { player, units } => {
                self.0.extend_from_slice(&[1, *player]);
                self.ids(units);
            }
            Command::Attack { player, units, target } => {
                self.0.extend_from_slice(&[2, *player]);
                self.ids(units);
                self.u(u64::from(target.0));
            }
            Command::Form { player, officer, units, kind } => {
                self.0.extend_from_slice(&[3, *player]);
                self.u(u64::from(officer.0));
                self.ids(units);
                self.str(kind);
            }
            Command::Face { player, officer, dir } => {
                self.0.extend_from_slice(&[4, *player]);
                self.u(u64::from(officer.0));
                self.0.push(*dir);
            }
            Command::Disband { player, officer } => {
                self.0.extend_from_slice(&[5, *player]);
                self.u(u64::from(officer.0));
            }
            Command::Produce { player, building, kind } => {
                self.0.extend_from_slice(&[6, *player]);
                self.u(u64::from(building.0));
                self.u(u64::from(kind.0));
            }
            Command::Unproduce { player, building, kind } => {
                self.0.extend_from_slice(&[7, *player]);
                self.u(u64::from(building.0));
                self.u(u64::from(kind.0));
            }
            Command::Enter { player, units, building } => {
                self.0.extend_from_slice(&[8, *player]);
                self.ids(units);
                self.u(u64::from(building.0));
            }
            Command::Leave { player, building } => {
                self.0.extend_from_slice(&[9, *player]);
                self.u(u64::from(building.0));
            }
            Command::Rally { player, building, x, y } => {
                self.0.extend_from_slice(&[10, *player]);
                self.u(u64::from(building.0));
                self.i(*x);
                self.i(*y);
            }
            Command::Build { player, units, kind, x, y } => {
                self.0.extend_from_slice(&[11, *player]);
                self.ids(units);
                self.u(u64::from(kind.0));
                self.i(*x);
                self.i(*y);
            }
            Command::Repair { player, units, building } => {
                self.0.extend_from_slice(&[12, *player]);
                self.ids(units);
                self.u(u64::from(building.0));
            }
            Command::Gather { player, units, x, y } => {
                self.0.extend_from_slice(&[13, *player]);
                self.ids(units);
                self.i(*x);
                self.i(*y);
            }
            Command::Upgrade { player, buildings, upgrade } => {
                self.0.extend_from_slice(&[14, *player]);
                self.ids(buildings);
                self.u(u64::from(upgrade.0));
            }
            Command::CancelUpgrade { player, building } => {
                self.0.extend_from_slice(&[15, *player]);
                self.u(u64::from(building.0));
            }
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ReplayError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or(ReplayError("truncated"))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn byte(&mut self) -> Result<u8, ReplayError> {
        Ok(self.take(1)?[0])
    }

    fn u(&mut self) -> Result<u64, ReplayError> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(ReplayError("varint too long"))
    }

    fn len(&mut self) -> Result<usize, ReplayError> {
        let n = self.u()?;
        // Every element takes at least one byte.
        usize::try_from(n).ok().filter(|&n| n <= self.data.len() - self.pos).ok_or(ReplayError("bad length"))
    }

    fn i(&mut self) -> Result<i32, ReplayError> {
        let v = u32::try_from(self.u()?).map_err(|_| ReplayError("integer"))?;
        Ok(((v >> 1) as i32) ^ -((v & 1) as i32))
    }

    fn id(&mut self) -> Result<EntityId, ReplayError> {
        u32::try_from(self.u()?).map(EntityId).map_err(|_| ReplayError("entity id"))
    }

    fn kind(&mut self) -> Result<TypeId, ReplayError> {
        u16::try_from(self.u()?).map(TypeId).map_err(|_| ReplayError("type id"))
    }

    fn ids(&mut self) -> Result<Vec<EntityId>, ReplayError> {
        let n = self.len()?;
        (0..n).map(|_| self.id()).collect()
    }

    fn str(&mut self) -> Result<String, ReplayError> {
        let n = self.len()?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| ReplayError("text"))
    }

    fn command(&mut self) -> Result<Command, ReplayError> {
        let tag = self.byte()?;
        let player = self.byte()?;
        Ok(match tag {
            0 => Command::Move { player, units: self.ids()?, x: self.i()?, y: self.i()? },
            1 => Command::Stop { player, units: self.ids()? },
            2 => Command::Attack { player, units: self.ids()?, target: self.id()? },
            3 => Command::Form { player, officer: self.id()?, units: self.ids()?, kind: self.str()? },
            4 => Command::Face { player, officer: self.id()?, dir: self.byte()? },
            5 => Command::Disband { player, officer: self.id()? },
            6 => Command::Produce { player, building: self.id()?, kind: self.kind()? },
            7 => Command::Unproduce { player, building: self.id()?, kind: self.kind()? },
            8 => Command::Enter { player, units: self.ids()?, building: self.id()? },
            9 => Command::Leave { player, building: self.id()? },
            10 => Command::Rally { player, building: self.id()?, x: self.i()?, y: self.i()? },
            11 => Command::Build { player, units: self.ids()?, kind: self.kind()?, x: self.i()?, y: self.i()? },
            12 => Command::Repair { player, units: self.ids()?, building: self.id()? },
            13 => Command::Gather { player, units: self.ids()?, x: self.i()?, y: self.i()? },
            14 => Command::Upgrade {
                player,
                buildings: self.ids()?,
                upgrade: u16::try_from(self.u()?).map(UpgradeId).map_err(|_| ReplayError("upgrade id"))?,
            },
            15 => Command::CancelUpgrade { player, building: self.id()? },
            _ => return Err(ReplayError("unknown command")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_command() {
        let mut r = Replay::new("Missions\\England\\01\\eng01.m3d", 7);
        r.record(0, &[Command::Move { player: 1, units: vec![EntityId(3), EntityId(400)], x: -5, y: 70_000 }]);
        r.record(5, &[]);
        r.record(
            9,
            &[
                Command::Stop { player: 0, units: vec![] },
                Command::Attack { player: 2, units: vec![EntityId(1)], target: EntityId(2) },
                Command::Form { player: 3, officer: EntityId(9), units: vec![EntityId(10)], kind: "LINE".into() },
                Command::Face { player: 3, officer: EntityId(9), dir: 200 },
                Command::Disband { player: 3, officer: EntityId(9) },
                Command::Produce { player: 1, building: EntityId(4), kind: TypeId(300) },
                Command::Unproduce { player: 1, building: EntityId(4), kind: TypeId(2) },
                Command::Enter { player: 1, units: vec![EntityId(5)], building: EntityId(4) },
                Command::Leave { player: 1, building: EntityId(4) },
                Command::Rally { player: 1, building: EntityId(4), x: -3, y: 9 },
                Command::Build { player: 1, units: vec![EntityId(5)], kind: TypeId(7), x: 100, y: 200 },
                Command::Repair { player: 1, units: vec![], building: EntityId(4) },
                Command::Gather { player: 1, units: vec![EntityId(5)], x: 1, y: 2 },
                Command::Upgrade { player: 1, buildings: vec![EntityId(4), EntityId(6)], upgrade: UpgradeId(1900) },
                Command::CancelUpgrade { player: 1, building: EntityId(4) },
            ],
        );
        r.checkpoints.push((100, 0xdead_beef_0123_4567));
        let bytes = r.encode();
        let back = Replay::decode(&bytes).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.entries.len(), 2, "empty ticks are not stored");
        assert_eq!(back.commands_at(9).len(), 15);
        assert!(back.commands_at(5).is_empty());
    }

    #[test]
    fn rejects_damaged_data() {
        let bytes = Replay::new("m", 0).encode();
        assert!(Replay::decode(&bytes[..bytes.len() - 1]).is_err());
        assert!(Replay::decode(b"NOPE").is_err());
        let mut long = bytes.clone();
        long.push(0);
        assert!(Replay::decode(&long).is_err());
    }
}
