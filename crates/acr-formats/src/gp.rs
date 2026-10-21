//! `GPAK` sprite packs (`*.GP`).
//!
//! Reverse engineered from the shipped data; see `docs/formats/gp.md` for the
//! evidence behind each rule. Layout (little-endian):
//!
//! ```text
//! file    "GPAK" | u16 frame_count | u16 flags | u32 dict_offset | u16 dict_len
//!         | u32 frame_offset[frame_count]
//!         dictionary: dict_len palette indices shared by all frames
//! chunk   23-byte header, then shape data, then color data
//!         i32 next      offset of the next chunk relative to the frame start, -1 = last
//!         i16 x, y      position of the chunk inside the frame
//!         i16 w, h
//!         i32 -1
//!         u8  encoding  see [`Encoding`]
//!         u16 shape_len (+24, truncated to 14 bits; unreliable for large chunks)
//!         u16 unknown
//!         u16 rows      equal to h
//! shape   per row: 0x00 = empty row
//!                  0x01..=0x6F n = n byte pairs (skip, len)
//!                  0x8T..=0xFT  = T nibble pairs: low nibble = skip, high = len;
//!                                 bit 0x20 of the tag adds 16 to len, 0x40 adds 16 to skip
//!         skips are relative to the end of the previous run in the row
//! color   1 padding byte, then the pixels of all runs in order, encoded per [`Encoding`],
//!         optionally followed by 1 padding byte
//! ```

use std::fmt;

const MAGIC: &[u8; 4] = b"GPAK";
const CHUNK_HEADER_LEN: usize = 23;
/// Guards against corrupt `next` links looping forever.
const MAX_CHUNKS_PER_FRAME: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpError {
    pub offset: usize,
    pub reason: &'static str,
}

impl fmt::Display for GpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {:#x}", self.reason, self.offset)
    }
}

impl std::error::Error for GpError {}

type Result<T> = std::result::Result<T, GpError>;

fn err<T>(offset: usize, reason: &'static str) -> Result<T> {
    Err(GpError { offset, reason })
}

/// How a chunk's pixels are stored and how the original engine blends them.
///
/// Names describe the storage, which is verified; the blending semantics noted on
/// some variants are still hypotheses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// 0: palette indices, compressed with back-references into the file dictionary.
    DictLz,
    /// 1: 2 bits per pixel, offsets into the owning player's 4-shade color ramp.
    PlayerColor,
    /// 2..=4: uncompressed palette indices (likely translucent overlays of varying strength).
    Raw(u8),
    /// 5: no color data; the shape alone (likely a shadow).
    Shadow,
    /// 0x26 / 0x27: 4 bits per pixel.
    Packed4(u8),
    /// 0x2A / 0x2B: palette indices, compressed with back-references into the chunk's own output.
    SelfLz(u8),
    /// 0xFF: no pixels.
    Empty,
    Unknown(u8),
}

impl Encoding {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::DictLz,
            1 => Self::PlayerColor,
            2..=4 => Self::Raw(b),
            5 => Self::Shadow,
            0x26 | 0x27 => Self::Packed4(b),
            0x2A | 0x2B => Self::SelfLz(b),
            0xFF => Self::Empty,
            _ => Self::Unknown(b),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::DictLz => 0,
            Self::PlayerColor => 1,
            Self::Shadow => 5,
            Self::Empty => 0xFF,
            Self::Raw(b) | Self::Packed4(b) | Self::SelfLz(b) | Self::Unknown(b) => b,
        }
    }
}

/// A horizontal run of pixels inside a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub x: u16,
    pub y: u16,
    pub len: u16,
}

/// One decoded chunk (layer) of a frame.
#[derive(Debug, Clone)]
pub struct Layer {
    pub encoding: Encoding,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub runs: Vec<Run>,
    /// One value per pixel of `runs`, in order. Its meaning depends on `encoding`:
    /// palette index, player-ramp offset (0..4), 4-bit value, or 0 for shadows.
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub layers: Vec<Layer>,
}

