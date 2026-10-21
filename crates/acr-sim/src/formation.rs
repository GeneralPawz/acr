//! Formations (brigades): an officer, an optional drummer and flag bearer, and soldiers in
//! the slots of an `ORDERS.LST` shape.
//!
//! Recovered from `LoadOrders` (0x4ed6e0), `PlaceBrigade` (0x41a7be), `Arrange` (0x4ad9af),
//! `PickCom` (0x41a65d), the brigade move handler (0x41b83a) and `AddKeepPositions`
//! (0x41c63e); see `docs/simulation.md`. A shape cell is 32 px in both directions; text
//! lines further down stand further in front, text columns go to the formation's right.

use acr_formats::defs::{OrderGroups, OrderList, OrderShape};

use crate::fixed::Pos;
use crate::motion::send_to;
use crate::trig::{cos, get_dir, norma, sin};
use crate::world::{EntityId, World};

/// Priority of formation moves (`AddKeepPositions` uses 0x90 when forming).
const FORM_PRIO: u8 = 0x90;
/// Keep-positions tolerance, `Norma` in 1/16 px (0x41c0a9).
const KEEP_TOLERANCE: i32 = 100;

/// A compiled shape (`OrdDesc`, 62 bytes at 0x23a95a0).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shape {
    pub name: String,
    /// Soldier x per text line (half cells, centred: `2·col − avgX`); empty lines kept.
    pub lines: Vec<Vec<i32>>,
    /// `sum(2·row) / N` over the soldier slots.
    pub avg_y: i32,
    /// Commander slots, centred `(2·col − avgX, 2·row − avgY)`.
    pub commanders: Vec<(i32, i32)>,
    /// Header numbers: attack/defence bonus standing, normal, with flag bearer.
    pub bonus: [i32; 6],
    /// Soldier slot permutations for turning by +90°, −90° (SYM4) and 180° (SYM2, SYM4).
    pub perm_left: Option<Vec<usize>>,
    pub perm_right: Option<Vec<usize>>,
    pub perm_back: Option<Vec<usize>>,
    /// `#KARE*`: the defence bonus applies from every side.
    pub kare: bool,
}

impl Shape {
    pub fn compile(s: &OrderShape) -> Option<Self> {
        let raw: Vec<Vec<i32>> = s
            .rows
            .iter()
            .map(|r| r.iter().enumerate().filter(|(_, c)| **c == b'*').map(|(i, _)| 2 * i as i32).collect())
            .collect();
        let n: i32 = raw.iter().map(|l| l.len() as i32).sum();
        if n == 0 {
            return None;
        }
        let avg_x = raw.iter().flatten().sum::<i32>() / n;
        let avg_y = raw.iter().enumerate().map(|(r, l)| 2 * r as i32 * l.len() as i32).sum::<i32>() / n;
        let commanders = s.commanders().iter().map(|&(c, r)| (2 * c - avg_x, 2 * r - avg_y)).collect();
        let lines: Vec<Vec<i32>> = raw.iter().map(|l| l.iter().map(|x| x - avg_x).collect()).collect();
        let mut bonus = [0; 6];
        for (b, p) in bonus.iter_mut().zip(&s.params) {
            *b = i32::from(*p as i8);
        }
        let mut shape = Shape {
            name: s.name.clone(),
            lines,
            avg_y,
            commanders,
            bonus,
            perm_left: None,
            perm_right: None,
            perm_back: None,
            kare: s.name.to_ascii_uppercase().contains("#KARE"),
        };
        if s.symmetry == 2 || s.symmetry == 4 {
            shape.perm_back = shape.build_perm_back();
        }
        if s.symmetry == 4
            && let Some((l, r)) = shape.build_perm_90()
        {
            shape.perm_left = Some(l);
            shape.perm_right = Some(r);
        }
        Some(shape)
    }

    /// Number of soldier slots.
    pub fn slots(&self) -> usize {
        self.lines.iter().map(Vec::len).sum()
    }

    /// `(line, x)` of every soldier slot in slot order.
    fn slot_cells(&self) -> Vec<(i32, i32)> {
        self.lines.iter().enumerate().flat_map(|(r, l)| l.iter().map(move |&x| (r as i32, x))).collect()
    }

