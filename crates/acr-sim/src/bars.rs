//! The 3D boxes of buildings (`3DBARS`, map at 0x6bc600 with 256 px cells): they stop
//! projectiles and line traces and hide units from searches. `Add3DBar` (0x443697) runs
//! when an object is created, `Delete3DBars` (0x443a25) when it dies; `Bar3DHeight`
//! (0x443b06) and `Bar3DOwner` (0x443d37) look them up. See `docs/simulation.md`.

/// One box: a rectangle turned by 45° (`U = x − y`, `V = x + y`) with a height above the
/// ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bar {
    pub u: (i32, i32),
    pub v: (i32, i32),
    pub height: i32,
    /// The object (unit index) the box belongs to.
    pub owner: u32,
}

/// The box map.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Bars {
    /// Cells per row.
    pub side: i32,
    /// Box indices per 256 px cell, in insertion order.
    pub cells: Vec<Vec<u16>>,
    /// Box slots (a removed box leaves a free slot).
    pub list: Vec<Option<Bar>>,
}

impl Bars {
    /// An empty map for a square map of `px` pixels.
    pub fn new(px: i32) -> Self {
        let side = (px >> 8) + 1;
        Self { side, cells: vec![Vec::new(); (side * side) as usize], list: Vec::new() }
    }

    fn cell(&self, cx: i32, cy: i32) -> Option<usize> {
        (cx >= 0 && cy >= 0 && cx < self.side && cy < self.side).then(|| (cy * self.side + cx) as usize)
    }

    /// `Add3DBar(x1, y1, x2, y2, h, owner)` (0x443697): the box goes into the cells of nine
    /// sample points only (corners, edge points and the centre, with the engine's two
    /// misplaced edge points), each cell once.
    pub fn add(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, height: i32, owner: u32) {
        let bar = Bar { u: (x1 - y1, x2 - y2), v: (x1 + y1, x2 + y2), height, owner };
        let slot = match self.list.iter().position(Option::is_none) {
            Some(s) => s,
            None => {
                self.list.push(None);
                self.list.len() - 1
            }
        };
        if slot >= 0x2000 {
            self.list.pop();
            return;
        }
        self.list[slot] = Some(bar);
        let dc = (y2 - y1 + x2 - x1) >> 1;
        let d8 = (x2 - x1 - y2 + y1) >> 1;
        let (d14, d1c) = (d8 >> 1, dc >> 1);
        let pts = [
            (x1 >> 8, y1 >> 8),
            (x2 >> 8, y2 >> 8),
            ((x1 + d8) >> 8, (y1 - d8) >> 8),
            ((x1 + dc) >> 8, (y1 + dc) >> 8),
            ((x1 + x2) >> 9, (y1 + y2) >> 9),
            ((x1 + d14) >> 8, (y1 - d14) >> 8),
            ((x2 - d14) >> 8, (y2 - d14) >> 8),
            ((x1 + d1c) >> 8, (y1 + d1c) >> 8),
            ((x1 - d14) >> 8, (y2 + d14) >> 8),
        ];
        for (cx, cy) in pts {
            if let Some(c) = self.cell(cx, cy)
                && !self.cells[c].contains(&(slot as u16))
            {
                self.cells[c].push(slot as u16);
            }
        }
    }

    /// `Delete3DBars(owner)` (0x443a25).
    pub fn remove(&mut self, owner: u32) {
        let gone: Vec<u16> = (0..self.list.len())
            .filter(|&s| self.list[s].is_some_and(|b| b.owner == owner))
            .map(|s| s as u16)
            .collect();
        if gone.is_empty() {
            return;
        }
        for c in &mut self.cells {
            c.retain(|s| !gone.contains(s));
        }
        for s in gone {
            self.list[usize::from(s)] = None;
        }
    }

    /// `Bar3DHeight` / `Bar3DOwner` (0x443b06 / 0x443d37): the highest box covering a point
    /// (the first on a tie) whose owner `exists`, with its owner; `(0, None)` if none.
    pub fn at(&self, x: i32, y: i32, exists: impl Fn(u32) -> bool) -> (i32, Option<u32>) {
        let Some(c) = self.cell(x >> 8, y >> 8) else { return (0, None) };
        let (u, v) = (x - y, x + y);
        let mut best = (0, None);
        for &s in &self.cells[c] {
            let Some(b) = self.list[usize::from(s)] else { continue };
            if b.u.0 <= u && u <= b.u.1 && b.v.0 <= v && v <= b.v.1 && b.height > best.0 && exists(b.owner) {
                best = (b.height, Some(b.owner));
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_covers_its_turned_rectangle() {
        let mut b = Bars::new(4096);
        // ATCBLG at (1000, 1000) px: LOCATION -250 -273, 3DBARS 164 270 100 86 204.
        let (x0, y0) = (1000 - 250 + 164, 1000 - 2 * 273 + 2 * 270);
        b.add(x0, y0, x0 + 100 + 86, y0 + 100 - 86, 204, 7);
        assert_eq!(b.at(x0 + 50, y0, |_| true), (204, Some(7)));
        assert_eq!(b.at(x0 - 50, y0, |_| true), (0, None));
        assert_eq!(b.at(x0 + 50, y0, |_| false), (0, None));
        b.remove(7);
        assert_eq!(b.at(x0 + 50, y0, |_| true), (0, None));
    }
}
