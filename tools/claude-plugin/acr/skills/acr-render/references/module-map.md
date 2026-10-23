# Module map of the 3D path

One line per module, checked against the `//!` headers on origin/main (e0cfd78). Every
`ingame/` module below is render only: it reads `World` and never writes it back.

## `crates/acr-game/src/ingame/` (the game side of the 3D frame)

| Module | Lines | What it does |
|---|---:|---|
| `three_d.rs` | 1527 | `ThreeD`: settings (`apply`), model files under `ACR/MODELS/` (`start`, `on_loaded`), per-frame instances and decals (`update`), particles (`update_particles`), ragdolls/cloth/gear (`update_ragdolls`), the 3D pass (`render`), free-camera keys |
| `ragdoll_view.rs` | 1571 | XPBD ragdolls for hit and dying men and animals near the camera; holds `Gaits` and `Strikes`; budget by `calibrate` |
| `strike_view.rs` | 1211 | melee strike layer: `strike_clip` by kind and height, trunk bend, step-in, arm IK, rider horse turn (`Turns`) |
| `gait_view.rs` | 328 | walking and crawling clips played by distance (phase lock), foot and hand IK, limps |
| `crew_view.rs` | 510 | gun crews: losses from the gun's life (`Crew::mask`), crew corpses, work clips (`crew_clip`, `Crews::work`), smooth gun turn (`gun_yaw`), uniforms by nation |
| `cloth_view.rs` | 477 | XPBD coat skirts of the dying and dead near the camera (`Skirts`; `ACR_CLOTH=all` adds men on their feet) |
| `gear_view.rs` | 306 | knapsack, quivers, slung musket lag and bob on a damped spring (`Gear`) |
| `maul_view.rs` | 271 | animals stand over a man they knocked down, eased in and out |
| `damage_view.rs` | 140 | `acr_sim::body` to `acr_render::damage::Damage`; `crawl_clip` |
| `living_wounds.rs` | 420 | wounds on the living from `HitRecord::injury`, carried to the corpse; spurts; who lies crawling |
| `tears.rs` | 426 | wound volumes (tissue torn out), hashed from the hit |
| `gore.rs` | 197 | blood, wounds, dirt and burns on corpses by cause of death; the corpse pool |
| `debris.rs` | 591 | severed pieces, thrown halves, gibs, mist, guts, dropped musket and hat; `KEEP_CAP` 1500 |
| `piles.rs` | 255 | corpses lie on each other: footprints, lift and tilt (`stack`) |
| `splatter.rs` | 505 | blood droplets behind hits, sprayed blood on units, ground impacts; stateless from `World::hits` |
| `trail.rs` | 219 | blood drops and crawl smears of the bleeding |
| `seep.rs` | 226 | `decay_stain`: the decay island under a rotting corpse (`Dirt::Seep`) |
| `kept_marks.rs` | 202 | `battlefield_marks=keep`: copies settled decals, merges them, `CAP` 8192 |
| `ground_marks.rs` | 246 | prints, ruts, craters and scorch stamped into `GroundState` (detailed ground) |
| `particle_view.rs` | 803 | emitters: earth fountains, shell rings, dust, mud, haze, water splashes; `soil_at` |
| `water_view.rs` | 164 | ship heave/pitch/roll from `Waves`, wakes, shot rings |
| `trample.rs` | 123 | trodden paths darken the terrain vertices (sprite and 3D mode) |
| `effects.rs` | 546 | the original's projectiles, smoke, explosions, building fires, `#WORK` animations, weapon sounds |
| `gun_marks.rs` | 268 | the ground a selected cannon's fire mode covers (yellow lines) |
| `canister_mark.rs` | 50 | dead file: not in the `mod` list of `ingame.rs`, superseded by `gun_marks.rs` |
| `shouts.rs` | 167 | panic shouts over fleeing units (0x600bc8) |
| `probe.rs` | 223 | what the driver reads from a running game (native only) |

## `crates/acr-render/src/`

| Module | What it does |
|---|---|
| `view3d.rs` | `View3d`: the original projection as a matrix, the free camera (`Orbit`), `ground_point`, `facing_yaw`, `model_matrix(_tilted)`, `ground_at` |
| `model.rs` | `ModelData::from_glb`, clips, `pose`, `clip_for_animation`, `corpse_decay`, `decay_pose`, `HALF_DECAY`, `set_decay_cap`, `node_part`, `without_nodes` |
| `model_renderer.rs` | `ModelRenderer` (bone texture, instancing, LOD lists, clean/discard pipelines, shadow casters), `DepthTarget` (the 3D pass), `BloodPool`, `Dirt` |
| `model.wgsl`, `model_shadow.wgsl` | skinning, morphs, decay, wounds, looks, cuts; shadow casters |
| `damage.rs` | `Damage`: cuts, fractures, torn trunk, head, belly, pieces, wound volumes; `Rig`; `splits_parts` |
| `lod.rs` | index-only LOD levels (meshopt-rs), `far_mesh`, `far_shadow`, `sloppy`, `DAMAGED_UP_TO`, `SHADOW_STEP` |
| `looks.rs` | per-soldier faces, builds, hair, tints (`faces.choose` port), 16 baked variants |
| `layers.rs` | layer check of dressed models (inner layers poking through) |
| `shadow.rs`, `shadow.wgsl` | one 2048² sun shadow map fitted to the drawn models; `LIGHT` |
| `particles.rs`, `.wgsl` | CPU particle pool (`MAX_PARTICLES` 4096), instanced quads |
| `water.rs`, `.wgsl` | Gerstner water surface (`Waves`, `Disturbances`) |
| `ground.rs`, `ground.wgsl` | detailed ground materials over the original tiles |
| `ground_state.rs` | soil state atlas (prints, craters) |
| `terrain.rs`, `.wgsl` | the original terrain; `vs_depth` for the 3D pass; `set_trample` |
| `scene.rs`, `objects_view.rs` | map objects in draw order; sprite `Placement` in the 3D pass |
| `blood.wgsl` | decals: pools, droplets, smears, craters, seeps (instanced quads) |
| `fog.rs` | fog of war (original) |
| `gpu_timer.rs` | timestamp queries for `gputime` (`mark_encoder`, `mark_pass`) |
| `math.rs` | column-major 4×4 matrices, quaternions `[x, y, z, w]` |
| `tests/shaders.rs` | `shaders_translate_to_webgl2`: every shader to GLSL ES 3.00 |

## `crates/acr-physics/src/` (no dependencies; rayon on native only)

| Module | What it does |
|---|---|
| `world.rs` | XPBD solver: capsules, joints with limits and muscle drives, ground and capsule contacts, sleeping, islands |
| `ragdoll.rs` | ragdolls from a skeleton description: capsules, Dempster masses, joint limits, tone; `skin_matrix` |
| `gait.rs` | kinematic locomotion layers over clips (`Cycle`) |
| `cloth.rs` | light XPBD cloth for skirts |
| `math.rs` | `f32` vectors and quaternions |
