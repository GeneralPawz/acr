//! Engine objects mirrored into a script's address space.
//!
//! AI DLLs call `GetOBJ(i)` and read the returned `OneObject` directly (F_aiobj field table:
//! all 12 AI DLLs read and write the same 20 fields, no pointer chains). ACR keeps one
//! 0x100-byte mirror per unit index at [`MIRROR_BASE`] + 0x100 · index in the calling DLL's
//! memory, refreshes it whenever the unit is handed out and before every AI call, and copies
//! the fields the AIs write back into the world after the call.
//!
//! Field offsets (AC 1.46 `OneObject`, evidence in `docs/script-runtime.md`):
//!
//! | off | type | field |
//! |---|---|---|
//! | +0x14 | u16 | `Index` |
//! | +0x1f | u16 | `NIndex` (unit type) |
//! | +0x25 | bits | bit0 Ready, bit1 NoSearchVictim, bit4 StandGround |
//! | +0x26 | bits | bit1 InFire |
//! | +0x27 | bits | bit4 NewBuilding |
//! | +0x29 | bits | bit1 UnlimitedMotion |
//! | +0x2a | bits | bit5 AutoKill |
//! | +0x58 | u16 | `Sdoxlo` (0 = alive) |
//! | +0x5c | u8 | `NewState` |
//! | +0x71 | u16 | `Serial` |
//! | +0x78 / +0x7a | u16 | `Life` / `MaxLife` |
//! | +0x84 / +0x88 | i32 | `DstX` / `DstY` (attack point, −1 = none) |
//! | +0x95 | u8 | `NNUM` (owner) |
//! | +0x9a | u8 | `NMask` |
//! | +0x9c / +0x9d | u8 | `RType` / `RAmount` |
//! | +0xb0 / +0xb4 | i32 | `RealX` / `RealY` (1/16 px) |
//! | +0xe8 | u8 | `GroundState` |
//! | +0xea | u8 | `RealDir` |
//! | +0xf0 | u16 | `NInside` |

use acr_script::memory::Memory;
use acr_script::runtime::Result;
use acr_sim::world::EntityId;

use crate::api::Call;
use crate::state::ScriptState;

/// Start of the mirror area (between the `VirtualAlloc` range and the thunks).
pub const MIRROR_BASE: u32 = 0x7000_0000;
/// Bytes per mirrored object.
pub const MIRROR_SIZE: u32 = 0x100;
/// Number of unit indices that fit (`Group[]` has 0xFFFF entries in the original).
pub const MIRROR_SLOTS: u32 = 0xFFFF;

/// Guest address of the mirror of unit `index`.
pub fn mirror_addr(index: u16) -> u32 {
    MIRROR_BASE + u32::from(index) * MIRROR_SIZE
}

/// The unit index of an engine object pointer the DLL hands back (`AddOneUnitToGroup`,
/// `GetOrderType`): the `Index` field of the mirror.
pub fn object_index(c: &Call, ptr: u32) -> Result<Option<u16>> {
    if !(MIRROR_BASE..MIRROR_BASE + MIRROR_SLOTS * MIRROR_SIZE).contains(&ptr) || !c.raw.mem.is_mapped(ptr) {
        return Ok(None);
    }
    Ok(Some(c.raw.mem.read_u16(ptr + 0x14)?))
}

/// Writes the mirror of a unit into guest memory.
pub fn write_mirror(mem: &mut Memory, st: &ScriptState, world: &acr_sim::World, id: EntityId) -> Result<u32> {
    let Some(u) = world.units.get(id.0 as usize) else { return Ok(0) };
    let a = mirror_addr(id.0 as u16);
    if !mem.is_range_mapped(a, MIRROR_SIZE) {
        mem.map(a & !0xFFFF, 0x1_0000);
    }
    let t = world.catalog.get(u.kind);
    let mut b = [0u8; MIRROR_SIZE as usize];
    let put16 = |b: &mut [u8], o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    let put32 = |b: &mut [u8], o: usize, v: i32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    put16(&mut b, 0x14, id.0 as u16);
    put16(&mut b, 0x1f, u.kind.0);
    // Ready (bit 0): finished; buildings under construction are not.
    b[0x25] = u8::from(!u.building || u.stage >= t.economy.build_stages);
    b[0x27] = if u.building { 0x10 } else { 0 };
    // Units inside a building are hidden and have Sdoxlo = 1.
    let hidden = u.inside_of.is_some();
    b[0x28] = u8::from(hidden);
    b[0x29] = if u.unlimited { 2 } else { 0 };
    put16(&mut b, 0x58, if !u.living() { u.death.max(1) } else { u16::from(hidden) });
    b[0x5c] = u.stance;
    put16(&mut b, 0x71, st.serial(id));
    put16(&mut b, 0x78, u.life.clamp(0, 0xFFFF) as u16);
    put16(&mut b, 0x7a, t.life.clamp(0, 0xFFFF) as u16);
    put32(&mut b, 0x84, -1);
    put32(&mut b, 0x88, -1);
    b[0x95] = u.owner;
    b[0x9a] = st.unit_nmask(id, u.owner);
    b[0x9c] = u.carry_type.unwrap_or(0xFF);
    b[0x9d] = u.carried.clamp(0, 255) as u8;
    put16(&mut b, 0xf0, u.inside.len() as u16);
    put32(&mut b, 0xb0, u.pos.x);
    put32(&mut b, 0xb4, u.pos.y);
    b[0xea] = u.dir;
    mem.write_bytes(a, &b)?;
    Ok(a)
}

/// Copies the fields an AI may have written back into the world: `Life` (the "immortal
/// group" trick), the stance (`NewState`), the carried load (`RType`/`RAmount`, dropped by
/// the AI before it sends peasants to fight) and `UnlimitedMotion`. TODO(acr-sim):
/// NoSearchVictim and AutoKill once the simulation has them.
pub fn read_back(mem: &Memory, world: &mut acr_sim::World, index: u16) -> Result<()> {
    let a = mirror_addr(index);
    let life = i32::from(mem.read_u16(a + 0x78)?);
    let stance = mem.read_u8(a + 0x5c)?;
    let (rtype, ramount) = (mem.read_u8(a + 0x9c)?, mem.read_u8(a + 0x9d)?);
    let unlimited = mem.read_u8(a + 0x29)? & 2 != 0;
    let Some(u) = world.units.get_mut(usize::from(index)).filter(|u| u.living()) else { return Ok(()) };
    if u.life.clamp(0, 0xFFFF) != life {
        u.life = life;
    }
    u.stance = stance;
    if rtype == 0xFF && ramount == 0 {
        u.carry_type = None;
        u.carried = 0;
    }
    if !unlimited {
        u.unlimited = false;
    }
    Ok(())
}
