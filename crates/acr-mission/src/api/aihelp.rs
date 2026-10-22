//! Helpers only the AI DLLs (and DipServer) use: forces, per-area lists, damage values,
//! defensive regrouping, gathering and repair processes.

use acr_script::runtime::Result;
use acr_sim::world::EntityId;

use super::call::magic;
use super::{Api, ApiFn, Call, Status};
use crate::state::{GroupOrder, Member};

const fn f(name: &'static str, original: u32, status: Status, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status, handler }
}

const DONE: Status = Status::Done;
pub const FUNCTIONS: &[ApiFn] = &[
    f(
        "GetEnemyForce",
        0x571aa8,
        Status::Partial("objects in the zone circle; the area-distance limit is not applied"),
        get_enemy_force,
    ),
    f("GetGroupForce", 0x56f574, DONE, get_group_force),
    f("GetEnemy3", 0x5818a5, DONE, get_enemy3),
    f("GetMaxDamage", 0x575250, DONE, get_max_damage),
    f("GetMinDamage", 0x57521b, DONE, get_max_damage),
    f("GetUnitStorm", 0x575285, DONE, get_unit_storm),
    f("GetSearchRadius", 0x56f46b, DONE, get_search_radius),
    f("GetMonUsage", 0x5751e5, DONE, get_mon_usage),
    f("CheckIfBuildingIsFired", 0x57c39e, DONE, check_if_building_is_fired),
    f("CreateNonFiredEnemyBuildingsTopList", 0x57b2fd, DONE, create_non_fired_enemy_buildings_top_list),
    f(
        "CreateTopListEnArmy",
        0x57bad9,
        Status::Partial("buildings count their garrison as force"),
        create_top_list_en_army,
    ),
    f("CreateFriendBuildingsTopList", 0x57b859, DONE, create_friend_buildings_top_list),
    f("SearchCapturedTopToDeffend", 0x5734df, DONE, search_captured_top),
    f("MakeEnemyAgrTopGrp", 0x573730, DONE, make_enemy_agr_top_grp),
    f("CheckDef", 0x573c0d, DONE, check_def),
    f("FindNextZoneOnTheSafeWayToObject", 0x57a33c, DONE, find_next_zone_to_object),
    f("SGP_RepairBuilding", 0x577b9c, DONE, sgp_repair_building),
    f("SGP_TakeResources", 0x577be1, DONE, sgp_take_resources),
    f("IsResourceEnabled", 0x577225, DONE, is_resource_enabled),
    f("SetToInternalResourcesZero", 0x56eba8, DONE, set_to_internal_resources_zero),
    // These two replace the approximations in dip.rs (later tables win).
    f("ArtAttackObject", 0x570b32, DONE, art_attack_object),
    f("ArtAttackGroud", 0x5709b5, DONE, art_attack_ground),
    f(
        "MakeOneShotToBuild",
        0x573da9,
        Status::Partial("buildings found by their position's cell (the engine keeps a building map)"),
        make_one_shot_to_build,
    ),
];

