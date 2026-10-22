//! One loaded script DLL: the x86 machine plus what the runtime knows about it.

use acr_script::runtime::{Host, Machine};

/// What kind of script a DLL is, from its exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    /// `ProcessScenary()` once per tick (missions, history battles, user missions).
    Mission,
    /// `InitAI()` once per computer nation, `ProcessAI()` per tick (`AI\*.dll`).
    Ai,
    /// `DipServer.dll`: `InitDipForThisMap()` at the start, `ProcessDipRelations()` with every
    /// script call.
    Diplomacy,
}

/// `ProcessDipRelations` export of `DipServer.dll`.
pub const PROCESS_DIP: &str = "?ProcessDipRelations@@YAXXZ";

/// An 8-byte scratch block in a machine's heap, allocated once (for handles passed in).
pub fn scratch(vm: &mut ScriptVm) -> acr_script::runtime::Result<u32> {
    if vm.scratch == 0 {
        let m = &mut vm.machine;
        vm.scratch = m
            .process
            .heap
            .alloc(&mut m.mem, 64, true)
            .ok_or_else(|| acr_script::runtime::Error::Host("guest heap exhausted".into()))?;
    }
    Ok(vm.scratch)
}

/// A script DLL running in the interpreter.
#[derive(Debug, Clone)]
pub struct ScriptVm {
    pub machine: Machine,
    pub kind: ScriptKind,
    /// Game path of the DLL, as `GetModuleFileNameA` reports it.
    pub path: String,
    /// Address of the per-tick export.
    tick_fn: u32,
    /// Set when the script failed; it is not called again (the original would have crashed).
    pub failed: Option<String>,
    /// Instructions executed by the last call.
    pub last_steps: u64,
    /// Scratch block for arguments passed in from other machines (see [`scratch`]).
    pub scratch: u32,
}

/// Instruction budget of one script call. The heaviest original `ProcessScenary` needs about
/// 30k instructions, an AI `ProcessAI` a few hundred thousand; a call that runs much longer
/// is stuck.
pub const STEP_LIMIT: u64 = 50_000_000;

impl ScriptVm {
    /// Loads a DLL and runs `DllMain(DLL_PROCESS_ATTACH)`, which also runs the C++ static
    /// constructors and, in missions, `OnInit` (the engine itself never calls `OnInit`; the
    /// DLLs call it from their `DllMain`).
    pub fn load(data: &[u8], path: &str, host: &mut dyn Host) -> Result<Self, String> {
        let mut machine = Machine::new(data, path).map_err(|e| format!("{path}: {e}"))?;
        machine.step_limit = STEP_LIMIT;
        let (kind, tick) = if let Some(a) = machine.image.export("ProcessScenary") {
            (ScriptKind::Mission, a)
        } else if let Some(a) = machine.image.export("ProcessAI") {
            (ScriptKind::Ai, a)
        } else if let Some(a) = machine.image.export(PROCESS_DIP) {
            (ScriptKind::Diplomacy, a)
        } else {
            return Err(format!("{path}: neither ProcessScenary nor ProcessAI is exported"));
        };
        match machine.attach(host) {
            Ok(true) => {}
            Ok(false) => return Err(format!("{path}: DllMain returned FALSE")),
            Err(e) => return Err(format!("{path}: DllMain: {e}")),
        }
        Ok(Self { machine, kind, path: path.to_owned(), tick_fn: tick, failed: None, last_steps: 0, scratch: 0 })
    }

    /// Calls an export by name; a failure marks the script as failed.
    pub fn call_export(&mut self, host: &mut dyn Host, name: &str) -> Result<u32, String> {
        self.call_export_with(host, name, &[])
    }

    /// Calls an export with arguments.
    pub fn call_export_with(&mut self, host: &mut dyn Host, name: &str, args: &[u32]) -> Result<u32, String> {
        let Some(addr) = self.machine.image.export(name) else {
            return Err(format!("{}: no export {name}", self.path));
        };
        self.call_args(host, addr, name, args)
    }

    /// Runs the per-tick export (`ProcessScenary` or `ProcessAI`) unless the script failed.
    pub fn tick(&mut self, host: &mut dyn Host) -> Result<(), String> {
        if let Some(e) = &self.failed {
            return Err(e.clone());
        }
        let name = match self.kind {
            ScriptKind::Mission => "ProcessScenary",
            ScriptKind::Ai => "ProcessAI",
            ScriptKind::Diplomacy => PROCESS_DIP,
        };
        self.call(host, self.tick_fn, name).map(|_| ())
    }

    fn call(&mut self, host: &mut dyn Host, addr: u32, name: &str) -> Result<u32, String> {
        self.call_args(host, addr, name, &[])
    }

    fn call_args(&mut self, host: &mut dyn Host, addr: u32, name: &str, args: &[u32]) -> Result<u32, String> {
        let before = self.machine.steps;
        let r = self.machine.call(host, addr, args);
        self.last_steps = self.machine.steps - before;
        r.map_err(|e| {
            let at = self.machine.last_error_eip.map(|a| format!(" (eip {a:#010x})")).unwrap_or_default();
            let msg = format!("{}: {name}: {e}{at}", self.path);
            self.failed = Some(msg.clone());
            msg
        })
    }
}
