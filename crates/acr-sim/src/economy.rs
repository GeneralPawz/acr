//! Economy as the engine runs it: the per-player stock, the census every 8 ticks
//! (`City::Census` 0x42e658: housing, population, upkeep rates, desertion), the per-tick
//! upkeep (0x4a6b8d: food per head, `RESCONSUMER`), starvation (0x511ab3), prices
//! (`GetUnitCost` 0x514781), production queues (`Produce` 0x49e649, `ProduceObjLink`
//! 0x49eeb7, `UnProduce` 0x4dbd47), mines (`TakeResourceFromSpriteLink` 0x5f75b3) and units
//! entering buildings (`GoToMine` 0x5f43d0). See `docs/simulation.md`.

use crate::catalog::{ObjectType, TypeId};
use crate::combat::die;
use crate::fixed::Pos;
use crate::path::{Mover, create_path, dist_to};
use crate::trig::{get_dir, norma};
use crate::world::{AnimKind, EntityId, FRM_DEC, Order, PLAYERS, SPEED_SH, Unit, World};

/// Resource indices (`NRES.DAT [RESTYPES]` order).
pub const WOOD: usize = 0;
pub const GOLD: usize = 1;
pub const STONE: usize = 2;
pub const FOOD: usize = 3;
pub const IRON: usize = 4;
pub const COAL: usize = 5;

/// Food eaten per unit per tick, in thousandths (`[CONST] RES/UNIT FOOD 3`).
const FOOD_PER_UNIT: i32 = 3;
/// Priority of the orders units get when they leave a building.
const SPAWN_PRIO: u8 = 16;

/// A unit being produced (`ProduceObjLink` order).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Production {
    pub kind: TypeId,
    /// Progress, +2 per tick, done at `stages` (the unit's `BUILDSTAGES`).
    pub progress: i32,
    pub stages: i32,
    /// Number of objects of the type when it was ordered: the refund price.
    pub power: u16,
}

/// A mine bound to a deposit (`TakeResourceFromSprite`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mine {
    /// Resource index.
    pub resource: usize,
    /// Accumulator (u16 in the engine), paid out per 1000.
    pub acc: u16,
}

/// A resource deposit on the map (mine sites, `[INTERNAL_SOURCE]` sprites).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Deposit {
    /// World pixels.
    pub x: i32,
    pub y: i32,
    pub resource: usize,
}

/// The lock base cell of an object (0x516fde): spawn and gathering points are relative to it.
pub fn base_cell(t: &ObjectType, pos: Pos) -> (i32, i32) {
    ((pos.x + (t.location.0 << 4)) >> 8, (pos.y + ((2 * t.location.1) << 4)) >> 8)
}

/// `City::ProcessCreation` (0x432582) for one player: `EnumUnits` (0x42e658: housing,
/// population, upkeep rates, type counts; players short of gold lose `NAEMNIK` units, which
/// desert to player 7, and `TURNOFF` units stop), then `RefreshAbility` (0x42ffed).
pub fn census(w: &mut World, player: u8) {
    let catalog = w.catalog.clone();
    let n = usize::from(player);
    let short = w.players[n].gold_short;
    let mut counts = vec![0u16; catalog.len()];
    let mut ready = vec![0u16; catalog.len()];
    let (mut housing, mut population, mut speed) = (0, 0, [0i32; 8]);
    let mut deserters = Vec::new();
    let mut turned = Vec::new();
    for (i, u) in w
        .units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.alive && u.owner == player && (u.death == 0 || u.inside_of.is_some()))
    {
        let t = catalog.get(u.kind);
        let e = &t.economy;
        let k = usize::from(u.kind.0);
        counts[k] = counts[k].saturating_add(1);
        let max = match w.players[n].adv.get(&u.kind) {
            Some(a) => a.build_stages,
            None => e.build_stages,
        };
        if !t.building || u.stage >= max {
            ready[k] = ready[k].saturating_add(1);
            if e.is_farm {
                let farm = w.players[n].adv.get(&u.kind).map_or(e.farm, |a| a.farm);
                housing += farm + u.farm_extra;
            }
            if !t.building {
                population += 1;
            }
        }
        if let Some((r, v)) = e.consumer {
            speed[r] += v;
        }
        turned.push((i, short && e.turn_off));
        if short && e.mercenary {
            deserters.push(i);
        }
    }
    for (i, off) in turned {
        w.units[i].turned_off = off;
    }
    for i in deserters {
        w.units[i].owner = 7;
    }
    let p = &mut w.players[n];
    p.housing = housing;
    p.population = population;
    p.consumption = speed;
    p.type_count = counts;
    p.ready_count = ready;
    crate::tech::refresh_ability(w, player);
}

