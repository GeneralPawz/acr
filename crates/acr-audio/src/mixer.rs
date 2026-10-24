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
    /// Starts again from the beginning at its end (a weather loop) instead of ending.
    looping: bool,
    /// Fade: the current level (0..=1), its target and the change per output frame.
    level: f32,
    target: f32,
    ramp: f32,
    /// The weather's ambience: ducks under the other sound effects ([`Mixer::ducking`]).
    ambient: bool,
}

/// How deep the ambience ducks under the battle: its gain is `1 / (1 + DUCK_DEPTH * level)`
/// with the other effects' RMS level, at least [`DUCK_FLOOR`]; the level follows at once and
/// falls back with [`DUCK_RELEASE`] (seconds, 1/e).
const DUCK_DEPTH: f32 = 8.0;
const DUCK_FLOOR: f32 = 0.25;
const DUCK_RELEASE: f32 = 1.2;

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
    /// Output meter since the last [`Mixer::take_meter`]: for the log, to tell silence
    /// from a device that never pulls samples.
    meter: Meter,
    /// The other effects' level the ambience ducks under, and the ambience's gain now.
    duck_level: f32,
    duck_gain: f32,
}

/// What the mixer rendered since the meter was last read.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Meter {
    /// Calls of [`Mixer::render`] (device callbacks).
    pub blocks: u32,
    /// Output frames rendered.
    pub frames: u64,
    /// Largest absolute output sample (after clamping).
    pub peak: f32,
    /// Sound effects started.
    pub started: u32,
}

impl Mixer {
    /// `rate` is the output sample rate.
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            voices: Vec::new(),
            next_id: 0,
            music: None,
            sfx_volume: 1.0,
            music_volume: 0.7,
            meter: Meter::default(),
            duck_level: 0.0,
            duck_gain: 1.0,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The output sample rate changed (the device was opened again).
    pub fn set_rate(&mut self, rate: u32) {
        self.rate = rate.max(1);
    }

    /// Starts a sound. `pan` ranges from -1 (left) to 1 (right).
    pub fn play(&mut self, sound: Sound, volume: f32, pan: f32) -> VoiceId {
        let id = VoiceId(self.next_id);
        self.next_id += 1;
        self.meter.started += 1;
        let pan = pan.clamp(-1.0, 1.0);
        let gain = [volume * (1.0 - pan.max(0.0)), volume * (1.0 + pan.min(0.0))];
        self.voices.push(Voice {
            id,
            sound,
            position: 0.0,
            gain,
            looping: false,
            level: 1.0,
            target: 1.0,
            ramp: 0.0,
            ambient: false,
        });
        id
    }

    /// Starts an ambience sound (a thunder roll, a drip): like [`Mixer::play`], but it ducks
    /// under the other effects.
    pub fn play_ambient(&mut self, sound: Sound, volume: f32, pan: f32) -> VoiceId {
        let id = self.play(sound, volume, pan);
        if let Some(v) = self.voices.last_mut() {
            v.ambient = true;
        }
        id
    }

    /// The ambience's gain under the other effects now (1: no ducking).
    pub fn ducking(&self) -> f32 {
        self.duck_gain
    }

    /// Starts a sound that loops until [`Mixer::fade_out`] or [`Mixer::stop`], fading in over
    /// `fade` seconds. The loop runs from the last frame straight into the first, so the
    /// sound should be made seamless. It is ambience: it ducks under the other effects.
    pub fn play_looped(&mut self, sound: Sound, volume: f32, fade: f32) -> VoiceId {
        let id = self.play_ambient(sound, volume, 0.0);
        let ramp = self.ramp(fade);
        if let Some(v) = self.voices.last_mut() {
            v.looping = true;
            v.level = if ramp < 1.0 { 0.0 } else { 1.0 };
            v.ramp = ramp;
        }
        id
    }

