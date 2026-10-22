//! Resources, production, upgrades and unit availability, collectors and the AI's economy
//! helpers.
//!
//! Resource ids (NRES.DAT `[RESTYPES]`): 0 wood, 1 gold, 2 stone, 3 food, 4 iron, 5 coal; they
//! index [`acr_sim::world::Player::resources`] (the map's `RESU` order).

use acr_script::runtime::Result;

use super::call::magic;
use super::{Api, ApiFn, Call, Caller, Status};
use crate::state::Production;

const fn f(name: &'static str, original: u32, status: Status, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status, handler }
}

const DONE: Status = Status::Done;
const AI_CITY: Status = Status::Stub("the AI city (projects, statistics) is not modelled yet: 0");
const NO_GATHER: Status = Status::Stub("no gathering in acr-sim yet");

pub const FUNCTIONS: &[ApiFn] = &[
    f("GetResource", 0x562d7a, DONE, get_resource),
    f("AddResource", 0x562dea, DONE, add_resource),
    f("SetResource", 0x562eca, DONE, set_resource),
    f("TakeFood", 0x5664dd, NO_GATHER, check_group),
    f("TakeWood", 0x566637, NO_GATHER, check_group),
    f("GetMoney", 0x568f03, DONE, get_money),
    f("GetExtraction", 0x5699eb, AI_CITY, zero),
    f("IsResourceEnabled", 0x577225, Status::Stub("no resource map in acr-sim yet: 0"), zero),
    f("SetToInternalResourcesZero", 0x56eba8, Status::Stub("unit stance/carried resources not modelled"), zero),
    f(
        "ProduceUnit",
        0x566dd2,
        Status::Partial("ACR's stand-in production: the unit appears next to the building after 100 ticks"),
        produce_unit,
    ),
    f(
        "ProduceUnitFast",
        0x566f36,
        Status::Partial("stand-in production after 15·(Speed+1) ticks (the original's 30·FAST stages)"),
        produce_unit_fast,
    ),
    f("CheckProduction", 0x5672cc, DONE, check_production),
    f("TryUnit", 0x56829b, AI_CITY, zero),
    f("TryUpgrade", 0x5684d6, AI_CITY, zero),
    f("TryUpgradeEx", 0x568629, AI_CITY, zero),
    f("UpgIsDone", 0x569259, Status::Partial("upgrades applied by InitialUpgrade only"), upg_is_done),
    f("UpgIsRun", 0x5692a9, Status::Partial("as UpgIsDone"), upg_is_done),
    f(
        "InitialUpgrade",
        0x55c02f,
        Status::Partial("marked done for the nation; effects need acr-sim upgrades"),
        initial_upgrade,
    ),
    f("InitialUpgradeNew", 0x55c202, Status::Partial("as InitialUpgrade"), initial_upgrade_new),
    f(
        "DisableUpgrade",
        0x55c335,
        Status::Partial("flag kept for the build menu (no upgrades in acr-sim)"),
        disable_upgrade,
    ),
    f("SetUpgradeEnableStatus", 0x55c437, Status::Partial("flag kept for the build menu"), set_upgrade_enable_status),
    f("EnableUnit", 0x55c5bb, Status::Partial("flag kept for the build menu (no production UI yet)"), enable_unit),
    f("SetUnitEnableState", 0x55c4f9, Status::Partial("flag kept for the build menu"), set_unit_enable_state),
    f("GetUpgradeCost", 0x56304a, Status::Stub("no upgrade costs in the catalog yet: 0"), zero),
    f("GetUnitCost", 0x562f80, Status::Stub("no unit costs in the catalog yet: 0"), zero),
    f(
        "AI_Torg",
        0x569083,
        Status::Partial("market of ECONOMY.TXT (patch values); no drift back to nominal yet"),
        ai_torg,
    ),
    f("GetTorgResult", 0x5690c0, Status::Partial("as AI_Torg"), get_torg_result),
    f(
        "SetBuildingsCollector",
        0x577c6f,
        Status::Partial("current buildings only; new ones are not added yet"),
        set_buildings_collector,
    ),
    f("SetUnitsCollector", 0x577d70, Status::Partial("current units only"), set_units_collector),
    f(
        "SetEnemyBuildingsCollector",
        0x577e71,
        Status::Partial("current enemy buildings only"),
        set_enemy_buildings_collector,
    ),
    f("SGP_RepairBuilding", 0x577b9c, Status::Stub("no repairs in acr-sim yet"), zero),
    f("SGP_TakeResources", 0x577be1, NO_GATHER, zero),
    f("CreateFarmList", 0x57c20d, Status::Stub("no fields: the list stays empty"), create_farm_list),
    f("CreateMeatList", 0x57bff5, Status::Stub("no hunting: the list stays empty"), create_farm_list),
    f("MakeOneShotToBuild", 0x573da9, Status::Stub("no artillery fire in acr-sim yet"), zero),
    f("GetUnits", 0x568f4d, DONE, get_units),
    f("GetReadyUnits", 0x5690f0, DONE, get_units),
];

