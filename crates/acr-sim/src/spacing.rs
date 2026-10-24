//! Body spacing (optional rule [`crate::Rules::body_spacing`], not in the original game):
//! men and animals have round bodies, horses long ones (capsules along the heading), that
//! take up space. Every tick, two bodies closer than the sum of their radii are pushed
//! apart, softly (half the overlap per tick, at most [`MAX_PUSH`]) and by mass (a horse
//! moves a man more than the other way round; two horses mostly sideways).
//! Off (the default) nothing here runs and the state hash is unchanged.
//!
//! The original engine only keeps one unit per 16 px lock cell (the occupancy grid); two
//! units in neighbouring cells can still stand a pixel apart, and units walking through
//! locks (out of a building, panicking) ignore the grid. See `docs/body-spacing.md`.
//!
//! - Sizes are in world pixels at the models' scale of 30 px per metre
//!   (`acr_render::view3d::PIXELS_PER_METRE`): a man 0.3 m (9 px), a horse and rider a
//!   2.4 m × 0.7 m capsule ([`HORSE`]), animals by size. Infantry slots are 32 px apart, so
//!   men (18 px) in their slots never touch; a group move of riders gets rows
//!   [`RIDER_ROW`] apart under the rule (`motion::place`).
//! - Pushes never move a body into a locked (map) cell; a standing unit moves its occupancy
//!   reservation along, only into a free cell.
//! - Units walking through locks ([`crate::Unit::unlimited`]: just out of a building, off a
//!   ship, panicking) neither push nor are pushed; once they take a cell they spread out.
//! - Dead and dying units, units inside buildings, buildings, cannons and ships have no body
//!   here.

use crate::World;
use crate::catalog::{Medium, MotionStyle, ObjectType};
use crate::world::Unit;

/// World pixels per metre (the models' scale, `acr_render::view3d::PIXELS_PER_METRE`).
pub const PX_PER_METRE: i32 = 30;
/// Most a body is pushed in one tick, 1/16 px (2 px).
pub const MAX_PUSH: i32 = 32;
/// Side of the spatial hash cells, 1/16 px (64 px). A body reaching further than half a
/// cell (a horse, 36 px) searches two cells around it, every other body one: the pair
/// distance stays under the searched range (horse–horse 72 px < 128, man–horse 45 < 64).
const GRID_SHIFT: i32 = 10;

/// A body: a capsule (a segment of half length `half_len` along the unit's heading, swept
/// by `radius`; a circle when `half_len` is 0), in 1/16 px, and a relative mass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub radius: i32,
    pub half_len: i32,
    pub mass: i32,
}

impl Shape {
    const fn circle(radius: i32, mass: i32) -> Self {
        Self { radius, half_len: 0, mass }
    }

    /// Farthest point of the body from its centre, 1/16 px.
    pub const fn reach(&self) -> i32 {
        self.radius + self.half_len
    }
}

/// Radius in 1/16 px of `cm` centimetres.
const fn cm(cm: i32) -> i32 {
    cm * PX_PER_METRE * crate::fixed::SUBPIXEL / 100
}

/// A horse and rider: a capsule 2.4 m long and 0.7 m wide (`engd18.glb`: the horse is
/// 2.33 m nose to tail and 0.60 m across the body, 0.72 m across the saddle), so radius
/// 0.35 m and a segment of 2 × 0.85 m. Heavy: a man takes 8/9 of a push against it.
pub const HORSE: Shape = Shape { radius: cm(35), half_len: cm(85), mass: 8 };
/// Pitch of the rows of riders in a group move under the rule, 1/16 px: a horse's length
/// and 4 px (76 px; the original 4 x `GEOMETRY` radius gives 60 px, 12 px short).
pub const RIDER_ROW: i32 = 2 * HORSE.reach() + 4 * crate::fixed::SUBPIXEL;
/// A man: 0.3 m (shoulders and hanging arms about 0.55 m across, coat to knapsack 0.43 m
/// deep in `engm18.glb`).
pub const MAN: Shape = Shape::circle(cm(30), 1);

