//! "Top zones" (the map's `TOP1` pathfinding areas), danger maps, safe ways, enemy armies and
//! the attack helpers.

use acr_script::runtime::Result;

use super::util::{Circle, norma};
use super::{Api, ApiFn, Call, Status};

const fn f(name: &'static str, original: u32, status: Status, handler: super::Handler) -> ApiFn {
    ApiFn { name, original, status, handler }
}

const DONE: Status = Status::Done;

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
    f("CreateDangerMap", 0x57c3ea, DONE, create_danger_map),
    f("CreateDangerMapForDef", 0x57c7c0, DONE, create_danger_map_for_def),
    f("CreateDangerMapForTom", 0x57c9ee, DONE, create_danger_map_for_tom),
    f("EnumEnemyArmies", 0x578245, DONE, enum_enemy_armies),
    f("Attack3", 0x57ea15, DONE, attack3),
    f("Attack4", 0x580042, DONE, attack4),
    f("Attack6", 0x580b71, DONE, attack3),
    f("AttackByTomahawks", 0x574667, DONE, attack_by_tomahawks),
    f("AttackEnemyInZone2", 0x57dfb9, DONE, attack_enemy_in_zone2),
    f(
        "AttackEnemyInZone",
        0x5642b7,
        Status::Partial("walls are not modelled (their pass is skipped)"),
        attack_enemy_in_zone,
    ),
    f("AttackBuildingsInZone", 0x5617df, DONE, attack_buildings_in_zone),
    f("AttackGrpByGrp", 0x57385f, DONE, attack_grp_by_grp),
    f("GrpAttackObject", 0x570d0c, DONE, grp_attack_object),
    f(
        "SGP_AttackByArchers",
        0x5790f3,
        Status::Partial("FindArcherTarget does not score walls (no wall types are loaded)"),
        sgp_attack_by_archers,
    ),
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
    Ok(top_zone_px(api, c.i32(0)?, c.i32(1)?))
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

/// The per-object term of a danger map (`CreateDangerMap*`, 0x57c3ea..0x57daac; the rest of
/// the functions is shared).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Danger {
    /// `CreateDangerMap` (0x57c3ea).
    Plain,
    /// `CreateDangerMapForDef` (0x57c7c0): enemy units only, by `STORMFORCE`; no clamp.
    Def,
    /// `CreateDangerMapForTom` (0x57c9ee).
    Tom,
    /// `CreateDangerMapForFire` (0x57d6c4).
    Fire,
    /// `CreateDangerMapWithoutPeasants` (0x57daac).
    NoPeasants,
    /// `CreateDangerMapForStorm` (0x57cd88).
    Storm,
    /// `CreateDangerMapForStormHard` (0x57d19e).
    StormHard,
}

/// `GetTopZone` (0x5791cb) of a pixel point: the area of its 64 px cell, 0xFFFF outside.
pub fn top_zone_px(api: &Api, x: i32, y: i32) -> u32 {
    let Some(t) = &api.st.topo else { return 0xFFFF };
    let (cx, cy) = (x >> 6, y >> 6);
    if cx < 0 || cy < 0 || cx >= t.cells.width as i32 || cy >= t.cells.height as i32 {
        return 0xFFFF;
    }
    u32::from(t.cells.get(cx as usize, cy as usize).unwrap_or(0xFFFF))
}

/// `bool GetBuildingOposit(word index, int* value)` (0x57b9ae): a living building's defence,
/// `n · 2 · (a + (b − a) · n / cap)` for `OPPOSIT a b`, `n` units inside and the capacity
/// `cap` (the doubling is the engine comparing two field addresses, always true); `None`
/// (value 0) for anything else or no capacity.
pub fn building_oposit(api: &Api, index: u16) -> Option<i32> {
    let u = api.world.units.get(usize::from(index)).filter(|u| u.active())?;
    if !u.building {
        return None;
    }
    let (a, b) = api.world.catalog.get(u.kind).role.opposit;
    let n = u.inside.len() as i32;
    let cap = api.world.adv(u.owner, u.kind).max_inside & 0xFFFF;
    if cap == 0 {
        return None;
    }
    Some(n * ((a + (b - a) * n / cap) * 2))
}

/// `void CreateDangerMap*(byte nat, short* map, int nzones, int* weight, int steps)`: every
/// object adds its term — scaled by `weight[CATEGORY]` — to the value (a `short`) of its area
/// (`GetTopZone` of its position, areas below `nzones`); then `steps` rounds spread each
/// area's `value / (2 · steps)` to its linked areas, in place and in area order; at the end
/// the values are clamped to [−6, 30000] (not by `ForDef`).
pub fn danger_map(api: &mut Api, c: &mut Call, kind: Danger) -> Result<u32> {
    let mask = api.st.nmask[usize::from(c.u8(0)? & 7)];
    let map = c.u32(1)?;
    let nzones = c.i32(2)?;
    let weights = c.u32(3)?;
    let steps = c.i32(4)?;
    let mut m = vec![0i16; nzones.max(0) as usize];
    for i in 0..api.world.units.len() {
        let u = &api.world.units[i];
        // Units inside have `Sdoxlo` 1 in the engine.
        if !u.active() || (kind == Danger::Def && u.building) {
            continue;
        }
        let z = top_zone_px(api, u.pos.x >> 4, u.pos.y >> 4) as i32;
        if z >= nzones {
            continue;
        }
        let t = api.world.catalog.get(u.kind);
        let w = if weights == 0 || kind == Danger::Def {
            0
        } else {
            c.read_u32(weights + 4 * u32::from(t.role.category))? as i32
        };
        let enemy = api.st.unit_nmask(u.id, u.owner) & mask == 0;
        let n = u.inside.len() as i32;
        let kills = i32::from(u.kills);
        let mine = t.role.usage == acr_sim::catalog::usage::MINE;
        let res = t.economy.resource_base != 0;
        let nat7 = u.owner == 7 && u.building;
        let v = i32::from(m[z as usize]);
        let v = match kind {
            Danger::Def if enemy => v + (((t.role.storm_force & 0xff) >> 5) | 1),
            Danger::Def => v,
            _ if !enemy => v.wrapping_sub(w),
            Danger::StormHard => {
                if nat7 {
                    v.wrapping_add(w << 8)
                } else if u.building {
                    let mut v = if mine {
                        v.wrapping_sub(w.wrapping_mul(n))
                    } else {
                        v.wrapping_add(w.wrapping_mul(building_oposit(api, i as u16).unwrap_or(0)))
                    };
                    if res {
                        v = v.wrapping_sub(w << 4);
                    }
                    v
                } else if t.peasant {
                    v
                } else {
                    v.wrapping_add(w)
                }
            }
            _ if nat7 => v.wrapping_add(if kind == Danger::Plain { w << 7 } else { w << 8 }),
            _ if res => v.wrapping_sub(w << 8),
            _ if n != 0 && mine => v.wrapping_sub(w.wrapping_mul(n) << if kind == Danger::Plain { 4 } else { 2 }),
            _ if n != 0 => {
                let f = match kind {
                    Danger::Fire => n + kills - ((u.max_life & 0xFFFF) >> 10),
                    Danger::Storm => {
                        // `n² · b / (2 · MaxInside) + kills − 350` (`OPPOSIT a b`).
                        let cap = 2 * (t.economy.max_inside & 0xFFFF);
                        n.wrapping_mul(n).wrapping_mul(t.role.opposit.1).checked_div(cap).unwrap_or(0) + kills - 350
                    }
                    _ => n + kills,
                };
                let f = if kind == Danger::Tom { f.wrapping_mul(8) } else { f };
                v.wrapping_add(w.wrapping_mul(f))
            }
            Danger::Tom => v.wrapping_sub(w),
            _ if t.peasant => v.wrapping_sub(if kind == Danger::Storm { w } else { w * 2 }),
            _ => v.wrapping_add(w),
        };
        m[z as usize] = v as i16;
    }
    for _ in 0..steps.max(0) {
        for i in 0..m.len() {
            let add = i32::from(m[i]) / (2 * steps);
            for (a, _) in links(api, i as u16) {
                if let Some(x) = m.get_mut(usize::from(a)) {
                    *x = (i32::from(*x) + add) as i16;
                }
            }
        }
    }
    for (i, v) in m.iter().enumerate() {
        let v = if kind == Danger::Def { *v } else { (*v).clamp(-6, 30000) };
        c.write_u16(map + 2 * i as u32, v as u16)?;
    }
    Ok(0)
}

fn create_danger_map(api: &mut Api, c: &mut Call) -> Result<u32> {
    danger_map(api, c, Danger::Plain)
}

