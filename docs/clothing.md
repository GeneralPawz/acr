# Clothing: sewing patterns draped by cloth simulation

Today's clothes are shells cut from the body triangles (`body.derive`, `Character.shell`),
which is why hems are jagged and the coat reads as patches. The replacement cuts each
garment from a historical 2D pattern, drapes it on the MakeHuman body with Blender's cloth
simulation on the render VM, and skins the result to skeleton H. Loose parts (coat skirts,
later straps and coat tails) keep their own bones and a `_cloth` weight so that the game's
XPBD engine can move them later.

Status (first slice, branch `m2-clothing`): the pipeline runs end to end in a normal
`cargo xtask assets engm18` build, but the drape is **not yet convincing** (open seams,
stretched spikes, garments poking through each other; see "Known problems"). It is
switched off: `engm18.BUILD["garments"]` is `False`, and the body-derived shells remain
the default.

| module (`assets/blender/lib/acr_assets/`) | |
|---|---|
| `garments.py` | patterns as data (pure Python, unit-tested in `tests/test_garments.py`): panels, seams, placement, cloth settings, drafting to measure |
| `drape.py` | needs `bpy`: measure the body, mesh and place the panels, sew, simulate, weld, solidify, skin, split into export layers, push the covered skin in |

## Pipeline

1. **Measure** (`drape.Body`): girths (chest, waist, seat, neck, upper arm, wrist, thigh,
   knee, calf, ankle) from slices of the rest-pose skin, lengths from the rig (nape to
   waist, arm, waist to knee, knee to ankle). The slices of the decimated hull are noisy, so
   each figure is kept within 0.85..1.2 of the default in `garments.MEASURE`.
2. **Draft** (`garments.uniform(m)`): every panel is drafted from the measurements plus
   the garment's ease (`garments.EASE`), so the same draft fits every build.
3. **Mesh** (`drape._mesh2d`): the outline is sampled every `drape.SPACING[garment]` cm
   (corners kept), interior points on a triangular lattice, constrained Delaunay
   (`mathutils.geometry.delaunay_2d_cdt`).
4. **Place** (`drape.place`): each panel is wrapped, arc length preserved, on a cylinder
   around its region (an elliptical one for the torso and neck, round ones for the arm and
   leg), `gap` cm off the skin, faces turned outwards. Left panels are drafted; the right
   ones are mirrored copies.
5. **Sew**: the two sides of each seam are paired by arc-length fraction; the pairs become
   loose edges, which Blender's cloth treats as sewing springs (`use_sewing_springs`).
6. **Simulate** (`drape.simulate`): one cloth object per garment in `garments.ORDER`
   (breeches, gaiters, shirt, waistcoat, coat), colliding with the skin, the shoes and the
   garments draped before; `shrink_min` tightens it to fit. Settings per cloth in
   `garments.CLOTH` (mass, tension, bending, shrink, thickness). The shirt pieces are not
   simulated.
7. **Bake and clean**: the evaluated mesh replaces the cloth; sewn pairs closer than 2 cm
   are welded, other sewing edges dropped; `solidify` gives the cloth its thickness with
   rims along the cut edges (the pattern outline).
8. **Skin and split** (`drape.finish`): weights interpolated from the nearest face of the
   skin; skirt panels (coat skirts, turnbacks) take the skirt bones of
   `rig.SKIRT_COLUMNS` instead. `part` comes from the dominant bone and the mesh is split
   at the part borders, as the shells were, so `_PART`, `_LIMB`, the decay masks, morphs
   and cuts all work through the normal `Character` finishing (docs/assets-humans.md,
   docs/damage-model.md).
9. **One object per material key** and skirt flag: `coat`, `facings` (lapels, cuffs,
   collar), `coat_skirts`, `turnbacks` (white lining), `waistcoat`, `breeches`, `gaiters`,
   `shirt`, `stock`. The atlas painter (`outfit.dress`) now gives each painter and colour
   its own atlas cell, so a coat and its facings can differ.
10. **Skin under the clothes** (`drape.hide_skin`): skin vertices whose outward ray hits a
    garment within 6 cm are pushed 12 mm towards their bone. They stay (decay and cuts need
    them) but cannot poke through.

Crossbelts are laid over a joined surface of the draped garments instead of the skin.

## Pattern format

```python
Panel(name, garment, key, outline, edges, place, mirror=True, skirt=False, grain=(0, 1))
Seam(a, b, garment, ease=0.15)
```

