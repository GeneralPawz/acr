//! American Conquest, rebuilt: the game application.
//!
//! The screen is laid out on the original 1024x768 canvas and scaled to the window with
//! letterboxing, so menus look like the original at any resolution.

mod canvas;
mod hud;
mod ingame;
mod intro;
mod mission;
mod mission_ui;
mod music;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
mod random_map;
mod savegame;
pub mod ui;
mod unit_sounds;
#[cfg(target_arch = "wasm32")]
mod web;

use std::sync::Arc;

use acr_app::{Assets, Gpu};
use acr_render::{ImageDraw, ImageRenderer, SpriteRenderer};
use canvas::Canvas;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

pub enum Event {
    GpuReady(Box<Gpu>),
    Loaded { path: String, result: Result<Vec<u8>, String> },
}

/// GPU resources that exist once the window has a surface.
struct Graphics {
    gpu: Gpu,
    images: ImageRenderer,
    #[allow(dead_code)] // used by the in-game screens that follow
    sprites: SpriteRenderer,
}

/// What the app shows.
enum Mode {
    /// The startup movies, then the menus.
    Intro,
    /// The original menus.
    Menus,
    /// A started game; the menus show the loading screen until it is ready.
    InGame(Box<ingame::InGame>),
}

pub struct Game {
    proxy: EventLoopProxy<Event>,
    assets: Box<dyn Assets>,
    window: Option<Arc<Window>>,
    graphics: Option<Graphics>,
    ui: ui::Ui,
    mode: Mode,
    audio: music::Audio,
    unit_sounds: unit_sounds::UnitSounds,
    intro: intro::Intro,
    cursor: PhysicalPosition<f64>,
    last_frame: Option<web_time::Instant>,
    modifiers: ModifiersState,
}

impl Game {
    pub fn new(proxy: EventLoopProxy<Event>, assets: Box<dyn Assets>) -> Self {
        let paths = assets.paths();
        Self {
            proxy,
            ui: ui::Ui::new(&paths),
            mode: Mode::Intro,
            audio: music::Audio::new(&paths),
            unit_sounds: unit_sounds::UnitSounds::new(),
            intro: intro::Intro::new(&paths),
            assets,
            window: None,
            graphics: None,
            cursor: Default::default(),
            last_frame: None,
            modifiers: ModifiersState::empty(),
        }
    }

    fn load(&self, path: &str) {
        let proxy = self.proxy.clone();
        let owned = path.to_owned();
        self.assets.request(
            path,
            Box::new(move |result| {
                let _ = proxy.send_event(Event::Loaded { path: owned, result });
            }),
        );
    }

    fn canvas(&self) -> Option<Canvas> {
        self.graphics.as_ref().map(|g| Canvas::fit(g.gpu.size()))
    }

    fn virtual_cursor(&self) -> Option<[f32; 2]> {
        Some(self.canvas()?.to_virtual([self.cursor.x as f32, self.cursor.y as f32]))
    }

    /// Hands the menus' file requests to the platform loader.
    fn load_requested(&mut self) {
        let mut paths = self.ui.requests();
        paths.extend(self.unit_sounds.take_requests());
        if let Mode::InGame(game) = &self.mode {
            paths.extend(game.take_requests());
        }
        for path in paths {
            self.load(&path);
        }
    }