/// The per-tick upkeep (0x4a6b8d) of every player: food per head, then `RESCONSUMER`.
pub fn upkeep(w: &mut World) {
    let mult = 2000 >> SPEED_SH;
    for p in &mut w.players {
        p.harch = p.harch.wrapping_add((p.population * FOOD_PER_UNIT) as u16);
        let d = i32::from(p.harch) / mult;
        if d != 0 {
            if p.resources[FOOD] > d {
                p.resources[FOOD] -= d;
                p.harch = p.harch.wrapping_sub((mult * d) as u16);
                if p.harch == 0 {
                    p.harch = 1;
                }
            } else {
                p.resources[FOOD] = 0;
                p.harch = 0;
            }
        }
        if p.harch == 0 && p.resources[FOOD] != 0 {
            p.harch = 1;
        }
        let mult = 2_000_000 >> SPEED_SH;
        for j in 0..8 {
            let r = p.consumption_rest[j] + p.consumption[j] * 100;
            p.resources[j] -= r / mult;
            if p.resources[j] < 0 {
                p.resources[j] = 0;
            }
            p.consumption_rest[j] = r % mult;
            if j == GOLD {
                p.gold_short = p.resources[GOLD] < 2;
            }
        }
    }
}

/// Starvation (0x511ab3), in the object loop: while a player has no food left (`Harch` 0),
/// each of its units dies with probability 51/32768 per tick unless it is `NO_HUNGRY`.
pub fn starve(w: &mut World, i: usize) {
    let u = &w.units[i];
    if u.building || w.players[usize::from(u.owner)].harch != 0 {
        return;
    }
    if w.random.rando() <= 50 && !w.catalog.get(u.kind).economy.no_hungry {
        // City+0xeb16 (0x511b69): hunger frightens the nation for a while.
        w.players[usize::from(u.owner)].hungry_time = (w.tick.0 + 1) as u32;
        let record = crate::combat::DeathRecord::of(crate::combat::DeathCause::Hunger, None);
        crate::combat::record_death(w, i, record);
        die(w, i);
    }
}

/// The price after `n` existing objects of a type with base price `base` (each multiplies
/// by `COSTPERCENT`).
pub fn cost_after(t: &ObjectType, base: [i32; 8], n: u32) -> [i32; 8] {
    let e = &t.economy;
    let mut cost = base;
    if e.cost_percent == 100 {
        return cost;
    }
    let mut v: i32 = 1000;
    for _ in 0..n {
        if v < 1_000_000 {
            v = v.wrapping_mul(e.cost_percent) / 100;
            for c in &mut cost {
                *c = c.wrapping_mul(e.cost_percent) / 100;
                if *c < 0 {
                    *c = 10_000_000;
                }
            }
        }
    }
    cost
}

/// `GetUnitCost` (0x514781): the price for `player` now; `at` (world px) applies
/// `LOWCOSTRADIUS` for buildings being placed.
pub fn unit_cost(w: &World, player: u8, kind: TypeId, at: Option<(i32, i32)>) -> [i32; 8] {
    let t = w.catalog.get(kind);
    let mut n = u32::from(w.players[usize::from(player)].type_count.get(usize::from(kind.0)).copied().unwrap_or(0));
    if let Some((x, y)) = at.filter(|_| t.economy.low_cost_radius != 0) {
        let near = w.units_of(player).any(|u| {
            let (ux, uy) = u.pos.pixels();
            u.kind == kind && norma(ux - x, uy - y) < t.economy.low_cost_radius
        });
        if !near {
            n = 0;
        }
    }
    cost_after(t, w.adv(player, kind).cost, n)
}

/// `CanAfford` + `ApplyCost` (0x514e81).
fn pay(w: &mut World, player: u8, cost: &[i32; 8]) -> bool {
    let stock = &mut w.players[usize::from(player)].resources;
    if stock.iter().zip(cost).any(|(s, c)| s < c) {
        return false;
    }
    for (s, c) in stock.iter_mut().zip(cost) {
        *s -= c;
    }
    true
}

/// Whether building `b` may produce `kind` now: the type is in its `[FIXED_PRODUCE]` list
/// (checked by the produce command 0x4dbac2 through 0x49e060), enabled for the owner (the
/// command panel only offers enabled types), and the building has done every stage of the
/// type's `[STAGEDEPENDENT]` mask (`Produce` 0x49e649). Without nation data everything may
/// be produced.
pub fn can_produce(w: &World, b: usize, kind: TypeId) -> bool {
    let tech = &w.catalog.tech;
    if !tech.loaded {
        return true;
    }
    let u = &w.units[b];
    let p = &w.players[usize::from(u.owner)];
    tech.of(u.kind).is_some_and(|t| t.produce.contains(&kind))
        && p.enabled.get(usize::from(kind.0)).copied().unwrap_or(false)
        && stages_done(u.stages, tech.of(kind).map_or(0, |t| t.stage_mask))
}

/// Whether every stage `s` in `mask` is in state 2 (done) in the 3-bit fields of `stages`.
pub fn stages_done(stages: u16, mask: u8) -> bool {
    (0..5).all(|s| mask & (1 << s) == 0 || (stages >> (3 * s)) & 7 == 2)
}

