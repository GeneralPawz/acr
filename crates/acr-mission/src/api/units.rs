//! Counting units, creating objects, unit information, buildings and transports.

use acr_script::runtime::Result;
use acr_sim::Pos;
use acr_sim::world::EntityId;

use super::call::magic;
use super::util::{Circle, is_point};
use super::{Api, ApiFn, Call, Status};
use crate::state::{Group, NO_UNIT};
use crate::world_ops;

const fn f(name: &'static str, original: u32, status: Status, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status, handler }
}

const DONE: Status = Status::Done;

pub const FUNCTIONS: &[ApiFn] = &[
    f("GetUnitsAmount0", 0x55c70a, DONE, get_units_amount0),
    f("GetUnitsAmount1", 0x55dd05, DONE, get_units_amount1),
    f("GetUnitsAmount2", 0x55e05c, DONE, get_units_amount2),
    f("GetTotalAmount0", 0x55e6c6, DONE, get_total_amount0),
    f("GetTotalAmount1", 0x55e7f2, DONE, get_total_amount1),
    f("GetUnitsByNation", 0x566ca6, DONE, get_units_by_nation),
    f("GetAmountOfWarriors", 0x55eaba, DONE, get_amount_of_warriors),
    f("GetInsideAmount", 0x57819e, DONE, get_inside_amount),
    f("GetMaxInsideAmount", 0x5781e5, Status::Done, get_max_inside_amount),
    f("GetInsideList", 0x56c713, DONE, get_inside_list),
    f("GetNInside", 0x5604e2, DONE, get_n_inside),
    f("InviteUnitIntoBuilding", 0x56c638, DONE, invite_unit_into_building),
    f("LeaveBuilding", 0x56c6ca, DONE, leave_building),
    f("CheckLeaveAbility", 0x560b17, Status::Done, check_leave_ability),
    f("SGP_ComeIntoBuilding", 0x577c2a, DONE, sgp_come_into_building),
    f("CreateObject0", 0x55eded, DONE, create_object0),
    f("CreateFields", 0x577ff4, Status::Done, create_fields),
    f("DieUnit", 0x5701ae, DONE, die_unit),
    f("GetUnitInfo", 0x567a0e, DONE, get_unit_info),
    f("GetUnitInfo2", 0x578f2c, DONE, get_unit_info2),
    f("SetUnitInfo", 0x567e8a, DONE, set_unit_info),
    f("GetUnitGlobalInfo", 0x567cb9, DONE, get_unit_global_info),
    f("GetUnitExCaps", 0x56be77, Status::Done, get_unit_ex_caps),
    f("GetCurrentUnits", 0x578173, DONE, get_current_units),
    f("GetMaxUnits", 0x578148, DONE, get_max_units),
    f("CheckBuildingsComplete", 0x566a30, DONE, check_buildings_complete),
    f("SendUnitsToTransport", 0x56093a, Status::Done, send_units_to_transport),
    f("SetMaxInTransport", 0x57033a, DONE, set_max_in_transport),
    f("GetZoneCoor", 0x56c266, DONE, get_zone_coor),
    f("GetBuildingEntryPoint", 0x56ea8c, DONE, get_building_entry_point),
    f("AssignDesantStartGroup", 0x5701ea, DONE, assign_desant),
    f("AssignDesantDestGroup", 0x570224, DONE, assign_desant),
    f("CreateFormation", 0x56fbf8, DONE, create_formation),
];

/// Circles of a zone argument for the counting functions (`'@'` before `'ZONE'`).
fn zone(api: &mut Api, c: &Call, i: u32, what: &str) -> Result<Option<Vec<Circle>>> {
    api.zone_arg(c, i, what)
}

/// `int GetUnitsAmount0(GAMEOBJ* zone, byte nation)`: living objects of the nation inside the
/// zone (a unit inside k overlapping zones counts k times).
fn get_units_amount0(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(1)?);
    if n >= 8 {
        api.st.error("GetUnitsAmount(Zone,Nation) : Incorrect parameter <Nation>");
        return Ok(0);
    }
    let Some(circles) = zone(api, c, 0, "GetUnitsAmount(Zone,Nation)")? else { return Ok(0) };
    let units: Vec<_> = api.world.units.iter().filter(|u| u.active() && u.owner == n).collect();
    Ok(circles.iter().map(|z| units.iter().filter(|u| z.contains(u)).count()).sum::<usize>() as u32)
}

/// `int GetUnitsAmount1(GAMEOBJ* zone, GAMEOBJ* units)`: living group members in the zone.
fn get_units_amount1(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 1, "GetUnitsAmount(Zone,Units)")? else { return Ok(0) };
    let Some(circles) = zone(api, c, 0, "GetUnitsAmount(Zone,Units)")? else { return Ok(0) };
    let ids = api.members(g);
    Ok(circles
        .iter()
        .map(|z| ids.iter().filter(|id| z.contains(&api.world.units[id.0 as usize])).count())
        .sum::<usize>() as u32)
}

