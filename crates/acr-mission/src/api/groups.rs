//! Selections, unit groups, orders to the selection and the scripted group processes
//! (`SGP_*`).
//!
//! The selection of each nation is a list of (unit index, serial) like `Selm`/`SerN`; groups
//! are the `UnitsGroup` entries. Orders become [`Command`]s for the current tick.

use acr_script::runtime::Result;
use acr_sim::Command;
use acr_sim::world::EntityId;

use super::call::magic;
use super::util::norma;
use super::{Api, ApiFn, Call, Status};
use crate::state::{GroupOrder, Member, NO_UNIT};
use crate::world_ops;

const fn f(name: &'static str, original: u32, status: Status, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status, handler }
}

const DONE: Status = Status::Done;

pub const FUNCTIONS: &[ApiFn] = &[
    f("ClearSelection", 0x55f51f, DONE, clear_selection),
    f("SelectUnits", 0x55f865, DONE, select_units),
    f("SelectUnitsType", 0x55feda, DONE, select_units_type),
    f("SelectUnitsInZone", 0x55cf09, DONE, select_units_in_zone),
    f("SelectTypeOfUnitsInZone", 0x55d669, DONE, select_type_of_units_in_zone),
    f("SaveSelectedUnits", 0x5671ad, DONE, save_selected_units),
    f("AddUnitToSel", 0x56c8c7, DONE, add_unit_to_sel),
    f("SelSendTo", 0x561ad6, DONE, sel_send_to),
    f("SelSendAndKill", 0x561be4, DONE, sel_send_to),
    f("Patrol", 0x561cfb, DONE, patrol),
    f(
        "SelAttackGroup",
        0x560258,
        Status::Partial("idle selected units attack the first living enemy (AttackObj 0x90)"),
        sel_attack_group,
    ),
    f("SelDie", 0x5601c0, Status::Partial("World::kill; finished buildings do not burn down yet"), sel_die),
    f("SelErase", 0x56021c, DONE, sel_erase),
    f("SelChangeNation", 0x561eaf, DONE, sel_change_nation),
    f("SelCenter", 0x563981, DONE, sel_center),
    f(
        "SetStandGround",
        0x5621cd,
        Status::Partial("formations have no flag bearer, so its extra bonus is never added"),
        set_stand_ground,
    ),
    f("AllowAttack", 0x562234, DONE, allow_attack),
    f("SetDestPoint", 0x567609, DONE, set_dest_point),
    f("CleanGroup", 0x56cf09, DONE, clean_group),
    f("RemoveGroup", 0x568031, DONE, remove_group),
    f("CopyUnits", 0x56ea22, DONE, copy_units),
    f("AddOneUnitToGroup", 0x5670a8, DONE, add_one_unit_to_group),
    f("InsertUnitToGroup", 0x56bf98, DONE, insert_unit_to_group),
    f("DeleteUnitFromGroup", 0x56ce96, DONE, delete_unit_from_group),
    f("RemoveUnitFromGroup", 0x56c17e, DONE, remove_unit_from_group),
    f("RemoveNUnits", 0x56d757, DONE, remove_n_units),
    f("CollectUnits", 0x56d814, DONE, collect_units),
    f("RemoveNearestUnit", 0x56e48c, DONE, remove_nearest_unit),
    f("RemoveNearestNotBusyUnit", 0x56e265, DONE, remove_nearest_not_busy_unit),
    f("RemovePanicUnits", 0x56da24, DONE, remove_panic_units),
    f("DetectPanic", 0x56d9b7, DONE, detect_panic),
    f("RemoveUpperGroundUnits", 0x56eeef, DONE, remove_upper_ground_units),
    f("DeleteHiddenUnits", 0x56d8b9, DONE, delete_hidden_units),
    f("GetNUnits", 0x5679d8, DONE, get_n_units),
    f("GetUnitsTypeInGroup", 0x570862, DONE, get_units_type_in_group),
    f("GetGrpCenter", 0x5789cb, DONE, get_grp_center),
    f("UnitsCenter", 0x5637f1, DONE, units_center),
    f("FilterUnitsByCategory", 0x56df95, DONE, filter_units_by_category),
    f(
        "SetUnitsState",
        0x578c09,
        Status::Partial("formations have no flag bearer, so its extra bonus is never added"),
        set_units_state,
    ),
    f("CancelAllPurposes", 0x578bc5, DONE, cancel_all_purposes),
    f("GetCurrentPurpose", 0x578b24, DONE, get_current_purpose),
    f("PushAllUnitsAway", 0x56083e, DONE, push_all_units_away),
    f("SetNMASK", 0x56bcb3, DONE, set_nmask),
    f("GetUnitNMASK", 0x56bc78, DONE, get_unit_nmask),
    f("SGP_MoveToPoint", 0x578a14, DONE, sgp_move_to_point),
    f("SGP_MakeOneStepTo", 0x578a6f, DONE, sgp_make_one_step_to),
];

/// `'MOVE'` purpose kind.
const PURPOSE_MOVE: u32 = 0x4d4f_5645;

