//! The weather's ambience (#224, ACR; the original has no weather): a rain loop with
//! occasional thunder, a wind loop in rain and snow, nothing in fog or clear weather.
//!
//! The sounds are made here from filtered noise (no recorded clips, so nothing to license):
//! loops with their ends crossfaded into their starts, a few thunder rolls. They play as
//! effects, so the effects volume of the options scales them. They follow the weather that
//! is drawn ([`crate::ingame::InGame::weather_heard`]): start and stop with the mode, fading
//! over [`FADE`] seconds, and stop when the game is left.

use acr_audio::{Sound, VoiceId};
use acr_render::weather::Weather;

use crate::music::Audio;

/// Sample rate of the made sounds.
const RATE: u32 = 22050;
/// Fade in and out of the loops when the weather changes, seconds.
const FADE: f32 = 2.0;
/// Seconds between thunder rolls in rain: at least the first, plus up to the second.
const THUNDER_GAP: (f32, f32) = (14.0, 30.0);
/// Loop volumes (before the effects volume): rain, wind in rain, wind in snow.
const RAIN_VOLUME: f32 = 0.45;
const WIND_RAIN_VOLUME: f32 = 0.18;
const WIND_SNOW_VOLUME: f32 = 0.35;

/// The made sounds, built on first use.
struct Sounds {
    rain: Sound,
    wind: Sound,
    thunder: Vec<Sound>,
}

#[derive(Default)]
pub struct WeatherSound {
    /// The weather the loops play for.
    mode: Weather,
    loops: Vec<VoiceId>,
    /// Seconds to the next thunder roll.
    thunder_in: f32,
    rng: Noise,
    sounds: Option<Sounds>,
}

