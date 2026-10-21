//! Sprite viewer: browse and animate every `.GP` sprite pack of an original installation.
//!
//! Controls: Left/Right file, PageUp/PageDown 10 files, Up/Down frame, Space play/pause,
//! G grid, +/- zoom, 1-8 player colour.
//!
//! Runs natively (`cargo run -p acr-viewer -- <game-dir>`) and in the browser
//! (`cargo xtask serve`), where the player picks their game folder.

use std::sync::Arc;

use acr_formats::{Frame, GpFile, Palette, normalize_path};
use acr_render::{SpriteAtlas, SpriteDraw, SpriteRenderer};
use acr_sim::FixedClock;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::run;

const PALETTE_PATH: &str = "0\\AGEW_1.PAL";
const ANIMATION_FPS: u32 = 15;
const PREFERRED_START: &str = "ANMBIZC.GP";

/// Reads game files; implemented per platform.
pub trait Assets {
    /// All `.GP` paths, sorted.
    fn sprite_paths(&self) -> Vec<String>;
    /// Starts reading `path`; the result arrives as [`UserEvent::Loaded`].
    fn request(&self, path: &str, proxy: &EventLoopProxy<UserEvent>);
}

pub enum UserEvent {
    GpuReady(Box<Gpu>),
    Loaded { path: String, result: Result<Vec<u8>, String> },
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: SpriteRenderer,
}

impl Gpu {
    async fn new(window: Arc<Window>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("acr-viewer"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface not supported by adapter")?;
        surface.configure(&device, &config);
        let renderer = SpriteRenderer::new(&device, config.format);
        Ok(Self { surface, device, queue, config, renderer })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }
}

struct LoadedSprite {
    path: String,
    frames: Vec<Frame>,
    failed: usize,
    /// Union of all frame bounds, so animations do not jitter.
    bounds: (i32, i32, i32, i32),
    atlas: Option<SpriteAtlas>,
}

pub struct App {
    proxy: EventLoopProxy<UserEvent>,
    assets: Box<dyn Assets>,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    palette: Option<Palette>,
    files: Vec<String>,
    current: usize,
    sprite: Option<LoadedSprite>,
    frame: usize,
    playing: bool,
    grid: bool,
    scale: f32,
    player: u8,
    clock: FixedClock,
    last: Instant,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>, assets: Box<dyn Assets>, start: Option<&str>) -> Self {
        let files = assets.sprite_paths();
        let want = normalize_path(start.unwrap_or(PREFERRED_START));
        let current = files.iter().position(|f| normalize_path(f).ends_with(&want)).unwrap_or(0);
        Self {
            proxy,
            assets,
            window: None,
            gpu: None,
            palette: None,
            files,
            current,
            sprite: None,
            frame: 0,
            playing: true,
            grid: false,
            scale: 2.0,
            player: 0,
            clock: FixedClock::new(ANIMATION_FPS, 4),
            last: Instant::now(),
        }
    }

    fn open_current(&mut self) {
        if let Some(path) = self.files.get(self.current) {
            self.assets.request(path, &self.proxy);
        }
    }

    fn step_file(&mut self, delta: isize) {
        if self.files.is_empty() {
            return;
        }
        self.current = (self.current as isize + delta).rem_euclid(self.files.len() as isize) as usize;
        self.open_current();
    }

    fn on_loaded(&mut self, path: String, result: Result<Vec<u8>, String>) {
        let bytes = match result {
            Ok(b) => b,
            Err(e) => return log::error!("{path}: {e}"),
        };
        if path == PALETTE_PATH {
            self.palette = Palette::parse(&bytes);
            self.sync_gpu();
            return;
        }
        // Ignore stale results when the user skipped ahead.
        if self.files.get(self.current) != Some(&path) {
            return;
        }
        let file = match GpFile::parse(bytes) {
            Ok(f) => f,
            Err(e) => return log::error!("{path}: {e}"),
        };
        let mut failed = 0;
        let frames: Vec<Frame> = (0..file.frame_count())
            .map(|i| {
                file.decode_frame(i).unwrap_or_else(|_| {
                    failed += 1;
                    Frame::default()
                })
            })
            .collect();
        let bounds = frames
            .iter()
            .filter_map(Frame::bounds)
            .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
            .unwrap_or((0, 0, 1, 1));
        self.sprite = Some(LoadedSprite { path, frames, failed, bounds, atlas: None });
        self.frame = 0;
        self.fit_scale();
        self.sync_gpu();
        self.update_title();
    }

    /// Picks the largest integer zoom (up to 4) at which the sprite fits the window.
    fn fit_scale(&mut self) {
        let (Some(sprite), Some(window)) = (&self.sprite, &self.window) else { return };
        let size = window.inner_size();
        let (x0, y0, x1, y1) = sprite.bounds;
        let fit = (size.width as f32 / (x1 - x0) as f32).min(size.height as f32 / (y1 - y0) as f32);
        self.scale = fit.floor().clamp(1.0, 4.0);
    }

