//! Formations (brigades): an officer, an optional drummer and flag bearer, and soldiers in
//! the slots of an `ORDERS.LST` shape.
//!
//! Recovered from `LoadOrders` (0x4ed6e0), `PlaceBrigade` (0x41a7be), `Arrange` (0x4ad9af),
//! `PickCom` (0x41a65d), the brigade move handler (0x41b83a) and `AddKeepPositions`
//! (0x41c63e); see `docs/simulation.md`. A shape cell is 32 px in both directions; text
//! lines further down stand further in front, text columns go to the formation's right.

use std::collections::BTreeMap;

use acr_formats::defs::{NationDef, OrderGroups, OrderList, OrderShape};

use crate::catalog::{Catalog, TypeId};
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
    /// The six header numbers `n1..n6` (`%s%s%d%d%d%d%d%d`, 0x4ee7cd; missing ones are 0),
    /// stored at shape+0x36, +0x38, +0x37, +0x39, +0x3a, +0x3b: attack and defence bonus
    /// standing (`n1`, `n3`), normal (`n2`, `n4`) and the flag bearer's extra (`n5`, `n6`).
    pub bonus: [i32; 6],
    /// The `ORD_GROUPS.LST` group listing the shape (shape+0x3c, 0x4ed6c4; the last one).
    pub group: Option<usize>,
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
            group: None,
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

/// One line of `ORD_GROUPS.LST` (table 0x23abb6c, 20 bytes each): a formation, the groups a
/// formed brigade of it can turn into, and its size variants. The groups are listed in the
/// order of `NRES.DAT [ORDERICONS]`, whose line `i` is group `i`'s icon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Group {
    pub name: String,
    /// The re-forming buttons of a brigade of this group (+8): group indices.
    pub buttons: Vec<usize>,
    /// Shape indices, smallest first (+0x10).
    pub variants: Vec<usize>,
}

/// One formation of an officer (`[OFFICERS]` of the nation files, 17 bytes at od+6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OfficerFormation {
    /// The `ORD_GROUPS.LST` group (+0), which is also its `[ORDERICONS]` line.
    pub group: usize,
    /// The size variants (+7), as listed; their slot counts are the sizes (+3).
    pub shapes: Vec<usize>,
    /// The soldier types it takes (+0xd).
    pub kinds: Vec<TypeId>,
}

/// What an officer type can lead (`NM+0xa`): the drummer it needs (`NONE`: none), its flag
/// bearer and its formations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Officer {
    pub drummer: Option<TypeId>,
    pub flag: Option<TypeId>,
    pub formations: Vec<OfficerFormation>,
}

/// All formation shapes and groups of a match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Formations {
    pub shapes: Vec<Shape>,
    pub groups: Vec<Group>,
    /// Officer types and what they lead.
    pub officers: BTreeMap<TypeId, Officer>,
}

impl Formations {
    pub fn new(orders: &OrderList, groups: &OrderGroups) -> Self {
        let mut shapes: Vec<Shape> = orders.shapes.iter().filter_map(Shape::compile).collect();
        let find = |shapes: &[Shape], n: &str| shapes.iter().position(|s| s.name.eq_ignore_ascii_case(n));
        let groups: Vec<Group> = groups
            .groups
            .iter()
            .map(|g| Group {
                name: g.name.clone(),
                buttons: g.values.iter().filter_map(|&v| usize::try_from(v).ok()).collect(),
                variants: g.shapes.iter().filter_map(|s| find(&shapes, s)).collect(),
            })
            .collect();
        for (i, g) in groups.iter().enumerate() {
            for &v in &g.variants {
                shapes[v].group = Some(i);
            }
        }
        Self { shapes, groups, officers: BTreeMap::new() }
    }

