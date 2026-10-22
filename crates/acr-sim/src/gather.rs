//! Peasant gathering from map sprites (`TakeResource` 0x4d2e10, `TakeResLink` 0x4d2fb0):
//! work at a source (trees: 4 wood per cycle of the work animation) until the load reaches
//! `PORTION`, carry it to the nearest `RESOURCEBASE` building, deliver, and go back. See
//! `docs/simulation.md`.

use crate::economy::{FOOD, STONE, WOOD, base_cell};
use crate::fixed::Pos;
use crate::motion::{set_anim, try_to_stand};
use crate::path::{Mover, create_path};
use crate::trig::{get_dir, norma};
use crate::world::{AnimKind, FRM_DEC, Order, World};

/// Delivery factors of the player (`WoodEff`, `StoneEff` 100; `FoodEff` 1010, whose last two
/// digits are the food factor in tenths).
const WOOD_EFF: i32 = 100;
const STONE_EFF: i32 = 100;
const FOOD_EFF: i32 = 1010;

/// A resource source on the map: a tree, field or carcass (`[SOURCES]` sprites).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Source {
    /// World pixels.
    pub x: i32,
    pub y: i32,
    /// Source type (`ResType`: 0 wood, 3 field food, 8 meat, ...).
    pub kind: u8,
    /// Amount per work cycle (`ResPerWork`).
    pub per_work: i32,
    /// Work radius in px.
    pub radius: i32,
}

/// Sources sorted into 128 px cells.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub list: Vec<Source>,
    side: i32,
    cells: Vec<Vec<u32>>,
}

impl Sources {
    pub fn new(list: Vec<Source>, map_side_px: i32) -> Self {
        let side = (map_side_px >> 7).max(1);
        let mut cells = vec![Vec::new(); (side * side) as usize];
        for (i, s) in list.iter().enumerate() {
            let (cx, cy) = ((s.x >> 7).clamp(0, side - 1), (s.y >> 7).clamp(0, side - 1));
            cells[(cy * side + cx) as usize].push(i as u32);
        }
        Self { list, side, cells }
    }

    fn cell(&self, cx: i32, cy: i32) -> &[u32] {
        if cx < 0 || cy < 0 || cx >= self.side || cy >= self.side {
            return &[];
        }
        &self.cells[(cy * self.side + cx) as usize]
    }

    /// `DetermineResource` (0x4d3ef0): type of the nearest source within 64 px of `(x, y)`.
    pub fn determine(&self, x: i32, y: i32) -> Option<u8> {
        let mut best: Option<(i32, u8)> = None;
        for cy in (y >> 7) - 1..=(y >> 7) + 1 {
            for cx in (x >> 7) - 1..=(x >> 7) + 1 {
                for &k in self.cell(cx, cy) {
                    let s = &self.list[k as usize];
                    let d = norma(s.x - x, s.y - y);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, s.kind));
                    }
                }
            }
        }
        best.filter(|b| b.0 < 64).map(|b| b.1)
    }

    /// `FindResourceObject` (0x4d2200, simplified): the nearest source of `kind`, searching
    /// rings of 128 px cells outwards (up to 12).
    pub fn find(&self, x: i32, y: i32, kind: u8) -> Option<usize> {
        let (cx, cy) = (x >> 7, y >> 7);
        for r in 0..12i32 {
            let mut best: Option<(i32, usize)> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    for &k in self.cell(cx + dx, cy + dy) {
                        let s = &self.list[k as usize];
                        if s.kind == kind {
                            let d = norma(s.x - x, s.y - y);
                            if best.is_none_or(|b| d < b.0) {
                                best = Some((d, k as usize));
                            }
                        }
                    }
                }
            }
            if let Some((_, k)) = best {
                return Some(k);
            }
        }
        None
    }
}

/// `TakeResource` (0x4d2e10): sends a peasant to gather sources of type `kind`, starting
/// with the one nearest to world pixel `(x, y)` (`FindResourceObject` searches from the
/// clicked point, not from the peasant). `append` queues the order after the current ones
/// (order type 2, used for units leaving a building towards a rally point); otherwise it
/// replaces them. False if the unit cannot gather it or no source is found.
pub fn take_resource(w: &mut World, i: usize, x: i32, y: i32, kind: u8, prio: u8, append: bool) -> bool {
    let u = &w.units[i];
    let t = w.catalog.get(u.kind);
    let Some(entry) = t.economy.take_res.get(usize::from(kind)).copied().flatten() else { return false };
    if !u.active() || u.unlimited || prio < u.prio && !u.orders.is_empty() || entry.work == 0 {
        return false;
    }
    let Some(src) = w.sources.find(x, y, kind) else { return false };
    let s = w.sources.list[src];
    let u = &mut w.units[i];
    if u.carry_type != Some(kind) {
        u.carried = 0;
        u.carry_type = None;
    }
    let order = Order::Gather { kind, source: Some(src), at: (s.x, s.y), prio };
    if append {
        u.orders.insert(0, order);
    } else {
        u.orders.clear();
        u.orders.push(order);
        u.prio = prio;
        u.path.delay = 0;
    }
    true
}

