# Water (M2, #287)

The original has no water surface: `fs_present` (terrain.wgsl) tints the drawn ground by
the `SEA2` depth through `WCOST.GRD`, per 32 × 32 block of the original screen. This
document decides how ACR gets real water in the 3D path, and records what phase 1 built.

Water stays **render only**. Nothing here feeds back into `acr-sim`: ships keep their
simulation positions, the hashes stay unchanged, and replays stay valid.

## Stack constraints

- **wgpu/WGSL**, native (DX12/Vulkan/Metal) and **WebGL2** (wasm). Every shader must
  translate to GLSL ES 3.00 (`crates/acr-render/tests/shaders.rs`).
- **WebGL2 has no compute shaders and no storage buffers.** A simulation on the GPU means
  ping-pong between two textures with a fragment shader.
- **Float render targets** need `EXT_color_buffer_float`. It is on nearly all desktop
  WebGL2 browsers and most recent mobile GPUs, but not guaranteed. wgpu's GL backend
  reports it through the texture format's `RENDER_ATTACHMENT` capability, so check
  `adapter.get_texture_format_features(Rgba16Float)` at start-up. Fallback: `Rgba8Unorm`
  with the height and velocity packed into two channels each (16-bit fixed point). That
  is enough for a damped wave equation.
- The cameras are **orthographic**, both the fixed one and the free one (`view3d.rs`). The
  view vector is therefore one constant per frame. That makes Fresnel and the depth tricks
  below cheap.

## Options considered

| Option | Licence | Fits our stack? | Verdict |
|---|---|---|---|
| wgpu `water` example (`examples/features/src/water`) | MIT OR Apache-2.0 | WGSL, runs on WebGL2. It is a demo of a hex terrain with a reflection and refraction render-to-texture, and a simple sine-wave vertex shader. | Reference for the reflection-pass setup only. The look is too stylised to adopt. |
| `bevy_water` (Neopallium) | MIT OR Apache-2.0 | WGSL, but built on Bevy's material, PBR, depth-prepass and asset systems, none of which we have. Its wave function (summed directional waves with a noise offset) is portable. | Reference. Porting the shader means rewriting all of its bindings, so there is no time to gain over writing our own. |
| Bevy FFT-ocean community crates (Tessendorf) | mostly MIT/Apache (check each crate before use) | Need **compute** for the FFT, so they do not run on WebGL2. A fragment-shader FFT (log₂N ping-pong passes on float targets) is possible but adds ~2·log₂N passes per frame. | No. |
| three.js `Water` / `Water2` (`examples/jsm/objects`) | MIT | GLSL. They scroll normal-map textures and add planar reflection and refraction renders; `Water2` adds a flow map. The look is good, and the flow-map idea suits our rivers. | Reference for the flow map and the Fresnel mix. Do **not** ship its normal-map images (`waternormals.jpg`): make our own. |
| Tessendorf FFT vs Gerstner | — | FFT gives realistic open ocean with thousands of waves. Gerstner gives 4–8 waves analytically, in the vertex and fragment shader, on any GPU, and the CPU can evaluate the same function (ship buoyancy). | Gerstner. Our water is lakes, rivers and coasts seen from 30° above at RTS scale, where FFT detail is not visible. |

**Decision: build our own, use the ready-made code as reference.** No third-party code is
copied, so phase 1 needs no licence entry. If a later phase copies shader code from one of
the MIT/Apache sources above, record it in `assets/LICENSES.md`.

The reasons:
1. None of the candidates fits our renderer as it is. Each one assumes its own engine
   (Bevy materials, three.js `Reflector`), or compute (FFT).
2. The part that matters for the game is the **interaction**: wakes, splashes, ship
   motion, and later debris. No candidate offers that for WebGL2. It is ours to build
   anyway.
3. The surface shading (Gerstner, Schlick-Fresnel, depth absorption, foam) is textbook. It
   is about 200 lines of WGSL, and it reads our own data directly: the `SEA2` depth and
   the sun of `shadow.rs`.

## Design

### Surface (phase 1, built)

