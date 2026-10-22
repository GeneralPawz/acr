//! One loaded script DLL: the x86 machine plus what the runtime knows about it.

use acr_script::runtime::{Host, Machine};

/// What kind of script a DLL is, from its exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    /// `ProcessScenary()` once per tick (missions, history battles, user missions).
    Mission,
    /// `InitAI()` once per computer nation, `ProcessAI()` per tick (`AI\*.dll`).
    Ai,
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
        } else {
            return Err(format!("{path}: neither ProcessScenary nor ProcessAI is exported"));
        };
        match machine.attach(host) {
            Ok(true) => {}
            Ok(false) => return Err(format!("{path}: DllMain returned FALSE")),
            Err(e) => return Err(format!("{path}: DllMain: {e}")),
        }
        Ok(Self { machine, kind, path: path.to_owned(), tick_fn: tick, failed: None, last_steps: 0 })
    }

    /// Calls an export by name; a failure marks the script as failed.
    pub fn call_export(&mut self, host: &mut dyn Host, name: &str) -> Result<u32, String> {
        let Some(addr) = self.machine.image.export(name) else {
            return Err(format!("{}: no export {name}", self.path));
        };
        self.call(host, addr, name)
    }

    /// Runs the per-tick export (`ProcessScenary` or `ProcessAI`) unless the script failed.
    pub fn tick(&mut self, host: &mut dyn Host) -> Result<(), String> {
        if let Some(e) = &self.failed {
            return Err(e.clone());
        }
        let name = if self.kind == ScriptKind::Mission { "ProcessScenary" } else { "ProcessAI" };
        self.call(host, self.tick_fn, name).map(|_| ())
    }

    fn call(&mut self, host: &mut dyn Host, addr: u32, name: &str) -> Result<u32, String> {
        let before = self.machine.steps;
        let r = self.machine.call(host, addr, &[]);
        self.last_steps = self.machine.steps - before;
        r.map_err(|e| {
            let at = self.machine.last_error_eip.map(|a| format!(" (eip {a:#010x})")).unwrap_or_default();
            let msg = format!("{}: {name}: {e}{at}", self.path);
            self.failed = Some(msg.clone());
            msg
        })
    }
}