    /// Reads the `[OFFICERS]` sections: `OFFICER DRUMMER [FLAG] n`, then per formation
    /// `GROUP m #SHAPE*m k TYPE*k`. The first nation naming an officer type wins.
    pub fn set_officers(&mut self, catalog: &Catalog, nations: &[NationDef]) {
        for g in nations.iter().flat_map(|n| &n.officers) {
            let kind = |i: usize| g.commanders.get(i).and_then(|c| catalog.find(c));
            let Some(officer) = kind(0) else { continue };
            if self.officers.contains_key(&officer) {
                continue;
            }
            let formations = g
                .formations
                .iter()
                .filter_map(|f| {
                    Some(OfficerFormation {
                        group: self.groups.iter().position(|x| x.name.eq_ignore_ascii_case(&f.name))?,
                        shapes: f.variants.iter().filter_map(|v| self.shape_index(v)).collect(),
                        kinds: f.units.iter().filter_map(|u| catalog.find(u)).collect(),
                    })
                })
                .collect();
            self.officers.insert(officer, Officer { drummer: kind(1), flag: kind(2), formations });
        }
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

/// A brigade order (`BOrder`, stack at Brigade+0x43); the last one runs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BOrder {
    /// `KeepPositions` (0x41c0a9): members return to their slots and face the brigade's way.
    Keep,
    /// Formation melee (`Bitva` 0x41ee89, link 0x41e50c).
    Bitva(Box<Bitva>),
    /// The AI landing of a transport (`MakeDiversion` 0x425654, link 0x424957; see
    /// `desant`).
    Diversion(Box<crate::desant::Diversion>),
}

impl BOrder {
    /// The order's priority byte (+1): 0 for both kinds here.
    pub fn prio(&self) -> u8 {
        0
    }
}

/// The state of a formation melee: the enemies found near the brigade.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bitva {
    /// Pathfinding area of the first member (+0x10), 0xFFFF = none.
    pub zone: u16,
    /// Search box in 128 px cells (+0xe16..): `x0, y0, x1, y1`.
    pub bbox: (i32, i32, i32, i32),
    /// Listed enemies (+0x412), at most 512.
    pub enemies: Vec<EntityId>,
    /// Enemies ever listed (bitmap +0x12, never cleared).
    pub seen: Vec<u64>,
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
    /// Brigade orders (keep positions, formation melee), the last one runs.
    pub orders: Vec<BOrder>,
    /// `AttEnm` (+0x2e bit4): set by attack-moves; a marching close-combat formation then
    /// starts melee against enemies within 100 px.
    pub att_enm: bool,
    /// A shooting formation (+0x2e bit3, `FORMLIKESHOOTERS`): members attack one by one.
    pub shooters: bool,
    /// The soldier type (+0x1a), which filling takes.
    pub kind: Option<TypeId>,
    /// The members' current attack and defence bonus (+0x1c, +0x1d).
    pub bonus: (i32, i32),
    /// Stand-still countdown (+0x22): set to 50 when the formation stands ground; it runs
    /// down by `FrmDec` per city tick while fewer than 3 members have orders, and at 0 the
    /// standing bonuses apply (0x434486).
    pub stand_timer: u16,
    /// Tick of the last stand-ground switch (+0x26): it switches at most once per tick.
    pub stamp: u32,
    /// Index in the owner's brigade list (`Brigade+0x20`, the slot `GetFreeBrigade` 0x42c845
    /// handed out): the interface shows it + 1 as the formation's number.
    pub number: u16,
}

/// The first index of `owner`'s brigade list no living brigade holds (`GetFreeBrigade`
/// 0x42c845 takes the first slot whose `Enabled` bit, +0x2e bit 0, is clear).
pub fn free_number(world: &World, owner: u8) -> u16 {
    let used: Vec<u16> = world.brigades.iter().filter(|b| b.alive && b.owner == owner).map(|b| b.number).collect();
    (0..).find(|n| !used.contains(n)).unwrap_or(0)
}

