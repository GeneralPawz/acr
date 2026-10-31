//! Platform layer shared by the ACR apps (game, viewer).
//!
//! - [`Gpu`]: surface, device and queue for a winit window, natively and in the browser.
//! - [`Assets`]: reading game files, synchronously from disk ([`NativeAssets`]) or
//!   asynchronously from the player's folder or a URL in the browser (`web::WebAssets`).
//! - [`spawn`]: run a future to completion on either platform.
//! - [`saves`]: where savegames are stored.

use std::sync::Arc;

use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
mod native;
pub mod saves;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeAssets;

/// Delivers the bytes of a requested file (or an error message).
pub type LoadCallback = Box<dyn FnOnce(Result<Vec<u8>, String>)>;

/// Reads game files by their (case-insensitive) game path.
pub trait Assets {
    /// All visible game paths, sorted.
    fn paths(&self) -> Vec<String>;
    /// Reads `path` and calls `done` with the result: immediately on native, later in the
    /// browser. Callers forward the result into their event loop.
    fn request(&self, path: &str, done: LoadCallback);
}

/// Runs `future` to completion: blocking on native, as a browser task on the web.
pub fn spawn(future: impl Future<Output = ()> + 'static) {
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(future);
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(future);
}

/// The frames the swapchain may queue: 2, or `ACR_FRAME_LATENCY` (native, 1–3). One frame
/// lost the device on the Snapdragon X DX12 driver within seconds (2026-10-23).
fn frame_latency() -> u32 {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(n) = std::env::var("ACR_FRAME_LATENCY").ok().and_then(|v| v.parse::<u32>().ok()) {
        return n.clamp(1, 3);
    }
    2
}

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    /// The adapter's name, driver and backend (the performance recorder's snapshots; the
    /// graphics preset `auto` picks by its type).
    pub info: wgpu::AdapterInfo,
    /// Kept to recreate the surface when Android gives the window a new one.
    instance: wgpu::Instance,
    /// The window's surface is gone (Android, while the app is in the background).
    suspended: bool,
    /// The share of the surface's size the scene is drawn at (render scale, #440); 1 draws
    /// straight into the swapchain.
    scale: f32,
}

/// The size the scene is drawn at: the surface's size times `scale`, rounded, at least 1 px.
pub fn scaled_size(surface: [u32; 2], scale: f32) -> [u32; 2] {
    surface.map(|v| ((v as f32 * scale).round() as u32).max(1))
}

impl Gpu {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        // On the desktop only the primary backends (DX12, Vulkan, Metal) are tried first:
        // probing OpenGL on Windows creates a hidden legacy GL context, which other programs
        // see as the desktop composition switching off (PowerToys crashed on it). OpenGL stays
        // the fallback when no primary adapter exists; `WGPU_BACKEND` still overrides both.
        let (instance, surface, adapter) = match Self::adapter(window.clone(), cfg!(not(target_arch = "wasm32"))).await
        {
            Ok(found) => found,
            Err(e) if cfg!(not(target_arch = "wasm32")) => {
                log::warn!("no DX12/Vulkan/Metal adapter ({e}), trying all backends");
                Self::adapter(window.clone(), false).await?
            }
            Err(e) => return Err(e),
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("acr"),
                // Timestamp queries for the driver's `gputime`, and the block-compressed texture
                // formats KTX2 model textures transcode to (`acr_render::ktx2`), where the
                // adapter has them.
                required_features: adapter.features()
                    & (wgpu::Features::TIMESTAMP_QUERY
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
                        | wgpu::Features::TEXTURE_COMPRESSION_BC
                        | wgpu::Features::TEXTURE_COMPRESSION_ETC2
                        | wgpu::Features::TEXTURE_COMPRESSION_ASTC),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        // A lost device or an uncaught validation error otherwise ends the process with
        // nothing in the log (the window has no console).
        device.set_device_lost_callback(|reason, message| {
            log::error!("GPU device lost ({reason:?}): {message}");
            log::logger().flush();
        });
        device.on_uncaptured_error(Arc::new(|error: wgpu::Error| {
            log::error!("uncaptured GPU error: {error}");
            log::logger().flush();
            panic!("wgpu error: {error}");
        }));
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface not supported by adapter")?;
        // Two frames queued: one (lower latency, docs/rendering-3d.md, "Input latency") lost
        // the device on the Snapdragon X DX12 driver; the hardware cursor keeps input responsive.
        // The game prepares the next frame (simulation, models) before it asks for the
        // swapchain image, so that work still overlaps the GPU's previous frame.
        // `ACR_FRAME_LATENCY` (native) overrides it.
        config.desired_maximum_frame_latency = frame_latency();
        // The default is the adapter's first mode, which on Android (Vulkan) can be an unlocked
        // one: the game then drew 300-400 frames a second on a phone. Vsync keeps it at the
        // display's rate (and the battery and heat down).
        #[cfg(target_os = "android")]
        {
            config.present_mode = wgpu::PresentMode::AutoVsync;
        }
        log::info!(
            "surface: {:?}, {} frame(s) queued, {:?}",
            config.present_mode,
            config.desired_maximum_frame_latency,
            config.format
        );
        surface.configure(&device, &config);
        let info = adapter.get_info();
        log::info!("adapter: {} ({:?}, {:?})", info.name, info.device_type, info.backend);
        Ok(Self { surface, device, queue, config, info, instance, suspended: false, scale: 1.0 })
    }

