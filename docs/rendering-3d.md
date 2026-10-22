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
| `models` | `off`, `units`, `all` | `off` | `units`: units with a model are drawn in 3D; `all`: finished buildings too (under construction, burning down and ruined they stay sprites) |
| `camera` | `fixed`, `free` | `fixed` | `free`: the camera turns and tilts (below) |
| `gore` | `full`, `reduced`, `off` | `full` | 3D corpses: `full` decompose (#178), `reduced` discolour, collapse and sink without dissolving, `off` fade like the original (every other pixel once the death counter passes 100) |

Sprites stay the default; with `models=off` and `camera=fixed` the frame is drawn exactly as
before (no depth buffer, no extra pass). There is no menu entry yet.

**Free camera keys** (`camera=free` only): `,` and `.` turn the view, Page Up / Page Down tilt
it (15°–80°), End returns to the original angle. The camera turns about the ground point at
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

Files are requested when a type with a model first appears and only when `models` is not
`off`; a file that fails to load is logged once and the type stays a sprite.

## Architecture

```
acr-render
  math.rs            column-major 4×4 matrices, quaternions
  view3d.rs          View3d: the original projection as a matrix, the free camera (Orbit),
                     ground picking, model placement (ground_point, facing_yaw, model_matrix)
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
  ui/settings.rs     models / camera / gore
  native.rs          ACR_MODELS_DIR
```

### One frame with models or the free camera

1. **Terrain colour** as before (palette ground target, present pass). Under the free camera
   the ground triangles go through the 3D view matrix instead and are depth-tested among
   themselves (hills no longer come in row order); the water tint, which works on screen
   blocks of the original view, is left out.
2. **3D pass** (one render pass, depth + stencil `Depth24PlusStencil8` at output size):
   1. *terrain depth* (`vs_depth`): the same mesh through the same matrix, at the plain ground
      height (the drawn ground is lifted by up to 7 px of random relief) and pushed back by
      24 world pixels, so a model's feet are not cut;
   2. *models*: skinned, instanced, depth test and write; their pixels set stencil 1;
   3. *ground reset*: a full-screen triangle at the far plane where stencil is 0, so the
      sprites draw over the terrain as in the original;
   4. *sprites* in their usual order with a depth test (no writes) against the models.
3. Fog of war and interface as before.

Sprites therefore keep the original painter's order among themselves and with the ground,
and meet the models through the depth buffer: each sprite is a vertical billboard at its
ground row. Ground sprites (`Layer::Ground`, and what the zoomed-out view bakes into its
ground) lie at the far plane, under every model.

### Skinning that fits WebGL2

- Every mesh is skinned to a **palette** of matrices: skinned meshes to their joints
  (global × inverse bind), static meshes (gear parented to socket bones, buildings) to their
  node. One vertex format, one pipeline: position, normal, uv, `COLOR_0`, 4 joints/weights,
  two morph target offsets (13 vertex attributes with the instance data, WebGL2 allows 16).
- Poses are sampled on the CPU (translation/rotation/scale channels, step and linear;
  cubic splines use their key values) **once per distinct (model, clip, frame)**: units in
  the same animation frame share a palette. The palettes go into an `Rgba32Float` texture,
  1024 texels wide, 3 texels (affine rows) per matrix; instances carry their palette's first
  texel. No storage buffers, no uniform arrays.
- Instancing: all instances of one model type are drawn with one `draw_indexed` per
  primitive (13 for `engm18.glb`). The instance buffer is bound from each model's first
  instance (WebGL2 has no base instance); indices are pre-offset (no base vertex).
- No back-face culling: clothes are thin shells seen from both sides once they tatter; the
  normal is flipped towards the viewer for lighting.
- Light: Lambert from the game's light vector `(-15, 14, 30)` (upper left, towards the
  viewer) with half ambient; colours are linear, sRGB is encoded by the target or the shader.

### Animation states (#166)

The model plays the clip named like the unit's current `.MD` animation, at the time of the
unit's sprite frame: `clip time = frame / fps`, `fps` = 24 (clips are keyed on sprite frames
at Blender's default rate; a root node's `extras.acr_fps` overrides it). The clip is found by
name without `#`: exact (`MOTION_L1`), without variant digits (`MOTION_L`), the family's
first (`DEATHLIE1` for `DEATHLIE3`), then `STAND`, then the first clip; a model without clips
shows its bind pose. Facing: `yaw = −(RealDir − 64) · 2π / 256` (models face south);
buildings always face south (one view).

### Decay (#178) and gore (#183)

Per corpse one value from the death counter (`Unit::death`):
`decay = clamp((death − 100) / (removed − 100), 0, 1)`, `removed` = 2000 for types with
`#DEATHLIE2`, else 1200 (the original's removal). From it: the morph weights
`deflate = ramp(decay, 0.18, 0.55)`, `lie_flat = 1` while lying (`#DEATHLIE*`), and a sink of
`ramp(decay, 0.78, 1) · 0.20 m`. The fragment shader applies each material's
`extras.acr_decay` (discolour towards the tint, dissolve by the `COLOR_0` mask
`0.75 R + 0.25 G` with a dark rotten edge) exactly as `docs/assets-humans.md` gives it;
dissolving is alpha-tested (`discard`), nothing is blended or sorted. `gore=reduced` keeps
everything but the dissolve; `gore=off` draws the corpse like the original's half
transparent sprite, every other pixel. Sprite corpses always keep the original fade.

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

About 17 µs of GPU time per drawn musketeer. Poses are cheap (one per distinct clip frame);
the cost is the vertex work, so LODs and fewer triangles per figure are the next lever.

## Development aids

`ACR_3D_COMPARE` (native only) compares models with their sprites: `overlay` draws the
sprites of units with a model half transparent in front of the model, `toggle` swaps sprites
and models every 2 seconds (pause the game with the Pause key to compare one moment).
`ACR_MODELS_DIR` points the game at a folder of models (above). The log reports every 5 s
the frame rate, the models drawn and the poses sampled.

## Limits and open points

- No meshopt/Draco decoding, no KTX2 or JPEG textures: export models uncompressed with PNG
  textures (the pipeline's default).
- No shadows from models; no nation colour mask yet (#162); no LODs.
- Free camera: fog of war, the minimap frame and the water tint follow the original view
  only; sprites (trees, units without models) show their stored direction.
- Dropped gear (musket, hat) does not detach at death yet; strapped gear floats over a
  deflated body (assets-humans.md, insight 6).
- No blending between clips; morph target normals are ignored.
