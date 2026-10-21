//! Relief, forests and settlements as chains of linked pieces (0x466C4A and the class
//! around it, 0x4654C8..0x4673DA).
//!
//! Every piece listed in a `.terr` line is loaded with its links (`*.SMP.LNK`): footprint
//! cells on a grid of 256-pixel cells and links `(kind, direction, dx, dy, mask)`. A chain
//! starts at a random cell with a random link kind; each step picks a random direction
//! allowed by the mask, a random piece with a link of that kind and direction, and moves
//! on through the piece's other link. Footprints must not overlap earlier chains. Links
//! with kind > 63 spawn further chains of that kind. Accepted chains are pasted at the end,
//! grouped by piece file.

use std::collections::HashMap;

use super::Gen;
use super::pieces::Blend;
use crate::files::TerrLine;

#[derive(Clone, Copy)]
struct Link {
    kind: u8,
    dx: i32,
    dy: i32,
    mask: u32,
}

struct LibPiece {
    name: String,
    /// Pixel offset of the piece within its cell (the link file's flags word).
    offset: (i32, i32),
    cells: Vec<(i32, i32)>,
    links: Vec<Link>,
}

/// Branch state of a chain (0x29 bytes in the original; only slot 0 is ever used because
/// the branch search at 0x4660DF never runs).
struct Slot {
    active: bool,
    x: i32,
    y: i32,
    kind: u8,
    mask: u32,
}

/// One accepted piece of a chain.
struct Placed {
    px: i32,
    py: i32,
    x: i32,
    y: i32,
    piece: usize,
    link: usize,
}

pub(crate) struct Chains {
    n: i32,
    occupied: Vec<u16>,
    second: Vec<u16>,
    id: u16,
    pieces: Vec<LibPiece>,
    index: HashMap<(u8, u8), Vec<(u16, u8)>>,
    /// Start link choices of the current `.terr` line.
    starts: Vec<(i32, i32)>,
    /// Chains to spawn from links with kind > 63: `(x, y, mask, kind)`.
    specials: Vec<(i32, i32, u32, u8)>,
    /// Accepted positions by piece file, in order of first use.
    groups: Vec<(String, Vec<(i32, i32)>)>,
}

impl Chains {
    fn new(n: i32) -> Self {
        let cells = (n * n) as usize;
        Self {
            n,
            occupied: vec![0; cells],
            second: vec![0; cells],
            id: 1,
            pieces: Vec::new(),
            index: HashMap::new(),
            starts: Vec::new(),
            specials: Vec::new(),
            groups: Vec::new(),
        }
    }

    fn mark(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x >= self.n || y >= self.n {
            return;
        }
        let i = (y * self.n + x) as usize;
        if self.occupied[i] == 0 {
            self.occupied[i] = self.id;
        }
        self.second[i] = self.id;
    }

    fn taken(&self, x: i32, y: i32) -> bool {
        x < 0 || y < 0 || x >= self.n || y >= self.n || self.occupied[(y * self.n + x) as usize] != 0
    }

    /// 0x466A6C
    fn fits(&self, x: i32, y: i32, piece: usize, link: usize) -> bool {
        let p = &self.pieces[piece];
        let (bx, by) = (x - p.links[link].dx, y - p.links[link].dy);
        p.cells.iter().all(|&(cx, cy)| !self.taken(bx + cx, by + cy))
    }

    /// 0x466992
    fn occupy(&mut self, x: i32, y: i32, piece: usize, link: usize) {
        let p = &self.pieces[piece];
        let (bx, by) = (x - p.links[link].dx, y - p.links[link].dy);
        let cells: Vec<(i32, i32)> = p.cells.iter().map(|&(cx, cy)| (bx + cx, by + cy)).collect();
        for (cx, cy) in cells {
            self.mark(cx, cy);
        }
        self.id = self.id.wrapping_add(1);
    }

    /// 0x466FA9
    fn register(&mut self, piece: usize, px: i32, py: i32) {
        let name = &self.pieces[piece].name;
        match self.groups.iter_mut().find(|(n, _)| n == name) {
            Some((_, list)) => list.push((px, py)),
            None => self.groups.push((name.clone(), vec![(px, py)])),
        }
    }
}

