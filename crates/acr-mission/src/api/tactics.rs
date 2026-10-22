//! "Top zones" (the map's `TOP1` pathfinding areas), danger maps, safe ways, enemy armies and
//! the attack helpers.

use acr_script::runtime::Result;
use acr_sim::Command;
use acr_sim::world::EntityId;

use super::util::{Circle, norma};
use super::{Api, ApiFn, Call, Status};

const fn f(name: &'static str, original: u32, status: Status, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status, handler }
}

const DONE: Status = Status::Done;
const ATTACK: Status = Status::Partial(
    "idle members attack the nearest enemy in the zone (Command::Attack); the original's target rules need combat",
);

pub const FUNCTIONS: &[ApiFn] = &[
    f("GetNZones", 0x579399, DONE, get_n_zones),
    f("GetTopZone", 0x5791cb, DONE, get_top_zone),
    f("GetTopZoneCoor", 0x579229, DONE, get_top_zone_coor),
    f("GetTopZRealCoor", 0x581847, DONE, get_top_z_real_coor),
    f("GetTopDist", 0x579370, DONE, get_top_dist),
    f("GetTopDst", 0x5658c7, DONE, get_top_dst),
    f("GetZonesDist", 0x57931b, DONE, get_zones_dist),
    f("GetNextZone", 0x5792c5, DONE, get_next_zone),
    f("GetListOfNearZones", 0x579281, DONE, get_list_of_near_zones),
    f("FindNextZoneOnTheSafeWay", 0x579414, DONE, find_next_zone_on_the_safe_way),
    f(
        "CreateDangerMap",
        0x57c3ea,
        Status::Partial("every object counts as category 0 (no CATEGORY in the catalog); no garrisons/resource bases"),
        create_danger_map,
    ),
    f(
        "CreateDangerMapForDef",
        0x57c7c0,
        Status::Partial("STORMFORCE not in the catalog: every enemy unit adds 1"),
        create_danger_map_for_def,
    ),
    f("CreateDangerMapForTom", 0x57c9ee, Status::Partial("as CreateDangerMap"), create_danger_map),
    f(
        "EnumEnemyArmies",
        0x578245,
        Status::Partial("one army per enemy nation (centre, count, force = count); the original clusters 256-px cells"),
        enum_enemy_armies,
    ),
    f("Attack3", 0x57ea15, ATTACK, attack_in_zone),
    f("Attack4", 0x580042, ATTACK, attack_in_zone),
    f("Attack6", 0x580b71, ATTACK, attack_in_zone),
    f("AttackByTomahawks", 0x574667, ATTACK, attack_in_zone),
    f("AttackEnemyInZone2", 0x57dfb9, ATTACK, attack_in_zone_no_count),
    f("AttackEnemyInZone", 0x5642b7, ATTACK, attack_enemy_in_zone),
    f("AttackBuildingsInZone", 0x5617df, ATTACK, attack_buildings_in_zone),
    f("AttackGrpByGrp", 0x57385f, ATTACK, attack_grp_by_grp),
    f("GrpAttackObject", 0x570d0c, ATTACK, grp_attack_object),
    f("SGP_AttackByArchers", 0x5790f3, ATTACK, sgp_attack_by_archers),
];

/// Area centre in area cells.
pub fn area_center(api: &Api, area: u16) -> (i32, i32) {
    api.st
        .topo
        .as_ref()
        .and_then(|t| t.areas.get(usize::from(area)))
        .map_or((0, 0), |a| (i32::from(a.x as i16), i32::from(a.y as i16)))
}

fn n_areas(api: &Api) -> usize {
    api.st.topo.as_ref().map_or(0, |t| t.areas.len())
}

/// `TopZoneAt(cx, cy)` (0x4271c0; the y bound is checked against the width, as in the original).
fn zone_at(api: &Api, cx: i32, cy: i32) -> u16 {
    let Some(t) = &api.st.topo else { return 0xFFFF };
    let w = t.cells.width as i32;
    if cx < 0 || cy < 0 || cx >= w || cy >= w {
        return 0xFFFF;
    }
    t.cells.get(cx as usize, cy as usize).unwrap_or(0xFFFF)
}

