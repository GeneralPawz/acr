//! The AI landing (Brigade.cpp "DESANT"): a computer player's new transport gets a brigade
//! with the diversion order (`MakeDiversion` 0x425654, link 0x424957). The transport sails to
//! a coast place of its own island, takes the units of the AI's start group aboard
//! (`AssignDesantStartGroup`, at most `SetMaxInTransport`), sails to a coast place of an
//! enemy island, unloads them into the destination group (`AssignDesantDestGroup`) and sinks
//! some time later.
//!
//! Engine data used by the order:
//! - the islands: connected land areas (`ResearchIslands` 0x41f701, `TopIslands` 0x6ba1a8);
//! - each nation's island, the one with most of its objects (`CheckIsland` 0x41fd11,
//!   `City+0xeb15`);
//! - the coast places (`CreateCostPlaces` 0x423bcd, 0x6ba198): a water point and a land point
//!   10 cells apart across the shore, refreshed one per tick (`ProcessCostPoints` 0x4243fd).
//!
//! The script groups live in the script host: before a step it puts the start group's units
//! into [`DesantCity::start`] and afterwards takes [`DesantCity::taken`] and
//! [`DesantCity::landed`] back into the groups.

use crate::fixed::Pos;
use crate::map::LockMap;
use crate::trig::norma;
use crate::world::{EntityId, Order, World};

/// `B_DiversionOrder` (0x530 bytes): the transport's passengers and the landing's phase.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diversion {
    /// `IDX`/`USN` (+0x10/+0x290), `NU` (+0x510).
    pub units: Vec<EntityId>,
    /// `MaxU` (+0x514): `SetMaxInTransport` when the order was made.
    pub max_u: i32,
    /// `Phase` (+0x518): 0 loading at the own coast, 1 landing at an enemy coast.
    pub phase: u8,
    /// `CostPointIndex` (+0x524).
    pub cost_point: Option<usize>,
    /// `StartTime` (+0x528).
    pub start_time: i32,
    /// +0x52c: the last tick a passenger was still on its way.
    pub last_seen: i32,
}

/// A coast place (`CostPlace`, 21 bytes): coordinates in 16 px cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CostPlace {
    /// The water point (+0, +4).
    pub xw: i32,
    pub yw: i32,
    /// The land point (+8, +0xc).
    pub xc: i32,
    pub yc: i32,
    /// The island of the land point (+0x10); `None` where it has no land area (the engine
    /// leaves the byte uninitialised).
    pub island: Option<u8>,
    /// The free land radius around the land point (+0x11), 6..=12.
    pub r: i32,
    /// +0x12: the land around the point is free and few units of nation 0 stand there.
    pub access: bool,
    /// +0x13: the transport near the water point, 0xFFFF for none; 0 until the place is
    /// refreshed (the engine leaves it uninitialised, fresh memory).
    pub transport: u16,
}

/// A computer player's landing setup (`City` fields).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DesantCity {
    /// `AssignDesantStartGroup` (+0xf4e): the start group's units (alive or hidden, in group
    /// order), put here by the script host before every step; `None` without a start group.
    pub start: Option<Vec<EntityId>>,
    /// `AssignDesantDestGroup` (+0xf50) is set.
    pub dest: bool,
    /// Units taken from the end of the start group during the step (the host removes them
    /// from the group after cleaning it, as `0x56d0cc` does).
    pub taken: Vec<EntityId>,
    /// Units that landed (the host adds them to the destination group).
    pub landed: Vec<EntityId>,
    /// `SetMaxInTransport` (+0x189), 80 at the start.
    pub max_in_transport: i32,
    /// `TransportID/SN` (+0x185/+0x187): the docked transport that loads now.
    pub transport: Option<EntityId>,
    /// `MyIsland` (+0xeb15), 0xFF unknown.
    pub my_island: u8,
}

impl Default for DesantCity {
    fn default() -> Self {
        Self {
            start: None,
            dest: false,
            taken: Vec::new(),
            landed: Vec::new(),
            max_in_transport: 80,
            transport: None,
            my_island: 0xFF,
        }
    }
}