/// The body of a type, `None` for things without one here (buildings, cannons, ships,
/// flying animals).
pub fn shape_of(t: &ObjectType) -> Option<Shape> {
    if t.building
        || !t.mobile()
        || t.medium != Medium::Land
        || matches!(t.motion_style, MotionStyle::ComplexRotate | MotionStyle::Ships | MotionStyle::NewShips)
    {
        return None;
    }
    if let Some(beast) = crate::body::Beast::of(&t.md_name) {
        return match beast.size(crate::body::Beast::small(&t.md_name)) {
            // The eagle flies.
            _ if beast == crate::body::Beast::Other => None,
            1 => Some(Shape::circle(cm(35), 2)),
            2 | 3 => Some(Shape::circle(cm(50), 4)),
            _ => Some(Shape::circle(cm(70), 8)),
        };
    }
    if is_rider(t) {
        return Some(HORSE);
    }
    Some(MAN)
}

/// A horse and rider. `USAGE` alone misses most of them (dragoons and their officers are
/// `STRELOK`, Sioux riders `LIGHTINFANTRY`); every rider of the original data has
/// `GEOMETRY` radius 15 (all 26 with the horse sounds `MANHOR`, `PUSTIHKAKON`), every man 10.
pub fn is_rider(t: &ObjectType) -> bool {
    matches!(t.role.usage, 8 | 22 | 24) || t.radius2 >= 15
}

fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Directions (×256) for two bodies on the very same point.
const SPREAD: [(i32, i32); 8] =
    [(256, 0), (181, 181), (0, 256), (-181, 181), (-256, 0), (-181, -181), (0, -256), (181, -181)];

fn has_body(u: &Unit) -> bool {
    u.active() && !u.building && !u.unlimited
}

type P = (i64, i64);

/// The body's segment, 1/16 px.
fn segment(x: i32, y: i32, dir: u8, s: Shape) -> (P, P) {
    let (x, y) = (i64::from(x), i64::from(y));
    if s.half_len == 0 {
        return ((x, y), (x, y));
    }
    let h = i64::from(s.half_len);
    let (hx, hy) = (h * i64::from(crate::trig::cos(dir)) / 256, h * i64::from(crate::trig::sin(dir)) / 256);
    ((x - hx, y - hy), (x + hx, y + hy))
}

/// The point of segment `a..b` nearest to `p`.
fn nearest(p: P, (a, b): (P, P)) -> P {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    let den = ex * ex + ey * ey;
    if den == 0 {
        return a;
    }
    let num = ((p.0 - a.0) * ex + (p.1 - a.1) * ey).clamp(0, den);
    (a.0 + ex * num / den, a.1 + ey * num / den)
}

fn cross(o: P, a: P, b: P) -> i64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn dist2((p, q): (P, P)) -> i64 {
    (q.0 - p.0) * (q.0 - p.0) + (q.1 - p.1) * (q.1 - p.1)
}

/// Closest points `(on a, on b)` of two segments and whether they cross. Without a crossing
/// the closest pair has an end point of one segment in it, so four candidates suffice
/// (in a fixed order: ties keep the first).
fn closest(a: (P, P), b: (P, P)) -> (P, P, bool) {
    // Two circles (most pairs): their centres.
    if a.0 == a.1 && b.0 == b.1 {
        return (a.0, b.0, false);
    }
    let d1 = cross(b.0, b.1, a.0).signum();
    let d2 = cross(b.0, b.1, a.1).signum();
    let d3 = cross(a.0, a.1, b.0).signum();
    let d4 = cross(a.0, a.1, b.1).signum();
    let crossing = d1 * d2 < 0 && d3 * d4 < 0;
    let mut best = (a.0, nearest(a.0, b));
    for cand in [(a.1, nearest(a.1, b)), (nearest(b.0, a), b.0), (nearest(b.1, a), b.1)] {
        if dist2(cand) < dist2(best) {
            best = cand;
        }
    }
    (best.0, best.1, crossing)
}