/// `word MakeOneShotToBuild(GAMEOBJ* zone, GAMEOBJ* grp, byte nat)` (0x573da9): the enemy
/// buildings in the 128 px cells of the zone's rings; every member that is not attacking
/// fires once (`AttackObj(.., 0x8F, 0, 1)`, then priority 0xF0) at the one nearest by path
/// within its range band (musket shooters: the inner eighth); attacking members are released
/// with `rando() < 600`. Returns the last target, 0xFFFF if none.
fn make_one_shot_to_build(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(z) =
        (if c.u32(0)? == 0 { None } else { api.zone_circles(&c.handle(0)?) }).and_then(|z| z.first().copied())
    else {
        return Ok(0);
    };
    let Some(g) = api.group(c, 1)? else { return Ok(0xFFFF) };
    let nat_mask = api.world.nation_mask[usize::from(c.u8(2)? & 7)];
    let side = api.world.buckets.side();
    let mut targets: Vec<usize> = Vec::new();
    for r in 0..=((z.r >> 7) + 1) as usize {
        for &(a, b) in acr_sim::morale::ring(r) {
            let (cx, cy) = ((z.x >> 7) + a, (z.y >> 7) + b);
            if cx < 0 || cy < 0 || cx >= side - 1 || cy >= side - 1 {
                continue;
            }
            let cell: Vec<usize> =
                api.world.buckets.all(cx, cy).map(|k| k as usize).filter(|&k| api.world.units[k].building).collect();
            let any = cell.first().copied();
            let own = cell.iter().copied().find(|&k| api.world.units[k].nmask & nat_mask != 0);
            if let Some(b) = any
                && Some(b) != own
                && api.world.units[b].living()
                && !targets.contains(&b)
                && targets.len() < 1600
            {
                targets.push(b);
            }
        }
    }
    if targets.is_empty() {
        return Ok(0xFFFF);
    }
    let catalog = api.world.catalog.clone();
    let mut res = 0xFFFF;
    for id in api.members(g) {
        let i = id.0 as usize;
        let u = &api.world.units[i];
        if u.target().is_some() {
            if api.world.random.rando() < 600 && !api.world.units[i].unlimited {
                let u = &mut api.world.units[i];
                u.orders.clear();
                u.prio = 0;
            }
            continue;
        }
        let adv = api.world.adv(u.owner, u.kind);
        let rmin = adv.attacks[1].radius.0;
        let mut rmax = acr_sim::combat::range_bounds(&adv).1;
        if matches!(adv.attacks[1].kind.as_str(), "VISTREL" | "VISTREL2") {
            rmax = rmin + ((rmax - rmin) >> 3);
        }
        let (ux, uy) = (u.pos.x, u.pos.y);
        let mut best: Option<(i32, usize)> = None;
        for &t in &targets {
            let tu = &api.world.units[t];
            let d = super::util::norma(ux - tu.pos.x, uy - tu.pos.y) >> 4;
            if d > rmin && d < rmax + catalog.get(tu.kind).add_shot_radius {
                let td = super::tactics::top_dist(api, tu.pos.x >> 10, tu.pos.y >> 10, ux >> 10, uy >> 10);
                if best.is_none_or(|b| td < b.0) {
                    best = Some((td, t));
                }
            }
        }
        if let Some((_, t)) = best
            && acr_sim::combat::attack_obj_times(api.world, i, t, 0x8F, acr_sim::combat::Placement::Replace, 1)
        {
            let u = &mut api.world.units[i];
            u.prio = 0xF0;
            if let Some(acr_sim::world::Order::Attack { prio, .. }) = u.orders.last_mut() {
                *prio = 0xF0;
            }
            res = t as u32;
        }
    }
    Ok(res)
}

/// `void ArtAttackObject(GAMEOBJ* guns, byte nat, word target)` (0x570b32): every living gun
/// that is not attacking attacks the object (priority 0x90, after its orders, one shot).
fn art_attack_object(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let Some(t) = api.object(c.u16(2)?).filter(|u| u.living()).map(|u| u.id.0 as usize) else { return Ok(0) };
    for id in api.members(g) {
        if api.world.units[id.0 as usize].target().is_none() {
            acr_sim::combat::attack_obj_times(api.world, id.0 as usize, t, 0x90, acr_sim::combat::Placement::Append, 1);
        }
    }
    Ok(0)
}

/// `void ArtAttackGroud(GAMEOBJ* guns, byte nat, int x, int y)` (0x5709b5): every living gun
/// that is not attacking fires one shot at the ground (`NewAttackPoint(x, y, 0x90, 2, 1)`).
fn art_attack_ground(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let (x, y) = (c.i32(2)?, c.i32(3)?);
    for id in api.members(g) {
        if api.world.units[id.0 as usize].target().is_none() {
            acr_sim::artillery::attack_point(
                api.world,
                id.0 as usize,
                x,
                y,
                0x90,
                acr_sim::combat::Placement::Append,
                1,
            );
        }
    }
    Ok(0)
}