/// Attack and defence bonus of brigade `b`'s shape (`n2`, `n4`, or standing `n1`, `n3`), plus
/// the flag bearer's extra (`n5`, `n6`) while it has one (0x419d57, 0x4dfd35, 0x4e0000).
fn shape_bonus(world: &World, b: usize, standing: bool) -> (i32, i32) {
    let br = &world.brigades[b];
    let Some(s) = world.formations.shapes.get(br.shape) else { return (0, 0) };
    let (mut a, mut d) = if standing { (s.bonus[0], s.bonus[2]) } else { (s.bonus[1], s.bonus[3]) };
    if br.members.get(2).is_some_and(Option::is_some) {
        (a, d) = (a + s.bonus[4], d + s.bonus[5]);
    }
    (a, d)
}

/// The living members of brigade `b` (alive and still in it), with their member index.
fn members_at(world: &World, b: usize) -> Vec<(usize, usize)> {
    world.brigades[b]
        .members
        .iter()
        .enumerate()
        .filter_map(|(k, &m)| Some((k, alive(world, m)?)))
        .filter(|&(_, i)| world.units[i].brigade == Some(b as u16))
        .collect()
}

/// Sets the bonuses of brigade `b` and of all its members, and their stand-ground flag.
fn set_bonus(world: &mut World, b: usize, bonus: (i32, i32), stand: bool) {
    world.brigades[b].bonus = bonus;
    for (_, i) in members_at(world, b) {
        let u = &mut world.units[i];
        (u.attack_bonus, u.defence_bonus, u.stand_ground) = (bonus.0, bonus.1, stand);
    }
}

/// `MakeStandGroundTemp` (0x4dfe94), once per tick: unless a soldier stands already, every
/// member stands with the normal bonuses and the stand-still countdown starts at 50; a
/// `#KARE` formation's members then clear their orders and face away from its centre.
pub fn make_stand_ground(world: &mut World, b: usize) {
    let now = world.tick.0 as u32;
    if world.brigades[b].stamp == now {
        return;
    }
    let members = members_at(world, b);
    if !members.iter().any(|&(k, i)| k > 2 && world.units[i].stand_ground) {
        let bonus = shape_bonus(world, b, false);
        set_bonus(world, b, bonus, true);
        world.brigades[b].stand_timer = 50;
    }
    let units: Vec<usize> = members.iter().map(|&(_, i)| i).collect();
    crate::states::kare_face_outward(world, b, &units);
    world.brigades[b].stamp = now;
}

/// `CancelStandGround` (0x4e0c3f; 0x4e0ad7 once per tick with `once`): every member stops
/// standing with the normal bonuses, and the countdown ends.
pub fn cancel_stand_ground(world: &mut World, b: usize, once: bool) {
    let now = world.tick.0 as u32;
    if once {
        if world.brigades[b].stamp == now {
            return;
        }
        world.brigades[b].stamp = now;
    }
    let bonus = shape_bonus(world, b, false);
    set_bonus(world, b, bonus, false);
    world.brigades[b].stand_timer = 0;
}

/// 0x4dfd35, once per tick: the standing bonuses (`n1`, `n3`) for every member, which stands.
fn standing_bonus(world: &mut World, b: usize) {
    let now = world.tick.0 as u32;
    if world.brigades[b].stamp == now {
        return;
    }
    let bonus = shape_bonus(world, b, true);
    set_bonus(world, b, bonus, true);
    world.brigades[b].stamp = now;
}

fn alive(world: &World, id: Option<EntityId>) -> Option<usize> {
    id.map(|e| e.0 as usize).filter(|&i| world.units.get(i).is_some_and(|u| u.living()))
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
    world.brigades[b].orders = vec![BOrder::Keep];
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
    let flag_kind = world.formations.officers.get(&world.units[officer].kind).and_then(|o| o.flag);
    let flag = units.iter().copied().find(|&i| {
        let u = &world.units[i];
        i != officer && u.brigade.is_none() && Some(u.kind) == flag_kind && u.living()
    });
    create(world, officer, drummer, flag, &soldiers, shape);
}

