//! Little-endian bit reader: Bink packs fields starting at the least significant bit of
//! each byte.
//!
//! Reading past the end yields zero bits instead of failing on every call; callers check
//! [`BitReader::overrun`] at safe points (end of a plane or audio block).

pub struct BitReader<'a> {
    data: &'a [u8],
    /// Position in bits from the start of `data`.
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// 64 bits starting at byte `byte`, zero-padded past the end.
    #[inline]
    fn load(&self, byte: usize) -> u64 {
        match self.data.get(byte..byte + 8) {
            Some(chunk) => u64::from_le_bytes(chunk.try_into().unwrap_or_default()),
            None => {
                let mut buf = [0u8; 8];
                if let Some(rest) = self.data.get(byte..) {
                    buf[..rest.len()].copy_from_slice(rest);
                }
                u64::from_le_bytes(buf)
            }
        }
    }

    /// The next `n` bits (at most 32) without consuming them.
    #[inline]
    pub fn peek(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        let v = self.load(self.pos >> 3) >> (self.pos & 7);
        (v & ((1u64 << n) - 1)) as u32
    }

    #[inline]
    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    /// Reads `n` bits (at most 32) as an unsigned number, first bit read is bit 0.
    #[inline]
    pub fn read(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.pos += n as usize;
        v
    }

    #[inline]
    pub fn bit(&mut self) -> bool {
        self.read(1) != 0
    }

    /// Bits consumed so far.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Total bits in the buffer.
    pub fn bit_len(&self) -> usize {
        self.data.len() * 8
    }

    /// Bits not yet read; negative after an overrun.
    pub fn left(&self) -> isize {
        self.bit_len() as isize - self.pos as isize
    }

    /// True when more bits were read than the buffer holds.
    pub fn overrun(&self) -> bool {
        self.pos > self.bit_len()
    }

    /// Skips to the next multiple of 32 bits.
    pub fn align32(&mut self) {
        self.pos = self.pos.next_multiple_of(32);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Packs `(value, bits)` fields LSB first, with zero padding at the end.
    pub(crate) fn pack(fields: &[(u32, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (pos, bit) in fields.iter().flat_map(|&(v, n)| (0..n).map(move |b| (v >> b) & 1)).enumerate() {
            if pos / 8 == out.len() {
                out.push(0);
            }
            out[pos / 8] |= (bit as u8) << (pos % 8);
        }
        out.extend([0; 8]);
        out
    }

    #[test]
    fn reads_lsb_first() {
        let mut r = BitReader::new(&[0b1010_0110, 0xFF, 0x01]);
        assert!(!r.bit());
        assert_eq!(r.read(3), 0b011);
        assert_eq!(r.read(4), 0b1010);
        assert_eq!(r.read(9), 0x1FF);
        assert_eq!(r.position(), 17);
        assert_eq!(r.left(), 7);
    }

    #[test]
    fn reads_32_bits_unaligned() {
        let bytes = [0xF0, 0x12, 0x34, 0x56, 0x78, 0x0A];
        let mut r = BitReader::new(&bytes);
        r.skip(4);
        assert_eq!(r.read(32), 0x8563_412F);
    }

    #[test]
    fn pads_with_zeros_and_reports_overrun() {
        let mut r = BitReader::new(&[0xFF]);
        assert_eq!(r.read(12), 0xFF);
        assert!(r.overrun());
        assert_eq!(r.left(), -4);
    }

    #[test]
    fn aligns_to_32_bits() {
        let mut r = BitReader::new(&[0; 8]);
        r.align32();
        assert_eq!(r.position(), 0);
        r.skip(5);
        r.align32();
        assert_eq!(r.position(), 32);
    }
}