/// Horse against horse: the push turned mostly across `a`'s heading `dir`, so riders side
/// by side spread and riders nose to tail step aside instead of shoving each other along
/// the column (a quarter of the lengthwise part stays). `(ux, uy)` is the direction from
/// `a` to `b` (×256), `(rx, ry)` `b`'s offset from `a`.
fn sideways(ux: i64, uy: i64, dir: u8, rx: i64, ry: i64, parity: usize) -> (i64, i64) {
    let (hx, hy) = (i64::from(crate::trig::cos(dir)), i64::from(crate::trig::sin(dir)));
    let along = (ux * hx + uy * hy) / 256;
    let (mut lx, mut ly) = (ux - along * hx / 256, uy - along * hy / 256);
    if lx * lx + ly * ly < 96 * 96 {
        // Nose to tail: aside, to the side `b` is on (on the axis, fixed by the indices).
        let side = match (hx * ry - hy * rx).signum() {
            0 if parity & 1 == 0 => 1,
            0 => -1,
            s => s,
        };
        (lx, ly) = (-hy * side * 3 / 4, hx * side * 3 / 4);
    }
    let (vx, vy) = (lx + along * hx / 256 / 4, ly + along * hy / 256 / 4);
    let n = isqrt(vx * vx + vy * vy).max(1);
    (vx * 256 / n, vy * 256 / n)
}

/// Search range in grid cells of a body.
const fn range_of(s: Shape) -> i32 {
    if s.reach() > 1 << (GRID_SHIFT - 1) { 2 } else { 1 }
}

/// One tick of the rule: pushes overlapping bodies apart (after the units moved).
pub fn separate(w: &mut World) {
    if !w.rules.body_spacing {
        return;
    }
    // Shapes per type, then the bodies in unit order.
    let mut shapes: Vec<Option<Option<Shape>>> = vec![None; w.catalog.len()];
    let mut bodies: Vec<Body> = Vec::new();
    for (i, u) in w.units.iter().enumerate() {
        if !has_body(u) {
            continue;
        }
        let k = usize::from(u.kind.0);
        let s = *shapes[k].get_or_insert_with(|| shape_of(w.catalog.get(u.kind)));
        if let Some(s) = s {
            bodies.push((i, u.pos.x, u.pos.y, s, u.dir));
        }
    }
    if bodies.len() < 2 {
        return;
    }
    let push = {
        crate::prof_scope!("sim.spacing_pairs");
        pushes(&bodies)
    };
    let map = w.map.clone();
    for (b, &(i, ..)) in bodies.iter().enumerate() {
        let (mut px, mut py) = push[b];
        if px == 0 && py == 0 {
            continue;
        }
        let len = isqrt(px * px + py * py);
        if len > i64::from(MAX_PUSH) {
            px = px * i64::from(MAX_PUSH) / len;
            py = py * i64::from(MAX_PUSH) / len;
        }
        let (px, py) = (px as i32, py as i32);
        // The whole push, else along one axis, else none.
        for (qx, qy) in [(px, py), (px, 0), (0, py)] {
            if (qx, qy) != (0, 0) && try_push(w, &map.land, i, qx, qy) {
                break;
            }
        }
    }
}

/// A body for the pair search: unit index, position, shape, heading.
type Body = (usize, i32, i32, Shape, u8);