/// `int GetUnitsAmount2(GAMEOBJ* zone, GAMEOBJ* type, byte nation)`.
fn get_units_amount2(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(2)?);
    if n >= 8 {
        api.st.error("GetUnitsAmount(Zone,UnitType,Nation) : Incorrect parameter <Nation>");
        return Ok(0);
    }
    let Some(t) = api.type_arg(c, 1, "GetUnitsAmount(Zone,UnitType,Nation)")? else { return Ok(0) };
    let Some(circles) = zone(api, c, 0, "GetUnitsAmount(Zone,UnitType,Nation)")? else { return Ok(0) };
    let units: Vec<_> = api.world.units.iter().filter(|u| u.active() && u.owner == n && u.kind == t).collect();
    Ok(circles.iter().map(|z| units.iter().filter(|u| z.contains(u)).count()).sum::<usize>() as u32)
}

/// `int GetTotalAmount0(GAMEOBJ* units)`: living members.
fn get_total_amount0(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 0, "GetTotalAmount(Units)")? else { return Ok(0) };
    Ok(api.members_h(g).len() as u32)
}

/// `int GetTotalAmount1(GAMEOBJ* type, byte nation)` (0x55e7f2): the nation's cached object
/// count of the type (`City+0xeb73[type]`), refreshed by the census (`EnumUnits` 0x42e658,
/// every 8 ticks per player and after a unit is produced): living objects of the owner,
/// including buildings under construction and units inside buildings.
fn get_total_amount1(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(1)?);
    let Some(t) = api.type_arg(c, 0, "GetTotalAmount(UnitType,Nation)")? else { return Ok(0) };
    if n >= 8 {
        api.st.error("GetTotalAmount(UnitType,Nation) : Invalid parameter <Nation>");
        return Ok(0);
    }
    let counts = &api.world.players[usize::from(n)].type_count;
    Ok(counts.get(usize::from(t.0)).map_or(0, |&k| u32::from(k)))
}

/// `int GetUnitsByNation(GAMEOBJ* units, byte nation)`: living members owned by the nation
/// (serials compared without bit 0, so captured units match).
fn get_units_by_nation(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 0, "GetUnitsByNation(Units,Nat)")? else { return Ok(0) };
    let n = c.u8(1)?;
    let members = api.st.groups[g].members.clone();
    Ok(members.iter().filter_map(|&m| api.member_loose(m)).filter(|u| u.owner == n).count() as u32)
}

/// `int GetAmountOfWarriors(byte nation)`: the original loops over the global unit indices
/// `0..NtNUnits[nation]` without checking the owner (kept as is).
fn get_amount_of_warriors(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    if n >= 8 {
        api.st.error("GetAmountOfWarriors : Invalid <Nat>");
        return Ok(0);
    }
    let count = api.world.units.iter().filter(|u| u.owner == n).count();
    Ok(api
        .world
        .units
        .iter()
        .take(count)
        .filter(|u| u.living() && !u.building && api.world.catalog.get(u.kind).medium == acr_sim::catalog::Medium::Land)
        .count() as u32)
}

/// The living building at an `int index` argument (`0 <= i < 0xFFFF`).
fn building_arg(api: &Api, c: &Call, i: u32) -> Result<Option<usize>> {
    let b = c.i32(i)?;
    Ok(u16::try_from(b)
        .ok()
        .filter(|&b| b != 0xFFFF)
        .and_then(|b| api.object(b))
        .filter(|u| u.living())
        .map(|u| u.id.0 as usize))
}

/// `int GetInsideAmount(int index)`: units inside a building.
fn get_inside_amount(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(building_arg(api, c, 0)?.map_or(0, |b| api.world.units[b].inside.len() as u32))
}

/// `int GetMaxInsideAmount(int index)`: `MaxInside + AddInside`.
fn get_max_inside_amount(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(b) = building_arg(api, c, 0)? else { return Ok(0) };
    let u = &api.world.units[b];
    let e = &api.world.catalog.get(u.kind).economy;
    let max = api.world.adv(u.owner, u.kind).max_inside.max(e.peasant_absorber);
    Ok((max + u.add_inside).max(0) as u32)
}

/// `int GetInsideList(int index, u16** list)`: the building's inside list (unit indices). The
/// original hands out its own array; ACR copies it into the DLL's memory (one buffer per
/// building, rewritten on every call).
fn get_inside_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    if !(0..0xFFFF).contains(&i) {
        return Ok(0);
    }
    let Some(b) = api.object(i as u16).map(|u| u.id.0 as usize) else {
        c.out_u32(1, 0)?;
        return Ok(0);
    };
    let ids: Vec<u16> = api.world.units[b].inside.iter().map(|id| id.0 as u16).collect();
    if ids.is_empty() {
        c.out_u32(1, 0)?;
        return Ok(0);
    }
    let key = (api.vm, format!("inside{b}").into_bytes());
    let bytes: Vec<u8> = ids.iter().flat_map(|v| v.to_le_bytes()).collect();
    let p = match api.st.text_cache.get(&key) {
        Some(&p) if c.raw.heap.size(p).is_some_and(|s| s as usize >= bytes.len()) => {
            c.raw.mem.write_bytes(p, &bytes)?;
            p
        }
        _ => {
            let p = c.alloc_bytes(&[bytes.clone(), vec![0; 64]].concat())?;
            api.st.text_cache.insert(key, p);
            p
        }
    };
    c.out_u32(1, p)?;
    Ok(ids.len() as u32)
}