/// Sets up a brigade of `shape`: the officer, drummer and flag bearer stand ground, the
/// soldiers do not; everybody gets the normal bonuses (0x419d57); the formation is placed
/// centred on the officer facing 0 and its members walk to their slots with priority 0x90
/// (`PlaceBrigade` 0x41a7be, `AddKeepPositions` 0x41c63e).
fn create(
    world: &mut World,
    officer: usize,
    drummer: Option<usize>,
    flag: Option<usize>,
    soldiers: &[usize],
    shape: usize,
) {
    let id = world.brigades.len() as u16;
    let unit_id = |i: Option<usize>| i.map(|i| world.units[i].id);
    let mut members = vec![unit_id(Some(officer)), unit_id(drummer), unit_id(flag)];
    members.extend(soldiers.iter().map(|&s| Some(world.units[s].id)));
    let kind = world.units[soldiers[0]].kind;
    let owner = world.units[officer].owner;
    let number = free_number(world, owner);
    world.brigades.push(Brigade {
        owner,
        alive: true,
        shape,
        dir: 0,
        members,
        slots: Vec::new(),
        orders: Vec::new(),
        att_enm: false,
        shooters: world.catalog.get(kind).shooters_formation,
        kind: Some(kind),
        bonus: (0, 0),
        stand_timer: 0,
        stamp: 0,
        number,
    });
    let b = usize::from(id);
    let bonus = shape_bonus(world, b, false);
    world.brigades[b].bonus = bonus;
    for (k, m) in world.brigades[b].members.clone().into_iter().enumerate() {
        let Some(m) = m else { continue };
        let u = &mut world.units[m.0 as usize];
        u.brigade = Some(id);
        (u.attack_bonus, u.defence_bonus) = bonus;
        u.stand_ground = k < 3;
    }
    let p = world.units[officer].pos;
    place(world, b, p.x, p.y, 0, true);
    send_members(world, b, FORM_PRIO);
}

/// Free units of `kind` owned by `owner` within `r` (1/16 px) of `(x, y)`, at most `max`, in
/// the order of the 128 px cells ring by ring around the point (0x4191a3 with 0x41901d;
/// `count_near`, 0x52c9d8, counts them): living, not inside anything and in no formation.
pub fn near(world: &World, x: i32, y: i32, r: i32, kind: TypeId, owner: u8, max: usize) -> Vec<usize> {
    let mut out = Vec::new();
    if max == 0 {
        return out;
    }
    let (cx, cy) = (x >> 11, y >> 11);
    for ring in 0..=(r >> 11) as usize + 1 {
        for &(dx, dy) in crate::morale::ring(ring) {
            for &k in world.buckets.get(cx + dx, cy + dy) {
                let u = &world.units[k as usize];
                if u.active()
                    && u.brigade.is_none()
                    && u.owner == owner
                    && u.kind == kind
                    && norma(u.pos.x - x, u.pos.y - y) < r
                {
                    out.push(k as usize);
                    if out.len() >= max {
                        return out;
                    }
                }
            }
        }
    }
    out
}

/// How many free units of `kind` stand within 1024 px of `(x, y)` (0x52c9d8).
pub fn count_near(world: &World, x: i32, y: i32, kind: TypeId, owner: u8) -> usize {
    near(world, x, y, NEAR, kind, owner, usize::MAX).len()
}

/// The search radius of forming and filling, 1024 px (0x4000).
pub const NEAR: i32 = 0x4000;

/// `CreateBrigade` (0x4198b3) for network command 0x3a (0x4e174a): officer `officer` (free and
/// alive) forms `shape` from exactly its slot count of free soldiers of `kind` within
/// 1024 px. Its drummer type, unless `NONE`, must have a free unit within 1024 px too; a
/// flag bearer is taken if one is near. The interface then selects the brigade (0x41f198).
/// Returns the brigade.
pub fn form_near(world: &mut World, officer: usize, shape: usize, kind: TypeId) -> Option<usize> {
    let o = &world.units[officer];
    if o.brigade.is_some() || !o.living() {
        return None;
    }
    let data = world.formations.officers.get(&o.kind)?.clone();
    let (x, y, owner) = (o.pos.x, o.pos.y, o.owner);
    let n = world.formations.shapes.get(shape)?.slots();
    let soldiers = near(world, x, y, NEAR, kind, owner, n);
    if soldiers.len() != n {
        return None;
    }
    let drummer = match data.drummer {
        Some(d) => Some(*near(world, x, y, NEAR, d, owner, 1).first()?),
        None => None,
    };
    let flag = data.flag.and_then(|f| near(world, x, y, NEAR, f, owner, 1).first().copied());
    create(world, officer, drummer, flag, &soldiers, shape);
    Some(world.brigades.len() - 1)
}

