//! The original menus, rebuilt as a small retained UI.
//!
//! - [`library`]: game files loaded on demand, [`texts`]: interface texts, [`fonts`]:
//!   the original fonts.
//! - [`dialog`]: widgets of one screen (the original `DialogsSystem`).
//! - [`draw`]: sprites on the 1024x768 canvas and their GPU textures.
//! - [`screens`]: the menu screens and the navigation between them.
//!
//! Layouts, fonts and behaviour come from the original's dialog code (see `docs/ui.md`).

pub mod dialog;
pub mod draw;
pub mod fonts;
pub mod library;
pub mod missions;
pub mod screens;
pub mod settings;
pub mod setup;
pub mod texts;

use acr_render::{ImageDraw, ImageRenderer};

use crate::canvas::Canvas;
use draw::{Draw, Rect, Textures};
use fonts::Fonts;
use library::Library;
pub use settings::Settings;
pub use setup::{Difficulty, GameKind, GameSetup, MissionSetup};
use texts::Texts;

/// Menu ticks per second: animations such as the main menu slide advance once per tick.
pub const TICK_RATE: f64 = 60.0;

/// What the menus ask the app to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    Quit,
    StartGame(Box<GameSetup>),
    /// The options changed; apply volumes and window size.
    Settings(Box<Settings>),
    /// Leave the running game for the main menu (`INTF_GMENU_PAD4`).
    LeaveGame,
    /// Save the running game under this name (CP1252), from the save dialog.
    SaveGame(Vec<u8>),
    /// Start a saved game, from a load dialog.
    LoadGame(Box<crate::savegame::SaveGame>),
}

pub use dialog::Key;

/// Input forwarded from the window, in canvas coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    MouseMove([f32; 2]),
    MouseDown([f32; 2]),
    MouseUp([f32; 2]),
    /// Wheel notches, positive away from the user.
    Wheel(f32),
    Key(Key),
}

/// The menu mouse cursor: `Cursor_01.GP` frame 0 (cursor pack = palette set + 5, set by
/// `LoadFog(1)`), hotspot (0, 0) from `curs_xy.dat`, drawn over everything.
const CURSOR: &str = "CURSOR_01.GP";

/// Files every screen uses: cursor, sounds and the random table of drop-down rows.
const ALWAYS: [&str; 7] = [
    CURSOR,
    dialog::RANDOM_LST,
    dialog::sound::CLICK,
    dialog::sound::START,
    dialog::sound::HOVER[0],
    dialog::sound::HOVER[1],
    dialog::sound::HOVER[2],
];

/// Shared services for screens.
pub struct Ctx<'a> {
    pub lib: &'a Library,
    pub fonts: &'a Fonts,
    pub texts: &'a Texts,
    pub settings: &'a Settings,
    pub profile: &'a std::cell::RefCell<screens::player_name::Profile>,
    /// Where savegames are kept.
    pub saves: &'a dyn acr_app::saves::SaveStore,
    /// Written player profiles (`Players\<name>.txt`), see `single_mission::progress`.
    pub profiles: &'a dyn acr_app::saves::SaveStore,
}

/// What a screen wants after handling input or a tick.
pub enum Nav {
    Stay,
    /// Open a screen on top; returning pops back here.
    Push(Box<dyn Screen>),
    /// Close this screen.
    Pop,
    /// Close this screen and report `value` to the one below (`on_return`).
    Return(i32),
    Quit,
    /// Close this screen and open another in its place.
    Replace(Box<dyn Screen>),
    Start(Box<GameSetup>),
    /// Store new options and close this screen.
    Apply(Box<Settings>),
    /// Close every screen and leave the running game.
    LeaveGame,
    /// Close this screen and save the running game under this name.
    Save(Vec<u8>),
    /// Close every screen and start a saved game.
    Load(Box<crate::savegame::SaveGame>),
}