/// The push (1/16 px, before the cap) of every body from the bodies it overlaps.
///
/// Every pair closer than their reaches is worked once, by the lower index, or by the only
/// one of the two that searches far enough (a man two cells from a horse). The bodies are
/// sorted by cell into one table (each cell's bodies in unit order, with their largest
/// reach); a body scans each row of its search range from one lookup and skips cells too
/// far for any of their bodies. The pairs and their pushes are those of the plain search
/// over every cell (`pushes_reference`), and integer sums do not depend on the order.
fn pushes(bodies: &[Body]) -> Vec<(i64, i64)> {
    let key = |x: i32, y: i32| ((y >> GRID_SHIFT) as i64) << 32 | ((x >> GRID_SHIFT) as u32 as i64);
    let mut order: Vec<(i64, u32)> =
        bodies.iter().enumerate().map(|(b, &(_, x, y, ..))| (key(x, y), b as u32)).collect();
    order.sort_unstable();
    // The bodies in cell order, and the cells: key, first and end in `sorted`, largest reach.
    let sorted: Vec<(u32, Body)> = order.iter().map(|&(_, b)| (b, bodies[b as usize])).collect();
    let mut cells: Vec<(i64, u32, u32, i32)> = Vec::new();
    for (n, &(k, b)) in order.iter().enumerate() {
        let reach = bodies[b as usize].3.reach();
        match cells.last_mut() {
            Some(c) if c.0 == k => {
                c.2 = n as u32 + 1;
                c.3 = c.3.max(reach);
            }
            _ => cells.push((k, n as u32, n as u32 + 1, reach)),
        }
    }
    let side = 1i64 << GRID_SHIFT;
    let mut push = vec![(0i64, 0i64); bodies.len()];
    for (a, &(ia, xa, ya, sa, da)) in bodies.iter().enumerate() {
        let (gx, gy) = (xa >> GRID_SHIFT, ya >> GRID_SHIFT);
        let range = range_of(sa);
        let seg_a = segment(xa, ya, da, sa);
        let mut work = |cx: i32, cy: i32, cell: &(i64, u32, u32, i32)| {
            // Too far for any body in the cell: its nearest point lies beyond both reaches.
            let (x0, y0) = (i64::from(cx) * side, i64::from(cy) * side);
            let ex = (x0 - i64::from(xa)).max(i64::from(xa) - (x0 + side - 1)).max(0);
            let ey = (y0 - i64::from(ya)).max(i64::from(ya) - (y0 + side - 1)).max(0);
            let far = i64::from(sa.reach() + cell.3);
            if ex * ex + ey * ey >= far * far {
                return;
            }
            for &(b, (ib, xb, yb, sb, db)) in &sorted[cell.1 as usize..cell.2 as usize] {
                let b = b as usize;
                let rb = range_of(sb);
                let b_sees_a = (cx - gx).abs() <= rb && (cy - gy).abs() <= rb;
                if b == a || (b < a && b_sees_a) {
                    continue;
                }
                let (cdx, cdy) = (i64::from(xb - xa), i64::from(yb - ya));
                let reach = i64::from(sa.reach() + sb.reach());
                if cdx * cdx + cdy * cdy >= reach * reach {
                    continue;
                }
                if let Some((dxa, dya, dxb, dyb)) = pair(seg_a, (ia, sa, da), (ib, xb, yb, sb, db), cdx, cdy) {
                    push[a].0 += dxa;
                    push[a].1 += dya;
                    push[b].0 += dxb;
                    push[b].1 += dyb;
                }
            }
        };
        for cy in gy - range..=gy + range {
            // Cells left of x = 0 (none on a map) sort after the row's others: one by one.
            for cx in gx - range..(gx + range + 1).min(0) {
                let k = i64::from(cy) << 32 | (cx as u32 as i64);
                let at = cells.partition_point(|c| c.0 < k);
                if let Some(cell) = cells.get(at).filter(|c| c.0 == k) {
                    work(cx, cy, cell);
                }
            }
            let (lo, hi) = ((gx - range).max(0), gx + range);
            if lo > hi {
                continue;
            }
            let (klo, khi) = (i64::from(cy) << 32 | i64::from(lo), i64::from(cy) << 32 | i64::from(hi));
            let mut at = cells.partition_point(|c| c.0 < klo);
            while let Some(cell) = cells.get(at).filter(|c| c.0 <= khi) {
                work((cell.0 as u32) as i32, cy, cell);
                at += 1;
            }
        }
    }
    push
}

