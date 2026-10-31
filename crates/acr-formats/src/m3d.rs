//! `.m3d` maps: terrain, placed objects and mission data.
//!
//! Reverse engineered from the map loader and saver of `dmcr.exe` 1.46 and checked
//! against every shipped map; see `docs/formats/m3d.md` for the evidence behind each
//! rule. Layout (little-endian):
//!
//! ```text
//! header  "3DMP" | "4DMP" | "5DMP"    size shift s = magic[0] - '2' (1..=3)
//!         u32 width = 2*(128<<s) + 1  u32 height = 2*(128<<s)   terrain vertices (ignored by the loader)
//! chunk   [u8;4] tag | u32 len (counts itself, not the tag) | len - 4 bytes payload
//!         ... repeated, terminated by "ENDM" or end of file
//! ```
//!
//! The loader dispatches on the tag and lets each chunk decoder read its own data; the
//! length is only used to skip unknown chunks. The saver writes the last chunk (`UORG`)
//! with a length 4 bytes too large, so its payload swallows the closing `ENDM`.

use std::fmt;

/// A chunk tag as stored in the file, e.g. `*b"SURF"`.
pub type Tag = [u8; 4];

/// Width and height of the `VIE1` preview image (one palette index per pixel).
pub const PREVIEW_WIDTH: usize = 400;
pub const PREVIEW_HEIGHT: usize = 224;
/// Size of the older `VIEW` preview, which the 1.46 loader skips.
pub const OLD_PREVIEW_WIDTH: usize = 292;
pub const OLD_PREVIEW_HEIGHT: usize = 190;
/// Number of player (nation) slots in per-player tables.
pub const PLAYERS: usize = 8;
/// Distance between neighbouring terrain vertices in world pixels.
pub const VERTEX_SPACING: i32 = 32;

const END: &Tag = b"ENDM";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M3dError {
    pub offset: usize,
    pub chunk: Option<Tag>,
    pub reason: &'static str,
}

impl fmt::Display for M3dError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(tag) = self.chunk {
            write!(f, "{}: ", tag_name(&tag))?;
        }
        write!(f, "{} at byte {:#x}", self.reason, self.offset)
    }
}

impl std::error::Error for M3dError {}

type Result<T> = std::result::Result<T, M3dError>;

/// Renders a tag for messages; non-printable bytes become `?`.
pub fn tag_name(tag: &Tag) -> String {
    tag.iter().map(|&b| if b.is_ascii_graphic() { b as char } else { '?' }).collect()
}

/// All grid sizes of a map follow from the size shift in the magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapSize {
    /// 1 for `3DMP` (normal), 2 for `4DMP` (large), 3 for `5DMP`.
    pub shift: u8,
}

impl MapSize {
    /// Terrain vertices per row (513 for shift 1).
    pub fn vertex_width(self) -> usize {
        self.vertex_height() + 1
    }

    /// Terrain vertex rows (512 for shift 1).
    pub fn vertex_height(self) -> usize {
        256 << self.shift
    }

    /// Side length of the map in world pixels.
    pub fn world_size(self) -> i32 {
        VERTEX_SPACING * self.vertex_height() as i32
    }

    /// Side length of the area objects are placed in, in world pixels (15360 for shift 1).
    /// The terrain grid extends beyond it; the `SEA2` grid covers it plus 2 cells.
    pub fn playable_size(self) -> i32 {
        VERTEX_SPACING * (240 << self.shift)
    }

    /// Side of the pathfinding area grid (`TOP1`/`TOPW`), 64-pixel cells.
    pub fn area_grid(self) -> usize {
        128 << self.shift
    }

    /// Side of the coarse grids (`CAEP`, `RIV1`, `DXIP`), 128-pixel cells.
    pub fn coarse_grid(self) -> usize {
        64 << self.shift
    }

    /// Width and height of the `UORG` layers.
    pub fn ground_size(self) -> (usize, usize) {
        (256 << self.shift, 128 << self.shift)
    }

    fn section_bytes(self) -> usize {
        (128 << self.shift) * self.vertex_height() * 6
    }
}

/// A row-major grid of `width * height` cells.
///
/// For the per-vertex terrain layers, vertex `(x, y)` sits at world pixel
/// `(32 * x, 32 * y - (x odd ? 16 : 0))`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid<T> {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<T>,
}

