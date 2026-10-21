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

#[cfg(test)]
mod tests {
    use super::*;

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
