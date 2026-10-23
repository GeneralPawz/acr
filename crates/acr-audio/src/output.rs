//! Plays a [`Mixer`] on the default audio device (WASAPI natively, Web Audio in the browser).
//!
//! In the browser, audio may only start after a user gesture; create the output in
//! response to the first click or key press.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::Mixer;

/// What the device thread reports back: the stream died, buffers ran dry.
#[derive(Default)]
struct Health {
    /// The stream reported an error after which it plays nothing more (device gone,
    /// stream invalidated): it is opened again on the default device.
    broken: AtomicBool,
    /// Buffer underruns since the last [`AudioOutput::take_underruns`].
    xruns: AtomicU32,
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
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !health.broken.load(Ordering::Relaxed) || last_try.elapsed().as_secs_f32() < 2.0 {
            continue;
        }
        last_try = std::time::Instant::now();
        drop(stream.take());
        match open(mixer, health) {
            Ok(s) => {
                log::info!("audio: output reopened");
                stream = Some(s);
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
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, mixer.clone(), health.clone(), |s| s),
        cpal::SampleFormat::I16 => {
            build::<i16>(&device, &config, mixer.clone(), health.clone(), |s| (s * 32767.0) as i16)
        }
        cpal::SampleFormat::U16 => {
            build::<u16>(&device, &config, mixer.clone(), health.clone(), |s| ((s + 1.0) * 32767.5) as u16)
        }
        other => return Err(format!("unsupported sample format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
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