    /// 180° permutation (0x4edbaa): the k-th slot from the top of a column maps to the k-th
    /// from the bottom of the mirrored column.
    fn build_perm_back(&self) -> Option<Vec<usize>> {
        let cells = self.slot_cells();
        let xmin = cells.iter().map(|c| c.1).min()?;
        let xmax = cells.iter().map(|c| c.1).max()?;
        let w = xmax - xmin;
        cells
            .iter()
            .map(|&(r, x)| {
                let cnt = cells.iter().filter(|&&(r2, x2)| x2 == x && r2 < r).count();
                let mirror = w - (x - xmin) + xmin;
                let mut col: Vec<(usize, i32)> =
                    cells.iter().enumerate().filter(|(_, c)| c.1 == mirror).map(|(k, c)| (k, c.0)).collect();
                col.sort_by_key(|c| std::cmp::Reverse(c.1));
                col.get(cnt).map(|c| c.0)
            })
            .collect()
    }

    /// ±90° permutations (0x4ee10f): the shape must be as many characters wide as lines high.
    fn build_perm_90(&self) -> Option<(Vec<usize>, Vec<usize>)> {
        let cells = self.slot_cells();
        let xmin = cells.iter().map(|c| c.1).min()?;
        let xmax = cells.iter().map(|c| c.1).max()?;
        let rmin = cells.iter().map(|c| c.0).min()?;
        let rmax = cells.iter().map(|c| c.0).max()?;
        let h = (rmax - rmin + 1) as usize;
        let hw = ((xmax - xmin + 2) >> 1) as usize;
        if hw != h {
            return None;
        }
        let mut grid = vec![vec![None; hw]; h];
        for (k, &(r, x)) in cells.iter().enumerate() {
            grid[(r - rmin) as usize][((x - xmin) >> 1) as usize] = Some(k);
        }
        let mut left = Vec::with_capacity(cells.len());
        let mut right = Vec::with_capacity(cells.len());
        for &(r, x) in &cells {
            let (rr, ci) = ((r - rmin) as usize, ((x - xmin) >> 1) as usize);
            grid[ci][rr]?;
            grid[h - 1 - rr][hw - 1 - ci]?;
            left.push(grid[hw - 1 - ci][rr]?);
            right.push(grid[ci][h - 1 - rr]?);
        }
        Some((left, right))
    }

    /// Slot positions (1/16 px) for a formation centred at `(x0, y0)` facing `dir`
    /// (`Arrange`, 0x4ad9af). Diagonal facings come out about 6 % smaller, as in the engine.
    pub fn positions(&self, x0: i32, y0: i32, dir: u8) -> Vec<(i32, i32)> {
        let (c, s) = (cos(dir), sin(dir));
        let l = norma(c, s);
        let (cx, sy) = (c * 0x800 / l, s * 0x800 / l);
        self.slot_cells()
            .into_iter()
            .map(|(r, xi)| {
                let x = x0 - ((xi * sy + (self.avg_y - 2 * r) * cx) >> 3);
                let y = y0 + ((xi * cx + (2 * r - self.avg_y) * sy) >> 3);
                (x, y)
            })
            .collect()
    }

    /// Commander slot `i` (raw `TCos/TSin`, as `PickCom` 0x41a65d).
    pub fn commander_position(&self, i: usize, x0: i32, y0: i32, dir: u8) -> (i32, i32) {
        let (c, s) = (cos(dir), sin(dir));
        let (cx, cy) = self.commanders[i];
        (x0 + (-cx * s + cy * c), y0 + (cy * s + cx * c))
    }
}

/// One line of `ORD_GROUPS.LST`: a formation and its size variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Group {
    pub name: String,
    /// Shape indices, smallest first.
    pub variants: Vec<usize>,
}

/// All formation shapes and groups of a match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Formations {
    pub shapes: Vec<Shape>,
    pub groups: Vec<Group>,
}

impl Formations {
    pub fn new(orders: &OrderList, groups: &OrderGroups) -> Self {
        let shapes: Vec<Shape> = orders.shapes.iter().filter_map(Shape::compile).collect();
        let find = |n: &str| shapes.iter().position(|s| s.name.eq_ignore_ascii_case(n));
        let groups = groups
            .groups
            .iter()
            .map(|g| Group { name: g.name.clone(), variants: g.shapes.iter().filter_map(|s| find(s)).collect() })
            .collect();
        Self { shapes, groups }
    }