/// The types building `b` can produce now, in menu order (for the command panel).
pub fn producible(w: &World, b: usize) -> Vec<TypeId> {
    let u = &w.units[b];
    match w.catalog.tech.of(u.kind) {
        Some(t) if w.catalog.tech.loaded => t.produce.iter().copied().filter(|&k| can_produce(w, b, k)).collect(),
        _ => Vec::new(),
    }
}

/// `Produce` (0x49e649): queues a unit in a finished building, paying the full price now.
pub fn produce(w: &mut World, b: usize, kind: TypeId) -> bool {
    let u = &w.units[b];
    if !u.living() || !u.building || u.stage < w.max_stage(u) || u.research.is_some() {
        return false;
    }
    if !stages_done(u.stages, w.catalog.tech.of(kind).map_or(0, |t| t.stage_mask)) {
        return false;
    }
    let player = u.owner;
    let power = w.players[usize::from(player)].type_count.get(usize::from(kind.0)).copied().unwrap_or(0);
    let cost = unit_cost(w, player, kind, None);
    if !pay(w, player, &cost) {
        return false;
    }
    let stages = w.adv(player, kind).build_stages;
    w.units[b].queue.push(Production { kind, progress: 0, stages, power });
    true
}

/// `UnProduce` (0x4dbd47): cancels one queued order of `kind` (a waiting one first, else
/// the running one) with a full refund; progress is lost.
pub fn unproduce(w: &mut World, b: usize, kind: TypeId) {
    let q = &w.units[b].queue;
    let Some(k) = (1..q.len()).find(|&k| q[k].kind == kind).or_else(|| (q.first()?.kind == kind).then_some(0)) else {
        return;
    };
    let order = w.units[b].queue.remove(k);
    let owner = w.units[b].owner;
    let refund = cost_after(w.catalog.get(kind), w.adv(owner, kind).cost, u32::from(order.power));
    let stock = &mut w.players[usize::from(w.units[b].owner)].resources;
    for (s, c) in stock.iter_mut().zip(refund) {
        *s = s.saturating_add(c);
    }
}

/// The engine part of `City::ProcessCreation` (0x432582) for nation `k = tmtmt & 7`
/// (0..6), every nation, AI or not: the gathering speeds, one `rando`, for computer players
/// `HandleFields` (0x431ead: `MakeSlowZasev` with chance 1/8 when the city has mills), then
/// `CreateFields` with chance 1/16 (0x4a69b4).
pub fn city_tick(w: &mut World) {
    let tmtmt = w.tick.0 + 1;
    let k = (tmtmt & 7) as u8;
    if k == 7 {
        return;
    }
    let gt = tmtmt.wrapping_mul(2) as u32;
    let p = &mut w.players[usize::from(k)];
    let dt = gt.wrapping_sub(p.gather_since) as i32;
    if dt > 1000 {
        for s in 0..3 {
            p.gather_speed[s] = p.gathered[s] * 25 / dt;
        }
        p.gathered = [0; 3];
        p.gather_since = gt;
    }
    w.random.rando();
    if w.ai[usize::from(k)] {
        // The city's mills (`MelnID`, at most 4).
        let mills = crate::aiplace::ready_mills(w, k);
        if !mills.is_empty() && w.random.rando() < 0x1000 {
            for b in mills {
                if let Some(field) = mill_product(w, b) {
                    let pos = w.units[b].pos;
                    create_fields_slow(w, k, pos.x, pos.y, field);
                }
            }
        }
    }
    if w.random.rando() < 0x800 {
        create_fields(w, k);
    }
}

fn is_ready_mill(w: &World, i: usize, k: u8) -> bool {
    let u = &w.units[i];
    u.owner == k && u.active() && u.building && u.stage >= w.max_stage(u) && w.catalog.get(u.kind).economy.mill
}

/// The first type a building produces (`PAble[NIndex][0]`: fields for a mill).
fn mill_product(w: &World, b: usize) -> Option<TypeId> {
    w.catalog.tech.of(w.units[b].kind).and_then(|t| t.produce.first().copied())
}

/// `CreateFields` (0x577ff4): every ready mill of the nation plants its fields around it
/// (`CreateFieldsAt` 0x51522f, each field paid).
pub fn create_fields(w: &mut World, k: u8) {
    for b in 0..w.units.len() {
        if is_ready_mill(w, b, k)
            && !w.units[b].no_sowing
            && let Some(field) = mill_product(w, b)
        {
            let pos = w.units[b].pos;
            crate::sprite::plant_fields(w, k, pos.x, pos.y, field);
        }
    }
}

/// `CreateFieldsSlow` (0x5153cd): 20 fields at random points of the planting diamond (or the
/// 7 × 7 grid points with chance `0x50/32768` each), each paid.
fn create_fields_slow(w: &mut World, k: u8, x16: i32, y16: i32, field: TypeId) {
    match w.catalog.get(field).economy.ex_sprites.clone() {
        Some(ex) => {
            let d = (ex.n * ex.step) << 4;
            for _ in 0..20 {
                let a = ((w.random.rando() * (2 * d + 1)) >> 15) - d;
                let b = ((w.random.rando() * (2 * d + 1)) >> 15) - d;
                crate::sprite::create_sprite_object(w, k, field, x16 + a + b, y16 + a - b, false);
            }
        }
        None => {
            for i in -3..=3 {
                for j in -3..=3 {
                    if w.random.rando() < 0x50 {
                        crate::sprite::create_sprite_object(
                            w,
                            k,
                            field,
                            x16 + ((i + j) << 10),
                            y16 + ((i - j) << 10),
                            false,
                        );
                    }
                }
            }
        }
    }
}