/// `int GetNInside(byte nation)`: units inside the selected buildings of a nation.
fn get_n_inside(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = usize::from(api.assign(c.u8(0)?) & 7);
    let sel = api.st.selection[n].clone();
    Ok(sel.iter().filter_map(|&m| api.member(m)).map(|u| u.inside.len() as u32).sum())
}

/// `void InviteUnitIntoBuilding(int building, int unit, byte ordType)` (0x56c638): both
/// indices in `0..=0xFFFF` name living objects (no building test here), then
/// `unit->GoToMine(building, 0xFF, ordType)`; `GoToMine` does the rest of the checks.
fn invite_unit_into_building(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(b), Some(u)) = (building_arg(api, c, 0)?, building_arg(api, c, 1)?) else { return Ok(0) };
    let ord = c.u8(2)?;
    acr_sim::economy::enter_ex(api.world, u, b, 0xFF, ord);
    Ok(0)
}

/// `void LeaveBuilding(int building, word unitType)` (0x56c6ca): an index in `0..0xFFFF`
/// naming an object with someone inside (OB+0xf0) gets `LeaveMine(unitType)` (0x5f518b,
/// [`acr_sim::economy::leave_mine`]): after `EXITPAUSE` the first unit of that type walks
/// out (transports unload through `LeaveTransport`). No building or order test here.
fn leave_building(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    let Some(b) = u16::try_from(i).ok().filter(|&i| i != 0xFFFF).and_then(|i| api.object(i)).map(|u| u.id.0 as usize)
    else {
        return Ok(0);
    };
    if !api.world.units[b].inside.is_empty() {
        acr_sim::economy::leave_mine(api.world, b, acr_sim::TypeId(c.u16(1)?));
    }
    Ok(0)
}

/// `void SGP_ComeIntoBuilding(GAMEOBJ* grp, byte mode, int building)`.
fn sgp_come_into_building(api: &mut Api, c: &mut Call) -> Result<u32> {
    let h = c.handle(0)?;
    if h.magic != magic::GROUP || usize::from(h.index) >= api.st.groups.len() {
        return Ok(0);
    }
    let b = c.i32(2)?;
    let Some(u) = u16::try_from(b).ok().and_then(|b| api.object(b)).filter(|u| u.living() && u.building) else {
        return Ok(0);
    };
    let order = crate::state::GroupOrder::ComeIntoBuilding { building: b as u16, serial: api.st.serial(u.id) };
    let orders = &mut api.st.groups[usize::from(h.index)].orders;
    match c.u8(1)? {
        1 => orders.insert(0, order),
        2 => orders.push(order),
        _ => *orders = vec![order],
    }
    Ok(0)
}

/// `bool CreateObject0(GAMEOBJ* dst, GAMEOBJ* formation, GAMEOBJ* type, byte nation,
/// GAMEOBJ* zone, byte dir)`: a new group of `NUnits(formation)` objects in the formation's
/// slots around the (first) zone; always writes a new group handle.
fn create_object0(api: &mut Api, c: &mut Call) -> Result<u32> {
    const WHAT: &str = "CreateObject(DstObj,Formation,UnitType,Nation,Zone,Direction)";
    let n = api.assign(c.u8(3)?);
    if n >= 8 {
        api.st.error(format!("{WHAT} : Invalid parameter <Nation>"));
        return Ok(0);
    }
    let form = c.handle(1)?;
    let shape =
        (form.magic == magic::FORMATION).then(|| api.world.formations.shapes.get(usize::from(form.index))).flatten();
    let Some(shape) = shape.cloned() else {
        api.st.error(format!("{WHAT} : Invalid parameter <Formation>"));
        return Ok(0);
    };
    let Some(t) = api.type_arg(c, 2, WHAT)? else { return Ok(0) };
    let zh = c.handle(4)?;
    let centre = if is_point(&zh) || zh.magic == magic::ZONE { api.zone_point(&zh) } else { None };
    let Some((x, y)) = centre else {
        api.st.error(format!("{WHAT} : Invalid parameter <Zone>"));
        return Ok(0);
    };
    let mut members = Vec::new();
    for (px, py) in shape.positions(x << 4, y << 4, c.u8(5)?) {
        if let Some(id) = world_ops::create(api.world, t, n, Pos::new(px, py))
            && members.len() < 1024
        {
            members.push(api.member_of(id));
        }
    }
    let g = api.st.groups.len();
    api.st.groups.push(Group { members, ..Group::default() });
    c.set_handle(c.u32(0)?, g as u16, 0, magic::GROUP)?;
    Ok(1)
}

