---
name: acr-render
description: "The ACR 3D frame: three_d.rs and the ingame view modules, acr-render, acr-physics, how sim animations pick clips, the settings that gate 3D features, WebGL2/wasm limits, profiling and budgets, known pitfalls. Use before changing models, gore, decals, particles, ragdolls, shadows, LOD or 3D speed."
---

# The 3D frame

Load `acr-workflow` (rules), `acr-vm` (builds, runs) and `acr-driver` (test scenes) with this.
Long maps: `references/module-map.md` (every view module, acr-render, acr-physics, one line
each) and `references/perf-and-debug.md` (profiler scopes, budgets, bench scenes, `ACR_*`
switches).

**Rule zero:** the 3D path is render only. Nothing in `ingame/*_view.rs`, `three_d.rs` or
acr-physics writes to `World`; variety comes from hashes of (unit, tick) or the hit, never the
sim RNG. The pinned state hashes must not move.

## One frame

| Step | Code | What happens |
|---|---|---|
| 1 | `ThreeD::update` (`ingame/three_d.rs`) | one `ModelInstance` per drawn unit: `instance()`, strike clip, gun crew, `Damage`, wound volumes, crawl clip, dropped gear, guts, trails; then spray, `piles::stack`, maulers, `update_ragdolls` (ragdolls, `Skirts`, carried `Gear`), debris, crew corpses, blasts, splatter, impacts, seeps, kept marks. Decals go into `self.pools` |
| 2 | `ThreeD::update_particles` | particle emitters and landings stamped into the ground state |
| 3 | `ThreeD::render` | `set_custom_poses` (crews + ragdolls), `ModelRenderer::prepare`, `prepare_pools`, sprites, water, particles; shadow map; one `DepthTarget` pass: terrain depth, models, ground shadows, water, particles, ground reset, sprites |

The instance list holds one entry per unit first; appended entries (debug pieces, guts, crew
corpses, debris) come after. Indices kept in `corpses`, `rigged`, `targets` point into the
first part, so append, never insert.

Models load lazily: a type's `<name>.glb` under `ACR/MODELS/` (or `ACR_MODELS_DIR`) is
requested when it first appears; `on_loaded` logs triangles, radius and clip names.
`gore.glb` is requested once gore is on.

## Sim state to clip

1. `instance()` (`three_d.rs`): `anim = u.anim.name()` (`AnimKind::name`, acr-sim
   `world.rs`: `#STAND`, `#MOTION_L`, `#MOTION_L1`, `#ATTACK1`, `#PATTACKi`, `#UATTACKi`,
   `#PSTANDi`, `#TRANSX3`, `#DEATH`, `#DEATHLIE1..3`, `#WORK`, `#BUILDLO_k`), clip time =
   `frame / data.fps` (24, or root `extras.acr_fps`); buildings at time 0, facing south.
2. `ModelData::clip_for_animation` (acr-render `model.rs`): exact name without `#`, else
   without trailing digits (`MOTION_L`), else base + `1` (`DEATHLIE1`), else `STAND`, else
   the first clip, else `None` (bind pose). Test: `animations_pick_clips`.
3. `sample` clamps to the keys: past the end the last key holds. Clips loop only where the
   caller wraps the time (`world_time`, crew `GUN_PUSH`).
4. Overrides, in the order `update` applies them:

| Override | Code | Rule |
|---|---|---|
| Strike | `strike_view::strike_clip` | `#ATTACK*` with a unit target: `<ANIM>_<KIND>_<HEIGHT>`, `<ANIM>_<HEIGHT>`, `<ANIM>_<KIND>_MID`; a man on the ground: `<ANIM>_<KIND>_FINISH`, `<ANIM>_FINISH`; riders `_CUT_DOWN_L` for the shorter turn |
| World clock | `world_time` | root `extras.acr_clock = "world"` (ships): loops alive at a per-unit phase, runs from the death tick when dead |
| Gun crew | `crew_view::crew_work`, `Crews::work` | `#PATTACK0/#UATTACK0` CHEST_CARRY fwd/back, `#MOTION_L0/R0` GUN_PUSH (looped), `#PSTAND0` holds it, `#PATTACK1` TRAIL_LIFT, `#MOTION_L1/R1` TRAIL_WALK fwd/back, `#PSTAND1` holds its first frame, `#PATTACK4/5` DEPLOY; a `#STAND` of ≤ 2 ticks between works keeps the last pose |
| Crawl | `damage_view::crawl_clip` | CRAWL2, CRAWL1 (one leg), CRAWLARM (one arm), WRITHE (no arm), on game time; missing clip: `DEATHLIE1` + `lie_flat` morph |
| Killed crawling | `LivingWounds::lying` | the crawl clip held at the death tick; same fallback |
| Gait | `gait_view` (inside `ragdoll_view`) | `MOTION*`/`CRAWL*` played by distance, feet and hands planted by IK |
| Ragdoll | `ragdoll_view` | the physics pose replaces the clip and clears `lie_flat` |

