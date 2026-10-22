//! Map terrain: the ground mesh, its textures, light and water, as the original draws them.
//!
//! The rules come from the software renderer of `dmcr.exe` 1.46 (see
//! `docs/formats/terrain.md`): a triangle mesh with odd columns shifted half a cell up,
//! projected as `screen = (x, y / 2 - height - lift)`; 64 × 64 textures mapped in a
//! rotated texture space; blended triangles composed from up to three textures through
//! the transition masks `MS*.MSK` and the blend tables `AGEW_TR*.GRD`; per-vertex light
//! from the height slopes through the darkness table `AGEW_1DR.GRD`; water tinted by
//! depth through `WCOST.GRD`. Everything stays in palette indices until the last pass.
//!
//! The zoomed-out view (key L) draws the ground with its own rules (`MiniMap4X.cpp`,
//! 0x5fdddd): 8 × 8-pixel triangles that take unscaled texture patches at random offsets,
//! blend the textures of the three vertices by per-vertex random weights, shade by the
//! vertex light, and drop every other row; see [`TerrainRenderer::zoomed`].
//!
//! [`TerrainData`] prepares a map on the CPU; [`TerrainRenderer`] draws it.

use acr_formats::Palette;
use acr_formats::grd::BlendTable;
use acr_formats::m3d::M3dMap;
use acr_formats::terrain::{MaskTable, RandomTable, TILE_SIZE, TextureFlags, TileSet};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::model_renderer::DEPTH_FORMAT;
use crate::shadow::ShadowMap;
use crate::view3d::{COS_E, DEPTH_RANGE, Orbit, View3d};

/// Light vector set at game start (`SetLight(-15, 14, 30)`, 0x46993B).
const LIGHT: [i32; 3] = [-15, 14, 30];
/// Darkness levels used for terrain: rows 64..96 of `0\AGEW_1DR.GRD` (table at 0x7AFB28).
const SHADE_FIRST_ROW: usize = 64;
const SHADE_ROWS: usize = 32;
/// Remap applied to the light level (table at 0x644470).
const LIGHT_CURVE: [i32; 33] = [
    0, 1, 1, 3, 5, 7, 9, 11, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 0,
];
/// Layered masks per file and hard masks (`MS.MSK`).
const MASKS: usize = 108;

/// The game files the terrain needs, independent of the map.
pub struct GroundAssets {
    tiles: TileSet,
    flags: TextureFlags,
    layered: [MaskTable; 4],
    hard: MaskTable,
    random: RandomTable,
    blend50: BlendTable,
    blend75: BlendTable,
    shade: BlendTable,
    wcost: BlendTable,
    pub palette: Palette,
}

impl GroundAssets {
    /// Game paths, in the order [`GroundAssets::parse`] expects their contents.
    pub const FILES: [&'static str; 13] = [
        "TILES3.BMP",
        "TEXTURES.LST",
        "MS_A.MSK",
        "MS_B.MSK",
        "MS_C.MSK",
        "MS_D.MSK",
        "MS.MSK",
        "RANDOM.LST",
        "0\\AGEW_TR.GRD",
        "0\\AGEW_TR4.GRD",
        "0\\AGEW_1DR.GRD",
        "WCOST.GRD",
        "0\\AGEW_1.PAL",
    ];

    pub fn parse(files: &[Vec<u8>]) -> Result<Self, String> {
        let file = |i: usize| files.get(i).map(Vec::as_slice).ok_or_else(|| format!("{} missing", Self::FILES[i]));
        let bad = |i: usize| format!("{}: invalid", Self::FILES[i]);
        let mask = |i: usize| MaskTable::parse(file(i)?).filter(|m| m.0.len() >= MASKS).ok_or_else(|| bad(i));
        let table =
            |i: usize, rows: usize| BlendTable::parse(file(i)?).filter(|t| t.rows() >= rows).ok_or_else(|| bad(i));
        Ok(Self {
            tiles: TileSet::parse(file(0)?).ok_or_else(|| bad(0))?,
            flags: TextureFlags::parse(&String::from_utf8_lossy(file(1)?)),
            layered: [mask(2)?, mask(3)?, mask(4)?, mask(5)?],
            hard: mask(6)?,
            random: RandomTable::parse(file(7)?).ok_or_else(|| bad(7))?,
            blend50: table(8, 256)?,
            blend75: table(9, 256)?,
            shade: table(10, SHADE_FIRST_ROW + SHADE_ROWS)?,
            wcost: table(11, 256)?,
            palette: Palette::parse(file(12)?).ok_or_else(|| bad(12))?,
        })
    }

    /// Colour of texture `tile` on the minimap at light level `light` (0..32): the first
    /// texel of the texture (table 0x6d4658, filled at 0x446fd4) darkened by the terrain
    /// shading rows of `AGEW_1DR.GRD` (0x44c2ff).
    pub fn minimap_color(&self, tile: u8, light: u8) -> u8 {
        let row = SHADE_FIRST_ROW + usize::from(light).min(SHADE_ROWS - 1);
        self.shade.lookup(row as u8, self.tiles.texel(tile, 0, 0))
    }

    /// Per darkness row of `0\AGEW_1DR.GRD`, the opacity of black it amounts to with the
    /// terrain palette (rows past the table are black), for the fog overlay.
    pub fn fog_darkness(&self) -> [f32; crate::fog::ROWS] {
        let fitted = self.shade.darkness(&self.palette);
        std::array::from_fn(|row| fitted.get(row).map_or(1.0, |&d| d as f32))
    }

