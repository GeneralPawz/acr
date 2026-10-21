//! American Conquest, rebuilt: the game application.
//!
//! The screen is laid out on the original 1024x768 canvas and scaled to the window with
//! letterboxing, so menus look like the original at any resolution.

mod canvas;
mod intro;
mod menu;
mod music;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use std::sync::Arc;

use acr_app::{Assets, Gpu};
use acr_render::{ImageDraw, ImageRenderer, SpriteRenderer};
use canvas::Canvas;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowId};

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

/// What a screen wants the app to do after handling input.
pub(crate) enum Action {
    None,
    /// Play the startup movies again.
    Intro,
    Quit,
}

pub struct Game {
    proxy: EventLoopProxy<Event>,
    assets: Box<dyn Assets>,
    window: Option<Arc<Window>>,
    graphics: Option<Graphics>,
    intro: intro::Intro,
    menu: menu::MainMenu,
    music: music::Playlist,
    cursor: PhysicalPosition<f64>,
    modifiers: ModifiersState,
}

impl Game {
    pub fn new(proxy: EventLoopProxy<Event>, assets: Box<dyn Assets>) -> Self {
        let paths = assets.paths();
        let music = music::Playlist::new(&paths);
        Self {
            proxy,
            assets,
            window: None,
            graphics: None,
            intro: intro::Intro::new(&paths),
            menu: menu::MainMenu::default(),
            music,
            cursor: Default::default(),
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

    fn sync_textures(&mut self) {
        if let Some(g) = &self.graphics {
            self.menu.upload(&g.gpu.device, &g.gpu.queue, &g.images);
        }
    }

    fn redraw(&mut self) {
        if self.intro.active() {
            if let Some(path) = self.intro.next_request() {
                self.load(&path);
            }
            if self.intro.update() {
                self.music.set_music(None);
            }
        } else if let Some(track) = self.music.next_track() {
            self.load(&track);
        }
        let Some(g) = &mut self.graphics else { return };
        let canvas = Canvas::fit(g.gpu.size());
        let draws: Vec<ImageDraw> = if self.intro.active() {
            self.intro.upload(&g.gpu.device, &g.gpu.queue, &g.images);
            self.intro.draws(&canvas)
        } else {
            self.menu.draws(&canvas)
        };
        let Some(frame) = g.gpu.next_frame() else { return };
        g.images.prepare(&g.gpu.device, &g.gpu.queue, g.gpu.size(), &draws);
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("game"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
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

    fn apply(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        match action {
            Action::None => {}
            Action::Intro => {
                self.intro = intro::Intro::new(&self.assets.paths());
                self.music.set_music(None);
            }
            Action::Quit => {
                self.music.stop();
                event_loop.exit();
            }
        }
    }

    /// Click or key during the movies: skip the current one. In the browser the first
    /// gesture only enables audio and restarts the movie with sound.
    fn intro_input(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if !self.music.started() {
            self.music.start();
            let audio = self.intro.restart();
            self.music.set_music(audio);
            return;
        }
        self.intro.skip();
        self.music.set_music(None);
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
        for path in menu::MainMenu::assets() {
            self.load(path);
        }
        // Browsers only allow audio after a user gesture; native starts right away.
        #[cfg(not(target_arch = "wasm32"))]
        self.music.start();
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
                self.sync_textures();
            }
            Event::Loaded { path, result } => match result {
                Ok(bytes) => {
                    let (movie, audio) = self.intro.on_loaded(&path, &bytes);
                    if movie {
                        self.music.set_music(audio);
                    } else if !self.music.on_loaded(&path, &bytes) {
                        self.menu.on_loaded(&path, bytes);
                        self.sync_textures();
                    }
                }
                Err(e) => log::error!("{path}: {e}"),
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.apply(Action::Quit, event_loop),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.graphics {
                    g.gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let hover = self.virtual_cursor().is_some_and(|p| self.menu.hit(p).is_some());
                if let Some(window) = &self.window {
                    window.set_cursor(if hover { CursorIcon::Pointer } else { CursorIcon::Default });
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } if self.intro.active() => self.intro_input(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape | NamedKey::Space | NamedKey::Enter),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } if self.intro.active() && !self.alt_down() => self.intro_input(),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                #[cfg(target_arch = "wasm32")]
                self.music.start();
                if let Some(p) = self.virtual_cursor() {
                    let action = self.menu.click(p);
                    self.apply(action, event_loop);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key, state: ElementState::Pressed, .. }, ..
            } => {
                #[cfg(target_arch = "wasm32")]
                self.music.start();
                match logical_key {
                    Key::Named(NamedKey::F11) => self.toggle_fullscreen(),
                    Key::Named(NamedKey::Enter) if self.alt_down() => self.toggle_fullscreen(),
                    _ => {}
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::RedrawRequested => self.redraw(),
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

fn event_loop() -> Result<EventLoop<Event>, String> {
    EventLoop::<Event>::with_user_event().build().map_err(|e| e.to_string())
}
