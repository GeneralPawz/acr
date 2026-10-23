# Skeleton H, rig R, the export contract and the clips

Authority: `assets/blender/lib/acr_assets/rig.py` (bones), `export.py` (exporter switches), `docs/assets-humans.md` "Export contract", `docs/rendering-3d.md` (what the renderer reads). Where a doc and the code differ, the code wins.

## Skeleton H (`rig.skeleton_h(prefix="", offset=...)`)

75 bones, 60 deforming. Metres, Blender axes (+X the character's left, −Y forward, +Z up), rest pose a relaxed A-pose with the arms 35° out (`rig.A_POSE`). `docs/assets-humans.md` §2 still says 59/52: that count predates the skirt bones and hit sockets.

| group | bones | deform |
|---|---|---|
| root | `root` (ground point) | no |
| trunk | `hips` → `spine` → `spine1` → `spine2` → `neck` → `head` | yes |
| arm, per side `_l`/`_r` | `clavicle` → `upperarm` → `forearm` → `hand` | yes |
| fingers, per side | `thumb_01..03`, `index_01..03`, `middle_01..03`, `ring_01..03`, `pinky_01..03` on `hand` | yes, but carry no weights (MakeHuman mittens; fists are baked by `grips.close`) |
| leg, per side | `thigh` → `shin` → `foot` → `toe` (from `hips`) | yes |
| coat skirt | `skirt_{fl,fr,bl,br}_01` → `_02` (from `hips`, `rig.SKIRT_COLUMNS`) | yes |
| sockets | `socket.hat` (head), `socket.back` (spine2), `socket.hip_l` (bayonet/sword), `socket.hip_r` (cartridge box), `socket.hand_l`, `socket.hand_r` | no |
| hit sockets | `socket.hit_{head,chest,belly,back,arm_l,arm_r,thigh_l,thigh_r}` (`rig.HIT_SOCKETS`) | no |
| added by builders | `socket.club_r`, `socket.blade_r`, `socket.sling` (`line._add_sockets`), `socket.tip` etc. on items (`<item>.socket.*` empties), `gear.*` lag bones (`gear.lag_bone`) | no |

- `rig.BODY_BONES`: the 22 bones that weight the skin.
- `rig.PARTS` (body parts 0..9: torso, head, upperarm_l, forearm_l, upperarm_r, forearm_r, thigh_l, shin_l, thigh_r, shin_r) and `rig.JOINTS` (neck, shoulders, elbows, hips, knees) define cuts; `part_of_bone` maps fingers to their hand and skirt bones to the torso.
- Prefixed copies: a rider is `rider.*` on the horse's armature (`mount.join`, `socket.saddle`); the gun crew is `c0.`..`c3.` on the gun's (`guncrew`).
- BVH retargeting (CMU/Mixamo-like) is a name map plus rest-pose world rotations (`docs/assets-humans.md` §2); the bone rolls follow no BVH convention.

## Horse rig R (`rig.rig_r()`, procedural horse only)

`root`, `pelvis`, `spine_01`, `spine_02`, `chest`, `neck_01`, `neck_02`, `head`, `tail_01..03`, legs `front_{scapula,humerus,radius,cannon,pastern,hoof}_[lr]` and `back_{femur,tibia,hock,pastern,hoof}_[lr]`, sockets `socket.saddle`, `socket.head`, `socket.holster_l/_r`. The cavalry uses the WildMesh horse's own rig instead (`mount.py`, pruned to its weighted bones).

## Export contract (what the renderer reads)

Exporter switches are in `export.export_glb`: GLB, +Y up, modifiers applied, extras on, skins, morph targets without normals, one animation per action (`ACTIONS`, sampled), active colour attribute as `COLOR_0`, custom attributes whose names start with `_`, images PNG, no compression by default.

- File `<md name lower case>.glb`; one armature; root node = asset name; parts `<asset>.<part>`; root extras `acr_asset`, `acr_type`, plus `acr_clips`, `acr_triangles`, `acr_layers`, `acr_limb_radius`, `acr_light`, face extras (`humans.prepare_export`).
- Exported scene pose is `STAND`, not the bind pose (`export_rest_position_armature=False`).
- Materials: the renderer reads the base colour factor and base-colour texture (embedded PNG; nearest filtering if the sampler says so), alpha MASK cutoff and extras (`acr_decay`, `acr_tint`, `acr_layer`, `acr_roughness` for wet tissue). Normal, metallic-roughness, occlusion and emissive maps are ignored: bake shading into the base colour. Light is Lambert from (−15, 14, 30) with half ambient; no back-face culling.
- Vertices: position, normal, UV (`TEXCOORD_0`), `COLOR_0` (decay mask on `acr_decay` materials: R blotch noise, G fray noise, B layer code / 8), 4 joints and 4 weights, two GPU morph offsets, `_PART` (0..9; stump caps 16 + joint, 32 + joint; horse 48), `_LIMB` (0..1 along a limb, −1 trunk/head/held gear), `_GORE` (vec4). With the instance data that is 14 vertex attributes of WebGL2's 16: a new per-vertex channel needs a renderer change first.
- Morph targets: `deflate`, `emaciated`, per-soldier `face_*` and `body_*`.
- Gear that drops at death (hat, musket, sabre) is a separate node on a socket bone; strapped gear (knapsack, cartridge box, saddle) is skinned.
- `<name>.caps`: stump caps, hidden until a joint is cut.
- LODs are not authored: the renderer builds index LODs at load (`lod.rs`: up to 100/35/12/3 %, error-capped), locking positions where `_PART`s meet.

## Clips and the `.MD` animations

- A unit script copies its `.MD` animation lines (`MD = {"STAND": "#STAND 9 1 5 0", "DEATH": "@DEATH 9 4 0 70", ...}`). `rig.md_clip_frames`: an `@` range has |last − first| + 1 frames (backwards if last < first); a `#` line has its second number of frames.
- `rig.clip_keys(n, [(fraction, pose)])` puts keys on sprite frames, so clip frame i = sprite frame i; played at 24 fps (root `extras.acr_fps` overrides).
- Names: `STAND`, `MOTION_L`/`_R`/`_LB`/`_RB` (walk), `PSTAND*`, `PATTACK*`/`UATTACK*` (reload or ready phases), `ATTACK` (weapon index 0, the `.MD` fallback), `ATTACK1`, `ATTACK2` (index i; a musketeer's shot is `ATTACK1`, the bayonet `ATTACK`), `REST`, `TRANS*`, `DEATH`, `DEATHLIE1..3`. ACR-only: strike clips `ATTACK_CUT_MID`, `ATTACK_CUT_HIGH`, `ATTACK_THRUST_MID`, `ATTACK_THRUST_DOWN`, `ATTACK_DOWN`; finishing blows `ATTACK_<KIND>_FINISH`; crawl `CRAWL2`, `CRAWL1`, `CRAWLARM`, `WRITHE`; gun `GUN_PUSH`, `CHEST_CARRY`, `TRAIL_LIFT`, `TRAIL_WALK`, `DEPLOY`. List every clip in `acr_clips`.
- Renderer lookup by name: exact, then without variant digits, then the family's first (`DEATHLIE1` for `DEATHLIE3`), then `STAND`, then the first clip. A state with no clip falls back, which shows as a pop (the gun crew's `#PSTAND1` flicker, b6fe069).
- Events: missiles leave on `SETACTIVEPOINT #ATTACKn f` rounded up to even (`clips.release_frame`); melee blows land on frame 2 (`poses.STRIKE_HIT_FRAME`). Looping clips end on a copy of frame 0.
