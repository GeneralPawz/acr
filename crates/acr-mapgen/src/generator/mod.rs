//! The generator proper: one [`Gen`] holds the engine state the original keeps in
//! globals (heights, textures, water, territories, sprites, units). Addresses refer to
//! `dmcr.exe` 1.46.

mod graph;
mod nature;
mod pieces;
mod shape;
mod territory;

use std::collections::HashMap;
use std::rc::Rc;

use acr_formats::m3d::{Grid, Locks, MapSize, Peace, Sprite, SpriteGroup, Unit, Water};
use acr_formats::terrain::RandomTable;
use acr_formats::{M3dMap, Piece, PieceLinks};

use crate::files::{self, ResDat};
use crate::rng::Rng;
use crate::{Error, Generated, PEACE_TIMES, Settings, Source};

/// A map piece with its link file.
pub(crate) struct PieceFile {
    pub piece: Piece,
    pub links: Option<PieceLinks>,
}

/// A sprite while generating: the engine keeps its ground height to drop sprites that a
/// later piece buried (0x4675A3).
#[derive(Clone)]
pub(crate) struct SpriteRec {
    pub group: SpriteGroup,
    pub x: i32,
    pub y: i32,
    pub index: u16,
    pub z: i32,
    pub alive: bool,
}

pub(crate) struct Gen<'a> {
    pub source: &'a mut dyn Source,
    pub rng: Rng<'a>,
    pub random: &'a [u16],
    /// Size shift (1..=3).
    pub s: u32,
    /// Terrain vertices per row and rows.
    pub vw: usize,
    pub vh: usize,
    /// Side of the `SEA2` area in 32-pixel cells (without the 2 extra cells).
    pub sea: i32,
    pub heights: Vec<i16>,
    pub tiles: Vec<u8>,
    /// Land mask, 256 × 256 (+1).
    pub mask: Vec<u8>,
    /// `SEA2` layers, `(sea + 2)²`.
    pub depth: Vec<u8>,
    pub bright: Vec<u8>,
    /// Territories on the coarse grid (128-pixel cells), `(64 << s)²`.
    pub terr: Vec<u8>,
    /// Coarse start cell of each start point (slot) and whether it exists.
    pub start_cell: [(i32, i32); 8],
    pub active: [bool; 8],
    /// Unowned land cells (coarse grid) for pieces without an owner.
    pub unowned: Vec<(i32, i32)>,
    /// Offsets by distance (0x546AC0): `ring[r]` = all `(dx, dy)` with `⌊√(dx²+dy²)⌋ = r`.
    pub ring: Vec<Vec<(i8, i8)>>,
    pub sprites: Vec<SpriteRec>,
    pub units: Vec<Unit>,
    pub locked: Vec<(u16, u16)>,
    pub unlocked: Vec<(u16, u16)>,
    /// Owner of objects in pasted pieces (0x11D6CA8 ^ 0x85).
    pub owner: u8,
    pub factures: Vec<[u8; 5]>,
    pub texture_count: usize,
    /// `UORG` layers (ground paint) and the paint depth buffer of piece `NRG1` chunks.
    pub ground8: Vec<u8>,
    pub ground16: Vec<u16>,
    pub paint_depth: Vec<i32>,
    cache: HashMap<String, Option<Rc<PieceFile>>>,
}

