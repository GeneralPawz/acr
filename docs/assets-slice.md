# Vertical slice (#168): musketeer, dragoon, wooden fort, a tree

The first assets built entirely by `cargo xtask assets` (scripted Blender 5.2, headless) and
checked against the original sprites with `acr model-check`: `EngM18` musketeer, `EngKri`
peasant, `EngD18` dragoon, `EngKre` wooden fort and the broadleaf tree `TREES.GP` 104.
Pipeline: [assets.md](assets.md); characters and the export contract:
[assets-humans.md](assets-humans.md); renderer: [rendering-3d.md](rendering-3d.md).
Part of #168, #169, #170, #172, #178, #183.

```sh
cargo xtask assets --game-dir "C:\path\to\American Conquest" --check     # all, 2 min 47 s
blender -b --factory-startup --python assets/blender/tools/preview.py -- \
    target/assets/units/engm18.glb target/assets/preview/engm18 --ref target/assets/ref/EngM18/ref.json
```

## Numbers

IoU = silhouette intersection over union per stored direction, mean over the directions, at
the ground point (`acr model-check`); "shifted" = at the best shift within ±24 px (the
check's anchor search). Colour difference = mean absolute RGB difference (0–255) where both
are solid, render unlit.

| asset | check | IoU | IoU shifted | colour diff | triangles (all layers/states) | `.glb` |
|---|---|---:|---:|---:|---:|---:|
| `engm18` | `#STAND` | **0.751** | 0.770 | 48.7 | 4 852 | 1.21 MB |
| | `#DEATHLIE1` | 0.442 | 0.587 | 60.9 | | |
| `engkri` | `#STAND` | 0.635 | 0.710 | 37.8 | 4 740 | 0.99 MB |
| | `#DEATHLIE1` | 0.441 | 0.572 | 57.5 | | |
| `engd18` | `#STAND` | **0.758** | 0.782 | 40.0 | 7 197 | 1.82 MB |
| | `#DEATHLIE1` | 0.502 | 0.514 | 37.1 | | |
| `engkre` | `#STANDLO` + section 0 state 0 (wood) | **0.910** | 0.910 | 3.8 | 30 108 | 5.98 MB |
| | state 1 (stone house in scaffolding) | 0.899 | 0.900 | 2.3 | | |
| | state 2 (stone house) | 0.901 | 0.901 | 2.2 | | |
| | state 3 (ruin of the section) | 0.882 | 0.883 | 5.2 | | |
| | `#BUILDLO_0` | 0.593 | 0.599 | 0.2 | | |
| | `#BUILDLO_1` | 0.798 | 0.800 | 0.3 | | |
| | `#BUILDLO_2` | 0.839 | 0.843 | 0.4 | | |
| | `#BUILDLO_3` | 0.871 | 0.876 | 1.5 | | |
| | `#DEATHLIE1` (ruin) | 0.810 | 0.810 | 0.9 | | |
| `broadleaf` | `TREELIST:104` | 0.684 | 0.684 | 23.8 | 1 740 | 0.19 MB |
| `incdo1` (#162 proof) | `#STANDLO` | 0.903 | 0.903 | 2.4 | – | 0.13 MB |

Target for `EngM18 #STAND` was IoU ≥ 0.75 with ≤ 6k triangles: **0.751 with 4 852
triangles** (bones 836, flesh 1 828, clothes ~1 900, stump caps 288, gear ~200).

The unit sprites stand **2–4 px lower** than a model whose feet are on the ground point (the
check's best shift is (0, 2..4) in every direction for both humans, as in the first run):
the original renders put the ground under the anchor. The models keep their feet on the
ground; matching the sprites exactly would need the renderer to place units ~6 world px
(0.2 m) further south, or a 0.1 m lower model. The lying poses lose most to it (0.44 → 0.59).

## Screenshots

All under `target/assets/` (git-ignored; rebuilt by the commands above):

| what | files |
|---|---|
| sprite \| model \| silhouettes, original camera | `check/<asset>/<check>/side_<k>.png` (e.g. `check/engm18/STAND/side_0.png`, `check/engkre/STANDLO-2/side_0.png`) |
| lit, original camera (4×) | `preview/engm18/orig_{0,2,4,6,8}.png`, `preview/engd18/orig_{0,4}.png`, `preview/engkri/orig_{0,4}.png`, `preview/engkre/orig_0.png`, `preview/broadleaf/orig_0.png` |
| free view | `preview/engm18/free_{0..3}.png`, `preview/engd18/free_{0..3}.png`, `preview/engkre/free_{0,1}.png`, `preview/engkre-build1/free_0.png`, `preview/broadleaf/free_{0,1}.png`, `preview/broadleaf-felled/free_0.png`, `preview/engd18-lie/free_0.png` |
| decay of a musketeer corpse (from the glTF alone) | `preview/engm18-lie/decay_{0.00,0.20,0.35,0.50,0.65,0.80,0.90}.png` |
| in game (renderer of main, `models=all`, Miss7E01, 72 EngM18 and dragoons) | `preview/ingame_fixed.png`, `preview/ingame_free.png` |

## Art direction

- **Units: stylised low poly, flat colours per part, lit by the renderer.** One colour per
  part comes from the sprites (the pixels the part covers, robust median near a palette
  colour); a 64-px procedural tile (wool weave, linen, leather, wood grain) adds close-up
  variation. At game zoom they read like the sprites (red coats, white waistcoats, buff
  belts, black gaiters); close up they are clean, toy-like figures without the sprite's
  pixel mosaic. Sprite textures were tried first: at 60 px per figure they turn into
  wrong-coloured speckles in the free view.
- **Buildings: the sprite is the texture.** The fort's sprites are 900 px wide (3 cm per
  pixel), so projecting each state's sprite onto simple kit geometry (palisade stakes, log
  boxes, gable/hip/lean-to roofs) reproduces the original exactly at the original camera
  (colour difference 0.2–5) and looks like a painted model from nearby; faces no view sees
  get the part's mean colour, which shows when the free camera looks at the back.
- **Nature: procedural shapes, sprite colours.** The tree's crown is clumps placed at the
  k-means clusters of the sprite's crown pixels, coloured with the sprite's dominant leaf
  and bark colours.

## Problems and insights

1. **Silhouette fitting works and is cheap.** `tools/silhouette.py` rebuilds a model and
   scores all directions in one Blender session (0.4 s per pose, ~8 s per rebuild); a
   coordinate descent over 10–20 pose angles or size constants took EngM18 from 0.57 to
   0.75, the fort from 0.79 to 0.91, the dragoon from 0.62 to 0.76. It also games the
   metric (it lowered hats, sank the body, nodded the head 100°), so every parameter needs
   bounds.
2. **Interior layout of buildings is not constrained by the silhouette** (all solid); it
   rests on measurements by eye. Projection hides the errors at the original camera, the
   free view shows them.
3. **Lying poses are the weak spot** (IoU 0.44–0.50): the corpse sprites are motion-blurred
   last frames, and the vertical anchor offset hurts flat shapes most.
4. **Blender's exporter and slotted actions:** two armatures sharing an action export no
   animation in `ACTIONS` mode, so horse and rider share one armature (`rider.*` bones on
   `socket.saddle`). Exporting keys without sampling changed the STAND pose slightly (0.751
   → 0.746), so clips stay sampled; constant channels of fingers and sockets are not keyed.
5. **glTF and the decay mask:** generic viewers (and Blender's importer) multiply `COLOR_0`
   into the base colour; the renderer reads it as the mask for `acr_decay` materials, so
   previews must ignore it (`preview.py` does).
6. **File size:** units 1–1.8 MB, mostly float attributes (`WEIGHTS_0`, `_GORE`, positions)
   and sampled clips; the fort 6 MB of PNG atlases (10 states). JPEG cut the fort to 3.4 MB,
   but the renderer decodes PNG only for now.
7. **Shared cargo target directories** between worktrees made `cargo xtask` run another
   branch's binary twice (cargo saw its own fingerprint as fresh while `debug/xtask.exe` had
   been replaced); the slice used a private target directory.

## Before scaling to all groups

- Place units 0.2 m south (or record the offset per type) so models and sprites coincide;
  re-fit lying poses after that.
- Batch the checks: render every `CHECKS` entry in one Blender session and cache `acr
  model-ref` output per (type, arguments); today each check starts Blender and the acr tool.
- A body kit per group (European foot, cavalry, natives) with parametric clothing so a new
  unit is a palette + a parts list + a fitted pose, not a script.
- Bounds for every fitted parameter; fit clothing sizes per group, poses per type.
- Size: quantised attributes (`KHR_mesh_quantization`, `_GORE`/`_PART` as bytes),
  meshopt and KTX2 once the renderer decodes them; LODs (drop the bone layer and caps
  until a corpse needs them).
- Buildings: measure plans semi-automatically (ground lines from the sprite's lowest
  opaque pixels), fill unseen faces from similar seen ones, bake the projection into
  per-part textures so states can share geometry.
- Trees: leaf cards instead of clumps for an organic crown; one model per look-alike group.
