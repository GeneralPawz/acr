//! The enemy armies the scripts see through `EnumEnemyArmies` (`GlobalEnemyInfo` 0x6ba1b0,
//! `ResearchArmyDistribution` 0x4423d0). See `docs/simulation.md`.
//!
//! Every nation group (`EnemyInfo`, shared by allied nations) clusters the enemy units of a
//! few `USAGE`s into 256 px cells once every 256 ticks: cells with more than 3 units that
//! touch join one army, smaller cells next to such an army join it too, and every remaining
//! cell is an army of its own.

use crate::world::{PLAYERS, World};

/// `StrCod` (0x644454): the army category of a `USAGE`, 0xFF = not counted. The engine's
/// table has 26 entries and is read past its end for the usages from 26 on (the bytes of the
/// int table behind it).
const CATEGORY: [u8; 35] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0x05, 0xff, 0xff, 0x02, 0x04, 0x03, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0x00, 0x01, 0x02, 0x02, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
];

/// An army (`ArmyInfo`, 12 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Army {
    /// `MinX, MinY, MaxX, MaxY` in 256 px cells (bytes).
    pub bounds: [u8; 4],
    /// Sum of the units' `FORCE`.
    pub force: u16,
    pub count: u16,
    /// The mean cell's centre in pixels (`i16`).
    pub x: i16,
    pub y: i16,
}

/// The `EnemyInfo` groups and their armies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnemyArmies {
    /// `EINF[nat]` as the index of the nation that created it (`BasicNation`), `None` for
    /// none (always for nation 7); formed at the first tick.
    pub groups: Option<[Option<u8>; PLAYERS]>,
    /// The armies of each `EnemyInfo`, by its creator.
    pub armies: [Vec<Army>; PLAYERS],
}

impl EnemyArmies {
    /// The armies nation `nat` sees, `None` if it has no `EnemyInfo`.
    pub fn of(&self, nat: u8) -> Option<&[Army]> {
        let g = self.groups.as_ref()?.get(usize::from(nat))?.as_ref()?;
        Some(&self.armies[usize::from(*g)])
    }
}

/// `GlobalEnemyInfo::Setup` (0x427373): nations 0..6 with units get an `EnemyInfo`, shared by
/// every other such nation whose mask meets its creator's (later creators overwrite).
fn setup(w: &World) -> [Option<u8>; PLAYERS] {
    let mut has_units = [false; PLAYERS];
    for u in w.units.iter().filter(|u| u.alive) {
        has_units[usize::from(u.owner) & 7] = true;
    }
    let mut g = [None; PLAYERS];
    for i in 0..7 {
        if !has_units[i] || g[i].is_some() {
            continue;
        }
        for j in 0..7 {
            if has_units[j] && w.nation_mask[i] & w.nation_mask[j] != 0 {
                g[j] = Some(i as u8);
            }
        }
    }
    g
}

/// The `GlobalEnemyInfo::Process` (0x4275e1) part that refreshes the armies: `EINF[k]`
/// researches when `(tmtmt & 255) == 8k + 1`, k = 0..6.
pub fn tick(w: &mut World) {
    if w.armies.groups.is_none() {
        w.armies.groups = Some(setup(w));
    }
    let t = ((w.tick.0 + 1) & 0xff) as usize;
    if t & 7 != 1 || t >> 3 >= 7 {
        return;
    }
    let groups = w.armies.groups.expect("set");
    if let Some(g) = groups[t >> 3] {
        let a = research(w, &groups, g);
        w.armies.armies[usize::from(g)] = a;
    }
}

/// A 256 px cell with units (`CellInfo`, 12 bytes).
#[derive(Clone, Copy)]
struct Cell {
    force: u16,
    count: u16,
    x: i16,
    y: i16,
    id: u16,
}