/// The landing data of the world.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Desant {
    pub places: Vec<CostPlace>,
    /// `TopIslands`: the island of each land area; empty without `TOP1`.
    pub islands: Vec<u8>,
    /// `NIslands`.
    pub n_islands: usize,
    pub cities: [DesantCity; 8],
    /// A mission script runs (`SCENINF.hLib` without `SetStandartVictory`): a computer
    /// player's island, once known, is not looked at again. Set by the host.
    pub mission: bool,
}

const NO_TRANSPORT: u16 = 0xFFFF;

/// `ArrangeAreas` (0x41f66a): marks the areas reachable from `id` through the links, at most
/// 100 links deep.
fn arrange(islands: &mut [u16], links: &[Vec<u16>], id: usize, island: u16, deep: i32) {
    islands[id] = island;
    if deep < 100 {
        for &l in &links[id] {
            let l = usize::from(l);
            if islands.get(l) == Some(&0xFFFF) {
                arrange(islands, links, l, island, deep + 1);
            }
        }
    }
}

/// `ResearchIslands` (0x41f701): numbers the connected groups of land areas. From each
/// unnumbered area the links are followed, then once more from every area of the island so
/// far (in index order) to get past the depth limit.
pub fn research_islands(links: &[Vec<u16>]) -> (Vec<u8>, usize) {
    let n = links.len();
    let mut isl = vec![0xFFFFu16; n];
    let mut count = 0usize;
    for j in 0..n {
        if isl[j] != 0xFFFF {
            continue;
        }
        let id = u16::from(count as u8);
        arrange(&mut isl, links, j, id, 0);
        for p in 0..n {
            if isl[p] == id {
                arrange(&mut isl, links, p, id, 0);
            }
        }
        count += 1;
    }
    (isl.iter().map(|&i| i as u8).collect(), count)
}

/// `CreateCostPlaces` (0x423bcd), at map load: every 32 px cell (15 cells from the edge) whose
/// four `SEA2` depths are partly deep (> 128) gives a coast direction from the depth gradient;
/// the cell is a place when 10 cells (16 px) towards the water a 12 × 12 box is open water,
/// 10 cells towards the land a 12 × 12 box is free land and no place was taken within 20
/// cells. The free land radius grows from 6 while the box around the land point stays free.
pub fn create_cost_places(w: &mut World) {
    let map = w.map.clone();
    let Some(depth) = map.water_depth.as_ref() else { return };
    let links: Vec<Vec<u16>> = map.land_areas.as_ref().map(|a| a.links.clone()).unwrap_or_default();
    let (islands, n) = research_islands(&links);
    w.desant.islands = islands;
    w.desant.n_islands = n;
    let mut taken = LockMap::new(map.land.width, map.land.height);
    let maxx = (240 << map.shift) - 15;
    let z = |x: i32, y: i32| i32::from(depth.get(x as usize, y as usize).unwrap_or(0));
    let mut places = Vec::new();
    for ix in 15..maxx {
        for iy in 15..maxx {
            let (z0, z1, z2, z3) = (z(ix, iy), z(ix + 1, iy), z(ix, iy + 1), z(ix + 1, iy + 1));
            let mixed = [z0, z1, z2, z3].iter().fold(0u8, |m, &v| m | if v > 128 { 1 } else { 2 });
            if mixed != 3 || taken.check_bar(2 * ix - 10, 2 * iy - 10, 20, 20) {
                continue;
            }
            let (dx, dy) = (z0 + z2 - z1 - z3, z0 + z1 - z2 - z3);
            let n = norma(dx, dy);
            if n == 0 {
                continue;
            }
            let (dx, dy) = (-(dx * 10) / n, -(dy * 10) / n);
            if map.water.check_bar(2 * ix + dx - 6, 2 * iy + dy - 6, 12, 12)
                || map.land.check_bar(2 * ix - dx - 6, 2 * iy - dy - 6, 12, 12)
            {
                continue;
            }
            let mut r = 6;
            while !map.land.check_bar(2 * ix - dx - r - 1, 2 * iy - dy - r - 1, 2 * r + 2, 2 * r + 2) {
                r += 1;
            }
            taken.set_bar(2 * ix - 10, 2 * iy - 10, 20);
            let (xc, yc) = (2 * ix - dx, 2 * iy - dy);
            let area = map.land_areas.as_ref().map_or(0xFFFF, |a| a.area_at(xc >> 2, yc >> 2));
            let island = (area < 0xFFFE).then(|| w.desant.islands.get(usize::from(area)).copied()).flatten();
            places.push(CostPlace { xw: 2 * ix + dx, yw: 2 * iy + dy, xc, yc, island, r, access: true, transport: 0 });
        }
    }
    w.desant.places = places;
}

