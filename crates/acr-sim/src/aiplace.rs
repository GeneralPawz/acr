//! Where computer players place their buildings (`City::ProcessCreation` 0x432582,
//! `FindApproximateBuildingPlace` 0x4306db, `SmartCreationUnit` 0x514712). See
//! `docs/simulation.md`.
//!
//! The city centre is the mean position of the nation's peasants on its first city tick. An
//! accepted building project first gets an approximate 128 px cell: the first cell of the
//! rings around the centre that passes the test for its kind (`CheckBuilding`,
//! `CheckMelnica`, `CheckWoodSklad`, `CheckTower`, `CheckPort`, the deposit search for
//! mines), skipping cells near enemies and cells too far along the path areas. Once per city
//! tick the city tries the exact spot, the cell corner ± 128 px, with the full site test on 9
//! rings of 32 px around it; a failure marks the cell unusable and searches again, and after
//! 6 failed tries the project is dropped.

use crate::TypeId;
use crate::catalog::{ObjectType, usage};
use crate::fixed::Pos;
use crate::trig::norma;
use crate::world::{EntityId, World};

/// `InfoMap` bits.
const I_TREES: u8 = 1;
const I_STONES: u8 = 2;
const I_FREE: u8 = 4;
const I_NOLOCK: u8 = 8;
const I_FLAT: u8 = 0x10;
/// `CantBuild` bits.
const C_SKLAD: u8 = 1;
const C_BUILDING: u8 = 8;
const C_MILL: u8 = 0x10;
const C_PORT: u8 = 0x20;

/// The AI planning maps (`CantBuild` 0x6bc3e0, `InfoMap` 0x6bc46c), 128 px cells, shared by
/// all nations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PlanMaps {
    /// Cells per row (`SsMax + 1`).
    pub w: i32,
    /// `SsMaxX = SsMaxY` (the map side in 128 px cells).
    pub max: i32,
    pub cant: Vec<u8>,
    pub info: Vec<u8>,
}

/// The enemy-danger map of a computer nation (`EINF+0xb0e`, 1024 px cells: 1 enemy, 2 next
/// to one) and the tick it was built.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Danger {
    pub w: i32,
    pub map: Vec<u8>,
    pub last: u64,
}

/// A pending building project (`BPR`, City+0x14c7c).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Project {
    pub kind: TypeId,
    pub usage: u8,
    /// The approximate cell (128 px cells).
    pub near: (i32, i32),
    /// The placed site (`PlaceFound`, `BPR.Index`).
    pub placed: Option<EntityId>,
    /// `AttemptsToFindApprPlace`.
    pub attempts: u16,
    /// `Options`: 0x200 wood, 0x400 stone storehouse.
    pub options: i32,
}

/// The building part of a computer player's city.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct City {
    /// `CenterX/CenterY` (128 px cells), found once.
    pub center: Option<(i32, i32)>,
    pub project: Option<Project>,
    /// `MelnID` (at most 4), wood and stone storehouses (at most 8 each).
    pub mills: Vec<EntityId>,
    pub wood: Vec<EntityId>,
    pub stone: Vec<EntityId>,
    pub danger: Danger,
}

/// `CreateInfoMap` (0x43f300) at the first use.
fn plan_maps(w: &mut World) -> &mut PlanMaps {
    if w.plan.is_none() {
        let max = w.map.land.width >> 3;
        let wd = max + 1;
        let mut p = PlanMaps { w: wd, max, cant: vec![0; (wd * wd) as usize], info: vec![0; (wd * wd) as usize] };
        let (sprites, trees, stones) = sprite_counts(w, wd);
        for y in 0..=max {
            for x in 0..=max {
                p.info[(x + y * wd) as usize] = info_cell(w, x, y, &sprites, &trees, &stones, wd);
            }
        }
        w.plan = Some(std::sync::Arc::new(p));
    }
    std::sync::Arc::make_mut(w.plan.as_mut().expect("set"))
}

/// Sprites, trees (more than 2 make a wood cell) and stones per 128 px cell.
fn sprite_counts(w: &World, wd: i32) -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let n = (wd * wd) as usize;
    let (mut s, mut t, mut st) = (vec![0u16; n], vec![0u16; n], vec![0u16; n]);
    let idx = |x: i32, y: i32| {
        let (cx, cy) = (x >> 7, y >> 7);
        (cx >= 0 && cy >= 0 && cx < wd && cy < wd).then(|| (cx + cy * wd) as usize)
    };
    for &(x, y, _) in w.spacing.iter() {
        if let Some(i) = idx(x, y) {
            s[i] = s[i].saturating_add(1);
        }
    }
    for f in &w.sprites {
        if let Some(i) = idx(f.x, f.y) {
            s[i] = s[i].saturating_add(1);
        }
    }
    for src in &w.sources.list {
        if let Some(i) = idx(src.x, src.y) {
            match usize::from(src.kind) {
                crate::economy::WOOD => t[i] = t[i].saturating_add(1),
                crate::economy::STONE => st[i] = st[i].saturating_add(1),
                _ => {}
            }
        }
    }
    (s, t, st)
}

