//! Deterministic pseudo-random numbers (PCG32), identical on every platform.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self { state: 0, inc: (stream << 1) | 1 };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }

    /// Uniform value in `0..bound` without modulo bias.
    pub fn below(&mut self, bound: u32) -> u32 {
        assert!(bound > 0, "bound must be positive");
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % bound;
            }
        }
    }
}

/// Entries in the engine's random table.
pub(crate) const SYNC_TABLE_LEN: usize = 8192;

/// The engine's table: `RANDOM.LST` (8192 × i16, loaded at 0x557843) holds exactly the first
/// 8192 outputs of the MSVC C runtime `rand()` after `srand(1)` (checked against the file), so
/// it is generated here instead of read.
static SYNC_TABLE: [u16; SYNC_TABLE_LEN] = {
    let mut t = [0u16; SYNC_TABLE_LEN];
    let mut state: u32 = 1;
    let mut i = 0;
    while i < SYNC_TABLE_LEN {
        state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
        t[i] = ((state >> 16) & 0x7fff) as u16;
        i += 1;
    }
    t
};

/// The original synchronised random source `rando()` (0x584891): returns `table[index]`
/// (0..=32767) and advances the 13-bit index. Game logic that must behave like the original
/// (motion, path search cooldowns) draws from this, in the original call order. The index is
/// 0 at the start of a match (0x53de83) unless a seed sets it (0x546fd4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SyncRandom {
    pub index: u16,
}

impl SyncRandom {
    pub fn new(index: u16) -> Self {
        Self { index: index & (SYNC_TABLE_LEN as u16 - 1) }
    }

    pub fn rando(&mut self) -> i32 {
        let v = SYNC_TABLE[usize::from(self.index)];
        self.index = (self.index + 1) & (SYNC_TABLE_LEN as u16 - 1);
        i32::from(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_random_matches_random_lst() {
        let mut r = SyncRandom::new(0);
        let got: Vec<i32> = (0..8).map(|_| r.rando()).collect();
        assert_eq!(got, [41, 18467, 6334, 26500, 19169, 15724, 11478, 29358]);
        let mut r = SyncRandom::new(8191);
        r.rando();
        assert_eq!(r.index, 0);
    }

    #[test]
    fn matches_pcg32_reference_output() {
        // Reference values from the PCG paper's demo (seed 42, stream 54).
        let mut rng = Rng::new(42, 54);
        let got: Vec<u32> = (0..6).map(|_| rng.next_u32()).collect();
        assert_eq!(got, [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e]);
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(1, 1);
        assert!((0..1000).all(|_| rng.below(7) < 7));
    }
}