/// The active units (not buildings) of the 128 px cells `(cx - 2 ..= cx + 2, cy - 2 ..= cy + 2)`
/// in row order, each cell's units in id order (the engine's cell lists `MCount`).
fn cells_around(w: &World, cx: i32, cy: i32) -> Vec<Vec<usize>> {
    let mut cells = vec![Vec::new(); 25];
    for (i, u) in w.units.iter().enumerate() {
        if !u.active() || u.building {
            continue;
        }
        let (x, y) = ((u.cell.0 >> 3) - cx + 2, (u.cell.1 >> 3) - cy + 2);
        if (0..5).contains(&x) && (0..5).contains(&y) {
            cells[(y * 5 + x) as usize].push(i);
        }
    }
    cells
}

/// `ProcessCostPoints` (0x4243fd), every tick before `tmtmt` advances: place
/// `tmtmt % NCost` is accessible when the land box of its radius around the land point is
/// free and at most 6 land units of nation 0 (not `CAPTURE` types) stand in the 5 × 5 cells
/// of 128 px around it (`CheckUnitsNearPoint` 0x424125); its transport is the first living
/// transport of the first cell around the water point that holds units (0xFFFF if that cell
/// has none, `CheckTransportNearPoint` 0x4242f9).
pub fn process_cost_points(w: &mut World) {
    let n = w.desant.places.len();
    if n == 0 {
        return;
    }
    let k = (w.tick.0 % n as u64) as usize;
    let p = w.desant.places[k];
    let catalog = w.catalog.clone();
    let mut access = !w.map.land.check_bar(p.xc - p.r, p.yc - p.r, 2 * p.r, 2 * p.r);
    let near: usize = cells_around(w, p.xc >> 3, p.yc >> 3)
        .iter()
        .flatten()
        .filter(|&&i| {
            let u = &w.units[i];
            let t = catalog.get(u.kind);
            u.owner == 0 && t.medium == crate::catalog::Medium::Land && !t.role.capture
        })
        .count();
    if near > 6 {
        access = false;
    }
    let mut transport = NO_TRANSPORT;
    if let Some(cell) = cells_around(w, p.xw >> 3, p.yw >> 3).iter().find(|c| !c.is_empty())
        && let Some(&i) = cell.iter().find(|&&i| catalog.get(w.units[i].kind).transport)
    {
        transport = i as u16;
    }
    let place = &mut w.desant.places[k];
    place.access = access;
    place.transport = transport;
}

