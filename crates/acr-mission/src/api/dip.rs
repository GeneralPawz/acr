//! `DipServer.dll`, the diplomacy system for neutral tribes (`$PSB` map groups), runs in the
//! interpreter like the scripts. This module holds the 48 `dmcr.exe` functions only it
//! imports and the 6 functions the AI DLLs import from it, which are forwarded into its
//! machine (the AI hands its squads over: `AddStorm`, `AddTomahawks`, ...).
//!
//! Semantics from the RE notes of `dmcr.exe` 1.46 (addresses per function); the squad
//! tactics themselves are DipServer's own code.

use acr_script::runtime::Result;
use acr_sim::Command;
use acr_sim::world::EntityId;

use super::call::magic;
use super::util::{is_point, norma};
use super::{Api, ApiFn, Call, Caller, Status};
use crate::events::MissionEvent;
use crate::state::{Member, NO_UNIT};

const fn f(name: &'static str, original: u32, status: Status, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status, handler }
}

const DONE: Status = Status::Done;
const FORWARD: Status = Status::Done;
const TACTIC: Status = Status::Partial("approximation on acr-sim (see the function's comment)");

pub const FUNCTIONS: &[ApiFn] = &[
    // Imported by the AI DLLs from DipServer.dll: forwarded into its machine.
    f("?SetAIForDiplo@@YA_NEHH@Z", 0, FORWARD, forward),
    f("?AddStorm@@YAXPAUGAMEOBJ@@EH@Z", 0, FORWARD, forward),
    f("?AddCannon@@YAXPAUGAMEOBJ@@E@Z", 0, FORWARD, forward),
    f("?AddTomahawks@@YAXPAUGAMEOBJ@@EGHH@Z", 0, FORWARD, forward),
    f("?AddFirers@@YAXPAUGAMEOBJ@@E@Z", 0, FORWARD, forward),
    f("?AddPsKillers@@YAXPAUGAMEOBJ@@E_N@Z", 0, FORWARD, forward),
    // dmcr.exe functions only DipServer imports.
    f("GetAmountOfGroups", 0x56bdda, DONE, get_amount_of_groups),
    f("GetGroupName", 0x56be12, DONE, get_group_name),
    f("GetGroupCharge", 0x574575, DONE, get_group_charge),
    f("isUnitAbsorber", 0x5752be, DONE, is_unit_absorber),
    f("AttackEnemyPeasantsInZone", 0x564d83, TACTIC, attack_enemy_peasants_in_zone),
    f("CreatePeasantsTopList", 0x57bd81, TACTIC, create_peasants_top_list),
    f("CreateDangerMapForFire", 0x57d6c4, TACTIC, danger_map),
    f(
        "CheckIfBuildingIsFired",
        0x57c39e,
        Status::Partial("no burning buildings in acr-sim yet: only removed ones"),
        check_if_building_is_fired,
    ),
    f("ArtAttackObject", 0x570b32, TACTIC, art_attack_object),
    f("ArtAttackGroud", 0x5709b5, TACTIC, art_attack_ground),
    f("CreateNonFiredEnemyBuildingsTopList", 0x57b2fd, TACTIC, create_enemy_buildings_top_list),
    f("CreateDangerMapWithoutPeasants", 0x57daac, TACTIC, danger_map),
    f("CheckIfNotBusy", 0x56f78f, TACTIC, check_if_not_busy),
    f("GetAlliesForce", 0x571dc4, TACTIC, get_allies_force),
    f("CreateTopListForStorm", 0x57b459, TACTIC, create_enemy_buildings_top_list),
    f("CreateTopListForStormHard", 0x57b5e9, TACTIC, create_enemy_buildings_top_list),
    f("SetDangerMap", 0x56fba1, Status::Stub("the engine's danger map is not read by acr-sim"), zero),
    f("CreateDangerMapForStorm", 0x57cd88, TACTIC, danger_map),
    f("CreateDangerMapForStormHard", 0x57d19e, TACTIC, danger_map),
    f("GetBuildingOposit", 0x57b9ae, TACTIC, get_building_oposit),
    f("OBJ_SmartSendTo", 0x56f0ef, Status::Partial("a single-unit Command::Move (offsets added)"), obj_smart_send_to),
    f("GetMaxAttRadius", 0x56f41e, DONE, get_max_att_radius),
    f("RemoveAttackingUnits", 0x570728, DONE, remove_attacking_units),
    f("FindNextZoneOnTheSafeWayToLimitedObject", 0x57ab1a, Status::Stub("0xFFFF: no way found"), no_zone),
    f("CheckLandLocking", 0x56f65a, DONE, check_land_locking),
    f("?RClose@@YAXPAX@Z", 0x40250c, DONE, r_close),
    f("?RBlockRead@@YAKPAX0K@Z", 0x40247f, DONE, r_block_read),
    f("?RFileSize@@YAKPAX@Z", 0x40242d, DONE, r_file_size),
    f("?RReset@@YAPAXPBD@Z", 0x402338, DONE, r_reset),
    f("?RBlockWrite@@YAKPAX0K@Z", 0x4024b0, Status::Partial("written files live in memory"), r_block_write),
    f("?RRewrite@@YAPAXPBD@Z", 0x402401, Status::Partial("written files live in memory"), r_rewrite),
    f("?Ggetch@@YAHPAVGFILE@@@Z", 0x404fde, DONE, g_getch),
    f("?Gopen@@YAPAVGFILE@@PAD0@Z", 0x404f14, DONE, g_open),
    f("?Gclose@@YAXPAVGFILE@@@Z", 0x40501a, DONE, r_close),
    f("GetZoneR", 0x56c319, DONE, get_zone_r),
    f("GetTCos", 0x56f6e9, DONE, get_tcos),
    f("GetTSin", 0x56f6d4, DONE, get_tsin),
    f("SetAI_Guard", 0x570251, Status::Stub("AI guard flag not modelled"), zero),
    f(
        "AddUnitsInZoneToGroup",
        0x56c914,
        Status::Partial("USAGE filter not modelled (any type)"),
        add_units_in_zone_to_group,
    ),
    f("GetMoneyString", 0x56f6fe, DONE, get_money_string),
    f("EraseGroup", 0x56d7e1, DONE, erase_group),
    f("SetGroupStartNMASK", 0x56f6a2, DONE, set_group_start_nmask),
    f(
        "?ActivateDipDialog@@YAXPAD@Z",
        0x5c3980,
        Status::Partial("reported as an event; no dialog pages yet"),
        activate_dip_dialog,
    ),
    f("GetNatNMASK", 0x56f689, DONE, get_nat_nmask),
    f("SetGroupNMASK", 0x56bceb, DONE, set_group_nmask),
    f("ChangeUnitColor", 0x56c778, DONE, change_unit_color),
    f("?SendDipCommand@@YAXPADH@Z", 0x5c3b00, DONE, send_dip_command),
    f(
        "GetLastFullWay",
        0x57c3d7,
        Status::Partial("the path of the last safe-way search is not kept: 0"),
        get_last_full_way,
    ),
];

