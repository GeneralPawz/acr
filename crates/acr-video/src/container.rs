//! The `.bik` container: header, audio track descriptions and frame index.
//!
//! Every frame packet holds, for each audio track, a little-endian `u32` size followed by
//! that many bytes of audio, and then the video data up to the end of the packet.

use std::sync::Arc;

use crate::BinkError;

/// Video flag: the file has an extra alpha plane.
pub const FLAG_ALPHA: u32 = 0x0010_0000;
/// Audio flag: 16-bit output (always set in practice).
pub const AUDIO_16BITS: u16 = 0x4000;
/// Audio flag: two channels.
pub const AUDIO_STEREO: u16 = 0x2000;
/// Audio flag: DCT instead of RDFT coding.
pub const AUDIO_DCT: u16 = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioTrack {
    pub sample_rate: u32,
    pub flags: u16,
    pub id: u32,
}

impl AudioTrack {
    pub fn channels(&self) -> u16 {
        if self.flags & AUDIO_STEREO != 0 { 2 } else { 1 }
    }

    pub fn uses_dct(&self) -> bool {
        self.flags & AUDIO_DCT != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Revision letter after `BIK`, e.g. `b'i'`.
    pub revision: u8,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    /// Frame rate as a fraction `fps_num / fps_den`.
    pub fps_num: u32,
    pub fps_den: u32,
    pub video_flags: u32,
    pub audio: Vec<AudioTrack>,
}

impl Header {
    pub fn frame_rate(&self) -> f64 {
        f64::from(self.fps_num) / f64::from(self.fps_den)
    }

    /// Playing time in seconds.
    pub fn duration(&self) -> f64 {
        f64::from(self.frame_count) / self.frame_rate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameEntry {
    offset: usize,
    size: usize,
    keyframe: bool,
}

/// A parsed `.bik` file; the bytes are shared so audio and video can be decoded
/// independently (e.g. audio on the mixer thread).
#[derive(Clone)]
pub struct BinkFile {
    pub header: Header,
    data: Arc<[u8]>,
    frames: Arc<[FrameEntry]>,
}

/// One frame's audio packets (one per track, possibly empty) and video data.
pub struct Packet<'a> {
    pub audio: Vec<&'a [u8]>,
    pub video: &'a [u8],
    pub keyframe: bool,
}

fn u32_at(data: &[u8], pos: usize) -> Result<u32, BinkError> {
    data.get(pos..pos + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap_or_default()))
        .ok_or(BinkError("file truncated"))
}

impl BinkFile {
    pub fn parse(data: impl Into<Arc<[u8]>>) -> Result<Self, BinkError> {
        let data: Arc<[u8]> = data.into();
        if data.len() < 44 || &data[0..3] != b"BIK" {
            return Err(BinkError("not a Bink file"));
        }
        let revision = data[3];
        if !(b'c'..=b'i').contains(&revision) {
            // 'b' uses a different bitstream; 'k' and KB2 add fields we do not handle.
            return Err(BinkError("unsupported Bink revision"));
        }
        let frame_count = u32_at(&data, 8)?;
        let width = u32_at(&data, 20)?;
        let height = u32_at(&data, 24)?;
        let fps_num = u32_at(&data, 28)?;
        let fps_den = u32_at(&data, 32)?;
        let video_flags = u32_at(&data, 36)?;
        let track_count = u32_at(&data, 40)? as usize;
        if frame_count == 0 || frame_count > 1_000_000 || fps_num == 0 || fps_den == 0 {
            return Err(BinkError("invalid header"));
        }
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(BinkError("invalid frame size"));
        }
        if track_count > 256 {
            return Err(BinkError("too many audio tracks"));
        }
        // Per track: max decoded size (u32), then rate+flags (2 x u16), then ids (u32).
        let mut pos = 44 + 4 * track_count;
        let mut audio = Vec::with_capacity(track_count);
        for _ in 0..track_count {
            let v = u32_at(&data, pos)?;
            audio.push(AudioTrack { sample_rate: v & 0xFFFF, flags: (v >> 16) as u16, id: 0 });
            pos += 4;
        }
        for track in &mut audio {
            track.id = u32_at(&data, pos)?;
            pos += 4;
        }