/// `ReformBrigade` (0x4e1cd3, network command 0x41): the living soldiers of brigade `b` take
/// the first variant of `group` with room for all of them, in their order, placed at their
/// mean position with the brigade's facing; then they walk to their slots (priority 0x90)
/// and the formation stops standing ground with the new shape's bonuses (0x4e0ad7).
pub fn reform(world: &mut World, b: usize, group: usize) {
    let Some(br) = world.brigades.get(b).filter(|br| br.alive) else { return };
    let soldiers: Vec<EntityId> =
        members_at(world, b).into_iter().filter(|&(k, _)| k > 2).map(|(_, i)| world.units[i].id).collect();
    let Some(g) = world.formations.groups.get(group) else { return };
    let Some(shape) = g.variants.iter().copied().find(|&v| soldiers.len() <= world.formations.shapes[v].slots()) else {
        return;
    };
    if soldiers.is_empty() {
        return;
    }
    let n = soldiers.len() as i64;
    let x = (soldiers.iter().map(|e| i64::from(world.units[e.0 as usize].pos.x)).sum::<i64>() / n) as i32;
    let y = (soldiers.iter().map(|e| i64::from(world.units[e.0 as usize].pos.y)).sum::<i64>() / n) as i32;
    let dir = br.dir;
    let br = &mut world.brigades[b];
    br.members.truncate(3);
    br.members.extend(soldiers.into_iter().map(Some));
    br.shape = shape;
    place(world, b, x, y, dir, true);
    send_members(world, b, FORM_PRIO);
    cancel_stand_ground(world, b, true);
}

/// `FillBrigade` (0x4193a1, network command 0x52, the `FILLFORM` icon): free soldiers of the
/// brigade's type within 1024 px of its soldiers' mean position take the empty or dead slots,
/// with the brigade's bonuses; the formation stops standing ground (0x4e0c3f), is placed
/// there anew with its facing and its members walk to their slots (priority 0x90).
pub fn fill(world: &mut World, b: usize) {
    let Some(br) = world.brigades.get(b).filter(|br| br.alive) else { return };
    let (Some(kind), owner, dir) = (br.kind, br.owner, br.dir) else { return };
    let slots = world.formations.shapes[br.shape].slots() + 3;
    let living = members_at(world, b);
    let soldiers: Vec<usize> = living.iter().filter(|&&(k, _)| k > 2).map(|&(_, i)| i).collect();
    let empty: Vec<usize> = (3..slots).filter(|k| !living.iter().any(|&(m, _)| m == *k)).collect();
    if soldiers.is_empty() || empty.is_empty() {
        return;
    }
    let n = soldiers.len() as i64;
    let x = (soldiers.iter().map(|&i| i64::from(world.units[i].pos.x)).sum::<i64>() / n) as i32;
    let y = (soldiers.iter().map(|&i| i64::from(world.units[i].pos.y)).sum::<i64>() / n) as i32;
    let new = near(world, x, y, NEAR, kind, owner, empty.len());
    let bonus = world.brigades[b].bonus;
    world.brigades[b].members.resize(slots, None);
    for (&k, &i) in empty.iter().zip(&new) {
        world.brigades[b].members[k] = Some(world.units[i].id);
        let u = &mut world.units[i];
        u.brigade = Some(b as u16);
        (u.attack_bonus, u.defence_bonus) = bonus;
        u.stand_ground = false;
    }
    cancel_stand_ground(world, b, false);
    place(world, b, x, y, dir, true);
    send_members(world, b, FORM_PRIO);
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
    dissolve(world, b);
}