impl<T: Copy> Grid<T> {
    pub fn get(&self, x: usize, y: usize) -> Option<T> {
        if x < self.width && y < self.height { self.cells.get(y * self.width + x).copied() } else { None }
    }
}

/// Sprite list an object in `TRE1`/`TREE` belongs to; the names are the list files the
/// engine loads for each group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpriteGroup {
    /// `AG`: `treelist`.
    Trees,
    /// `ST`: `stonlist`.
    Stones,
    /// `HO`: `holelist`.
    Holes,
    /// `CO`: `complex`.
    Complex,
    /// `PS`: `meat`.
    Meat,
    /// `SA`: `Animations`.
    Animations,
    /// Any other code; the loader drops these objects.
    Unknown([u8; 2]),
}

impl SpriteGroup {
    pub fn from_code(code: [u8; 2]) -> Self {
        match &code {
            b"AG" => Self::Trees,
            b"ST" => Self::Stones,
            b"HO" => Self::Holes,
            b"CO" => Self::Complex,
            b"PS" => Self::Meat,
            b"SA" => Self::Animations,
            _ => Self::Unknown(code),
        }
    }

    /// The two-letter code `from_code` reads this group from.
    pub fn code(self) -> [u8; 2] {
        match self {
            Self::Trees => *b"AG",
            Self::Stones => *b"ST",
            Self::Holes => *b"HO",
            Self::Complex => *b"CO",
            Self::Meat => *b"PS",
            Self::Animations => *b"SA",
            Self::Unknown(code) => code,
        }
    }
}

/// A static map object (tree, stone, ...): `TRE1` (14 bytes) or the older `TREE` (12 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub group: SpriteGroup,
    /// World position in pixels.
    pub x: i32,
    pub y: i32,
    /// Index into the group's sprite list.
    pub index: u16,
    /// `TRE1` only; restored into the object only in some engine states (meaning unknown).
    pub extra: Option<u16>,
}

/// A unit or building placed on the map (`UNI3`, 54 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// Owning nation (player slot); the loader skips values >= 8.
    pub nation: u8,
    /// Type index at save time; the loader ignores it and looks up `name` instead.
    pub type_index: u16,
    /// World position in 1/16 pixels.
    pub x: i32,
    pub y: i32,
    /// Hit points and construction stage at save time; ignored by the 1.46 loader.
    pub life: u16,
    pub stage: u16,
    /// Copied into two engine globals before the unit is created (meaning unknown).
    pub unknown: [i16; 2],
    /// Guess: facing direction (0..=255).
    pub direction: u8,
    /// Two engine flag bits (bit 0 and bit 1), meaning unknown.
    pub flags: u8,
    /// Unit type id, e.g. `EngMel(EN)`, matched against the nation's unit list.
    pub name: String,
}

/// A wall (`WAL2`); no shipped map contains one, so the segment fields stay raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wall {
    pub kind: u8,
    pub wall_type: u32,
    pub segments: Vec<[u8; 26]>,
}

/// `LOC1`: passability overrides on the 64-pixel area grid, as `(x, y)` cells.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locks {
    /// Cells forced impassable (nearly all lie outside every land area).
    pub locked: Vec<(u16, u16)>,
    /// Cells forced passable.
    pub unlocked: Vec<(u16, u16)>,
}

/// `SEA2`: two byte layers on a 32-pixel grid. The names are guesses from rendering:
/// `depth` is 0 on land and rises smoothly into the sea, `brightness` is 128 on land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Water {
    pub depth: Grid<u8>,
    pub brightness: Grid<u8>,
}

/// `TOP1`/`TOPW`: pathfinding areas for land and water units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Areas {
    pub areas: Vec<Area>,
    /// `areas.len()²` entries: next area on the way from area `i` (row) to area `j`, 0xFFFF = unreachable.
    pub next_hop: Vec<u16>,
    /// `areas.len()²` entries: path length between two areas, 0xFFFF = unreachable.
    pub distance: Vec<u16>,
    /// Area index of each 64-pixel cell, 0xFFFF = none.
    pub cells: Grid<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    /// Center in area-grid cells.
    pub x: u16,
    pub y: u16,
    /// The raw 85-byte record (contains stale pointers at 9 and 15).
    pub record: Vec<u8>,
    /// u16 list whose length is at record offset 7 (empty in all shipped maps).
    pub extra: Vec<u16>,
    /// Neighbouring areas: `(area, distance)`.
    pub links: Vec<(u16, u16)>,
}