fn create_danger_map_for_def(api: &mut Api, c: &mut Call) -> Result<u32> {
    danger_map(api, c, Danger::Def)
}

fn create_danger_map_for_tom(api: &mut Api, c: &mut Call) -> Result<u32> {
    danger_map(api, c, Danger::Tom)
}

/// `int EnumEnemyArmies(byte nat, int fields, int* buf, int len)` (0x578245): per army of the
/// nation's `EnemyInfo` (`acr_sim::armies`), `fields & 1`: x, y (pixels), `& 2`: unit count,
/// `& 4`: force, `& 8`: min x, min y, max x, max y (256 px cells); a field is written whole
/// when it starts below `len`, as in the engine. Returns the entries written / their size.
fn enum_enemy_armies(api: &mut Api, c: &mut Call) -> Result<u32> {
    let nat = c.u8(0)?;
    let Some(armies) = api.world.armies.of(nat).map(<[_]>::to_vec) else { return Ok(0) };
    let (fields, buf, len) = (c.i32(1)?, c.u32(2)?, c.i32(3)?);
    let mut out: Vec<i32> = Vec::new();
    for a in &armies {
        if fields & 1 != 0 && (out.len() as i32) < len {
            out.extend([i32::from(a.x), i32::from(a.y)]);
        }
        if fields & 2 != 0 && (out.len() as i32) < len {
            out.push(i32::from(a.count));
        }
        if fields & 4 != 0 && (out.len() as i32) < len {
            out.push(i32::from(a.force));
        }
        if fields & 8 != 0 && (out.len() as i32) < len {
            out.extend(a.bounds.map(i32::from));
        }
    }
    for (j, &v) in out.iter().enumerate() {
        c.write_i32(buf.wrapping_add(4 * j as u32), v)?;
    }
    let k = out.len() as i32;
    let size =
        2 * (fields & 1) + i32::from(fields & 2 != 0) + i32::from(fields & 4 != 0) + 4 * i32::from(fields & 8 != 0);
    Ok(if size > 0 { (k / size) as u32 } else { 0 })
}

/// The alliance mask of a nation as the scripts see it (`NMASK(nat)`).
fn nat_mask(api: &Api, nat: u8) -> u8 {
    api.st.nmask[usize::from(nat & 7)]
}

/// A member's combat data: maximum and minimum range, the reach of index 1 (0 for melee
/// units), its own kill mask and its medium (`LockType`).
struct Fighter {
    max_r: i32,
    min_r: i32,
    det2_1: i32,
    kill: u8,
    water: bool,
}

fn fighter(api: &Api, u: &acr_sim::Unit) -> Fighter {
    let adv = api.world.adv(u.owner, u.kind);
    let (min_r, max_r) = acr_sim::combat::range_bounds(&adv);
    let t = api.world.catalog.get(u.kind);
    Fighter {
        max_r,
        min_r,
        det2_1: adv.attacks[1].detect.1,
        kill: t.attacks.iter().fold(0, |m, a| m | a.mask),
        water: t.medium == acr_sim::catalog::Medium::Water,
    }
}

fn material(api: &Api, k: usize) -> u8 {
    api.world.catalog.get(api.world.units[k].kind).materials
}

fn is_water(api: &Api, k: usize) -> bool {
    api.world.catalog.get(api.world.units[k].kind).medium == acr_sim::catalog::Medium::Water
}

/// `TopDist` between two objects (64 px area cells).
fn obj_top_dist(api: &Api, a: usize, b: usize) -> i32 {
    let (pa, pb) = (api.world.units[a].pos, api.world.units[b].pos);
    top_dist(api, pa.x >> 10, pa.y >> 10, pb.x >> 10, pb.y >> 10)
}

fn obj_dist(api: &Api, a: usize, b: usize) -> i32 {
    let (pa, pb) = (api.world.units[a].pos, api.world.units[b].pos);
    norma(pa.x - pb.x, pa.y - pb.y) >> 4
}

/// Which building of a cell `Building(cell, mask)` (0x56144f) reports.
#[derive(Clone, Copy)]
enum CellBuilding {
    /// Attack3: a building of the cell that is not one of the nation's friends'.
    NotFriend,
    /// Attack4/6: a building whose mask has a bit outside the nation's.
    Outside,
    None,
}

/// `EnemyScan` (E_tactics 0.4): the living enemies of `nat` in the 128 px cells of the ring
/// tables around the zone, within its radius and a path length of `(R16 >> 9) + 3`, at most
/// 1600; with a building rule, also one building per scanned cell.
fn enemy_scan(api: &Api, z: &Circle, nat: u8, bld: CellBuilding) -> Vec<usize> {
    let mask = nat_mask(api, nat);
    let (cx16, cy16, r16) = (z.x << 4, z.y << 4, z.r << 4);
    let (ccx, ccy) = (cx16 >> 11, cy16 >> 11);
    let side = api.world.buckets.side();
    let mut out: Vec<usize> = Vec::new();
    for r in 0..=((r16 >> 11) + 1) as usize {
        for &(a, b) in acr_sim::morale::ring(r) {
            let (x, y) = (ccx + a, ccy + b);
            if x < 0 || y < 0 || x >= side - 1 || y >= side - 1 {
                continue;
            }
            for &k in api.world.buckets.get(x, y) {
                let k = k as usize;
                let u = &api.world.units[k];
                if out.len() < 1600
                    && u.living()
                    && api.st.unit_nmask(u.id, u.owner) & mask == 0
                    && norma(u.pos.x - cx16, u.pos.y - cy16) < r16
                    && top_dist(api, u.pos.x >> 10, u.pos.y >> 10, cx16 >> 10, cy16 >> 10) < (r16 >> 9) + 3
                {
                    out.push(k);
                }
            }
            let cell: &[u32] = api.world.buckets.buildings_in(x, y);
            let living = |k: &&u32| api.world.units[**k as usize].living();
            let pick = match bld {
                CellBuilding::NotFriend => {
                    let any = cell.iter().find(living);
                    let own = cell.iter().filter(living).find(|&&k| api.world.units[k as usize].nmask & mask != 0);
                    any.filter(|_| any != own)
                }
                CellBuilding::Outside => {
                    cell.iter().filter(living).find(|&&k| api.world.units[k as usize].nmask & !mask != 0)
                }
                CellBuilding::None => None,
            };
            if let Some(&b) = pick
                && out.len() < 1600
                && !out.contains(&(b as usize))
            {
                out.push(b as usize);
            }
        }
    }
    out
}

/// `ClearOrders` (0x4efad1).
pub(super) fn clear_orders(api: &mut Api, i: usize) {
    let u = &mut api.world.units[i];
    u.orders.clear();
    u.path.clear();
    u.prio = 0;
}

/// The attack order just given runs at priority 0xF0 (`PrioryLevel = order prio = 0xF0`).
fn prio_f0(api: &mut Api, i: usize) {
    let u = &mut api.world.units[i];
    u.prio = 0xF0;
    if let Some(acr_sim::world::Order::Attack { prio, .. }) = u.orders.last_mut() {
        *prio = 0xF0;
    }
}

/// `ClearBOrders` (0x415cca) of a member's brigade.
pub(super) fn clear_brigade_orders(api: &mut Api, i: usize) {
    if let Some(b) = api.world.units[i].brigade {
        api.world.brigades[usize::from(b)].orders.clear();
    }
}

fn attack_obj(api: &mut Api, i: usize, t: usize, prio: u8, p: acr_sim::combat::Placement) -> bool {
    acr_sim::combat::attack_obj_ex(api.world, i, t, prio, p)
}

/// `AttackObj` with `nTimes` shots or strikes (0 = until the target is dead).
fn attack_obj_n(api: &mut Api, i: usize, t: usize, prio: u8, p: acr_sim::combat::Placement, times: u16) -> bool {
    acr_sim::combat::attack_obj_times(api.world, i, t, prio, p, times)
}

/// The living members of a group (serial checked), as unit indices.
fn living_members(api: &Api, g: usize) -> Vec<usize> {
    api.members(g).into_iter().map(|e| e.0 as usize).collect()
}

/// Group index without validation beyond the range (Attack3/4/6 do not check the handle).
fn raw_group(api: &Api, c: &Call, i: u32) -> Result<Option<usize>> {
    if c.u32(i)? == 0 {
        return Ok(None);
    }
    let g = usize::from(c.handle(i)?.index);
    Ok((g < api.st.groups.len()).then_some(g))
}

fn zone_of_call(api: &Api, c: &Call, i: u32) -> Result<Option<Circle>> {
    Ok(if c.u32(i)? == 0 { None } else { api.zone_circles(&c.handle(i)?).and_then(|z| z.first().copied()) })
}