fn n_areas(api: &Api) -> usize {
    api.st.topo.as_ref().map_or(0, |t| t.areas.len())
}

fn zone_of_unit(api: &Api, u: &acr_sim::Unit) -> u16 {
    super::tactics::zone_of(api, u.pos.x >> 10, u.pos.y >> 10)
}

fn enemy(api: &Api, u: &acr_sim::Unit, nat: u8) -> bool {
    api.st.unit_nmask(u.id, u.owner) & api.st.nmask[usize::from(nat & 7)] == 0
}

/// `int GetEnemyForce(GAMEOBJ* zone, byte nation)` (0x571aa8): `FORCE` of the enemies minus
/// that of the friends in the zone.
fn get_enemy_force(api: &mut Api, c: &mut Call) -> Result<u32> {
    let zone = if c.u32(0)? == 0 { None } else { api.zone_circles(&c.handle(0)?) };
    let Some(z) = zone.and_then(|z| z.first().copied()) else { return Ok(0) };
    let nat = c.u8(1)?;
    let catalog = api.world.catalog.clone();
    let sum: i32 = api
        .world
        .units
        .iter()
        .filter(|u| u.active() && z.contains(u))
        .map(|u| {
            let f = catalog.get(u.kind).role.force;
            if enemy(api, u, nat) { f } else { -f }
        })
        .sum();
    Ok(sum as u32)
}

fn get_group_force(api: &mut Api, c: &mut Call) -> Result<u32> {
    if c.u32(0)? == 0 {
        return Ok(0);
    }
    let h = c.handle(0)?;
    if h.magic != magic::GROUP || usize::from(h.index) >= api.st.groups.len() {
        return Ok(0);
    }
    let catalog = api.world.catalog.clone();
    let members = api.members(usize::from(h.index));
    Ok(members.iter().map(|id| catalog.get(api.world.units[id.0 as usize].kind).role.force).sum::<i32>() as u32)
}

/// `int GetEnemy3(int zone, int r, byte nation)`: enemies within `r` px of the area's corner.
fn get_enemy3(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (zone, r, nat) = (c.i32(0)?, c.i32(1)?, c.u8(2)?);
    if zone < 0 || zone as usize >= n_areas(api) || r < 0 {
        return Ok(0);
    }
    let (ax, ay) = super::tactics::area_center(api, zone as u16);
    let circle = super::Circle { x: ax << 6, y: ay << 6, r };
    Ok(api.world.units.iter().filter(|u| u.active() && enemy(api, u, nat) && circle.contains(u)).count().min(1600)
        as u32)
}

/// Type argument as `word` index into the type list; `None` when out of range.
fn type_index(api: &Api, c: &Call) -> Result<Option<acr_sim::TypeId>> {
    let t = c.u16(0)?;
    Ok((usize::from(t) < api.world.catalog.len()).then_some(acr_sim::TypeId(t)))
}

/// `word GetMaxDamage(word type)` / `GetMinDamage` (0x575250 / 0x57521b): `MaxDamage[0]` /
/// `MinDamage[0]` of the type, 0xFFFF if invalid.
fn get_max_damage(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(t) = type_index(api, c)? else { return Ok(0xFFFF) };
    let min = c.name() == "GetMinDamage";
    let a = api.world.catalog.get(t).attacks.first();
    Ok(a.map_or(0, |a| u32::from((if min { a.min_damage } else { a.damage }) as u16)))
}

/// `int GetUnitStorm(word type)` (0x575285): the type's `STORMFORCE`, 0xFFFF if invalid.
fn get_unit_storm(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(type_index(api, c)?.map_or(0xFFFF, |t| api.world.catalog.get(t).role.storm_force as u32))
}

/// `word GetMonUsage(word type)` (0x5751e5): the type's `USAGE`; for an invalid type only `ax`
/// is set (0xFFFF), the upper half of `eax` keeps the argument.
fn get_mon_usage(api: &mut Api, c: &mut Call) -> Result<u32> {
    let raw = c.u32(0)?;
    Ok(type_index(api, c)?.map_or((raw & 0xFFFF_0000) | 0xFFFF, |t| u32::from(api.world.catalog.get(t).role.usage)))
}

