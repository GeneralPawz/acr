//! Runs the original mission and AI scripts (32-bit x86 DLLs) on the ACR simulation.
//!
//! `acr-script` executes the DLLs' machine code and traps their calls into the game API;
//! this crate implements that API ([`api`]) on top of `acr-sim`'s [`World`] and drives the
//! scripts the way `dmcr.exe` does:
//!
//! - **Mission start**: the DLL is loaded and `DllMain` runs (it calls the script's own
//!   `OnInit`; the engine never calls `OnInit` itself, see 0x55afdd). The mission text file
//!   (briefing, pages) is parsed into [`texts::MissionTexts`].
//! - **Ticks**: the simulation step 0x4a61e7 runs `ProcessScenary()` once every 8 ticks (when
//!   `tmtmt & 7 == 7`), followed by the timer bookkeeping and the group processes (0x55a1d2).
//!   [`Scripts::tick`] does the same before every [`World::step`]. Script effects that create
//!   or remove objects change the world at once (the scripts query the result in the same
//!   call); orders become [`Command`]s for that tick's step, so the tick's command stream
//!   holds both the players' and the scripts' orders.
//! - **Events**: pages, messages, victory and defeat are queued as [`MissionEvent`]s for the
//!   game UI ([`Scripts::take_events`]).
//! - **Questions**: the original's `AskQuestion` blocks inside the script until the player
//!   answers. ACR rolls the script call back (scripts, their state and the world are restored
//!   from a snapshot), reports the question and reruns the call once [`Scripts::answer`] has
//!   been given; meanwhile [`Scripts::waiting`] tells the game to stop stepping the world.
//!
//! Scripts are part of the deterministic simulation: the interpreter, the guest heap and the
//! API are deterministic, so every peer (or a replay) that runs the same scripts on the same
//! world gets the same results. [`Scripts`] is `Clone` for snapshots.
//!
//! Why a separate crate: `acr-script` stays a generic Win32/x86 sandbox and `acr-sim` stays
//! free of scripting; the API layer needs both and is the only place that knows the original
//! engine's script-facing structures.

pub mod api;
pub mod events;
pub mod guest;
pub mod state;
pub mod texts;
pub mod vm;
pub mod world_ops;

use acr_formats::M3dMap;
use acr_formats::string_table::Texts;
use acr_sim::{Command, World};

pub use events::MissionEvent;
pub use state::{SCRIPT_PERIOD, ScriptSetup, ScriptState};
use texts::MissionTexts;
use vm::ScriptVm;

use api::{Api, Caller};

/// An AI DLL and the nations it plays. The original loads the DLL once per process
/// (`LoadLibrary` returns the same module for every nation using it), so the nations share
/// one machine and the DLL tells them apart with `GetAINation`.
#[derive(Debug, Clone)]
struct AiScript {
    vm: ScriptVm,
    nations: Vec<u8>,
}

/// State saved before a mission call that may ask a question.
#[derive(Debug, Clone)]
struct Snapshot {
    vm: ScriptVm,
    state: Box<ScriptState>,
    world: Box<World>,
}

/// All scripts of a running game and the engine-side state they share.
#[derive(Debug, Clone)]
pub struct Scripts {
    state: ScriptState,
    mission: Option<ScriptVm>,
    /// The mission imports a question function, so its calls are snapshotted.
    asks: bool,
    ais: Vec<AiScript>,
    events: Vec<MissionEvent>,
    errors: Vec<String>,
    /// Answers given for the pending call.
    answers: Vec<u32>,
    /// A question is open: the tick has to be rerun after the answer.
    waiting: bool,
    /// Entries of `ai_started` already handed out by [`Scripts::ai_requests`].
    ai_requests_seen: usize,
    /// `CenterFound` per nation: the AI city has seen a peasant.
    center_found: [bool; 8],
    /// Log script activity (for `acr mission-run --trace`).
    pub trace: bool,
    trace_lines: Vec<String>,
}

const MISSION_VM: u8 = 0;

impl Scripts {
    /// The script layer of a game on `map`. `texts` is the mission text file (may be empty).
    pub fn new(map: &M3dMap, world: &World, texts: &[u8], setup: ScriptSetup) -> Self {
        let mut state = ScriptState::new(setup, MissionTexts::parse(texts));
        state.bind_map(map, world);
        Self {
            state,
            mission: None,
            asks: false,
            ais: Vec::new(),
            events: Vec::new(),
            errors: Vec::new(),
            answers: Vec::new(),
            waiting: false,
            ai_requests_seen: 0,
            center_found: [false; 8],
            trace: false,
            trace_lines: Vec::new(),
        }
    }