pub trait Screen {
    /// Files that must be loaded before the screen is built.
    fn assets(&self, ctx: &Ctx) -> Vec<String>;
    /// Creates the widgets once [`Screen::assets`] are available.
    fn build(&mut self, ctx: &Ctx);
    /// One menu tick of `1 / TICK_RATE` seconds.
    fn tick(&mut self, _ctx: &Ctx) -> Nav {
        Nav::Stay
    }
    fn input(&mut self, ctx: &Ctx, input: Input) -> Nav;
    /// A screen pushed by this one closed with [`Nav::Return`].
    fn on_return(&mut self, _ctx: &Ctx, _value: i32) -> Nav {
        Nav::Stay
    }
    fn draw(&self, ctx: &Ctx, out: &mut Vec<Draw>);
    /// Sounds to play (game paths), collected since the last call.
    fn take_sounds(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// A dialog over the running game: drawn with the game's palette.
    fn in_game(&self) -> bool {
        false
    }
    /// Input would be dropped now, e.g. while the main menu slides; the test driver waits.
    fn busy(&self) -> bool {
        false
    }
    /// The screen's widgets, for the test driver's widget tree (`drive`).
    fn dialog(&self) -> Option<&dialog::Dialog> {
        None
    }
    /// The screen's type name, e.g. `MainMenu`, for the test driver.
    fn name(&self) -> &'static str {
        let full = std::any::type_name::<Self>();
        full.rsplit("::").next().unwrap_or(full)
    }
}

struct Entry {
    screen: Box<dyn Screen>,
    built: bool,
}

/// Name of the options file in the settings store.
const SETTINGS_FILE: &str = "settings";

/// All menu state: the screen stack plus the loaded files, texts and textures.
pub struct Ui {
    lib: Library,
    fonts: Fonts,
    texts: Texts,
    text_sources: Vec<String>,
    texts_loaded: bool,
    stack: Vec<Entry>,
    textures: Textures,
    mouse: Option<[f32; 2]>,
    sounds: Vec<String>,
    tick_time: f64,
    settings: Settings,
    profile: std::cell::RefCell<screens::player_name::Profile>,
    /// A game is running: the screens are dialogs over it and closing the last one
    /// returns to the game.
    in_game: bool,
    /// The game's cursor frame of `CURSOR_00.GP` and its hotspot, while no dialog is open.
    game_cursor: Option<(usize, [i32; 2])>,
    saves: Box<dyn acr_app::saves::SaveStore>,
    profiles: Box<dyn acr_app::saves::SaveStore>,
    /// Where the options are kept between runs (ACR's `settings.txt`, see
    /// [`Settings::encode`]); none in tests.
    settings_store: Option<Box<dyn acr_app::saves::SaveStore>>,
    /// Shown on the loading screen while a saved game is replayed (0..=1).
    loading_progress: Option<f32>,
}

impl Ui {
    pub fn new(paths: &[String]) -> Self {
        let lib = Library::new(paths);
        let normalized: Vec<String> = paths.iter().map(|p| acr_formats::normalize_path(p)).collect();
        let text_sources = Texts::sources(&normalized);
        let mut ui = Self {
            lib,
            fonts: Fonts::default(),
            texts: Texts::default(),
            text_sources,
            texts_loaded: false,
            stack: Vec::new(),
            textures: Textures::default(),
            mouse: None,
            sounds: Vec::new(),
            tick_time: 0.0,
            settings: Settings::default(),
            profile: Default::default(),
            in_game: false,
            game_cursor: None,
            saves: Box::new(acr_app::saves::MemorySaves::default()),
            profiles: Box::new(acr_app::saves::MemorySaves::default()),
            settings_store: None,
            loading_progress: None,
        };
        ui.stack.push(Entry { screen: Box::new(screens::main_menu::MainMenu::default()), built: false });
        ui
    }

    /// The open screens from the bottom, each with whether it is built (test driver).
    pub fn screens(&self) -> impl Iterator<Item = (&dyn Screen, bool)> {
        self.stack.iter().map(|e| (&*e.screen as &dyn Screen, e.built))
    }

    /// The canvas position of the mouse, as the screens last saw it.
    pub fn mouse(&self) -> Option<[f32; 2]> {
        self.mouse
    }

    /// The started game is shown: the loading screen goes away.
    pub fn enter_game(&mut self) {
        self.stack.clear();
        self.in_game = true;
    }