fn next(api: &Api, a: u16, b: u16) -> u16 {
    let n = n_areas(api);
    api.st.topo.as_ref().and_then(|t| t.next_hop.get(usize::from(a) * n + usize::from(b))).copied().unwrap_or(0xFFFF)
}

fn dist(api: &Api, a: u16, b: u16) -> u16 {
    let n = n_areas(api);
    api.st.topo.as_ref().and_then(|t| t.distance.get(usize::from(a) * n + usize::from(b))).copied().unwrap_or(0xFFFF)
}

/// `TopDist` (0x43c7d8) between two area-grid cells.
pub fn top_dist(api: &Api, x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
    const OFF: [(i32, i32); 8] = [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    let find = |x: i32, y: i32| {
        let a = zone_at(api, x, y);
        if a != 0xFFFF {
            return a;
        }
        OFF.iter().map(|(dx, dy)| zone_at(api, x + dx, y + dy)).find(|&a| a != 0xFFFF).unwrap_or(0xFFFF)
    };
    let (a1, a2) = (find(x1, y1), find(x2, y2));
    if a1 >= 0xFFFE || a2 >= 0xFFFE {
        return 0xFFFF;
    }
    let direct = norma(x1 - x2, y1 - y2);
    if a1 == a2 {
        return direct;
    }
    let n1 = next(api, a1, a2);
    if n1 == 0xFFFF {
        return 0xFFFF;
    }
    if n1 == a2 {
        return direct;
    }
    let n2 = next(api, a2, a1);
    if n2 == 0xFFFF {
        return 0xFFFF;
    }
    if n2 == a1 {
        return direct;
    }
    let (c1, c2) = (area_center(api, n1), area_center(api, n2));
    let mut r = norma(x1 - c1.0, y1 - c1.1) + norma(x2 - c2.0, y2 - c2.1);
    if n1 != n2 {
        r += i32::from(dist(api, n1, n2));
    }
    r
}

fn get_n_zones(api: &mut Api, _: &mut Call) -> Result<u32> {
    Ok(n_areas(api) as u32)
}

/// `int GetTopZone(int x, int y)` (pixels): the area of the 64-px cell, 0xFFFF = none.
fn get_top_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(t) = &api.st.topo else { return Ok(0xFFFF) };
    let (cx, cy) = (c.i32(0)? >> 6, c.i32(1)? >> 6);
    if cx < 0 || cy < 0 || cx >= t.cells.width as i32 || cy >= t.cells.height as i32 {
        return Ok(0xFFFF);
    }
    Ok(u32::from(t.cells.get(cx as usize, cy as usize).unwrap_or(0xFFFF)))
}

fn zone_coor(api: &mut Api, c: &mut Call, shift: u32) -> Result<u32> {
    let z = c.i32(0)?;
    if z < 0 || z as usize >= n_areas(api) {
        c.out_i32(1, 0)?;
        c.out_i32(2, 0)?;
        return Ok(0);
    }
    let (x, y) = area_center(api, z as u16);
    c.out_i32(1, x << shift)?;
    c.out_i32(2, y << shift)?;
    Ok(1)
}

/// `bool GetTopZoneCoor(int zone, int* x, int* y)`: centre in area cells.
fn get_top_zone_coor(api: &mut Api, c: &mut Call) -> Result<u32> {
    zone_coor(api, c, 0)
}

/// `bool GetTopZRealCoor(int zone, int* x, int* y)`: centre cell corner in pixels.
fn get_top_z_real_coor(api: &mut Api, c: &mut Call) -> Result<u32> {
    zone_coor(api, c, 6)
}

fn get_top_dist(api: &mut Api, c: &mut Call) -> Result<u32> {
    Ok(top_dist(api, c.i32(0)? >> 6, c.i32(1)? >> 6, c.i32(2)? >> 6, c.i32(3)? >> 6) as u32)
}

