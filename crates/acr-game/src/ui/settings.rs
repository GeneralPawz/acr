//! Player options edited in the options screen (0x5c9740).

/// The original's option globals, with their ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `WarSound`: sound effect volume 0..=100.
    pub sound_volume: i32,
    /// `MidiSound`: music volume 0..=100.
    pub music_volume: i32,
    /// Music mode: 0 no music (`NOCD`), 1 random tracks (`RNDMUS`). The removed third
    /// choice (2, "Spotify mitmischen") reads as no music.
    pub music_mode: usize,
    /// Game mode: 0 slow (`SLOWMOD`), 1 fast (`FASTMOD`).
    pub game_mode: usize,
    /// `ScrollSpeed`: 2..=10.
    pub scroll_speed: i32,
    /// `FPSTime`: frame delay 0..=100; the slider shows `100 - FPSTime`.
    pub frame_time: i32,
    /// Index into [`RESOLUTIONS`].
    pub resolution: usize,
    /// Record games (`Spiel aufzeichnen`) into `record_file`.
    pub record: bool,
    pub record_file: Vec<u8>,
    /// ACR: the in-game control scheme (#129, see docs/controls.md).
    pub controls: Controls,
    /// ACR: the view scrolls with the cursor at the window edge. Always on with the original
    /// controls; the modern scheme can switch it off.
    pub edge_scroll: bool,
    /// ACR (#163): which objects are drawn as 3D models where a model exists
    /// (docs/rendering-3d.md). Only in `settings.txt` / `localStorage` for now.
    pub models: Models,
    /// ACR (#164): the original fixed camera or the free one.
    pub camera: CameraMode,
    /// ACR (#183): blood and gore of 3D corpses; sprites keep the original fade.
    pub gore: Gore,
    /// ACR: how long 3D corpses lie and decay, in minutes of game time (the simulation
    /// removes them after about a minute as the original does; the renderer keeps them).
    pub corpse_minutes: u32,
    /// ACR: how far 3D corpses rot (`corpse_decay=full|half`, full by default): `half`
    /// stops at the half-rotted stage, never the bones (`acr_render::model::HALF_DECAY`),
    /// to look at thrown remains.
    pub corpse_decay_half: bool,
    /// ACR (#225, #254): new games use the optional hit-location rule (wounds, lost limbs,
    /// bleeding out); `hit_locations=on|off`, off (the original rules) by default.
    pub hit_locations: bool,
    /// ACR: new games use the optional body-spacing rule (men, horses and animals take up
    /// space, docs/body-spacing.md); `body_spacing=on|off`, off by default.
    pub body_spacing: bool,
    /// ACR: sun shadows of the 3D models (`shadows=on|off`, on by default;
    /// docs/rendering-3d.md "Shadows").
    pub shadows: bool,
    /// ACR: levels of detail of the 3D models (`lod=on|off`, on by default;
    /// docs/rendering-3d.md "Levels of detail"): distant figures drawn with fewer triangles.
    pub lod: bool,
    /// ACR: wind sways the trees and foliage of the 3D path (`wind=on|off`, on by default;
    /// docs/rendering-3d.md "Wind"): a vertex-shader bend from the models' `_WIND` weights.
    pub wind: bool,
    /// ACR: the map's trees and stones drawn as 3D models where `map_objects.json` names one
    /// (`map_models=on|off`, on by default; docs/rendering-3d.md "Map trees and stones").
    pub map_models: bool,
    /// ACR: the water surface of the 3D path (`water=detailed|original`, detailed by
    /// default; docs/water.md): waves, reflection and foam, or the original's depth tint.
    pub water_detailed: bool,
    /// ACR: the ground in 3D mode (`ground=detailed|original`, detailed by default):
    /// material textures over the original tiles (docs/ground.md). Sprite mode always
    /// draws the original ground.
    pub ground_detailed: bool,
    /// ACR: `battlefield_marks=fade|keep` (fade by default): with `keep` craters, scorch,
    /// earth scuffs, blood pools and the debris of the fallen stay for the whole battle
    /// (render only, capped and merged; `ingame/kept_marks.rs`, docs/rendering-3d.md).
    pub marks_keep: bool,
    /// ACR: `decay_stain=on|off`: a decaying corpse's fluids soak into the ground under it
    /// (a cadaver decomposition island: a dark, greasy stain drying to brown, the grass
    /// dying around it; docs/rendering-3d.md). `None` (not in the file): on with
    /// `gore=full`, off otherwise ([`Settings::decay_stain`]).
    pub decay_stain: Option<bool>,
    /// ACR (#224): `weather=off|rain|snow|fog` (off by default): rain, snow or fog over the
    /// 3D path, a look only (docs/rendering-3d.md "Weather (#224)"); no gameplay effect.
    pub weather: Weather,
}