    fn play_sounds(&mut self) {
        for (path, bytes) in self.ui.take_sounds() {
            self.audio.play(&path, &bytes);
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        if matches!(self.mode, Mode::Intro) {
            if let Some(path) = self.intro.next_request() {
                self.load(&path);
            }
            if self.intro.update() {
                self.audio.set_music(None);
            }
            if !self.intro.active() {
                self.mode = Mode::Menus;
            }
        } else if let Some(track) = self.audio.next_track() {
            self.load(&track);
        }
        let now = web_time::Instant::now();
        let dt = self.last_frame.map_or(0.0, |t| (now - t).as_secs_f64());
        self.last_frame = Some(now);
        if matches!(self.mode, Mode::Menus) || self.ui.dialog_open() {
            let action = self.ui.update(dt);
            self.apply(action, event_loop);
        }
        self.load_requested();
        self.play_sounds();
        if let Mode::InGame(game) = &self.mode
            && let Some(reason) = game.failure()
        {
            log::error!("cannot start the game: {reason}");
            self.leave_game();
        }
        if let Mode::InGame(game) = &self.mode
            && game.mission_over()
        {
            self.leave_game();
        }
        let Some(g) = &mut self.graphics else { return };
        let mut layers = Vec::new();
        if let Mode::InGame(game) = &mut self.mode {
            game.sync_gpu(&g.gpu.device, &g.gpu.queue, g.gpu.config.format);
            // The game stands still while a dialog (game menu, options) is open.
            let game_dt = if self.ui.dialog_open() { 0.0 } else { dt as f32 };
            game.update(game_dt, g.gpu.size());
            self.ui.set_loading_progress(game.loading_progress());
            self.unit_sounds.play(&game.take_sound_events(), &mut self.audio);
            if game.ready() {
                if !self.ui.in_game() {
                    self.ui.enter_game();
                }
                // Mission pages and questions are dialogs over the game, one at a time.
                if !self.ui.dialog_open()
                    && let Some(window) = game.mission_window()
                {
                    self.ui.open_dialog(window);
                }
                let (l, image, cursor) = game.interface(&self.ui.ctx(), g.gpu.size(), game_dt);
                layers = l;
                // Mission pages, hints and the end of the game over the interface.
                layers.push((Canvas::fit(g.gpu.size()), game.overlay(&self.ui.ctx())));
                if let Some(image) = image {
                    self.ui.set_image(hud::MINIMAP_IMAGE, image);
                }
                self.ui.set_game_cursor(cursor);
            }
        }
        let canvas = Canvas::fit(g.gpu.size());
        let draws: Vec<ImageDraw> = match &self.mode {
            Mode::Intro => {
                self.intro.upload(&g.gpu.device, &g.gpu.queue, &g.images);
                self.intro.draws(&canvas)
            }
            Mode::Menus | Mode::InGame(_) => self.ui.draws(&g.gpu.device, &g.gpu.queue, &g.images, &canvas, &layers),
        };
        let Some(frame) = g.gpu.next_frame() else { return };
        g.images.prepare(&g.gpu.device, &g.gpu.queue, g.gpu.size(), &draws);
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
        let mut load = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        if let Mode::InGame(game) = &mut self.mode
            && game.ready()
        {
            game.render(&g.gpu.device, &g.gpu.queue, &mut encoder, &view, g.gpu.size());
            load = wgpu::LoadOp::Load;
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("game"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            g.images.render(&mut pass);
        }
        g.gpu.queue.submit([encoder.finish()]);
        g.gpu.queue.present(frame);
    }

    fn apply(&mut self, action: ui::Action, event_loop: &ActiveEventLoop) {
        match action {
            ui::Action::None => {}
            ui::Action::Quit => {
                self.audio.stop();
                event_loop.exit();
            }
            ui::Action::LeaveGame => self.leave_game(),
            ui::Action::Settings(settings) => {
                if let Mode::InGame(game) = &mut self.mode {
                    game.apply_settings(&settings);
                }
                let (effects, music) = settings.volumes();
                self.audio.set_volumes(effects, music);
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(window) = &self.window
                    && window.fullscreen().is_none()
                {
                    let [w, h] =
                        ui::settings::RESOLUTIONS[settings.resolution.min(ui::settings::RESOLUTIONS.len() - 1)];
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(w, h));
                }
            }
            ui::Action::StartGame(setup) => {
                log::info!("starting {setup:?}");
                self.start_game(setup);
            }
            ui::Action::SaveGame(name) => self.save_game(&name),
            ui::Action::LoadGame(save) => self.load_game(save),
        }
    }

    /// Click or key during the movies: skip the current one. In the browser the first
    /// gesture only enables audio and restarts the movie with sound.
    fn intro_input(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if !self.audio.started() {
            self.audio.start();
            let audio = self.intro.restart();
            self.audio.set_music(audio);
            return;
        }
        self.intro.skip();
        self.audio.set_music(None);
    }

    /// Forwards input to the menus and the dialogs over the game.
    fn ui_input(&mut self, input: ui::Input, event_loop: &ActiveEventLoop) {
        if matches!(self.mode, Mode::Menus) || self.ui.dialog_open() {
            let action = self.ui.input(input);
            self.apply(action, event_loop);
            self.load_requested();
            self.play_sounds();
        }
    }

    /// Starts the map at `path` (a game path such as `Missions\...\map.m3d`) without the
    /// movies and menus, for development.
    pub fn start_map(&mut self, path: &str) {
        let mission = ui::MissionSetup {
            id: String::new(),
            name: path.as_bytes().to_vec(),
            map: path.to_owned(),
            script: String::new(),
            texts: String::new(),
            difficulty: ui::Difficulty::default(),
            campaign: None,
        };
        let setup = ui::GameSetup { kind: ui::GameKind::Mission(mission), players: Vec::new(), local_player: 0 };
        self.start_game(Box::new(setup));
    }

    fn start_game(&mut self, setup: Box<ui::GameSetup>) {
        let mut game = ingame::InGame::new(setup);
        game.apply_settings(self.ui.settings());
        game.start(&self.assets.paths());
        self.mode = Mode::InGame(Box::new(game));
        self.load_requested();
    }

    /// Where savegames are kept: `Save\` in the game folder, or the browser's storage.
    pub fn set_saves(&mut self, saves: Box<dyn acr_app::saves::SaveStore>) {
        self.ui.set_saves(saves);
    }

    /// The save dialog's OK: writes the running game as `Save\<name>.sav`.
    fn save_game(&mut self, name: &[u8]) {
        let Mode::InGame(game) = &self.mode else { return };
        let (unix, local) = acr_app::saves::local_time();
        let Some(save) = game.snapshot(name, savegame::SaveTime { unix, local }) else { return };
        let file = ui::screens::save_load::file_name(name);
        match self.ui.saves_mut().write(&file, &save.encode()) {
            Ok(()) => log::info!("saved {file} at tick {}", save.tick),
            Err(e) => log::error!("cannot save {file}: {e}"),
        }
    }

    /// A load dialog's OK: starts the saved game, which replays up to its tick.
    fn load_game(&mut self, save: Box<savegame::SaveGame>) {
        log::info!("loading a game saved at tick {}: {:?}", save.tick, save.setup);
        self.ui.leave_game();
        self.ui.show_loading();
        let mut game = ingame::InGame::from_save(save);
        game.apply_settings(self.ui.settings());
        game.start(&self.assets.paths());
        self.mode = Mode::InGame(Box::new(game));
        self.load_requested();
    }

    /// `UM_REPLAY`: the mission starts again from its map.
    fn restart_game(&mut self) {
        let Mode::InGame(game) = &self.mode else { return };
        let setup = game.setup.clone();
        self.ui.leave_game();
        self.ui.show_loading();
        self.start_game(setup);
    }

    /// Ends the current game and returns to the main menu.
    fn leave_game(&mut self) {
        self.ui.leave_game();
        self.mode = Mode::Menus;
        if let Some(window) = &self.window {
            window.set_cursor_visible(false);
        }
        self.load_requested();
    }

    fn alt_down(&self) -> bool {
        self.modifiers.alt_key()
    }

    fn toggle_fullscreen(&self) {
        if let Some(window) = &self.window {
            let next = if window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) };
            window.set_fullscreen(next);
        }
    }
}