    /// The detailed ground's tables (`ground.wgsl`): the brightness of each light level
    /// relative to the plain texel (linear luminance over the textures' texels), and per
    /// texture its mean colour (linear RGB) and material class.
    fn detail_tables(&self) -> ([f32; 32], Vec<[f32; 4]>) {
        let lin = |c: u8| {
            let [r, g, b] = self.palette.0[usize::from(c)];
            [r, g, b].map(|v| crate::ground::srgb_to_linear(f32::from(v) / 255.0))
        };
        let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let count = crate::ground::TILE_CLASS.len() as u8;
        let mut shade = [0.0; 32];
        for (level, out) in shade.iter_mut().enumerate() {
            let row = (SHADE_FIRST_ROW + level) as u8;
            let (mut lit, mut plain) = (0.0, 0.0);
            for t in 0..count {
                for i in (0..TILE_SIZE * TILE_SIZE).step_by(7) {
                    let c = self.tiles.texel(t, i % TILE_SIZE, i / TILE_SIZE);
                    lit += lum(lin(self.shade.lookup(row, c)));
                    plain += lum(lin(c));
                }
            }
            *out = if plain > 0.0 { lit / plain } else { 1.0 };
        }
        let tiles = (0..=255u8)
            .map(|t| {
                let mut sum = [0.0f32; 3];
                for i in 0..TILE_SIZE * TILE_SIZE {
                    let c = lin(self.tiles.texel(t, i % TILE_SIZE, i / TILE_SIZE));
                    (0..3).for_each(|k| sum[k] += c[k]);
                }
                let n = (TILE_SIZE * TILE_SIZE) as f32;
                [sum[0] / n, sum[1] / n, sum[2] / n, f32::from(crate::ground::class_of(t))]
            })
            .collect();
        (shade, tiles)
    }

    /// 256 textures of 64 × 64, 8 per row (indices past the file read texture 0).
    fn tile_atlas(&self) -> Vec<u8> {
        let mut out = vec![0; 512 * 2048];
        for t in 0..=255u8 {
            let (ox, oy) = (usize::from(t % 8) * TILE_SIZE, usize::from(t / 8) * TILE_SIZE);
            for y in 0..TILE_SIZE {
                for x in 0..TILE_SIZE {
                    out[(oy + y) * 512 + ox + x] = self.tiles.texel(t, x, y);
                }
            }
        }
        out
    }

    /// 216 masks of 64 × 32, 8 per row: layered masks with one bit per file (A, B, C, D),
    /// then the hard masks.
    fn mask_atlas(&self) -> Vec<u8> {
        let mut out = vec![0; 512 * 27 * 32];
        let mut paint = |slot: usize, mask: &acr_formats::terrain::Mask, bit: u8| {
            let (ox, oy) = ((slot % 8) * 64, (slot / 8) * 32);
            for (y, runs) in mask.rows.iter().enumerate().take(32) {
                for &(x, len) in runs {
                    for x in usize::from(x)..(usize::from(x) + usize::from(len)).min(64) {
                        out[(oy + y) * 512 + ox + x] |= bit;
                    }
                }
            }
        };
        for (file, table) in self.layered.iter().enumerate() {
            for (i, mask) in table.0.iter().take(MASKS).enumerate() {
                paint(i, mask, 1 << file);
            }
        }
        for (i, mask) in self.hard.0.iter().take(MASKS).enumerate() {
            paint(MASKS + i, mask, 1);
        }
        out
    }

    /// Rows 0..256 `AGEW_TR`, 256..512 `AGEW_TR4`, 512..544 darkness, 544..800 `WCOST`.
    fn tables(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 * 800);
        let rows = |t: &BlendTable, range: std::ops::Range<usize>, out: &mut Vec<u8>| {
            for row in range {
                out.extend((0..=255u8).map(|c| t.lookup(row as u8, c)));
            }
        };
        rows(&self.blend50, 0..256, &mut out);
        rows(&self.blend75, 0..256, &mut out);
        rows(&self.shade, SHADE_FIRST_ROW..SHADE_FIRST_ROW + SHADE_ROWS, &mut out);
        rows(&self.wcost, 0..256, &mut out);
        out
    }
}

/// A map's terrain, prepared for drawing and for placing objects on it.
pub struct TerrainData {
    /// Vertices per row and vertex rows.
    pub width: usize,
    pub height: usize,
    heights: Vec<i16>,
    /// Per vertex: texture, light level, random lift, zoomed-view blend weight.
    vertices: Vec<[u8; 4]>,
    /// Per cell (column pair, row): transition variants S0..S6, 0.
    sections: Vec<[u8; 8]>,
    /// Per cell and zoomed-view triangle: base texture and two overlays ([`zoom_layers`]), 0.
    zoom_layers: Vec<[u8; 4]>,
    water: Option<(usize, usize, Vec<u8>)>,
    /// Lowest and highest vertex height.
    pub height_range: (i32, i32),
    /// Edge of the square that clamps height lookups, in world pixels.
    playable: i32,
}

