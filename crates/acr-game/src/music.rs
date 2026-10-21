//! Background music: the tracks in `Music\` played one after another in shuffled order.
//!
//! Replaces the original's separate `vopl.exe` player; the music belongs to the game
//! process and ends with it.

use acr_audio::VorbisMusic;
use acr_audio::output::AudioOutput;
use acr_sim::Rng;

pub struct Playlist {
    tracks: Vec<String>,
    next: usize,
    output: Option<AudioOutput>,
    /// A track was requested and has not arrived yet.
    pending: Option<String>,
}

impl Playlist {
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
        Self { tracks, next: 0, output: None, pending: None }
    }

    /// Opens the audio device. In the browser this must run inside a user gesture.
    pub fn start(&mut self) {
        if self.output.is_some() {
            return;
        }
        match AudioOutput::start() {
            Ok(output) => self.output = Some(output),
            Err(e) => log::warn!("no audio: {e}"),
        }
    }

    pub fn stop(&mut self) {
        self.output = None;
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
}
