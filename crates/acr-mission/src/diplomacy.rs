//! The player's side of `DipServer.dll`: clicks on tribe buildings and the requests of its
//! dialog pages.
//!
//! The original shows the tribe dialogs with `intExplorer.dll`, a small browser for CML pages.
//! DipServer writes a page to `Dip\tmp.cml` (through `RRewrite`/`RBlockWrite`, so here into
//! [`crate::ScriptState::files`]) and opens it with `ActivateDipDialog("LW_new&|LW_file&Dip/tmp.cml")`
//! ([`crate::MissionEvent::Diplomacy`]). A link of the page (`{DS|perform&1&0&2}`) is a request
//! to the server DLL named by the prefix: intExplorer calls DipServer's
//! `DS_SendRequest(data, size)` with the commands as a binary command set, polls
//! `DS_Process()` / `DS_GetRequestResult()` and executes the commands of the answer (usually
//! `LW_file&Dip/tmp.cml`: show the rewritten page). What changes the game (paying, units changing
//! sides) goes through `SendDipCommand` and reaches every peer as a network command.
//!
//! These calls are interface actions of the local player, not part of a simulation tick.

use acr_sim::World;

use crate::api::{Api, Caller};
use crate::{DIP_VM, Scripts};

const CHECK_BUILDING: &str = "?CheckDipBuilding@@YAXEH@Z";
const SEND_REQUEST: &str = "?DS_SendRequest@@YAKPADH@Z";
const PROCESS: &str = "?DS_Process@@YAXXZ";
const GET_RESULT: &str = "?DS_GetRequestResult@@YAKKPAPADPAH@Z";
const CLOSE_REQUEST: &str = "?DS_CloseRequest@@YAXK@Z";
/// `DS_GetRequestResult`: the answer is complete.
const RESULT_READY: u32 = 0x80;
/// `DS_Process` handles one command of a request per call.
const MAX_PROCESS: usize = 64;

/// One command of a command set: name and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DipCommand {
    pub name: Vec<u8>,
    pub args: Vec<Vec<u8>>,
}

/// Parses the text form `cmd&arg&arg|cmd2&...` (DipServer 0x1000f290): names are cut to 15
/// bytes, `\XX` in arguments is a hexadecimal byte.
pub fn parse_commands(text: &[u8]) -> Vec<DipCommand> {
    let mut out = Vec::new();
    for part in text.split(|&b| b == b'|').filter(|p| !p.is_empty()) {
        let mut fields = part.split(|&b| b == b'&');
        let name = fields.next().unwrap_or_default();
        let args = fields.map(unescape).collect();
        out.push(DipCommand { name: name[..name.len().min(15)].to_vec(), args });
    }
    out
}

fn unescape(arg: &[u8]) -> Vec<u8> {
    let hex = |b: u8| match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    };
    let mut out = Vec::with_capacity(arg.len());
    let mut i = 0;
    while i < arg.len() {
        if arg[i] == b'\\' && i + 2 < arg.len() {
            out.push(hex(arg[i + 1]) << 4 | hex(arg[i + 2]));
            i += 3;
        } else {
            out.push(arg[i]);
            i += 1;
        }
    }
    out
}

/// The binary command set (0x1000f550): u16 count; per command u8 name length, name, u16
/// argument count, per argument u32 length and bytes.
pub fn encode_commands(commands: &[DipCommand]) -> Vec<u8> {
    let mut out = (commands.len() as u16).to_le_bytes().to_vec();
    for c in commands {
        out.push(c.name.len() as u8);
        out.extend_from_slice(&c.name);
        out.extend_from_slice(&(c.args.len() as u16).to_le_bytes());
        for a in &c.args {
            out.extend_from_slice(&(a.len() as u32).to_le_bytes());
            out.extend_from_slice(a);
        }
    }
    out
}