/// `GetExtraction` (0x5699eb) for nation `k`: the food and wood gathering speeds, for stone,
/// iron, coal and gold the peasants inside mines of that resource (`WasIn*`).
pub fn extraction(w: &World, k: u8, res: usize) -> i32 {
    let p = &w.players[usize::from(k) & 7];
    match res {
        FOOD => p.gather_speed[2],
        WOOD => p.gather_speed[0],
        STONE | IRON | COAL | GOLD => w
            .units
            .iter()
            .filter(|u| u.owner == k && u.living() && u.mine.is_some_and(|m| m.resource == res))
            .map(|u| u.inside.iter().filter(|e| w.catalog.get(w.units[e.0 as usize].kind).peasant).count() as i32)
            .sum(),
        _ => 0,
    }
}

/// A building's work for one tick: the head of its production queue, then its mine.
pub fn building_link(w: &mut World, b: usize) {
    if w.units[b].turned_off {
        return;
    }
    if w.units[b].research.is_some() {
        // The research order is in front: production and mining wait.
        crate::upgrade::research_link(w, b);
        return;
    }
    if w.units[b].unconstruct {
        unconstruct_link(w, b);
        return;
    }
    if !w.units[b].queue.is_empty() {
        production_link(w, b);
    }
    mine_link(w, b);
}

/// The `UNCONSTRUCT` order (link 0x4a0d47), once per tick: first the units inside leave, one
/// per tick; then the building loses a stage per tick (its first tick reaches the last
/// stage again and so re-runs the completion, as in the engine); at stage 0 it is gone and
/// the owner gets its price back.
fn unconstruct_link(w: &mut World, b: usize) {
    if !w.units[b].inside.is_empty() {
        let first = w.units[b].inside.remove(0);
        release(w, b, vec![first]);
        return;
    }
    // The sections go with the building (OB+0xaa = 0, 0x4a0dc7).
    w.units[b].stages = 0;
    if w.units[b].stage <= 0 {
        let (owner, kind) = (w.units[b].owner, w.units[b].kind);
        let cost = unit_cost(w, owner, kind, None);
        let id = w.units[b].id;
        w.remove(id);
        let pl = &mut w.players[usize::from(owner)];
        for (s, c) in pl.resources.iter_mut().zip(&cost).take(6) {
            *s = s.saturating_add(*c);
        }
        census(w, owner);
        return;
    }
    let u = &mut w.units[b];
    u.life = 100;
    u.stage -= 1;
    crate::construction::next_stage(w, b);
    w.units[b].stage -= 1;
}

/// `ProduceObjLink` (0x49eeb7).
fn production_link(w: &mut World, b: usize) {
    if w.units[b].switched_off {
        return; // STOPBUILD (0x49eec5)
    }
    let catalog = w.catalog.clone();
    let (owner, bt) = (w.units[b].owner, catalog.get(w.units[b].kind));
    let head = w.units[b].queue[0].clone();
    let pt = catalog.get(head.kind);
    let p = &w.players[usize::from(owner)];
    let peasant = pt.peasant;
    if p.housing < p.population + 1 && (peasant || !bt.economy.peasant_convertor) {
        return; // no room: production waits
    }
    let converts = bt.economy.peasant_convertor && !peasant;
    if converts && !w.units[b].inside.iter().any(|&id| catalog.get(w.units[id.0 as usize].kind).peasant) {
        return; // NOT_ENOUGH_PEASANTS
    }
    // The depot limit (0x49f21b): over it, the order is cancelled with a refund.
    if let Some((g, n)) = pt.economy.depot_group {
        let [depots, count] = [None, Some(g)].map(|group| {
            w.units_of(owner)
                .filter(|u| {
                    let e = &catalog.get(u.kind).economy;
                    match group {
                        None => e.art_depot && (!u.building || u.stage >= w.max_stage(u)),
                        Some(g) => e.depot_group.is_some_and(|(ug, _)| ug == g),
                    }
                })
                .count() as i32
        });
        if count >= depots * n {
            unproduce(w, b, head.kind);
            return;
        }
    }
    let stages = w.adv(owner, head.kind).build_stages;
    let q = &mut w.units[b].queue[0];
    if q.stages != stages && q.stages > 0 {
        q.progress = stages * q.progress / q.stages;
        q.stages = stages;
    }
    let enabled = w.players[usize::from(owner)].enabled.get(usize::from(head.kind.0)).copied().unwrap_or(true);
    if !enabled {
        // The type was disabled meanwhile: the order is dropped without refund.
        w.units[b].queue.remove(0);
        return;
    }
    let ship = bt.port && pt.medium == crate::catalog::Medium::Water;
    if ship && w.units[b].queue[0].progress == 0 {
        // A ship starts only with a free 15 × 15 birth box at the port's water point.
        let Some((px, py)) = w.units[b].port_point else { return };
        if w.map.water.check_bar(px - 7, py - 7, 15, 15) || w.occupancy.check_bar(px - 7, py - 7, 15, 15) {
            crate::ship::clear_birth_place(w, px, py);
            return;
        }
    }
    let q = &mut w.units[b].queue[0];
    q.progress += 1 << SPEED_SH;
    if q.progress < q.stages {
        return;
    }
    w.units[b].queue.remove(0);
    if ship && let Some((px, py)) = w.units[b].port_point {
        // At the water point, heading 96, then to the rally point ± 1024 px (y drawn first).
        let id = w.spawn(head.kind, owner, Pos::new(px << 8, py << 8), 96);
        if let Some(r) = w.units[b].rally.filter(|r| r.x > 0) {
            let ry = w.random.rando();
            let rx = w.random.rando();
            let areas = w.map.water_areas.clone();
            let (tx, ty) = ((r.x + ((rx - 0x4000) >> 2)) >> 4, (r.y + ((ry - 0x4000) >> 2)) >> 4);
            crate::motion::smart_send_to(&mut w.units[id.0 as usize], areas.as_ref(), tx, ty, 0, 0, 0x80);
        }
        census(w, owner);
        return;
    }
    spawn_from(w, b, head.kind);
    if converts
        && let Some(k) = w.units[b].inside.iter().position(|&id| catalog.get(w.units[id.0 as usize].kind).peasant)
    {
        let pid = w.units[b].inside.remove(k);
        w.units[pid.0 as usize].inside_of = None;
        w.remove(pid);
    }
    census(w, owner);
}