fn info_cell(w: &World, x: i32, y: i32, sprites: &[u16], trees: &[u16], stones: &[u16], wd: i32) -> u8 {
    let mut ms = 0;
    if !w.map.land.check_bar(x * 8, y * 8, 8, 8) {
        ms |= I_NOLOCK;
    }
    let h = &w.map.heights;
    let h1 = h.at(x * 128, y * 128);
    let h2 = h.at(x * 128 + 128, y * 128);
    let h3 = h.at(x * 128, y * 128 + 128);
    let h4 = h3; // the engine samples the third corner twice
    let hh = (h1 + h2 + h3 + h4) >> 2;
    if [h1, h2, h3, h4].iter().all(|&v| (v - hh).abs() < 20) {
        ms |= I_FLAT;
    }
    let o = (x + y * wd) as usize;
    if ms & I_NOLOCK != 0 && ms & I_FLAT != 0 && sprites[o] == 0 {
        ms |= I_FREE;
    }
    if trees[o] > 2 {
        ms |= I_TREES;
    }
    if stones[o] > 5 {
        ms |= I_STONES;
    }
    if trees[o] != 0 || stones[o] != 0 {
        ms &= !I_FREE;
    }
    ms
}

/// `RenewInfoMap` (0x43f763), after a building's locks were set: the 3 × 3 cells around it
/// get their lock and free bits again (the flat bit stays) and any tree or stone marks them.
pub fn renew(w: &mut World, px: i32, py: i32) {
    let Some(p) = w.plan.clone() else { return };
    let wd = p.w;
    let (sprites, trees, stones) = sprite_counts(w, wd);
    let mut p = (*p).clone();
    for y in (py >> 7) - 1..=(py >> 7) + 1 {
        for x in (px >> 7) - 1..=(px >> 7) + 1 {
            if x < 0 || y < 0 || x >= wd || y >= wd {
                continue;
            }
            let o = (x + y * wd) as usize;
            let mut ms = p.info[o] & I_FLAT;
            if !w.map.land.check_bar(x * 8, y * 8, 8, 8) {
                ms |= I_NOLOCK;
            }
            if ms & I_NOLOCK != 0 && ms & I_FLAT != 0 && sprites[o] == 0 {
                ms |= I_FREE;
            }
            if trees[o] != 0 {
                ms |= I_TREES;
            }
            if stones[o] != 0 {
                ms |= I_STONES;
            }
            p.info[o] = ms;
        }
    }
    w.plan = Some(std::sync::Arc::new(p));
}

/// The danger-map builder (0x429d9c) for nation `k`.
fn build_danger(w: &World, k: u8) -> Danger {
    let wd = ((w.map.land.width << 4) >> 10).max(1);
    let mut map = vec![0u8; (wd * wd) as usize];
    let mask = w.nation_mask[usize::from(k) & 7];
    for u in &w.units {
        if !u.living() || u.nmask & mask != 0 || u.owner == 7 || u.unlimited {
            continue;
        }
        let (cx, cy) = (u.pos.x >> 14, u.pos.y >> 14);
        if cx >= 0 && cy >= 0 && cx < wd - 1 && cy < wd - 1 {
            map[(cx + cy * wd) as usize] = 1;
        }
    }
    for x in 1..wd - 2 {
        for y in 1..wd - 2 {
            if map[(x + y * wd) as usize] == 1 {
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    let o = ((x + dx) + (y + dy) * wd) as usize;
                    if map[o] == 0 {
                        map[o] = 2;
                    }
                }
            }
        }
    }
    Danger { w: wd, map, last: w.tick.0 + 1 }
}

/// `GNFO.Process` (0x4275e1): nation `k`'s danger map is rebuilt when `tmtmt & 63 == 8k + 6`
/// and it is older than 256 ticks; built at the first use.
fn refresh_danger(w: &mut World, k: u8) {
    let tmtmt = w.tick.0 + 1;
    let c = &w.cities[usize::from(k)];
    if c.danger.map.is_empty() || (tmtmt & 63 == 8 * u64::from(k) + 6 && tmtmt - c.danger.last > 256) {
        let d = build_danger(w, k);
        w.cities[usize::from(k)].danger = d;
    }
}