/// Releases every member of brigade `b` (`Disband` 0x41dc18).
pub fn dissolve(world: &mut World, b: usize) {
    let members = std::mem::take(&mut world.brigades[b].members);
    for m in members.into_iter().flatten() {
        let u = &mut world.units[m.0 as usize];
        u.brigade = None;
        u.attack_bonus = 0;
        u.defence_bonus = 0;
    }
    world.brigades[b].alive = false;
    world.brigades[b].orders.clear();
}

/// Takes a unit out of its brigade (the brigade part of `City::UnRegisterNewUnit` 0x42e2bd):
/// its member slot is emptied.
pub fn remove_member(world: &mut World, i: usize) {
    let Some(b) = world.units[i].brigade.map(usize::from) else { return };
    let id = world.units[i].id;
    if let Some(br) = world.brigades.get_mut(b) {
        for m in &mut br.members {
            if *m == Some(id) {
                *m = None;
            }
        }
    }
    let u = &mut world.units[i];
    u.brigade = None;
    u.attack_bonus = 0;
    u.defence_bonus = 0;
}

/// The brigades' orders (`ProcessBrigades` 0x434486 from `City::Process` 0x432582): they run
/// only on the owner's city tick, `tmtmt & 7 == owner` (never for nation 7), and every
/// brigade of that nation costs two random numbers around its order link.
pub fn run_brigades(world: &mut World) {
    let city = ((world.tick.0 + 1) & 7) as u8;
    if city == 7 {
        return;
    }
    for b in 0..world.brigades.len() {
        let br = &world.brigades[b];
        if !br.alive || br.owner != city {
            continue;
        }
        // The stand-still countdown: without a brigade order and while fewer than 3 members
        // have orders it runs down by `FrmDec`; at 0 the standing bonuses apply (0x4344d9).
        if br.stand_timer != 0 && br.orders.is_empty() {
            let busy = members_at(world, b).iter().filter(|&&(_, i)| !world.units[i].orders.is_empty()).count();
            let br = &mut world.brigades[b];
            if busy < 3 {
                br.stand_timer = br.stand_timer.saturating_sub(crate::world::FRM_DEC as u16);
            }
            if br.stand_timer == 0 {
                standing_bonus(world, b);
            }
        }
        let br = &world.brigades[b];
        world.random.rando();
        match br.orders.last() {
            Some(BOrder::Keep) => keep_positions(world, b),
            Some(BOrder::Bitva(_)) => bitva_link(world, b),
            Some(BOrder::Diversion(_)) => crate::desant::diversion_link(world, b),
            None => {}
        }
        world.random.rando();
    }
}

