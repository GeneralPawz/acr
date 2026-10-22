//! The mission script of the running game (`acr-mission`): loads the DLL, the mission texts
//! and the AI DLLs, runs them before every simulation tick and shows what they report.
//!
//! The original shows `ShowPage` pages as modal windows inside the script call, so the game
//! stands still until the player closes them; ACR pauses the simulation while a page or a
//! question is open. Hints and central texts are drawn over the map for their time. Until
//! the in-game interface has its own message widgets this module draws them with the menu
//! fonts.

use std::collections::BTreeMap;

use acr_formats::M3dMap;
use acr_formats::string_table::Texts;
use acr_mission::{MissionEvent, ScriptSetup, Scripts};
use acr_sim::{Command, World};

use crate::ui::draw::{Draw, Rect};
use crate::ui::fonts::FontId;
use crate::ui::setup::Control;
use crate::ui::{Ctx, GameKind, GameSetup};

/// Breaks a line into pieces at most `max` pixels wide, at spaces.
fn wrap(line: &[u8], max: i32, width: &dyn Fn(&[u8]) -> i32) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    for word in line.split(|&b| b == b' ') {
        let mut next = cur.clone();
        if !next.is_empty() {
            next.push(b' ');
        }
        next.extend_from_slice(word);
        if !cur.is_empty() && width(&next) > max {
            out.push(std::mem::take(&mut cur));
            cur = word.to_vec();
        } else {
            cur = next;
        }
    }
    out.push(cur);
    out
}

const MISSIONS: &str = "Missions\\Missions.txt";
const NATIONS: &str = "NATIONS.LST";
/// The diplomacy system for neutral tribes, loaded in every game like the original.
const DIPSERVER: &str = "DipServer.dll";
/// Seconds a hint stays when the script gives no time.
const MESSAGE_SECONDS: f64 = 6.0;
const FONT: FontId = FontId("fon16y5");
const PAGE_FONT: FontId = FontId("fon16y3");

/// A dialog the player has to close before the game goes on.
enum Dialog {
    Page(Vec<u8>),
    Question(Vec<u8>),
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
    messages: Vec<(Vec<u8>, f64)>,
    dialog: Option<Dialog>,
    clock: f64,
    /// `Some(true)` after victory, `Some(false)` after defeat.
    pub ended: Option<bool>,
    /// `SetStartPoint`: centre the view here (world pixels).
    camera: Option<(i32, i32)>,
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
            messages: Vec::new(),
            dialog: None,
            clock: 0.0,
            ended: None,
            camera: None,
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
        self.messages.retain(|(_, until)| *until > self.clock);
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
    /// question is open, or the game has ended).
    pub fn before_step(&mut self, world: &mut World) -> Option<Vec<Command>> {
        if self.dialog.is_some() || self.ended.is_some() {
            return None;
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
        for e in scripts.take_events() {
            log::info!("mission: {}", acr_mission::describe(&e));
            match e {
                MissionEvent::Page { text, .. } => self.dialog = Some(Dialog::Page(text)),
                MissionEvent::Question { text, .. } | MissionEvent::Choice { text, .. } => {
                    self.dialog = Some(Dialog::Question(text));
                }
                MissionEvent::Hint { text, ticks } => {
                    let secs = if ticks == 0 { MESSAGE_SECONDS } else { f64::from(ticks) / 10.0 };
                    self.messages.push((text, self.clock + secs.max(2.0)));
                }
                MissionEvent::CentralText { text, ticks } => {
                    self.messages.push((text, self.clock + f64::from(ticks) / 10.0));
                }
                MissionEvent::Victory => self.ended = Some(true),
                MissionEvent::Defeat => self.ended = Some(false),
                MissionEvent::Error { title, text } => log::error!("{title}: {text}"),
                MissionEvent::Camera { x, y } => self.camera = Some((x, y)),
                MissionEvent::Briefing => {}
            }
        }
        if scripts.waiting() {
            return None;
        }
        Some(commands)
    }

    /// A view position the script asked for (`SetStartPoint`), once.
    pub fn take_camera(&mut self) -> Option<(i32, i32)> {
        self.camera.take()
    }

    /// Files requested while running (AI DLLs started by the mission).
    pub fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.new_requests)
    }

    /// Closes an open page; answers an open question with `yes`.
    pub fn close_dialog(&mut self, yes: bool) -> bool {
        match self.dialog.take() {
            Some(Dialog::Question(_)) => {
                if let Some(s) = &mut self.scripts {
                    s.answer(u32::from(yes));
                }
                true
            }
            Some(Dialog::Page(_)) => true,
            None => false,
        }
    }

    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// Message overlay on the 1024 × 768 canvas.
    pub fn draws(&self, ctx: &Ctx, out: &mut Vec<Draw>) {
        let mut y = 520;
        for (text, _) in self.messages.iter().rev().take(4) {
            out.push(Draw::text(FONT, text, 40, y));
            y -= 22;
        }
        let banner = match self.ended {
            Some(true) => Some(&b"WINNER"[..]),
            Some(false) => Some(&b"LOOSER"[..]),
            None => None,
        };
        if let Some(b) = banner {
            out.push(Draw::text(FontId("fonG60y5"), b, 400, 340));
        }
        let Some(dialog) = &self.dialog else { return };
        let (text, hint): (&[u8], &[u8]) = match dialog {
            Dialog::Page(t) => (t, b"Enter / click: OK"),
            Dialog::Question(t) => (t, b"Enter / click: OK    Esc: No"),
        };
        let rect = Rect::new(162, 134, 700, 500);
        out.push(Draw::fill([0, 0, 0, 200], rect));
        let width = |t: &[u8]| ctx.fonts.width(ctx.lib, PAGE_FONT, t);
        let mut y = rect.y0 + 20;
        for line in text.split(|&b| b == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            for part in wrap(line, rect.width() - 40, &width) {
                if y > rect.y1 - 50 {
                    break;
                }
                out.push(Draw::text(PAGE_FONT, &part, rect.x0 + 20, y).clipped(rect));
                y += 18;
            }
        }
        out.push(Draw::text(PAGE_FONT, hint, rect.x0 + 20, rect.y1 - 30));
    }
}