/// `CheckIsland` (0x41fd11): the island with the most objects of the nation (every object of
/// its list on a land area counts, the first maximum wins), 0xFF without objects. A computer
/// player's known island stays during a mission.
pub fn check_island(w: &mut World, nation: u8) {
    let k = usize::from(nation & 7);
    if w.ai[k] && w.desant.cities[k].my_island != 0xFF && w.desant.mission {
        return;
    }
    let Some(areas) = w.map.land_areas.as_ref() else { return };
    if w.desant.islands.is_empty() {
        return;
    }
    let mut count = [0i32; 64];
    let mut any = false;
    for u in w.units.iter().filter(|u| u.alive && u.owner == nation) {
        any = true;
        let a = areas.area_at(u.pos.x >> 10, u.pos.y >> 10);
        if a < 0xFFFE
            && let Some(&i) = w.desant.islands.get(usize::from(a))
            && let Some(c) = count.get_mut(usize::from(i))
        {
            *c += 1;
        }
    }
    if !any {
        w.desant.cities[k].my_island = 0xFF;
        return;
    }
    let (mut best, mut island) = (0, 0xFF);
    for (i, &c) in count.iter().enumerate().take(w.desant.n_islands.min(64)) {
        if c > best {
            (best, island) = (c, i as u8);
        }
    }
    w.desant.cities[k].my_island = island;
}

/// The engine's per-tick landing upkeep (main loop 0x4a63bb, 0x4a651a), before `tmtmt`
/// advances: one coast place, and every 256 ticks one nation's island.
pub fn tick(w: &mut World) {
    process_cost_points(w);
    crate::combat::slow_death(w);
    let tmtmt = w.tick.0;
    if tmtmt & 0xff == 1 {
        check_island(w, ((tmtmt >> 8) & 7) as u8);
    }
}

/// `FindCostPoint` (0x42469f): the accessible place without a transport on island `isl`
/// whose water point is nearest to `(x, y)` (16 px cells) but more than 20 cells away.
fn find_cost_point(w: &World, x: i32, y: i32, isl: u8) -> Option<usize> {
    let mut best = (100_000, None);
    for (i, p) in w.desant.places.iter().enumerate() {
        let r = norma(x - p.xw, y - p.yw);
        if r > 20 && r < best.0 && p.access && p.island == Some(isl) && p.transport == NO_TRANSPORT {
            best = (r, Some(i));
        }
    }
    best.1
}

/// `FindCostPointEx` (0x424789): as [`find_cost_point`] on the islands of the nations 0..6
/// not allied with `mask` whose island is known.
fn find_cost_point_ex(w: &World, x: i32, y: i32, mask: u8) -> Option<usize> {
    let mut set = [false; 256];
    for i in 0..7 {
        let isl = w.desant.cities[i].my_island;
        if w.nation_mask[i] & mask == 0 && isl != 0xFF {
            set[usize::from(isl)] = true;
        }
    }
    let mut best = (100_000, None);
    for (i, p) in w.desant.places.iter().enumerate() {
        let r = norma(x - p.xw, y - p.yw);
        if r > 20
            && r < best.0
            && p.access
            && p.transport == NO_TRANSPORT
            && p.island.is_some_and(|isl| set[usize::from(isl)])
        {
            best = (r, Some(i));
        }
    }
    best.1
}

/// `City::RegisterNewUnit` (0x42dda3) for the new objects of a computer player: a transport
/// gets a brigade with the diversion order (0x42ded0). The other branches only fill the
/// city's lists, which the AI DLLs (all with a units collector) do not use.
pub fn register_new_unit(w: &mut World, i: usize) {
    let u = &w.units[i];
    let k = usize::from(u.owner & 7);
    let t = w.catalog.get(u.kind);
    if !w.ai[k] || u.building || !t.transport || t.peasant || t.officer || t.drummer {
        return;
    }
    let order = Diversion {
        units: Vec::new(),
        max_u: w.desant.cities[k].max_in_transport,
        phase: 0,
        cost_point: None,
        start_time: 10_000_000,
        last_seen: 0,
    };
    let owner = u.owner;
    let id = u.id;
    let number = crate::formation::free_number(w, owner);
    w.brigades.push(crate::formation::Brigade {
        owner,
        alive: true,
        shape: 0,
        dir: 0,
        members: vec![Some(id)],
        slots: Vec::new(),
        orders: vec![crate::formation::BOrder::Diversion(Box::new(order))],
        att_enm: false,
        shooters: false,
        kind: None,
        bonus: (0, 0),
        stand_timer: 0,
        stamp: 0,
        number,
    });
}

