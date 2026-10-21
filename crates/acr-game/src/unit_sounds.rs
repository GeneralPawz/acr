//! Sound effects of units: the `SOUNDLIST.TXT` groups their definitions name.
//!
//! The original plays a group through `PlayEffect` (0x5863c4): it starts at a random
//! variant and takes the first one whose buffer is not playing, so a busy group does not
//! stack copies of the same sample; if every variant plays, nothing is started. Volume
//! and pan are DirectSound values from the caller. TODO: the callers that place sounds
//! in the world (pan and volume from the screen position) are not traced yet; positions
//! use a linear pan and fade towards the screen edges.

use std::collections::HashMap;

use acr_audio::VoiceId;
use acr_formats::SoundList;
use acr_formats::normalize_path;
use acr_sim::Rng;

use crate::music::Audio;

pub const SOUND_LIST: &str = "SOUNDLIST.TXT";

/// A sound to play: group name from `SOUNDLIST.TXT`, pan (-1 left .. 1 right), volume 0..=1.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEvent {
    pub name: String,
    pub pan: f32,
    pub volume: f32,
}

pub struct UnitSounds {
    list: Option<SoundList>,
    /// Sample bytes by normalized path; `None` while loading.
    samples: HashMap<String, Option<Vec<u8>>>,
    /// The voice last started per sample, to skip samples that are still playing.
    voices: HashMap<String, VoiceId>,
    rng: Rng,
    requests: Vec<String>,
}

impl Default for UnitSounds {
    fn default() -> Self {
        Self::new()
    }
}

impl UnitSounds {
    pub fn new() -> Self {
        Self {
            list: None,
            samples: HashMap::new(),
            voices: HashMap::new(),
            rng: Rng::new(0x5a1d, 3),
            requests: vec![SOUND_LIST.to_owned()],
        }
    }

    /// Files to load; deliver them with [`UnitSounds::on_loaded`].
    pub fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.requests)
    }

    /// Takes a loaded file if it is the sound list or one of its samples.
    pub fn on_loaded(&mut self, path: &str, bytes: &[u8]) -> bool {
        if path == SOUND_LIST {
            let list = SoundList::parse(bytes);
            for sound in &list.sounds {
                for file in &sound.files {
                    if self.samples.insert(normalize_path(file), None).is_none() {
                        self.requests.push(file.clone());
                    }
                }
            }
            self.list = Some(list);
            return true;
        }
        match self.samples.get_mut(&normalize_path(path)) {
            Some(slot @ None) => {
                *slot = Some(bytes.to_vec());
                true
            }
            _ => false,
        }
    }

    /// Plays one variant of each event's group like the original's `PlayEffect`.
    pub fn play(&mut self, events: &[SoundEvent], audio: &mut Audio) {
        for event in events {
            let Some(sound) = self.list.as_ref().and_then(|l| l.get(&event.name)) else { continue };
            let n = sound.files.len();
            if n == 0 {
                continue;
            }
            let start = self.rng.below(n as u32) as usize;
            for k in 0..n {
                let file = &sound.files[(start + k) % n];
                let key = normalize_path(file);
                if self.voices.get(&key).is_some_and(|v| audio.is_playing(*v)) {
                    continue;
                }
                let Some(Some(bytes)) = self.samples.get(&key) else { break };
                if let Some(voice) = audio.play_at(&key, bytes, event.volume, event.pan) {
                    log::debug!("sound {} -> {key} (pan {:.2}, volume {:.2})", event.name, event.pan, event.volume);
                    self.voices.insert(key, voice);
                }
                break;
            }
        }
    }
}