    pub fn in_game(&self) -> bool {
        self.in_game
    }

    /// The loading screen, e.g. while a mission restarts.
    pub fn show_loading(&mut self) {
        self.stack.clear();
        self.stack.push(Entry { screen: Box::new(screens::loading::Loading::default()), built: false });
    }

    /// Opens the game menu (F12, `UM_MENU`) over the running game.
    pub fn open_game_menu(&mut self) {
        if self.in_game && self.stack.is_empty() {
            self.stack.push(Entry { screen: Box::new(screens::game_menu::GameMenu::default()), built: false });
        }
    }

    /// Opens a dialog over the running game (mission pages and questions, objectives).
    pub fn open_dialog(&mut self, screen: Box<dyn Screen>) {
        if self.in_game {
            self.stack.push(Entry { screen, built: false });
        }
    }

    /// Whether a dialog over the running game takes the input.
    pub fn dialog_open(&self) -> bool {
        self.in_game && !self.stack.is_empty()
    }

    /// The game's cursor (`CURSOR_00.GP` frame and hotspot); `None` shows the menu cursor.
    pub fn set_game_cursor(&mut self, cursor: Option<(usize, [i32; 2])>) {
        self.game_cursor = cursor;
    }

    /// The mouse in canvas coordinates while the game has the input.
    pub fn set_mouse(&mut self, p: [f32; 2]) {
        self.mouse = Some(p);
    }

    /// Replaces a dynamic image drawn with [`draw::Sprite::Image`].
    pub fn set_image(&mut self, name: &'static str, image: acr_formats::raster::Image) {
        self.textures.set_image(name, image);
    }

    /// Back to the menu a game was started from, like the original whose menu loops start
    /// the game and continue after it (0x5c4e30, 0x5d66b0): the campaign's or the single
    /// missions' list; random maps return to the main menu. A won campaign mission opens
    /// the missions it names and stores the player's progress (0x4bcbb1).
    pub fn finish_game(&mut self, setup: &GameSetup, won: Option<bool>) {
        self.leave_game();
        let GameKind::Mission(mission) = &setup.kind else { return };
        match mission.campaign {
            Some((campaign, index)) => {
                if won == Some(true) {
                    let mut progress = screens::single_mission::progress(&self.ctx());
                    progress.win(campaign, index);
                    self.store_progress(progress);
                }
                self.push(Box::new(screens::campaign::Campaigns::at(campaign)));
                self.push(Box::new(screens::single_mission::SingleMission::campaign(campaign).at(index)));
            }
            None => self.push(Box::new(screens::single_mission::SingleMission::default())),
        }
    }

    /// Keeps the player's campaign progress and writes it to `Players\<name>.txt`.
    fn store_progress(&mut self, progress: acr_formats::missions::Progress) {
        let name = acr_formats::CodePage::Cp1252.decode(&self.profile.borrow().name);
        if let Err(e) = self.profiles.write(&name, &progress.write()) {
            log::error!("campaign progress of {name}: {e}");
        }
        self.profile.borrow_mut().progress = Some(progress);
    }

    fn push(&mut self, screen: Box<dyn Screen>) {
        self.stack.push(Entry { screen, built: false });
    }

    /// Back to the main menu after a game ends or fails to start.
    pub fn leave_game(&mut self) {
        self.in_game = false;
        self.stack.clear();
        self.stack.push(Entry { screen: Box::new(screens::main_menu::MainMenu::default()), built: false });
    }

    /// Files to request from the platform; deliver them with [`Ui::on_loaded`].
    pub fn requests(&mut self) -> Vec<String> {
        self.prepare();
        self.lib.take_requests()
    }

    /// Takes a loaded file; returns whether the menus asked for it.
    pub fn on_loaded(&mut self, path: &str, result: Result<Vec<u8>, String>) -> bool {
        self.lib.insert(path, result)
    }

