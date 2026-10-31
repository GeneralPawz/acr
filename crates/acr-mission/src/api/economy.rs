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
const NO_GATHER: Status = Status::Stub("no gathering in acr-sim yet");

pub const FUNCTIONS: &[ApiFn] = &[
    f("GetResource", 0x562d7a, DONE, get_resource),
    f("AddResource", 0x562dea, DONE, add_resource),
    f("SetResource", 0x562eca, DONE, set_resource),
    f("TakeFood", 0x5664dd, DONE, take_food),
    f("TakeWood", 0x566637, DONE, take_wood),
    f("GetMoney", 0x568f03, DONE, get_money),
    f("GetExtraction", 0x5699eb, DONE, get_extraction),
    f("IsResourceEnabled", 0x577225, Status::Stub("no resource map in acr-sim yet: 0"), zero),
    f("SetToInternalResourcesZero", 0x56eba8, Status::Stub("unit stance/carried resources not modelled"), zero),
    f("ProduceUnit", 0x566dd2, DONE, produce_unit),
    f("ProduceUnitFast", 0x566f36, DONE, produce_unit_fast),
    f("CheckProduction", 0x5672cc, DONE, check_production),
    f("TryUnit", 0x56829b, DONE, try_unit),
    f("TryUpgrade", 0x5684d6, DONE, try_upgrade),
    f("TryUpgradeEx", 0x568629, DONE, try_upgrade),
    f("UpgIsDone", 0x569259, DONE, upg_is_done),
    f("UpgIsRun", 0x5692a9, DONE, upg_is_done),
    f("InitialUpgrade", 0x55c02f, Status::Done, initial_upgrade),
    f("InitialUpgradeNew", 0x55c202, DONE, initial_upgrade_new),
    f("DisableUpgrade", 0x55c335, DONE, disable_upgrade),
    f("SetUpgradeEnableStatus", 0x55c437, DONE, set_upgrade_enable_status),
    f("EnableUnit", 0x55c5bb, DONE, enable_unit),
    f("SetUnitEnableState", 0x55c4f9, DONE, set_unit_enable_state),
    f("GetUpgradeCost", 0x56304a, DONE, get_upgrade_cost),
    f("GetUnitCost", 0x562f80, DONE, get_unit_cost),
    f("AI_Torg", 0x569083, DONE, ai_torg),
    f("GetTorgResult", 0x5690c0, DONE, get_torg_result),
    f("SetBuildingsCollector", 0x577c6f, DONE, set_buildings_collector),
    f("SetUnitsCollector", 0x577d70, DONE, set_units_collector),
    f("SetEnemyBuildingsCollector", 0x577e71, DONE, set_enemy_buildings_collector),
    f("SGP_RepairBuilding", 0x577b9c, Status::Stub("no repairs in acr-sim yet"), zero),
    f("SGP_TakeResources", 0x577be1, NO_GATHER, zero),
    f("CreateFarmList", 0x57c20d, DONE, create_farm_list),
    f("CreateMeatList", 0x57bff5, DONE, create_meat_list),
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
        let (bi, ready) = (u.id.0 as usize, u.stage >= api.world.max_stage(u));
        if !u.building || !ready || (fast.is_none() && !u.queue.is_empty()) {
            continue;
        }
        let power = api.world.players[usize::from(u.owner)].type_count.get(usize::from(t.0)).copied().unwrap_or(0);
        api.world.units[bi].queue.push(acr_sim::economy::Production {
            kind: t,
            progress,
            stages,
            power,
            endless: false,
        });
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
        u.building && u.queue.is_empty() && u.stage >= api.world.max_stage(u)
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

/// `InitialUpgrade` (0x55c02f): applies the upgrade's effect for a nation
/// (`PerformNewUpgrade` 0x532281) and marks it done and no longer offered unless it is
/// individual or a stage.
fn initial(api: &mut Api, n: u8, u: u16) {
    let def =
        api.world.catalog.tech.loaded.then(|| api.world.catalog.tech.upgrade(acr_sim::tech::UpgradeId(u)).clone());
    if def.is_some() && usize::from(n) < api.world.players.len() {
        acr_sim::upgrade::perform(api.world, n, acr_sim::tech::UpgradeId(u), None);
    }
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

/// Remembers a collector group of a nation (`CITY+0xf46/+0xf48/+0xf4a`).
fn remember(api: &mut Api, c: &Call, set: fn(&mut crate::aicity::City, usize)) -> Result<()> {
    let n = c.u8(0)?;
    let h = c.handle(1)?;
    if n < 8 && h.magic == magic::GROUP && usize::from(h.index) < api.st.groups.len() {
        set(&mut api.st.cities[usize::from(n)], usize::from(h.index));
    }
    Ok(())
}

/// `SetBuildingsCollector(byte nat, GAMEOBJ* grp)` (0x577c6f): the nation's buildings join
/// the group, now and whenever one is created.
fn set_buildings_collector(api: &mut Api, c: &mut Call) -> Result<u32> {
    remember(api, c, |city, g| city.buildings_collector = Some(g))?;
    collector(api, c, |_, u, n| u.building && u.owner == n)
}

/// `SetUnitsCollector(byte nat, GAMEOBJ* grp)` (0x577d70): the nation's units join the group,
/// and so do the units its AI produces later.
fn set_units_collector(api: &mut Api, c: &mut Call) -> Result<u32> {
    remember(api, c, |city, g| city.units_collector = Some(g))?;
    collector(api, c, |_, u, n| !u.building && u.owner == n)
}

/// `SetEnemyBuildingsCollector(byte nat, GAMEOBJ* grp)` (0x577e71): enemy buildings join the
/// group, now and whenever one is created (0x588569). Every object slot that holds a building
/// not yet collected (OB+0x23 bit1) and not allied with `nat` (OB+0x9a & NATIONS[nat].NMask)
/// is appended with its serial and marked collected, so a later call (for any nation) skips
/// it; the group itself is not searched for duplicates.
fn set_enemy_buildings_collector(api: &mut Api, c: &mut Call) -> Result<u32> {
    let n = c.u8(0)?;
    let h = c.handle(1)?;
    if n >= 8 || h.magic != magic::GROUP || usize::from(h.index) >= api.st.groups.len() {
        return Ok(0);
    }
    let g = usize::from(h.index);
    api.st.cities[usize::from(n)].enemy_collector = Some(g);
    let mask = api.st.nmask[usize::from(n)];
    let pick: Vec<_> = api
        .world
        .units
        .iter()
        .filter(|u| u.alive && u.building)
        .filter(|u| !api.st.extra.get(&u.id.0).is_some_and(|e| e.collected))
        .filter(|u| api.st.unit_nmask(u.id, u.owner) & mask == 0)
        .map(|u| api.member_of(u.id))
        .collect();
    for m in pick {
        api.st.extra.entry(u32::from(m.id)).or_default().collected = true;
        api.st.groups[g].members.push(m);
    }
    Ok(0)
}

/// `GetTopZone` (0x5791cb) of a unit: the path area of its 64 px cell, `None` outside.
fn unit_area(api: &Api, u: &acr_sim::Unit) -> Option<usize> {
    let t = api.st.topo.as_ref()?;
    let (cx, cy) = (u.pos.x >> 10, u.pos.y >> 10);
    if cx < 0 || cy < 0 || cx >= t.cells.width as i32 || cy >= t.cells.height as i32 {
        return None;
    }
    let a = usize::from(t.cells.get(cx as usize, cy as usize).unwrap_or(0xFFFF));
    (a < t.areas.len()).then_some(a)
}

/// `void CreateFarmList(word* out, byte nation)` (0x57c20d): per path area the last building
/// of the nation (finished or not) that takes farm fields (acceptor bit 8), else 0xFFFF.
fn create_farm_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (out, nat) = (c.u32(0)?, c.u8(1)?);
    let n = api.st.topo.as_ref().map_or(0, |t| t.areas.len());
    let mut list = vec![0xFFFFu16; n];
    for u in &api.world.units {
        if u.owner == nat
            && u.living()
            && u.building
            && api.world.catalog.get(u.kind).economy.resource_base & 0x100 != 0
            && let Some(a) = unit_area(api, u)
        {
            list[a] = u.id.0 as u16;
        }
    }
    for (i, v) in list.iter().enumerate() {
        c.write_u16(out + 2 * i as u32, *v)?;
    }
    Ok(0)
}

/// `void CreateMeatList(word* out, byte nation, int max)` (0x57bff5): per path area the last
/// living non-allied unit whose body becomes meat, 0xFFFF where there are none or more than
/// `max`.
fn create_meat_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (out, nat, max) = (c.u32(0)?, c.u8(1)?, c.i32(2)?);
    let n = api.st.topo.as_ref().map_or(0, |t| t.areas.len());
    let (mut list, mut count) = (vec![0xFFFFu16; n], vec![0i32; n]);
    let mask = api.world.nation_mask[usize::from(nat) & 7];
    for u in &api.world.units {
        if u.nmask & mask == 0
            && u.living()
            && api.world.catalog.get(u.kind).economy.meat.is_some()
            && let Some(a) = unit_area(api, u)
        {
            count[a] += 1;
            list[a] = u.id.0 as u16;
        }
    }
    for (i, v) in list.iter().enumerate() {
        c.write_u16(out + 2 * i as u32, if count[i] > max { 0xFFFF } else { *v })?;
    }
    Ok(0)
}