/// Named circular zone used by mission scripts (`ZON1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
    pub unknown: u8,
    pub name: String,
}

/// Named unit group used by mission scripts (`ZON1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    /// The raw 49-byte record (stale pointers, member count at 8, unknown fields from 16).
    pub record: Vec<u8>,
    /// Positions (1/16 pixels) of the member units; the loader binds the nearest unit.
    pub members: Vec<(i32, i32)>,
}

/// A formation of units (`FOR1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brigade {
    pub nation: u32,
    pub index: u32,
    pub unknown_a: [u8; 2],
    pub unknown_b: [u8; 16],
    pub unknown_c: u8,
    /// Formation type name, matched against the engine's formation list.
    pub formation: String,
    pub unknown_d: [u16; 2],
    pub members: Vec<BrigadeMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrigadeMember {
    pub unknown: [u16; 2],
    /// Position (1/16 pixels) of the unit to bind, `None` for an empty slot.
    pub position: Option<(i32, i32)>,
}

/// `PSMS` record; no shipped map contains one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Psms {
    pub name: String,
    pub pairs: Vec<(u16, u16)>,
    pub records: Vec<[u8; 14]>,
}

/// `CAEP` (reversed "PEAC"): peace time and per-player settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peace {
    /// One byte per 128-pixel cell, meaning unknown.
    pub cells: Grid<u8>,
    /// Peace time; the engine also stores it divided by 60 (likely seconds -> minutes).
    pub time: u32,
    /// Guess: team (alliance) per player slot, 0xFF = unused.
    pub players: [u8; PLAYERS],
    pub flags: [u8; 3],
}

/// One `DXIP` entry: opaque data attached to a 128-pixel cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellData {
    pub cell: u32,
    pub data: Vec<u8>,
}

/// A decoded map. Every chunk is optional; unknown chunks are kept in `unknown`.
#[derive(Debug, Clone, Default)]
pub struct M3dMap {
    pub size: Option<MapSize>,
    /// The two header values (vertex width, vertex height) as stored.
    pub header: [u32; 2],
    /// Tag, payload offset and payload length of every chunk, in file order.
    pub chunks: Vec<(Tag, usize, usize)>,
    /// `VIE1`: 400×224 palette indices.
    pub preview: Option<Vec<u8>>,
    /// `VIEW`: 292×190 palette indices (older maps; not read by the 1.46 loader).
    pub old_preview: Option<Vec<u8>>,
    /// `SURF`: terrain height per vertex.
    pub heights: Option<Grid<i16>>,
    /// `TILE`: terrain texture index per vertex.
    pub textures: Option<Grid<u8>>,
    /// `SECT`: 6 bytes per (vertex row, vertex column pair), meaning unknown.
    pub sections: Option<Vec<u8>>,
    pub sprites: Vec<Sprite>,
    pub units: Vec<Unit>,
    pub walls: Vec<Wall>,
    pub locks: Option<Locks>,
    pub water: Option<Water>,
    /// `RESU`: start resources, `[player][resource]`.
    pub resources: Option<[[u32; 8]; PLAYERS]>,
    pub land_areas: Option<Areas>,
    pub water_areas: Option<Areas>,
    /// `GAT1`: capacity and 10-byte gate records; absent from all shipped maps.
    pub gates: Option<(u32, Vec<[u8; 10]>)>,
    pub zones: Vec<Zone>,
    pub groups: Vec<Group>,
    pub brigades: Vec<Brigade>,
    /// `.DLL`: mission script DLL path, empty if none.
    pub mission_dll: Option<String>,
    /// `AIIA`: three values, meaning unknown.
    pub ai: Option<[u32; 3]>,
    pub psms: Vec<Psms>,
    pub peace: Option<Peace>,
    /// `RIV1`: two byte layers on the coarse grid.
    pub rivers: Option<[Grid<u8>; 2]>,
    pub cell_data: Vec<CellData>,
    /// `UORG`: a byte layer and a u16 layer, see [`MapSize::ground_size`].
    pub ground: Option<(Grid<u8>, Grid<u16>)>,
    /// Chunks with unknown tags.
    pub unknown: Vec<(Tag, Vec<u8>)>,
}

