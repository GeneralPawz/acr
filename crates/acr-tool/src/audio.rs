//! `audio-check` and `play`: validate and try out the sound and music decoders.

use std::path::Path;
use std::time::{Duration, Instant};

use acr_audio::output::AudioOutput;
use acr_audio::{MusicSource, Sound, VorbisMusic};
use acr_formats::Vfs;

use crate::{Command, Result};

pub const COMMANDS: &[Command] = &[
    Command {
        name: "audio-check",
        usage: "<game-dir>   (decode every sound effect and music track)",
        run: |a| match a {
            [dir] => check(Path::new(dir)),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
    Command {
        name: "play",
        usage: "<game-dir> <path>   (play a .wav or .ogg on the default device)",
        run: |a| match a {
            [dir, path] => play(Path::new(dir), path),
            _ => Err(crate::WRONG_ARGS.into()),
        },
    },
];

fn is_music(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".ogg")
}

fn is_sound(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".wav")
}

/// Decodes every `.wav` and `.ogg` the game can see and reports failures.
pub fn check(dir: &Path) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let paths: Vec<String> = vfs.list().map(|(p, _)| p.to_owned()).filter(|p| is_sound(p) || is_music(p)).collect();
    let (mut sounds, mut music_seconds, mut failures) = (0, 0.0, Vec::new());
    for path in &paths {
        let bytes = vfs.read(path)?;
        if is_sound(path) {
            match Sound::parse(&bytes) {
                Ok(_) => sounds += 1,
                Err(e) => failures.push(format!("{path}: {e}")),
            }
            continue;
        }
        match VorbisMusic::new(bytes) {
            Ok(mut music) => {
                let (rate, channels) = music.format();
                let mut samples = 0usize;
                while let Some(block) = music.next_block() {
                    samples += block.len();
                }
                music_seconds += samples as f64 / f64::from(rate) / f64::from(channels);
            }
            Err(e) => failures.push(format!("{path}: {e}")),
        }
    }
    let tracks = paths.iter().filter(|p| is_music(p)).count();
    println!("{sounds} sound effects, {tracks} music tracks ({:.0} min) decoded", music_seconds / 60.0);
    for failure in &failures {
        println!("  failed: {failure}");
    }
    Ok(())
}

/// Plays one file to the end on the default output device.
pub fn play(dir: &Path, path: &str) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let bytes = vfs.read(path)?;
    let output = AudioOutput::start()?;
    let started = Instant::now();
    {
        let mut mixer = output.mixer.lock().map_err(|_| "mixer poisoned")?;
        if is_music(path) {
            mixer.set_music(Some(Box::new(VorbisMusic::new(bytes)?)));
        } else {
            let sound = Sound::parse(&bytes)?;
            mixer.play(sound, 1.0, 0.0);
        }
    }
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let mixer = output.mixer.lock().map_err(|_| "mixer poisoned")?;
        if !mixer.music_playing() && !mixer.has_voices() {
            break;
        }
    }
    eprintln!("played {path} in {:.1}s", started.elapsed().as_secs_f32());
    Ok(())
}