/// `TakeResLink` (0x4d2fb0), once per tick, acting only at the end of an animation cycle.
pub fn gather_link(w: &mut World, i: usize, kind: u8, source: Option<usize>, at: (i32, i32), prio: u8) {
    if w.units[i].frame < w.units[i].anim_frames - FRM_DEC {
        return;
    }
    w.random.rando();
    w.units[i].prio = prio;
    let catalog = w.catalog.clone();
    let t = catalog.get(w.units[i].kind);
    let Some(entry) = t.economy.take_res.get(usize::from(kind)).copied().flatten() else {
        w.units[i].orders.pop();
        return;
    };
    let dst = entry.credited;
    if dst < 8 && w.units[i].carried >= t.economy.portion[dst] && !carry_home(w, i, kind, dst) {
        return;
    }
    // Gather.
    let (ux, uy) = w.units[i].pos.pixels();
    let src = match source.filter(|&s| s < w.sources.list.len()) {
        Some(s) => s,
        None => match w.sources.find(at.0, at.1, kind) {
            Some(s) => {
                let (sx, sy) = (w.sources.list[s].x, w.sources.list[s].y);
                if let Some(Order::Gather { source, at, .. }) = w.units[i].orders.last_mut() {
                    *source = Some(s);
                    *at = (sx, sy);
                }
                s
            }
            None => {
                w.units[i].orders.pop();
                return;
            }
        },
    };
    let s = w.sources.list[src];
    let d = norma(ux - s.x, uy - s.y);
    let u = &mut w.units[i];
    if d < s.radius {
        u.dest = None;
        u.vel = (0, 0);
        let dir = get_dir(s.x - ux, s.y - uy);
        let dd = dir.wrapping_sub(u.dir) as i8;
        if i32::from(dd).abs() >= 16 {
            u.dir = if dd > 0 { u.dir.wrapping_add(16) } else { u.dir.wrapping_sub(16) };
            return;
        }
        u.dir = dir;
        u.frame = 0;
        if u.local_stance != entry.work {
            u.stance = entry.work;
            try_to_stand(u, t, &mut w.random, false);
            return;
        }
        u.in_motion = false;
        u.carry_type = Some(kind);
        u.carried = (u.carried + s.per_work) & 0xFF;
        set_anim(u, t, AnimKind::Attack(entry.work - 1));
        return;
    }
    let cell = (s.x >> 4, s.y >> 4);
    let mover = Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
    if let Some((x, y)) = create_path(&w.map.land, mover, &mut u.path, &mut w.random, cell.0, cell.1, t.stop_distance) {
        u.dest = Some(Pos::new(x, y));
    }
}

/// Walks a loaded peasant to the nearest base and delivers there; true once delivered.
fn carry_home(w: &mut World, i: usize, kind: u8, dst: usize) -> bool {
    let catalog = w.catalog.clone();
    let u = &w.units[i];
    let base = w
        .units
        .iter()
        .filter(|b| {
            let bt = catalog.get(b.kind);
            b.living()
                && b.building
                && b.owner == u.owner
                && b.stage >= bt.economy.build_stages
                && bt.economy.resource_base & (1 << kind) != 0
        })
        .min_by_key(|b| norma(b.pos.x - u.pos.x, b.pos.y - u.pos.y));
    let Some(base) = base else {
        w.units[i].orders.pop();
        return false;
    };
    let bt = catalog.get(base.kind);
    let (bx, by) = base_cell(bt, base.pos);
    let (target, reach) = if let Some(&(x, y)) = bt.economy.born_points.last() {
        ((bx + x, by + y), 5)
    } else if !bt.economy.concentrator.is_empty() {
        let c = bt
            .economy
            .concentrator
            .iter()
            .map(|&(x, y)| (bx + x, by + y))
            .min_by_key(|&(x, y)| norma(x - u.cell.0, y - u.cell.1))
            .expect("not empty");
        (c, 6)
    } else {
        ((base.pos.x >> 8, base.pos.y >> 8), 6)
    };
    if norma(target.0 - u.cell.0, target.1 - u.cell.1) < reach {
        let mut r = u.carried;
        match dst {
            WOOD => r = r * WOOD_EFF / 100,
            STONE => r = r * STONE_EFF / 100,
            FOOD => r = r * (FOOD_EFF % 100) / 10,
            _ => {}
        }
        let owner = usize::from(u.owner);
        w.players[owner].resources[dst] = w.players[owner].resources[dst].saturating_add(r);
        let u = &mut w.units[i];
        u.carried = 0;
        u.carry_type = None;
        u.stance = 0;
        return true;
    }
    let t = catalog.get(w.units[i].kind);
    let u = &mut w.units[i];
    u.stance = 0;
    let mover = Mover { x: u.cell.0, y: u.cell.1, lx: u.lx };
    if let Some((x, y)) =
        create_path(&w.map.land, mover, &mut u.path, &mut w.random, target.0, target.1, t.stop_distance)
    {
        u.dest = Some(Pos::new(x, y));
    }
    false
}
