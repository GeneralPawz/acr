# Clothing: sewing patterns draped by cloth simulation

The body-derived shells (`body.derive`, `Character.shell`) gave jagged hems and a coat of
patches. Garments are now cut from historical 2D patterns, draped on the MakeHuman body with
Blender's cloth simulation (in the normal asset build, on the render VM), and skinned to
skeleton H. Loose parts (the coat skirts) keep their own bones and a `_cloth` weight so that
the game's XPBD engine can move them later.

The pipeline runs in the line infantry family `acr_assets/line.py` (unit scripts are data:
[humans-roster.md](humans-roster.md)). `engm18.BUILD["garments"]` switches the musketeer between the draped uniform (`True`) and
the shells of #178 (`False`, the fallback).

| module (`assets/blender/lib/acr_assets/`) | |
|---|---|
| `garments.py` | patterns as data (pure Python, unit-tested in `tests/test_garments.py`): panels, seams, inlays, placement, cloth settings, drafting to measure |
| `drape.py` | needs `bpy`: measure the body, mesh and place the panels, sew, simulate, weld, skin, split into export layers, push the covered skin in |

## Pipeline

1. **Measure** (`drape.Body`): the full-resolution MakeHuman body (`mbody.repose`, about
   13k vertices, not the decimated hull), each vertex classified by its nearest bone.
   Girths (chest, waist, seat, neck, upper arm, wrist, thigh, knee, calf, ankle) are convex
   girths of 3 cm slices; lengths come from the rig. Each figure is kept within 0.8..1.25
   of the default in `garments.MEASURE`. Torso cross-sections per 2 cm become ellipses
   (robust half widths, scaled so that the perimeter is the slice's convex girth, so armpit
   outliers do not widen them).
2. **Draft** (`garments.uniform(m)`): every panel is drafted from the measurements plus the
   garment's ease (`garments.EASE`), so the same draft fits every build.
3. **Mesh** (`drape._mesh2d`): the outline sampled every `drape.SPACING[garment]` cm
   (3.5 cm; corners kept), interior points on a triangular lattice, constrained Delaunay
   (`mathutils.geometry.delaunay_2d_cdt`). **Inlays** (lapels, turnbacks: panels with
   `tack` = their base panel) are not separate cloth: their outlines are cut into the base
   panel as constraint edges and the triangles inside take the inlay's material. One
   surface, a crisp outline, no z-fighting.
4. **Place** (`drape.place`): each panel is wrapped, arc length preserved, around its region
   (elliptical cylinders for the torso and neck, round ones round the arm and the leg),
   `gap` cm off the skin, faces turned outwards. Torso panels at centre front and back go
   **over the shoulders**: from 4 cm below the shoulder joint they run flat across (x = u),
   up to the top of the skin and on towards the other side, so the shoulder seams start a
   few cm apart instead of a body's depth. Left panels are drafted; the right ones are
   mirrored copies.
5. **Sew**: the two sides of each seam are paired by arc-length fraction; the pairs become
   loose edges, which Blender's cloth treats as sewing springs.
6. **Simulate** (`drape.simulate`), one cloth object per garment in `garments.ORDER`
   (breeches, gaiters, shirt, waistcoat, coat), colliding with the skin, the shoes and the
   garments before it. Two phases in one run (keyframed settings):
   - frames 1..45 (`SEW_FRAMES`): no gravity, no shrink, the sewing force ramps from 5 to
     40 (`SEW_FORCE`);
   - then over 8 frames gravity fades in, the sewing force drops to 15 and `shrink_min`
     fades in; the cloth settles until frame 75 (`FRAMES`).
   Quality 8, air damping 5, no self-collision. Per garment (`garments.CLOTH`): mass,
   tension, bending, shrink, and a **collision margin** that grows outwards (breeches and
   gaiters 3 mm, waistcoat 4 mm, coat 6 mm; the collider thickness of a draped garment is
   its own margin). Pinned edges (`place["pin"]`: the middle half of the gaiters'
   garters, so the seam at its ends can close) stay where they were placed. The shirt pieces (wristbands, stock) are not simulated.
7. **Weld**: sewn pairs closer than 4.5 cm (`WELD`, about one triangle) are merged at their
   midpoint, the other sewing edges dropped. Single-sided cloth (`SOLIDIFY = False`): the
   renderer draws back faces shaded (`model.wgsl`, `front_facing`) and 2–3 mm of thickness
   is a tenth of a pixel at 30 px/m; `True` adds a solidify with rims along the outline.
