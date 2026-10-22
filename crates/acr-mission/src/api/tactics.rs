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
    f("CreateDangerMap", 0x57c3ea, Status::Partial("no garrisons/resource bases"), create_danger_map),
    f(
        "CreateDangerMapForDef",
        0x57c7c0,
        Status::Partial("enemy units weigh (STORMFORCE >> 5) | 1; the rest follows CreateDangerMap"),
        create_danger_map_for_def,
    ),
    f("CreateDangerMapForTom", 0x57c9ee, Status::Partial("as CreateDangerMap"), create_danger_map),
    f(
        "EnumEnemyArmies",
        0x578245,
        Status::Partial("one army per enemy nation (centre, count, force = count); the original clusters 256-px cells"),
        enum_enemy_armies,
    ),
    f("Attack3", 0x57ea15, DONE, attack3),
    f("Attack4", 0x580042, DONE, attack4),
    f(
        "Attack6",
        0x580b71,
        Status::Partial("ranged members enter empty buildings only of their own nation (no capture)"),
        attack3,
    ),
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
        Status::Partial(
            "one attack on the nearest enemy (0x90); the per-tick FindArcherTarget process is not reproduced",
        ),
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
    // The weight of an object is `weight[CATEGORY]` (only `ENGBLG*` have category 1).
    let mut cat_w = [0i32; 2];
    if !def && weights != 0 {
        for (k, w) in cat_w.iter_mut().enumerate() {
            *w = c.read_u32(weights + 4 * k as u32)? as i32;
        }
    }
    let mut m = vec![0i32; nzones];
    for u in api.world.units.iter().filter(|u| u.living()) {
        let z = zone_at(api, u.pos.x >> 10, u.pos.y >> 10);
        let Some(v) = m.get_mut(usize::from(z)) else { continue };
        let enemy = api.st.unit_nmask(u.id, u.owner) & mask == 0;
        let t = api.world.catalog.get(u.kind);
        let w0 = cat_w[usize::from(t.role.category.min(1))];
        *v = i32::from(if def {
            // CreateDangerMapForDef: `(STORMFORCE >> 5) | 1` per enemy unit.
            if enemy && !u.building { (*v + ((t.role.storm_force >> 5) | 1)) as i16 } else { *v as i16 }
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
fn clear_orders(api: &mut Api, i: usize) {
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
fn clear_brigade_orders(api: &mut Api, i: usize) {
    if let Some(b) = api.world.units[i].brigade {
        api.world.brigades[usize::from(b)].orders.clear();
    }
}

fn attack_obj(api: &mut Api, i: usize, t: usize, prio: u8, p: acr_sim::combat::Placement) -> bool {
    acr_sim::combat::attack_obj_ex(api.world, i, t, prio, p)
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
    let times = u8::from(!six);
    let _ = times;
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
                        // Attack6 sends ranged members into an empty building (GoToMine): here
                        // only into one of their own nation.
                        let b = t;
                        if api.world.units[b].owner == api.world.units[i].owner {
                            acr_sim::economy::enter(api.world, i, b, 0xFF, false);
                            prio_f0(api, i);
                        }
                    } else if attack_obj(api, i, t, 0x90, acr_sim::combat::Placement::Replace) {
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
        if attack_obj(api, i, t, 0x90, acr_sim::combat::Placement::Replace) {
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
                if attack_obj(api, i, t, 0x8F, acr_sim::combat::Placement::Replace) {
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
            attack_obj(api, i, t, 0x90, acr_sim::combat::Placement::Front);
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

/// `bool SGP_AttackByArchers(byte nat, GAMEOBJ* grp, int, int)`: ACR attacks the nearest enemy
/// of the group once with priority 0x90 (the original installs a per-tick target search,
/// `'ARCA'`).
fn sgp_attack_by_archers(api: &mut Api, c: &mut Call) -> Result<u32> {
    let Some(g) = api.group(c, 1)? else { return Ok(0) };
    let ids = living_members(api, g);
    if ids.is_empty() {
        return Ok(0);
    }
    let n = ids.len() as i64;
    let x = (ids.iter().map(|&i| i64::from(api.world.units[i].pos.x >> 4)).sum::<i64>() / n) as i32;
    let y = (ids.iter().map(|&i| i64::from(api.world.units[i].pos.y >> 4)).sum::<i64>() / n) as i32;
    let mask = nat_mask(api, c.u8(0)?);
    let z = Circle { x, y, r: 960 };
    let targets: Vec<usize> = api
        .world
        .units
        .iter()
        .filter(|u| u.living() && api.st.unit_nmask(u.id, u.owner) & mask == 0 && z.contains(u))
        .map(|u| u.id.0 as usize)
        .collect();
    for i in ids {
        let up = api.world.units[i].pos;
        if let Some(&t) =
            targets.iter().min_by_key(|&&t| norma(up.x - api.world.units[t].pos.x, up.y - api.world.units[t].pos.y))
        {
            attack_obj(api, i, t, 0x90, acr_sim::combat::Placement::Replace);
        }
    }
    Ok(1)
}