/// `int GetTopDst(GAMEOBJ* a, GAMEOBJ* b)`: `TopDist` between two zone centres.
fn get_top_dst(api: &mut Api, c: &mut Call) -> Result<u32> {
    let mut pts = [(0, 0); 2];
    for (i, p) in pts.iter_mut().enumerate() {
        let z = if c.u32(i as u32)? == 0 { None } else { api.zone_point(&c.handle(i as u32)?) };
        match z {
            Some(z) => *p = z,
            None => {
                api.st.error(format!("GetTopDst : Invalid parameter <Z{}>", i + 1));
                return Ok(0xFFFF);
            }
        }
    }
    Ok(top_dist(api, pts[0].0 >> 6, pts[0].1 >> 6, pts[1].0 >> 6, pts[1].1 >> 6) as u32)
}

fn valid_zone(api: &Api, z: i32) -> Option<u16> {
    (z >= 0 && (z as usize) < n_areas(api)).then_some(z as u16)
}

fn get_zones_dist(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(a), Some(b)) = (valid_zone(api, c.i32(0)?), valid_zone(api, c.i32(1)?)) else { return Ok(0xFFFF) };
    Ok(if a == b { 0 } else { u32::from(dist(api, a, b)) })
}

fn get_next_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(a), Some(b)) = (valid_zone(api, c.i32(0)?), valid_zone(api, c.i32(1)?)) else { return Ok(0xFFFF) };
    Ok(if a == b { u32::from(b) } else { u32::from(next(api, a, b)) })
}

/// `int GetListOfNearZones(int zone, u16** links)`: the link array `(area, dist)` pairs, copied
/// once into the calling DLL's memory (the original hands out a pointer into engine memory).
fn get_list_of_near_zones(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(z) = valid_zone(api, c.i32(0)?) else { return Ok(0) };
    let links: Vec<(u16, u16)> =
        api.st.topo.as_ref().map(|t| t.areas[usize::from(z)].links.clone()).unwrap_or_default();
    let key = (api.vm, format!("links{z}").into_bytes());
    let p = match api.st.text_cache.get(&key) {
        Some(&p) => p,
        None => {
            let bytes: Vec<u8> = links.iter().flat_map(|(a, d)| [a.to_le_bytes(), d.to_le_bytes()].concat()).collect();
            let p = c.alloc_bytes(&bytes)?;
            api.st.text_cache.insert(key, p);
            p
        }
    };
    c.out_u32(1, p)?;
    Ok(links.len() as u32)
}

fn links(api: &Api, a: u16) -> Vec<(u16, u16)> {
    api.st.topo.as_ref().and_then(|t| t.areas.get(usize::from(a))).map(|r| r.links.clone()).unwrap_or_default()
}