    pub fn shape_index(&self, name: &str) -> Option<usize> {
        let name = name.strip_prefix('#').unwrap_or(name);
        self.shapes.iter().position(|s| s.name[1..].eq_ignore_ascii_case(name))
    }

    /// Shape for `count` soldiers: a group name picks its first variant with enough slots
    /// (`ReformBrigade`, 0x4e1cd3); a shape name is used as is.
    pub fn choose(&self, kind: &str, count: usize) -> Option<usize> {
        if let Some(g) = self.groups.iter().find(|g| g.name.eq_ignore_ascii_case(kind)) {
            return g.variants.iter().copied().find(|&v| count <= self.shapes[v].slots());
        }
        self.shape_index(kind)
    }
}

/// A formation in the world (`Brigade`, 0x4F bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Brigade {
    pub owner: u8,
    pub alive: bool,
    pub shape: usize,
    pub dir: u8,
    /// `[officer, drummer, flag bearer, soldier slots...]`.
    pub members: Vec<Option<EntityId>>,
    /// Target position of each member in world pixels.
    pub slots: Vec<(i32, i32)>,
    /// Keep-positions order active.
    pub keep: bool,
}

fn alive(world: &World, id: Option<EntityId>) -> Option<usize> {
    id.map(|e| e.0 as usize).filter(|&i| world.units.get(i).is_some_and(|u| u.alive))
}

/// Centre of a brigade's living soldiers (1/16 px).
pub fn center(world: &World, b: usize) -> Option<(i32, i32)> {
    let br = world.brigades.get(b)?;
    let soldiers: Vec<usize> = br.members.iter().skip(3).filter_map(|&m| alive(world, m)).collect();
    if soldiers.is_empty() {
        return None;
    }
    let n = soldiers.len() as i64;
    let x = soldiers.iter().map(|&i| i64::from(world.units[i].pos.x)).sum::<i64>() / n;
    let y = soldiers.iter().map(|&i| i64::from(world.units[i].pos.y)).sum::<i64>() / n;
    Some((x as i32, y as i32))
}

/// Places the brigade at `(x0, y0)` facing `dir`. With `sort`, soldiers are assigned to
/// slots by their projection on the facing (rearmost to the first line) and then
/// left-to-right (`Arrange`); otherwise they keep their slots (`Arrange2`, 0x4adc6a).
fn place(world: &mut World, b: usize, x0: i32, y0: i32, dir: u8, sort: bool) {
    let shape = world.formations.shapes[world.brigades[b].shape].clone();
    let n = shape.slots();
    let positions = shape.positions(x0, y0, dir);
    let br = &world.brigades[b];
    let mut members = br.members.clone();
    members.resize(n + 3, None);
    if sort || members[3..].iter().any(Option::is_none) {
        let mut soldiers: Vec<EntityId> =
            members[3..].iter().filter_map(|&m| alive(world, m)).map(|i| EntityId(i as u32)).collect();
        let (c, s) = (cos(dir), sin(dir));
        let key = |e: &EntityId, a: i32, bb: i32| {
            let p = world.units[e.0 as usize].pos;
            i64::from(p.x >> 5) * i64::from(a >> 4) + i64::from(p.y >> 5) * i64::from(bb >> 4)
        };
        soldiers.sort_by_key(|e| key(e, c, s));
        let mut done = 0;
        for line in &shape.lines {
            if line.is_empty() || done >= soldiers.len() {
                continue;
            }
            let k = line.len().min(soldiers.len() - done);
            soldiers[done..done + k].sort_by_key(|e| key(e, -s, c));
            done += k;
        }
        for (i, slot) in members[3..].iter_mut().enumerate() {
            *slot = soldiers.get(i).copied();
        }
    }
    let mut slots = vec![(x0 >> 4, y0 >> 4); n + 3];
    for (i, &(x, y)) in positions.iter().enumerate() {
        slots[3 + i] = (x >> 4, y >> 4);
    }
    // Commanders take the nearest free C slot, officer first (PickCom).
    let mut taken: Vec<usize> = Vec::new();
    for (m, slot) in slots.iter_mut().enumerate().take(3) {
        let Some(u) = alive(world, members[m]) else { continue };
        let p = world.units[u].pos;
        let mut best = (100_000_000, None);
        for c in 0..shape.commanders.len() {
            if taken.contains(&c) {
                continue;
            }
            let (x, y) = shape.commander_position(c, x0, y0, dir);
            let d = norma(p.x - x, p.y - y);
            if d < best.0 {
                best = (d, Some((c, x, y)));
            }
        }
        if let Some((c, x, y)) = best.1 {
            taken.push(c);
            *slot = (x >> 4, y >> 4);
        }
    }
    let br = &mut world.brigades[b];
    br.members = members;
    br.slots = slots;
    br.dir = dir;
}

