//! The mission script of the running game (`acr-mission`): loads the DLL, the mission texts
//! and the AI DLLs, runs them before every simulation tick and shows what they report.
//!
//! The original shows `ShowPage` pages and questions as modal windows inside the script
//! call, so the game stands still until the player closes them; ACR hands them to the
//! screen stack ([`MissionLink::window`]) and runs no script and no simulation step until
//! they are answered. Hints, the central text and the end of the game are drawn over the
//! map ([`crate::mission_ui`]).

use std::cell::Cell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use acr_formats::M3dMap;
use acr_formats::string_table::Texts;
use acr_mission::{MissionEvent, ScriptSetup, Scripts};
use acr_sim::{Command, World};

use crate::mission_ui::Messages;
use crate::ui::draw::Draw;
use crate::ui::screens::mission::{Answer, ChoiceWindow, MessageWindow, Window};
use crate::ui::setup::Control;
use crate::ui::{Ctx, GameKind, GameSetup, Screen};

const MISSIONS: &str = "Missions\\Missions.txt";
const NATIONS: &str = "NATIONS.LST";
/// The diplomacy system for neutral tribes, loaded in every game like the original.
const DIPSERVER: &str = "DipServer.dll";
/// The victory or defeat text stays this long before the game ends (0x4bc7b0: 7000 ms).
const END_SECONDS: f64 = 7.0;

/// A window the player has to close before the game goes on.
struct Pending {
    /// Taken when the window is opened.
    screen: Option<Box<dyn Screen>>,
    answer: Answer,
    /// The script waits for the answer (questions, not pages).
    question: bool,
}

pub struct MissionLink {
    script: Option<String>,
    texts: Option<String>,
    /// Look the script/texts up in `Missions.txt` by map path (games started from a map).
    lookup_map: Option<String>,
    /// Requested files and their contents once loaded (`None` = failed).
    files: BTreeMap<String, Option<Vec<u8>>>,
    text_sources: Vec<String>,
    dip_files: Vec<String>,
    /// Computer players of a random map (nations).
    computers: Vec<u8>,
    local_player: u8,
    difficulty: u8,
    scripts: Option<Scripts>,
    /// AI DLLs to load once their files are there.
    pending_ais: Vec<(u8, String)>,
    new_requests: Vec<String>,
    /// Tried and failed (no script, or it did not load).
    off: bool,
    messages: Messages,
    pending: VecDeque<Pending>,
    clock: f64,
    /// `Some(true)` after victory, `Some(false)` after defeat.
    pub ended: Option<bool>,
    /// When the game ended (seconds of `clock`).
    ended_at: f64,
    /// Escape after the end: leave at once.
    leave: bool,
    /// `SetStartPoint`: centre the view here (world pixels).
    camera: Option<(i32, i32)>,
    /// Answers of the windows closed since [`MissionLink::take_answers`] (savegames).
    answered: Vec<u32>,
    /// `ActivateDipDialog` commands for the diplomacy panel.
    diplomacy: Vec<Vec<u8>>,
}