impl Api<'_> {
    /// A group index from a handle argument: `'UNIT'` and in range, silently.
    pub(super) fn group(&self, c: &Call, i: u32) -> Result<Option<usize>> {
        if c.u32(i)? == 0 {
            return Ok(None);
        }
        let h = c.handle(i)?;
        Ok((h.magic == magic::GROUP && usize::from(h.index) < self.st.groups.len()).then_some(usize::from(h.index)))
    }

    /// The unit in a slot if its serial matches (any state).
    fn by_serial(&self, m: Member) -> Option<&acr_sim::Unit> {
        let u = self.object(m.id)?;
        (self.st.serial(u.id) == m.serial).then_some(u)
    }

    /// A nation argument through `AssignTBL`, `None` (and an error) when ≥ 8.
    fn nation(&mut self, c: &Call, i: u32, what: &str) -> Result<Option<u8>> {
        let n = self.assign(c.u8(i)?);
        if n >= 8 {
            self.st.error(format!("{what} : Invalid parameter <Nat>"));
            return Ok(None);
        }
        Ok(Some(n))
    }

    /// Living selected units of a nation (serial-valid).
    pub(crate) fn selected(&self, n: u8) -> Vec<EntityId> {
        self.st.selection[usize::from(n)].iter().filter_map(|&m| self.member(m)).map(|u| u.id).collect()
    }

    /// Selected objects of a nation by slot only (SelDie, SelErase, SelChangeNation ignore
    /// serials and act on whatever occupies the slot).
    fn selected_slots(&self, n: u8) -> Vec<EntityId> {
        self.st.selection[usize::from(n)].iter().filter_map(|&m| self.object(m.id)).map(|u| u.id).collect()
    }
}

// --- selection ---------------------------------------------------------------------------

/// `ReClearSelection` + `ImClearSelection`.
fn clear(api: &mut Api, n: u8) {
    api.st.selection[usize::from(n)].clear();
}

fn clear_selection(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(n) = api.nation(c, 0, "ClearSelection(Nat)")? {
        clear(api, n);
    }
    Ok(0)
}

/// `void SelectUnits(GAMEOBJ* units, bool add)`: the serial-valid members of the first valid
/// member's nation (dead units included).
fn select_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 0, "SelectUnits(Units,Add)")? else { return Ok(0) };
    let members: Vec<(Member, u8)> = api.st.groups[g]
        .members
        .iter()
        .filter_map(|&m| api.by_serial(m).map(|u| (Member { id: m.id, serial: m.serial }, u.owner)))
        .collect();
    let Some(&(_, n)) = members.first() else { return Ok(0) };
    if !c.bool(1)? {
        clear(api, n);
    }
    api.st.selection[usize::from(n)].extend(members.iter().filter(|(_, o)| *o == n).map(|(m, _)| *m));
    Ok(0)
}

/// `void SelectUnitsType(GAMEOBJ* type, byte nation, bool add)`: all living objects of the type.
fn select_units_type(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(1)?);
    let Some(t) = api.type_arg(c, 0, "SelectUnitsType(UnitsType,Nation,Add)")? else { return Ok(0) };
    if n >= 8 {
        api.st.error("SelectUnitsType(UnitsType,Nation,Add) : Invalid parameter <Nation>");
        return Ok(0);
    }
    if !c.bool(2)? {
        clear(api, n);
    }
    let add: Vec<Member> = api
        .world
        .units
        .iter()
        .filter(|u| u.living() && u.kind == t && u.owner == n)
        .map(|u| api.member_of(u.id))
        .collect();
    api.st.selection[usize::from(n)].extend(add);
    Ok(0)
}

/// Shared part of the zone selections: living units of `n` inside every circle, appended
/// unless already selected before the call (overlapping zones may add a unit twice).
fn select_in_zone(api: &mut Api, c: &Call, zone: u32, n: u8, kind: Option<acr_sim::TypeId>, what: &str) -> Result<()> {
    let Some(circles) = api.zone_arg(c, zone, what)? else { return Ok(()) };
    let before: Vec<u16> = api.st.selection[usize::from(n)].iter().map(|m| m.id).collect();
    let mut add = Vec::new();
    for z in circles {
        for u in api.world.units.iter() {
            if u.living()
                && u.owner == n
                && kind.is_none_or(|k| u.kind == k)
                && z.contains(u)
                && !before.contains(&(u.id.0 as u16))
            {
                add.push(api.member_of(u.id));
            }
        }
    }
    api.st.selection[usize::from(n)].extend(add);
    Ok(())
}

fn select_units_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(1)?);
    if n >= 8 {
        api.st.error("GetUnitsAmount(Zone,Nation) : Incorrect parameter <Nation>");
        return Ok(0);
    }
    if !c.bool(2)? {
        clear(api, n);
    }
    select_in_zone(api, c, 0, n, None, "GetUnitsAmount(Zone,Nation)")?;
    Ok(0)
}

fn select_type_of_units_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(2)?);
    if n >= 8 {
        api.st.error("GetTypeOfUnitsAmount(Zone,Type,Nation) : Incorrect parameter <Nation>");
        return Ok(0);
    }
    let Some(t) = api.type_arg(c, 1, "GetTypeOfUnitsAmount(Zone,Type,Nation)")? else { return Ok(0) };
    if !c.bool(3)? {
        clear(api, n);
    }
    select_in_zone(api, c, 0, n, Some(t), "GetTypeOfUnitsAmount(Zone,Type,Nation)")?;
    Ok(0)
}