/// `GetSafeVal` (0x42a152) for a 128 px cell.
fn safe_val(d: &Danger, x: i32, y: i32) -> u8 {
    let (x, y) = (x >> 3, y >> 3);
    if x < 0 || y < 0 || x >= d.w || y >= d.w {
        return 0;
    }
    d.map[(x + y * d.w) as usize]
}

/// `GetTopology` (0x416554) of a pixel point: its 64 px cell's area, else the first area of
/// the rings 0..19 around it.
fn topology(w: &World, px: i32, py: i32) -> u16 {
    let Some(g) = w.map.land_areas.as_ref() else { return 0xFFFF };
    let (cx, cy) = (px >> 6, py >> 6);
    let a = g.area_at(cx, cy);
    if a < 0xFFFE {
        return a;
    }
    (0..20)
        .flat_map(crate::morale::ring)
        .map(|&(dx, dy)| g.area_at(cx + dx, cy + dy))
        .find(|&a| a < 0xFFFE)
        .unwrap_or(0xFFFF)
}

/// `TopDist` (0x43c7d8) between two 64 px cells.
fn top_dist(w: &World, x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
    let Some(g) = w.map.land_areas.as_ref() else { return norma(x1 - x2, y1 - y2) };
    const OFF: [(i32, i32); 8] = [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    let find = |x: i32, y: i32| {
        let a = g.area_at(x, y);
        if a < 0xFFFE {
            return a;
        }
        OFF.iter().map(|(dx, dy)| g.area_at(x + dx, y + dy)).find(|&a| a < 0xFFFE).unwrap_or(0xFFFF)
    };
    let (a1, a2) = (find(x1, y1), find(x2, y2));
    if a1 >= 0xFFFE || a2 >= 0xFFFE {
        return 0xFFFF;
    }
    let direct = norma(x1 - x2, y1 - y2);
    if a1 == a2 {
        return direct;
    }
    let n1 = g.next_hop(a1, a2);
    if n1 == 0xFFFF {
        return 0xFFFF;
    }
    if n1 == a2 {
        return direct;
    }
    let n2 = g.next_hop(a2, a1);
    if n2 == 0xFFFF {
        return 0xFFFF;
    }
    if n2 == a1 {
        return direct;
    }
    let c = |a: u16| g.centers.get(usize::from(a)).copied().unwrap_or((0, 0));
    let (c1, c2) = (c(n1), c(n2));
    let mut r = norma(x1 - c1.0, y1 - c1.1) + norma(x2 - c2.0, y2 - c2.1);
    if n1 != n2 {
        r += i32::from(g.dist.get(usize::from(n1) * g.count + usize::from(n2)).copied().unwrap_or(0xFFFF));
    }
    r
}

/// What a cell test needs besides the maps.
struct Ctx<'a> {
    p: &'a PlanMaps,
    /// Storehouse or tower/port positions of the type (128 px cells).
    list: &'a [(i32, i32)],
}

impl Ctx<'_> {
    fn inner(&self, x: i32, y: i32, b: i32) -> bool {
        x > b && y > b && x < self.p.max - b && y < self.p.max - b
    }
    fn i(&self, o: i32) -> u8 {
        usize::try_from(o).ok().and_then(|o| self.p.info.get(o)).copied().unwrap_or(0)
    }
    fn c(&self, o: i32) -> u8 {
        usize::try_from(o).ok().and_then(|o| self.p.cant.get(o)).copied().unwrap_or(0)
    }
}

#[derive(Clone, Copy)]
enum Test {
    Building,
    Melnica,
    WoodSklad,
    Tower,
    Port,
}