/// `int GetSearchRadius(int index)`: the enemy search radius in pixels, -1 for no object.
fn get_search_radius(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(u) = u16::try_from(c.i32(0)?).ok().and_then(|i| api.object(i)) else { return Ok(u32::MAX) };
    Ok((api.world.catalog.get(u.kind).search_radius >> 4) as u32)
}

/// Writes a `u16 list[NAreas]`.
fn write_list(c: &mut Call, out: u32, list: &[u16]) -> Result<()> {
    for (i, v) in list.iter().enumerate() {
        c.write_u16(out + 2 * i as u32, *v)?;
    }
    Ok(())
}

/// `void CreateTopListEnArmy(u16* list, byte nation, int minForce)`: per area an enemy object
/// where the enemies' force reaches `minForce`.
fn create_top_list_en_army(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (out, nat, min) = (c.u32(0)?, c.u8(1)?, c.i32(2)?);
    let n = n_areas(api);
    let mut list = vec![0xFFFFu16; n];
    let mut count = vec![0i32; n];
    for u in api
        .world
        .units
        .iter()
        .filter(|u| u.active() && api.world.catalog.get(u.kind).economy.meat.is_none() && enemy(api, u, nat))
    {
        let z = usize::from(zone_of_unit(api, u));
        if z >= n {
            continue;
        }
        if u.building && !u.inside.is_empty() {
            count[z] = (count[z] + 2 * u.inside.len() as i32).max(min);
        } else {
            count[z] += 1;
        }
        list[z] = u.id.0 as u16;
    }
    for (l, c) in list.iter_mut().zip(&count) {
        if *c < min {
            *l = 0xFFFF;
        }
    }
    write_list(c, out, &list)?;
    Ok(0)
}

/// `void CreateFriendBuildingsTopList(u16* list, byte nation)`.
fn create_friend_buildings_top_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (out, nat) = (c.u32(0)?, c.u8(1)?);
    let n = n_areas(api);
    let mut list = vec![0xFFFFu16; n];
    for u in api.world.units.iter().filter(|u| u.building && ((u.living() && u.owner != 7) || nat == 7)) {
        if enemy(api, u, nat) {
            continue;
        }
        let z = usize::from(zone_of_unit(api, u));
        if z < n {
            list[z] = u.id.0 as u16;
        }
    }
    write_list(c, out, &list)?;
    Ok(0)
}

/// `int SearchCapturedTopToDeffend(byte nation, u16* flags)` (0x5734df): bit 0 areas with own
/// capturable objects (`CAPTURE`), bit 1 with own buildings, bit 2 their neighbours.
fn search_captured_top(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (nat, out) = (c.u8(0)?, c.u32(1)?);
    let n = n_areas(api);
    let mut flags = vec![0u16; n];
    for u in api.world.units.iter().filter(|u| u.living() && u.owner == nat) {
        let z = usize::from(zone_of_unit(api, u));
        if z < n {
            if api.world.catalog.get(u.kind).role.capture {
                flags[z] |= 1;
            }
            if u.building {
                flags[z] |= 2;
            }
        }
    }
    for z in 0..n {
        if flags[z] & 3 != 0 {
            let links = api.st.topo.as_ref().map(|t| t.areas[z].links.clone()).unwrap_or_default();
            for (a, _) in links {
                if let Some(f) = flags.get_mut(usize::from(a)) {
                    *f |= 4;
                }
            }
        }
    }
    write_list(c, out, &flags)?;
    Ok(0)
}

