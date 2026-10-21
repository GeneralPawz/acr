//! Audio: background music and sound effects through one shared mixer.
//!
//! Music: the tracks in `Music\` played one after another in shuffled order. Replaces the
//! original's separate `vopl.exe` player; the music belongs to the game process and ends
//! with it.

use std::collections::HashMap;

use acr_audio::output::AudioOutput;
use acr_audio::{MusicSource, Sound, VorbisMusic};
use acr_sim::Rng;

pub struct Audio {
    tracks: Vec<String>,
    next: usize,
    output: Option<AudioOutput>,
    /// A track was requested and has not arrived yet.
    pending: Option<String>,
    sounds: HashMap<String, Option<Sound>>,
    volumes: (f32, f32),
}

impl Audio {
    pub fn new(paths: &[String]) -> Self {
        // Top-level tracks only; `Music\Ger\` holds narration for the historical battles.
        let mut tracks: Vec<String> = paths
            .iter()
            .filter(|p| {
                let lower = p.to_ascii_lowercase().replace('/', "\\");
                lower.starts_with("music\\") && lower.ends_with(".ogg") && lower.matches('\\').count() == 1
            })
            .cloned()
            .collect();
        let seed =
            web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        let mut rng = Rng::new(seed, 1);
        for i in (1..tracks.len()).rev() {
            tracks.swap(i, rng.below(i as u32 + 1) as usize);
        }
        Self { tracks, next: 0, output: None, pending: None, sounds: HashMap::new(), volumes: (1.0, 0.7) }
    }

    /// Opens the audio device. In the browser this must run inside a user gesture.
    pub fn start(&mut self) {
        if self.output.is_some() {
            return;
        }
        match AudioOutput::start() {
            Ok(output) => {
                self.output = Some(output);
                self.set_volumes(self.volumes.0, self.volumes.1);
            }
            Err(e) => log::warn!("no audio: {e}"),
        }
    }

    pub fn stop(&mut self) {
        self.output = None;
    }

    /// True once the audio device is open.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn started(&self) -> bool {
        self.output.is_some()
    }

    /// Plays `source` (e.g. a movie soundtrack) instead of the playlist; `None` stops it
    /// and the playlist continues.
    pub fn set_music(&mut self, source: Option<Box<dyn MusicSource>>) {
        if let Some(Ok(mut mixer)) = self.output.as_ref().map(|o| o.mixer.lock()) {
            mixer.set_music(source);
        }
    }

    /// The track to load when nothing is playing; it is then expected in `on_loaded`.
    pub fn next_track(&mut self) -> Option<String> {
        let output = self.output.as_ref()?;
        if self.pending.is_some() || self.tracks.is_empty() {
            return None;
        }
        if output.mixer.lock().map(|m| m.music_playing()).unwrap_or(true) {
            return None;
        }
        let track = self.tracks[self.next % self.tracks.len()].clone();
        self.next += 1;
        self.pending = Some(track.clone());
        Some(track)
    }

    /// Takes a loaded file if it is the requested track; returns whether it was.
    pub fn on_loaded(&mut self, path: &str, bytes: &[u8]) -> bool {
        if self.pending.as_deref() != Some(path) {
            return false;
        }
        self.pending = None;
        match (VorbisMusic::new(bytes.to_vec()), &self.output) {
            (Ok(music), Some(output)) => {
                if let Ok(mut mixer) = output.mixer.lock() {
                    mixer.set_music(Some(Box::new(music)));
                }
            }
            (Err(e), _) => log::error!("{path}: {e}"),
            (Ok(_), None) => {}
        }
        true
    }

    /// Plays a sound effect from the bytes of its `.wav` file.
    pub fn play(&mut self, path: &str, bytes: &[u8]) {
        let Some(output) = &self.output else { return };
        let sound = self
            .sounds
            .entry(path.to_owned())
            .or_insert_with(|| Sound::parse(bytes).map_err(|e| log::error!("{path}: {e}")).ok());
        if let (Some(sound), Ok(mut mixer)) = (sound, output.mixer.lock()) {
            mixer.play(sound.clone(), 1.0, 0.0);
        }
    }

    /// Plays a sound effect at `volume` (0..=1, scaled by the effects volume) and `pan`
    /// (-1 left .. 1 right); returns its voice to check whether it still plays.
    pub fn play_at(&mut self, path: &str, bytes: &[u8], volume: f32, pan: f32) -> Option<acr_audio::VoiceId> {
        let output = self.output.as_ref()?;
        let sound = self
            .sounds
            .entry(path.to_owned())
            .or_insert_with(|| Sound::parse(bytes).map_err(|e| log::error!("{path}: {e}")).ok())
            .clone()?;
        let mut mixer = output.mixer.lock().ok()?;
        Some(mixer.play(sound, volume, pan))
    }

    /// Whether a voice started by [`Audio::play_at`] is still playing.
    pub fn is_playing(&self, voice: acr_audio::VoiceId) -> bool {
        self.output.as_ref().and_then(|o| o.mixer.lock().ok()).is_some_and(|m| m.is_playing(voice))
    }

    /// Sets the effect and music volumes (0..=1).
    pub fn set_volumes(&mut self, effects: f32, music: f32) {
        self.volumes = (effects, music);
        if let Some(output) = &self.output
            && let Ok(mut mixer) = output.mixer.lock()
        {
            mixer.sfx_volume = effects;
            mixer.music_volume = music;
        }
    }
}