pub use acr_render::weather::Weather;

/// `models=off|units|all`: sprites everywhere (the original), 3D units, 3D units and
/// buildings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Models {
    #[default]
    Off,
    Units,
    All,
}

/// `camera=fixed|free`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    Fixed,
    Free,
}

/// `gore=full|reduced|off`: corpses decompose fully, decompose without dissolving (they
/// discolour, collapse and sink), or fade like the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gore {
    #[default]
    Full,
    Reduced,
    Off,
}

/// The in-game controls: the original's, or the modern scheme with wheel zoom and drag
/// panning (docs/controls.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Controls {
    #[default]
    Original,
    Modern,
}

/// The lines of the control combo box ("Steuerung"), German like the game's texts, and the
/// scheme and edge scrolling each stands for.
pub const CONTROL_CHOICES: [(&[u8], Controls, bool); 3] = [
    (b"Steuerung: Original", Controls::Original, true),
    (b"Steuerung: Modern", Controls::Modern, true),
    (b"Steuerung: Modern, Rand aus", Controls::Modern, false),
];

/// Window sizes offered as "Grafikauflösung"; the menus always lay out 1024x768 and scale.
pub const RESOLUTIONS: [[u32; 2]; 4] = [[800, 600], [1024, 768], [1280, 960], [1600, 1200]];

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound_volume: 100,
            music_volume: 70,
            music_mode: 1,
            game_mode: 0,
            scroll_speed: 4,
            // `FPSTime = 50` at start-up (0x4a8541).
            frame_time: 50,
            resolution: 1,
            record: false,
            record_file: Vec::new(),
            controls: Controls::Original,
            edge_scroll: true,
            models: Models::Off,
            camera: CameraMode::Fixed,
            gore: Gore::Full,
            corpse_minutes: 5,
            corpse_decay_half: false,
            decay_stain: None,
            hit_locations: false,
            body_spacing: false,
            shadows: true,
            lod: true,
            wind: true,
            map_models: true,
            water_detailed: true,
            ground_detailed: true,
            marks_keep: false,
            weather: Weather::Off,
        }
    }
}

impl Settings {
    /// Whether decaying corpses stain the ground (`decay_stain`, by default with `gore=full`).
    pub fn decay_stain(&self) -> bool {
        self.decay_stain.unwrap_or(self.gore == Gore::Full)
    }

    /// Effect and music gain for the mixer (0..=1).
    pub fn volumes(&self) -> (f32, f32) {
        let gain = |v: i32| (v.clamp(0, 100) as f32 / 100.0).powi(2);
        let music = if self.music_mode == 1 { gain(self.music_volume) } else { 0.0 };
        (gain(self.sound_volume), music)
    }

    /// Edge scrolling in effect: always with the original controls.
    pub fn edge_scrolls(&self) -> bool {
        self.controls == Controls::Original || self.edge_scroll
    }

    /// The line of [`CONTROL_CHOICES`] for these settings.
    pub fn control_choice(&self) -> usize {
        match self.controls {
            Controls::Original => 0,
            Controls::Modern if self.edge_scroll => 1,
            Controls::Modern => 2,
        }
    }

    /// Applies a line of [`CONTROL_CHOICES`].
    pub fn set_control_choice(&mut self, i: usize) {
        let (_, controls, edge) = CONTROL_CHOICES[i.min(CONTROL_CHOICES.len() - 1)];
        self.controls = controls;
        self.edge_scroll = edge;
    }