/// `void MakeEnemyAgrTopGrp(byte nation, GAMEOBJ* groups /*[NAreas]*/, u16* zoneMask)`:
/// clears one group per area and puts the enemies of the marked areas in them.
fn make_enemy_agr_top_grp(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (nat, groups, mask) = (c.u8(0)?, c.u32(1)?, c.u32(2)?);
    let n = n_areas(api);
    let mut gidx = vec![None; n];
    for (z, slot) in gidx.iter_mut().enumerate() {
        let h = c.handle_at(groups + 8 * z as u32)?;
        if h.magic == magic::GROUP && usize::from(h.index) < api.st.groups.len() {
            api.st.groups[usize::from(h.index)].members.clear();
            *slot = Some(usize::from(h.index));
        }
    }
    let mut adds: Vec<(usize, Member)> = Vec::new();
    for u in api
        .world
        .units
        .iter()
        .filter(|u| u.active() && api.world.catalog.get(u.kind).economy.meat.is_none() && enemy(api, u, nat))
    {
        let z = usize::from(zone_of_unit(api, u));
        if z >= n || c.read_u16(mask + 2 * z as u32)? == 0 {
            continue;
        }
        if let Some(g) = gidx[z] {
            adds.push((g, api.member_of(u.id)));
        }
    }
    for (g, m) in adds {
        api.st.groups[g].members.push(m);
    }
    Ok(0)
}

/// `bool CheckDef(GAMEOBJ* grp, word homeZone, GAMEOBJ* dst, u16* zoneMask)`: members that
/// fight outside the marked areas or idle away from home move to `dst` (the original skips
/// the member after each move).
fn check_def(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(g), Some(d)) = (api.group(c, 0)?, api.group(c, 2)?) else { return Ok(0) };
    let (home, mask) = (c.u16(1)?, c.u32(3)?);
    let n = n_areas(api);
    let mut i = 0;
    while i < api.st.groups[g].members.len() {
        let m = api.st.groups[g].members[i];
        if let Some(u) = api.member(m) {
            let z = zone_of_unit(api, u);
            if usize::from(z) < n {
                let marked = c.read_u16(mask + 2 * u32::from(z))? != 0;
                let attacking = u.target().is_some();
                if (attacking && !marked) || (u.orders.is_empty() && z != home) {
                    api.st.groups[g].members.remove(i);
                    api.st.groups[d].members.push(m);
                }
            }
        }
        i += 1;
    }
    Ok(1)
}

/// `int FindNextZoneOnTheSafeWayToObject(int start, short* danger, u16* objMap, int* maxDanger,
/// int, u16* objOut)`: Dijkstra from `start` to the nearest area with an object.
fn find_next_zone_to_object(api: &mut Api, c: &mut Call) -> Result<u32> {
    let start = c.i32(0)?;
    let n = n_areas(api).min(0x800);
    if start < 0 || start as usize >= n {
        return Ok(0xFFFF);
    }
    let (danger, objmap) = (c.u32(1)?, c.u32(2)?);
    let d = |c: &Call, a: usize| -> Result<i32> { Ok(i32::from(c.read_u16(danger + 2 * a as u32)? as i16)) };
    let mut cost = vec![i32::MAX; n];
    let mut parent = vec![usize::MAX; n];
    let mut done = vec![false; n];
    let s = start as usize;
    cost[s] = 0;
    loop {
        let Some(cur) = (0..n).filter(|&a| !done[a] && cost[a] != i32::MAX).min_by_key(|&a| cost[a]) else {
            return Ok(0xFFFF);
        };
        done[cur] = true;
        let obj = c.read_u16(objmap + 2 * cur as u32)?;
        if obj != 0xFFFF {
            let mut path = vec![cur];
            let mut maxd = d(c, cur)?;
            let mut z = cur;
            while parent[z] != usize::MAX {
                z = parent[z];
                maxd = maxd.max(d(c, z)?);
                path.push(z);
            }
            c.out_i32(3, maxd)?;
            c.out_u16(5, obj)?;
            // Path runs object → start: the zone after start is the second-to-last.
            return Ok(if path.len() > 1 { path[path.len() - 2] as u32 } else { path[0] as u32 });
        }
        let links = api.st.topo.as_ref().map(|t| t.areas[cur].links.clone()).unwrap_or_default();
        for (a, w) in links {
            let a = usize::from(a);
            if a >= n || done[a] {
                continue;
            }
            let nc = cost[cur] + i32::from(w) + d(c, a)?;
            if nc < cost[a] {
                cost[a] = nc;
                parent[a] = cur;
            }
        }
    }
}