/// Appends a unit to a group unless its index is already in it (`AddOneUnitToGroup`).
fn add_unique(api: &mut Api, g: usize, m: Member) {
    let grp = &mut api.st.groups[g];
    if !grp.members.iter().any(|x| x.id == m.id) {
        grp.members.push(m);
    }
}

/// `void SaveSelectedUnits(byte nation, GAMEOBJ* grp, bool add)`.
fn save_selected_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = usize::from(c.u8(0)? & 7);
    let h = if c.u32(1)? == 0 { None } else { Some(c.handle(1)?) };
    let Some(g) = h.filter(|h| h.magic == magic::GROUP && usize::from(h.index) < api.st.groups.len()) else {
        api.st.error("SaveSelectedUnits : Invalid <Units>");
        return Ok(0);
    };
    let g = usize::from(g.index);
    if !c.bool(2)? {
        api.st.groups[g].members.clear();
    }
    let sel: Vec<Member> = api.st.selection[n].iter().copied().filter(|&m| api.by_serial(m).is_some()).collect();
    for m in sel {
        add_unique(api, g, m);
    }
    Ok(0)
}

/// `void AddUnitToSel(int index)`: to its owner's selection.
fn add_unit_to_sel(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    if !(0..0xFFFF).contains(&i) {
        return Ok(0);
    }
    if let Some(u) = api.object(i as u16).filter(|u| u.living()) {
        let (n, m) = (usize::from(u.owner & 7), api.member_of(u.id));
        if !api.st.selection[n].iter().any(|x| x.id == m.id) {
            api.st.selection[n].push(m);
        }
    }
    Ok(0)
}

// --- orders ------------------------------------------------------------------------------

/// `bool SelSendTo(byte nation, GAMEOBJ* zone, byte dir, byte type)` and `SelSendAndKill`.
fn sel_send_to(api: &mut Api, c: &mut Call) -> Result<u32> {
    let what =
        if c.name() == "SelSendTo" { "SelSendTo(Nat,Zone,Dir,Type)" } else { "SelSendAndKill(Nat,Zone,Dir,Type)" };
    let Some(n) = api.nation(c, 0, what)? else { return Ok(0) };
    let Some((x, y)) = (if c.u32(1)? == 0 { None } else { api.zone_point(&c.handle(1)?) }) else {
        api.st.error(format!("{what} : Invalid parameter <Zone>"));
        return Ok(0);
    };
    // 0x4db0d3: the facing `dir`, the order type `type & 7` and the absolute-facing bit
    // `type & 0x10` (0x11eaf5c); priority 0x10, or 0x80 (an attack-move) for SelSendAndKill.
    let (dir, ty) = (c.u8(2)?, c.u8(3)?);
    let prio = if c.name() == "SelSendTo" { 0x10 } else { 0x80 };
    let units = api.selected(n);
    if !units.is_empty() {
        api.commands.push(Command::ScriptMove {
            player: n,
            units,
            x,
            y,
            dir: u16::from(dir),
            prio,
            ord: ty & 7,
            abs_dir: ty & 0x10 != 0,
            off_x: 0,
            off_y: 0,
        });
    }
    Ok(1)
}

/// `bool Patrol(byte nation, GAMEOBJ* zone, byte dir)`.
fn patrol(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(n) = api.nation(c, 0, "Patrol(Nat,Zone,Dir)")? else { return Ok(0) };
    let Some((x, y)) = (if c.u32(1)? == 0 { None } else { api.zone_point(&c.handle(1)?) }) else {
        api.st.error("Patrol(Nat,Zone,Dir) : Invalid parameter <Zone>");
        return Ok(0);
    };
    let units: Vec<EntityId> =
        api.selected(n).into_iter().filter(|&id| api.world.units[id.0 as usize].brigade.is_none()).collect();
    if !units.is_empty() {
        api.commands.push(Command::Patrol { player: n, units, x, y });
    }
    Ok(1)
}

/// `void SetStandGround(byte nation, byte val)` (0x5621cd): every selected object (by slot)
/// stands or stops standing; formation members switch their formation.
fn set_stand_ground(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(n) = api.nation(c, 0, "SetStandGround(Nat,val)")? else { return Ok(0) };
    let ids = api.selected_slots(n);
    acr_sim::states::set_stand_ground(api.world, &ids, c.u8(1)? != 0);
    Ok(0)
}

/// `void AllowAttack(byte nation, byte val)` (0x562234): `SetSearchVictim(nat, val ? 2 : 3)`.
fn allow_attack(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(n) = api.nation(c, 0, "AllowAttack(Nat,val)")? else { return Ok(0) };
    let ids = api.selected_slots(n);
    acr_sim::states::allow_attack(api.world, &ids, c.u8(1)? != 0);
    Ok(0)
}