/// `bool Attack3(grp, zone, nat, int* nClose)` (0x57ea15) and `Attack6` (0x580b71): every
/// member that is not attacking takes the enemy of the zone nearest by path that it can hurt
/// — unless one is closer than half its range (Attack3 counts it as close and drops the
/// member's attack). Ranged members attack at once (0x90, one shot for Attack3), melee
/// members after the scan; attacking members drop a target that left the zone.
fn attack3(api: &mut Api, c: &mut Call) -> Result<u32> {
    let six = c.name() == "Attack6";
    let Some(z) = zone_of_call(api, c, 1)? else { return Ok(0) };
    let nat = c.u8(2)?;
    let scan = enemy_scan(api, &z, nat, if six { CellBuilding::Outside } else { CellBuilding::NotFriend });
    c.out_i32(3, 0)?;
    if scan.is_empty() {
        return Ok(0);
    }
    let Some(g) = raw_group(api, c, 0)? else { return Ok(1) };
    // Attack3 gives every attack one shot (0x57f346, 0x57f64e), Attack6 none (0x5814e8).
    let times = u16::from(!six);
    let (cx16, cy16, r16) = (z.x << 4, z.y << 4, z.r << 4);
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let mut n_prio = 0usize;
    let mut n_close = 0;
    let members = living_members(api, g);
    let n_group = api.st.groups[g].members.len();
    for i in members {
        let f = fighter(api, &api.world.units[i]);
        let half = f.max_r >> 1;
        let attacking = api.world.units[i].target().is_some();
        let mut close = false;
        if !attacking {
            let (mut best, mut best_top, mut best_d) = (None, 10_000, 10_000);
            for &e in &scan {
                if f.kill & material(api, e) == 0 {
                    continue;
                }
                let top = obj_top_dist(api, e, i);
                let d = obj_dist(api, i, e);
                if top < best_top {
                    (best, best_top, best_d) = (Some(e), top, d);
                }
                if d < half {
                    close = !six;
                    break;
                }
            }
            if !close && let Some(t) = best {
                if f.det2_1 == 0 {
                    if pending.len() < 128 {
                        pending.push((i, t));
                    }
                } else if best_d > f.max_r || api.world.units[i].orders.is_empty() {
                    let empty_building = api.world.units[t].building && api.world.units[t].inside.is_empty();
                    if six && empty_building {
                        // Attack6 sends ranged members into an empty building (GoToMine), which
                        // takes an enemy's.
                        acr_sim::economy::enter(api.world, i, t, 0xFF, false);
                        prio_f0(api, i);
                    } else if attack_obj_n(api, i, t, 0x90, acr_sim::combat::Placement::Replace, times) {
                        prio_f0(api, i);
                    }
                }
            }
        } else {
            let target = api.world.units[i].target().map(|e| e.0 as usize);
            for &e in &scan {
                if f.kill & material(api, e) == 0 {
                    continue;
                }
                if let Some(tg) = target.filter(|&t| api.world.units[t].alive) {
                    let tp = api.world.units[tg].pos;
                    if norma(tp.x - cx16, tp.y - cy16) > r16 {
                        api.world.units[i].pop_order();
                        api.world.units[i].prio = 0;
                        break;
                    }
                }
                let d = obj_dist(api, i, e);
                if f.det2_1 != 0 {
                    if d < half {
                        close = !six;
                        break;
                    }
                } else {
                    let e_r = api.world.adv(api.world.units[e].owner, api.world.units[e].kind).attacks[0].radius.1;
                    if d < e_r {
                        close = !six;
                        break;
                    } else if api.world.units[i].prio == 0xF0 {
                        n_prio += 1;
                        break;
                    }
                }
            }
        }
        if close {
            n_close += 1;
            if attacking && !api.world.units[i].unlimited {
                api.world.units[i].pop_order();
                api.world.units[i].prio = 0;
            }
        }
    }
    let lim = n_group >> 5;
    for (i, t) in pending {
        if attack_obj_n(api, i, t, 0x90, acr_sim::combat::Placement::Replace, times) {
            if n_prio < lim {
                prio_f0(api, i);
            }
            n_prio += 1;
        }
    }
    c.out_i32(3, n_close)?;
    Ok(1)
}

/// `bool Attack4(grp, zone, nat, int* nThreat)` (0x580042): melee members only; they go
/// after the enemy shooters of the zone nearest by path unless an enemy melee unit has them
/// within its reach (then they count as threatened).
fn attack4(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(z) = zone_of_call(api, c, 1)? else { return Ok(0) };
    let nat = c.u8(2)?;
    let scan = enemy_scan(api, &z, nat, CellBuilding::Outside);
    c.out_i32(3, 0)?;
    if scan.is_empty() {
        return Ok(0);
    }
    let Some(g) = raw_group(api, c, 0)? else { return Ok(1) };
    let (cx16, cy16, r16) = (z.x << 4, z.y << 4, z.r << 4);
    let mut threatened_n = 0;
    for i in living_members(api, g) {
        let f = fighter(api, &api.world.units[i]);
        if f.det2_1 != 0 {
            continue;
        }
        let attacking = api.world.units[i].target().is_some();
        let mut threatened = false;
        if !attacking {
            let (mut best, mut best_top, mut best_d, mut rng) = (None, 10_000, 10_000, f.max_r);
            for &e in &scan {
                if f.kill & material(api, e) == 0 {
                    continue;
                }
                let d = obj_dist(api, i, e);
                let ef = fighter(api, &api.world.units[e]);
                if ef.det2_1 != 0 {
                    let top = obj_top_dist(api, e, i);
                    if top < best_top {
                        (best, best_top, best_d) = (Some(e), top, d);
                        rng = api.world.adv(api.world.units[e].owner, api.world.units[e].kind).attacks[1].radius.1;
                    }
                } else if d < ef.max_r {
                    threatened = true;
                    break;
                }
            }
            if !threatened
                && let Some(t) = best
                && (best_d > rng || api.world.units[i].orders.is_empty())
                && attack_obj(api, i, t, 0x90, acr_sim::combat::Placement::Replace)
            {
                prio_f0(api, i);
            }
        } else {
            let target = api.world.units[i].target().map(|e| e.0 as usize);
            for &e in &scan {
                if f.kill & material(api, e) == 0 {
                    continue;
                }
                if let Some(tg) = target.filter(|&t| api.world.units[t].alive) {
                    let tp = api.world.units[tg].pos;
                    if norma(tp.x - cx16, tp.y - cy16) > r16 {
                        api.world.units[i].pop_order();
                        api.world.units[i].prio = 0;
                        break;
                    }
                }
                let ef = fighter(api, &api.world.units[e]);
                if ef.det2_1 == 0 && obj_dist(api, i, e) < ef.max_r {
                    if !api.world.units[i].unlimited {
                        api.world.units[i].pop_order();
                        api.world.units[i].prio = 0;
                    }
                    threatened = true;
                    break;
                }
            }
        }
        if threatened {
            threatened_n += 1;
        }
    }
    c.out_i32(3, threatened_n)?;
    Ok(1)
}

/// `bool AttackByTomahawks(grp, zone, nat, int* nClose)` (0x574667): reloaded members take
/// the enemy nearest by path (same medium) when it is within the zone's radius or they fight
/// in melee (0x8F, one shot); attacking members are released with `rando() < 500`; reloading
/// shooters with an enemy close count as close and step back.
fn attack_by_tomahawks(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(z) = zone_of_call(api, c, 1)? else { return Ok(0) };
    let nat = c.u8(2)?;
    let scan = enemy_scan(api, &z, nat, CellBuilding::None);
    c.out_i32(3, 0)?;
    let Some(g) = raw_group(api, c, 0)? else { return Ok(u32::from(!scan.is_empty())) };
    let rz = z.r;
    let mut n_close = 0;
    for i in living_members(api, g) {
        let f = fighter(api, &api.world.units[i]);
        let half = f.max_r >> 1;
        let melee = api.world.units[i].melee_only;
        let attacking = api.world.units[i].target().is_some();
        let delay = api.world.units[i].delay;
        let mut close = false;
        if !attacking && delay == 0 {
            let (mut best, mut best_top, mut best_d) = (None, 10_000, 10_000);
            for &e in &scan {
                if f.kill & material(api, e) == 0 || is_water(api, e) != f.water {
                    continue;
                }
                let top = obj_top_dist(api, e, i);
                let d = obj_dist(api, i, e);
                if top <= best_top {
                    (best, best_top, best_d) = (Some(e), top, d);
                }
                if d < half {
                    close = true;
                    break;
                }
            }
            if !close
                && let Some(t) = best
                && (best_d < rz || melee)
            {
                clear_brigade_orders(api, i);
                // One throw per order (0x574f08).
                if attack_obj_n(api, i, t, 0x8F, acr_sim::combat::Placement::Replace, 1) {
                    prio_f0(api, i);
                }
            }
        } else if attacking {
            if api.world.random.rando() < 500 && !api.world.units[i].unlimited {
                clear_orders(api, i);
            }
        } else {
            for &e in &scan {
                if f.kill & material(api, e) == 0 {
                    continue;
                }
                let d = obj_dist(api, i, e);
                let e_max = fighter(api, &api.world.units[e]).max_r;
                if f.det2_1 != 0 && (d < half || (d < e_max && delay != 0)) && !melee {
                    close = true;
                    break;
                }
            }
        }
        if close {
            n_close += 1;
            if api.world.units[i].target().is_some() && !melee && !api.world.units[i].unlimited {
                clear_orders(api, i);
            }
        }
    }
    c.out_i32(3, n_close)?;
    Ok(u32::from(!scan.is_empty()))
}

