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
//! [`TerrainData`] prepares a map on the CPU; [`TerrainRenderer`] draws it.

use acr_formats::Palette;
use acr_formats::grd::BlendTable;
use acr_formats::m3d::M3dMap;
use acr_formats::terrain::{MaskTable, RandomTable, TILE_SIZE, TextureFlags, TileSet};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

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
    /// Per vertex: texture, light level, random lift, 0.
    vertices: Vec<[u8; 4]>,
    /// Per cell (column pair, row): transition variants S0..S6, 0.
    sections: Vec<[u8; 8]>,
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
                [tile, light, lift(v, tile), 0]
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
        let water = map.water.as_ref().map(|w| (w.depth.width, w.depth.height, w.depth.cells.clone()));
        let range =
            heights.cells.iter().fold((i32::MAX, i32::MIN), |(lo, hi), &h| (lo.min(h.into()), hi.max(h.into())));
        Ok(Self {
            width,
            height,
            heights: heights.cells.clone(),
            vertices,
            sections,
            water,
            height_range: range,
            playable: size.playable_size(),
        })
    }

    fn h(&self, index: usize) -> i32 {
        self.heights.get(index).copied().map_or(0, i32::from)
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
}

struct MapTextures {
    group: wgpu::BindGroup,
    width: usize,
    height: usize,
    water: (usize, usize),
    height_range: (i32, i32),
}

struct Target {
    size: (u32, u32),
    view: wgpu::TextureView,
    group: wgpu::BindGroup,
}

pub struct TerrainRenderer {
    ground_pipeline: wgpu::RenderPipeline,
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
}

const GROUND_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Uint;

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: (usize, usize),
    format: wgpu::TextureFormat,
    data: &[u8],
) -> wgpu::TextureView {
    device
        .create_texture_with_data(
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
        .create_view(&Default::default())
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
        let pipeline = |label, layout: &wgpu::PipelineLayout, vs, fs, format| {
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
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let ground_pipeline = pipeline("terrain ground", &ground_layout, "vs_ground", "fs_ground", GROUND_FORMAT);
        let present_pipeline = pipeline("terrain present", &present_layout, "vs_full", "fs_present", output);

        let r8 = wgpu::TextureFormat::R8Uint;
        let tiles = texture(device, queue, "terrain tiles", (512, 2048), r8, &assets.tile_atlas());
        let masks = texture(device, queue, "terrain masks", (512, 27 * 32), r8, &assets.mask_atlas());
        let tables = texture(device, queue, "terrain tables", (256, 800), r8, &assets.tables());
        let palette_format =
            if output.is_srgb() { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
        let rgba: Vec<u8> = assets.palette.0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
        let palette = texture(device, queue, "terrain palette", (256, 1), palette_format, &rgba);
        Self {
            ground_pipeline,
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
        }
    }

    pub fn set_map(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &TerrainData) {
        let heights: Vec<u8> = data.heights.iter().flat_map(|h| h.to_le_bytes()).collect();
        let vertices: Vec<u8> = data.vertices.iter().flatten().copied().collect();
        let sections: Vec<u8> = data.sections.iter().flatten().copied().collect();
        let (ww, wh, depth) = data.water.clone().unwrap_or((1, 1, vec![0]));
        let size = (data.width, data.height);
        let cells = ((data.width - 1) / 2 * 2, data.height);
        self.map = Some(MapTextures {
            width: data.width,
            height: data.height,
            water: (ww, wh),
            height_range: data.height_range,
            group: self.bind_group(
                device,
                &[
                    texture(device, queue, "terrain heights", size, wgpu::TextureFormat::R16Sint, &heights),
                    texture(device, queue, "terrain vertices", size, wgpu::TextureFormat::Rgba8Uint, &vertices),
                    texture(device, queue, "terrain sections", cells, wgpu::TextureFormat::Rgba8Uint, &sections),
                    texture(device, queue, "terrain water", (ww, wh), wgpu::TextureFormat::R8Uint, &depth),
                ],
            ),
        });
    }

    /// Binds the map textures (heights, vertices, sections, water) with the shared ones.
    fn bind_group(&self, device: &wgpu::Device, map: &[wgpu::TextureView; 4]) -> wgpu::BindGroup {
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
        // The ground is drawn at original resolution when zoomed in, else at output size.
        let needed = |r: f32| ((size[0] / camera.scale * r).ceil(), (size[1] / camera.scale * r).ceil());
        let mut ratio = camera.scale.min(1.0);
        let (w, h) = needed(ratio);
        ratio *= (self.max_size as f32 / w.max(h)).min(1.0);
        let (w, h) = needed(ratio);
        let target_size = ((w as u32).clamp(1, self.max_size), (h as u32).clamp(1, self.max_size));
        self.ensure_target(device, target_size);
        let Some(map) = &self.map else {
            clear_pass(encoder, output, wgpu::Color::BLACK);
            return;
        };

        // Visible cells: columns by x, rows by screen y widened by the map's height range.
        let view_w = size[0] / camera.scale;
        let view_h = size[1] / camera.scale;
        let cells_per_row = ((map.width - 1) / 2) as i32;
        let cp0 = ((camera.x / 64.0).floor() as i32 - 1).clamp(0, cells_per_row);
        let cp1 = (((camera.x + view_w) / 64.0).ceil() as i32 + 1).clamp(0, cells_per_row);
        let (lo, hi) = map.height_range;
        let rows = map.height as i32 - 1;
        let row0 = (((camera.y + lo.min(0) as f32) / 16.0).floor() as i32 - 2).clamp(0, rows);
        let row1 = (((camera.y + view_h + hi.max(0) as f32 + 8.0) / 16.0).ceil() as i32 + 2).clamp(0, rows);
        let (ncp, nrows) = (cp1 - cp0, row1 - row0);

        let globals = Globals {
            camera: [camera.x, camera.y, camera.scale, ratio],
            sizes: [target_size.0 as f32, target_size.1 as f32, size[0], size[1]],
            cells: [cp0, row0, ncp.max(1), map.width as i32],
            map: [map.height as i32, map.water.0 as i32, map.water.1 as i32, 0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        let target = self.target.as_ref().expect("target created above");
        {
            let mut pass = clear_pass(encoder, &target.view, wgpu::Color::TRANSPARENT);
            if ncp > 0 && nrows > 0 {
                pass.set_pipeline(&self.ground_pipeline);
                pass.set_bind_group(0, &map.group, &[]);
                pass.draw(0..(ncp * nrows * 12) as u32, 0..1);
            }
        }
        let mut pass = clear_pass(encoder, output, wgpu::Color::BLACK);
        pass.set_pipeline(&self.present_pipeline);
        pass.set_bind_group(0, &map.group, &[]);
        pass.set_bind_group(1, &target.group, &[]);
        pass.draw(0..3, 0..1);
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
    fn camera_projection() {
        let camera = Camera { x: 100.0, y: 50.0, scale: 2.0 };
        assert_eq!(camera.project(100, 100, 0), [0.0, 0.0]);
        assert_eq!(camera.project(110, 200, 10), [20.0, 80.0]);
    }
}
