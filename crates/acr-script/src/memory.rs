//! Paged 32-bit guest address space for the x86 interpreter.
//!
//! The 4 GiB address space is split into 65536 pages of 64 KiB (the Windows allocation
//! granularity). Pages are allocated on [`Memory::map`] and zero-filled; every access to an
//! unmapped page fails with a [`Fault`] instead of crashing the host. There is no page
//! protection: guest code is trusted not to write to its own code.

use std::fmt;

/// log2 of the page size.
pub const PAGE_BITS: u32 = 16;
/// Page size in bytes (64 KiB).
pub const PAGE_SIZE: u32 = 1 << PAGE_BITS;
const PAGE_MASK: u32 = PAGE_SIZE - 1;
const PAGE_COUNT: usize = 1 << (32 - PAGE_BITS);

/// Access to an unmapped guest address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fault {
    pub addr: u32,
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "access to unmapped address {:#010x}", self.addr)
    }
}

impl std::error::Error for Fault {}

pub type Result<T> = std::result::Result<T, Fault>;

#[derive(Clone)]
pub struct Memory {
    pages: Vec<Option<Box<[u8]>>>,
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mapped = self.pages.iter().filter(|p| p.is_some()).count();
        write!(f, "Memory {{ {mapped} pages mapped }}")
    }
}

/// Page index and offset of an address.
fn split(addr: u32) -> (usize, usize) {
    ((addr >> PAGE_BITS) as usize, (addr & PAGE_MASK) as usize)
}

impl Memory {
    pub fn new() -> Self {
        Self { pages: (0..PAGE_COUNT).map(|_| None).collect() }
    }

    /// Maps zeroed pages covering `addr..addr + len`. Already mapped pages are kept as they are.
    pub fn map(&mut self, addr: u32, len: u32) {
        if len == 0 {
            return;
        }
        let first = addr >> PAGE_BITS;
        let last = ((u64::from(addr) + u64::from(len) - 1) >> PAGE_BITS) as u32;
        for page in first..=last {
            let slot = &mut self.pages[page as usize];
            if slot.is_none() {
                *slot = Some(vec![0; PAGE_SIZE as usize].into_boxed_slice());
            }
        }
    }

    /// Unmaps every page that lies completely inside `addr..addr + len`.
    pub fn unmap(&mut self, addr: u32, len: u32) {
        let first = addr.div_ceil(PAGE_SIZE);
        let end = ((u64::from(addr) + u64::from(len)) >> PAGE_BITS) as u32;
        for page in first..end {
            self.pages[page as usize] = None;
        }
    }

    pub fn is_mapped(&self, addr: u32) -> bool {
        self.pages[split(addr).0].is_some()
    }

    /// True if the whole range `addr..addr + len` is mapped.
    pub fn is_range_mapped(&self, addr: u32, len: u32) -> bool {
        if len == 0 {
            return true;
        }
        let Some(last) = addr.checked_add(len - 1) else { return false };
        (addr >> PAGE_BITS..=last >> PAGE_BITS).all(|p| self.pages[p as usize].is_some())
    }

    fn page(&self, addr: u32) -> Result<&[u8]> {
        self.pages[split(addr).0].as_deref().ok_or(Fault { addr })
    }

    fn page_mut(&mut self, addr: u32) -> Result<&mut [u8]> {
        self.pages[split(addr).0].as_deref_mut().ok_or(Fault { addr })
    }

    pub fn read_u8(&self, addr: u32) -> Result<u8> {
        Ok(self.page(addr)?[split(addr).1])
    }

    pub fn write_u8(&mut self, addr: u32, value: u8) -> Result<()> {
        let off = split(addr).1;
        self.page_mut(addr)?[off] = value;
        Ok(())
    }

    /// Reads `N` bytes; the fast path stays within one page.
    fn read_array<const N: usize>(&self, addr: u32) -> Result<[u8; N]> {
        let off = split(addr).1;
        if off + N <= PAGE_SIZE as usize {
            return Ok(self.page(addr)?[off..off + N].try_into().unwrap());
        }
        let mut out = [0; N];
        self.read_bytes(addr, &mut out)?;
        Ok(out)
    }

    fn write_array<const N: usize>(&mut self, addr: u32, bytes: [u8; N]) -> Result<()> {
        let off = split(addr).1;
        if off + N <= PAGE_SIZE as usize {
            self.page_mut(addr)?[off..off + N].copy_from_slice(&bytes);
            return Ok(());
        }
        self.write_bytes(addr, &bytes)
    }