/// `int FindNextZoneOnTheSafeWay(int start, int fin, short* danger, int* maxDanger, int
/// maxSteps)`: Dijkstra from `fin` with cost = link distance + danger of the area entered;
/// returns the next area from `start` (0xFFFF if none).
fn find_next_zone_on_the_safe_way(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (start, fin) = (c.i32(0)?, c.i32(1)?);
    let (Some(s), Some(fz)) = (valid_zone(api, start), valid_zone(api, fin)) else { return Ok(0xFFFF) };
    if s != fz && dist(api, s, fz) == 0xFFFF {
        return Ok(0xFFFF);
    }
    let danger_ptr = c.u32(2)?;
    let n = n_areas(api).min(0x800);
    let danger = |c: &Call, a: u16| -> Result<i32> { Ok(i32::from(c.read_u16(danger_ptr + 2 * u32::from(a))? as i16)) };
    let mut closed = vec![0xFFFFu16; n];
    let mut cost = vec![0xFFFFu16; n];
    let mut parent = vec![0xFFFFu16; n];
    // Sorted by cost, descending: the last entry is the cheapest.
    let mut queue: Vec<u16> = vec![fz];
    cost[usize::from(fz)] = 0;
    while let Some(cur) = queue.pop() {
        let ci = usize::from(cur);
        closed[ci] = cost[ci];
        cost[ci] = 0xFFFF;
        if cur == s {
            let mut maxd = danger(c, cur)?;
            let mut z = cur;
            for _ in 0..c.i32(4)? {
                if z == 0xFFFF {
                    break;
                }
                z = parent[usize::from(z)];
                if z != 0xFFFF {
                    maxd = maxd.max(danger(c, z)?);
                }
            }
            c.out_i32(3, maxd)?;
            return Ok(u32::from(parent[usize::from(s)]));
        }
        for (a, d) in links(api, cur) {
            let ai = usize::from(a);
            if ai >= n || closed[ai] != 0xFFFF {
                continue;
            }
            let nc = i32::from(d) + i32::from(closed[ci]) + danger(c, a)?;
            if cost[ai] != 0xFFFF {
                if i32::from(cost[ai]) > nc {
                    queue.retain(|&q| q != a);
                } else {
                    continue;
                }
            }
            let key = nc as u16;
            // Binary insertion as in the original (ties go behind cheaper entries).
            let pos = queue.partition_point(|&q| i32::from(cost[usize::from(q)]) > nc);
            cost[ai] = key;
            queue.insert(pos, a);
            parent[ai] = cur;
        }
    }
    Ok(0xFFFF)
}

/// The area of a 64-px cell, for other modules.
pub fn zone_of(api: &Api, cx: i32, cy: i32) -> u16 {
    zone_at(api, cx, cy)
}

/// `CreateDangerMap` for the DipServer variants.
pub fn danger_map_pub(api: &mut Api, c: &mut Call) -> Result<u32> {
    danger_map(api, c, false)
}

/// Danger map of `CreateDangerMap` with a per-object term.
fn danger_map(api: &mut Api, c: &mut Call, def: bool) -> Result<u32> {
    let nat = c.u8(0)? & 7;
    let map = c.u32(1)?;
    let nzones = c.i32(2)?.max(0) as usize;
    let weights = c.u32(3)?;
    let steps = c.i32(4)?;
    let mask = api.st.nmask[usize::from(nat)];
    let w0 = if def || weights == 0 { 0 } else { c.read_u32(weights)? as i32 };
    let mut m = vec![0i32; nzones];
    for u in api.world.units.iter().filter(|u| u.living()) {
        let z = zone_at(api, u.pos.x >> 10, u.pos.y >> 10);
        let Some(v) = m.get_mut(usize::from(z)) else { continue };
        let enemy = api.st.unit_nmask(u.id, u.owner) & mask == 0;
        let t = api.world.catalog.get(u.kind);
        *v = i32::from(if def {
            if enemy && !u.building { (*v + 1) as i16 } else { *v as i16 }
        } else if enemy {
            if u.owner == 7 && u.building {
                (*v + (w0 << 7)) as i16
            } else if t.peasant {
                (*v - w0 * 2) as i16
            } else {
                (*v + w0) as i16
            }
        } else {
            (*v - w0) as i16
        });
    }
    if steps > 0 {
        for _ in 0..steps {
            for i in 0..nzones {
                let add = m[i] / (2 * steps);
                for (a, _) in links(api, i as u16) {
                    if let Some(x) = m.get_mut(usize::from(a)) {
                        *x = i32::from((*x + add) as i16);
                    }
                }
            }
        }
    }
    for (i, v) in m.iter().enumerate() {
        let v = if def { *v } else { (*v).clamp(-6, 30000) };
        c.write_u16(map + 2 * i as u32, v as i16 as u16)?;
    }
    Ok(0)
}

/// `void CreateDangerMap(byte nat, short* map, int nzones, int* weight, int steps)`.
fn create_danger_map(api: &mut Api, c: &mut Call) -> Result<u32> {
    danger_map(api, c, false)
}

