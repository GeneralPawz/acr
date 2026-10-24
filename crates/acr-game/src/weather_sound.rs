//! The weather's ambience (#224, ACR; the original has no weather; docs/rendering-3d.md
//! "Weather sound"):
//!
//! - rain: a light and a heavy rain loop crossfaded by the rain's strength (the drops drawn
//!   follow the same [`acr_render::weather::rain_intensity`]), softer zoomed out; drips and
//!   close drops at a close zoom; wind and a howl in the gusts; rolling thunder a moment after
//!   each lightning flash ([`acr_render::weather::strike`]: the flash is drawn at `at`, the
//!   roll starts `delay` seconds later, louder and with a crack when near);
//! - snow: only the wind, muffled;
//! - fog: a very quiet distant rumble;
//! - the wind swells with the gusts that sway the trees ([`acr_render::weather::gust`] at the
//!   window centre on the same clock, times the weather's wind strength; none with `wind=off`).
//!
//! The sounds are made here from filtered, seeded noise (no recorded clips, nothing to
//! license): loops with their ends crossfaded into their starts, thunder rolls and drips.
//! They play as ambience in the effects volume: the mixer ducks them under the battle's
//! sounds (`acr_audio::Mixer::play_ambient`). They follow the weather drawn
//! ([`crate::ingame::InGame::weather_heard`]), fading over [`FADE`] seconds when it changes,
//! and stop when the game is left. Render side only: the lightning and the rain's strength
//! are functions of the wind clock, the drips use their own seeded noise; the simulation is
//! never read for them or touched.

use acr_audio::{Sound, VoiceId};
use acr_render::weather::{STRIKE_SLOT, Weather, strike};

use crate::music::Audio;

/// Sample rate of the made sounds.
const RATE: u32 = 22050;
/// Fade in and out of the loops when the weather changes, seconds.
const FADE: f32 = 2.0;
/// Loop volumes at full strength (before the effects volume and the ducking).
const RAIN_VOLUME: f32 = 0.5;
const WIND_RAIN_VOLUME: f32 = 0.22;
const HOWL_VOLUME: f32 = 0.12;
const WIND_SNOW_VOLUME: f32 = 0.4;
const DISTANT_VOLUME: f32 = 0.09;
/// Rain zoomed all the way out, relative to close up.
const RAIN_FAR: f32 = 0.4;
/// Drips: from this nearness on (0..1), at most this many a second at full strength and
/// nearness, at this volume.
const DRIP_NEAR: f32 = 0.5;
const DRIP_RATE: f32 = 3.0;
const DRIP_VOLUME: f32 = 0.22;
/// A thunder roll's volume: far, plus this much more when near.
const THUNDER_VOLUME: (f32, f32) = (0.5, 0.5);

/// What the weather sound hears in a frame ([`crate::ingame::InGame::weather_heard`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Heard {
    pub weather: Weather,
    /// The wind clock (`ThreeD::wind_clock`), seconds.
    pub clock: f32,
    /// The rain's strength (0.4..1; 1 outside rain).
    pub intensity: f32,
    /// The trees' wind at the window centre: the weather's wind strength times the gust
    /// (`ThreeD::wind_at`); 0 with `wind=off`.
    pub wind: f32,
    /// How close the camera is (0: zoomed out .. 1: close up, [`nearness`]).
    pub near: f32,
}

impl Default for Heard {
    fn default() -> Self {
        Heard { weather: Weather::Off, clock: 0.0, intensity: 1.0, wind: 0.0, near: 0.4 }
    }
}

/// The nearness of a camera scale: 0 at half scale and below, 1/3 at the normal 1, 1 from
/// scale 4 (a close-up).
pub fn nearness(scale: f32) -> f32 {
    ((scale.max(1e-3).log2() + 1.0) / 3.0).clamp(0.0, 1.0)
}