impl TerrainData {
    pub fn new(map: &M3dMap, assets: &GroundAssets) -> Result<Self, String> {
        let size = map.size.ok_or("map without size")?;
        let heights = map.heights.as_ref().ok_or("map without SURF")?;
        let textures = map.textures.as_ref().ok_or("map without TILE")?;
        let (width, height) = (heights.width, heights.height);
        let cells_per_row = (width - 1) / 2;
        let lift = |v: usize, tile: u8| {
            if assets.flags.has(tile, TextureFlags::PLAIN) { 0 } else { (assets.random.get(v % 0x1FC5) & 7) as u8 }
        };
        let vertices = (0..width * height)
            .map(|v| {
                let tile = textures.cells[v];
                let light = if assets.flags.has(tile, TextureFlags::BRIGHT) {
                    0
                } else {
                    vertex_light(&heights.cells, width, v)
                };
                [tile, light, lift(v, tile), zoom_weight(&assets.random, v)]
            })
            .collect();
        // Without SECT the engine picks variants from the random table (0x44864B).
        let section = |i: usize| match &map.sections {
            Some(s) => s.get(i).copied().unwrap_or(0),
            None => (assets.random.get(i) % 3) as u8,
        };
        let sections = (0..cells_per_row * height)
            .map(|cell| std::array::from_fn(|k| if k < 7 { section(cell * 6 + k) } else { 0 }))
            .collect();
        let tile = |v: usize| textures.cells.get(v).copied().unwrap_or(0);
        let zoom_layers = (0..cells_per_row * height)
            .flat_map(|cell| {
                let v = cell / cells_per_row * width + cell % cells_per_row * 2;
                ZOOM_TRIANGLES.map(|t| {
                    let [base, a, b] = zoom_layers(t.vertices.map(|(dx, dy)| tile(v + dx + dy * width)));
                    [base, a, b, 0]
                })
            })
            .collect();
        let water = map.water.as_ref().map(|w| (w.depth.width, w.depth.height, w.depth.cells.clone()));
        let range =
            heights.cells.iter().fold((i32::MAX, i32::MIN), |(lo, hi), &h| (lo.min(h.into()), hi.max(h.into())));
        Ok(Self {
            width,
            height,
            heights: heights.cells.clone(),
            vertices,
            sections,
            zoom_layers,
            water,
            height_range: range,
            playable: size.playable_size(),
        })
    }

    fn h(&self, index: usize) -> i32 {
        self.heights.get(index).copied().map_or(0, i32::from)
    }

    /// The `SEA2` depth grid (32 px cells): width, height and the bytes (water above 127).
    pub fn water(&self) -> Option<(usize, usize, &[u8])> {
        self.water.as_ref().map(|(w, h, cells)| (*w, *h, &cells[..]))
    }

    /// Ground height at world pixel `(x, y)`: the plane through the triangle below
    /// (`GetHeight`, 0x4D0530, ported with its integer rounding).
    pub fn height_at(&self, x: i32, y: i32) -> i32 {
        let w = self.width;
        let x = x.clamp(0, self.playable);
        let y = y.clamp(32, self.playable);
        let col = (x >> 5) as usize;
        let at = |row: usize, c: usize| self.h(row * w + c);
        if col & 1 == 1 {
            let fx = x & 31;
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
            let fx = x & 31;
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
}

/// Integer `div` as the engine's helper (0x6153B4): truncates toward zero.
fn div(a: i32, b: i32) -> i32 {
    if b == 0 { 0 } else { a / b }
}

/// The light vector scaled to length 256 (0x4693C0).
fn light_vector() -> [i32; 3] {
    let [x, y, z] = LIGHT;
    let len = f64::from(x * x + y * y + z * z).sqrt() as i32;
    [div(x << 8, len), div(y << 8, len), div(z << 8, len)]
}

/// Light level 1..=31 of vertex `v` from the slopes to its neighbours (0x446AC0); higher is
/// darker. The engine picks the neighbours by the parity of the vertex *index*, which with
/// 513 vertices per row alternates between rows; that quirk is kept.
pub fn vertex_light(heights: &[i16], width: usize, v: usize) -> u8 {
    let h = |i: isize| -> i32 { if i < 0 { 0 } else { heights.get(i as usize).copied().map_or(0, i32::from) } };
    let (v, w) = (v as isize, width as isize);
    let down = h(v + w);
    let up = h(v - w);
    let (n10, n14, n1c, n24) = if v & 1 == 1 {
        (h(v + w + 1), h(v + 1), h(v - 1), h(v + w - 1))
    } else {
        (h(v + 1), h(v - w + 1), h(v - w - 1), h(v - 1))
    };
    let dy = up - down;
    let dx = (n10 - n1c + n14 - n24) >> 1;
    let dz = 64;
    let len = f64::from(dx * dx + dy * dy + dz * dz).sqrt() as i32 + 1;
    let [lx, ly, lz] = light_vector();
    let q = div(dx * lx + dy * ly + dz * lz, len);
    let b = ((q >> 4) + 16).clamp(1, 31);
    let b = (32 - LIGHT_CURVE[(32 - b) as usize]).clamp(1, 31);
    (32 - b) as u8
}

/// Blend weight of vertex `v` in the zoomed-out view: `RANDOM.LST[v & 0x1fff] & 0x7f`
/// (0x5fa44a). A texture weighs `255 − weight` at its own vertices and `weight` at the
/// others.
pub fn zoom_weight(random: &RandomTable, v: usize) -> u8 {
    (random.get(v) & 0x7F) as u8
}

/// One triangle of a cell in the zoomed-out view (0x5fdddd).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoomTriangle {
    /// Vertices P1, P2, P3 as `(column, row)` offsets from the cell's top-left vertex.
    pub vertices: [(usize, usize); 3],
    /// The vertex whose light is used at each corner.
    pub lights: [(usize, usize); 3],
    /// Points left (◁, drawn by 0x5fa936) instead of right (▷, 0x5fa3cc).
    pub left: bool,
}

/// The four triangles of a cell in drawing order (0x5fe443..0x5fe5f2). P1 and P2 form
/// the vertical edge, P3 is the apex. The fourth call passes the light of `v + W + 1` for
/// P2 instead of its own; that is kept (`vs_zoom` in `terrain.wgsl` has the same table).
pub const ZOOM_TRIANGLES: [ZoomTriangle; 4] = [
    ZoomTriangle { vertices: [(0, 0), (0, 1), (1, 1)], lights: [(0, 0), (0, 1), (1, 1)], left: false },
    ZoomTriangle { vertices: [(1, 0), (1, 1), (2, 0)], lights: [(1, 0), (1, 1), (2, 0)], left: false },
    ZoomTriangle { vertices: [(1, 0), (1, 1), (0, 0)], lights: [(1, 0), (1, 1), (0, 0)], left: true },
    ZoomTriangle { vertices: [(2, 0), (2, 1), (1, 1)], lights: [(2, 0), (1, 1), (1, 1)], left: true },
];

/// Textures a zoomed-view triangle with vertex textures `[a, b, c]` (P1, P2, P3) draws
/// (0x5fa3cc, 0x5fa936): the smallest index as base, then up to two overlays in the
/// original's order. An overlay equal to the base is absent.
pub fn zoom_layers([a, b, c]: [u8; 3]) -> [u8; 3] {
    let (base, first, second) = if a <= b && a <= c {
        (a, b, c)
    } else if b <= c && b <= a {
        (b, a, c)
    } else {
        (c, b, a)
    };
    let second = if second == first { base } else { second };
    [base, first, second]
}

/// What part of the map to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Top-left corner in original screen pixels: `x` = world x, `y` = world y / 2.
    pub x: f32,
    pub y: f32,
    /// Output pixels per original pixel.
    pub scale: f32,
}