/// `bool AttackEnemyInZone2(grp, zone, nat)` (0x57dfb9): members that are not attacking take
/// the enemy nearest by path beyond their minimum range (same medium, 0x8F); attacking
/// members are released with `rando() < 900`.
fn attack_enemy_in_zone2(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(z) = zone_of_call(api, c, 1)? else { return Ok(0) };
    let nat = c.u8(2)?;
    let scan = enemy_scan(api, &z, nat, CellBuilding::None);
    let Some(g) = raw_group(api, c, 0)? else { return Ok(u32::from(!scan.is_empty())) };
    for i in living_members(api, g) {
        if api.world.units[i].target().is_some() {
            if api.world.random.rando() < 900 && !api.world.units[i].unlimited {
                clear_orders(api, i);
            }
            continue;
        }
        let f = fighter(api, &api.world.units[i]);
        let me = api.world.units[i].nmask;
        let mut best: Option<(i32, usize)> = None;
        for &e in &scan {
            let eu = &api.world.units[e];
            if f.kill & material(api, e) == 0 || eu.nmask & me != 0 || is_water(api, e) != f.water {
                continue;
            }
            if obj_dist(api, i, e) <= f.min_r {
                continue;
            }
            let top = obj_top_dist(api, e, i);
            if top < 10_000 && best.is_none_or(|b| top < b.0) {
                best = Some((top, e));
            }
        }
        if let Some((_, t)) = best {
            clear_brigade_orders(api, i);
            attack_obj(api, i, t, 0x8F, acr_sim::combat::Placement::Replace);
        }
    }
    Ok(u32::from(!scan.is_empty()))
}

/// `void AttackEnemyInZone(grp, zone, enemyNat)` (0x5642b7): the living objects of
/// `enemyNat` in the zone (alliances not checked); idle members take, in this order of
/// preference: a unit in their range band nearest by path (the engine compares the 1/16 px
/// distance with the pixel ranges, kept), for building attackers a building in the band, a
/// visible one of those, any target within a path length of 560; priority 1. Members with an
/// attack order drop it with `rando() < 600`.
fn attack_enemy_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(z) = api.zone_arg(c, 1, "AttackEnemyInZone")?.and_then(|z| z.first().copied()) else { return Ok(0) };
    let Some(g) = raw_group(api, c, 0)? else { return Ok(0) };
    let en = c.u8(2)?;
    let list: Vec<usize> = api
        .world
        .units
        .iter()
        .filter(|u| u.living() && u.owner == en && z.contains(u))
        .take(800)
        .map(|u| u.id.0 as usize)
        .collect();
    if list.is_empty() {
        return Ok(0);
    }
    for i in living_members(api, g) {
        if !api.world.units[i].orders.is_empty() {
            if api.world.random.rando() < 600 && api.world.units[i].target().is_some() {
                api.world.units[i].pop_order();
            }
            continue;
        }
        let f = fighter(api, &api.world.units[i]);
        let ab = api.world.catalog.get(api.world.units[i].kind).role.attacks_buildings;
        let up = api.world.units[i].pos;
        let mut best: Option<(i32, usize)> = None;
        for &e in &list {
            let eu = &api.world.units[e];
            if eu.building || f.kill & material(api, e) == 0 {
                continue;
            }
            let d16 = norma(up.x - eu.pos.x, up.y - eu.pos.y);
            if f.min_r < d16 && d16 < f.max_r {
                let top = obj_top_dist(api, e, i);
                if best.is_none_or(|b| top < b.0) {
                    best = Some((top, e));
                }
            }
        }
        if best.is_none() && ab {
            for &e in &list {
                if !api.world.units[e].building {
                    continue;
                }
                let d = obj_dist(api, i, e);
                if f.min_r < d && d < f.max_r {
                    let v = obj_top_dist(api, e, i) + d;
                    if best.is_none_or(|b| v < b.0) {
                        best = Some((v, e));
                    }
                }
            }
        }
        let saved = best;
        if let Some((_, t)) = best
            && (ab || f.max_r > 150)
            && !acr_sim::combat::visible_between(api.world, api.world.units[t].pos, up, i)
        {
            best = None;
        }
        if best.is_none() {
            best = saved;
        }
        if best.is_none() {
            for &e in &list {
                let eu = &api.world.units[e];
                if f.kill & material(api, e) == 0 && !(ab && eu.building) {
                    continue;
                }
                let top = obj_top_dist(api, e, i);
                if obj_dist(api, i, e) > f.min_r && top < 560 && best.is_none_or(|b| top < b.0) {
                    best = Some((top, e));
                }
            }
        }
        if let Some((_, t)) = best {
            attack_obj(api, i, t, 1, acr_sim::combat::Placement::Replace);
        }
    }
    Ok(0)
}

/// `void AttackBuildingsInZone(grp, zone, nat)` (0x5617df): nation and zone are not used;
/// every member without a target probes 50 random cells around itself (`FindTarget`
/// 0x5614df, two random numbers each) for an enemy ship, building or (close by) land unit
/// in its range band and attacks it (0x90, in front, 10 shots).
fn attack_buildings_in_zone(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group_arg(c, 0, "AttackBuildingsInZone")? else { return Ok(0) };
    if api.zone_arg(c, 1, "AttackBuildingsInZone")?.is_none() {
        return Ok(0);
    }
    for i in living_members(api, g) {
        if api.world.units[i].target().is_some() {
            continue;
        }
        if let Some(t) = find_target(api, i) {
            attack_obj_n(api, i, t, 0x90, acr_sim::combat::Placement::Front, 10);
        }
    }
    Ok(0)
}

/// `FindTarget` (0x5614df): 50 random 128 px cells within the unit's maximum range.
fn find_target(api: &mut Api, i: usize) -> Option<usize> {
    let f = fighter(api, &api.world.units[i]);
    let u = &api.world.units[i];
    let (ux, uy, nm) = (u.pos.x, u.pos.y, !u.nmask);
    let (cx, cy) = (ux >> 11, uy >> 11);
    let k = (f.max_r >> 7) + 1;
    let side = api.world.buckets.side();
    let (mut c1, mut c2, mut c3, mut best3) = (None, None, None, i32::MAX);
    let within = |api: &Api, e: usize| {
        let p = api.world.units[e].pos;
        let d = norma(p.x - ux, p.y - uy);
        f.min_r * 16 < d && d < f.max_r * 16
    };
    for _ in 0..50 {
        let dx = ((api.world.random.rando() * (2 * k + 1)) >> 15) - k;
        let dy = ((api.world.random.rando() * (2 * k + 1)) >> 15) - k;
        // The engine bounds dy by cx too (a bug, kept).
        if dx < -cx || dx > side - cx || dy < -cx || dy > side - cx {
            continue;
        }
        let (x, y) = (cx + dx, cy + dy);
        if api.world.buckets.presence(x, y) & nm == 0 {
            continue;
        }
        let units: Vec<usize> = api.world.buckets.get(x, y).iter().map(|&k| k as usize).collect();
        let enemy = |e: &usize| api.world.units[*e].nmask & !nm == 0 && api.world.units[*e].living();
        if units.len() >= 3
            && let Some(&s) = units.iter().find(|e| enemy(e))
            && is_water(api, s)
            && api.world.catalog.get(api.world.units[s].kind).role.usage != 16
            && within(api, s)
        {
            c1 = Some(s);
            continue;
        }
        if let Some(&b) = api
            .world
            .buckets
            .buildings_in(x, y)
            .iter()
            .find(|&&b| enemy(&(b as usize)))
            .filter(|&&b| within(api, b as usize))
        {
            c2 = Some(b as usize);
            continue;
        }
        if norma(dx, dy) < 20
            && units.len() >= 3
            && let Some(&l) = units.iter().find(|e| enemy(e))
            && !is_water(api, l)
            && within(api, l)
        {
            let p = api.world.units[l].pos;
            let d = norma(p.x - ux, p.y - uy);
            if d < best3 {
                (best3, c3) = (d, Some(l));
            }
        }
    }
    c1.or(c2).or(c3)
}