/// `void DieUnit(word index)`.
fn die_unit(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(u) = api.object(c.u16(0)?) {
        let id = u.id;
        world_ops::destruct(api.world, id);
    }
    Ok(0)
}

/// `void CreateFields(byte nat)` (0x577ff4): every ready mill of the nation plants fields.
fn create_fields(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    if n < 8 {
        acr_sim::economy::create_fields(api.world, n);
    }
    Ok(0)
}

/// `bool CheckLeaveAbility(byte nat)` (0x560b17): a selected unit of the nation's selection
/// is a docked transport.
fn check_leave_ability(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?) & 7;
    let docked = api.selected(n).iter().any(|id| {
        let u = &api.world.units[id.0 as usize];
        api.world.catalog.get(u.kind).transport && matches!(u.dock, acr_sim::ship::Dock::At(_))
    });
    Ok(u32::from(docked))
}

/// `void SendUnitsToTransport(byte nat)` (0x56093a): the other selected units board the
/// selection's last transport (`GoToTransport`, priority 0x90).
fn send_units_to_transport(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?) & 7;
    let ids = api.selected(n);
    let is_t = |api: &Api, id: &EntityId| {
        let u = &api.world.units[id.0 as usize];
        u.living() && api.world.catalog.get(u.kind).transport
    };
    let Some(&t) = ids.iter().rev().find(|id| is_t(api, id)) else { return Ok(0) };
    let riders: Vec<EntityId> = ids.iter().copied().filter(|id| !is_t(api, id)).collect();
    for id in riders {
        acr_sim::ship::go_to_transport(api.world, id.0 as usize, t.0 as usize, 0x90);
    }
    Ok(0)
}

/// `void SetMaxInTransport(byte nat, int n)` (0x57033a): how many units a new transport of
/// the AI loads (`City+0x189`, 80 at the start; see `acr_sim::desant`).
fn set_max_in_transport(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    if n < 8 {
        api.world.desant.cities[usize::from(n)].max_in_transport = c.i32(1)?;
    }
    Ok(0)
}

/// `AssignDesantStartGroup(byte nat, GAMEOBJ* grp)` / `AssignDesantDestGroup` (0x5701ea /
/// 0x570224, `City+0xf4e` / `+0xf50`): the AI landing takes its units from the start group
/// and puts them into the destination group when they are ashore. The nation is not
/// remapped; the start group's `nat` must be below 8 (the destination's is not checked).
fn assign_desant(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = usize::from(c.u8(0)?);
    let Some(g) = api.group(c, 1)? else { return Ok(0) };
    if n >= 8 {
        return Ok(0);
    }
    if c.name() == "AssignDesantStartGroup" {
        api.st.desant[n].0 = Some(g);
    } else {
        api.st.desant[n].1 = Some(g);
        api.world.desant.cities[n].dest = true;
    }
    Ok(0)
}

/// Writes the AC `OneUnit` struct (0x24 bytes) for a unit.
fn write_one_unit(api: &Api, c: &mut Call, out: u32, id: EntityId, full: bool) -> Result<()> {
    let u = &api.world.units[id.0 as usize];
    let t = api.world.catalog.get(u.kind);
    c.write_u16(out, id.0 as u16)?;
    c.write_u16(out + 2, api.st.serial(id))?;
    c.write_u16(out + 4, u.life.clamp(0, 0xFFFF) as u16)?;
    c.write_u16(out + 6, t.life.clamp(0, 0xFFFF) as u16)?;
    c.write_u8(out + 8, u.attack_bonus as u8)?;
    c.write_u8(out + 9, u.defence_bonus as u8)?;
    let stages = api.world.max_stage(u);
    c.write_u16(out + 0xa, u.stage.clamp(0, 0xFFFF) as u16)?;
    c.write_u16(out + 0xc, stages.clamp(0, 0xFFFF) as u16)?;
    c.write_u16(out + 0xe, u.kills)?;
    c.write_u16(out + 0x10, u.kind.0)?;
    c.write_u8(out + 0x12, t.role.usage)?;
    c.write_u8(out + 0x13, u8::from(u.building))?;
    c.write_i32(out + 0x14, u.pos.x >> 4)?;
    c.write_i32(out + 0x18, u.pos.y >> 4)?;
    c.write_u8(out + 0x1c, u.owner)?;
    c.write_u8(out + 0x1d, u.dir)?;
    c.write_u8(out + 0x1e, u8::from(u.stage == stages))?;
    // Reload in percent: `MaxDelay ? delay·100/MaxDelay : 0` (low byte).
    c.write_u8(out + 0x1f, if u.max_delay != 0 { (u.delay * 100 / u.max_delay) as u8 } else { 0 })?;
    if full {
        c.write_u8(out + 0x20, u.stance)?;
        c.write_u8(out + 0x21, api.st.unit_nmask(id, u.owner))?;
    }
    c.write_u8(out + 0x22, u8::from(!u.orders.is_empty()))?;
    if full {
        c.write_u8(out + 0x23, u8::from(u.target().is_some()))?;
    }
    Ok(())
}