fn create_danger_map_for_def(api: &mut Api, c: &mut Call) -> Result<u32> {
    danger_map(api, c, true)
}

/// `int EnumEnemyArmies(byte nat, int fields, int* buf, int len)`.
fn enum_enemy_armies(api: &mut Api, c: &mut Call) -> Result<u32> {
    let nat = c.u8(0)?;
    if nat >= 7 {
        return Ok(0);
    }
    let (fields, buf, len) = (c.i32(1)?, c.u32(2)?, c.i32(3)?);
    let mask = api.st.nmask[usize::from(nat)];
    let mut armies: Vec<(i64, i64, i32)> = Vec::new();
    for n in 0..8u8 {
        let units: Vec<_> = api.world.units.iter().filter(|u| u.living() && u.owner == n && !u.building).collect();
        if units.is_empty() || api.st.nmask[usize::from(n)] & mask != 0 {
            continue;
        }
        let k = units.len() as i64;
        let x = units.iter().map(|u| i64::from(u.pos.x >> 4)).sum::<i64>() / k;
        let y = units.iter().map(|u| i64::from(u.pos.y >> 4)).sum::<i64>() / k;
        armies.push((x, y, units.len() as i32));
    }
    let mut k = 0i32;
    let put = |c: &mut Call, v: i32, k: &mut i32| -> Result<()> {
        c.write_i32(buf + 4 * *k as u32, v)?;
        *k += 1;
        Ok(())
    };
    for (x, y, n) in &armies {
        if k < len && fields & 1 != 0 {
            put(c, *x as i32, &mut k)?;
            put(c, *y as i32, &mut k)?;
        }
        if k < len && fields & 2 != 0 {
            put(c, *n, &mut k)?;
        }
        if k < len && fields & 4 != 0 {
            put(c, *n, &mut k)?;
        }
        if k < len && fields & 8 != 0 {
            for v in [(*x >> 8) as i32, (*y >> 8) as i32, (*x >> 8) as i32, (*y >> 8) as i32] {
                put(c, v, &mut k)?;
            }
        }
    }
    let size =
        2 * (fields & 1) + i32::from(fields & 2 != 0) + i32::from(fields & 4 != 0) + 4 * i32::from(fields & 8 != 0);
    Ok(if size > 0 { (k / size) as u32 } else { 0 })
}

/// Enemies of `nat` (by alliance mask) inside the circle, as ids.
fn enemies_in(api: &Api, z: &Circle, nat: u8) -> Vec<EntityId> {
    let mask = api.st.nmask[usize::from(nat & 7)];
    api.world
        .units
        .iter()
        .filter(|u| u.living() && api.st.unit_nmask(u.id, u.owner) & mask == 0 && z.contains(u))
        .map(|u| u.id)
        .collect()
}

/// Sends every idle living member of `g` (owned by `nat`) after the nearest of `targets`.
fn attack_nearest(api: &mut Api, g: usize, targets: &[EntityId]) {
    let members = api.members(g);
    let mut orders: Vec<(u8, EntityId, EntityId)> = Vec::new();
    for id in members {
        let u = &api.world.units[id.0 as usize];
        if !u.idle() || u.building {
            continue;
        }
        let best = targets.iter().min_by_key(|t| {
            let e = &api.world.units[t.0 as usize];
            norma(u.pos.x - e.pos.x, u.pos.y - e.pos.y)
        });
        if let Some(&t) = best {
            orders.push((u.owner, id, t));
        }
    }
    for (owner, id, target) in orders {
        api.commands.push(Command::Attack { player: owner, units: vec![id], target });
    }
}

/// Group index without validation beyond the range (Attack3/4/6 do not check the handle).
fn raw_group(api: &Api, c: &Call, i: u32) -> Result<Option<usize>> {
    if c.u32(i)? == 0 {
        return Ok(None);
    }
    let g = usize::from(c.handle(i)?.index);
    Ok((g < api.st.groups.len()).then_some(g))
}