/// The transport's landing point when it is docked (`DstX > 0`), in 1/16 px.
fn docked(w: &World, t: usize) -> Option<Pos> {
    match w.units[t].dock {
        crate::ship::Dock::At(p) if p.x > 0 => Some(p),
        _ => None,
    }
}

/// `NewMonsterSmartSendTo(x, y, 0, 0, 0x90, 0)` (0x52bc95) to a point in pixels.
fn send(w: &mut World, i: usize, x: i32, y: i32) {
    let map = w.map.clone();
    let water = w.catalog.get(w.units[i].kind).medium == crate::catalog::Medium::Water;
    let areas = if water { map.water_areas.as_ref() } else { map.land_areas.as_ref() };
    crate::motion::smart_send_to(&mut w.units[i], areas, x, y, 0, 0, 0x90);
}

fn diversion(w: &mut World, b: usize) -> &mut Diversion {
    match w.brigades[b].orders.last_mut() {
        Some(crate::formation::BOrder::Diversion(d)) => d,
        _ => unreachable!("not a diversion brigade"),
    }
}

/// `MakeDiversionLink` (0x424957), from the owner's `ExecuteBrigades`.
pub fn diversion_link(w: &mut World, b: usize) {
    let k = usize::from(w.brigades[b].owner & 7);
    // Nothing during the peace time (`PeaceTimeLeft`, 0x66795c).
    if w.peace.active() || w.desant.cities[k].start.is_none() {
        return;
    }
    let Some(&Some(tid)) = w.brigades[b].members.first() else {
        // `DeleteAll`, disabled.
        w.brigades[b].members.clear();
        w.brigades[b].orders.clear();
        w.brigades[b].alive = false;
        return;
    };
    let t = tid.0 as usize;
    let tmtmt = (w.tick.0 + 1) as i32;
    // `TransportID`: dropped when gone or not docked.
    let mut cur = w.desant.cities[k].transport;
    if let Some(c) = cur
        && !(w.units.get(c.0 as usize).is_some_and(|u| u.alive) && docked(w, c.0 as usize).is_some())
    {
        cur = None;
        w.desant.cities[k].transport = None;
    }
    if !w.units[t].living() {
        w.brigades[b].members.clear();
        return;
    }
    let dock = docked(w, t);
    if w.units[t].stand_time > 200 && dock.is_none() {
        crate::combat::die(w, t);
        return;
    }
    if w.units[t].delay > 100 && w.units[t].delay < 130 {
        crate::combat::die(w, t);
        return;
    }
    let phase = diversion(w, b).phase;
    if let (Some(_), 0) = (dock, phase) {
        load(w, b, k, t, cur, tmtmt);
        return;
    }
    let (x, y) = (w.units[t].pos.x >> 8, w.units[t].pos.y >> 8);
    if dock.is_none() && phase == 0 {
        // To a coast place of the own island.
        if diversion(w, b).cost_point.is_none() {
            let isl = w.desant.cities[k].my_island;
            let cp = find_cost_point(w, x, y, isl);
            diversion(w, b).cost_point = cp;
            if let Some(cp) = cp {
                let p = w.desant.places[cp];
                send(w, t, p.xw << 4, p.yw << 4);
            }
        }
        if let Some(cp) = diversion(w, b).cost_point {
            let p = w.desant.places[cp];
            if !p.access || (p.transport != NO_TRANSPORT && p.transport != t as u16) {
                diversion(w, b).cost_point = None;
            } else if w.units[t].orders.is_empty() {
                if norma(x - p.xw, y - p.yw) < 18 {
                    send(w, t, p.xc << 4, p.yc << 4);
                } else {
                    send(w, t, p.xw << 4, p.yw << 4);
                }
            }
        }
    }
    if diversion(w, b).phase == 1 && docked(w, t).is_none() {
        // To a coast place of an enemy island.
        if w.random.rando() < 0x200 {
            diversion(w, b).cost_point = None;
        }
        if diversion(w, b).cost_point.is_none() {
            let cp = find_cost_point_ex(w, x, y, 1 << k);
            diversion(w, b).cost_point = cp;
            if let Some(cp) = cp {
                let p = w.desant.places[cp];
                send(w, t, p.xw << 4, p.yw << 4);
            }
        }
        if let Some(cp) = diversion(w, b).cost_point {
            let p = w.desant.places[cp];
            if !p.access || (p.transport != NO_TRANSPORT && p.transport != t as u16) {
                diversion(w, b).cost_point = None;
            } else {
                let nn = norma(x - p.xw, y - p.yw);
                let idle = w.units[t].orders.is_empty();
                if nn < 40 {
                    if idle {
                        send(w, t, p.xc << 4, p.yc << 4);
                    }
                } else if idle {
                    send(w, t, p.xw << 4, p.yw << 4);
                }
            }
        }
    }
    if diversion(w, b).phase == 1 && docked(w, t).is_some() {
        land(w, b, k, t);
    }
}