    /// The game's interface texts for `GetTextByID` (`Comment.txt`, `Text\*.txt`, ...).
    pub fn set_strings(&mut self, strings: Texts) {
        self.state.strings = strings;
    }

    pub fn state(&self) -> &ScriptState {
        &self.state
    }

    /// Loads the mission script and runs its `DllMain` (which runs the script's `OnInit`).
    pub fn load_mission(&mut self, world: &mut World, dll: &[u8], path: &str) -> Result<(), String> {
        let mut commands = Vec::new();
        let mut api = Api {
            st: &mut self.state,
            world,
            commands: &mut commands,
            events: &mut self.events,
            caller: Caller::Mission,
            vm: MISSION_VM,
        };
        match ScriptVm::load(dll, path, &mut api) {
            Ok(vm) => {
                self.asks = vm.machine.process.thunks.iter().any(|t| t.name.starts_with("Ask"));
                self.mission = Some(vm);
                self.state.pending_commands.extend(commands);
                Ok(())
            }
            Err(e) => {
                self.fail(e.clone());
                Err(e)
            }
        }
    }

    /// Loads an AI DLL for `nation` (or reuses the machine of an already loaded DLL with the
    /// same path) and runs `InitAI` for it.
    pub fn load_ai(&mut self, world: &mut World, nation: u8, dll: &[u8], path: &str) -> Result<(), String> {
        let mut commands = Vec::new();
        let i = match self.ais.iter().position(|a| a.vm.path.eq_ignore_ascii_case(path)) {
            Some(i) => i,
            None => {
                let mut api = Api {
                    st: &mut self.state,
                    world,
                    commands: &mut commands,
                    events: &mut self.events,
                    caller: Caller::Ai(nation),
                    vm: self.ais.len() as u8 + 1,
                };
                let vm = ScriptVm::load(dll, path, &mut api)?;
                self.ais.push(AiScript { vm, nations: Vec::new() });
                self.ais.len() - 1
            }
        };
        let mut api = Api {
            st: &mut self.state,
            world,
            commands: &mut commands,
            events: &mut self.events,
            caller: Caller::Ai(nation),
            vm: i as u8 + 1,
        };
        let ai = &mut self.ais[i];
        ai.vm.call_export(&mut api, "InitAI")?;
        ai.nations.push(nation);
        self.state.pending_commands.extend(commands);
        Ok(())
    }

    /// AI DLLs the mission asked for (`StartAI`) since the last call: `(nation, game path)`.
    /// The caller reads them and calls [`Scripts::load_ai`].
    pub fn ai_requests(&mut self) -> Vec<(u8, String)> {
        let new = self.state.ai_started[self.ai_requests_seen..].to_vec();
        self.ai_requests_seen = self.state.ai_started.len();
        new
    }

    /// Whether the game has ended (victory or defeat): the original stops the simulation.
    pub fn ended(&self) -> bool {
        self.state.victory || self.state.defeat
    }

    /// A question is open: do not step the world; call [`Scripts::answer`], then
    /// [`Scripts::tick`] again for the same tick.
    pub fn waiting(&self) -> bool {
        self.waiting
    }

    /// Answers the open question (OK = 1, No = 0, or the chosen line).
    pub fn answer(&mut self, answer: u32) {
        if self.waiting {
            self.answers.push(answer);
            self.waiting = false;
        }
    }

    /// Runs the scripts for the tick about to be simulated, *before* [`World::step`];
    /// returns the orders to pass to that step. Returns nothing while a question is open or
    /// after the game has ended.
    pub fn tick(&mut self, world: &mut World) -> Vec<Command> {
        if self.waiting || self.ended() {
            return Vec::new();
        }
        let mut commands = std::mem::take(&mut self.state.pending_commands);
        api::run_production(&mut self.state, world);
        // The step that is about to run has `tmtmt` = tick + 1.
        let tmtmt = world.tick.0 + 1;
        let phase = (tmtmt % SCRIPT_PERIOD) as u8;
        if phase == SCRIPT_PERIOD as u8 - 1 {
            if !self.run_mission(world, &mut commands) {
                return Vec::new();
            }
            self.state.after_script();
            self.run_group_orders(world, &mut commands);
        }
        self.run_ai(world, &mut commands, phase);
        commands
    }

