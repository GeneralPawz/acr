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
    f(
        "GetTotalAmount1",
        0x55e7f2,
        Status::Partial("counted live; the original reads a per-nation cache refreshed every 8 ticks"),
        get_total_amount1,
    ),
    f("GetUnitsByNation", 0x566ca6, DONE, get_units_by_nation),
    f("GetAmountOfWarriors", 0x55eaba, DONE, get_amount_of_warriors),
    f("GetInsideAmount", 0x57819e, DONE, get_inside_amount),
    f("GetMaxInsideAmount", 0x5781e5, Status::Done, get_max_inside_amount),
    f("GetInsideList", 0x56c713, DONE, get_inside_list),
    f("GetNInside", 0x5604e2, DONE, get_n_inside),
    f(
        "InviteUnitIntoBuilding",
        0x56c638,
        Status::Partial("economy::enter (capacity and owner rules of acr-sim)"),
        invite_unit_into_building,
    ),
    f(
        "LeaveBuilding",
        0x56c6ca,
        Status::Partial("everybody leaves (acr-sim releases all units at once)"),
        leave_building,
    ),
    f("CheckLeaveAbility", 0x560b17, Status::Done, check_leave_ability),
    f("SGP_ComeIntoBuilding", 0x577c2a, DONE, sgp_come_into_building),
    f("CreateObject0", 0x55eded, DONE, create_object0),
    f("CreateFields", 0x577ff4, Status::Done, create_fields),
    f("DieUnit", 0x5701ae, Status::Partial("World::kill; no SLOWDEATH burning yet"), die_unit),
    f("GetUnitInfo", 0x567a0e, DONE, get_unit_info),
    f("GetUnitInfo2", 0x578f2c, DONE, get_unit_info2),
    f("SetUnitInfo", 0x567e8a, Status::Partial("life and position; no damage/shield bonus yet"), set_unit_info),
    f("GetUnitGlobalInfo", 0x567cb9, Status::Partial("as GetUnitInfo"), get_unit_global_info),
    f("GetUnitExCaps", 0x56be77, Status::Partial("StandTime 0, order kind 8 (send-to) or -1"), get_unit_ex_caps),
    f("GetCurrentUnits", 0x578173, DONE, get_current_units),
    f("GetMaxUnits", 0x578148, DONE, get_max_units),
    f("CheckBuildingsComplete", 0x566a30, DONE, check_buildings_complete),
    f("SendUnitsToTransport", 0x56093a, Status::Done, send_units_to_transport),
    f(
        "SetMaxInTransport",
        0x57033a,
        Status::Partial("stored; the AI landing process is not modelled"),
        set_max_in_transport,
    ),
    f("GetZoneCoor", 0x56c266, DONE, get_zone_coor),
    f("GetBuildingEntryPoint", 0x56ea8c, Status::Stub("no building entry points in the catalog yet: 0"), zero),
    f(
        "AssignDesantStartGroup",
        0x5701ea,
        Status::Partial("stored; the AI landing process is not modelled"),
        assign_desant,
    ),
    f(
        "AssignDesantDestGroup",
        0x570224,
        Status::Partial("stored; the AI landing process is not modelled"),
        assign_desant,
    ),
    f(
        "CreateFormation",
        0x56fbf8,
        Status::Partial("moves officer, drummer and soldiers into the group; Command::Form"),
        create_formation,
    ),
];

fn zero(_: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(0)
}

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

/// `int GetTotalAmount1(GAMEOBJ* type, byte nation)`: objects of the type the nation owns
/// (buildings included).
fn get_total_amount1(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(1)?);
    let Some(t) = api.type_arg(c, 0, "GetTotalAmount(UnitType,Nation)")? else { return Ok(0) };
    if n >= 8 {
        api.st.error("GetTotalAmount(UnitType,Nation) : Invalid parameter <Nation>");
        return Ok(0);
    }
    Ok(api.world.units.iter().filter(|u| u.living() && u.owner == n && u.kind == t).count() as u32)
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

/// `void InviteUnitIntoBuilding(int building, int unit, byte ordType)`: `GoToMine` with
/// priority 0x7f.
fn invite_unit_into_building(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(b), Some(u)) = (building_arg(api, c, 0)?, building_arg(api, c, 1)?) else { return Ok(0) };
    if api.world.units[b].building && !api.world.units[u].building {
        acr_sim::economy::enter(api.world, u, b, 0x7f, false);
    }
    Ok(0)
}