impl Frame {
    /// Bounding box `(min_x, min_y, max_x, max_y)` of all runs, exclusive max.
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for layer in &self.layers {
            for run in &layer.runs {
                let x0 = i32::from(layer.x) + i32::from(run.x);
                let y0 = i32::from(layer.y) + i32::from(run.y);
                let (x1, y1) = (x0 + i32::from(run.len), y0 + 1);
                b = Some(match b {
                    None => (x0, y0, x1, y1),
                    Some((a, b_, c, d)) => (a.min(x0), b_.min(y0), c.max(x1), d.max(y1)),
                });
            }
        }
        b
    }
}

#[derive(Debug, Clone, Copy)]
struct ChunkHeader {
    offset: usize,
    end: usize,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    encoding: u8,
}

#[derive(Debug, Clone)]
pub struct GpFile {
    data: Vec<u8>,
    dict: std::ops::Range<usize>,
    frames: Vec<std::result::Result<Vec<ChunkHeader>, GpError>>,
}

impl GpFile {
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        if data.get(..4) != Some(MAGIC) {
            return err(0, "missing GPAK magic");
        }
        let count = usize::from(read_u16(&data, 4)?);
        let dict_offset = read_u32(&data, 8)? as usize;
        let dict_len = usize::from(read_u16(&data, 12)?);
        let dict = dict_offset..dict_offset + dict_len;
        if dict_len > 0 && dict.end > data.len() {
            return err(8, "dictionary out of bounds");
        }
        let offsets = (0..count)
            .map(|i| read_u32(&data, 14 + 4 * i).map(|o| o as usize))
            .collect::<Result<Vec<_>>>()?;
        // A frame's chunks end where the next frame (or the file) begins.
        let mut sorted = offsets.clone();
        sorted.sort_unstable();
        let frame_end = |start: usize| {
            sorted
                .iter()
                .copied()
                .find(|&o| o > start)
                .unwrap_or(data.len())
        };
        let frames = offsets
            .iter()
            .map(|&start| parse_frame_chunks(&data, start, frame_end(start)))
            .collect();
        Ok(Self { data, dict, frames })
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The shared dictionary of palette indices used by [`Encoding::DictLz`] chunks.
    pub fn dictionary(&self) -> &[u8] {
        &self.data[self.dict.clone()]
    }

    pub fn decode_frame(&self, index: usize) -> Result<Frame> {
        let headers = self.frames[index].as_ref().map_err(Clone::clone)?;
        let layers = headers
            .iter()
            .map(|h| decode_chunk(&self.data, h, self.dictionary()))
            .collect::<Result<Vec<_>>>()?;
        Ok(Frame { layers })
    }
}

fn parse_frame_chunks(data: &[u8], start: usize, frame_end: usize) -> Result<Vec<ChunkHeader>> {
    let mut chunks: Vec<ChunkHeader> = Vec::new();
    let mut rel = 0i64;
    loop {
        if chunks.len() >= MAX_CHUNKS_PER_FRAME {
            return err(start, "too many chunks in frame");
        }
        let offset = usize::try_from(start as i64 + rel).or(err(start, "negative chunk link"))?;
        let header = data.get(offset..offset + CHUNK_HEADER_LEN).ok_or(GpError {
            offset,
            reason: "chunk header out of bounds",
        })?;
        let next = i32::from_le_bytes(header[0..4].try_into().unwrap());
        let i16_at = |i: usize| i16::from_le_bytes([header[i], header[i + 1]]);
        let end = if next == -1 {
            frame_end
        } else {
            let end = start as i64 + i64::from(next);
            if end <= offset as i64 {
                return err(offset, "chunk link does not advance");
            }
            end as usize
        };
        chunks.push(ChunkHeader {
            offset,
            end: end.min(data.len()),
            x: i16_at(4),
            y: i16_at(6),
            width: i16_at(8) as u16,
            height: i16_at(10) as u16,
            encoding: header[16],
        });
        if next == -1 {
            return Ok(chunks);
        }
        rel = i64::from(next);
    }
}