/// Phase 0 at the own coast (0x424b02..0x425066): one unit of the start group per call while
/// this transport is the city's loading one; the passengers walk to the landing point and
/// board. Full (or 512 ticks without anybody on the way) it sets off.
fn load(w: &mut World, b: usize, k: usize, t: usize, cur: Option<EntityId>, tmtmt: i32) {
    let tid = w.units[t].id;
    let (nu, max_u) = (diversion(w, b).units.len() as i32, diversion(w, b).max_u);
    if nu < max_u {
        if cur == Some(tid) {
            let taken = w.desant.cities[k].start.as_mut().and_then(|s| s.pop());
            if let Some(uni) = taken {
                w.desant.cities[k].taken.push(uni);
                let i = uni.0 as usize;
                // `UnRegisterNewUnit` (0x42e2bd): out of its brigade.
                crate::formation::remove_member(w, i);
                if let Some(p) = docked(w, t) {
                    send(w, i, p.x >> 4, p.y >> 4);
                }
                diversion(w, b).units.push(uni);
            }
        } else if cur.is_none() {
            w.desant.cities[k].transport = Some(tid);
        }
    } else {
        if cur == Some(tid) {
            w.desant.cities[k].transport = None;
        }
        let last = diversion(w, b).last_seen;
        if w.units[t].inside.len() as i32 >= max_u || tmtmt - last > 0x200 {
            let d = diversion(w, b);
            d.phase = 1;
            d.cost_point = None;
            w.units[t].dock = crate::ship::Dock::Away;
            w.units[t].stand_time = 0;
        }
    }
    // Control of loading: idle passengers (or gathering ones) are sent again.
    let target = match w.units[t].dock {
        crate::ship::Dock::At(p) => p,
        crate::ship::Dock::Away => Pos::new(-1, -1),
        crate::ship::Dock::Unset => Pos::new(0, 0),
    };
    let mut q = 0;
    while q < diversion(w, b).units.len() {
        let i = diversion(w, b).units[q].0 as usize;
        let exists = w.units.get(i).is_some_and(|u| u.alive);
        if exists && w.units[i].active() {
            let u = &w.units[i];
            if u.orders.is_empty() || matches!(u.orders.last(), Some(Order::Gather { .. })) {
                if norma(u.pos.x - target.x, u.pos.y - target.y) < 0xa00 {
                    crate::ship::go_to_transport(w, i, t, 0x90);
                } else {
                    send(w, i, target.x >> 4, target.y >> 4);
                }
            }
            diversion(w, b).last_seen = tmtmt;
        }
        if !exists || !w.units[i].living() {
            diversion(w, b).units.remove(q);
        } else {
            q += 1;
        }
    }
    if !w.units[t].inside.is_empty() {
        let d = diversion(w, b);
        if d.start_time > tmtmt {
            d.start_time = tmtmt;
        } else if tmtmt - d.last_seen > 0x100 {
            // Nobody on the way for 256 ticks: the units not aboard go back to the city
            // (`RegisterNewUnit`, a no-op with a units collector) and the load is complete.
            let units = std::mem::take(&mut d.units);
            let kept: Vec<EntityId> = units.into_iter().filter(|e| !w.units[e.0 as usize].active()).collect();
            let d = diversion(w, b);
            d.units = kept;
            d.max_u = d.units.len() as i32;
            d.start_time = tmtmt;
        }
    }
}

