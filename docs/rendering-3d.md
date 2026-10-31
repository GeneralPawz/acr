# 3D rendering: models next to sprites, the original camera as a matrix

Milestone M2 (#176) replaces sprites by 3D models step by step. The renderer (#163) draws
glTF models where a model exists and keeps the sprites everywhere else; the camera (#164)
reproduces the original projection exactly, so the HUD, picking, the minimap and edge
scrolling work unchanged, and offers a free camera as a setting. Corpses decompose instead
of fading (#178) behind the blood and gore setting (#183). Nothing of this touches the
simulation: models are chosen, posed and decayed from the same unit state the sprites are
drawn from (animation, frame, `RealDir`, death counter), so replays and savegames are
unaffected. Asset conventions: [assets.md](assets.md); the human models and their decay
parameters: [assets-humans.md](assets-humans.md).

## Settings

`settings.txt` (`%LOCALAPPDATA%\ACR\` or `ACR_SETTINGS_DIR`) or, in the browser, the
`localStorage` entry `acr-settings` (base64 of the same lines):

| key | values | default | |
|---|---|---|---|
| `models` | `off`, `units`, `all` | `off` | `units`: units with a model are drawn in 3D; `all`: buildings too: under construction and upgraded (#302), finished, burning down, falling in and as ruins (#303), with their hit damage (#304) |
| `camera` | `fixed`, `free` | `fixed` | `free`: the camera turns and tilts (below); the modern controls' Ctrl + Space, C switches it for the running battle |
| `shadows` | `on`, `off` | `on` | sun shadows of the 3D models on the ground and on each other (below); tests switch them with `ACR_SHADOWS=on\|off` or the driver's `{"cmd": "shadows", "on": false}` |
| `material_detail` | `on`, `off` | `on` | the models' glTF normal maps and a faint highlight from their roughness and metallic (below, "Material detail (#234)"); `off` is the plain Lambert look; tests switch it with `ACR_MATERIAL_DETAIL=on\|off` or the driver's `{"cmd": "material_detail", "on": false}` |
| `wind` | `on`, `off` | `on` | wind sways trees and foliage that carry `_WIND` weights (below, "Wind"); the driver's `props` request takes `"wind": false` |
| `map_models` | `on`, `off` | `on` | the map's trees and stones are drawn as models where `assets/nature/map_objects.json` names one (below, "Map trees and stones"); the driver's `{"cmd": "map_models", "on": false}` switches it |
| `graphics` | `auto`, `low`, `medium`, `high`, `ultra` | `auto` | the graphics preset ("Graphics presets" below): the frame's triangle, draw and shadow-caster limits, and the single switches once when chosen; `auto` takes the adapter's (`ACR_GPU_CLASS` stands in for it, native) |
| `lod` | `on`, `off` | `on` | the models' distance levels, built at load by `lod.rs` or carried by the glb ("Authored levels"); `ACR_LOD` overrides it |
| `water` | `detailed`, `original` | `detailed` | the water surface in 3D mode, [water.md](water.md); native override `ACR_WATER` |
| `ground` | `detailed`, `original` | `detailed` | the ground in 3D mode (`models` not `off` or `camera=free`): material textures over the original tiles, [ground.md](ground.md); sprite mode always draws the original |
| `ground_cover` | `on`, `off` | `on` | grass tufts, flowers, ferns, leaf litter and pebbles on the detailed ground, swaying with the wind (`ground_cover.rs`, [ground.md](ground.md) "Phase 5: ground cover"); needs `ground=detailed`, hidden zoomed out and under snow; `ACR_GROUND_COVER=on|off` wins (native) |
| `gore` | `full`, `reduced`, `off` | `full` | 3D corpses: `full` decompose (#178), `reduced` discolour, collapse and sink without dissolving, `off` fade like the original (every other pixel once the death counter passes 100) |
| `corpse_minutes` | 1 to 60 | `5` | how long a corpse lies before it goes (game minutes) |
| `corpse_decay` | `full`, `half` | `full` | `half`: decay stops at `model::HALF_DECAY` (0.45), for looking at thrown flesh; the driver's blank start takes `"corpse_decay"` too |
| `battlefield_marks` | `fade`, `keep` | `fade` | `keep`: craters, scuffs, blood splatter and pools and the debris of the fallen stay for the whole battle (below, "Kept battlefield marks") |
| `decay_stain` | `on`, `off` | `on` with `gore=full`, else `off` | the fluids of decaying corpses stain the ground under them (below, "Decay stains"); written only once set |
| `weather` | `off`, `rain`, `snow`, `fog` | `off` | rain, snow or fog over the 3D frame, a look only with no gameplay effect (below, "Weather (#224)"); the driver's `{"cmd": "weather", "mode": "rain"}` overrides it (without `mode`: back to the setting) |
| `hit_locations`, `body_spacing`, `mortars`, `corpse_piles`, `cannon_fire` | `on`, `off` | `off` | the optional `acr_sim::Rules`, page 4 of the options; they apply from the next battle ([simulation.md](simulation.md)) |

Sprites stay the default; with `models=off` and `camera=fixed` the frame is drawn exactly as
before (no depth buffer, no extra pass). The game menu's options dialog (Spieleinstellungen)
sets these on its pages 2 to 4 ([ui.md](ui.md#game-menu-0x5d9c30-and-game-options-0x5dbe10));
`gore`, `corpse_minutes`, `corpse_decay`, `decay_stain`, `battlefield_marks` and the graphics take effect at
once, the four rules (page 4) from the next battle.

### Decay stains

`decay_stain=on` (`ingame/seep.rs`, `blood.wgsl` `seep`, render only): the "cadaver
decomposition island" of forensic taphonomy. From bloat on, purge fluid leaks from mouth
and nose, then through the ruptured skin, into the soil under head and torso; the stain
grows over active decay, wet and greasy, nearly black-brown with a greenish-grey rim, and
dries to a dark brown, cracked crust; on grass (`particle_view::Soil::turf`) the plants
around it yellow and die in a patchy ring. It is never red, and lies under every other decal
(blood is drawn over it).

- One decal per corpse (`Dirt::Seep`, kind 5 in `blood.wgsl`), keyed by unit: a soft,
  noisy ellipse 1.6 times as long as wide (about 0.95 × 1.5 m of dark core for a man, the
  dead grass to about 1.3 × 2.5 m; big animals up to 1.8 times by their girth).
- Its clock is the corpse's decay (`model::corpse_decay`): from 0.08 (`seep::ONSET`) a
  patch under the head, spreading to the whole island at 0.40 (`FULL`), drying over
  0.46–0.80, the grass dying over 0.15–0.55. With `corpse_decay=half` (decay stops at 0.45)
  it reaches its full, wet stage.
- It lies under the body as it lies: under the ragdoll's hips and neck once the body
  physics holds it (`Seeps::place`, then it stays), else along the facing, 0.6 m ahead of
  where the man stood. It never shrinks or gets wetter again.
- When the corpse goes (`corpse_minutes`) the stain stays: kept with
  `battlefield_marks=keep` (merged like the other kept marks), else fading over 5 minutes
  of game time (`seep::FADE_TICKS`). Large remains (body halves, `debris.rs`) get a smaller,
  rounder one that goes with them.
- `gore=off` never stains. Driver: blank start `"decay_stain": "on"|"off"`.

Cost (release, `ACR_PROFILE`, blank field, 200 men shot, decay 0.3–0.5, 1600 × 900,
Snapdragon X; `ACR-analysis/seep/perf.py`): `3d.seeps` 0.012 ms per frame, the decal pass
(`3d.debris_decals`, with the sort that draws the stains first) 0.20 → 0.22 ms; GPU 18.09 →
18.24 ms per frame (models pass with the decals 11.50 → 11.76 ms).

### Kept battlefield marks

`battlefield_marks=keep` (`ingame/kept_marks.rs`, render only, the simulation and its
hashes unchanged). The decals of hits and ground impacts are derived each frame from
`World::hits` / `World::impacts`, which the simulation drops after `HIT_KEEP` (750 ticks);
corpse pools go with their corpses, debris after `corpse_minutes`. With `keep`:

- a hit's splatter and mist and an impact's crater or scuff are copied, dried and fully
  visible, 500 ticks after the hit (`SETTLED`) and drawn from the copy from then on;
- a corpse's pool is copied when the corpse goes (its biggest state);
- debris (severed pieces, gibs, dropped gear) no longer expires; the newest 1500 stay
  (`debris::KEEP_CAP`), rotting down to bones, their pools no longer fading;
- on the detailed ground craters, furrows and scorch are stamps in the ground state
  (`ground_marks.rs`) whose displacement and scorch stay with either setting (only the
  compression and mud recover); the atlas is fixed (64 tiles of 256², 16 MB), the least
  recently stamped tile is recycled.

Bounded: a mark landing on a kept mark of the same kind (blood pool, droplet, smear,
crater, scuff) within 0.6 of its radius merges into it (areas add, up to 1.8× its first
radius) instead of adding a decal; beyond 8192 kept decals (`kept_marks::CAP`) the oldest
go. Kept decals are laid on the slope once, when kept.

Cost: one kept decal is a `BloodPool` (about 76 bytes, plus its first radius) on the CPU
and one 64-byte `GpuPool` instance re-uploaded each frame with the others; at the cap about
1.2 MB in all. Drawing is the decals' instanced quads (`blood.wgsl`), as for live ones.
Measured on the blank editor field (release, `models=all`, `ground=original` so impacts are
decals, HUD off; `ACR-analysis/settings/marks_bench.py`, 300 frames after the simulation
dropped every record):

| run | kept decals | `3d.update` CPU ms/frame | GPU ms/frame |
|---|---|---|---|
| 600 impacts, `fade` | 0 | 0.27 | 8.0 |
| 600 impacts, `keep` | 486 (114 merged) | 0.31 | 5.5 |
| 9000 impacts over 10000 × 6700 px, `fade` | 0 | 0.36 | 5.6 |
| 9000 impacts over 10000 × 6700 px, `keep` | 3085 (1011 merged) | 0.45 | 2.0 |

(The simulation caps the impacts it records within `HIT_KEEP`, so not all 9000 reach the
renderer.) About 0.03 ms CPU per 1000 kept decals; the GPU time on this machine varies by more
than the decals cost (the terrain pass alone moved 0.2–4.5 ms between runs), so it is not
separated. The cap (8192) bounds both; at that density a crater-strewn field merges long
before it.


**Free camera keys** (`camera=free` only): `,` and `.` turn the view, Page Up / Page Down tilt
it (15°–80°), End returns to the original angle; with the modern controls Shift + right drag
turns and tilts it too ([controls.md](controls.md#mouse)). The camera turns about the ground point at
the window centre; scrolling (arrow keys, window edge) follows the turned screen.

## Model files

A unit or building type whose `.MD` `NAME` has a file `<name>.glb` (lower case, `EngM18` →
`engm18.glb`) is drawn as that model. The files are looked up as game files at
**`ACR\Models\<name>.glb`**:

- in the game folder (`<game folder>\ACR\Models\`), natively and in the browser (`cargo xtask
  serve --game-dir` lists every file below the game folder; the folder picker passes them
  too);
- natively also in the folder `ACR_MODELS_DIR` names, else `ACR\Models\` next to the
  executable when that is not the game folder; these take precedence (for development and
  side-by-side runs).

Images are embedded PNG or JPEG (`image/png`, `image/jpeg`; `model.rs` `decode_png`,
`decode_jpeg`), both expanded to RGBA8 on load. JPEG goes through `zune-jpeg` (pure Rust,
native and wasm; about 0.2 MB of the native release binary). The asset pipeline stores
opaque base colours as JPEG and everything else as PNG (assets.md "Packing").

KTX2 textures (#297): an image of `image/ktx2` named by a texture's `KHR_texture_basisu`
`source` (`model.rs` `texture_image`) is Basis Universal (ETC1S or UASTC, zstd), written by
`assets/blender/lib/acr_assets/ktx2_pack.py`. `ktx2.rs` transcodes it with the pure-Rust
`basisu` crate (Apache-2.0, byte-exact port of Binomial's transcoder, builds for wasm) when
the renderer uploads it, to the best format the device asked for (`acr-app` requests
`TEXTURE_COMPRESSION_BC`, `_ETC2`, `_ASTC` where the adapter has them; `Target::of`): BC7,
else ASTC 4 × 4, else ETC2 RGBA, else RGBA8. The file's mips are uploaded as they are (no
`mipgen` pass); sRGB or linear as for PNGs. Streamed KTX2 pictures (texture streaming below)
transcode on the `acr-texture` thread natively and on the main thread in the browser (no
`createImageBitmap` for KTX2). One base size not a multiple of 4 goes to RGBA8.

Measured (render VM, RX 9060 XT, BC7; blank white field, an `EngM18` close-up and a killed
one, `EngCen`, `oak` and `broadleaf` props; steps `ACR-analysis/ktx2/steps.json`, the
driver's `textures` and PSNR over the content box against the PNG/JPEG glbs):

| | PNG/JPEG | KTX2 |
|---|---|---|
| showcase glbs (310 files) | 670.3 MB | 481.6 MB |
| texture GPU memory, regular scene | 154.8 MB | 38.7 MB |
| same with `hdengm18` as `EngM18` (loaded + streamed) | 161.7 + 55.9 MB | 54.5 + 14.0 MB |
| `engm18.glb` parse | 22.5 ms | 3.9 ms (the transcode moves to the upload) |
| first 20 frames (loads and uploads) | 38 ms | 45 ms |

PSNR KTX2 against PNG: unit 50.1 dB, face 43.7 dB (max delta 31 of 255), building 58.5 dB,
trees 49.5 dB (max 4), gore 77.5 dB; HD musketeer face 45.3 dB (max 23).

Close-ups (same machine, blank white field, zoom 4–5 and 14; steps
`ACR-analysis/ktx2/roll/closeup.json` and `tree_*.json`, `cmp.py` there): gore 46–60 dB
(severed arm, wound volumes, decay at +0, +1200, +2700 ticks), `EngM18` face 41.9 dB, birch
leaf cards 33–38 dB and horse mane and tail 32–36 dB, where the only large deltas are single
pixels at the alpha-mask edge flipping (0.1–0.25 % of the silhouette; the same with UASTC
without RDO, so not the RDO); the low-poly `broadleaf` and `pine` have no cards (47–51 dB).
The HD face matches once its streamed levels are in; while they stream the two show
different authored levels (KTX2 streams a little later).

Browser (Playwright Chromium, WebGL2 over ANGLE Vulkan on the RX 9060 XT, `tools/browser/run.sh
--unit EngD18`, two runs each, no console errors, 52.4 dB at zoom 5 against PNG):

| `EngD18` (horse and rider) | PNG | KTX2 |
|---|---|---|
| download (`engd18.glb`, `gore.glb`) | 6.08 + 1.71 MB | 5.79 + 1.70 MB |
| `engd18` parse + upload (transcode) | 111 + 205 ms | 19 + 206 ms |
| texture GPU memory | 312.7 MB | 12.8 MB |

Each part split off a model (`three_d.rs`: unarmed body, dropped gear, building states) is a
model of its own with all the images. KTX2 pictures are transcoded once per file and the
parts share the texture (`ModelRenderer::ktx2_views`); before that `engd18` took 1006 ms to
upload in the browser and 78 MB. PNG pictures are still uploaded once per part.
Native, all 310 models loaded in one run (`loadall.json`): parse 4.1 s → 0.6 s, upload
14.5 s → 18.1 s (sums over the models).

Files are requested when a type with a model first appears and only when `models` is not
`off`; a file that fails to load is logged once and the type stays a sprite.

## Architecture

```
acr-render
  math.rs            column-major 4×4 matrices, quaternions
  view3d.rs          View3d: the original projection as a matrix, the free camera (Orbit),
                     ground picking, model placement (ground_point, unit_ground_point:
                     units 6 world px south of their anchor, #196; facing_yaw, model_matrix)
  model.rs           ModelData::from_glb: meshes, materials, skins, clips; pose sampling;
                     clip choice from .MD animation names; corpse decay mapping
  model_renderer.rs  ModelRenderer (GPU skinning, instancing), DepthTarget (the 3D pass)
  model.wgsl         skinning, morph targets, decay, light
  terrain.rs/.wgsl   + vs_depth (terrain depth for the 3D pass), free-camera ground
  scene.rs           + Placement: sprite depth and free-camera placement
  objects_view.rs    + prepare/draw split for the 3D pass
acr-game
  ingame/three_d.rs  settings, model lookup and loading, instances from the simulation,
                     the frame's 3D pass, free-camera keys and scrolling
  ingame/piles.rs    corpse piles: lift, tilt and drape of overlapping corpses, cached
  ui/settings.rs     models / camera / gore
  native.rs          ACR_MODELS_DIR
```

### One frame with models or the free camera

1. **Terrain colour** as before (palette ground target, present pass). Under the free camera
   the ground triangles go through the 3D view matrix instead and are depth-tested among
   themselves (hills no longer come in row order); the water tint, which works on screen
   blocks of the original view, is left out (the water surface below replaces it).
   With `ground=detailed` and its textures loaded, the mesh is drawn once more straight
   into the output over the original (`ground.wgsl`, [ground.md](ground.md)).
2. **Shadow map** (`shadows=on`, models drawn): the models once more from the sun into a
   depth texture (below).
3. **3D pass** (one render pass, depth + stencil `Depth24PlusStencil8` at output size):
   1. *terrain depth* (`vs_depth`): the same mesh through the same matrix, at the plain ground
      height (the drawn ground is lifted by up to 7 px of random relief) and pushed back by
      24 world pixels, so a model's feet are not cut;
   2. *models*: skinned, instanced, depth test and write; their pixels set stencil 1; lit by
      the sun unless the shadow map says it is blocked;
   3. *ground shadows* (`vs_ground_shadow`): the terrain mesh again, placed as in step 1 with
      a depth test, darkening the drawn ground where the shadow map has a caster;
   3b. *water surface* (`water.rs`, docs/water.md): Gerstner waves, depth colour over the
      lakebed, Fresnel sky and shore foam, depth-tested and not written; with
      `water=detailed` it replaces the tint of step 1 (also under the free camera);
   3c. *world fog* (free camera with fog of war only, #309): the pass is split here and
      `fog_world.rs` reads the depth (below, "Fog of war under the free camera");
   4. *ground reset*: a full-screen triangle at the far plane where stencil is 0, so the
      sprites draw over the terrain as in the original;
   5. *sprites* in their usual order with a depth test (no writes) against the models.
4. Fog of war and interface as before; under the free camera the screen overlay is left out
   for the world fog of step 3c.

### Fog of war under the free camera (#309)

The original's overlay (`fog.rs`, 0x4bee1e) darkens screen cells of the original view, so
the fixed camera keeps it unchanged. Under the free camera the fog lies on the world
instead:

- `three_d.rs` `ground_fog` moves the fog grid (`acr_sim::fog::Fog::cells`, 128 px cells
  stamped at the projected point `y − 2·h`) onto the ground once per frame: cell `(cx, cy)`
  takes the grid's value at its ground point's projection, interpolated along `y`. Visibility
  itself is untouched: hidden enemies stay hidden by the same rules as in the 2D view.
- `fog_world.rs`/`fog_world.wgsl`: after the water, particles and mist, the 3D pass ends; a
  full-screen pass reads its depth (the depth texture now also has `TEXTURE_BINDING`, read
  through a depth-only view), rebuilds the world point of each pixel with the inverse of
  `View3d::world_to_clip`, interpolates the ground grid bilinearly at its `x`, `y`, turns the
  value into the darkness row of 0x484e63 and blends black of `AGEW_1DR.GRD`'s opacity, as
  `fog.wgsl` does. A model or tree is as dark as the ground it stands on, from any side; the
  edges are smooth. The 3D pass then resumes (depth and stencil kept) with the ground reset.
- Sprites, drawn after it, take the same share of black at their ground point
  (`WorldFogRenderer::black_at` → `SpriteDraw::fog`, instance location 5 of `sprite.wgsl`,
  the former padding).
- The minimap draws the free camera's footprint (the window corners' rays on `z = 0`, a
  trapezoid) instead of the rectangle (`interface.rs` `minimap_frame`, `minimap::compose`);
  a click still centres the camera, which turns about the window centre.
- Sprites of units without a model and projectiles pick their direction relative to the
  camera's yaw (`unit_sprites`, `effects::direction`), so they do not show their back when
  the camera is turned. Buildings, map objects (trees, stones), fires and other effects have
  one fixed picture and stay as they are.

Cost (render VM, RX 9060 XT, release, 1600 × 900, Miss7E01 at tick 100, free camera zoom 0.4,
yaw 45°, pitch 35°, `gputime` over 30 frames, two runs): `world fog` 0.047–0.057 ms; the other
passes unchanged within 0.001 ms with the fog off (`{"cmd":"fog","on":false}`). On a tiler the
split costs one more store and load of the colour and depth targets; only the free camera with
fog of war pays it. Steps: `ACR-analysis/genai/free-fog/shots_final.json`.

### Sun shadows

Sprites bring their shadows painted in; the models cast theirs from a sun
(`shadow.rs`, `shadow.wgsl`; `shadows=on|off`).

- **The sun** is the light the models were already shaded by, the game's
  `SetLight(-15, 14, 30)` (world: from the west, from the south, i.e. the viewer's side, and
  56° up). Shadows therefore fall east-north-east, to the right and up on the screen, about
  0.7 × the caster's height long (checked on the blank field: a soldier and a horseman at
  zoom 3, `ACR-analysis/shadowfix/`), consistent with the models' own Lambert light. Open
  point: the sprites' painted shadows (units, trees) are short blobs leaning a little to
  the left, i.e. a higher sun from the east; `shadow::LIGHT` is the one constant to change
  (it lights the models too, so both move together).
- **One shadow map**, `Depth32Float`, 2048², orthographic along the sun. It is fitted each
  frame to the models drawn (their culling margin grows so casters just off the screen still
  cast into it) within 1.1 window diagonals of the window centre: the side is rounded up to a
  half octave (at least 384 world px) and the corner snapped to whole texels, so shadows of
  still models do not swim while others move or the view scrolls. The fixed and the free
  camera are fitted the same way; under the free camera the far ground beyond the reach gets
  no shadows (they fade over the last fifth), no cascades.
- **Casters**: the model pipeline's own `vs_main` with the sun's matrix in the globals, so the
  skinning from the bone texture, physics palettes, morphs, fractures and the procedural
  collapse are exactly as drawn; `fs_shadow` (`model_shadow.wgsl`) drops what `fs_main`
  drops: alpha cut-outs, cut and torn-out parts, dissolved rot, a cap's outline; the half
  transparent fade casts a dithered half shadow. Slope-scaled depth bias (1.5) in the pass.
- **Receivers**: `sun_visibility` (3×3 taps of a comparison sampler with linear filtering,
  i.e. 36 compared texels, spread over at least 1.8 world px or one texel) with a normal
  offset of 1.5 texels and a small depth bias. On models it scales the direct (Lambert)
  term, so a shadowed side keeps the ambient half: a rider shades his horse, a bear the man
  under it. On the ground the terrain mesh is redrawn darkening by up to 42 %. Water is only
  a tint of the ground, so a ship's shadow lies on the lakebed colour, which reads right.
- **Sprites** (trees, buildings, the units without a model) neither cast into the map nor
  receive from it.
- **WebGL2**: a depth texture with a comparison sampler (`sampler2DShadow`), a uniform block,
  no storage buffers; one more `vec3` varying in the model shader (13 of the 15).

Sprites therefore keep the original painter's order among themselves and with the ground,
and meet the models through the depth buffer: each sprite is a vertical billboard at its
ground row. Ground sprites (`Layer::Ground`, and what the zoomed-out view bakes into its
ground) lie at the far plane, under every model.

### Skinning that fits WebGL2

- Every mesh is skinned to a **palette** of matrices: skinned meshes to their joints
  (global × inverse bind), static meshes (gear parented to socket bones, buildings) to their
  node. One vertex format, one pipeline: position, normal, uv, `COLOR_0`, 4 joints/weights,
  two morph target offsets and the damage coordinates `_PART`, `_LIMB`, `_GORE.x` (14
  vertex attributes with the instance data, WebGL2 allows 16).
- Poses are sampled on the CPU (translation/rotation/scale channels, step and linear;
  cubic splines use their key values) **once per distinct (model, clip, frame)**: units in
  the same animation frame share a palette. The palettes go into an `Rgba32Float` texture,
  1024 texels wide, 3 texels (affine rows) per matrix; instances carry their palette's first
  texel. No storage buffers, no uniform arrays.
- **Instance data** follows the palettes in the same texture; an instance carries its first
  texel (`extra.y`): wounds (2 texels), spray (centre, radius), spray look (amount, age,
  damaged); a damaged instance adds the 12 texels of `damage::pack` (below). The instance
  buffer holds only the transform, the decay/morph/transparency parameters and the two
  texel indices (locations 9–13).
- Instancing: all instances of one model type are drawn with one `draw_indexed` per
  primitive (13 for `engm18.glb`). The instance buffer is bound from each model's first
  instance (WebGL2 has no base instance); indices are pre-offset (no base vertex).
- No back-face culling: clothes are thin shells seen from both sides once they tatter; the
  normal is flipped towards the viewer for lighting.
- Light: Lambert from the game's light vector `(-15, 14, 30)` (upper left, towards the
  viewer) with half ambient; colours are linear, sRGB is encoded by the target or the shader.
  With `material_detail` the glTF normal map bends the normal and a faint highlight is added
  (next section).

### Material detail (#234)

`model.rs` reads per material the glTF `normalTexture` (and its `scale`), the
`metallicFactor`, `roughnessFactor` and `metallicRoughnessTexture` (G roughness, B metal)
into `Surface`; images used as either map are uploaded linear (`Rgba8Unorm`), the others
sRGB. The material bind group has them at bindings 3 and 4 (the white texel where a map is
missing), the material uniform their factors in `pbr`. `globals.params.w` is the setting.

- **Tangents:** none from the file and no attribute: all 16 vertex locations are in use on
  WebGL2 (15 is `_WIND`). `model.wgsl` `bumped` builds the cotangent frame from the
  screen-space derivatives of the world position and the uv (Schüler, "Normal Mapping
  Without Precomputed Tangents"), so mirrored uv islands come out right; the map's +y points
  to −v (glTF). The derivatives are taken at the top of `shade` (uniform control flow), the
  maps sampled with `textureSampleLevel` (they have one level). The relief fades out from
  one texel per pixel to three, before the unmipped map would shimmer; the shadow lookup
  keeps the geometric normal.
- **Highlight:** `specular`, GGX (Smith-Schlick, Schlick Fresnel) from the sun, times its
  visibility, at physical strength but with the roughness held at 0.45 or more: on the
  usual rough dielectrics (Blender's 0.5, genfit's 0.85) it stays a faint sheen of at most
  about 7/255 per channel on the coats and the horse in the close-ups below, invisible at
  gameplay zoom. Generated and nature sources bake their shading into the albedo and some
  say metallic 1: metal only tints the highlight towards the albedo (at most half way) and
  never darkens the diffuse, so nothing turns black. The cloth sheen, the gloss of
  leather and brass and the wet highlights stay as they were and take the bent normal.
- **Off:** no normal map and no highlight, the frame as before the change (the far levels
  never draw either).
- **Assets:** the pipeline keeps normal maps (tangent space, OpenGL convention, green +Y =
  glTF), roughness and metallic as factors (0.85 / 0, metal gear 0.5 / 0.3):
  `genfit.py` (gear and `nature.build_generated` props: the baked map at the category's
  `nrm` px, 1024 headgear, 512 others), `genbuild.py` (the source's map, or a 512 px bake of
  what the decimation took off: `normal_baked_px`), `nature.build` (the Poly Haven and
  Sketchfab source maps at 512 px through the collapse decimation: `normal_maps`; the
  procedural trees have none). Size cost: buildings +0.35–0.44 MB, ruins +0.43–0.51 MB,
  trees and rocks +0.6–1.1 MB. Checked in the game with `material_detail` on and off (relief,
  nothing black or inverted): `ACR-analysis/genai/materials/assets/`.
- **Cost** (render VM, RX 9060 XT, Vulkan, 1600 × 900, Monongahela `battle3` at tick 305,
  zoom 1, about 1700 men and horses and the forest as models; `gputime` 60 frames, two runs
  each, alternating in one process): models pass 2.965 and 2.942 ms on, 2.955 and 2.925 ms
  off; shadow map 2.79 / 2.75 on, 2.76 / 2.75 off: within the noise (no normal-mapped model
  in that scene). Steps: `ACR-analysis/genai/materials/perf.json`; close-ups:
  `closeups.json` and `sheet_*.png` in the same folder.

### Wind (trees and foliage)

Trees and foliage sway in the vertex shader from per-vertex weights baked by the asset
(`assets/nature/broadleaf.py`): `_WIND` (vec4) x trunk bend (0 at the root, 1 at the crown
top, height²), y branch weight (distance from the trunk axis / crown radius), z phase (0..1,
one per leaf clump), w leaf flutter (1 on leaves, 0 on bark). `COLOR_0` stays the decay mask
or vertex colour. The loader reads `_WIND` as `Unorm8x4` into vertex location 15 (the last
free one; 0 for every model without it, which the shader skips).

`model.wgsl` `sway` adds, in world metres after the instance transform (so the shadow map,
drawn with the same `vs_main`, sways too), with the model's scales `k = (trunk, branch,
flutter, tempo)` (`material.wind`, from the root's `extras.acr_wind`, `[1, 1, 1, 1]` without)
and the species clock `τ = k.w · t`:

- gust `g` (0..1): a field over the map on the game clock, two fronts running downwind, about
  40 m long at 5 m/s and 70 m at 3.7 m/s (`along · 0.0052 − 0.78 t` and `along · 0.003 −
  0.33 t`, world pixels), the first bent by `1.6 sin(across · 0.0021 + 0.05 t)` and the second
  tilted across the wind, summed and smoothstepped: gusts visibly roll through a wood, never
  as one even wave;
- trunk: `x · s · k.x · (0.06 · g + 0.07 · sin(1.1 τ + j) · (0.3 + 0.7 g))` along the wind:
  the lean comes with the gust, the rock at the tree's own phase `j` (a hash of `seed`);
- branches: `y · s · k.y · 0.05 · sin(2.3 τ + 2π z + j) · (0.4 + 0.6 g)` along the wind and
  half that across it;
- leaves: `w · s · k.z · 0.02 · (0.5 + 0.5 g) · sin(7 τ + 6π z + p · (0.11, 0.07))` along the
  normal.

Per species (`assets/nature/*.py` `SPEC["wind"]`, built by `acr_assets.nature`; `bend` is the
trunk's exponent in `x = (h / top)^bend`, `crown` gives crowns without leaf cards flutter on
the bark above that share of the height). Peak sway at the crown top (vertices above 90 % of
the height, gust 1, `s` 1, model scale 1; `ACR-analysis/genai/wind2/tools/amp.py` over the
built glbs), before (all scales 1, exponent 2) and after:

| Model | k (trunk, branch, flutter, tempo), bend | Height | Crown top before | After | At zoom 1 |
|---|---|---|---|---|---|
| conifer, spruce | 0.8, 0.4, 0.6, 0.8; 2.8 | 11–12 m | 0.15 m | 0.10–0.11 m (0.9 %) | 3 px |
| conifer_snow | 0.7, 0.3, 0.4, 0.8; 2.8 | 11 m | 0.15 m | 0.09 m | 3 px |
| pine (no cards) | 0.9, 0.5, 0.5, 0.85; 2.6, crown 0.55 | 6 m | 0.12 m | 0.11 m | 3 px |
| oak, old_tree, tree_large | 1.2–1.3, 1.6–1.8, 2, 0.9–0.95 | 9–10 m | 0.15–0.16 m | 0.19–0.22 m (2.2–2.5 %) | 6–7 px |
| broadleaf_ez | 1.4, 1.8, 2, 1 | 7 m | 0.16 m | 0.24 m | 7 px |
| birch (no cards) | 1.8, 2, 2.5, 1.2; crown 0.35 | 8 m | 0.14 m | 0.28 m (3.5 %) | 8.5 px |
| palm, palm_small | 2, 2–2.5, 2.5, 0.9–1; 4 | 5–9 m | 0.14–0.16 m | 0.27–0.30 m (3.4–5.5 %) | 8–9 px |
| dead_tree | 0.3, 0.3, 0, 0.7 | 8 m | 0.14 m | 0.04 m | 1 px |
| cactus, cactus_tall | 0, 0, 0 | 2–4 m | 0.06–0.09 m | 0 | 0 |
| bush, bush_small, bush_tall, shrub, fern | 2, 3, 2.5, 1.7–2 | 0.6–1.8 m | 0.04–0.06 m | 0.09–0.14 m | 3–4 px |
| reeds | 4, 2, 2.5, 1.6 | 1.4 m | 0.05 m | 0.17 m (12 %) | 5 px |

So conifers sway little and slowly with a stiff trunk, broadleaf and oak twice as much with
leaf flutter, palms bend from the top (exponent 4) with fluttering fronds, dead trees barely
move, cacti stand still, and bushes, reeds and ferns flutter at about twice the tempo (leaves
up to 13 rad/s, 2 Hz, 5 cm: no jitter up close). Map trees also scale with their fitted
`scale` (0.25–4). Rocks carry zero weights. `broadleaf.py` (the driver's `props` tree) keeps
its string `acr_wind` and so the scales 1.

`seed` is the instance's ground point, so neighbours are out of step. `globals.wind` holds the
direction (`WIND_DIR`, from the west-north-west), strength `s` (`WIND_STRENGTH` 1; 0 with
`wind=off`) and the clock (frame seconds summed in `ThreeD::tick`, wrapping at an hour).

Map trees with a model in `map_objects.json` are drawn as models (next section); the driver places tree models with `{"cmd": "props", "model":
"broadleaf", "at": [[x, y], ...], "wind": true}` (render only; a standing tree drops its
`.felled` and `.stump` nodes). Cost (render VM, RX 9060 XT, blank grass field, 420 broadleaf
trees ≈ 730 k triangles at zoom 1, `gputime` 120 frames, two runs each, steps in
`ACR-analysis/genai/wind/steps/field_{on,off}.json`): all passes 0.69–1.54 ms with wind,
0.86–0.89 ms without: no cost above the run-to-run noise. The vertex grows by 4 bytes
(120 instead of 116). The per-species tuning (gust field, `material.wind`) costs nothing
measurable either: random map seed 5, fog off, fixed camera at zoom 1 on the densest wood
`[11776, 14848]` with the 28 nature models, `gputime` 120 frames twice per run, two runs each
(`ACR-analysis/genai/wind2/steps/perf.json`): all passes 1.31–1.67 ms before, 1.31–1.68 ms
after (models 0.60–0.76 → 0.60–0.77, shadow map 0.28–0.37 → 0.28–0.36). Frame strips per
species (gameplay zoom and close-up, 6 frames 0.8 s apart, with a motion map) and of the
wood: `ACR-analysis/genai/wind2/strip_*.png`, `forest_*.png`.

### Weather (#224)

`weather=off|rain|snow|fog` (`acr-render` `weather.rs`, `weather.wgsl`; render only, no
gameplay effect, sim and replays untouched). It draws only in the 3D pass, so with
`models=off` and the fixed camera nothing changes. The numbers of each mode are in
`Weather::look`.

| Part | Where | Rain | Snow | Fog |
|---|---|---|---|---|
| Drops | `vs_drop`/`fs_drop`, last in the 3D pass, depth-tested against the models | streaks along the fall (640 px/s, 24 px blur, at least 1 px wide), one per 260 px² of ground | flakes falling 42 px/s, drifting 34 px/s downwind and swaying, one per 520 px² | none |
| Light grade | `fs_grade`: the frame × the blend constant + a lift, over terrain, models and sprites | × (0.70, 0.74, 0.82): darker, cooler | × (0.94, 0.97, 1.0) + (0.05, 0.06, 0.08): brighter, colder | × (0.90, 0.92, 0.95) |
| Distance fog | `fs_fog`: the ground point under each pixel, its distance ahead of the window centre | 0.22 at most | 0.28 | 0.55, from 500 px before the centre, scale 1800 px |
| Ground mist | `fs_mist` where no model drew (stencil 0), before the sprites; `model.wgsl` mixes the same by `exp(−z / 45 px)` so legs fade and heads stay clear | 0.06 | 0.05 | 0.32 |
| Ground | `ground.wgsl` `detail.weather` (the detailed ground only) | albedo × 0.7, a broad gloss | patchy snow on faces with `n.z` over about 0.85, less where churned or scorched, at most 40 % | slightly wet (0.3) |
| Models | `model.wgsl` `globals.weather`, `globals.fog` | albedo × 0.82, a dim broad gloss | none | mist only |
| Trees' wind | `ModelRenderer::wind` strength × `Weather::wind_scale` | × 2.2 | × 1.4 | × 1 |

- **Drops without a vertex buffer.** Each instance hashes its place in a box and falls by
  the render clock, wrapped into the box (`fract`), so the drops are fixed in the world as the
  camera moves. The box is the ground the view shows up to its height (`View3d::
  ground_bounds`, capped at 5000 px a side for the low free camera); the count is the box's
  ground area × the density, capped at `MAX_DROPS` 24000. Zoomed in, fewer drops are drawn.
  WebGL2 safe: only `vertex_index`, `instance_index` and one uniform buffer.
- **Readability.** The HUD, selection marks and health bars draw after the 3D pass and are
  untouched. The fog keeps units readable at gameplay zoom: the 1600 × 900 frame at zoom 1
  shows every regiment (`ACR-analysis/genai/weather/`).
- **The blend constant** is clamped to 0..1 by WebGL2, so the snow's brightening is the lift.
- **Mist on hills:** the models' mist thins with true height, so men on a hill stand clearer
  than the ground mist around them (low-lying fog in the valleys).

GPU cost (`gputime`, RX 9060 XT on the render VM, Monongahela `battle3`, 1600 × 900, zoom 1,
tick about 320, 60 frames, two alternating runs each; `ACR-analysis/genai/weather/modes.json`):

| mode | sum of all passes (ms) | `weather` (drops, fog, grade) | `weather mist` | `ground` |
|---|---:|---:|---:|---:|
| off | 3.27–3.30 | 0.007 | 0.007 | 0.306 |
| rain | 3.41–3.42 | 0.108 | 0.033 | 0.309 |
| snow | 3.45 | 0.080 | 0.033 | 0.385 |
| fog | 3.36 | 0.064 | 0.033 | 0.308 |

**Options:** Spieleinstellungen page 3, "Landschaft (nur 3D)", line `Wetter` (Aus
(Original) / Regen / Schnee / Nebel), at once on OK ([ui.md](ui.md#game-menu-0x5d9c30-and-game-options-0x5dbe10)).

**Storm on the wind clock** (`acr-render` `weather.rs`): the rain's strength, the lightning
and the gusts are plain functions of `ThreeD::wind_clock` (the clock the trees sway on), so
the picture and the sound agree without sharing state, and nothing reads the simulation or
its RNG.

| Function | What | Drawn | Heard |
|---|---|---|---|
| `rain_intensity(clock)` | 0.4 (drizzle) .. 1 (downpour), three sines of 53, 21 and 8.3 s | the drop count × it (`WeatherRenderer::set_storm`) | light and heavy rain crossfaded by it |
| `strike(slot)` | one strike in two of three 24 s slots (a hash of the slot): flash time `at`, thunder `delay` 0.8–6.8 s (270 m to 2.3 km), `near` 1..0, `pan` ±0.6 | `flash(clock)`: two or three flickers over 0.5 s, raising the light grade towards 1 (× 0.6) and lifting it by up to (0.10, 0.11, 0.14), brighter when near; rain only | a thunder roll at `at + delay` |
| `gust(at, dir, clock)` | `gust` of `model.wgsl`, the same numbers | the trees' sway | the wind's swell at the window centre × the weather's wind strength (`ThreeD::wind_at`; none with `wind=off`) |

**Sound** (`acr-game` `weather_sound.rs`, #224): `InGame::weather_heard` gives each frame the
weather drawn (the override or the setting; off without the 3D pass and outside a game), the
wind clock, the rain's strength, the wind at the window centre and the camera's nearness (0
zoomed out, 1/3 at scale 1, 1 from scale 4; `weather_sound::nearness`).

| Mode | Loops (volume before the effects volume and the ducking) | One-shots |
|---|---|---|
| rain | light rain 0.4 × √(1 − s) and heavy rain 0.5 × √s (s: the strength from 0.4..1 to 0..1), both × (0.4 + 0.6 × nearness); wind 0.22 × (0.2 + 0.8 g); howl 0.12 × g³ (g: the gust) | thunder at each strike's `at + delay`, volume 0.5 + 0.5 × near, a crack in the near rolls (near > 0.6), slower and deeper far ones; drips from nearness 0.5 on, up to 3 a second × the strength, volume up to 0.22, panned ±0.8 |
| snow | muffled wind 0.4 × (0.25 + 0.75 g) (0.4 × 0.2 with `wind=off`) | none |
| fog | a distant rumble 0.09 (a little more zoomed out) | none |
| off | none | none |

- **Made in code** from seeded noise at 22050 Hz on first use (no recording, nothing to
  license): light rain 6 s (1.5–9 kHz hiss, 35 bright drop ticks a second), heavy rain 7 s
  (0.5–6 kHz, 260 ticks a second, sheets swelling three times a loop), wind 10 s (120–450 Hz),
  howl 10 s (a narrow resonance wandering 360–720 Hz), muffled wind (the wind low-passed
  twice more at 240 Hz), distant rumble 12 s (below about 70 Hz in slow swells, a faint
  hiss), two near thunder rolls of 5–6 s (a crack, a 90–180 Hz rumble), two far ones of
  7–8 s (55–110 Hz, a 0.5 s rise), six drips of 0.2 s (a plink rising from 0.9–2.6 kHz by
  70 % in 30 ms after a splash tick). Loops crossfade their last 0.5–1 s into their start.
  The different loop lengths keep the layers from repeating together.
- **Ducking:** the loops (`Mixer::play_looped`) and the one-shots (`play_ambient`) are
  ambience. Each mixer block renders the other effects first and takes their RMS; the
  ambience then plays at `1 / (1 + 8 × level)`, at least 0.25, the level following at once
  and falling with 1.2 s (1/e), the gain ramped over the block. A volley takes the rain
  down to about 0.3 and it comes back in about 2 s (`Mixer::ducking`). The volume is the
  effects volume (`sound_volume`); there is no separate weather volume.
- **Proof:** `ACR_WEATHER_WAV=<dir> cargo test -p acr-game weather_captures -- --ignored`
  writes 30 s captures (`rain.wav`, `snow.wav`, `fog.wav`, 44.1 kHz stereo, through the real
  mixer, 60 updates a second), a CSV per second of strength, gust, nearness and ducking, and
  the strikes (`events.txt`). Captures and spectrograms: `ACR-analysis/genai/weather-sound/`.
  In `rain.wav`: close rain 0–8 s, the flash at 4.0 s and a far roll from 8.0 s (near 0.46),
  zooming out 8–12 s (the hiss falls, the drips stop), zoomed out 12–16 s, a gust with the
  howl at 5–8 s and 13–15 s, a stand-in volley 21–26 s with the rain ducked under it and
  coming back 26–29 s, the downpour from 19 s.
- `game.log` says `weather sound: <mode>` on each change (the driver runs without an audio
  device, so the log is its check). Tests: `weather_sound` (loops close, levels follow the
  strength, zoom and gusts, a roll after the flash, ducking under a volley),
  `weather::tests::storm_follows_the_clock`, `mixer::tests::ambience_ducks_under_the_battle`.

Open: gameplay effects (wet powder, slower marching, shorter sight in fog) as optional rules;
recorded CC0 sounds instead of the made ones, if wanted; a separate weather volume.

### Map trees and stones

`map_models=on` (`ingame/map_models.rs`, render only): the map's `TREELIST` and `STONLIST`
objects (`World::objects`, static for the whole battle) are drawn as models where
`assets/nature/map_objects.json` names one; the others keep their sprite.

- **Table** (compiled in with `include_str!`): `{"TREELIST": {"104": "broadleaf", "10-14":
  {"model": "pine", "scale": 1.2}}, "STONLIST": {...}}`. Keys are sprite indices (the frame
  of `TREES.GP` / `STONES.GP`, the line of the `.LST`) or ranges; the model name is the stem
  of `ACR\Models\<name>.glb` (`target/assets/nature/<name>.glb`); keys starting with `_`
  are comments. The models are loaded like units' (lazily, on first need) and drop their
  `.felled` and `.stump` nodes.
- **Scale:** `scale` from the table, else fitted to the sprite: the frame's opaque top above
  its ground point (`DY − min y` of the frame box, screen pixels) / cos 30° over the model's
  rest height (`ModelData::rest_height`) × 30 px/m, clamped to 0.25–4.
- **Struck by balls** (rule `ball_terrain`, `map_models::struck_transform`, from
  `World::struck`): a felled tree tips over about its foot towards the ball's flight
  (`FALL_TICKS` 24, ease-in, to `FALL_ANGLE` 1.48 rad) and lies; a knocked stone slides and
  tumbles to its new place (`SLIDE_TICKS` 10). Every strike throws splinters, chips and dust
  (`particle_view::wall_hit`). The `.felled`/`.stump` nodes of `broadleaf` are not used yet
  (map trees have none).
- **Place and turn:** at the object's ground point (`view3d::ground_point`, terrain height),
  standing (`#STAND`, time 0), turned by the props' hash of the place
  (`map_models::turn`), never the sim RNG. Wind and shadows come with the model's `_WIND`
  weights and the shadow map.
- **Models fitted to their own sprite** (`map_t<N>`, ruins and rubble 427–470, built by
  `acr_assets.nature.build_generated` with `map`, docs/assets.md): the entry
  `{"model": "map_t452", "turn": "sprite"}` keeps the sprite's facing instead of the hash,
  `RealDir` 64 (`map_models::SPRITE_DIR`, yaw 0: the one view `acr model-ref` gives a list
  frame and the fit used), so walls, arches and colonnades line up as in the original; the
  scale is 1 (built at the sprite's size) unless the entry gives one. Trees and rocks keep
  the random turn. Their base colour is brought up to the sprite's brightness at build time
  (`nature._map_tone`, docs/assets.md): the renderer reads base colour only, and Pixal3D had
  put part of some pillars' look into metallic, which drew them nearly black. All map_t
  models are mapped but the tower 467 (a blob, 9 % of its texture unbaked) and the fallen
  column 468 (drawn with black speckles; 2 mesh islands, likely a second shell).
- **The rest of the lists** (`assets/nature/bush_dry.py`, `flowers.py`, `flag.py`,
  `fallen_tree.py`): the dry twig bushes 143–172 are a Real Bush with its leaf atlas turned a
  dark straw brown (`nature` SPEC `dry`, `keep`), the flower and lily patches 327–330 a scripted
  clump of rosettes and flower heads, the flags 331 and 471 a scripted pole with a chequered
  cloth whose flutter runs out from the hoist ("turn": "sprite", the cloth towards the east as
  in the sprite), the fallen trees 431–446 (but 434, a ruin) the broadleaf_ez tree laid down
  (`lie`) with a table `scale` each from the sprite's width, as the fit by height reads a lying
  tree wrong; the standing dead brush 442–443 is `dead_tree`, 84 `bush_small`. Still sprites:
  Mount Rushmore 225, the rubble 429, the tower 467 and the column 468, STONLIST skulls 75–79
  and the head 132. 84, 331 and 471 are on none of the game's maps.
- **Sprites:** the replaced frames go to `ObjectsView::set_hidden` (kept over scene
  rebuilds); `Scene::visible` leaves them out, also in the zoomed-out view. `models=off` or
  `map_models=off` brings every sprite back at once.
- **Cost:** the transforms are built once per map and model set (`MapModels::place`); per
  frame only the objects inside the view's ground rectangle (`View3d::ground_bounds` up to
  2000 px high, plus 600 px for crowns and shadows) become instances (`3d.map_objects`).
- `game.log` names the densest 1024 px cell of trees and the most used tree types of the map
  (`3d: map trees: ...`), for tests and for filling the table.

Cost (render VM, RX 9060 XT, release, 1600 × 900, random map seed 5, fog off, fixed camera at
zoom 1 on the densest wood `[11776, 14848]`, every `TREELIST` type mapped to `broadleaf`:
8379 objects as models; `gputime` and `profile` over 120 frames, off/on/off/on, two runs;
steps in `ACR-analysis/genai/mapobj/steps/perf.json`):

| | off | on |
|---|---|---|
| GPU, all passes | 0.38–0.46 ms | 0.83–0.93 ms (models 0.35, shadow map 0.17, ground shadows 0.08; sprites 0.16 → 0.03) |
| CPU `3d.map_objects` | – | 0.05–0.08 ms |
| CPU `render.models_prepare` | 0.17–0.22 ms | 0.33–0.44 ms |

### Animation states (#166)

The model plays the clip named like the unit's current `.MD` animation, at the time of the
unit's sprite frame: `clip time = frame / fps`, `fps` = 24 (clips are keyed on sprite frames
at Blender's default rate; a root node's `extras.acr_fps` overrides it). The clip is found by
name without `#`: exact (`MOTION_L1`), without variant digits (`MOTION_L`), the family's
first (`DEATHLIE1` for `DEATHLIE3`), then `STAND`, then the first clip; a model without clips
shows its bind pose. Facing: `yaw = −(RealDir − 64) · 2π / 256` (models face south);
buildings always face south (one view).

### Individual looks (#283)

Models whose scene root carries `extras.acr_face_morphs` (the contract of
`docs/assets-humans.md`, "Faces and bodies") give every soldier his own face, build, hair,
beard, colouring and height. Models without it draw exactly as before.

- **Picker** (`looks.rs`): a port of `faces.choose(seed, style)`: the hash `_mix32`
  (lowbias32) and `Rng` drawing in the same order. Tests check it against Python values
  for seeds 0, 1, 42, 1776 and 2³²−1. The seed is the unit id (`ModelInstance::look`, set
  for every non-building unit in `three_d.rs`), so a unit keeps his look across frames,
  saves and replays. The model's type gives the style: `civilian` when
  `acr_look_odds.hair` has `loose`, else `regular`.
  - The tables (odds, colours, tones) are copied from `faces.py`: the extras are read
    into unordered JSON maps, but the draw order depends on the order of the tables.
- **Face and body morphs without GPU morph slots:** the targets `face_*` and `body_*` of each
  primitive are kept on the CPU. At `add_model`, 16 variants are baked (`bake_looks`), each
  with the weights of a fixed seed (`variant_weights`): pre-mixed vertices of just the
  primitives these morphs touch (flesh, coat, collar, hair and beards), which come first in
  the model's vertex buffer so that a variant buffer shares the index numbering.
  - A soldier draws the variant nearest to his own weights. Draws are grouped by (model,
    decaying, variant), and the touched primitives bind the variant's buffer.
  - `deflate` (and `lie_flat` on models that have it) still run on the GPU on top.
  - Cost: engm18, 4800 of 7122 vertices touched, so 16 × 4800 × 124 B ≈ 9.3 MiB of
    vertex buffers per model type (the base buffer is 0.86 MiB). engkri needs 8.9 MiB.
    The glb loads in about 25 ms (release), of which baking takes 3.6 ms.
- **Hair and beard:** meshes named `*.hair_*` / `*.beard_*` mark their vertices with
  `body.w` = variant index + 1 (`looks::VARIANT_NODES`). The instance's `extra.w` is the
  bit mask of the nodes his hair and beard choice shows, and the vertex shader moves the
  others off screen. With no look, the mask is 0, so the variants are hidden (the old
  guard of the asset branch).
- **Tints:** material `extras.acr_tint` (`"skin"`: both flesh primitives, body atlas and
  face map; `"hair"`) sets `material.look.z`. The fragment multiplies the colour by the
  soldier's skin tone (an sRGB multiplier, so raised to 2.2) or hair colour (linear chosen
  colour ÷ linear neutral base `(0.80, 0.78, 0.75)`).
- **Height:** a uniform scale `height_m / 1.73` about the ground point, applied to the
  instance transform before the caps and inserts are placed.
- **`_FACE`:** read as `Unorm8x4` into vertex location 14 (`_WIND` took the last one, 15:
  all 16 of WebGL2's locations are used). Per soldier, the fragment shades stubble (beard zone × stubble), forehead
  folds (wrinkle zone × `face_age`), pock marks (pock zone × pocks) and, for 15 %, a pale
  scar slanting across one cheek (face front, at `acr_face_frame` eye height − 4 cm).
- **Instance data:** 3 look texels follow the marks in the bone texture (`look_texel`).
  They hold the skin tint and stubble, the hair tint and age, and the pocks, scar side,
  eye height and 1 + the face tile. Instances without a look get neutral texels (tile 0).
- **Face tiles (#306):** a face material with `extras.acr_face_tiles` = n > 1
  (`Material::face_tiles`) holds n whole head tiles in a grid (`looks::tile_grid`: 2 × 2 for
  4), its UVs on the first. `looks::face_tile(seed, n)` picks the soldier's tile from its own
  hash of the unit id (`mix32(seed ^ 0x5F0C_E7A1) % n`), so the portrait varies independently
  of the morphs. The pick rides in the third look texel's w (no new vertex location or
  varying); the material uniform's `cloth.w` holds the grid, and `vs_main` shifts the UV by
  (tile % g, tile / g) / g. WebGL2-safe; one texture per model, no extra draws. Proof:
  `C:\Users\verwalter\WIP\ACR-analysis\genai\faces-tex\atlas\` (12 EngM18, studio,
  `group_painted_vs_atlas.png`, `closeups_painted_vs_atlas.png`).

Close-up check: `ACR-analysis/looks/looks_bench.py` places 8 EngM18 and 8 EngKri in rows
facing the camera at zoom 5 and 12.

**Ships and guns (#287).** A root with `extras.acr_clock = "world"` runs its clips on the
game clock instead of the sprite frame: looping while alive (each unit at its own phase), from
the death record's tick on once dead (ships: one sprite frame per direction, a sinking that
outlasts `#DEATH`). A root with `extras.acr_machine = true` is no body: no blood pools,
sprays, splatter or corpse wounds, no corpse pile, no decay and no corpse sink. Ships stand at
the terrain height of their ground point like every model; water is a tint on that ground.
A building with `acr_clock = "world"` and a clip named `WORK` plays that clip, looping on the
game clock, whenever it stands finished: the moving `#WORK` layer of the sprite (0x50927a),
e.g. the sails of `SpnMel` on their own node (`assets/buildings/spnmel.py`).

**Gun crew (#287).** The cannon's crew is part of its model (`acr_assets/guncrew.py`,
bones `c0.`..`c3.`), so it runs on the gun's sprite clock and the linstock touches the vent
on the shot frame of `ATTACK` (29). Moves of more than half a metre between stations are
walked (`guncrew.stepped`: contact and passing poses of `poses.walk_cycle`, about one step
per 0.55 m, the root sliding along with the cycle's bob).

- **Mortar** (#301, `assets/artillery/artmrt.py`, `acr_assets/mortarcrew.py`): the model is
  found by the type id (`ArtMrt(XX)` → `artmrt.glb`, `scene::definition_name`) while the sim
  plays the field gun's `.MD` keys, so it carries the cannon's clip names. Three men
  (`c0.`..`c2.`: two loaders whose right hands go to the muzzle's rim, IK kind `muzzle`; the
  firer with the linstock on the vent at frame 29), a bed without wheels, the barrel fixed at
  60° and a `flash` bone at the muzzle shown for 5 frames from the shot. Moving, the crew
  carries it (a mortar had no wheels; light ones were carried by 2-4 men by their handles
  or on handspikes): c0/c1 by iron handles on the bed's sides, c2 by one at its rear (the
  handles are added to the source bed), the bed `mortarcrew.LIFT` 0.15 m up on the `gun`
  bone in `GUN_PUSH`, lifted at the end of `CHEST_CARRY`, set down early in `DEPLOY`; the
  sim only moves the unit (IK kinds `handle`/`rear`, `guncrew.goals`). A man falls per
  third of the lost life (c1, c0, then the firer c2), each bleeding under where he lies.
- **Losses** (`acr-game/src/ingame/crew_view.rs`): the sim keeps the gun a single unit with
  one life; with `k = ceil(n · life / max_life)` of the model's `n` men standing (the
  highest `cN.` prefix + 1, at most `CREW` = 4), a man falls each time `k` drops, in
  `Crew::fall` order, derived from the model's own stations (`cN.hips` in `STAND`): the
  rearmost stays longest, the others fall from the highest number down (field gun: c3 the
  firer, c1 the loader, c0 the rammer, c2 the layer; mortar: c1, c0, the firer c2).
  `ModelData::crew` (`model.rs`, found on machine models with nodes `c0.`..`c3.`) knows the
  palette slots of each man (his bones and the gear on his sockets); `Crew::mask` collapses
  the rows of the fallen to one point, so their triangles vanish, and the gun's instance
  takes that palette through `set_custom_poses`. The fallen man is drawn on his own: an
  instance of the gun model posed in its `DEATH` clip (then `DEATHLIE1`) with only his
  slots shown, at the gun's transform of the moment he fell. That corpse decays like any
  other (`corpse_decay` on its own clock, `deflate`/`lie_flat`), carries blood and dirt and
  bleeds a pool under his hips where he lies (`Crew::lying`: the end of `DEATH`); it lies
  `corpse_minutes`. The drill goes on with the
  men left on the sim's timing; their tasks are not handed over (each man's tool hangs on
  his own bones, so another man's track would show the wrong tool), so a gap shows where a
  lost man worked. When the gun is destroyed, the men still standing become such corpses
  too (on the first frame its unit is drawn dead): they fall in
  `DEATH` on their own clock, bleed and rot like the others, and the gun's own `DEATH`
  shows none of the crew. The corpses keep the transform of the moment they fell, so they
  stay where the man fell when the gun is moved on (checked: `guncrew3/sheet_losses.png`).
- **Turning** (`crew_view::Crews::gun_yaw`): the sim turns a cannon a 16th (22.5°) per
  `#MOTION_L1`/`#MOTION_R1` cycle and sets the 16th at the cycle's end (left) or start
  (right), so its facing steps. The model is drawn at the facing moved through the step with
  the cycle's frame (`turning_dir`), followed at up to twice the gun's turning rate (a 16th
  per cycle at two frames per tick), so it turns smoothly.
- **Crew work** (`crew_view::crew_clip`, `Crews::work`; the roles of the original's sprite
  packs): `#PATTACK1` plays `TRAIL_LIFT` (`ArtPus_Pdg_pvrt`: c2 sets the handspike in the trail
  and lifts it, c1 goes behind the left wheel, c0 in front of the right one, c3 to the
  ammunition chest set down beside the trail), `#MOTION_L1` plays `TRAIL_WALK` (`ArtPus_pvrt`:
  the trail walked round a 16th, the wheels heaved opposite ways by a spoke's pitch, so the
  loop is seamless; c3 walks on the spot one stride per cycle, facing along the ground that
  runs past his station while the model turns, `guncrew.turn_heading`/`turn_slip`; his gait's stride is that slip and it is keyed every 2 frames (`guncrew._pace`; four keys a cycle slid his planted feet 0.15-0.25 m a step, #341), so a planted foot moves under 8 cm on the ground, `test_guncrew.foot_slide`), `#MOTION_R1` plays it backwards and `#UATTACK1` `TRAIL_LIFT` backwards.
  `#PSTAND1`, which the sim shows for one tick between two turn steps (and after a move, when
  the gun keeps its trail up), holds `TRAIL_WALK`'s first frame. It used to fall back to
  `STAND` (`clip_for_animation` finds no `PSTAND`), so every step popped the crew back to
  their battery stations for a tick: the flicker between "leaning in" and "standing beside".
  Moving (`#MOTION_L0/R0`, `ArtPus_Tlk`) plays `GUN_PUSH`: c0/c1 push on the wheels from
  behind, c2/c3 carry the chest by its rope handles behind the trail; `#PSTAND0` holds it,
  `#PATTACK0`/`#UATTACK0` play `CHEST_CARRY` forwards/backwards (the chest taken up from
  beside the trail; the sim drives off from `#STAND` without them, as the original's sprites
  switch straight from `pvrt` to `Tlk`). A `#STAND` of up to two ticks between two works
  keeps the last work's pose. Checked on the blank field: `ACR-analysis/crew`
  (`anim.log` per tick, `before_*_sheet.png` / `after_*_sheet.png`).
  Deploying for a fire mode (`#PATTACK4/5`, 171 frames, `ArtPusPdg_Vstr_1/2`) plays
  `DEPLOY`: c2 seats the handspike and raises the trail, c0/c1 heave the wheels behind
  them (the piece run up a spoke's pitch), c2 lays it (the trail shifted aside and back)
  and lowers it onto its bed, c3 stoops to the chest beside the trail; it starts and ends
  at `STAND`, and `#UATTACK4/5` (in the `.MD` the same frames reversed) play it backwards:
  limbering up. The chest no longer pops where the sim drives off from `#STAND` (or stops
  into it) without `#PATTACK0`/`#UATTACK0`: `Crews::work` plays `CHEST_CARRY` forwards
  (taken up) or backwards (set down) in `CHEST_BLEND` (0.6 s) across the switch, render
  only (`chest_blend`).
- **Hands on the grips** (`guncrew.key_clips`, m2-crew2): every frame of a work
  (`ik_spans`: between two keys with the same `ik`, walking steps included where both
  ends hold it) is solved, not only the keys: first the body (`place_body`: the root
  stepped horizontally until each goal is within 0.85 of the arm's full reach and no
  nearer than 0.24 m; the trail man stands where his right fist on the seated handspike
  is 0.36 m from the shoulder), then the arms (`_reach`: two-bone IK whose goal is the
  fist's centre, `socket.hand_*` of the grips system, the hand keeping its orientation).
  Goals (`goals`): the felloe's middle (0.66 m from the hub) 62° and 82° behind or in
  front of the top, 3 cm outside it (the wheel men 1.45 m behind or in front of the axle,
  leant well in, as `ArtPus_Tlk` draws them); the top of the rope loop at the carrier's own
  end of the chest (the nearer loop and hand; it used to be the far one, which stepped both
  carriers into the chest and each other); the handspike's
  head 0.10 m past its seat in the trail end's ring (y 2.64, on the trail's top: the seat
  used to fall back to the carriage's top, 0.7 m over the trail, so the spike was seated in
  the air) (the left fist 0.22 m behind the right, on the lever end); the left
  fist 0.35 m towards the butt of the level staff. `measure` reads the misses on every frame as played
  (keys interpolated) into `artpus.stats.json` `hands.grips` (per grip: frames, mean, max,
  worst clip/man/frame); `test_guncrew.Grips` holds every max under 3 cm.
- **The wreck:** the gun's `DEATH` throws the barrel off its trunnions, drops the left wheel
  and swaps the carriage for its charred copy (`wreck`, black). Seen from above, the black
  trail and a dead man's tricorn next to it read like a second barrel; there is no double
  (one barrel in every view of a gun killed at full strength, before any crew corpse exists).
- **Spray:** a gun with a crew is not counted a machine for sprayed blood, so blood from hits
  in front of it speckles the crew (and the carriage near them). Wound marks need a skeleton
  H rig (`wound_rig`) and are not placed on the crew yet: `Rig::from_skeleton` finds the
  joints by their bare names (`upperarm_l`, ...), the crew's are `c0.upperarm_l` etc. It is
  feasible: a prefix for `from_skeleton` gives one rig per man (the slots are the gun
  model's own palette slots, so marks packed against it land on the gun's and the corpse's
  instances alike), and the gun's hits would have to be given to men (the hit that drops
  `k` to the man who falls, the rest by where they land).
- **Uniforms by nation:** the coat (`<gun>.crewN.coat.*`) and waistcoat (`.waistcoat.*`)
  materials take `Tint::Coat` / `Tint::Facings` (or `extras.acr_tint` `"coat"`/`"facings"`).
  `ModelInstance::uniform` holds the target colours; they go into the first two look texels
  (unused by a model without looks) with the luminance of the materials' `acr_base_rgb`, and
  the fragment replaces the cloth's colour by the uniform's at the cloth's own luminance
  over the base's (a multiplier cannot turn a red waistcoat blue). The
  catalog has one cannon type per nation, `ArtPus(EN)`, `(FR)`, `(SP)`, `(US)`, all drawn by
  `artpus.glb`; the suffix gives the uniform (else the owner's first unit named `Eng*`,
  `Frn*`, `Spn*` or `Usa*`).
  England: dark blue, red waistcoat; France: bleu de roi throughout; Spain: lighter blue,
  red; USA: dark blue, buff. Others keep the model's blue and red.
- **Faces:** the men are built with the MakeHuman head (`heads.refine`) and its painted
  face map (`heads.face_map`, 1024², baked per man); no face morphs, looks or hair variants
  (a gun has no looks), and the face material drops `acr_tint` "skin" (it would read a look
  texel). Crew 18 768 → 24 432 triangles (skin 1 532 → 2 786 per man), the whole model
  29 130.
- **Hands (IK at build time, `guncrew.hands`):** keys whose pose carries `"ik"` get a
  two-bone IK pass after they are keyed (scene at that frame, so the gun's bones are where
  the clip puts them; the gun points are taken with its bones at rest): `staff` the left
  hand 0.35 m down the level sponge/rammer staff towards the butt; `spike` the handspike
  turned to point at its seat on the trail (y = 2.45 m) and the left hand 0.38 m down it;
  `linstock` the right arm moved and the linstock turned so the match touches the vent
  (top of the breech, 0.24 m before its end) on the shot; `wheel` both hands on the spokes
  of the man's wheel at the felloe (0.66 m from the hub, 36° and 62° behind the top), an
  upright staff staying upright; the pushers stand at x = ±1.02. The elbow bends out and
  down, the hand keeps its keyed orientation. The build reports the misses
  (`stats.hands.by_kind`: keys, mean, max in m): linstock 2 / 0.007 / 0.008, staff 18 /
  0.08 / 0.11, wheel 64 / 0.26 / 0.37, spike 38 / 0.21 / 0.34. The wheel and the spike are
  still out of reach from the keyed body poses (the arms end stretched towards them): the
  pushers' bodies and the spike's seat (found 0.98 m high, too high: the search takes the
  highest carriage vertex near y = 2.45) need another pass. Between keys the arms
  interpolate, and the fingers stay open (no grip).

### Decay (#178) and gore (#183)

Per corpse one value from the death counter (`Unit::death`):
`decay = clamp((death − 100) / (removed − 100), 0, 1)`, `removed` = 2000 for types with
`#DEATHLIE2`, else 1200 (the original's removal). From it: the morph weights
`deflate = ramp(decay, 0.18, 0.55)`, `lie_flat = 1` while lying (`#DEATHLIE*`; only models that carry the morph, not skeleton H, whose skirt bones do it), and a sink of
`ramp(decay, 0.78, 1) · 0.20 m`. With `corpse_decay=half` (settings; driver blank start
`"corpse_decay": "half"`) decay stops at `model::HALF_DECAY` (0.45): the flesh half
dissolved, hair and cloth going, never bleached bones or the sink, for looking at thrown
remains. The fragment shader applies each material's
`extras.acr_decay` (discolour towards the tint, dissolve by the `COLOR_0` mask
`0.75 R + 0.25 G` with a dark rotten edge) exactly as `docs/assets-humans.md` gives it;
dissolving is alpha-tested (`discard`), nothing is blended or sorted. `gore=reduced` keeps
everything but the dissolve; `gore=off` draws the corpse like the original's half
transparent sprite, every other pixel. Sprite corpses always keep the original fade.

**Animal corpses (masks, `acr_assets/animals.py`).** The WildMesh animals carry real decay
masks since animals3: their own meshes are the `fur` layer (the pack's materials copied with
`acr_decay` extras), a copy of the body inset by 1.2 % of its height with 40 % of its
triangles is the meat under the fur (`flesh` with dark red windows of its own,
`decay.ANIMAL_FLESH`, `inner: true`), and a tube skeleton of the deform bones longer than
3.5 % of the height is the `bone` layer. `inner: true` in `acr_decay` (like `layer: "bone"`)
keeps a layer from being drawn on the living. The fur opens in patches from decay 0.24 onto
the meat, the meat goes between 0.36 and 0.80, the bones stay. Two exporter traps made the
first try export no masks: the exporter writes the *render* colour attribute as `COLOR_0`
(`color_attributes.render_color_index`), which stayed on the removed pack attribute
`VertexColors` (bear, elk), and it reads materials from the depsgraph's evaluated copies,
which predate a custom property set on an existing material (hence the copies).

**Procedural decay (models without masks).** A skinned model with no `acr_decay` material
that is not a machine (`ModelData::decays_procedurally`: animal glbs built before animals3)
rots by a fallback in `model.wgsl` (material `look.w`), driven by the same per-corpse
`decay`; living animals (decay 0) are untouched:

- colour: mottled value noise from the rest position (`7×` and `23×` per metre) pulls the
  colour, with its blood and dirt, towards olive-grey rot in patches and brown-black
  elsewhere (`ramp(decay, 0.02, 0.55)`), and darkens it by 40 % (`ramp(decay, 0.1, 0.85)`);
- collapse: after the palette, in model space (metres above the ground point), the height is
  flattened by up to 30–42 % (`ramp(decay, 0.15, 0.6)`, a little noise per vertex);
- sink: then lowered by `0.3 m · ramp(decay, 0.7, 1)`. Both happen after the palette, so a
  corpse posed by the body physics (`ragdoll_view`, whose palette cancels the instance
  transform) collapses and sinks as well; for these models the instance transform is not
  sunk (the footprint for the piles still is);
- dissolve (`gore=full` only): noise from the rest position (`5×` and `19×`, stretched
  so that its values spread over 0..1) against `1.08 · ramp(decay, 0.62, 1)`, with a dark
  rim; ragged dark shreds remain at about 0.85 and nothing at the end. Without inner
  geometry the dissolving body is a hollow shell (a bone layer comes with real masks).
- Bench (`ACR-analysis/animalsim/tools/bench_decay.py`, `corpse_minutes=1`): a bear, an
  alligator and an elk shot dead and photographed at 40..590 ticks, with a watcher of
  their side so the fog does not grey them.

An animal that leaves a carcass (`TRANSFORMTOMEAT`) is removed by the simulation at the end of
its death clip (not alive, death counter 1); in 3D its body lingers like a soldier's for
`corpse_minutes`. The simulation's `MEAT` sprite (`sprite::leave_carcass`, a `ResSprite`
for hunting) is not drawn by the game at all yet (dynamic sprite objects have no draw path;
the static scene holds the map's sprites only), so nothing shows beside the 3D carcass; when
that path comes it must leave out a `MEAT` sprite whose animal still lies in 3D. **Asset side, later:**
`deflate` / `lie_flat` morph targets for the animals (the masked corpses do not collapse yet;
they sink with the instance transform).

#### Deaths by cause: wounds, blood, dirt, burns, pools

The simulation keeps why each unit died in `World::deaths` (`combat::DeathRecord`: the
cause and the attacker's position), for the renderer only: it is not part of the state
hash, and savegames replay their commands and record it again. The cause comes from the
weapon kind of the killing blow (`NRES.DAT [WEAPONS]`): `MECH`, `PIKA` melee; `VISTREL*`
gunshot; `STRELA*`, `OSTRELA` arrow; `TAMAGAVK`, `KOPIE`, `KAMEN` thrown; `IADRO` cannonball;
`KARTECH` grapeshot; `IADROMOR`, `VAFLIA` explosion; a unit that dies burning is burned,
one that starves (`economy::starve`) died of hunger, the rest (scripts) is unknown.

`acr-game/src/ingame/gore.rs` turns the record into per-corpse `Wounds` (instance
attributes 13, 14) and a `BloodPool`:

| Cause | Wounds (radius) | Body blood | Pool (radius, trail) | Burn |
|---|---|---|---|---|
| Melee | 3 (0.10 m) | 1.0 | 0.85 m, 0.2 | — |
| Gunshot | entry + exit (0.06 m) | 0.85 | 0.55 m, 0.1 | — |
| Arrow | 1 (0.06 m) | 0.7 | 0.45 m | — |
| Thrown | 1 (0.09 m) | 0.9 | 0.6 m, 0.1 | — |
| Cannonball | 1 (0.28 m) | 1.0 | 1.1 m, 1.2 | — |
| Grapeshot | 6 (0.06 m) | 1.0 | 0.9 m, 0.4 | — |
| Explosion | 3 (0.12 m) | 0.8 | 0.7 m, 0.3 | 0.6 |
| Burned | — | — | — | 1.0 |
| Hunger | — | — | — | — |
| Unknown | 1 (0.07 m) | 0.6 | 0.5 m | — |

- Wounds sit on the model's rest pose (the bind position, so they stay on the body in any
  pose): the first at chest height on the side facing the blow (turned into the model's
  frame), with two wounds the second is the exit on the far side, further ones spread over
  the front. Blood runs down from them (in the rest pose), fresh red drying to brown with
  the decay. Dirt rises from the ground in patches (`0.8 · ramp(death, 60, 900)`), burns
  char the body. The bone layer gets none of it.
- **Burns on the living and the dead (#207):** `Wounds::burn` is the largest of the
  cause's (table), `FireView::burn` (a man who burns or burned, see "Fire" below) and the
  driver's `burn` (`gore::Burns`, render only), so a burning man chars while he still
  stands and keeps it on his corpse. The shader darkens by 0.45 × the amount (soot) and
  chars the patches where the rest-pose noise exceeds 1 − amount, so the charred share
  grows with it (0.3: patches, 0.6: most of him, 1: black all over); the old curve was a
  step near 1 and showed nothing below it. `gore=off`: none. Proof (blank field, white,
  EngM18): `ACR-analysis/genai/gore/burn-amounts.png` (1.0, 0.6, 0.3, 0 standing).
- **Hunger (#207):** a starved corpse (`DeathCause::Hunger`, `economy::starve`) draws the
  `emaciated` morph at weight 1. The vertex slots are full, so `model.rs` loads
  `emaciated` into the second morph slot of models without `lie_flat` (every skeleton-H
  human; `ModelData::emaciated`), and `ModelInstance::morphs[2]` is its weight
  (`morphs[1]`, `lie_flat`, goes to that slot on the other models). The sim keeps no
  hunger state on the living, so only the dead show it. `gore=off`: none.
- **Tint noise:** blood, dirt and burn patches use the shader's rest-position noise
  (`noise3`), not `_GORE.yzw`: only `_GORE.x` (the cut edge) has a vertex slot. Both are
  noise fields over the rest pose, so the look is the same kind.
- Pools are a decal pipeline (`blood.wgsl`) drawn before the models over the terrain's
  depth, alpha blended, writing no depth: a noisy round pool under the chest (0.55 m ahead
  of the ground point, where a body lying face down has it) with a narrowing trail away
  from the blow; they spread over the first ~140 counter steps, dry from red to brown up to
  900 and fade over the last 15 % before removal. Pools are flat at the ground point's
  height: on slopes part of them can sink into the terrain.
- Blood trails (`ingame/trail.rs`, render only, deterministic from unit id and tick):
  every 10 ticks a bleeding walker leaves a small drop; a crawler (lost legs, #262) leaves
  a *smear* segment (decal kind 4) from where he was 10 ticks ago to where he is now, so
  the segments of one unit join into a ribbon about a body wide (half width 0.26 m). The
  segment's length sets its look: short (slow) segments are wider and denser, long (quick)
  ones narrower and broken into dabs; each starts as wide as the last one ended. Lying
  still (moved < 2 px) adds to the last segment a round patch that grows with each sample.
  The shader draws a segment with flat ends (a zero-length one round), its edges ragged
  and the ribbon wandering slightly, with streaks along the drag; all noise is in world
  space, so it runs on unbroken from one segment to the next. Smears are lifted 4 world
  units (pools 1.5) so bumpy ground does not cut the narrow ribbon up. Like pools they dry
  from glossy red to brown-black over 40..900 ticks and fade over the last 20 % of their
  3000 ticks; the pool where he dies comes from the death.
- `gore=reduced`: one wound, half the blood, half-size pools; `gore=off`: none of it.
- Not done yet (#183): cause-specific death clips, stuck missiles, pools that outlive the
  corpse. Dismemberment: see the next section.

#### Cut, broken and torn bodies (#262)

The renderer half of `docs/damage-model.md` §3 (`acr_render::damage`): a `Damage` on a
`ModelInstance` (default: whole).

- `limbs: [LimbDamage; 4]` (arm_l, arm_r, leg_l, leg_r), each `cut: u8` (the limb ends at
  this level, 255 whole) and `fracture: Option<Fracture { level, axis, angle }>` (rest pose
  axis, radians); `trunk_cut: Option<u8>` (0 hips .. 255 neck); `head_gone`, `belly_open`;
  `keep: Keep::Body | Piece { limb, from, to } | Upper`.
- Levels are the bytes of `acr_sim::body` (`ELBOW` 110, `WRIST` 205, `KNEE` 120, `ANKLE`
  215). The model's `Rig` (`ModelData::rig`, skeleton H only) holds the limb chains in the
  bind pose (from the inverse bind matrices of `upperarm`/`forearm`/`hand`,
  `thigh`/`shin`/`foot`/`toe`, `hips`/`neck`), so a level maps to a point, direction and
  bone. Vertex levels are `_LIMB`; models without it get them at load by projecting the
  skinned limb vertices (by `_PART`) onto the chains, as the bake does. Radius profiles are
  `extras.acr_limb_radius` when present, else measured from the vertices (outer radius per
  sixteenth of the level).
- Shader (`model.wgsl`): vertices past a fracture turn about the fracture point in the rest
  pose before skinning (blended over ±0.012 of the level); the vertex shader computes the
  kept distance past the cut (limb level, or rest height for the trunk), the fragment shader
  discards below zero after a ragged offset from `_GORE.x` and fine noise (the same offset
  on both sides of a cut, so body and piece meet). `Piece` keeps only `from..to` of one
  limb; `Upper` keeps the trunk above the tear, the arms and the head; `Body` with a trunk
  cut keeps the pelvis and legs. `belly_open` paints a wet gash on the front of the torso.
- Caps: every cut of the kept part gets an instance of the cap model, placed in the rest
  pose at the cut (`damage::caps`), facing away from the kept flesh, scaled by the radius,
  and carried by the bone at the cut in the instance's pose. The cap is
  `damage::procedural_cap` (rings: bone white, red flesh, skin rim; wet highlight; ragged
  outline in the shader) until `ModelRenderer::set_cap` swaps in the modelled cap of
  `gore.glb` (a unit disc in xy facing +z). The old joint-only caps (`*.caps` meshes) are
  no longer loaded (`model.rs` skips `<name>.caps`; #207's per-joint caps are covered by the
  placed caps, which also fit cuts between joints).
- Not yet: the trunk's torn-edge strip (the trunk cut uses the disc).

#### Torn-out tissue: wound volumes (#280)

Clean planes that split a man read as "a walking piece of meat in a cartoon". Hits now tear
chunks out: `Damage::volumes` holds up to 8 capsules (`damage::Volume`: rest-pose ends `a`,
`b`, radius, noise seed, kind `Tear` or `Gash`), two texels each after texel 11 of the
damage block (`DAMAGE_TEXELS` = 28; texel 11 `.w` is their number).

- **Shader** (`model.wgsl`, `holes`): a body fragment (`_PART` ≥ 0; gear is not torn)
  inside a volume is discarded. The boundary is radius × (1 + chunk noise at 5/r and 13/r
  + fibre noise stretched along rest y + `_GORE.x`), so it is never round; gashes are
  cleaner. Shreds: front faces just inside the boundary survive where a stretched noise is
  high (strands of skin and cloth hanging into the hole); just outside, a band of holes
  frays the cloth. Front faces near the edge are soaked dark with a raw red rim. Back faces
  near a volume (the far wall and the inside of the shell seen through the hole) are shaded
  as `tissue`: wet red meat by fibres, yellow fat right behind the edge, dark clots, bone
  flecks. Back faces of a sleeve just past a planar cut get the same.
- **From hits** (`ingame/tears.rs`, hashed from tick and victim, kept per unit by
  `LivingWounds`): cannonball a capsule along the track (radius 7.5–16.5 cm by energy);
  through an upper arm the shoulder and a chest chunk go too; a leg takes a ragged thigh
  bite above; a trunk torn in two a 17–28 cm capsule plus two blobs (the trunk cut stays
  inside it, so both halves end ragged). Musket at close range (energy ≥ `BALL_CLOSE`) or
  breaking bone: an exit cavity; a fracture also a partial volume beside the bone, so the
  limb hangs by a thread at its bend. Grapeshot 2–4 bites, shells 3–5 fragments over the
  body, sabre/bayonet/axe one thin deep slanted gash; a blade that takes a limb off keeps
  the planar cut.
- **Severing** (`Damage::sever`): a limb whose whole cross-section a volume covers is cut
  in the middle of the covered span, so the stump and the thrown piece (`debris.rs`, which
  now throws every limb the hit severs) end in the torn boundary; `caps` drops caps inside
  a volume (caps stay as the fallback for plain cuts).
- **Chunks**: the volume taken (`tears::size`) throws 1–6 gibs scaled by it; a cannonball
  that takes a limb or a chunk also bursts red mist.

**Inside the wound** (`damage::inserts`, drawn like the caps, carried by the bone of the
part they sit in): per volume an inside-out sphere (`procedural_cavity`, material
`Tissue::Cavity`) pulled into the part it bites (centre at most 0.3 × the part's radius
from its axis, radius at most 0.72 ×), of which only the far wall facing the viewer is
drawn: wet lumpy meat darkening into the deep centre. It closes a hole straight through,
so the ground does not show. A chest volume (trunk level 0.4..0.85) adds three blood-
smeared rib arcs (`procedural_rib`), a limb volume a splintered bone end with marrow
(`procedural_bone`) at the stump of a limb it severs or across one it shatters. Gear
(the knapsack) is torn by volumes too, but shows no tissue. Shreds of uniform from
`gore.glb` (materials named `cloth`, `Tissue::Cloth`) are crumpled in the vertex shader
and take the victim's coat colour (`debris::coat`: the commonest colour of the upper
sleeves), carried in the gib's spray texel (radius < 0).

#### Bodies in the game (#262 S5)

`ingame/damage_view.rs` turns `acr_sim::body` into a `Damage`, `ingame/debris.rs` keeps what
falls off. Render only; placement is hashed from the hit (tick, victim), never the sim RNG.

- **Body → damage** (`damage_view::damage`): the living draw `Unit::body`, corpses
  `DeathRecord::body` plus `torn`. `lost` → cut; a fracture → bend at its level, at least
  25°: closed and open about the rest pose's x (folds forwards) by the sim's `bend`,
  dislocated 1.5 times as far (up to 100°) about z, outwards per side; a fracture on a lost
  part is dropped. `Torn::InTwo { level }` → `trunk_cut = 255 − level` (the sim counts from
  the shoulders); `Headless` → `head_gone`; `belly_open` → flag; `Pulverized` → no body
  instance at all (the unit still counts as shown, so no sprite either). `gore=off` draws
  the original whole bodies.
- **Caps:** `gore.glb` (`ACR/MODELS/gore.glb`, requested once gore is on) is split into its
  mesh nodes (`ModelData::node_part`); `stump_cap` goes to `ModelRenderer::set_cap`. The
  renderer has one cap model for all cuts, so `stump_cap_clean` (sabre) is not used yet.
- **Pieces** (`Debris::take_hits`, new hits since the last frame): a `HitRecord::injury`
  with `Outcome::Off` throws the cut-off part (`Keep::Piece { limb, from: level }`) of the
  victim's model; `Torn::InTwo` throws the `Keep::Upper` half (the legs stay with the
  corpse where he stood). The throw: from the hit along the blow, ±0.5 rad spread, 0.8 m to
  1.2 + 3 × energy m, in 7 ticks on a 0.6 m arc, tumbling. A piece lies on its back in the
  bind pose (flattened to the ground) with the middle of the piece on the landing point,
  and gets a blood pool once landed. Pieces decay as corpses and go after `corpse_minutes`.
- **Gibs and mist** (`gore=full` only): `Pulverized` leaves 4–8 gibs, `InTwo` 3, `Headless`
  2 (skull fragment and jaw), a killing cannonball of energy ≥ 90 (point blank) 4, from
  flesh, bone, cloth and organ nodes, thrown 0.3–4.5 m along the ball's path. Pulverized and
  point-blank hits add a mist: three extra sprays of #239 (`splatter::splatter`, all round
  and far along the path) that settle as speckles. Cloth gibs carry the player for the
  nation tint, which the shader does not apply yet (#162).
- **Guts** (`gore=full`): an open belly, on a corpse or a living (writhing) man, gets the
  `guts_heap` at the belly (0.35 m ahead of the ground point, to one side) and an intestine
  coil lying out beyond it.
- **Crawlers:** `CRAWL2`, `CRAWL1` (one working leg), `CRAWLARM` (one working arm) or
  `WRITHE` (no working arm) by `damage_view::crawl_clip`, played on game time; models without
  the clips fall back to `DEATHLIE1` with the `lie_flat` morph. Held gear is a separate node
  on its socket, so each human model is also uploaded without its `musket`, `rammer`, `pike`
  and hat nodes (`ModelData::without_nodes`); crawlers and men with `Body::dropped` draw
  that variant, and the musket and hat (`node_part`, turned to lie flat by
  `debris::lying_flat`) lie on the ground where he was when it happened, until
  `corpse_minutes` have passed.
- **Gear dropped at death (#206):** a killed man (skeleton H, or a rider with `rider.*`
  bones) keeps his musket, pike, carbine (`gun`), sabre (`sabre`, `blade`) and hat on their
  sockets while he falls; at the landing (his `#DEATHLIE*`, or the end of the death clip)
  his model turns into the variant without them and each item that shows then (not scaled
  to nothing by the clip) lies on the ground beside him (`three_d::laid_gear`,
  `Debris::lay_gear`). An item starts where it is in the landing pose (the clip's node, or
  under a ragdoll the bone its socket hangs on) and turned as it is; a long one (half
  length > 0.3 m) turns alongside the body (hips to head), up to ±0.35 rad off by a hash of
  (unit, item); everything is pushed sideways out of the body (0.3 m plus its reach from
  the hips-head segment). It lies flat on the ground's slope (`model_matrix_tilted`), ages
  from the death tick, so it rots, sinks and goes with the corpse, and a shell blast throws
  it like other debris. Render only: positions and turns come from the pose and hashes,
  never the sim RNG. `gore=off` keeps the original: the gear stays on the fading corpse.
  Proof: `ACR-analysis/genai/deathgear/` (`volley.json`, `orbit-before-after.png`,
  `after-death.png`).
- `gore=reduced`: pieces and caps, no guts, gibs or mist; `gore=off`: none of it.

#### Hits and blood splatter

`World::hits` (`combat::HitRecord`) keeps the recent hits for the renderer, oldest first:
`make_damage` notes every hit that has a sender (projectile, hitscan line, round damage or
melee blow; not fire or hunger, which have none) with the tick, the victim and its position,
the direction attacker → victim, the weapon's cause (`DeathCause::of_weapon`), whether it
killed and whether the victim is a building. Hits older than `HIT_KEEP` (750) ticks are
dropped at the start of each step, at most `HIT_CAP` (4096) are kept. Like `deaths` it is
not part of the state hash and not saved (a loaded game replays its commands). The
direction is the round's own flight while a projectile does the damage (`World::hit_dir`,
set around the damage in `projectile::explode` and the `DAMAGEHEIGHT` damage of a rolling
ball), so a ricocheting cannonball sprays along its bounce, not attacker → victim. A
round striking a building records where it struck (`World::hit_at`, set beside `hit_dir`)
instead of the building's centre (#226).

`World::impacts` (`combat::ImpactRecord`) keeps the rounds that struck the ground the same
way (same `HIT_KEEP`, `HIT_CAP`, not hashed, not saved): tick, position, flight direction
(the projectile's velocity) and the cause of the shooter's attack (`cause_of_attack`).
`projectile::explode` on terrain contact records every damaging round of a sender:
cannonballs and shells always, other rounds only when they hurt no one; a hitscan musket
ball that hits nothing records its end point. Ground-impact sounds can read the same list.

`acr-game/src/ingame/splatter.rs` sprays droplets onto the ground behind each hit unit
(not buildings) in a cone along the direction of the shot. They are derived every frame
from `World::hits` and the tick, seeded from the hit (tick, victim), so they keep their
place without any renderer state and come back alike after loading; the record is kept as
long as a droplet lives for that reason. They use the pool decal (`BloodPool::splatter`,
`look.z` in `blood.wgsl`): a crisp round head with a tail tapering along the shot, the far
droplets smaller and longer. Every decal (pools, droplets, earth) lies on the ground under
it: `splatter::lay_on_ground` sets its centre's height and the slope from the ground
points one radius either side (`BloodPool::tilt`), and `vs_pool` raises each corner by
that slope, so they no longer sink into hillsides.

| Cause | Droplets | Behind the victim | Radius | Cone (half) |
|---|---|---|---|---|
| Gunshot | 6 | 0.2–1.8 m | 0.06–0.14 m | 0.30 rad |
| Arrow | 3 | 0.1–0.8 m | 0.05–0.10 m | 0.25 rad |
| Grapeshot | 9 | 0.2–2.2 m | 0.06–0.15 m | 0.55 rad |
| Cannonball | 18 | 0.3–3.5 m | 0.08–0.25 m | 0.45 rad |

A killing hit sprays half again as many. They dry from red to brown between 40 and 400
ticks after the hit and fade from 75 % of `HIT_KEEP` until just before it. `gore=reduced`:
half the droplets, 0.7× the size, 0.8× the reach; `gore=off` and `models=off`: none.

Melee blows, thrown weapons and explosions spray low at the victim's feet instead
(`Spray::low`): round drips without tails, the big ones close, and they reach neither the
troops behind nor walls.

| Cause | Drips | From the victim | Radius | Spread (half) |
|---|---|---|---|---|
| Melee | 5 | 0–0.7 m | 0.05–0.12 m | 1.4 rad, away from the blow |
| Thrown | 4 | 0–0.8 m | 0.05–0.12 m | 1.0 rad |
| Explosion | 12 | 0.1–1.6 m | 0.06–0.18 m | all round |

Rounds that struck the ground (`World::impacts`) churn the earth with the same decal in
earth colours (`BloodPool::dirt`, `look.z` 2 and 3 in `blood.wgsl`), shown with every
`gore` setting: a cannonball leaves a 0.45 m crater (a dark pit in a ragged ring of
earth thrown out along its flight), a shell a round 0.9 m one; musket balls (0.12 m),
grapeshot (0.14 m) and arrows (0.08 m) a dusty scuff. The fresh earth greys and fades
like the droplets.

In rain (`weather`, #224) fresh blood decals soak in (`splatter::soak`): a quarter wider
and darker at once (`age` raised by 0.4 of what is left), as diluted blood on wet ground;
earth, decay stains and kept marks stay as they are. Snow and fog leave blood bright.

The particles of a hit (#226, `particle_view.rs`, `ParticleView::update`):

- `wall_hit`: a round striking a building's wall throws wood splinters (`Splinter`) back
  out towards the shooter, chips of stone (`Clod`) for cannonballs and shells, and a puff
  of grey-brown dust off the wall at 1–2.5 m. Scaled by the cause (cannonball 1, shell 0.8,
  grapeshot 0.45, musket 0.2, arrow 0.1). Wood and dust: every `gore` setting. Muskets
  rarely reach it: a garrisoned building's weapons do no damage to walls and record no hit.
- `spurt`: a melee blow or thrown weapon throws 8 or 6 drops (1.6× for a kill) from the
  chest along the blow; a shell burst that does not tear the body throws 14 up and all
  around. `gore=full` only, like `shred`.
- A mortar's shell (`ImpactRecord::mortar`, rule `mortars`) bursts smaller than a
  howitzer's: `MORTAR_SCALE` 0.6 of the earth, sods and dust (the `Scaled` sink: 0.6 of the
  count, √0.6 of the speed so they reach 0.6 as far and high, puffs √0.6 as big, the
  scatter 0.6, the life 85 %), 0.6 of the fragment streaks (`Fragment::mortar`, picked by
  hash; every fragment still strikes in the sim), and a blast (`Blast::mortar`) that throws
  pieces and corpses over 0.6 of the radius, 0.6 as many. It leaves no shell fire, so no
  flare. There is no separate flash or camera shake; the original's `PU2` explosion
  sprite and the crater stamp stay as they are.

On the blank field (RX 9060 XT, 1600 × 900, zoom 1, `ACR-analysis/genai/hits/battle.json`:
300 v 300 musketeers and 6 guns a side firing, `gputime` 30 frames twice at 200 and 300
ticks) the GPU sum stays 2.27–2.54 ms on main and 2.27–2.57 ms on the branch; the particle
pass 0.007–0.015 ms on both.

The same spray lands on models behind the victim (#220, `splatter::sprayed_targets`):
troops within the cone (up to the cause's reach, at least 0.25 m wide) get speckles on the
side facing the victim, around a point at chest height (0.55 m around it, rest pose),
stronger the closer they stand and for a killing hit; a building whose footprint (70 % of
its model radius) the spray enters within reach gets a stain of 0.9 m on that wall at 1.3 m.
Each model keeps the strongest spray that reached it (`ModelInstance::sprayed`: point,
radius, amount, drying like the droplets), drawn in `model.wgsl` as speckles of two sizes
and a fine mist (instance attribute 15 and `extra.zw`; with it the model pipeline uses all
16 vertex attributes WebGL2 allows). The bone layer stays clean. Units are found through a
128 px grid. `gore=reduced` halves it, `off` removes it.

#### Corpse lifetime

The simulation removes a corpse after about a minute (death counter 1200, the original's
rule), too soon for piles to grow. In 3D the renderer keeps removed corpses: their units
stay in `World::units` (not alive) with position, facing and lying pose, and
`DeathRecord::tick` (render only) says when they died. A corpse is drawn until
`corpse_minutes` (settings, default 5, 1..60) of game time have passed, and its decay,
pools and settling run on that clock (two counter steps per tick from the death, removal at
twice the lifetime in ticks). `gore=off` keeps the original removal and fade.

#### Corpse piles (#221)

3D corpses that overlap lie on each other instead of through each other
(`acr-game/src/ingame/piles.rs`, render only; corpses as obstacles are #222):

- Each corpse (death counter > 0) is a rectangle on the ground along its facing: 1.7 m long,
  0.5 m wide, its centre 0.1 m ahead of the ground point (the head lies ahead, the chest
  0.55 m), with a rounded top 0.25 m thick. Larger models get a larger footprint
  (`1 + 2.5 · (radius / 2.2 m − 1)`, at most 1.8: `engd18.glb`, radius 2.6 m, gives 1.45).
- Bodies are stacked by time of death (`DeathRecord::tick`, ties by unit id). Each body
  samples the tops of the bodies placed before it and the terrain at 8 points (±0.25, ±0.75
  of the half length, ±0.5 of the half width), each over its own ground point. It tilts by
  the difference of the highest supports of its back and front halves (pitch) and half the
  difference across it (roll), at most 0.5 (27°), turned rather than sheared
  (`view3d::model_matrix_tilted`); the tilted plane is lifted onto the highest support
  (never below the ground point, at most 1.1 m). A lone body on a slope lies along it.
- **Drape (#229):** at four stations along the body (±0.25, ±0.75 of the half length) the
  higher side's support off that plane bends the body, at most ±0.4 m
  (`piles::MAX_DRAPE`): a man lying across another lies over him with his head and feet
  down on the ground, and one in a hollow sags into it. `ModelInstance::drape`
  (`acr_render::model_renderer::Drape`: four heights in metres and where the stations lie
  along the model's z) goes into two texels after the look; bit 1 of the instance data's
  fourth texel `z` marks it (bit 0: damaged). `vs_main` raises the model's up by the height
  interpolated along z after the palette and the decay collapse, and tilts the normal by
  the bend; the shadow pass uses the same `vs_main`. Bodies the ragdolls posed get no drape
  (the physics already laid them on the pile), nor do far meshes (`vs_far`).
- Piles settle: a body's thickness shrinks by 65 % of its `deflate` weight and its sink
  lowers its top, so the bodies above come down as the ones below decay.
- Blood pools and the dropped death gear (#206) stay on the ground; living units are not
  lifted.
- **Stacking only on change (#230):** `piles::Piles` keeps the footprints as last stacked
  and their rests. The same corpses in the same order reuse them; up to 16 bodies that
  died after all the others are placed alone against the bodies within their reach; a
  corpse gone, more new ones, or a thickness or ground point drifted by over 0.5 px
  (decay, sink) stacks the whole pile again through a flat grid (64 px cells, coarser when
  spread wide). The terrain under a corpse's samples is kept per unit
  (`ThreeD::pile_ground`). The log line every 500 frames gives the µs per frame and how
  often it stacked and placed alone.
- Cost, render VM (RX 9060 XT, release), blank grass field, 800 EngM18 killed into
  40 × 20 m (`ACR-analysis/genai/piles/steps/packed.json`, `profile` over 100 ticks):

  | Scope (ms per frame) | main (3 runs) | branch (4 runs) |
  |---|---|---|
  | `3d.piles` | 0.556 / 0.556 / 0.587 | 0.019 / 0.025 / 0.025 / 0.021 |
  | `3d.instances` (VM noise ±0.2) | 1.42 / 1.40 / 1.79 | 1.42 / 1.77 / 1.85 / 1.54 |

  GPU (`gputime`, zoom 2 on that pile, about 600 animated corpses draped): models
  5.42 → 5.68 ms, all passes 6.54 → 6.80 ms. Before: Monongahela (`battle3.m3d`) with
  530–570 corpses 0.25–0.5 ms per frame on the Snapdragon X.

#### Trampled soil (#223)

Where many units walk, the ground darkens into trodden paths and slowly recovers, in
sprite and 3D mode (`acr-game/src/ingame/trample.rs`, render only). Each tick every moving
land unit adds 0.02 wear to the terrain vertex under it (a unit crosses a vertex in about
three ticks: some five walkers start a path, fifteen wear it out, one leaves no trace) and
all wear decays by 0.9997 per tick (half gone after about four minutes). Once a second the
wear goes to `TerrainRenderer::set_trample`, which rewrites the per-vertex light levels of
the vertex texture: up to `TRAMPLE_DARKEN` (10) levels darker through the original's
darkness table, with a ragged edge from per-vertex noise; bright textures (light 0, water)
stay. Not saved: a loaded game starts with fresh ground.

#### Fire: shell fires and burning men (#256)

`ingame/fire_view.rs` (`FireView`, called from `ThreeD::update_particles` after the impact
particles, only with models on) draws fire from the simulation's state, render only:

- **Shell fires** (`World::ground_fires`, rule `hit_locations`): per fire 40 flames, 10
  embers, 4 glows and 6 smoke puffs a second (emission carried over between frames, so it
  does not depend on the frame rate), flaring up over 2 s and dying down over the last 6 s
  of its 30 s; smoke goes on at a quarter while it dies. Flames scatter over 0.75 of its
  radius.
- **Scorch:** once a second over a fire's first 10 s four overlapping ellipses go into the
  ground state's scorch channel, from 0.25 to 1.0; scorch is never faded, so the black
  patch stays after the fire (detailed ground only, like craters).
- **Burning men** (`Unit::burning`: shell fires and the original's `X`/`F` weapons, whose
  flag never goes out): three bands of flames over his height (legs, chest, head; 2.4 m tall
  on horseback), embers, a glow at his feet and smoke from his head. The particles stay in
  the world, so a walking man leaves a trail of flames and smoke. The corpse of a man who
  died burning (`DeathCause::Burned`) keeps low flames along his length for 8 s and
  smoulders for 40 s.
- **Burn amount for the tint (#207):** `ThreeD::burn(unit index)` / `FireView::burn`, 0..1:
  grows by 1/8 a second while he burns and stays; a burned corpse is at least 0.6. It
  drives the charring tint of his model (`Wounds::burn`, "Deaths by cause" above).
- **Rain** (`weather`): flames 0.6 times as large and as many, twice the smoke, pale grey
  (steam) instead of dark.
- **Particles** (`acr_render::particles`): new kinds `Flame` (rises at 90 px/s², a
  teardrop of noise from white-yellow root to dull red tip; with volume (m2-bld-chunks):
  three tongues in one quad, a wide dim sheath, the body and a bright core, each torn by
  its own noise rolling up at its own speed and as bright as the way through a round
  tongue is long, five value noises a fragment), `Ember` (a twinkling spark),
  `Glow` (a still, faint warm disc on the ground: the fire's light, cheaper than a point
  light) and `Smoke` (like dust, but rises, grows to four times its size and uses an
  arithmetic hash, as `sin` hashes of large arguments show blocks on dark smoke). The three
  emissive kinds are drawn last of the soft kinds, premultiplied with a small alpha, so
  they mostly add light. Variety from hashes of the fire or unit and an emission counter.
- The driver's `impact` with `"cause": "shell", "by": <howitzer>` also starts the shell
  fire (`ground_fire::shell_burst`), so a line of fires can be set without waiting for
  shells; `"by": <mortar>` (`ArtMrt`) starts none and draws the mortar's smaller burst.

Cost, RX 9060 XT, blank grass field, 10 shell fires 45 px apart in a line with two men
walking through (`ACR-analysis/genai/fire/scripts/perf.json`), zoom 1, 1600x900:

| | particles pass | all passes | `3d.particles` CPU |
|---|---|---|---|
| 10 fires burning (two runs of 30 frames) | 0.21, 0.30 ms | 0.84, 0.95 ms | 0.034 ms/frame |
| the same field after they burnt out | 0.02 ms | 0.76, 0.67 ms | 0.001 ms/frame |

Open: flames on the bones (spine, limbs) of the posed model instead of bands over his
height; real point lights; burning men panicking (sim, follow-up).

#### Burning, collapsing and ruined buildings (#303)

`ruin_view.rs`, render only, with `models=all`; variety from the unit id.

- **Burning down** (`SLOWDEATH`, `combat::slow_death`): the model stays drawn. Flames and
  embers rise from its `socket.fire.<i>` anchors and smoke from `socket.smoke.<i>` (EngKre);
  a model without them (the generated `genbuild` meshes) burns at up to 10 of its vertices
  above 60 % of its height. The anchors lit follow the original's count (`n·(T − L)/T`,
  `T` = 3/4 of its life, `effects::burning_points`, 0x505e5f), smoke from 10 %, and the
  model chars (`Wounds::burn` = 0.55 · share alight^0.8). The 2D fire sprites
  (`Effects::fire_sprites`) are left out for a building drawn in 3D.
- **Stages** (`ruin_view::Stage`, by the life lost): *small* from 25 % (the original's
  first fire points: half-strength flames 0.55 × size at the anchors, few sparks, thin pale
  smoke), *roof* from 50 % (full-size flames, 2 × 2 flame points over the roof at 92 % of
  its height, darker smoke), *blaze* from 75 % and while it falls in (1.45 × flames that
  rise and spread 1.4 × as far, 3 × 3 roof points, twice the sparks, thick dark smoke
  1.3 × as large). The rubble burns as a roof fire while more than 30 % of its flames are
  left, then small. A warm `Glow` disc lies round its walls (its light; at most 4 m,
  growing with the stage). Soot climbs the walls over each anchor alight
  (`building_hits::place`, `ScarKind::Scorch` 0.5–1.4 m, the first lit darkest, at most
  `MAX_FIRE_SOOT` 10 of the 32 scars) and soot all over up to 0.45.
- **Wind:** smoke drifts with the wind that sways the trees (`Particles::wind`,
  `SMOKE_DRIFT` 1.8 m/s × the weather's wind scale along `WIND_DIR`, none with `wind=off`):
  a particle takes it over its first 2 s, so a column rises straight and then bends;
  embers take 0.6 of it, flames 0.25. Shell fires and burning men drift alike.
- **Falling in:** from the death on, over `COLLAPSE_SECONDS` (2.5 s), the model sinks
  0.4 m, tilts up to 8° about an axis picked by its id (3° stay) and is squashed to 30 % of
  its height, charring to black (burn 0.55 → 1). Pale dust (smoke particles) and splinters
  fly over its footprint (`FireView::buildings`). The original's `DESTRUCT` explosions
  (`combat::destroyed_building_explodes`) burst over it, but thinned so the collapse shows:
  while a building falls in as a model (`ThreeD::veils`: its death up to 1 s after the
  collapse, explosions of its own `DESTRUCT` weapons within its footprint plus 48 px),
  their sprites are drawn half transparent and only for the first 20 % of their frames
  (`effects::projectile_sprites`, `VEIL_SHARE`). Sprite buildings and `models=off` keep the
  whole explosion.
- **Falling pieces:** on its first collapse `sync_gpu` splits six pieces off the finished
  model (`ruin_view::chunks`), one per `CHUNK_BOXES` box (four roof corners, a piece of
  the front and of the back wall, as fractions of the model's bounds), each a model of its
  own with its textures shrunk to 256 px (logged as `building pieces` with its triangles
  and the time of the cut). Each is a broken slab, not a clean block:
  - **Jagged outline:** the surface inside the box's distance field broken by two octaves
    of value noise (0.7 m and 0.22 m cells, `JAG` 0.35 m, at most 30 % of the box's
    smallest half extent; seeded by the box). Triangles the outline crosses are split at
    their midpoints down to `FRACTURE_EDGE` 0.15 m (at most 7 times) and kept by their
    centre; far from the box's faces no noise is sampled. Vertices are interpolated
    (position, normal, UV, colour), the source corners stay shared.
  - **Thickness and back:** the kept surface's second simplified level (`lod::levels`,
    about an eighth of its triangles, with only the vertices it uses), turned over and
    pushed in along the smoothed normals by a board on timber (`THICK_WOOD` 0.09 m) or a
    wall on stone (`THICK_STONE` 0.32 m; at most 35 % of the piece's radius), in its own
    material, darker (`Break::add_back`).
  - **Broken edge** (`Break::rim`): a strip from the outline down to the back in an inner
    material, `fracture.wood` (raw wood, 0.62/0.47/0.27 linear, pale at the surface,
    darker deep in) or `fracture.stone` (broken stone 0.46/0.44/0.41, mottled, one point
    in five pale mortar), told by the piece's mean texture colour (warm is wood, as
    `scarred`). The back's rim points are shifted in and out along the outline by a hash,
    so the edge is ragged; timber throws thin splinters (0.08–0.3 m) off 45 % of the
    outline edges, seen from both sides. Only open edges within `OUTLINE_BAND` (0.3 m) of
    the cut count: the generated meshes have open seams all over that do not break.
  Everything is hashed from the box and the points (millimetres), never the simulation.
  - **Off the frame:** a building that has lost `HEAVY_DAMAGE` (50 %) of its life asks
    for its type's pieces ahead; on native a worker thread cuts them and builds their
    levels of detail (`ModelRenderer::prepare_model`), and `sync_gpu` uploads one piece a
    frame (`add_prepared`). Where they are not ready by the time the first piece breaks off
    (`BREAK`, 0.35 s after the death: a building killed at once, or the browser, which
    has no threads) the plain pieces are cut at once as before (`chunks(model, false)`:
    the triangles whose centre lies in the box, 5–20 ms a type).
  Each breaks
  off 0.35–1.25 s after the death where the falling model has it then, flies outwards at
  1–2.6 m/s and up at 0.4–1.6 m/s, tumbling, under gravity; it bounces once (30 % of the
  speed kept) and lies still in the rubble, charred up to burn 0.4 only so it reads
  against the black shell; after 25 s it sinks away over 4 s (`chunk_fall`, from the unit
  id). A puff of dust rises where it lands (0.5 s, `Burning::landings`).
- **Ruin:** a model with state groups (`<name>.finished`, `.build<k>`, `.section<s>_<k>`,
  `.ruin`; docs/assets.md "States") draws `finished` plus its sections in their states
  while standing ("Construction and upgrades (#302)") and its `.ruin` group alone once fallen (`ModelData::without_groups`); others stay as the
  squashed, charred mesh. Every generated building (`genbuild`) carries a generic ruin
  (`acr_assets/ruin.py`, docs/assets.md "States"). A ruin state appears black and cools to
  burn 0.3 over 4 s (it carries its own soot). Flames go on for 20 s, smoke for 90 s.
  The ground round it chars over its first 10 s from the death (ten scorch ellipses round
  the footprint into the ground state's scorch channel, once a second, as shell fires),
  and stays black (detailed ground only).
- **Particles:** smoke rises from at most `SMOKE_COLUMNS` (6) of the smoke anchors, spread
  evenly, at 2 puffs a second each (0.7–1.1 m, 3.5–6 s); the collapse dust is low and short
  (1.5–3 s, 18 a second). The particles pass is fill-bound: larger puffs cost more than
  they save (1.0–1.6 m puffs measured 2.20 ms). It stays while the
  simulation keeps the unit (`#DEATHLIE*`); a type without `#DEATH` (EngKre, EngSkl) is
  removed by the simulation at once, and its ruin stays `RUIN_TICKS` (600 ticks, as long as
  a `#DEATHLIE1` lasts).
- **Driver:** `spawn` takes `max_life` after `life`, so a building spawned at 900 of 3000
  burns down at once (`SLOWDEATH` drains 5 life a tick from the next 128-tick check).

Measured (render VM, RX 9060 XT, 1600×900, blank grass field, free camera at zoom 1.4: an
intact EngKre, a burning EngKre and EngSkl at 900/3000 life; steps
`ACR-analysis/genai/destruction/steps4.json`): `3d.instances` 0.011 ms a frame,
`3d.particles` 0.044–0.088 ms; GPU particles pass 1.41 ms burning and 1.64 ms while they
fall (sum of all passes 2.25 and 2.52 ms). Frames: `ACR-analysis/genai/destruction/`.
With the smoke columns: GPU particles pass 1.41 → 0.76 ms in the same run (burning).
Ruins of EngSkl, EngCen, DelCen and an EngSkl burn → collapse → ruin sheet:
`ACR-analysis/genai/destruction/ruins/`.

Stages, wind and scorch (same VM and size, blank grass field, free camera at zoom 1, ten
buildings at 85 % lost (EngDo1 and SpnKre, all in a blaze), two runs of 30 frames each;
steps `ACR-analysis/bld-fire/scripts/perf.json` from `mk_steps.py`, settings models=all):
`3d.particles` 0.10–0.12 → 0.13–0.18 ms a frame, `3d.instances` 0.02–0.03 → 0.01–0.04 ms;
GPU particles pass 0.96–1.00 → 0.95–0.97 ms, models pass 0.96–0.99 → 1.42–1.47 ms (the
soot scars), sum of all passes 2.59–2.62 → 3.02–3.07 ms. Stage, hit-side and ruin sheets
main against branch: `ACR-analysis/bld-fire/` (`look.json`).

Thinned explosions and falling pieces (same VM and size, blank grass field, EngSkl and
EngBlg spawned at 20/3000 life, free camera and the fixed camera at zoom 1, 50 frames from
just before the death; steps `ACR-analysis/genai/collapse/steps.json`, EngBlg free camera
window): `3d.update` 0.066 → 0.079 ms a frame, `3d.instances` 0.019 → 0.029 ms,
`3d.particles` 0.080 → 0.086–0.092 ms; GPU sum of all passes 1.46 → 1.38–1.42 ms after
the collapse (noise). Sheets main against branch: `ACR-analysis/genai/collapse/compare-seq*.png`.

Fractured pieces (same VM and size, blank grass field, free camera at zoom 0.75 over five
buildings dying in the same tick: EngDo1, SpnKre, EngSkl, EngKre, EngBlg spawned at
20/3000 life, 5 ticks, a 300-frame `frametimes` pause (the workers' wall time the driver's
fast ticks lack), then one-tick `wait`s whose `ms` is the frame; settings `models=all`,
the quantized showcase models; steps `ACR-analysis/genai/collapse-frac/steps4.json`, two
runs each): the worst frame of the collapse 72 / 72 ms on main (the five types' pieces cut
and uploaded in the death frame) against 36 / 33 ms (the collapse frame itself; the
workers took 91–346 ms for 34k–135k triangles, the uploads at most `UPLOAD_MS` 3 ms a
frame). GPU sum of all passes with the pieces lying 1.70 / 1.73 → 2.27 / 2.28 ms (models
0.19 → 0.58 ms, shadow map 0.05 → 0.28 ms). Close-ups at zoom 4.5 (EngDo1) and 4 (SpnKre),
main against branch: `ACR-analysis/genai/collapse-frac/sheet_closeup_z45.png`; collapse
sequences (unquantized models, before the worker): `sheet_seq_do1.png`, `sheet_seq_kre.png`.

Open: the standing model keeps the pieces' triangles, so they are hidden in it until it
moved away; the browser (no threads) and a building killed outright get the plain pieces;
the pieces of the dense generated meshes keep the source's density (34k–135k triangles a
type), the back is cut to its shadow level only.

#### Construction and upgrades (#302)

`build_view.rs`, render only, with `models=all`. A building whose stage is below its
`BUILDSTAGES` (alive, not dying) is drawn as a model too:

- **Stage groups:** a model with `<name>.build<k>` groups (EngKre) draws the group of the
  `#BUILDLO_k` the simulation shows (k = stage·4 / stages, `construction::stage_anim`) with
  the parts outside every group, as a model of its own (`ModelData::without_groups`, built
  at load, `ThreeD::states`).
- **Rising (every other model):** `Rise` on the instance (flag bit 3 of the instance data,
  one texel after the drape, before the scars): the model is cut at a height that climbs
  from a 0.3 m foundation course at stage 0 to its top (`build_view::rise`, the top from
  `ruin_view::anchors`). The cut line steps in courses of 0.3 m that change about every
  metre along the walls (`rising_below` in `model.wgsl`), as logs or stones laid one course
  at a time; a 1.4 m band under it shows fresh, lighter material, upright poles about every
  2 m along the wall (across the fragment's normal), plank ledgers every 1.2 m and a pale
  rough top (`rising`). The vertex shader passes the distance below the cut unclamped in
  `cut.x` (it interpolates exactly over long walls) and the band as a negative `cut.w`;
  `fs_main` and `fs_shadow` drop what lies above. A rising instance is never `clean`.
  No fires for the life it has not got yet (`lost` is 0 while under construction).
- **Sections (upgrades):** the simulation keeps a fort's upgrade as section states on the
  same building (`Unit::stages`, five 3-bit fields: 0 none, 1 being built, 2 done, 3 ruin;
  `upgrade::section_state`, raised by `UPSTAGEn`). A model with `<name>.section<s>_<state>`
  groups (EngKre: the wooden corner, the stone house in scaffolding, the stone house, its
  ruin) draws each section's current state as an overlay instance with the building's
  transform and look (`ModelData::only_groups`). A single-mesh model draws `<NAME>UP.glb`
  instead of `<NAME>.glb` once every section of the type is done (`build_view::upgraded`),
  where that file exists.
- **Base/Up pairs:** every Pixal3D `*Base`/`*Up` pair is an `ADDSECTION` type: AtcKre (3
  sections), AtcVer (2), EngKre (1), EngKre2 (1), FrnKre (2), FrnKre2 (1), IncDo3, IncKre
  (3), MayKre, PueGor (4), SpnKre (3), UsaKre (1). `assets/buildings/<type>up.py` builds the
  Up model (fitted to `model-ref --sections 2`) for every pair with a `from_sprite` base:
  AtcVer, EngKre2, FrnKre, FrnKre2, IncKre, MayKre, PueGor, SpnKre, UsaKre (sprite IoU
  0.82-0.94). Left out: EngKre (scripted, with section groups), AtcKre and IncDo3 (no base
  model). FrnKre is not in the game's catalog (the driver's `spawn FrnKre` finds FrnKre2).
- **Driver:** `spawn` takes `progress` (0..1 of `BUILDSTAGES`, with its `#BUILDLO_k`) and
  `sections` (a state per `ADDSECTION`, in order).

Measured (render VM, RX 9060 XT, 1600×900, blank grass field, free camera at zoom 0.7 over
a 5×5 grid of EngSkl, steps `ACR-analysis/genai/build-stages/scripts/perf_*.json`, two runs
each): sum of all GPU passes 1.31–1.35 ms all finished, 1.47–1.56 ms all at 50 %
(models 0.58 → 0.64–0.69 ms, shadow map 0.10 → 0.19–0.20 ms: the rising casters go
through the `discard` pipeline); `3d.instances` 0.012–0.013 → 0.013–0.021 ms a frame.
Frames and sheets (generic 0–100 %, EngKre `BUILDLO_0..3`, sections and UsaKre Base/Up):
`ACR-analysis/genai/build-stages/`.

Open: the Up scripts for the other eleven pairs; a scaffold as geometry standing above the
cut (poles and platforms); per-section Up models for forts with several sections (one Up
model shows all of them done).

#### Hit damage on buildings (#304)

`building_hits.rs`, render only, with `models=all` (buildings in 3D); not gore, so shown
with every gore setting. Each hit record on a building (`HitRecord::building`) is placed
once, deterministic from the record and a hash of (victim, tick, its place among the
tick's building hits):

- **Placing:** a ray against the model's triangles (pose at time 0, model frame, built
  once per model on its first hit) through the point the round struck (`World::hit_at`;
  a hit recorded at the building's centre, e.g. a scripted `hit`, is spread over the face
  towards the shooter), along the flight: level at 12–62 % of the model's height for balls
  and shot, plunging 14–39° for arrows (walls and roofs). The first face met takes it.
- **Scars** (`model_renderer::Scars`, `model.wgsl` `scarred`): a cannonball's hole
  (0.2–0.3 m: a dark ragged hole, pale splinters round it on wood, a rounder crater with
  radial cracks on stone and plaster; wood is told from the fragment's colour, warm against
  grey), musket and grapeshot pocks (0.035–0.09 m), a shell's scorch (0.7–1.1 m of soot
  climbing the wall, a chipped crater) and an arrow's puncture. They lie in the model frame
  after the node transforms (the vertex shader passes that position as `rest` for a scarred
  instance, flag bit 2 of the instance data) and are drawn only within 1.2 radii of the
  struck face's plane. 32 per building (`MAX_SCARS`): pocks and punctures go first, then
  scorches, holes last.
- **Stuck arrows:** `damage::procedural_arrow` (0.62 m, three vanes) at the hit point along
  the flight, a little off it, tip 7 cm in; 24 per building (`MAX_ARROWS`), the oldest go.
  They are instances appended after the units.
- **Battered:** from 50 % of its life lost on, cracks (the contour of a noise) and soot
  patches grow over the whole model, full at 0 life. A repair takes away the oldest scars
  and arrows in the share of the lost life it mends, all at full life. A building that
  burns down or falls loses them (`ruin_view` draws it then).
- **GPU layout:** after the instance's data, look and drape: a header (count, batter, soot,
  groups), a sphere per group of 8 neighbouring scars (sorted by bearing around the model,
  then height) and two texels per scar. A fragment tests the spheres and reads only the
  scars of the near groups.

Measured (render VM, RX 9060 XT, 1600×900, blank grass field, free camera at zoom 1.5 on
an EngKre after 63 scripted hits (30 musket, 20 arrow, 10 cannonball, 3 shell; 56 % life
lost) next to an EngDo1 after 35; steps `ACR-analysis/genai/building-hits/steps.json` from
`mk_steps.py`): GPU models pass 0.23 ms on main, 0.58–0.69 ms with the scars (the fort
covers about half the window); an unhurt fort draws as on main (0.17 ms both).
`3d.instances` 0.057 → 0.12–0.15 ms a frame. Frames: `ACR-analysis/genai/building-hits/`.

- **Rubble (m2-bld-chunks):** a cannonball's hole and a shell's (a shell now bursts a hole
  under its scorch) knock out 3–5 (shell 6–8) pieces, `damage::procedural_rubble`: a broken
  block with pale broken ends, a stone block (0.06–0.14 m half size) or a board
  (0.2–0.45 × 0.05–0.09 m) by the struck triangle's colour (`ruin_view::is_wood` of
  material × texture × vertex colour at its middle; textures kept encoded until upload are
  decoded once for this). They drop out of the hole outwards (a shell's fly further), fall
  with gravity, bounce once and land on a face after whole quarter turns, and lie for the
  battle: 600 in all (`MAX_PIECES`), the oldest go; a new game clears them. Instances
  appended after the units, cast shadows.
- **Fire arrows:** `HitRecord::fire` (render only, not hashed; set in `make_damage` from
  the weapon's `F`/`X` flags, `OSTRELA`; the driver's `hit` takes `"fire": true`). The
  arrow sticks as any arrow and leaves a small scorch (0.16–0.26 m) and a small fire there
  (`FireView::arrow_fires`: a tongue or two, sparks, thin smoke) for 8 s
  (`ARROW_FIRE_SECONDS`), on for as long as the building burns once it caught (the
  original lights it after `FIRELIMIT` of them, `combat.rs` `FIRE_SLOW`, 0x4e8990); 8 per
  building (`MAX_ARROW_FIRES`).

Measured (m2-bld-chunks, render VM, RX 9060 XT, 1600×900, ten blazing buildings, steps
`ACR-analysis/bld-fire/scripts/perf.json`, `models=all`): particles pass 0.96 ms on main,
1.04–1.06 ms with the flames' volume below; sum of all passes 3.03–3.08 → 3.10–3.14 ms.
Frames and sheets: `ACR-analysis/bld-chunks/` (`cmp_*.png`, steps `scripts/look.json`).

Fire soot over a burning building's fires: "Burning, collapsing and ruined buildings
(#303)", Stages. Open: the record has no height (hashed); grey weathered timber reads as
stone to the colour test (EngDo1's rubble is stone blocks); rubble keeps one colour per
material, not the wall's.

### Projectiles (#308)

With models on, the rounds in flight are drawn as code-built models
(`acr-render/src/missiles.rs`) by `ingame/missile_view.rs`, called from `ThreeD::update`
after the debris; their `WEAPON.ADS` sprites are left out (`effects::projectile_sprites`
with `ThreeD::draws_rounds`). Smoke, flashes, explosions and debris keep their sprites.

- **Which objects:** `missile_view::round`: a `FLY`, `ANGLE` or `REFLECT` object whose
  shooter's attack (`p.sender`, `p.att`) is a missile weapon of `NRES.DAT [WEAPONS]`:
  `STRELA*`/`OSTRELA` arrow, `IADRO` cannonball, `KARTECH` grapeshot, `IADROMOR`/`VAFLIA`
  shell, `TAMAGAVK` tomahawk, `KOPIE` spear, `KAMEN` stone.
- **Placement:** at `pos >> 14`, the height `z / COS_E` like the ground, moved on along
  `vel` (and `az`) by the share of the tick the frame is into, so rounds fly smoothly between
  ticks. +z of the model along the velocity. A gun's `ANGLE` arc starts a few pixels above
  the ground in the simulation (the sprite's offset shows the muzzle): it is raised by
  `0.1 ×` the distance left to `dest`, at most 0.9 m (cannonballs and grapeshot).
- **Shells** (`missile_view::shell_arc`) fly a true parabola instead: the ticks flown come
  from the climb (`vz0 = |v| · tan(elevation)` less `az` a tick; the elevation of the
  `GunMode`, a mortar's 60°, else the weapon's `ANGLE`), the ticks left from running the
  simulation's own steps on to the burst (within 16 px of `dest` or below the ground, at
  most 1200). The height is the parabola from 0.9 m above the muzzle's ground to the ground
  at the burst with the elevation as its launch angle (peak `range · tan / 4`), the
  horizontal place the simulation's, so the shell lands on the burst at its tick. The
  simulation's own z also comes down there; drawn directly it peaked about 15 % higher and
  was still 1.5 m up at the last tick. Shells turn over slowly (0.3 rad a tick); the fuse
  throws an ember every frame and smoke once a tick. The missile models are added with
  `ModelRenderer::add_small_model` and always drawn at full detail: the far levels dropped
  the thin shafts entirely.
- **Looks:** arrows 0.8 m with a 40 mm iron head, a nock and three feathers, shafts 9.6 mm
  thick as in life; their material's `Finish::thin` (1 px) keeps them at least a pixel wide
  at any zoom: `vs_main` pushes vertices nearer the model's z axis than half a pixel out to
  it (`globals.forward.w` is world px per screen px, 0 in the shadow map), so the mesh stays
  thin close up and the shaft never flickers away zoomed out; cannonballs dark spheres of 0.09 m (life
  0.047 m) trailing a thin pale ribbon: every frame small swelling, fading puffs (`Dust`,
  1.4–2 px, 0.7–1 s) every 2.5 px of the way flown since the last frame (at most 32 a frame,
  a ball's last position kept per slot), so the trail is continuous between ticks; grapeshot
  0.03 m; shells 0.11 m; tomahawks turn end over end (1.7 rad per tick), stones tumble, spears roll. The
  phase is a hash of the slot, weapon and shooter.
- **Musket balls** are line traces in the simulation (0x5589ec), never objects in flight:
  for each `Gunshot` hit or impact of the current tick a thin pale streak (`streak`, 2.2 m,
  0.022 m radius, dithered half transparent) runs from 1.4 m above the shooter (`pos − dir`)
  to the victim's chest or the ground, at 4500 px/s through the tick.
- **Stuck arrows:** an arrow impact (a miss: `note_impact` records arrows only when they hit
  no one) sticks an arrow 0.12 m into the ground on the bearing of the nearest arrow seen
  in the last two ticks (else of the impact), 35–55° down: the simulation's arrows fly
  nearly flat and would lie hidden in the grass at their true angle.
  They stay `corpse_minutes` (at most 2048). Arrows in buildings: `building_hits.rs` (#304).
- **Arrows in bodies** (`ingame/body_arrows.rs`): every `Arrow` hit on a man (bolts too;
  a `Thrown` hit only when a spear was seen flying within 90 px of the victim, so tomahawks
  and stones never stick) leaves its shaft in him. The part and level come from
  `HitRecord::injury` (hit locations), else from a hash of the hit (trunk 55 %, limbs 37 %,
  head 8 %). The flight is the nearest arrow or spear seen in the last two ticks, else the
  blow's bearing coming down 8–30°, turned into his model frame by his facing at the hit
  and wobbled by the hash. In the rest pose the shaft enters square to the bone from the
  side it flew at (turned around the bone by up to ±0.7 rad), a third of its length in
  (limbs at most 2r + 5 cm, the head 1.7r, so the point can show through; spears 0.3 m),
  on the palette slot of that chain segment (`Rig::limbs`, `trunk`, `neck`). Each frame it
  is drawn as `transform × palette[slot] × rest`, with the crew or ragdoll palette where
  there is one and else `ModelData::pose` of the instance's clip and time (cached per unit
  while clip and time stay, so held corpses cost nothing): it follows clips, falls,
  ragdolls and corpses. A cut limb, a lost head or a body torn in two drops the shafts in
  the part gone. They stay while the man lives (the original heals no wounds) and with his
  corpse until it goes; at most 6 per man (his oldest goes) and 1536 in all. Not with
  `gore=off` (the original fade), nor on riders and animals (no skeleton-H `rig`).

Cost (RX 9060 XT, blank grass field, 100 archers and 200 musketeers firing at 60 peasants,
zoom 1, two runs of 80 ticks): `3d.missiles` 0.021 and 0.024 ms per frame. Proof frames:
`ACR-analysis/genai/projectiles/proof/` (steps in `../steps/proof_*.json`).

Arrows in bodies (same VM, blank grass, 20 musketeers each struck by 1–3 scripted arrow hits,
half then killed; steps `ACR-analysis/genai/projectiles/body/steps/hits.json`, 100 frames):
`3d.body_arrows` 0.006 ms per frame for about 45 shafts; GPU sum of all passes 0.763 ms
against 0.755 ms on main. A real volley (20 archers into the line, `steps/volley.json`):
0.005 ms. Frames: `ACR-analysis/genai/projectiles/body/`.

Open: streaks for building hits; arrows in riders and animals; shafts on a corpse late in
its procedural decay keep the rest-pose depth while the body collapses.

## Camera math

World coordinates of the 3D path are world pixels: `x` east, `y` south, `z` up as a true
height. A game height `h` (terrain `SURF`, screen pixels) is `z = h / cos 30°`. A model point
`(x, y, z)` in glTF metres (x east, y up, z south) of a model at ground point `G` facing
`RealDir d` lands at

```
yaw = −(d − 64) · 2π / 256
world = G + 30 · (x cos yaw + z sin yaw,  −x sin yaw + z cos yaw,  y)
G     = (gx, 2 · (gy >> 1), h / cos 30°)      (row rounded like the sprites' y >> 1)
```

The camera (`View3d`) looks along `forward` with screen axes `right` and `down`; with the
game camera's top-left `(cx, cy)` in original screen pixels and scale `s`, the target (the
ground point at the window centre) is `T = (cx + W/2s, 2 (cy + H/2s), 0)` and

```
yaw φ, pitch e (fixed camera: φ = 0, e = 30°)
right   = (cos φ, sin φ, 0)
down    = (−sin φ sin e, cos φ sin e, −cos e)
forward = (sin φ cos e, −cos φ cos e, −sin e)
screen  = (W/2, H/2) + s · ((P − T)·right, (P − T)·down)
depth   = 1/2 + (P − T)·a / 2R,   R = 32768,  a = forward (free) or (0, −1, 0) (fixed)
```

For `φ = 0, e = 30°` the screen position is `s · (x − cx, y/2 − z cos 30° − cy)`: with
`z = h / cos 30°` exactly the sprite projection `s · (x − cx, y/2 − h − cy)` of
`terrain::Camera::project` (unit test `view3d::fixed_camera_reproduces_the_sprite_projection`
checks points at scales 1, 1.5 and the zoomed-out 0.25 to 0.001 px through both the
function and the clip matrix). The fixed camera's depth is `−y`: along a view ray `y` falls
monotonically, so it orders every surface correctly, and a sprite (a vertical billboard at
row `y`) has one depth that matches the original's row order. Under the free camera depth is
the true view distance and sprites stand upright at their projected ground point (they still
show their stored view, so they look wrong from behind; they are ordered far to near).

**Picking:** the fixed camera keeps the game's own code. Under the free camera the ground
under the cursor is the view ray met with the terrain (`View3d::ground_at`, the ray against
the height found so far, six times), and unit boxes (`RECTANGLE`) stand at the projected
ground point in screen space, so click and rectangle selection work as before.

## Performance

### Graphics presets (m2-igpu)

`graphics=auto|low|medium|high|ultra` (`ui/graphics.rs`; Spieleinstellungen page 4
"Voreinstellung": Automatisch, Niedrig, Mittel, Hoch, Ultra). A preset gives the frame a
`lod::Budget` and, once when chosen, sets `shadows`, `material_detail`, `wind`, `ground_cover`
and `water`; switches changed afterwards stay the user's.

| Preset | Triangles | Draws | Shadow casters | LOD sizes | Switches off |
|---|---|---|---|---|---|
| Niedrig | 1.5 M | 700 | none | ×0.75 | shadows, material detail, wind, ground cover, detailed water |
| Mittel | 3 M | 1200 | 400 nearest, 0.6 of the reach, 600 k triangles | ×1 | ground cover |
| Hoch | no limit | no limit | all | ×1 | none (today's look) |
| Ultra | no limit | no limit | all | ×1.5 (finer levels hold further out) | none |

- **Budget** (`ModelRenderer::group_draws`, `lod::bias_step`): the triangles and draw calls
  of every model's levels times its instances on screen; when the levels by size would
  exceed it, all projected sizes are scaled by 0.75 per step until it fits (going back finer
  needs 15 % room). When nothing fits, the first step at the floor (all coarsest) is taken.
  The view is orthographic, so a step drops the whole army, not just far men.
- **Shadow casters:** within `shadow_reach` of the shadow map's reach, nearest to the view
  centre first, at most `casters` and `shadow_triangles`; the rest cast none.
- **auto:** `graphics::detect` from `wgpu::AdapterInfo::device_type`: integrated, virtual
  and unknown (the browser) Mittel, CPU Niedrig, discrete Hoch. On the first start (no
  `graphics` line yet) the detected preset's switches are applied and stored (never by the
  driver).
- The driver's `gputime` answers `models`: drawn, instances, triangles, draws, cast
  triangles and draws, `casters_cut`, `lod_sizes` and the twelve heaviest models.

Measured on the render VM (RX 9060 XT, 1600 × 900), mission Single3 (#PAGE0303) loaded by
the driver (`ACR-analysis/igpu/scene.json`: 2574 units, 700 British killed, 1 in 6 by
cannonball), free camera yaw 0, pitch 30°, on the British (3599, 13479); `ACR_GPU_CLASS`
picks the preset:

| Preset | zoom 1: tris / draws / cast tris | GPU ms | zoom 0.5: tris / cast tris | GPU ms |
|---|---|---|---|---|
| Ultra | 9.9 M / 1175 / 5.2 M | 15.1 | 7.5 M / 4.1 M | 13.1 |
| Hoch | 9.3 M / 1175 / 4.6 M | 14.2 | 6.0 M / 3.4 M | 9.8 |
| Mittel | 3.7 M / 844 / 0.38 M | 5.3 | 4.5 M / 0.41 M | 5.9 |
| Niedrig | 3.7 M / 844 / 0 | 5.0 | 4.5 M / 0 | 5.2 |

The floor of 3.7 M is mostly split corpses (cannonball), which stay at authored level 1
(about 10 k triangles, `DAMAGED_UP_TO`), plus 824 stump caps and wound inserts drawn whole.

Windows 11 ARM64, Snapdragon X (Adreno, DX12), release build, 1024 × 768, `engm18.glb`
(5760 triangles, 3868 without the bone layer):

| scene | models drawn | fps |
|---|---:|---:|
| `Miss7E03` start, 1176 English musketeers on the map (2842 units), same view without models | 0 | 166–180 |
| the same view as models | 392 (1 pose) | 79–81 |
| the same, before the bone layer was skipped for the living | 392 | 41–55 |
| `Miss7E01` start | 72 | 180–196 |
| `Miss7E01`, free camera turned | 72 | 143–157 |
| `Miss7E03` battle (marching, firing, dying) | 274 (53 poses) | 55 |

Shadows (`shadows=on`): Monongahela `battle3` at tick 1500, 1600 × 900, 2686 models drawn
(65 poses): 26.3–31.0 ms per driver frame with shadows, 28.7–36.9 ms without (four
alternating runs of 120 frames each, simulation included). The shadow pass is lost in
that noise; it repeats the vertex work of the models once more at 2048².

About 17 µs of GPU time per drawn musketeer. Poses are cheap (one per distinct clip frame);
the cost is the vertex work, so LODs and fewer triangles per figure are the next lever.

### Levels of detail and the cost of a battle (m2-lod)

GPU time per pass (`gputime`, Monongahela `battle3`, 1600 × 900, shadows on, after 300 ticks,
mean of 60 frames, Snapdragon X; scripts and frames in `ACR-analysis/lod/`; about 480 models
on the screen, 22 poses, 1742 instances):

| ms | z1 main (405de52) | z1 `ACR_LOD` off | z1 `ACR_LOD=on` | z2 main | z2 off | z2 `on` |
|---|---:|---:|---:|---:|---:|---:|
| shadow map | 93.3 | 81.9 | 3.7 | 90.7 | 78.3 | 4.8 |
| models | 88.8 | 84.0 | 20.2 | 88.0 | 93.5 | 22.9 |
| triangles drawn / cast | 6.1 M / 6.1 M | 6.1 M / 6.1 M | 0.72 M / 0.31 M | | | 1.15 M / 0.44 M |

- **Profile:** a musketeer is 11.4 k triangles, a dragoon or officer 22.9 k. Before, the 3D pass
  drew every model inside the shadow culling margin (2.5 × the radius beyond the window), and
  the shadow pass drew the same list again, whole, including casters outside the fitted map.
  Both passes cost about 13–14 ms per million triangles here.
- **Now:** the 3D pass draws only the models on the screen (1.5 × radius margin); the shadow
  pass draws its own list: casters within the map's reach, none smaller than one texel in
  radius, a level coarser than seen ([`lod::SHADOW_STEP`]) and without face variants. One
  instance buffer holds both lists.
- **Levels** (`lod.rs`, m2-lod2): index-only, built in `add_model` (5–35 ms per model): every
  primitive gets index lists for up to 100, 35, 12 and 3 % of its triangles from quadric error
  simplification on the rest pose (`meshopt-rs`, a pure Rust port of meshoptimizer 0.15, MIT;
  builds for wasm, so no asset change). Edge collapses onto existing vertices keep the
  silhouette and the attribute seams; positions where two body parts (`_PART`) or hair variants
  meet are locked (the port has no lock flag: one unreferenced copy of such a position makes
  the simplifier classify it `Locked`). Levels 2–3 simplify the position-welded shape where
  the seams stop the first pass far short (each corner then takes the vertex at its position
  whose normal is nearest the triangle's). Each level stops at an error cap
  (`lod::MAX_ERROR`: 1, 1.5, 2 % of the primitive's extent) rather than reach its ratio, so a
  soldier keeps about 48 / 21 / 13 % of its triangles. Skinning, morphs, `_LIMB`/`_GORE`, decay
  masks and the look variants' vertex buffers stay as they are (the levels share the vertices).
  The level is chosen per model by its projected size (2 × culling radius in screen pixels:
  520, 260, 160 px; finer again only 15 % above a threshold), so close-ups (the bench, the free
  camera near) stay at level 0; caps and wound inserts are always whole; the face and body
  variants are drawn up to level 1 only.
- **Shadow casters** take one level coarser than seen; from level 3 on a fifth list made by
  meshoptimizer's "sloppy" grid simplification (3 %, no seams or borders: a shadow has none).
  `lod::sloppy` runs it per hair/beard variant and over only the vertices that group uses:
  meshoptimizer picks each grid cell's representative among *every* position it is given, and
  given the whole model's buffer it took corners from unworn hair variants, which `vs_main`
  throws to clip `(2, 2, 2)`. Those triangles smeared from each soldier towards the shadow
  map's corner (light right + up, i.e. north: up the screen): the "way too long" streaks of
  large battles at zoom 1 (`lod::tests::shadow_list_keeps_to_its_own_vertices`).
  The streaks were huge triangles, so the fix also cut the shadow pass on Monongahela
  `battle3` (tick 1500, zoom 1, 1600 × 900) from 19.8 to 5.8–5.9 ms GPU time. Damaged
  casters follow the drawn list's rule: only [`Damage::splits_parts`] keeps them at level
  ≤ 1, a corpse with wound volumes alone casts from the far lists. 700 v 700 melee on the
  blank field (`ACR-analysis/perf/clash.py`, zoom 1), shadow map / all passes in ms GPU time,
  main 928a909 → both changes: before contact 38.1 / 60.8 → 8.7 / 42.8, at contact
  50.5 / 83.1 → 14.5 / 53.5, 30 s later 75.3 / 146.0 → 30.6 / 98.9.
- **Silhouette** (`lod::tests::real_models`, ignored; `ACR_LOD_MODELS=<folder>`): filled pixels
  of the rest pose projected along each axis, the worst axis against level 0
  (`ACR-analysis/lod2/silhouette_models.txt`): soldiers (engm18, engkf18, engd18, puevsda,
  siukstr) −0.3…−1 % at level 1, −2…−3 % at level 2, −5…−7 % at level 3; the Iroquois
  −0.4…−1.5 / −2…−5 / −5…−7 % except thin-limbed anmcrosm, anmoln, anmjug (−8…−17 % at 2,
  −12…−30 % at 3: thin feathers and limbs); ships −5 / −14 / −28 % (rigging; still open). The
  clustering before shrank the figures visibly (`ACR-analysis/lod/tri_z2.png`); the battle
  frames per forced level are in `ACR-analysis/lod2/levels_z2.png` (off, 1, 2, 3, by size).
- **On by default**: `lod=on|off` in the settings file; `ACR_LOD` (native) overrides it: `off`,
  `on`, or `<0..3>` forcing a level.
- **Fragments:** levels 2–3 draw with `fs_far` (a second pipeline over the same `shade`
  function, draws sorted near first): no face detail (stubble, wrinkles, pock marks, five value
  noises), one shadow tap instead of nine, no highlight on leather and brass; wounds, holes,
  cuts, decay and the look colours as near. The cut's ragged noise and the shreds' noise run
  only near a cut or a hole.

GPU time after m2-lod2 (same scene and script, `ACR-analysis/lod2/`, two runs of the default):

| ms | z1 off | z1 clustering (m2-lod) | z1 default | z2 off | z2 default |
|---|---:|---:|---:|---:|---:|
| shadow map | 76.3 | 3.7 | 3.3–3.5 | 79.3 | 7.4–7.7 |
| models | 87.0 | 20.2 | 26.5–29.1 | 87.8 | 36.2–39.6 |
| all passes | | | 33.0–36.0 | 171.3 | 46.4–50.3 |
| triangles drawn / cast | 6.1 M / 6.1 M | 0.72 M / 0.31 M | 1.57 M / 0.34 M | | 2.36 M / 0.71 M |

The models pass is still about 3 × the 10 ms goal at zoom 1: the quadric levels keep 2–3 × the
clustered triangles (a musketeer's level 3 is 2.9 k of 11.4 k, held by the region locks and the
many small gear primitives), and per-pass times on this tiler shift between passes from run to
run (a forced level 2 run: 14.9 + 22.9 ms). Next: merge the small gear primitives per material
before simplifying, an impostor or a single-primitive far mesh for level 3, and instancing the
look variants.

**Far meshes per material (m2-lod3).** From level 2 on a model no longer draws its primitives:
`ModelRenderer::far_meshes` joins, per material (and inner decay layer), the primitives'
level-1 lists and simplifies them as one welded shape without the region locks, only the
hair variants kept apart (`lod::far_mesh`: 12 % of the triangles with a 2 % error cap for level 2, 6 % and 4 % for
level 3, relative to the whole material's extent), so buttons, straps and the separate
pieces of a uniform melt into the rest; the shadow list is a sloppy grid of level 3
(`lod::far_shadow`). `_PART`/`_LIMB` stay per vertex, so cuts still discard, along a
coarser line. Each far list gets its own copy of the vertices it uses (appended to the
model's buffer). One draw per material instead of per primitive (a musketeer: 23
materials). Triangles per level (`lod::tests::real_models`, L2 / L3, worst silhouette
change at the level's size): engm18 2376 / 1173 (−3.6 / −7.1 %, before 3793 / 2900),
engd18 3693 / 1885 (−1.4 / −4.8 %), puevsda 3236 / 1610, siukstr 3194 / 1606, anmmed
1387 / 687, anmoln 1403 / 696 (−10 / −18 %, thin limbs). The error cap still stops level 3
above the 600–1000 aimed at for a soldier.

GPU time after m2-lod3 (`ACR-analysis/lod3/`, `runs.py`, two runs each, same scene):

| ms | z1 main (5c3f6ad) | z1 m2-lod3 | z2 main | z2 m2-lod3 | z1 / z2 off |
|---|---:|---:|---:|---:|---:|
| shadow map | 3.2 | 8.6–8.7 | 7.3–7.4 | 10.9–11.8 | 107.6 / 91.7 |
| models | 26.3–26.6 | 11.6–11.7 | 36.1–36.2 | 22.4–23.4 | 122.8 / 110.5 |
| all passes | 32.6–32.9 | 23.1–23.2 | 46.3 | 36.6–36.8 | 234.4 / 205.6 |
| shadows off: all passes | 39.4 | 19.7 | 52.1 | 31.7 | |
| triangles drawn / cast | 1.57 M / 0.34 M | 0.77 M / 0.13 M | 2.36 M / 0.71 M | 1.81 M / 0.42 M | 6.1 M / 6.1 M |

The split between the shadow map and the models pass is not to be trusted on this tiler:
the shadow pass "rose" while it casts a third of the triangles, and with shadows off the
models pass costs more than models plus shadows with them on; the sum is the measure.
Open: the look variants still cost a draw call per variant and primitive at levels 0–1
(zoom 2 is mostly there: 438 draws); a far level across materials (an atlas) would
make a soldier one draw.

**Gore at the far levels and fewer look draws (m2-lod4).** Filmed on the blank field per
forced level (`ACR_LOD=0..3`, `ACR-analysis/lod4/gore.py`; grids `base_close.png` before,
`new_close.png` and `new_decay.png` after; rows: torn in two by a cannonball, legs off by a
cannonball, musket wound, sabre cut on the arm (alive), sabre to the head, crawler; columns:
the levels): from the welded far meshes a cut man's waistcoat showed stripes along the
discarded line and the crawler's coat a white patch of shirt through it (level 2–3), since a
welded triangle may join a kept part to a discarded one or take a corner from another
part's bones. A damaged instance (`Damage` not none: cuts, fractures, a torn trunk, wound
volumes, severed pieces) now never draws coarser than level 1 (`lod::DAMAGED_UP_TO`, also
for its shadow); the per-primitive level-1 lists keep the part borders, and these frames
match level 0. Whole corpses (decay masks, deflate, the inner bone layer) and living men
look right from the far meshes. The cost is nil in a battle: few instances are damaged.

The look variants: a run of draws that differ only in the variant (same model, level,
decay) is split (`split_variants`): the primitives no variant touches are one draw across
the whole run, only the touched ones stay a draw per variant. Monongahela `battle3`, zoom 2:
438 → 288 draws; GPU time unchanged (`gputime.py`, two runs each: z1 22.9–23.0 → 23.1 ms,
z2 36.5–36.6 → 36.1 ms), so per-draw cost is not what limits this tiler here; an atlas
across materials was not tried. A slow zoom sweep through the thresholds (`zoomsweep.py`,
0.5 → 2) shows no visible pop at battle sizes (`pop_l3_l2.png`, `pop_l2_l1.png`); the
hysteresis stays at 15 %.

**Crashes.** A lost GPU device and uncaught wgpu errors are logged (`acr-app`, `Gpu::new`),
as are the quit and the end of the event loop; `gputime.py` in `ACR-analysis/lod3/` keeps
the driver's stderr (where a stack overflow is reported) and prints the exit code on an
early end. Twelve runs (lod on, off, shadows off; main and this branch) did not reproduce
the silent exit.

### Authored levels (m2-hd-lod-render)

High-detail masters bring their own levels, made offline in Blender (`acr_assets.hdbody`
`build_lods`: seam-locked decimation, normal and albedo baked from the master per level), and
the renderer draws those instead of index lists of its own.

- **Format:** one glb. A level is a mesh node with `extras.acr_lod` = 1, 2, 3 (or a node
  name ending `.lod1` … `.lod3`) and `extras.acr_lod_of` = the name of the master node it
  replaces (else the name before `.lod<k>`). Each level has its own material and textures
  and is skinned to the master's skin, so the skeleton, the clips and the palettes are
  shared. Nodes neither a level nor a master (the eyes, gear) draw at every level with the
  index levels of `lod.rs`. `Primitive::lod` holds the role; `lod::authored` gives a level's
  lists (a missing level draws the next finer one); the far meshes are not built for such a
  model. A model without levels loads as before. `hdman.py` writes its chain so (the
  sidecar `hdman.lods.glb` is gone).
- **Choice:** `lod::AUTHORED_SIZES` (2 × culling radius in px; about 2 × a man's height):
  600, 240, 80 px, i.e. the master while a man is over about 300 px tall (zoom 5.5), 10 k
  down to 120 px (zoom 2.2), 3 k down to 40 px, the crowd level below 40 px (zoom 0.75; at
  zoom 1 a man is 54 px). The 15 % hysteresis of `lod::choose` applies. Levels from 2 take
  `fs_far`. Shadows cast one level coarser; from the crowd level a "sloppy" list of
  `lod::AUTHORED_SHADOW` (400) triangles.
- **Cross-fade:** not built. A WebGL2-safe one would draw both levels for about 0.3 s with
  complementary screen-door masks (`discard` by a 4 × 4 Bayer threshold against a per-instance
  fade in the instance texels), so the pair goes through `fs_main`, not `fs_clean`; with the
  hysteresis no pop was visible at battle sizes (above), so it waits for a close-up need.
- **Impostors (design note):** below about 15 px a man is a few dozen pixels; an octahedral
  impostor atlas per type and clip frame (8 × 8 views of the crowd level, baked in Blender or
  at load into one texture) drawn as instanced quads would replace the 800 triangles by 2.
  Clip frames multiply the atlas (STAND, MOTION_L, ATTACK: about 60 frames), so it pays
  only for the original's zoom-out views of big battles.
- **Test aid:** `ACR_LOD_TINT=1` (native) tints each authored level's materials (master
  white, 10 k green, 3 k yellow, crowd red); `ACR_LOD=<k>` forces a level as before.
- **Texture memory:** see "Texture streaming" below: the finer levels' pictures load when
  first drawn. `hdman.glb` (35 MB file) carries 245 MB of pixels (201 MB the master's
  three 4 k maps, then 2 k, 1 k and 512 for the levels); 2.8 MB of them load with the model.

Measured on the render VM (RX 9060 XT, 1600 × 900, shadows on), blank field, grass, HUD
off, 1,024 EngM18 in a 32 × 32 block, one owner, after 20 ticks; CPU `profile` over 50
frames, GPU `gputime` 30 frames, two runs each (scripts, frames and logs in
`ACR-analysis/genai/hd-lod-render/`, `steps/crowd.json`):

| ms per frame | old body z1 | HD z1 (level 2) | HD z1 forced crowd | old z2 | HD z2 (level 2) |
|---|---:|---:|---:|---:|---:|
| GPU total | 1.68 | 3.64 | 3.00 | 2.60 | 3.63 |
| models pass | 0.78 | 2.78 | 2.16 | 1.82 | 2.84 |
| shadow map | 0.25 | 0.19 (1.09 with the crowd mesh as caster) | 0.19 | 0.24 | 0.19 |
| CPU `3d.update` | 2.0–2.8 | 1.8–2.4 | 2.1 | 2.0–2.7 | 2.0–2.3 |

The CPU side does not change (one palette per instance either way). The models pass is
2–3 × the old body's because the old body's far mesh at these sizes is about 700 triangles
and the HD body's "800" level is 2,470 (the decimation stops short; `hdbody.LODS`); the 3 k
level is 3,000. Proof sheets: `sheet-proof-hd-tint.png` (zoom 0.5, 1, 2, 4 and a free
close-up: red, yellow, yellow, green, white), `sheet-proof-hd.png`, `sheet-proof-old.png`.

### Texture streaming

Model textures carry mips, and the pictures of the finer authored levels load only when a
model first draws at that level (`acr-render/src/tex_cache.rs`, m2-tex-stream).

- **Which pictures wait:** `model::material_levels` gives each material its authored level
  when only primitives of that level, finer than the coarsest the glb carries, use it. A
  picture all of whose materials have such a level stays encoded at load
  (`ModelData::encoded`; its `Image` has the size from the PNG or JPEG header and no
  pixels). Everything else (the coarsest level, primitives without a level, models without
  authored levels) is decoded at load as before.
- **Request:** `group_draws` calls `want_textures(model, level)` for each authored model and
  level wanted or shown this frame; `TexCache::want` marks a resident picture used or queues
  a `Decoder` job. Shadows ask for none (they draw at a level whose pictures are in).
- **Decode:** native: one `acr-texture` thread per picture (PNG or JPEG to RGBA). wasm32: the
  browser's `createImageBitmap` on a `Blob` of the encoded bytes (`tex_cache::browser_decode`,
  web-sys, async via `wasm-bindgen-futures`), which decodes off the main thread; options
  `premultiplyAlpha: none`, `colorSpaceConversion: none`, so normal and roughness maps keep
  their values. The bitmap is then cut into strips of `strip_rows` rows (cropped bitmaps,
  also async). Without a window the old path stays: one picture per frame on the main thread.
- **Upload:** level 0 in strips of `UPLOAD_PER_FRAME` bytes (8 MB native, 2 MB wasm):
  `write_texture` rows (native) or one `copy_external_image_to_texture` per bitmap strip
  (wasm; wgpu's WebGL2 backend runs it as `texSubImage2D` from the `ImageBitmap`, but allows
  no source origin there, hence the cropped strips). Then the mips on the GPU, then the
  texture binds (a 4 k map with mips written at once stalled one frame by 190 ms on the
  RX 9060 XT).
- **Stand-in level:** a mesh only ever samples its own material's pictures (`GpuModel::group`):
  each authored level has its own UV layout and atlases (`hdbody.build_lods`), so another
  level's picture paints wrong or black patches (the former material stand-in did, on HD
  faces and hands, m2-standin-uv). Instead `group_draws` draws a model whose wanted level is
  not `level_ready` (a material of it lacks a streamed picture) at the next coarser level
  that is (`shown_level`): its own mesh, UVs and pictures, down to the coarsest, which is
  always loaded. It asks for the pictures of the wanted level and of every level between it
  and the one shown, so a close-up goes coarsest, then level 1 (small maps, decoded first),
  then the master. Shadows step the same way. A split body never draws coarser than
  `DAMAGED_UP_TO`; at that level a missing picture samples white (never another level's).
  Test: `model_renderer::tests::draws_never_take_another_levels_textures`.
- **Time to full detail:** a 4 k PNG takes about 0.2–0.3 s to decode on its thread (the three
  master maps in parallel), then 11 frames of upload. The "6,000 frames" seen in the driver
  were a `wait` of 200 held ticks, which runs frames at about 0.04 ms without vsync: the
  maps were in after a fraction of a second of wall time (numbers below).
- **Eviction:** `TexCache::evict` each frame drops pictures unused for `IDLE` (1,800) frames,
  and, while the streamed pictures exceed `BUDGET` (512 MB native, 192 MB wasm), the least
  recently used ones unused for `GRACE` (120) frames. The model then draws at the stand-in
  level; a later draw asks again. Tests: `tex_cache::tests`.
- **Mips:** made on the GPU for every model texture, loaded or streamed, except
  nearest-filtered ones (projected sprite colours): `mipgen::MipGen`, one render pass per
  level that samples the level above (a one-level view; the GL backend sets
  `TEXTURE_BASE_LEVEL`/`MAX_LEVEL`) with a linear filter at the centre of each 2 × 2 block,
  down to 1 × 1 (`mipgen::mip_levels`). sRGB textures average in linear light (decode on
  sampling, encode on writing). Model textures carry `RENDER_ATTACHMENT` for it. The linear
  sampler filters between mips. +33 % memory per picture. The pictures match the former CPU
  chain within 2 of 255 per channel (screenshots below).
- **Compressed formats (built, #297; the design note as written before):** BC7 (desktop) or ASTC (mobile) via KTX2 would cut a
  4 k map from 85 MB (with mips) to 21 MB. WebGL2 exposes them only as extensions
  (`EXT_texture_compression_bptc`, `WEBGL_compressed_texture_astc`), so the glb would need
  both plus an RGBA fallback, or Basis Universal (`KHR_texture_basisu`, UASTC transcoded at
  load to whatever the device has: wgpu `Features::TEXTURE_COMPRESSION_BC/ASTC`). It needs
  a transcoder crate that builds for wasm, a `KHR_texture_basisu` reader in `model.rs` and a
  `toktx`/`basisu` step in `gltf_pack.py`; mips would come from the file.
  - WebGL2 support: Chrome and Edge on desktop expose `EXT_texture_compression_bptc` (BC7)
    and S3TC (BC1–3); Android and iOS expose ETC2 (core in WebGL2 on mobile GPUs, often
    emulated and slow on desktop ANGLE) and ASTC. wgpu's GL backend reports
    `TEXTURE_COMPRESSION_BC`, `_ETC2`, `_ASTC` from those extensions; the device must ask for
    the feature, so it needs a probe at adapter creation and a fallback.
  - Sizes for one 4 k map with mips: RGBA8 85 MB on the GPU; BC7, ASTC 4 × 4 or ETC2 RGBA
    21 MB; ETC2 RGB or BC1 11 MB. On disk UASTC with RDO and zstd about 8–12 MB (the JPEG is
    about 3–5 MB), ETC1S about 1–2 MB at visibly lower quality.
  - Costs (estimates, not measured): transcoding UASTC to BC7 is a fast block remap, tens of
    ms per 4 k map in wasm on the main thread, so it would want strips or a Web Worker too;
    the usual transcoder is Binomial's C++ (the `basis-universal` crate wraps it and needs a
    C++ toolchain for `wasm32-unknown-unknown`); the KTX2 container itself is easy (`ktx2`
    crate). No mip generation, no decode, 4 × less upload and
    memory. Normal maps need BC5/EAC RG for quality.

Measured on the render VM (RX 9060 XT, 1600 × 900, shadows on), blank field, grass, HUD
off, 1,000 HD men (`hdman.glb` as `EngM18`) in a 32 × 32 block; the camera at zoom 1, then a
free close-up at zoom 9 (master level), then back to zoom 1; `frametimes` (each frame waited
for on the GPU) and `textures`. Steps `ACR-analysis/genai/tex-stream/steps/zoom.json`; main
with the driver requests only (`m2-tex-stream-base`) in `main-1`, the branch in
`branch2-1`, `branch2-2`; sheets `sheet-close.png`, `sheet-z1.png`.

| | main (2095bac) | streaming |
|---|---:|---:|
| HD body's textures at start (zoom 1) | 245.4 MB (no mips) | 2.8 MB loaded + 11.2 MB level 2 = 14.0 MB |
| after the close-up (240 frames) | 245.4 MB | 2.8 + 279.6 MB (master 3 × 89.5 MB with mips) |
| back at zoom 1, 1,900 frames later | 245.4 MB | 2.8 + 11.2 MB (master dropped) |
| `engm18.glb` parse at load | 398 ms | 10 ms |
| zoom-in, 240 frames: mean / max ms | 20.3 / 34.0 | 19.5–19.7 / 32.7 |
| back at zoom 1, 60 frames: mean / max ms | 6.0 / 21.9 | 3.3 / 6.3 |
| zoom 1, 1,900 frames: mean / max ms | 6.0 / 20.6 | 4.6 / 5.3 |

Off-thread decode and GPU mips (m2-wasm-decode), same steps and machine, runs one after
the other (`ACR-analysis/genai/wasm-decode/`, `main-2/3`, `branch-2/3`, steps `zoom.json`,
`--models hd-lod-render/models-hd`):

| ms per frame, mean / max | main (f106991) | branch |
|---|---:|---:|
| zoom 1, 60 frames | 7.1 / 114, 6.0 / 103 | 7.1 / 118, 6.0 / 97 |
| zoom-in, 240 frames (masters stream in) | 20.8 / 33.6, 22.9 / 55.1 | 21.1 / 57.0, 19.7 / 35.4 |
| back at zoom 1, 60 frames | 6.1 / 10.7, 4.9 / 7.1 | 5.7 / 10.4, 4.8 / 6.5 |
| zoom 1, 1,900 frames | 5.7 / 13.9, 4.5 / 23.5 | 5.6 / 18.9, 5.0 / 5.8 |

Native does not change measurably: the decode was already on threads, and the close-up's
frames are the 38 M triangles' (the maxima move between runs either way). Screenshots of
both differ by at most 2 of 255 per channel (`sheet-mips.png`). The point is the browser:
there the old path decoded a 4 k JPEG (zune-jpeg in wasm) and built its mips on the main
thread in one frame, an estimated 300–500 ms per master map; now that frame runs only a
2 MB strip copy, and the picture's last frame its mip passes (13 small draws). No browser
runs on the render VM (no Chromium, Node or `wasm-bindgen` CLI), so this is not measured
in a browser yet.

The three master maps arrive within 1–2 s of the zoom (decode on threads, 8 MB a frame);
before that the master mesh draws with the 10 k level's pictures. The first frame of each
run (about 100 ms, both builds) is pipeline warm-up.

## Where a battle's frame goes (m2-perf)

The user's report: a big battle turns "almost unplayable (mouse lag)" once the formations
clash, not before. **Profiler:** `acr_sim::prof` (named scopes, native only, no effect on
the simulation's state): `ACR_PROFILE=1` logs every 5 s the frame rate and ms per frame per
scope (`frame.advance`, `frame.acquire`, `frame.render_cpu`, `frame.present`, `sim.*`,
`3d.*`, `rag.*`, `render.models_prepare`; `ticks`, `rag.steps(count)` and
`rag.awake(count)` are counts shown as "ms"); the driver's `{"cmd":"profile"}` returns the
totals since its last call. The game also times the scopes always, for the performance
recorder (1 Hz `perf:` log line, Ctrl + Space, R snapshots; [testing.md](testing.md#performance-recorder)). **Scene:** `ACR-analysis/perf/clash.py`: 700 against 700
musketeers (melee only) meet on the blank field, hit locations, body spacing, gore full,
models all, 1600 × 900, zoom 1; measured before contact, at contact (50 ticks) and 30 s
later (50 ticks). CPU per driver frame (one tick, 0.1 s of game time, so the ragdolls run
their 4 steps), GPU from `gputime` (10 frames). Snapdragon X; runs in
`ACR-analysis/perf/clash_base2` (main) and `clash_new` (m2-perf).

| ms per frame | before contact | contact main → m2-perf | +30 s main | +30 s m2-perf |
|---|---:|---:|---:|---:|
| sim tick (`sim.step`) | 1.3–2.3 | 3.2 → 2.6 | 3.1 | 2.5 |
| · units (motion, paths) / spacing / victim search | 0.4 / 1.3 / 0.5 | 1.5 / 1.1 / 0.6 | 1.6 / 0.8 / 0.7 | |
| 3D update (`3d.update`) | 2.5–3.5 | 23.6 → 9.1 | **52.2** | **18.7** |
| · ragdoll physics (`rag.physics`) | 0 | 18.9 → 5.5 | **43.1** | **12.5** |
| · awake ragdolls | 0 | 34 → 12 | 56 | 12 |
| · instances / debris and decals / piles | 1.6–2.0 / 0 / 0 | 2.5 / 0.4 / 0 | 4.5 / 1.4 / 0.2 | 3.3 / 1.2 / 0.2 |
| · cloth / gear / gaits / strikes | 0.02 / 0.14 / 0.3 / 0.4 | | 0.16 / 0.18 / 0.2 / 1.0 | 0.11 / 0.16 / 0.1 / 0.7 |
| render CPU (of it `models_prepare`) | 3.6–6.1 (1.7–2.1) | 10.6 → 5.0 | 9.2 (5.7) | 7.7 (4.9) |
| GPU models | 21–25 | 36 → 29 | **105** | **61** |
| GPU shadow map | 31–37 | 48 → 43 | 87 | 64 |
| GPU all passes | 54–65 | 87 → 74 | **196** | **128** |

Ticks per frame never exceed the clock's cap of 4, and the simulation is not the problem
(at most 3.2 ms a tick: body spacing, combat and paths together). The clash triggered two
costs that grow with the fight:

1. **Ragdoll physics** (CPU): the budget (60 awake on this machine) came from a
   calibration of 66 µs per ragdoll step on flat ground; in the pile a step cost about
   260 µs per ragdoll, and a slow frame steps up to 4 times, so physics took 43 ms per
   frame and made the frames slower still. Now the budget follows the measured cost
   (`docs/body-sim.md`, Budget): 12 awake in this scene, 12.5 ms. The others keep their
   death clips (as beyond the budget before); those nearest the window centre get
   ragdolls first.
2. **Shot corpses drawn whole** (GPU): every instance with any `Damage` was held at
   level 1 or finer (`lod::DAMAGED_UP_TO`), and under gore full every man shot or stabbed
   carries wound volumes, so the battlefield's corpses and wounded drew their full
   primitives. Wound volumes discard by the rest position (`holes` in model.wgsl), which
   the far meshes keep; only cuts, fractures, a torn trunk, an open belly or a severed
   piece need the part borders. The drawn list now caps only those
   (`Damage::splits_parts`, the entry's `split`); the shadow casters' list is unchanged
   (m2-shadowfix's). Models pass 105 → 61 ms; the frames of both runs look alike
   (`ACR-analysis/perf/after30s_base_vs_new.png`, top main, bottom m2-perf). The shadow
   map's drop (87 → 64) is mostly this tiler's run-to-run shift between passes (the
   unchanged "before contact" frames moved 61 → 54 ms too).

### Ragdolls without the cap (m2-perf2)

The cap of 12 awake ragdolls above was a quality trade. m2-perf2 removes its cause
(`docs/body-sim.md`, Bench): frozen corpses are static collision geometry instead of
bodies woken by every man falling onto them, a step visits only awake bodies, and islands
of awake ragdolls step in parallel (rayon, native). Same clash scene, `clash_base` (main
928a909) against `clash_new2` (m2-perf2 rebased on 9030897) in `ACR-analysis/perf2/`; a
driver frame is one tick, so 4 physics steps (the worst case of the clock):

| ms per frame | contact main → m2-perf2 | +30 s main → m2-perf2 |
|---|---:|---:|
| ragdoll physics (`rag.physics`) | 6.7 → 3.5 | **16.8 → 4.0** |
| awake ragdolls (mean) | 11.7 → 18.2 | **12.0 → 27.6** |
| µs per awake ragdoll step | 143 → 48 | 350 → 36 |
| 3D update (`3d.update`) | 11.1 → 7.0 | 25.4 → 10.5 |
| instances (`3d.instances`) | 2.4 → 2.0 | 4.6 → 3.5 |
| render CPU (of it `models_prepare`) | 6.6 (3.4) → 4.9 (2.7) | 9.0 (5.6) → 9.4 (5.3) |
| GPU all passes (main 9030897 → m2-perf2) | 53.5 → 55.3 | 98.9 → 109.3 |

With 4 steps per frame the adaptive rule lets 28 ragdolls be awake in 4 ms (2.3× more
than the cap, in a quarter of the time); at 2 steps per frame (30 fps and faster) it is
57, at 1 step the calibration's full 82. The GPU is unchanged by this work (the models
pass varies 67–78 ms between runs at +30 s; more of the dead now lie in ragdoll poses,
each its own palette instead of the shared `DEATHLIE` pose). Frames side by side:
`ACR-analysis/perf2/after30s_base_vs_new.png` (top main, bottom m2-perf2).

Next levers: the GPU models pass at +30 s (≈ 70 ms against 30 before contact) grows with
the dead and wounded, not with skinning alone: profile it per entry kind (decaying
layers, wound volumes that `discard` and so lose early depth on the tiler, caps and
inserts) before baking corpse meshes; a static-pose pipeline (palette written once per
frozen corpse into a persistent region of the bone texture, no per-frame upload) and
cached shadow contributions of static corpses (m2-shadowfix's files); incremental
instance building for corpses (`3d.instances`, `models_prepare` 3.5 + 5.3 ms at +30 s).

### The dead in the models pass (m2-perf3)

**By kind.** `ACR_MODEL_DEBUG=<list>` (native, or `ACR_MODEL_DEBUG_FILE=<path>` re-read every
frame to switch while running) leaves out or simplifies kinds of entries: `living`, `dead`,
`dead_volumes`, `dead_plain`, `inner`, `caps`, `gore`, `holes`, `looks`; `old_inner` and
`no_clean` restore the state before this work for A/B runs in one process
(`ACR-analysis/perf3/ab.py`: the clash scene, each configuration 3–4 times interleaved,
10 frames each; the GPU is shared with other sessions, so the minimum counts). At +30 s
(models pass 74 ms, minimum of 3):

| left out | models ms | the kind costs |
|---|---:|---:|
| nothing | 74.4 | |
| the living | 38.1 | ≈ 36 |
| corpses with wound volumes | 34.7 | **≈ 40** |
| corpses without volumes | 70.1 | ≈ 4 |
| caps and wound inserts | 69.0 | ≈ 5 |
| blood, wounds, marks, spray (colour only) | 68.2 | ≈ 6 |
| the volumes' `holes` test (still the `discard` pipeline) | 66.5 | ≈ 8 |
| look variants | 64.5 | ≈ 10 |

Under `gore=full` nearly every corpse carries wound volumes, so "the dead" are the wounded
dead: they cost as much as all the living together. Their price is the `discard`
pipeline (no early depth test on the tiler, `holes` evaluated for every fragment) and
the inner layers, not the colour branches.

**No `discard` where nothing drops (`fs_clean`, `fs_far_clean`, `fs_shadow_clean`).**
`shade` takes a constant `clean`; its nine `discard`s are `if (!clean)`, which the compiler
folds away. A part draws through the clean pipelines when the instance is intact
(`Damage::is_none`), not half transparent and not dissolving (decay past the first dissolve
window of any of its layers, `gore=full`), and its material has no alpha cut-out, is no
stump cap and no wound cavity (`GpuModel::discards`). Clean parts draw first (near, then
far), then the rest with the old pipelines; within each draw the instances go front to
back. Shadow casters likewise: the clean ones with an empty fragment shader (depth only).
The colour is unchanged: the same `shade`, and a clean entry's `discard`s could not fire.

**Inner layers only when they can show.** The bone layer (a third of a man's triangles)
was drawn for every corpse from its first tick of decay. Now, for a body with a rig, only
once an outer layer dissolves (`GpuModel::opens_at`, the flesh at 0.20 under
`gore=full`; never under `gore=reduced`) or when it is damaged (a wound or cut may show
it). Gibs and pieces without a rig keep theirs from the start (a bone chunk's bone is
outside), and animals (a guessed rig) are unchanged.

| GPU ms (min / median of 4; +30 s: of 3) | before contact | contact | +30 s |
|---|---:|---:|---:|
| models main → m2-perf3 | 32.1 / 33.6 → **20.4 / 23.3** | 41.0 / 41.9 → **27.4 / 29.2** | 83.2 / 94.6 → **74.4 / 87.9** |
| shadow map | 3.5 / 3.8 → 1.5 / 3.1 | 10.8 / 13.2 → 6.9 / 7.2 | 29.7 / 35.3 → 28.8 / 32.4 |
| all passes | 52.9 / 57.7 → **35.4 / 37.7** | 59.2 / 66.1 → **46.9 / 48.7** | 123.5 / 146.2 → **109.5 / 127.2** |
| `models_prepare` CPU | 2.3 → 2.2 | 4.5 → 3.5 | 7.6 → 6.4 |

(`ACR-analysis/perf3/ab2.log`; an earlier run while another game shared the GPU,
`ab1.log`, gave 2–3 × the times with the same direction.) The gain is largest while most
men are alive and intact; at +30 s the wounded dead still draw with `discard`. Nothing
changes visibly: the same frame paused, main against m2-perf3, differs only where the
awake ragdolls move between two frames, as much as main against main
(`ACR-analysis/perf3/closeup_main_new.png`: top main and m2-perf3 at zoom 4, below the
pixels differing by more than 8 between two main frames and between main and m2-perf3).
A first try that hid the inner layers of every intact corpse turned the gibs' bones flesh
coloured, hence the rig test.

Next levers for the dead: split a wounded corpse's parts by whether a volume can reach
them (the capsules in the rest pose against each primitive's rest bounds), so only the
parts around the wounds keep `discard`; that is ≈ 40 ms of the 74 at +30 s. Then a static
pose for settled corpses (palette written once into a persistent region of the bone
texture), cached shadows of static corpses, and incremental instance building.

### Ragdoll steps off the render thread (m2-rag-perf)

On native the physics steps that fall due in a frame run on a worker thread
(`Ragdolls::dispatch` at the end of `ThreeD::update`, `Ragdolls::join` at the start of the
next): they overlap the render's CPU work, the GPU wait and the next ticks of the
simulation. The worker samples the ground from `HeightField` (acr-render `terrain.rs`: the
map's heights shared behind an `Arc`, the same `GetHeight` port). A frame's palettes
therefore show the physics as the previous frame's steps left it, one frame (at most 4
steps, 67 ms) behind; hits, the drive of the muscles and the settling still run on the
render thread before the steps, in the same order as before apart from the settling
moving ahead of them. Ragdolls are render only, so no hash is touched; the steps
themselves are unchanged (deterministic per island). The budget follows the worker's
measured time as before (4000 µs of physics per frame). `ACR_RAG_INLINE=1` steps on the
render thread as before (the browser always does). Profiler: `rag.worker` (the worker's
time per frame), `rag.join` (the wait for it at the next frame).

Clash scene (`ACR-analysis/rag-perf/clash.json`, `mkclash.py` of perf5: 700 against 700,
`ACR_PROFILE=1`, driver frame = one tick = 4 steps), one build, `ACR_RAG_INLINE=1` against
threaded, two runs each, render VM (Ryzen, RX 9060 XT, loaded by other builds; runs
`inl2`/`inl3`, `thr2`/`thr3`):

| ms per frame (render thread) | contact inline → threaded | +30 s inline → threaded |
|---|---:|---:|
| ragdoll physics on the render thread (`rag.physics` → `rag.join`) | 2.5–3.0 → 0.0–1.2 | **4.1 → 0.4–0.5** |
| worker (`rag.worker`) | | 4.0 |
| 3D update (`3d.update`) | 6.1–7.3 → 3.8 (8.0 in a loaded run) | **9.6 → 6.3–6.4** |
| awake ragdolls (mean) | 23–24 → 24 | 30–34 → 26–28 |
| µs per awake ragdoll step | 26–32 | 31–37 |
| render CPU / GPU all passes | 3.2 / 2.5 → 3.1 / 2.6 | 4.8 / 3.8 → 5.0 / 3.7 |

The calibration on the VM gives 18–23 µs per step (budget at the pool's 192); in the pile a
step costs about 2 × that, so the 4 ms let 26–34 ragdolls be awake at 4 steps per frame. With
the steps on a worker that 4 ms no longer adds to the frame unless the rest of the frame is
shorter. The piles look alike (`ACR-analysis/rag-perf/close_inline_vs_thread.png`: the
same moment 30 s into the clash at zoom 4, inline left, threaded right; the falls differ by
the hits taken, as between two inline runs).

### Parallel loops and view culling (m2-par-loops)

**View cull.** `ThreeD::update` leaves out the plain living well off this frame's view
(`off_view`: the ground point projected half a window plus 200 screen px × zoom beyond the
edges, or behind the camera): alive, not crawling, not dropping his gear, no gun or ship,
no studio pose held. Such a man still counts as shown (no sprite at the edge), his crawl
state is cleared and his blood trail goes on; corpses, buildings, crews and the wounded
crawlers are always built, so piles, ragdolls, dropped gear, pools, stains and kept marks
are unchanged. The view is the current frame's (built before `update`), so a jump across
the map shows its men in the first frame. `ACR_CULL=off` (native) builds every instance.

**Parallel loops** (`acr_render::par::map`: rayon on native from 64 items, in order on one
thread in wasm; the results keep the items' order). `models_prepare`: the looks, each
instance's culling, instance texels and entry (`Staged`), the shared poses (collected in
first-wanted order, then sampled), the caps and wound inserts with their entries; only the
palette and data layout stays sequential, so the bone texture, instance buffer and draws
are the same as before. `3d.instances`: each body's damage and torn tissue
(`body_damage`, the largest per-unit cost) before the loop, taken by unit inside it.

Render VM (12 cores, RX 9060 XT), release, ms per frame (`3d.instances` per tick,
`models_prepare` per rendered frame), main e19129b against m2-par-loops, 2 runs each
(`ACR-analysis/par-loops/`, `cmp.py`):

| scene, phase | `3d.instances` | `models_prepare` | `3d.update` |
|---|---:|---:|---:|
| clash (`clash.json`), before contact | 2.05 → 1.37 | 1.38 → 1.05 | 2.6 → 2.0 |
| clash, contact | 2.32 → 1.46 | 1.84 → 1.53 | 5.7 → 4.7 |
| clash, +30 s | 3.37 → 1.88 | 2.84 → 2.45 | 8.9 → 7.6 |
| mission #SINGLE8 (`mission.json`), before | 12.2 → 3.9 (no cull 6.3) | 6.8 → 2.0 (2.2) | 14.4 → 5.0 |
| mission, fight | 10.7 → 2.5 (6.2) | 4.8 → 1.3 (2.5) | 19.5 → 9.6 |
| mission, fight +30 s | 11.6 → 2.9 (6.3) | 3.9 → 1.5 (2.5) | 21.6 → 11.1 |

(Mission main from `perf5/mis_main1`, one run; "no cull" is m2-par-loops with
`ACR_CULL=off`.) The GPU is unchanged. Frames: the clash before contact and the mission
before the fight and after a camera jump are pixel-identical to main or no-cull; in the
fight they differ only where the awake ragdolls differ between two runs of the same build
(clash +30 s: 11977 pixels differing by more than 8, main against main 11939;
`sheet_clash.png`, `sheet_mission.png`). The user's Snapdragon X has 12 cores (X Elite) or
8–10 (X Plus): the parallel share should scale alike; the cull does not depend on cores.
What stays sequential in `models_prepare`: `group_draws` (sorts) and the uploads, about
1 ms of the 2.5 at +30 s.

### Debris, decals and the body spacing (m2-perf6)

- **Decals and debris off the view:** with the view cull above, the spray, mist and churned
  earth of a hit or impact well off the view (`off_view_at`, the same margin) are not
  made, the decals still left there are dropped before they are laid on the ground, and a
  piece, gib or dropped gear whose start and landing both lie well off the view is not
  built (`Debris::draw`, `shown`). Nothing but the draw reads them; the kept marks and
  decay stains keep their own state and are untouched.
- **Laying decals** (`splatter::lay_on_ground_par`: the height and slope under each decal)
  runs on `acr_render::par::map`.
- **Body spacing** (sim, `sim.spacing`): a cell table instead of a lookup per cell, same
  pairs and pushes (`docs/body-spacing.md` "Cost").
- New profiler scopes: `3d.debris_draw`, `3d.debris_hits`, `3d.splatter`, `3d.lay_decals`,
  `3d.keep_marks`, `3d.crew_corpses` (inside `3d.debris_decals`), `sim.spacing_pairs`.

Render VM, release, ms per tick, main 1878609 (`perf6/mis_main1`, `par-loops/mis_rebased`)
against m2-perf6 (`mis_new2`, `mis_new3`), clash 1 + 2 runs (`ACR-analysis/perf6/`,
`sc.py`):

| scene, phase | `sim.spacing` | `sim.step` | `3d.debris_decals` |
|---|---:|---:|---:|
| mission, before | 2.4–3.2 → 1.4–1.8 | 3.6–5.5 → 2.7–4.2 | 0.1–0.4 → 0.2–0.3 |
| mission, fight | 2.2–2.3 → 1.2–1.3 | 4.4–4.8 → 3.5–3.7 | 1.3–1.4 → 1.0–1.1 |
| mission, fight +30 s | 2.1 → 1.2 | 4.5–4.7 → 3.9–4.1 | 1.8 → 1.4–1.5 |
| clash, contact | 0.8 → 0.45–0.5 | 2.0–2.4 → 1.8–2.1 | 0.2–0.3 → 0.3–0.4 |
| clash, +30 s | 0.7–0.8 → 0.45 | 2.0–2.4 → 1.9–2.0 | 0.7–0.8 → 0.8–1.0 |

The clash keeps everything in view, so its decals gain nothing (the parallel laying is
within the noise). The mission's state answers (counts, impacts, population at ticks 331,
2305, 2607) are equal to main's; its frames before the fight, at +30 s and after the camera
jump and back are pixel-identical to main in one of the two runs (the other differs in
56 pixels where two runs of one build differ; `sheet_mission.png`, `sheet_clash.png`).
What stays in `3d.debris_decals` at +30 s (≈ 1.4 ms): the laying of the decals in view
(0.4), the spray of hits in view (0.2), map objects (0.2), the debris hits (0.2).

### Input latency

- **Hardware cursor** (native, `acr-game/src/hw_cursor.rs`): the cursor the interface
  draws (`CURSOR_00.GP` in the game with its animation and hotspots, `CURSOR_01.GP` in the
  menus) becomes the window's system cursor (winit `CustomCursor`; the frame scaled with
  bilinear filtering as the interface draws it, cached per frame and scale), and the
  interface leaves it out of the frame. The pointer moves at the display's rate however
  slowly the game draws. While a cursor's file loads it is drawn as before;
  `ACR_SOFT_CURSOR` keeps the drawn cursor, and so do the test driver (its screenshots show
  the cursor) and the browser.
- **Frame queue:** the swapchain queues two frames (`desired_maximum_frame_latency` 2;
  `ACR_FRAME_LATENCY` 1–3 overrides; the present mode stays the surface default). One
  frame lost the GPU device within seconds on the Snapdragon X DX12 driver (2026-10-23),
  so the default went back to two; the hardware cursor keeps the pointer responsive. The game
  runs the simulation and builds the 3D frame before it acquires the swapchain image, so
  that work still overlaps the GPU's previous frame; only the command encoding waits.
  With the GPU at 60–130 ms per frame in a big fight, a second queued frame added as much
  again between an order and its sight.
- **Input events** were already handled before each frame (winit's `window_event` only
  records state and queues commands; `about_to_wait` requests the redraw), never behind
  sim ticks.

Latency was judged by design, not measured end to end (no input from the driver reaches the
OS cursor): with the system cursor the pointer is the compositor's, at display rate (a
mouse move to its sight ≈ one display refresh instead of one to three game frames, 60–400 ms
in the clash before); an order's effect shows one game frame sooner. To try it: a big
battle (or the blank field with two large armies), move the mouse over the fighting as
the frame rate drops; `ACR_SOFT_CURSOR=1 ACR_FRAME_LATENCY=2` gives the old behaviour for
comparison, `ACR_PROFILE=1` writes the breakdown to the log.

### A large mission: "Das Französische Heer." (m2-perf5)

The user's report: the French army single mission (`#SINGLE8`, `Missions\Singles\Single8`)
gets "a bit laggy once troops started getting into fights". 4502 French (2220 FrnS17,
1764 FrnM18, 480 FrnD18, guns, officers) against 2046 EngM18 and 28 English buildings; the
3D view builds 9600–10600 instances per frame and draws 520–580 of them. Script
`ACR-analysis/perf5/mission.json` (the menus, then all French ordered to the British town;
the fast troops meet the English around (10000, 4000) from tick ≈ 2000), render VM (RX 9060
XT, Vulkan), showcase settings, 1600 × 900, zoom 1, HUD off; a driver frame is one tick
(4 ragdoll steps). Main e19129b, `ACR-analysis/perf5/mis_main1`; also
`ACR-analysis/perf/mission_profile.md`.

| ms per frame | marching (camera on the army) | fight | fight +30 s |
|---|---:|---:|---:|
| sim tick (`sim.step`; of it spacing) | 4.6 (2.8) | 4.3 (2.1) | 3.6 (2.0) |
| 3D update (`3d.update`) | 14.4 | 19.5 | 21.6 |
| · instances (`3d.instances`) | **12.2** | **10.7** | **11.6** |
| · ragdolls (of it `rag.physics`; awake) | 1.2 (0; 0) | 5.8 (3.7; 37) | 6.2 (3.8; 43) |
| · debris and decals | 0.2 | 2.1 | 2.8 |
| render CPU (of it `models_prepare`) | 10.1 (6.8) | 8.2 (4.8) | 6.6 (3.9) |
| GPU all passes (models / shadow map) | 3.4 (1.7 / 0.9) | 3.4 (1.4 / 1.0) | 3.5 (1.5 / 1.1) |
| GPU all passes, `ACR_LOD=off` | 20.4 / 20.2 | 5.3 / 5.3 | 5.7 / 5.5 |

On this machine the frame is CPU bound (≈ 30 ms); the GPU idles. The clash on the same VM
(`perf5/clash_main1`): `3d.update` 2.6 / 5.6 / 8.9, `3d.instances` 2.1 / 2.3 / 3.4, render
CPU 1.9 / 2.7 / 4.4, GPU 2.1 / 2.6 / 3.9 ms before / at / 30 s after contact. The mission's
cost is the map's unit count, not the fight: `3d.instances` walks every unit of the map
(pools, stains, wounds, clip choice, gear) whether it is in view or not, and
`models_prepare` every built instance. The fight adds ragdolls (≈ 5 ms) and decals (≈ 2.5).
The authored levels and texture streaming are in this run (main); LOD off (all at level 0)
is the row above: the authored levels spare ≈ 17 of 20 ms GPU over the marching army
(and ≈ 2 in the fight). CPU scopes vary by ±25 % between runs (the LOD-off run gave
`3d.instances` 9.0 / 11.0 / 14.3 ms).

### Next levers (measured, not done)

Ranked by the measured share they could spare. VM numbers for the mission; Snapdragon X
numbers (the user's laptop) for the GPU from the clash above.

1. **The sequential rest of the CPU frame** after m2-par-loops (view cull and parallel
   loops, above: mission `3d.instances` 12 → 3–4 ms, `models_prepare` 7 → 2 ms).
   - `models_prepare` is split by the profiler scopes `render.mp_staging` (the parallel
     per-instance work), `render.mp_layout` (palettes and data in order), `render.mp_group_draws`
     and `render.mp_upload` (`ModelRenderer::prepare_ns`; m2-perf7). Render VM under load,
     ms per frame: clash +30 s 2.3 = 1.3 / 0.35 / 0.36 / 0.30; mission #SINGLE8 fight
     +60 s 1.3–1.6 = 0.9–1.2 / 0.13–0.21 / 0.10–0.11 / 0.10–0.13. The sequential part is
     ≈ 1 ms only in the clash (everything in view); in the mission it is ≈ 0.35–0.45 ms.
   - The corpses, which `3d.instances` always builds. A cull of settled corpses well off
     the view (m2-perf7, not landed: `ACR-analysis/perf7/corpse_cull_wip.patch`) kept, per
     culled corpse, its pool, `Seeps::corpse` (seen), kept pool, `unit_models` entry and
     footprint with its rest unused; it culled only corpses with no ragdoll or a frozen one
     (kept by a `Ragdolls::off_view` set so `settle` does not drop it), the gear laid or
     none, not pulverized, not the victim of a hit newer than the ragdolls' `hit_seen`, and
     farther than a blast's reach + 64 px (Chebyshev) from this frame's untaken shell bursts
     and grazes. Note that a corpse keeps `alive` set until the simulation removes it
     (`death` ≥ 1000): test `death > 0`, not `!alive`. Measured on the mission with the
     camera moved from the fight to the British town: 570 of the corpses culled, 1836 → 1266
     instances built, `3d.instances` 3.6–4.3 → 3.3–3.9 ms (noise of the loaded VM is as
     large). Not landed: with the ragdoll steps inline (`ACR_RAG_INLINE=1`) two runs culling
     only the living differ by 2 pixels in the frame over the town, the corpse cull by 151
     (small red specks around the living English there), and the fight +60 s by 726–911
     against 199. Something drawn in view still depends on the corpses off it; the
     re-entry frames themselves stay within the run-to-run noise (206–355 against 299–359).
     Find that dependence first (candidates: the ragdoll budget, `allowed()` from measured
     step costs; the blood sprayed onto `targets`; the splatter decals).
2. **Ragdoll physics in piles** (≈ 4 ms of 6 in the fight at 4 steps, measured on main
   e19129b before m2-rag-perf moved the steps to a worker; see "Ragdoll steps off the
   render thread" above): a step still costs about 2 × the calibration for the contacts
   that really happen in a pile.
3. **Static corpses' pose** (GPU, Snapdragon): the frozen corpses' palettes are already
   reused on the CPU (`frozen_palette`); they are still uploaded and skinned every frame.
   A persistent region of the bone texture per frozen body (written when it changes) and a
   cached shadow layer for static casters (designed in m2-perf4's notes) would spare the
   upload and part of the shadow map; the VM shows no measurable GPU cost (3.5 ms).
4. **Decals and debris** (1.4 ms in the mission fight after m2-perf6, above): what is left
   is in view; the splatter of a hit is rebuilt every frame from all recent hits and could
   be cached once dried.
5. **Sim** (mission ≈ 3.5–4 ms per tick after m2-perf6): `sim.spacing` 1.2,
   `sim.search_victims` 0.8–1.0, `sim.units` 0.6, `sim.projectiles` 0.6–0.8.

Done meanwhile: the shadow casters follow `Damage::splits_parts` like the drawn list
(9030897); `fs_main`'s gore branches already test per-instance values, so flags alone
would gain little.

## Development aids

`ACR_3D_COMPARE` (native only) compares models with their sprites: `overlay` draws the
sprites of units with a model half transparent in front of the model, `toggle` swaps sprites
and models every 2 seconds (pause the game with the Pause key to compare one moment).
`ACR_MODELS_DIR` points the game at a folder of models (above). `ACR_DEBUG_DAMAGE` (native
only) forces damage on every 3D human: a spec of `Damage::parse` (`cut:arm_l:160`,
`frac:leg_l:170:55` (degrees), `trunk:110`, `head`, `belly`, `piece:arm_l`, `upper`, comma
separated) or `demo` (a preset per unit id), overriding the game's damage; severed pieces
stand beside their body. The log reports every 5 s
the frame rate, the models drawn and the poses sampled.

## Limits and open points

- No meshopt/Draco decoding, no WebP textures: export models uncompressed with PNG, JPEG or
  KTX2 (`KHR_texture_basisu`) textures. `KHR_mesh_quantization` is read (integer
  positions, normals and morph targets; assets.md "Export and compression").
- No nation colour mask yet (#162).
- Free camera: water is the surface of docs/water.md (a flat one under `water=original`);
  buildings, trees and effects drawn as sprites keep their one picture from any side (units
  and projectiles turn with the camera, #309).
- Gear dropped at death (#206) avoids the man's own body only, not a horse or a
  neighbouring corpse; strapped gear floats over a deflated body (assets-humans.md,
  insight 6).
- No blending between clips; morph target normals are ignored.