impl MissionLink {
    pub fn new(setup: &GameSetup) -> Self {
        let (script, texts, lookup_map, difficulty) = match &setup.kind {
            GameKind::Mission(m) if !m.script.is_empty() => {
                (Some(m.script.clone()), Some(m.texts.clone()).filter(|t| !t.is_empty()), None, m.difficulty.index())
            }
            GameKind::Mission(m) => (None, None, Some(m.map.clone()), m.difficulty.index()),
            GameKind::RandomMap(_) => (None, None, None, 1),
        };
        // Random maps put player k's units in nation `color` (random_map.rs).
        let random = matches!(setup.kind, GameKind::RandomMap(_));
        let nation_of = |i: usize| if random { setup.players.get(i).map_or(i as u8, |p| p.color) } else { i as u8 };
        let computers = setup
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p.control, Control::Computer(_)))
            .map(|(i, _)| nation_of(i))
            .collect();
        let local_player = nation_of(setup.local_player);
        Self {
            script,
            texts,
            lookup_map,
            files: BTreeMap::new(),
            text_sources: Vec::new(),
            dip_files: Vec::new(),
            computers,
            local_player,
            difficulty: difficulty.min(3) as u8,
            scripts: None,
            pending_ais: Vec::new(),
            new_requests: Vec::new(),
            off: false,
            messages: Messages::default(),
            pending: VecDeque::new(),
            clock: 0.0,
            ended: None,
            ended_at: 0.0,
            leave: false,
            camera: None,
            answered: Vec::new(),
            diplomacy: Vec::new(),
        }
    }

    /// Files to load first; `paths` are all game paths.
    pub fn start(&mut self, paths: &[String]) -> Vec<String> {
        let normalized: Vec<String> = paths.iter().map(|p| acr_formats::normalize_path(p)).collect();
        self.text_sources = Texts::sources(&normalized);
        let mut want: Vec<String> = self.text_sources.clone();
        want.push(NATIONS.to_owned());
        // DipServer and the files it reads.
        self.dip_files = normalized
            .iter()
            .filter(|p| p.starts_with("DIP/") || p.starts_with("MISSIONS/CASH/") || p.starts_with("MISSIONS/RES/"))
            .cloned()
            .collect();
        want.extend(self.dip_files.iter().cloned());
        want.push(DIPSERVER.to_owned());
        // Every AI DLL (`TEXT\AI.TXT` names twelve), so `StartAI` never waits for a file.
        want.extend(normalized.iter().filter(|p| p.starts_with("AI/") && p.ends_with(".DLL")).cloned());
        want.extend(self.script.clone());
        want.extend(self.texts.clone());
        if self.lookup_map.is_some() {
            want.push(MISSIONS.to_owned());
        }
        self.request(want)
    }

    fn request(&mut self, paths: Vec<String>) -> Vec<String> {
        let mut out = Vec::new();
        for p in paths {
            let key = acr_formats::normalize_path(&p);
            if let std::collections::btree_map::Entry::Vacant(e) = self.files.entry(key) {
                e.insert(None);
                out.push(p);
            }
        }
        out
    }

    fn file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(&acr_formats::normalize_path(path)).and_then(|f| f.as_deref())
    }

    /// Takes a loaded file if it is one of ours; returns further files to load.
    pub fn on_loaded(&mut self, path: &str, result: &Result<Vec<u8>, String>) -> (bool, Vec<String>) {
        let key = acr_formats::normalize_path(path);
        let Some(slot) = self.files.get_mut(&key) else { return (false, Vec::new()) };
        *slot = Some(result.clone().unwrap_or_default());
        if key == acr_formats::normalize_path(MISSIONS)
            && let Some(map) = self.lookup_map.take()
        {
            let missions = acr_formats::missions::parse_missions(self.file(MISSIONS).unwrap_or_default());
            let norm = acr_formats::normalize_path(&map);
            if let Some(m) = missions.iter().find(|m| acr_formats::normalize_path(&m.map) == norm) {
                self.script = Some(m.script.clone());
                self.texts = Some(m.description.clone());
                let more = vec![m.script.clone(), m.description.clone()];
                return (true, self.request(more));
            }
        }
        (true, Vec::new())
    }

    fn all_loaded(&self) -> bool {
        self.lookup_map.is_none() && self.files.values().all(Option::is_some)
    }

    /// Starts the scripts once the world and all files are there. Uses the map's `.DLL` chunk
    /// when no mission names a script.
    pub fn update(&mut self, map: &M3dMap, world: &mut World, seconds: f64) -> Vec<String> {
        self.clock += seconds;
        // Answers of closed windows, in the order the script asked.
        while let Some(front) = self.pending.front()
            && front.screen.is_none()
            && let Some(answer) = front.answer.get()
        {
            if front.question
                && let Some(s) = &mut self.scripts
            {
                s.answer(answer);
            }
            self.pending.pop_front();
            self.answered.push(answer);
        }
        if self.scripts.is_some() || self.off {
            return Vec::new();
        }
        if self.script.is_none()
            && let Some(dll) = map.mission_dll.clone().filter(|d| !d.is_empty())
        {
            // History battles: pages in `<dll>.txt`, strings in `<dll>.str`.
            let (texts, strings) = acr_mission::map_script_texts(&dll);
            self.script = Some(dll.clone());
            self.texts = Some(texts.clone());
            self.text_sources.push(acr_formats::normalize_path(&strings));
            return self.request(vec![dll, texts, strings]);
        }
        if !self.all_loaded() {
            return Vec::new();
        }
        let texts = self.texts.as_deref().and_then(|t| self.file(t)).unwrap_or_default().to_vec();
        let setup = ScriptSetup { difficulty: self.difficulty, local_player: self.local_player };
        let mut scripts = Scripts::new(map, world, &texts, setup);
        let mut strings = Texts::default();
        for p in &self.text_sources {
            if let Some(b) = self.file(p) {
                strings.add(b);
            }
        }
        scripts.set_strings(strings);
        for p in &self.dip_files {
            if let Some(b) = self.file(p) {
                scripts.add_file(p, b.to_vec());
            }
        }
        if let Some(dll) = self.file(DIPSERVER).filter(|b| !b.is_empty())
            && let Err(e) = scripts.load_diplomacy(world, dll, DIPSERVER)
        {
            log::error!("{e}");
        }
        if let Some(path) = self.script.clone() {
            match self.file(&path).filter(|b| !b.is_empty()) {
                Some(dll) => {
                    if let Err(e) = scripts.load_mission(world, dll, &path) {
                        log::error!("mission script: {e}");
                    }
                }
                None => log::error!("mission script {path} not found"),
            }
        }
        let nations = self.file(NATIONS).unwrap_or_default().to_vec();
        let mut ais = Vec::new();
        for &n in &self.computers {
            if let Some(p) = scripts.ai_dll_for(world, n, &nations) {
                ais.push((n, p));
            }
        }
        if self.script.is_none() && ais.is_empty() && !scripts.has_diplomacy() {
            self.off = true;
            return Vec::new();
        }
        self.scripts = Some(scripts);
        self.pending_ais = ais;
        self.request(self.pending_ais.iter().map(|(_, p)| p.clone()).collect())
    }

    /// Runs the scripts for the next tick; `None` while the game must stand still (a page or
    /// question is open). After the end of the game the world goes on without scripts.
    pub fn before_step(&mut self, world: &mut World) -> Option<Vec<Command>> {
        if !self.pending.is_empty() {
            return None;
        }
        self.messages.tick();
        // Deterministic start: the world does not move before the scripts run (they would
        // otherwise start at a tick that depends on file loading).
        let Some(scripts) = &mut self.scripts else { return if self.off { Some(Vec::new()) } else { None } };
        // AI DLLs requested at start or by StartAI. All AI DLLs are preloaded, so a StartAI
        // takes effect in the same tick as in the original; should one still be loading, the
        // game waits for it instead of starting the AI at a timing-dependent tick.
        let mut wanted = std::mem::take(&mut self.pending_ais);
        wanted.extend(scripts.ai_requests());
        for (n, path) in wanted {
            match self.files.get(&acr_formats::normalize_path(&path)) {
                Some(Some(b)) if !b.is_empty() => {
                    if let Err(e) = scripts.load_ai(world, n, &b.clone(), &path) {
                        log::error!("AI {n}: {e}");
                    }
                }
                Some(Some(_)) => log::error!("AI DLL {path} not found"),
                Some(None) => self.pending_ais.push((n, path)),
                None => {
                    self.files.insert(acr_formats::normalize_path(&path), None);
                    self.new_requests.push(path.clone());
                    self.pending_ais.push((n, path));
                }
            }
        }
        if !self.pending_ais.is_empty() {
            return None;
        }
        let commands = scripts.tick(world);
        let waiting = scripts.waiting();
        self.handle_events();
        if waiting {
            return None;
        }
        Some(commands)
    }

    /// Shows what the scripts reported since the last call.
    fn handle_events(&mut self) {
        let Some(scripts) = &mut self.scripts else { return };
        for e in scripts.take_events() {
            log::info!("mission: {}", acr_mission::describe(&e));
            match e {
                MissionEvent::Page { header, text, .. } => {
                    self.open(false, |a| Box::new(MessageWindow::new(Window::Page, &header, text, a)))
                }
                MissionEvent::Question { header, text, .. } => {
                    self.open(true, |a| Box::new(MessageWindow::new(Window::Question, &header, text, a)))
                }
                MissionEvent::Choice { header, text, choices, width, orient, .. } => {
                    self.open(true, |a| Box::new(ChoiceWindow::new(width, orient, &header, text, choices, a)))
                }
                MissionEvent::Hint { text, ticks, list } => self.messages.add(text, ticks, list),
                // Drawn from the script state while its time runs.
                MissionEvent::CentralText { .. } => {}
                MissionEvent::Victory | MissionEvent::Defeat if self.ended.is_none() => {
                    self.ended = Some(matches!(e, MissionEvent::Victory));
                    self.ended_at = self.clock;
                }
                MissionEvent::Victory | MissionEvent::Defeat => {}
                MissionEvent::Error { title, text } => log::error!("{title}: {text}"),
                MissionEvent::Camera { x, y } => self.camera = Some((x, y)),
                MissionEvent::Briefing => {}
                MissionEvent::Diplomacy { command } => self.diplomacy.push(command),
            }
        }
    }

    /// Browser commands of the diplomacy panel DipServer sent (`ActivateDipDialog`), once.
    pub fn take_diplomacy(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.diplomacy)
    }

    /// `CheckDipBuilding(MyNation, index)`: a press of the left button on object `index`.
    pub fn dip_check_building(&mut self, world: &mut World, index: u16) {
        let Some(scripts) = &mut self.scripts else { return };
        scripts.dip_check_building(world, self.local_player, index);
        self.handle_events();
    }

    /// A request of a diplomacy page (`perform&1&0&2`); returns the browser commands of the
    /// answer.
    pub fn dip_request(&mut self, world: &mut World, request: &[u8]) -> Vec<acr_mission::diplomacy::DipCommand> {
        let Some(scripts) = &mut self.scripts else { return Vec::new() };
        let answer = scripts.dip_request(world, request);
        self.handle_events();
        answer
    }

    /// A page DipServer wrote (`Dip\tmp.cml`) or one of its files.
    pub fn dip_file(&self, path: &str) -> Option<Vec<u8>> {
        let path = path.replace("//", "/");
        let s = self.scripts.as_ref()?;
        s.script_file(&path).map(<[u8]>::to_vec).or_else(|| self.file(&path).map(<[u8]>::to_vec))
    }

    /// Whether DipServer runs.
    pub fn has_diplomacy(&self) -> bool {
        self.scripts.as_ref().is_some_and(Scripts::has_diplomacy)
    }

    /// Queues a window; `question`: the script waits for its answer.
    fn open(&mut self, question: bool, screen: impl FnOnce(Answer) -> Box<dyn Screen>) {
        let answer: Answer = Rc::new(Cell::new(None));
        self.pending.push_back(Pending { screen: Some(screen(answer.clone())), answer, question });
    }

    /// The next mission window to show, once (pages and questions in the order the script
    /// asked; the next one follows when this one is closed).
    pub fn window(&mut self) -> Option<Box<dyn Screen>> {
        self.pending.front_mut().and_then(|p| p.screen.take())
    }

    /// A page or question waits for the player.
    pub fn window_open(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Closes the first waiting window with `answer` without showing it (a loaded game
    /// replays the player's answers).
    pub fn answer_window(&mut self, answer: u32) {
        if let Some(p) = self.pending.pop_front()
            && p.question
            && let Some(s) = &mut self.scripts
        {
            s.answer(answer);
        }
    }

    /// Answers of the windows the player closed since the last call, in order.
    pub fn take_answers(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.answered)
    }

    /// The objectives window (F1, 0x5e11c0): only in a mission with objectives text.
    pub fn objectives(&self) -> Option<Box<dyn Screen>> {
        let briefing = self.scripts.as_ref()?.briefing();
        if briefing.is_empty() || !self.pending.is_empty() {
            return None;
        }
        Some(Box::new(MessageWindow::new(Window::Objectives, "NONE", briefing, Rc::new(Cell::new(None)))))
    }

    /// The victory or defeat text has been shown long enough: back to the menus (missions
    /// show no statistics, 0x5ab55d).
    pub fn over(&self) -> bool {
        self.ended.is_some() && (self.leave || self.clock - self.ended_at > END_SECONDS)
    }

    /// Escape: after the end of the game it leaves without waiting ("press ESC", 0x4a31f2
    /// sets `0x11eaf4c` to 2); returns whether it did.
    pub fn escape(&mut self) -> bool {
        self.leave = self.ended.is_some();
        self.leave
    }

    /// A view position the script asked for (`SetStartPoint`), once.
    pub fn take_camera(&mut self) -> Option<(i32, i32)> {
        self.camera.take()
    }

    /// Files requested while running (AI DLLs started by the mission).
    pub fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.new_requests)
    }

    /// Mission texts over the map (1024 × 768 canvas): hints, then the central box with the
    /// end of the game or `ShowCentralText` (a text starting with `#` wins over the end).
    pub fn draws(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.messages.draw(out);
        let central = self.scripts.as_ref().and_then(|s| s.state().central_text.as_ref()).map(|(t, _)| t.as_slice());
        let end = self.ended.map(|won| ctx.texts.get(if won { "WINNER" } else { "LOOSER" }));
        let text = match (central, end) {
            (Some(c), _) if c.first() == Some(&b'#') => Some(&c[1..]),
            (_, Some(e)) => Some(e),
            (c, None) => c,
        };
        if let Some(t) = text.filter(|t| !t.is_empty()) {
            crate::mission_ui::central_text(ctx, out, t, self.local_player);
        }
    }
}