fn cell_test(ctx: &Ctx, w: &World, t: Test, x: i32, y: i32) -> bool {
    let wd = ctx.p.w;
    let o = x + y * wd;
    match t {
        // CheckBuilding (0x43ffce).
        Test::Building => ctx.inner(x, y, 1) && ctx.c(o) & C_BUILDING == 0,
        // CheckMelnica (0x43f989).
        Test::Melnica => {
            ctx.inner(x, y, 1)
                && ctx.c(o) & C_MILL == 0
                && (-1..=1).all(|dy| (-1..=1).all(|dx| ctx.i(o + dx + dy * wd) & I_FREE != 0))
        }
        // CheckWoodSklad (0x43fc13), with the engine's neighbour list.
        Test::WoodSklad => {
            ctx.inner(x, y, 1)
                && !ctx.list.iter().any(|&(sx, sy)| norma(sx - x, sy - y) < 8)
                && ctx.c(o) & C_SKLAD == 0
                && [o - wd, o + wd, o + 1, o - 1, o - wd - 1, o - wd + 1, o + wd - 1, o - wd + 1]
                    .iter()
                    .any(|&n| ctx.i(n) & I_TREES != 0)
        }
        // CheckTower (0x44002d).
        Test::Tower => {
            !ctx.list.iter().any(|&(sx, sy)| norma(sx - x, sy - y) < 9)
                && ctx.inner(x, y, 1)
                && ctx.c(o) & C_BUILDING == 0
                && [o, o + 1, o + wd, o + wd + 1].iter().all(|&n| ctx.i(n) & I_FREE != 0)
        }
        // CheckPort (0x440154): deep water near the cell.
        Test::Port => {
            if !ctx.inner(x, y, 5)
                || ctx.list.iter().any(|&(sx, sy)| norma(sx - x, sy - y) < 9)
                || ctx.c(o) & C_PORT != 0
            {
                return false;
            }
            let Some(d) = w.map.water_depth.as_ref() else { return false };
            let (wx, wy) = (4 * x + 2, 4 * y + 2);
            let deep = |dx: i32, dy: i32| {
                let (sx, sy) = (wx + dx, wy + dy);
                sx >= 0 && sy >= 0 && d.get(sx as usize, sy as usize).is_some_and(|v| v >= 128)
            };
            [(0, -10), (0, 10), (-20, 0), (20, 0), (-20, -10), (20, -10), (-20, 10), (20, 10)]
                .iter()
                .any(|&(dx, dy)| deep(dx, dy))
        }
    }
}

/// `SearchPlace` (0x44033a) / `SearchTowerPlace` (0x44055a, `tower`: rings from 6, the half
/// plane towards nation 0, threshold 14, no same-area rule).
#[allow(clippy::too_many_arguments)]
fn search(
    w: &World,
    ctx: &Ctx,
    danger: &Danger,
    t: Test,
    x: i32,
    y: i32,
    r: usize,
    tower: Option<((i32, i32), (i32, i32))>,
) -> Option<(i32, i32)> {
    if cell_test(ctx, w, t, x, y) {
        return Some((x, y));
    }
    let g = w.map.land_areas.as_ref()?;
    let my = topology(w, x << 7, y << 7);
    if my >= 0xFFFE {
        return None;
    }
    let mut prev = 0i32;
    let (start, limit) = if tower.is_some() { (6, 14) } else { (1, 60) };
    for i in start..r {
        for &(dx, dy) in crate::morale::ring(i) {
            let (xx, yy) = (x + dx, y + dy);
            if !(xx > 0 && yy > 0 && xx < ctx.p.max && yy < ctx.p.max) {
                continue;
            }
            if let Some(((xc, yc), (xe, ye))) = tower
                && (xx - xc) * (xe - xc) + (yy - yc) * (ye - yc) <= 0
            {
                continue;
            }
            if safe_val(danger, xx, yy) != 0 {
                continue;
            }
            let top = g.area_at(2 * xx, 2 * yy);
            if top >= 0xFFFE {
                continue;
            }
            let mut tt = i32::from(g.dist.get(usize::from(my) * g.count + usize::from(top)).copied().unwrap_or(0xFFFF));
            if top == my && tower.is_none() {
                tt = 0;
            }
            if tt - prev < limit {
                if tt > prev {
                    prev = tt;
                }
                if cell_test(ctx, w, t, xx, yy) {
                    return Some((xx, yy));
                }
            }
        }
    }
    None
}

/// Positions (128 px cells) of the living objects of a type, at most 32.
fn type_list(w: &World, kind: TypeId) -> Vec<(i32, i32)> {
    w.units.iter().filter(|u| u.living() && u.kind == kind).take(32).map(|u| (u.pos.x >> 11, u.pos.y >> 11)).collect()
}

/// The deposit search for mines (0x430559): the nearest free deposit of the mine's resources
/// within 156 64-px cells (path distance beyond 45).
fn mine_search(w: &World, t: &ObjectType, x: i32, y: i32) -> Option<(i32, i32)> {
    let mask = t.economy.producer.map_or(0, |p| p.mask);
    let (cx, cy) = (2 * x, 2 * y);
    let mut best = (156, None);
    for d in &w.deposits {
        if mask & (1 << d.resource) == 0 {
            continue;
        }
        let (sx, sy) = (d.x >> 6, d.y >> 6);
        let mut dist = norma(cx - sx, cy - sy);
        if dist >= 45 {
            dist = top_dist(w, cx, cy, sx, sy);
        }
        let taken = w.units.iter().any(|u| {
            u.living()
                && w.catalog.get(u.kind).economy.producer.is_some()
                && norma(u.pos.x / 16 - d.x, u.pos.y / 16 - d.y) < 128
        });
        if dist < best.0 && !taken {
            best = (dist, Some((d.x >> 7, d.y >> 7)));
        }
    }
    best.1
}