impl WeatherSound {
    /// Once a frame, `dt` seconds after the last: `weather` is what the game draws (off
    /// outside a game).
    pub fn update(&mut self, weather: Weather, dt: f32, audio: &mut Audio) {
        let wanted = loops(weather);
        if weather != self.mode {
            log::info!("weather sound: {}", weather.name());
            for voice in self.loops.drain(..) {
                audio.fade_out(voice, FADE);
            }
            self.mode = weather;
            self.thunder_in = THUNDER_GAP.0 * 0.5;
        }
        // (Re)start the loops while the device is open (it may open later, or reopen).
        if !wanted.is_empty() && self.loops.is_empty() && audio.started() {
            let sounds = self.sounds.get_or_insert_with(make_sounds);
            for &(kind, volume) in wanted {
                let sound = if kind == Loop::Rain { &sounds.rain } else { &sounds.wind };
                self.loops.extend(audio.play_loop(sound.clone(), volume, FADE));
            }
        }
        if weather == Weather::Rain && audio.started() {
            self.thunder_in -= dt;
            if self.thunder_in <= 0.0 {
                self.thunder_in = THUNDER_GAP.0 + self.rng.unit() * THUNDER_GAP.1;
                let sounds = self.sounds.get_or_insert_with(make_sounds);
                let roll =
                    &sounds.thunder[(self.rng.unit() * sounds.thunder.len() as f32) as usize % sounds.thunder.len()];
                let volume = 0.35 + 0.45 * self.rng.unit();
                let pan = self.rng.unit() * 1.2 - 0.6;
                audio.play_sound(roll.clone(), volume, pan);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Loop {
    Rain,
    Wind,
}

/// The loops of a weather and their volumes.
fn loops(weather: Weather) -> &'static [(Loop, f32)] {
    match weather {
        Weather::Rain => &[(Loop::Rain, RAIN_VOLUME), (Loop::Wind, WIND_RAIN_VOLUME)],
        Weather::Snow => &[(Loop::Wind, WIND_SNOW_VOLUME)],
        Weather::Off | Weather::Fog => &[],
    }
}

fn make_sounds() -> Sounds {
    let mut noise = Noise(0x5eed_cafe_f00d_1234);
    Sounds { rain: rain(&mut noise), wind: wind(&mut noise), thunder: (0..3).map(|i| thunder(&mut noise, i)).collect() }
}

/// A seeded white-noise source (xorshift64*); `Default` gives a fixed seed.
struct Noise(u64);

impl Default for Noise {
    fn default() -> Self {
        Noise(0x9e37_79b9_7f4a_7c15)
    }
}

impl Noise {
    /// Uniform in 0..1.
    fn unit(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in -1..1.
    fn white(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

/// One-pole low-pass coefficient for a cutoff of `hz`.
fn pole(hz: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz / RATE as f32).exp()
}

/// Rain, 6 s: hiss (noise band-passed to about 0.8–7 kHz) under many small drop ticks.
fn rain(noise: &mut Noise) -> Sound {
    let n = (RATE * 6) as usize;
    let fade = RATE as usize / 2;
    let (lo, hi) = (pole(800.0), pole(7000.0));
    let (mut low, mut high) = (0.0f32, 0.0f32);
    let mut drop = 0.0f32;
    let samples: Vec<f32> = (0..n + fade)
        .map(|_| {
            let w = noise.white();
            high += hi * (w - high);
            low += lo * (high - low);
            let hiss = high - low;
            // A drop: a short tick of noise, about 60 a second, decaying in about 4 ms.
            if noise.unit() < 60.0 / RATE as f32 {
                drop = 0.4 + 0.6 * noise.unit();
            }
            drop *= 0.988;
            hiss * 0.8 + drop * w * 0.5
        })
        .collect();
    finish(looped(samples, fade))
}

/// Wind, 8 s: low noise (about 120–500 Hz) swelling in gusts that repeat with the loop.
fn wind(noise: &mut Noise) -> Sound {
    let n = (RATE * 8) as usize;
    let fade = RATE as usize;
    let (mut a, mut b) = (0.0f32, 0.0f32);
    let mut low = 0.0f32;
    let samples: Vec<f32> = (0..n + fade)
        .map(|i| {
            let t = (i % n) as f32 / n as f32 * std::f32::consts::TAU;
            // Gusts: slow swells with periods of 8, 4 and 8/3 s, so the loop repeats them.
            let gust = 0.55 + 0.25 * (t + 0.7).sin() + 0.15 * (2.0 * t + 2.1).sin() + 0.08 * (3.0 * t).sin();
            let cutoff = 150.0 + 350.0 * gust;
            let w = noise.white();
            a += pole(cutoff) * (w - a);
            b += pole(cutoff) * (a - b);
            low += pole(120.0) * (b - low);
            (b - low * 0.6) * gust
        })
        .collect();
    finish(looped(samples, fade))
}

/// A thunder roll, 5–7 s: a crack, then a low rumble with a few swells, dying away.
fn thunder(noise: &mut Noise, variant: u32) -> Sound {
    let secs = 5.0 + variant as f32;
    let n = (secs * RATE as f32) as usize;
    let swells: Vec<(f32, f32)> =
        (0..3).map(|_| (0.3 + noise.unit() * (secs - 2.0), 0.4 + noise.unit() * 0.6)).collect();
    let (mut a, mut b, mut crack) = (0.0f32, 0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let w = noise.white();
            a += pole(180.0) * (w - a);
            b += pole(90.0) * (a - b);
            crack += pole(2500.0) * (w - crack);
            let attack = (t / 0.05).min(1.0);
            let decay = (-t / (secs * 0.35)).exp();
            let swell: f32 = swells.iter().map(|&(at, gain)| gain * (-((t - at) / 0.5).powi(2)).exp()).sum();
            let crack_env = (-t / 0.25).exp() * if variant == 0 { 1.0 } else { 0.5 };
            b * attack * (decay + swell) * 6.0 + crack * crack_env * 0.6
        })
        .collect();
    finish(samples)
}

/// `samples` (its last `fade` frames running on past the loop) made into a seamless loop:
/// the overrun is crossfaded into the start, so the last frame leads into the first.
fn looped(mut samples: Vec<f32>, fade: usize) -> Vec<f32> {
    let n = samples.len() - fade;
    for i in 0..fade {
        let k = i as f32 / fade as f32;
        samples[i] = samples[i] * k.sqrt() + samples[n + i] * (1.0 - k).sqrt();
    }
    samples.truncate(n);
    samples
}

/// Normalised to 0.8 of full scale, as 16-bit mono.
fn finish(samples: Vec<f32>) -> Sound {
    let peak = samples.iter().fold(1e-6f32, |p, s| p.max(s.abs()));
    let scale = 0.8 * 32767.0 / peak;
    let pcm: Vec<i16> = samples.iter().map(|s| (s * scale) as i16).collect();
    Sound { rate: RATE, channels: 1, samples: pcm.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_loud_enough_and_loops_close() {
        let s = make_sounds();
        for (name, sound) in [("rain", &s.rain), ("wind", &s.wind)] {
            let x = &sound.samples;
            let peak = x.iter().map(|v| i32::from(*v).abs()).max().unwrap();
            assert!(peak > 20000, "{name} peak {peak}");
            // The step from the last frame into the first is no bigger than typical steps.
            let steps: f32 = x.windows(2).map(|w| (i32::from(w[1]) - i32::from(w[0])).abs() as f32).sum::<f32>()
                / (x.len() - 1) as f32;
            let seam = (i32::from(x[0]) - i32::from(x[x.len() - 1])).abs() as f32;
            assert!(seam < steps * 6.0, "{name} seam {seam} vs mean step {steps}");
        }
        assert_eq!(s.thunder.len(), 3);
    }

    #[test]
    fn weather_picks_its_loops() {
        assert_eq!(loops(Weather::Off).len(), 0);
        assert_eq!(loops(Weather::Fog).len(), 0);
        assert!(loops(Weather::Rain).iter().any(|l| l.0 == Loop::Rain));
        assert!(loops(Weather::Snow).iter().all(|l| l.0 == Loop::Wind));
    }
}