/// The spawn of `ProduceObjLink` (0x49eeb7, from 0x49f4b3): the unit appears at the
/// building's first born point facing the second, walks through locks and units to every
/// further born point (each one cell in, `+1`), takes its cell, steps to the last born point
/// and then follows the rally point (`DstX/DstY`):
///
/// - onto a building that can take it: the producer itself takes it in at once, another
///   building gets a `GoToMine`;
/// - onto a resource source: a peasant gathers it (`TakeResource`, appended);
/// - elsewhere: a free spot near the point (`FindUnitPosition` 0x51508c) and `SmartSendTo`.
fn spawn_from(w: &mut World, b: usize, kind: TypeId) -> EntityId {
    let catalog = w.catalog.clone();
    let (bu, bt) = (&w.units[b], catalog.get(w.units[b].kind));
    let (bx, by) = base_cell(bt, bu.pos);
    let pts = bt.economy.born_points.clone();
    let corner = |(x, y): (i32, i32), add: i32| Pos::new((bx + x + add) << 8, (by + y + add) << 8);
    let first = pts.first().map_or(bu.pos, |&p| corner(p, 0));
    let dir = match pts.get(1) {
        Some(&(x1, y1)) => get_dir(x1 - pts[0].0, y1 - pts[0].1),
        None => 64,
    };
    let (owner, rally) = (bu.owner, bu.rally);
    let id = w.spawn(kind, owner, first, dir);
    // The new unit's `BORN_SOUND` (0x49fab7).
    w.born.push(id);
    let i = id.0 as usize;
    let u = &mut w.units[i];
    w.occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
    // The engine's queue in execution order; the order stack is its reverse.
    let mut queue: Vec<Order> = pts
        .iter()
        .skip(1)
        .map(|&p| Order::SendTo { cell: Unit::cell_of(corner(p, 1), u.lx), prio: SPAWN_PRIO })
        .collect();
    queue.push(Order::ClearUnlimited);
    let out = pts.last().map_or(first, |&p| corner(p, 0));
    queue.push(Order::SendTo { cell: Unit::cell_of(out, u.lx), prio: SPAWN_PRIO });
    queue.reverse();
    u.orders = queue;
    u.prio = SPAWN_PRIO;
    if let Some(r) = rally.filter(|r| r.x > 0) {
        follow_rally(w, b, i, r);
    }
    let u = &mut w.units[i];
    if u.inside_of.is_none() {
        // SetOrderedUnlimitedMotion: land units walk through locks until `ClearUnlimited`.
        u.unlimited = true;
    }
    id
}