fn zero(_: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(0)
}

fn no_zone(_: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(0xFFFF)
}

/// Exports of `DipServer.dll` the AI calls, with the number of 32-bit arguments.
fn export_args(name: &str) -> Option<u32> {
    Some(match name {
        "?SetAIForDiplo@@YA_NEHH@Z" => 3,
        "?AddStorm@@YAXPAUGAMEOBJ@@EH@Z" => 3,
        "?AddCannon@@YAXPAUGAMEOBJ@@E@Z" => 2,
        "?AddTomahawks@@YAXPAUGAMEOBJ@@EGHH@Z" => 5,
        "?AddFirers@@YAXPAUGAMEOBJ@@E@Z" => 2,
        "?AddPsKillers@@YAXPAUGAMEOBJ@@E_N@Z" => 3,
        _ => return None,
    })
}

/// Calls the same export in DipServer's machine. A `GAMEOBJ*` first argument (all but
/// `SetAIForDiplo`) is copied into DipServer's memory first; the handle only carries the
/// group index, which both machines share.
fn forward(api: &mut Api, c: &mut Call) -> Result<u32> {
    let name = c.name().to_owned();
    let n = export_args(&name).unwrap_or(0);
    let mut args: Vec<u32> = (0..n).map(|i| c.u32(i)).collect::<Result<_>>()?;
    let Some(dip) = api.dip.take() else { return Ok(0) };
    let result = (|| {
        let Some(addr) = dip.machine.image.export(&name) else { return Ok(0) };
        if !name.starts_with("?SetAIForDiplo") {
            let mut handle = [0u8; 8];
            if args[0] != 0 {
                c.raw.mem.read_bytes(args[0], &mut handle)?;
            }
            let scratch = crate::vm::scratch(dip)?;
            dip.machine.mem.write_bytes(scratch, &handle)?;
            args[0] = if args[0] == 0 { 0 } else { scratch };
        }
        let mut inner = Api {
            st: &mut *api.st,
            world: &mut *api.world,
            commands: &mut *api.commands,
            events: &mut *api.events,
            caller: Caller::Diplomacy,
            vm: crate::DIP_VM,
            dip: None,
        };
        dip.machine.call(&mut inner, addr, &args)
    })();
    api.dip = Some(dip);
    result
}