impl M3dMap {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader { data, pos: 0, chunk: None };
        let magic: Tag = r.array()?;
        if &magic[1..] != b"DMP" || !(b'3'..=b'5').contains(&magic[0]) {
            return Err(r.fail_at(0, "missing 3DMP/4DMP/5DMP magic"));
        }
        let size = MapSize { shift: magic[0] - b'2' };
        let mut map = Self { size: Some(size), header: [r.u32()?, r.u32()?], ..Self::default() };
        while r.pos < data.len() {
            let start = r.pos;
            let tag: Tag = r.array()?;
            if &tag == END {
                break;
            }
            let len = r.u32()? as usize;
            let payload = len
                .checked_sub(4)
                .and_then(|n| data.get(r.pos..r.pos.checked_add(n)?))
                .ok_or_else(|| r.fail_at(start, "chunk length out of bounds"))?;
            map.chunks.push((tag, r.pos, payload.len()));
            let mut c = Reader { data: payload, pos: 0, chunk: Some(tag) };
            map.read_chunk(tag, size, &mut c).map_err(|e| M3dError { offset: e.offset + r.pos, ..e })?;
            let rest = &payload[c.pos..];
            if !rest.is_empty() && rest != END {
                return Err(r.fail_at(r.pos + c.pos, "chunk has unread bytes").with_chunk(tag));
            }
            r.pos += payload.len();
        }
        Ok(map)
    }

    fn read_chunk(&mut self, tag: Tag, size: MapSize, r: &mut Reader) -> Result<()> {
        let (w, h) = (size.vertex_width(), size.vertex_height());
        match &tag {
            b"VIE1" => self.preview = Some(r.take(PREVIEW_WIDTH * PREVIEW_HEIGHT)?.to_vec()),
            b"VIEW" => self.old_preview = Some(r.take(OLD_PREVIEW_WIDTH * OLD_PREVIEW_HEIGHT)?.to_vec()),
            b"SURF" => {
                let cells = r.take(w * h * 2)?.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
                self.heights = Some(Grid { width: w, height: h, cells });
            }
            b"TILE" => self.textures = Some(Grid { width: w, height: h, cells: r.take(w * h)?.to_vec() }),
            b"SECT" => self.sections = Some(r.take(size.section_bytes())?.to_vec()),
            b"TRE1" | b"TREE" => {
                let with_extra = &tag == b"TRE1";
                let n = r.count(if with_extra { 14 } else { 12 })?;
                for _ in 0..n {
                    let group = SpriteGroup::from_code(r.array()?);
                    let (x, y, index) = (r.i32()?, r.i32()?, r.u16()?);
                    let extra = if with_extra { Some(r.u16()?) } else { None };
                    self.sprites.push(Sprite { group, x, y, index, extra });
                }
            }
            b"UNI3" => {
                for _ in 0..r.count(54)? {
                    self.units.push(Unit {
                        nation: r.u8()?,
                        type_index: r.u16()?,
                        x: r.i32()?,
                        y: r.i32()?,
                        life: r.u16()?,
                        stage: r.u16()?,
                        unknown: [r.i16()?, r.i16()?],
                        direction: r.u8()?,
                        flags: r.u8()?,
                        name: r.string(33)?,
                    });
                }
            }
            b"WAL2" => {
                for _ in 0..r.count(9)? {
                    let kind = r.u8()?;
                    let n = r.count(26)?;
                    let wall_type = r.u32()?;
                    let segments = (0..n).map(|_| r.array()).collect::<Result<_>>()?;
                    self.walls.push(Wall { kind, wall_type, segments });
                }
            }
            b"LOC1" => {
                let (a, b) = (r.u32()? as usize, r.u32()? as usize);
                if a.saturating_add(b) > r.remaining() / 4 {
                    return Err(r.fail("lock counts exceed chunk"));
                }
                let mut list = |n| (0..n).map(|_| Ok((r.u16()?, r.u16()?))).collect::<Result<Vec<_>>>();
                self.locks = Some(Locks { locked: list(a)?, unlocked: list(b)? });
            }
            b"SEA2" => {
                let (w, h) = (r.u32()? as usize, r.u32()? as usize);
                if w.checked_mul(h).and_then(|n| n.checked_mul(2)) != Some(r.remaining()) {
                    return Err(r.fail("water size does not match chunk"));
                }
                let (mut a, mut b) = (Vec::with_capacity(w * h), Vec::with_capacity(w * h));
                for _ in 0..h {
                    a.extend_from_slice(r.take(w)?);
                    b.extend_from_slice(r.take(w)?);
                }
                let (depth, brightness) =
                    (Grid { width: w, height: h, cells: a }, Grid { width: w, height: h, cells: b });
                self.water = Some(Water { depth, brightness });
            }
            b"RESU" => {
                let mut res = [[0; 8]; PLAYERS];
                for v in res.iter_mut().flatten() {
                    *v = r.u32()?;
                }
                self.resources = Some(res);
            }
            b"TOP1" => self.land_areas = Some(read_areas(r, size)?),
            b"TOPW" => self.water_areas = Some(read_areas(r, size)?),
            b"GAT1" => {
                let n = r.count(10)?;
                let capacity = r.u32()?;
                self.gates = Some((capacity, (0..n).map(|_| r.array()).collect::<Result<_>>()?));
            }
            b"ZON1" => {
                let (zones, groups) = (r.count(18)?, r.count(50)?);
                for _ in 0..zones {
                    let (x, y, radius) = (r.i32()?, r.i32()?, r.i32()?);
                    r.take(4)?;
                    let unknown = r.u8()?;
                    let name = r.pstring()?;
                    self.zones.push(Zone { x, y, radius, unknown, name });
                }
                for _ in 0..groups {
                    let record = r.take(49)?.to_vec();
                    let name = r.pstring()?;
                    let n = u32::from_le_bytes(record[8..12].try_into().unwrap()) as usize;
                    if n > r.remaining() / 8 {
                        return Err(r.fail("group member count exceeds chunk"));
                    }
                    let members = (0..n).map(|_| Ok((r.i32()?, r.i32()?))).collect::<Result<_>>()?;
                    self.groups.push(Group { name, record, members });
                }
            }
            b"FOR1" => {
                for _ in 0..r.count(65)? {
                    let (nation, index) = (r.u32()?, r.u32()?);
                    let unknown_a = r.array()?;
                    let unknown_b = r.array()?;
                    let unknown_c = r.u8()?;
                    let n = usize::from(r.u16()?);
                    let formation = r.string(32)?;
                    let unknown_d = [r.u16()?, r.u16()?];
                    let first = (0..n).map(|_| r.u16()).collect::<Result<Vec<_>>>()?;
                    let second = (0..n).map(|_| r.u16()).collect::<Result<Vec<_>>>()?;
                    let mut members = Vec::with_capacity(n);
                    for (a, b) in first.into_iter().zip(second) {
                        let (x, y) = (r.i32()?, r.i32()?);
                        members.push(BrigadeMember { unknown: [a, b], position: (x != -1).then_some((x, y)) });
                    }
                    let b = Brigade { nation, index, unknown_a, unknown_b, unknown_c, formation, unknown_d, members };
                    self.brigades.push(b);
                }
            }
            b".DLL" => self.mission_dll = Some(r.string(200)?),
            b"AIIA" => self.ai = Some([r.u32()?, r.u32()?, r.u32()?]),
            b"PSMS" => {
                for _ in 0..r.count(40)? {
                    let name = r.string(32)?;
                    let (n, m) = (r.count(4)?, r.count(14)?);
                    let pairs = (0..n).map(|_| Ok((r.u16()?, r.u16()?))).collect::<Result<_>>()?;
                    let records = (0..m).map(|_| r.array()).collect::<Result<_>>()?;
                    self.psms.push(Psms { name, pairs, records });
                }
            }
            b"CAEP" => {
                let side = size.coarse_grid();
                let cells = Grid { width: side, height: side, cells: r.take(side * side)?.to_vec() };
                self.peace = Some(Peace { cells, time: r.u32()?, players: r.array()?, flags: r.array()? });
            }
            b"RIV1" => {
                let side = size.coarse_grid();
                let mut layer = || Ok(Grid { width: side, height: side, cells: r.take(side * side)?.to_vec() });
                self.rivers = Some([layer()?, layer()?]);
            }
            b"DXIP" => {
                let len = r.u32()? as usize;
                if len != r.remaining() {
                    return Err(r.fail("cell data length does not match chunk"));
                }
                for _ in 0..r.count(6)? {
                    let cell = r.u32()?;
                    let n = usize::from(r.u16()?);
                    self.cell_data.push(CellData { cell, data: r.take(n)?.to_vec() });
                }
            }
            b"UORG" => {
                let (w, h) = size.ground_size();
                let bytes = Grid { width: w, height: h, cells: r.take(w * h)?.to_vec() };
                let words = r.take(w * h * 2)?.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
                self.ground = Some((bytes, Grid { width: w, height: h, cells: words }));
            }
            _ => self.unknown.push((tag, r.take(r.remaining())?.to_vec())),
        }
        Ok(())
    }
}