/// Where the sounds go: the game's [`Audio`], or a bare mixer (the captures in the tests).
pub trait Sink {
    fn started(&self) -> bool;
    fn play_loop(&mut self, sound: Sound, volume: f32, fade: f32) -> Option<VoiceId>;
    fn set_volume(&mut self, voice: VoiceId, volume: f32);
    fn fade_out(&mut self, voice: VoiceId, fade: f32);
    fn play_once(&mut self, sound: Sound, volume: f32, pan: f32);
}

impl Sink for Audio {
    fn started(&self) -> bool {
        Audio::started(self)
    }
    fn play_loop(&mut self, sound: Sound, volume: f32, fade: f32) -> Option<VoiceId> {
        Audio::play_loop(self, sound, volume, fade)
    }
    fn set_volume(&mut self, voice: VoiceId, volume: f32) {
        Audio::set_volume(self, voice, volume);
    }
    fn fade_out(&mut self, voice: VoiceId, fade: f32) {
        Audio::fade_out(self, voice, fade);
    }
    fn play_once(&mut self, sound: Sound, volume: f32, pan: f32) {
        self.play_sound(sound, volume, pan);
    }
}

/// The made sounds, built on first use.
struct Sounds {
    rain_light: Sound,
    rain_heavy: Sound,
    wind: Sound,
    howl: Sound,
    wind_muffled: Sound,
    distant: Sound,
    thunder_near: Vec<Sound>,
    thunder_far: Vec<Sound>,
    drips: Vec<Sound>,
}

#[derive(Default)]
pub struct WeatherSound {
    /// The weather the loops play for.
    mode: Weather,
    loops: Vec<(Loop, VoiceId)>,
    /// The wind clock of the last frame (thunder is due when the clock passes it).
    last_clock: Option<f32>,
    /// Seconds to the next drip.
    drip_in: f32,
    rng: Noise,
    sounds: Option<Sounds>,
}

