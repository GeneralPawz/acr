//! Plays a [`Mixer`] on the default audio device (WASAPI natively, Web Audio in the browser).
//!
//! In the browser, audio may only start after a user gesture; create the output in
//! response to the first click or key press.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::Mixer;

/// Keeps the device stream alive; audio stops when this is dropped.
pub struct AudioOutput {
    _stream: cpal::Stream,
    pub mixer: Arc<Mutex<Mixer>>,
}

impl AudioOutput {
    pub fn start() -> Result<Self, String> {
        let device = cpal::default_host().default_output_device().ok_or("no audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let config = supported.config();
        let mixer = Arc::new(Mutex::new(Mixer::new(config.sample_rate)));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, mixer.clone(), |s| s),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, mixer.clone(), |s| (s * 32767.0) as i16),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, mixer.clone(), |s| ((s + 1.0) * 32767.5) as u16),
            other => return Err(format!("unsupported sample format {other:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, mixer })
    }
}

fn build<T: cpal::SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mixer: Arc<Mutex<Mixer>>,
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
            |e| log_error(&e.to_string()),
            None,
        )
        .map_err(|e| e.to_string())
}

fn log_error(message: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("audio: {message}");
    #[cfg(target_arch = "wasm32")]
    let _ = message;
}
