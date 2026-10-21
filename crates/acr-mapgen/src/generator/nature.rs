//! Start areas, mines, nature and the relief pieces (0x45F85C).

use super::Gen;
use crate::files;

pub(crate) struct Nature<'a> {
    /// Players the land shape is made for (start points to use).
    pub players: usize,
    /// Mines per start area: three counts (`Terrain\Res.dat`).
    pub minerals: [i32; 3],
    /// Start unit (peasant) per colour slot; `None` for no player.
    pub peasants: &'a [Option<String>; 8],
    /// `Data\Start%d.dat` number (0: 18 peasants).
    pub start_units: u32,
    pub climate: u32,
    pub landscape: u32,
    pub terr_file: &'a str,
    pub fract: [u8; 8],
    /// Peace time in seconds (border markers are placed when non-zero).
    pub peace: u32,
}

pub(crate) struct Outcome {
    pub starts: [Option<(i32, i32)>; 8],
    /// Player (colour slot) settled at each start point, 0xFF for none (`CAEP`).
    pub player_of_slot: [u8; 8],
    /// Extra start resources of the last start file block, in `RESU` order.
    pub extra_resources: [i32; 8],
}

const MINES: &str = "Pieces\\MINA0.LST";
const TREES: &str = "Pieces\\TRSS.LST";
const STONES: &str = "Pieces\\STNN0.LST";