    /// `City::ProcessCreation` of the nation whose phase this is (0x4a69a5..0x4a6afe): nation
    /// k runs on ticks with `tmtmt % 8 == k` (nation 7 never: its phase is the mission's). The
    /// AI starts on the call after the nation first owns a peasant (`CenterFound`), then
    /// `ProcessAI` runs once per call (0x42fb39).
    fn run_ai(&mut self, world: &mut World, commands: &mut Vec<Command>, nation: u8) {
        let Some(i) = self.ais.iter().position(|a| a.nations.contains(&nation)) else { return };
        let k = usize::from(nation);
        if !self.center_found[k] {
            self.center_found[k] =
                world.units.iter().any(|u| u.living() && u.owner == nation && world.catalog.get(u.kind).peasant);
            return;
        }
        let vm_id = i as u8 + 1;
        let ai = &mut self.ais[i];
        if ai.vm.failed.is_some() {
            return;
        }
        // Refresh the objects the DLL holds pointers to, run, copy the AI's writes back.
        let mirrored: Vec<u16> =
            self.state.mirrors.get(&vm_id).map(|s| s.iter().copied().collect()).unwrap_or_default();
        for &idx in &mirrored {
            let _ = guest::write_mirror(
                &mut ai.vm.machine.mem,
                &self.state,
                world,
                acr_sim::world::EntityId(u32::from(idx)),
            );
        }
        let mut api = Api {
            st: &mut self.state,
            world,
            commands,
            events: &mut self.events,
            caller: Caller::Ai(nation),
            vm: vm_id,
        };
        if let Err(e) = ai.vm.tick(&mut api) {
            self.errors.push(e);
        }
        let mirrored: Vec<u16> =
            self.state.mirrors.get(&vm_id).map(|s| s.iter().copied().collect()).unwrap_or_default();
        for idx in mirrored {
            let _ = guest::read_back(&ai.vm.machine.mem, world, idx);
        }
    }

    /// The AI DLL the original would load for a computer nation (`LoadAIDLL` from game
    /// start, 0x53e9d2 / 0x5b4a60): the nation of its first peasant (unit id suffix such as
    /// `(SP)`, matched in `NATIONS.LST`) gives the key `<NATIONID>.0`, whose text in
    /// `TEXT\AI.TXT` is the DLL path (`SPAIN.0 AI\Spain.dll`).
    pub fn ai_dll_for(&self, world: &World, nation: u8, nations_lst: &[u8]) -> Option<String> {
        let peasant =
            world.units.iter().find(|u| u.living() && u.owner == nation && world.catalog.get(u.kind).peasant)?;
        let id = &world.catalog.get(peasant.kind).id;
        let suffix = &id[id.rfind('(')?..];
        let text = String::from_utf8_lossy(nations_lst);
        let nation_id = text.lines().map(|l| l.split_whitespace().collect::<Vec<_>>()).find_map(|w| {
            (w.len() >= 7 && w[5..7].iter().any(|s| s.eq_ignore_ascii_case(suffix))).then(|| w[0].to_owned())
        })?;
        let key = format!("{nation_id}.0");
        let path = String::from_utf8_lossy(self.state.strings.get(&key)).into_owned();
        (path != key).then_some(path)
    }