/// Keep-positions handler (0x41c0a9): idle members more than 100/16 px off their slot are
/// sent back; members at their slot turn to the brigade's facing. The order ends when
/// everybody is in place and facing.
fn keep_positions(world: &mut World, b: usize) {
    let catalog = world.catalog.clone();
    {
        let br = &world.brigades[b];
        let (dir, members, slots) = (br.dir, br.members.clone(), br.slots.clone());
        let mut settled = true;
        for (m, &slot) in members.iter().zip(&slots) {
            let Some(i) = alive(world, *m) else { continue };
            let u = &mut world.units[i];
            let target = Pos::from_pixels(slot.0, slot.1);
            // In place: within the tolerance, or in the slot's cell as far as a walk gets
            // (`STOPDISTANCE`), which the engine checks first.
            let (cx, cy) = crate::world::Unit::cell_of(target, u.lx);
            let near = (u.cell.0 - cx).abs().max((u.cell.1 - cy).abs()) <= catalog.get(u.kind).stop_distance >> 8;
            if norma(target.x - u.pos.x, target.y - u.pos.y) > KEEP_TOLERANCE && !near {
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
            world.brigades[b].orders.pop();
        }
    }
}

/// The valid members of a brigade (alive and still in it).
fn members_of(world: &World, b: usize) -> Vec<usize> {
    world.brigades[b]
        .members
        .iter()
        .filter_map(|&m| alive(world, m))
        .filter(|&i| world.units[i].brigade == Some(b as u16))
        .collect()
}

/// Search box (128 px cells, grown by 4 and clamped at 0) and area of a brigade's members.
fn bitva_box(world: &World, b: usize) -> Option<((i32, i32, i32, i32), u16)> {
    let m = members_of(world, b);
    let first = *m.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for &i in &m {
        let p = world.units[i].pos;
        (x0, y0, x1, y1) = (x0.min(p.x >> 11), y0.min(p.y >> 11), x1.max(p.x >> 11), y1.max(p.y >> 11));
    }
    let (px, py) = world.units[first].pos.pixels();
    let zone = world.map.land_areas.as_ref().map_or(0xFFFF, |g| g.area_at(px >> 6, py >> 6));
    Some((((x0 - 4).max(0), (y0 - 4).max(0), x1 + 4, y1 + 4), zone))
}

/// `AddCell` (0x41e173): lists the enemy land units of 128 px cell `(x, y)` not listed before,
/// unless the cell lies in no area or in one more than 30 away (path length) from the
/// brigade's.
fn add_cell(world: &World, bv: &mut Bitva, x: i32, y: i32, owner: u8) {
    if bv.enemies.len() >= 512 {
        return;
    }
    if let Some(g) = world.map.land_areas.as_ref().filter(|g| g.count > 0) {
        let zs = [
            g.area_at(2 * x, 2 * y),
            g.area_at(2 * x + 1, 2 * y),
            g.area_at(2 * x, 2 * y + 1),
            g.area_at(2 * x + 1, 2 * y + 1),
        ];
        if zs.iter().all(|&z| z >= 0xFFFE) {
            return;
        }
        let far = |z: u16| {
            z < 0xFFFE
                && z != bv.zone
                && usize::from(bv.zone) < g.count
                && g.dist.get(usize::from(bv.zone) * g.count + usize::from(z)).is_none_or(|&d| d > 30)
        };
        if zs.iter().any(|&z| far(z)) {
            return;
        }
    }
    let mask = 1u8 << (owner & 7);
    for &k in world.buckets.get(x, y) {
        let e = &world.units[k as usize];
        let (word, bit) = (k as usize >> 6, 1u64 << (k & 63));
        if e.nmask & mask != 0 || !e.living() || world.catalog.get(e.kind).medium != crate::catalog::Medium::Land {
            continue;
        }
        if bv.seen.len() <= word {
            bv.seen.resize(word + 1, 0);
        }
        if bv.seen[word] & bit != 0 {
            continue;
        }
        bv.seen[word] |= bit;
        bv.enemies.push(e.id);
        if bv.enemies.len() >= 512 {
            return;
        }
    }
}

/// `Bitva` (0x41ee89): starts formation melee. The brigade lists the enemies in its members'
/// box of 128 px cells grown by 4.
pub fn bitva(world: &mut World, b: usize) {
    if matches!(world.brigades[b].orders.last(), Some(BOrder::Bitva(_))) {
        return;
    }
    let Some((bbox, zone)) = bitva_box(world, b) else { return };
    let mut bv = Bitva { zone, bbox, enemies: Vec::new(), seen: Vec::new() };
    let owner = world.brigades[b].owner;
    for x in bbox.0..=bbox.2 {
        for y in bbox.1..=bbox.3 {
            add_cell(world, &mut bv, x, y, owner);
        }
    }
    world.brigades[b].orders.push(BOrder::Bitva(Box::new(bv)));
}

/// The melee link (0x41e50c), on the city tick: refreshes the box now and then, probes 64
/// random cells of it for new enemies, and gives every member without a target the nearest
/// listed enemy (preferably one in range) with priority 0x88. Members leave their slots to
/// fight; when nobody fights any more the melee ends and the order below resumes.
fn bitva_link(world: &mut World, b: usize) {
    let Some(BOrder::Bitva(mut bv)) = world.brigades[b].orders.pop() else { return };
    let owner = world.brigades[b].owner;
    if world.random.rando() < 0x400 || bv.zone == 0xFFFF {
        match bitva_box(world, b) {
            Some((bbox, zone)) => (bv.bbox, bv.zone) = (bbox, zone),
            None => return,
        }
    }
    if world.random.rando() < 0x400 {
        bv.enemies.retain(|e| world.units.get(e.0 as usize).is_some_and(|u| u.living()));
    }
    let (x0, y0, x1, y1) = bv.bbox;
    for _ in 0..64 {
        let x = x0 + ((world.random.rando() * (x1 - x0 + 1)) >> 15);
        let y = y0 + ((world.random.rando() * (y1 - y0 + 1)) >> 15);
        add_cell(world, &mut bv, x, y, owner);
    }
    let catalog = world.catalog.clone();
    let mut busy = false;
    for i in members_of(world, b) {
        let u = &world.units[i];
        let t = catalog.get(u.kind);
        if t.role.artillery {
            continue;
        }
        if !u.orders.is_empty() && u.target().is_some() {
            busy = true;
            continue;
        }
        let adv = world.adv(u.owner, u.kind);
        let (min_r, max_r) = crate::combat::range_bounds(&adv);
        if max_r == 0 {
            continue;
        }
        let kill = t.attacks.iter().fold(0u8, |m, a| m | a.mask);
        let (mut b1, mut b2, mut t1, mut t2) = (1_000_000, 1_000_000, None, None);
        for e in &bv.enemies {
            let Some(en) = world.units.get(e.0 as usize).filter(|en| en.living()) else { continue };
            if catalog.get(en.kind).materials & kill == 0 {
                continue;
            }
            let d = norma(u.pos.x - en.pos.x, u.pos.y - en.pos.y) >> 4;
            if d <= min_r {
                continue;
            }
            if d < b1 {
                let away = en.in_motion && {
                    let dd = en.dir.wrapping_sub(get_dir(en.pos.x - u.pos.x, en.pos.y - u.pos.y)) as i8;
                    i32::from(dd).abs() <= 64
                };
                if !away {
                    (b1, t1) = (d, Some(e.0 as usize));
                }
            }
            if d < max_r && d < b2 {
                (b2, t2) = (d, Some(e.0 as usize));
            }
        }
        if let Some(t) = t2.or(t1) {
            crate::combat::attack_obj(world, i, t, 0x88);
            busy = true;
        }
    }
    if busy {
        world.brigades[b].orders.push(BOrder::Bitva(bv));
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

    #[test]
    fn groups_and_officers() {
        use acr_formats::defs::UnitDef;

        use crate::catalog::ObjectType;
        let groups = OrderGroups::parse(
            b"2
LINE 2 0 1 1 #LINE5
SQ 1 0 1 #SQ4
",
        );
        let mut f = Formations::new(&shapes(), &groups);
        assert_eq!(f.groups[0].buttons, [0, 1]);
        assert_eq!(f.shapes[1].group, Some(1));
        let unit = UnitDef::parse(
            b"NAME U
[END]
",
        );
        let mut c = Catalog::new();
        let off = c.add(ObjectType::from_def("Off(EN)", &unit));
        let flag = c.add(ObjectType::from_def("Flag(EN)", &unit));
        let pik = c.add(ObjectType::from_def("Pik(EN)", &unit));
        let nds = b"[OFFICERS]
Off(EN) NONE Flag(EN) 1
SQ 1 #SQ4 1 Pik(EN)
[END]
";
        f.set_officers(&c, &[NationDef::parse(nds)]);
        let o = &f.officers[&off];
        assert_eq!((o.drummer, o.flag), (None, Some(flag)));
        assert_eq!(o.formations, [OfficerFormation { group: 1, shapes: vec![1], kinds: vec![pik] }]);
    }
}