/// `FindApproximateBuildingPlace` (0x4306db).
fn approximate(w: &mut World, k: u8, kind: TypeId, wood: usize, stone: usize) -> Option<((i32, i32), i32)> {
    let center = w.cities[usize::from(k)].center?;
    plan_maps(w);
    refresh_danger(w, k);
    let t = w.catalog.get(kind).clone();
    let p = w.plan.clone().expect("built");
    let danger = w.cities[usize::from(k)].danger.clone();
    let (x, y) = center;
    let s = |list: &[(i32, i32)], test, r| search(w, &Ctx { p: &p, list }, &danger, test, x, y, r, None);
    let found = match t.role.usage {
        usage::MELNICA => s(&[], Test::Melnica, 25).map(|c| (c, 0)),
        usage::SKLAD => {
            let list = type_list(w, kind);
            // AC tests wood for both kinds; the 10-ring search is a prefix of the 60-ring one.
            if let Some(c) = s(&list, Test::WoodSklad, 10) {
                Some((c, 0x679))
            } else {
                s(&list, Test::WoodSklad, 60).map(|c| (c, if wood <= stone { 0x279 } else { 0x479 }))
            }
        }
        usage::TOWER => {
            let list = type_list(w, kind);
            let (mut ne, mut ex, mut ey) = (0, 0, 0);
            for u in w.units.iter().filter(|u| u.living() && u.kind != kind && u.owner == 0) {
                ne += 1;
                ex += u.pos.x >> 11;
                ey += u.pos.y >> 11;
            }
            if ne > 0 {
                (ex, ey) = (ex / ne, ey / ne);
            }
            let ctx = Ctx { p: &p, list: &list };
            if list.len() > 1 {
                search(w, &ctx, &danger, Test::Tower, x, y, 40, None)
            } else {
                search(w, &ctx, &danger, Test::Tower, x, y, 40, Some(((x, y), (ex, ey))))
            }
            .map(|c| (c, 0))
        }
        usage::MINE => mine_search(w, &t, x, y).map(|c| (c, 0)),
        usage::PORT => {
            let list = type_list(w, kind);
            s(&list, Test::Port, 60).map(|c| (c, 0))
        }
        _ => s(&[], Test::Building, 40).map(|c| (c, 0)),
    };
    if let Some(pr) = w.cities[usize::from(k)].project.as_mut() {
        pr.attempts += 1;
    }
    found
}

/// `SetUnusable` (0x4408c9).
fn set_unusable(w: &mut World, x: i32, y: i32, m: u8) {
    let p = plan_maps(w);
    if x >= 0 && y >= 0 && x < p.max && y < p.max {
        let o = (x + y * p.w) as usize;
        p.cant[o] |= m;
    }
}

/// `MarkUnusablePlace` (0x430cb3).
fn mark_unusable(w: &mut World, usage_: u8, (x, y): (i32, i32)) {
    match usage_ {
        usage::MELNICA => set_unusable(w, x, y, C_MILL),
        usage::CENTER => {
            set_unusable(w, x, y, C_SKLAD);
            set_unusable(w, x, y, C_BUILDING);
        }
        usage::SKLAD => set_unusable(w, x, y, C_SKLAD),
        usage::PORT => set_unusable(w, x, y, C_PORT),
        _ => set_unusable(w, x, y, C_BUILDING),
    }
}

