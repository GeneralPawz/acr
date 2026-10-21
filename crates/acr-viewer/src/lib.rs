//! Viewer for the original game data: sprites and maps.
//!
//! Tab switches between the sprite mode ([`sprites`]) and the map mode ([`map`]); see
//! their docs for the controls.
//!
//! Runs natively (`cargo run -p acr-viewer -- <game-dir> [sprite.gp | --map NAME]`) and in
//! the browser (`cargo xtask serve --app viewer`), where the player picks their game
//! folder.

mod map;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod sprites;
#[cfg(target_arch = "wasm32")]
mod web;

use std::sync::Arc;

use acr_app::{Assets, Gpu};
use acr_render::SpriteRenderer;
use map::MapView;
use sprites::SpriteView;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[cfg(not(target_arch = "wasm32"))]
pub use native::run;

/// Requests a game file; the result arrives as [`Event::Loaded`].
pub(crate) type Loader<'a> = dyn Fn(&str) + 'a;

pub enum Event {
    GpuReady(Box<Gpu>),
    Loaded { path: String, result: Result<Vec<u8>, String> },
}

/// What to show first.
#[derive(Debug, Clone, Default)]
pub struct Start {
    /// A sprite pack to open, e.g. `ANMBIZC.GP`.
    pub sprite: Option<String>,
    /// A map to open (name or path, with or without `.m3d`); starts in map mode.
    pub map: Option<String>,
    /// World position to centre the map on, in pixels.
    pub at: Option<[f32; 2]>,
    /// Map zoom, output pixels per original pixel.
    pub zoom: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Sprites,
    Map,
}

struct Graphics {
    gpu: Gpu,
    sprites: SpriteRenderer,
}

pub struct App {
    proxy: EventLoopProxy<Event>,
    assets: Box<dyn Assets>,
    window: Option<Arc<Window>>,
    graphics: Option<Graphics>,
    mode: Mode,
    sprites: SpriteView,
    map: MapView,
    map_started: bool,
    last: Instant,
}

impl App {
    pub fn new(proxy: EventLoopProxy<Event>, assets: Box<dyn Assets>, start: Start) -> Self {
        let paths = assets.paths();
        let mode = if start.map.is_some() { Mode::Map } else { Mode::Sprites };
        Self {
            proxy,
            sprites: SpriteView::new(&paths, start.sprite.as_deref()),
            map: MapView::new(&paths, &start),
            assets,
            window: None,
            graphics: None,
            mode,
            map_started: false,
            last: Instant::now(),
        }
    }

    fn window_size(&self) -> [f32; 2] {
        self.window.as_ref().map_or([1.0, 1.0], |w| {
            let s = w.inner_size();
            [s.width.max(1) as f32, s.height.max(1) as f32]
        })
    }

    /// Splits `self` into a file loader and the views, so both can be used at once.
    fn parts(&mut self) -> (impl Fn(&str) + '_, &mut SpriteView, &mut MapView) {
        let (assets, proxy) = (&self.assets, &self.proxy);
        let load = move |path: &str| {
            let proxy = proxy.clone();
            let owned = path.to_owned();
            assets.request(
                path,
                Box::new(move |result| {
                    let _ = proxy.send_event(Event::Loaded { path: owned, result });
                }),
            );
        };
        (load, &mut self.sprites, &mut self.map)
    }

    fn enter(&mut self, mode: Mode) {
        self.mode = mode;
        if mode == Mode::Map && !self.map_started {
            self.map_started = true;
            let (load, _, map) = self.parts();
            map.start(&load);
        }
        self.update_title();
    }

    fn on_loaded(&mut self, path: String, result: Result<Vec<u8>, String>) {
        let size = self.window_size();
        {
            let (load, sprites, map) = self.parts();
            match result {
                // Both views see every file: the palettes are shared.
                Ok(bytes) => {
                    sprites.on_loaded(&path, &bytes, size);
                    map.on_loaded(&path, &bytes, size, &load);
                }
                Err(e) => {
                    log::error!("{path}: {e}");
                    map.on_failed(&path);
                }
            }
        }
        self.sync_gpu();
        self.update_title();
    }

    fn sync_gpu(&mut self) {
        let Some(g) = &self.graphics else { return };
        self.sprites.sync_gpu(&g.gpu.device, &g.gpu.queue, &g.sprites);
        self.map.sync_gpu(&g.gpu.device, &g.gpu.queue, g.gpu.config.format);
    }