/// The rally part of the spawn (0x49fcc6..0x4a0160) for unit `i` of building `b`.
fn follow_rally(w: &mut World, b: usize, i: usize, rally: Pos) {
    let (dx, dy) = rally.pixels();
    if let Some(ins) = building_at(w, dx, dy) {
        if ins == b && can_enter(w, &w.units[i], &w.units[b]) {
            // Straight in: the unit never walks out.
            let (bid, u) = (w.units[b].id, &mut w.units[i]);
            u.orders.clear();
            u.inside_of = Some(bid);
            u.dest = None;
            let id = u.id;
            w.units[b].inside.push(id);
            return;
        }
        enter(w, i, ins, 0x90, true);
        return;
    }
    let peasant = w.catalog.get(w.units[i].kind).peasant;
    if let Some(rk) = crate::gather::determine(w, dx, dy).filter(|_| peasant) {
        // The engine draws random points within 128 px until one lies at a source of the
        // same type (its retry counter is never decremented); capped here at one pass
        // through the random table.
        let (mut x, mut y) = (dx, dy);
        for _ in 0..crate::rng::SYNC_TABLE_LEN / 2 {
            let rx = dx + (w.random.rando() & 255) - 128;
            let ry = dy + (w.random.rando() & 255) - 128;
            if rx > 0 && ry > 0 && crate::gather::determine(w, rx, ry) == Some(rk) {
                (x, y) = (rx, ry);
                break;
            }
        }
        crate::gather::take_resource(w, i, x, y, rk, 0x10, true);
        return;
    }
    let (x, y) = find_unit_position(w, w.units[i].kind, rally);
    let (x, y) = (x >> 4, y >> 4);
    w.units[i].orders.insert(0, Order::SmartSendTo { x, y, dx: 0, dy: 0, prio: 0x80, area: None });
}

/// The building the rally point lies on (0x49ed8e over rings 0..=3 of 128 px cells, where
/// the last match wins; simplified: a building that can take units in and has a lock cell
/// under the point).
fn building_at(w: &World, x: i32, y: i32) -> Option<usize> {
    let cell = (x >> 4, y >> 4);
    w.units.iter().enumerate().rev().find_map(|(k, u)| {
        let t = w.catalog.get(u.kind);
        let absorbs = t.economy.unit_absorber > 0 || t.economy.peasant_absorber > 0;
        (u.living() && u.building && absorbs && t.lock_cells(u.pos).any(|c| c == cell)).then_some(k)
    })
}

/// `FindUnitPosition` (0x51508c): the first free spot for a unit of `kind` on the rings
/// around `at` (1/16 px), 32 px apart; each ring is walked forwards or backwards by a
/// random draw. A spot is free (0x513b2c) when it lies on the map, the unit's footprint is
/// unlocked (0x50a60d) and no unit stands within the two radii plus 8 (0x50a495).
pub fn find_unit_position(w: &mut World, kind: TypeId, at: Pos) -> (i32, i32) {
    let catalog = w.catalog.clone();
    let t = catalog.get(kind);
    let r2 = t.radius2 << 4;
    let lx = ((r2 + 3) >> 7).max(1);
    let msx = 240 << w.map.shift;
    let free = |w: &World, x: i32, y: i32| {
        if x >> 9 <= 0 || y >> 9 <= 0 || x >> 9 >= msx || y >> 9 >= msx {
            return false;
        }
        let (cx, cy) = ((x - lx * 128) >> 8, (y - lx * 128) >> 8);
        if w.map.land.check_bar(cx, cy, lx, lx) {
            return false;
        }
        let (gx, gy) = (x >> 11, y >> 11);
        !w.units.iter().any(|u| {
            u.living()
                && u.inside_of.is_none()
                && ((u.pos.x >> 11) - gx).abs() <= 1
                && ((u.pos.y >> 11) - gy).abs() <= 1
                && norma(u.pos.x - x, u.pos.y - y) < r2 + 8 + (catalog.get(u.kind).radius2 << 4)
        })
    };
    for ring in &crate::trig::rings()[..50] {
        let forward = w.random.rando() < 0x4000;
        let test = |&(dx, dy): &(i8, i8)| {
            let (x, y) = (at.x + (i32::from(dx) << 9), at.y + (i32::from(dy) << 9));
            free(w, x, y).then_some((x, y))
        };
        let found = if forward { ring.iter().find_map(test) } else { ring.iter().rev().find_map(test) };
        if let Some(p) = found {
            return p;
        }
    }
    (at.x, at.y)
}

/// `TakeResourceFromSpriteLink` (0x5f75b3), every tick, no randomness: the mine adds
/// `(free + inside·per_peasant) << (SpeedSh + 2)` to its accumulator and pays out per 1000.
fn mine_link(w: &mut World, b: usize) {
    let Some(mut mine) = w.units[b].mine else { return };
    let Some(prod) = w.catalog.get(w.units[b].kind).economy.producer else { return };
    let work = (prod.free + w.units[b].inside.len() as i32 * prod.per_peasant) << (SPEED_SH + 2);
    let mut acc = i32::from(mine.acc) + work;
    if acc > 1000 {
        let p = &mut w.players[usize::from(w.units[b].owner)];
        p.resources[mine.resource] = p.resources[mine.resource].saturating_add(acc / 1000);
        acc %= 1000;
    }
    mine.acc = acc as u16;
    w.units[b].mine = Some(mine);
}