/// Phase 1 docked at the enemy coast (0x4253f9..0x42562b): everybody off; the passengers on
/// land join the destination group. Empty, the transport is free again and sinks some time
/// later (`delay` 350, see the checks at the start of the link).
fn land(w: &mut World, b: usize, k: usize, t: usize) {
    if !w.units[t].inside.is_empty() && w.units[t].orders.is_empty() {
        crate::ship::leave_all(w, t);
    }
    let mut on_way = 0;
    let mut q = 0;
    while q < diversion(w, b).units.len() {
        let i = diversion(w, b).units[q].0 as usize;
        let u = &w.units[i];
        if u.alive && u.brigade.is_none() {
            if u.inside_of.is_some() || u.unlimited {
                on_way += 1;
            } else if w.desant.cities[k].dest {
                w.desant.cities[k].landed.push(u.id);
                diversion(w, b).units.remove(q);
                continue;
            }
        }
        q += 1;
    }
    if w.units[t].inside.is_empty() && on_way == 0 {
        let u = &mut w.units[t];
        u.dock = crate::ship::Dock::Away;
        u.delay = 350;
        u.stand_time = 0;
        let d = diversion(w, b);
        d.phase = 0;
        d.cost_point = None;
        d.units.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coast_places_along_a_straight_shore() {
        // Deep water west of the 32 px column 200 (16 px cell 400), land east of it.
        let mut g = crate::map::MapGrid::open(1);
        let side = g.land.width;
        g.water.clr_rect(1, 1, 399, side - 2);
        g.land.set_rect(0, 0, 400, side);
        let n = 512;
        let cells = (0..n * n).map(|i| if i % n < 200 { 255 } else { 0 }).collect();
        g.water_depth = Some(acr_formats::m3d::Grid { width: n, height: n, cells });
        let mut w = World::new(
            std::sync::Arc::new(g),
            std::sync::Arc::new(crate::Catalog::new()),
            std::sync::Arc::new(crate::formation::Formations::default()),
        );
        create_cost_places(&mut w);
        let p = &w.desant.places;
        // One column (x 199) every 10 cells from y 15 (the taken 20 × 20 boxes), the water
        // point 10 cells (16 px) west of the shore, the land point 10 east, free land 8 around.
        assert_eq!(p.len(), 45);
        assert_eq!((p[0].xw, p[0].yw, p[0].xc, p[0].yc, p[0].r), (388, 30, 408, 30, 8));
        assert_eq!((p[1].yw, p[44].yw), (50, 910));
        assert!(p.iter().all(|c| c.island.is_none() && c.access && c.transport == 0));
    }

    #[test]
    fn islands_follow_the_links() {
        // 0-1-2 connected, 3 alone, 4-5 connected.
        let links = vec![vec![1], vec![0, 2], vec![1], vec![], vec![5], vec![4]];
        let (isl, n) = research_islands(&links);
        assert_eq!(isl, vec![0, 0, 0, 1, 2, 2]);
        assert_eq!(n, 3);
    }

    #[test]
    fn deep_chains_are_joined_by_the_second_pass() {
        // A chain of 150 areas: the depth limit stops the first walk at 101 areas, the walk
        // from every area of the island joins the rest.
        let n = 150;
        let links: Vec<Vec<u16>> = (0..n)
            .map(|i| {
                let mut l = Vec::new();
                if i > 0 {
                    l.push(i as u16 - 1);
                }
                if i + 1 < n {
                    l.push(i as u16 + 1);
                }
                l
            })
            .collect();
        let (isl, count) = research_islands(&links);
        assert_eq!(count, 1);
        assert!(isl.iter().all(|&i| i == 0));
    }
}