/// `void SetUnitsState(GAMEOBJ* grp, bool allowAttack, bool standGround, bool meleeOnly,
/// bool noFFAvoid)` (0x578c09), for the alive serial-valid members.
fn set_units_state(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let ids = api.members(g);
    let (a, s, m, f) = (c.u8(1)? != 0, c.u8(2)? != 0, c.u8(3)? != 0, c.u8(4)? != 0);
    acr_sim::states::set_units_state(api.world, &ids, a, s, m, f);
    Ok(0)
}

/// `void SetAI_Guard(GAMEOBJ* grp, bool val)` (0x570251): the alive serial-valid members
/// become guards (OB+0xf2 bit2) or stop being guards.
pub(super) fn set_ai_guard(api: &mut Api, c: &mut Call) -> Result<u32> {
    if c.u32(0)? == 0 || c.handle(0)?.magic != magic::GROUP {
        return Ok(0);
    }
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let ids = api.members(g);
    acr_sim::states::set_guard(api.world, &ids, c.u8(1)? != 0);
    Ok(0)
}

/// `bool DetectPanic(int index)` (0x56d9b7): the object runs in panic (`UnlimitedMotion` and
/// a panic order).
fn detect_panic(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    Ok(u32::from((0..0xFFFF).contains(&i) && api.object(i as u16).is_some_and(acr_sim::Unit::panicking)))
}

/// `void SelAttackGroup(byte nation, GAMEOBJ* enemy)`: idle selected units attack the first
/// living enemy.
fn sel_attack_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    let Some(g) = api.group_arg(c, 1, "SelAttackGroup : Invalid <Enemy>")? else { return Ok(0) };
    if n >= 8 {
        api.st.error("SelAttackGroup : Invalid <Nat>");
        return Ok(0);
    }
    let Some(&target) = api.members(g).first() else { return Ok(0) };
    let units: Vec<EntityId> =
        api.selected(n).into_iter().filter(|&id| api.world.units[id.0 as usize].idle()).collect();
    for id in units {
        api.attack(id, target, 0x90, false);
    }
    Ok(0)
}

fn sel_die(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(n) = api.nation(c, 0, "SelDie(Nat)")? else { return Ok(0) };
    for id in api.selected_slots(n) {
        world_ops::destruct(api.world, id);
    }
    Ok(1)
}

/// `void SelErase(byte nation)`: no `AssignTBL`.
fn sel_erase(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    if n >= 8 {
        api.st.error("SelDie(Nat) : Invalid parameter <Nat>");
        return Ok(0);
    }
    for id in api.selected_slots(n) {
        world_ops::erase(api.world, id);
    }
    Ok(0)
}

/// `void SelChangeNation(byte src, byte dst)`: the selected objects change owner, the source
/// selection is cleared.
fn sel_change_nation(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (src, dst) = (c.u8(0)?, c.u8(1)?);
    if src >= 8 || dst >= 8 {
        api.st.error("SelChangeNation(SrcNat,DstNat) : Invalid parameters");
        return Ok(0);
    }
    if src == dst {
        api.st.error("SelChangeNation(SrcNat,DstNat) : SrcNat and DstNat must be different");
        return Ok(0);
    }
    for id in api.selected_slots(src) {
        world_ops::set_owner(api.world, id, dst);
        api.st.extra.entry(id.0).or_default().nmask = None;
    }
    clear(api, src);
    Ok(0)
}

/// Writes a point zone `{x, y, 0x40000000 + r}` or type −1 when there is no unit.
fn write_center(c: &mut Call, dst: u32, units: &[(i32, i32)], r: u32) -> Result<u32> {
    if units.is_empty() {
        c.write_u32(dst + 4, u32::MAX)?;
        return Ok(0);
    }
    let n = units.len() as i64;
    let sx: i64 = units.iter().map(|p| i64::from(p.0)).sum();
    let sy: i64 = units.iter().map(|p| i64::from(p.1)).sum();
    c.set_handle(dst, ((sx >> 4) / n) as u16, ((sy >> 4) / n) as u16, 0x4000_0000u32.wrapping_add(r))?;
    Ok(1)
}

/// `bool SelCenter(GAMEOBJ* dst, byte nation, int R)`: a point zone at the selection centre.
fn sel_center(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(1)?);
    if n >= 8 {
        api.st.error("SelCenter(DstZone,byte Nat,R) : invalid <Nat>");
        return Ok(0);
    }
    let pts: Vec<(i32, i32)> =
        api.selected(n).iter().map(|id| api.world.units[id.0 as usize].pos).map(|p| (p.x, p.y)).collect();
    write_center(c, c.u32(0)?, &pts, c.u32(2)?)
}

/// `bool UnitsCenter(GAMEOBJ* dst, GAMEOBJ* grp, word R)`.
fn units_center(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 1, "UnitsCenter(DstZone,Units,R)")? else { return Ok(0) };
    let pts: Vec<(i32, i32)> =
        api.members(g).iter().map(|id| api.world.units[id.0 as usize].pos).map(|p| (p.x, p.y)).collect();
    write_center(c, c.u32(0)?, &pts, u32::from(c.u16(2)?))
}

// --- groups ------------------------------------------------------------------------------

/// `int CleanGroup(GAMEOBJ*)`: keeps the valid living members, returns their number.
fn clean_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let kept: Vec<Member> = api.st.groups[g].members.iter().copied().filter(|&m| api.member_h(m).is_some()).collect();
    let n = kept.len();
    api.st.groups[g].members = kept;
    Ok(n as u32)
}