        let mut frames = Vec::with_capacity(frame_count as usize);
        for i in 0..frame_count as usize {
            let raw = u32_at(&data, pos + 4 * i)?;
            let next =
                if i + 1 == frame_count as usize { data.len() as u32 } else { u32_at(&data, pos + 4 * (i + 1))? & !1 };
            let offset = (raw & !1) as usize;
            let end = next as usize;
            if end <= offset || end > data.len() {
                return Err(BinkError("invalid frame index"));
            }
            frames.push(FrameEntry { offset, size: end - offset, keyframe: raw & 1 != 0 || i == 0 });
        }
        let header = Header { revision, width, height, frame_count, fps_num, fps_den, video_flags, audio };
        Ok(Self { header, data, frames: frames.into() })
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Splits frame `index` into its audio and video parts.
    pub fn packet(&self, index: usize) -> Result<Packet<'_>, BinkError> {
        let entry = self.frames.get(index).ok_or(BinkError("frame index out of range"))?;
        let mut bytes = &self.data[entry.offset..entry.offset + entry.size];
        let mut audio = Vec::with_capacity(self.header.audio.len());
        for _ in &self.header.audio {
            let size = u32_at(bytes, 0)? as usize;
            let body = bytes.get(4..4 + size).ok_or(BinkError("audio packet exceeds frame"))?;
            audio.push(body);
            bytes = &bytes[4 + size..];
        }
        Ok(Packet { audio, video: bytes, keyframe: entry.keyframe })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal file: one stereo 44.1 kHz track, two frames.
    pub(crate) fn sample_file() -> Vec<u8> {
        let mut out = b"BIKi".to_vec();
        for v in [0u32, 2, 100, 2, 32, 16, 25, 1, 0, 1, 0x1000, 0xE000_AC44, 7] {
            out.extend(v.to_le_bytes());
        }
        let index = out.len();
        out.extend([0u8; 8]);
        let first = out.len() as u32;
        out.extend(4u32.to_le_bytes());
        out.extend([1, 2, 3, 4]);
        out.extend([0xAA; 4]);
        let second = out.len() as u32;
        out.extend(0u32.to_le_bytes());
        out.extend([0xBB; 8]);
        out[index..index + 4].copy_from_slice(&(first | 1).to_le_bytes());
        out[index + 4..index + 8].copy_from_slice(&second.to_le_bytes());
        let size = out.len() as u32 - 8;
        out[4..8].copy_from_slice(&size.to_le_bytes());
        out
    }

    #[test]
    fn parses_header_tracks_and_packets() {
        let file = BinkFile::parse(sample_file()).unwrap();
        let h = &file.header;
        assert_eq!((h.revision, h.width, h.height, h.frame_count), (b'i', 32, 16, 2));
        assert_eq!(h.frame_rate(), 25.0);
        assert_eq!(h.audio, vec![AudioTrack { sample_rate: 44100, flags: 0xE000, id: 7 }]);
        assert_eq!(h.audio[0].channels(), 2);
        assert!(!h.audio[0].uses_dct());

        let p = file.packet(0).unwrap();
        assert!(p.keyframe);
        assert_eq!(p.audio, vec![&[1u8, 2, 3, 4][..]]);
        assert_eq!(p.video, &[0xAA; 4]);
        let p = file.packet(1).unwrap();
        assert!(!p.keyframe);
        assert_eq!(p.audio, vec![&[][..]]);
        assert_eq!(p.video, &[0xBB; 8]);
    }

    #[test]
    fn rejects_other_files() {
        assert!(BinkFile::parse(b"RIFF....".to_vec()).is_err());
        let mut kb2 = sample_file();
        kb2[3] = b'k';
        assert!(BinkFile::parse(kb2).is_err());
    }
}