/// Sends every living member to its slot (fine-level `SendTo`) and starts keeping positions.
fn send_members(world: &mut World, b: usize, prio: u8) {
    let br = world.brigades[b].clone();
    for (m, &slot) in br.members.iter().zip(&br.slots) {
        if let Some(i) = alive(world, *m) {
            send_to(&mut world.units[i], Pos::from_pixels(slot.0, slot.1), prio, true);
        }
    }
    world.brigades[b].keep = true;
}

/// Forms a brigade under `officer` (unit index) from `units`: drummers among them take the
/// drummer slot, the soldiers are the units of the first soldier's type.
pub fn form(world: &mut World, officer: usize, units: &[usize], kind: &str) {
    if world.units[officer].brigade.is_some() {
        return;
    }
    let catalog = world.catalog.clone();
    let mut drummer = None;
    let mut soldiers = Vec::new();
    for &i in units {
        let u = &world.units[i];
        let t = catalog.get(u.kind);
        if i == officer || u.brigade.is_some() || u.building || t.officer {
            continue;
        }
        if t.drummer {
            drummer = drummer.or(Some(i));
        } else if soldiers.first().is_none_or(|&f: &usize| world.units[f].kind == u.kind) {
            soldiers.push(i);
        }
    }
    let Some(shape) = world.formations.choose(kind, soldiers.len()) else { return };
    soldiers.truncate(world.formations.shapes[shape].slots());
    if soldiers.is_empty() {
        return;
    }
    let id = world.brigades.len() as u16;
    let mut members = vec![Some(world.units[officer].id), drummer.map(|d| world.units[d].id), None];
    members.extend(soldiers.iter().map(|&s| Some(world.units[s].id)));
    for m in members.iter().flatten() {
        world.units[m.0 as usize].brigade = Some(id);
    }
    world.brigades.push(Brigade {
        owner: world.units[officer].owner,
        alive: true,
        shape,
        dir: 0,
        members,
        slots: Vec::new(),
        keep: false,
    });
    let p = world.units[officer].pos;
    place(world, usize::from(id), p.x, p.y, 0, true);
    send_members(world, usize::from(id), FORM_PRIO);
}

/// The brigade move handler (0x41b83a): re-places the formation at the target (turning by
/// slot permutations so soldiers do not cross) and sends every member to its own slot.
pub fn send(world: &mut World, b: usize, x: i32, y: i32, dir: Option<u8>) {
    let Some((cx, cy)) = center(world, b) else { return };
    if norma(x - cx, y - cy) < 0x100 {
        return; // also drops pure turn orders, like the engine
    }
    let nd = dir.unwrap_or_else(|| get_dir((x - cx) >> 4, (y - cy) >> 4));
    turn_and_place(world, b, x, y, nd);
    send_members(world, b, crate::motion::PLAYER_PRIO);
}

fn turn_and_place(world: &mut World, b: usize, x: i32, y: i32, nd: u8) {
    let br = &world.brigades[b];
    let shape = &world.formations.shapes[br.shape];
    let dd = nd.wrapping_sub(br.dir) as i8;
    let mut new_dir = nd;
    let perm = if dd.unsigned_abs() < 32 {
        None
    } else if shape.perm_left.is_some() {
        if (32..96).contains(&dd) {
            shape.perm_left.clone()
        } else if (-95..=-32).contains(&dd) {
            shape.perm_right.clone()
        } else {
            shape.perm_back.clone()
        }
    } else if shape.perm_back.is_some() {
        if dd.unsigned_abs() > 64 { shape.perm_back.clone() } else { None }
    } else {
        // No symmetry: the engine turns by only 32 per order.
        new_dir = if dd > 0 { br.dir.wrapping_add(32) } else { br.dir.wrapping_sub(32) };
        None
    };
    if let Some(p) = perm {
        let old = world.brigades[b].members.clone();
        let br = &mut world.brigades[b];
        for (i, &src) in p.iter().enumerate() {
            br.members[3 + i] = old.get(3 + src).copied().flatten();
        }
    }
    place(world, b, x, y, new_dir, false);
}