/// `int GetExtraction(byte res)` (0x5699eb), AI only: the city's food or wood gathering
/// speed, or the peasants in its stone, iron, coal or gold mines.
fn get_extraction(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    Ok(acr_sim::economy::extraction(api.world, n, usize::from(c.u8(0)?)) as u32)
}

/// The market slot of a script resource id (`INVECO`, 0x67169c, the inverse of the loaded
/// `ECO.EcResID`), `None` without `ECONOMY.TXT` or for an id past the six resources.
fn market_slot(api: &Api, res: u8) -> Option<usize> {
    api.world.catalog.market.as_ref()?.slot(usize::from(res))
}

/// `int GetTorgResult(byte sell, byte buy, int amount)` (0x5690c0 → 0x4aa039): what
/// `amount` of `sell` buys of `buy` at the market's current rates, `trunc(amount ·
/// Sell[s] / Buy[b])`. The rates are the player's market (`world.market`).
fn get_torg_result(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(s), Some(b)) = (market_slot(api, c.u8(0)?), market_slot(api, c.u8(1)?)) else { return Ok(0) };
    Ok(api.world.market.quote(s, b, c.i32(2)?) as u32)
}

/// `void AI_Torg(byte sell, byte buy, int amount)` (0x569083 → 0x4aa090): the city's
/// nation (`City+0x11dcc`) trades through `ECO.PerformOperation` (0x4a9d80), the same
/// trade as the player's market (`acr_sim::market::trade`); the rates drift back with
/// `Process` (0x4aa0af) every tick.
fn ai_torg(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    if market_slot(api, c.u8(0)?).is_none() || market_slot(api, c.u8(1)?).is_none() {
        return Ok(0);
    }
    acr_sim::market::trade(api.world, n, usize::from(c.u8(0)?), usize::from(c.u8(1)?), c.i32(2)?);
    Ok(0)
}