fn zero(_: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(0)
}

fn check_group(api: &mut Api, c: &mut Call) -> Result<u32> {
    let _ = api.group_arg(c, 0, c.name())?;
    Ok(0)
}

/// A resource slot of a nation, `None` (and an error) for bad arguments.
fn slot(api: &mut Api, c: &Call, what: &str) -> Result<Option<(usize, usize)>> {
    let n = api.assign(c.u8(0)?);
    let r = c.u8(1)?;
    if n >= 8 || r >= 6 {
        api.st.error(format!("{what} : Invalid parameters"));
        return Ok(None);
    }
    Ok(Some((usize::from(n), usize::from(r))))
}

fn res(api: &Api, n: usize, r: usize) -> i32 {
    api.world.players[n].resources[r] as i32
}

/// `int GetResource(byte nation, byte res)`.
fn get_resource(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(slot(api, c, "GetResource")?.map_or(0, |(n, r)| res(api, n, r) as u32))
}

/// `void AddResource(byte nation, byte res, int amount)`: clamped at 0, no upper limit.
fn add_resource(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some((n, r)) = slot(api, c, "AddResource")? {
        let v = res(api, n, r).wrapping_add(c.i32(2)?).max(0);
        api.world.players[n].resources[r] = v as u32;
    }
    Ok(0)
}

/// `void SetResource(byte nation, byte res, int amount)`.
fn set_resource(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some((n, r)) = slot(api, c, "SetResource")? {
        api.world.players[n].resources[r] = c.i32(2)?.max(0) as u32;
    }
    Ok(0)
}

/// `int GetMoney(byte res)`: the AI nation's resource.
fn get_money(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else {
        api.st.error("ERROR: AI function must be called only from AI section.");
        return Ok(0);
    };
    let r = usize::from(c.u8(0)?);
    Ok(if r < 6 { res(api, usize::from(n), r) as u32 } else { 0 })
}

/// Queues a unit in every idle (or any, when `fast`) living building of the group.
fn produce(api: &mut Api, c: &mut Call, fast: Option<i32>) -> Result<u32> {
    const WHAT: &str = "ProduceUnit(Units,UnitType,Dest)";
    let Some(b) = api.group_arg(c, 0, WHAT)? else { return Ok(0) };
    let Some(d) = api.group_arg(c, 2, WHAT)? else { return Ok(0) };
    let Some(t) = api.type_arg(c, 1, WHAT)? else { return Ok(0) };
    let delay = match fast {
        Some(speed) => 15 * (speed.min(6) + 1).max(1) as u64,
        None => 100,
    };
    let tick = api.world.tick.0;
    for m in api.st.groups[b].members.clone() {
        let Some(u) = api.member_loose(m).filter(|u| u.building) else { continue };
        let id = u.id;
        let busy = api.st.production.iter().any(|p| p.building == id);
        if fast.is_none() && busy {
            continue;
        }
        api.st.production.push(Production { building: id, kind: t, group: d, ready: tick + delay });
    }
    Ok(0)
}

fn produce_unit(api: &mut Api, c: &mut Call) -> Result<u32> {
    produce(api, c, None)
}

fn produce_unit_fast(api: &mut Api, c: &mut Call) -> Result<u32> {
    let speed = c.i32(3)?;
    produce(api, c, Some(speed))
}

/// `bool CheckProduction(GAMEOBJ* buildings)`: an idle finished building exists.
fn check_production(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(b) = api.group_arg(c, 0, "CheckProduction : Invalid <Units>")? else { return Ok(0) };
    let idle = api
        .members(b)
        .into_iter()
        .any(|id| api.world.units[id.0 as usize].building && !api.st.production.iter().any(|p| p.building == id));
    Ok(u32::from(idle))
}

fn upgrade_index(c: &Call, i: u32) -> Result<Option<u16>> {
    if c.u32(i)? == 0 {
        return Ok(None);
    }
    let h = c.handle(i)?;
    Ok((h.magic == magic::UPGRADE).then_some(h.index))
}

