//! Bink 1 video (revisions `c`..`i`): YUV 4:2:0 frames coded as 8x8 blocks.
//!
//! Each plane is coded row of blocks by row of blocks. Before every block row the decoder
//! refills nine "bundles" (block types, colours, motion vectors, DC values, run lengths,
//! ...) from Huffman- or delta-coded runs; the blocks of the row then take values from
//! those bundles in order, and read a few fields (DCT coefficients, residues, run flags)
//! directly from the bitstream. Blocks either copy the previous frame (optionally moved
//! by a motion vector, with a DCT or residue correction), fill or pattern-fill colours,
//! or code a fresh DCT block. A "scaled" block covers 16x16 pixels with an 8x8 block
//! upscaled 2x.

use crate::BinkError;
use crate::bits::BitReader;
use crate::huffman::{Codebooks, Tree};
use crate::idct::{idct_add, idct_put};
use crate::tables::{INTER_QUANT, INTRA_QUANT, PATTERNS, SCAN};

// Block types.
const SKIP: u8 = 0;
const SCALED: u8 = 1;
const MOTION: u8 = 2;
const RUN: u8 = 3;
const RESIDUE: u8 = 4;
const INTRA: u8 = 5;
const FILL: u8 = 6;
const INTER: u8 = 7;
const PATTERN: u8 = 8;
const RAW: u8 = 9;

// Byte bundles, in bitstream order (the two DC bundles sit between Y_OFF and RUN).
const BLOCK_TYPES: usize = 0;
const SUB_BLOCK_TYPES: usize = 1;
const COLORS: usize = 2;
const PATTERN_BITS: usize = 3;
const X_OFF: usize = 4;
const Y_OFF: usize = 5;
const RUNS: usize = 6;

/// Run lengths of block type codes 12..15.
const BLOCK_TYPE_RUNS: [usize; 4] = [4, 8, 12, 32];
/// Bits of the first DC value of a DC bundle.
const DC_START_BITS: u32 = 11;

struct Plane {
    data: Vec<u8>,
    stride: usize,
    /// Size in 8x8 blocks.
    bw: usize,
    bh: usize,
    /// Width in pixels.
    width: usize,
}

impl Plane {
    fn new(width: usize, height: usize) -> Self {
        let (bw, bh) = (width.div_ceil(8), height.div_ceil(8));
        // Room for a 16x16 scaled block in the last column/row of an odd block count.
        let stride = bw.next_multiple_of(2) * 8;
        let rows = bh.next_multiple_of(2) * 8;
        Self { data: vec![0; stride * rows], stride, bw, bh, width }
    }
}

/// Decoded values of one kind with read/write positions.
#[derive(Default)]
struct Bundle<T> {
    data: Vec<T>,
    /// Width of the "number of values" field.
    count_bits: u32,
    tree: Tree,
    /// Write position.
    dec: usize,
    /// Read position.
    ptr: usize,
    /// A zero count ended this bundle for the rest of the plane.
    done: bool,
}

impl<T: Copy + Default> Bundle<T> {
    fn with_capacity(n: usize) -> Self {
        Self { data: vec![T::default(); n], count_bits: 0, tree: Tree::default(), dec: 0, ptr: 0, done: false }
    }

    fn reset(&mut self) {
        self.dec = 0;
        self.ptr = 0;
        self.done = false;
    }

    /// Reads the number of new values, unless values from an earlier refill remain.
    fn start(&mut self, r: &mut BitReader) -> Result<Option<usize>, BinkError> {
        if self.done || self.dec > self.ptr {
            return Ok(None);
        }
        let count = r.read(self.count_bits) as usize;
        if count == 0 {
            self.done = true;
            return Ok(None);
        }
        if self.dec + count > self.data.len() {
            return Err(BinkError("bundle overflow"));
        }
        Ok(Some(count))
    }

    fn push(&mut self, v: T) {
        if let Some(slot) = self.data.get_mut(self.dec) {
            *slot = v;
            self.dec += 1;
        }
    }

