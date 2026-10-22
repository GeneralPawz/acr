//! Body spacing (optional rule [`crate::Rules::body_spacing`], not in the original game):
//! men, horses and animals have round bodies that take up space. Every tick, two bodies
//! closer than the sum of their radii are pushed apart, softly (half the overlap per tick,
//! at most [`MAX_PUSH`]) and by mass (a horse moves a man more than the other way round).
//! Off (the default) nothing here runs and the state hash is unchanged.
//!
//! The original engine only keeps one unit per 16 px lock cell (the occupancy grid); two
//! units in neighbouring cells can still stand a pixel apart, and units walking through
//! locks (out of a building, panicking) ignore the grid. See `docs/body-spacing.md`.
//!
//! - Radii are in world pixels at the models' scale of 30 px per metre
//!   (`acr_render::view3d::PIXELS_PER_METRE`): a man 0.3 m (9 px), a horse and rider one
//!   0.5 m circle (15 px), animals by size. Formation slots are 32 px apart, so men (18 px)
//!   and riders (30 px) in their slots never touch and formations keep their shape.
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
/// Side of the spatial hash cells, 1/16 px (64 px: more than the largest pair distance).
const GRID_SHIFT: i32 = 10;

/// A body: radius in 1/16 px and a relative mass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub radius: i32,
    pub mass: i32,
}

/// Radius in 1/16 px of `cm` centimetres.
const fn cm(cm: i32) -> i32 {
    cm * PX_PER_METRE * crate::fixed::SUBPIXEL / 100
}

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
            1 => Some(Shape { radius: cm(35), mass: 2 }),
            2 | 3 => Some(Shape { radius: cm(50), mass: 4 }),
            _ => Some(Shape { radius: cm(70), mass: 8 }),
        };
    }
    // FASTHORSE, HARDHORCE, HORSE-STRELOK: a rider. One circle of 0.5 m stands for the
    // 2.4 m × 0.7 m horse: it keeps riders in 32 px formation slots apart from each other.
    if matches!(t.role.usage, 8 | 22 | 24) {
        return Some(Shape { radius: cm(50), mass: 5 });
    }
    Some(Shape { radius: cm(30), mass: 1 })
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

/// One tick of the rule: pushes overlapping bodies apart (after the units moved).
pub fn separate(w: &mut World) {
    if !w.rules.body_spacing {
        return;
    }
    // Shapes per type, then the bodies in unit order.
    let mut shapes: Vec<Option<Option<Shape>>> = vec![None; w.catalog.len()];
    let mut bodies: Vec<(usize, i32, i32, Shape)> = Vec::new();
    for (i, u) in w.units.iter().enumerate() {
        if !has_body(u) {
            continue;
        }
        let k = usize::from(u.kind.0);
        let s = *shapes[k].get_or_insert_with(|| shape_of(w.catalog.get(u.kind)));
        if let Some(s) = s {
            bodies.push((i, u.pos.x, u.pos.y, s));
        }
    }
    if bodies.len() < 2 {
        return;
    }
    // Spatial hash: body indices sorted by grid cell (stable, so ties keep unit order).
    let key = |x: i32, y: i32| ((y >> GRID_SHIFT) as i64) << 32 | ((x >> GRID_SHIFT) as u32 as i64);
    let mut order: Vec<(i64, u32)> =
        bodies.iter().enumerate().map(|(b, &(_, x, y, _))| (key(x, y), b as u32)).collect();
    order.sort_unstable();
    let mut push = vec![(0i64, 0i64); bodies.len()];
    for (a, &(ia, xa, ya, sa)) in bodies.iter().enumerate() {
        let (gx, gy) = (xa >> GRID_SHIFT, ya >> GRID_SHIFT);
        for cy in gy - 1..=gy + 1 {
            for cx in gx - 1..=gx + 1 {
                let k = (i64::from(cy)) << 32 | (cx as u32 as i64);
                let start = order.partition_point(|&(kk, _)| kk < k);
                for &(kk, b) in &order[start..] {
                    if kk != k {
                        break;
                    }
                    let b = b as usize;
                    // Each pair once.
                    if b <= a {
                        continue;
                    }
                    let (ib, xb, yb, sb) = bodies[b];
                    let need = i64::from(sa.radius + sb.radius);
                    let (dx, dy) = (i64::from(xb - xa), i64::from(yb - ya));
                    let d2 = dx * dx + dy * dy;
                    if d2 >= need * need {
                        continue;
                    }
                    let d = isqrt(d2);
                    // Unit direction ×256 from a to b.
                    let (ux, uy) = if d == 0 {
                        let (x, y) = SPREAD[(ia ^ ib) & 7];
                        (i64::from(x), i64::from(y))
                    } else {
                        (dx * 256 / d, dy * 256 / d)
                    };
                    let overlap = need - d;
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
    let map = w.map.clone();
    for (b, &(i, _, _, _)) in bodies.iter().enumerate() {
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
        assert_eq!(isqrt(1_000_000), 1000);
        assert_eq!(isqrt(99), 9);
    }
}
