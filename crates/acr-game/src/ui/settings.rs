//! Player options edited in the options screen (0x5c9740).

/// The original's option globals, with their ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `WarSound`: sound effect volume 0..=100.
    pub sound_volume: i32,
    /// `MidiSound`: music volume 0..=100.
    pub music_volume: i32,
    /// Music mode: 0 no music (`NOCD`), 1 random tracks (`RNDMUS`).
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
}

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
        }
    }
}

impl Settings {
    /// Effect and music gain for the mixer (0..=1).
    pub fn volumes(&self) -> (f32, f32) {
        let gain = |v: i32| (v.clamp(0, 100) as f32 / 100.0).powi(2);
        let music = if self.music_mode == 0 { 0.0 } else { gain(self.music_volume) };
        (gain(self.sound_volume), music)
    }
}