impl Gen<'_> {
    /// Loads a piece list into the library (0x4659CA, 0x46566D); pieces without a link
    /// file are skipped.
    fn load_library(&mut self, chains: &mut Chains, list: &str) {
        for name in self.list(list) {
            let Some(file) = self.piece(&name) else { continue };
            let Some(links) = &file.links else { continue };
            let idx = chains.pieces.len();
            let piece = LibPiece {
                name: name.clone(),
                offset: (i32::from(links.flags as u16 as i16), i32::from((links.flags >> 16) as u16 as i16)),
                cells: links.cells.iter().map(|&(x, y)| (i32::from(x), i32::from(y))).collect(),
                links: links
                    .links
                    .iter()
                    .map(|l| Link { kind: l.kind, dx: l.dx.into(), dy: l.dy.into(), mask: l.value })
                    .collect(),
            };
            for (li, l) in links.links.iter().enumerate() {
                chains.index.entry((l.kind, l.direction)).or_default().push((idx as u16, li as u8));
            }
            chains.pieces.push(piece);
        }
    }

    /// 0x466C4A
    pub fn chains(&mut self, terr: &[TerrLine], fract: &[u8; 8]) {
        self.noise_textures(fract);
        let n = 30 << self.s;
        let mut chains = Chains::new(n);
        let mut v = 0;
        for row in 0..self.vh {
            for col in 0..self.vw {
                if self.tiles[v] > 0x7F {
                    chains.mark((col >> 3) as i32, (row >> 3) as i32);
                }
                v += 1;
            }
        }
        for line in terr {
            let count = line.count << (self.s - 1);
            chains.starts = line.starts.clone();
            self.load_library(&mut chains, &line.list);
            for _ in 0..count {
                // 0x465A51
                let x = self.rng.rand() % n;
                let y = self.rng.rand() % n;
                if chains.starts.is_empty() {
                    continue;
                }
                let k = (self.rng.rand() % chains.starts.len() as i32) as usize;
                let (kind, len) = chains.starts[k];
                self.chain(&mut chains, x, y, kind as u8, len);
            }
            let mut i = 0;
            while i < chains.specials.len() {
                let (x, y, _, kind) = chains.specials[i];
                self.chain(&mut chains, x, y, kind, 50);
                i += 1;
            }
        }
        self.paste_chains(&chains);
        self.filter_locks();
    }

    /// One chain (0x465ABE). Returns false if its first piece does not fit.
    fn chain(&mut self, c: &mut Chains, x: i32, y: i32, kind: u8, mut count: i32) -> bool {
        let mut slots: Vec<Slot> = (0..4).map(|_| Slot { active: false, x: -1, y: -1, kind, mask: 0 }).collect();
        slots[0] = Slot { active: true, x, y, kind, mask: 0xFF };
        let min = if count >= 10 {
            6
        } else if count >= 4 {
            4
        } else {
            1
        };
        let mut placed: Vec<Placed> = Vec::new();
        let mut placed_count = 0usize;
        let mut pending: Vec<(i32, i32, u32, u8)> = Vec::new();
        let mut first = true;
        let mut ended = false;
        let mut history = [usize::MAX; 7];
        loop {
            #[allow(clippy::needless_range_loop)] // the original's slot loop
            for si in 0..4 {
                if !slots[si].active {
                    continue;
                }
                let mut tries = 0;
                let mut go_on = true;
                loop {
                    let r = (self.rng.rand() & 7) as u8;
                    let slot = &slots[si];
                    if slot.mask & (1 << r) != 0 {
                        let options = c.index.get(&(slot.kind, r)).map_or(0, Vec::len);
                        if options != 0 {
                            let j = self.rng.below(options as i32) as usize;
                            let (piece, link) = c.index[&(slot.kind, r)][j];
                            let (piece, link) = (usize::from(piece), usize::from(link));
                            let (sx, sy) = (slot.x, slot.y);
                            if !history.contains(&piece) && c.fits(sx, sy, piece, link) {
                                first = false;
                                let p = &c.pieces[piece];
                                let l = p.links[link];
                                if placed_count < 0x80 {
                                    placed.push(Placed {
                                        px: ((sx - l.dx) << 8) + p.offset.0,
                                        py: ((sy - l.dy) << 8) + p.offset.1,
                                        x: sx,
                                        y: sy,
                                        piece,
                                        link,
                                    });
                                    placed_count += 1;
                                }
                                history.rotate_right(1);
                                history[0] = piece;
                                let nl = p.links.len();
                                let mut next = None;
                                if nl == 1 {
                                    ended = true;
                                    go_on = false;
                                } else if nl == 2 {
                                    next = Some(1 - link);
                                } else {
                                    next = (0..nl).find(|&i| i != link && p.links[i].kind < 15);
                                }
                                for nb in &p.links {
                                    if nb.kind > 0x3F && pending.len() < 0x80 {
                                        pending.push((sx + nb.dx - l.dx, sy + nb.dy - l.dy, nb.mask, nb.kind));
                                    }
                                }
                                if let Some(nx) = next {
                                    let to = p.links[nx];
                                    let slot = &mut slots[si];
                                    slot.x = sx + to.dx - l.dx;
                                    slot.y = sy + to.dy - l.dy;
                                    slot.mask = to.mask;
                                    go_on = false;
                                }
                            } else if first {
                                return false;
                            }
                        }
                    }
                    tries += 1;
                    if tries >= 50 || !go_on {
                        break;
                    }
                }
            }
            count -= 1;
            if count <= 0 {
                break;
            }
        }
        if placed_count >= min || (ended && placed_count > 4) {
            for p in &placed {
                c.register(p.piece, p.px, p.py);
                c.occupy(p.x, p.y, p.piece, p.link);
            }
            c.specials.extend(pending);
        }
        true
    }

    /// Pastes the accepted pieces (0x4673DA) and drops buried sprites.
    fn paste_chains(&mut self, c: &Chains) {
        let saved = self.owner;
        self.owner = 7;
        for (name, positions) in &c.groups {
            let blend = if name.contains("LOW_") { Blend::Min } else { Blend::Max };
            for &(px, py) in positions {
                self.paste_blend(name, px, py, blend);
            }
        }
        for i in 0..self.sprites.len() {
            let s = &self.sprites[i];
            if s.alive && s.z < self.height_at(s.x, s.y) {
                self.sprites[i].alive = false;
            }
        }
        self.owner = saved;
    }

    /// Keeps only locked cells on steep ground (0x468E38).
    fn filter_locks(&mut self) {
        let locked = std::mem::take(&mut self.locked);
        for (cx, cy) in locked {
            let (x, y) = (i32::from(cx) << 6, i32::from(cy) << 6);
            let _centre = self.height_at(x + 32, y + 32);
            let a = self.height_at(x, y);
            let b = self.height_at(x + 64, y);
            let c = self.height_at(x, y + 64);
            let d = self.height_at(x + 64, y + 64);
            let gx = (b + d - a - c) / 2;
            let gy = (a + b - c - d) / 2;
            let slope = f64::from(gx * gx + gy * gy).sqrt() as i32;
            if slope > 0x23 {
                self.locked.push((cx, cy));
            }
        }
    }
}
