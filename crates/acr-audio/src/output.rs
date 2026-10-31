//! Plays a [`Mixer`] on the default audio device (WASAPI natively, AAudio on Android, Web
//! Audio in the browser).
//!
//! On Android (#440) the callback asks for [`ANDROID_BUFFER_MS`] ms of audio at a time: the
//! default burst is a few milliseconds, which wakes the mixer and the CPU hundreds of times a
//! second; a larger one wakes it about 25 times. cpal's AAudio backend offers no power-saving
//! performance mode (it opens with the default mode, or low latency with its `realtime`
//! feature, which is off), so the buffer is the lever the backend allows.
//!
//! In the browser, audio may only start after a user gesture; create the output in
//! response to the first click or key press.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::Mixer;

/// The callback size on Android, in milliseconds of audio: sound effects and music in a strategy game
/// bear 40 ms of delay.
pub const ANDROID_BUFFER_MS: u32 = 40;

/// The callback size in frames for `sample_rate`: [`ANDROID_BUFFER_MS`], rounded up to a
/// multiple of 64 frames.
pub fn android_buffer_frames(sample_rate: u32) -> u32 {
    (sample_rate * ANDROID_BUFFER_MS).div_ceil(1000).div_ceil(64) * 64
}

/// What the device thread reports back: the stream died, buffers ran dry.
#[derive(Default)]
struct Health {
    /// The stream reported an error after which it plays nothing more (device gone,
    /// stream invalidated): it is opened again on the default device.
    broken: AtomicBool,
    /// Buffer underruns since the last [`AudioOutput::take_underruns`].
    xruns: AtomicU32,
    /// The stream should be paused (the app is in the background); the device thread
    /// follows it within 100 ms.
    paused: AtomicBool,
}

/// What to do to a stream that is `applied` (paused or not) when `wanted` is the state
/// it should be in: `Some(true)` pauses it, `Some(false)` plays it, `None` leaves it.
#[cfg(not(target_arch = "wasm32"))]
fn pause_change(wanted: bool, applied: bool) -> Option<bool> {
    (wanted != applied).then_some(wanted)
}

/// Keeps the device stream alive; audio stops when this is dropped.
///
/// Natively the stream lives on its own thread, which also reopens it after a lost device:
/// WASAPI needs a multithreaded COM apartment, and the window's thread is single-threaded
/// (opening there again failed with `RPC_E_CHANGED_MODE`, and the game stayed silent).
pub struct AudioOutput {
    #[cfg(target_arch = "wasm32")]
    stream: cpal::Stream,
    #[cfg(not(target_arch = "wasm32"))]
    thread: Option<(Arc<AtomicBool>, std::thread::JoinHandle<()>)>,
    pub mixer: Arc<Mutex<Mixer>>,
    health: Arc<Health>,
}