/// `bool AttackGrpByGrp(attackers, victims)` (0x57385f): members not attacking take the
/// victim nearest by path they can hurt (same medium, 0x8F); attacking members are released
/// with `rando() < 900`.
fn attack_grp_by_grp(api: &mut Api, c: &mut Call) -> Result<u32> {
    let (Some(a), Some(v)) = (raw_group(api, c, 0)?, raw_group(api, c, 1)?) else { return Ok(0) };
    let victims = living_members(api, v);
    for i in living_members(api, a) {
        if api.world.units[i].target().is_some() {
            if api.world.random.rando() < 900 && !api.world.units[i].unlimited {
                clear_orders(api, i);
            }
            continue;
        }
        let f = fighter(api, &api.world.units[i]);
        let me = api.world.units[i].nmask;
        let mut best: Option<(i32, usize)> = None;
        for &e in &victims {
            if f.kill & material(api, e) == 0 || api.world.units[e].nmask & me != 0 || is_water(api, e) != f.water {
                continue;
            }
            let top = obj_top_dist(api, e, i);
            if top < 10_000 && best.is_none_or(|b| top < b.0) {
                best = Some((top, e));
            }
        }
        if let Some((_, t)) = best {
            attack_obj(api, i, t, 0x8F, acr_sim::combat::Placement::Replace);
        }
    }
    Ok(1)
}

/// `bool GrpAttackObject(nat, grp, target)` (0x570d0c): members not attacking attack the
/// object unless it is a friend of the nation (0x8F); attacking members are released with
/// `rando() < 600`.
fn grp_attack_object(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = raw_group(api, c, 1)? else { return Ok(0) };
    let t = c.i32(2)?;
    let Some(target) =
        u16::try_from(t).ok().and_then(|t| api.object(t)).filter(|u| u.living()).map(|u| u.id.0 as usize)
    else {
        return Ok(0);
    };
    let mask = nat_mask(api, c.u8(0)?);
    for i in living_members(api, g) {
        if api.world.units[i].target().is_some() {
            if api.world.random.rando() < 600 && !api.world.units[i].unlimited {
                clear_orders(api, i);
            }
        } else if api.world.units[target].nmask & mask == 0 {
            attack_obj(api, i, target, 0x8F, acr_sim::combat::Placement::Replace);
        }
    }
    Ok(1)
}

/// `bool SGP_AttackByArchers(byte nat, GAMEOBJ* grp, ...)` (0x5790f3): the group's centre
/// (members not dying nor inside, 0x578787; none: 0) and its top area (0x416554) become a
/// process (0x576c80 with type 0: it replaces the group's processes) that keeps the group
/// shooting at `FindArcherTarget`'s choice ([`find_archer_target`], handler 0x578ff6 in
/// `run_group_orders`). The nation is taken as it is (no `AssignTBL`); the third and fourth
/// arguments are not read.
fn sgp_attack_by_archers(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 1)? else { return Ok(0) };
    let ids = living_members(api, g);
    if ids.is_empty() {
        return Ok(0);
    }
    let n = ids.len() as i64;
    let x = (ids.iter().map(|&i| i64::from(api.world.units[i].pos.x >> 4)).sum::<i64>() / n) as i32;
    let y = (ids.iter().map(|&i| i64::from(api.world.units[i].pos.y >> 4)).sum::<i64>() / n) as i32;
    let zone = top_zone_ring(api, x, y);
    let order = crate::state::GroupOrder::ArcherAttack { nation: c.u8(0)?, x, y, zone, missed: false };
    api.st.groups[g].orders = vec![order];
    Ok(1)
}

/// The top area of a point (pixels) or of the nearest 64 px cell around it that has one
/// (0x416554: rings 0..19 of `Rarr`); 0xFFFF if none.
pub fn top_zone_ring(api: &Api, x: i32, y: i32) -> u16 {
    let Some(t) = &api.st.topo else { return 0xFFFF };
    let (w, h) = (t.cells.width as i32, t.cells.height as i32);
    let (cx, cy) = (x >> 6, y >> 6);
    let at = |x: i32, y: i32| {
        (x >= 0 && y >= 0 && x < w && y < h)
            .then(|| t.cells.get(x as usize, y as usize))
            .flatten()
            .filter(|&a| a < 0xFFFE)
    };
    acr_sim::trig::rings()[..20]
        .iter()
        .flatten()
        .find_map(|&(dx, dy)| at(cx + i32::from(dx), cy + i32::from(dy)))
        .unwrap_or(0xFFFF)
}