/// The pushes `(a, b)` of one pair within reach (`cdx, cdy` from `a` to `b`), `None` when
/// the bodies do not overlap.
fn pair(
    seg_a: (P, P),
    (ia, sa, da): (usize, Shape, u8),
    (ib, xb, yb, sb, db): Body,
    cdx: i64,
    cdy: i64,
) -> Option<(i64, i64, i64, i64)> {
    let need = i64::from(sa.radius + sb.radius);
    let (pa, pb, crossing) = closest(seg_a, segment(xb, yb, db, sb));
    let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
    let d2 = dx * dx + dy * dy;
    if !crossing && d2 >= need * need {
        return None;
    }
    let d = isqrt(d2);
    // Unit direction ×256 from a to b: between the closest points; centre to centre when
    // the segments cross or touch.
    let (mut ux, mut uy) = if d > 0 && !crossing {
        (dx * 256 / d, dy * 256 / d)
    } else {
        let c = isqrt(cdx * cdx + cdy * cdy);
        if c == 0 {
            let (x, y) = SPREAD[(ia ^ ib) & 7];
            (i64::from(x), i64::from(y))
        } else {
            (cdx * 256 / c, cdy * 256 / c)
        }
    };
    if sa.half_len > 0 && sb.half_len > 0 {
        (ux, uy) = sideways(ux, uy, da, cdx, cdy, ia ^ ib);
    }
    // Crossing segments are deeper in each other than their distance says.
    let overlap = if crossing { need + d } else { need - d };
    let (ma, mb) = (i64::from(sa.mass), i64::from(sb.mass));
    // Soft: half the overlap per tick, shared by the other's mass.
    let pa = overlap * mb / (ma + mb) / 2;
    let pb = overlap * ma / (ma + mb) / 2;
    Some((-(ux * pa / 256), -(uy * pa / 256), ux * pb / 256, uy * pb / 256))
}

/// The pair loop as it was before the cell table (m2-perf6): the reference the test
/// `pushes_match_the_reference` holds [`pushes`] to.
#[cfg(test)]
fn pushes_reference(bodies: &[Body]) -> Vec<(i64, i64)> {
    // Spatial hash: body indices sorted by grid cell (stable, so ties keep unit order).
    let key = |x: i32, y: i32| ((y >> GRID_SHIFT) as i64) << 32 | ((x >> GRID_SHIFT) as u32 as i64);
    let mut order: Vec<(i64, u32)> =
        bodies.iter().enumerate().map(|(b, &(_, x, y, ..))| (key(x, y), b as u32)).collect();
    order.sort_unstable();
    let mut push = vec![(0i64, 0i64); bodies.len()];
    for (a, &(ia, xa, ya, sa, da)) in bodies.iter().enumerate() {
        let (gx, gy) = (xa >> GRID_SHIFT, ya >> GRID_SHIFT);
        let range = range_of(sa);
        let seg_a = segment(xa, ya, da, sa);
        for cy in gy - range..=gy + range {
            for cx in gx - range..=gx + range {
                let k = (i64::from(cy)) << 32 | (cx as u32 as i64);
                let start = order.partition_point(|&(kk, _)| kk < k);
                for &(kk, b) in &order[start..] {
                    if kk != k {
                        break;
                    }
                    let b = b as usize;
                    let (ib, xb, yb, sb, db) = bodies[b];
                    // Each pair once: by the lower index, or by the only one of the two
                    // that searches far enough (a man two cells from a horse).
                    let rb = range_of(sb);
                    let b_sees_a = (cx - gx).abs() <= rb && (cy - gy).abs() <= rb;
                    if b == a || (b < a && b_sees_a) {
                        continue;
                    }
                    let (cdx, cdy) = (i64::from(xb - xa), i64::from(yb - ya));
                    let reach = i64::from(sa.reach() + sb.reach());
                    if cdx * cdx + cdy * cdy >= reach * reach {
                        continue;
                    }
                    let need = i64::from(sa.radius + sb.radius);
                    let (pa, pb, crossing) = closest(seg_a, segment(xb, yb, db, sb));
                    let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
                    let d2 = dx * dx + dy * dy;
                    if !crossing && d2 >= need * need {
                        continue;
                    }
                    let d = isqrt(d2);
                    // Unit direction ×256 from a to b: between the closest points; centre
                    // to centre when the segments cross or touch.
                    let (mut ux, mut uy) = if d > 0 && !crossing {
                        (dx * 256 / d, dy * 256 / d)
                    } else {
                        let c = isqrt(cdx * cdx + cdy * cdy);
                        if c == 0 {
                            let (x, y) = SPREAD[(ia ^ ib) & 7];
                            (i64::from(x), i64::from(y))
                        } else {
                            (cdx * 256 / c, cdy * 256 / c)
                        }
                    };
                    if sa.half_len > 0 && sb.half_len > 0 {
                        (ux, uy) = sideways(ux, uy, da, cdx, cdy, ia ^ ib);
                    }
                    // Crossing segments are deeper in each other than their distance says.
                    let overlap = if crossing { need + d } else { need - d };
                    let (ma, mb) = (i64::from(sa.mass), i64::from(sb.mass));
                    // Soft: half the overlap per tick, shared by the other's mass.
                    let pa = overlap * mb / (ma + mb) / 2;
                    let pb = overlap * ma / (ma + mb) / 2;
                    push[a].0 -= ux * pa / 256;
                    push[a].1 -= uy * pa / 256;
                    push[b].0 += ux * pb / 256;
                    push[b].1 += uy * pb / 256;
                }
            }
        }
    }
    push
}

