# `acr_assets` module map

`assets/blender/lib/acr_assets/`. "pure" = no `bpy`, importable and unit-tested outside Blender. Each module's docstring is the authority; read it before editing.

## Core: rig, posing, export

| module | pure | owns |
|---|---|---|
| `__init__` | | `Reference` (the sprite reference of `acr model-ref`); import pattern `from acr_assets import ...` |
| `conventions` | yes | 30 px/m, axes, the original camera's math, directions |
| `scene` | | `reset`, the root node, naming, sockets |
| `rig` | yes | skeleton H (`skeleton_h`), horse rig R (`rig_r`), `BODY_BONES`, `PARTS`, `JOINTS`, `HIT_SOCKETS`, `SKIRT_COLUMNS`, `md_clip_frames`, `clip_keys`, `fall_slide` |
| `armature` | | armature from a rig table (`build`), `pose` (armature-axis Euler, `world`/`world_loc`, hand turned to the item on `acr_grip` rigs), `add_clip`, `set_clip`, `attach`, `bind`, `_fcurves` (Blender 5 channelbags) |
| `poses` | yes | placeholder poses and clips: `STAND_MUSKET`, `walk_cycle`, `death_keys`, `musketeer_clips`, `musketeer_strikes`, `STRIKE_HIT_FRAME` (2) |
| `clips` | yes | clips by weapon (bow, throw, sling, blowpipe, heal, drum, colour, sword/club/spear strikes, `STOOP`, `LEAN`), `unit_clips`, `drummer_clips`, `colour_clips`, `release_frame`; Blender side `apply` solves `reach` and keys `show` |
| `crawl` | yes | looping crawl clips `CRAWL2`, `CRAWL1`, `CRAWLARM`, `WRITHE` |
| `finish` | yes | finishing blows `ATTACK_<KIND>_FINISH`; `musket_clips` solved in Blender |
| `aim` | | two-bone IK for two-handed weapons: `_solve`, `_off_hand`, `left_on_shaft`, ramrod stow; results written back as pose specs |
| `grips` | | fists baked into the rest mesh (`close`), hand sockets in the fist (`setup`), grip `frame`, `hold_matrix`, `turn_hand`, `palm_errors` |
| `export` | | `export_glb`: the exporter settings (the contract's switches) |
| `registry` | yes | family hooks: `material`, `painter`, `garment`, `hat`, `gear`, `mask`, `build_hat`, `build_gear`; same key twice with another value raises |

## Humans: body, dress, looks

| module | pure | owns |
|---|---|---|
| `humans` | | `Character` (`body`, `shell`, `bone_layer`, `caps`, `morphs`, `masks`, `limb_levels`, `colours`, `triangles`), `prepare_export`, `set_decay`, `drop` |
| `body` | | skin hull, `skin_weights` (1/d⁴), part split at `rig.JOINTS`, body-derived shells, straps, skirt on bones, bone layer, stump caps, morph targets |
| `mbody` | | MakeHuman body as the skin hull, reposed onto skeleton H, `TARGET_TRIS` 2000 (fingers merge into mittens) |
| `mhead` | | MakeHuman head on skeleton H, `TARGET_TRIS` 1300, `face_*` morphs from MakeHuman targets |
| `heads` | | refined head, 512² face map (`face_map`), `face_*`/`body_*` morphs, `_FACE`, hair and beard variants (`variants`, `finish`) |
| `faces` | yes | face fields, morph zones, hair/beard regions, `choose(seed, style)` (ported to `looks.rs`) |
| `garments` | yes | sewing patterns: panels, seams, `MEASURE`, `EASE`, `CLOTH`, `ORDER`, `LAYER_RANK`, `uniform`, `seam_fractions` |
| `drape` | | patterns → meshes (`_mesh2d`), `place`, `simulate`, weld, `finish`, `transfer_weights`, `inherit_weights`, `hide_skin`, `hide_under`, `settle`; built but off: `layer_order`, `cull_hidden`, `relax_edges` |
| `outfit` | | one baked atlas per character (`atlas_uv`, Cycles bake of position/normal/AO, numpy painters), `dress` |
| `masks` | yes* | painted masks over the atlas (lace, body paint); numpy only inside the mask functions |
| `straps` | | belts over the draped surface or the body (`ch.strap_surface`, `strap_offsets`, `strap_under`) |
| `gear` | | gear meshes (`musket`, hats, boxes), `rigid`, `held`/`hold` (library item onto a socket), `seat`, `seat_pieces`, `lag_bone` |
| `held` | yes | held gear library (`Item`, `ITEMS`, `GRIPS`, `FIST`, `fists`), `set_materials`, `vane` |
| `limb` | yes | `_LIMB` levels along arm and leg chains (`ARM_LEVELS`, `LEG_LEVELS`, shared with `render/body.rs`) |
| `decay` | | `COLOR_0` decay mask, `_GORE`, decay layer windows (`acr_decay` extras) |
| `detail` | | procedural detail tiles and the box UV `detail` |
| `materials` | | Principled materials limited to what the exporter maps |
| `project` | | sprite colours projected through the original camera, `dominant` |
| `camera` | | the original camera as a Blender camera |

## Families (data-driven unit builders)

| module | family (`docs/humans-roster.md`) |
|---|---|
| `line` | 1+2 line infantry, drummers, standard bearers; `make`, `build`, `spec`, `_musket_clips`, sidearms, `key_show`, `unscale` |
| `native` | 3 Woodlands; base of `plains` (9) and `pueblo` (10), which wrap its `dress`/`hair`/`kit` per call |
| `officers` | 4 officers on foot |
| `cavalry` | 5 dragoons, mounted officers, `*KF18`, `SpnRei`, native riders; `close_hands`, reins IK, `MOTIONS` |
| `pike` | 6 pike and shot, 17th c. |
| `meso` | 7 Aztec, Maya |
| `inca` | 8 Inca |
| `frontier` | 11 frontiersmen; base of `conquista` (12) |
| `civil` | 13 peasants, priest |
| `guncrew` | `ArtPus` crew `c0.`..`c3.` in the gun's armature; per-frame grip IK (`ik_spans`, `key_clips`, `place_body`, `solve`, `measure`) |

## Animals, horses, ships, buildings

| module | owns |
|---|---|
| `animals` | WildMesh/Sketchfab animals from `ACR_SOURCES`: append, scale to `height`, decimate to `tris`, sample source actions onto `.MD` clips, decay masks |
| `black_bear` | the brown bear reshaped and repainted into `AnmMedSm` (rest-mesh edits by vertex groups) |
| `coat` | animal coat atlas: Smart-UV `atlas`, Cycles bake, numpy paint |
| `mount` | WildMesh horse as a mount: `load` (scale to `WITHERS` 1.5 m, prune, decimate, sample gaits), `join`, `key_clip`, `saddlecloth` |
| `horse` | the procedural horse on rig R (`ACR_HORSE=procedural`) |
| `vessels` | downloaded bases: import, strip PBR to base colour, shrink textures, decimate, rigid-bone rigs, step-scaled hidden parts |
| `ships` | Poly Haven Dutch ships: scale, draft, rocking, sails, wreck states |
| `kit`, `shapes`, `surfaces` | building kit, parametric meshes in game ground coordinates, building surface textures |
