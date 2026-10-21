//! Deterministic allocator for guest memory (backs `HeapAlloc`, `VirtualAlloc` and the
//! game's `_ExMalloc`).
//!
//! Bookkeeping lives on the host side, so guest code cannot corrupt it. Blocks are
//! 8-byte aligned and handed out first-fit from a sorted free list, falling back to a bump
//! pointer that maps new pages on demand.

use std::collections::BTreeMap;

use crate::memory::{Memory, PAGE_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Block {
    /// Bytes requested by the guest (`HeapSize`).
    size: u32,
    /// Bytes reserved for the block.
    capacity: u32,
}

#[derive(Debug, Clone)]
pub struct Heap {
    start: u32,
    end: u32,
    /// First never-used address.
    top: u32,
    /// Mapped up to here.
    mapped: u32,
    used: BTreeMap<u32, Block>,
    /// Free spans: start -> length.
    free: BTreeMap<u32, u32>,
}

const ALIGN: u32 = 8;

impl Heap {
    /// A heap managing the address range `start..end` (page aligned).
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end, top: start, mapped: start, used: BTreeMap::new(), free: BTreeMap::new() }
    }

    /// Allocates `size` bytes; returns `None` when the range is exhausted.
    pub fn alloc(&mut self, mem: &mut Memory, size: u32, zero: bool) -> Option<u32> {
        let capacity = size.max(1).checked_next_multiple_of(ALIGN)?;
        let addr = match self.free.iter().find(|&(_, &len)| len >= capacity).map(|(&a, &l)| (a, l)) {
            Some((addr, len)) => {
                self.free.remove(&addr);
                if len > capacity {
                    self.free.insert(addr + capacity, len - capacity);
                }
                addr
            }
            None => {
                let addr = self.top;
                let new_top = addr.checked_add(capacity).filter(|&t| t <= self.end)?;
                if new_top > self.mapped {
                    let to = new_top.next_multiple_of(PAGE_SIZE);
                    mem.map(self.mapped, to - self.mapped);
                    self.mapped = to;
                }
                self.top = new_top;
                addr
            }
        };
        if zero {
            mem.fill(addr, capacity, 0).ok()?;
        }
        self.used.insert(addr, Block { size, capacity });
        Some(addr)
    }

    /// Frees a block; returns false if `addr` is not an allocated block.
    pub fn free(&mut self, addr: u32) -> bool {
        let Some(block) = self.used.remove(&addr) else { return false };
        let (mut start, mut len) = (addr, block.capacity);
        // Merge with the following and preceding free spans.
        if let Some(next) = self.free.remove(&(start + len)) {
            len += next;
        }
        if let Some((&prev, &prev_len)) = self.free.range(..start).next_back()
            && prev + prev_len == start
        {
            self.free.remove(&prev);
            start = prev;
            len += prev_len;
        }
        if start + len == self.top {
            self.top = start;
        } else {
            self.free.insert(start, len);
        }
        true
    }

    /// Resizes a block, moving it if it does not fit; `None` if `addr` is unknown or the
    /// heap is exhausted.
    pub fn realloc(&mut self, mem: &mut Memory, addr: u32, size: u32, zero: bool) -> Option<u32> {
        let block = *self.used.get(&addr)?;
        if size <= block.capacity {
            if zero && size > block.size {
                mem.fill(addr + block.size, size - block.size, 0).ok()?;
            }
            self.used.insert(addr, Block { size, ..block });
            return Some(addr);
        }
        let new = self.alloc(mem, size, zero)?;
        let mut buf = vec![0; block.size as usize];
        mem.read_bytes(addr, &mut buf).ok()?;
        mem.write_bytes(new, &buf).ok()?;
        self.free(addr);
        Some(new)
    }

    /// Requested size of an allocated block.
    pub fn size(&self, addr: u32) -> Option<u32> {
        self.used.get(&addr).map(|b| b.size)
    }

    /// True if `addr` lies inside this heap's address range.
    pub fn contains(&self, addr: u32) -> bool {
        (self.start..self.end).contains(&addr)
    }

    /// Number of live allocations.
    pub fn live(&self) -> usize {
        self.used.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_frees_and_reuses() {
        let mut mem = Memory::new();
        let mut h = Heap::new(0x100_0000, 0x200_0000);
        let a = h.alloc(&mut mem, 10, true).unwrap();
        let b = h.alloc(&mut mem, 100, false).unwrap();
        assert_eq!(a, 0x100_0000);
        assert_eq!(b, a + 16);
        assert_eq!(h.size(a), Some(10));
        mem.write_u32(a, 0xDEAD_BEEF).unwrap();
        // Growing moves the block and keeps the contents.
        let c = h.realloc(&mut mem, a, 64, false).unwrap();
        assert_ne!(c, a);
        assert_eq!(mem.read_u32(c).unwrap(), 0xDEAD_BEEF);
        // The hole left by `a` is reused first-fit.
        assert_eq!(h.alloc(&mut mem, 16, true).unwrap(), a);
        assert!(h.free(b));
        assert!(!h.free(b));
        assert_eq!(h.live(), 2);
    }

    #[test]
    fn maps_pages_on_demand_and_reports_exhaustion() {
        let mut mem = Memory::new();
        let mut h = Heap::new(0x100_0000, 0x102_0000);
        let a = h.alloc(&mut mem, 0x18000, true).unwrap();
        assert!(mem.is_range_mapped(a, 0x18000));
        assert!(h.alloc(&mut mem, 0x10000, false).is_none());
        // Freeing the last block lowers the bump pointer again.
        h.free(a);
        assert_eq!(h.alloc(&mut mem, 0x20000, false), Some(0x100_0000));
    }
}