/// Installs a group process like `AddProcess(mode)`: 1 = first, 2 = last, else the only one.
fn add_order(api: &mut Api, g: usize, mode: u8, order: GroupOrder) {
    let orders = &mut api.st.groups[g].orders;
    match mode {
        1 => orders.insert(0, order),
        2 => orders.push(order),
        _ => *orders = vec![order],
    }
}

/// `void SGP_RepairBuilding(GAMEOBJ* grp, byte mode, int building)`: keeps the members
/// repairing the building until it is whole or gone (`'REPB'`).
fn sgp_repair_building(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let b = c.i32(2)?;
    let Some(u) = u16::try_from(b).ok().and_then(|i| api.object(i)).filter(|u| u.living() && u.building) else {
        return Ok(0);
    };
    if u.life >= api.world.catalog.get(u.kind).life {
        return Ok(0);
    }
    let order = GroupOrder::Repair { building: b as u16, serial: api.st.serial(u.id) };
    add_order(api, g, c.u8(1)?, order);
    Ok(0)
}

/// `void SGP_TakeResources(GAMEOBJ* grp, byte mode, byte nation, byte res)`: keeps the members
/// gathering `res` near the nation's accepting buildings (`'TRES'`).
fn sgp_take_resources(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let order = GroupOrder::TakeResources { nation: c.u8(2)?, res: c.u8(3)?, last: 0, bases: Vec::new() };
    add_order(api, g, c.u8(1)?, order);
    Ok(0)
}

/// Runs one step of the `'REPB'` process; true when it ends.
pub fn repair_step(api: &mut Api, g: usize, building: u16, serial: u16) -> bool {
    let Some(b) = api.object(building).filter(|b| b.living() && api.st.serial(b.id) == serial) else { return true };
    if b.life >= api.world.catalog.get(b.kind).life {
        return true;
    }
    let bi = b.id;
    for id in api.members(g) {
        let u = &api.world.units[id.0 as usize];
        let building_it = matches!(u.orders.first(), Some(acr_sim::world::Order::Build { .. }));
        if !u.unlimited && !building_it {
            acr_sim::construction::build_obj(api.world, id.0 as usize, bi.0 as usize, 0x90);
        }
    }
    false
}

/// Runs one step of the `'TRES'` process (0x5773e1).
pub fn take_resources_step(api: &mut Api, g: usize, nation: u8, res: u8, last: &mut u64, bases: &mut Vec<(i32, i32)>) {
    let tmtmt = api.world.tick.0 + 1;
    let dt = tmtmt.wrapping_sub(*last);
    if *last == 0 || dt > 64 || api.world.random.rando() < 0x1000 {
        *last = tmtmt;
        bases.clear();
        for u in api.world.units.iter().filter(|u| u.active() && u.owner == nation && u.building) {
            let t = api.world.catalog.get(u.kind);
            if u.stage >= api.world.max_stage(u) && t.economy.resource_base & (1 << res) != 0 && bases.len() < 16 {
                bases.push((u.pos.x, u.pos.y));
            }
        }
    }
    if bases.is_empty() {
        return;
    }
    for id in api.members(g) {
        let u = &api.world.units[id.0 as usize];
        if u.unlimited {
            continue;
        }
        let cur = u.orders.iter().find_map(|o| match o {
            acr_sim::world::Order::Gather { kind, .. } => Some(*kind),
            _ => None,
        });
        if cur == Some(res) {
            continue;
        }
        let k = ((api.world.random.rando() * bases.len() as i32) >> 15) as usize;
        let (cx, cy) = (bases[k].0 >> 4, bases[k].1 >> 4);
        let mut best: Option<(i32, i32, i32)> = None;
        for _ in 0..32 {
            let x = cx + (api.world.random.rando() & 0x7ff) - 0x400;
            let y = cy + (api.world.random.rando() & 0x7ff) - 0x400;
            if api.world.sources.determine(x, y) == Some(res) {
                let d = super::norma(cx - x, cy - y);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, x, y));
                }
            }
        }
        let i = id.0 as usize;
        match best {
            Some((_, x, y)) => {
                acr_sim::gather::take_resource(api.world, i, x, y, res, 0x81, false);
            }
            None if api.world.units[i].orders.is_empty() => {
                let owner = api.world.units[i].owner;
                api.commands.push(acr_sim::Command::Move { player: owner, units: vec![id], x: cx, y: cy });
            }
            None => {}
        }
    }
}