impl WeatherSound {
    /// Once a frame, `dt` seconds after the last.
    pub fn update(&mut self, heard: Heard, dt: f32, audio: &mut impl Sink) {
        let weather = heard.weather;
        if weather != self.mode {
            log::info!("weather sound: {}", weather.name());
            for (_, voice) in self.loops.drain(..) {
                audio.fade_out(voice, FADE);
            }
            self.mode = weather;
            self.last_clock = None;
        }
        if !audio.started() {
            return;
        }
        let wanted = loops(weather);
        if self.loops.is_empty() && !wanted.is_empty() {
            // (Re)start the loops while the device is open (it may open later, or reopen).
            let sounds = self.sounds.get_or_insert_with(make_sounds);
            for &kind in wanted {
                if let Some(voice) = audio.play_loop(sounds.of(kind).clone(), level(kind, &heard), FADE) {
                    self.loops.push((kind, voice));
                }
            }
        } else {
            for &(kind, voice) in &self.loops {
                audio.set_volume(voice, level(kind, &heard));
            }
        }
        let last = self.last_clock.replace(heard.clock);
        if weather != Weather::Rain {
            return;
        }
        // Thunder whose roll starts between the last frame's clock and this one's.
        if let Some(last) = last.filter(|&l| l <= heard.clock) {
            let first = ((last - 8.0) / STRIKE_SLOT).floor() as i64;
            for slot in first..=(heard.clock / STRIKE_SLOT).floor() as i64 {
                let Some(s) = strike(slot) else { continue };
                let due = s.at + s.delay;
                if last < due && due <= heard.clock {
                    let sounds = self.sounds.get_or_insert_with(make_sounds);
                    let rolls = if s.near > 0.6 { &sounds.thunder_near } else { &sounds.thunder_far };
                    let roll = &rolls[slot.rem_euclid(rolls.len() as i64) as usize];
                    let volume = THUNDER_VOLUME.0 + THUNDER_VOLUME.1 * s.near;
                    audio.play_once(roll.clone(), volume, s.pan);
                }
            }
        }
        // Drips and close drops near the camera.
        let close = ((heard.near - DRIP_NEAR) / (1.0 - DRIP_NEAR)).clamp(0.0, 1.0);
        if close <= 0.0 {
            self.drip_in = 0.5;
            return;
        }
        self.drip_in -= dt;
        if self.drip_in <= 0.0 {
            let rate = DRIP_RATE * heard.intensity * close;
            // Exponential gaps: a random, not a ticking, pattern.
            self.drip_in = -(1.0 - self.rng.unit()).max(1e-4).ln() / rate.max(0.05);
            let sounds = self.sounds.get_or_insert_with(make_sounds);
            let drip = &sounds.drips[(self.rng.unit() * sounds.drips.len() as f32) as usize % sounds.drips.len()];
            let volume = DRIP_VOLUME * close * (0.4 + 0.6 * self.rng.unit());
            audio.play_once(drip.clone(), volume, self.rng.white() * 0.8);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Loop {
    RainLight,
    RainHeavy,
    Wind,
    Howl,
    WindMuffled,
    Distant,
}

impl Sounds {
    fn of(&self, kind: Loop) -> &Sound {
        match kind {
            Loop::RainLight => &self.rain_light,
            Loop::RainHeavy => &self.rain_heavy,
            Loop::Wind => &self.wind,
            Loop::Howl => &self.howl,
            Loop::WindMuffled => &self.wind_muffled,
            Loop::Distant => &self.distant,
        }
    }
}

/// The loops of a weather.
fn loops(weather: Weather) -> &'static [Loop] {
    match weather {
        Weather::Rain => &[Loop::RainLight, Loop::RainHeavy, Loop::Wind, Loop::Howl],
        Weather::Snow => &[Loop::WindMuffled],
        Weather::Fog => &[Loop::Distant],
        Weather::Off => &[],
    }
}

/// A loop's volume now.
fn level(kind: Loop, h: &Heard) -> f32 {
    let strong = ((h.intensity - 0.4) / 0.6).clamp(0.0, 1.0);
    let rain = RAIN_VOLUME * (RAIN_FAR + (1.0 - RAIN_FAR) * h.near);
    // The gust (0..1) and the wind's strength relative to rain's; none with `wind=off`.
    let scale = h.weather.wind_scale();
    let gust = (h.wind / scale).clamp(0.0, 1.0);
    let breeze = if h.wind > 0.0 { (scale / 2.2).min(1.0) } else { 0.0 };
    match kind {
        Loop::RainLight => rain * (1.0 - strong).sqrt() * 0.8,
        Loop::RainHeavy => rain * strong.sqrt(),
        Loop::Wind => WIND_RAIN_VOLUME * breeze * (0.2 + 0.8 * gust),
        Loop::Howl => HOWL_VOLUME * breeze * gust * gust * gust,
        // Snow's wind is all there is: a calm hush even without the trees' wind.
        Loop::WindMuffled => WIND_SNOW_VOLUME * if h.wind > 0.0 { 0.25 + 0.75 * gust } else { 0.2 },
        Loop::Distant => DISTANT_VOLUME * (0.8 + 0.2 * (1.0 - h.near)),
    }
}

fn make_sounds() -> Sounds {
    let mut noise = Noise(0x5eed_cafe_f00d_1234);
    let n = &mut noise;
    Sounds {
        rain_light: rain(n, 6.0, 35.0, (1500.0, 9000.0), [0.45, 0.9, 0.0]),
        rain_heavy: rain(n, 7.0, 260.0, (500.0, 6000.0), [1.0, 0.35, 0.15]),
        wind: wind(n, false),
        howl: howl(n),
        wind_muffled: wind(n, true),
        distant: distant(n),
        thunder_near: (0..2).map(|i| thunder(n, i, true)).collect(),
        thunder_far: (0..2).map(|i| thunder(n, i, false)).collect(),
        drips: (0..6).map(|i| drip(n, i)).collect(),
    }
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

/// Rain, `secs` long: hiss (noise band-passed to `band` Hz) under `drops` small drop ticks a
/// second; `mix` is the hiss, the ticks and how deep the sheets of rain swell (three a loop).
fn rain(noise: &mut Noise, secs: f32, drops: f32, band: (f32, f32), mix: [f32; 3]) -> Sound {
    let [hiss, ticks, sheets] = mix;
    let n = (RATE as f32 * secs) as usize;
    let fade = RATE as usize / 2;
    let (lo, hi) = (pole(band.0), pole(band.1));
    let (mut low, mut high) = (0.0f32, 0.0f32);
    let (mut drop, mut tick_low) = (0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..n + fade)
        .map(|i| {
            let t = (i % n) as f32 / n as f32 * std::f32::consts::TAU;
            let w = noise.white();
            high += hi * (w - high);
            low += lo * (high - low);
            // A drop: a short, bright tick of noise, decaying in about 4 ms.
            if noise.unit() < drops / RATE as f32 {
                drop = 0.4 + 0.6 * noise.unit();
            }
            drop *= 0.988;
            let tick = noise.white();
            tick_low += pole(1200.0) * (tick - tick_low);
            let swell = 1.0 - sheets * (0.5 + 0.5 * (3.0 * t + 0.4).sin());
            (high - low) * hiss * swell + drop * (tick - tick_low) * ticks
        })
        .collect();
    finish(looped(samples, fade))
}

/// Wind, 10 s: low noise (about 120–450 Hz) breathing slightly; the gusts come from the
/// volume ([`level`]). `muffled`: low-passed again (snow).
fn wind(noise: &mut Noise, muffled: bool) -> Sound {
    let n = RATE as usize * 10;
    let fade = RATE as usize;
    let (mut a, mut b, mut low, mut m1, mut m2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..n + fade)
        .map(|i| {
            let t = (i % n) as f32 / n as f32 * std::f32::consts::TAU;
            let breath = 0.8 + 0.12 * (t + 0.7).sin() + 0.08 * (2.0 * t + 2.1).sin();
            let cutoff = 150.0 + 300.0 * breath;
            let w = noise.white();
            a += pole(cutoff) * (w - a);
            b += pole(cutoff) * (a - b);
            low += pole(120.0) * (b - low);
            let out = (b - low * 0.6) * breath;
            m1 += pole(240.0) * (out - m1);
            m2 += pole(240.0) * (m1 - m2);
            if muffled { m2 } else { out }
        })
        .collect();
    finish(looped(samples, fade))
}

/// The howl of a strong gust, 10 s: noise through a narrow resonance wandering 360–720 Hz.
fn howl(noise: &mut Noise) -> Sound {
    let n = RATE as usize * 10;
    let fade = RATE as usize;
    let (mut low, mut band) = (0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..n + fade)
        .map(|i| {
            let t = (i % n) as f32 / n as f32 * std::f32::consts::TAU;
            let centre = 540.0 + 130.0 * (t + 0.3).sin() + 50.0 * (3.0 * t + 1.0).sin();
            let f = 2.0 * (std::f32::consts::PI * centre / RATE as f32).sin();
            // A state-variable band-pass, Q about 7.
            let high = noise.white() - low - 0.14 * band;
            band += f * high;
            low += f * band;
            band * (0.7 + 0.3 * (2.0 * t).sin())
        })
        .collect();
    finish(looped(samples, fade))
}

/// A distant rumble for fog, 12 s: noise low-passed to about 70 Hz in slow swells, with a
/// faint airy hiss.
fn distant(noise: &mut Noise) -> Sound {
    let n = RATE as usize * 12;
    let fade = RATE as usize;
    let (mut a, mut b, mut h1, mut h2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..n + fade)
        .map(|i| {
            let t = (i % n) as f32 / n as f32 * std::f32::consts::TAU;
            let w = noise.white();
            a += pole(70.0) * (w - a);
            b += pole(70.0) * (a - b);
            h1 += pole(4000.0) * (w - h1);
            h2 += pole(1500.0) * (h1 - h2);
            let swell = 0.65 + 0.25 * (t + 1.1).sin() + 0.1 * (2.0 * t).sin();
            b * 8.0 * swell + (h1 - h2) * 0.04
        })
        .collect();
    finish(looped(samples, fade))
}

/// A thunder roll: `near` a crack, then a low rumble with a few swells, dying away over
/// 5–6 s; far a slower, deeper, longer rumble (7–8 s) without the crack.
fn thunder(noise: &mut Noise, variant: u32, near: bool) -> Sound {
    let secs = variant as f32 + if near { 5.0 } else { 7.0 };
    let n = (secs * RATE as f32) as usize;
    let swells: Vec<(f32, f32)> =
        (0..4).map(|_| (0.3 + noise.unit() * (secs - 2.0), 0.4 + noise.unit() * 0.6)).collect();
    let (rumble, attack_secs) = if near { ((180.0, 90.0), 0.05) } else { ((110.0, 55.0), 0.5) };
    let (mut a, mut b, mut crack) = (0.0f32, 0.0f32, 0.0f32);
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let w = noise.white();
            a += pole(rumble.0) * (w - a);
            b += pole(rumble.1) * (a - b);
            crack += pole(2500.0) * (w - crack);
            let attack = (t / attack_secs).min(1.0);
            let decay = (-t / (secs * 0.35)).exp();
            let swell: f32 = swells.iter().map(|&(at, gain)| gain * (-((t - at) / 0.6).powi(2)).exp()).sum();
            let crack_env = if near { (-t / 0.25).exp() * (1.0 - 0.4 * variant as f32) } else { 0.0 };
            b * attack * (decay + swell) * 6.0 + crack * crack_env * 0.6
        })
        .collect();
    finish(samples)
}

/// A drip (0.2 s): a water plink rising in pitch (an air bubble), 900–2600 Hz to start, after
/// a tiny splash tick.
fn drip(noise: &mut Noise, variant: u32) -> Sound {
    let n = RATE as usize / 5;
    let f0 = 900.0 + 300.0 * variant as f32 + 200.0 * noise.unit();
    let mut phase = 0.0f32;
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = f0 * (1.0 + 0.7 * (t / 0.03).min(1.0));
            phase += std::f32::consts::TAU * f / RATE as f32;
            let ring = phase.sin() * (-t / 0.03).exp() * (t / 0.001).min(1.0);
            let splash = noise.white() * (-t / 0.003).exp() * 0.5;
            ring + splash
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
    use acr_audio::Mixer;

    impl Sink for Mixer {
        fn started(&self) -> bool {
            true
        }
        fn play_loop(&mut self, sound: Sound, volume: f32, fade: f32) -> Option<VoiceId> {
            Some(self.play_looped(sound, volume, fade))
        }
        fn set_volume(&mut self, voice: VoiceId, volume: f32) {
            Mixer::set_volume(self, voice, volume);
        }
        fn fade_out(&mut self, voice: VoiceId, fade: f32) {
            Mixer::fade_out(self, voice, fade);
        }
        fn play_once(&mut self, sound: Sound, volume: f32, pan: f32) {
            self.play_ambient(sound, volume, pan);
        }
    }

    const ALL: [Loop; 6] = [Loop::RainLight, Loop::RainHeavy, Loop::Wind, Loop::Howl, Loop::WindMuffled, Loop::Distant];

    #[test]
    fn sounds_are_loud_enough_and_loops_close() {
        let s = make_sounds();
        for kind in ALL {
            let x = &s.of(kind).samples;
            let peak = x.iter().map(|v| i32::from(*v).abs()).max().unwrap();
            assert!(peak > 20000, "{kind:?} peak {peak}");
            // The step from the last frame into the first is no bigger than typical steps.
            let steps: f32 = x.windows(2).map(|w| (i32::from(w[1]) - i32::from(w[0])).abs() as f32).sum::<f32>()
                / (x.len() - 1) as f32;
            let seam = (i32::from(x[0]) - i32::from(x[x.len() - 1])).abs() as f32;
            assert!(seam < steps * 6.0, "{kind:?} seam {seam} vs mean step {steps}");
        }
        assert_eq!((s.thunder_near.len(), s.thunder_far.len(), s.drips.len()), (2, 2, 6));
    }

    #[test]
    fn weather_picks_its_loops() {
        assert!(loops(Weather::Off).is_empty());
        assert_eq!(loops(Weather::Fog), &[Loop::Distant]);
        assert!(loops(Weather::Rain).contains(&Loop::RainHeavy));
        assert_eq!(loops(Weather::Snow), &[Loop::WindMuffled]);
    }

    #[test]
    fn levels_follow_rain_zoom_and_gusts() {
        let h = Heard { weather: Weather::Rain, clock: 0.0, intensity: 1.0, wind: 2.2, near: 1.0 };
        let far = Heard { near: 0.0, ..h };
        let drizzle = Heard { intensity: 0.4, ..h };
        let calm = Heard { wind: 0.2, ..h };
        assert!(level(Loop::RainHeavy, &far) < level(Loop::RainHeavy, &h) * 0.5);
        assert!(level(Loop::RainHeavy, &drizzle) < 1e-3 && level(Loop::RainLight, &drizzle) > 0.2);
        assert!(level(Loop::Wind, &calm) < level(Loop::Wind, &h) * 0.4);
        assert!(level(Loop::Howl, &calm) < 1e-3);
        assert_eq!(level(Loop::Wind, &Heard { wind: 0.0, ..h }), 0.0);
        assert_eq!(nearness(1.0), 1.0 / 3.0);
        assert_eq!((nearness(0.5), nearness(4.0)), (0.0, 1.0));
    }

    /// Plays the weather into a bare mixer for `secs`, 60 frames a second; `heard(t)` gives
    /// what is heard at `t` seconds, `battle(t)` a shot to fire in that frame (its volume).
    /// Returns the stereo output at 44.1 kHz and the ducking in each frame.
    fn capture(secs: f32, heard: impl Fn(f32) -> Heard, battle: impl Fn(f32) -> Option<f32>) -> (Vec<f32>, Vec<f32>) {
        const OUT: u32 = 44100;
        let mut mixer = Mixer::new(OUT);
        let mut sound = WeatherSound::default();
        let shot = musket_shot();
        let frames = (secs * 60.0) as usize;
        let block = OUT as usize / 60;
        let (mut out, mut ducks) = (Vec::new(), Vec::new());
        let mut buf = vec![0.0f32; block * 2];
        for f in 0..frames {
            let t = f as f32 / 60.0;
            sound.update(heard(t), 1.0 / 60.0, &mut mixer);
            if let Some(v) = battle(t) {
                mixer.play(shot.clone(), v, 0.0);
            }
            mixer.render(&mut buf, 2);
            out.extend_from_slice(&buf);
            ducks.push(mixer.ducking());
        }
        (out, ducks)
    }

    /// A stand-in musket shot: a noise crack with a short boom (the game's are the
    /// original's WAVs).
    fn musket_shot() -> Sound {
        let mut noise = Noise(7);
        let mut low = 0.0f32;
        let samples: Vec<f32> = (0..RATE as usize / 2)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let w = noise.white();
                low += pole(300.0) * (w - low);
                w * (-t / 0.03).exp() + low * 3.0 * (-t / 0.15).exp()
            })
            .collect();
        finish(samples)
    }

    #[test]
    fn rain_ducks_under_a_volley_and_thunders_after_the_flash() {
        // A clock 3 s before the first strike's flash.
        let s = (0..).find_map(strike).unwrap();
        let start = s.at - 3.0;
        let heard = |t: f32| Heard {
            weather: Weather::Rain,
            clock: start + t,
            intensity: acr_render::weather::rain_intensity(start + t),
            wind: 2.2,
            near: 1.0,
        };
        let volley = |t: f32| ((10.0..12.0).contains(&t) && ((t * 60.0) as u32).is_multiple_of(30)).then_some(1.0);
        let (out, ducks) = capture(13.0, heard, volley);
        // The low band (below about 150 Hz, left channel) where the rumble lies.
        let mut low = 0.0f32;
        let k = 1.0 - (-std::f32::consts::TAU * 150.0 / 44100.0).exp();
        let band: Vec<f32> = out.iter().step_by(2).map(|&v| {
            low += k * (v - low);
            low
        }).collect();
        let rms = |from: f32, to: f32| {
            let x = &band[(from * 44100.0) as usize..(to * 44100.0) as usize];
            (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
        };
        // The roll starts `delay` after the flash: it fills the low band the rain leaves.
        let due = 3.0 + s.delay;
        let (roll, before) = (rms(due + 0.3, due + 1.8), rms(1.0, 3.0));
        assert!(roll > before * 2.0, "roll {roll} vs rain {before} (strike {s:?})");
        assert!(rms(due - 1.0, due) < roll * 0.5, "nothing before the roll");
        assert!(ducks[11 * 60] < 0.6 && ducks[2 * 60] > 0.99, "{} {}", ducks[11 * 60], ducks[2 * 60]);
    }

    /// Writes 30 s captures of each weather as WAVs (with a CSV of what was heard and an
    /// event list) into `$ACR_WEATHER_WAV` (a folder):
    /// `ACR_WEATHER_WAV=… cargo test -p acr-game weather_captures -- --ignored`.
    #[test]
    #[ignore]
    fn weather_captures() {
        let Ok(dir) = std::env::var("ACR_WEATHER_WAV") else { return };
        std::fs::create_dir_all(&dir).unwrap();
        let first = (0..).find_map(strike).unwrap();
        let start = first.at - 4.0;
        // Rain: close 0–8 s, zooming out 8–12, far 12–16, back close by 18, a volley 21–26.
        let near = |t: f32| match t {
            t if t < 8.0 => 1.0,
            t if t < 12.0 => 1.0 - (t - 8.0) / 4.0,
            t if t < 16.0 => 0.0,
            t if t < 18.0 => (t - 16.0) / 2.0,
            _ => 1.0,
        };
        let mut notes = String::new();
        for slot in (start / STRIKE_SLOT).floor() as i64..=((start + 30.0) / STRIKE_SLOT) as i64 {
            if let Some(s) = strike(slot).filter(|s| s.at >= start && s.at < start + 30.0) {
                notes += &format!(
                    "rain: flash at {:.1} s, thunder at {:.1} s (near {:.2}, pan {:.2})\n",
                    s.at - start,
                    s.at + s.delay - start,
                    s.near,
                    s.pan
                );
            }
        }
        for weather in [Weather::Rain, Weather::Snow, Weather::Fog] {
            let scale = weather.wind_scale();
            let heard = |t: f32| {
                let clock = start + t;
                let rain = weather == Weather::Rain;
                Heard {
                    weather,
                    clock,
                    intensity: if rain { acr_render::weather::rain_intensity(clock) } else { 1.0 },
                    wind: scale * acr_render::weather::gust([7680.0, 7680.0], [0.94, 0.34], clock),
                    near: if rain { near(t) } else { 0.6 },
                }
            };
            let volley = |t: f32| {
                (weather == Weather::Rain && (21.0..26.0).contains(&t) && ((t * 60.0) as u32).is_multiple_of(25)).then_some(0.9)
            };
            let (out, ducks) = capture(30.0, heard, volley);
            write_wav(&format!("{dir}/{}.wav", weather.name()), &out);
            let track: Vec<String> = (0..30)
                .map(|s| {
                    let h = heard(s as f32);
                    let duck = ducks[s * 60..(s + 1) * 60].iter().cloned().fold(1.0, f32::min);
                    format!("{s},{:.2},{:.2},{:.2},{duck:.2}", h.intensity, h.wind / scale, h.near)
                })
                .collect();
            let csv = format!("second,rain,gust,near,duck\n{}\n", track.join("\n"));
            std::fs::write(format!("{dir}/{}.csv", weather.name()), csv).unwrap();
        }
        std::fs::write(format!("{dir}/events.txt"), notes).unwrap();
    }

    fn write_wav(path: &str, stereo: &[f32]) {
        let data: Vec<u8> =
            stereo.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        for v in [1u16, 2] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        for v in [44100u32, 44100 * 4] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        for v in [4u16, 16] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(path, wav).unwrap();
    }
}