    fn update_title(&self) {
        let Some(window) = &self.window else { return };
        let title = match self.mode {
            Mode::Sprites => self.sprites.title(),
            Mode::Map => self.map.title(),
        };
        window.set_title(&title);
        // winit's web backend does not touch the document title.
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            document.set_title(&title);
        }
    }

    fn on_key(&mut self, key: &Key, pressed: bool) {
        if pressed && *key == Key::Named(NamedKey::Tab) {
            return self.enter(if self.mode == Mode::Map { Mode::Sprites } else { Mode::Map });
        }
        let size = self.window_size();
        let mode = self.mode;
        {
            let (load, sprites, map) = self.parts();
            match mode {
                Mode::Sprites if pressed => sprites.on_key(key, &load),
                Mode::Sprites => {}
                Mode::Map => map.on_key(key, pressed, &load, size),
            }
        }
        self.update_title();
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let elapsed = now - self.last;
        self.last = now;
        let size = self.window_size();
        match self.mode {
            Mode::Sprites => {
                if self.sprites.tick(elapsed.as_nanos() as u64) {
                    self.update_title();
                }
            }
            Mode::Map => {
                self.map.update(elapsed.as_secs_f32(), size);
                self.update_title();
            }
        }

        let Some(g) = &mut self.graphics else { return };
        let Some(frame) = g.gpu.next_frame() else { return };
        let screen = g.gpu.size();
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
        match self.mode {
            Mode::Sprites => {
                let draws = self.sprites.draws(screen[0], screen[1]);
                let atlas = self.sprites.atlas();
                if let Some(atlas) = atlas {
                    g.sprites.prepare(&g.gpu.device, &g.gpu.queue, atlas, screen, &draws);
                }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("viewer"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.16, g: 0.22, b: 0.12, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                if let Some(atlas) = atlas {
                    g.sprites.render(&mut pass, atlas);
                }
            }
            Mode::Map => self.map.render(&g.gpu.device, &g.gpu.queue, &mut encoder, &view, screen),
        }
        g.gpu.queue.submit([encoder.finish()]);
        g.gpu.queue.present(frame);
    }

    fn map_anchor(&self) -> [f32; 2] {
        let [w, h] = self.window_size();
        self.map.cursor().unwrap_or([w / 2.0, h / 2.0])
    }
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes().with_title("ACR viewer");
        #[cfg(not(target_arch = "wasm32"))]
        let attributes = attributes.with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        // In the browser the page's CSS sizes the canvas; winit reports it via `Resized`.
        #[cfg(target_arch = "wasm32")]
        let attributes = winit::platform::web::WindowAttributesExtWebSys::with_append(attributes, true);
        let window = match event_loop.create_window(attributes) {
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
        {
            let (load, sprites, _) = self.parts();
            sprites.start(&load);
        }
        self.enter(self.mode);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::GpuReady(gpu) => {
                let mut gpu = *gpu;
                let [w, h] = self.window_size();
                gpu.resize(w as u32, h as u32);
                let sprites = SpriteRenderer::new(&gpu.device, gpu.config.format);
                self.graphics = Some(Graphics { gpu, sprites });
                self.sync_gpu();
            }
            Event::Loaded { path, result } => self.on_loaded(path, result),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.graphics {
                    g.gpu.resize(size.width, size.height);
                }
                self.sprites.fit_scale(self.window_size());
                self.update_title();
            }
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key, state, .. }, .. } => {
                self.on_key(&logical_key, state == ElementState::Pressed)
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.map.on_cursor(Some([position.x as f32, position.y as f32]))
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => self.map.on_cursor(None),
            WindowEvent::MouseInput { state, button: MouseButton::Right | MouseButton::Middle, .. } => {
                self.map.on_drag(state == ElementState::Pressed)
            }
            WindowEvent::MouseWheel { delta, .. } if self.mode == Mode::Map => {
                let up = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                let at = self.map_anchor();
                self.map.zoom_by(if up > 0.0 { 1 } else { -1 }, at);
            }
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

/// Builds the event loop shared by both platforms.
fn event_loop() -> Result<EventLoop<Event>, String> {
    EventLoop::<Event>::with_user_event().build().map_err(|e| e.to_string())
}