/// Moves unit `i` by `(dx, dy)` (1/16 px) if it stays off locked cells; a standing unit
/// holding its cell takes the new cell's reservation, only when it is free.
fn try_push(w: &mut World, land: &crate::map::LockMap, i: usize, dx: i32, dy: i32) -> bool {
    let u = &w.units[i];
    let to = crate::Pos::new(u.pos.x + dx, u.pos.y + dy);
    let (from_cell, to_cell) = (Unit::cell_of(u.pos, u.lx), Unit::cell_of(to, u.lx));
    if from_cell != to_cell {
        if land.check_bar(to_cell.0, to_cell.1, u.lx, u.lx) {
            return false;
        }
        if !u.in_motion && u.cell == from_cell {
            let lx = u.lx;
            w.occupancy.clr_bar(from_cell.0, from_cell.1, lx);
            if w.occupancy.check_bar(to_cell.0, to_cell.1, lx, lx) {
                w.occupancy.set_bar(from_cell.0, from_cell.1, lx);
                return false;
            }
            w.occupancy.set_bar(to_cell.0, to_cell.1, lx);
            w.units[i].cell = to_cell;
        }
    }
    w.units[i].pos = to;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radii_follow_the_model_scale() {
        assert_eq!(cm(30), 144); // 9 px
        assert_eq!(cm(50), 240); // 15 px
        assert_eq!((HORSE.radius, HORSE.half_len, HORSE.reach()), (168, 408, 576)); // 10.5, 25.5, 36 px
        assert_eq!(isqrt(1_000_000), 1000);
        assert_eq!(isqrt(99), 9);
    }

    #[test]
    fn pushes_match_the_reference() {
        // Dense crowds of men, riders and a big animal, some on one point, near x = 0 and
        // on cell edges: the cell table finds the same pairs as the plain search.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n) as i32
        };
        let big = Shape { radius: cm(60), half_len: cm(40), mass: 12 };
        for round in 0..40 {
            let n = 50 + 40 * round;
            let span = 2000 + 400 * round;
            let mut bodies: Vec<Body> = Vec::new();
            for i in 0..n {
                let s = match next(10) {
                    0 | 1 => HORSE,
                    2 if round % 3 == 0 => big,
                    _ => MAN,
                };
                let (x, y) = if i > 0 && next(8) == 0 {
                    let p = bodies[next(i as u64) as usize];
                    (p.1, p.2)
                } else {
                    (next(span as u64), next(span as u64) + if round % 2 == 0 { 0 } else { 5000 })
                };
                bodies.push((i * 3 + 1, x, y, s, next(256) as u8));
            }
            assert_eq!(pushes(&bodies), pushes_reference(&bodies), "round {round}");
        }
    }
}