fn read_areas(r: &mut Reader, size: MapSize) -> Result<Areas> {
    let n = r.count(85)?;
    let mut areas = Vec::with_capacity(n);
    for _ in 0..n {
        let record = r.take(85)?;
        let field = |i: usize| u16::from_le_bytes([record[i], record[i + 1]]);
        let extra = (0..field(7)).map(|_| r.u16()).collect::<Result<_>>()?;
        let links = (0..field(13)).map(|_| Ok((r.u16()?, r.u16()?))).collect::<Result<_>>()?;
        areas.push(Area { x: field(0), y: field(2), record: record.to_vec(), extra, links });
    }
    let mut matrix = || r.u16s(n.checked_mul(n).ok_or_else(|| r.fail("area count too large"))?);
    let (next_hop, distance) = (matrix()?, matrix()?);
    let side = size.area_grid();
    Ok(Areas { areas, next_hop, distance, cells: Grid { width: side, height: side, cells: r.u16s(side * side)? } })
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    chunk: Option<Tag>,
}

impl<'a> Reader<'a> {
    fn fail_at(&self, offset: usize, reason: &'static str) -> M3dError {
        M3dError { offset, chunk: self.chunk, reason }
    }

    fn fail(&self, reason: &'static str) -> M3dError {
        self.fail_at(self.pos, reason)
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let bytes =
            self.data.get(self.pos..self.pos.saturating_add(n)).ok_or_else(|| self.fail("unexpected end of data"))?;
        self.pos += n;
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        self.array().map(u16::from_le_bytes)
    }

