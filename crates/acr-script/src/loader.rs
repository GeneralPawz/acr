//! Maps a PE32 DLL into guest [`Memory`] the way the Windows loader would.
//!
//! Headers and sections are copied to the image base (or another base, applying the base
//! relocations), and every import address table slot is pointed at a *thunk*: an address
//! in a reserved range that never contains code. When the interpreter reaches a thunk it
//! calls the bound Rust function instead (see [`crate::runtime`]).

use crate::memory::Memory;
use crate::pe::{Import, PeError, PeImage};

/// Where import thunks live; each thunk occupies [`THUNK_STRIDE`] bytes.
pub const THUNK_BASE: u32 = 0x7FF0_0000;
pub const THUNK_STRIDE: u32 = 4;
/// Size of the thunk range (16384 thunks).
pub const THUNK_AREA: u32 = 0x1_0000;

/// A DLL mapped into guest memory.
#[derive(Debug, Clone)]
pub struct LoadedImage {
    pub base: u32,
    pub size: u32,
    pub entry_point: u32,
    /// `(name, absolute address)`; unnamed exports are listed as `#ordinal`.
    pub exports: Vec<(String, u32)>,
    /// Imports in IAT order; import `i` is bound to thunk address `THUNK_BASE + i * THUNK_STRIDE`.
    pub imports: Vec<Import>,
    /// Absolute address ranges of the executable sections.
    pub code: Vec<(u32, u32)>,
    /// Absolute addresses of the relocated 32-bit words (after rebasing).
    pub relocations: Vec<u32>,
}

impl LoadedImage {
    pub fn export(&self, name: &str) -> Option<u32> {
        self.exports.iter().find(|(n, _)| n == name).map(|&(_, a)| a)
    }

    pub fn is_code(&self, addr: u32) -> bool {
        self.code.iter().any(|&(s, e)| (s..e).contains(&addr))
    }

    /// Thunk address of import `index`.
    pub fn thunk(index: usize) -> u32 {
        THUNK_BASE + index as u32 * THUNK_STRIDE
    }
}

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_CNT_CODE: u32 = 0x20;

/// Maps `data` (a PE file parsed as `pe`) at `base` (normally `pe.image_base`).
pub fn load(mem: &mut Memory, data: &[u8], pe: &PeImage, base: u32) -> Result<LoadedImage, PeError> {
    if pe.imports.len() as u32 * THUNK_STRIDE > THUNK_AREA {
        return Err(PeError("too many imports"));
    }
    let size = pe.size_of_image;
    if base.checked_add(size).is_none_or(|end| end > THUNK_BASE) {
        return Err(PeError("image does not fit into the address space"));
    }
    if mem.is_range_mapped(base, 1) || mem.is_range_mapped(base + size - 1, 1) {
        return Err(PeError("image base already in use"));
    }
    mem.map(base, size);
    let headers = (pe.size_of_headers as usize).min(data.len());
    mem.write_bytes(base, &data[..headers]).map_err(|_| PeError("header write failed"))?;
    let mut code = Vec::new();
    for s in &pe.sections {
        let len = if s.virtual_size == 0 { s.raw_size } else { s.raw_size.min(s.virtual_size) } as usize;
        let start = s.raw_offset as usize;
        let raw = data.get(start..(start + len).min(data.len())).ok_or(PeError("section outside of file"))?;
        if s.virtual_address.checked_add(s.virtual_size.max(s.raw_size)).is_none_or(|e| e > size) {
            return Err(PeError("section outside of image"));
        }
        mem.write_bytes(base + s.virtual_address, raw).map_err(|_| PeError("section write failed"))?;
        if s.characteristics & (IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_CNT_CODE) != 0 {
            code.push((base + s.virtual_address, base + s.virtual_address + s.virtual_size.max(s.raw_size)));
        }
    }

    let delta = base.wrapping_sub(pe.image_base);
    let relocations: Vec<u32> = pe.relocations.iter().map(|&r| base + r).collect();
    if delta != 0 {
        for &a in &relocations {
            let v = mem.read_u32(a).map_err(|_| PeError("relocation outside of image"))?;
            mem.write_u32(a, v.wrapping_add(delta)).map_err(|_| PeError("relocation outside of image"))?;
        }
    }

    mem.map(THUNK_BASE, THUNK_AREA);
    for (i, import) in pe.imports.iter().enumerate() {
        mem.write_u32(base + import.iat_rva, LoadedImage::thunk(i)).map_err(|_| PeError("IAT outside of image"))?;
    }

    let exports = pe
        .exports
        .iter()
        .map(|e| (e.name.clone().unwrap_or_else(|| format!("#{}", e.ordinal)), base + e.rva))
        .collect();
    Ok(LoadedImage {
        base,
        size,
        entry_point: base + pe.entry_point_rva,
        exports,
        imports: pe.imports.clone(),
        code,
        relocations,
    })
}