impl Gen<'_> {
    pub fn nature(&mut self, n: &Nature) -> Outcome {
        let mut out = Outcome { starts: [None; 8], player_of_slot: [0xFF; 8], extra_resources: [0; 8] };
        // Which start points get a player.
        let mut state: [u8; 8] = self.active.map(u8::from);
        let (mut i, mut tries) = (0, 0);
        while i < n.players && tries < 1000 {
            let r = (self.rng.rand() & 7) as usize;
            if state[r] == 1 {
                state[r] = 2;
                i += 1;
            }
            tries += 1;
        }
        let mut player = 0usize;
        #[allow(clippy::needless_range_loop)]
        for slot in 0..8 {
            if !self.active[slot] {
                continue;
            }
            let tag = (slot << 4) as u8;
            for list in [TREES, STONES] {
                let entries = self.list(list);
                if entries.is_empty() {
                    continue;
                }
                let reps = if list == TREES { 1 } else { 2 };
                for _ in 0..reps {
                    let j = self.rng.below(entries.len() as i32) as usize;
                    self.place_band(&entries[j].clone(), tag + 1, tag + 0xE, 0x32);
                }
            }
            if state[slot] != 2 {
                continue;
            }
            let (cx, cy) = (self.start_cell[slot].0 << 7, self.start_cell[slot].1 << 7);
            self.reserve(cx, cy, 750);
            while player < 7 && n.peasants[player].is_none() {
                player += 1;
            }
            let Some(peasant) = n.peasants.get(player).cloned().flatten() else { continue };
            out.player_of_slot[slot] = player as u8;
            out.starts[player] = Some((cx, cy));
            let block = (n.start_units != 0)
                .then(|| self.source.read(&format!("Data\\Start{}.dat", n.start_units)))
                .flatten()
                .and_then(|d| files::parse_start_units(&d, &peasant));
            match block {
                Some(block) => {
                    // Wood, food, stone, gold, iron, coal -> RESU 0, 3, 2, 1, 4, 5 (0x4A15DC).
                    for (k, &r) in [0usize, 3, 2, 1, 4, 5].iter().enumerate() {
                        out.extra_resources[r] = block.resources[k];
                    }
                    out.extra_resources[6] = 0;
                    out.extra_resources[7] = 0;
                    for (dx, dy, id) in &block.units {
                        self.add_unit(player as u8, cx + dx * 32, cy + dy * 32, id);
                    }
                }
                None => {
                    out.extra_resources = [0; 8];
                    for _ in 0..18 {
                        let dx = (self.rng.rand() << 7) >> 15;
                        let dy = (self.rng.rand() << 7) >> 15;
                        self.add_unit(player as u8, cx + dx, cy + dy, &peasant);
                    }
                }
            }
            player += 1;
        }

        // Mines and features of every start area.
        for slot in 0..8 {
            if !self.active[slot] {
                continue;
            }
            let tag = (slot << 4) as u8;
            let mines = self.list(MINES);
            let g = (mines.len() / 4) as i32;
            if g != 0 {
                let pick = |g2: &mut Gen, k: usize| mines[(g2.rng.below(g) * 4) as usize + k].clone();
                let m = pick(self, 0);
                self.place_band(&m, tag + 2, tag + 0xE, 600);
                let m = pick(self, 3);
                self.place_band(&m, tag + 2, tag + 0xE, 600);
                let m = pick(self, 1);
                self.place_band(&m, tag + 3, tag + 0xE, 600);
                let m = pick(self, 2);
                if !self.place_band(&m, tag + 8, tag + 0xE, 600) {
                    self.place_band(&m, tag + 3, tag + 0xE, 600);
                }
                let [a, b, c] = n.minerals;
                let mut k = 1;
                while k < a || k < b || k < c {
                    if k < a {
                        let m = pick(self, 0);
                        self.place_near(&m, tag + 8, tag + 0xE, 400);
                    }
                    if k < b {
                        let m = pick(self, 1);
                        self.place_near(&m, tag + 8, tag + 0xE, 400);
                    }
                    if k < c {
                        let m = pick(self, 2);
                        self.place_near(&m, tag + 8, tag + 0xE, 400);
                        let m = pick(self, 3);
                        self.place_band(&m, tag + 8, tag + 0xE, 600);
                    }
                    k += 1;
                }
            }
            let saved = self.owner;
            self.owner = 7;
            let features = self
                .source
                .read(&format!("Terrain\\feature{}{}.dat", n.climate, n.landscape))
                .or_else(|| self.source.read(&format!("Terrain\\feature{}.dat", n.climate)));
            for (list, count, dist) in features.map(|d| files::parse_features(&d)).unwrap_or_default() {
                let entries = self.list(&list);
                if entries.is_empty() {
                    continue;
                }
                for _ in 0..count {
                    let j = self.rng.below(entries.len() as i32) as usize;
                    self.place_band(&entries[j].clone(), tag.wrapping_add(dist as u8), tag + 0xE, 600);
                }
            }
            self.owner = saved;
        }

        // Land nobody owns.
        if !self.unowned.is_empty() {
            let mines = self.list(MINES);
            let g = (mines.len() / 3) as i32;
            for _ in 0..30 {
                for k in 0..3 {
                    let j = (self.rng.below(g) * 4) as usize + k;
                    if let Some(m) = mines.get(j).cloned() {
                        self.place_band(&m, 0xFF, 0xFF, 0x14);
                    }
                }
            }
            for list in [STONES, TREES] {
                let entries = self.list(list);
                if entries.len() / 3 == 0 {
                    continue;
                }
                for _ in 0..20 {
                    let j = self.rng.below(entries.len() as i32) as usize;
                    self.place_band(&entries[j].clone(), 0xFF, 0xFF, 0x14);
                }
            }
        }

        let terr = self.source.read(n.terr_file).map(|d| files::parse_terr(&d)).unwrap_or_default();
        self.chains(&terr, &n.fract);
        if n.peace != 0 {
            self.borders();
        }
        out
    }

    /// Peace-time border markers between territories (0x46068D). The original creates
    /// them as `HO` sprites that turn into ground decals and never reach the saved map, so
    /// only the random numbers are drawn here.
    fn borders(&mut self) {
        let sh = self.s + 6;
        let stride = 1usize << sh;
        let limit = (0x3C << self.s) - 2;
        loop {
            let copy = self.terr.clone();
            let mut changed = false;
            for y in 2..limit {
                for x in 2..limit {
                    let i = ((y << sh) + x) as usize;
                    if copy[i] != 0xFF {
                        continue;
                    }
                    for nb in [copy[i + 1], copy[i - 1], copy[i + stride], copy[i - stride]] {
                        if nb != 0xFF {
                            self.terr[i] = nb;
                            changed = true;
                            break;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let edge = stride as i32 - 2;
        let mut i = 0usize;
        for y in 0..stride as i32 {
            for x in 0..stride as i32 {
                if x > 0 && x < edge && y > 0 && y < edge && self.terr[i] != 0xFF {
                    let (wx, wy) = ((x << 7) + 0x40, (y << 7) + 0x40);
                    let p = self.terr[i] >> 4;
                    let right = self.terr[i + 1];
                    if right != 0xFF && right >> 4 != p {
                        for k in -1..3 {
                            self.border_mark(wx + 0x40, wy + k * 32 - 16);
                        }
                    }
                    let down = self.terr[i + stride];
                    if down != 0xFF && down >> 4 != p {
                        for k in -1..3 {
                            self.border_mark(wx + k * 32 - 16, wy + 0x40);
                        }
                    }
                }
                i += 1;
            }
        }
    }

    /// 0x460604
    fn border_mark(&mut self, x: i32, y: i32) {
        let n = self.sea + 2;
        let (cx, cy) = (x >> 5, y >> 5);
        let deep = cx >= 0 && cy >= 0 && cx < n && cy < n && self.depth[(cy * n + cx) as usize] > 0x7F;
        if deep {
            self.shoal(x, y >> 1, 6);
            return;
        }
        let _index = self.rng.rand() % 6;
        let _dy = self.rng.rand() % 7;
        let _dx = self.rng.rand() % 7;
    }

    /// Brightens the water in a circle of `r` cells (0x53A36A with its last argument set):
    /// the peace-time border shows as light marks on the sea.
    fn shoal(&mut self, x: i32, y: i32, r: i32) {
        let (cx, cy) = (x >> 5, y >> 4);
        let n = self.sea + 2;
        for dx in -r..=r {
            for dy in -r..=r {
                let d16 = f64::from((dx * dx + dy * dy) << 8).sqrt() as i32;
                let (nx, ny) = (cx + dx, cy + dy);
                // The engine bounds-checks against the 256 << s row stride; columns and
                // rows beyond the saved `SEA2` area are padding.
                if d16 >> 4 <= r && nx >= 0 && ny >= 0 && nx < n && ny < n {
                    let v = r * 16 - d16;
                    let i = (ny * n + nx) as usize;
                    if i32::from(self.bright[i]) < v {
                        self.bright[i] = v as u8;
                    }
                }
            }
        }
    }
}