    fn i16(&mut self) -> Result<i16> {
        self.array().map(i16::from_le_bytes)
    }

    fn u32(&mut self) -> Result<u32> {
        self.array().map(u32::from_le_bytes)
    }

    fn i32(&mut self) -> Result<i32> {
        self.array().map(i32::from_le_bytes)
    }

    fn u16s(&mut self, n: usize) -> Result<Vec<u16>> {
        let bytes = self.take(n.checked_mul(2).ok_or_else(|| self.fail("count too large"))?)?;
        Ok(bytes.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect())
    }

    /// Reads a u32 element count and rejects it if `min_size`-byte elements cannot fit.
    fn count(&mut self, min_size: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n > self.remaining() / min_size {
            return Err(self.fail_at(self.pos - 4, "element count exceeds chunk"));
        }
        Ok(n)
    }

    /// A fixed-size, NUL-padded Latin-1 string.
    fn string(&mut self, len: usize) -> Result<String> {
        Ok(latin1(self.take(len)?))
    }

    /// A u8 length (including the NUL) followed by the string.
    fn pstring(&mut self) -> Result<String> {
        let n = usize::from(self.u8()?);
        self.string(n)
    }
}

impl M3dError {
    fn with_chunk(self, tag: Tag) -> Self {
        Self { chunk: Some(tag), ..self }
    }
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &Tag, payload: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend_from_slice(&(payload.len() as u32 + 4).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn a_sprite_group_code_reads_back() {
        for code in [*b"AG", *b"ST", *b"HO", *b"CO", *b"PS", *b"SA", *b"ZZ"] {
            assert_eq!(SpriteGroup::from_code(code).code(), code);
        }
    }

    fn header() -> Vec<u8> {
        [b"3DMP".as_slice(), &513u32.to_le_bytes(), &512u32.to_le_bytes()].concat()
    }

    #[test]
    fn parses_header_and_sizes() {
        let map = M3dMap::parse(&header()).unwrap();
        let size = map.size.unwrap();
        assert_eq!((size.shift, size.vertex_width(), size.vertex_height()), (1, 513, 512));
        assert_eq!((size.world_size(), size.playable_size()), (16384, 15360));
        assert_eq!((size.area_grid(), size.coarse_grid(), size.ground_size()), (256, 128, (512, 256)));
        assert_eq!(map.header, [513, 512]);
        assert!(M3dMap::parse(b"2DMP\0\0\0\0\0\0\0\0").is_err());
        assert!(M3dMap::parse(b"3DM").is_err());
    }

    #[test]
    fn reads_sprites_and_units() {
        let mut tre = 2u32.to_le_bytes().to_vec();
        tre.extend_from_slice(&[b'A', b'G', 10, 0, 0, 0, 20, 0, 0, 0, 3, 0, 7, 0]);
        tre.extend_from_slice(&[b'S', b'T', 1, 0, 0, 0, 2, 0, 0, 0, 4, 0, 0, 0]);
        let mut uni = 1u32.to_le_bytes().to_vec();
        uni.extend_from_slice(&[2, 5, 0]);
        uni.extend_from_slice(&160i32.to_le_bytes());
        uni.extend_from_slice(&320i32.to_le_bytes());
        uni.extend_from_slice(&[0x88, 0x13, 100, 0, 0xFF, 0xFF, 1, 0, 64, 3]);
        let mut name = [0u8; 33];
        name[..10].copy_from_slice(b"EngMel(EN)");
        name[20] = b'x';
        uni.extend_from_slice(&name);
        let data = [header(), chunk(b"TRE1", &tre), chunk(b"UNI3", &uni), b"ENDM".to_vec()].concat();
        let map = M3dMap::parse(&data).unwrap();
        assert_eq!(map.sprites.len(), 2);
        assert_eq!(map.sprites[0], Sprite { group: SpriteGroup::Trees, x: 10, y: 20, index: 3, extra: Some(7) });
        assert_eq!(map.sprites[1].group, SpriteGroup::Stones);
        let u = &map.units[0];
        assert_eq!((u.nation, u.type_index, u.x, u.y, u.life, u.stage), (2, 5, 160, 320, 5000, 100));
        assert_eq!((u.unknown, u.direction, u.flags, u.name.as_str()), ([-1, 1], 64, 3, "EngMel(EN)"));
        assert_eq!(map.chunks.iter().map(|c| c.0).collect::<Vec<_>>(), [*b"TRE1", *b"UNI3"]);
    }

    #[test]
    fn last_chunk_may_swallow_end_marker() {
        let mut data = header();
        data.extend_from_slice(b"AIIA");
        data.extend_from_slice(&20u32.to_le_bytes());
        data.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0]);
        data.extend_from_slice(b"ENDM");
        assert_eq!(M3dMap::parse(&data).unwrap().ai, Some([1, 2, 3]));
    }

    #[test]
    fn keeps_unknown_chunks_and_rejects_leftovers() {
        let data = [header(), chunk(b"XXXX", &[1, 2, 3])].concat();
        assert_eq!(M3dMap::parse(&data).unwrap().unknown, vec![(*b"XXXX", vec![1, 2, 3])]);
        let data = [header(), chunk(b"AIIA", &[0; 13])].concat();
        let e = M3dMap::parse(&data).unwrap_err();
        assert_eq!((e.chunk, e.reason), (Some(*b"AIIA"), "chunk has unread bytes"));
    }

    #[test]
    fn rejects_bad_lengths_and_counts() {
        let data = [header(), b"SURF".to_vec(), 1000u32.to_le_bytes().to_vec()].concat();
        assert_eq!(M3dMap::parse(&data).unwrap_err().reason, "chunk length out of bounds");
        let data = [header(), chunk(b"UNI3", &u32::MAX.to_le_bytes())].concat();
        assert_eq!(M3dMap::parse(&data).unwrap_err().reason, "element count exceeds chunk");
        let data = [header(), chunk(b"SURF", &[0; 10])].concat();
        assert_eq!(M3dMap::parse(&data).unwrap_err().reason, "unexpected end of data");
    }

    #[test]
    fn reads_zones_groups_and_water() {
        let mut zon = [1u32.to_le_bytes(), 1u32.to_le_bytes()].concat();
        for v in [100i32, 200, 50, 0] {
            zon.extend_from_slice(&v.to_le_bytes());
        }
        zon.extend_from_slice(&[9, 3, b'Z', b'1', 0]);
        let mut record = [0u8; 49];
        record[8] = 1;
        zon.extend_from_slice(&record);
        zon.extend_from_slice(&[3, b'G', b'1', 0]);
        zon.extend_from_slice(&[16i32.to_le_bytes(), 32i32.to_le_bytes()].concat());
        let sea = [2u32.to_le_bytes().as_slice(), &1u32.to_le_bytes(), &[1, 2, 3, 4]].concat();
        let data = [header(), chunk(b"ZON1", &zon), chunk(b"SEA2", &sea)].concat();
        let map = M3dMap::parse(&data).unwrap();
        assert_eq!(map.zones, vec![Zone { x: 100, y: 200, radius: 50, unknown: 9, name: "Z1".into() }]);
        assert_eq!((map.groups[0].name.as_str(), map.groups[0].members.as_slice()), ("G1", [(16, 32)].as_slice()));
        let water = map.water.unwrap();
        assert_eq!((water.depth.cells, water.brightness.cells), (vec![1, 2], vec![3, 4]));
    }

    #[test]
    fn grid_is_row_major() {
        let g = Grid { width: 3, height: 2, cells: vec![0, 1, 2, 10, 11, 12] };
        assert_eq!((g.get(2, 0), g.get(0, 1), g.get(3, 0), g.get(0, 2)), (Some(2), Some(10), None, None));
    }

    /// A small map with one non-trivial instance of most chunk types (`TOP1` only if `areas`).
    fn sample(areas: bool) -> Vec<u8> {
        let le = |v: &[u32]| v.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
        let mut area = vec![0u8; 85];
        area[13] = 1;
        let mut top = [le(&[1]), area, le(&[0x0005_0000])].concat();
        top.extend(vec![0; 4 + 256 * 256 * 2]);
        let mut brigade = [le(&[1, 0, 0]), vec![0; 19], vec![2, 0], vec![0; 36]].concat();
        brigade.extend(le(&[0x0001_0002, 0x0003_0004, 16, 32, u32::MAX, 0]));
        let cell = [le(&[1, 7]), vec![2, 0, 9, 9]].concat();
        [
            header(),
            chunk(b"LOC1", &le(&[1, 1, 0x0002_0001, 0x0004_0003])),
            chunk(b"RESU", &le(&[50_000; 64])),
            if areas { chunk(b"TOP1", &top) } else { Vec::new() },
            chunk(b"FOR1", &brigade),
            chunk(b".DLL", &[b'x'; 200]),
            chunk(b"DXIP", &[le(&[cell.len() as u32]), cell].concat()),
            chunk(b"WAL2", &[le(&[1]), vec![5], le(&[1, 3]), vec![1; 26]].concat()),
            chunk(b"GAT1", &[le(&[1, 4]), vec![2; 10]].concat()),
            chunk(b"PSMS", &[le(&[1]), vec![b'p'; 32], le(&[1, 1, 0x0006_0005]), vec![3; 14]].concat()),
        ]
        .concat()
    }

    #[test]
    fn reads_every_small_chunk() {
        let map = M3dMap::parse(&sample(true)).unwrap();
        assert_eq!(map.locks, Some(Locks { locked: vec![(1, 2)], unlocked: vec![(3, 4)] }));
        assert_eq!(map.resources.unwrap()[7][7], 50_000);
        let areas = map.land_areas.unwrap();
        assert_eq!((areas.areas[0].links.as_slice(), areas.cells.width), ([(0, 5)].as_slice(), 256));
        let b = &map.brigades[0];
        assert_eq!(b.members[0], BrigadeMember { unknown: [2, 4], position: Some((16, 32)) });
        assert_eq!((b.members[1].unknown, b.members[1].position), ([1, 3], None));
        assert_eq!((map.mission_dll.unwrap().len(), map.cell_data[0].cell, map.cell_data[0].data.len()), (200, 7, 2));
        assert_eq!((map.walls[0].kind, map.walls[0].wall_type, map.walls[0].segments.len()), (5, 3, 1));
        assert_eq!(map.gates, Some((4, vec![[2; 10]])));
        assert_eq!((map.psms[0].name.len(), map.psms[0].pairs[0], map.psms[0].records.len()), (32, (5, 6), 1));
    }

    #[test]
    fn never_panics_on_truncated_or_corrupt_input() {
        let data = sample(false);
        for len in 0..data.len() {
            let _ = M3dMap::parse(&data[..len]);
        }
        for i in 12..data.len() {
            let mut bad = data.clone();
            bad[i] ^= 0xA5;
            let _ = M3dMap::parse(&bad);
        }
    }
}
