//! The game API the scripts import from `dmcr.exe` (and `DipServer.dll`), implemented on the
//! ACR simulation.
//!
//! Every function is an [`ApiFn`]: its exported name, how complete the implementation is
//! ([`Status`]) and a handler. Handlers read their arguments through [`Call`], query and
//! change the [`World`] and the engine-side script state ([`ScriptState`]) and return `eax`.
//! Orders to units become [`acr_sim::Command`]s for the current tick; things the player
//! should see become [`MissionEvent`]s.
//!
//! The semantics come from the original functions in `dmcr.exe` 1.46; each handler names the
//! address it reproduces. `docs/script-runtime.md` has the coverage table.

mod aiobj;
pub mod call;
mod core;
mod economy;
pub mod game;
pub mod groups;
mod stubs;
mod tactics;
mod units;
mod util;

pub use economy::run_production;

pub use util::{Circle, norma};

use std::collections::BTreeMap;
use std::sync::OnceLock;

use acr_script::runtime::{Host, HostCall, Result};
use acr_sim::{Command, World};

pub use call::{Call, Handle};

use crate::events::MissionEvent;
use crate::state::ScriptState;

/// How complete a function's implementation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Behaves like the original as far as the simulation models the game.
    Done,
    /// Implemented, with the named gap (usually a simulation feature that does not exist yet).
    Partial(&'static str),
    /// Returns a neutral value (documented per function) and changes nothing.
    Stub(&'static str),
}

pub type Handler = fn(&mut Api<'_>, &mut Call<'_, '_>) -> Result<u32>;

/// One API function.
pub struct ApiFn {
    /// Imported name (C++ names mangled, as in the import table).
    pub name: &'static str,
    /// Address of the original in `dmcr.exe` 1.46 (0 for `DipServer.dll` functions).
    pub original: u32,
    pub status: Status,
    pub handler: Handler,
}

/// All implemented functions, by name.
fn table() -> &'static BTreeMap<&'static str, &'static ApiFn> {
    static TABLE: OnceLock<BTreeMap<&'static str, &'static ApiFn>> = OnceLock::new();
    TABLE.get_or_init(|| modules().flat_map(|m| m.iter()).map(|f| (f.name, f)).collect())
}

fn modules() -> impl Iterator<Item = &'static [ApiFn]> {
    // Later tables override earlier ones: real implementations replace the stubs.
    [
        stubs::FUNCTIONS,
        core::FUNCTIONS,
        game::FUNCTIONS,
        groups::FUNCTIONS,
        units::FUNCTIONS,
        economy::FUNCTIONS,
        tactics::FUNCTIONS,
        aiobj::FUNCTIONS,
    ]
    .into_iter()
}

/// Looks a function up by its imported name.
pub fn function(name: &str) -> Option<&'static ApiFn> {
    table().get(name).copied()
}

/// All functions in name order.
pub fn functions() -> impl Iterator<Item = &'static ApiFn> {
    table().values().copied()
}

/// Who is calling: the mission script or the AI of a nation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    Mission,
    /// An AI DLL working for this nation (`GetAINation`).
    Ai(u8),
    /// `DipServer.dll`.
    Diplomacy,
}

/// Everything a handler may touch during one script call.
pub struct Api<'a> {
    pub st: &'a mut ScriptState,
    pub world: &'a mut World,
    /// Orders for the current tick.
    pub commands: &'a mut Vec<Command>,
    pub events: &'a mut Vec<MissionEvent>,
    pub caller: Caller,
    /// Which machine is calling (0 = mission, 1.. = AI DLLs), for guest-memory caches.
    pub vm: u8,
}

impl Host for Api<'_> {
    fn call(&mut self, raw: &mut HostCall<'_>) -> Result<u32> {
        let mut call = Call { raw };
        let Some(f) = function(call.name()) else {
            *self.st.unknown_calls.entry(call.name().to_owned()).or_default() += 1;
            return Ok(0);
        };
        *self.st.calls.entry(f.name).or_default() += 1;
        (f.handler)(self, &mut call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every function the original DLLs import has an entry.
    #[test]
    fn covers_every_import() {
        let missing: Vec<&str> =
            acr_script::dmcr::ARG_COUNTS.iter().map(|(n, _)| *n).filter(|n| function(n).is_none()).collect();
        assert!(missing.is_empty(), "{} functions missing: {missing:?}", missing.len());
        assert_eq!(functions().count(), 216, "210 dmcr.exe functions and 6 DipServer.dll functions");
    }
}
