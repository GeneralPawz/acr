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

use crate::acr_missions::AcrMission;
use crate::mission_ui::Messages;
use crate::ui::draw::Draw;
use crate::ui::screens::mission::{Answer, ChoiceWindow, HistoryEntry, HistoryWindow, MessageWindow, Window};
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
    /// One of ACR's own missions: its rules and the computer's orders are Rust, no script.
    acr: Option<AcrMission>,
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
    /// The local player's fog of war: stamped and blurred once per tick from the world and
    /// the scripts' light spots, read by `GetFogSpotVisibility` and drawn by the game.
    fog: Option<acr_sim::fog::Fog>,
    /// The whole map stays revealed (the driver's blank field).
    open_fog: bool,
    /// The world tick the fog was last updated for.
    fog_tick: Option<u64>,
    /// The player whose fog it is (`GameSetup::local_player`).
    viewer: u8,
    difficulty: [u8; 8],
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
    /// The local player's mission history (`Player+0x1a783`): every page and question
    /// shown, oldest first.
    history: Vec<HistoryEntry>,
}

impl MissionLink {
    pub fn new(setup: &GameSetup) -> Self {
        let (script, texts, lookup_map, difficulty) = match &setup.kind {
            GameKind::Mission(m) if !m.script.is_empty() => {
                (Some(m.script.clone()), Some(m.texts.clone()).filter(|t| !t.is_empty()), None, m.difficulty.index())
            }
            GameKind::Mission(m) => (None, None, Some(m.map.clone()), m.difficulty.index()),
            GameKind::RandomMap(_) | GameKind::Blank(_) | GameKind::Acr(_) => (None, None, None, 1),
        };
        // Missions give one value to nations 0..=6 (nation 7 stays 0).
        let mut per_nation = [0u8; 8];
        per_nation[..7].fill(difficulty.min(3) as u8);
        // Random maps put player k's units in nation `color` (random_map.rs).
        let random = matches!(setup.kind, GameKind::RandomMap(_) | GameKind::Blank(_));
        let nation_of = |i: usize| if random { setup.players.get(i).map_or(i as u8, |p| p.color) } else { i as u8 };
        let computers = setup
            .players
            .iter()
            .enumerate()
            // ACR's missions play their computer players themselves ([`AcrMission::orders`]).
            .filter(|(_, p)| matches!(p.control, Control::Computer(_)) && !matches!(setup.kind, GameKind::Acr(_)))
            .map(|(i, _)| nation_of(i))
            .collect();
        // Random maps: every computer player plays at its own combo value (the original's
        // 0x5b4a60 passes it to `StartAI`, which stores it in `City+0x183`, 0x565b28).
        if random {
            for (i, p) in setup.players.iter().enumerate() {
                if let Control::Computer(d) = p.control {
                    per_nation[usize::from(nation_of(i) & 7)] = d.index() as u8;
                }
            }
        }
        let local_player = nation_of(setup.local_player);
        Self {
            acr: if let GameKind::Acr(m) = setup.kind { Some(m) } else { None },
            script,
            texts,
            lookup_map,
            files: BTreeMap::new(),
            text_sources: Vec::new(),
            dip_files: Vec::new(),
            computers,
            local_player,
            fog: None,
            fog_tick: None,
            open_fog: matches!(setup.kind, GameKind::Blank(_)),
            viewer: setup.local_player as u8,
            difficulty: per_nation,
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
            history: Vec::new(),
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
        if self.acr.is_some() {
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
    /// Sets the nations' alliance masks of the scripts' state (the editor's "Kampf" rule), which
    /// they copy to the world every tick.
    pub fn set_alliances(&mut self, masks: [u8; 8]) {
        if let Some(s) = &mut self.scripts {
            s.set_alliances(masks);
        }
    }

    pub fn before_step(&mut self, world: &mut World) -> Option<Vec<Command>> {
        if !self.pending.is_empty() {
            return None;
        }
        self.update_fog(world);
        self.messages.tick();
        if let Some(mission) = self.acr {
            if self.ended.is_none()
                && let Some(won) = mission.verdict(world)
            {
                self.ended = Some(won);
                self.ended_at = self.clock;
            }
            return Some(if self.ended.is_some() { Vec::new() } else { mission.orders(world) });
        }
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
                    self.history.push((header.clone(), text.clone()));
                    self.open(false, |a| Box::new(MessageWindow::new(Window::Page, &header, text, a)))
                }
                MissionEvent::Question { header, text, .. } => {
                    self.history.push((header.clone(), text.clone()));
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

    /// Whether a mission script runs (SCENINF+0x10, 0x28b7758, is set): the original then
    /// offers no attack-ground icon (0x5f63a5).
    pub fn has_script(&self) -> bool {
        self.scripts.as_ref().is_some_and(acr_mission::Scripts::has_mission)
    }

    /// The objectives window (F1, 0x5e11c0): only in a mission with objectives text.
    pub fn objectives(&self) -> Option<Box<dyn Screen>> {
        let briefing = self.scripts.as_ref()?.briefing();
        if briefing.is_empty() || !self.pending.is_empty() {
            return None;
        }
        Some(Box::new(MessageWindow::new(Window::Objectives, "NONE", briefing, Rc::new(Cell::new(None)))))
    }

    /// Whether the history has entries (the "M" drop-down offers `UM_HIST`, 0x4c6cb0).
    pub fn has_history(&self) -> bool {
        !self.history.is_empty()
    }

    /// The mission history window on the newest entry (0x562bea), if there is one.
    pub fn history(&self) -> Option<Box<dyn Screen>> {
        let window = HistoryWindow::newest(self.history.clone().into())?;
        Some(Box::new(window))
    }

    /// The victory or defeat text has been shown long enough: back to the menus (missions
    /// show no statistics, 0x5ab55d).
    pub fn over(&self) -> bool {
        self.ended.is_some() && (self.leave || self.clock - self.ended_at > END_SECONDS)
    }

    /// `AssignHint1` from the interface: a line on the message list for `frames` frames.
    pub fn hint(&mut self, text: Vec<u8>, frames: u32) {
        self.messages.add(text, frames, true);
    }

    /// `UM_SURRENDER` (0x4c6550 value 6): the local player gives up at once, without a
    /// question, like `LooseGame` (the defeat text, then back to the menus).
    pub fn surrender(&mut self) {
        if self.ended.is_none() {
            self.ended = Some(false);
            self.ended_at = self.clock;
        }
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

    /// Brings the fog up to the world's tick: stamped every tick, blurred every game frame,
    /// with the scripts' light spots (`SetLightSpot`, stamped by 0x55a2f1). Exactly once per
    /// tick, so a game that stands still (an open window) and a savegame's replay see the
    /// same fog. The scripts hold it while they run.
    pub fn update_fog(&mut self, world: &World) {
        if self.fog_tick == Some(world.tick.0) {
            return;
        }
        self.fog_tick = Some(world.tick.0);
        let light_spots = self.light_spots();
        let mut fog = match self.scripts.as_mut().and_then(|s| s.set_fog(None)).or_else(|| self.fog.take()) {
            Some(fog) => fog,
            None => acr_sim::fog::Fog::new(world.map.shift),
        };
        fog.reveal(world, self.viewer);
        fog.light_spots = light_spots;
        fog.blur(world);
        if self.open_fog {
            fog.open();
        }
        match self.scripts.as_mut() {
            Some(scripts) => {
                scripts.set_fog(Some(fog));
            }
            None => self.fog = Some(fog),
        }
    }

    /// The local player's fog of war, once the game runs.
    pub fn fog(&self) -> Option<&acr_sim::fog::Fog> {
        self.scripts.as_ref().and_then(|s| s.state().fog.as_ref()).or(self.fog.as_ref())
    }

    /// Reveals the whole map from the next tick on, as on the blank field (the driver's
    /// `fog` request).
    pub fn set_open_fog(&mut self, open: bool) {
        self.open_fog = open;
    }

    /// The fog to draw: none on a revealed map (the driver's blank field).
    pub fn drawn_fog(&self) -> Option<&acr_sim::fog::Fog> {
        self.fog().filter(|_| !self.open_fog)
    }

    /// Files requested while running (AI DLLs started by the mission).
    /// The script's light spots (`SetLightSpot`) for the fog: `(x, y, kind)` of the used
    /// slots (x ≠ 0, as 0x55a2f1 checks); kinds outside 0..=7 stamp like 8.
    pub fn light_spots(&self) -> [Option<(i32, i32, u8)>; acr_mission::state::LIGHT_SPOTS] {
        let mut out = [None; acr_mission::state::LIGHT_SPOTS];
        if let Some(scripts) = &self.scripts {
            for (o, l) in out.iter_mut().zip(scripts.state().light_spots.iter()) {
                *o = (l.x != 0).then_some((l.x, l.y, u8::try_from(l.kind).unwrap_or(8)));
            }
        }
        out
    }

    pub fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.new_requests)
    }

    /// Mission texts over the map (on a screen of `screen` canvas pixels): hints, then the
    /// central box with the end of the game or `ShowCentralText` (a text starting with `#`
    /// wins over the end).
    pub fn draws(&self, ctx: &Ctx, screen: [i32; 2], out: &mut Vec<Draw>) {
        self.messages.draw(screen, out);
        let central = self.scripts.as_ref().and_then(|s| s.state().central_text.as_ref()).map(|(t, _)| t.as_slice());
        let end = self.ended.map(|won| ctx.texts.get(if won { "WINNER" } else { "LOOSER" }));
        let text = match (central, end) {
            (Some(c), _) if c.first() == Some(&b'#') => Some(&c[1..]),
            (_, Some(e)) => Some(e),
            (c, None) => c,
        };
        if let Some(t) = text.filter(|t| !t.is_empty()) {
            crate::mission_ui::central_text(ctx, out, t, self.local_player, screen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::setup::{Difficulty, PlayerSetup, RandomMapSetup};

    /// A random map hands each computer player's combo value to its own nation (the colour).
    #[test]
    fn random_map_difficulty_is_per_computer_player() {
        let player = |color, control| PlayerSetup { name: Vec::new(), nation: 0, color, team: 0, control };
        let setup = GameSetup {
            kind: GameKind::RandomMap(RandomMapSetup::default()),
            players: vec![
                player(2, Control::Human),
                player(5, Control::Computer(Difficulty::Easy)),
                player(0, Control::Computer(Difficulty::Impossible)),
            ],
            local_player: 0,
            rules: Default::default(),
        };
        let link = MissionLink::new(&setup);
        assert_eq!(link.difficulty, [4, 1, 1, 1, 1, 0, 1, 0]);
    }
}