/// `ResearchArmyDistribution` (0x4423d0) of the `EnemyInfo` created by nation `g`.
fn research(w: &World, groups: &[Option<u8>; PLAYERS], g: u8) -> Vec<Army> {
    let sh = u32::from(w.map.shift) + 5;
    let wd = 32i32 << w.map.shift;
    let mut map = vec![0xFFFFu16; (wd * wd) as usize];
    let mut cells: Vec<Cell> = Vec::new();
    for u in w.units.iter().filter(|u| u.active()) {
        if groups[usize::from(u.owner) & 7] == Some(g) {
            continue;
        }
        let t = w.catalog.get(u.kind);
        if CATEGORY.get(usize::from(t.role.usage)).is_none_or(|&c| c == 0xff) {
            continue;
        }
        let (cx, cy) = (u.pos.x >> 10, u.pos.y >> 10);
        // The engine kills an object left of or above the map here (0x4e77a0).
        if cx < 0 || cy < 0 {
            continue;
        }
        let (x4, y4) = (cx >> 2, cy >> 2);
        let Some(slot) = usize::try_from(x4 + (y4 << sh)).ok().filter(|&o| x4 < wd && o < map.len()) else {
            continue;
        };
        if map[slot] == 0xFFFF {
            map[slot] = cells.len() as u16;
            cells.push(Cell { force: 0, count: 0, x: x4 as i16, y: y4 as i16, id: 0 });
        }
        let c = &mut cells[usize::from(map[slot])];
        c.force = c.force.wrapping_add(t.role.force as u8 as u16);
        c.count = c.count.wrapping_add(1);
    }
    if cells.is_empty() {
        return Vec::new();
    }
    for (i, c) in cells.iter_mut().enumerate() {
        c.id = (0x2000 + i) as u16;
    }
    // The four neighbours of a cell (the engine reads `+1` and `+W` without a bound).
    let neighbours = |c: &Cell| -> [Option<usize>; 4] {
        let o = i32::from(c.x) + (i32::from(c.y) << sh);
        let at = |o: i32| {
            usize::try_from(o).ok().and_then(|o| map.get(o)).filter(|&&v| v != 0xFFFF).map(|&v| usize::from(v))
        };
        [at(o + 1), at(o + wd), if o > 1 { at(o - 1) } else { None }, if o > wd { at(o - wd) } else { None }]
    };
    // Big cells (more than 3 units) that touch take the smallest id.
    loop {
        let mut changed = false;
        for i in 0..cells.len() {
            let c = cells[i];
            if c.count <= 3 || c.id == 0xFFFF {
                continue;
            }
            for n in neighbours(&c).into_iter().flatten() {
                if cells[n].id > c.id && cells[n].count > 3 {
                    cells[n].id = c.id;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Twice: small cells next to a big one join it.
    for _ in 0..2 {
        for i in 0..cells.len() {
            let c = cells[i];
            if c.count <= 3 || c.id == 0xFFFF {
                continue;
            }
            for n in neighbours(&c).into_iter().flatten() {
                if cells[n].count <= 3 {
                    cells[n].id = c.id;
                }
            }
        }
    }
    let mut armies = Vec::new();
    for i in 0..cells.len() {
        let id = cells[i].id;
        if id < 0x2000 {
            continue;
        }
        let k = armies.len() as u16;
        let mut a = Army { bounds: [0xfa, 0xfa, 0, 0], ..Army::default() };
        let (mut sx, mut sy, mut n) = (0i32, 0i32, 0i32);
        for c in cells.iter_mut().filter(|c| c.id == id) {
            let (x, y) = (i32::from(c.x), i32::from(c.y));
            sx += x;
            sy += y;
            n += 1;
            if x < i32::from(a.bounds[0]) {
                a.bounds[0] = x as u8;
            }
            if x > i32::from(a.bounds[2]) {
                a.bounds[2] = x as u8;
            }
            if y < i32::from(a.bounds[1]) {
                a.bounds[1] = y as u8;
            }
            if y > i32::from(a.bounds[3]) {
                a.bounds[3] = y as u8;
            }
            a.force = a.force.wrapping_add(c.force);
            a.count = a.count.wrapping_add(c.count);
            c.id = k;
        }
        if n != 0 {
            sx /= n;
            sy /= n;
        }
        a.x = ((sx << 8) + 0x80) as i16;
        a.y = ((sy << 8) + 0x80) as i16;
        armies.push(a);
    }
    armies
}