/// `bool GetUnitInfo(GAMEOBJ* units, int index, OneUnit* out)`.
fn get_unit_info(api: &mut Api, c: &mut Call) -> Result<u32> {
    let out = c.u32(2)?;
    c.write_u16(out, NO_UNIT)?;
    let Some(g) = api.group_arg(c, 0, "GetUnitInfo(Units,Index,Uni)")? else { return Ok(0) };
    let i = c.i32(1)?;
    let Some(&m) = usize::try_from(i).ok().and_then(|i| api.st.groups[g].members.get(i)) else { return Ok(0) };
    let Some(id) = api.member_h(m).map(|u| u.id) else { return Ok(0) };
    write_one_unit(api, c, out, id, true)?;
    Ok(1)
}

/// `bool GetUnitGlobalInfo(int index, OneUnit* out)` (0x567cb9): `out->Index = 0xFFFF`, then an
/// object at `index < 0xFFFF` that is not dying (`Sdoxlo == 0`, or `Hidden` inside a
/// building: ACR's `living()`) fills the fields `0..=0x1f` and `0x22` exactly as
/// `GetUnitInfo` (0x567a0e) does; no serial check, and `0x20`/`0x21`/`0x23` are left alone.
fn get_unit_global_info(api: &mut Api, c: &mut Call) -> Result<u32> {
    let out = c.u32(1)?;
    c.write_u16(out, NO_UNIT)?;
    let i = c.i32(0)?;
    let Some(id) =
        u16::try_from(i).ok().filter(|&i| i != 0xFFFF).and_then(|i| api.object(i)).filter(|u| u.living()).map(|u| u.id)
    else {
        return Ok(0);
    };
    write_one_unit(api, c, out, id, false)?;
    Ok(1)
}

/// `bool GetUnitInfo2(word index, UnitInfo2* out)` (packed): no state checks.
fn get_unit_info2(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = api.object(c.u16(0)?) else { return Ok(0) };
    let (x, y, owner, id) = (u.pos.x >> 4, u.pos.y >> 4, u.owner, u.id);
    let inside = u.inside.len() as i32;
    let t = api.world.catalog.get(u.kind);
    let e = &t.economy;
    let max = e.unit_absorber.max(e.peasant_absorber);
    let usage = t.role.usage;
    let out = c.u32(1)?;
    c.write_u16(out, id.0 as u16)?;
    c.write_i32(out + 2, inside)?;
    c.write_i32(out + 6, max)?;
    c.write_i32(out + 0xa, x)?;
    c.write_i32(out + 0xe, y)?;
    c.write_i32(out + 0x12, i32::from(owner))?;
    c.write_u8(out + 0x16, usage)?;
    Ok(1)
}

/// `void SetUnitInfo(OneUnit* u)` (0x567e8a): for `u->Index != 0xFFFF` naming an object
/// with the same serial (OB+0x71) that is not dying (`Sdoxlo` OB+0x58 == 0), writes
/// `Life` (OB+0x78) from +4 and the formation attack and defence bonuses (OB+0x61/+0x62,
/// signed bytes) from +8/+9; an object that is not a building (OB+0x27 bit 4) also gets
/// `RealX/RealY = x << 4, y << 4` from +0x14/+0x18, written raw (no occupancy update).
fn set_unit_info(api: &mut Api, c: &mut Call) -> Result<u32> {
    let p = c.u32(0)?;
    let idx = c.read_u16(p)?;
    if idx == 0xFFFF {
        return Ok(0);
    }
    let serial = c.read_u16(p + 2)?;
    let Some(u) = api.object(idx).filter(|u| u.living() && api.st.serial(u.id) == serial) else { return Ok(0) };
    let id = u.id;
    let life = i32::from(c.read_u16(p + 4)?);
    let (attack, defence) = (i32::from(c.read_u8(p + 8)? as i8), i32::from(c.read_u8(p + 9)? as i8));
    let (x, y) = (c.read_u32(p + 0x14)? as i32, c.read_u32(p + 0x18)? as i32);
    let u = &mut api.world.units[id.0 as usize];
    u.life = life;
    (u.attack_bonus, u.defence_bonus) = (attack, defence);
    if !u.building {
        u.pos = Pos::new(x << 4, y << 4);
    }
    Ok(0)
}

