//! Platform-independent state hashing, used to compare simulations across runs, machines
//! and targets (native vs. wasm32) for replays and lockstep desync detection.

use std::hash::{Hash, Hasher};

/// 64-bit FNV-1a. Unlike `DefaultHasher` its output is fixed forever, and `usize`
/// values (lengths of collections) are hashed as `u64` so 32- and 64-bit targets agree.
#[derive(Debug, Clone)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for StateHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn write_usize(&mut self, i: usize) {
        self.write(&(i as u64).to_le_bytes());
    }

    fn write_isize(&mut self, i: isize) {
        self.write(&(i as i64).to_le_bytes());
    }

    fn write_u16(&mut self, i: u16) {
        self.write(&i.to_le_bytes());
    }

    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_le_bytes());
    }

    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_le_bytes());
    }

    fn write_i16(&mut self, i: i16) {
        self.write(&i.to_le_bytes());
    }

    fn write_i32(&mut self, i: i32) {
        self.write(&i.to_le_bytes());
    }

    fn write_i64(&mut self, i: i64) {
        self.write(&i.to_le_bytes());
    }
}

/// Hashes any value with [`StateHasher`].
pub fn state_hash<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut h = StateHasher::default();
    value.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_reference_values() {
        let mut h = StateHasher::default();
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn lengths_hash_like_u64() {
        let mut a = StateHasher::default();
        a.write_usize(3);
        let mut b = StateHasher::default();
        b.write_u64(3);
        assert_eq!(a.finish(), b.finish());
    }
}