// --- named map groups ----------------------------------------------------------------------

fn get_amount_of_groups(api: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(api.st.map_groups.len() as u32)
}

/// `char* GetGroupName(int i)`: the name of map group `i`, copied into the caller's memory.
fn get_group_name(api: &mut Api, c: &mut Call) -> Result<u32> {
    let i = c.i32(0)?;
    let Some(g) = usize::try_from(i).ok().and_then(|i| api.st.map_groups.get(i)) else { return Ok(0) };
    let key = (api.vm, format!("group{i}").into_bytes());
    if let Some(&p) = api.st.text_cache.get(&key) {
        return Ok(p);
    }
    let name = g.name.clone().into_bytes();
    let p = c.alloc_str(&name)?;
    api.st.text_cache.insert(key, p);
    Ok(p)
}

/// `bool GetGroupCharge(GAMEOBJ*)`: every living member has reloaded.
fn get_group_charge(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    Ok(u32::from(api.members(g).iter().all(|id| api.world.units[id.0 as usize].delay == 0)))
}

/// `bool isUnitAbsorber(int index)`: the object can take units inside.
fn is_unit_absorber(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = u16::try_from(c.i32(0)?).ok().and_then(|i| api.object(i)).filter(|u| u.living()) else {
        return Ok(0);
    };
    Ok(u32::from(api.world.catalog.get(u.kind).economy.unit_absorber > 0))
}

fn erase_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(g) = api.group(c, 0)? {
        api.st.groups[g].members.clear();
    }
    Ok(0)
}

fn set_group_start_nmask(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some(g) = api.group(c, 0)? {
        api.st.groups[g].start_nmask = c.u8(1)?;
    }
    Ok(0)
}

fn set_group_nmask(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let mask = c.u8(1)?;
    let members = api.st.groups[g].members.clone();
    for m in members {
        if let Some(u) = api.by_serial_pub(m) {
            let id = u.id.0;
            api.st.extra.entry(id).or_default().nmask = Some(mask);
        }
    }
    Ok(0)
}

fn get_nat_nmask(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(u32::from(api.st.nmask[usize::from(c.u8(0)? & 7)]))
}

/// `void ChangeUnitColor(int index, byte nation)`: the object changes owner.
fn change_unit_color(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(1)?;
    let Some(id) = u16::try_from(c.i32(0)?).ok().and_then(|i| api.object(i)).map(|u| u.id) else { return Ok(0) };
    if n < 8 {
        crate::world_ops::set_owner(api.world, id, n);
        api.st.extra.entry(id.0).or_default().nmask = None;
    }
    Ok(0)
}

/// `void AddUnitsInZoneToGroup(GAMEOBJ* zone, GAMEOBJ* grp, byte nation, int usage)`: living
/// objects in the zone (of `nation`, or any for 0xFF) join the group.
fn add_units_in_zone_to_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(circles) = api.zone_arg(c, 0, "AddUnitsInZoneToGroup")? else { return Ok(0) };
    let Some(g) = api.group_arg(c, 1, "AddUnitsInZoneToGroup")? else { return Ok(0) };
    let n = c.u8(2)?;
    let mut add: Vec<Member> = Vec::new();
    for z in &circles {
        for u in api.world.units.iter().filter(|u| u.active() && (n == 0xFF || u.owner == n) && z.contains(u)) {
            let m = api.member_of(u.id);
            if !add.contains(&m) && !api.st.groups[g].members.iter().any(|x| x.id == m.id) {
                add.push(m);
            }
        }
    }
    api.st.groups[g].members.extend(add);
    Ok(0)
}

/// `void RemoveAttackingUnits(GAMEOBJ*)`: members inside buildings or attacking leave.
fn remove_attacking_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let members = std::mem::take(&mut api.st.groups[g].members);
    let kept = members
        .into_iter()
        .filter(|&m| api.by_serial_pub(m).is_none_or(|u| u.inside_of.is_none() && u.target().is_none()))
        .collect();
    api.st.groups[g].members = kept;
    Ok(0)
}

