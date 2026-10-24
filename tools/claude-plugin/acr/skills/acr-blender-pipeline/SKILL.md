---
name: acr-blender-pipeline
description: "ACR's scripted Blender 5.2 asset pipeline. Covers cargo xtask assets on the VM, the acr_assets modules, skeleton H, the renderer's export contract, .MD clip names, stats.json, tests, sources, licences, showcase installs and known pitfalls. Load before editing assets/**.py."
---

# ACR's Blender asset pipeline

Every model is a Python script that Blender 5.2 runs headless (`-b --factory-startup`); nothing generated is committed. Read doc sections on demand, not whole files:

| question | read |
|---|---|
| layout, axes (30 px/m, +X east, +Y north, +Z up, models face −Y), sprite check, compression | `docs/assets.md` |
| skeleton H, layers, export contract, faces, held gear, grips, clips, straps, seated gear, decay | `docs/assets-humans.md` (sections by `###` heading) |
| sewing patterns, drape, layer order, trisoup, garment numbers, known problems | `docs/clothing.md` |
| every human unit, its family and module, known defects after the full rebuild (§6) | `docs/humans-roster.md` |
| one family's SPEC keys, look and open points | `docs/families/<family>.md` |
| what the game does with the glb (skinning, clips, LODs, looks, decay, layers) | `docs/rendering-3d.md` |

## Where things are

| path | what |
|---|---|
| `assets/<category>/<name>.py` | one script per model (`units`, `animals`, `artillery`, `buildings`, `gore`, `nature`, `ships`): `TYPE` (the `.MD` NAME), optional `CHECKS`, `MD` lines, `build(ctx)` |
| `assets/blender/build.py` | entry: runs one script in an empty scene, `export.export_glb`, writes `<name>.stats.json` from `ctx.stats` (+ `glb_bytes`) |
| `assets/blender/lib/acr_assets/` | the library; module map in `references/modules.md` |
| `assets/blender/tests/` | pure-Python unittests (no `bpy`) |
| `assets/blender/tools/` | `preview.py` (lit renders of a glb), `silhouette.py` (fit to sprites), `palette_check.py` (colours per region against the sprites), `clips_sheet.py`, `held_sheet.py`, `makehuman_*.py` (derive `data/makehuman/*.json`) |
| `target/assets/<category>/<name>.glb` | output (git-ignored), with `.stats.json`, and `.blend` with `--blend` |
| `assets/LICENSES.md` | every third-party source: author, licence, URL, how it is used |

Family modules (one per roster family) are data-driven: a unit script holds its `MD` lines, palette and `SPEC`, and calls `<family>.build(ctx, spec())`. Families: line 1+2, native 3, officers 4, cavalry 5, pike 6, meso 7, inca 8, plains 9, pueblo 10, frontier 11, conquista 12, civil 13 (`docs/humans-roster.md` §2). Families add keys through `registry` hooks, never by editing shared tables.

## Build, fetch, measure

```
acr-vm-cargo -- cargo xtask assets engm18 \
  --get 'target/assets/units/engm18.glb=C:/Users/verwalter/WIP/ACR-analysis/<topic>/engm18.glb' \
  --get 'target/assets/units/engm18.stats.json=C:/Users/verwalter/WIP/ACR-analysis/<topic>/engm18.stats.json'
```

- Name = the script's file stem, lower case. Several names in one call build in turn. Never `--help` (builds everything). `--blend` also saves the scene for inspection. `--check` scores the model against the sprites with `acr model-check` (IoU per `CHECKS` entry); the VM sets `ACR_GAME_DIR` and `ACR_SOURCES`.
- Times: most humans 20–50 s; draped line infantry about 3 min (`docs/clothing.md` "Numbers").
- Unit tests: `acr-vm-cargo -- python3 -m unittest discover -s assets/blender/tests` (also the `blender-tests` step of the landing checks). Some read a built model's stats: `test_guncrew.Grips` reads `target/assets/artillery/artpus.stats.json` (`ACR_ARTPUS_STATS` overrides) and skips without it.

**`stats.json`** holds what the asset measured while building. Read numbers here before looking at pictures:

| key | written by | meaning |
|---|---|---|
| `glb_bytes` | `build.py` | file size |
| `triangles` (per layer), `light`, `colours`, `aim_errors_m`, `drape` (per garment: `unposed_m`, `sim_s`, `sew`, `welded`, ...) | `line.build` | line-family soldiers |
| `hands.grips.<work>` = `{frames, mean_m, max_m, worst: [clip, man, frame]}` | `guncrew.key_clips` via `artpus.py` | fist-to-grip miss per frame as played; test limits: mean < 0.01 m, max < 0.10 m |
| `source_triangles`, `triangles`, `size_m`, `lift`, `clips` | `animals.build` | animals |
| `withers_m`, `stride_m` | `mount.load` | the WildMesh horse |
| `length_m`, `height_m`, `draft_m` | `ships.build` | ships |

Add a number to `ctx.stats` whenever you fix something measurable, so the next agent can check it without a render.