- `outline`: [(u, v)] in cm, counter-clockwise, v along the grain (up the body), u across.
- `edges`: {name: (i, j)}: the outline from vertex i forwards to vertex j.
- `place`: `region` (`torso`, `neck`, `arm`, `leg`), `theta` (degrees: torso/neck 0 = centre
  front, 90 = the wearer's left; arm 0 = top of the arm, 90 = front; leg 0 = front,
  90 = outside), `dir` (+1 when u runs towards increasing theta), `u0` (the u at theta),
  `at` (landmark: `waist`, `knee`, `ankle`, `wrist`, `collar`, `stock`), `lift` (cm, v = 0
  sits this far above the landmark), `gap` (cm off the skin), `layer`.
- `key`: material key (`humans.LAYER_OF`, `outfit.PAINTER`); `skirt` marks free cloth.
- A seam side is a chain of `(panel, edge, reversed, side)`; both chains run the same way
  on the body. All-"L" seams are sewn on both sides; an "R" link means the mirrored copy
  (centre front and back seams), sewn once. `ease` is the allowed length mismatch (gathered
  sleeve heads, eased shoulders); the tests check it.

## The British infantry uniform (1768 Royal Warrant)

After the reconstructed patterns (Hunter, Rogers): a fitted cutaway coat with lapels to the
waist in the facing colour, the skirts cut separately at the waist and falling to the knee,
their front corners turned back to show the white lining; two-piece sleeves (top and under
sleeve) with deep round cuffs; a small stand collar; a sleeveless waistcoat to below the
waist, buttoned down the front; breeches with a front and back per leg, a fall front, a
waistband and knee bands; knee-high gaiters buttoned up the outside, dropping over the
instep; of the shirt only the wristbands and the black stock. The engm18 palette gives blue
facings (a royal regiment), white turnbacks, white waistcoat and breeches, black gaiters.

## Adding a garment or a nation

- A garment: a drafting function in `garments.py` returning `(panels, seams)`, registered
  in `GARMENTS`, its ease in `EASE`, its cloth in `CLOTH`, its place in `ORDER` (inner
  garments first), a `SPACING` in `drape.py`; new material keys in `humans.LAYER_OF` and
  `outfit.PAINTER` and a colour in the unit's palette. Extend `tests/test_garments.py`.
- A nation: mostly colours (facings, lining, waistcoat) in the unit's palette; a different
  cut (French coats without lapels, Spanish cuffs, the waistcoat with sleeves) is a variant
  of the drafting function selected by an argument.
- A unit: `drape.dress(ch, garments=...)` in its `make`, behind its own `build["garments"]`
  switch, and the crossbelts over `drape.surface(...)` as in `engm18.make`.

## Runtime cloth (planned)

The coat skirts and turnbacks are separate objects (`<unit>.coat_skirts`,
`<unit>.turnbacks`) weighted to the skirt bones, with the FLOAT attribute `_cloth`
(exported as `_CLOTH`): 0 at the waist seam (follows the skin), 1 at the hem (free). The
XPBD engine can pin the vertices with `_cloth` = 0 to the skinned pose and simulate the rest
against capsules of the leg bones; until it does, the `skirt_*` bones of the clips move the
skirts. Straps and coat tails can get the same attribute.

## Known problems (next slice)

Sheets of the first drape: `ACR-analysis/clothing/sheet-v1-views.png` and
`sheet-v2-views.png` (front, side, back, three-quarter, STAND).

- Seams do not close: only about 40 % of the coat's sewn pairs meet within 2 cm after 36
  frames; the rest are dropped, so panels hang apart and stretched triangles show as
  spikes (longest edge 0.2 m). Stronger and ramped sewing (`sewing_force_max`, gravity off
  until the seams close), longer runs, and placing the panels closer to their seam partners
  (shoulder seams over the shoulder line, not on a vertical cylinder) are the next steps.
- The measured girths are noisy (chest 83 cm, waist 71 cm after clamping): measure on the
  full-resolution MakeHuman body (`mbody.repose`) instead of the decimated hull.
- Layers interpenetrate after skinning (coat skirts on skirt bones vs breeches on the
  thighs): drape with a collision margin per layer and check in the clip poses.
- No hem fold yet beyond the solidify rim; buttons and lace only as painted texture.
- Not yet checked: the clips (walk, drill, crawl, death), the in-game ragdoll and the looks
  bench, because the drape itself is not yet good enough.