Corpse decay: `model::corpse_decay(death, removed_at)` (clamped by `set_decay_cap`);
`decay_pose` gives `deflate`, `lie_flat` and the sink.

## Settings that gate features

`ThreeD::apply` (`three_d.rs`) reads `ui/settings.rs` `Settings`, called from
`InGame::apply_settings` (`ingame/interface.rs`). Keys in `settings.txt`:

| Key | Values (default) | Gate |
|---|---|---|
| `models` | off, units, all (off) | `update` returns at once on off; `all` adds finished buildings. `active()` = models ≠ off or camera free |
| `camera` | fixed, free (fixed) | `ThreeD::free`, `Orbit`; keys `,` `.` PgUp PgDn End |
| `gore` | full, reduced, off (full) | off: the original fade (half transparent once `death > 100`), whole bodies, the original corpse removal, no stains; reduced: no dissolve, no guts, gibs, mist or wound volumes |
| `corpse_minutes` | 1..60 (5) | `corpse_ticks`: how long 3D corpses and debris stay |
| `corpse_decay` | full, half (full) | `model::set_decay_cap(HALF_DECAY = 0.45)` |
| `battlefield_marks` | fade, keep (fade) | `marks_keep`: `kept_marks.rs` (`CAP` 8192), `debris::KEEP_CAP` 1500 |
| `decay_stain` | on, off (on with gore=full; written only once set) | `Settings::decay_stain()`, `ThreeD::set_decay_stain`, `seep.rs`; never with gore=off |
| `shadows` | on, off (on) | `ModelRenderer::shadows`; `ACR_SHADOWS`, driver `shadows` |
| `lod` | on, off (on) | `ModelRenderer::lods`; `ACR_LOD` |
| `ground` | detailed, original (detailed) | only in 3D mode (`request_ground` needs `active()`) and with the textures in `ACR\Models\ground` |
| `water` | detailed, original (detailed) | `water_detailed()`; the surface draws when not zoomed out and (free camera or detailed) |

`hit_locations` and `body_spacing` are sim rules: they apply from the next battle, and the
driver's blank start takes them as its own keys (default false).

## WebGL2 and wasm

- The native device already asks for `Limits::downlevel_webgl2_defaults()`
  (`acr-app/src/lib.rs`), so native runs catch limit overruns.
- No storage buffers, no compute. Palettes and per-instance data live in one `Rgba32Float`
  texture, `BONE_WIDTH` 1024, 3 texels per matrix (`model_renderer.rs`).
- Vertex locations: 15 of 16 used (0–8, 14 per vertex; 9–13 per instance). Varyings: `VsOut`
  in `model.wgsl` uses locations 0–12. Adding either needs a slot freed first.
- No base instance, no base vertex: the instance buffer is bound from each model's first
  instance and indices are pre-offset.
- Every shader must translate to GLSL ES 3.00: add a new `.wgsl` to `SHADERS` in
  `crates/acr-render/tests/shaders.rs` (`shaders_translate_to_webgl2`).
- No timestamp queries (`gpu_timer::supported`), no profiler (`acr_sim::prof` is a no-op),
  no `ACR_*` switches, single thread (acr-physics takes rayon only off wasm32).
