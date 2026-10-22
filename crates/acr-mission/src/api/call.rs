//! Typed access to the arguments and guest memory of one API call.

use acr_script::runtime::{Error, HostCall, Result};

/// A script handle (`GAMEOBJ`, 8 bytes in the DLL's memory): `u16` index, `u16` serial or
/// sub-index, `u32` type magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    /// Guest address of the handle.
    pub addr: u32,
    pub index: u16,
    pub serial: u16,
    pub magic: u32,
}

/// Type magics written into handles by the `Register*` functions (`'UNIT'` etc. as
/// little-endian `u32`, the way MSVC stores multi-character constants).
pub mod magic {
    /// Unit groups (`RegisterDynGroup`, `RegisterUnits`): `'UNIT'`.
    pub const GROUP: u32 = u32::from_be_bytes(*b"UNIT");
    /// Zones (`RegisterZone`): `'ZONE'`.
    pub const ZONE: u32 = u32::from_be_bytes(*b"ZONE");
    /// Unit types (`RegisterUnitType`): `'UTYP'`.
    pub const UNIT_TYPE: u32 = u32::from_be_bytes(*b"UTYP");
    /// Upgrades (`RegisterUpgrade`): `'UPGR'`.
    pub const UPGRADE: u32 = u32::from_be_bytes(*b"UPGR");
    /// Formations (`RegisterFormation`): `'FORM'`.
    pub const FORMATION: u32 = u32::from_be_bytes(*b"FORM");
}

/// Arguments of a `cdecl` call plus the guest memory, with typed accessors.
pub struct Call<'c, 'h> {
    pub raw: &'c mut HostCall<'h>,
}

impl Call<'_, '_> {
    pub fn name(&self) -> &str {
        self.raw.name
    }

    /// The `i`th argument as a raw 32-bit stack slot.
    pub fn u32(&self, i: u32) -> Result<u32> {
        self.raw.arg(i)
    }

    pub fn i32(&self, i: u32) -> Result<i32> {
        Ok(self.u32(i)? as i32)
    }

    /// A `char`/`byte` argument (the low byte of the slot; the callers push full registers).
    pub fn u8(&self, i: u32) -> Result<u8> {
        Ok(self.u32(i)? as u8)
    }

    pub fn u16(&self, i: u32) -> Result<u16> {
        Ok(self.u32(i)? as u16)
    }

    pub fn i16(&self, i: u32) -> Result<i16> {
        Ok(self.u32(i)? as i16)
    }

    /// A `bool` argument (low byte non-zero).
    pub fn bool(&self, i: u32) -> Result<bool> {
        Ok(self.u8(i)? != 0)
    }

    /// A NUL-terminated string argument (CP1252 bytes); a null pointer is an empty string.
    pub fn bytes(&self, i: u32) -> Result<Vec<u8>> {
        match self.u32(i)? {
            0 => Ok(Vec::new()),
            p => Ok(self.raw.mem.read_cstr(p, 0x10000)?),
        }
    }

    /// A string argument as text (CP1252 decoded).
    pub fn str(&self, i: u32) -> Result<String> {
        Ok(crate::texts::cp1252(&self.bytes(i)?))
    }

    /// The handle a pointer argument points to.
    pub fn handle(&self, i: u32) -> Result<Handle> {
        let addr = self.u32(i)?;
        self.handle_at(addr)
    }

    pub fn handle_at(&self, addr: u32) -> Result<Handle> {
        if addr == 0 {
            return Err(Error::Host(format!("{}: null handle", self.raw.name)));
        }
        let mem = &self.raw.mem;
        Ok(Handle { addr, index: mem.read_u16(addr)?, serial: mem.read_u16(addr + 2)?, magic: mem.read_u32(addr + 4)? })
    }

    /// Writes a handle (index, serial, magic) to guest memory.
    pub fn set_handle(&mut self, addr: u32, index: u16, serial: u16, magic: u32) -> Result<()> {
        let mem = &mut self.raw.mem;
        mem.write_u16(addr, index)?;
        mem.write_u16(addr + 2, serial)?;
        mem.write_u32(addr + 4, magic)?;
        Ok(())
    }

    pub fn write_u8(&mut self, addr: u32, v: u8) -> Result<()> {
        Ok(self.raw.mem.write_u8(addr, v)?)
    }

    pub fn write_u16(&mut self, addr: u32, v: u16) -> Result<()> {
        Ok(self.raw.mem.write_u16(addr, v)?)
    }

    pub fn write_u32(&mut self, addr: u32, v: u32) -> Result<()> {
        Ok(self.raw.mem.write_u32(addr, v)?)
    }

    pub fn write_i32(&mut self, addr: u32, v: i32) -> Result<()> {
        self.write_u32(addr, v as u32)
    }

    pub fn read_u8(&self, addr: u32) -> Result<u8> {
        Ok(self.raw.mem.read_u8(addr)?)
    }

    pub fn read_u16(&self, addr: u32) -> Result<u16> {
        Ok(self.raw.mem.read_u16(addr)?)
    }

    pub fn read_u32(&self, addr: u32) -> Result<u32> {
        Ok(self.raw.mem.read_u32(addr)?)
    }

    /// Writes through an out-pointer argument unless it is null.
    pub fn out_u32(&mut self, i: u32, v: u32) -> Result<()> {
        match self.u32(i)? {
            0 => Ok(()),
            p => self.write_u32(p, v),
        }
    }

    pub fn out_i32(&mut self, i: u32, v: i32) -> Result<()> {
        self.out_u32(i, v as u32)
    }

    pub fn out_u16(&mut self, i: u32, v: u16) -> Result<()> {
        match self.u32(i)? {
            0 => Ok(()),
            p => self.write_u16(p, v),
        }
    }

    pub fn out_u8(&mut self, i: u32, v: u8) -> Result<()> {
        match self.u32(i)? {
            0 => Ok(()),
            p => self.write_u8(p, v),
        }
    }

    /// Copies bytes into a fresh guest heap block and returns its address.
    pub fn alloc_bytes(&mut self, data: &[u8]) -> Result<u32> {
        let p = self
            .raw
            .heap
            .alloc(self.raw.mem, data.len().max(1) as u32, true)
            .ok_or_else(|| Error::Host("guest heap exhausted".into()))?;
        self.raw.mem.write_bytes(p, data)?;
        Ok(p)
    }

    /// Copies a NUL-terminated string into a fresh guest heap block and returns its address.
    pub fn alloc_str(&mut self, s: &[u8]) -> Result<u32> {
        let p = self
            .raw
            .heap
            .alloc(self.raw.mem, s.len() as u32 + 1, true)
            .ok_or_else(|| Error::Host("guest heap exhausted".into()))?;
        self.raw.mem.write_bytes(p, s)?;
        Ok(p)
    }
}