8. **Skin and split** (`drape.finish`):
   - Weights interpolated from the nearest face of the skin, restricted to the bones of the
     vertex's own side (the skin inside the shoes is deleted, so a gaiter's tongue could
     find the other foot).
   - Skirt panels (coat skirts, turnbacks) take the skirt bones of `rig.SKIRT_COLUMNS` and
     a **corrective** share of the thigh behind them: up to 55 % at the hem in front, 30 %
     at the sides and back, little at the back vent. The clips only move the skirt bones
     when the body lies down, so without it a striding leg passes through the skirt.
   - `part` from the dominant bone, the mesh split at the part borders like the skin, so
     `_PART`, `_LIMB`, the decay masks, the morphs and cuts all work through the normal
     `Character` finishing (docs/assets-humans.md, docs/damage-model.md).
9. **One object per material key** and skirt flag: `coat`, `facings` (lapels, cuffs,
   collar), `coat_skirts`, `turnbacks` (white lining), `waistcoat`, `breeches`, `gaiters`,
   `shirt`, `stock`. `outfit.PAINTER` has own painters `facings` and `lining` (painted like
   wool), so they get their own atlas cells and colours; other units are unchanged.
10. **Skin under the clothes** (`drape.hide_skin`): skin vertices whose outward ray hits a
    garment within 6 cm are pushed 12 mm towards their bone. They stay (decay and cuts need
    them) but cannot poke through.

The crossbelts are laid over a joined surface of the draped garments (28 samples, 14/18 mm
off it) instead of the skin.

## Layers

The garments collide only in the rest pose (the drape). What keeps them in order under
skinning, in every clip and ragdoll pose, is cheap and static; there is no per-frame
collision between layers (too dear for hundreds of men):

- **Weights from the layer beneath** (`drape.transfer_weights(..., under=...)`): each
  garment takes its weights from the nearest face of the skin *or of the garments already
  dressed* (not the free skirts), so the coat moves with the waistcoat and shirt under it.
  The crossbelts take the weights of the coat and facings under each vertex
  (`drape.inherit_weights`), no longer the bones' smooth weights.
- **Covered under-layers pushed in** (`drape.hide_under`, after `hide_skin`): waistcoat
  vertices under the coat or facings 6 mm towards their bone, the shirt 4 mm, the breeches
  under the waistcoat or gaiters 4 mm, as the skin under the clothes is pushed 12 mm.
- **Depth bias in the shader**: every layer draws `LAYER_BIAS` (1 mm) × its rank nearer the
  camera (`model.wgsl`, `material.cloth.z`), so a millimetre of crossing never flickers.
  The rank is the material extra `acr_layer` if present, else from the material key
  (`acr_render::layers::layer_rank`: skin 0, breeches 1, gaiters 2, shirt and stock 3,
  waistcoat 4, coat 5, facings and lining 6, belts and brass 7, cartridge box and knapsack
  8). The shadow pass shares `vs_main`: it moves by the same few mm along its own view
  direction, far below the shadow bias. WebGL2: a uniform, no new bindings.
- Ragdolls drive the same bones and weights, so the same holds for physics poses (the
  check covers the death and lying clips; a free ragdoll is not sampled).

**Check**: `cargo run --release -p acr-render --example model_layers -- <model.glb>...
[--steps 24] [--max-crossing N]` skins the model through every clip (24 frames each) and
lists, per layer pair, the inner vertices covered in the rest pose, how many come out
through the outer layer by more than 1 mm in some frame, and the deepest crossing with its
clip and frame (`acr_render::layers`). The test `belts_stay_on_the_coat` runs it on
`ACR_MODELS_DIR/engm18.glb` when that is set.

engm18, 24 frames × 19 clips (crossing vertices / worst):

| pair | before | after |
|---|---|---|
| coat → belts | 43 / 38 mm | 8 / 10 mm |
| waistcoat, facings, breeches → belts | 15 / 33 mm | 0 |
| waistcoat → coat | 49 / 39 mm | 54 / 24 mm |
| waistcoat → facings | 4 / 39 mm | 3 / 1.8 mm |
| stock → coat | 36 / 37 mm | 10 / 36 mm |
| skin → coat | 65 / 37 mm | 48 / 40 mm |
| breeches → coat (mostly the skirts over a striding or lying leg) | 218 / 40 mm | 223 / 40 mm |
| all pairs | 502 | 434 |