/// `void LeaveBuilding(int building, word unitType)`.
fn leave_building(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(b) = building_arg(api, c, 0)?
        && !api.world.units[b].inside.is_empty()
    {
        acr_sim::economy::leave(api.world, b);
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

/// `void SetMaxInTransport(byte nat, int n)` (0x57033a).
fn set_max_in_transport(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    if n < 8 {
        api.st.desant[usize::from(n)].2 = c.i32(1)?;
    }
    Ok(0)
}

/// `AssignDesantStartGroup(byte nat, GAMEOBJ* grp)` / `AssignDesantDestGroup` (0x5701ea /
/// 0x570224): the AI landing's groups.
fn assign_desant(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = usize::from(c.u8(0)? & 7);
    let Some(g) = api.group(c, 1)? else { return Ok(0) };
    if c.name() == "AssignDesantStartGroup" {
        api.st.desant[n].0 = g as u16;
    } else {
        api.st.desant[n].1 = g as u16;
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
    c.write_u8(out + 0x1e, u8::from(u.stage >= stages))?;
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

/// `bool GetUnitGlobalInfo(int index, OneUnit* out)`: no serial check.
fn get_unit_global_info(api: &mut Api, c: &mut Call) -> Result<u32> {
    let out = c.u32(1)?;
    c.write_u16(out, NO_UNIT)?;
    let i = c.i32(0)?;
    let Some(id) = u16::try_from(i).ok().and_then(|i| api.object(i)).filter(|u| u.living()).map(|u| u.id) else {
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

/// `void SetUnitInfo(OneUnit*)`: life and (for non-buildings) position.
fn set_unit_info(api: &mut Api, c: &mut Call) -> Result<u32> {
    let p = c.u32(0)?;
    let idx = c.read_u16(p)?;
    let serial = c.read_u16(p + 2)?;
    let Some(u) = api.object(idx).filter(|u| u.living() && api.st.serial(u.id) == serial) else { return Ok(0) };
    let id = u.id;
    let life = i32::from(c.read_u16(p + 4)?);
    let (x, y) = (c.read_u32(p + 0x14)? as i32, c.read_u32(p + 0x18)? as i32);
    let u = &mut api.world.units[id.0 as usize];
    u.life = life;
    if !u.building {
        // TODO(acr-sim): teleport with occupancy update; the original writes RealX/RealY raw.
        u.pos = Pos::new(x << 4, y << 4);
    }
    Ok(0)
}

/// `bool GetUnitExCaps(int index, UnitExCaps* caps, bool needOrderType)`.
fn get_unit_ex_caps(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    if i < 0 || i as usize >= api.world.units.len() {
        return Ok(0);
    }
    let u = &api.world.units[i as usize];
    if !u.living() {
        return Ok(0);
    }
    let (dir, dest, kind, owner, busy) = (u.dir, u.dest, u.kind, u.owner, !u.orders.is_empty());
    let name = api.world.catalog.get(kind).id.clone();
    let out = c.u32(1)?;
    c.write_i32(out, i32::from(dir))?;
    c.write_i32(out + 4, dest.map_or(-1, |d| d.x >> 4))?;
    c.write_i32(out + 8, dest.map_or(-1, |d| d.y >> 4))?;
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
    c.write_i32(out + 0x14, i32::from(owner))?;
    c.write_i32(out + 0x18, 0)?;
    c.write_i32(out + 0x1c, if c.bool(2)? && busy { 8 } else { -1 })?;
    Ok(1)
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
/// GAMEOBJ* soldiers, GAMEOBJ* dest, char* form)`.
fn create_formation(api: &mut Api, c: &mut Call) -> Result<u32> {
    let mut g = [0usize; 5];
    for (k, arg) in [1u32, 2, 3, 4, 5].iter().enumerate() {
        let h = c.handle(*arg)?;
        if (h.magic != magic::GROUP && *arg != 4) || usize::from(h.index) >= api.st.groups.len() {
            return Ok(0);
        }
        g[k] = usize::from(h.index);
    }
    let [off, drum, flag, sold, dest] = g;
    for &k in &[off, drum, flag, sold] {
        let kept: Vec<_> = api.st.groups[k].members.iter().copied().filter(|&m| api.member(m).is_some()).collect();
        api.st.groups[k].members = kept;
    }
    let name = c.str(6)?;
    let Some(shape) = api.world.formations.shapes.iter().find(|s| s.name == name) else { return Ok(0) };
    let n = shape.slots();
    api.st.groups[dest].members.clear();
    if api.st.groups[sold].members.len() < n
        || api.st.groups[off].members.is_empty()
        || api.st.groups[drum].members.is_empty()
    {
        return Ok(0);
    }
    let officer = api.st.groups[off].members.remove(0);
    let drummer = api.st.groups[drum].members.remove(0);
    let soldiers: Vec<_> = api.st.groups[sold].members.drain(..n).collect();
    let mut all = vec![officer, drummer];
    all.extend(soldiers.iter().copied());
    if !api.st.groups[flag].members.is_empty() {
        all.push(api.st.groups[flag].members.remove(0));
    }
    api.st.groups[dest].members = all.clone();
    let ids: Vec<EntityId> = all.iter().filter_map(|&m| api.member(m)).map(|u| u.id).collect();
    let Some(&first) = ids.first() else { return Ok(0) };
    let owner = api.world.units[first.0 as usize].owner;
    api.commands.push(acr_sim::Command::Form { player: owner, officer: first, units: ids[1..].to_vec(), kind: name });
    Ok(1)
}