impl Camera {
    /// Output position of world point `(x, y)` at ground height `h`.
    pub fn project(&self, x: i32, y: i32, h: i32) -> [f32; 2] {
        [(x as f32 - self.x) * self.scale, (((y >> 1) - h) as f32 - self.y) * self.scale]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    camera: [f32; 4],
    sizes: [f32; 4],
    cells: [i32; 4],
    map: [i32; 4],
    view: crate::math::Mat4,
    mode: [f32; 4],
}

/// How far the depth pass pushes the terrain back (world pixels along the depth axis): the
/// drawn ground is lifted by up to 7 pixels (`lift`), the depth uses the plain height.
const DEPTH_BIAS: f32 = 24.0;

struct MapTextures {
    group: wgpu::BindGroup,
    /// The per-vertex texture (tile, light, lift, zoom weight) and its data from the map,
    /// rewritten with darker light levels where the soil is trampled.
    vertices: wgpu::Texture,
    base: Vec<[u8; 4]>,
    width: usize,
    height: usize,
    water: (usize, usize),
    height_range: (i32, i32),
}

/// The detailed ground's uniform (`Detail` in `ground.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DetailUniform {
    sun: [f32; 4],
    shade: [f32; 32],
    classes: [[f32; 4]; 16],
    tiles: [[f32; 4]; 256],
}

/// The detailed ground (docs/ground.md), once its textures are loaded.
struct DetailGround {
    /// Over the original view (painter's order, like the original ground).
    pipeline: wgpu::RenderPipeline,
    /// Under the free camera, depth-tested among its own triangles.
    free_pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    depth: Option<((u32, u32), wgpu::TextureView)>,
}

struct Target {
    size: (u32, u32),
    view: wgpu::TextureView,
    group: wgpu::BindGroup,
}

pub struct TerrainRenderer {
    /// Draws the zoomed-out view's ground (key L, `MiniMap4X.cpp`) instead of the normal
    /// one. The original renders it per 32 × 32-pixel block into a cache (0x5ffbd6 →
    /// 0x5fdddd) and copies the cached blocks to the screen (0x5fb93e); here it is drawn
    /// every frame.
    pub zoomed: bool,
    /// The free camera (`None`: the original view).
    pub orbit: Option<Orbit>,
    /// A flat colour shown instead of the ground (the driver's blank field, `white` or
    /// `greenscreen`); the terrain still gives the 3D pass its depth.
    pub backdrop: Option<wgpu::Color>,
    /// The 3D pass draws the water surface (`water.rs`): the depth tint is left out.
    pub water_surface: bool,
    ground_pipeline: wgpu::RenderPipeline,
    /// The ground under the free camera: depth-tested, the hills no longer come in order.
    free_pipeline: wgpu::RenderPipeline,
    /// The terrain's depth in the 3D pass.
    depth_pipeline: wgpu::RenderPipeline,
    /// Depth buffer of the free camera's ground target.
    free_depth: Option<((u32, u32), wgpu::TextureView)>,
    /// The models' shadows on the ground in the 3D pass (`terrain_shadow.wgsl`), made at
    /// the first use (it needs the shadow map's layout).
    shadow_pipeline: std::cell::OnceCell<wgpu::RenderPipeline>,
    output: wgpu::TextureFormat,
    /// Vertices of the cells drawn by the last `render`.
    drawn: u32,
    zoom_pipeline: wgpu::RenderPipeline,
    present_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    target_layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    tiles: wgpu::TextureView,
    masks: wgpu::TextureView,
    tables: wgpu::TextureView,
    palette: wgpu::TextureView,
    map: Option<MapTextures>,
    target: Option<Target>,
    max_size: u32,
    /// Draw the detailed ground (3D mode with `ground=detailed`) once its textures are set.
    pub detailed: bool,
    detail: Option<DetailGround>,
    /// Light levels and texture colours for [`DetailUniform`].
    detail_tables: ([f32; 32], Vec<[f32; 4]>),
}

const GROUND_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Uint;
/// `terrain.wgsl` with the shadow receiver and the ground-shadow pass.
pub const TERRAIN_SHADOW_SHADER: &str =
    concat!(include_str!("terrain.wgsl"), include_str!("shadow.wgsl"), include_str!("terrain_shadow.wgsl"));
/// `terrain.wgsl` with the detailed ground (`ground.wgsl`).
pub const TERRAIN_DETAIL_SHADER: &str = concat!(include_str!("terrain.wgsl"), include_str!("ground.wgsl"));
/// Depth of the free camera's ground target.
const FREE_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