Left: the coat skirts over the legs (free cloth: the thigh share and the runtime cloth
handle them, see below), the stock under the waistcoat and coat collar in the death clips
(the neck is excluded from the push-in), the skin and waistcoat under the coat at the
armpits in raised-arm and crawl frames (DEATH, CRAWL2, ATTACK1).

## Pattern format

```python
Panel(name, garment, key, outline, edges, place, mirror=True, skirt=False, grain=(0, 1), tack=None)
Seam(a, b, garment, ease=0.15)
```

- `outline`: [(u, v)] in cm, counter-clockwise, v along the grain (up the body), u across.
- `edges`: {name: (i, j)}: the outline from vertex i forwards to vertex j.
- `place`: `region` (`torso`, `neck`, `arm`, `leg`), `theta` (degrees: torso/neck 0 = centre
  front, 90 = the wearer's left; arm 0 = top of the arm, 90 = front; leg 0 = front,
  90 = outside), `dir` (+1 when u runs towards increasing theta), `u0` (the u at theta),
  `at` (landmark: `waist`, `knee`, `ankle`, `wrist`, `collar`, `stock`), `lift` (cm, v = 0
  sits this far above the landmark), `gap` (cm off the skin), `pin` (an edge to pin).
- `key`: material key (`humans.LAYER_OF`, `outfit.PAINTER`); `skirt` marks free cloth;
  `tack` makes the panel an inlay of the named panel (drafted in the same frame).
- A seam side is a chain of `(panel, edge, reversed, side)`; both chains run the same way on
  the body. All-"L" seams are sewn on both sides; an "R" link means the mirrored copy
  (centre front and back seams), sewn once. `ease` is the allowed length mismatch (eased
  sleeve heads and shoulders); the tests check it. Seams of inlays are skipped by the drape.

## The British infantry uniform (1768 Royal Warrant)

After the reconstructed patterns (Hunter, Rogers):
- **Coat**: fitted body, fronts hooked together at the top of the chest, lapels to the waist
  in the facing colour with brass buttons (painted), the skirts cut separately at the
  waist, falling to the knee and cut away in front, their front corners turned back to show
  the white lining; two-piece sleeves (top and under sleeve) with deep round cuffs; a small
  stand collar.
- **Waistcoat**: sleeveless, to below the waist, buttoned down the front, the front corners
  cut away.
- **Breeches**: front and back per leg, a waistband, buttoned knee bands (the fall front is
  not cut: it would lie under the waistcoat).
- **Gaiters**: knee high, buttoned up the outside, the front dropping over the instep,
  gartered below the knee (pinned).
- **Shirt**: only the wristbands and the black stock.

The engm18 palette gives blue facings (a royal regiment), white turnbacks, white waistcoat
and breeches, black gaiters.

## Numbers (engm18 on the VM)

| garment | spacing | tris |
|---|---|---|
| breeches | 4.2 cm | 896 |
| gaiters, garter (inlay) | 3.5 cm | 586 + 48 |
| waistcoat (under the coat) | 5.0 cm | 442 |
| coat (body, facings, skirts, turnbacks; hem rims) | 3.5 cm | 1399 + 523 + 1080 + 140 |
| buttons (brass, pewter) | – | 120 + 78 |
| shirt, stock | 2.0 cm | 88 + 104 |

The whole model is 11.7k triangles (12.0k before the coarser waistcoat and breeches; the shell version 8.0k). The build takes about
3 minutes, most of it outside the drape. Checked: the STAND views, walk, present, reload,
crawl, death and lying clips (sheets), the looks bench and scripted hits in the game
(musket, cannonball, sabre to the arm, shell, a leg shot off: the ragdolls fall cleanly
with the coat).

Fast iteration: `ACR-analysis/clothing/harness/run.sh <worktree> <tag> [nosim]` runs
`engm18.make` with the garments on the VM and renders front/side/back/three-quarter views
coloured by material key in about 15 s.

## Adding a garment or a nation

- A garment: a drafting function in `garments.py` returning `(panels, seams)`, registered in
  `GARMENTS`, its ease in `EASE`, its cloth in `CLOTH`, its place in `ORDER` (inner garments
  first), a `SPACING` in `drape.py`; new material keys in `humans.LAYER_OF` and
  `outfit.PAINTER` and a colour in the unit's palette. Extend `tests/test_garments.py`.