fn upg_is_done(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    let Some(u) = upgrade_index(c, 0)? else {
        api.st.error("UpgIsDone : Invalid Upgrade ID");
        return Ok(0);
    };
    Ok(u32::from(api.st.upgrades_done.contains(&(n, u))))
}

/// `void InitialUpgrade(char* mapGroup, char* upgrade)`: for the nation of the group's first
/// living unit.
fn initial_upgrade(api: &mut Api, c: &mut Call) -> Result<u32> {
    let group = c.str(0)?;
    let Some(mg) = api.st.map_groups.iter().find(|g| g.name == group) else {
        api.st.error("InitialUpgrade : Unknown <Group>");
        return Ok(0);
    };
    let members = mg.members.clone();
    let name = c.str(1)?;
    let u = match api.st.upgrades.iter().position(|x| *x == name) {
        Some(i) => i,
        None => {
            api.st.upgrades.push(name);
            api.st.upgrades.len() - 1
        }
    } as u16;
    if let Some(n) = members.iter().find_map(|&m| api.member(m)).map(|u| u.owner) {
        api.st.upgrades_done.insert((n, u));
    }
    Ok(0)
}

/// `void InitialUpgradeNew(byte nation, GAMEOBJ* upgrade, word unit)`.
fn initial_upgrade_new(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = upgrade_index(c, 1)? else { return Ok(0) };
    api.st.upgrades_done.insert((c.u8(0)?, u));
    Ok(0)
}

/// Flags of the build menus: `(nation, kind, index) -> state` (+1 enable, −1 disable).
fn set_flag(api: &mut Api, n: u8, upgrade: bool, index: u16, state: i8) {
    let key = (n, upgrade, index);
    if state == 0 {
        api.st.availability.remove(&key);
    } else {
        api.st.availability.insert(key, state);
    }
}

fn upgrade_flag(api: &mut Api, c: &mut Call, state: Option<i32>) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    if n >= 8 {
        return Ok(0);
    }
    let Some(u) = upgrade_index(c, 1)? else {
        api.st.error("DisableUpgrade : Incorrect parameter <Upgrade>");
        return Ok(0);
    };
    let s = state.map_or(-1, |s| s.signum()) as i8;
    set_flag(api, n, true, u, s);
    Ok(0)
}

fn disable_upgrade(api: &mut Api, c: &mut Call) -> Result<u32> {
    upgrade_flag(api, c, None)
}

fn set_upgrade_enable_status(api: &mut Api, c: &mut Call) -> Result<u32> {
    let s = c.i32(2)?;
    upgrade_flag(api, c, Some(s))
}

fn unit_flag(api: &mut Api, c: &mut Call, state: i32) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    if n >= 8 {
        return Ok(0);
    }
    let Some(t) = api.type_arg(c, 1, "EnableUnit")? else { return Ok(0) };
    set_flag(api, n, false, t.0, state.signum() as i8);
    Ok(0)
}

/// `void EnableUnit(byte nation, GAMEOBJ* type, bool state)`.
fn enable_unit(api: &mut Api, c: &mut Call) -> Result<u32> {
    let s = if c.bool(2)? { 0 } else { -1 };
    unit_flag(api, c, s)
}

fn set_unit_enable_state(api: &mut Api, c: &mut Call) -> Result<u32> {
    let s = c.i32(2)?;
    unit_flag(api, c, s)
}

/// Fills a collector group with the matching current objects.
fn collector(api: &mut Api, c: &mut Call, pick: impl Fn(&Api, &acr_sim::Unit, u8) -> bool) -> Result<u32> {
    let n = c.u8(0)?;
    if n >= 8 {
        return Ok(0);
    }
    let h = c.handle(1)?;
    if h.magic != magic::GROUP || usize::from(h.index) >= api.st.groups.len() {
        return Ok(0);
    }
    let add: Vec<_> =
        api.world.units.iter().filter(|u| u.living() && pick(api, u, n)).map(|u| api.member_of(u.id)).collect();
    let g = &mut api.st.groups[usize::from(h.index)].members;
    for m in add {
        if !g.iter().any(|x| x.id == m.id) {
            g.push(m);
        }
    }
    Ok(0)
}

fn set_buildings_collector(api: &mut Api, c: &mut Call) -> Result<u32> {
    collector(api, c, |_, u, n| u.building && u.owner == n)
}