/// `CheckCreationAbility` (0x513b2c), building branch: the site test at `(x, y)` (1/16 px);
/// mines snap to the deposit.
fn site_ok(w: &World, k: u8, t: &ObjectType, mut x: i32, mut y: i32) -> Option<(i32, i32)> {
    let msx = w.map.land.width >> 1;
    if (x >> 9) <= 0 || (y >> 9) <= 0 || (x >> 9) >= msx || (y >> 9) >= msx {
        return None;
    }
    let (pdx, pdy) = (t.location.0 << 4, (2 * t.location.1) << 4);
    let (cmx, cmy, br) = t.check_center;
    let r = (br + 15) << 8;
    let center = |x: i32, y: i32| {
        let (xm, ym) = ((x + pdx) >> 8, (y + pdy) >> 8);
        (xm, ym, ((xm + cmx) << 8) + 128, ((ym + cmy) << 8) + 128)
    };
    let (mut xm, mut ym, mut xc, mut yc) = center(x, y);
    let mine = t.economy.producer.is_some();
    if mine {
        // CheckMinePosition (0x4d2100): the deposit within the radius.
        let mask = t.economy.producer.map_or(0, |p| p.mask);
        let (px, py, rp) = (x >> 4, y >> 4, r >> 4);
        let d = w.deposits.iter().rfind(|d| mask & (1 << d.resource) != 0 && norma(d.x - px, d.y - py) < rp)?;
        (x, y) = (d.x << 4, d.y << 4);
        (xm, ym, xc, yc) = center(x, y);
    } else {
        // CheckSpritesInAreaNew (0x4d1ec0): sprites (fields 240 px nearer) block.
        let (px, py, rp) = (xc >> 4, yc >> 4, r >> 4);
        // Trees, carcasses, animations and stones without a deposit never block; holes block
        // at their position, deposits with their radius.
        let hole =
            w.objects.iter().any(|o| o.group == acr_formats::m3d::SpriteGroup::Holes && norma(o.x - px, o.y - py) < rp);
        if hole || w.spacing.iter().any(|&(sx, sy, sr)| sr > 0 && norma(sx - px, sy - py) < rp + sr) {
            return None;
        }
        if w.sprites.iter().any(|s| {
            s.group == acr_formats::m3d::SpriteGroup::Complex && !s.removed && norma(s.x - px, s.y - py) < rp - 240
        }) {
            return None;
        }
    }
    // CheckMonstersInArea (0x5133bc): enemy objects.
    let mask = w.nation_mask[usize::from(k) & 7];
    if w.units
        .iter()
        .any(|u| u.living() && u.nmask & mask == 0 && u.owner != 7 && norma(u.pos.x - xc, u.pos.y - yc) < r)
    {
        return None;
    }
    // CheckBuildingsInArea (0x512e73).
    let (bx0, by0, bx1, by1) = t.build_bar;
    let rect = (xc + bx0 * 16, yc + by0 * 16, xc + bx1 * 16, yc + by1 * 16);
    if rect.0 != rect.2 || rect.1 != rect.3 {
        if mine {
            let (mx, my) = ((rect.0 + rect.2) >> 1, (rect.1 + rect.3) >> 1);
            if w.units.iter().any(|u| {
                u.building
                    && w.catalog.get(u.kind).economy.producer.is_some()
                    && norma(u.pos.x - mx, u.pos.y - my) < 0x780
            }) {
                return None;
            }
        } else {
            let rot = |(x0, y0, x1, y1): (i32, i32, i32, i32)| {
                let (n0x, n0y, n1x, n1y) = (x0 + y0, y0 - x0, x1 + y1, y1 - x1);
                ((n0x + n1x) >> 1, (n0y + n1y) >> 1, (n1x - n0x).abs() >> 1, (n1y - n0y).abs() >> 1)
            };
            let (cx, cy, rx, ry) = rot(rect);
            for u in w.units.iter().filter(|u| u.building) {
                let bt = w.catalog.get(u.kind);
                let (a0, b0, a1, b1) = bt.build_bar;
                if a0 == a1 && b0 == b1 {
                    continue;
                }
                let (bcx, bcy, brx, bry) =
                    rot((u.pos.x + a0 * 16, u.pos.y + b0 * 16, u.pos.x + a1 * 16, u.pos.y + b1 * 16));
                if (bcx - cx).abs() < brx + rx && (bcy - cy).abs() < bry + ry {
                    return None;
                }
            }
        }
    }
    if !mine {
        let (mut maxz, mut minz) = (-100000, 100000);
        for &(cx, cy) in &t.check_points {
            let (px, py) = (xm + cx, ym + cy);
            let z = w.map.heights.at(px << 4, py << 4);
            maxz = maxz.max(z);
            minz = minz.min(z);
            if w.map.land.check_vline(px, py - 4, 8) || w.map.land.check_hline(px - 3, py, 6) {
                return None;
            }
        }
        if t.check_points.is_empty() {
            (maxz, minz) = (0, 0);
        }
        if (maxz - minz).abs() > 50 {
            return None;
        }
    }
    if t.port && crate::ship::port_point(&w.map.water, Pos::new(xc, yc)).is_none() {
        return None;
    }
    Some((x, y))
}