    /// The settings file (`settings.txt` in `%LOCALAPPDATA%\ACR\`, `localStorage` in the
    /// browser): `key=value` lines. The original keeps its options in the registry and
    /// `mode.dat`; ACR does not touch those.
    pub fn encode(&self) -> String {
        let hex: String = self.record_file.iter().map(|b| format!("{b:02x}")).collect();
        let controls = match self.controls {
            Controls::Original => "original",
            Controls::Modern => "modern",
        };
        let models = match self.models {
            Models::Off => "off",
            Models::Units => "units",
            Models::All => "all",
        };
        let camera = match self.camera {
            CameraMode::Fixed => "fixed",
            CameraMode::Free => "free",
        };
        let gore = match self.gore {
            Gore::Full => "full",
            Gore::Reduced => "reduced",
            Gore::Off => "off",
        };
        format!(
            "sound_volume={}\nmusic_volume={}\nmusic_mode={}\ngame_mode={}\nscroll_speed={}\nframe_time={}\n\
             resolution={}\nrecord={}\nrecord_file={hex}\ncontrols={controls}\nedge_scroll={}\n\
             models={models}\ncamera={camera}\ngore={gore}\ncorpse_minutes={}\ncorpse_decay={}\nhit_locations={}\nbody_spacing={}\nshadows={}\nlod={}\nwind={}\nmap_models={}\nwater={}\n\
             ground={}\nbattlefield_marks={}\nweather={}\n{}",
            self.sound_volume,
            self.music_volume,
            self.music_mode,
            self.game_mode,
            self.scroll_speed,
            self.frame_time,
            self.resolution,
            u8::from(self.record),
            u8::from(self.edge_scroll),
            self.corpse_minutes,
            if self.corpse_decay_half { "half" } else { "full" },
            if self.hit_locations { "on" } else { "off" },
            if self.body_spacing { "on" } else { "off" },
            if self.shadows { "on" } else { "off" },
            if self.lod { "on" } else { "off" },
            if self.wind { "on" } else { "off" },
            if self.map_models { "on" } else { "off" },
            if self.water_detailed { "detailed" } else { "original" },
            if self.ground_detailed { "detailed" } else { "original" },
            if self.marks_keep { "keep" } else { "fade" },
            self.weather.name(),
            match self.decay_stain {
                Some(true) =>
                    "decay_stain=on
",
                Some(false) =>
                    "decay_stain=off
",
                None => "",
            },
        )
    }