    pub fn read_u16(&self, addr: u32) -> Result<u16> {
        self.read_array(addr).map(u16::from_le_bytes)
    }

    pub fn read_u32(&self, addr: u32) -> Result<u32> {
        self.read_array(addr).map(u32::from_le_bytes)
    }

    pub fn read_u64(&self, addr: u32) -> Result<u64> {
        self.read_array(addr).map(u64::from_le_bytes)
    }

    pub fn write_u16(&mut self, addr: u32, value: u16) -> Result<()> {
        self.write_array(addr, value.to_le_bytes())
    }

    pub fn write_u32(&mut self, addr: u32, value: u32) -> Result<()> {
        self.write_array(addr, value.to_le_bytes())
    }

    pub fn write_u64(&mut self, addr: u32, value: u64) -> Result<()> {
        self.write_array(addr, value.to_le_bytes())
    }

    /// Fills `out` from guest memory; addresses wrap around at 4 GiB.
    pub fn read_bytes(&self, addr: u32, out: &mut [u8]) -> Result<()> {
        let mut done = 0;
        while done < out.len() {
            let a = addr.wrapping_add(done as u32);
            let off = split(a).1;
            let n = (PAGE_SIZE as usize - off).min(out.len() - done);
            out[done..done + n].copy_from_slice(&self.page(a)?[off..off + n]);
            done += n;
        }
        Ok(())
    }

    pub fn write_bytes(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        let mut done = 0;
        while done < data.len() {
            let a = addr.wrapping_add(done as u32);
            let off = split(a).1;
            let n = (PAGE_SIZE as usize - off).min(data.len() - done);
            self.page_mut(a)?[off..off + n].copy_from_slice(&data[done..done + n]);
            done += n;
        }
        Ok(())
    }

    /// Sets `len` bytes starting at `addr` to `value`.
    pub fn fill(&mut self, addr: u32, len: u32, value: u8) -> Result<()> {
        let mut done = 0u32;
        while done < len {
            let a = addr.wrapping_add(done);
            let off = split(a).1;
            let n = (PAGE_SIZE - off as u32).min(len - done);
            self.page_mut(a)?[off..off + n as usize].fill(value);
            done += n;
        }
        Ok(())
    }

    /// Reads a NUL-terminated byte string of at most `max` bytes (without the terminator).
    pub fn read_cstr(&self, addr: u32, max: usize) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for i in 0..max as u32 {
            match self.read_u8(addr.wrapping_add(i))? {
                0 => break,
                b => out.push(b),
            }
        }
        Ok(out)
    }

    /// Reads a NUL-terminated UTF-16 string of at most `max` code units.
    pub fn read_wstr(&self, addr: u32, max: usize) -> Result<Vec<u16>> {
        let mut out = Vec::new();
        for i in 0..max as u32 {
            match self.read_u16(addr.wrapping_add(2 * i))? {
                0 => break,
                c => out.push(c),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmapped_access_faults() {
        let mut m = Memory::new();
        assert_eq!(m.read_u32(0x1234), Err(Fault { addr: 0x1234 }));
        assert_eq!(m.write_u8(0, 1), Err(Fault { addr: 0 }));
    }

    #[test]
    fn values_cross_page_boundaries() {
        let mut m = Memory::new();
        m.map(0x1_FFF0, 0x20);
        assert!(m.is_range_mapped(0x1_0000, 0x2_0000));
        m.write_u32(0x1_FFFE, 0xAABB_CCDD).unwrap();
        assert_eq!(m.read_u32(0x1_FFFE).unwrap(), 0xAABB_CCDD);
        assert_eq!(m.read_u16(0x2_0000).unwrap(), 0xAABB);
        m.write_u64(0x2_0000 - 3, 0x0102_0304_0506_0708).unwrap();
        assert_eq!(m.read_u64(0x2_0000 - 3).unwrap(), 0x0102_0304_0506_0708);
        m.fill(0x1_FFFF, 2, 0x55).unwrap();
        assert_eq!(m.read_u16(0x1_FFFF).unwrap(), 0x5555);
    }

    #[test]
    fn reads_strings_and_unmaps() {
        let mut m = Memory::new();
        m.map(0x1000, 1);
        m.write_bytes(0x1000, b"abc\0def").unwrap();
        assert_eq!(m.read_cstr(0x1000, 100).unwrap(), b"abc");
        assert_eq!(m.read_cstr(0x1000, 2).unwrap(), b"ab");
        m.unmap(0, PAGE_SIZE);
        assert!(!m.is_mapped(0x1000));
    }
}
