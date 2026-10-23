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
pub struct AudioOutput {
    stream: cpal::Stream,
    pub mixer: Arc<Mutex<Mixer>>,
    health: Arc<Health>,
}

impl AudioOutput {
    pub fn start() -> Result<Self, String> {
        let mixer = Arc::new(Mutex::new(Mixer::new(48_000)));
        let health = Arc::new(Health::default());
        let stream = open(&mixer, &health)?;
        Ok(Self { stream, mixer, health })
    }

    /// After a stream error that ends the sound (the device went away), opens the default
    /// device again with the same mixer, so music and effects go on. True if the stream is
    /// (again) fine.
    pub fn recover(&mut self) -> bool {
        if !self.health.broken.load(Ordering::Relaxed) {
            return true;
        }
        match open(&self.mixer, &self.health) {
            Ok(stream) => {
                log::info!("audio: output reopened");
                self.stream = stream;
                self.health.broken.store(false, Ordering::Relaxed);
                true
            }
            Err(e) => {
                log::warn!("audio: cannot reopen the output: {e}");
                false
            }
        }
    }

    /// Buffer underruns since the last call.
    pub fn take_underruns(&self) -> u32 {
        self.health.xruns.swap(0, Ordering::Relaxed)
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