fn decode_chunk(data: &[u8], h: &ChunkHeader, dict: &[u8]) -> Result<Layer> {
    let encoding = Encoding::from_byte(h.encoding);
    let mut layer = Layer {
        encoding,
        x: h.x,
        y: h.y,
        width: h.width,
        height: h.height,
        runs: Vec::new(),
        pixels: Vec::new(),
    };
    if encoding == Encoding::Empty {
        return Ok(layer);
    }
    let body = &data[h.offset + CHUNK_HEADER_LEN..h.end];
    let shape_end = parse_shape(body, h.height, &mut layer.runs).map_err(|r| GpError {
        offset: h.offset + CHUNK_HEADER_LEN,
        reason: r,
    })?;
    let count: usize = layer.runs.iter().map(|r| usize::from(r.len)).sum();
    // Skip the padding byte that precedes every color section.
    let color = body.get(shape_end + 1..).unwrap_or_default();
    let color_offset = h.offset + CHUNK_HEADER_LEN + shape_end + 1;
    let fail = |reason| GpError {
        offset: color_offset,
        reason,
    };
    layer.pixels = match encoding {
        Encoding::DictLz => decode_dict_lz(color, count, dict).map_err(fail)?,
        Encoding::SelfLz(_) => decode_self_lz(color, count).map_err(fail)?,
        Encoding::Raw(_) => color
            .get(..count)
            .ok_or(fail("raw pixels truncated"))?
            .to_vec(),
        Encoding::PlayerColor => {
            unpack_bits(color, count, 2).ok_or(fail("2-bit pixels truncated"))?
        }
        Encoding::Packed4(_) => {
            unpack_bits(color, count, 4).ok_or(fail("4-bit pixels truncated"))?
        }
        Encoding::Shadow | Encoding::Unknown(_) | Encoding::Empty => vec![0; count],
    };
    Ok(layer)
}

/// Parses `rows` rows of run data, returning the number of bytes consumed.
fn parse_shape(
    body: &[u8],
    rows: u16,
    runs: &mut Vec<Run>,
) -> std::result::Result<usize, &'static str> {
    let byte = |i: usize| body.get(i).copied().ok_or("shape data truncated");
    let mut pos = 0;
    for y in 0..rows {
        let tag = byte(pos)?;
        pos += 1;
        let mut x = 0u16;
        let mut push = |skip: u16, len: u16| {
            x += skip;
            if len > 0 {
                runs.push(Run { x, y, len });
            }
            x += len;
        };
        match tag {
            0 => {}
            1..=0x6F => {
                for _ in 0..tag {
                    push(u16::from(byte(pos)?), u16::from(byte(pos + 1)?));
                    pos += 2;
                }
            }
            0x70..=0x7F => return Err("unsupported shape tag 0x7X"),
            _ => {
                let len_extra = if tag & 0x20 != 0 { 16 } else { 0 };
                let skip_extra = if tag & 0x40 != 0 { 16 } else { 0 };
                for _ in 0..tag & 0x0F {
                    let b = byte(pos)?;
                    push(
                        u16::from(b & 0x0F) + skip_extra,
                        u16::from(b >> 4) + len_extra,
                    );
                    pos += 1;
                }
            }
        }
    }
    Ok(pos)
}

/// Control bytes are read MSB first; a set bit means a 2-byte reference
/// `offset:12 | (len-3):4` into the file dictionary, a clear bit a literal index.
fn decode_dict_lz(
    src: &[u8],
    count: usize,
    dict: &[u8],
) -> std::result::Result<Vec<u8>, &'static str> {
    let mut out = Vec::with_capacity(count);
    let mut pos = 0;
    while out.len() < count {
        let ctrl = *src.get(pos).ok_or("compressed pixels truncated")?;
        pos += 1;
        for bit in (0..8).rev() {
            if out.len() >= count {
                break;
            }
            if ctrl >> bit & 1 == 0 {
                out.push(*src.get(pos).ok_or("compressed pixels truncated")?);
                pos += 1;
            } else {
                let v = read_u16_at(src, pos).ok_or("compressed pixels truncated")?;
                pos += 2;
                let offset = usize::from(v & 0x0FFF);
                let len = usize::from(v >> 12) + 3;
                let take = len.min(count - out.len());
                match dict.get(offset..offset + take) {
                    Some(run) => out.extend_from_slice(run),
                    None => out.resize(out.len() + take, 0),
                }
            }
        }
    }
    Ok(out)
}

