//! `bik-check`, `bik-frames` and `bik-play`: validate the Bink decoder on `Video\*.bik`.

use std::path::Path;
use std::time::{Duration, Instant};

use acr_audio::output::AudioOutput;
use acr_formats::Vfs;
use acr_formats::raster::Image;
use acr_video::{BinkAudio, VideoPlayer};

use crate::Result;

fn bik_paths(vfs: &Vfs) -> Vec<String> {
    vfs.list().map(|(p, _)| p.to_owned()).filter(|p| p.to_ascii_lowercase().ends_with(".bik")).collect()
}

/// Decodes every frame and all audio of every `.bik` file and reports speed and levels.
pub fn check(dir: &Path) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths = bik_paths(&vfs);
    if paths.is_empty() {
        println!("no .bik files found");
    }
    let mut failures = 0;
    for path in &paths {
        let bytes = vfs.read(path)?;
        let mut player = match VideoPlayer::new(bytes) {
            Ok(p) => p,
            Err(e) => {
                println!("{path}: {e}");
                failures += 1;
                continue;
            }
        };
        let h = player.header().clone();
        let tracks: Vec<String> =
            h.audio.iter().map(|t| format!("{} Hz x{} flags {:#06x}", t.sample_rate, t.channels(), t.flags)).collect();
        println!(
            "{path}: BIK{} {}x{} {} frames @ {:.2} fps ({:.1} s), audio [{}]",
            h.revision as char,
            h.width,
            h.height,
            h.frame_count,
            h.frame_rate(),
            h.duration(),
            tracks.join(", ")
        );

        let started = Instant::now();
        let mut error = None;
        loop {
            match player.decode_next() {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        let video_time = started.elapsed().as_secs_f64();
        let frames = player.frames_decoded();
        match error {
            Some(e) => {
                failures += 1;
                println!("  video: FAILED at frame {} of {}: {e}", frames - 1, h.frame_count);
            }
            None => println!(
                "  video: {frames} frames in {video_time:.2} s = {:.0} fps ({:.1}x real time)",
                frames as f64 / video_time,
                frames as f64 / video_time / h.frame_rate()
            ),
        }

        if let Some(mut audio) = player.audio() {
            let channels = f64::from(h.audio[0].channels());
            let rate = f64::from(h.audio[0].sample_rate);
            let started = Instant::now();
            let (mut samples, mut peak, mut energy) = (0usize, 0f32, 0f64);
            while let Some(block) = audio.next_samples() {
                samples += block.len();
                for &s in block {
                    peak = peak.max(s.abs());
                    energy += f64::from(s) * f64::from(s);
                }
            }
            let seconds = samples as f64 / channels / rate;
            let rms = (energy / samples.max(1) as f64).sqrt();
            println!(
                "  audio: {seconds:.1} s decoded in {:.2} s, peak {peak:.3}, rms {rms:.3}",
                started.elapsed().as_secs_f64()
            );
        } else if !h.audio.is_empty() {
            println!("  audio: unsupported track format");
        }
    }
    if failures > 0 {
        return Err(format!("{failures} file(s) failed").into());
    }
    Ok(())
}

/// Writes every `every`-th frame of one file as `frame_NNNN.png`.
pub fn frames(dir: &Path, path: &str, out: &Path, every: usize) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let mut player = VideoPlayer::new(vfs.read(path)?)?;
    std::fs::create_dir_all(out)?;
    let (width, height) = (player.width(), player.height());
    let mut written = 0;
    let mut index = 0;
    while player.decode_next()? {
        if index % every.max(1) == 0 {
            player.convert();
            let image = Image { width, height, rgba: player.rgba().to_vec() };
            crate::write_png(&out.join(format!("frame_{index:04}.png")), &image)?;
            written += 1;
        }
        index += 1;
    }
    eprintln!("wrote {written} of {index} frames to {}", out.display());
    Ok(())
}

/// Plays the soundtrack of one file on the default output device.
pub fn play(dir: &Path, path: &str) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let player = VideoPlayer::new(vfs.read(path)?)?;
    let audio: BinkAudio = player.audio().ok_or("no playable audio track")?;
    let output = AudioOutput::start()?;
    output.mixer.lock().map_err(|_| "mixer poisoned")?.set_music(Some(Box::new(audio)));
    while output.mixer.lock().map_err(|_| "mixer poisoned")?.music_playing() {
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