/// Reads a binary command set (0x1000f780); stops at the first truncated entry.
pub fn decode_commands(data: &[u8]) -> Vec<DipCommand> {
    let mut out = Vec::new();
    let mut p = 0usize;
    let take = |p: &mut usize, n: usize| -> Option<&[u8]> {
        let s = data.get(*p..*p + n)?;
        *p += n;
        Some(s)
    };
    let Some(n) = take(&mut p, 2).map(|b| u16::from_le_bytes([b[0], b[1]])) else { return out };
    for _ in 0..n {
        let Some(len) = take(&mut p, 1).map(|b| usize::from(b[0])) else { break };
        let Some(name) = take(&mut p, len).map(<[u8]>::to_vec) else { break };
        let Some(nargs) = take(&mut p, 2).map(|b| u16::from_le_bytes([b[0], b[1]])) else { break };
        let mut args = Vec::new();
        for _ in 0..nargs {
            let Some(len) = take(&mut p, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else {
                break;
            };
            let Some(a) = take(&mut p, len) else { break };
            args.push(a.to_vec());
        }
        out.push(DipCommand { name, args });
    }
    out
}

impl Scripts {
    /// The player pressed the left button on object `index` (`CheckDipBuilding(MyNation,
    /// index)`, called by the engine for every press on the map, 0x4c446c): DipServer opens the
    /// dialog of a tribe whose building it is ([`crate::MissionEvent::Diplomacy`]).
    pub fn dip_check_building(&mut self, world: &mut World, nation: u8, index: u16) {
        self.call_dip(world, CHECK_BUILDING, &[u32::from(nation), u32::from(index)]);
    }

    /// Sends a request of a dialog page (the part after `DS|`, e.g. `perform&1&0&2`) and returns
    /// the commands of the answer (`LW_file&Dip/tmp.cml`, ...).
    pub fn dip_request(&mut self, world: &mut World, request: &[u8]) -> Vec<DipCommand> {
        let mut commands = parse_commands(request);
        // intExplorer adds two arguments of its own to every command; the server echoes the
        // first of them back.
        for c in &mut commands {
            c.args.push(Vec::new());
            c.args.push(Vec::new());
        }
        let data = encode_commands(&commands);
        let Some(dip) = self.dip.as_mut().filter(|d| d.failed.is_none()) else { return Vec::new() };
        let Some(buf) = dip.machine.process.heap.alloc(&mut dip.machine.mem, data.len() as u32 + 8, true) else {
            return Vec::new();
        };
        let mut answer = Vec::new();
        if dip.machine.mem.write_bytes(buf, &data).is_ok() {
            let handle = self.call_dip(world, SEND_REQUEST, &[buf, data.len() as u32]);
            if let Some(handle) = handle {
                answer = self.dip_result(world, handle);
                self.call_dip(world, CLOSE_REQUEST, &[handle]);
            }
        }
        if let Some(dip) = self.dip.as_mut() {
            dip.machine.process.heap.free(buf);
        }
        decode_commands(&answer)
    }

    fn dip_result(&mut self, world: &mut World, handle: u32) -> Vec<u8> {
        let Some(out) = self.dip.as_mut().and_then(|d| crate::vm::scratch(d).ok()) else { return Vec::new() };
        for _ in 0..MAX_PROCESS {
            self.call_dip(world, PROCESS, &[]);
            let r = self.call_dip(world, GET_RESULT, &[handle, out, out + 4]);
            if r == Some(RESULT_READY) {
                let Some(dip) = self.dip.as_ref() else { break };
                let mem = &dip.machine.mem;
                let (Ok(p), Ok(n)) = (mem.read_u32(out), mem.read_u32(out + 4)) else { break };
                let mut data = vec![0; n.min(0x10000) as usize];
                return if mem.read_bytes(p, &mut data).is_ok() { data } else { Vec::new() };
            }
        }
        Vec::new()
    }

    /// Calls a DipServer export outside a tick; its orders run with the next tick.
    fn call_dip(&mut self, world: &mut World, name: &str, args: &[u32]) -> Option<u32> {
        let dip = self.dip.as_mut().filter(|d| d.failed.is_none())?;
        let mut commands = Vec::new();
        let mut api = Api {
            st: &mut self.state,
            world,
            commands: &mut commands,
            events: &mut self.events,
            caller: Caller::Diplomacy,
            vm: DIP_VM,
            dip: None,
        };
        let r = dip.call_export_with(&mut api, name, args);
        self.state.pending_commands.extend(commands);
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.errors.push(e);
                None
            }
        }
    }

    /// A file the scripts can read or have written (`Dip\tmp.cml`), by game path.
    pub fn script_file(&self, path: &str) -> Option<&[u8]> {
        self.state.files.get(&acr_formats::normalize_path(path)).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_sets() {
        let c = parse_commands(b"LW_new&|LW_file&Dip/tmp.cml");
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, b"LW_new");
        assert_eq!(c[1].args, vec![b"Dip/tmp.cml".to_vec()]);
        let c = parse_commands(b"perform&1&0&2");
        assert_eq!(c[0].args.len(), 3);
        assert_eq!(decode_commands(&encode_commands(&c)), c);
        assert_eq!(parse_commands(b"x&a\\41b")[0].args[0], b"aAb");
    }
}
