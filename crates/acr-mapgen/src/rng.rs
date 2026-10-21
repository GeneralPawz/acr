//! The engine's random number generator (`mrand`, 0x4526F0).
//!
//! A cursor into the 8192 entries of `RANDOM.LST`; every time the cursor wraps, a 16-bit
//! salt grows by 0x3571 and is XORed into the values. The random map generator seeds it
//! with the low 13 bits of the map seed and a salt of 0 (0x457207).

pub struct Rng<'a> {
    table: &'a [u16],
    index: u32,
    salt: u16,
}

impl<'a> Rng<'a> {
    /// `table` must hold 8192 entries (`RANDOM.LST`).
    pub fn new(table: &'a [u16], seed: u32) -> Self {
        assert!(table.len() >= 0x2000, "RANDOM.LST needs 8192 entries");
        Self { table, index: seed & 0x1FFF, salt: 0 }
    }

    /// The cursor; the generator uses its value once to pick the land shape variant.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// The next value, 0..=0x7FFF.
    pub fn rand(&mut self) -> i32 {
        self.index += 1;
        if self.index > 0x1FFF {
            self.salt = self.salt.wrapping_add(0x3571);
        }
        self.index &= 0x1FFF;
        i32::from((self.table[self.index as usize] ^ self.salt) & 0x7FFF)
    }

    /// `rand() * n >> 15`: uniform in `0..n` for `n` up to 0x10000.
    pub fn below(&mut self, n: i32) -> i32 {
        (self.rand() * n) >> 15
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salts_on_wrap() {
        let table: Vec<u16> = (0..0x2000).map(|i| i as u16).collect();
        let mut rng = Rng::new(&table, 0x1FFE);
        assert_eq!(rng.rand(), 0x1FFF);
        assert_eq!(rng.rand(), 0x3571);
        assert_eq!(rng.rand(), 1 ^ 0x3571);
    }
}