fn set_units_collector(api: &mut Api, c: &mut Call) -> Result<u32> {
    collector(api, c, |_, u, n| !u.building && u.owner == n)
}

fn set_enemy_buildings_collector(api: &mut Api, c: &mut Call) -> Result<u32> {
    collector(api, c, |api, u, n| u.building && api.st.unit_nmask(u.id, u.owner) & api.st.nmask[usize::from(n)] == 0)
}

/// `void CreateFarmList(word* out, byte nation)`: `out[NAreas]` = 0xFFFF (no fields yet).
fn create_farm_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let out = c.u32(0)?;
    let n = api.st.topo.as_ref().map_or(0, |t| t.areas.len());
    for i in 0..n {
        c.write_u16(out + 2 * i as u32, 0xFFFF)?;
    }
    Ok(0)
}

/// Resource id -> economy index (`INVECO`, 0x67169c).
const INVECO: [usize; 6] = [2, 5, 4, 0, 1, 3];
/// Economy index -> resource id (`ECO.EcResID`).
const EC_RES: [usize; 6] = [3, 4, 0, 5, 2, 1];

/// `int GetTorgResult(byte sell, byte buy, int amount)`: `trunc(amount * Sell[s] / Buy[b])`.
fn get_torg_result(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (s, b) = (usize::from(c.u8(0)?), usize::from(c.u8(1)?));
    if s >= 6 || b >= 6 {
        return Ok(0);
    }
    let m = &api.st.market;
    Ok((f64::from(c.i32(2)?) * m.sell[INVECO[s]] / m.buy[INVECO[b]]) as i32 as u32)
}

/// `void AI_Torg(byte sell, byte buy, int amount)`: `ECO.PerformOperation` (0x4a9d80) for
/// the AI nation; the prices are shared by all nations.
fn ai_torg(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    let (s, b) = (usize::from(c.u8(0)?), usize::from(c.u8(1)?));
    if s >= 6 || b >= 6 {
        return Ok(0);
    }
    let (es, eb) = (INVECO[s], INVECO[b]);
    let (sr, br) = (EC_RES[es], EC_RES[eb]);
    let n = usize::from(n);
    let mut a = c.i32(2)?;
    a = a.min(res(api, n, sr));
    let m = &mut api.st.market;
    let got = (f64::from(a) * m.sell[es] / m.buy[eb]) as i32;
    let p = &mut api.world.players[n].resources;
    p[sr] = (p[sr] as i32).wrapping_sub(a) as u32;
    p[br] = (p[br] as i32).wrapping_add(got) as u32;
    let t = f64::from(got) * m.buy_change[eb];
    m.buy[eb] = (m.buy[eb] + t * m.buy_range[eb][2]) / (1.0 + t);
    m.sell[eb] = (m.sell[eb] + t * m.sell_range[eb][2]) / (1.0 + t);
    let t = f64::from(a) * m.sell_change[es];
    m.sell[es] = (m.sell[es] + t * m.sell_range[es][0]) / (1.0 + t);
    m.buy[es] = (m.buy[es] + t * m.buy_range[es][0]) / (1.0 + t);
    Ok(0)
}

/// `int GetUnits(GAMEOBJ* type)` / `GetReadyUnits`: objects of the type the AI nation owns.
fn get_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    let Some(t) = api.type_arg(c, 0, c.name())? else { return Ok(0) };
    Ok(api.world.units.iter().filter(|u| u.living() && u.owner == n && u.kind == t).count() as u32)
}

/// Delivers finished stand-in productions: the unit appears below the building and joins the
/// destination group.
pub fn run_production(st: &mut crate::state::ScriptState, world: &mut acr_sim::World) {
    let now = world.tick.0;
    let mut i = 0;
    while i < st.production.len() {
        let p = st.production[i].clone();
        if p.ready > now {
            i += 1;
            continue;
        }
        st.production.remove(i);
        let Some(b) = world.units.get(p.building.0 as usize).filter(|u| u.living()) else { continue };
        let (owner, pos) = (b.owner, b.pos);
        let r = world.catalog.get(b.kind).radius2.max(64) << 4;
        let spot = acr_sim::Pos::new(pos.x, pos.y + r);
        if let Some(id) = crate::world_ops::create(world, p.kind, owner, spot)
            && p.group < st.groups.len()
        {
            let serial = st.serial(id);
            st.groups[p.group].members.push(crate::state::Member { id: id.0 as u16, serial });
        }
    }
}