/// `bool GetUnitExCaps(int index, UnitExCaps* caps, bool needOrderType)` (0x56be77): an
/// object at `0 <= index < MAXOBJECT` that is not dying (`Sdoxlo == 0`, or `Hidden` inside a
/// building: ACR's `living()`) fills `caps`: `RealDir` (OB+0xea), `DestX/DestY >> 4`
/// (OB+0xb8/+0xbc, −1 without a destination), `NIndex`, the type's `MonsterID`, `NNUM`,
/// `StandTime` (OB+0x91) and, with `needOrderType`, the kind of its executed order
/// (`LocalOrder->DoLink` looked up by 0x4b0f8c, see [`order_kind`]), else −1.
fn get_unit_ex_caps(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    let Some(u) = u16::try_from(i).ok().and_then(|i| api.object(i)).filter(|u| u.living()) else { return Ok(0) };
    let (dir, dest, kind, owner, stand) = (u.dir, u.dest, u.kind, u.owner, u.stand_time);
    let order = order_kind(api.world, u);
    let name = api.world.catalog.get(kind).id.clone();
    let out = c.u32(1)?;
    c.write_i32(out + 4, dest.map_or(-1, |d| d.x >> 4))?;
    c.write_i32(out + 8, dest.map_or(-1, |d| d.y >> 4))?;
    c.write_i32(out + 0x14, i32::from(owner))?;
    c.write_i32(out, i32::from(dir))?;
    c.write_i32(out + 0xc, i32::from(kind.0))?;
    let key = (api.vm, format!("unitid:{name}").into_bytes());
    let p = match api.st.text_cache.get(&key) {
        Some(&p) => p,
        None => {
            let p = c.alloc_str(name.as_bytes())?;
            api.st.text_cache.insert(key, p);
            p
        }
    };
    c.write_u32(out + 0x10, p)?;
    c.write_i32(out + 0x18, i32::from(stand))?;
    c.write_i32(out + 0x1c, if c.bool(2)? { order } else { -1 })?;
    Ok(1)
}

/// The kind of a unit's executed order as 0x4b0f8c reports it: the index of its link in the
/// engine's table of 34 order links (0x66a258), −1 without an order. ACR keeps the orders a
/// building runs as state, so they are mapped in the order its building link runs them.
pub fn order_kind(world: &acr_sim::World, u: &acr_sim::world::Unit) -> i32 {
    use acr_sim::world::Order;
    if u.building {
        return match u.orders.last() {
            // `AttackObjLink` (0x51bcaf, building branch 0x51c30a).
            Some(Order::Attack { .. }) => 2,
            _ if u.research.is_some() => 4, // `PerformUpgradeLink` 0x54f6bb
            _ if u.unconstruct => 33,       // `UNCONSTRUCT` 0x4a0d47
            _ if !u.queue.is_empty() => 1,  // `ProduceObjLink` 0x49eeb7
            _ if u.mine.is_some() => 7,     // `TakeResourceFromSpriteLink` 0x5f75b3
            _ => -1,
        };
    }
    match u.orders.last() {
        None => -1,
        Some(Order::Build { .. }) => 0, // 0x524654
        Some(Order::Attack { .. }) if world.catalog.get(u.kind).guns.is_some() => 16, // 0x520ef8
        Some(Order::Attack { .. }) => 2, // 0x51bcaf
        Some(Order::Gather { .. }) => 3, // 0x4d2fb0
        Some(Order::Enter { .. }) => 5, // 0x5f46f7
        Some(Order::SendTo { .. }) => 8, // 0x5172c6
        Some(Order::ClearUnlimited) => 11, // 0x527dec
        Some(Order::WaterSendTo { .. }) => 12, // 0x54ac93
        Some(Order::LeaveMine { .. }) => 6, // 0x5f522e
        Some(Order::LeaveTransport { .. }) => 17, // 0x54e188
        Some(Order::GoToTransport { .. }) => 18, // 0x54dbb0
        Some(Order::Fishing) => 19,     // 0x54eb16
        Some(Order::SmartSendTo { .. }) => 21, // 0x52b84a
        Some(Order::Turn { .. }) => 26, // 0x41bd0d
        Some(Order::Patrol { .. }) => 27, // 0x4ea3f0
        Some(Order::AttackPoint { .. }) => 30, // 0x51bc5b
        Some(Order::Panic { .. }) => 32, // 0x600720
    }
}

/// `int GetCurrentUnits(byte nation)`: population (`NGidot`, from the census).
fn get_current_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    Ok(if n < 8 { api.world.players[usize::from(n)].population.max(0) as u32 } else { 0 })
}

/// `int GetMaxUnits(byte nation)`: housing (`NFarms`).
fn get_max_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    Ok(if n < 8 { api.world.players[usize::from(n)].housing.max(0) as u32 } else { 0 })
}

/// `bool CheckBuildingsComplete(GAMEOBJ* buildings)`: no idle member building is still
/// under construction.
fn check_buildings_complete(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 0, "CheckBuildingsComplete(Buildings)")? else { return Ok(0) };
    let unfinished = api.members_h(g).into_iter().any(|id| {
        let u = &api.world.units[id.0 as usize];
        u.building && u.orders.is_empty() && u.stage < api.world.max_stage(u)
    });
    Ok(u32::from(!unfinished))
}

