//! American Conquest, rebuilt: the game application.
//!
//! The screen is laid out on the original 1024x768 canvas and scaled to the window with
//! letterboxing, so menus look like the original at any resolution.

mod canvas;
mod ingame;
mod intro;
mod music;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod ui;
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
        if matches!(self.mode, Mode::Menus) {
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
        let Some(g) = &mut self.graphics else { return };
        if let Mode::InGame(game) = &mut self.mode {
            game.sync_gpu(&g.gpu.device, &g.gpu.queue, g.gpu.config.format);
            game.update(dt as f32, g.gpu.size());
        }
        let canvas = Canvas::fit(g.gpu.size());
        let draws: Vec<ImageDraw> = match &self.mode {
            Mode::Intro => {
                self.intro.upload(&g.gpu.device, &g.gpu.queue, &g.images);
                self.intro.draws(&canvas)
            }
            Mode::InGame(game) if game.ready() => Vec::new(),
            Mode::Menus | Mode::InGame(_) => self.ui.draws(&g.gpu.device, &g.gpu.queue, &g.images, &canvas),
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
            ui::Action::Settings(settings) => {
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
                let mut game = ingame::InGame::new(setup);
                game.start(&self.assets.paths());
                self.mode = Mode::InGame(Box::new(game));
                self.load_requested();
            }
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

    /// Forwards input to the menus.
    fn ui_input(&mut self, input: ui::Input, event_loop: &ActiveEventLoop) {
        if matches!(self.mode, Mode::Menus) {
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
        let mut game = ingame::InGame::new(Box::new(setup));
        game.start(&self.assets.paths());
        self.mode = Mode::InGame(Box::new(game));
        self.load_requested();
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
                if let Mode::InGame(game) = &mut self.mode
                    && game.on_loaded(&path, &result)
                {
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
                    game.on_cursor(Some([position.x as f32, position.y as f32]));
                    if let Some(window) = &self.window {
                        // The original in-game cursor follows with the game panel.
                        window.set_cursor_visible(game.ready());
                    }
                }
                if let Some(p) = self.virtual_cursor() {
                    self.ui_input(ui::Input::MouseMove(p), event_loop);
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } if matches!(self.mode, Mode::Intro) => {
                self.intro_input()
            }
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key: Key::Named(named), state, .. }, .. }
                if matches!(self.mode, Mode::InGame(_))
                    && !matches!(named, NamedKey::F11)
                    && !(named == NamedKey::Enter && self.alt_down()) =>
            {
                let pressed = state == ElementState::Pressed;
                if named == NamedKey::Escape && pressed {
                    // The original opens its in-game menu here; until that exists, leave.
                    return self.leave_game();
                }
                if let Mode::InGame(game) = &mut self.mode {
                    game.on_key(named, pressed);
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
            WindowEvent::MouseInput { state, button, .. } if matches!(&self.mode, Mode::InGame(g) if g.ready()) => {
                if let Mode::InGame(game) = &mut self.mode {
                    match (button, state) {
                        (MouseButton::Left, _) => game.on_left(state == ElementState::Pressed),
                        (MouseButton::Right, ElementState::Pressed) => game.on_right(),
                        _ => {}
                    }
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
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
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