/// `bool TryUnit(GAMEOBJ* type, int max, byte costPercent, byte probability)` (0x56829b):
/// proposes a project of the AI city when the census counts fewer than `max` objects of the
/// type (`City+0xeb73`), the city can produce it (`City+0x10b73`), `rando()` falls below
/// the probability and `costPercent` covers the project's share of the stock; while a
/// building project is pending (`City+0x14b73` bit 0) buildings are refused after the draw.
/// With game option 2 at 1 (0x6e45c9) artillery and towers are refused.
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
    let ty = api.world.catalog.get(t);
    if api.world.game_options[1] == 1 && (ty.role.artillery || ty.role.usage == acr_sim::catalog::usage::TOWER) {
        return Ok(0);
    }
    let count = i32::from(api.world.players[usize::from(n)].type_count.get(usize::from(t.0)).copied().unwrap_or(0));
    let Some(&k) = city.index.get(&t).filter(|_| max > count) else { return Ok(0) };
    let need = city.props[k].percent;
    if api.world.random.rando() >= crate::aicity::perconv(prob) || percent < need {
        return Ok(0);
    }
    // One building project at a time (`PresentProject`, 0x56841c).
    if api.world.catalog.get(t).building && acr_sim::aiplace::project_pending(api.world, n) {
        return Ok(0);
    }
    api.st.cities[usize::from(n)].best.push(k);
    Ok(1)
}