/// `SmartCreationUnit` (0x514712): the site test at the point, then on 9 rings of 32 px around
/// it; the first passing spot is built (`CreateObject`).
fn smart_creation(w: &mut World, k: u8, kind: TypeId, x: i32, y: i32) -> Option<EntityId> {
    let t = w.catalog.get(kind).clone();
    let spot = site_ok(w, k, &t, x, y).or_else(|| {
        if t.port {
            return None;
        }
        let (x0, y0) = ((x >> 8) << 8, (y >> 8) << 8);
        (1..10).find_map(|r| {
            crate::morale::ring(r).iter().find_map(|&(dx, dy)| site_ok(w, k, &t, x0 + (dx << 9), y0 + (dy << 9)))
        })
    })?;
    crate::construction::place(w, k, kind, spot.0 >> 4, spot.1 >> 4)
}

/// `FindCenter` (0x4407b6): the mean position of the nation's peasants (128 px cells).
fn find_center(w: &World, k: u8) -> Option<(i32, i32)> {
    let (mut sx, mut sy, mut n) = (0i64, 0i64, 0i64);
    for u in w.units.iter().filter(|u| u.owner == k && u.alive && w.catalog.get(u.kind).peasant) {
        sx += i64::from(u.pos.x >> 4);
        sy += i64::from(u.pos.y >> 4);
        n += 1;
    }
    (n > 0).then(|| (((sx / n) as i32) >> 7, ((sy / n) as i32) >> 7))
}

/// The first city tick of a computer nation (`StartAIEx` 0x565ae0 lists its mills and
/// storehouses; `FindCenter` on the first `ProcessCreation`). Returns whether the city has
/// its centre (the AI runs only then).
pub fn start(w: &mut World, k: u8) -> bool {
    let ki = usize::from(k);
    if w.cities[ki].center.is_none() {
        if w.cities[ki].mills.is_empty() {
            let mills: Vec<EntityId> = w
                .units
                .iter()
                .filter(|u| u.owner == k && u.living() && w.catalog.get(u.kind).role.usage == usage::MELNICA)
                .take(4)
                .map(|u| u.id)
                .collect();
            w.cities[ki].mills = mills;
        }
        w.cities[ki].center = find_center(w, k);
        return false;
    }
    true
}

/// Whether a building project is pending (`PresentProject`): `TryUnit` refuses buildings.
pub fn project_pending(w: &World, k: u8) -> bool {
    w.cities[usize::from(k) & 7].project.is_some()
}

/// A building project the AI accepted: it starts if an approximate place exists (true),
/// otherwise it is dropped silently.
pub fn accept(w: &mut World, k: u8, kind: TypeId) -> bool {
    let ki = usize::from(k);
    if w.cities[ki].project.is_some() {
        return false;
    }
    let (nw, ns) = (w.cities[ki].wood.len(), w.cities[ki].stone.len());
    let Some((near, options)) = approximate(w, k, kind, nw, ns) else { return false };
    let usage_ = w.catalog.get(kind).role.usage;
    w.cities[ki].project = Some(Project { kind, usage: usage_, near, placed: None, attempts: 0, options });
    true
}

/// The pending project's step of the city tick: one exact try while no site exists;
/// without a buildings collector, peasants are sent to a placed site.
pub fn process(w: &mut World, k: u8, collector: bool) {
    let ki = usize::from(k);
    let Some(pr) = w.cities[ki].project.clone() else { return };
    match pr.placed {
        None => {
            let x16 = (pr.near.0 << 11) + (w.random.rando() & 0x1000) - 0x800;
            let y16 = (pr.near.1 << 11) + (w.random.rando() & 0x1000) - 0x800;
            if let Some(id) = smart_creation(w, k, pr.kind, x16, y16) {
                placed(w, k, &pr, id, collector);
            } else if pr.attempts > 4 {
                w.cities[ki].project = None;
            } else {
                mark_unusable(w, pr.usage, pr.near);
                let (nw, ns) = (w.cities[ki].wood.len(), w.cities[ki].stone.len());
                match approximate(w, k, pr.kind, nw, ns) {
                    Some((near, options)) => {
                        if let Some(p) = w.cities[ki].project.as_mut() {
                            p.near = near;
                            p.options = options;
                        }
                    }
                    None => w.cities[ki].project = None,
                }
            }
        }
        Some(id) => {
            let b = id.0 as usize;
            let gone = !w.units.get(b).is_some_and(|u| u.alive);
            if gone || (!collector && send_builders(w, k, b, &pr)) {
                w.cities[ki].project = None;
            }
        }
    }
}

