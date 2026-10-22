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
    f("TakeFood", 0x5664dd, DONE, take_food),
    f("TakeWood", 0x566637, DONE, take_wood),
    f("GetMoney", 0x568f03, DONE, get_money),
    f("GetExtraction", 0x5699eb, AI_CITY, zero),
    f("IsResourceEnabled", 0x577225, Status::Stub("no resource map in acr-sim yet: 0"), zero),
    f("SetToInternalResourcesZero", 0x56eba8, Status::Stub("unit stance/carried resources not modelled"), zero),
    f("ProduceUnit", 0x566dd2, DONE, produce_unit),
    f("ProduceUnitFast", 0x566f36, DONE, produce_unit_fast),
    f("CheckProduction", 0x5672cc, DONE, check_production),
    f("TryUnit", 0x56829b, Status::Partial("AI city without tech conditions; see aicity.rs"), try_unit),
    f("TryUpgrade", 0x5684d6, AI_CITY, zero),
    f("TryUpgradeEx", 0x568629, AI_CITY, zero),
    f("UpgIsDone", 0x569259, DONE, upg_is_done),
    f("UpgIsRun", 0x5692a9, DONE, upg_is_done),
    f(
        "InitialUpgrade",
        0x55c02f,
        Status::Partial("marked done for the nation; effects need acr-sim upgrades"),
        initial_upgrade,
    ),
    f("InitialUpgradeNew", 0x55c202, Status::Partial("as InitialUpgrade"), initial_upgrade_new),
    f("DisableUpgrade", 0x55c335, DONE, disable_upgrade),
    f("SetUpgradeEnableStatus", 0x55c437, DONE, set_upgrade_enable_status),
    f("EnableUnit", 0x55c5bb, DONE, enable_unit),
    f("SetUnitEnableState", 0x55c4f9, DONE, set_unit_enable_state),
    f("GetUpgradeCost", 0x56304a, DONE, get_upgrade_cost),
    f("GetUnitCost", 0x562f80, DONE, get_unit_cost),
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

/// `TakeFood`/`TakeWood`: idle peasants of the group gather `kind` around themselves
/// (`TakeResource(own position, kind, 0x80)`, no owner test).
fn take(api: &mut Api, c: &mut Call, kind: u8) -> Result<u32> {
    let what = if kind == 0 { "TakeWood(Units)" } else { "TakeFood(Units)" };
    let Some(g) = api.group_arg(c, 0, what)? else { return Ok(0) };
    for id in api.members_h(g) {
        let u = &api.world.units[id.0 as usize];
        if u.orders.is_empty() && api.world.catalog.get(u.kind).peasant {
            let (x, y) = u.pos.pixels();
            acr_sim::gather::take_resource(api.world, id.0 as usize, x, y, kind, 0x80, false);
        }
    }
    Ok(0)
}

fn take_food(api: &mut Api, c: &mut Call) -> Result<u32> {
    take(api, c, acr_sim::economy::FOOD as u8)
}

fn take_wood(api: &mut Api, c: &mut Call) -> Result<u32> {
    take(api, c, acr_sim::economy::WOOD as u8)
}

/// `int GetUpgradeCost(byte nation, GAMEOBJ* upgrade, byte res)`: `UPGRADE[u]->Cost[res]`.
fn get_upgrade_cost(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    if n >= 8 {
        api.st.error("GetUpgradeCost(Nat,Upgrade,ResID) : Invalid <Nat>");
        return Ok(0);
    }
    let Some(u) = upgrade_index(c, 1)? else {
        api.st.error("GetUpgradeCost(Nat,Upgrade,ResID) : Invalid <Upgrade>");
        return Ok(0);
    };
    let r = usize::from(c.u8(2)?);
    if r >= 6 {
        api.st.error("GetUpgradeCost(Nat,Upgrade,ResID) : Invalid <ResID>");
        return Ok(0);
    }
    let tech = &api.world.catalog.tech;
    if !tech.loaded || usize::from(u) >= tech.upgrades.len() {
        return Ok(0);
    }
    Ok(u32::from(tech.upgrade(acr_sim::tech::UpgradeId(u)).cost[r] as u16))
}

/// `int GetUnitCost(byte nation, GAMEOBJ* type, byte res)`: the current price.
fn get_unit_cost(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = api.assign(c.u8(0)?);
    if n >= 8 {
        api.st.error("GetUnitCost(Nat,UnitType,ResID) : Invalid <Nat>");
        return Ok(0);
    }
    let Some(t) = api.type_arg(c, 1, "GetUnitCost(Nat,UnitType,ResID)")? else { return Ok(0) };
    let r = usize::from(c.u8(2)?);
    if r >= 6 {
        api.st.error("GetUnitCost(Nat,UnitType,ResID) : Invalid <ResID>");
        return Ok(0);
    }
    Ok(acr_sim::economy::unit_cost(api.world, n, t, None)[r] as u32)
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
    api.world.players[n].resources[r]
}