impl ApplicationHandler<Event> for Game {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = match event_loop.create_window(window_attributes(event_loop)) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("cannot create window: {e}");
                return event_loop.exit();
            }
        };
        // The menus draw the original cursor.
        window.set_cursor_visible(false);
        self.window = Some(window.clone());
        let proxy = self.proxy.clone();
        acr_app::spawn(async move {
            match Gpu::new(window).await {
                Ok(gpu) => {
                    let _ = proxy.send_event(Event::GpuReady(Box::new(gpu)));
                }
                Err(e) => log::error!("GPU initialisation failed: {e}"),
            }
        });
        self.load_requested();
        // Browsers only allow audio after a user gesture; native starts right away.
        #[cfg(not(target_arch = "wasm32"))]
        self.audio.start();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::GpuReady(gpu) => {
                let gpu = *gpu;
                let images = ImageRenderer::new(&gpu.device, gpu.config.format);
                let sprites = SpriteRenderer::new(&gpu.device, gpu.config.format);
                self.graphics = Some(Graphics { gpu, images, sprites });
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    if let Some(g) = &mut self.graphics {
                        g.gpu.resize(size.width, size.height);
                    }
                }
            }
            Event::Loaded { path, result } => {
                if let Ok(bytes) = &result
                    && self.unit_sounds.on_loaded(&path, bytes)
                {
                    return self.load_requested();
                }
                if let Mode::InGame(game) = &mut self.mode
                    && game.on_loaded(&path, &result)
                {
                    // The in-game interface may have asked for the same file (the palette).
                    self.ui.on_loaded(&path, result);
                    return self.load_requested();
                }
                if let Ok(bytes) = &result {
                    let (movie, audio) = self.intro.on_loaded(&path, bytes);
                    if movie {
                        self.audio.set_music(audio);
                        return;
                    }
                    if self.audio.on_loaded(&path, bytes) {
                        return;
                    }
                }
                if !self.ui.on_loaded(&path, result) {
                    log::debug!("unrequested file {path}");
                }
                self.load_requested();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.apply(ui::Action::Quit, event_loop),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.graphics {
                    g.gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if let Mode::InGame(game) = &mut self.mode {
                    game.mouse_moved([position.x as f32, position.y as f32]);
                }
                if let Some(p) = self.virtual_cursor() {
                    self.ui.set_mouse(p);
                    self.ui_input(ui::Input::MouseMove(p), event_loop);
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } if matches!(self.mode, Mode::Intro) => {
                self.intro_input()
            }
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key, state, .. }, .. }
                if matches!(&self.mode, Mode::InGame(g) if g.ready())
                    && (!self.ui.dialog_open() || state == ElementState::Released)
                    && !matches!(logical_key, Key::Named(NamedKey::F11))
                    && !(logical_key == Key::Named(NamedKey::Enter) && self.alt_down()) =>
            {
                let pressed = state == ElementState::Pressed;
                // F1: the mission objectives (0x4a30e8 → 0x5e11c0).
                if let Mode::InGame(game) = &mut self.mode
                    && pressed
                    && logical_key == Key::Named(NamedKey::F1)
                    && let Some(window) = game.objectives_window()
                {
                    self.ui.open_dialog(window);
                    self.load_requested();
                    return;
                }
                // Escape during the victory or defeat message leaves the game.
                if let Mode::InGame(game) = &mut self.mode
                    && pressed
                    && logical_key == Key::Named(NamedKey::Escape)
                    && game.mission_escape()
                {
                    return;
                }
                if let Mode::InGame(game) = &mut self.mode
                    && game.key(&self.ui.ctx(), &logical_key, pressed) == ingame::GameKey::Menu
                {
                    self.ui.open_game_menu();
                    self.load_requested();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape | NamedKey::Space | NamedKey::Enter),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } if matches!(self.mode, Mode::Intro) && !self.alt_down() => self.intro_input(),
            WindowEvent::MouseInput { state, button, .. }
                if matches!(&self.mode, Mode::InGame(g) if g.ready()) && !self.ui.dialog_open() =>
            {
                let Mode::InGame(game) = &mut self.mode else { return };
                let ctx = self.ui.ctx();
                let result = match (button, state) {
                    (MouseButton::Left, _) => game.left_button(&ctx, state == ElementState::Pressed),
                    (MouseButton::Right, ElementState::Pressed) => {
                        game.right_button(&ctx);
                        ingame::GameMouse::None
                    }
                    _ => ingame::GameMouse::None,
                };
                match result {
                    ingame::GameMouse::Menu => {
                        self.ui.open_game_menu();
                        self.load_requested();
                    }
                    ingame::GameMouse::Restart => self.restart_game(),
                    ingame::GameMouse::None => {}
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                #[cfg(target_arch = "wasm32")]
                self.audio.start();
                if let Some(p) = self.virtual_cursor() {
                    let input = match state {
                        ElementState::Pressed => ui::Input::MouseDown(p),
                        ElementState::Released => ui::Input::MouseUp(p),
                    };
                    self.ui_input(input, event_loop);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.ui_input(ui::Input::Wheel(lines), event_loop);
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key, text, state: ElementState::Pressed, .. },
                ..
            } => {
                #[cfg(target_arch = "wasm32")]
                self.audio.start();
                let key = match logical_key {
                    Key::Named(NamedKey::F11) => return self.toggle_fullscreen(),
                    Key::Named(NamedKey::Enter) if self.alt_down() => return self.toggle_fullscreen(),
                    Key::Named(named) => match named {
                        NamedKey::Enter => Some(ui::Key::Enter),
                        NamedKey::Escape => Some(ui::Key::Escape),
                        NamedKey::ArrowUp => Some(ui::Key::Up),
                        NamedKey::ArrowDown => Some(ui::Key::Down),
                        NamedKey::ArrowLeft => Some(ui::Key::Left),
                        NamedKey::ArrowRight => Some(ui::Key::Right),
                        NamedKey::PageUp => Some(ui::Key::PageUp),
                        NamedKey::PageDown => Some(ui::Key::PageDown),
                        NamedKey::Home => Some(ui::Key::Home),
                        NamedKey::End => Some(ui::Key::End),
                        NamedKey::Backspace => Some(ui::Key::Backspace),
                        NamedKey::Delete => Some(ui::Key::Delete),
                        NamedKey::Space => Some(ui::Key::Char(b' ')),
                        _ => None,
                    },
                    _ => text.as_deref().and_then(cp1252).map(ui::Key::Char),
                };
                if let Some(key) = key {
                    self.ui_input(ui::Input::Key(key), event_loop);
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                if let Mode::InGame(game) = &mut self.mode {
                    game.set_modifiers(m.state());
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// A 4:3 window sized like the original (up to 1024x768 scaled to the monitor), or a
/// canvas filling the page in the browser.
fn window_attributes(event_loop: &ActiveEventLoop) -> winit::window::WindowAttributes {
    let attributes = Window::default_attributes().with_title("American Conquest");
    #[cfg(target_arch = "wasm32")]
    {
        let _ = event_loop;
        winit::platform::web::WindowAttributesExtWebSys::with_append(attributes, true)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let height = event_loop
            .primary_monitor()
            .map(|m| m.size().to_logical::<f64>(m.scale_factor()).height * 0.85)
            .unwrap_or(768.0)
            .clamp(480.0, 1536.0);
        let attributes = attributes.with_inner_size(winit::dpi::LogicalSize::new(height * 4.0 / 3.0, height));
        // The icon embedded by build.rs as resource 1.
        #[cfg(windows)]
        let attributes = {
            use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
            let icon = winit::window::Icon::from_resource(1, None).ok();
            attributes.with_window_icon(icon.clone()).with_taskbar_icon(icon)
        };
        attributes
    }
}

/// The byte of a typed character in the game texts' code page (CP1252 for the Western
/// versions), as the original's `ToAscii` gives it; also `€`, `Œ`, `“` and the other
/// characters CP1252 places in 0x80..=0x9F.
fn cp1252(text: &str) -> Option<u8> {
    let mut chars = text.chars();
    let c = chars.next()?;
    if chars.next().is_some() || c.is_control() {
        return None;
    }
    match acr_formats::CodePage::Cp1252.encode(c.encode_utf8(&mut [0; 4]))?.as_slice() {
        &[b] => Some(b),
        _ => None,
    }
}

fn event_loop() -> Result<EventLoop<Event>, String> {
    EventLoop::<Event>::with_user_event().build().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::cp1252;

    #[test]
    fn typed_characters_map_to_cp1252() {
        assert_eq!(cp1252("a"), Some(b'a'));
        assert_eq!(cp1252("ä"), Some(0xe4));
        assert_eq!(cp1252("ß"), Some(0xdf));
        assert_eq!(cp1252("€"), Some(0x80));
        assert_eq!(cp1252("Ж"), None);
        assert_eq!(cp1252("\u{8}"), None);
        assert_eq!(cp1252("ab"), None);
    }
}
