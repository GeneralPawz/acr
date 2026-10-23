# Profiling, budgets and debug switches of the 3D path

## Profiler scopes (`acr_sim::prof`, native only)

`ACR_PROFILE=1` (read in `acr-game/src/native.rs`) logs every 5 s; the driver's
`{"cmd":"profile"}` switches it on and returns `{scope: {"ms": total, "calls": n}}` since the
last call. Names in the code (grep `prof_scope!` / `Scope::new` / `prof::add`):

| Group | Scopes |
|---|---|
| frame | `frame.advance`, `frame.acquire`, `frame.render_cpu`, `frame.present`, `game.after_tick`, `ticks` (a count) |
| sim | `sim.step`, `sim.units`, `sim.spacing`, `sim.search_victims`, `sim.object_loop`, `sim.projectiles`, `sim.brigades`, `sim.bleed` |
| 3D update | `3d.update`, `3d.instances`, `3d.spray_trail`, `3d.piles`, `3d.ragdolls`, `3d.blast_corpses`, `3d.cloth`, `3d.gear`, `3d.debris_decals`, `3d.blast_debris`, `3d.seeps`, `3d.kept_marks`, `3d.particles` |
| ragdolls | `rag.physics`, `rag.drive`, `rag.settle_palettes`, `rag.gaits`, `rag.strikes`, `rag.steps(count)`, `rag.awake(count)` (counts shown as "ms") |
| render | `render.models_prepare` |

A new cost centre gets its own `acr_sim::prof_scope!("3d.<name>")`; the macro is a no-op in
the browser.

## GPU time

`{"cmd":"gputime","frames":n}` (default 30; `drive/shot.rs` `gputime`): redraws the current
frame n times without advancing the game and returns ms per segment from timestamp queries.
Segments are the `mark_encoder` labels in `ThreeD::render`: `3d prepare`, `shadow map`,
`terrain depth`, `models`, `ground shadows`, `water`, `particles`, `sprites`; with the timer
on, the 3D pass is split at each label (timestamps inside one pass are unreliable on tiling
GPUs). Without timestamp support the answer is `{"supported": false, "reason": ...}`.

The `3d:` log line every 5 s (`ThreeD::render`) gives fps, models drawn, poses, instances,
triangles and draws for drawn and cast lists.

## Budgets written in the docs

| What | Budget | Source |
|---|---|---|
| Models pass at zoom 1, Monongahela `battle3` | 10 ms goal (was about 3× over after m2-lod2) | rendering-3d.md "Levels of detail" |
| Ragdoll physics | 4000 µs per frame; awake = 4000 µs / (µs per step × steps per frame), at least `MIN_AWAKE` (12) | body-sim.md "In the game", `ragdoll_view::calibrate` |
| Ground material pass | ≤ 1.0 ms at 1080p native, ≤ 2.0 ms WebGL2 | ground.md "Phase 6" |
| Ground state stamps | ≤ 0.3 ms native, ≤ 0.5 ms WebGL2 | ground.md "Phase 6" |
| Particles | ≤ 0.5 ms at 5k native, ≤ 1.0 ms at 3k WebGL2 | ground.md "Phase 6" |
| Water surface | ≤ 0.3 ms at 1600 × 900 native, ≤ 1 ms at 1280 × 720 WebGL2 | water.md "Performance budgets" (still unmeasured) |
| Runtime cloth | the 24 men nearest the camera focus within 900 px | clothing.md |

## Reference scenes (local scripts; port the steps to `acr-vm-drive`)

| Scene | Script | Used for |
|---|---|---|
| 700 v 700 melee on the blank field, zoom 1, 1600 × 900 | `ACR-analysis/perf/clash.py` | m2-perf, m2-perf2, m2-perf3 (before contact, contact, +30 s) |
| A/B of entry kinds in one process | `ACR-analysis/perf3/ab.py` | `ACR_MODEL_DEBUG_FILE` switching |
| Monongahela `battle3`, tick 1500 | `ACR-analysis/lod3/gputime.py`, `runs.py` | LOD and shadow passes |
| 200 decaying corpses | `ACR-analysis/seep/perf.py` | decay stains |
| 600 / 9000 impacts | `ACR-analysis/settings/marks_bench.py` | kept marks |
| Six ragdoll deaths side view | `ACR-analysis/ragdoll/bench_ragdoll.py` | ragdolls |

The numbers in docs/rendering-3d.md were measured on the Snapdragon X laptop (Adreno, DX12).
The render VM (RX 9060 XT, Vulkan) gives other absolute numbers: compare before and after on
the same machine, never a VM number against a doc number.

## Environment switches (native only)

| Variable | Effect |
|---|---|
| `ACR_PROFILE=1` | profiler on, log every 5 s |
| `ACR_FRAME_LATENCY=1..3` | swapchain queue (default 2; 1 lost the device on the Snapdragon) |
| `ACR_SOFT_CURSOR=1` | drawn cursor instead of the hardware cursor |
| `ACR_SHADOWS=on\|off` | overrides `shadows` (also driver `{"cmd":"shadows","on":false}`) |
| `ACR_LOD=off\|on\|0..3` | overrides `lod`, or forces a level |
| `ACR_WATER=original\|detailed` | overrides `water` |
| `ACR_MODEL_DEBUG=<list>` / `ACR_MODEL_DEBUG_FILE=<path>` | leave out kinds: `living`, `dead`, `dead_volumes`, `dead_plain`, `inner`, `caps`, `gore`, `holes`, `looks`; `old_inner`, `no_clean` restore older paths for A/B |
| `ACR_NO_CLOTH`, `ACR_NO_GEAR`, `ACR_NO_GAIT`, `ACR_NO_STRIKE` | switch a layer off (`three_d.rs`, `ragdoll_view.rs`) |
| `ACR_CLOTH=all` | skirt cloth also on men on their feet (by default only the dying and dead near the camera) |
| `ACR_STRIKE_DEBUG` | logs the strike layer per man |
| `ACR_DEBUG_DAMAGE=<spec>\|demo` | forces damage on every 3D human (`Damage::parse`: `cut:arm_l:160`, `frac:leg_l:170:55`, `trunk:110`, `head`, `belly`, `piece:arm_l`, `upper`) |
| `ACR_3D_COMPARE=overlay\|toggle` | sprites over models, or swap every 2 s |
| `ACR_MODELS_DIR` | model folder (vm_drive sets it per run) |

With `acr-vm-drive` pass them as `--env K=V` (repeatable).