    /// Runs `ProcessScenary`; false if the call was rolled back for a question.
    fn run_mission(&mut self, world: &mut World, commands: &mut Vec<Command>) -> bool {
        let Some(vm) = &mut self.mission else { return true };
        if vm.failed.is_some() {
            return true;
        }
        let snapshot = self.asks.then(|| Snapshot {
            vm: vm.clone(),
            state: Box::new(self.state.clone()),
            world: Box::new(world.clone()),
        });
        let events_before = self.events.len();
        let commands_before = commands.len();
        self.state.answers = self.answers.iter().copied().collect();
        self.state.answers_used = 0;
        let calls_before = self.state.calls.values().sum::<u64>();
        let mut api = Api {
            st: &mut self.state,
            world,
            commands,
            events: &mut self.events,
            caller: Caller::Mission,
            vm: MISSION_VM,
        };
        let result = vm.tick(&mut api);
        if self.trace {
            let after = self.state.calls.values().sum::<u64>();
            self.trace_lines.push(format!(
                "ProcessScenary: {} API calls, {} instructions",
                after - calls_before,
                vm.last_steps
            ));
        }
        match result {
            Ok(()) => {
                self.answers.clear();
                true
            }
            Err(e) if e.contains(api::game::QUESTION_PENDING) => {
                let Some(snap) = snapshot else { return true };
                // The question stays reported; everything else of the call is undone.
                let question = self.events.pop();
                self.events.truncate(events_before);
                self.events.extend(question);
                commands.truncate(commands_before);
                *vm = snap.vm;
                let calls = std::mem::take(&mut self.state.calls);
                self.state = *snap.state;
                self.state.calls = calls;
                *world = *snap.world;
                self.waiting = true;
                false
            }
            Err(e) => {
                self.errors.push(e.clone());
                self.events.push(MissionEvent::Error { title: "Mission script error".into(), text: e });
                true
            }
        }
    }

    /// Runs the head process of every group that has one (`SGP_*`, after the script).
    fn run_group_orders(&mut self, world: &mut World, commands: &mut Vec<Command>) {
        let mut api = Api {
            st: &mut self.state,
            world,
            commands,
            events: &mut self.events,
            caller: Caller::Mission,
            vm: MISSION_VM,
        };
        api::groups::run_group_orders(&mut api);
    }

    /// The objectives text: `#BRIEFING` without the `{X}` markers and without the sections
    /// switched off by `DisableMission` (as `CreateMissText`, 0x55ad9b, writes `Miss.txt`).
    pub fn briefing(&self) -> Vec<u8> {
        let Some(b) = self.state.texts.get("BRIEFING") else { return Vec::new() };
        let mut out = Vec::new();
        let mut hidden = false;
        let body = &b.body;
        let mut i = 0;
        while i < body.len() {
            if body[i] == b'{' && i + 2 < body.len() && body[i + 2] == b'}' {
                hidden = self.state.text_disable[api::game::section(body[i + 1])];
                i += 3;
                continue;
            }
            if !hidden {
                out.push(body[i]);
            }
            i += 1;
        }
        out
    }

    /// Events raised since the last call, in order.
    pub fn take_events(&mut self) -> Vec<MissionEvent> {
        std::mem::take(&mut self.events)
    }

    /// Trace lines since the last call (`trace` must be on): every API call with its
    /// arguments and result, and a summary per `ProcessScenary`.
    pub fn take_trace(&mut self) -> Vec<String> {
        self.state.trace = self.trace;
        let mut out = std::mem::take(&mut self.state.trace_log);
        out.append(&mut self.trace_lines);
        out
    }

    /// Script failures (a failed script is not called again).
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Instructions executed by all scripts so far.
    pub fn instructions(&self) -> u64 {
        self.mission.iter().map(|m| m.machine.steps).sum::<u64>()
            + self.ais.iter().map(|a| a.vm.machine.steps).sum::<u64>()
    }

    fn fail(&mut self, e: String) {
        self.errors.push(e.clone());
        self.events.push(MissionEvent::Error { title: "Mission script error".into(), text: e });
    }
}

/// A one-line description of an event for logs.
pub fn describe(e: &MissionEvent) -> String {
    let text = |t: &[u8]| {
        let s = texts::cp1252(t);
        let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if s.chars().count() > 100 { format!("{}…", s.chars().take(100).collect::<String>()) } else { s }
    };
    match e {
        MissionEvent::Page { id, text: t, .. } => format!("page {id}: {}", text(t)),
        MissionEvent::Question { id, text: t, .. } => format!("question {id}: {}", text(t)),
        MissionEvent::Choice { id, choices, .. } => format!("choice {id}: {} options", choices.len()),
        MissionEvent::CentralText { text: t, ticks } => format!("central text ({ticks} ticks): {}", text(t)),
        MissionEvent::Hint { text: t, ticks } => format!("hint ({ticks} frames): {}", text(t)),
        MissionEvent::Victory => "VICTORY".into(),
        MissionEvent::Defeat => "DEFEAT".into(),
        MissionEvent::Briefing => "objectives changed".into(),
        MissionEvent::Camera { x, y } => format!("camera to ({x}, {y})"),
        MissionEvent::Error { title, text } => format!("error: {title}: {text}"),
    }
}
