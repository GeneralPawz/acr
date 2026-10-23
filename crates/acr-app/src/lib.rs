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

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

impl Gpu {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        // On the desktop only the primary backends (DX12, Vulkan, Metal) are tried first:
        // probing OpenGL on Windows creates a hidden legacy GL context, which other programs
        // see as the desktop composition switching off (PowerToys crashed on it). OpenGL stays
        // the fallback when no primary adapter exists; `WGPU_BACKEND` still overrides both.
        let (_instance, surface, adapter) = match Self::adapter(window.clone(), cfg!(not(target_arch = "wasm32"))).await
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
                // Timestamp queries for the driver's `gputime`, where the adapter has them.
                required_features: adapter.features()
                    & (wgpu::Features::TIMESTAMP_QUERY
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES),
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
        Ok(Self { surface, device, queue, config })
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
        self.surface.configure(&self.device, &self.config);
    }

    pub fn size(&self) -> [f32; 2] {
        [self.config.width as f32, self.config.height as f32]
    }

    /// The next frame to draw into, or `None` if this frame should be skipped.
    pub fn next_frame(&mut self) -> Option<wgpu::SurfaceTexture> {
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