/// `bool CheckIfNotBusy(GAMEOBJ*)`: no living member has an order (the original also looks at
/// the brigade state and random thresholds by difficulty).
fn check_if_not_busy(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let ids = api.members(g);
    Ok(u32::from(!ids.is_empty() && ids.iter().all(|id| api.world.units[id.0 as usize].orders.is_empty())))
}

/// `int GetMaxAttRadius(int index)`: the largest weapon range in pixels, -1 for no object.
fn get_max_att_radius(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = u16::try_from(c.i32(0)?).ok().and_then(|i| api.object(i)) else { return Ok(u32::MAX) };
    let t = api.world.catalog.get(u.kind);
    Ok(t.attacks.iter().map(|a| a.radius.1).max().unwrap_or(0) as u32)
}

/// `bool CheckIfBuildingIsFired(int index)`: true when the building is gone (or burning).
fn check_if_building_is_fired(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(u32::from(u16::try_from(c.i32(0)?).ok().and_then(|i| api.object(i)).is_none_or(|u| !u.living())))
}

/// `int GetBuildingOposit(int index, byte nation)`: units inside as the defence value.
fn get_building_oposit(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(u16::try_from(c.i32(0)?).ok().and_then(|i| api.object(i)).map_or(0, |u| u.inside.len() as u32))
}

/// `bool CheckLandLocking(int x, int y, int w, int h)` (1/16 px): the rectangle of lock
/// cells is blocked.
fn check_land_locking(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (x, y, w, h) = (c.i32(0)? >> 4, c.i32(1)? >> 4, (c.i32(2)? >> 4) + 1, (c.i32(3)? >> 4) + 1);
    Ok(u32::from(api.world.map.land.check_bar(x, y, w, h)))
}

/// `bool GetZoneR(GAMEOBJ* zone, int* r)`.
fn get_zone_r(api: &mut Api, c: &mut Call) -> Result<u32> {
    if c.u32(0)? == 0 {
        return Ok(0);
    }
    let h = c.handle(0)?;
    let r = if h.magic == magic::ZONE {
        api.st
            .zone_groups
            .get(usize::from(h.index))
            .and_then(|z| z.first())
            .and_then(|&z| api.st.zones.get(z))
            .map(|z| z.r)
    } else if is_point(&h) {
        Some((h.magic & 0xFF_FFFF) as i32)
    } else {
        None
    };
    let Some(r) = r else { return Ok(0) };
    c.out_i32(1, r)?;
    Ok(1)
}

fn get_tcos(_: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(acr_sim::trig::cos(c.u8(0)?) as u32)
}

fn get_tsin(_: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(acr_sim::trig::sin(c.u8(0)?) as u32)
}

/// `char* GetMoneyString(int cost[6], char* buf)`: "100 Gold, 50 Wood" style.
fn get_money_string(api: &mut Api, c: &mut Call) -> Result<u32> {
    const NAMES: [&str; 6] = ["RES_WOOD", "RES_GOLD", "RES_STONE", "RES_FOOD", "RES_IRON", "RES_COAL"];
    let (cost, buf) = (c.u32(0)?, c.u32(1)?);
    let mut parts: Vec<Vec<u8>> = Vec::new();
    for (i, id) in NAMES.iter().enumerate() {
        let v = c.read_u32(cost + 4 * i as u32)? as i32;
        if v != 0 {
            let mut s = format!("{v} ").into_bytes();
            s.extend(api.text(id.as_bytes()));
            parts.push(s);
        }
    }
    let mut s = parts.join(&b", "[..]);
    s.push(0);
    c.raw.mem.write_bytes(buf, &s)?;
    Ok(buf)
}

/// `void ActivateDipDialog(char* page)`: the diplomacy dialog (intExplorer pages).
fn activate_dip_dialog(api: &mut Api, c: &mut Call) -> Result<u32> {
    let page = c.str(0)?;
    api.events.push(MissionEvent::Error { title: "Diplomacy".into(), text: format!("dialog {page} (not shown yet)") });
    Ok(0)
}

/// `void SendDipCommand(char* data, int size)`: queued like a network command and executed
/// with `PerformDipCommand` at the next tick on every machine.
fn send_dip_command(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (p, n) = (c.u32(0)?, c.u32(1)?.min(0x4b0));
    let mut data = vec![0; n as usize];
    c.raw.mem.read_bytes(p, &mut data)?;
    api.st.dip_commands.push(data);
    Ok(0)
}