/// `bool TryUpgrade(GAMEOBJ* upgrade, int costPercent, int probability)` and `TryUpgradeEx`
/// (two more distance limits): proposes researching an upgrade an idle building of the AI
/// nation offers.
fn try_upgrade(api: &mut Api, c: &mut Call) -> Result<u32> {
    if c.u32(0)? == 0 {
        return Ok(0);
    }
    let h = c.handle(0)?;
    if h.magic == u32::from_be_bytes(*b"SAFE") {
        return Ok(0);
    }
    let Caller::Ai(n) = api.caller else {
        api.st.error("ERROR: AI function must be called only from AI section.");
        return Ok(0);
    };
    if h.magic != magic::UPGRADE {
        return Ok(0);
    }
    let (percent, prob) = (c.i32(1)?, c.i32(2)?);
    if prob > 100 {
        api.st.error("TryUpgrade : invalid <Probability>");
        return Ok(0);
    }
    let city = &api.st.cities[usize::from(n)];
    if city.best.len() >= 127 {
        return Ok(0);
    }
    let Some(&k) = city.upgrade_index.get(&h.index) else { return Ok(0) };
    let need = city.props[k].percent;
    if api.world.random.rando() < crate::aicity::perconv(prob.max(0)) && percent >= need {
        if c.name() == "TryUpgradeEx" && !near_city(api, n, k, c.i32(3)?, c.i32(4)?) {
            return Ok(0);
        }
        api.st.cities[usize::from(n)].best.push(k);
        return Ok(1);
    }
    Ok(0)
}

/// `TryUpgradeEx`'s distance limits (0x568629): the producer lies closer than `max_center`
/// px to the city centre and closer than `max_building` px to one of the city's buildings
/// (its first 64, mines excepted; the producer itself usually among them).
fn near_city(api: &Api, n: u8, k: usize, max_center: i32, max_building: i32) -> bool {
    let Some(b) = api.st.cities[usize::from(n)].props[k].producer else { return false };
    let p = &api.world.units[b.0 as usize];
    let (px, py) = p.pos.pixels();
    let (cx, cy) = api.world.cities[usize::from(n)].center.unwrap_or((0, 0));
    if crate::api::norma(px - cx * 128, py - cy * 128) >= max_center {
        return false;
    }
    let d2 = api
        .world
        .units
        .iter()
        .filter(|u| u.owner == n && u.living() && u.building)
        .take(64)
        .filter(|u| api.world.catalog.get(u.kind).role.usage != acr_sim::catalog::usage::MINE)
        .map(|u| crate::api::norma(u.pos.x - p.pos.x, u.pos.y - p.pos.y) >> 4)
        .min()
        .unwrap_or(100_000);
    d2 < max_building
}