/// `bool GetZoneCoor(GAMEOBJ* zone, int* x, int* y)` (the `'ZONE'` test comes first here).
fn get_zone_coor(api: &mut Api, c: &mut Call) -> Result<u32> {
    if c.u32(0)? == 0 {
        c.out_i32(1, 0)?;
        c.out_i32(2, 0)?;
        return Ok(0);
    }
    let h = c.handle(0)?;
    let p = if h.magic == magic::ZONE {
        api.st
            .zone_groups
            .get(usize::from(h.index))
            .and_then(|z| z.first())
            .and_then(|&z| api.st.zones.get(z))
            .map(|z| (z.x, z.y))
    } else if is_point(&h) {
        Some((i32::from(h.index), i32::from(h.serial)))
    } else {
        None
    };
    let Some((x, y)) = p else { return Ok(0) };
    c.out_i32(1, x)?;
    c.out_i32(2, y)?;
    Ok(1)
}

/// `bool CreateFormation(byte nat, GAMEOBJ* officers, GAMEOBJ* drummers, GAMEOBJ* flags,
/// GAMEOBJ* soldiers, GAMEOBJ* dest, char* form)` (0x56fbf8): cleans the four source groups,
/// finds the shape by name, empties `dest` and moves into it, with `CopyUnits` (0x56ea22),
/// the first officer, the first drummer, the shape's number of soldiers and the first flag
/// bearer; then `CreateFromGroup` (0x41a00e) forms the brigade from `dest`. A missing
/// officer leaves `dest` empty; a missing drummer moves the officer back; too few soldiers
/// moved move only the soldiers back (officer and drummer stay in `dest`).
fn create_formation(api: &mut Api, c: &mut Call) -> Result<u32> {
    // Only officers, drummers, flags and dest are checked for the group magic.
    for arg in [1u32, 2, 3, 5] {
        if c.handle(arg)?.magic != magic::GROUP {
            return Ok(0);
        }
    }
    let mut g = [0usize; 5];
    for (k, arg) in [1u32, 2, 3, 4, 5].into_iter().enumerate() {
        let Some(i) = api.group(c, arg)? else { return Ok(0) };
        g[k] = i;
    }
    let [off, drum, flag, sold, dest] = g;
    // CleanGroup (0x56cf09) of the sources.
    for k in [off, drum, flag, sold] {
        let kept: Vec<_> = api.st.groups[k].members.iter().copied().filter(|&m| api.member_h(m).is_some()).collect();
        api.st.groups[k].members = kept;
    }
    let name = c.str(6)?;
    let Some(shape) = api.world.formations.shapes.iter().position(|s| s.name == name) else { return Ok(0) };
    let n = api.world.formations.shapes[shape].slots();
    let len = |api: &Api, k: usize| api.st.groups[k].members.len();
    let mv = |api: &mut Api, s: usize, d: usize, start: i32, cnt: usize| {
        super::groups::copy_members(&mut api.st.groups, s, d, start, cnt as i32, true, true);
    };
    api.st.groups[dest].members.clear();
    if len(api, sold) < n {
        return Ok(0);
    }
    mv(api, off, dest, 0, 1);
    if len(api, dest) != 1 {
        return Ok(0);
    }
    mv(api, drum, dest, 0, 1);
    if len(api, dest) != 2 {
        mv(api, dest, off, 0, 1);
        return Ok(0);
    }
    mv(api, sold, dest, 0, n);
    if len(api, dest) != n + 2 {
        mv(api, dest, sold, 2, n);
        return Ok(0);
    }
    mv(api, flag, dest, 0, 1);
    let mut units = Vec::new();
    for &m in &api.st.groups[dest].members {
        let Some(u) = api.member_h(m) else { return Ok(0) };
        units.push(u.id.0 as usize);
    }
    Ok(u32::from(acr_sim::formation::create_from_group(api.world, &units, shape)))
}