- Use `web_time::Instant` in acr-game runtime code; `std::time::Instant` panics in the browser.
- Settings come from `localStorage` `acr-settings` (base64 of the same lines).
- `acr-check` runs clippy for `wasm32-unknown-unknown` on acr-game, acr-render, acr-physics.
- glTF: no meshopt/Draco, no WebP; uncompressed meshes. Textures PNG, JPEG or KTX2
  (`KHR_texture_basisu`, `ktx2.rs`): the showcase models are KTX2 (ETC1S colours, UASTC masks,
  normals and data), transcoded at upload to BC7, ASTC, ETC2 or RGBA8 on native and WebGL2.
  Parts split off one model (unarmed body, dropped gear, states) share its transcoded
  textures (`ModelRenderer::ktx2_views`); a new glb needs `ktx2_pack.py` at install.

## Performance

- **CPU:** `--env ACR_PROFILE=1` on `acr-vm-drive`, or the driver: `{"cmd":"profile"}` once to
  switch on, `wait` N ticks, `profile` again; divide `ms` by the frames.
- **GPU:** `{"cmd":"gputime","frames":30}` gives ms per pass. The split between shadow map and
  models is not trustworthy on the Snapdragon tiler; compare the sum of all passes.
- **Budgets:** models pass 10 ms at zoom 1 in Monongahela `battle3` (not met); ragdolls
  4000 µs per frame (`ragdoll_view::calibrate`, `MIN_AWAKE` 12); more in the reference.
- **Swapchain:** `desired_maximum_frame_latency` stays 2 (`frame_latency` in
  `acr-app/src/lib.rs`). 1 lost the DX12 device within seconds on the Snapdragon X (047578d).
- **Done levers:** per-material far meshes (LOD 2–3), `Damage::splits_parts` caps only split
  bodies at level 1, `fs_clean` pipelines without `discard`, inner layers only once they can
  show, frozen corpses as static colliders and parallel islands, hardware cursor.
- **Next levers (measured):** split wounded corpses' parts by whether a wound volume reaches
  them (≈ 40 of 74 ms models pass at +30 s in the clash); a static pose for settled corpses;
  cached shadows of static corpses; incremental instance building; rayon over per-instance
  loops (`3d.instances`, `models_prepare`). Doc numbers are Snapdragon X: compare on one machine.

## Known pitfalls

- A clip fallback hides a missing clip: `#PSTAND1` fell back to `STAND` and the gun crew
  flickered every turn step (b6fe069). Check the clip names in `game.log` after a model change.
- A sim step records its hits under the tick it began with. Views that took only hits newer
  than the drawn tick skipped every hit; remember the newest hit taken (237fea1).
- meshopt "sloppy" picked corners from unworn hair variants that `vs_main` throws to clip
  `(2, 2, 2)`: long shadow streaks. Simplify per variant over its own vertices (c005507).
- Welded far meshes join kept and cut parts: stripes on cut men. Split bodies never draw
  coarser than level 1 (e7ff3de, then `splits_parts` in 928a909).
- Hiding inner layers of every intact corpse made gibs' bones flesh-coloured; the rig test
  keeps them for pieces without a rig (798c2a1).
- The Blender exporter writes the *render* colour attribute as `COLOR_0` and reads materials
  from evaluated copies: decay masks went missing (7b909a4).
- A ragdoll's palette cancels the instance transform: a sink or tilt put on the transform
  never reaches a physics-posed body. Do it in the shader after the palette (procedural
  decay, rendering-3d.md "Decay (#178) and gore (#183)").
- Timestamps inside one pass are unreliable on tiling GPUs: `gputime` splits the pass.
- `canister_mark.rs` is a dead file (not in `ingame.rs` `mod` list); edit `gun_marks.rs`.

## Read on demand

| Topic | Where |
|---|---|
| Frame order, shadows, skinning, looks | rendering-3d.md "Architecture" and its subsections |
| Clips, gun crew | rendering-3d.md "Animation states (#166)", "Gun crew (#287)" |
| Decay, gore, cuts, pieces, splatter, piles | rendering-3d.md "Decay (#178) and gore (#183)" and its `####` parts |
| Settings, stains, kept marks | rendering-3d.md "Settings", "Decay stains", "Kept battlefield marks" |
| Camera math, picking | rendering-3d.md "Camera math" |
| LOD and costs | rendering-3d.md "Performance" to "Next levers (measured, not done)" |
| Ragdolls, gait, wound volumes | body-sim.md §1 "In the game (#278)", §2, §3 |
| Strike layer | melee.md "Animation" |
| Ground, particles | ground.md; water: water.md; cloth: clothing.md "Runtime cloth" |