fn get_last_full_way(_: &mut Api, c: &mut Call) -> Result<u32> {
    c.out_u32(0, 0)?;
    Ok(0)
}

// --- files (`RReset` & co. read the game's archives) -----------------------------------------

fn r_reset(api: &mut Api, c: &mut Call) -> Result<u32> {
    let path = acr_formats::normalize_path(&c.str(0)?);
    if !api.st.files.contains_key(&path) {
        return Ok(u32::MAX);
    }
    Ok(api.st.open_file(path))
}

fn g_open(api: &mut Api, c: &mut Call) -> Result<u32> {
    let path = acr_formats::normalize_path(&c.str(0)?);
    let mode = c.str(1)?;
    if mode.starts_with('w') {
        api.st.files.insert(path.clone(), Vec::new());
    } else if !api.st.files.contains_key(&path) {
        return Ok(0);
    }
    Ok(api.st.open_file(path))
}

fn r_rewrite(api: &mut Api, c: &mut Call) -> Result<u32> {
    let path = acr_formats::normalize_path(&c.str(0)?);
    api.st.files.insert(path.clone(), Vec::new());
    Ok(api.st.open_file(path))
}

fn r_close(api: &mut Api, c: &mut Call) -> Result<u32> {
    api.st.open_files.remove(&c.u32(0)?);
    Ok(0)
}

fn r_file_size(api: &mut Api, c: &mut Call) -> Result<u32> {
    let h = c.u32(0)?;
    Ok(api.st.open_files.get(&h).and_then(|(p, _)| api.st.files.get(p)).map_or(0, |f| f.len() as u32))
}

fn r_block_read(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (h, buf, n) = (c.u32(0)?, c.u32(1)?, c.u32(2)?);
    let Some((path, pos)) = api.st.open_files.get(&h).cloned() else { return Ok(0) };
    let data = api.st.files.get(&path).map(Vec::as_slice).unwrap_or_default();
    let end = (pos + n as usize).min(data.len());
    let chunk = data[pos.min(end)..end].to_vec();
    c.raw.mem.write_bytes(buf, &chunk)?;
    api.st.open_files.insert(h, (path, end));
    Ok(chunk.len() as u32)
}

fn r_block_write(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (h, buf, n) = (c.u32(0)?, c.u32(1)?, c.u32(2)?);
    let Some((path, pos)) = api.st.open_files.get(&h).cloned() else { return Ok(0) };
    let mut chunk = vec![0; n as usize];
    c.raw.mem.read_bytes(buf, &mut chunk)?;
    let f = api.st.files.entry(path.clone()).or_default();
    f.truncate(pos);
    f.extend_from_slice(&chunk);
    api.st.open_files.insert(h, (path, pos + chunk.len()));
    Ok(n)
}

/// `int Ggetch(GFILE*)`: the next byte, -1 at the end.
fn g_getch(api: &mut Api, c: &mut Call) -> Result<u32> {
    let h = c.u32(0)?;
    let Some((path, pos)) = api.st.open_files.get(&h).cloned() else { return Ok(u32::MAX) };
    let b = api.st.files.get(&path).and_then(|f| f.get(pos)).copied();
    match b {
        Some(b) => {
            api.st.open_files.insert(h, (path, pos + 1));
            Ok(u32::from(b))
        }
        None => Ok(u32::MAX),
    }
}

// --- tactics (approximations) ----------------------------------------------------------------

/// Danger-map variants for DipServer's squads: the plain danger map of [`super::tactics`].
fn danger_map(api: &mut Api, c: &mut Call) -> Result<u32> {
    super::tactics::danger_map_pub(api, c)
}

/// `void AttackEnemyPeasantsInZone(GAMEOBJ* grp, GAMEOBJ* zone, byte nation)`: idle members
/// attack the nearest enemy peasant in the zone.
fn attack_enemy_peasants_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let Some(z) =
        (if c.u32(1)? == 0 { None } else { api.zone_circles(&c.handle(1)?) }).and_then(|z| z.first().copied())
    else {
        return Ok(0);
    };
    let mask = api.st.nmask[usize::from(c.u8(2)? & 7)];
    let targets: Vec<EntityId> = api
        .world
        .units
        .iter()
        .filter(|u| {
            u.active()
                && api.world.catalog.get(u.kind).peasant
                && api.st.unit_nmask(u.id, u.owner) & mask == 0
                && z.contains(u)
        })
        .map(|u| u.id)
        .collect();
    attack_nearest(api, g, &targets);
    Ok(0)
}

