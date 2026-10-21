//! The 16 fixed Huffman code sets of Bink video and the per-plane symbol permutations
//! ("trees") sent in the bitstream.

use crate::bits::BitReader;
use crate::tables::{TREE_CODES, TREE_LENGTHS};

/// Longest code in any set.
const MAX_BITS: u32 = 7;

/// Lookup tables for all code sets: indexed by the next 7 bits, each entry holds the
/// code index (0..16) and its length.
pub struct Codebooks {
    lookup: [[(u8, u8); 1 << MAX_BITS]; 16],
}

impl Codebooks {
    pub fn new() -> Self {
        let mut lookup = [[(0u8, 0u8); 1 << MAX_BITS]; 16];
        for (set, table) in lookup.iter_mut().enumerate() {
            for index in 0..16 {
                let code = usize::from(TREE_CODES[set][index]);
                let len = TREE_LENGTHS[set][index];
                // Codes are read LSB first, so every index whose low `len` bits equal the
                // code starts with it.
                for high in 0..1usize << (MAX_BITS - u32::from(len)) {
                    table[code | (high << len)] = (index as u8, len);
                }
            }
        }
        Self { lookup }
    }

    /// Reads one code of `set` and returns its index (0..16).
    #[inline]
    pub fn read(&self, r: &mut BitReader, set: u8) -> u8 {
        let (index, len) = self.lookup[usize::from(set & 15)][r.peek(MAX_BITS) as usize];
        r.skip(usize::from(len));
        index
    }
}

impl Default for Codebooks {
    fn default() -> Self {
        Self::new()
    }
}

/// A code set plus the mapping from code index to symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tree {
    pub set: u8,
    pub symbols: [u8; 16],
}

impl Default for Tree {
    fn default() -> Self {
        Self { set: 0, symbols: std::array::from_fn(|i| i as u8) }
    }
}

impl Tree {
    /// Reads a tree description.
    ///
    /// Set 0 is four raw bits per symbol. Otherwise the symbol order is either an explicit
    /// list of up to 8 leading symbols (the rest follow in ascending order) or the result
    /// of up to four rounds of bit-driven merging, like the passes of a merge sort.
    pub fn read(r: &mut BitReader) -> Self {
        let set = r.read(4) as u8;
        let mut tree = Self { set, ..Self::default() };
        if set == 0 {
            return tree;
        }
        if r.bit() {
            let count = r.read(3) as usize + 1;
            let mut used = [false; 16];
            for slot in &mut tree.symbols[..count] {
                *slot = r.read(4) as u8;
                used[usize::from(*slot)] = true;
            }
            let rest = (0..16u8).filter(|&s| !used[usize::from(s)]);
            for (slot, symbol) in tree.symbols[count..].iter_mut().zip(rest) {
                *slot = symbol;
            }
        } else {
            let rounds = r.read(2) as usize + 1;
            let mut list: [u8; 16] = std::array::from_fn(|i| i as u8);
            for round in 0..rounds {
                let size = 1 << round;
                let mut merged = [0u8; 16];
                for start in (0..16).step_by(size * 2) {
                    merge(r, &list[start..start + size * 2], &mut merged[start..start + size * 2]);
                }
                list = merged;
            }
            tree.symbols = list;
        }
        tree
    }

    #[inline]
    pub fn decode(&self, books: &Codebooks, r: &mut BitReader) -> u8 {
        self.symbols[usize::from(books.read(r, self.set))]
    }
}

/// Interleaves the two halves of `src` into `dst`; each bit picks the half the next
/// element comes from (0 = first half) until one half is used up.
fn merge(r: &mut BitReader, src: &[u8], dst: &mut [u8]) {
    let (a, b) = src.split_at(src.len() / 2);
    let (mut i, mut j) = (0, 0);
    for slot in dst.iter_mut() {
        let take_b = if i == a.len() {
            true
        } else if j == b.len() {
            false
        } else {
            r.bit()
        };
        if take_b {
            *slot = b[j];
            j += 1;
        } else {
            *slot = a[i];
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::tests::pack;

    #[test]
    fn code_sets_are_complete_prefix_codes() {
        for (set, lengths) in TREE_LENGTHS.iter().enumerate() {
            // Kraft sum of a complete prefix code is exactly 1.
            let kraft: u32 = lengths.iter().map(|&l| 1 << (MAX_BITS - u32::from(l))).sum();
            assert_eq!(kraft, 1 << MAX_BITS, "set {set}");
        }
    }

    #[test]
    fn every_code_decodes_to_its_index() {
        let books = Codebooks::new();
        for set in 0..16u8 {
            for index in 0..16 {
                let code = u32::from(TREE_CODES[usize::from(set)][index]);
                let len = u32::from(TREE_LENGTHS[usize::from(set)][index]);
                let bytes = pack(&[(code, len), (0x5, 3)]);
                let mut r = BitReader::new(&bytes);
                assert_eq!(books.read(&mut r, set), index as u8);
                assert_eq!(r.read(3), 0x5, "set {set} index {index} consumed the wrong length");
            }
        }
    }

    #[test]
    fn set_zero_is_raw_nibbles() {
        let bytes = pack(&[(0, 4), (0xB, 4)]);
        let mut r = BitReader::new(&bytes);
        let tree = Tree::read(&mut r);
        assert_eq!(tree, Tree::default());
        assert_eq!(tree.decode(&Codebooks::new(), &mut r), 0xB);
    }

    #[test]
    fn explicit_symbol_list_is_followed_by_the_rest() {
        // set 3, explicit, 2 symbols: 9, 4
        let bytes = pack(&[(3, 4), (1, 1), (1, 3), (9, 4), (4, 4)]);
        let tree = Tree::read(&mut BitReader::new(&bytes));
        assert_eq!(tree.set, 3);
        assert_eq!(tree.symbols, [9, 4, 0, 1, 2, 3, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15]);
    }

    #[test]
    fn merge_rounds_permute() {
        // One round: each pair (2k, 2k+1) is swapped when its bit is 1. Swap the first
        // pair only: bit 1 then (forced) the remaining element; then bit 0 for the others.
        let mut fields = vec![(1, 4), (0, 1), (0, 2), (1, 1)];
        fields.extend(std::iter::repeat_n((0, 1), 7));
        let tree = Tree::read(&mut BitReader::new(&pack(&fields)));
        assert_eq!(tree.symbols, [1, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    }
}
