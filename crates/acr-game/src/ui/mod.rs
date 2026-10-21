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
use draw::{Draw, Textures};
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
}

struct Entry {
    screen: Box<dyn Screen>,
    built: bool,
}

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
        };
        ui.stack.push(Entry { screen: Box::new(screens::main_menu::MainMenu::default()), built: false });
        ui
    }

    /// Back to the main menu after a game ends or fails to start.
    pub fn leave_game(&mut self) {
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

    fn ctx(&self) -> Ctx<'_> {
        Ctx { lib: &self.lib, fonts: &self.fonts, texts: &self.texts, settings: &self.settings, profile: &self.profile }
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
                };
                let nav = self.stack.last_mut().map_or(Nav::Quit, |e| e.screen.on_return(&ctx, value));
                let action = self.navigate(nav);
                if action == Action::None { self.after_pop() } else { action }
            }
            Nav::Quit => Action::Quit,
            Nav::Apply(settings) => {
                self.settings = (*settings).clone();
                self.stack.pop();
                self.after_pop();
                Action::Settings(settings)
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
            return Action::Quit;
        }
        self.textures.trim();
        if let Some(p) = self.mouse {
            let ctx = Ctx {
                lib: &self.lib,
                fonts: &self.fonts,
                texts: &self.texts,
                settings: &self.settings,
                profile: &self.profile,
            };
            self.stack.last_mut().unwrap().screen.input(&ctx, Input::MouseMove(p));
        }
        Action::None
    }

    /// Stack depth and whether the top screen is built, for debugging.
    pub fn debug_state(&self) -> String {
        format!("stack {} built {:?}", self.stack.len(), self.stack.iter().map(|e| e.built).collect::<Vec<_>>())
    }

    /// Rasterizes the current frame's sprites on the CPU (for tests and probes); returns
    /// the number of draws.
    pub fn debug_rasterize(&self) -> usize {
        let mut draws = Vec::new();
        if let Some(entry) = self.stack.iter().rev().find(|e| e.built) {
            entry.screen.draw(&self.ctx(), &mut draws);
        }
        for d in &draws {
            let _ = draw::rasterize(&self.lib, &self.fonts, &d.sprite);
        }
        draws.len()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Image draws for the current frame, in window pixels.
    pub fn draws(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &ImageRenderer,
        canvas: &Canvas,
    ) -> Vec<ImageDraw> {
        let mut draws = Vec::new();
        // Keep showing the previous screen while the next one loads.
        if let Some(entry) = self.stack.iter().rev().find(|e| e.built) {
            let ctx = self.ctx();
            entry.screen.draw(&ctx, &mut draws);
        }
        if let Some([x, y]) = self.mouse {
            draws.push(Draw::frame(CURSOR, 0, x.floor() as i32, y.floor() as i32));
        }
        let mut out = Vec::new();
        self.textures.resolve(device, queue, images, &self.lib, &self.fonts, canvas, &draws, &mut out);
        out
    }
}
