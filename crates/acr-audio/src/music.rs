//! Streaming music (`Music\*.ogg`, Ogg Vorbis).

use std::io::Cursor;

use lewton::inside_ogg::OggStreamReader;

/// A source of music blocks, pulled by the mixer as it needs more samples.
pub trait MusicSource: Send {
    /// Sample rate and channel count of all blocks.
    fn format(&self) -> (u32, u16);
    /// The next block of interleaved samples, or `None` at the end.
    fn next_block(&mut self) -> Option<Vec<i16>>;
}

pub struct VorbisMusic {
    reader: OggStreamReader<Cursor<Vec<u8>>>,
}

impl VorbisMusic {
    pub fn new(bytes: Vec<u8>) -> Result<Self, String> {
        let reader = OggStreamReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        Ok(Self { reader })
    }
}

impl MusicSource for VorbisMusic {
    fn format(&self) -> (u32, u16) {
        (self.reader.ident_hdr.audio_sample_rate, u16::from(self.reader.ident_hdr.audio_channels))
    }

    fn next_block(&mut self) -> Option<Vec<i16>> {
        loop {
            match self.reader.read_dec_packet_itl() {
                Ok(Some(block)) if block.is_empty() => continue,
                Ok(Some(block)) => return Some(block),
                // Treat a corrupt packet as the end rather than looping forever.
                Ok(None) | Err(_) => return None,
            }
        }
    }
}