    /// Changes a playing voice's volume (both sides; the pan is reset).
    pub fn set_volume(&mut self, id: VoiceId, volume: f32) {
        if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
            v.gain = [volume; 2];
        }
    }

    /// Fades a voice out over `fade` seconds, then ends it.
    pub fn fade_out(&mut self, id: VoiceId, fade: f32) {
        let ramp = self.ramp(fade);
        if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
            v.target = 0.0;
            v.ramp = ramp;
        }
    }

    /// The level change per output frame of a fade over `secs` (1: at once).
    fn ramp(&self, secs: f32) -> f32 {
        if secs > 0.0 { (1.0 / (secs * self.rate as f32)).min(1.0) } else { 1.0 }
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

    /// The meter since the last call, then reset.
    pub fn take_meter(&mut self) -> Meter {
        std::mem::take(&mut self.meter)
    }

    /// Sound effects playing now.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// True while music is set and has not run out.
    pub fn music_playing(&self) -> bool {
        self.music.is_some()
    }

    /// Fills `out` (interleaved, `channels` per frame) with the mix, overwriting it.
    pub fn render(&mut self, out: &mut [f32], channels: u16) {
        self.mix(out, channels);
        out.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
        self.meter.blocks += 1;
        self.meter.frames += (out.len() / usize::from(channels.max(1))) as u64;
        self.meter.peak = out.iter().fold(self.meter.peak, |p, s| p.max(s.abs()));
    }

    fn mix(&mut self, out: &mut [f32], channels: u16) {
        out.fill(0.0);
        let channels = usize::from(channels.max(1));
        let frames = out.len() / channels;
        let rate = f64::from(self.rate);

        let sfx = self.sfx_volume;
        // The battle first, then the ambience ducked under it.
        let render = |voice: &mut Voice, out: &mut [f32], gain: (f32, f32)| -> bool {
            let step = f64::from(voice.sound.rate) / rate;
            let src_channels = usize::from(voice.sound.channels);
            let src_frames = voice.sound.frames();
            for frame in 0..frames {
                let mut index = voice.position as usize;
                if index + 1 >= src_frames {
                    if !voice.looping || src_frames < 2 {
                        return false;
                    }
                    voice.position -= (src_frames - 1) as f64;
                    index = voice.position as usize;
                }
                if voice.level != voice.target {
                    voice.level = if voice.level < voice.target {
                        (voice.level + voice.ramp).min(voice.target)
                    } else {
                        (voice.level - voice.ramp).max(voice.target)
                    };
                }
                let t = (voice.position - index as f64) as f32;
                // The gain ramps over the block (the ducking moves smoothly).
                let sfx = gain.0 + gain.1 * frame as f32 / frames as f32;
                for (c, slot) in out[frame * channels..][..channels].iter_mut().enumerate() {
                    let sc = c.min(src_channels - 1);
                    let a = f32::from(voice.sound.samples[index * src_channels + sc]);
                    let b = f32::from(voice.sound.samples[(index + 1) * src_channels + sc]);
                    *slot += (a + (b - a) * t) / 32768.0 * voice.gain[c.min(1)] * voice.level * sfx;
                }
                voice.position += step;
                if voice.level <= 0.0 && voice.target <= 0.0 {
                    return false;
                }
            }
            true
        };
        self.voices.retain_mut(|voice| voice.ambient || render(voice, out, (sfx, 0.0)));
        if frames > 0 {
            let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
            let fall = (-(frames as f32) / (DUCK_RELEASE * rate as f32)).exp();
            self.duck_level = rms.max(self.duck_level * fall);
        }
        let from = self.duck_gain;
        let to = (1.0 / (1.0 + DUCK_DEPTH * self.duck_level)).max(DUCK_FLOOR);
        self.duck_gain = to;
        self.voices.retain_mut(|voice| !voice.ambient || render(voice, out, (sfx * from, sfx * (to - from))));

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
    fn meter_counts_blocks_and_peak() {
        let mut m = Mixer::new(100);
        m.play(sound(100, &[16384, -24576, 0]), 1.0, 0.0);
        let mut out = [0.0; 2];
        m.render(&mut out, 1);
        m.render(&mut out, 1);
        let meter = m.take_meter();
        assert_eq!((meter.blocks, meter.frames, meter.peak, meter.started), (2, 4, 0.75, 1));
        assert_eq!(m.take_meter(), Meter::default());
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

    #[test]
    fn loops_until_faded_out() {
        let mut m = Mixer::new(100);
        let id = m.play_looped(sound(100, &[0, 16384, 0]), 1.0, 0.0);
        let mut out = [0.0; 6];
        m.render(&mut out, 1);
        // Frames 0, 1 and back to 0 (the last frame is the first again): a seamless cycle.
        assert_eq!(out, [0.0, 0.5, 0.0, 0.5, 0.0, 0.5]);
        assert!(m.is_playing(id));
        m.set_volume(id, 0.5);
        m.render(&mut out, 1);
        assert_eq!(out[1], 0.25);
        m.fade_out(id, 0.04);
        m.render(&mut out, 1);
        assert_eq!(out[1], 0.125);
        assert!(!m.is_playing(id));
    }

    #[test]
    fn fades_in() {
        let mut m = Mixer::new(100);
        m.play_looped(sound(100, &[16384, 16384, 16384]), 1.0, 0.04);
        let mut out = [0.0; 4];
        m.render(&mut out, 1);
        assert_eq!(out, [0.125, 0.25, 0.375, 0.5]);
    }

    #[test]
    fn ambience_ducks_under_the_battle() {
        let mut m = Mixer::new(100);
        let rain = m.play_looped(sound(100, &[8192, 8192, 8192]), 1.0, 0.0);
        let mut out = [0.0; 10];
        m.render(&mut out, 1);
        assert_eq!(m.ducking(), 1.0);
        assert_eq!(out[9], 0.25);
        // A loud shot: the rain ducks, smoothly over the block, then comes back.
        m.play(sound(100, &[16384; 12]), 1.0, 0.0);
        m.render(&mut out, 1);
        let ducked = m.ducking();
        assert!(ducked < 0.3, "{ducked}");
        assert!(out[0] > out[9]);
        for _ in 0..60 {
            m.render(&mut out, 1);
        }
        assert!(m.ducking() > 0.9 && m.is_playing(rain), "{}", m.ducking());
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