**Checks on a built glb** (all through `acr-vm-cargo`):
- `cargo run --release -p acr-render --example model_layers -- <glb>... --steps 8`: cloth layers skinned through every clip; last line `summary <glb> poking N crossing M` (`--max-poking`, `--max-crossing` gate).
- `cargo run --release -p acr-render --example gear_volume -- <glb>...`: body or cloth vertices inside closed carried gear.
- `C:\Users\verwalter\WIP\ACR-analysis\grips\measure_grips.py` (Blender on a `--blend` file): palm gap and knuckle-line angle per clip frame.

## Skeleton H, contract, clips

Read `references/skeleton-and-contract.md` before touching bones, sockets, clips, extras or attributes. In short:
- `rig.skeleton_h()`: 75 bones, 60 deforming; root at the ground point; rest pose A-pose with arms 35° out; facing −Y; sockets `socket.*` are non-deforming bones. Horse rig R: `rig.rig_r()`. Rider and gun crew share one armature with their mount or gun via a bone prefix (`rider.`, `c0.`..`c3.`).
- Poses are Euler degrees about the armature axes of the rest pose, relative to the parent (`armature.pose`); positive spine X bends the trunk forward.
- Clips are actions named after the `.MD` keys (`STAND`, `MOTION_L`, `ATTACK1`, `DEATH`, `DEATHLIE1` ...), keyed on sprite frames (`rig.md_clip_frames`, `rig.clip_keys`) and played at 24 fps (`extras.acr_fps` overrides). The exported scene pose is `STAND`.

## Sources and licences

- Bought or downloaded bases (WildMesh pack, Sketchfab, Poly Haven) are read through `ACR_SOURCES` (VM `/home/claude/sources`, locally `D:\ACR-models\extracted`). Never commit them or anything built from the WildMesh pack.
- Only CC0 or CC-BY sources, plus the bought WildMesh pack used locally. A new source gets a row in `assets/LICENSES.md` in the same commit.
- MakeHuman 1.x data (CC0) is derived into `assets/blender/data/makehuman/*.json` by `assets/blender/tools/makehuman_body.py` and `makehuman_head.py`.

## Putting a glb into the showcase

The user's build reads `C:\Users\verwalter\WIP\ACR-analysis\showcase\models\<name>.glb`. Those files are hard links (link counts 3–8: backups and other copies share them).
1. Copy the current file to `showcase\backup-<topic>\` (create the folder; one per change).
2. Delete `models\<name>.glb`, then copy the new glb in. Never write over it in place (`cp` onto it, `Set-Content`, `>`): that rewrites every linked copy, the backups included.
3. Say in your report which files went in and where the backup is. `acr-vm-drive` syncs the showcase models up before each run.

## Known open defects

Read [references/open-defects.md](references/open-defects.md) before working on humans: shared bugs with their workarounds, and the rebuild debt.

## Pitfalls (from history; each one cost a rebuild)

1. `Mesh.materials.clear()` also drops the `material_index` attribute in Blender 5.2: every face falls to slot 0 and the exporter drops the unused slots (drum, musket, saddle in one colour; 9883ad7). Use `held.set_materials(mesh, mats)`.
2. `drape.hide_under` once used `arm_tree`, a name local to `hide_skin`: a `NameError` in every drafted-garment build (435b192). Run a full build of one unit per touched family, not just the tests.
3. Garment weights must not come from the skin's hand faces: the baked fists curl the fingers in at the thigh, and a skirt swung with the hand (`drape.transfer_weights`, b8ce553).
4. A clip must key every bone it does not want to inherit: `armature.add_clip` keys every non-finger, non-socket bone at every key; `line.key_show` keys socket scales in every clip; `line.unscale` resets bone scales before export, or a socket hidden by the last clip bakes a 0 scale into its item.
5. IK solved only on keys misses between them: interpolation drifts off the grip. Solve and key every frame of a hold (`guncrew.ik_spans`, `key_clips`) and measure as played (935be84).
6. Two armatures sharing an action export no animation in `ACTIONS` mode: horse and rider, gun and crew are one armature.
7. Blender 5 actions are layered: `action.fcurves` is empty; use `armature._fcurves(act)` and set `animation_data.action_slot`.
8. `bmesh.ops.delete` renumbers vertices (key data by `BMVert`, not index); new shape keys start at value 1.0 in 5.2; one face twice over the same vertices makes the export report "mesh not valid" (use `held.vane`; `Mesh.validate()`).
9. `faces.new(...): face already exists` broke SpnAlb (`conquista._tassets`, `docs/humans-roster.md` §6).
10. Generic viewers multiply `COLOR_0` into the colour; the renderer reads it as the decay mask. Judge looks in the game or with `tools/preview.py`, not in a glTF viewer.
11. Shell garments derived from the body hull leave jagged "teeth"; draped garments are clean (`docs/clothing.md`, trisoup). Prefer converting a garment to a draft over patching a shell.
12. A shared target dir across worktrees runs stale binaries; on the VM each worktree has its own.