/// Size of the ground target for an output of `size` pixels and the target pixels per
/// original pixel: the ground is drawn at original resolution when zoomed in, else at
/// output size, within `max` pixels. The zoomed-out view draws twice the rows and keeps
/// every other one (0x5fe62f).
fn target_size(size: [f32; 2], scale: f32, zoomed: bool, max: u32) -> ((u32, u32), f32) {
    let rows = if zoomed { 2.0 } else { 1.0 };
    let needed = |r: f32| ((size[0] / scale * r).ceil(), (size[1] / scale * r * rows).ceil());
    let mut ratio = scale.min(1.0);
    let (w, h) = needed(ratio);
    ratio *= (max as f32 / w.max(h)).min(1.0);
    let (w, h) = needed(ratio);
    (((w as u32).clamp(1, max), (h as u32).clamp(1, max)), ratio)
}

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: (usize, usize),
    format: wgpu::TextureFormat,
    data: &[u8],
) -> wgpu::TextureView {
    texture_object(device, queue, label, size, format, data).create_view(&Default::default())
}

fn texture_object(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: (usize, usize),
    format: wgpu::TextureFormat,
    data: &[u8],
) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size.0 as u32, height: size.1 as u32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        data,
    )
}

/// Light levels a fully trampled vertex is darkened by (higher levels are darker).
pub const TRAMPLE_DARKEN: u32 = 10;

/// A vertex's data with its light darkened by trample `amount` (0..255).
fn trampled(v: [u8; 4], amount: u8) -> [u8; 4] {
    let [tile, light, lift, zoom] = v;
    if light == 0 || amount == 0 {
        return v;
    }
    let darker = (u32::from(light) + (u32::from(amount) * TRAMPLE_DARKEN + 127) / 255).min(31) as u8;
    [tile, darker, lift, zoom]
}