/// `int GetResource(byte nation, byte res)`.
fn get_resource(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(slot(api, c, "GetResource")?.map_or(0, |(n, r)| res(api, n, r) as u32))
}

/// `void AddResource(byte nation, byte res, int amount)`: clamped at 0, no upper limit.
fn add_resource(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some((n, r)) = slot(api, c, "AddResource")? {
        let v = res(api, n, r).wrapping_add(c.i32(2)?).max(0);
        api.world.players[n].resources[r] = v;
    }
    Ok(0)
}

/// `void SetResource(byte nation, byte res, int amount)`.
fn set_resource(api: &mut Api, c: &mut Call) -> Result<u32> {
    if let Some((n, r)) = slot(api, c, "SetResource")? {
        api.world.players[n].resources[r] = c.i32(2)?.max(0);
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

/// `ProduceUnit`/`ProduceUnitFast`: `OneObject::Produce(type, destGroup)` (0x49e649) on every
/// idle (or, fast, every) finished building of the group. With a script group as destination
/// the unit is free and joins the group when it is born; "fast" starts the order with only
/// `30·(Speed+1)` stages left.
fn produce(api: &mut Api, c: &mut Call, fast: Option<i32>) -> Result<u32> {
    const WHAT: &str = "ProduceUnit(Units,UnitType,Dest)";
    let Some(b) = api.group_arg(c, 0, WHAT)? else { return Ok(0) };
    let Some(d) = api.group_arg(c, 2, WHAT)? else { return Ok(0) };
    let Some(t) = api.type_arg(c, 1, WHAT)? else { return Ok(0) };
    let stages = api.world.catalog.get(t).economy.build_stages;
    let progress = match fast {
        Some(speed) => (stages - 30 * (speed.min(6) + 1)).max(0),
        None => 0,
    };
    for m in api.st.groups[b].members.clone() {
        let Some(u) = api.member_loose(m) else { continue };
        let (bi, ready) = (u.id.0 as usize, u.stage >= api.world.catalog.get(u.kind).economy.build_stages);
        if !u.building || !ready || (fast.is_none() && !u.queue.is_empty()) {
            continue;
        }
        let power = api.world.players[usize::from(u.owner)].type_count.get(usize::from(t.0)).copied().unwrap_or(0);
        api.world.units[bi].queue.push(acr_sim::economy::Production { kind: t, progress, stages, power });
        api.st.production.push(Production { building: api.world.units[bi].id, kind: t, group: d, ready: 0 });
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
    let idle = api.members(b).into_iter().any(|id| {
        let u = &api.world.units[id.0 as usize];
        u.building && u.queue.is_empty() && u.stage >= api.world.catalog.get(u.kind).economy.build_stages
    });
    Ok(u32::from(idle))
}

fn upgrade_index(c: &Call, i: u32) -> Result<Option<u16>> {
    if c.u32(i)? == 0 {
        return Ok(None);
    }
    let h = c.handle(i)?;
    Ok((h.magic == magic::UPGRADE).then_some(h.index))
}

/// `bool UpgIsDone(GAMEOBJ*)` / `UpgIsRun` (done or being researched), for the AI nation.
fn upg_is_done(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    let Some(u) = upgrade_index(c, 0)? else {
        api.st.error("UpgIsDone : Invalid Upgrade ID");
        return Ok(0);
    };
    let run = c.name() == "UpgIsRun";
    Ok(api.upgrade_state(n, u).map_or(0, |s| u32::from(s.done || (run && s.doing))))
}

/// Marks an upgrade done for a nation like `InitialUpgrade` (0x55c02f): done, and no longer
/// offered unless it is individual or a stage. TODO(acr-sim): apply the upgrade's effect
/// (`PerformNewUpgrade` 0x532281) once acr-sim performs upgrades.
fn initial(api: &mut Api, n: u8, u: u16) {
    let def =
        api.world.catalog.tech.loaded.then(|| api.world.catalog.tech.upgrade(acr_sim::tech::UpgradeId(u)).clone());
    if let Some(s) = api.upgrade_state(n, u)
        && !def.as_ref().is_some_and(|d| d.individual || d.stage_up)
    {
        s.done = true;
        s.permanent = false;
        s.enabled = false;
    }
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
    let Some(u) = api.upgrade_named(&name) else {
        api.st.error(format!("RegisterUpgrade : Unknown upgrade : {name}"));
        return Ok(0);
    };
    if let Some(n) = members.iter().find_map(|&m| api.member(m)).map(|u| u.owner) {
        initial(api, n, u);
    }
    Ok(0)
}

/// `void InitialUpgradeNew(byte nation, GAMEOBJ* upgrade, word unit)`.
fn initial_upgrade_new(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = upgrade_index(c, 1)? else { return Ok(0) };
    initial(api, c.u8(0)? & 7, u);
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
    crate::aicity::apply_availability(api.st, api.world);
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
    // `EnableUnit(true)` also sets `Enabled` at once (unmet tech conditions turn it off again).
    if state == 0
        && c.name() == "EnableUnit"
        && let Some(e) = api.world.players[usize::from(n)].enabled.get_mut(usize::from(t.0))
    {
        *e = true;
    }
    crate::aicity::apply_availability(api.st, api.world);
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
    p[sr] = p[sr].wrapping_sub(a);
    p[br] = p[br].wrapping_add(got);
    let t = f64::from(got) * m.buy_change[eb];
    m.buy[eb] = (m.buy[eb] + t * m.buy_range[eb][2]) / (1.0 + t);
    m.sell[eb] = (m.sell[eb] + t * m.sell_range[eb][2]) / (1.0 + t);
    let t = f64::from(a) * m.sell_change[es];
    m.sell[es] = (m.sell[es] + t * m.sell_range[es][0]) / (1.0 + t);
    m.buy[es] = (m.buy[es] + t * m.buy_range[es][0]) / (1.0 + t);
    Ok(0)
}

/// `bool TryUnit(GAMEOBJ* type, int max, byte costPercent, byte probability)`: proposes a
/// project of the AI city when fewer than `max` objects of the type exist, the city can
/// produce it now and, with the given probability, it costs at most `costPercent` of the
/// stock.
fn try_unit(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else {
        api.st.error("ERROR: AI function must be called only from AI section.");
        return Ok(0);
    };
    let Some(t) = api.type_arg(c, 0, "TryUnit : invalid <UnitType>")? else { return Ok(0) };
    let (max, percent, prob) = (c.i32(1)?, i32::from(c.u8(2)?), i32::from(c.u8(3)?));
    if prob > 100 {
        api.st.error("TryUnit : invalid <Probability>");
        return Ok(0);
    }
    let city = &api.st.cities[usize::from(n)];
    if city.best.len() >= 127 {
        return Ok(0);
    }
    let count = api.world.units.iter().filter(|u| u.living() && u.owner == n && u.kind == t).count() as i32;
    let Some(&k) = city.index.get(&t).filter(|_| max > count) else { return Ok(0) };
    let need = city.props[k].percent;
    if api.world.random.rando() < crate::aicity::perconv(prob) && percent >= need {
        api.st.cities[usize::from(n)].best.push(k);
        return Ok(1);
    }
    Ok(0)
}

/// `int GetUnits(GAMEOBJ* type)` / `GetReadyUnits`: objects of the type the AI nation owns
/// (`UnitAmount`), or only the finished ones (`ReadyAmount`).
fn get_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    let Some(t) = api.type_arg(c, 0, c.name())? else { return Ok(0) };
    let ready_only = c.name() == "GetReadyUnits";
    let stages = api.world.catalog.get(t).economy.build_stages;
    Ok(api
        .world
        .units
        .iter()
        .filter(|u| u.living() && u.owner == n && u.kind == t && (!ready_only || !u.building || u.stage >= stages))
        .count() as u32)
}

/// Puts units born from script productions into their destination groups
/// (`ProduceObjLink` 0x49f6a0: the new unit joins the group and takes its StartNMask).
/// Called before every tick with the units created since the last call.
pub fn run_production(st: &mut crate::state::ScriptState, world: &mut acr_sim::World) {
    let first = st.seen_units.min(world.units.len());
    st.seen_units = world.units.len();
    for i in first..world.units.len() {
        let u = &world.units[i];
        if u.building || !u.alive {
            continue;
        }
        // The script order of this type whose building is nearest to the newborn unit.
        let best = st
            .production
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == u.kind)
            .filter_map(|(k, p)| {
                let b = world.units.get(p.building.0 as usize).filter(|b| b.owner == u.owner)?;
                Some((crate::api::norma(b.pos.x - u.pos.x, b.pos.y - u.pos.y), k))
            })
            .min();
        let Some((_, k)) = best else { continue };
        let p = st.production.remove(k);
        let id = u.id;
        let serial = st.serial(id);
        if let Some(g) = st.groups.get_mut(p.group) {
            g.members.push(crate::state::Member { id: id.0 as u16, serial });
            if g.start_nmask != 0 {
                st.extra.entry(id.0).or_default().nmask = Some(g.start_nmask);
            }
        }
    }
}