/// `bool GetBuildingEntryPoint(int* x, int* y, word index, int addDistance)` (0x56ea8c): the
/// building's first `CONCENTRATOR` point (relative to the lock base cell 0x516fde, in
/// pixels), moved `addDistance` px further away from the building's centre when it lies more
/// than 20 px from it. False for no object, no building or no `CONCENTRATOR`.
fn get_building_entry_point(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = api.object(c.u16(2)?).filter(|u| u.building) else { return Ok(0) };
    let t = api.world.catalog.get(u.kind);
    let Some(&(ex, ey)) = t.economy.concentrator.first() else { return Ok(0) };
    let (bx, by) = acr_sim::economy::base_cell(t, u.pos);
    let (mut x, mut y) = ((bx + ex) << 4, (by + ey) << 4);
    let (px, py) = (u.pos.x >> 4, u.pos.y >> 4);
    let d = super::util::norma(x - px, y - py);
    if d > 20 {
        let add = c.i32(3)?;
        x += (x - px).wrapping_mul(add) / d;
        y += (y - py).wrapping_mul(add) / d;
    }
    c.out_i32(0, x)?;
    c.out_i32(1, y)?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_formats::defs::{OrderGroups, OrderList, UnitDef};
    use acr_script::heap::Heap;
    use acr_script::memory::Memory;
    use acr_script::runtime::HostCall;
    use acr_script::x86::Cpu;
    use acr_sim::catalog::{Catalog, ObjectType};
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;
    use acr_sim::{Pos, World};

    use super::super::call::magic;
    use super::super::{Api, Call, Caller, function};
    use crate::state::{Group, Member, ScriptSetup, ScriptState};
    use crate::texts::MissionTexts;

    const STACK: u32 = 0x1_8000;
    const DATA: u32 = 0x1_c000;

    fn def(name: &str, extra: &str) -> UnitDef {
        UnitDef::parse(
            format!(
                "NAME {name}\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
                 @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n{extra}[END]\r\n"
            )
            .as_bytes(),
        )
    }

    /// Groups 0..=4: officers (1), drummers (`drummers`), flags (1), soldiers (`soldiers`), dest.
    fn call(drummers: usize, soldiers: usize) -> (u32, World, ScriptState) {
        let mut c = Catalog::new();
        let pik = c.add(ObjectType::from_def("Pik(EN)", &def("Pik", "")));
        let off = c.add(ObjectType::from_def("Off(EN)", &def("Off", "OFFICER\r\n")));
        let drum = c.add(ObjectType::from_def("Drum(EN)", &def("Drum", "BARABAN\r\n")));
        let orders = OrderList::parse(b"#KARE15 SYM0 9 2 9 3\r\n*****\r\n* C *\r\n** C*\r\n*****\r\n#END\r\n#EXIT\r\n");
        let f = Formations::new(&orders, &OrderGroups::parse(b"1\r\nKARE 2 0 1 1 #KARE15\r\n"));
        let mut world = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f));
        let mut st = ScriptState::new(ScriptSetup::default(), MissionTexts::default());
        let mut group = |kind, n: usize, world: &mut World| {
            let members = (0..n)
                .map(|i| {
                    let id = world.spawn(kind, 0, Pos::from_pixels(1000 + 30 * i as i32, 1000), 0);
                    Member { id: id.0 as u16, serial: st.serial(id) }
                })
                .collect();
            st.groups.push(Group { members, ..Group::default() });
        };
        group(off, 1, &mut world);
        group(drum, drummers, &mut world);
        group(off, 1, &mut world);
        group(pik, soldiers, &mut world);
        group(pik, 1, &mut world);
        let mut mem = Memory::new();
        mem.map(0x1_0000, 0x1_0000);
        for g in 0..5u32 {
            mem.write_u16(DATA + 8 * g, g as u16).expect("index");
            mem.write_u32(DATA + 8 * g + 4, magic::GROUP).expect("magic");
        }
        for (i, b) in b"#KARE15\0".iter().enumerate() {
            mem.write_u8(DATA + 0x100 + i as u32, *b).expect("name");
        }
        let args = [0, DATA, DATA + 8, DATA + 16, DATA + 24, DATA + 32, DATA + 0x100];
        let mut cpu = Cpu::new();
        cpu.regs[usize::from(acr_script::x86::decode::reg::ESP)] = STACK;
        for (i, &a) in args.iter().enumerate() {
            mem.write_u32(STACK + 4 + 4 * i as u32, a).expect("stack");
        }
        let mut heap = Heap::new(0x2_0000, 0x3_0000);
        let name = "CreateFormation";
        let mut raw = HostCall { dll: "test", name, cpu: &mut cpu, mem: &mut mem, heap: &mut heap };
        let (mut commands, mut events) = (Vec::new(), Vec::new());
        let mut api = Api {
            st: &mut st,
            world: &mut world,
            commands: &mut commands,
            events: &mut events,
            caller: Caller::Ai(0),
            vm: 0,
            dip: None,
        };
        let r = (function(name).expect("known").handler)(&mut api, &mut Call { raw: &mut raw }).expect("call");
        (r, world, st)
    }

    fn sizes(st: &ScriptState) -> Vec<usize> {
        st.groups.iter().map(|g| g.members.len()).collect()
    }

    #[test]
    fn create_formation_moves_officer_drummer_soldiers_and_flag() {
        let (r, world, st) = call(1, 18);
        assert_eq!(r, 1);
        // KARE15 has 15 slots: officer, drummer, 15 soldiers and the flag bearer move to dest.
        assert_eq!(world.formations.shapes[0].slots(), 15);
        assert_eq!(sizes(&st), [0, 0, 0, 3, 18]);
        assert_eq!(world.brigades.len(), 1);
        assert_eq!(world.brigades[0].members.len(), 18);
    }

    #[test]
    fn create_formation_failures_move_back_only_part() {
        // No drummer: the officer goes back.
        let (r, world, st) = call(0, 18);
        assert_eq!((r, sizes(&st)), (0, vec![1, 0, 1, 18, 0]));
        assert!(world.brigades.is_empty());
        // Too few soldiers: nothing moves.
        let (r, _, st) = call(1, 14);
        assert_eq!((r, sizes(&st)), (0, vec![1, 1, 1, 14, 0]));
    }
}
