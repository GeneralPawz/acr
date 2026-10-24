# The ground in 3D mode: materials, ground state, particles

Up close, the original ground is pixel art: the 8-bit tiles of `TILES3.BMP`, shaded through
the darkness table. Next to realistic models (#287, realism over sprite parity) it looks
like a comic. Trampling (#223) only darkens the light level per vertex, and blood, dirt
and craters are flat decals: there is no flying earth, and hooves leave the same marks as
boots. This plan replaces the ground in 3D mode in phases. Sprite mode and the zoomed-out
view (key L) keep the original.

Everything here is render-only. Nothing feeds back into `acr-sim`, so replays, savegames
and the hashes stay as they are.

## What the ground is today (terrain.rs, terrain.wgsl)

- **Pass 1** draws the triangle mesh (4 triangles per cell, odd columns half a cell up) into
  an `R8Uint` target of palette indices. It composes the textures with transition masks
  and the original blend tables, and shades each texel by the per-vertex light level (0..31,
  flat ground 3) through `AGEW_1DR.GRD`.
- **Pass 2** (`fs_present`) tints water by depth through `WCOST.GRD` and resolves the palette.
- The **3D pass** writes the terrain depth (`vs_depth`) and darkens the drawn ground under
  model shadows (`terrain_shadow.wgsl`). Blood and dirt are decals (`blood.wgsl`,
  `BloodPool::dirt`); the crater is a decal too.
- **Trampling** (`ingame/trample.rs`) rewrites the light levels of the vertex texture once a
  second (`set_trample`), up to 10 levels darker.
- The **free camera** draws the same mesh through the view matrix, depth-tested, into the
  same palette target.

## Phase 1: material ground (built)

**Setting.** `ground=detailed|original`, default `detailed`. It applies in 3D mode only:
`models` other than `off`, or `camera=free`.

**Classes.** Each of the 136 textures belongs to one class, taken from a contact sheet of
`TILES3.BMP` (`ACR-analysis/ground/tiles3_sheet.png`; note that the BMP is stored
bottom-up, so texture *t* is at row 33 − *t*/4). The table is `acr_render::ground::TILE_CLASS`.

| class | set (second set) | repeat | textures |
|---|---|---|---|
| grass | ambientCG `Grass001` (`Grass004`) | 2 m (2.5 m) | 0 (the editor's green), 4, 5, 12, 16, 18, 20, 22, 39 |
| meadow | ambientCG `Grass007` (`Ground037`, grass with bare patches) | 2 m (3 m) | 1–3, 6, 7, 13, 17, 19, 21, 23, 63–65, 68, 69, 96 |
| dirt | `dirt` (`park_dirt`) | 3 m | 15, 34, 67, 71 (blank `dirt`), 84 |
| mud | `brown_mud_02` | 2.5 m | 29, 30, 49–51, 53–55 (grey swamp mud) |
| sand | `sand_01` | 3 m | 72, 75, 79, 100–102, 104–106, 108, 109, 114 |
| rock | `rock_ground` (`rocky_terrain_02`, mossy) | 3 m (4 m) | 8–11, 31, 33, 38, 46, 47, 56, 57, 80–83, 86, 90, 97, 98 |
| forest floor | `forest_ground_04` | 2.5 m | 28, 58, 59, 62 |
| snow | `snow_02` | 3 m | 95, 99, 103 (blank `snow`), 107, 110 |
| field | `dry_mud_field_001` | 3 m | 27, 32, 73, 85, 92, 111, 113 |
| cracked | `mud_cracked_dry_03` | 3 m | 42, 43, 52 (dry cracked earth, town squares) |
| red | `red_dirt_mud_01` | 2.5 m | 66, 70, 74, 78, 112 (red-brown soil and red sand) |
| gravel | `gravel_ground_01` | 2 m | 14, 26, 87, 91 (gravel, roads) |
| desert | `dry_ground_rocks` (`sandy_gravel_02`) | 3 m | 120–135 (tan dry soil and shore, e.g. Single2, the Columbus maps) |
| steppe | `grass_ground` (dry grass on soil) | 2.5 m | 93, 94, 116–119 |
| snowy | `snow_03` (snow over ground) | 3 m | 35 |
| keep (original) | none | | paving and cobbles 24, 25, 36, 37, 40, 41, 44, 45, 48; logs 76, 77; water 60, 61, 88, 89, 115 |

The split (m2-ground-pbr) follows `tiles3_sheet.png` and the mean colour of each texture
(`ACR-analysis/genai/ground/scripts/tilemeans.py`); a class histogram of every shipped map is
`scripts/maphist.py`. Grass, meadow and rock cover 85–95 % of most maps (rock up to 66 % on
`usa02` and the Pizarro maps), dirt and desert the Columbus maps and `Single2`, red soil
26–34 % of `battle1` and `battle2` (much of it a lakebed under water); real snow covers under
1 % of any map, so the snow classes are shown on the blank field. No tile is a ploughed
field or a distinct road surface: roads in AC are paving (*keep*) or gravel.

**Assets.** The maps are CC0 from Poly Haven and ambientCG: diffuse, OpenGL normal,
roughness and displacement, at 2k.
- `assets/ground/fetch_polyhaven.py` downloads Poly Haven sets into
  `D:\ACR-models\polyhaven-textures\<id>\`; the later downloads are in
  `polyhaven-textures2\` and `ambientcg\` (ledger `D:\ACR-models\LEDGER.csv`), with a
  `SOURCE.json` per set.
- `cargo xtask ground [--src] [--out] [--size]` runs `assets/ground/pack.py` over the
  sources root (default `D:\ACR-models`), which writes two 1024² PNGs per texture layer
  into `<out>/ground/`: `<layer>_a.png` (albedo RGB + height A) and `<layer>_n.png` (normal
  RG + roughness B). It reads both the Poly Haven and the ambientCG file names.
- The layers are `assets/ground/materials.py` = `acr_render::ground::LAYERS`: the 15
  classes (`CLASSES`, class *i* is layer *i*), then the five second sets `grass2`,
  `meadow2`, `dirt2`, `desert2`, `rock2` (`SECOND` maps a class to its second layer).
- The files are about 105 MB (40 PNGs) and are not committed. The game looks them up as
  `ACR\Models\ground\*.png`: in the game folder, or natively in `ACR_MODELS_DIR\ground\`.
- Credits are in `assets/LICENSES.md`.

**Loading.** `ThreeD` asks for the 40 files once 3D mode is active, decodes them and builds
box-filtered mipmaps on the CPU (0.34 s natively for the first 18; 40 files take 0.41 s on
the render VM, 18 took 0.18 s there). `TerrainRenderer::set_materials` uploads
them as two `texture_2d_array`s: albedo as `Rgba8UnormSrgb`, normals as `Rgba8Unorm`.
- Memory: 20 layers × 2 × 1024² × 4 B × 4/3 ≈ 224 MB natively (9 layers were 100 MB).
- The browser drops the top level: 512², about 56 MB.
- If a file is missing, the original ground stays and the log says so.

**Drawing** (`ground.wgsl`, appended to `terrain.wgsl`). After the present pass, the mesh
is drawn once more, straight into the output (no palette target):
- **Placement:** as the original ground places it, or through the view matrix under the free
  camera. Under the free camera it is depth-tested against an output-sized `Depth24Plus`;
  in the original view it keeps the painter's order.
- **Per triangle**, from the three vertices' textures: the classes and each texture's mean
  colour (linear) are flat varyings, and the barycentric weights are interpolated.
- **Height blend:** weight = barycentric weight + 0.5 × the material's height. The highest
  wins within a 0.2 band, faded out where the barycentric weight is under 0.2. Borders
  follow the stones and tufts of the material instead of the original's masks.
- **Colour:** each class contributes `vertex texture's mean colour × albedo / class mean
  albedo`, clamped to 0.25..2.2. This is the low-frequency macro tint that keeps the map's
  AC colours; from afar the mips go to the mean and the ratio goes to 1.
- **Anti-tiling:** two samples per class, at the repeat and at a turned repeat about 1.9 ×
  larger, mixed by value noise with a period of about 5 m. Classes with a second set (grass,
  meadow, dirt, rock, desert) take the second sample from that set, so two photographs
  alternate; the others repeat their own set. Each sample is divided by its own layer's mean
  before the mix, so the tile tint is kept. The sample count is unchanged. Sampling uses
  `textureSampleGrad` with the gradients taken in uniform control flow.
- **Light:** the original light level (and so the trample darkening), mapped to brightness
  by `shade[level]`. That is the luminance ratio of `AGEW_1DR.GRD` row 64 + level over the
  textures' texels, computed on the CPU.
- **Normal map:** it adds the small relief only, as
  `(0.5 + 0.5 n_mapped·sun) / (0.5 + 0.5 n_geom·sun)`. The geometric normal comes from
  derivatives, because the large relief is already in the level.
- **Alpha:** it keeps the original where a vertex's class is *keep* (blended by the same
  weights) and over water. The original view's water depth is sampled per pixel like
  `fs_present`, so the original tinted lakebed shows through. Without sRGB output (WebGL2
  canvas) the result is gamma-encoded in the shader.
- Model shadows, blood and dirt decals, and the 3D pass then go over it as before.

**Tests.** The driver's blank field has a new `patches` option,
`[{"x","y","radius","texture"}]`, that paints round patches of other textures, so class
transitions can be seen without a real map. The shader validation test also covers
`terrain.wgsl + ground` for GLSL ES.

**Frames** (`ACR-analysis/ground/`, blank field 1280 × 720, HUD off, grass with patches of
dirt 71, mud 66, rock 8, water 89 and snow 103, slope 8, hill, water):
- `original/` and `detailed/` (`detailed-rebased/`: the same after the rebase onto the water surface): `z1_000.png`, `z2_000.png` and `z4_000.png` (zoom 1, 2, 4),
  `free_000.png` (yaw 20°, pitch 20°, zoom 4) and `water_000.png`;
- `cmp_z2.png`: side by side;
- the real-map check, Monongahela `battle3.m3d`: `map-original/` and `map-detailed/`
  (`map_z*_000.png`, `map_free_000.png`) and `cmp_map_z2.png`.

**Measured** (Monongahela `battle3`, 1280 × 720, zoom 1, about 2700 models, shadows on,
driver frames including the simulation, mean of 300 frames per row):

| ticks | original | detailed |
|---|---:|---:|
| 7–307 | 38.3 ms | 38.9 ms |
| 307–607 | 41.8 ms | 45.4 ms |
| 607–907 | 43.3 ms | 48.1 ms |

That is +0.6 to +4.8 ms per frame, single runs, with the noise of the battle. A GPU timer
query is still to do. The pass costs one more draw of the visible mesh (about 4 bytes of
vertex fetch per corner) and 4–12 texture samples per pixel (2 for the albedo plus 2 for
the normal per distinct class, with shared classes reused).

**Phase 1 limits, honestly:**
- The materials are still flat: there is no displacement or parallax yet, so a close-up
  reads as a well-textured plane, not soil with depth.
- The colours follow the original tiles. The editor green (texture 0) is a saturated
  green, so grass looks lawn-like. It is somewhat brighter and more saturated than the
  dithered original (see `cmp_map_z2.png`); the tint and shade calibration needs a pass.
- `dry_ground_01` is cracked clay. That suits dry dirt, but on a large field it reads as a
  pattern; a looser soil set may serve the dirt class better.
- There is no triplanar mapping yet. AC slopes are gentle, but the steepest hill sides
  stretch the texture.
- The shore is a soft alpha cut to the original water, which is still the palette tint.

**Phase 1 finish (m2-ground2):**
- **Calibration.** On battle3 (`cmp_map_z2.png`) the luminance matched (63 vs 62) but the
  detailed grass was far more saturated (mean HSV saturation 178 vs 144) and its blue was
  crushed (mean RGB 76,64,22 vs 74,62,33): the per-channel ratio `albedo / class mean`
  multiplies the strong chroma of the photo into the tint. Now the material's detail is
  mostly luminance (`CHROMA_DETAIL` = 0.35 of the per-channel ratio) and the tile tint is
  desaturated to `TINT_SATURATION` = 0.82. Not yet re-measured on battle3.
- **Dirt** uses Poly Haven `dirt` (loose soil, CC0) instead of cracked clay.
- **Skip:** the pass is not drawn below zoom 0.75 in the original view.
- **Triplanar:** not done yet.

**More sets (m2-ground-pbr).** Six new classes (cracked, red, gravel, desert, steppe,
snowy); better sets for grass and meadow (ambientCG grass instead of the brownish Poly Haven
turf), sand, rock (grey stony ground; the mossy `rocky_terrain_02` is its second set) and
forest floor; second sets for five classes. The normal maps were already in use (`micro`).
- Measured (RX 9060 XT, 1600 × 900, HUD off, fog off, zoom 1, `gputime` 2 × 30 frames, the
  `ground` segment, main → branch): battle3 grass and rock at (14720, 3456) 0.28–0.35 →
  0.29–0.39 ms; Single2 shore and desert at (7552, 6272) 0.37–0.40 → 0.39–0.45 ms; battle1
  cracked plaza at (7040, 8064) 0.30–0.37 → 0.32–0.37 ms; the blank snow field with patches
  0.31–0.35 → 0.30–0.36 ms. About +0.02 ms: a second set costs no extra sample, only cache
  misses on another layer.
- Frames: `ACR-analysis/genai/ground/` (`sheet_<scene>.png`: main and branch at zoom 1,
  zoom 4 and the free camera; `steps/`, `before/`, `after/`).
- `snow_01` showed regular ripples at zoom 1, so `snow_02` stays.
- Not done: second sets for the other ten classes (11 MB per layer natively), a shore class
  apart from the desert tiles, triplanar mapping.

## Phase 2: ground state texture

One RGBA texture per map stores what happens to the ground. It replaces the per-vertex
trample darkening and the flat dirt and crater decals.

- **Resolution:** 16 cm per texel (≈ 4.8 world px). A normal map of 15 360 px is 3200²
  texels = 40 MB in RGBA8: too much for WebGL2 next to the materials. Instead:
  - **Tiles:** 64 m (≈ 1920 px) tiles of 400², allocated on the first stamp. A battle
    touches a few dozen; a page table texture maps a tile to an atlas slot, with an atlas of
    16 × 16 slots of 400² = 6400², or 8 × 8 = 3200² on WebGL2 (40 MB / 10 MB).
  - **Fallback:** where no tile exists, a coarse 1 texel per cell copy (the old per-vertex
    wear) still darkens.
- **Channels** (RGBA8 + a second R8 or RG8 for displacement):
  - R: compression / trampling (flattened, darker, smoother; also flattens grass in
    phase 5);
  - G: mud / wetness (darker, glossier through the roughness, mixes towards the mud class);
  - B: scorch (black-brown, kills the grass);
  - A: blood (soaked into soil, replacing the pool decal's colour; the pool's sheen stays a
    decal);
  - second texture R8: displacement, signed, 0.5 = level, ±25 cm (craters, furrows,
    ruts, hoof holes).
- **Stamps:** a render-to-texture pass of instanced quads with stamp shapes from a small
  stamp atlas (16 shapes, 64²), with max or add blending per channel. Stamps come from the
  render side only (`trample.rs` → a stamp list), at most a few hundred per frame:
  - **boots:** alternating left/right prints, 28 × 11 cm, compression 0.15 and
    displacement −1 cm, spaced by the gait's stride (the stride of `gait_view.rs`);
  - **hooves:** pairs 13 cm round, deeper (−3 cm), compression 0.3, spaced by the horse
    gait (walk 1.6 m, gallop 3 m), with mud splash in wet ground;
  - **cannon wheels:** two parallel ruts 8 cm wide, 1.5 m apart, continuous along the
    path, −4 cm;
  - **crawl drags and bodies:** a smear of compression and blood under ragdolls and
    crawlers, from the existing trail code (`trail.rs`);
  - **cannonballs:** at each bounce of #258, a crater with a raised rim (−20 cm / +6 cm,
    radius 0.6 m) plus a furrow along the roll to the next bounce; scorch for shells.
- **Recovery:** each second a pass fades compression and mud slowly (as the 0.9997 per tick
  of today) and keeps craters. Blood dries (darker, then fades over the corpse lifetime).
  Not saved, like trampling now.
- **Shading:** `fs_detail` reads the state:
  - compression darkens and lowers roughness, and blends towards the dirt class;
  - mud blends towards the mud class with wet darkening;
  - scorch multiplies;
  - displacement drives the normal (finite differences) and parallax.
- **Geometry:** near the camera (within about 40 m under the free camera, or at zoom ≥ 2),
  a second, finer mesh is drawn instead of the cells: 8 × 8 sub-quads per cell, displaced
  in the vertex shader by the displacement channel and the material height, with parallax
  occlusion mapping (8–16 steps, fewer with distance) for the small relief. Far away the
  plain mesh stays. In the original view, displacement only shades (normals, POM), because
  the painter's order of the ground must stay.

**Built (m2-ground2), as a first version:**
- `acr_render::ground_state::GroundState`: tiles of 256² texels of 5 px (≈ 17 cm), 1280 px
  per tile, allocated on the first stamp into an 8 × 8 slot atlas (2048² RGBA8 = 16 MB on
  every platform; the least recently stamped tile is recycled when it is full), and a
  64 × 64 `R8Uint` page table. One RGBA8 texture: R compression, G mud, B scorch,
  A displacement (±25 cm); **blood is not in it yet** (the pool decal stays).
- Stamps are painted on the CPU (ellipse, stroke, crater with rim) and only the changed
  rectangles are uploaded (`TerrainRenderer::upload_ground_state`). Holes take the deeper
  value, rims the higher, so repeated marks never dig without end. Recovery: one tile per
  tick fades compression and mud by 0.9997 per tick (dithered rounding: truncating the bytes
  took a whole step per pass, so with few tiles every path was gone in seconds); prints shallower than 4 cm level out,
  craters and ruts stay.
- Marks (`acr-game/src/ingame/ground_marks.rs`, render only, after each tick): boots
  (one print per 0.375 m, alternating feet), hoof pairs (deeper, −3.5 cm, wider apart,
  some mud), two gun wheel ruts 1.5 m apart, crawl drags, a body press where a man or
  horse fell, cannonball craters (−18 cm, rim +5 cm) with a 1.8 m furrow along the flight
  at every impact (each bounce of #258 is one), scorched shell craters, grapeshot pits.
- Detailed mode replaces the per-vertex trample darkening (cleared once, restored when the
  ground returns to original) and the flat dirt/crater decals; original mode keeps both.
- Shading (`fs_detail`): compression darkens and desaturates towards bare soil, mud darkens
  and gets a sun glint, holes are darker, scorch goes black-brown, and the displacement's
  slope (finite differences of 4 more state taps) bends the normal.
- Limits: a 17 cm texel is coarser than a boot (28 × 11 cm), so a boot print is a soft
  dent of about 25 cm, not a sole; prints read as a darker trodden band rather than single
  steps except up close. No parallax occlusion and no finer mesh yet: the relief is
  shading only. No GPU timer measurement yet.
- Frames: `ACR-analysis/ground2/detailed/` (`scripts/scene.py`: a company, three dragoons
  and a gun on patrol over grass and a mud patch, an enemy gun firing on them). The gun's
  two ruts show clearly at zoom 2; the men died early under the cannon, so their prints and
  the craters are mostly under bodies and blood in these frames. Driver note: `wait` does
  not render, so the scene records one frame first to upload the materials (the marks
  only run once they are loaded).

## Phase 3: particles

The particles must work in WebGL2, which has no compute and no storage buffers. There are
two options:
- **A, CPU-simulated instancing:** simple ballistic motion with drag, a ground hit and a
  stamp on landing, uploaded as an instance buffer each frame. That is 20 bytes per
  particle (position, velocity packed, life, kind); 4000 particles = 80 kB/frame.
- **B, GPU ping-pong:** two RGBA32F textures of position and velocity, updated by a
  fragment shader, drawn with instanced quads that read the texture in the vertex shader
  (WebGL2 allows this). It scales to 100k+, but has no CPU-side landing events, so the
  stamps would come from a second pass.

The plan is to measure A first: native and wasm at 2k, 5k and 10k particles. A battle's
cannon fire needs maybe 2–4k live clods. Choose B only if A costs over 1 ms on wasm at 5k.
Both draw instanced: clods as small low-poly lumps (one 20-triangle mesh, scaled and
tinted per instance by the ground class under the spawn point), dust and spray as soft
camera-facing quads with a depth fade.

**Effects:**
- earth clods and mud spray from hooves at a gallop and from impacts; they land and stamp
  small prints;
- dust on dry classes (dirt, sand, field) under marching columns and cavalry, density by
  compression rate;
- grass bits on grass and meadow;
- a cannonball's earth fountain at each bounce (#258, `World::impacts`): 40–80 clods plus a
  dust puff, scaled by the ball's speed; shell blasts with 150 clods, a scorch stamp and
  smoke (smoke stays the existing sprite effect);
- later: water splashes for balls on water.

**Built (m2-ground3), first version (option A):**
- `acr_render::particles`: a fixed pool of 4096 (`MAX_PARTICLES`, allocated once), swap
  removal, gravity and drag per kind, a ground hit with one bounce for clods, then a rest
  (clods 6–18 s, mud 2.5–7.5 s, grass 3–9 s), shrinking away in the last second. Kinds:
  `Clod`, `Mud`, `Dust`, `Grass`, `Spray`, `Droplet`, `Splinter` (not emitted yet). The
  `Burst`/`ParticleSink` interface of docs/water.md, refined with size, colour, scatter and
  a hash seed. Clods and mud that land leave a small print in the ground state
  (`Particles::take_landings`, at most 256 per frame).
- Drawing (`particles.wgsl`): one instance buffer (24 bytes per particle), six vertices
  per instance, camera-facing quads that keep their centre's depth pulled towards the
  camera by their radius (so a clod on a slope is not cut). Hard kinds are lumpy lit
  impostors (an outline from three sines of the seed, a bulging normal, sun diffuse, grain,
  a glint on mud and drops) drawn first with depth writes; dust and spray are soft puffs
  drawn after, without depth writes. Drawn after the water in the 3D pass. No shadows, no
  depth fade against the ground.
- Emitters (`acr-game/src/ingame/particle_view.rs`, render only, hashes of the impact or
  footfall): a cannonball's earth fountain thrown forward along the flight (35–105 clods
  plus fine earth, grass bits on turf, a dust puff, scaled by the speed in `flight_dir`; each
  bounce is an impact), a shell's ring of 160 clods and dark dust, a musket ball's small puff
  and crumbs, grapeshot pits, mud flicked back from hooves on wet ground (the mud class or
  the ground state's mud), dust under hooves on dry ground, a column's haze (one puff for
  about every third step on dirt, sand and field), and balls into water (see
  docs/water.md). The soil colour comes from the class under the point
  (`TerrainData::tile_at`); mud replaces clods on wet ground. Earth needs `ground=detailed`
  (marks on), water `water=detailed`.
- Cost (release, `cargo test --release -p acr-render particle_cost -- --ignored
  --nocapture`): about 4000 live (half clods, half dust) step plus instance fill
  ≈ 0.066 ms per frame on the Snapdragon X, well under the 0.5 ms budget. The GPU cost is not
  measured (no timestamp queries yet).
- Driver: `{"cmd":"impact","at":[x,y],"dir":[dx,dy],"speed":12,"cause":"cannonball"}`
  records a round striking the ground or water that hits no one, so craters, furrows and
  fountains can be filmed without casualties. Scene: `ACR-analysis/ground3/scripts/scene.py`.

**Impact realism and GPU timer (m2-ground4):**
- **GPU timer** (`acr_render::gpu_timer`, driver `{"cmd":"gputime","frames":n}`): timestamp
  queries at the pass boundaries, mean ms per segment over `n` offscreen frames. It needs
  `TIMESTAMP_QUERY` and `TIMESTAMP_QUERY_INSIDE_ENCODERS` (requested where the adapter has
  them; WebGL2 replies `"supported": false` with the reason). Timestamps inside a pass were
  useless on the Snapdragon X (Adreno, a tiler: all work of the 3D pass landed on its first
  mark), so while timing the 3D pass is split at each segment into passes that keep depth
  and stencil (a little extra load/store; one pass as before when not timing). Segments:
  `terrain` (palette pass + present), `ground` (detailed material pass), `shadow map`,
  `terrain depth`, `models`, `ground shadows` (shadow decals), `water`, `particles`,
  `sprites`, `interface`.
- **Measured** (Monongahela `battle3`, 1600 × 900, Snapdragon X, DX12, about 2700 models,
  shadows on, after 300 ticks, mean of 60 frames; `ACR-analysis/ground4/timing-*`):

  | ms | z1 detailed | z1 original | z2 detailed | z2 original |
  |---|---:|---:|---:|---:|
  | terrain (palette + present) | 0.33 | 0.32 | 0.18 | 0.17 |
  | ground (material pass) | 1.68 | – | 1.68 | – |
  | water | 0.16 | 0.01 | 0.12 | 0.01 |
  | shadow map | 93.3 | 92.8 | 90.7 | 90.9 |
  | models | 88.8 | 86.4 | 88.0 | 87.6 |
  | ground shadows | 0.39 | 0.39 | 0.24 | 0.24 |
  | particles (none live) | 0.01 | 0.01 | 0.01 | 0.01 |
  | sprites | 0.39 | 0.22 | 0.32 | 0.15 |
  | total | 185.2 | 180.2 | 181.4 | 179.2 |

  The detailed ground and water cost about 1.8–2 ms; the models and their shadow map cost
  about 180 ms, two orders of magnitude more: that is where a battle's GPU time goes (a
  matter for the model renderer, not the ground). On the blank field at 1280 × 720 during
  one impact: particles 0.36 ms (zoom 2), 1.2 ms (free camera, zoom 4, close-up fill of
  the big dust puffs), 0.15 ms (shell, zoom 2); the material pass 3.1–4.4 ms there
  (the whole screen is detailed ground).
- **Cannonball:** a sheet of earth thrown 2–4 m up and forward: a fan of five narrow bursts
  (±0.55 rad, the sides lower and shorter) of mid-size clods (11–23 cm), torn sods (new kind
  `Sod`: a slab, 20–40 cm, grass on its top edge, matted roots, earth below; big lumps on
  bare ground), crumbs, grass bits, a quick dust burst and a brown-grey cloud that hangs
  5–8 s.
- **Shell:** the ring of clods and sods, a dark column of earth and smoke straight up, a
  dust ring rolling out along the ground, and a hanging cloud.
- **Dust and spray:** three octaves of value noise (procedural, no texture) shape and dent
  each puff, turning slowly; lit as a dented ball, denser parts darker. Soft contact: a
  puff fades out towards the ground by the height of each fragment above it (the
  particle's clearance from the CPU step plus the quad's up vector), so there is no hard
  line where it meets the ground. It is not a depth-buffer fade: the 3D pass cannot sample
  its own depth attachment (and WebGL2 would need a copy); against models a puff still
  cuts.
- **Craters and furrows:** the ball's crater is 0.55 m, −24 cm with a +8 cm rim, the furrow
  2.4 m long, 22 cm wide, −12 cm. The ground shader shows dug ground (holes and rims) as
  bare brown soil and darkens holes more (up to 45 %). The crater carries no extra mud: the
  fountain reads the state under it and would turn to mud.
- **Frames** (`ACR-analysis/ground4/`, blank field 1280 × 720, HUD off, one round per
  driver process, `scripts/impact.py`): `sheet_z2.png`, `sheet_free.png`, `sheet_shell.png`
  (before = main, after = this branch, ticks 2–22 then +200) and `pair_*_t6.png`.
- **Not done:** the battle3 per-class colour calibration after the desaturation, and
  triplanar mapping.
- **Honest look:** better than the crumbs and blob: the fan reads, the furrow and crater
  read at zoom 2, the dust has body. Still impostors: at zoom 2 a sod is a few pixels and
  its grass top barely shows; the clods are flat-lit discs without shadows; the dust is
  still a sprite cloud rather than a volume.

## Phase 4: dung

Horses and other animals drop dung over time. A render-side hash per (unit id, minute)
decides: about one drop every 6–10 minutes per horse, using no simulation state and no
RNG. A drop is a small mesh (3 variants, about 60 triangles, made in Blender like the
other assets) plus a stamp in the ground state. It ages: fresh and glossy, then dry and
dark, flattened when trampled (when the compression channel rises over it), and gone
after about 20 minutes. Its cost is negligible: a few hundred instances at most.

## Phase 5: ground cover (built) and trees (plan)

**Ground cover** (`acr_render::ground_cover`, `ground_cover.wgsl`; m2-ground-cover). Setting
`ground_cover=on|off`, default `on`; it draws only with the detailed ground and its marks
(`ground=detailed`, 3D mode, textures loaded), not in the zoomed-out view and not under
`weather=snow`. `ACR_GROUND_COVER=on|off` overrides it on native builds (tests, timing).

- **Placement:** a jittered grid of 12 px (40 cm) cells. Each cell's hash of its grid
  position decides whether it holds a piece (the class's density), the kind, the spot in
  the cell, the size, the turn and the tint variation: the same field always grows the same
  cover, independent of the sim. The class is the one of the vertex under the piece
  (`TerrainData::tile_at` → `TILE_CLASS`).
- **Per class** (`COVER`): grass 55 % of cells (5 % of them flowers), meadow 65 % (14 %
  flowers), forest floor 50 % (litter 55 %, ferns 30 %, grass), steppe 50 % dry grass, field
  15 % dry grass, dirt 10 %, rock 16 % and gravel 16 % mostly pebbles, desert 9 %, sand 4 %,
  cracked and red soil 5–6 % dry tufts and pebbles, mud 3 % grass; snow, snowy ground and
  the kept original textures none.
- **Cards:** no textures and no asset-pipeline meshes; the shapes are cut in the fragment
  shader. Upright kinds (grass 16–30 cm, dry grass 22–42 cm, flowers, ferns 32–55 cm) are
  three crossed cards (18 vertices) with 5–9 tapering blades, flower heads in four colours
  on stems, or two arching fronds with leaflets; blades stay at least about a pixel wide.
  Flat kinds (leaf litter, pebbles) are one card on the ground with five leaves or three lit
  lumpy stones. Opaque (alpha-tested, depth-written), drawn after the ground shadows; they
  do not receive model shadows yet.
- **Wind:** the models' `globals.wind` (`wind` setting, scaled by the weather): a slow sway
  rolling over the field plus a flutter, bending each card by its height squared (25 % of
  the height at the tip at strength 1).
- **Fade:** each piece has a rank (its hash over the density); the shader grows a piece in
  and out as the zoom fade (0 below zoom 0.55, all from 1.3) passes its rank, so the
  gameplay zoom shows a thinned, fine-grained cover and zooming in fills it without pops.
  The free camera also fades it out between 55 and 110 m from its target.
- **Interactions (ground state):** where the compression channel (boots, hooves, bodies)
  rises, the cards are laid over and darkened (fully above 0.45) and thinned (from 0.25);
  scorch over 40/255 (shell craters, `fire_view` fires) and dug ground (displacement beyond
  ±14/255 ≈ 2.8 cm: craters, furrows, ruts) clear it. Chunks of 16 × 16 cells are cached
  and re-placed every second (4 stale chunks per frame, new chunks up to 96 per frame), so
  marks reach the cover within a second or two.
- **Cost** (RX 9060 XT on the render VM, 1600 × 900, blank field with a meadow patch of
  radius 2600 px, 240 men marching, zoom 1; `ACR-analysis/genai/groundcover/steps/
  bench.json`, two runs): GPU `ground cover` 0.136–0.147 ms (off 0.007); CPU
  `3d.ground_cover` 0.11 ms per frame (6.6–6.8 ms over 61 frames). Under the 0.3 ms target,
  so on by default. Not measured on the Snapdragon tiler or WebGL2 yet.
- **Frames:** `ACR-analysis/genai/groundcover/sheet_{meadow,forest,desert,steppe}.png`
  (rows off/on, columns zoom 1, zoom 3, free camera; `scripts/steps.py`, `sheets.py`).
- **Limits:** the driver's `wait` renders nothing, so marches under `wait` leave no boot
  prints; trampling is shown by the unit test only (`trodden_and_burnt_ground_thins_the_cover`).
  No shadows on the cards. In the original view depth is the row, so a card is pulled 3 px
  towards the camera against the slope behind it.
- **Trees:** Poly Haven CC0 trees (`D:\ACR-models\polyhaven`) built into glb with 3 LODs
  plus an impostor (an octahedral or 8-view billboard atlas). The tree sprites are replaced
  by them in 3D mode, keeping the map's positions and kinds. A cannonball hit (render-side
  from `World::impacts` near a tree) fells the tree: a pivot at the base, a fall
  animation, and the log stays. The simulation's tree (wood resource, passability) is
  unchanged, so a felled render tree may still count as wood. That is acceptable as
  render-only, and it is documented.

## Phase 6: performance budgets

| | native (dGPU / Snapdragon X) | WebGL2 |
|---|---|---|
| material pass | ≤ 1.0 ms at 1080p | ≤ 2.0 ms at 1080p, 512² materials |
| ground state stamps + fade | ≤ 0.3 ms | ≤ 0.5 ms, 3200² atlas |
| fine mesh + POM near camera | ≤ 1.0 ms | POM steps halved, fine mesh only at zoom ≥ 3 |
| particles | ≤ 0.5 ms at 5k | ≤ 1.0 ms at 3k (CPU) |
| grass cards | ≤ 1.0 ms | off by default |

**Scaling:**
- `ground=original` is the original path: no extra pass, no textures loaded.
- `ground=detailed` scales with the zoom: below zoom 1 the material mips reach the mean,
  and the pass could then be skipped (to do: skip at zoom < 0.75 and in the zoomed-out
  view, which already happens).
- POM and the fine mesh only near the camera; particles are capped per frame and their
  count halves on wasm.
- A later `ground=detailed-low` could keep phase 1 without phases 2–5.

## Status

- Phase 1 is built: setting, classes, CC0 sets, packing, loading, drawing, the blank
  field's `patches`, frames and a first measurement.
- Phase 1 finish partly built (calibration from the battle3 measurement, `dirt` set, skip
  below zoom 0.75); phase 2 first version built (state texture, stamps, shading).
- Phase 3 first version built (m2-ground3): the particle pool, renderer and emitters, the
  driver's `impact`.
- Impact realism and the GPU timer built (m2-ground4).
- Next: re-measure battle3 colours, triplanar, a scene where the marks are not under casualties,
  blood in the state, POM and a finer mesh near the camera, a GPU timer, particle
  shadows and splinters, then dung.
- Earlier plan: calibrate the tint and shade against the original (a mean-colour check per zoom
  on the blank field), triplanar on steep slopes, and a skip below zoom 0.75. Then phase 2
  (the ground state texture with boot and hoof stamps and cannonball craters and furrows),
  then phase 3.