    /// Reads [`Settings::encode`]'s format; missing, unknown or invalid entries keep their
    /// defaults, and values are clamped to the options' ranges.
    pub fn decode(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            let int = value.parse::<i32>().ok();
            match key {
                "sound_volume" => s.sound_volume = int.map_or(s.sound_volume, |v| v.clamp(0, 100)),
                "music_volume" => s.music_volume = int.map_or(s.music_volume, |v| v.clamp(0, 100)),
                "music_mode" => s.music_mode = int.map_or(s.music_mode, |v| usize::from(v == 1)),
                "game_mode" => s.game_mode = int.map_or(s.game_mode, |v| v.clamp(0, 1) as usize),
                "scroll_speed" => s.scroll_speed = int.map_or(s.scroll_speed, |v| v.clamp(2, 10)),
                "frame_time" => s.frame_time = int.map_or(s.frame_time, |v| v.clamp(0, 100)),
                "resolution" => {
                    s.resolution = int.map_or(s.resolution, |v| v.clamp(0, RESOLUTIONS.len() as i32 - 1) as usize)
                }
                "record" => s.record = value == "1",
                "record_file" => {
                    s.record_file = (0..value.len() / 2)
                        .filter_map(|i| u8::from_str_radix(value.get(2 * i..2 * i + 2)?, 16).ok())
                        .collect()
                }
                "controls" => s.controls = if value == "modern" { Controls::Modern } else { Controls::Original },
                "edge_scroll" => s.edge_scroll = value != "0",
                "models" => {
                    s.models = match value {
                        "units" => Models::Units,
                        "all" => Models::All,
                        _ => Models::Off,
                    }
                }
                "camera" => s.camera = if value == "free" { CameraMode::Free } else { CameraMode::Fixed },
                "hit_locations" => s.hit_locations = value == "on",
                "corpse_decay" => s.corpse_decay_half = value == "half",
                "body_spacing" => s.body_spacing = value == "on",
                "shadows" => s.shadows = value != "off",
                "lod" => s.lod = value != "off",
                "wind" => s.wind = value != "off",
                "map_models" => s.map_models = value != "off",
                "water" => s.water_detailed = value != "original",
                "ground" => s.ground_detailed = value != "original",
                "battlefield_marks" => s.marks_keep = value == "keep",
                "weather" => s.weather = Weather::parse(value).unwrap_or_default(),
                "decay_stain" => {
                    s.decay_stain = match value {
                        "on" => Some(true),
                        "off" => Some(false),
                        _ => None,
                    }
                }
                "corpse_minutes" => s.corpse_minutes = int.map_or(s.corpse_minutes, |v| v.clamp(1, 60) as u32),
                "gore" => {
                    s.gore = match value {
                        "reduced" => Gore::Reduced,
                        "off" => Gore::Off,
                        _ => Gore::Full,
                    }
                }
                _ => {}
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_file_round_trip() {
        let s = Settings {
            sound_volume: 30,
            music_volume: 0,
            music_mode: 0,
            game_mode: 1,
            scroll_speed: 9,
            frame_time: 20,
            resolution: 3,
            record: true,
            record_file: b"Spiel \xe4".to_vec(),
            controls: Controls::Modern,
            edge_scroll: false,
            models: Models::Units,
            camera: CameraMode::Free,
            gore: Gore::Reduced,
            shadows: false,
            lod: false,
            wind: false,
            map_models: false,
            water_detailed: false,
            ground_detailed: false,
            corpse_minutes: 12,
            corpse_decay_half: true,
            decay_stain: Some(false),
            hit_locations: true,
            body_spacing: true,
            marks_keep: true,
            weather: Weather::Snow,
        };
        assert_eq!(Settings::decode(&s.encode()), s);
        // The removed "Spotify mitmischen" (music_mode=2) reads as no music.
        assert_eq!(Settings::decode("music_mode=2\nspotify=1").music_mode, 0);
        assert_eq!(Settings::decode("music_mode=1").music_mode, 1);
        assert_eq!(Settings::decode("corpse_minutes=500").corpse_minutes, 60);
        // The decay stain follows the gore unless set.
        assert!(Settings::decode("").decay_stain());
        assert!(!Settings::decode("gore=reduced").decay_stain());
        assert!(
            Settings::decode(
                "gore=reduced
decay_stain=on"
            )
            .decay_stain()
        );
        assert!(!Settings::decode("decay_stain=off").decay_stain());
        assert_eq!(Settings::decode(""), Settings::default());
        assert_eq!(Settings::decode("weather=fog").weather, Weather::Fog);
        assert_eq!(Settings::decode("weather=hail").weather, Weather::Off);
        let odd = Settings::decode("scroll_speed=99\nfoo=1\nresolution=x\ncontrols=modern\nmodels=3d\ngore=off");
        assert_eq!((odd.scroll_speed, odd.resolution, odd.controls), (10, 1, Controls::Modern));
        assert_eq!((odd.models, odd.camera, odd.gore), (Models::Off, CameraMode::Fixed, Gore::Off));
        let all = Settings::decode("models=all\ncamera=free");
        assert_eq!((all.models, all.camera, all.gore), (Models::All, CameraMode::Free, Gore::Full));
    }

    #[test]
    fn control_choices() {
        let mut s = Settings::default();
        assert_eq!(s.control_choice(), 0);
        for i in 0..CONTROL_CHOICES.len() {
            s.set_control_choice(i);
            assert_eq!(s.control_choice(), i);
        }
        assert!(!s.edge_scrolls());
        s.set_control_choice(0);
        assert!(s.edge_scrolls());
    }
}