/// Control bytes are read LSB first; a set bit means a 2-byte reference
/// `distance:13 | (len-3):3` that copies from `distance + 1` pixels back in the output.
fn decode_self_lz(src: &[u8], count: usize) -> std::result::Result<Vec<u8>, &'static str> {
    let mut out: Vec<u8> = Vec::with_capacity(count);
    let mut pos = 0;
    while out.len() < count {
        let ctrl = *src.get(pos).ok_or("compressed pixels truncated")?;
        pos += 1;
        for bit in 0..8 {
            if out.len() >= count {
                break;
            }
            if ctrl >> bit & 1 == 0 {
                out.push(*src.get(pos).ok_or("compressed pixels truncated")?);
                pos += 1;
            } else {
                let v = read_u16_at(src, pos).ok_or("compressed pixels truncated")?;
                pos += 2;
                let distance = usize::from(v & 0x1FFF);
                let len = (usize::from(v >> 13) + 3).min(count - out.len());
                let start = out.len().checked_sub(distance + 1);
                for i in 0..len {
                    // Overlapping copies are intentional (run-length style repeats).
                    let b = start.and_then(|s| out.get(s + i)).copied().unwrap_or(0);
                    out.push(b);
                }
            }
        }
    }
    Ok(out)
}

/// Unpacks `count` values of `bits` width, least significant bits first.
fn unpack_bits(src: &[u8], count: usize, bits: u32) -> Option<Vec<u8>> {
    let per_byte = (8 / bits) as usize;
    let mask = (1u8 << bits) - 1;
    let bytes = src.get(..count.div_ceil(per_byte))?;
    Some(
        (0..count)
            .map(|i| bytes[i / per_byte] >> ((i % per_byte) as u32 * bits) & mask)
            .collect(),
    )
}

fn read_u16_at(data: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*data.get(pos)?, *data.get(pos + 1)?]))
}

fn read_u16(data: &[u8], pos: usize) -> Result<u16> {
    read_u16_at(data, pos).ok_or(GpError {
        offset: pos,
        reason: "unexpected end of file",
    })
}

fn read_u32(data: &[u8], pos: usize) -> Result<u32> {
    data.get(pos..pos + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(GpError {
            offset: pos,
            reason: "unexpected end of file",
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_pairs_and_nibbles() {
        // Row 0: 2 pairs (skip 1, len 2), (skip 3, len 1).
        // Row 1: empty. Row 2: tag 0xA1 = 1 nibble pair, +16 len: 0x21 -> skip 1, len 2+16.
        let body = [2, 1, 2, 3, 1, 0, 0xA1, 0x21];
        let mut runs = Vec::new();
        assert_eq!(parse_shape(&body, 3, &mut runs), Ok(body.len()));
        assert_eq!(
            runs,
            vec![
                Run { x: 1, y: 0, len: 2 },
                Run { x: 6, y: 0, len: 1 },
                Run {
                    x: 1,
                    y: 2,
                    len: 18
                },
            ]
        );
    }

    #[test]
    fn dict_lz_literals_and_references() {
        let dict = [10, 11, 12, 13, 14];
        // ctrl 0b0100_0000: literal 7, then reference offset 1 len 3+1, then literal 9.
        let src = [0b0100_0000, 7, 0x01, 0x10, 9];
        assert_eq!(
            decode_dict_lz(&src, 6, &dict).unwrap(),
            vec![7, 11, 12, 13, 14, 9]
        );
        // A final reference is clamped to the remaining pixel count.
        assert_eq!(decode_dict_lz(&src, 3, &dict).unwrap(), vec![7, 11, 12]);
    }

    #[test]
    fn self_lz_copies_own_output() {
        // ctrl 0b0000_0100 (LSB first): literals 1, 2, then reference 2 back, len 3+1.
        let src = [0b0000_0100, 1, 2, 0x01, 0x20];
        assert_eq!(decode_self_lz(&src, 6).unwrap(), vec![1, 2, 1, 2, 1, 2]);
        // Distance 0 repeats the previous pixel.
        let src = [0b0000_0100, 1, 2, 0x00, 0x00];
        assert_eq!(decode_self_lz(&src, 5).unwrap(), vec![1, 2, 2, 2, 2]);
    }

    #[test]
    fn unpacks_low_bits_first() {
        assert_eq!(unpack_bits(&[0b1110_0100], 4, 2).unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(unpack_bits(&[0x21, 0x03], 3, 4).unwrap(), vec![1, 2, 3]);
    }
}