/// `void RemoveGroup(GAMEOBJ* src, GAMEOBJ* dst)`: moves all raw entries.
fn remove_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(s), Some(d)) = (api.group(c, 0)?, api.group(c, 1)?) else {
        api.st.error("RemoveGroup(Source,Dest) : Invalid <Source>");
        return Ok(0);
    };
    if s != d {
        let moved = std::mem::take(&mut api.st.groups[s].members);
        api.st.groups[d].members.extend(moved);
    }
    Ok(0)
}

/// `void CopyUnits(GAMEOBJ* src, GAMEOBJ* dst, int start, int n, bool add, bool remove)`.
fn copy_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(s), Some(d)) = (api.group(c, 0)?, api.group(c, 1)?) else { return Ok(0) };
    let len = api.st.groups[s].members.len() as i32;
    let mut start = c.i32(2)?;
    let mut n = c.i32(3)?;
    if start >= len {
        start = len - 1;
    }
    start = start.max(0);
    if start + n > len {
        n = len - start;
    }
    let n = n.max(0) as usize;
    let start = start as usize;
    let slice: Vec<Member> = api.st.groups[s].members.iter().skip(start).take(n).copied().collect();
    if c.bool(4)? {
        api.st.groups[d].members.extend(slice);
    } else {
        api.st.groups[d].members = slice;
    }
    if c.bool(5)? && n > 0 {
        let m = &mut api.st.groups[s].members;
        let end = (start + n).min(m.len());
        m.drain(start..end);
    }
    Ok(0)
}

/// `void AddOneUnitToGroup(GAMEOBJ* grp, OneObject* ob)`: `ob` is an engine object pointer
/// (from `GetOBJ`); the unit index is read from it.
fn add_one_unit_to_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let h = c.handle(0)?;
    if usize::from(h.index) >= api.st.groups.len() {
        return Ok(0);
    }
    let Some(id) = crate::guest::object_index(c, c.u32(1)?)? else { return Ok(0) };
    if let Some(u) = api.object(id) {
        let m = api.member_of(u.id);
        add_unique(api, usize::from(h.index), m);
    }
    Ok(0)
}

/// `int InsertUnitToGroup(GAMEOBJ* src or NULL, GAMEOBJ* dst, int index)`.
fn insert_unit_to_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let dst = if c.u32(1)? == 0 { None } else { Some(c.handle(1)?) };
    let Some(dst) = dst.filter(|h| h.magic == magic::GROUP && usize::from(h.index) < api.st.groups.len()) else {
        return Ok(u32::MAX);
    };
    let d = usize::from(dst.index);
    let index = c.i32(2)?;
    let m = if c.u32(0)? != 0 {
        let src = c.handle(0)?;
        if src.magic != magic::GROUP {
            return Ok(u32::MAX);
        }
        let s = usize::from(src.index);
        match api.st.groups.get(s).and_then(|g| usize::try_from(index).ok().and_then(|i| g.members.get(i))) {
            Some(&m) => m,
            None => return Ok(u32::MAX),
        }
    } else {
        match u16::try_from(index).ok().and_then(|i| api.object(i)).filter(|u| u.living()) {
            Some(u) => api.member_of(u.id),
            None => return Ok(u32::MAX),
        }
    };
    api.st.groups[d].members.push(m);
    Ok(api.st.groups[d].members.len() as u32 - 1)
}

fn delete_unit_from_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    if let Some(m) = usize::try_from(c.i32(1)?).ok().and_then(|p| api.st.groups[g].members.get_mut(p)) {
        m.id = NO_UNIT;
    }
    Ok(0)
}

fn remove_unit_from_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    if let Ok(p) = usize::try_from(c.i32(1)?)
        && p < api.st.groups[g].members.len()
    {
        api.st.groups[g].members.remove(p);
    }
    Ok(0)
}

/// The type/usage filter of `RemoveNUnits`/`CollectUnits`.
fn filter_matches(api: &Api, u: &acr_sim::Unit, filter: i32) -> bool {
    let kind = i32::from(u.kind.0);
    let usage = i32::from(api.world.catalog.get(u.kind).role.usage);
    match filter {
        0xFFFF => true,
        f if f >= 30000 => usage != f - 30000,
        f if f >= 20000 => usage == f - 20000,
        f if f >= 10000 => kind != f - 10000,
        f => kind == f,
    }
}

/// `MoveN` (0x56d1fb): moves up to `n` matching members from the end of `src` to `dst`,
/// leaving holes in `src`; returns how many moved.
fn move_n(api: &mut Api, d: usize, s: usize, n: i32, filter: i32) -> u32 {
    if n <= 0 {
        return 0;
    }
    let mut left = n;
    let mut moved = Vec::new();
    for i in (0..api.st.groups[s].members.len()).rev() {
        if left == 0 {
            break;
        }
        let m = api.st.groups[s].members[i];
        if let Some(u) = api.member_h(m)
            && filter_matches(api, u, filter)
        {
            moved.push(Member { id: m.id, serial: api.st.serial(u.id) });
            api.st.groups[s].members[i].id = NO_UNIT;
            left -= 1;
        }
    }
    api.st.groups[d].members.extend(moved);
    (n - left) as u32
}