    fn fill(&mut self, v: T, n: usize) {
        let end = (self.dec + n).min(self.data.len());
        self.data[self.dec..end].fill(v);
        self.dec = end;
    }

    #[inline]
    fn next(&mut self) -> T {
        let v = self.data.get(self.ptr).copied().unwrap_or_default();
        self.ptr += 1;
        v
    }
}

/// Decodes the video stream of one file frame by frame.
pub struct VideoDecoder {
    width: usize,
    height: usize,
    revision: u8,
    has_alpha: bool,
    books: Codebooks,
    /// The last decoded frame (Y, U, V and optional alpha).
    frame: Vec<Plane>,
    /// The frame being decoded; swapped with `frame` afterwards.
    next: Vec<Plane>,
    bundles: [Bundle<u8>; 7],
    intra_dc: Bundle<i16>,
    inter_dc: Bundle<i16>,
    color_high: [Tree; 16],
    color_last: u8,
}

impl VideoDecoder {
    pub fn new(width: u32, height: u32, revision: u8, has_alpha: bool) -> Self {
        let (width, height) = (width as usize, height as usize);
        let planes = || {
            let mut planes = vec![
                Plane::new(width, height),
                Plane::new(width.div_ceil(2), height.div_ceil(2)),
                Plane::new(width.div_ceil(2), height.div_ceil(2)),
            ];
            if has_alpha {
                planes.push(Plane::new(width, height));
            }
            planes
        };
        let blocks = width.div_ceil(8) * height.div_ceil(8);
        let mut frame = planes();
        // Start from black (Y=16, U=V=128) in case the first frame is not a keyframe.
        for (i, plane) in frame.iter_mut().enumerate() {
            plane.data.fill(match i {
                0 => 16,
                3 => 255,
                _ => 128,
            });
        }
        Self {
            width,
            height,
            revision,
            has_alpha,
            books: Codebooks::new(),
            frame,
            next: planes(),
            bundles: std::array::from_fn(|_| Bundle::with_capacity(blocks * 64)),
            intra_dc: Bundle::with_capacity(blocks * 32),
            inter_dc: Bundle::with_capacity(blocks * 32),
            color_high: [Tree::default(); 16],
            color_last: 0,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Decodes one frame's video data; the result replaces the current frame.
    pub fn decode(&mut self, data: &[u8]) -> Result<(), BinkError> {
        let mut r = BitReader::new(data);
        if self.has_alpha {
            if self.revision >= b'i' {
                r.skip(32);
            }
            self.decode_plane(&mut r, 3)?;
        }
        if self.revision >= b'i' {
            r.skip(32);
        }
        let swap = self.revision >= b'h';
        let mut decoded = [false; 3];
        for plane in 0..3 {
            // From revision h on, the chroma planes are stored as V then U.
            let index = if plane == 0 || !swap { plane } else { plane ^ 3 };
            self.decode_plane(&mut r, index)?;
            decoded[index] = true;
            if r.position() >= r.bit_len() {
                break;
            }
        }
        // Planes missing from a short packet stay as they were.
        for (index, done) in decoded.iter().enumerate() {
            if !done {
                self.next[index].data.copy_from_slice(&self.frame[index].data);
            }
        }
        std::mem::swap(&mut self.frame, &mut self.next);
        Ok(())
    }

    fn init_bundles(&mut self, width: usize, bw: usize) {
        let width = width.next_multiple_of(8);
        let bits = |n: usize| (n + 511).ilog2() + 1;
        self.bundles[BLOCK_TYPES].count_bits = bits(width >> 3);
        self.bundles[SUB_BLOCK_TYPES].count_bits = bits(width >> 4);
        self.bundles[COLORS].count_bits = bits(bw * 64);
        self.intra_dc.count_bits = bits(width >> 3);
        self.inter_dc.count_bits = bits(width >> 3);
        self.bundles[X_OFF].count_bits = bits(width >> 3);
        self.bundles[Y_OFF].count_bits = bits(width >> 3);
        self.bundles[PATTERN_BITS].count_bits = bits(bw << 3);
        self.bundles[RUNS].count_bits = bits(bw * 48);
    }

    /// Reads the Huffman trees of all bundles and empties them.
    fn read_trees(&mut self, r: &mut BitReader) {
        for (i, bundle) in self.bundles.iter_mut().enumerate() {
            if i == COLORS {
                for tree in &mut self.color_high {
                    *tree = Tree::read(r);
                }
                self.color_last = 0;
            }
            bundle.tree = Tree::read(r);
            bundle.reset();
        }
        self.intra_dc.reset();
        self.inter_dc.reset();
    }

    fn read_block_types(&mut self, r: &mut BitReader, which: usize) -> Result<(), BinkError> {
        let books = &self.books;
        let b = &mut self.bundles[which];
        let Some(count) = b.start(r)? else { return Ok(()) };
        let end = b.dec + count;
        if r.bit() {
            let v = r.read(4) as u8;
            b.fill(v, count);
            return Ok(());
        }
        let mut last = 0;
        while b.dec < end {
            let v = b.tree.decode(books, r);
            if v < 12 {
                last = v;
                b.push(v);
            } else {
                let run = BLOCK_TYPE_RUNS[usize::from(v - 12)];
                if end - b.dec < run {
                    return Err(BinkError("block type run too long"));
                }
                b.fill(last, run);
            }
        }
        Ok(())
    }

    fn read_color(&mut self, r: &mut BitReader) -> u8 {
        let high = self.color_high[usize::from(self.color_last)].decode(&self.books, r);
        self.color_last = high;
        let low = self.bundles[COLORS].tree.decode(&self.books, r);
        let v = (high << 4) | low;
        if self.revision < b'i' {
            // Older revisions store colours as sign + magnitude around 128.
            let magnitude = i32::from(v & 0x7F);
            let signed = if v & 0x80 != 0 { -magnitude } else { magnitude };
            return (signed + 0x80) as u8;
        }
        v
    }

    fn read_colors(&mut self, r: &mut BitReader) -> Result<(), BinkError> {
        let Some(count) = self.bundles[COLORS].start(r)? else { return Ok(()) };
        if r.bit() {
            let v = self.read_color(r);
            self.bundles[COLORS].fill(v, count);
        } else {
            for _ in 0..count {
                let v = self.read_color(r);
                self.bundles[COLORS].push(v);
            }
        }
        Ok(())
    }

    fn read_patterns(&mut self, r: &mut BitReader) -> Result<(), BinkError> {
        let books = &self.books;
        let b = &mut self.bundles[PATTERN_BITS];
        let Some(count) = b.start(r)? else { return Ok(()) };
        for _ in 0..count {
            let low = b.tree.decode(books, r);
            let high = b.tree.decode(books, r);
            b.push(low | (high << 4));
        }
        Ok(())
    }

    fn read_motion(&mut self, r: &mut BitReader, which: usize) -> Result<(), BinkError> {
        let books = &self.books;
        let b = &mut self.bundles[which];
        let Some(count) = b.start(r)? else { return Ok(()) };
        let signed = |r: &mut BitReader, v: u8| if v != 0 && r.bit() { v.wrapping_neg() } else { v };
        if r.bit() {
            let v = r.read(4) as u8;
            let v = signed(r, v);
            b.fill(v, count);
        } else {
            for _ in 0..count {
                let v = b.tree.decode(books, r);
                let v = signed(r, v);
                b.push(v);
            }
        }
        Ok(())
    }

    fn read_runs(&mut self, r: &mut BitReader) -> Result<(), BinkError> {
        let books = &self.books;
        let b = &mut self.bundles[RUNS];
        let Some(count) = b.start(r)? else { return Ok(()) };
        if r.bit() {
            let v = r.read(4) as u8;
            b.fill(v, count);
        } else {
            for _ in 0..count {
                let v = b.tree.decode(books, r);
                b.push(v);
            }
        }
        Ok(())
    }

    /// DC values: a start value, then groups of up to 8 deltas sharing one bit width.
    fn read_dcs(b: &mut Bundle<i16>, r: &mut BitReader, has_sign: bool) -> Result<(), BinkError> {
        let Some(count) = b.start(r)? else { return Ok(()) };
        let mut v = r.read(DC_START_BITS - u32::from(has_sign)) as i32;
        if v != 0 && has_sign && r.bit() {
            v = -v;
        }
        b.push(v as i16);
        let rest = count - 1;
        for group in (0..rest).step_by(8) {
            let n = (rest - group).min(8);
            let width = r.read(4);
            for _ in 0..n {
                if width != 0 {
                    let mut delta = r.read(width) as i32;
                    if delta != 0 && r.bit() {
                        delta = -delta;
                    }
                    v += delta;
                    if !(-32768..=32767).contains(&v) {
                        return Err(BinkError("DC value out of range"));
                    }
                }
                b.push(v as i16);
            }
        }
        Ok(())
    }

    fn decode_plane(&mut self, r: &mut BitReader, index: usize) -> Result<(), BinkError> {
        let (bw, width) = (self.next[index].bw, self.next[index].width);
        self.init_bundles(width.max(8), bw);
        self.read_trees(r);

        let mut next = std::mem::take(&mut self.next);
        let frame = std::mem::take(&mut self.frame);
        let result = self.decode_blocks(r, &mut next[index], &frame[index]);
        self.next = next;
        self.frame = frame;
        result?;
        if r.overrun() {
            return Err(BinkError("plane data truncated"));
        }
        r.align32();
        Ok(())
    }

    fn decode_blocks(&mut self, r: &mut BitReader, dst: &mut Plane, prev: &Plane) -> Result<(), BinkError> {
        let stride = dst.stride;
        let (bw, bh) = (dst.bw, dst.bh);
        // Last valid top-left corner of a reference block.
        let ref_end = ((bh - 1) * 8 * stride + (bw - 1) * 8) as isize;
        let mut ublock = [0u8; 64];
        let mut coeffs = [0i32; 64];
        let mut coded = [0u8; 64];

        for by in 0..bh {
            self.read_block_types(r, BLOCK_TYPES)?;
            self.read_block_types(r, SUB_BLOCK_TYPES)?;
            self.read_colors(r)?;
            self.read_patterns(r)?;
            self.read_motion(r, X_OFF)?;
            self.read_motion(r, Y_OFF)?;
            Self::read_dcs(&mut self.intra_dc, r, false)?;
            Self::read_dcs(&mut self.inter_dc, r, true)?;
            self.read_runs(r)?;

            let mut bx = 0;
            while bx < bw {
                let at = by * 8 * stride + bx * 8;
                let kind = self.bundles[BLOCK_TYPES].next();
                if kind == SCALED && (by & 1 == 1 || bx & 1 == 1) {
                    // The lower/right part of a 16x16 block decoded earlier.
                    bx += 2;
                    continue;
                }
                let moved = |bundles: &mut [Bundle<u8>; 7]| -> Result<usize, BinkError> {
                    let x = isize::from(bundles[X_OFF].next() as i8);
                    let y = isize::from(bundles[Y_OFF].next() as i8);
                    let pos = at as isize + x + y * stride as isize;
                    if pos < 0 || pos > ref_end {
                        return Err(BinkError("motion vector out of bounds"));
                    }
                    Ok(pos as usize)
                };
                match kind {
                    SKIP => copy_block(&mut dst.data, at, &prev.data, at, stride),
                    SCALED => {
                        let sub = self.bundles[SUB_BLOCK_TYPES].next();
                        match sub {
                            RUN => {
                                let pattern = &PATTERNS[r.read(4) as usize];
                                self.read_run_block(r, pattern, |i, v| ublock[usize::from(i)] = v)?;
                            }
                            INTRA => {
                                let dc = self.intra_dc.next();
                                let (q, n) = read_dct(r, dc, &mut coeffs, &mut coded)?;
                                unquantize(&mut coeffs, &INTRA_QUANT[q], &coded[..n]);
                                idct_put(&mut ublock, 8, &coeffs);
                            }
                            FILL => {
                                let v = self.bundles[COLORS].next();
                                for row in dst.data[at..].chunks_mut(stride).take(16) {
                                    row[..16].fill(v);
                                }
                            }
                            PATTERN => self.read_pattern_block(&mut ublock, 8),
                            RAW => ublock.iter_mut().for_each(|p| *p = self.bundles[COLORS].next()),
                            _ => return Err(BinkError("invalid 16x16 block type")),
                        }
                        if sub != FILL {
                            scale_block(&ublock, &mut dst.data[at..], stride);
                        }
                        bx += 1;
                    }
                    MOTION => {
                        let src = moved(&mut self.bundles)?;
                        copy_block(&mut dst.data, at, &prev.data, src, stride);
                    }
                    RUN => {
                        let pattern = &PATTERNS[r.read(4) as usize];
                        let data = &mut dst.data;
                        self.read_run_block(r, pattern, |i, v| {
                            data[at + usize::from(i & 7) + usize::from(i >> 3) * stride] = v;
                        })?;
                    }
                    RESIDUE => {
                        let src = moved(&mut self.bundles)?;
                        copy_block(&mut dst.data, at, &prev.data, src, stride);
                        let mut residue = [0i16; 64];
                        let masks = r.read(7) as i32;
                        read_residue(r, &mut residue, masks)?;
                        for (row, values) in dst.data[at..].chunks_mut(stride).zip(residue.as_chunks::<8>().0.iter()) {
                            for (p, &v) in row[..8].iter_mut().zip(values) {
                                *p = (i32::from(*p) + i32::from(v)) as u8;
                            }
                        }
                    }
                    INTRA => {
                        let dc = self.intra_dc.next();
                        let (q, n) = read_dct(r, dc, &mut coeffs, &mut coded)?;
                        unquantize(&mut coeffs, &INTRA_QUANT[q], &coded[..n]);
                        idct_put(&mut dst.data[at..], stride, &coeffs);
                    }
                    FILL => {
                        let v = self.bundles[COLORS].next();
                        for row in dst.data[at..].chunks_mut(stride).take(8) {
                            row[..8].fill(v);
                        }
                    }
                    INTER => {
                        let src = moved(&mut self.bundles)?;
                        copy_block(&mut dst.data, at, &prev.data, src, stride);
                        let dc = self.inter_dc.next();
                        let (q, n) = read_dct(r, dc, &mut coeffs, &mut coded)?;
                        unquantize(&mut coeffs, &INTER_QUANT[q], &coded[..n]);
                        idct_add(&mut dst.data[at..], stride, &coeffs);
                    }
                    PATTERN => self.read_pattern_block(&mut dst.data[at..], stride),
                    RAW => {
                        for row in dst.data[at..].chunks_mut(stride).take(8) {
                            for p in &mut row[..8] {
                                *p = self.bundles[COLORS].next();
                            }
                        }
                    }
                    _ => return Err(BinkError("invalid block type")),
                }
                bx += 1;
            }
        }
        Ok(())
    }

    /// Fills 64 pixels in `pattern` order with runs that are either one colour or one
    /// colour per pixel.
    fn read_run_block(
        &mut self,
        r: &mut BitReader,
        pattern: &[u8; 64],
        mut put: impl FnMut(u8, u8),
    ) -> Result<(), BinkError> {
        let mut i = 0;
        while i < 63 {
            let run = usize::from(self.bundles[RUNS].next()) + 1;
            if i + run > 64 {
                return Err(BinkError("run out of bounds"));
            }
            if r.bit() {
                let v = self.bundles[COLORS].next();
                pattern[i..i + run].iter().for_each(|&p| put(p, v));
            } else {
                for &p in &pattern[i..i + run] {
                    put(p, self.bundles[COLORS].next());
                }
            }
            i += run;
        }
        if i == 63 {
            put(pattern[63], self.bundles[COLORS].next());
        }
        Ok(())
    }

    /// Two colours and one 8-bit mask per row (bit 0 = leftmost pixel).
    fn read_pattern_block(&mut self, dst: &mut [u8], stride: usize) {
        let colors = [self.bundles[COLORS].next(), self.bundles[COLORS].next()];
        for row in dst.chunks_mut(stride).take(8) {
            let mut mask = self.bundles[PATTERN_BITS].next();
            for p in &mut row[..8] {
                *p = colors[usize::from(mask & 1)];
                mask >>= 1;
            }
        }
    }

    /// Converts the current frame to RGBA (`width * height * 4` bytes) using BT.601
    /// limited-range coefficients.
    pub fn to_rgba(&self, out: &mut [u8]) {
        let (y_plane, u_plane, v_plane) = (&self.frame[0], &self.frame[1], &self.frame[2]);
        let alpha = self.frame.get(3);
        for (row, line) in out.chunks_exact_mut(self.width * 4).take(self.height).enumerate() {
            let ys = &y_plane.data[row * y_plane.stride..];
            let us = &u_plane.data[(row / 2) * u_plane.stride..];
            let vs = &v_plane.data[(row / 2) * v_plane.stride..];
            for (col, px) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let y = (i32::from(ys[col]) - 16) * 298;
                let u = i32::from(us[col / 2]) - 128;
                let v = i32::from(vs[col / 2]) - 128;
                px[0] = ((y + 409 * v + 128) >> 8).clamp(0, 255) as u8;
                px[1] = ((y - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8;
                px[2] = ((y + 516 * u + 128) >> 8).clamp(0, 255) as u8;
                px[3] = alpha.map_or(255, |a| a.data[row * a.stride + col]);
            }
        }
    }
}

/// Copies an 8x8 block from `src[from..]` to `dst[to..]` (both with row stride `stride`).
#[inline]
fn copy_block(dst: &mut [u8], to: usize, src: &[u8], from: usize, stride: usize) {
    for row in 0..8 {
        let (d, s) = (to + row * stride, from + row * stride);
        dst[d..d + 8].copy_from_slice(&src[s..s + 8]);
    }
}

/// Draws an 8x8 block at twice the size.
fn scale_block(src: &[u8; 64], dst: &mut [u8], stride: usize) {
    for (y, line) in src.as_chunks::<8>().0.iter().enumerate() {
        for half in 0..2 {
            let row = &mut dst[(2 * y + half) * stride..][..16];
            for (pair, &v) in row.as_chunks_mut::<2>().0.iter_mut().zip(line) {
                pair.fill(v);
            }
        }
    }
}

fn read_coefficient(r: &mut BitReader, bits: u32) -> i32 {
    if bits == 0 {
        return if r.bit() { -1 } else { 1 };
    }
    let t = (r.read(bits) | (1 << bits)) as i32;
    if r.bit() { -t } else { t }
}

/// Work list of coefficient groups for the bit-plane coders of DCT blocks and residues.
///
/// Entries are `(first coefficient, mode)`: mode 0 is a group of 16 coefficients (four
/// quads) not yet split, 1 the same group after its first quad was handled, 2 a quad of
/// four, and 3 a single coefficient. `(0, 0)` marks a finished entry. Entries are added
/// at the front (single coefficients) and at the back (quads).
struct CoefList {
    coef: [u8; 128],
    mode: [u8; 128],
    start: usize,
    end: usize,
}

impl CoefList {
    fn new(initial: &[(u8, u8)]) -> Self {
        let mut list = Self { coef: [0; 128], mode: [0; 128], start: 64, end: 64 };
        for &(c, m) in initial {
            list.coef[list.end] = c;
            list.mode[list.end] = m;
            list.end += 1;
        }
        list
    }

    fn push_front(&mut self, c: u8) -> Result<(), BinkError> {
        if self.start == 0 {
            return Err(BinkError("coefficient list overflow"));
        }
        self.start -= 1;
        self.coef[self.start] = c;
        self.mode[self.start] = 3;
        Ok(())
    }

    fn push_back(&mut self, c: u8) -> Result<(), BinkError> {
        if self.end == 128 || c >= 64 {
            return Err(BinkError("coefficient list overflow"));
        }
        self.coef[self.end] = c;
        self.mode[self.end] = 2;
        self.end += 1;
        Ok(())
    }
}

/// What a bit-plane coder does with a newly significant coefficient (index in coding
/// order); returns `false` to stop decoding the block.
type Significant<'a> = &'a mut dyn FnMut(&mut BitReader, u8) -> Result<bool, BinkError>;

/// One pass over the work list: every live entry whose flag bit is set is refined.
fn coef_pass(r: &mut BitReader, list: &mut CoefList, on_coef: Significant) -> Result<bool, BinkError> {
    let mut pos = list.start;
    while pos < list.end {
        let (c, mode) = (list.coef[pos], list.mode[pos]);
        if (c == 0 && mode == 0) || !r.bit() {
            pos += 1;
            continue;
        }
        match mode {
            0 | 2 => {
                if mode == 0 {
                    // Handle the first quad now and keep the other three for later.
                    list.coef[pos] = c + 4;
                    list.mode[pos] = 1;
                } else {
                    list.coef[pos] = 0;
                    list.mode[pos] = 0;
                    pos += 1;
                }
                for k in c..c + 4 {
                    if r.bit() {
                        list.push_front(k)?;
                    } else if !on_coef(r, k)? {
                        return Ok(false);
                    }
                }
            }
            1 => {
                list.mode[pos] = 2;
                for k in 1..=3 {
                    list.push_back(c + 4 * k)?;
                }
            }
            _ => {
                list.coef[pos] = 0;
                list.mode[pos] = 0;
                pos += 1;
                if !on_coef(r, c)? {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

/// Reads the AC coefficients of a DCT block (DC given) and its quantizer index. `coded`
/// receives the coding-order indices of the non-zero coefficients; returns the quantizer
/// index and their count.
fn read_dct(
    r: &mut BitReader,
    dc: i16,
    block: &mut [i32; 64],
    coded: &mut [u8; 64],
) -> Result<(usize, usize), BinkError> {
    block.fill(0);
    block[0] = i32::from(dc);
    let mut count = 0usize;
    let mut list = CoefList::new(&[(4, 0), (24, 0), (44, 0), (1, 3), (2, 3), (3, 3)]);
    let planes = r.read(4);
    for bits in (0..planes).rev() {
        coef_pass(r, &mut list, &mut |r, k| {
            if count == 64 {
                return Err(BinkError("too many coefficients"));
            }
            block[usize::from(SCAN[usize::from(k)])] = read_coefficient(r, bits);
            coded[count] = k;
            count += 1;
            Ok(true)
        })?;
    }
    Ok((r.read(4) as usize, count))
}

/// Scales the DC and the coded coefficients (given in coding order) by the quantizer.
fn unquantize(block: &mut [i32; 64], quant: &[u32; 64], coded: &[u8]) {
    block[0] = block[0].wrapping_mul(quant[0] as i32) >> 11;
    for &k in coded {
        let pos = usize::from(SCAN[usize::from(k)]);
        block[pos] = block[pos].wrapping_mul(quant[usize::from(k)] as i32) >> 11;
    }
}

/// Reads a motion-compensation residue: bit planes of coefficients added directly to the
/// pixels (no transform). At most `masks + 1` refinements are read.
fn read_residue(r: &mut BitReader, block: &mut [i16; 64], mut masks: i32) -> Result<(), BinkError> {
    let mut list = CoefList::new(&[(4, 0), (24, 0), (44, 0), (0, 2)]);
    let mut significant = [0u8; 64];
    let mut count = 0usize;
    let mut mask = 1i16 << r.read(3);
    while mask != 0 {
        for &p in &significant[..count] {
            if !r.bit() {
                continue;
            }
            let v = &mut block[usize::from(p)];
            *v = if *v < 0 { *v - mask } else { *v + mask };
            masks -= 1;
            if masks < 0 {
                return Ok(());
            }
        }
        let go_on = coef_pass(r, &mut list, &mut |r, k| {
            if count == 64 {
                return Err(BinkError("too many residue coefficients"));
            }
            let p = SCAN[usize::from(k)];
            significant[count] = p;
            count += 1;
            block[usize::from(p)] = if r.bit() { -mask } else { mask };
            masks -= 1;
            Ok(masks >= 0)
        })?;
        if !go_on {
            return Ok(());
        }
        mask >>= 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::tests::pack;

    /// Bits of one 8x8 plane (one block) filled with `color`, starting at a 32-bit boundary.
    fn fill_plane(fields: &mut Vec<(u32, u32)>, color: u8) {
        // 23 trees, all code set 0 (raw nibbles): block types, sub types, 16 colour-high
        // trees, colours, pattern, x, y, runs.
        fields.extend(std::iter::repeat_n((0, 4), 23));
        // One block type value, run-filled with FILL; count fields are 10 bits wide.
        fields.extend([(1, 10), (1, 1), (u32::from(FILL), 4)]);
        fields.push((0, 9)); // no sub block types (9-bit count: (8 >> 4) + 511 < 512)
        // One colour, coded individually: high nibble then low nibble.
        fields.extend([(1, 10), (0, 1), (u32::from(color >> 4), 4), (u32::from(color & 15), 4)]);
        // No patterns, motion values, DCs or runs.
        fields.extend(std::iter::repeat_n((0, 10), 6));
        let used: u32 = fields.iter().map(|f| f.1).sum();
        if !used.is_multiple_of(32) {
            fields.push((0, 32 - used % 32));
        }
    }

    #[test]
    fn decodes_fill_blocks_into_rgba() {
        let mut fields = vec![(0, 32)];
        fill_plane(&mut fields, 235); // Y
        fill_plane(&mut fields, 128); // V (stored before U from revision h on)
        fill_plane(&mut fields, 128); // U
        let data = pack(&fields);
        let mut decoder = VideoDecoder::new(8, 8, b'i', false);
        decoder.decode(&data).unwrap();
        assert!(decoder.frame[0].data[..8].iter().all(|&y| y == 235));
        let mut rgba = vec![0; 8 * 8 * 4];
        decoder.to_rgba(&mut rgba);
        assert_eq!(rgba[..4], [255, 255, 255, 255]);
    }

    #[test]
    fn truncated_frames_fail_without_panicking() {
        let mut fields = vec![(0, 32)];
        fill_plane(&mut fields, 100);
        let data = pack(&fields);
        let mut decoder = VideoDecoder::new(64, 48, b'i', false);
        for len in 0..data.len() {
            let _ = decoder.decode(&data[..len]);
        }
    }

    #[test]
    fn residue_refines_signs_and_magnitudes() {
        // mask 1 << 1; the (0, 2) quad entry is flagged, all four coefficients coded
        // directly with signs +, -, +, +; then stop after the masks budget.
        let fields =
            [(1, 3), (0, 1), (0, 1), (0, 1), (1, 1), (0, 1), (0, 1), (0, 1), (1, 1), (0, 1), (0, 1), (0, 1), (0, 1)];
        let bytes = pack(&fields);
        let mut block = [0i16; 64];
        read_residue(&mut BitReader::new(&bytes), &mut block, 3).unwrap();
        assert_eq!([block[0], block[1], block[8], block[9]], [2, -2, 2, 2]);
    }
}