impl AudioOutput {
    #[cfg(target_arch = "wasm32")]
    pub fn start() -> Result<Self, String> {
        let mixer = Arc::new(Mutex::new(Mixer::new(48_000)));
        let health = Arc::new(Health::default());
        let stream = open(&mixer, &health)?;
        Ok(Self { stream, mixer, health })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn start() -> Result<Self, String> {
        let mixer = Arc::new(Mutex::new(Mixer::new(48_000)));
        let health = Arc::new(Health::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let (m, h, s) = (mixer.clone(), health.clone(), stop.clone());
        let handle = std::thread::Builder::new()
            .name("acr-audio".into())
            .spawn(move || device_thread(&m, &h, &s, &tx))
            .map_err(|e| e.to_string())?;
        match rx.recv() {
            Ok(Ok(())) => Ok(Self { thread: Some((stop, handle)), mixer, health }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("audio thread ended".into()),
        }
    }

    /// After a stream error that ends the sound (the device went away), the default device
    /// is opened again with the same mixer, so music and effects go on (natively by the
    /// audio thread, every 2 s until it works). True if the stream is (again) fine.
    pub fn recover(&mut self) -> bool {
        if !self.health.broken.load(Ordering::Relaxed) {
            return true;
        }
        #[cfg(target_arch = "wasm32")]
        match open(&self.mixer, &self.health) {
            Ok(stream) => {
                log::info!("audio: output reopened");
                self.stream = stream;
                self.health.broken.store(false, Ordering::Relaxed);
                return true;
            }
            Err(e) => log::warn!("audio: cannot reopen the output: {e}"),
        }
        false
    }

    /// Pauses or resumes the device stream (the app went to the background or came back).
    /// The mixer drops new sounds meanwhile, so none pile up.
    pub fn set_paused(&mut self, paused: bool) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.set_paused(paused);
        }
        self.health.paused.store(paused, Ordering::Relaxed);
        #[cfg(target_arch = "wasm32")]
        {
            let result = if paused { self.stream.pause() } else { self.stream.play() };
            if let Err(e) = result {
                log::warn!("audio: cannot {} the stream: {e}", if paused { "pause" } else { "resume" });
            }
        }
    }

    /// Buffer underruns since the last call.
    pub fn take_underruns(&self) -> u32 {
        self.health.xruns.swap(0, Ordering::Relaxed)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for AudioOutput {
    fn drop(&mut self) {
        if let Some((stop, handle)) = self.thread.take() {
            stop.store(true, Ordering::Relaxed);
            let _ = handle.join();
        }
    }
}

/// The audio thread: opens the stream (reporting the first result on `started`), keeps it,
/// reopens it after a lost device every 2 s until that works, and ends on `stop`.
#[cfg(not(target_arch = "wasm32"))]
fn device_thread(
    mixer: &Arc<Mutex<Mixer>>,
    health: &Arc<Health>,
    stop: &AtomicBool,
    started: &std::sync::mpsc::Sender<Result<(), String>>,
) {
    let mut stream = match open(mixer, health) {
        Ok(s) => {
            let _ = started.send(Ok(()));
            Some(s)
        }
        Err(e) => {
            let _ = started.send(Err(e));
            return;
        }
    };
    let mut last_try = std::time::Instant::now();
    let mut paused = false;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let (Some(change), Some(s)) = (pause_change(health.paused.load(Ordering::Relaxed), paused), &stream) {
            let result = if change { s.pause() } else { s.play() };
            match result {
                Ok(()) => paused = change,
                Err(e) => {
                    log::warn!("audio: cannot {} the stream: {e}", if change { "pause" } else { "resume" });
                    health.broken.store(true, Ordering::Relaxed);
                }
            }
        }
        if !health.broken.load(Ordering::Relaxed) || last_try.elapsed().as_secs_f32() < 2.0 {
            continue;
        }
        last_try = std::time::Instant::now();
        drop(stream.take());
        match open(mixer, health) {
            Ok(s) => {
                log::info!("audio: output reopened");
                stream = Some(s);
                // A new stream plays.
                paused = false;
                health.broken.store(false, Ordering::Relaxed);
            }
            Err(e) => log::warn!("audio: cannot reopen the output: {e}"),
        }
    }
}

/// Opens and starts a stream on the default output device that plays `mixer`.
fn open(mixer: &Arc<Mutex<Mixer>>, health: &Arc<Health>) -> Result<cpal::Stream, String> {
    let device = cpal::default_host().default_output_device().ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config = supported.config();
    let name = device.description().map(|d| d.name().to_owned()).unwrap_or_else(|e| format!("? ({e})"));
    log::info!(
        "audio: device \"{name}\", {} Hz, {} channel(s), {:?}, buffer {:?}",
        config.sample_rate,
        config.channels,
        supported.sample_format(),
        config.buffer_size
    );
    if let Ok(mut m) = mixer.lock() {
        m.set_rate(config.sample_rate);
    }
    let format = supported.sample_format();
    let stream = match large_callback(&config) {
        Some(big) => build_for(&device, format, &big, mixer, health)
            .inspect(|_| log::info!("audio: callback of {:?} frames", big.buffer_size))
            .or_else(|e| {
                // The device's own choice if it refuses.
                log::warn!("audio: no large callback ({e}), using the device's default");
                build_for(&device, format, &config, mixer, health)
            }),
        None => build_for(&device, format, &config, mixer, health),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

/// `config` with the larger callback of Android (see the module docs); `None` elsewhere.
fn large_callback(config: &cpal::StreamConfig) -> Option<cpal::StreamConfig> {
    cfg!(target_os = "android").then(|| cpal::StreamConfig {
        buffer_size: cpal::BufferSize::Fixed(android_buffer_frames(config.sample_rate)),
        ..*config
    })
}

fn build_for(
    device: &cpal::Device,
    format: cpal::SampleFormat,
    config: &cpal::StreamConfig,
    mixer: &Arc<Mutex<Mixer>>,
    health: &Arc<Health>,
) -> Result<cpal::Stream, String> {
    match format {
        cpal::SampleFormat::F32 => build::<f32>(device, config, mixer.clone(), health.clone(), |s| s),
        cpal::SampleFormat::I16 => {
            build::<i16>(device, config, mixer.clone(), health.clone(), |s| (s * 32767.0) as i16)
        }
        cpal::SampleFormat::U16 => {
            build::<u16>(device, config, mixer.clone(), health.clone(), |s| ((s + 1.0) * 32767.5) as u16)
        }
        other => Err(format!("unsupported sample format {other:?}")),
    }
}

fn build<T: cpal::SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mixer: Arc<Mutex<Mixer>>,
    health: Arc<Health>,
    convert: fn(f32) -> T,
) -> Result<cpal::Stream, String> {
    let channels = config.channels;
    let mut scratch: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            *config,
            move |out: &mut [T], _| {
                scratch.resize(out.len(), 0.0);
                match mixer.lock() {
                    Ok(mut m) => m.render(&mut scratch, channels),
                    Err(_) => scratch.fill(0.0),
                }
                for (o, s) in out.iter_mut().zip(&scratch) {
                    *o = convert(*s);
                }
            },
            move |e| on_error(&health, &e),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Errors of the device thread: underruns are counted for the log; a lost device or an
/// invalidated stream marks the stream for [`AudioOutput::recover`]; a reroute to a new
/// default device needs nothing.
fn on_error(health: &Health, e: &cpal::Error) {
    match e.kind() {
        cpal::ErrorKind::Xrun => {
            health.xruns.fetch_add(1, Ordering::Relaxed);
        }
        cpal::ErrorKind::DeviceChanged => log::info!("audio: {e}"),
        cpal::ErrorKind::DeviceNotAvailable
        | cpal::ErrorKind::StreamInvalidated
        | cpal::ErrorKind::HostUnavailable
        | cpal::ErrorKind::BackendError => {
            log::error!("audio stream: {e}");
            health.broken.store(true, Ordering::Relaxed);
        }
        _ => log::warn!("audio stream: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_callback_is_about_40_ms_in_whole_64_frame_steps() {
        assert_eq!(android_buffer_frames(48_000), 1920);
        assert_eq!(android_buffer_frames(44_100), 1792);
        assert_eq!(android_buffer_frames(8_000), 320);
        for rate in [8_000, 22_050, 44_100, 48_000, 96_000] {
            let frames = android_buffer_frames(rate);
            assert_eq!(frames % 64, 0);
            let ms = frames as f32 / rate as f32 * 1000.0;
            assert!((40.0..44.0).contains(&ms), "{rate} Hz: {ms} ms");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_stream_follows_the_wanted_pause_state() {
        assert_eq!(pause_change(true, false), Some(true));
        assert_eq!(pause_change(false, true), Some(false));
        assert_eq!(pause_change(true, true), None);
        assert_eq!(pause_change(false, false), None);
    }
}