    /// Pushes palette and sprite data to the GPU once both exist.
    fn sync_gpu(&mut self) {
        let Some(gpu) = &self.gpu else { return };
        if let Some(palette) = &self.palette {
            gpu.renderer.set_palette(&gpu.queue, palette);
        }
        if let Some(sprite) = &mut self.sprite
            && sprite.atlas.is_none()
        {
            sprite.atlas = Some(gpu.renderer.upload(&gpu.device, &gpu.queue, &sprite.frames));
        }
    }

    fn update_title(&self) {
        let Some(window) = &self.window else { return };
        let title = match &self.sprite {
            Some(s) => format!(
                "{} [{}/{}]  frame {}/{}{}  x{}  player {}",
                s.path,
                self.current + 1,
                self.files.len(),
                self.frame + 1,
                s.frames.len(),
                if s.failed > 0 { format!(" ({} undecodable)", s.failed) } else { String::new() },
                self.scale,
                self.player + 1,
            ),
            None => "ACR sprite viewer - loading".to_owned(),
        };
        window.set_title(&title);
        // winit's web backend does not touch the document title.
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            document.set_title(&title);
        }
    }

    fn on_key(&mut self, key: &Key) {
        let frames = self.sprite.as_ref().map_or(0, |s| s.frames.len()).max(1);
        match key {
            Key::Named(NamedKey::ArrowRight) => self.step_file(1),
            Key::Named(NamedKey::ArrowLeft) => self.step_file(-1),
            Key::Named(NamedKey::PageDown) => self.step_file(10),
            Key::Named(NamedKey::PageUp) => self.step_file(-10),
            Key::Named(NamedKey::ArrowDown) => self.frame = (self.frame + 1) % frames,
            Key::Named(NamedKey::ArrowUp) => self.frame = (self.frame + frames - 1) % frames,
            Key::Named(NamedKey::Space) => self.playing = !self.playing,
            Key::Character(c) => match c.as_str() {
                "g" | "G" => self.grid = !self.grid,
                "+" | "=" => self.scale = (self.scale + 1.0).min(8.0),
                "-" => self.scale = (self.scale - 1.0).max(1.0),
                d if d.len() == 1 && ("1"..="8").contains(&d) => self.player = d.as_bytes()[0] - b'1',
                _ => {}
            },
            _ => {}
        }
        self.update_title();
    }

    fn draws(&self, width: f32, height: f32) -> Vec<SpriteDraw> {
        let Some(sprite) = &self.sprite else { return Vec::new() };
        let (x0, y0, x1, y1) = sprite.bounds;
        let (cell_w, cell_h) = ((x1 - x0) as f32 * self.scale, (y1 - y0) as f32 * self.scale);
        let draw = |frame, left: f32, top: f32| SpriteDraw {
            frame,
            x: left - x0 as f32 * self.scale,
            y: top - y0 as f32 * self.scale,
            scale: self.scale,
            player: self.player,
        };
        if !self.grid {
            return vec![draw(self.frame, ((width - cell_w) / 2.0).floor(), ((height - cell_h) / 2.0).floor())];
        }
        let cols = ((width / cell_w) as usize).max(1);
        let rows = ((height / cell_h) as usize).max(1);
        (self.frame..sprite.frames.len())
            .take(cols * rows)
            .enumerate()
            .map(|(i, f)| draw(f, (i % cols) as f32 * cell_w, (i / cols) as f32 * cell_h))
            .collect()
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let ticks = self.clock.advance((now - self.last).as_nanos() as u64);
        self.last = now;
        if self.playing
            && !self.grid
            && let Some(s) = &self.sprite
        {
            self.frame = (self.frame + ticks as usize) % s.frames.len().max(1);
            if ticks > 0 {
                self.update_title();
            }
        }

        let Some(gpu) = &mut self.gpu else { return };
        let screen = [gpu.config.width as f32, gpu.config.height as f32];
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            _ => return,
        };
        let draws = self.draws(screen[0], screen[1]);
        let Some(gpu) = &mut self.gpu else { return };
        let view = frame.texture.create_view(&Default::default());
        let atlas = self.sprite.as_ref().and_then(|s| s.atlas.as_ref());
        if let Some(atlas) = atlas {
            gpu.renderer.prepare(&gpu.device, &gpu.queue, atlas, screen, &draws);
        }
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
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
                gpu.renderer.render(&mut pass, atlas);
            }
        }
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(frame);
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes().with_title("ACR sprite viewer");
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
        let init = async move {
            match Gpu::new(window).await {
                Ok(gpu) => {
                    let _ = proxy.send_event(UserEvent::GpuReady(Box::new(gpu)));
                }
                Err(e) => log::error!("GPU initialisation failed: {e}"),
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(init);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(init);

        self.assets.request(PALETTE_PATH, &self.proxy);
        self.open_current();
        self.update_title();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GpuReady(mut gpu) => {
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    gpu.resize(size.width, size.height);
                }
                self.gpu = Some(*gpu);
                self.sync_gpu();
            }
            UserEvent::Loaded { path, result } => self.on_loaded(path, result),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
                self.fit_scale();
                self.update_title();
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key, state: ElementState::Pressed, .. }, ..
            } => self.on_key(&logical_key),
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
fn event_loop() -> Result<EventLoop<UserEvent>, String> {
    EventLoop::<UserEvent>::with_user_event().build().map_err(|e| e.to_string())
}