- **A grid at the water level `z = 0`** (where the original's tint puts the water) over the
  visible part of the map. `View3d::ground_bounds(±12)` sets the area. Under the free
  camera it is cut to 7000 px around the target, and there are 16–192 cells per side
  (about 6 px per cell at zoom 1). The vertices come from `vertex_index` alone, with no
  buffers.
- **Depth** is the `SEA2` grid as an `R8Unorm` texture with linear filtering: depth units
  = `byte − 127`, sampled at world `(x, y) / 32` like `acr_sim::map` does. Fragments with
  ≤ 0 units are discarded, and the waterline fades in over 5 units.
- **Waves**: 4 Gerstner waves of 260, 150, 90 and 55 px (about 30 px to the metre, with
  the deep-water speed `√(g·k)`), amplitudes 4–0.8 px. They are scaled by depth: 15 % in
  the shallows, full from 40 units. In the fragment shader 4 more ripples (12–31 px)
  shade the surface only. The normal is computed analytically per pixel.
- **Depth colour and absorption**: shallow green-blue to deep blue by √depth. The opacity
  rises from 30 % to 94 %, so the lakebed (the drawn ground and its shadows) shows
  through in the shallows. Ship shadows therefore lie dimmed on the lakebed.
- **Reflection**: Schlick-Fresnel (F0 = 0.02) of a sky gradient, plus the sun's glint
  (`shadow::sun()`, the light the models use). No screen-space reflection yet (see the
  phases).
- **Foam**: a broken band of two noise octaves along the shore (the first 18 depth units),
  and a little on the crests of deep water.
- **Depth test**: the surface is tested against the terrain depth and the models, and does
  not write depth. A hull below the waterline is hidden. Sprites draw over the water as
  they draw over the ground. A trough takes the depth of the point where its view ray
  crosses `z = 0`. Without that, the terrain's depth (pushed back by only 24 px) cuts
  holes into troughs at low free-camera angles.
- **Order in the 3D pass** (rendering-3d.md): terrain depth, models, ground shadows,
  **water**, ground reset, sprites. While the surface is drawn, `TerrainRenderer::
  water_surface` turns the original tint off (`mode.z` in terrain.wgsl).
- **Setting** `water=detailed|original` (default `detailed`; native override `ACR_WATER`):
  - `detailed` with models: the surface replaces the tint, under the fixed and the free
    camera.
  - `original`: the original tint under the fixed camera. Under the free camera, where the
    tint cannot work, a flat surface with no waves, foam or glint is drawn, so water no
    longer vanishes there.
  - Without the 3D path (models off), and in the zoomed-out map view, the tint stays as
    it was.

Rivers: the depth gradient is already in the texture. A flow direction along the river
(perpendicular to the gradient, signed by the downhill direction of the terrain) will
scroll the ripples and foam in phase 3.

### Interaction (phase 2): a height field around the camera

- **Simulation**: a damped wave equation `h'' = c²∇²h − k·h'` on a ping-pong texture pair,
  512² texels over about 2048 × 2048 world px (4 px per texel) and centred on the camera.
  It is re-centred in whole texels, copying the overlap. Storage is `Rgba16Float` (h, h'),
  or `Rgba8Unorm` packed where float targets are missing. There are 1–2 fragment steps per
  frame at a fixed 60 Hz substep. The shoreline (depth ≤ 0) is a reflecting or damping
  boundary, read from the `SEA2` texture.
- **Sources** are splatted as small additive quads into the velocity channel before each
  step. They come from the game state that `ThreeD::update` reads already (render only):
  - ships: a moving pressure blob at the bow and a trough at the stern, scaled by speed.
    The plain wave equation gives a V-shaped wake that is not Kelvin's 19.5° wake, so add
    an analytic **Kelvin wake decal** in the ship's frame (the classic cusp pattern,
    fading over ~600 px) on the surface.
  - cannonballs: when a projectile ends on water (`projectile.rs`, `ground_media ==
    WATER`, the same test the shell burst uses), an impulse ring plus a splash (below).
  - swimmers and wading men (units whose ground is water), at a small amplitude.
  - bodies and debris falling in: an impulse at the entry point. After that,
    `acr-physics` bodies with buoyancy (below) add small sources while they bob.
- **Shading**: the surface adds the height field's height to the Gerstner displacement and
  its gradient to the normal, plus foam where `|∇h|` is large. The field fades out
  towards its border so the edge never shows.

### Ships (phase 1b): buoyancy for the render only

Evaluate the same Gerstner sum on the CPU (`water.rs`, the constants shared with the
shader) at 4–6 hull points per ship, at the bow, stern, port and starboard of the model's
bounds. From those points take:
- **heave**: the mean height;
- **pitch** and **roll**: the slopes, low-passed with a time constant of about 1 s for
  hull inertia.

Phase 2 adds the height field (read back with a 1-frame delay, or kept on the CPU at a low
resolution). The result goes into `view3d::model_matrix_tilted`, which exists already.
The simulation position does not change. Small boats and corpses use the same sampler.

### Debris and bodies afloat (phase 4)

`acr-physics` (XPBD): a buoyancy force per particle below the sampled surface, plus drag.
Debris and corpses that fall into water float, drift with the river flow and sink
slowly. Render only, like the ragdolls.

### Splash particles: a shared interface

The ground particle system (m2-ground, `docs/ground.md`) has not been pushed yet. Both
systems should use one instanced billboard renderer with this small interface, which
whichever lands first implements:

```rust
/// One burst, emitted by game-state observers (render only, never read by acr-sim).
pub struct Burst {
    pub at: [f32; 3],          // world px, z true height
    pub dir: [f32; 3],         // main direction (unit), e.g. up for a splash
    pub spread: f32,           // cone half angle, radians
    pub speed: [f32; 2],       // min/max initial speed, px/s
    pub count: u16,
    pub kind: ParticleKind,    // Spray, Droplet, Foam (water); Dust, Clod, Grass (ground)
    pub life: [f32; 2],        // seconds
}
pub trait ParticleSink { fn emit(&mut self, burst: Burst); }
```

The renderer uses CPU-integrated particles (gravity, drag), up to 4096, in one instanced
draw with soft alpha. They are depth-tested against the 3D pass and lit by
`shadow::sun()`. Water kinds that land on the surface spawn a small ring in the height
field (the water system implements the sink callback for `Droplet`).

## Performance budgets

| | Native (DX12, mid GPU) | WebGL2 (integrated GPU) |
|---|---|---|
| Surface (phase 1) | ≤ 0.3 ms at 1600 × 900 | ≤ 1 ms at 1280 × 720 |
| Height field 512², 2 steps | ≤ 0.2 ms | ≤ 0.6 ms (256² with packed `Rgba8` if no float targets) |
| Splash particles (4096) | ≤ 0.2 ms | ≤ 0.5 ms |
| Ship buoyancy (CPU, 6 points × 50 ships) | < 0.05 ms | < 0.1 ms |

Phase 1 measured (Snapdragon X, Adreno, DX12, 1600 × 900, blank field, 3 ships on a
3000 × 2000 lake that fills the view; see below). Each figure is the mean time per
`record` frame (simulation, render, read-back and PNG write) over 40 frames:

| water | ms per recorded frame (two runs) |
|---|---:|
| `detailed` | 611, 572 |
| `original` | 375, 287 |

This measurement does **not** isolate the GPU cost. In driver mode a frame is only drawn
for a read-back, and the time goes mostly into the PNG encoding: a water frame's PNG is
1.9 MB against 1.2 MB for the flat tint, which explains most of the gap. A GPU timestamp
query, or an fps reading in a windowed run, is still to be done. Per pixel the surface
costs one texture sample, 8 sines and 2 noise lookups. The grid has at most
192² × 6 vertices, with 4 Gerstner waves each.

## Phases

1. **Surface** (built): Gerstner waves scaled by depth, depth colour and absorption, sky
   Fresnel, the sun's glint, shore and crest foam, fixed and free camera,
   `water=detailed|original`.
   1b. Ship heave, pitch and roll from the CPU-side Gerstner sum.
2. **Interaction**: the height field, with ship bow waves and a Kelvin decal, cannonball
   rings, swimmers, and bodies falling in.
3. **Look**: river flow (a flow map from the depth gradient and the terrain slope), ship
   shadows on the surface (sampling the shadow map in `water.wgsl`), a screen-space
   reflection of models where affordable (native only, behind a quality setting), and
   our own foam and normal textures.
4. **Particles and buoyancy**: splashes through the shared particle interface, and
   floating debris and corpses in `acr-physics`.

## Testing

`ACR-analysis/water/water_bench.py MODE [OUT]` (`ACR_EXE` = a release build) starts the
blank field with a 3000 × 2000 lake east of the centre, places a frigate, a pinta and a
santa (the last of player 2), and writes:
- `fixed_shore.png`, `fixed_close.png`, `fixed_far.png`;
- `free_low.png` (12° pitch);
- `sail/` (the frigate under way);
- the timing.

`MODE` sets `ACR_WATER`.