/// Binds mines without a deposit to one within 128 px (`CheckMinePosition` 0x4d2100); the
/// engine does this at creation and every 256 ticks (`HandleMines` 0x5f7f33).
pub fn bind_mines(w: &mut World) {
    let catalog = w.catalog.clone();
    for i in 0..w.units.len() {
        let u = &w.units[i];
        if !u.living() || u.mine.is_some() {
            continue;
        }
        let Some(prod) = catalog.get(u.kind).economy.producer else { continue };
        let (x, y) = u.pos.pixels();
        let found = w.deposits.iter().find(|d| prod.mask & (1 << d.resource) != 0 && norma(d.x - x, d.y - y) < 128);
        if let Some(d) = found {
            w.units[i].mine = Some(Mine { resource: d.resource, acc: 0 });
        }
    }
}

/// Whether unit `u` may go into building `b` (`GoToMine`'s tests, 0x5f43d0): `b` is not
/// dying and finished or busy; a `HIGHUNIT` does not fit a `HIGHUNITCANTENTER` building;
/// the unit is a peasant or `CANSTORM`, and mobile; the building takes units
/// (`UNITABSORBER`), or peasants (`PEASANTABSORBER`) and the unit is one; it has
/// `CONCENTRATOR` points; a friend's building must have room (an enemy's need not: the unit
/// storms it, see [`enter_link`]).
pub fn can_enter(w: &World, u: &Unit, b: &Unit) -> bool {
    let bt = w.catalog.get(b.kind);
    let ut = w.catalog.get(u.kind);
    let ready = !b.building || b.stage >= w.max_stage(b);
    let kind_ok = bt.economy.unit_absorber > 0 || (bt.economy.peasant_absorber > 0 && ut.peasant);
    let cap = w.adv(b.owner, b.kind).max_inside + b.add_inside;
    let full = u.nmask & b.nmask != 0 && b.inside.len() as i32 >= cap;
    b.living()
        && (ready || !b.orders.is_empty())
        && !(ut.role.high_unit && bt.role.high_unit_cant_enter)
        && (ut.peasant || ut.role.usage == crate::catalog::usage::PEASANT || ut.role.can_storm)
        && ut.mobile()
        && kind_ok
        && !full
        && !bt.economy.concentrator.is_empty()
}

/// Orders a unit into a building (mine, garrison, an enemy's building to storm): `GoToMine`
/// (0x5f43d0). Refused by [`can_enter`] or below the unit's current priority; a unit already
/// going into this building keeps its order. `append` queues the order after the current
/// ones (order type 2), otherwise it replaces them. The order keeps `prio & 0x7f`.
pub fn enter(w: &mut World, i: usize, b: usize, prio: u8, append: bool) -> bool {
    if !can_enter(w, &w.units[i], &w.units[b]) || w.units[i].prio > prio {
        return false;
    }
    let id = w.units[b].id;
    let u = &mut w.units[i];
    if matches!(u.orders.last(), Some(Order::Enter { building, .. }) if *building == id) {
        return true;
    }
    let prio = prio & 0x7f;
    if append {
        u.orders.insert(0, Order::Enter { building: id, prio });
        return true;
    }
    u.orders.clear();
    u.orders.push(Order::Enter { building: id, prio });
    u.prio = prio;
    u.path.delay = 0;
    true
}

/// `GoToMineLink` (0x5f46f7): walk to the building's first `CONCENTRATOR` point (lock-cell
/// distance below 2), then:
/// - a friend's building takes the unit inside (no room: the order ends);
/// - an empty enemy building is captured ([`crate::capture::change_side`], its orders
///   cleared) and the unit goes inside;
/// - a guarded one (`SetAI_Guard`) kills the unit;
/// - a garrisoned one is stormed: the unit plays `#ATTACK` to its end, then one random
///   defender `D` (`rando()·n >> 15` of the `n` inside) faces
///   `STORMFORCE(D) · (a + (b − a)·n / MaxInside) · rando() >> 15` (`OPPOSIT a b` of the
///   building) and dies when that is below the stormer's `STORMFORCE`; the stormer always
///   dies (a type without `STORMFORCE` at once). When the garrison is gone, the next
///   stormer takes the building.
///
/// A gone or dying building ends the order.
pub fn enter_link(w: &mut World, i: usize, building: EntityId, prio: u8) {
    let b = building.0 as usize;
    w.units[i].prio = prio;
    if !w.units.get(b).is_some_and(|u| u.alive) || !w.units[b].living() {
        w.units[i].pop_order();
        return;
    }
    let allied = w.units[i].nmask & w.units[b].nmask != 0;
    let cap = w.adv(w.units[b].owner, w.units[b].kind).max_inside + w.units[b].add_inside;
    if allied && w.units[b].inside.len() as i32 >= cap {
        w.units[i].pop_order();
        return;
    }
    let catalog = w.catalog.clone();
    let bt = catalog.get(w.units[b].kind);
    let (bx, by) = base_cell(bt, w.units[b].pos);
    let Some(&(cx, cy)) = bt.economy.concentrator.first() else {
        w.units[i].pop_order();
        return;
    };
    let target = (bx + cx, by + cy);
    let u = &w.units[i];
    let t = catalog.get(u.kind);
    let mover = Mover::of(u, t);
    if dist_to(mover, target.0, target.1) >= 2 {
        let u = &mut w.units[i];
        if let Some((x, y)) =
            create_path(&w.map.land, mover, &mut u.path, &mut w.random, target.0, target.1, t.stop_distance)
        {
            u.dest = Some(Pos::new(x, y));
        }
        return;
    }
    let u = &mut w.units[i];
    u.prio = 0x7f;
    u.dest = None;
    if !allied {
        if w.units[b].guard {
            if w.units[i].owner != w.units[b].owner {
                crate::combat::die(w, i);
            }
            return;
        }
        if !w.units[b].inside.is_empty() {
            storm(w, i, b);
            return;
        }
        // An empty enemy building is taken (0x5f4b6c..0x5f4e59).
        let new = w.units[i].owner;
        crate::capture::change_side(w, b, new);
        let bu = &mut w.units[b];
        bu.orders.clear();
        bu.path.clear();
        bu.prio = 0;
    }
    let u = &mut w.units[i];
    w.occupancy.clr_bar(u.cell.0, u.cell.1, u.lx);
    u.orders.clear();
    u.in_motion = false;
    u.dest = None;
    u.inside_of = Some(building);
    let id = u.id;
    w.units[b].inside.push(id);
}