fn remove_n_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(d), Some(s)) = (api.group(c, 0)?, api.group(c, 1)?) else { return Ok(0) };
    Ok(move_n(api, d, s, c.i32(2)?, c.i32(3)?))
}

fn collect_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(d), Some(s)) = (api.group(c, 0)?, api.group(c, 1)?) else { return Ok(0) };
    let need = c.i32(2)?;
    let have = api.st.groups[d].members.len() as i32;
    if have < need {
        move_n(api, d, s, need - have, c.i32(3)?);
    }
    Ok(api.st.groups[d].members.len() as u32)
}

/// Moves the living member nearest to (x, y) within `maxR` (pixels, `Norma`).
fn remove_nearest(api: &mut Api, c: &mut Call, idle_only: bool) -> Result<u32> {
    let (Some(s), Some(d)) = (api.group(c, 0)?, api.group(c, 1)?) else { return Ok(0) };
    let (x, y, mut best) = (c.i32(2)?, c.i32(3)?, c.i32(4)?);
    let mut found = None;
    for (i, &m) in api.st.groups[s].members.iter().enumerate() {
        let Some(u) = api.member(m) else { continue };
        if idle_only && !u.orders.is_empty() {
            continue;
        }
        let dist = norma(x - (u.pos.x >> 4), y - (u.pos.y >> 4));
        if dist < best {
            best = dist;
            found = Some((i, m));
        }
    }
    let Some((i, m)) = found else { return Ok(0) };
    api.st.groups[s].members.remove(i);
    add_unique(api, d, m);
    Ok(1)
}

fn remove_nearest_unit(api: &mut Api, c: &mut Call) -> Result<u32> {
    remove_nearest(api, c, false)
}

fn remove_nearest_not_busy_unit(api: &mut Api, c: &mut Call) -> Result<u32> {
    remove_nearest(api, c, true)
}

/// `int RemovePanicUnits(GAMEOBJ* src, GAMEOBJ* dst)` (0x56da24): panicking members move from
/// `src` to `dst`; returns how many.
fn remove_panic_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(s), Some(d)) = (api.group(c, 0)?, api.group(c, 1)?) else { return Ok(0) };
    let mut n = 0;
    for i in 0..api.st.groups[s].members.len() {
        let m = api.st.groups[s].members[i];
        if m.id == NO_UNIT || !api.object(m.id).is_some_and(acr_sim::Unit::panicking) {
            continue;
        }
        api.st.groups[s].members[i].id = NO_UNIT;
        api.st.groups[d].members.push(m);
        n += 1;
    }
    Ok(n)
}

/// `void RemoveUpperGroundUnits(GAMEOBJ* src, GAMEOBJ* dst or NULL)`: members that are not
/// inside a building move to `dst` (or become invalid without `dst`).
fn remove_upper_ground_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(s) = api.group(c, 0)? else { return Ok(0) };
    let d = api.group(c, 1)?;
    for i in 0..api.st.groups[s].members.len() {
        let m = api.st.groups[s].members[i];
        if m.id == NO_UNIT || api.by_serial(m).is_none_or(|u| u.inside_of.is_some()) {
            continue;
        }
        match d {
            Some(d) => {
                add_unique(api, d, m);
                api.st.groups[s].members[i].id = NO_UNIT;
            }
            None => api.st.groups[s].members[i].serial = 0xFFFF,
        }
    }
    Ok(0)
}

/// `void DeleteHiddenUnits(GAMEOBJ*)`: empties the slots of members inside buildings.
fn delete_hidden_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    for i in 0..api.st.groups[g].members.len() {
        let m = api.st.groups[g].members[i];
        if api.by_serial(m).is_some_and(|u| u.inside_of.is_some()) {
            api.st.groups[g].members[i].id = NO_UNIT;
        }
    }
    Ok(0)
}

/// `void SetDestPoint(GAMEOBJ* grp, GAMEOBJ* zone)`: the rally point of idle member buildings.
fn set_dest_point(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 0, "SetDestPoint(Units,Zone)")? else { return Ok(0) };
    let Some((x, y)) = (if c.u32(1)? == 0 { None } else { api.zone_point(&c.handle(1)?) }) else {
        api.st.error("SetDestPoint(Units,Zone) : Incorrect parameter <Zone>");
        return Ok(0);
    };
    for id in api.members_h(g) {
        let u = &mut api.world.units[id.0 as usize];
        if u.building && u.orders.is_empty() {
            u.rally = Some(acr_sim::Pos::from_pixels(x, y));
        }
    }
    Ok(0)
}

/// `void PushAllUnitsAway(byte nation)`: everybody leaves the selected buildings.
fn push_all_units_away(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?) & 7;
    for id in api.selected(n) {
        let i = id.0 as usize;
        if api.world.catalog.get(api.world.units[i].kind).transport {
            acr_sim::ship::leave_all(api.world, i);
        } else if !api.world.units[i].inside.is_empty() {
            acr_sim::economy::leave(api.world, i);
        }
    }
    Ok(0)
}