/// `int GetUnits(GAMEOBJ* type)` / `GetReadyUnits`: objects of the type the AI nation owns
/// (`UnitAmount`), or only the finished ones (`ReadyAmount`).
fn get_units(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Caller::Ai(n) = api.caller else { return Ok(0) };
    let Some(t) = api.type_arg(c, 0, c.name())? else { return Ok(0) };
    let ready_only = c.name() == "GetReadyUnits";
    let w = &*api.world;
    Ok(w.units
        .iter()
        .filter(|u| {
            u.living() && u.owner == n && u.kind == t && (!ready_only || !u.building || u.stage >= w.max_stage(u))
        })
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
        if u.building && u.alive {
            // A new building joins the collectors (0x588569): its nation's buildings
            // collector and the enemy collector of every nation it is not allied with.
            let (id, owner) = (u.id, u.owner);
            let nm = st.unit_nmask(id, owner);
            let serial = st.serial(id);
            for n in 0..8 {
                let city = &st.cities[n];
                let g = if n == usize::from(owner) { city.buildings_collector } else { None };
                let e = if nm & st.nmask[n] == 0 { city.enemy_collector } else { None };
                for g in [g, e].into_iter().flatten() {
                    if let Some(grp) = st.groups.get_mut(g)
                        && !grp.members.iter().any(|m| u32::from(m.id) == id.0)
                    {
                        grp.members.push(crate::state::Member { id: id.0 as u16, serial });
                    }
                }
            }
            continue;
        }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_script::heap::Heap;
    use acr_script::memory::Memory;
    use acr_script::runtime::HostCall;
    use acr_script::x86::Cpu;
    use acr_sim::World;
    use acr_sim::catalog::Catalog;
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;
    use acr_sim::market::MarketDef;

    use super::super::{Api, Call, Caller, function};
    use crate::state::{ScriptSetup, ScriptState};
    use crate::texts::MissionTexts;

    const STACK: u32 = 0x1_8000;

    fn world() -> World {
        let mut text = String::from("a b c d e f g h i j k\n");
        for (r, b, s) in [
            ("FOOD", "20 25 40", "5 15 20"),
            ("IRON", "140 190 240", "80 110 140"),
            ("WOOD", "40 50 60", "20 30 40"),
            ("COAL", "140 190 240", "80 110 140"),
            ("STONE", "75 90 105", "30 45 60"),
            ("GOLD", "270 300 330", "180 210 240"),
        ] {
            text +=
                &format!("{r}\nBUY {b} CHANGE 100 1.0 TIMERETURN 0.0001\nSELL {s} CHANGE 100 1.0 TIMERETURN 0.0001\n");
        }
        let mut c = Catalog::new();
        c.market = MarketDef::parse(text.as_bytes());
        let f = Formations::new(&Default::default(), &Default::default());
        World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(f))
    }

    fn call(name: &str, world: &mut World, caller: Caller, args: [u32; 3]) -> u32 {
        let mut st = ScriptState::new(ScriptSetup::default(), MissionTexts::default());
        let mut mem = Memory::new();
        mem.map(0x1_0000, 0x1_0000);
        for (i, &a) in args.iter().enumerate() {
            mem.write_u32(STACK + 4 + 4 * i as u32, a).expect("stack");
        }
        let mut cpu = Cpu::new();
        cpu.regs[usize::from(acr_script::x86::decode::reg::ESP)] = STACK;
        let mut heap = Heap::new(0x2_0000, 0x3_0000);
        let mut raw = HostCall { dll: "test", name, cpu: &mut cpu, mem: &mut mem, heap: &mut heap };
        let (mut commands, mut events) = (Vec::new(), Vec::new());
        let mut api =
            Api { st: &mut st, world, commands: &mut commands, events: &mut events, caller, vm: 0, dip: None };
        (function(name).expect("known").handler)(&mut api, &mut Call { raw: &mut raw }).expect("call")
    }

    fn object(name: &str) -> acr_sim::catalog::ObjectType {
        let def = acr_formats::defs::UnitDef::parse(
            format!(
                "NAME {name}\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
                 @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n"
            )
            .as_bytes(),
        );
        acr_sim::catalog::ObjectType::from_def(name, &def)
    }

    /// Calls `name` on a state of the caller's, with group handles for `groups` at 0x1_c000.
    fn call_st(name: &str, world: &mut World, st: &mut ScriptState, groups: u16, args: [u32; 3]) -> u32 {
        let mut mem = Memory::new();
        mem.map(0x1_0000, 0x1_0000);
        for g in 0..u32::from(groups) {
            mem.write_u16(0x1_c000 + 8 * g, g as u16).expect("index");
            mem.write_u32(0x1_c004 + 8 * g, super::super::call::magic::GROUP).expect("magic");
        }
        for (i, &a) in args.iter().enumerate() {
            mem.write_u32(STACK + 4 + 4 * i as u32, a).expect("stack");
        }
        let mut cpu = Cpu::new();
        cpu.regs[usize::from(acr_script::x86::decode::reg::ESP)] = STACK;
        let mut heap = Heap::new(0x2_0000, 0x3_0000);
        let mut raw = HostCall { dll: "test", name, cpu: &mut cpu, mem: &mut mem, heap: &mut heap };
        let (mut commands, mut events) = (Vec::new(), Vec::new());
        let mut api =
            Api { st, world, commands: &mut commands, events: &mut events, caller: Caller::Mission, vm: 0, dip: None };
        (function(name).expect("known").handler)(&mut api, &mut Call { raw: &mut raw }).expect("call")
    }

    #[test]
    fn enemy_buildings_are_collected_once() {
        let mut w = world();
        let mut c = (*w.catalog).clone();
        let kind = c.add(object("Hut"));
        w.catalog = Arc::new(c);
        let mut b = |owner, x| {
            let id = w.spawn(kind, owner, acr_sim::Pos::from_pixels(x, 1000), 0);
            w.units[id.0 as usize].building = true;
            id.0 as u16
        };
        let (a, e1, e2) = (b(0, 1000), b(1, 1200), b(2, 1400));
        let mut st = ScriptState::new(ScriptSetup::default(), MissionTexts::default());
        st.groups = vec![Default::default(); 3];
        // Nation 1 is allied with 0: only nation 2's building is an enemy of nation 0.
        st.nmask[0] = 0b011;
        st.nmask[1] = 0b011;
        let ids = |st: &ScriptState, g: usize| st.groups[g].members.iter().map(|m| m.id).collect::<Vec<_>>();
        call_st("SetEnemyBuildingsCollector", &mut w, &mut st, 3, [0, 0x1_c000, 0]);
        assert_eq!(ids(&st, 0), [e2]);
        assert_eq!(st.cities[0].enemy_collector, Some(0));
        // Nation 2's enemies: 0's and 1's buildings; its own is already marked and skipped by
        // every later call, whichever nation makes it.
        call_st("SetEnemyBuildingsCollector", &mut w, &mut st, 3, [2, 0x1_c008, 0]);
        assert_eq!(ids(&st, 1), [a, e1]);
        call_st("SetEnemyBuildingsCollector", &mut w, &mut st, 3, [1, 0x1_c010, 0]);
        assert!(ids(&st, 2).is_empty());
        // A second call into the same group adds nothing, since all are marked.
        call_st("SetEnemyBuildingsCollector", &mut w, &mut st, 3, [0, 0x1_c000, 0]);
        assert_eq!(ids(&st, 0), [e2]);
    }

    #[test]
    fn leave_building_orders_one_unit_of_the_type_out() {
        let mut w = world();
        let mut c = (*w.catalog).clone();
        let hut = c.add(object("Hut"));
        let man = c.add(object("Man"));
        w.catalog = Arc::new(c);
        let b = w.spawn(hut, 0, acr_sim::Pos::from_pixels(1000, 1000), 0);
        w.units[b.0 as usize].building = true;
        let mut st = ScriptState::new(ScriptSetup::default(), MissionTexts::default());
        // Empty: nothing happens.
        call_st("LeaveBuilding", &mut w, &mut st, 0, [b.0, u32::from(man.0), 0]);
        assert!(w.units[b.0 as usize].orders.is_empty());
        let u = w.spawn(man, 0, acr_sim::Pos::from_pixels(1000, 1000), 0);
        w.units[b.0 as usize].inside.push(u);
        w.units[u.0 as usize].inside_of = Some(b);
        call_st("LeaveBuilding", &mut w, &mut st, 0, [b.0, u32::from(man.0), 0]);
        assert!(matches!(
            w.units[b.0 as usize].orders.last(),
            Some(acr_sim::world::Order::LeaveMine { kind, .. }) if *kind == man
        ));
        // Nobody walks out at once.
        assert_eq!(w.units[b.0 as usize].inside, [u]);
        // Out of range indices do nothing.
        call_st("LeaveBuilding", &mut w, &mut st, 0, [0xFFFF, u32::from(man.0), 0]);
        call_st("LeaveBuilding", &mut w, &mut st, 0, [u32::MAX, u32::from(man.0), 0]);
    }

    #[test]
    fn ai_trades_on_the_players_market() {
        let mut w = world();
        // 100 wood (sells at 30) buy 10 gold (buys at 300).
        assert_eq!(call("GetTorgResult", &mut w, Caller::Ai(1), [0, 1, 100]), 10);
        assert_eq!(call("GetTorgResult", &mut w, Caller::Ai(1), [0, 6, 100]), 0);
        w.players[1].resources[0] = 60;
        let mut expect = w.clone();
        acr_sim::market::trade(&mut expect, 1, 0, 1, 100);
        call("AI_Torg", &mut w, Caller::Ai(1), [0, 1, 100]);
        // At most the stock: 60 wood for 6 gold, and the shared rates moved.
        assert_eq!(w.players[1].resources[0], 0);
        assert_eq!(w.players[1].resources[1], expect.players[1].resources[1]);
        assert_eq!(w.market, expect.market);
        assert!(w.market.buy[5] > 300.0 && w.market.sell[2] < 30.0);
        // Only the AI section trades.
        w.players[0].resources[0] = 100;
        call("AI_Torg", &mut w, Caller::Mission, [0, 1, 100]);
        assert_eq!(w.players[0].resources[0], 100);
    }
}