/// `FindArcherTarget(nat, x, y, maxd, zone)` (0x43e5dd): the best target for archers of
/// nation `nat` around `(x, y)` (pixels). Objects within 960 px (a square, strictly; not
/// dying, not inside, no artillery) are scored on a grid of 32 px cells centred on the
/// point: an enemy building adds 50 if it is a town centre, tower or mine (`USAGE` 3, 5, 7),
/// else by its life (below 4000: 30, 8000: 20, 20000: 8, else 3); an enemy unit adds 1 to
/// its cell and the 8 around it, a friend takes 1 off them. Scores are bytes and wrap. A
/// cell's last enemy (in index order) stands for it. Then, over the cells with a positive
/// score in row order, `(score << 8) / max(8, Norma)` of the cell offset (0 beyond `maxd`)
/// must beat the best so far, and the 64 px cell at half the offset must lie in an area
/// (not the top row or left column) that is `zone` or within 40 of it (`LinksDist`). No
/// random numbers. Walls (OB+0x29 bit0) are scored by another rule (0x43e3cc); the data
/// has none.
pub fn find_archer_target(api: &Api, nat: u8, x: i32, y: i32, maxd: i32, zone: u16) -> Option<usize> {
    let t = api.st.topo.as_ref()?;
    let n_areas = t.areas.len();
    if usize::from(zone) >= n_areas {
        return None;
    }
    let mybit = (1u32 << (nat & 31)) as u8;
    let (lo_x, hi_x, lo_y, hi_y) = ((x - 960) << 4, (x + 960) << 4, (y - 960) << 4, (y + 960) << 4);
    let (bx, by) = ((x - 1024) << 4, (y - 1024) << 4);
    let mut score = [0i8; 4096];
    let mut id = [None::<usize>; 4096];
    let catalog = &api.world.catalog;
    for (k, u) in api.world.units.iter().enumerate() {
        if !(lo_x < u.pos.x && u.pos.x < hi_x && lo_y < u.pos.y && u.pos.y < hi_y) || !u.active() {
            continue;
        }
        let c = (((u.pos.x - bx) >> 9) + (((u.pos.y - by) >> 9) << 6)) as usize;
        let ty = catalog.get(u.kind);
        if c >= 4096 || ty.role.artillery {
            continue;
        }
        let friend = api.st.unit_nmask(u.id, u.owner) & mybit != 0;
        if u.building {
            if !friend {
                use acr_sim::catalog::usage;
                let add = if matches!(ty.role.usage, usage::CENTER | usage::TOWER | usage::MINE) {
                    50
                } else {
                    match u.life as u16 {
                        0..4000 => 30,
                        4000..8000 => 20,
                        8000..20000 => 8,
                        _ => 3,
                    }
                };
                score[c] = score[c].wrapping_add(add);
                id[c] = Some(k);
            }
        } else {
            for d in [0i32, 1, -1, 64, -64, 63, -63, 65, -65] {
                if let Some(s) = score.get_mut((c as i32 + d) as usize) {
                    *s = if friend { s.wrapping_sub(1) } else { s.wrapping_add(1) };
                }
            }
            if !friend {
                id[c] = Some(k);
            }
        }
    }
    let (mut best, mut res) = (0, None);
    let (cx0, cy0) = (x >> 6, y >> 6);
    let (w, h) = (t.cells.width as i32, t.cells.height as i32);
    for c in 0..4096i32 {
        let Some(o) = id[c as usize] else { continue };
        let s = i32::from(score[c as usize]);
        if s <= 0 {
            continue;
        }
        let (dx, dy) = ((c & 63) - 32, (c >> 6) - 32);
        let d = match norma(dx, dy).max(8) {
            d if d > maxd => 100_000,
            d => d,
        };
        let s = (s << 8) / d;
        if s <= best {
            continue;
        }
        let (zx, zy) = (cx0 + (dx >> 1), cy0 + (dy >> 1));
        if zx <= 0 || zx >= w || zy <= 0 || zy >= h {
            continue;
        }
        let Some(a) = t.cells.get(zx as usize, zy as usize).filter(|&a| usize::from(a) < n_areas) else { continue };
        let links = t.distance.get(usize::from(a) * n_areas + usize::from(zone)).copied().unwrap_or(0xFFFF);
        if a == zone || links < 40 {
            (best, res) = (s, Some(o));
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_formats::defs::UnitDef;
    use acr_formats::m3d::{Area, Areas, Grid};
    use acr_script::heap::Heap;
    use acr_script::memory::Memory;
    use acr_script::runtime::HostCall;
    use acr_script::x86::Cpu;
    use acr_sim::armies::Army;
    use acr_sim::catalog::{Catalog, ObjectType};
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;
    use acr_sim::{Pos, World};

    use super::super::{Api, Call, Caller, function};
    use crate::state::{NO_UNIT, ScriptSetup, ScriptState};
    use crate::texts::MissionTexts;

    const SOLDIER: &str = "NAME Sol\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
        @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n";
    const PEASANT: &str = "NAME Kri\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
        @MOTION_R 9 0 10 19\r\nLIFE 50\r\nGEOMETRY 1 10 40\r\nPEASANT\r\n[END]\r\n";
    const HOUSE: &str = "NAME Hou\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\n\
        LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\n[END]\r\n";

    const STACK: u32 = 0x1_8000;
    const DATA: u32 = 0x1_c000;

    /// A 4096 px map whose west half is area 0 and east half area 1, linked; enemy (nation 1)
    /// soldiers and a peasant in the west, one of nation 0's soldiers, a nation-7 house in
    /// the east.
    struct Fixture {
        world: World,
        st: ScriptState,
        mem: Memory,
        peasant: acr_sim::TypeId,
    }

    impl Fixture {
        fn new() -> Self {
            let mut c = Catalog::new();
            let sol = c.add(ObjectType::from_def("Sol(EN)", &UnitDef::parse(SOLDIER.as_bytes())));
            let kri = c.add(ObjectType::from_def("Kri(EN)", &UnitDef::parse(PEASANT.as_bytes())));
            let hou = c.add(ObjectType::from_def("Hou(EN)", &UnitDef::parse(HOUSE.as_bytes())));
            let mut world = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
            for i in 0..5 {
                world.spawn(sol, 1, Pos::from_pixels(500 + 40 * i, 500), 0);
            }
            world.spawn(kri, 1, Pos::from_pixels(500, 700), 0);
            world.spawn(sol, 0, Pos::from_pixels(500, 900), 0);
            world.spawn(hou, 7, Pos::from_pixels(3000, 1000), 0);
            let mut st = ScriptState::new(ScriptSetup::default(), MissionTexts::default());
            let cells = (0..64 * 64).map(|i| u16::from(i % 64 >= 32)).collect();
            let area = |x, a: u16| Area { x, y: 32, record: Vec::new(), extra: Vec::new(), links: vec![(a, 2048)] };
            st.topo = Some(Areas {
                areas: vec![area(16, 1), area(48, 0)],
                next_hop: vec![0, 1, 0, 1],
                distance: vec![0, 2048, 2048, 0],
                cells: Grid { width: 64, height: 64, cells },
            });
            let mut mem = Memory::new();
            mem.map(0x1_0000, 0x1_0000);
            Self { world, st, mem, peasant: kri }
        }

        /// Calls the API function with the 32-bit arguments.
        fn call(&mut self, name: &str, args: &[u32]) -> u32 {
            let mut cpu = Cpu::new();
            cpu.regs[usize::from(acr_script::x86::decode::reg::ESP)] = STACK;
            for (i, &a) in args.iter().enumerate() {
                self.mem.write_u32(STACK + 4 + 4 * i as u32, a).expect("stack");
            }
            let mut heap = Heap::new(0x2_0000, 0x3_0000);
            let mut raw = HostCall { dll: "test", name, cpu: &mut cpu, mem: &mut self.mem, heap: &mut heap };
            let (mut commands, mut events) = (Vec::new(), Vec::new());
            let mut api = Api {
                st: &mut self.st,
                world: &mut self.world,
                commands: &mut commands,
                events: &mut events,
                caller: Caller::Mission,
                vm: 0,
                dip: None,
            };
            (function(name).expect("known").handler)(&mut api, &mut Call { raw: &mut raw }).expect("call")
        }

        fn danger(&mut self, name: &str, steps: u32) -> [i16; 2] {
            self.mem.write_u32(DATA + 0x100, 10).expect("weight of category 0");
            self.call(name, &[0, DATA, 2, DATA + 0x100, steps]);
            [0, 1].map(|i| self.mem.read_u16(DATA + 2 * i).expect("map") as i16)
        }
    }

    #[test]
    fn danger_maps_weigh_objects_by_kind_and_spread_over_links() {
        let mut f = Fixture::new();
        // West: 5 enemy soldiers +10 each, the enemy peasant −20, the own soldier −10; east:
        // the nation-7 building +10 << 7.
        assert_eq!(f.danger("CreateDangerMap", 0), [20, 1280]);
        // One step, in place and in area order: east += 20 / 2, then west += 1290 / 2.
        assert_eq!(f.danger("CreateDangerMap", 1), [665, 1290]);
        // Storm: the peasant counts −10 and the building 10 << 8.
        assert_eq!(f.danger("CreateDangerMapForStorm", 0), [30, 2560]);
        // ForDef: (STORMFORCE >> 5) | 1 per enemy unit, buildings and friends left out, no
        // clamp.
        assert_eq!(f.danger("CreateDangerMapForDef", 0), [6, 0]);
        // ForTom: every other enemy object −10, the peasant too.
        assert_eq!(f.danger("CreateDangerMapForTom", 0), [-6, 2560]);
    }

    #[test]
    fn storm_lists_take_enemy_barracks_by_their_defence() {
        const BARRACKS: &str = "NAME Bar
BUILDING
LIFE 1000
LOCATION 0 0 64 64
            LOCKPOINTS 4 0 0 1 0 0 1 1 1
UNITABSORBER 10
OPPOSIT 10 30
[END]
";
        let mut f = Fixture::new();
        let bar = ObjectType::from_def("Bar(EN)", &UnitDef::parse(BARRACKS.as_bytes()));
        let mut c = (*f.world.catalog).clone();
        let bar = c.add(bar);
        f.world.catalog = Arc::new(c);
        let b = f.world.spawn(bar, 1, Pos::from_pixels(3000, 2000), 0);
        f.call("CreateTopListForStorm", &[DATA, 0]);
        let list = |f: &Fixture| [0, 1].map(|i| f.mem.read_u16(DATA + 2 * i).expect("list"));
        assert_eq!(list(&f), [NO_UNIT, b.0 as u16]);
        // Two inside of 10: 2 · 2 · (10 + 20 · 2 / 10) = 56.
        for _ in 0..2 {
            let s = f.world.units[0].id;
            f.world.units[0].inside_of = Some(b);
            f.world.units[b.0 as usize].inside.push(s);
        }
        assert_eq!(f.call("GetBuildingOposit", &[b.0, DATA + 0x200]), 1);
        assert_eq!(f.mem.read_u32(DATA + 0x200).expect("value"), 56);
        f.call("CreateTopListForStormHard", &[DATA, 0, 56]);
        assert_eq!(list(&f), [NO_UNIT, NO_UNIT], "56 is not below 56");
        f.call("CreateTopListForStormHard", &[DATA, 0, 57]);
        assert_eq!(list(&f), [NO_UNIT, b.0 as u16]);
    }

    #[test]
    fn peasants_top_list_keeps_areas_with_enough_peasants() {
        let mut f = Fixture::new();
        let kri = f.peasant;
        let p = f.world.spawn(kri, 1, Pos::from_pixels(600, 700), 0);
        f.world.spawn(kri, 1, Pos::from_pixels(3000, 700), 0);
        f.call("CreatePeasantsTopList", &[DATA, 0, 2, 0]);
        let list = [0, 1].map(|i| f.mem.read_u16(DATA + 2 * i).expect("list"));
        assert_eq!(list, [p.0 as u16, NO_UNIT], "the last peasant of the west, one is too few in the east");
    }

    #[test]
    fn enum_enemy_armies_writes_whole_fields_while_below_len() {
        let mut f = Fixture::new();
        let a = |x, n| Army { bounds: [1, 2, 3, 4], force: 7 * n, count: n, x, y: 100 };
        f.world.armies.groups = Some([Some(0), Some(1), None, None, None, None, None, None]);
        f.world.armies.armies[0] = vec![a(300, 2), a(900, 5)];
        assert_eq!(f.call("EnumEnemyArmies", &[0, 3, DATA, 100]), 2);
        let buf =
            |f: &Fixture, n: u32| (0..n).map(|i| f.mem.read_u32(DATA + 4 * i).expect("buf") as i32).collect::<Vec<_>>();
        assert_eq!(buf(&f, 6), [300, 100, 2, 900, 100, 5]);
        // The second army's position starts below 9 and is written whole (one past the end).
        assert_eq!(f.call("EnumEnemyArmies", &[0, 15, DATA, 9]), 1);
        assert_eq!(buf(&f, 10), [300, 100, 2, 14, 1, 2, 3, 4, 900, 100]);
        assert_eq!(f.call("EnumEnemyArmies", &[2, 15, DATA, 9]), 0, "no EnemyInfo");
    }

    /// An archer of nation 0 in the west half of the fixture's areas, alone on a new map,
    /// in a group of its own; the tests add the targets.
    struct Archers {
        f: Fixture,
        foot: acr_sim::TypeId,
        tower: acr_sim::TypeId,
        archer: acr_sim::world::EntityId,
    }

    impl Archers {
        fn new() -> Self {
            const ARCHER: &str = "NAME Arc\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
                DAMAGE 1 30\r\nATTACK_RADIUS 1 60 600\r\nDET_RADIUS 1 60 7000\r\nWEAPONKIND 1 STRELA\r\n\
                MATHERIAL 1 BODY\r\nCANKILL 1 BODY\r\n[END]\r\n";
            const FOOT: &str = "NAME Ft\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n\
                MATHERIAL 1 BODY\r\n[END]\r\n";
            const TOWER: &str = "NAME Tow\r\nBUILDING\r\nUSAGE TOWER\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\n\
                LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nMATHERIAL 1 BODY\r\n[END]\r\n";
            let mut f = Fixture::new();
            let mut c = Catalog::new();
            let arc = c.add(ObjectType::from_def("Arc(EN)", &UnitDef::parse(ARCHER.as_bytes())));
            let foot = c.add(ObjectType::from_def("Ft(EN)", &UnitDef::parse(FOOT.as_bytes())));
            let tower = c.add(ObjectType::from_def("Tow(EN)", &UnitDef::parse(TOWER.as_bytes())));
            f.world = World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
            let archer = f.world.spawn(arc, 0, Pos::from_pixels(1000, 1000), 0);
            let m = crate::state::Member { id: archer.0 as u16, serial: f.st.serial(archer) };
            f.st.groups.push(crate::state::Group { members: vec![m], ..Default::default() });
            Self { f, foot, tower, archer }
        }

        fn target(&mut self) -> Option<usize> {
            let (mut commands, mut events) = (Vec::new(), Vec::new());
            let api = Api {
                st: &mut self.f.st,
                world: &mut self.f.world,
                commands: &mut commands,
                events: &mut events,
                caller: Caller::Mission,
                vm: 0,
                dip: None,
            };
            super::find_archer_target(&api, 0, 1000, 1000, 40, 0)
        }

        fn run_processes(&mut self) {
            let (mut commands, mut events) = (Vec::new(), Vec::new());
            let mut api = Api {
                st: &mut self.f.st,
                world: &mut self.f.world,
                commands: &mut commands,
                events: &mut events,
                caller: Caller::Mission,
                vm: 0,
                dip: None,
            };
            super::super::groups::run_group_orders(&mut api);
        }
    }

    /// SelAttackGroup (0x560258): enemies in member order, skipping a dying one; a unit that
    /// took an enemy keeps it (its `EnemyID` is set), and one already attacking is left alone.
    #[test]
    fn sel_attack_group_takes_the_first_enemy_it_can() {
        use super::super::call::magic;
        let mut a = Archers::new();
        let foes: Vec<_> =
            (0..3).map(|n| a.f.world.spawn(a.foot, 1, Pos::from_pixels(1100 + 40 * n, 1000), 0)).collect();
        a.f.world.units[foes[0].0 as usize].death = 1;
        let members =
            foes.iter().map(|&id| crate::state::Member { id: id.0 as u16, serial: a.f.st.serial(id) }).collect();
        a.f.st.groups.push(crate::state::Group { members, ..Default::default() });
        a.f.st.selection[0] = a.f.st.groups[0].members.clone();
        a.f.mem.write_u16(DATA, 1).expect("handle");
        a.f.mem.write_u16(DATA + 2, 0).expect("handle");
        a.f.mem.write_u32(DATA + 4, magic::GROUP).expect("handle");
        a.f.call("SelAttackGroup", &[0, DATA]);
        assert_eq!(a.f.world.units[a.archer.0 as usize].target(), Some(foes[1]));
        // Already attacking: the next call changes nothing.
        a.f.st.groups[1].members.swap(1, 2);
        a.f.call("SelAttackGroup", &[0, DATA]);
        assert_eq!(a.f.world.units[a.archer.0 as usize].target(), Some(foes[1]));
    }

    /// GetTotalAmount1 (0x55e7f2) reads the census cache, not the live objects.
    #[test]
    fn get_total_amount1_reads_the_census() {
        use super::super::call::magic;
        let mut f = Fixture::new();
        f.mem.write_u16(DATA, 0).expect("handle");
        f.mem.write_u32(DATA + 4, magic::UNIT_TYPE).expect("handle");
        assert_eq!(f.call("GetTotalAmount1", &[DATA, 1]), 0, "no census yet");
        acr_sim::economy::census_all(&mut f.world);
        assert_eq!(f.call("GetTotalAmount1", &[DATA, 1]), 5);
        assert_eq!(f.call("GetTotalAmount1", &[DATA, 0]), 1);
        f.world.units[0].death = 1;
        assert_eq!(f.call("GetTotalAmount1", &[DATA, 1]), 5, "until the next census");
    }

    /// GetUnitGlobalInfo (0x567cb9): no serial, a dying object fails, `0x20`/`0x21`/`0x23`
    /// are not written.
    #[test]
    fn get_unit_global_info_fills_the_short_record() {
        let mut f = Fixture::new();
        f.mem.write_u32(DATA + 0x20, 0xAAAA_AAAA).expect("out");
        assert_eq!(f.call("GetUnitGlobalInfo", &[6, DATA]), 1);
        assert_eq!(f.mem.read_u16(DATA).expect("out"), 6);
        assert_eq!(f.mem.read_u8(DATA + 0x1c).expect("out"), 0, "owner");
        assert_eq!(f.mem.read_u8(DATA + 0x20).expect("out"), 0xAA);
        assert_eq!(f.mem.read_u8(DATA + 0x23).expect("out"), 0xAA);
        f.world.units[6].death = 1;
        assert_eq!(f.call("GetUnitGlobalInfo", &[6, DATA]), 0);
        assert_eq!(f.mem.read_u16(DATA).expect("out"), NO_UNIT);
        assert_eq!(f.call("GetUnitGlobalInfo", &[0xFFFF, DATA]), 0);
    }

    /// GetUnitExCaps (0x56be77): direction, destination (−1 without), type, owner,
    /// `StandTime` and, only when asked, the index of the order's link in 0x66a258.
    #[test]
    fn get_unit_ex_caps_reports_stand_time_and_order_kind() {
        let mut f = Fixture::new();
        let u = &mut f.world.units[6];
        u.dir = 64;
        u.stand_time = 17;
        u.dest = Some(Pos::from_pixels(320, 480));
        u.orders.push(acr_sim::world::Order::SendTo { cell: (5, 7), prio: 0 });
        assert_eq!(f.call("GetUnitExCaps", &[6, DATA, 1]), 1);
        let rd = |f: &Fixture, o: u32| f.mem.read_u32(DATA + o).expect("caps") as i32;
        assert_eq!([rd(&f, 0), rd(&f, 4), rd(&f, 8), rd(&f, 0x14), rd(&f, 0x18)], [64, 320, 480, 0, 17]);
        assert_eq!(rd(&f, 0x1c), 8, "NewMonsterSendToLink is entry 8");
        assert_eq!(f.call("GetUnitExCaps", &[6, DATA, 0]), 1);
        assert_eq!(rd(&f, 0x1c), -1, "not asked");
        f.world.units[6].orders.clear();
        f.world.units[6].dest = None;
        assert_eq!(f.call("GetUnitExCaps", &[6, DATA, 1]), 1);
        assert_eq!([rd(&f, 4), rd(&f, 8), rd(&f, 0x1c)], [-1, -1, -1]);
        f.world.units[6].death = 1;
        assert_eq!(f.call("GetUnitExCaps", &[6, DATA, 1]), 0);
        assert_eq!(f.call("GetUnitExCaps", &[-1i32 as u32, DATA, 1]), 0);
    }

    /// OBJ_SendTo (0x56f0a1 → 0x517045): the order type places the move, a lower priority
    /// is refused, the order keeps `prio & 0x7f`.
    #[test]
    fn obj_send_to_places_by_order_type_and_priority() {
        use acr_sim::world::Order;
        let mut f = Fixture::new();
        let cell = |x| acr_sim::world::Unit::cell_of(Pos::from_pixels(x, 900), f.world.units[6].lx);
        let (c1, c2, c3) = (cell(800), cell(1600), cell(2000));
        f.call("OBJ_SendTo", &[6, 800, 900, 0x85, 0]);
        let u = &f.world.units[6];
        assert_eq!(u.orders, vec![Order::SendTo { cell: c1, prio: 5 }]);
        assert_eq!((u.dest, u.prio), (Some(Pos::from_pixels(800, 900)), 5));
        f.call("OBJ_SendTo", &[6, 1600, 900, 4, 0]);
        assert_eq!(f.world.units[6].orders.len(), 1, "below the current priority");
        f.call("OBJ_SendTo", &[6, 1600, 900, 6, 2]);
        assert_eq!(f.world.units[6].orders[0], Order::SendTo { cell: c2, prio: 6 }, "appended");
        f.call("OBJ_SendTo", &[6, 2000, 900, 7, 0x81]);
        let u = &f.world.units[6];
        assert_eq!((u.orders.len(), u.orders.last()), (3, Some(&Order::SendTo { cell: c3, prio: 7 })), "in front");
    }

    /// SetLightSpot (0x564017) takes a point zone's centre; an index above 63 is an error;
    /// ClearLightSpot (0x564121) zeroes the slot.
    #[test]
    fn light_spots_fill_and_clear_their_slots() {
        let mut f = Fixture::new();
        f.mem.write_u16(DATA, 1200).expect("handle");
        f.mem.write_u16(DATA + 2, 900).expect("handle");
        f.mem.write_u32(DATA + 4, 0x4000_0100).expect("handle");
        f.call("SetLightSpot", &[DATA, 3, 5]);
        assert_eq!(f.st.light_spots[5], crate::state::LightSpot { x: 1200, y: 900, kind: 3 });
        let errors = f.st.errors;
        f.call("SetLightSpot", &[DATA, 3, 64]);
        assert_eq!(f.st.errors, errors + 1);
        f.call("ClearLightSpot", &[5]);
        assert_eq!(f.st.light_spots[5], Default::default());
    }

    /// FindArcherTarget (0x43e5dd) weighs a tower (50) over a lone soldier (1 in its cell and
    /// around): 50·256/19 against 256/8 although the soldier is four times nearer.
    #[test]
    fn archers_prefer_a_tower_to_a_nearer_soldier() {
        let mut a = Archers::new();
        let soldier = a.f.world.spawn(a.foot, 1, Pos::from_pixels(1000, 1130), 0);
        assert_eq!(a.target(), Some(soldier.0 as usize));
        let tower = a.f.world.spawn(a.tower, 1, Pos::from_pixels(1000, 1600), 0);
        assert_eq!(a.target(), Some(tower.0 as usize));
    }

    /// Friends take 1 off the cells around them: an enemy among two of the archers' own
    /// units scores −1 and is passed over for a lone one further away; artillery is never
    /// a target.
    #[test]
    fn archers_skip_an_enemy_among_friends() {
        let mut a = Archers::new();
        let near = a.f.world.spawn(a.foot, 1, Pos::from_pixels(1000, 1130), 0);
        a.f.world.spawn(a.foot, 0, Pos::from_pixels(1005, 1130), 0);
        a.f.world.spawn(a.foot, 0, Pos::from_pixels(1010, 1130), 0);
        let far = a.f.world.spawn(a.foot, 1, Pos::from_pixels(1500, 1000), 0);
        assert_eq!(a.target(), Some(far.0 as usize));
        assert_ne!(a.target(), Some(near.0 as usize));
    }

    /// The process (`'ACRA'`, 0x578ff6) never ends: with nothing to shoot its purpose reads
    /// `'PCRA'`; an enemy that turns up later is attacked by every member (0x90).
    #[test]
    fn the_archer_process_keeps_looking() {
        let mut a = Archers::new();
        let order = crate::state::GroupOrder::ArcherAttack { nation: 0, x: 1000, y: 1000, zone: 0, missed: false };
        a.f.st.groups[0].orders = vec![order];
        a.run_processes();
        assert_eq!(a.f.st.groups[0].orders.first().map(|o| o.kind()), Some(0x4152_4350));
        assert!(a.f.world.units[a.archer.0 as usize].orders.is_empty());
        let enemy = a.f.world.spawn(a.foot, 1, Pos::from_pixels(1200, 1000), 0);
        a.run_processes();
        assert_eq!(a.f.st.groups[0].orders.first().map(|o| o.kind()), Some(0x4152_4341));
        assert_eq!(a.f.world.units[a.archer.0 as usize].target(), Some(enemy));
    }

    #[test]
    fn top_list_en_army_counts_objects_and_lifts_whole_buildings() {
        const BARRACKS: &str = "NAME Bar\r\nBUILDING\r\nLIFE 1000\r\nLOCATION 0 0 64 64\r\n\
            LOCKPOINTS 4 0 0 1 0 0 1 1 1\r\nUNITABSORBER 10\r\nOPPOSIT 10 30\r\n[END]\r\n";
        let mut f = Fixture::new();
        let list = |f: &Fixture| [0, 1].map(|i| f.mem.read_u16(DATA + 2 * i).expect("list"));
        // West: 5 enemy soldiers and the enemy peasant count 6, the last one (the peasant) is
        // listed; the own soldier and the nation-7 house in the east do not count.
        f.call("CreateTopListEnArmy", &[DATA, 0, 6]);
        assert_eq!(list(&f), [5, NO_UNIT]);
        f.call("CreateTopListEnArmy", &[DATA, 0, 7]);
        assert_eq!(list(&f), [NO_UNIT, NO_UNIT]);
        // An undamaged enemy barracks lifts its area's count to the minimum, an empty one too.
        let mut c = (*f.world.catalog).clone();
        let bar = c.add(ObjectType::from_def("Bar(EN)", &UnitDef::parse(BARRACKS.as_bytes())));
        f.world.catalog = Arc::new(c);
        let b = f.world.spawn(bar, 1, Pos::from_pixels(3000, 2000), 0);
        f.call("CreateTopListEnArmy", &[DATA, 0, 7]);
        assert_eq!(list(&f), [NO_UNIT, b.0 as u16]);
        // Damaged, it counts 1.
        f.world.units[b.0 as usize].life -= 1;
        f.call("CreateTopListEnArmy", &[DATA, 0, 2]);
        assert_eq!(list(&f), [5, NO_UNIT]);
    }

    #[test]
    fn enemy_force_sums_the_cells_of_the_zone_rings() {
        const FORT: &str = "NAME Frc\r\nUSERLC 0 G SHADOW 0 0\r\n#STAND 9 1 0 0\r\n@MOTION_L 9 0 0 9\r\n\
            @MOTION_R 9 0 10 19\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nFORCE 3\r\n[END]\r\n";
        let mut f = Fixture::new();
        let mut c = (*f.world.catalog).clone();
        let frc = c.add(ObjectType::from_def("Frc(EN)", &UnitDef::parse(FORT.as_bytes())));
        f.world.catalog = Arc::new(c);
        f.world.spawn(frc, 1, Pos::from_pixels(1500, 2000), 0);
        f.world.spawn(frc, 1, Pos::from_pixels(1550, 2000), 0);
        f.world.spawn(frc, 0, Pos::from_pixels(1500, 2050), 0);
        f.world.spawn(frc, 1, Pos::from_pixels(1900, 2000), 0);
        f.world.step(&[]);
        // A point zone: x, y, then '@' << 24 | r.
        f.mem.write_u16(DATA + 0x300, 1500).expect("x");
        f.mem.write_u16(DATA + 0x302, 2000).expect("y");
        f.mem.write_u32(DATA + 0x304, 0x4000_0000 | 300).expect("r");
        // 3 + 3 enemies − 3 own; the enemy 400 px away is outside.
        assert_eq!(f.call("GetEnemyForce", &[DATA + 0x300, 0]) as i32, 3);
        assert_eq!(f.call("GetEnemyForce", &[DATA + 0x300, 1]) as i32, -3);
    }
}
