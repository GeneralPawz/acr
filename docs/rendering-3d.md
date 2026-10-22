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
  ingame/piles.rs    corpse piles: lift and tilt of overlapping corpses
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
  - `deflate` and `lie_flat` still run on the GPU on top.
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
- **`_FACE`:** read as `Unorm8x4` into vertex location 14, so 15 of WebGL2's 16 locations
  are now used. Per soldier, the fragment shades stubble (beard zone × stubble), forehead
  folds (wrinkle zone × `face_age`), pock marks (pock zone × pocks) and, for 15 %, a pale
  scar slanting across one cheek (face front, at `acr_face_frame` eye height − 4 cm).
- **Instance data:** 3 look texels follow the marks in the bone texture (`look_texel`).
  They hold the skin tint and stubble, the hair tint and age, and the pocks, scar side,
  eye height and a has-look flag. Instances without a look get neutral texels.

Close-up check: `ACR-analysis/looks/looks_bench.py` places 8 EngM18 and 8 EngKri in rows
facing the camera at zoom 5 and 12.

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
- Not done yet (#183): cause-specific death clips, the `emaciated` morph for hunger, stuck
  missiles, pools that outlive the corpse. Dismemberment: see the next section.

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
  no longer loaded.
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
ball), so a ricocheting cannonball sprays along its bounce, not attacker → victim.

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
- Bodies are stacked by time of death: the larger death counter lies lower, ties by unit id.
  Each body samples the tops of the bodies placed before it at 8 points (±0.25, ±0.75 of
  the half length, ±0.5 of the half width); its lift is the mean of the highest back and
  front supports (or of left and right, whichever is more), at most 1.1 m; it tilts by the
  difference along the body (pitch) and half the difference across it (roll), at most
  0.5 (27°), turned rather than sheared (`view3d::model_matrix_tilted`).
- Piles settle: a body's thickness shrinks by 65 % of its `deflate` weight and its sink
  lowers its top, so the bodies above come down as the ones below decay.
- Blood pools stay on the ground; living units are not lifted.
- The bodies go into a flat grid (64 px cells, coarser when they are spread wide) every
  frame. Monongahela (`battle3.m3d`) with 530–570 English corpses on the map: 0.25–0.5 ms
  per frame (release, logged every 500 frames); 800 bodies packed into 40 × 20 m: 0.85 ms.

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
`ACR_MODELS_DIR` points the game at a folder of models (above). `ACR_DEBUG_DAMAGE` (native
only) forces damage on every 3D human: a spec of `Damage::parse` (`cut:arm_l:160`,
`frac:leg_l:170:55` (degrees), `trunk:110`, `head`, `belly`, `piece:arm_l`, `upper`, comma
separated) or `demo` (a preset per unit id), overriding the game's damage; severed pieces
stand beside their body. The log reports every 5 s
the frame rate, the models drawn and the poses sampled.

## Limits and open points

- No meshopt/Draco decoding, no KTX2 or JPEG textures: export models uncompressed with PNG
  textures (the pipeline's default).
- No shadows from models; no nation colour mask yet (#162); no LODs.
- Free camera: fog of war, the minimap frame and the water tint follow the original view
  only; sprites (trees, units without models) show their stored direction.
- Dropped gear (musket, hat) does not detach at death yet, only for crawlers and the
  disarmed (#262 S5); strapped gear floats over a deflated body (assets-humans.md, insight 6).
- No blending between clips; morph target normals are ignored.
