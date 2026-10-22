//! Mixes sound effects and one music stream into interleaved `f32` output.
//!
//! Everything here is plain computation so it can be unit tested and run inside any
//! audio callback. Sources at other sample rates are resampled linearly.

use crate::music::MusicSource;
use crate::wav::Sound;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VoiceId(u64);

struct Voice {
    id: VoiceId,
    sound: Sound,
    /// Position in source frames, fractional for resampling.
    position: f64,
    gain: [f32; 2],
}

struct MusicStream {
    source: Box<dyn MusicSource>,
    rate: u32,
    channels: u16,
    block: Vec<i16>,
    position: f64,
}

pub struct Mixer {
    rate: u32,
    voices: Vec<Voice>,
    next_id: u64,
    music: Option<MusicStream>,
    pub sfx_volume: f32,
    pub music_volume: f32,
    /// Loudness (RMS, 0..=1) of the sound effects in the last rendered block, after
    /// `sfx_volume`, without the music: for ducking other applications' music.
    pub sfx_level: f32,
}

impl Mixer {
    /// `rate` is the output sample rate.
    pub fn new(rate: u32) -> Self {
        Self { rate, voices: Vec::new(), next_id: 0, music: None, sfx_volume: 1.0, music_volume: 0.7, sfx_level: 0.0 }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Starts a sound. `pan` ranges from -1 (left) to 1 (right).
    pub fn play(&mut self, sound: Sound, volume: f32, pan: f32) -> VoiceId {
        let id = VoiceId(self.next_id);
        self.next_id += 1;
        let pan = pan.clamp(-1.0, 1.0);
        let gain = [volume * (1.0 - pan.max(0.0)), volume * (1.0 + pan.min(0.0))];
        self.voices.push(Voice { id, sound, position: 0.0, gain });
        id
    }

    pub fn stop(&mut self, id: VoiceId) {
        self.voices.retain(|v| v.id != id);
    }

    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.voices.iter().any(|v| v.id == id)
    }

    pub fn has_voices(&self) -> bool {
        !self.voices.is_empty()
    }

    /// Replaces the current music; `None` stops it.
    pub fn set_music(&mut self, source: Option<Box<dyn MusicSource>>) {
        self.music = source.map(|source| {
            let (rate, channels) = source.format();
            MusicStream { source, rate, channels: channels.max(1), block: Vec::new(), position: 0.0 }
        });
    }

    /// True while music is set and has not run out.
    pub fn music_playing(&self) -> bool {
        self.music.is_some()
    }

    /// Fills `out` (interleaved, `channels` per frame) with the mix, overwriting it.
    pub fn render(&mut self, out: &mut [f32], channels: u16) {
        out.fill(0.0);
        let channels = usize::from(channels.max(1));
        let frames = out.len() / channels;
        let rate = f64::from(self.rate);

        let sfx = self.sfx_volume;
        self.voices.retain_mut(|voice| {
            let step = f64::from(voice.sound.rate) / rate;
            let src_channels = usize::from(voice.sound.channels);
            let src_frames = voice.sound.frames();
            for frame in 0..frames {
                let index = voice.position as usize;
                if index + 1 >= src_frames {
                    return false;
                }
                let t = (voice.position - index as f64) as f32;
                for (c, slot) in out[frame * channels..][..channels].iter_mut().enumerate() {
                    let sc = c.min(src_channels - 1);
                    let a = f32::from(voice.sound.samples[index * src_channels + sc]);
                    let b = f32::from(voice.sound.samples[(index + 1) * src_channels + sc]);
                    *slot += (a + (b - a) * t) / 32768.0 * voice.gain[c.min(1)] * sfx;
                }
                voice.position += step;
            }
            true
        });
        let energy: f32 = out.iter().map(|s| s * s).sum();
        self.sfx_level = if out.is_empty() { 0.0 } else { (energy / out.len() as f32).sqrt().min(1.0) };

        if let Some(music) = &mut self.music {
            let step = f64::from(music.rate) / rate;
            let src_channels = usize::from(music.channels);
            for frame in 0..frames {
                // Keep the current and next frame available for interpolation.
                while (music.position as usize + 1) * src_channels >= music.block.len() {
                    let consumed = (music.position as usize).min(music.block.len() / src_channels);
                    match music.source.next_block() {
                        Some(next) => {
                            music.block.drain(..consumed * src_channels);
                            music.position -= consumed as f64;
                            music.block.extend(next);
                        }
                        None => {
                            self.music = None;
                            return;
                        }
                    }
                }
                let index = music.position as usize;
                let t = (music.position - index as f64) as f32;
                for (c, slot) in out[frame * channels..][..channels].iter_mut().enumerate() {
                    let sc = c.min(src_channels - 1);
                    let a = f32::from(music.block[index * src_channels + sc]);
                    let b = f32::from(music.block[(index + 1) * src_channels + sc]);
                    *slot += (a + (b - a) * t) / 32768.0 * self.music_volume;
                }
                music.position += step;
            }
        }
        out.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn sound(rate: u32, samples: &[i16]) -> Sound {
        Sound { rate, channels: 1, samples: Arc::from(samples) }
    }

    #[test]
    fn plays_at_output_rate_and_ends() {
        let mut m = Mixer::new(100);
        let id = m.play(sound(100, &[16384, 16384, 16384]), 1.0, 0.0);
        let mut out = [0.0; 4];
        m.render(&mut out, 2);
        assert_eq!(out, [0.5, 0.5, 0.5, 0.5]);
        m.render(&mut out, 2);
        assert!(!m.is_playing(id));
    }

    #[test]
    fn resamples_linearly() {
        let mut m = Mixer::new(200);
        m.play(sound(100, &[0, 16384, 16384]), 1.0, 0.0);
        let mut out = [0.0; 3];
        m.render(&mut out, 1);
        assert_eq!(out, [0.0, 0.25, 0.5]);
    }

    #[test]
    fn pans_hard_left() {
        let mut m = Mixer::new(100);
        m.play(sound(100, &[16384, 16384]), 1.0, -1.0);
        let mut out = [0.0; 2];
        m.render(&mut out, 2);
        assert_eq!(out, [0.5, 0.0]);
    }

    struct Blocks(Vec<Vec<i16>>);

    impl MusicSource for Blocks {
        fn format(&self) -> (u32, u16) {
            (100, 1)
        }
        fn next_block(&mut self) -> Option<Vec<i16>> {
            (!self.0.is_empty()).then(|| self.0.remove(0))
        }
    }

    #[test]
    fn music_spans_blocks_and_stops_at_end() {
        let mut m = Mixer::new(100);
        m.music_volume = 1.0;
        m.set_music(Some(Box::new(Blocks(vec![vec![8192, 8192], vec![16384, 16384]]))));
        let mut out = [0.0; 3];
        m.render(&mut out, 1);
        assert_eq!(out, [0.25, 0.25, 0.5]);
        m.render(&mut out, 1);
        assert!(!m.music_playing());
    }
}