    /// Sounds to play since the last call (game paths of `.wav` files, already loaded).
    pub fn take_sounds(&mut self) -> Vec<(String, std::rc::Rc<[u8]>)> {
        let mut ready = Vec::new();
        for path in std::mem::take(&mut self.sounds) {
            match self.lib.bytes(&path) {
                Some(bytes) => ready.push((path, bytes)),
                None if !self.lib.settled(&path) => self.sounds.push(path),
                None => {}
            }
        }
        ready
    }

    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            lib: &self.lib,
            fonts: &self.fonts,
            texts: &self.texts,
            settings: &self.settings,
            profile: &self.profile,
            saves: &*self.saves,
            profiles: &*self.profiles,
        }
    }

    /// Loads texts and fonts, then builds the top screen once its files are there.
    fn prepare(&mut self) {
        if !self.texts_loaded {
            if !self.text_sources.iter().all(|p| self.lib.settled(p)) {
                return;
            }
            if !self.lib.settled(screens::player_name::PLAYERS) {
                return;
            }
            for path in &self.text_sources {
                if let Some(bytes) = self.lib.bytes(path) {
                    self.texts.add(&bytes);
                }
            }
            if let Some(players) = self.lib.bytes(screens::player_name::PLAYERS) {
                *self.profile.borrow_mut() = screens::player_name::Profile::load(&players);
            }
            self.texts_loaded = true;
        }
        for path in self.fonts.sources().iter().map(String::as_str).chain(ALWAYS) {
            self.lib.want(path);
        }
        if !self.fonts.ready(&self.lib) {
            return;
        }
        let Some(top) = self.stack.last() else { return };
        if top.built {
            return;
        }
        let ctx = Ctx {
            lib: &self.lib,
            fonts: &self.fonts,
            texts: &self.texts,
            settings: &self.settings,
            profile: &self.profile,
            saves: &*self.saves,
            profiles: &*self.profiles,
        };
        let assets = top.screen.assets(&ctx);
        let waiting: Vec<&String> = assets.iter().filter(|p| !self.lib.settled(p)).collect();
        if !waiting.is_empty() {
            log::debug!("screen waits for {waiting:?}");
            return;
        }
        log::debug!("building screen {}", self.stack.len());
        let top = self.stack.last_mut().unwrap();
        top.screen.build(&ctx);
        top.built = true;
        self.textures.trim();
        if let Some(p) = self.mouse {
            top.screen.input(&ctx, Input::MouseMove(p));
        }
    }

    fn ready(&self) -> bool {
        self.stack.last().is_some_and(|e| e.built)
    }

    /// Advances animations by `dt` seconds.
    pub fn update(&mut self, dt: f64) -> Action {
        self.prepare();
        if !self.ready() {
            return Action::None;
        }
        self.tick_time = (self.tick_time + dt).min(0.25);
        let mut action = Action::None;
        while self.tick_time >= 1.0 / TICK_RATE {
            self.tick_time -= 1.0 / TICK_RATE;
            let ctx = Ctx {
                lib: &self.lib,
                fonts: &self.fonts,
                texts: &self.texts,
                settings: &self.settings,
                profile: &self.profile,
                saves: &*self.saves,
                profiles: &*self.profiles,
            };
            let nav = self.stack.last_mut().map_or(Nav::Stay, |e| e.screen.tick(&ctx));
            self.collect_sounds();
            action = self.navigate(nav);
            if action != Action::None || !self.ready() {
                break;
            }
        }
        action
    }

    pub fn input(&mut self, input: Input) -> Action {
        match input {
            Input::MouseMove(p) | Input::MouseDown(p) | Input::MouseUp(p) => self.mouse = Some(p),
            _ => {}
        }
        if !self.ready() {
            return Action::None;
        }
        let ctx = Ctx {
            lib: &self.lib,
            fonts: &self.fonts,
            texts: &self.texts,
            settings: &self.settings,
            profile: &self.profile,
            saves: &*self.saves,
            profiles: &*self.profiles,
        };
        let nav = self.stack.last_mut().unwrap().screen.input(&ctx, input);
        self.collect_sounds();
        self.navigate(nav)
    }

    fn collect_sounds(&mut self) {
        if let Some(top) = self.stack.last_mut() {
            for sound in top.screen.take_sounds() {
                self.lib.want(&sound);
                self.sounds.push(sound);
            }
        }
    }

    fn navigate(&mut self, nav: Nav) -> Action {
        match nav {
            Nav::Stay => Action::None,
            Nav::Push(screen) => {
                log::debug!("opening screen {}", self.stack.len() + 1);
                self.stack.push(Entry { screen, built: false });
                Action::None
            }
            Nav::Replace(screen) => {
                self.stack.pop();
                self.stack.push(Entry { screen, built: false });
                Action::None
            }
            Nav::Pop => {
                self.stack.pop();
                self.after_pop()
            }
            Nav::Return(value) => {
                self.stack.pop();
                let ctx = Ctx {
                    lib: &self.lib,
                    fonts: &self.fonts,
                    texts: &self.texts,
                    settings: &self.settings,
                    profile: &self.profile,
                    saves: &*self.saves,
                    profiles: &*self.profiles,
                };
                let nav = self.stack.last_mut().map_or(Nav::Quit, |e| e.screen.on_return(&ctx, value));
                let action = self.navigate(nav);
                if action == Action::None { self.after_pop() } else { action }
            }
            Nav::Quit => Action::Quit,
            Nav::Apply(settings) => {
                self.settings = (*settings).clone();
                if let Some(store) = &mut self.settings_store
                    && let Err(e) = store.write(SETTINGS_FILE, self.settings.encode().as_bytes())
                {
                    log::warn!("cannot store the options: {e}");
                }
                self.stack.pop();
                self.after_pop();
                Action::Settings(settings)
            }
            Nav::LeaveGame => {
                self.leave_game();
                Action::LeaveGame
            }
            Nav::Save(name) => {
                self.stack.pop();
                self.after_pop();
                Action::SaveGame(name)
            }
            Nav::Load(save) => {
                self.in_game = false;
                self.show_loading();
                Action::LoadGame(save)
            }
            Nav::Start(setup) => {
                self.stack.clear();
                self.stack.push(Entry { screen: Box::new(screens::loading::Loading::default()), built: false });
                Action::StartGame(setup)
            }
        }
    }

    fn after_pop(&mut self) -> Action {
        if self.stack.is_empty() {
            return if self.in_game { Action::None } else { Action::Quit };
        }
        self.textures.trim();
        if let Some(p) = self.mouse {
            let ctx = Ctx {
                lib: &self.lib,
                fonts: &self.fonts,
                texts: &self.texts,
                settings: &self.settings,
                profile: &self.profile,
                saves: &*self.saves,
                profiles: &*self.profiles,
            };
            self.stack.last_mut().unwrap().screen.input(&ctx, Input::MouseMove(p));
        }
        Action::None
    }

    /// Where savegames are kept (the game folder, or the browser's storage).
    pub fn set_profiles(&mut self, profiles: Box<dyn acr_app::saves::SaveStore>) {
        self.profiles = profiles;
    }

    pub fn set_saves(&mut self, saves: Box<dyn acr_app::saves::SaveStore>) {
        self.saves = saves;
    }

    /// Where the options are kept between runs; reads them from there. True if a stored
    /// file was found.
    pub fn set_settings_store(&mut self, store: Box<dyn acr_app::saves::SaveStore>) -> bool {
        let stored = store.read(SETTINGS_FILE);
        if let Some(bytes) = &stored {
            self.settings = Settings::decode(&String::from_utf8_lossy(bytes));
        }
        self.settings_store = Some(store);
        stored.is_some()
    }

    pub fn saves_mut(&mut self) -> &mut dyn acr_app::saves::SaveStore {
        &mut *self.saves
    }

    /// The replay progress of a loaded game, shown on the loading screen.
    pub fn set_loading_progress(&mut self, progress: Option<f32>) {
        self.loading_progress = progress;
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Image draws for the current frame, in window pixels: `layers` (each with its own
    /// canvas, e.g. the in-game interface), then the screens and the cursor; with the number
    /// of draws from the first layer (the in-game world overlays, which the fog covers).
    pub fn draws(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &ImageRenderer,
        canvas: &Canvas,
        layers: &[(Canvas, Vec<Draw>)],
    ) -> (Vec<ImageDraw>, usize) {
        let mut out = Vec::new();
        let mut first = 0;
        for (i, (c, draws)) in layers.iter().enumerate() {
            self.textures.resolve(device, queue, images, &self.lib, &self.fonts, c, draws, &mut out);
            if i == 0 {
                first = out.len();
            }
        }
        let mut draws = Vec::new();
        // Keep showing the previous screen while the next one loads.
        if let Some(entry) = self.stack.iter().rev().find(|e| e.built) {
            let ctx = self.ctx();
            entry.screen.draw(&ctx, &mut draws);
            if entry.screen.in_game() {
                draws = draws.into_iter().map(Draw::in_game).collect();
            }
            if let (Some(p), false) = (self.loading_progress, self.in_game) {
                // ACR's own: the original restores a savegame without a progress display.
                let text = format!("{} {}%", String::from_utf8_lossy(self.texts.get("INTF_LOAD")), (p * 100.0) as u32);
                let font = screens::font::F18W;
                let w = self.fonts.width(&self.lib, font, text.as_bytes());
                draws.push(Draw::text(font, text.as_bytes(), 512 - w / 2, 700));
            }
        }
        self.textures.resolve(device, queue, images, &self.lib, &self.fonts, canvas, &draws, &mut out);
        // The cursor goes anywhere in the window, also beside the menu canvas.
        let mut draws = Vec::new();
        if let Some([x, y]) = self.mouse {
            let (x, y) = (x.floor() as i32, y.floor() as i32);
            match self.game_cursor {
                _ if self.in_game && !self.stack.is_empty() => {
                    draws.push(Draw::game_frame(crate::hud::cursor::GP, 0, x, y, 0))
                }
                Some((frame, [hx, hy])) if self.in_game => {
                    draws.push(Draw::game_frame(crate::hud::cursor::GP, frame, x - hx, y - hy, 0))
                }
                _ => draws.push(Draw::frame(CURSOR, 0, x, y)),
            }
        }
        let window = Canvas { clip: Rect::ALL, ..*canvas };
        self.textures.resolve(device, queue, images, &self.lib, &self.fonts, &window, &draws, &mut out);
        (out, first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAMPAIGNS: &[u8] =
        b"1\r\nKolumbus\r\nk.txt\r\nk.bmp\r\n3\r\n#c1@ ON 1 1 1\r\n#c2@ OFF 2 2 2\r\n#c3@ OFF 1 1 1\r\n";

    fn campaign_setup(index: usize) -> GameSetup {
        GameSetup {
            kind: GameKind::Mission(MissionSetup {
                id: format!("#c{}@", index + 1),
                name: Vec::new(),
                map: String::new(),
                script: String::new(),
                texts: String::new(),
                difficulty: Difficulty::from_index(1),
                campaign: Some((0, index)),
            }),
            players: Vec::new(),
            local_player: 0,
        }
    }

    #[test]
    fn a_won_campaign_mission_opens_the_next_and_returns_to_its_list() {
        let mut ui = Ui::new(&[screens::campaign::CAMPAIGNS.to_owned()]);
        ui.lib.want(screens::campaign::CAMPAIGNS);
        ui.lib.take_requests();
        ui.lib.insert(screens::campaign::CAMPAIGNS, Ok(CAMPAIGNS.to_vec()));
        ui.profile.borrow_mut().name = b"Einstein".to_vec();
        ui.finish_game(&campaign_setup(0), Some(false));
        assert_eq!(ui.stack.len(), 3, "main menu, campaigns, the campaign's missions");
        assert_eq!(screens::single_mission::progress(&ui.ctx()).shown(0), 1);

        ui.finish_game(&campaign_setup(0), Some(true));
        let progress = screens::single_mission::progress(&ui.ctx());
        assert!(progress.is_open(0, 1) && !progress.is_open(0, 2));
        assert_eq!(ui.profiles.read("Einstein").as_deref(), Some(&b"1 3 16843009 33686017 16843008 "[..]));

        // A new program run reads the progress back from the written profile.
        ui.profile.borrow_mut().progress = None;
        assert_eq!(screens::single_mission::progress(&ui.ctx()).shown(0), 2);
    }
}