/// The storm of a garrisoned enemy building (0x5f4e5e..0x5f502e), once the stormer stands at
/// its door: see [`enter_link`].
fn storm(w: &mut World, i: usize, b: usize) {
    let catalog = w.catalog.clone();
    let t = catalog.get(w.units[i].kind);
    let u = &mut w.units[i];
    if u.anim != AnimKind::Attack(0) {
        // The frame is not reset (0x5f5033).
        crate::motion::set_anim(u, t, AnimKind::Attack(0));
        return;
    }
    if u.frame < u.anim_frames - FRM_DEC {
        return;
    }
    let sf = t.role.storm_force;
    let n = w.units[b].inside.len() as i32;
    if sf != 0 && n != 0 {
        let k = ((w.random.rando() * n) >> 15) as usize;
        if let Some(d) = w.units[b].inside.get(k).map(|e| e.0 as usize) {
            let bt = catalog.get(w.units[b].kind);
            let (o1, o2) = bt.role.opposit;
            let full = w.adv(w.units[b].owner, w.units[b].kind).max_inside;
            let sfd = catalog.get(w.units[d].kind).role.storm_force;
            let scale = if full == 0 { o1 } else { o1 + (o2 - o1) * n / full };
            let v = (sfd * scale * w.random.rando()) >> 15;
            if v < sf {
                // The defender dies and is gone at once.
                let id = w.units[d].id;
                w.units[b].inside.retain(|&e| e != id);
                crate::combat::die(w, d);
                w.units[d].inside_of = None;
                w.units[d].alive = false;
            }
        }
    }
    crate::combat::die(w, i);
}

/// Lets everybody out of a building at its born points.
pub fn leave(w: &mut World, b: usize) {
    let inside = std::mem::take(&mut w.units[b].inside);
    release(w, b, inside);
}

/// Puts units of building `b` (already off its list) back outside at its born points.
fn release(w: &mut World, b: usize, inside: Vec<crate::world::EntityId>) {
    let catalog = w.catalog.clone();
    let bt = catalog.get(w.units[b].kind);
    let (bx, by) = base_cell(bt, w.units[b].pos);
    let pts = bt.economy.born_points.clone();
    let pos = w.units[b].pos;
    for id in inside {
        let u = &mut w.units[id.0 as usize];
        u.inside_of = None;
        let first = pts.first().map_or(pos, |&(x, y)| Pos::new((bx + x) << 8, (by + y) << 8));
        let out = pts.last().map_or(first, |&(x, y)| Pos::new((bx + x) << 8, (by + y) << 8));
        u.pos = first;
        u.cell = Unit::cell_of(first, u.lx);
        u.unlimited = true;
        u.orders.clear();
        u.orders.push(Order::ClearUnlimited);
        u.orders.push(Order::SendTo { cell: Unit::cell_of(out, u.lx), prio: SPAWN_PRIO });
        u.dest = Some(out);
    }
}

/// Deposits of a map: `[INTERNAL_SOURCE]` sprites of the map objects.
pub fn deposits(objects: &[crate::world::MapObject], shapes: &crate::map::SpriteShapes) -> Vec<Deposit> {
    objects
        .iter()
        .filter_map(|o| Some(Deposit { x: o.x, y: o.y, resource: shapes.source(o.group, o.index)? }))
        .collect()
}

/// Ends the unlimited walk out of a building: the unit takes its cell in the occupancy grid.
pub(crate) fn settle(u: &mut Unit, occupancy: &mut crate::map::LockMap) {
    if u.unlimited {
        u.unlimited = false;
        occupancy.set_bar(u.cell.0, u.cell.1, u.lx);
    }
}

/// All players' census, as after loading a map.
pub fn census_all(w: &mut World) {
    for p in 0..PLAYERS as u8 {
        census(w, p);
    }
}