/// `bool IsResourceEnabled(byte nation, word res)`: a source of `res` lies within ±512 px of
/// a finished building of the nation that accepts it (16 random probes per building, 2
/// `rando()` each).
fn is_resource_enabled(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (nat, res) = (c.u8(0)?, c.u16(1)?);
    let mask = 1u32.checked_shl(u32::from(res)).unwrap_or(0) as u16;
    if mask == 0 {
        return Ok(0);
    }
    let bases: Vec<(i32, i32)> = api
        .world
        .units
        .iter()
        .filter(|u| {
            let t = api.world.catalog.get(u.kind);
            u.active()
                && u.owner == nat
                && u.building
                && u.stage >= api.world.max_stage(u)
                && t.economy.resource_base & mask != 0
        })
        .map(|u| (u.pos.x >> 4, u.pos.y >> 4))
        .collect();
    for (bx, by) in bases {
        for _ in 0..16 {
            let x = bx + (api.world.random.rando() & 0x3ff) - 0x200;
            let y = by + (api.world.random.rando() & 0x3ff) - 0x200;
            if api.world.sources.determine(x, y).map(u16::from) == Some(res) {
                return Ok(1);
            }
        }
    }
    Ok(0)
}

/// `void SetToInternalResourcesZero(GAMEOBJ*)`: drops the members' loads and stances and the
/// walk-through-locks flag of units standing on free ground.
fn set_to_internal_resources_zero(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 0)? else { return Ok(0) };
    let ids: Vec<EntityId> =
        api.st.groups[g].members.clone().into_iter().filter_map(|m| api.by_serial_pub(m)).map(|u| u.id).collect();
    for id in ids {
        let u = &mut api.world.units[id.0 as usize];
        u.carried = 0;
        u.carry_type = None;
        u.stance = 0;
        if u.unlimited && u.orders.is_empty() {
            let (cx, cy, lx) = (u.cell.0, u.cell.1, u.lx);
            if !api.world.map.land.check_bar(cx - 1, cy - 1, lx + 2, lx + 2) {
                api.world.units[id.0 as usize].unlimited = false;
            }
        }
    }
    Ok(0)
}

/// `bool CheckIfBuildingIsFired(int index)` (0x57c39e): the object is gone or burning
/// (`InFire`, OB+0x26 bit1).
fn check_if_building_is_fired(api: &mut Api, c: &mut Call) -> Result<u32> {
    let u = u16::try_from(c.i32(0)?).ok().and_then(|i| api.object(i));
    Ok(u32::from(u.is_none_or(|u| u.burning)))
}

/// `void CreateNonFiredEnemyBuildingsTopList(u16* list, byte nation)` (0x57b2fd): per area the
/// last living enemy building (not nation 7) that is not burning, else 0xFFFF.
fn create_non_fired_enemy_buildings_top_list(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (out, nat) = (c.u32(0)?, c.u8(1)?);
    let n = n_areas(api);
    let mut list = vec![0xFFFFu16; n];
    let mask = api.world.nation_mask[usize::from(nat) & 7];
    for u in
        api.world.units.iter().filter(|u| u.building && u.living() && !u.burning && u.owner != 7 && u.nmask & mask == 0)
    {
        let z = usize::from(zone_of_unit(api, u));
        if z < n {
            list[z] = u.id.0 as u16;
        }
    }
    write_list(c, out, &list)?;
    Ok(0)
}