- A nation: mostly colours (facings, lining, waistcoat) in the unit's palette; a different
  cut (French coats without lapels, Spanish cuffs, a waistcoat with sleeves) is a variant
  of the drafting function selected by an argument.
- A unit: `drape.dress(ch, garments=...)` in its `make` behind its own `build["garments"]`
  switch, shoes from `mbody.shoes`, and the crossbelts over `drape.surface(...)` as in
  `engm18.make`.

## Materials

Each atlas painter has a finish (`outfit.FINISH`: roughness, sheen, gloss), written to the
material extras `acr_roughness`, `acr_sheen`, `acr_gloss`. The game (`model.wgsl`,
`Material.cloth`) adds the sheen at grazing angles only (a soft pale rim where the cloth
turns away, nothing face on) and the gloss as a highlight; the Blender previews use the
Principled roughness, sheen and specular (`decay.export_material`). Broadcloth (coat,
facings, lining) is matte with the strongest sheen and a fine felted nap, no painted
highlights; kersey (waistcoat, breeches) flatter; linen (shirt, stock) slubbed with almost
no sheen; the gaiters black cloth, not leather; shoes blacked leather with a dull gloss;
brass and pewter buttons glossy.

Geometry: brass buttons on the lapels (7 a side) and the cuffs (3 up the back), pewter
ones down the outside of the gaiters (`drape.buttons`: 6-triangle domes cast onto the
draped cloth, skinned like the cloth under them); the garter is a black leather inlay
strap round the gaiter top; the coat skirts, turnbacks and facings have a 5–6 mm rim
along their cut edges, turned in (`drape.hem`), so the hems and cuffs show their edge.

## Runtime cloth

`acr_physics::Cloth` (`crates/acr-physics/src/cloth.rs`) is a light XPBD cloth: particles
with skinned targets and a freedom (`_CLOTH`), a damped drive back to the drape that
weakens towards the hem, distance constraints down the skirt (rigid) and across it
(one-sided, soft), bending as distances two apart, capsules, the ground, quadratic air
drag towards the wind; pinned particles move to their targets over the substeps. Tests:
a man standing keeps his drape, walking swings the hem a little and running makes it
flap, a fallen man's skirts drape to the ground, the legs push the skirt out on the
target's side, wind lifts the hem.

In the game (`ingame/cloth_view.rs`) the proxy is the skirt rig: 4 columns (`skirt_fl`,
`bl`, `br`, `fr`) × waist, knee and hem, open in front. Targets come from the palette (clip,
gait or ragdoll) blended with the thigh's corrective share (where the skirt is drawn),
smoothed over a tick (units move once a tick), in physics space so motion gives inertia;
the shins collide as capsules, the terrain as ground; upright the drive is firm, lying
down it lets go. The cloth's offset from its target turns the two skirt bones of each
column about their heads. Budget: the 24 men nearest the camera focus within 900 px.

Switched on for the dying and the dead on a ragdoll (the skirts hang and drape instead
of sticking out rigidly with the hips); `ACR_CLOTH=all` also simulates men on their feet,
`ACR_NO_CLOTH` switches it off. Not yet for walkers: in the game the near skirt of a
walker comes out lifted (the offline probe on the walk clip keeps the hems within a few
cm, the game does not; not found yet), and thigh capsules pushed the hems through to the
wrong side of a leg swinging past them, so the thigh share does the thighs.

Bench: `ACR-analysis/clothing/cloth/cloth_bench.py` (a walker and a man shot dead, close
up from the side, frames `walk*`, `fall*`; `ACR_NO_CLOTH=1` for the skinned skirts).

## Known problems

- Not every sewn pair meets within 5 mm: in the coat 166 of 251 do, 242 within 4.5 cm
  (welded), 9 stay open. Worst: the sleeves' front and back seams (9 and 8 cm) and the back
  armhole (8 cm), the side seam at the waist (7 cm), the hook at the top of the chest. The
  sleeves are still placed round a straight arm axis and the side pleats are not modelled.
- The shoulders and the upper back are crumpled and a little puffed.
- The gaiters take their colour from the sprites (brown-black), not the palette's black.
- No per-frame push-out of the layers (see "Layers": weights, push-in and a depth bias
  instead); the runtime cloth only for ragdolls (above).
