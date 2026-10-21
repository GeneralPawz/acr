//! Playback: frames at the file's frame rate, audio as a [`MusicSource`] for the mixer.

use std::sync::Arc;

use acr_audio::MusicSource;

use crate::BinkError;
use crate::audio::AudioDecoder;
use crate::container::{BinkFile, FLAG_ALPHA, Header};
use crate::video::VideoDecoder;

/// Plays the video of a `.bik` file: call [`VideoPlayer::advance_to`] with the time since
/// playback started and draw [`VideoPlayer::rgba`] when it reports a new picture.
pub struct VideoPlayer {
    file: BinkFile,
    video: VideoDecoder,
    /// Number of frames decoded so far.
    decoded: usize,
    rgba: Vec<u8>,
}

impl VideoPlayer {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Result<Self, BinkError> {
        let file = BinkFile::parse(bytes)?;
        let h = &file.header;
        let video = VideoDecoder::new(h.width, h.height, h.revision, h.video_flags & FLAG_ALPHA != 0);
        let rgba = vec![0; h.width as usize * h.height as usize * 4];
        Ok(Self { file, video, decoded: 0, rgba })
    }

    pub fn header(&self) -> &Header {
        &self.file.header
    }

    pub fn width(&self) -> u32 {
        self.file.header.width
    }

    pub fn height(&self) -> u32 {
        self.file.header.height
    }

    /// The first audio track as a mixer source, if it has one we can decode.
    pub fn audio(&self) -> Option<BinkAudio> {
        BinkAudio::new(self.file.clone(), 0).ok()
    }

    /// Frames decoded so far.
    pub fn frames_decoded(&self) -> usize {
        self.decoded
    }

    /// Decodes the next frame without converting it; returns `false` at the end.
    pub fn decode_next(&mut self) -> Result<bool, BinkError> {
        if self.decoded >= self.file.frame_count() {
            return Ok(false);
        }
        let packet = self.file.packet(self.decoded)?;
        self.decoded += 1;
        self.video.decode(packet.video)?;
        Ok(true)
    }

    /// Decodes every frame due at `seconds` after the start (frame `i` is shown from
    /// `i / fps`). Returns `true` when the picture changed; skipped-over frames are still
    /// decoded because later frames build on them, but only the last is converted.
    pub fn advance_to(&mut self, seconds: f64) -> Result<bool, BinkError> {
        let due = ((seconds.max(0.0) * self.file.header.frame_rate()) as usize + 1).min(self.file.frame_count());
        if due <= self.decoded {
            return Ok(false);
        }
        while self.decoded < due {
            self.decode_next()?;
        }
        self.video.to_rgba(&mut self.rgba);
        Ok(true)
    }

    /// The current picture, `width * height` RGBA pixels.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Converts the last decoded frame into [`VideoPlayer::rgba`].
    pub fn convert(&mut self) {
        self.video.to_rgba(&mut self.rgba);
    }

    /// True once `seconds` is past the last frame.
    pub fn finished(&self, seconds: f64) -> bool {
        seconds >= self.file.header.duration()
    }
}

/// One audio track of a file, decoded packet by packet as the mixer asks for more.
pub struct BinkAudio {
    file: BinkFile,
    track: usize,
    frame: usize,
    decoder: AudioDecoder,
    scratch: Vec<f32>,
}

impl BinkAudio {
    pub fn new(file: BinkFile, track: usize) -> Result<Self, BinkError> {
        let info = file.header.audio.get(track).ok_or(BinkError("no such audio track"))?;
        let decoder = AudioDecoder::new(info)?;
        Ok(Self { file, track, frame: 0, decoder, scratch: Vec::new() })
    }

    /// Decodes the next non-empty packet as floats; `None` at the end or on an error.
    pub fn next_samples(&mut self) -> Option<&[f32]> {
        while self.frame < self.file.frame_count() {
            let packet = self.file.packet(self.frame).ok()?;
            self.frame += 1;
            self.scratch.clear();
            self.decoder.decode(packet.audio[self.track], &mut self.scratch).ok()?;
            if !self.scratch.is_empty() {
                return Some(&self.scratch);
            }
        }
        None
    }
}

impl MusicSource for BinkAudio {
    fn format(&self) -> (u32, u16) {
        (self.decoder.rate(), self.decoder.channels())
    }

    fn next_block(&mut self) -> Option<Vec<i16>> {
        let samples = self.next_samples()?;
        Some(samples.iter().map(|&s| (s * 32768.0).clamp(-32768.0, 32767.0) as i16).collect())
    }
}