/// Turns a brigade in place (the turning variant 0x41b44a re-places at the current centre).
pub fn face(world: &mut World, officer: usize, dir: u8) {
    let Some(b) = world.units[officer].brigade.map(usize::from) else { return };
    let Some((cx, cy)) = center(world, b) else { return };
    turn_and_place(world, b, cx, cy, dir);
    send_members(world, b, crate::motion::PLAYER_PRIO);
}

pub fn disband(world: &mut World, officer: usize) {
    let Some(b) = world.units[officer].brigade.map(usize::from) else { return };
    let members = std::mem::take(&mut world.brigades[b].members);
    for m in members.into_iter().flatten() {
        world.units[m.0 as usize].brigade = None;
    }
    world.brigades[b].alive = false;
    world.brigades[b].keep = false;
}

/// Keep-positions handler (0x41c0a9), once per tick before the units move: idle members
/// more than 100/16 px off their slot are sent back; members at their slot turn to the
/// brigade's facing. The order ends when everybody is in place and facing.
pub fn run_brigades(world: &mut World) {
    let catalog = world.catalog.clone();
    for b in 0..world.brigades.len() {
        let br = &world.brigades[b];
        if !br.alive || !br.keep {
            continue;
        }
        let (dir, members, slots) = (br.dir, br.members.clone(), br.slots.clone());
        let mut settled = true;
        for (m, &slot) in members.iter().zip(&slots) {
            let Some(i) = alive(world, *m) else { continue };
            let u = &mut world.units[i];
            let target = Pos::from_pixels(slot.0, slot.1);
            if norma(target.x - u.pos.x, target.y - u.pos.y) > KEEP_TOLERANCE {
                settled = false;
                if u.orders.is_empty() && !u.in_motion {
                    send_to(u, target, crate::motion::PLAYER_PRIO, true);
                }
                continue;
            }
            let rot = catalog.get(u.kind).rotate;
            let dd = dir.wrapping_sub(u.dir) as i8;
            if i32::from(dd).abs() > rot {
                settled = false;
                if u.orders.is_empty() && !u.in_motion {
                    u.dir = if dd > 0 { u.dir.wrapping_add(rot as u8) } else { u.dir.wrapping_sub(rot as u8) };
                }
            } else if u.orders.is_empty() && !u.in_motion {
                u.dir = dir;
            }
        }
        if settled {
            world.brigades[b].keep = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes() -> OrderList {
        OrderList::parse(
            b"#LINE5 SYM2 1 2 3 4 5 6\r\n  C C\r\n\r\n*****\r\n#END\r\n#SQ4 SYM4 0 0 0 0\r\n**\r\n**\r\n#END\r\n#EXIT\r\n",
        )
    }

    #[test]
    fn compiles_line_shape() {
        let s = Shape::compile(&shapes().shapes[0]).unwrap();
        assert_eq!(s.slots(), 5);
        assert_eq!(s.lines[2], [-4, -2, 0, 2, 4]);
        assert_eq!(s.avg_y, 4);
        assert_eq!(s.commanders, [(0, -4), (4, -4)]);
        assert_eq!(s.perm_back, Some(vec![4, 3, 2, 1, 0]));
        assert_eq!(s.bonus, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn square_has_quarter_turns() {
        let s = Shape::compile(&shapes().shapes[1]).unwrap();
        assert!(s.perm_left.is_some() && s.perm_right.is_some());
        assert_eq!(s.perm_back, Some(vec![3, 2, 1, 0]));
    }

    #[test]
    fn positions_are_32_px_apart_facing_east() {
        let s = Shape::compile(&shapes().shapes[0]).unwrap();
        let p = s.positions(0, 0, 0);
        // Facing east (dir 0): the line runs north-south, 32 px (512 sub-pixels) apart.
        assert_eq!(p, [(0, -1024), (0, -512), (0, 0), (0, 512), (0, 1024)]);
        let (cx, cy) = s.commander_position(0, 0, 0, 0);
        assert_eq!((cx, cy), (-1024, 0), "commanders stand behind the line");
    }
}
