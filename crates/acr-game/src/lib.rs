//! American Conquest, rebuilt: the game application.
//!
//! The menus are laid out on the original 1024x768 canvas and scaled to the window with
//! letterboxing, so they look like the original at any resolution. The in-game interface
//! uses the same scale over the whole window, laid out for its size like the original's
//! `RealLx` × `RealLy` (see `docs/ui.md`, screen size).

mod acr_missions;
mod agony;
#[cfg(target_os = "android")]
mod android;
mod animal_sounds;
mod blank_map;
mod canvas;
mod crowd_sounds;
mod diplomacy;
pub mod drive;
mod hit_sounds;
mod hud;
#[cfg(not(target_arch = "wasm32"))]
mod hw_cursor;
mod ingame;
mod input_mode;
mod intro;
mod mission;
mod mission_ui;
mod music;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub mod native;
mod pacing;
mod perf;
mod random_map;
mod savegame;
mod suspend;
pub mod tactics;
mod touch;
pub mod ui;
mod unit_sounds;
mod weather_sound;
#[cfg(target_arch = "wasm32")]
mod web;
pub mod webui;

use std::sync::Arc;

use acr_app::{Assets, Gpu};
use acr_render::upscale::{RenderScale, Upscaler};
use acr_render::{ImageDraw, ImageRenderer, SpriteRenderer};
use canvas::Canvas;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

pub enum Event {
    GpuReady(Box<Gpu>),
    Loaded {
        path: String,
        result: Result<Vec<u8>, String>,
    },
    /// A message from the Svelte UI (JSON, `acr-ui-protocol`).
    Ui(String),
    /// A finger the Svelte UI's host did not leave to the page (Android, `webui::android`): it
    /// takes the path of the window's touches. `x`, `y` are window pixels.
    UiTouch {
        id: u64,
        phase: winit::event::TouchPhase,
        x: f64,
        y: f64,
    },
    /// The Svelte UI asks for an asset (`webui::assets`).
    UiAsset {
        path: String,
        reply: webui::AssetReply,
    },
    /// The page's user gesture: the audio may start now.
    AudioUnlock,
}

/// GPU resources that exist once the window has a surface.
struct Graphics {
    gpu: Gpu,
    images: ImageRenderer,
    #[allow(dead_code)] // used by the in-game screens that follow
    sprites: SpriteRenderer,
    /// The offscreen scene and its stretch to the window, below 100 % render scale.
    upscaler: Upscaler,
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
    /// The Svelte UI, once it said `Ready` (docs/webui.md).
    webui: Option<webui::WebUi>,
    /// Where the driver collects the UI's messages (`--ui classic`), in place of a page.
    ui_outbox: Option<webui::Outbox>,
    /// `native_ui=svelte` is in effect: the window gets the webview host once the GPU is ready.
    #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
    native_ui: bool,
    /// The webview that shows the Svelte UI: the opaque full-screen menus on Linux
    /// (`webui::native`), the transparent popup on Android (`webui::android`).
    #[cfg(any(all(feature = "webview-gtk", target_os = "linux"), target_os = "android"))]
    native_host: Option<Box<dyn webui::NativeHost>>,
    mode: Mode,
    audio: music::Audio,
    suspension: suspend::Suspension,
    unit_sounds: unit_sounds::UnitSounds,
    weather_sound: weather_sound::WeatherSound,
    intro: intro::Intro,
    cursor: PhysicalPosition<f64>,
    /// The fingers on the screen and their gesture ([`Game::touch`]).
    touches: touch::Touches,
    /// Touch or mouse, by the last input used ([`input_mode::InputMode`]).
    input_mode: input_mode::InputMode,
    /// The clock of the touch gestures (a long press).
    touch_clock: web_time::Instant,
    /// The test driver's own touch time, which replaces the clock while it feeds fingers.
    touch_time: Option<std::time::Duration>,
    last_frame: Option<web_time::Instant>,
    /// `ACR_PROFILE`: frames since the last profile line and when it was written.
    profile: Option<(web_time::Instant, u32)>,
    /// The always-on performance recorder (`perf.rs`, docs/testing.md).
    perf: perf::Recorder,
    /// When the next frame is due (`pacing.rs`, #440).
    pacing: pacing::Pacing,
    /// Where the work per frame is reported (Android's performance hints, #440).
    work_sink: Option<Box<dyn pacing::WorkSink>>,
    work: pacing::Work,
    /// The time the last frame waited for the swapchain, which is not work.
    acquire_wait: std::time::Duration,
    /// A hash of the interface's draws of the last frame, for render on demand.
    draws_signature: u64,
    modifiers: ModifiersState,
    /// The options came from the settings file: the window takes their resolution.
    stored_settings: bool,
    /// The interface's draw layers from the last [`Game::advance`], for [`Game::render`].
    layers: Vec<(Canvas, Vec<ui::draw::Draw>)>,
    /// The window size of the test driver without a GPU: no window, nothing drawn.
    headless: Option<[f32; 2]>,
    /// Files requested from the platform and not delivered yet.
    in_flight: std::rc::Rc<std::cell::Cell<usize>>,
    /// The test driver (`--drive`): commands on stdin, answers on stdout.
    #[cfg(not(target_arch = "wasm32"))]
    driver: Option<Box<drive::Driver>>,
    /// The cursor as the system's cursor (native, not under the test driver).
    #[cfg(not(target_arch = "wasm32"))]
    hw_cursor: hw_cursor::HardwareCursor,
}