/// Idle members attack the nearest target.
fn attack_nearest(api: &mut Api, g: usize, targets: &[EntityId]) {
    for id in api.members(g) {
        let u = &api.world.units[id.0 as usize];
        if !u.orders.is_empty() {
            continue;
        }
        let best = targets.iter().min_by_key(|t| {
            let e = &api.world.units[t.0 as usize];
            norma(u.pos.x - e.pos.x, u.pos.y - e.pos.y)
        });
        if let Some(&target) = best {
            let owner = u.owner;
            api.commands.push(Command::Attack { player: owner, units: vec![id], target });
        }
    }
}

/// `ArtAttackObject(GAMEOBJ* guns, int target, ...)`.
fn art_attack_object(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let Some(t) = u16::try_from(c.i32(1)?).ok().and_then(|i| api.object(i)).filter(|u| u.living()).map(|u| u.id) else {
        return Ok(0);
    };
    attack_nearest(api, g, &[t]);
    Ok(1)
}

/// `ArtAttackGroud(GAMEOBJ* guns, int x, int y, ...)`: guns move to the point (no ground fire
/// in acr-sim yet).
fn art_attack_ground(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let ids = api.members(g);
    if let Some(&first) = ids.first() {
        let owner = api.world.units[first.0 as usize].owner;
        api.commands.push(Command::Move { player: owner, units: ids, x: c.i32(1)?, y: c.i32(2)? });
    }
    Ok(1)
}

/// `void OBJ_SmartSendTo(word index, int x, int y, int dx, int dy, byte prio, byte type)`.
fn obj_smart_send_to(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = api.object(c.u16(0)?).filter(|u| u.living()) else { return Ok(0) };
    let (id, owner) = (u.id, u.owner);
    let (x, y) = (c.i32(1)? + c.i32(3)?, c.i32(2)? + c.i32(4)?);
    api.commands.push(Command::Move { player: owner, units: vec![id], x, y });
    Ok(0)
}

/// `int GetAlliesForce(byte nation, GAMEOBJ* zone)`: allied units in the zone.
fn get_allies_force(api: &mut Api, c: &mut Call) -> Result<u32> {
    let mask = api.st.nmask[usize::from(c.u8(0)? & 7)];
    let Some(z) =
        (if c.u32(1)? == 0 { None } else { api.zone_circles(&c.handle(1)?) }).and_then(|z| z.first().copied())
    else {
        return Ok(0);
    };
    Ok(api
        .world
        .units
        .iter()
        .filter(|u| u.active() && !u.building && api.st.unit_nmask(u.id, u.owner) & mask != 0 && z.contains(u))
        .count() as u32)
}

/// Top lists: `u16 list[NZones]` with an object index per area (0xFFFF = none). ACR lists the
/// enemy buildings of `nation` (first argument) per area.
fn create_enemy_buildings_top_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let out = c.u32(0)?;
    let nat = c.u8(1)? & 7;
    top_list(api, c, out, |api, u| u.building && api.st.unit_nmask(u.id, u.owner) & api.st.nmask[usize::from(nat)] == 0)
}

fn create_peasants_top_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let out = c.u32(0)?;
    let nat = c.u8(1)? & 7;
    top_list(api, c, out, |api, u| {
        api.world.catalog.get(u.kind).peasant && api.st.unit_nmask(u.id, u.owner) & api.st.nmask[usize::from(nat)] == 0
    })
}

fn top_list(api: &mut Api, c: &mut Call, out: u32, pick: impl Fn(&Api, &acr_sim::Unit) -> bool) -> Result<u32> {
    let n = api.st.topo.as_ref().map_or(0, |t| t.areas.len());
    let mut list = vec![NO_UNIT; n];
    for u in api.world.units.iter().filter(|u| u.active() && pick(api, u)) {
        let z = super::tactics::zone_of(api, u.pos.x >> 10, u.pos.y >> 10);
        if let Some(slot) = list.get_mut(usize::from(z))
            && *slot == NO_UNIT
        {
            *slot = u.id.0 as u16;
        }
    }
    for (i, v) in list.iter().enumerate() {
        c.write_u16(out + 2 * i as u32, *v)?;
    }
    Ok(0)
}

impl Api<'_> {
    /// The object in a member slot if the serial matches (any state).
    pub fn by_serial_pub(&self, m: Member) -> Option<&acr_sim::Unit> {
        let u = self.object(m.id)?;
        (self.st.serial(u.id) == m.serial).then_some(u)
    }
}
