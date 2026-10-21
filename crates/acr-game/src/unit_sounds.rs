//! Sound effects of units: the `SOUNDLIST.TXT` groups their definitions name.
//!
//! The original plays a group through `PlayEffect` (0x5863c4): it starts at a random
//! variant and takes the first one whose buffer is not playing, so a busy group does not
//! stack copies of the same sample; if every variant plays, nothing is started. Sounds in
//! the world go through 0x5868d3, see [`placed`].

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

/// Original pixels around the view in which world sounds are still heard (0x586807):
/// 1024 left and right, 512 above and below.
const HEAR_MARGIN: [f32; 2] = [1024.0, 512.0];
/// DirectSound pan limit for world sounds (hundredths of a dB the far channel loses).
const MAX_PAN: f32 = 4000.0;
/// DirectSound volume of a sound outside the view but within the margin (-8 dB).
const OFF_SCREEN_VOLUME: f32 = -800.0;

/// Volume (0..=1) and pan (-1..=1) of a sound at `(x, y)` in original screen pixels
/// (`x` = world x, `y` = world y / 2) for the view `[left, top, width, height]`, or
/// `None` if it is too far away to be heard.
///
/// The original (0x5868d3) skips sounds beyond [`HEAR_MARGIN`], plays sounds in the view
/// at full volume and the others at -8 dB, and pans by twice the distance from the view
/// centre, clamped to ±4000. TODO: it also skips sounds on cells the player has not
/// explored when fog of war is on (`0x775e18`, #33).
pub fn placed(x: f32, y: f32, view: [f32; 4]) -> Option<(f32, f32)> {
    let [left, top, width, height] = view;
    let (right, bottom) = (left + width, top + height);
    if x < left - HEAR_MARGIN[0]
        || x > right + HEAR_MARGIN[0]
        || y < top - HEAR_MARGIN[1]
        || y > bottom + HEAR_MARGIN[1]
    {
        return None;
    }
    let pan = (2.0 * (x - (left + right) / 2.0)).clamp(-MAX_PAN, MAX_PAN);
    let on_screen = (left..right).contains(&x) && (top..bottom).contains(&y);
    let volume = if on_screen { 0.0 } else { OFF_SCREEN_VOLUME };
    Some((gain(volume), (1.0 - gain(-pan.abs())).copysign(pan)))
}

/// Amplitude of a DirectSound attenuation in hundredths of a dB.
fn gain(hundredths_db: f32) -> f32 {
    10f32.powf(hundredths_db / 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: [f32; 4] = [1000.0, 500.0, 1024.0, 768.0];

    #[test]
    fn centre_is_full_volume_and_unpanned() {
        assert_eq!(placed(1512.0, 884.0, VIEW), Some((1.0, 0.0)));
    }

    #[test]
    fn pans_by_distance_from_the_centre() {
        // 250 px right of the centre: pan 500, the left channel loses 5 dB.
        let (volume, pan) = placed(1762.0, 600.0, VIEW).unwrap();
        assert_eq!(volume, 1.0);
        assert!((pan - (1.0 - 10f32.powf(-0.25))).abs() < 1e-6);
        let (_, left) = placed(1262.0, 600.0, VIEW).unwrap();
        assert_eq!(left, -pan);
    }

    #[test]
    fn off_screen_sounds_are_quieter_until_out_of_range() {
        // 1488 px right of the centre, off screen: -8 dB, the left channel loses 29.76 dB.
        let (volume, pan) = placed(3000.0, 600.0, VIEW).unwrap();
        assert!((volume - 10f32.powf(-0.4)).abs() < 1e-6);
        assert!((pan - (1.0 - 10f32.powf(-1.488))).abs() < 1e-6);
        // A wide view reaches the pan limit: the far channel loses 40 dB.
        let (_, pan) = placed(4600.0, 600.0, [0.0, 0.0, 5120.0, 1440.0]).unwrap();
        assert!((pan - 0.99).abs() < 1e-6);
        assert!(placed(1000.0 - 1024.0, 500.0 - 512.0, VIEW).is_some());
        assert_eq!(placed(1000.0 - 1025.0, 600.0, VIEW), None);
        assert_eq!(placed(1500.0, 500.0 + 768.0 + 513.0, VIEW), None);
    }
}