impl Game {
    pub fn new(proxy: EventLoopProxy<Event>, assets: Box<dyn Assets>) -> Self {
        let paths = assets.paths();
        Self {
            proxy,
            ui: ui::Ui::new(&paths),
            webui: None,
            ui_outbox: None,
            #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
            native_ui: false,
            #[cfg(any(all(feature = "webview-gtk", target_os = "linux"), target_os = "android"))]
            native_host: None,
            mode: Mode::Intro,
            audio: music::Audio::new(&paths),
            suspension: Default::default(),
            unit_sounds: unit_sounds::UnitSounds::new(),
            weather_sound: Default::default(),
            intro: intro::Intro::new(&paths),
            assets,
            window: None,
            graphics: None,
            cursor: Default::default(),
            touches: touch::Touches::default(),
            input_mode: input_mode::InputMode::initial(),
            touch_clock: web_time::Instant::now(),
            touch_time: None,
            last_frame: None,
            pacing: Default::default(),
            work_sink: None,
            work: Default::default(),
            acquire_wait: Default::default(),
            draws_signature: 0,
            profile: None,
            perf: perf::Recorder::from_env(),
            modifiers: ModifiersState::empty(),
            stored_settings: false,
            layers: Vec::new(),
            headless: None,
            in_flight: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            driver: None,
            #[cfg(not(target_arch = "wasm32"))]
            hw_cursor: hw_cursor::HardwareCursor::new(),
        }
    }

    fn load(&self, path: &str) {
        let proxy = self.proxy.clone();
        let owned = path.to_owned();
        self.in_flight.set(self.in_flight.get() + 1);
        self.assets.request(
            path,
            Box::new(move |result| {
                let _ = proxy.send_event(Event::Loaded { path: owned, result });
            }),
        );
    }

    /// The `native_ui=svelte` setting is in effect ([`webui::native_ui_active`]): the window gets
    /// the webview that shows the Svelte UI's full-screen menus.
    #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
    pub fn enable_native_ui(&mut self) {
        self.native_ui = true;
    }

    /// The Android WebView host of `native_ui=svelte` (`webui::android::Host`), created before
    /// the window exists.
    #[cfg(target_os = "android")]
    pub fn set_native_host(&mut self, host: Box<dyn webui::NativeHost>) {
        self.native_host = Some(host);
    }

    /// The app is in the foreground (or not): the native webview does no work while it is away, and
    /// the page gets everything again when it returns, as the messages sent meanwhile are dropped.
    fn set_native_host_active(&mut self, active: bool) {
        #[cfg(any(all(feature = "webview-gtk", target_os = "linux"), target_os = "android"))]
        if let Some(host) = &mut self.native_host {
            host.set_active(active);
            if active && let Some(web) = &mut self.webui {
                web.resync();
            }
        }
        #[cfg(not(any(all(feature = "webview-gtk", target_os = "linux"), target_os = "android")))]
        let _ = active;
    }

    /// Answers the Svelte UI's waiting asset requests and publishes what changed.
    fn publish_webui(&mut self) {
        let Some(mut web) = self.webui.take() else { return };
        web.pump_assets(&self.ui);
        let (mode, loading) = match &self.mode {
            Mode::Intro => (acr_ui_protocol::Mode::Intro, None),
            Mode::Menus => (acr_ui_protocol::Mode::Menus, None),
            Mode::InGame(g) if g.ready() => (acr_ui_protocol::Mode::Game, None),
            Mode::InGame(g) => (acr_ui_protocol::Mode::Loading, g.loading_progress()),
        };
        let size = self.frame_size();
        // What the engine left out of the interface this frame (nothing outside a running game).
        let hud = match &mut self.mode {
            Mode::InGame(g) if g.ready() => g.take_claimed(),
            _ => Vec::new(),
        };
        web.publish(&mut self.ui, mode, loading, size, hud);
        // An opaque webview covers the window exactly while the UI draws the screen; the movies and
        // the game are the engine's. A transparent one ignores this and stays shown.
        #[cfg(any(all(feature = "webview-gtk", target_os = "linux"), target_os = "android"))]
        if let Some(host) = &mut self.native_host {
            host.show(web.shown() && !matches!(self.mode, Mode::Intro));
        }
        self.webui = Some(web);
    }

    fn canvas(&self) -> Option<Canvas> {
        self.frame_size().map(Canvas::fit)
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
        let now = web_time::Instant::now();
        let dt = self.last_frame.map_or(0.0, |t| (now - t).as_secs_f64());
        self.last_frame = Some(now);
        #[cfg(target_arch = "wasm32")]
        web::serve_requests(self, event_loop);
        {
            acr_sim::prof_scope!("frame.advance");
            self.advance(dt, event_loop);
        }
        // Before drawing, so the frame leaves out a cursor the system now shows.
        #[cfg(not(target_arch = "wasm32"))]
        if let (Some(window), Some(size)) = (&self.window, self.frame_size()) {
            if cfg!(target_os = "android") && self.input_mode.draws_cursor() {
                // Android has no window cursors: the interface draws it for a mouse.
                self.ui.set_hardware_cursor(false);
            } else {
                self.hw_cursor.update(&mut self.ui, window, event_loop, Canvas::fit(size).scale);
            }
        }
        self.render();
        #[cfg(target_arch = "wasm32")]
        web::frame_done(now.elapsed().as_secs_f32() * 1000.0);
        self.profile_log(now);
        self.perf_frame();
        let settings = self.ui.settings();
        let fps = pacing::cap_fps(settings.frame_cap, settings.power_save, pacing::battery_mode());
        let on_demand = pacing::on_demand(settings.power_save, cfg!(target_os = "android"));
        let signature = if on_demand { self.look_signature() } else { None };
        self.pacing.observe(signature);
        self.pacing.frame_done(web_time::Instant::now(), fps, on_demand);
        self.report_work(now, fps, on_demand);
    }