/// `int GetNUnits(GAMEOBJ*)`: the raw entry count (holes and dead units included).
fn get_n_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    match api.group(c, 0)? {
        Some(g) => Ok(api.st.groups[g].members.len() as u32),
        None => {
            api.st.error("GetNUnits(Units) : Incorrect parameter <Units>");
            Ok(0)
        }
    }
}

fn get_units_type_in_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(u32::MAX) };
    let t = c.u16(1)?;
    Ok(api.members_h(g).iter().filter(|id| api.world.units[id.0 as usize].kind.0 == t).count() as u32)
}

/// `bool GetGrpCenter(GAMEOBJ*, int* x, int* y)`: mean of the pixel positions.
fn get_grp_center(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let ids = api.members(g);
    if ids.is_empty() {
        return Ok(0);
    }
    let n = ids.len() as i64;
    let sx: i64 = ids.iter().map(|id| i64::from(api.world.units[id.0 as usize].pos.x >> 4)).sum();
    let sy: i64 = ids.iter().map(|id| i64::from(api.world.units[id.0 as usize].pos.y >> 4)).sum();
    c.out_i32(1, (sx / n) as i32)?;
    c.out_i32(2, (sy / n) as i32)?;
    Ok(1)
}

/// `void FilterUnitsByCategory(GAMEOBJ* src, GAMEOBJ* dst, int category)`.
fn filter_units_by_category(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(s), Some(d)) = (api.group(c, 0)?, api.group(c, 1)?) else { return Ok(0) };
    let cat = c.i32(2)?;
    let mut i = 0;
    while i < api.st.groups[s].members.len() {
        let m = api.st.groups[s].members[i];
        let hit = api.by_serial(m).is_some_and(|u| {
            let t = api.world.catalog.get(u.kind);
            let kind = |k: usize, names: &[&str]| names.contains(&t.attacks[k].kind.as_str());
            match cat {
                0 => kind(1, &["OSTRELA"]) || kind(2, &["OSTRELA"]),
                1 => t.attacks[1].weapon.is_none() && t.attacks[0].damage != 0,
                2 => kind(1, &["VISTREL", "VISTREL2"]),
                3 => t.peasant,
                4 => u.inside_of.is_none(),
                5 => kind(1, &["TAMAGAVK", "KOPIE"]),
                _ => false,
            }
        });
        if hit {
            api.st.groups[d].members.push(m);
            api.st.groups[s].members.remove(i);
        } else {
            i += 1;
        }
    }
    Ok(0)
}

fn set_nmask(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    if (0..0xFFFF).contains(&i) && api.object(i as u16).is_some() {
        api.st.extra.entry(i as u32).or_default().nmask = Some(c.u8(1)?);
    }
    Ok(0)
}

fn get_unit_nmask(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    if !(0..0xFFFF).contains(&i) {
        return Ok(0);
    }
    Ok(api.object(i as u16).map_or(0, |u| u32::from(api.st.unit_nmask(u.id, u.owner))))
}

// --- purposes ----------------------------------------------------------------------------

fn cancel_all_purposes(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(g) = api.group(c, 0)? {
        api.st.groups[g].orders.clear();
    }
    Ok(0)
}

fn get_current_purpose(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    Ok(api.st.groups[g].orders.first().map_or(0, GroupOrder::kind))
}

impl GroupOrder {
    /// The four-character kind `GetCurrentPurpose` returns.
    pub fn kind(&self) -> u32 {
        match self {
            GroupOrder::Move { .. } => PURPOSE_MOVE,
            GroupOrder::ComeIntoBuilding { .. } => u32::from_be_bytes(*b"IBLD"),
            GroupOrder::Repair { .. } => u32::from_be_bytes(*b"REPB"),
            GroupOrder::TakeResources { .. } => u32::from_be_bytes(*b"TRES"),
            GroupOrder::ArcherAttack { missed: false, .. } => 0x4152_4341,
            GroupOrder::ArcherAttack { missed: true, .. } => 0x4152_4350,
        }
    }
}

/// `GroupMovePurpose` (0x5785a9): a group move plus a `'MOVE'` purpose that replaces all
/// others. `dir` 0x200 faces the direction of travel, any other value its low byte;
/// `flags & 1` gives priority 0x81 instead of 0x90.
#[allow(clippy::too_many_arguments)]
fn group_move_purpose(api: &mut Api, nation: u8, g: usize, x: i32, y: i32, off: (i32, i32), dir: i32, flags: i32) {
    let units = api.members(g);
    if !units.is_empty() {
        api.commands.push(Command::ScriptMove {
            player: nation,
            units,
            x,
            y,
            dir: if dir == 0x200 { 0x200 } else { (dir & 0xff) as u16 },
            prio: if flags & 1 != 0 { 0x81 } else { 0x90 },
            ord: 0,
            abs_dir: false,
            off_x: off.0,
            off_y: off.1,
        });
    }
    let tmtmt = api.world.tick.0 as i64 + 1;
    api.st.groups[g].orders = vec![GroupOrder::Move { t0: tmtmt, half: None, flags }];
}

