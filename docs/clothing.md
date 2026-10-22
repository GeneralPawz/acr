# Clothing: sewing patterns draped by cloth simulation

The body-derived shells (`body.derive`, `Character.shell`) gave jagged hems and a coat of
patches. Garments are now cut from historical 2D patterns, draped on the MakeHuman body with
Blender's cloth simulation (in the normal asset build, on the render VM), and skinned to
skeleton H. Loose parts (the coat skirts) keep their own bones and a `_cloth` weight so that
the game's XPBD engine can move them later.

`engm18.BUILD["garments"]` switches the musketeer between the draped uniform (`True`) and
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
   its own margin). Pinned edges (`place["pin"]`: the gaiters' garters) stay where they
   were placed. The shirt pieces (wristbands, stock) are not simulated.
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

| garment | sim | sewn pairs welded (< 4.5 cm) | within 5 mm | tris |
|---|---|---|---|---|
| breeches | 1.6 s | 138 / 140 | 89 | 1374 |
| gaiters | 1.2 s | 28 / 28 | 7 | 798 |
| waistcoat | 1.5 s | 59 / 60 | 44 | 910 |
| coat (body, facings, skirts, turnbacks) | 3.7 s | 244 / 251 | 157 | 1389 + 256 + 958 + 60 |
| shirt, stock | – | 6 / 6 | 2 | 88 + 104 |

The whole model is 12.1k triangles (the shell version 8.0k). The build takes about
3 minutes, most of it outside the drape.

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

## Runtime cloth (planned)

The coat skirts and turnbacks are separate objects (`<unit>.coat_skirts`,
`<unit>.turnbacks`) weighted to the skirt bones (plus the corrective thigh share), with the
FLOAT attribute `_cloth` (exported as `_CLOTH`): 0 at the waist seam (follows the skin), 1
at the hem (free). The XPBD engine can pin the vertices with `_cloth` = 0 to the skinned
pose and simulate the rest against capsules of the leg bones; until it does, the `skirt_*`
bones of the clips and the thigh share move the skirts. Straps and coat tails can get the
same attribute.

## Known problems

- Not every sewn pair meets within 5 mm: in the coat 157 of 251 do, 244 within 4.5 cm
  (welded), 7 stay open (worst: the side seam at the waist where the back narrows into the
  pleats, 10 cm; the sleeves' front seam near the armpit, where the arm is close to the
  body; the back armhole; the collar). A stronger (100) or longer (60 frames) sewing phase
  did not help; the sleeve placement round a straight arm axis and the side pleats (not
  modelled) are the likely causes.
- The shoulders are slightly puffed (the fold placement leaves the cloth a little high).
- The gaiters are bulky with folds at the garter.
- No hem fold; buttons and lace only as painted texture.
- The layers are kept apart by margins at rest and by the corrective skirt weights in the
  clips; there is no per-frame push-out.