    /// Tells the performance hints (`set_work_sink`) how long this frame's work took and what
    /// the frame budget is (the rate in effect, idle included).
    fn report_work(&mut self, started: web_time::Instant, fps: Option<u32>, on_demand: bool) {
        let Some(sink) = &mut self.work_sink else { return };
        let fps = self.pacing.rate(fps, on_demand);
        let refresh = self.window.as_ref().and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz());
        let work = started.elapsed().saturating_sub(std::mem::take(&mut self.acquire_wait));
        self.work.frame(sink.as_mut(), pacing::frame_budget(fps, refresh), work);
    }

    /// Reports the main thread's work per frame to `sink` from now on.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub fn set_work_sink(&mut self, sink: Box<dyn pacing::WorkSink>) {
        self.work_sink = Some(sink);
    }

    /// A hash of what the last frame showed (the interface's draws and the world's look), for
    /// render on demand (`pacing.rs`); `None` when it cannot be told or is in motion.
    fn look_signature(&self) -> Option<u64> {
        // The Svelte UI is drawn elsewhere; the movies move.
        if self.webui.is_some() || matches!(self.mode, Mode::Intro) || self.driver_active() {
            return None;
        }
        let world = match &self.mode {
            Mode::InGame(game) => game.look_signature(game.paused() || self.ui.dialog_open())?,
            _ => 0,
        };
        Some(self.draws_signature ^ world.rotate_left(17))
    }

    fn driver_active(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.driver.is_some();
        #[cfg(target_arch = "wasm32")]
        false
    }

    /// Records the frame just made (`perf.rs`), and writes a snapshot when Ctrl + Space, R
    /// asked for one or the frames stayed slow.
    fn perf_frame(&mut self) {
        if !self.perf.on() {
            return;
        }
        let mut counts = perf::Counts::default();
        if let Some([w, h]) = self.frame_size() {
            counts.window = [w as u16, h as u16];
        }
        let mut asked = false;
        if let Mode::InGame(game) = &mut self.mode {
            game.perf_counts(&mut counts);
            asked = std::mem::take(&mut game.perf_request);
            if self.perf.wants_memory() {
                let textures = game.texture_stats().map(|t| t.loaded_bytes + t.streamed_bytes);
                self.perf.memory(None, textures);
            }
        }
        let device = self.graphics.as_ref().map(|g| &g.gpu.device);
        let due = self.perf.frame(&counts, device);
        if asked {
            let _ = self.perf_snapshot(perf::Reason::Key);
        } else if let Some(reason) = due {
            let _ = self.perf_snapshot(reason);
        }
    }

    /// Writes a performance snapshot and shows the toast: its path, or why not.
    fn perf_snapshot(&mut self, reason: perf::Reason) -> Result<String, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let settings = self.ui.settings().encode();
            let gpu = self.graphics.as_ref().map(|g| &g.gpu);
            let ctx = perf::Context {
                settings: &settings,
                adapter: gpu.map(|g| &g.info),
                features: gpu.map(|g| g.device.features()),
                gpu_memory: gpu
                    .and_then(|g| g.device.generate_allocator_report())
                    .map(|r| (r.total_allocated_bytes, r.total_reserved_bytes)),
            };
            let result = self.perf.snapshot(reason, &ctx);
            match &result {
                Ok(_) => {
                    if let Mode::InGame(game) = &mut self.mode {
                        game.perf_hint(perf::SAVED);
                    }
                }
                Err(e) => log::warn!("perf: no snapshot: {e}"),
            }
            result.map(|p| p.display().to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = reason;
            Err("no snapshots in the browser".into())
        }
    }

    /// With `ACR_PROFILE` set (native), logs every 5 s where the frames' time went
    /// (`acr_sim::prof`, docs/rendering-3d.md).
    fn profile_log(&mut self, now: web_time::Instant) {
        if !acr_sim::prof::enabled() {
            return;
        }
        let (since, frames) = self.profile.get_or_insert((now, 0));
        *frames += 1;
        let seconds = (now - *since).as_secs_f64();
        if seconds >= 5.0 {
            let totals = acr_sim::prof::take();
            log::info!(
                "profile: {:.1} fps over {seconds:.1} s, per frame:\n{}",
                f64::from(*frames) / seconds,
                acr_sim::prof::report(&totals, *frames)
            );
            self.profile = Some((now, 0));
        }
    }

    /// The window size the game lays out for: the surface, or the test driver's virtual
    /// window when there is no GPU.
    fn frame_size(&self) -> Option<[f32; 2]> {
        match &self.graphics {
            Some(g) => Some(g.gpu.size()),
            None => self.headless,
        }
    }

    /// One frame of everything but drawing: the movies, the menus, the game and its
    /// interface, `dt` seconds after the last one. The interface's draw layers are kept for
    /// [`Game::render`].
    fn advance(&mut self, dt: f64, event_loop: &ActiveEventLoop) {
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
        if matches!(self.mode, Mode::Menus) || self.ui.dialog_open() {
            let action = self.ui.update(dt);
            self.apply(action, event_loop);
        }
        self.publish_webui();
        self.load_requested();
        self.play_sounds();
        self.audio.report();
        let weather = match (&self.mode, self.frame_size()) {
            (Mode::InGame(game), Some(size)) => game.weather_heard(size),
            _ => Default::default(),
        };
        self.weather_sound.update(weather, dt as f32, &mut self.audio);
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
        self.layers.clear();
        let Some(size) = self.frame_size() else { return };
        let mut layers = Vec::new();
        // Pixels per density-independent pixel, in render pixels.
        let density = self.window.as_ref().map_or(1.0, |w| w.scale_factor() as f32) * self.input_factor() as f32;
        if let Mode::InGame(game) = &mut self.mode {
            match &self.graphics {
                Some(g) => game.sync_gpu(&g.gpu.device, &g.gpu.queue, g.gpu.config.format),
                None => game.sync_headless(),
            }
            // The game stands still while a dialog (game menu, options) is open.
            let game_dt = if self.ui.dialog_open() { 0.0 } else { dt as f32 };
            game.update(game_dt, size);
            self.ui.set_loading_progress(game.loading_progress());
            self.unit_sounds.play(&game.take_sound_events(), &mut self.audio);
            let ticks = game.take_ambient_ticks();
            if ticks > 0
                && let Some(view) = game.ambient_view(ticks, size)
            {
                let ambient = self.unit_sounds.ambient(&view);
                self.unit_sounds.play(&ambient, &mut self.audio);
                self.unit_sounds.crowds(&view, &mut self.audio);
            }
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
                game.set_scale_factor(density);
                let scale = Canvas::fit(size).scale;
                game.set_hud_claim(self.webui.as_ref().map(|w| w.hud_claim(scale)).unwrap_or_default());
                let (l, images, cursor) = game.interface(&self.ui.ctx(), size, game_dt);
                layers = l;
                // The "Zielhilfe" icon's switch is kept in `settings.txt` (`aim_guides`).
                let guides = game.aim_guides();
                if guides != self.ui.settings().aim_guides {
                    self.ui.change_settings(|s| s.aim_guides = guides);
                }
                // The editor window's place is kept in `settings.txt` (`editor_window`).
                if let Some(place) = game.editor_window()
                    && place != self.ui.settings().editor_window
                {
                    self.ui.change_settings(|s| s.editor_window = place);
                }
                // Mission pages, hints and the end of the game over the interface.
                layers.push((game.screen_canvas(), game.engine_overlay(&self.ui.ctx())));
                for (name, image) in images {
                    if let Some(web) = &mut self.webui {
                        web.set_image(name, image.clone());
                    }
                    self.ui.set_image(name, image);
                }
                self.ui.set_game_cursor(cursor);
            }
        }
        self.layers = layers;
    }

    /// Draws the frame [`Game::advance`] prepared into the window.
    fn render(&mut self) {
        let Some(g) = &mut self.graphics else { return };
        let acquire = acr_sim::prof::Scope::new("frame.acquire");
        let waited = web_time::Instant::now();
        let frame = g.gpu.next_frame();
        self.acquire_wait = waited.elapsed();
        let Some(frame) = frame else { return };
        drop(acquire);
        let view = frame.texture.create_view(&Default::default());
        self.perf.gpu_begin(&g.gpu.device, &g.gpu.queue);
        {
            acr_sim::prof_scope!("frame.render_cpu");
            self.draw_frame(&view);
        }
        self.perf.gpu_end();
        if let Some(g) = &self.graphics {
            acr_sim::prof_scope!("frame.present");
            g.gpu.queue.present(frame);
        }
    }

    /// The render scale in effect: the setting, else 100 % (below it the view shows less of the
    /// world in sprite mode, so it stays an opt-in).
    fn render_scale(&self) -> RenderScale {
        self.ui.settings().render_scale.unwrap_or_default()
    }

    /// Makes the GPU's render size follow the render scale (the layout, the scene and the
    /// input follow the GPU's size).
    fn sync_render_scale(&mut self) {
        let scale = self.render_scale().factor();
        if let Some(g) = &mut self.graphics
            && g.gpu.render_scale() != scale
        {
            log::info!("render scale {scale} of {:?}", g.gpu.surface_size());
            g.gpu.set_render_scale(scale);
        }
    }

    /// Window pixels to render pixels: the factor the scene is drawn smaller by (1 at 100 %).
    fn input_factor(&self) -> f64 {
        self.graphics.as_ref().map_or(1.0, |g| f64::from(g.gpu.size()[0]) / f64::from(g.gpu.surface_size()[0]))
    }

    /// Draws the frame [`Game::advance`] prepared into `view`, a texture of the surface's
    /// size and format: the scene at the render scale, stretched over `view` below 100 %.
    fn draw_frame(&mut self, view: &wgpu::TextureView) {
        self.sync_render_scale();
        let scale = self.render_scale();
        let Some(g) = &mut self.graphics else { return };
        if g.gpu.render_scale() >= 1.0 {
            return self.draw_scene(view);
        }
        let size = g.gpu.size().map(|v| v as u32);
        let scene = g.upscaler.target(&g.gpu.device, size);
        self.draw_scene(&scene);
        let Some(g) = &mut self.graphics else { return };
        let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
        g.upscaler.blit(&mut encoder, view, scale.filter());
        g.gpu.queue.submit([encoder.finish()]);
    }

    /// Draws the frame into `view`, a texture of the render size ([`Gpu::size`]).
    fn draw_scene(&mut self, view: &wgpu::TextureView) {
        let Some(g) = &mut self.graphics else { return };
        let canvas = Canvas::fit(g.gpu.size());
        let (draws, world_draws): (Vec<ImageDraw>, usize) = match &self.mode {
            Mode::Intro => {
                self.intro.upload(&g.gpu.device, &g.gpu.queue, &g.images);
                (self.intro.draws(&canvas), 0)
            }
            Mode::Menus | Mode::InGame(_) => {
                self.ui.draws(&g.gpu.device, &g.gpu.queue, &g.images, &canvas, &self.layers)
            }
        };
        // The driver's `hud` off: the world's own draws only (no panel, minimap or cursor).
        let mut draws = draws;
        if let Mode::InGame(game) = &self.mode
            && game.hud_hidden
        {
            draws.truncate(world_draws);
        }
        self.draws_signature = draws_signature(&draws);
        g.images.prepare(&g.gpu.device, &g.gpu.queue, g.gpu.size(), &draws);
        let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
        acr_render::gpu_timer::mark_encoder(&mut encoder, "start");
        let mut load = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        let mut fog = None;
        if let Mode::InGame(game) = &mut self.mode
            && game.ready()
        {
            game.render(&g.gpu.device, &g.gpu.queue, &mut encoder, view, g.gpu.size());
            load = wgpu::LoadOp::Load;
            fog = Some(game);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("game"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // The fog of war covers the world and its overlays, not the interface (0x4bee1e).
            match fog {
                Some(game) => {
                    let split = world_draws as u32;
                    g.images.render_range(&mut pass, 0..split);
                    game.render_fog(&mut pass);
                    g.images.render_range(&mut pass, split..u32::MAX);
                }
                None => g.images.render(&mut pass),
            }
        }
        acr_render::gpu_timer::mark_encoder(&mut encoder, "interface");
        acr_render::gpu_timer::resolve(&mut encoder);
        g.gpu.queue.submit([encoder.finish()]);
    }

    fn apply(&mut self, action: ui::Action, event_loop: &ActiveEventLoop) {
        match action {
            ui::Action::None => {}
            ui::Action::Quit => {
                log::info!("quit");
                self.audio.stop();
                event_loop.exit();
            }
            ui::Action::LeaveGame => self.leave_game(),
            ui::Action::Settings(settings) => {
                if let Mode::InGame(game) = &mut self.mode {
                    game.apply_settings(&settings, self.input_mode);
                }
                let (effects, music) = settings.volumes();
                self.audio.set_volumes(effects, music);
                self.apply_resolution();
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
        let setup = ui::GameSetup {
            kind: ui::GameKind::Mission(mission),
            players: Vec::new(),
            local_player: 0,
            rules: Default::default(),
        };
        self.start_game(Box::new(setup));
    }

    fn start_game(&mut self, mut setup: Box<ui::GameSetup>) {
        // The setting `hit_locations=on` (#225, #254) turns the optional rule on for every new
        // game; it goes into the setup, so savegames and restarts keep what they began with.
        if self.ui.settings().hit_locations {
            setup.rules.hit_locations = true;
        }
        // Likewise `body_spacing=on` (`docs/body-spacing.md`).
        if self.ui.settings().body_spacing {
            setup.rules.body_spacing = true;
        }
        // And `mortars=on` (#301, docs/simulation.md "Mortar").
        if self.ui.settings().mortars {
            setup.rules.mortars = true;
        }
        // And `corpse_piles=on` (#222, docs/simulation.md "Corpse piles").
        if self.ui.settings().corpse_piles {
            setup.rules.corpse_piles = true;
        }
        // And `group_move` (on by default with the modern controls; docs/simulation.md
        // "Group moves").
        if self.ui.settings().group_move() {
            setup.rules.group_move = true;
        }
        // And `cannon_fire=on` (round shot sets men on fire under hit locations).
        if self.ui.settings().cannon_fire {
            setup.rules.cannon_fire = true;
        }
        // And `formation_attack=on` (docs/simulation.md "Formation attack").
        if self.ui.settings().formation_attack {
            setup.rules.formation_attack = true;
        }
        // And `animal_give_up=on` (#355, docs/simulation.md "Animals give up").
        if self.ui.settings().animal_give_up {
            setup.rules.animal_give_up = true;
        }
        // And `dart_fire=on` (blowpipe darts set men on fire under hit locations).
        if self.ui.settings().dart_fire {
            setup.rules.dart_fire = true;
        }
        // And `aimed_fire=on` (shots beyond 3 m are aimed: more exact, a longer reload).
        if self.ui.settings().aimed_fire {
            setup.rules.aimed_fire = true;
        }
        // And `ball_terrain` (with hit locations, on by default with the modern controls).
        if self.ui.settings().ball_terrain() {
            setup.rules.ball_terrain = true;
        }
        let mut game = ingame::InGame::new(setup);
        game.apply_settings(self.ui.settings(), self.input_mode);
        game.start(&self.assets.paths());
        self.mode = Mode::InGame(Box::new(game));
        self.load_requested();
    }

    /// Where savegames are kept: `Save\` in the game folder, or the browser's storage.
    pub fn set_profiles(&mut self, profiles: Box<dyn acr_app::saves::SaveStore>) {
        self.ui.set_profiles(profiles);
    }

    pub fn set_saves(&mut self, saves: Box<dyn acr_app::saves::SaveStore>) {
        self.ui.set_saves(saves);
    }

    /// Where the options are kept between runs (`settings.txt` in `%LOCALAPPDATA%\ACR`, the
    /// browser's storage); takes the stored options. `place` names it in the log, which says
    /// whether a stored file was read and the models mode it gave (a wrong path silently
    /// meant the defaults, models off).
    pub fn set_settings_store(&mut self, store: Box<dyn acr_app::saves::SaveStore>, place: &str) {
        self.stored_settings = self.ui.set_settings_store(store);
        let s = self.ui.settings();
        if self.stored_settings {
            log::info!("settings: loaded {place}: models={:?} controls={:?}", s.models, s.controls);
        } else {
            log::warn!("settings: no settings file at {place}, using the defaults: models={:?}", s.models);
        }
        let (effects, music) = self.ui.settings().volumes();
        self.audio.set_volumes(effects, music);
    }

    /// The window size of the options' resolution (not in fullscreen or the browser).
    fn apply_resolution(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.driver.is_none()
            && let Some(window) = &self.window
            && window.fullscreen().is_none()
        {
            let settings = self.ui.settings();
            let [w, h] = ui::settings::RESOLUTIONS[settings.resolution.min(ui::settings::RESOLUTIONS.len() - 1)];
            let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(w, h));
        }
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

    /// The app goes to the background (winit's `suspended`; Android's app switch destroys
    /// the window): the running game is paused, the audio stops, and the game is saved to
    /// the autosave slot, as the system may kill the process while it is away. Returns what
    /// was done, for the log and the driver.
    fn suspend_app(&mut self) -> serde_json::Value {
        let mut reply = serde_json::Map::new();
        if let Mode::InGame(game) = &mut self.mode
            && self.suspension.suspend(game.paused())
        {
            game.set_paused(true);
        }
        self.audio.set_paused(true);
        if let Mode::InGame(game) = &self.mode {
            reply.insert("paused".into(), game.paused().into());
        }
        let started = web_time::Instant::now();
        match self.autosave() {
            Ok(Some((tick, bytes, encode))) => {
                let total = started.elapsed();
                log::info!(
                    "autosave: tick {tick}, {bytes} bytes, encoded in {:.1} ms, all in {:.1} ms",
                    encode.as_secs_f64() * 1e3,
                    total.as_secs_f64() * 1e3
                );
                reply.insert("autosave".into(), suspend::autosave_file().into());
                reply.insert("bytes".into(), bytes.into());
                reply.insert("encode_ms".into(), (encode.as_secs_f64() * 1e3).into());
                reply.insert("total_ms".into(), (total.as_secs_f64() * 1e3).into());
            }
            Ok(None) => {}
            Err(e) => log::error!("cannot autosave: {e}"),
        }
        reply.into()
    }

    /// The app is back: the audio plays on, the game stays paused (the player resumes it).
    fn resume_app(&mut self) {
        self.audio.set_paused(false);
        if self.suspension.resume() {
            log::info!("resumed: the game stays paused");
        }
    }

    /// Writes the running game to the autosave slot: `None` when there is nothing to save
    /// (menus, a game still loading, a finished game); else the tick, size and encode time.
    fn autosave(&mut self) -> Result<Option<(u64, usize, web_time::Duration)>, String> {
        let Mode::InGame(game) = &self.mode else { return Ok(None) };
        let (unix, local) = acr_app::saves::local_time();
        let Some(save) = game.autosave_snapshot(suspend::AUTOSAVE, savegame::SaveTime { unix, local }) else {
            return Ok(None);
        };
        let started = web_time::Instant::now();
        let bytes = save.encode();
        let encode = started.elapsed();
        self.ui.saves_mut().write(&suspend::autosave_file(), &bytes)?;
        Ok(Some((save.tick, bytes.len(), encode)))
    }

    /// A load dialog's OK: starts the saved game, which replays up to its tick.
    fn load_game(&mut self, save: Box<savegame::SaveGame>) {
        log::info!("loading a game saved at tick {}: {:?}", save.tick, save.setup);
        self.ui.leave_game();
        self.ui.show_loading();
        let mut game = ingame::InGame::from_save(save);
        game.apply_settings(self.ui.settings(), self.input_mode);
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

    /// Ends the current game and returns to the menu it was started from.
    fn leave_game(&mut self) {
        match &self.mode {
            Mode::InGame(game) => self.ui.finish_game(&game.setup, game.mission_result()),
            _ => self.ui.leave_game(),
        }
        self.mode = Mode::Menus;
        self.unit_sounds.stop_crowds(&mut self.audio);
        if let Some(window) = &self.window {
            window.set_cursor_visible(false);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.hw_cursor.hidden();
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
        if let Some(window) = &self.window {
            // Android gives the window a new surface each time the app comes back.
            if let Some(g) = &mut self.graphics
                && let Err(e) = g.gpu.resume(window.clone())
            {
                log::error!("cannot recreate the surface: {e}");
            }
            self.resume_app();
            self.set_native_host_active(true);
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let attributes = match &self.driver {
            // Without a GPU the driver needs no window at all.
            Some(_) if self.headless.is_some() => return self.load_requested(),
            Some(d) => d.window_attributes(window_attributes(event_loop)),
            None => window_attributes(event_loop),
        };
        #[cfg(target_arch = "wasm32")]
        let attributes = window_attributes(event_loop);
        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("cannot create window: {e}");
                return event_loop.exit();
            }
        };
        // The menus draw the original cursor.
        window.set_cursor_visible(false);
        // The wheel and the middle button belong to the game, not to the page.
        #[cfg(target_arch = "wasm32")]
        web::guard_canvas(&window);
        self.window = Some(window.clone());
        #[cfg(not(target_arch = "wasm32"))]
        let driven = self.driver.is_some();
        #[cfg(target_arch = "wasm32")]
        let driven = false;
        if self.stored_settings && !driven {
            self.apply_resolution();
        }
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
        // Browsers only allow audio after a user gesture; native starts right away (but the
        // test driver plays nothing).
        #[cfg(not(target_arch = "wasm32"))]
        if !driven {
            self.audio.start();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(g) = &mut self.graphics {
            g.gpu.suspend();
        }
        self.suspend_app();
        self.set_native_host_active(false);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        self.pacing.input();
        match event {
            Event::GpuReady(gpu) => {
                let gpu = *gpu;
                // The graphics preset `auto` stands for (docs/rendering-3d.md "Graphics
                // presets"); on the first start its switches are set once.
                let detected = ui::graphics::detect(&gpu.info);
                ui::graphics::set_detected(detected);
                log::info!("graphics: {} adapter gives preset {}", gpu.info.name, detected.name());
                if self.ui.graphics_unset() && self.ui.settings().graphics == ui::graphics::Graphics::Auto {
                    #[cfg(not(target_arch = "wasm32"))]
                    let driven = self.driver.is_some();
                    #[cfg(target_arch = "wasm32")]
                    let driven = false;
                    // The driver never writes the user's options.
                    self.ui.change_settings_stored(|s| detected.apply_switches(s), !driven);
                }
                let images = ImageRenderer::new(&gpu.device, gpu.config.format);
                let sprites = SpriteRenderer::new(&gpu.device, gpu.config.format);
                let upscaler = Upscaler::new(&gpu.device, gpu.config.format);
                self.graphics = Some(Graphics { gpu, images, sprites, upscaler });
                #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
                if self.native_ui
                    && self.native_host.is_none()
                    && let Some(window) = &self.window
                {
                    match webui::native::Host::new(window, self.proxy.clone()) {
                        Ok(host) => self.native_host = Some(Box::new(host)),
                        Err(e) => log::error!("webui: no native host, the engine draws: {e}"),
                    }
                }
                self.sync_render_scale();
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    if let Some(g) = &mut self.graphics {
                        g.gpu.resize(size.width, size.height);
                    }
                }
            }
            Event::Ui(json) => webui::input::on_message(self, event_loop, &json),
            Event::UiTouch { id, phase, x, y } => {
                let at = self.to_render(PhysicalPosition::new(x, y));
                self.switch_input(input_mode::InputMode::Touch);
                self.touch_at(id, phase, at, event_loop)
            }
            Event::UiAsset { path, reply } => match self.webui.as_mut() {
                Some(web) => web.request_asset(path, reply),
                None => reply(Err(webui::AssetError::NotFound("the UI is not connected".into()))),
            },
            Event::AudioUnlock => {
                #[cfg(target_arch = "wasm32")]
                self.audio.start();
            }
            Event::Loaded { path, result } => {
                self.in_flight.set(self.in_flight.get().saturating_sub(1));
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
        // Any event (input, resize, focus) restores the full rate at once (render on demand).
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.pacing.input();
        }
        match event {
            WindowEvent::CloseRequested => self.apply(ui::Action::Quit, event_loop),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.graphics {
                    g.gpu.resize(size.width, size.height);
                }
                #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
                if let (Some(host), Some(window)) = (&self.native_host, &self.window) {
                    host.resize(window);
                }
            }
            #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
            WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(host), Some(window)) = (&self.native_host, &self.window) {
                    host.resize(window);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = self.to_render(position);
                self.switch_input(input_mode::InputMode::Mouse);
                self.cursor_moved(position, event_loop)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.switch_input(input_mode::InputMode::Mouse);
                self.mouse_input(state, button, event_loop)
            }
            WindowEvent::Touch(mut touch) => {
                touch.location = self.to_render(touch.location);
                self.switch_input(input_mode::InputMode::Touch);
                self.touch(touch, event_loop)
            }
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key, text, state, .. }, .. } => {
                self.keyboard_input(logical_key, text.as_deref(), state, event_loop)
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.switch_input(input_mode::InputMode::Mouse);
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.mouse_wheel(lines, event_loop);
            }
            WindowEvent::Focused(false) => {
                // Keys and buttons released elsewhere never arrive.
                if let Mode::InGame(game) = &mut self.mode {
                    game.release_all();
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers_changed(m.state()),
            WindowEvent::RedrawRequested => {
                // The test driver advances the game itself.
                #[cfg(not(target_arch = "wasm32"))]
                if self.driver.is_some() {
                    return;
                }
                self.redraw(event_loop)
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The webview's input and IPC are GTK events, which this loop does not wake for.
        #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
        if self.native_host.is_some() {
            webui::native::pump();
        }
        self.touch_tick(event_loop);
        #[cfg(not(target_arch = "wasm32"))]
        if self.driver.is_some() {
            return self.drive(event_loop);
        }
        let Some(window) = &self.window else { return };
        // A resting finger sends no events but a long press needs the clock to tick.
        if self.touches.down() {
            self.pacing.input();
        }
        // Capped: sleep until the next frame is due instead of asking for it at once (#440).
        match self.pacing.wait(web_time::Instant::now()) {
            // A shown webview needs the loop awake often enough to pump GTK.
            #[cfg(all(feature = "webview-gtk", target_os = "linux"))]
            Some(wait) if self.native_host.as_ref().is_some_and(|h| h.visible()) => event_loop.set_control_flow(
                ControlFlow::WaitUntil(web_time::Instant::now() + wait.min(std::time::Duration::from_millis(8))),
            ),
            Some(wait) => event_loop.set_control_flow(ControlFlow::WaitUntil(web_time::Instant::now() + wait)),
            None => {
                event_loop.set_control_flow(ControlFlow::Wait);
                window.request_redraw();
            }
        }
    }
}

/// A hash of the draws of a frame: the images and where they go.
fn draws_signature(draws: &[ImageDraw]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    draws.len().hash(&mut h);
    for d in draws {
        d.texture.id().hash(&mut h);
        for v in d.dst.iter().chain(&d.src).chain(&d.color) {
            v.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

/// Input from the window (or the test driver), in window pixels.
impl Game {
    /// A window position in render pixels (the layout's), below 100 % render scale.
    fn to_render(&self, p: PhysicalPosition<f64>) -> PhysicalPosition<f64> {
        let f = self.input_factor();
        PhysicalPosition::new(p.x * f, p.y * f)
    }

    /// A finger on the screen, through the touch gestures (`touch.rs`): tap, drag and long
    /// press (with a swipe: the order's arrow) work as the mouse, two fingers pan and zoom, a ▶ unpauses.
    fn touch(&mut self, touch: winit::event::Touch, event_loop: &ActiveEventLoop) {
        self.touch_at(touch.id, touch.phase, touch.location, event_loop)
    }

    /// [`Game::touch`] for a finger `id` at `location` (render pixels).
    fn touch_at(
        &mut self,
        id: u64,
        phase: winit::event::TouchPhase,
        location: PhysicalPosition<f64>,
        event_loop: &ActiveEventLoop,
    ) {
        use winit::event::TouchPhase;
        let phase = match phase {
            TouchPhase::Started => touch::Phase::Started,
            TouchPhase::Moved => touch::Phase::Moved,
            TouchPhase::Ended | TouchPhase::Cancelled => touch::Phase::Ended,
        };
        if let Some(window) = &self.window {
            self.touches.set_scale(window.scale_factor() * self.input_factor());
        }
        let paused = matches!(&self.mode, Mode::InGame(g) if g.paused());
        let now = self.touch_now();
        let gestures = self.touches.event(id, phase, [location.x, location.y], now, paused);
        self.gestures(gestures, event_loop);
    }

    /// A finger of the test driver (`{"cmd": "touch"}`): the game goes into touch mode, `time`
    /// (ms) sets the driver's touch clock, and `phase` is the finger's event (`None`: only the
    /// clock runs, a long press).
    pub(crate) fn drive_touch(
        &mut self,
        id: u64,
        phase: Option<touch::Phase>,
        pos: [f32; 2],
        time: Option<u64>,
        event_loop: &ActiveEventLoop,
    ) {
        if self.input_mode != input_mode::InputMode::Touch {
            self.input_mode = input_mode::InputMode::Touch;
            if let Mode::InGame(game) = &mut self.mode {
                game.set_input_mode(self.ui.settings(), self.input_mode);
            }
        }
        if let Some(ms) = time {
            self.touch_time = Some(std::time::Duration::from_millis(ms));
        }
        let now = self.touch_now();
        let paused = matches!(&self.mode, Mode::InGame(g) if g.paused());
        let mut gestures = match phase {
            Some(phase) => self.touches.event(id, phase, [f64::from(pos[0]), f64::from(pos[1])], now, paused),
            None => Vec::new(),
        };
        gestures.extend(self.touches.tick(now));
        self.gestures(gestures, event_loop);
    }

    /// An input of `kind` arrived: on a change of the mode the top bar and the cursor follow,
    /// and what the other input still held is let go (a finger's button, a pressed key).
    fn switch_input(&mut self, kind: input_mode::InputMode) {
        let mode = self.input_mode.after(kind);
        if mode == self.input_mode {
            return;
        }
        self.input_mode = mode;
        self.touches = touch::Touches::default();
        if let Mode::InGame(game) = &mut self.mode {
            game.release_all();
            game.set_input_mode(self.ui.settings(), mode);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.hw_cursor.hidden();
    }

    /// The touch gestures' clock: a resting finger becomes a right click.
    fn touch_tick(&mut self, event_loop: &ActiveEventLoop) {
        let gestures = self.touches.tick(self.touch_now());
        self.gestures(gestures, event_loop);
    }

    /// The touch clock: the driver's time while it feeds fingers, else the real one.
    fn touch_now(&self) -> std::time::Duration {
        self.touch_time.unwrap_or_else(|| self.touch_clock.elapsed())
    }

    fn gestures(&mut self, gestures: Vec<touch::Gesture>, event_loop: &ActiveEventLoop) {
        use touch::Gesture;
        let now = self.touch_now();
        if let Mode::InGame(game) = &mut self.mode {
            game.set_touch_time(now);
        }
        for g in gestures {
            match g {
                Gesture::Move([x, y]) => self.cursor_moved(PhysicalPosition::new(x, y), event_loop),
                Gesture::LeftDown => self.mouse_input(ElementState::Pressed, MouseButton::Left, event_loop),
                Gesture::LeftUp => self.mouse_input(ElementState::Released, MouseButton::Left, event_loop),
                Gesture::RightDown => self.mouse_input(ElementState::Pressed, MouseButton::Right, event_loop),
                Gesture::RightUp => self.mouse_input(ElementState::Released, MouseButton::Right, event_loop),
                Gesture::MiddleDown => self.mouse_input(ElementState::Pressed, MouseButton::Middle, event_loop),
                Gesture::MiddleUp => self.mouse_input(ElementState::Released, MouseButton::Middle, event_loop),
                Gesture::Unpause => {
                    if let Mode::InGame(game) = &mut self.mode
                        && game.ready()
                        && !self.ui.dialog_open()
                    {
                        game.set_paused(false);
                    }
                }
                Gesture::Zoom(lines) => {
                    if matches!(self.mode, Mode::InGame(_)) {
                        self.mouse_wheel(lines, event_loop);
                    }
                }
            }
        }
    }

    fn cursor_moved(&mut self, position: PhysicalPosition<f64>, event_loop: &ActiveEventLoop) {
        self.cursor = position;
        if let Mode::InGame(game) = &mut self.mode {
            game.mouse_moved([position.x as f32, position.y as f32]);
        }
        if let Some(p) = self.virtual_cursor() {
            self.ui.set_mouse(p);
            self.ui_input(ui::Input::MouseMove(p), event_loop);
        }
    }

    fn mouse_input(&mut self, state: ElementState, button: MouseButton, event_loop: &ActiveEventLoop) {
        if state == ElementState::Pressed && matches!(self.mode, Mode::Intro) {
            return self.intro_input();
        }
        if matches!(&self.mode, Mode::InGame(g) if g.ready()) && !self.ui.dialog_open() {
            let Mode::InGame(game) = &mut self.mode else { return };
            let ctx = self.ui.ctx();
            let result = match (button, state) {
                (MouseButton::Left, _) => game.left_button(&ctx, state == ElementState::Pressed),
                (MouseButton::Right, _) => {
                    game.right_button(&ctx, state == ElementState::Pressed);
                    ingame::GameMouse::None
                }
                (MouseButton::Middle, _) => {
                    game.middle_button(state == ElementState::Pressed);
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
                ingame::GameMouse::History => {
                    if let Some(window) = game.history_window() {
                        self.ui.open_dialog(window);
                        self.load_requested();
                    }
                }
                ingame::GameMouse::Escape => self.press_escape(event_loop),
                ingame::GameMouse::None => {}
            }
            return;
        }
        if button == MouseButton::Left {
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
    }

    /// The Escape key, pressed and released (the touch top bar's plaque, Android's back key).
    fn press_escape(&mut self, event_loop: &ActiveEventLoop) {
        for state in [ElementState::Pressed, ElementState::Released] {
            self.keyboard_input(Key::Named(NamedKey::Escape), None, state, event_loop);
        }
    }

    fn mouse_wheel(&mut self, lines: f32, event_loop: &ActiveEventLoop) {
        // Over the running game the wheel zooms (modern controls only).
        if let Mode::InGame(game) = &mut self.mode
            && game.ready()
            && !self.ui.dialog_open()
        {
            if !game.editor_wheel(&self.ui.ctx(), lines) {
                game.wheel(lines);
            }
            return;
        }
        self.ui_input(ui::Input::Wheel(lines), event_loop);
    }

    fn modifiers_changed(&mut self, state: ModifiersState) {
        self.modifiers = state;
        if let Mode::InGame(game) = &mut self.mode {
            game.set_modifiers(state);
        }
    }

    fn keyboard_input(
        &mut self,
        logical_key: Key,
        text: Option<&str>,
        state: ElementState,
        event_loop: &ActiveEventLoop,
    ) {
        // Android's back key (KEYCODE_BACK, winit's `BrowserBack`) is Escape.
        #[cfg(target_os = "android")]
        let logical_key = match logical_key {
            Key::Named(NamedKey::BrowserBack | NamedKey::GoBack) => Key::Named(NamedKey::Escape),
            key => key,
        };
        let pressed = state == ElementState::Pressed;
        if matches!(&self.mode, Mode::InGame(g) if g.ready())
            && (!self.ui.dialog_open() || !pressed)
            && !matches!(logical_key, Key::Named(NamedKey::F11))
            && !(logical_key == Key::Named(NamedKey::Enter) && self.alt_down())
        {
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
            let Mode::InGame(game) = &mut self.mode else { return };
            match game.key(&self.ui.ctx(), &logical_key, pressed) {
                ingame::GameKey::Menu => {
                    self.ui.open_game_menu();
                    self.load_requested();
                }
                // A leader chord of the modern controls: a press of the key it stands for.
                ingame::GameKey::Leader(named) => {
                    self.keyboard_input(Key::Named(named), None, ElementState::Pressed, event_loop);
                    self.keyboard_input(Key::Named(named), None, ElementState::Released, event_loop);
                }
                ingame::GameKey::None => {}
            }
            return;
        }
        if !pressed {
            return;
        }
        if matches!(self.mode, Mode::Intro)
            && matches!(logical_key, Key::Named(NamedKey::Escape | NamedKey::Space | NamedKey::Enter))
            && !self.alt_down()
        {
            return self.intro_input();
        }
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
            _ => text.and_then(cp1252).map(ui::Key::Char),
        };
        if let Some(key) = key {
            self.ui_input(ui::Input::Key(key), event_loop);
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

#[cfg(not(target_os = "android"))]
fn event_loop() -> Result<winit::event_loop::EventLoop<Event>, String> {
    winit::event_loop::EventLoop::<Event>::with_user_event().build().map_err(|e| e.to_string())
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