/// `void SGP_MoveToPoint(byte nat, GAMEOBJ* grp, int x, int y, int dir, int offX, int offY,
/// int flags)`.
fn sgp_move_to_point(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 1)? else { return Ok(0) };
    group_move_purpose(api, c.u8(0)?, g, c.i32(2)?, c.i32(3)?, (c.i32(5)?, c.i32(6)?), c.i32(4)?, c.i32(7)?);
    Ok(0)
}

/// `int SGP_MakeOneStepTo(byte nat, GAMEOBJ* grp, int x, int y, int dir, int flags)`: one
/// pathfinding-area step towards (x, y).
fn sgp_make_one_step_to(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 1)? else { return Ok(0) };
    let ids = api.members(g);
    if ids.is_empty() {
        return Ok(0);
    }
    let n = ids.len() as i64;
    let cx = (ids.iter().map(|id| i64::from(api.world.units[id.0 as usize].pos.x >> 4)).sum::<i64>() / n) as i32;
    let cy = (ids.iter().map(|id| i64::from(api.world.units[id.0 as usize].pos.y >> 4)).sum::<i64>() / n) as i32;
    let (x, y) = (c.i32(2)?, c.i32(3)?);
    let Some(areas) = api.world.map.land_areas.clone() else { return Ok(0) };
    let a1 = areas.area_at(cx >> 6, cy >> 6);
    let a2 = areas.area_at(x >> 6, y >> 6);
    if a1 >= 0xFFFE || a2 >= 0xFFFE {
        return Ok(0);
    }
    let next = areas.next_hop(a1, a2);
    let (tx, ty) = if next == a2 || a1 == a2 {
        (x, y)
    } else if next < 0xFFFE {
        let (ax, ay) = super::tactics::area_center(api, next);
        (ax << 6, ay << 6)
    } else {
        return Ok(0);
    };
    group_move_purpose(api, c.u8(0)?, g, tx, ty, (0, 0), c.i32(4)?, c.i32(5)?);
    Ok(1)
}

/// Runs the head purpose of every group (0x55a2b4).
pub fn run_group_orders(api: &mut Api) {
    let tmtmt = api.world.tick.0 as i64 + 1;
    for g in 0..api.st.groups.len() {
        let Some(head) = api.st.groups[g].orders.first().cloned() else { continue };
        let pop = match head {
            GroupOrder::Move { t0, half, flags } => {
                let dt = tmtmt - t0;
                if dt <= 12 {
                    false
                } else if dt > 768 {
                    true
                } else {
                    let ids = api.members(g);
                    let alive = ids.len();
                    let moving = ids.iter().filter(|id| !api.world.units[id.0 as usize].orders.is_empty()).count();
                    if moving == 0 {
                        true
                    } else if let Some(h) = half {
                        2 * moving > alive && tmtmt - h > 128
                    } else if alive > moving {
                        if let Some(GroupOrder::Move { half, .. }) = api.st.groups[g].orders.first_mut() {
                            *half = Some(tmtmt);
                        }
                        flags & 0x20 != 0
                    } else {
                        false
                    }
                }
            }
            GroupOrder::Repair { building, serial } => super::aihelp::repair_step(api, g, building, serial),
            GroupOrder::TakeResources { nation, res, mut last, mut bases } => {
                super::aihelp::take_resources_step(api, g, nation, res, &mut last, &mut bases);
                if let Some(GroupOrder::TakeResources { last: l, bases: b, .. }) = api.st.groups[g].orders.first_mut() {
                    *l = last;
                    *b = bases;
                }
                false
            }
            GroupOrder::ArcherAttack { nation, x, y, zone, .. } => {
                // Handler 0x578ff6: every member slot whose unit is still the one that joined
                // (in any state; AttackObj refuses the dying) attacks FindArcherTarget's choice
                // (0x90, replacing its orders).
                let target = super::tactics::find_archer_target(api, nation, x, y, 40, zone);
                if let Some(t) = target {
                    let slots = api.st.groups[g].members.clone();
                    for m in slots {
                        if let Some(i) = api.by_serial(m).map(|u| u.id.0 as usize) {
                            acr_sim::combat::attack_obj_ex(api.world, i, t, 0x90, acr_sim::combat::Placement::Replace);
                        }
                    }
                }
                // The purpose becomes 'ACRA' or 'PCRA' (0x5790d8 / 0x5790e7).
                if let Some(GroupOrder::ArcherAttack { missed, .. }) = api.st.groups[g].orders.first_mut() {
                    *missed = target.is_none();
                }
                false
            }
            GroupOrder::ComeIntoBuilding { building, serial } => {
                // Handler 0x577945: every call, members not already on their way are sent in
                // (GoToMine prio 0x90); the process never ends by itself.
                let b = usize::from(building);
                let valid = api.object(building).is_some_and(|u| api.st.serial(u.id) == serial);
                if valid {
                    let bid = api.world.units[b].id;
                    for id in api.members(g) {
                        let going = api.world.units[id.0 as usize]
                            .orders
                            .iter()
                            .any(|o| matches!(o, acr_sim::world::Order::Enter { building, .. } if *building == bid));
                        if !going {
                            acr_sim::economy::enter(api.world, id.0 as usize, b, 0x90, false);
                        }
                    }
                }
                false
            }
        };
        if pop {
            api.st.groups[g].orders.remove(0);
        }
    }
}