/// `bool Attack3/4/6/AttackByTomahawks(GAMEOBJ* grp, GAMEOBJ* zone, byte nat, int* n)`.
fn attack_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    c.out_i32(3, 0)?;
    attack_in_zone_no_count(api, c)
}

fn attack_in_zone_no_count(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = raw_group(api, c, 0)? else { return Ok(0) };
    let zone = if c.u32(1)? == 0 { None } else { api.zone_circles(&c.handle(1)?) };
    let Some(z) = zone.and_then(|z| z.first().copied()) else { return Ok(0) };
    let targets = enemies_in(api, &z, c.u8(2)?);
    if targets.is_empty() {
        return Ok(0);
    }
    attack_nearest(api, g, &targets);
    Ok(1)
}

/// `void AttackEnemyInZone(GAMEOBJ* grp, GAMEOBJ* zone, byte enemyNat)`: targets are the
/// living objects of `enemyNat` in the zone (alliances are not checked).
fn attack_enemy_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(z) = api.zone_arg(c, 1, "AttackEnemyInZone")?.and_then(|z| z.first().copied()) else { return Ok(0) };
    let Some(g) = raw_group(api, c, 0)? else { return Ok(0) };
    let en = c.u8(2)?;
    let targets: Vec<EntityId> =
        api.world.units.iter().filter(|u| u.living() && u.owner == en && z.contains(u)).map(|u| u.id).collect();
    attack_nearest(api, g, &targets);
    Ok(0)
}

/// `void AttackBuildingsInZone(GAMEOBJ* grp, GAMEOBJ* zone, byte nat)`: the original ignores
/// zone and nation and attacks random targets near each unit; ACR takes the nearest enemy
/// building of each member.
fn attack_buildings_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 0, "AttackBuildingsInZone")? else { return Ok(0) };
    if api.zone_arg(c, 1, "AttackBuildingsInZone")?.is_none() {
        return Ok(0);
    }
    let Some(&first) = api.members(g).first() else { return Ok(0) };
    let owner = api.world.units[first.0 as usize].owner;
    let mask = api.st.unit_nmask(first, owner);
    let targets: Vec<EntityId> = api
        .world
        .units
        .iter()
        .filter(|u| u.living() && u.building && api.st.unit_nmask(u.id, u.owner) & mask == 0)
        .map(|u| u.id)
        .collect();
    attack_nearest(api, g, &targets);
    Ok(0)
}

fn attack_grp_by_grp(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(a), Some(v)) = (raw_group(api, c, 0)?, raw_group(api, c, 1)?) else { return Ok(0) };
    let targets = api.members(v);
    attack_nearest(api, a, &targets);
    Ok(1)
}

/// `bool GrpAttackObject(byte nat, GAMEOBJ* grp, int target)`.
fn grp_attack_object(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = raw_group(api, c, 1)? else { return Ok(0) };
    let t = c.i32(2)?;
    let Some(target) = u16::try_from(t).ok().and_then(|t| api.object(t)).filter(|u| u.living()).map(|u| u.id) else {
        return Ok(0);
    };
    attack_nearest(api, g, &[target]);
    Ok(1)
}

/// `bool SGP_AttackByArchers(byte nat, GAMEOBJ* grp, int, int)`: ACR attacks the nearest enemy
/// of the group once (the original installs a per-tick target search, `'ARCA'`).
fn sgp_attack_by_archers(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 1)? else { return Ok(0) };
    let ids = api.members(g);
    if ids.is_empty() {
        return Ok(0);
    }
    let n = ids.len() as i64;
    let x = (ids.iter().map(|id| i64::from(api.world.units[id.0 as usize].pos.x >> 4)).sum::<i64>() / n) as i32;
    let y = (ids.iter().map(|id| i64::from(api.world.units[id.0 as usize].pos.y >> 4)).sum::<i64>() / n) as i32;
    let targets = enemies_in(api, &Circle { x, y, r: 960 }, c.u8(0)?);
    attack_nearest(api, g, &targets);
    Ok(1)
}