impl TerrainRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, output: wgpu::TextureFormat, assets: &GroundAssets) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("terrain.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let tex = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: both,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let uint = wgpu::TextureSampleType::Uint;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: both,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex(1, wgpu::TextureSampleType::Sint),
                tex(2, uint),
                tex(3, uint),
                tex(4, uint),
                tex(5, uint),
                tex(6, uint),
                tex(7, uint),
                tex(8, wgpu::TextureSampleType::Float { filterable: false }),
                tex(9, uint),
            ],
        });
        let target_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain ground"),
            entries: &[tex(0, uint)],
        });
        let ground_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain ground"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let present_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain present"),
            bind_group_layouts: &[Some(&layout), Some(&target_layout)],
            immediate_size: 0,
        });
        let pipeline = |label,
                        layout: &wgpu::PipelineLayout,
                        vs,
                        fs,
                        format,
                        depth: Option<wgpu::DepthStencilState>,
                        write_mask: wgpu::ColorWrites| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let all = wgpu::ColorWrites::ALL;
        let ground_pipeline =
            pipeline("terrain ground", &ground_layout, "vs_ground", "fs_ground", GROUND_FORMAT, None, all);
        let zoom_pipeline = pipeline("terrain zoomed", &ground_layout, "vs_zoom", "fs_zoom", GROUND_FORMAT, None, all);
        let present_pipeline = pipeline("terrain present", &present_layout, "vs_full", "fs_present", output, None, all);
        let depth_state = |format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let free_pipeline = pipeline(
            "terrain ground (free camera)",
            &ground_layout,
            "vs_ground",
            "fs_ground",
            GROUND_FORMAT,
            Some(depth_state(FREE_DEPTH_FORMAT)),
            all,
        );
        let depth_pipeline = pipeline(
            "terrain depth",
            &ground_layout,
            "vs_depth",
            "fs_depth",
            output,
            Some(depth_state(DEPTH_FORMAT)),
            wgpu::ColorWrites::empty(),
        );

        let r8 = wgpu::TextureFormat::R8Uint;
        let tiles = texture(device, queue, "terrain tiles", (512, 2048), r8, &assets.tile_atlas());
        let masks = texture(device, queue, "terrain masks", (512, 27 * 32), r8, &assets.mask_atlas());
        let tables = texture(device, queue, "terrain tables", (256, 800), r8, &assets.tables());
        let palette_format =
            if output.is_srgb() { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
        let rgba: Vec<u8> = assets.palette.0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
        let palette = texture(device, queue, "terrain palette", (256, 1), palette_format, &rgba);
        Self {
            zoomed: false,
            orbit: None,
            backdrop: None,
            water_surface: false,
            ground_pipeline,
            free_pipeline,
            depth_pipeline,
            free_depth: None,
            shadow_pipeline: std::cell::OnceCell::new(),
            output,
            drawn: 0,
            zoom_pipeline,
            present_pipeline,
            layout,
            target_layout,
            globals,
            tiles,
            masks,
            tables,
            palette,
            map: None,
            target: None,
            max_size: device.limits().max_texture_dimension_2d,
            detailed: false,
            detail: None,
            detail_tables: assets.detail_tables(),
        }
    }

    /// Whether the detailed ground's textures are set.
    pub fn has_materials(&self) -> bool {
        self.detail.is_some()
    }

    /// Sets the detailed ground's textures (docs/ground.md) and makes its pipelines.
    pub fn set_materials(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &crate::ground::MaterialImages,
    ) {
        let layers = images.layers.len() as u32;
        let levels = images.layers.first().map_or(1, |l| l.0.len()) as u32;
        let array = |label, format, pick: &dyn Fn(&crate::ground::MaterialSet) -> &Vec<Vec<u8>>| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: images.size, height: images.size, depth_or_array_layers: layers },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (layer, set) in images.layers.iter().enumerate() {
                for (level, data) in pick(set).iter().enumerate() {
                    let side = (images.size >> level).max(1);
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: level as u32,
                            origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                            aspect: wgpu::TextureAspect::All,
                        },
                        data,
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * side), rows_per_image: None },
                        wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
                    );
                }
            }
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let albedo = array("ground albedo", wgpu::TextureFormat::Rgba8UnormSrgb, &|s| &s.0);
        let normals = array("ground normals", wgpu::TextureFormat::Rgba8Unorm, &|s| &s.1);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ground materials"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: if cfg!(target_arch = "wasm32") { 1 } else { 8 },
            ..Default::default()
        });
        let (shade, tiles) = &self.detail_tables;
        let sun = crate::shadow::LIGHT;
        let len = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt();
        let mut uniform = DetailUniform {
            sun: [sun[0] / len, sun[1] / len, sun[2] / len, if self.output.is_srgb() { 1.0 } else { 0.0 }],
            shade: *shade,
            classes: [[0.0; 4]; 16],
            tiles: [[0.0; 4]; 256],
        };
        for (i, mean) in images.means.iter().enumerate() {
            let repeat = crate::ground::REPEAT_METRES[i] * crate::view3d::PIXELS_PER_METRE;
            uniform.classes[i] = [mean[0], mean[1], mean[2], repeat];
        }
        for (out, t) in uniform.tiles.iter_mut().zip(tiles) {
            *out = *t;
            // A class without a texture set keeps the original.
            if out[3] as usize >= images.layers.len() {
                out[3] = f32::from(crate::ground::KEEP);
            }
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ground detail"),
            size: size_of::<DetailUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&uniform));
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let float_array = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: both,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ground detail"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: both,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                float_array(1),
                float_array(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: both,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ground detail"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&albedo) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&normals) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ground detail"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_DETAIL_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ground detail"),
            bind_group_layouts: &[Some(&self.layout), None, Some(&layout)],
            immediate_size: 0,
        });
        // Over the original by the alpha (kept textures, water); the target's alpha stays.
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipeline = |label, depth: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_detail"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_detail"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.output,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let free_pipeline = pipeline(
            "ground detail (free camera)",
            Some(wgpu::DepthStencilState {
                format: FREE_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
        );
        self.detail =
            Some(DetailGround { pipeline: pipeline("ground detail", None), free_pipeline, group, depth: None });
    }

    /// Trampled soil (#223): darkens each vertex's light level by `amounts` (0..255 per
    /// vertex, map order) up to [`TRAMPLE_DARKEN`] levels; bright textures (light 0) stay.
    pub fn set_trample(&self, queue: &wgpu::Queue, amounts: &[u8]) {
        let Some(map) = &self.map else { return };
        if amounts.len() != map.base.len() {
            return;
        }
        let data: Vec<u8> = map.base.iter().zip(amounts).flat_map(|(v, &a)| trampled(*v, a)).collect();
        queue.write_texture(
            map.vertices.as_image_copy(),
            &data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * map.width as u32), rows_per_image: None },
            wgpu::Extent3d { width: map.width as u32, height: map.height as u32, depth_or_array_layers: 1 },
        );
    }

    pub fn set_map(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &TerrainData) {
        let heights: Vec<u8> = data.heights.iter().flat_map(|h| h.to_le_bytes()).collect();
        let vertices: Vec<u8> = data.vertices.iter().flatten().copied().collect();
        let sections: Vec<u8> = data.sections.iter().flatten().copied().collect();
        let zoom_layers: Vec<u8> = data.zoom_layers.iter().flatten().copied().collect();
        let (ww, wh, depth) = data.water.clone().unwrap_or((1, 1, vec![0]));
        let size = (data.width, data.height);
        let cells = ((data.width - 1) / 2 * 2, data.height);
        let zoom = ((data.width - 1) / 2 * ZOOM_TRIANGLES.len(), data.height);
        let vertex_texture =
            texture_object(device, queue, "terrain vertices", size, wgpu::TextureFormat::Rgba8Uint, &vertices);
        let vertex_view = vertex_texture.create_view(&Default::default());
        self.map = Some(MapTextures {
            vertices: vertex_texture,
            base: data.vertices.clone(),
            width: data.width,
            height: data.height,
            water: (ww, wh),
            height_range: data.height_range,
            group: self.bind_group(
                device,
                &[
                    texture(device, queue, "terrain heights", size, wgpu::TextureFormat::R16Sint, &heights),
                    vertex_view,
                    texture(device, queue, "terrain sections", cells, wgpu::TextureFormat::Rgba8Uint, &sections),
                    texture(device, queue, "terrain water", (ww, wh), wgpu::TextureFormat::R8Uint, &depth),
                    texture(device, queue, "terrain zoom layers", zoom, wgpu::TextureFormat::Rgba8Uint, &zoom_layers),
                ],
            ),
        });
    }

    /// Binds the map textures (heights, vertices, sections, water, zoom layers) with the
    /// shared ones.
    fn bind_group(&self, device: &wgpu::Device, map: &[wgpu::TextureView; 5]) -> wgpu::BindGroup {
        fn view(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(view) }
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
                view(1, &map[0]),
                view(2, &map[1]),
                view(3, &map[2]),
                view(4, &self.tiles),
                view(5, &self.masks),
                view(6, &self.tables),
                view(7, &map[3]),
                view(8, &self.palette),
                view(9, &map[4]),
            ],
        })
    }

    /// Recreates the ground target when the needed size changes.
    fn ensure_target(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain ground"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: GROUND_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain ground"),
            layout: &self.target_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        self.target = Some(Target { size, view, group });
    }

    /// Draws the terrain of the current map over all of `output` (`size` pixels).
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        size: [f32; 2],
        camera: &Camera,
    ) {
        // The zoomed-out ground has its own geometry for the original view only.
        let zoomed = self.zoomed && self.orbit.is_none();
        let (target_size, ratio) = target_size(size, camera.scale, zoomed, self.max_size);
        self.ensure_target(device, target_size);
        let Some(map) = &self.map else {
            clear_pass(encoder, output, wgpu::Color::BLACK);
            return;
        };
        let view = View3d { camera: *camera, size, orbit: self.orbit };

        // Visible cells: columns by x, rows by screen y widened by the map's height range.
        let view_w = size[0] / camera.scale;
        let view_h = size[1] / camera.scale;
        let cells_per_row = ((map.width - 1) / 2) as i32;
        let (lo, hi) = map.height_range;
        let rows = map.height as i32 - 1;
        let (cp0, cp1, row0, row1) = if view.is_free() {
            let [x0, y0, x1, y1] = view.ground_bounds(lo.min(0) as f32 / COS_E, (hi.max(0) + 8) as f32 / COS_E);
            (
                ((x0 / 64.0).floor() as i32 - 1).clamp(0, cells_per_row),
                ((x1 / 64.0).ceil() as i32 + 1).clamp(0, cells_per_row),
                ((y0 / 32.0).floor() as i32 - 2).clamp(0, rows),
                ((y1 / 32.0).ceil() as i32 + 2).clamp(0, rows),
            )
        } else {
            (
                ((camera.x / 64.0).floor() as i32 - 1).clamp(0, cells_per_row),
                (((camera.x + view_w) / 64.0).ceil() as i32 + 1).clamp(0, cells_per_row),
                (((camera.y + lo.min(0) as f32) / 16.0).floor() as i32 - 2).clamp(0, rows),
                (((camera.y + view_h + hi.max(0) as f32 + 8.0) / 16.0).ceil() as i32 + 2).clamp(0, rows),
            )
        };
        let (ncp, nrows) = (cp1 - cp0, row1 - row0);
        self.drawn = if ncp > 0 && nrows > 0 { (ncp * nrows * 12) as u32 } else { 0 };

        let globals = Globals {
            camera: [camera.x, camera.y, camera.scale, ratio],
            sizes: [target_size.0 as f32, target_size.1 as f32, size[0], size[1]],
            cells: [cp0, row0, ncp.max(1), map.width as i32],
            map: [map.height as i32, map.water.0 as i32, map.water.1 as i32, i32::from(zoomed)],
            view: view.world_to_clip(),
            mode: [
                if view.is_free() { 1.0 } else { 0.0 },
                DEPTH_BIAS * 0.5 / DEPTH_RANGE,
                if self.water_surface { 1.0 } else { 0.0 },
                0.0,
            ],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        if view.is_free() && self.free_depth.as_ref().is_none_or(|(s, _)| *s != target_size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("terrain ground depth"),
                size: wgpu::Extent3d { width: target_size.0, height: target_size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FREE_DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.free_depth = Some((target_size, texture.create_view(&Default::default())));
        }
        let target = self.target.as_ref().expect("target created above");
        {
            let depth = self.free_depth.as_ref().filter(|_| view.is_free()).map(|(_, v)| v);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: depth.map(|view| wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if self.drawn > 0 {
                pass.set_pipeline(if depth.is_some() {
                    &self.free_pipeline
                } else if zoomed {
                    &self.zoom_pipeline
                } else {
                    &self.ground_pipeline
                });
                pass.set_bind_group(0, &map.group, &[]);
                pass.draw(0..self.drawn, 0..1);
            }
        }
        if let Some(color) = self.backdrop {
            clear_pass(encoder, output, color);
            return;
        }
        {
            let mut pass = clear_pass(encoder, output, wgpu::Color::BLACK);
            pass.set_pipeline(&self.present_pipeline);
            pass.set_bind_group(0, &map.group, &[]);
            pass.set_bind_group(1, &target.group, &[]);
            pass.draw(0..3, 0..1);
        }
        // The detailed ground over the original (docs/ground.md); the zoomed-out view
        // stays the original.
        let drawn = self.drawn;
        let Some(detail) = self.detail.as_mut().filter(|_| self.detailed && !zoomed && drawn > 0) else {
            return;
        };
        let out_size = output.texture().size();
        let out_size = (out_size.width, out_size.height);
        if view.is_free() && detail.depth.as_ref().is_none_or(|(s, _)| *s != out_size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ground detail depth"),
                size: wgpu::Extent3d { width: out_size.0, height: out_size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FREE_DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            detail.depth = Some((out_size, texture.create_view(&Default::default())));
        }
        let depth = detail.depth.as_ref().filter(|_| view.is_free()).map(|(_, v)| v);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ground detail"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: depth.map(|view| wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(if depth.is_some() { &detail.free_pipeline } else { &detail.pipeline });
        pass.set_bind_group(0, &map.group, &[]);
        pass.set_bind_group(2, &detail.group, &[]);
        pass.draw(0..drawn, 0..1);
    }

    /// Writes the terrain's depth into the 3D pass (`model_renderer::DepthTarget`) for the
    /// view of the last [`TerrainRenderer::render`].
    pub fn render_depth(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(map) = &self.map else { return };
        if self.drawn == 0 {
            return;
        }
        pass.set_pipeline(&self.depth_pipeline);
        pass.set_bind_group(0, &map.group, &[]);
        pass.draw(0..self.drawn, 0..1);
    }

    /// Darkens the drawn ground where the models' shadows fall: in the 3D pass after
    /// [`Self::render_depth`] (and the models, which cover it where they stand in front).
    pub fn render_shadows(&self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'_>, shadow: &ShadowMap) {
        let Some(map) = &self.map else { return };
        if self.drawn == 0 || !shadow.active {
            return;
        }
        let pipeline = self.shadow_pipeline.get_or_init(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("terrain shadow"),
                source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADOW_SHADER.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain shadow"),
                bind_group_layouts: &[Some(&self.layout), None, Some(&shadow.layout)],
                immediate_size: 0,
            });
            // Black over the ground by the shadow's alpha; the target's alpha is kept.
            let darken = wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("terrain shadow"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_ground_shadow"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(crate::model_renderer::depth_test_only()),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_ground_shadow"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.output,
                        blend: Some(darken),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &map.group, &[]);
        pass.set_bind_group(2, &shadow.group, &[]);
        pass.draw(0..self.drawn, 0..1);
    }
}

fn clear_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    color: wgpu::Color,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("terrain"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 513;

    fn terrain(height: impl Fn(usize, usize) -> i16) -> TerrainData {
        let rows = 64;
        let heights = (0..W * rows).map(|v| height(v % W, v / W)).collect();
        TerrainData {
            width: W,
            height: rows,
            heights,
            vertices: Vec::new(),
            sections: Vec::new(),
            zoom_layers: Vec::new(),
            water: None,
            height_range: (0, 0),
            playable: 15360,
        }
    }

    #[test]
    fn flat_ground_has_light_level_3() {
        let flat = vec![0i16; W * 8];
        assert_eq!(vertex_light(&flat, W, 3 * W + 10), 3);
        assert_eq!(vertex_light(&flat, W, 3 * W + 11), 3);
    }

    #[test]
    fn slopes_facing_the_light_are_brighter() {
        // Rising to the right faces away from the light (x = -15), rising downwards faces it.
        let right: Vec<i16> = (0..W * 8).map(|v| (v % W) as i16 * 8).collect();
        let down: Vec<i16> = (0..W * 8).map(|v| (v / W) as i16 * -16).collect();
        assert!(vertex_light(&right, W, 3 * W + 10) > 3);
        assert!(vertex_light(&down, W, 3 * W + 10) < 3);
    }

    #[test]
    fn height_at_matches_vertices_and_planes() {
        let flat = terrain(|_, _| 40);
        for (x, y) in [(100, 100), (333, 1000), (32 * 7, 32 * 9), (32 * 7 + 5, 32 * 9 - 7)] {
            assert_eq!(flat.height_at(x, y), 40, "({x}, {y})");
        }
        // A plane along y: height = vertex row * 8, i.e. y / 4 at even columns.
        let ramp = terrain(|x, y| (y * 8) as i16 - if x % 2 == 1 { 4 } else { 0 });
        for (x, y) in [(64, 320), (64 + 16, 320), (96, 336), (100, 1000)] {
            assert!((ramp.height_at(x, y) - y / 4).abs() <= 1, "({x}, {y}): {}", ramp.height_at(x, y));
        }
    }

    #[test]
    fn zoom_layers_follow_the_original_order() {
        // Base is the smallest texture; overlays keep the order of 0x5fa3cc.
        assert_eq!(zoom_layers([4, 4, 4]), [4, 4, 4]);
        assert_eq!(zoom_layers([1, 5, 3]), [1, 5, 3]);
        assert_eq!(zoom_layers([1, 5, 5]), [1, 5, 1], "the second overlay repeats the first");
        assert_eq!(zoom_layers([5, 1, 3]), [1, 5, 3]);
        assert_eq!(zoom_layers([5, 3, 1]), [1, 3, 5]);
        assert_eq!(zoom_layers([5, 5, 1]), [1, 5, 1]);
        assert_eq!(zoom_layers([2, 2, 7]), [2, 2, 7], "an overlay equal to the base is absent");
    }

    #[test]
    fn zoom_triangles_tile_the_cell() {
        // In the zoomed view vertex (col, row) sits at (8 col, 8 row - 4 if col is odd).
        let at = |(c, r): (usize, usize)| (8 * c as i32, 8 * r as i32 - if c % 2 == 1 { 4 } else { 0 });
        let mut area = 0;
        for t in ZOOM_TRIANGLES {
            let [p1, p2, p3] = t.vertices.map(at);
            assert_eq!((p1.0, p2.0 - p1.0, p2.1 - p1.1), (p2.0, 0, 8), "P1 above P2");
            assert_eq!(p3.0 - p1.0, if t.left { -8 } else { 8 }, "apex to the side");
            assert_eq!(p3.1 - p1.1, 4, "apex half way down");
            area += 32;
        }
        assert_eq!(area, 16 * 8, "four triangles cover a 16 x 8 cell");
        assert_eq!(ZOOM_TRIANGLES[3].lights[1], (1, 1), "the fourth triangle's light quirk");
    }

    #[test]
    fn zoomed_target_has_twice_the_rows() {
        let (normal, r) = target_size([800.0, 600.0], 0.25, false, 8192);
        assert_eq!((normal, r), ((800, 600), 0.25));
        let (zoomed, r) = target_size([800.0, 600.0], 0.25, true, 8192);
        assert_eq!((zoomed, r), ((800, 1200), 0.25));
        assert_eq!(target_size([800.0, 600.0], 2.0, false, 8192).0, (400, 300), "original pixels when zoomed in");
        let (capped, r) = target_size([4000.0, 3000.0], 0.25, true, 2048);
        assert!(capped.0 <= 2048 && capped.1 <= 2048 && r < 0.25);
    }

    #[test]
    fn zoom_weight_is_the_random_table_low_bits() {
        let random = RandomTable((0..0x2000u16).map(|i| i.wrapping_mul(40503)).collect());
        for v in [0, 1, 513, 0x1FFF, 0x2000 + 5] {
            assert_eq!(zoom_weight(&random, v), (random.get(v) & 0x7F) as u8);
        }
        assert_eq!(zoom_weight(&random, 0x2005), zoom_weight(&random, 5));
    }

    #[test]
    fn camera_projection() {
        let camera = Camera { x: 100.0, y: 50.0, scale: 2.0 };
        assert_eq!(camera.project(100, 100, 0), [0.0, 0.0]);
        assert_eq!(camera.project(110, 200, 10), [20.0, 80.0]);
    }
}