pub(crate) fn run(settings: &Settings, source: &mut dyn Source) -> Result<Generated, Error> {
    let need = |source: &mut dyn Source, path: &str| source.read(path).ok_or_else(|| Error::File(path.into()));
    let random = RandomTable::parse(&need(source, "RANDOM.LST")?).ok_or(Error::File("RANDOM.LST".into()))?;
    let res = ResDat::parse(&need(source, "Terrain\\Res.dat")?).ok_or(Error::File("Terrain\\Res.dat".into()))?;
    let nations = files::parse_ai_nations(&need(source, "AI\\AI.DAT")?);
    if !(1..=3).contains(&settings.size) {
        return Err(Error::Setting("size"));
    }
    let style = res.styles.get(settings.landscape as usize).ok_or(Error::Setting("landscape"))?;
    let (players, msp) = style.1.get(settings.shape as usize).cloned().ok_or(Error::Setting("players"))?;
    let minerals = res.minerals.get(settings.minerals as usize).map_or([1, 1, 1], |m| m.1);
    let resources = res.resources.get(settings.resources as usize).map_or([0; 8], |r| r.1);
    // The start unit (peasant) of every slot's nation (0x456F46).
    let mut peasants: [Option<String>; 8] = Default::default();
    for (slot, n) in settings.nations.iter().enumerate() {
        if let Some(n) = n {
            peasants[slot] = Some(nations.get(*n as usize).ok_or(Error::Setting("nation"))?.1.clone());
        }
    }
    let texture_count = source.read("TILES3.BMP").map_or(136, |b| b.len() >> 12).max(1);

    let s = u32::from(settings.size);
    let vh = 256usize << s;
    let vw = vh + 1;
    let sea = 240i32 << s;
    let table = random.0;
    let mut g = Gen {
        rng: Rng::new(&table, settings.seed),
        random: &table,
        source,
        s,
        vw,
        vh,
        sea,
        heights: vec![0; vw * vh],
        tiles: vec![0; vw * vh],
        mask: Vec::new(),
        depth: vec![0; ((sea + 2) * (sea + 2)) as usize],
        bright: vec![0; ((sea + 2) * (sea + 2)) as usize],
        terr: vec![0xFF; (64usize << s) * (64usize << s)],
        start_cell: [(0, 0); 8],
        active: [false; 8],
        unowned: Vec::new(),
        ring: territory::ring_table(),
        sprites: Vec::new(),
        units: Vec::new(),
        locked: Vec::new(),
        unlocked: Vec::new(),
        owner: 7,
        factures: Vec::new(),
        texture_count,
        ground8: vec![0x81; (0x100usize << s) * (0x80usize << s)],
        ground16: vec![0; (0x100usize << s) * (0x80usize << s)],
        paint_depth: vec![0; (0x200usize << s) * (0x100usize << s)],
        cache: HashMap::new(),
    };

    // The relief's piece set: `Terrain\terr2.dat` lines `climate size relief file` (0x4572F5).
    let mut terr_file = String::new();
    if let Some(list) = g.source.read("Terrain\\terr2.dat") {
        let t = files::tokens(&list);
        for c in t.as_chunks::<4>().0 {
            let n = |i: usize| c[i].parse::<i32>().ok();
            if n(0) == Some(settings.climate.into()) && n(1) == Some(s as i32) && n(2) == Some(settings.relief.into()) {
                terr_file = c[3].clone();
            }
        }
    }

    // 0x45CD31
    let msp_path = format!("Terrain\\{msp}");
    let data = need(g.source, &msp_path)?;
    let variants = files::msp_variants(&data).ok_or(Error::File(msp_path.clone()))?;
    let variant = (g.rng.index() * variants) >> 15;
    let shape = files::msp_shape(&data, variant).ok_or(Error::File(msp_path))?;
    g.mask = shape.mask.clone();
    g.blur_mask();
    g.water();
    g.shore_textures(settings.climate);
    let order = g.start_points(&shape);
    g.territories(&order);

    // Relief rules: loaded only (and the hills file picked at random) but never applied
    // to random maps (0x45DAFB; the texturing pass 0x4516F0 is not reached).
    if settings.relief != 2 {
        for _ in 0..100 {
            let r = g.rng.rand() & 7;
            if g.source.read(&format!("Terrain\\Hills{r}.dat")).is_some() {
                break;
            }
        }
    }

    let peace = PEACE_TIMES[settings.digit(3) as usize];
    let fract = g
        .source
        .read(&format!("Terrain\\fract_set{}.txt", settings.climate))
        .and_then(|d| files::parse_fract(&d))
        .unwrap_or([0; 8]);
    g.factures = g
        .source
        .read(&format!("Terrain\\factures_set{}.dat", settings.climate))
        .map_or_else(|| vec![[0xFF; 5]; 256], |d| files::parse_factures(&d));

    let nature = nature::Nature {
        players: players.max(0) as usize,
        minerals,
        peasants: &peasants,
        start_units: settings.digit(0),
        climate: settings.climate.into(),
        landscape: settings.landscape.into(),
        terr_file: &terr_file,
        fract,
        peace,
    };
    let outcome = g.nature(&nature);

    g.elevate();
    for t in &mut g.tiles {
        *t &= 0x7F;
    }
    let ground = g.ground();

    // Start resources: the menu's amounts plus the last start file's extras (0x4574E9).
    let mut resu = [[0u32; 8]; 8];
    for row in &mut resu {
        for (i, r) in row.iter_mut().enumerate() {
            *r = resources[i].wrapping_add(outcome.extra_resources[i] as u32);
        }
    }

    let size = MapSize { shift: s as u8 };
    let coarse = 64usize << s;
    let map = M3dMap {
        size: Some(size),
        header: [vw as u32, vh as u32],
        heights: Some(Grid { width: vw, height: vh, cells: g.heights.clone() }),
        textures: Some(Grid { width: vw, height: vh, cells: g.tiles.clone() }),
        sprites: g
            .sprites
            .iter()
            .filter(|s| s.alive)
            .map(|s| Sprite { group: s.group, x: s.x, y: s.y, index: s.index, extra: Some(0) })
            .collect(),
        units: std::mem::take(&mut g.units),
        locks: Some(Locks { locked: g.locked.clone(), unlocked: g.unlocked.clone() }),
        water: Some(Water {
            depth: Grid { width: (sea + 2) as usize, height: (sea + 2) as usize, cells: g.depth.clone() },
            brightness: Grid { width: (sea + 2) as usize, height: (sea + 2) as usize, cells: g.bright.clone() },
        }),
        resources: Some(resu),
        peace: Some(Peace {
            cells: Grid { width: coarse, height: coarse, cells: g.terr.clone() },
            time: peace,
            players: outcome.player_of_slot,
            flags: [settings.digit(1) as u8, settings.digit(2) as u8, settings.digit(4) as u8],
        }),
        rivers: Some([
            Grid { width: coarse, height: coarse, cells: vec![0; coarse * coarse] },
            Grid { width: coarse, height: coarse, cells: vec![0; coarse * coarse] },
        ]),
        ground: Some(ground),
        mission_dll: Some(String::new()),
        ..M3dMap::default()
    };
    Ok(Generated { map, starts: outcome.starts })
}