/// `on_placed` (0x432e48): the site is recorded; a mill blocks a 13 × 13 diamond of cells for
/// mills, storehouses and buildings.
fn placed(w: &mut World, k: u8, pr: &Project, id: EntityId, collector: bool) {
    let ki = usize::from(k);
    if pr.usage == usage::MELNICA {
        for dx in -6..=6 {
            for dy in -6..=6 {
                set_unusable(w, pr.near.0 + ((dx + dy) >> 1), pr.near.1 + ((dx - dy) >> 1), 0x19);
            }
        }
        if w.cities[ki].mills.len() < 4 {
            w.cities[ki].mills.push(id);
        }
    }
    if matches!(pr.usage, usage::SKLAD | usage::CENTER) && pr.options & 0xff == 121 {
        let alive = |w: &World, e: &EntityId| w.units.get(e.0 as usize).is_some_and(|u| u.living());
        if pr.options & 0x200 != 0 {
            if w.cities[ki].wood.len() < 8 {
                w.cities[ki].wood.push(id);
            } else if let Some(j) = w.cities[ki].wood.iter().position(|e| !alive(w, e)) {
                w.cities[ki].wood[j] = id;
            }
        }
        if pr.options & 0x400 != 0 {
            if w.cities[ki].stone.len() < 8 {
                w.cities[ki].stone.push(id);
            } else if let Some(j) = w.cities[ki].stone.iter().position(|e| !alive(w, e)) {
                w.cities[ki].stone[j] = id;
            }
        }
    }
    if collector {
        // The DLL builds it.
        w.cities[ki].project = None;
    } else if let Some(p) = w.cities[ki].project.as_mut() {
        p.placed = Some(id);
    }
}

/// `CreateTeamOfPeasants` (0x4312f7) and `BuildWithSelected` (0x4db525), for cities without
/// a buildings collector: the nearest peasants (squares of 128 px cells up to 25, then 200)
/// go to build.
fn send_builders(w: &mut World, k: u8, b: usize, pr: &Project) -> bool {
    let t = w.catalog.get(pr.kind);
    let stages = w.adv(k, pr.kind).build_stages;
    let nb = t.economy.build_points.len() as i32;
    let (min, max) = if stages > 800 || pr.usage == usage::MELNICA || pr.usage == usage::CENTER {
        let max = nb & 0xff;
        (if nb > 7 { nb - 5 } else { (max - 1) & 0xff }, max)
    } else if pr.usage == usage::TOWER {
        (5, 6)
    } else {
        match stages {
            ..100 => (1, 2),
            100..150 => (2, 5),
            150..200 => (3, 6),
            200..250 => (4, 5),
            250..300 => (4, 6),
            300..500 => (6, 8),
            _ => (10, 14),
        }
    };
    let max = max.min(120) as usize;
    let (xc, yc) = (w.units[b].pos.x >> 11, w.units[b].pos.y >> 11);
    let mut team = search_peasants(w, k, xc, yc, 25, max);
    if team.is_empty() {
        team = search_peasants(w, k, xc, yc, 200, max);
    }
    let _ = min;
    if team.is_empty() {
        return false;
    }
    let bp = w.units[b].pos;
    team.sort_by_key(|&i| norma(w.units[i].pos.x - bp.x, w.units[i].pos.y - bp.y));
    for i in team {
        w.random.rando();
        crate::construction::build_obj(w, i, b, 0xFF);
    }
    true
}

/// `SearchPeasantsInSquare` (0x43108d): peasants of the nation in growing squares of 128 px
/// cells that are not building.
fn search_peasants(w: &World, k: u8, xc: i32, yc: i32, r: i32, max: usize) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let take = |cx: i32, cy: i32, out: &mut Vec<usize>| {
        for &j in w.buckets.get(cx, cy) {
            let j = j as usize;
            let u = &w.units[j];
            if out.len() < max
                && u.owner == k
                && u.living()
                && w.catalog.get(u.kind).peasant
                && !matches!(u.orders.last(), Some(crate::world::Order::Build { .. }))
                && !out.contains(&j)
            {
                out.push(j);
            }
        }
    };
    for i in 0..r {
        if out.len() >= max {
            break;
        }
        for dy in -i..=i {
            take(xc - i, yc + dy, &mut out);
        }
        for dy in -i..=i {
            take(xc + i, yc + dy, &mut out);
        }
        for dx in -i + 1..i {
            take(xc + dx, yc - i, &mut out);
            take(xc + dx, yc + i, &mut out);
        }
    }
    out
}

/// The city's mills (`MelnID`) that still stand and are finished.
pub fn ready_mills(w: &World, k: u8) -> Vec<usize> {
    w.cities[usize::from(k) & 7]
        .mills
        .iter()
        .map(|e| e.0 as usize)
        .filter(|&i| w.units.get(i).is_some_and(|u| u.living() && u.stage >= w.max_stage(u)))
        .collect()
}
