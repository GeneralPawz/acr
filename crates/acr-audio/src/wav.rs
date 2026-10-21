//! RIFF/WAVE files with 8- or 16-bit PCM, as used by the original `Sound\*.wav`.

use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavError(pub &'static str);

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for WavError {}

/// Decoded sound: interleaved signed 16-bit samples.
#[derive(Debug, Clone)]
pub struct Sound {
    pub rate: u32,
    pub channels: u16,
    pub samples: Arc<[i16]>,
}

impl Sound {
    pub fn parse(bytes: &[u8]) -> Result<Self, WavError> {
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(WavError("not a RIFF/WAVE file"));
        }
        let mut format: Option<(u16, u16, u32, u16)> = None;
        let mut pos = 12;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body = &bytes[pos + 8..(pos + 8 + len).min(bytes.len())];
            match id {
                b"fmt " if body.len() >= 16 => {
                    let u16_at = |i: usize| u16::from_le_bytes([body[i], body[i + 1]]);
                    let rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
                    format = Some((u16_at(0), u16_at(2), rate, u16_at(14)));
                }
                b"data" => {
                    let (tag, channels, rate, bits) = format.ok_or(WavError("data chunk before fmt chunk"))?;
                    if tag != 1 {
                        return Err(WavError("only uncompressed PCM is supported"));
                    }
                    if channels == 0 || rate == 0 {
                        return Err(WavError("invalid format"));
                    }
                    let samples: Vec<i16> = match bits {
                        16 => body.as_chunks::<2>().0.iter().map(|&s| i16::from_le_bytes(s)).collect(),
                        8 => body.iter().map(|&s| (i16::from(s) - 128) << 8).collect(),
                        _ => return Err(WavError("unsupported bit depth")),
                    };
                    return Ok(Self { rate, channels, samples: samples.into() });
                }
                _ => {}
            }
            // Chunks are padded to even sizes.
            pos += 8 + len + (len & 1);
        }
        Err(WavError("no data chunk"))
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn wav(rate: u32, channels: u16, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut out = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * u32::from(channels * bits / 8)).to_le_bytes());
        out.extend((channels * bits / 8).to_le_bytes());
        out.extend(bits.to_le_bytes());
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn parses_16_bit_mono() {
        let s = Sound::parse(&wav(22050, 1, 16, &[0x01, 0x00, 0xFF, 0xFF])).unwrap();
        assert_eq!((s.rate, s.channels), (22050, 1));
        assert_eq!(&*s.samples, &[1, -1]);
    }

    #[test]
    fn parses_8_bit() {
        let s = Sound::parse(&wav(11025, 1, 8, &[128, 255, 0])).unwrap();
        assert_eq!(&*s.samples, &[0, 127 << 8, -128 << 8]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(Sound::parse(b"GPAK").is_err());
    }
}