impl Gen<'_> {
    /// World position of terrain vertex `v` (0x4463D5, 0x44641E).
    pub fn vertex_pos(&self, v: usize) -> (i32, i32) {
        let (col, row) = ((v % self.vw) as i32, (v / self.vw) as i32);
        (col << 5, (row << 5) - ((col & 1) << 4))
    }

    /// A piece and its links, read once.
    pub fn piece(&mut self, path: &str) -> Option<Rc<PieceFile>> {
        let key = path.to_ascii_uppercase();
        if let Some(p) = self.cache.get(&key) {
            return p.clone();
        }
        let piece = self.source.read(path).map(|d| Piece::parse(&d));
        let links = self.source.read(&format!("{path}.lnk")).map(|d| PieceLinks::parse(&d));
        let file = piece.map(|piece| Rc::new(PieceFile { piece, links }));
        self.cache.insert(key, file.clone());
        file
    }

    /// Whitespace-separated entries of a `.lst` file (0x45F114); missing files are empty.
    pub fn list(&mut self, path: &str) -> Vec<String> {
        self.source.read(path).map(|d| files::tokens(&d)).unwrap_or_default()
    }

    /// Ground height at a world position (`GetHeight`, 0x4D0530).
    pub fn height_at(&self, x: i32, y: i32) -> i32 {
        let w = self.vw;
        let playable = self.sea << 5;
        let x = x.clamp(0, playable);
        let y = y.clamp(32, playable);
        let col = (x >> 5) as usize;
        let at = |row: usize, c: usize| self.heights.get(row * w + c).copied().map_or(0, i32::from);
        let fx = x & 31;
        if col & 1 == 1 {
            let t = 15 - (fx >> 1) + y;
            let (row, fy) = ((t >> 5) as usize, t & 31);
            let base = (row as i32) << 5;
            if fy > 32 - fx {
                let a = at(row, col + 1);
                let c = at(row + 1, col);
                let b = at(row + 1, col + 1);
                c + (((x - ((col as i32) << 5)) * (((b + a) >> 1) - c)) >> 5) + (((y - base - 16) * (b - a)) >> 5)
            } else {
                let a = at(row, col + 1);
                let b = at(row, col);
                let c = at(row + 1, col);
                let dx = (x - ((col as i32 + 1) << 5)) * (((c + b) >> 1) - a);
                (((y - base) * (c - b)) >> 5) - (dx >> 5) + a
            }
        } else {
            let t = (fx >> 1) + y;
            let (row, fy) = ((t >> 5) as usize, t & 31);
            let base = (row as i32) << 5;
            if fy < fx {
                let b0 = at(row, col);
                let a1 = at(row, col + 1);
                let c = at(row + 1, col + 1);
                b0 + (((x - ((col as i32) << 5)) * (((c + a1) >> 1) - b0)) >> 5) + (((y - base) * (c - a1)) >> 5)
            } else {
                let d = at(row + 1, col + 1);
                let b0 = at(row, col);
                let c0 = at(row + 1, col);
                let dx = (x - ((col as i32 + 1) << 5)) * (((c0 + b0) >> 1) - d);
                (((y - base - 16) * (c0 - b0)) >> 5) - (dx >> 5) + d
            }
        }
    }

    /// Adds a sprite (0x4D0300); `z` is the ground height with pieces' temporary 0x800 lift
    /// removed again.
    pub fn add_sprite(&mut self, group: SpriteGroup, x: i32, y: i32, index: u16) {
        let lim = 64 << self.s;
        if x >> 7 >= lim || y >> 7 >= lim {
            return;
        }
        let mut z = self.height_at(x, y) as i16;
        if z > 0x400 {
            z = z.wrapping_sub(0x800);
        }
        self.sprites.push(SpriteRec { group, x, y, index, z: i32::from(z), alive: true });
    }

    /// Creates a unit of `slot` at a world position (1/16 pixels in the map).
    pub fn add_unit(&mut self, slot: u8, x: i32, y: i32, name: &str) {
        self.units.push(Unit {
            nation: slot,
            type_index: 0,
            x: x << 4,
            y: y << 4,
            life: 0,
            stage: 0,
            unknown: [0; 2],
            direction: 0,
            flags: 0,
            name: name.to_string(),
        });
    }
}