    /// An instance, the window's surface and an adapter for it; `primary` limits the search
    /// to the primary backends unless `WGPU_BACKEND` names others.
    async fn adapter(
        window: Arc<Window>,
        primary: bool,
    ) -> Result<(wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter), String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if primary && std::env::var_os("WGPU_BACKEND").is_none() {
            descriptor.backends = wgpu::Backends::PRIMARY;
        }
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance.create_surface(window).map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })
            .await
            .map_err(|e| e.to_string())?;
        Ok((instance, surface, adapter))
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        if !self.suspended {
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Stops drawing: the window's surface is about to go away (Android, app in the
    /// background).
    pub fn suspend(&mut self) {
        self.suspended = true;
    }

    /// A new surface for `window` after [`Gpu::suspend`], on the same device, so every
    /// texture and pipeline stays valid.
    pub fn resume(&mut self, window: Arc<Window>) -> Result<(), String> {
        let size = window.inner_size();
        self.surface = self.instance.create_surface(window).map_err(|e| e.to_string())?;
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.suspended = false;
        Ok(())
    }

    /// The size the game lays out for and draws the scene at: the surface's, times the render
    /// scale ([`Gpu::set_render_scale`]).
    pub fn size(&self) -> [f32; 2] {
        scaled_size(self.surface_size(), self.scale).map(|v| v as f32)
    }

    /// The surface's (the window's) size in pixels.
    pub fn surface_size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    /// Draws the scene at `scale` (0..=1) of the surface's size; the caller upscales it.
    pub fn set_render_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(0.1, 1.0);
    }

    /// The render scale in effect.
    pub fn render_scale(&self) -> f32 {
        self.scale
    }

    /// The next frame to draw into, or `None` if this frame should be skipped.
    pub fn next_frame(&mut self) -> Option<wgpu::SurfaceTexture> {
        if self.suspended {
            return None;
        }
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => Some(f),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scaled_size;

    #[test]
    fn render_scale_target_sizes() {
        assert_eq!(scaled_size([3168, 1440], 0.75), [2376, 1080]);
        assert_eq!(scaled_size([3168, 1440], 0.5), [1584, 720]);
        assert_eq!(scaled_size([3168, 1440], 0.85), [2693, 1224]);
        assert_eq!(scaled_size([1024, 768], 1.0), [1024, 768]);
        // Rounded, and never empty.
        assert_eq!(scaled_size([1001, 1], 0.5), [501, 1]);
        assert_eq!(scaled_size([1, 1], 0.5), [1, 1]);
        assert_eq!(scaled_size([0, 0], 0.5), [1, 1]);
    }
}
