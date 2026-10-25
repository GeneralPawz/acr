//! Graphics presets (`graphics=auto|low|medium|high|ultra`, docs/rendering-3d.md
//! "Graphics presets"): Niedrig, Mittel, Hoch, Ultra. A preset sets the single 3D switches
//! (shadows, levels of detail, material detail, wind, ground cover, water) once when chosen,
//! and gives the frame its limits ([`Budget`]: triangles, draw calls, shadow casters, the
//! levels' distances). Hoch is today's full look without limits; Ultra holds the finer levels
//! further out; Niedrig and Mittel are for integrated GPUs.
//!
//! `auto` (the default) takes the preset of the adapter ([`detect`]): on the first start
//! (no `graphics` line in `settings.txt` yet) its switches are applied and stored; after that
//! the switches are the user's, and only the limits follow the detected preset.

use std::sync::atomic::{AtomicU8, Ordering};

use acr_render::lod::Budget;

use super::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Graphics {
    #[default]
    Auto,
    Low,
    Medium,
    High,
    Ultra,
}

/// The presets in menu order (after "Automatisch").
pub const PRESETS: [Graphics; 4] = [Graphics::Low, Graphics::Medium, Graphics::High, Graphics::Ultra];

/// The preset `auto` stands for ([`set_detected`]); High until an adapter is known.
static DETECTED: AtomicU8 = AtomicU8::new(3);

impl Graphics {
    pub fn name(self) -> &'static str {
        match self {
            Graphics::Auto => "auto",
            Graphics::Low => "low",
            Graphics::Medium => "medium",
            Graphics::High => "high",
            Graphics::Ultra => "ultra",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Graphics::Auto, Graphics::Low, Graphics::Medium, Graphics::High, Graphics::Ultra]
            .into_iter()
            .find(|g| g.name() == value)
    }

    fn index(self) -> u8 {
        match self {
            Graphics::Auto => 0,
            Graphics::Low => 1,
            Graphics::Medium => 2,
            Graphics::High => 3,
            Graphics::Ultra => 4,
        }
    }

    /// The preset in effect: `auto` resolved to the detected one.
    pub fn resolved(self) -> Self {
        match self {
            Graphics::Auto => match DETECTED.load(Ordering::Relaxed) {
                1 => Graphics::Low,
                2 => Graphics::Medium,
                4 => Graphics::Ultra,
                _ => Graphics::High,
            },
            g => g,
        }
    }

    /// The frame's limits. Measured on #PAGE0303 (Single3, docs/rendering-3d.md).
    pub fn budget(self) -> Budget {
        match self.resolved() {
            Graphics::Low => Budget {
                triangles: 1_500_000,
                draws: 700,
                casters: 0,
                shadow_triangles: 0,
                shadow_reach: 0.0,
                lod_scale: 0.75,
            },
            Graphics::Medium => Budget {
                triangles: 3_000_000,
                draws: 1200,
                casters: 400,
                shadow_triangles: 600_000,
                shadow_reach: 0.6,
                lod_scale: 1.0,
            },
            Graphics::Ultra => Budget { lod_scale: 1.5, ..Budget::NONE },
            _ => Budget::NONE,
        }
    }

    /// Sets the single switches the preset stands for (when the preset is chosen).
    pub fn apply_switches(self, s: &mut Settings) {
        let p = self.resolved();
        let rich = matches!(p, Graphics::High | Graphics::Ultra);
        s.shadows = p != Graphics::Low;
        s.lod = true;
        s.material_detail = p != Graphics::Low;
        s.wind = p != Graphics::Low;
        s.ground_cover = rich;
        s.water_detailed = p != Graphics::Low;
    }
}

/// Sets the preset `auto` stands for.
pub fn set_detected(g: Graphics) {
    DETECTED.store(g.resolved().index(), Ordering::Relaxed);
}

/// The preset for an adapter: integrated GPUs (and software ones) Niedrig or Mittel, the
/// rest Hoch. `ACR_GPU_CLASS=integrated|discrete|low|medium|high|ultra` (native) stands in
/// for the adapter (the weak-GPU simulation on the render VM).
pub fn detect(info: &wgpu::AdapterInfo) -> Graphics {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(v) = std::env::var("ACR_GPU_CLASS") {
        match v.as_str() {
            "integrated" => return Graphics::Medium,
            "discrete" => return Graphics::High,
            other => {
                if let Some(g) = Graphics::parse(other) {
                    return g.resolved();
                }
            }
        }
    }
    match info.device_type {
        wgpu::DeviceType::Cpu => Graphics::Low,
        wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::VirtualGpu => Graphics::Medium,
        wgpu::DeviceType::DiscreteGpu => Graphics::High,
        // The browser hides the type: a mobile or WebGL2 GPU is the likely one.
        wgpu::DeviceType::Other => Graphics::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_and_high_keeps_todays_look() {
        for g in [Graphics::Auto, Graphics::Low, Graphics::Medium, Graphics::High, Graphics::Ultra] {
            assert_eq!(Graphics::parse(g.name()), Some(g));
        }
        assert_eq!(Graphics::High.budget(), Budget::NONE);
        let mut s = Settings::default();
        Graphics::High.apply_switches(&mut s);
        assert_eq!(s, Settings::default());
        let mut low = Settings::default();
        Graphics::Low.apply_switches(&mut low);
        assert!(!low.shadows && low.lod);
    }
}
