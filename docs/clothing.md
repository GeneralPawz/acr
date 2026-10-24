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
5. **Sew**: the two sides of each seam are paired by arc-length fraction along the pattern
   (`garments.seam_fractions`; until #287 it counted samples, which skewed the pairs of
   edges cut into different numbers of samples and opened skin holes at the shins of the
   stockings and leggings); the pairs become loose edges, which Blender's cloth treats as
   sewing springs.
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
  (`acr_render::layers::layer_rank`: skin 0, legwear 1, gaiters, boots, breechcloths and
  kilts 2, shirts 3, waistcoats, doublets and quilted jackets 4, coats, cloaks, tunics and
  armour 5, facings, lining, sashes, bands and mantles 6, belts, brass and cords 7, boxes,
  pouches, horns and scabbards 8). Every cloth key of every family has a rank or is listed
  in `NOT_LAYERS` (hair, feathers, jewellery, weapons, hats); until #287's trisoup pass
  about 70 keys of the 17th-century, frontier, native, meso and Inca families had none, so
  they drew without a bias and the check skipped them. `garments.LAYER_RANK` is the same
  table for the build; `tests/test_garments.py` (`LayerRanks`) fails when the two differ or
  a registered cloth key has no rank. The shadow pass shares `vs_main`: it moves by the same few mm along its own view
  direction, far below the shadow bias. WebGL2: a uniform, no new bindings.
- Ragdolls drive the same bones and weights, so the same holds for physics poses (the
  check covers the death and lying clips; a free ragdoll is not sampled).

**Check**: `cargo run --release -p acr-render --example model_layers -- <model.glb>...
[--steps 24] [--max-crossing N]` skins the model through every clip (24 frames each) and
lists, per layer pair, the inner vertices covered in the rest pose, how many come out
through the outer layer by more than 1 mm in some frame, and the deepest crossing with its
clip and frame (`acr_render::layers`). The test `belts_stay_on_the_coat` runs it on
`ACR_MODELS_DIR/engm18.glb` when that is set. It also counts **poking** vertices: inner
vertices that lie 1–12 mm *above* an outer triangle already in the rest pose (`POKE`; the
depth bias cannot hide them, and the frame check skipped them). The last lines are one
`summary <glb> poking N crossing M` per model; `--max-poking N` and `--max-crossing N` make
it fail, so CI or a family's rebuild can gate on it (all 104 humans take 12 s at
`--steps 8`).

**Trisoup survey** (#287, all 104 human glbs of the showcase, `--steps 8`, output in
`ACR-analysis/trisoup/before/`): every unit has 50–420 poking and 20–550 crossing
vertices. Worst pairs, summed over the units (poking / crossing): skin under breeches
2209 / 2524, coat at the facings 1225 / 440 (mostly the inlay borders and hem rims, not
visible), skin under the coat 916 / 2189, waistcoat under the coat 866 / 2154, breechcloth
under a native's shirt 644 / 218, breeches under the coat skirts 174 / 4193. The patches
in the sheets: FrnBuk's shirt through the cloak 125 / 148, EngOf17's buff coat over the
falling band 45 / 22 and the band over the brass 115 / 19, EngMus' doublet over the band
76 / 32, SpnS15's doublet under the armour 47 / 169, DelKri's breechcloth through the shirt
140 / 37 and skin through the shirt 82 / 86.

**`drape.settle`** (all families, before `Character.morphs`; **off**, `humans.SETTLE` /
`ACR_SETTLE=1` in a local Blender, a constant for the VM). Since #287's second trisoup pass:
first every draped cloth polygon whose winding faces the nearest bone is turned outwards
(`drape.orient_outwards`; the mirrored right panels, capes, flaps and the falling band came
out inwards, 8–300 polygons per garment; gear is left alone, its closed boxes face the body).
Then, top layer first, every vertex of a ranked layer finds the nearest triangle of each
ranked layer above it (`brass` excluded: buttons are domes, not a layer) and, where it lies
over that triangle up to 15 mm above it or less than 4 mm below it, moves along that
triangle's outward normal to 4 mm below it (the same measure as the check); unmoved
neighbours follow half the mean move of their moved ones; 3 passes. An outer surface within
1.5 mm is the same cloth (an inlay's border) and is left alone, and so are the free skirts and
the skin's head, neck, hands and feet.

**Second trisoup pass** (#287; sample frnbuk, engof17, engmus, spns15, delkri, engm18,
frnm18, irolut, mayvoi; `model_layers --steps 8`; tables and renders in
`ACR-analysis/trisoup2/`: `layers_before.txt`, `layers_v1.txt` normal settle alone,
`layers_v2.txt` with the polygons turned outwards; close-ups on the blank field, white, HUD
off, `ACR_LOD=0`, in `before/`, `v1/`, `after/`, sheet `sheet_before_v2.png`):

| unit | poking before | v1 | v2 | crossing before | v2 |
|---|---|---|---|---|---|
| frnbuk | 234 | 223 | 163 | 266 | 362 |
| engof17 | 375 | 472 | 268 | 368 | 395 |
| engmus | 359 | 186 | 173 | 186 | 252 |
| spns15 | 226 | 235 | 196 | 408 | 492 |
| delkri | 332 | 244 | 152 | 224 | 395 |
| engm18 | 157 | 145 | 95 | 400 | 463 |
| frnm18 | 164 | 169 | 90 | 398 | 484 |
| irolut | 121 | 93 | 54 | 59 | 110 |
| mayvoi | 66 | 63 | 116 | 22 | 28 |

- The check now also counts, per pair, the pokes **against**: the inner vertex's winding
  normal points against the outer triangle's. Before the pass that was 1060 of the 2034
  pokes: about half the counted pokes are a winding turned inwards (the inner vertex lies
  *under* the cloth, the check reads it as above), not a patch on the screen. Turning the
  polygons outwards is most of the v2 drop; crossing rose because more vertices count as
  covered once the windings agree. mayvoi rose: its sandals are closed, the bone test turned
  their soles.
- **Renders**: the patches are real (FrnBuk's shirt in white sawteeth along the cloak's
  shoulders and hem, EngOf17's band through the buff coat at the collar and back, DelKri's
  shirt collar against the neck skin, EngMus' band). Neither v1 nor v2 changes them visibly;
  v2 opens new white slivers on EngOf17's back and side (the fallband's polygons, 271 of
  them, turned by the bone test, and the buff pushed under it), and FrnBuk's cloak sinks
  under the shoulder belt on the back (belts rank 7 over the cloak's 5; a buccaneer's belt
  is under his cloak). Hence still off.
- What the renders show is mostly the **edges** of the outer layer: the sawteeth run along
  the cloak's and the buff coat's borders, where an inner vertex is beside the outer
  triangle rather than over it (the check and the settle both skip those), and the inner
  layer is 1–3 cm out, beyond the 15 mm reach. Next: settle against the outer layer's
  border (extend the outer edge's triangles a few cm for the nearest search, or pull the
  inner layer in under a band along every outer boundary), ranks per unit where a garment
  is worn over the belts (cloaks, mantles), and the orientation by a flood fill over the
  connected cloth from a seed known to face out instead of per polygon.
- **Deleting covered inner faces** (not done): cuts, wounds and decay show the layer under a
  hole, so a face may only go where an outer layer *and* the skin under it stay in every
  clip and no wound region reaches; that needs the per-clip coverage of the check exported
  back to Blender. Pushing the inner faces in keeps them for the wounds; deleting is for
  later, and only far from openings (collar, cuffs, front, hems, cloak edges).
- **Atlas gutters** (`outfit.atlas_uv`, on): the islands of a painter's cell keep
  `outfit.GUTTER` (4 px) apart and from the cell border (Smart UV's margin is now
  `2 × 4 px / cell px`, at least the old 0.006, at most 0.05), so mips to level 2 and the
  bake's 6 px edge dilation no longer reach the neighbour island; one-object cells go the
  same way (no longer `coat.atlas_uv`). Built into the sample above; not separately
  measured on renders.
- **Gate**: `layers::tests::settled_units_poke_little` (with `ACR_MODELS_DIR`) fails when a
  sample unit pokes more than 400 vertices in the rest pose: a ratchet at the shipped level,
  to be lowered as the patches go.

**Third trisoup pass** (#287; built and rendered on frnbuk and engof17 only, the VM takes
about 4–5 min a unit; `model_layers --steps 8`; renders one blank field per driver process,
white, HUD off, `ACR_LOD=0`; everything in `ACR-analysis/trisoup3/`: `before/`, `v1/`
order and cull, `after/` order, cull and edge relax, `sheet_v1.png`, `sheet_before_after.png`,
`build*.log`, `tools/`). Three new steps in `Character.morphs`, all **off**
(`drape.ORDER`, `drape.CULL`, `drape.RELAX`, constants for the VM):

- **Per-unit layer order** (`drape.layer_order`): for every pair of layers that lie on each
  other, about 400 faces of each cast ±3 cm along their outward normal for the other; the
  majority says which is outside. `OVER` keys (falling band, band, lapels, sashes, capes and
  mantles, lace) win against cloth and belts, the skin is under all, brass keeps its base
  rank, gear boxes go by the votes. Pairs are kept strongest first (one that closes a cycle
  is dropped); a key's rank is the longest chain under it. It goes to the materials as
  `acr_layer` (`Character.colours`, `outfit.dress`), which the shader's bias already read;
  `acr_render::layers` now reads it too (else the key's rank). FrnBuk: belts 3 under the
  cloak 4, shirt 2; EngOf17: band 6 over buff 5 (as before: the band was never ranked
  under the coat; the coat pokes through it by more than the bias).
- **Hidden faces deleted** (`drape.cull_hidden`): a garment face (not the skin, which
  wounds and decay need; not the free skirts) goes when its corners, centre and a 1.5 cm
  ring round it all find a higher-ranked layer along the face's outward normal, from 2.5 cm
  under it (a layer poking through covers it) to 5 cm over it, turned the same way (not a
  sleeve's inside seen from the torso). Rest pose only; the covering layers carry the
  weights of the layer under them. EngOf17 11348 → 10264 triangles (mostly breeches and
  hose under the buff coat and boots), FrnBuk 9807 → 9330 (397 shirt faces under the cloak).
  A single normal ray per sample instead of a hemisphere: a hemisphere from a face that
  pokes through escapes and keeps it.
- **Free edges relaxed** (`drape.relax_edges`): ear triangles (two free edges) sharper than
  22° deleted, then 10 Taubin passes along every free-edge loop, inner neighbours following
  half the move, shape keys moved alike. No resample to 1.5 cm and no turned hem strip yet.

| unit | poking before | order+cull | +relax | crossing before | order+cull | +relax |
|---|---|---|---|---|---|---|
| engof17 | 375 | 304 | 432 | 368 | 349 | 388 |
| frnbuk | 234 | 179 | 222 | 266 | 253 | 256 |

- **Renders**: order and cull change nothing visible (`sheet_v1.png`): the deleted faces were
  hidden anyway. The sawteeth are the **outer** layer's own ragged border and its spikes,
  not an inner layer showing: the falling band's lower edge on EngOf17's back is a row of
  long thin triangles (not ears, so the relax keeps them), the buff coat pokes through the
  band from below, FrnBuk's cloak ends in torn red points over the shirt, and the belt
  vanishes under a fold of the shirt that lies outside it. With the relax FrnBuk's cloak
  hem on the back is visibly smoother, but black slivers open at EngOf17's armholes and
  back and white gaps on FrnBuk's side (free edges that are seams to a neighbour object
  move apart), and poking rises over the 400 gate on EngOf17. Hence all three stay off;
  no wound or decay check was rendered, as nothing is deleted by default.
- Next: smooth only the free edges that are no seam (match coincident boundary vertices
  across objects first and move them together), resample the band's and the cloak's hem
  with a turned strip, and pull an inner layer under an outer *border* rather than under
  its interior.

**Fourth trisoup pass** (#287; everything in `ACR-analysis/trisoup4/`). The garment meshes
of FrnBuk and EngOf17 were dumped after every stage (`tools/stages.py`, a local Blender:
place, sim frame 45 and 75, weld, finish, the shell cut and its border smoothing, the band,
the final mesh) and rendered alone, flat with wireframe, front and back
(`sheet_stages_engof17.png`, `sheet_stages_frnbuk_cloak.png`). The draped buff coat is clean
through every stage: **the sim and the weld make no spikes**. The spikes are made by two
generators:

- **The falling band** (`drape.falling_band`, EngOf17, EngMus, EngB17 and every unit with the
  band): each vertex is cast straight down onto the cloth; past the edge of the shoulder or
  the back the ray found the cloth up to 35 cm lower, a row of long thin spikes. Now
  (`drape.BAND_CLEAN`, **on**) a ring drops at most `BAND_SLOPE` (0.8) × its radial step
  below the ring inside it, and the rings are smoothed round the neck (`BAND_SMOOTH`, 4
  passes) without going under the cloth. In game (`sheet_before_after.png`, `before/`,
  `after/`) the band is a smooth collar on the chest and back of all three; the front tips
  still stand off the shoulders a little from the side. `model_layers --steps 8`
  (`layers_before.txt`, `layers_after.txt`): poking / crossing engof17 375 / 368 → 382 / 353,
  engmus 405 / 196 → 248 / 158, engb17 299 / 157 → 196 / 117; the buff coat through the
  band, worst 33.5 → 4.2 mm.
- **Shells** (`bd.derive`, FrnBuk's cloak, blouse, sash, and every family's shells): the cut
  takes whole faces of the decimated hull, so its border is a row of triangle teeth, and
  172 of the cloak's 199 border edges run along the hull's part borders, which
  `smooth_border` keeps fixed as seams; the smoothing never touched them. Tried
  (`body.CLEAN_CUT`, **off**): teeth and notches of the face set removed, a part border
  counted as a seam only where the face across it is kept too, border moves capped at 2 cm.
  The cloak's back hem comes out smoother (`sheet_v3.png`), but the neck hole and the
  blouse's top grow new spikes (the border is put back onto the nearest skin, near the neck
  that is the chin) and the sash loses its teeth with its faces. Next: cut shells along a
  smoothed per-vertex field (an isoline through the triangles) instead of whole faces, and
  lift the border onto the skin of its own part only.

**Fifth trisoup pass: shells cut along a contour** (#287; everything in
`ACR-analysis/trisoup5/`). Every `ch.shell` garment goes through `body.derive` (with the
MakeHuman body): cavalry (boots, breechcloth, breeches, buffcoat, coat, cuffs, cuff_lace,
cuirass, lapels, lapel_lace, leggings, sash, shirt, sleeves, vambraces, waistcoat), civil
(blouse, cappa, habit, habit_legs, hood, sash), frontier (blouse, cape, cloak, cuirass,
fringe, sash, slops, tassets), guncrew and line (breeches, coat, cuffs, gaiters,
waistcoat), inca (hair, quilt, unku, yacolla), meso (ichcahuipilli, maxtlatl, maya_jacket,
sandals, tilmatli, tlahuiztli), native (armbands, belt, breechcloth, cloak, garters,
leggings, matchcoat, shirt), plains (shoulder_strips, sleeve_strips, war_shirt), pueblo
(bandolier, blanket, jerkin, sash, tunic, wraps). Now (`body.CONTOUR`, **on**; `CLEAN_CUT`
removed):

- **The field:** `keep_face` still picks faces, but the cut follows `selection_field`: per
  vertex the area-weighted share of kept faces round it (welded by position over the
  hull's part seams, so both sides of a seam get the same value), smoothed 3 times
  (`CONTOUR_PASSES`). Values within 0.1 of 0.5 snap onto the line (no slivers).
- **The cut** (`contour_cut`): every edge the 0.5 isoline crosses is split where it crosses
  (weights and rest normals interpolated, home and part from the nearer end; the twin
  edges on a part seam are split alike, so seams stay closed), the two points are joined
  through the triangle, the outer side deleted. The border is a curve through the
  triangles, not a staircase of faces. If the smoothing eats a narrow diagonal selection
  (less than 70 % of the picked area left: a bandolier, strips of quills) it is redone with
  1 pass, then 0.
- **Strips** (kept faces within `STRIP_HEIGHT` = 16 cm of height: sashes, belts, garters,
  armbands, the fringe): the field is a ramp in z through the band's top and bottom (bounded
  sideways by the smoothed selection), so both edges are level lines through the
  triangles: a clean ribbon round the body instead of a row of teeth. (Not a separate
  ribbon mesh along a curve: the level cut gives the same edge and keeps the skin's weights.)
- **Offset and border:** the cut shell is pushed out by its offset along its own
  interpolated rest normals; the outer border (boundary edges without a twin across a part
  seam) is relaxed along its curve 4 times, each vertex at most 1.5 cm (`BORDER_MOVE`),
  and **never projected onto the skin** (round 4 put the neck hole onto the chin).
- **Hem** (`hem_border`): the outer border is extruded 0.7 × the offset back towards the
  skin along the shell's own normals, so the edge reads as cloth or plate.

Stage renders (`tools/stages.py` with `ACR_CONTOUR=0/1`, `stages_0/`, `stages_1/`; the
native units do not build in the local stage tool, `uv.smart_project` needs a context):
`sheet_stages.png` (FrnBuk, SpnSha, IncSha, MaySha, SpnS15, PueSha; before row over after
row). The teeth are gone from FrnBuk's cloak, blouse and sash (the sash is now a level band;
before it had flaps up the sides), IncSha's unku and yacolla, MaySha's tilmatli, PueSha's
bandolier; SpnSha's hood is one collar instead of loose shards. SpnS15's cuirass and slops
are not shells (unchanged). In game (`before/`, `after/`, `sheet_before_after.png`: FrnBuk,
SpnSha, IncSha, MaySha, SpnS15, DelKri, IroLut, blank field, white, HUD off, `ACR_LOD=0`)
the change is small at this distance: the edges are slightly cleaner, but **the big teeth
that remain** (the white zig-zag under FrnBuk's red cape and at its hem, the white spikes at
the back of SpnSha's cappa) look the same before and after, so they are not the shell cut
(next: render those meshes alone; the FrnBuk ones may be the fringe/cape, the SpnSha ones
the habit through the cappa). `model_layers --steps 8` (poking / crossing, before → after):
delkri 332 / 224 → 396 / 229, frnbuk 247 / 273 → 315 / 305, incsha 156 / 61 → 217 / 106,
irolut 121 / 59 → 106 / 71, maysha 67 / 26 → 62 / 19, spns15 226 / 408 → 234 / 421,
spnsha 77 / 216 → 111 / 202: more poking on most, probably the hem (now pushed along the
shell's normals, not put back onto the nearest skin) and the 0.5 isoline lying half a
triangle outside the old face border; not yet checked pair by pair. Only MaySha (better on
both counts) was copied to the showcase.

**Sixth trisoup pass: the shoulder teeth are not a seam crack** (#287; diagnosis only, no
code change; tools and crops in `ACR-analysis/trisoup6/`, `sheet_shoulder_diagnosis.png`).
Hypothesis tested: shells offset along per-part normals, so the twins of a part seam
(trunk | upper arm) drift apart and the layer below shows through. It does not hold:
- `rest_normal` is computed on the hull before `split_parts`, so seam twins already share
  one normal and are pushed identically. After `derive` every seam twin of every shell of
  DelKri, SpnSha, MaySha, IncSha and FrnBuk is at exactly the same place (0 near-twins
  within 3 mm on the shirt, cappa, habit, tilmatli, yacolla, unku, cloak; 1–4 at 2–3 mm on
  habit_legs, hood and sash, none at the shoulder). The skinning keeps them together too:
  in the exported glbs (DelKri, STAND) no twin pair of any mesh is more than 2 mm apart.
- Offset fold-over is not it either: pushing the flesh out 2 / 4 / 6.5 cm along the rest
  normals turns over only 2 / 8 / 22 faces in the shoulder zone (z 1.25–1.55, head part
  excluded); the turned faces are at the hands, crotch and the head.
- The hem is not it: `HEM = 0` leaves the shards unchanged.
What the teeth are: **the shell's top border running across the trapezius**. The decimated
MakeHuman shoulder line is a fan of long sliver triangles in which torso (clavicle) and
upper-arm faces interleave (top view of the flesh coloured by part); the neck opening of
every torso shell (the head part removed, plus the z limits: DelKri's shirt `c.z < 1.50`,
the habit / cappa / yacolla / tilmatli borders) crosses that fan, and the isoline cut
through the slivers leaves thin shards of cloth standing over the shoulder with skin (or
the inner layer) between them. Seen from the back in game they are the dark spikes on
DelKri's shirt and the white spikes through SpnSha's cappa. Side finding: 195–246 flesh
vertices (armpits, hands, crotch) have a `rest_normal` opposite to the geometric normal
(the bone-side flip in `hull` misfires where the home is the upper arm or a finger), so
shells are pushed *into* the body there. Next: re-mesh the shoulder line of the MakeHuman
body before the weights (no slivers across the trapezius), or keep the trapezius whole in
every torso shell (cut the neck opening by distance from the neck bone, not by part / z),
and drop the bone-side flip of the rest normals for a closed hull.

**Seventh trisoup pass: shoulder line, neck opening by distance, outward rest normals**
(#287; built on the VM for DelKri, SpnSha, IncSha, MaySha, FrnBuk, IroLut, EngKri, EngM18;
everything in `ACR-analysis/trisoup7/`: `sheet_before_after.png`, `before/`, `after/`,
`glb_after/`, `layers_{before,after}.txt`, `sh/` stage crops, `tools/`). Three steps in
`body.py`, all **off** by default (`SHOULDER_REMESH`, `NECK_CUT`, `HULL_WINDING`; the first two
on since the eighth pass):
- `remesh_shoulders` (called from `Character.body` after `mbody.build`, before the
  weights): the faces between 12 cm below the shoulder joint and 8 cm above the neck's root,
  out to 14 cm past the shoulder joint, are triangulated, long edges (> 1.6 × the median)
  split, Delaunay flips (not across creases, no folds) and a tangential relax back onto the
  surface, then two light smoothing passes for the points the decimation leaves standing up.
  DelKri: slivers (aspect > 4) in the zone 12 → 6, 284 faces; the white/blue fan over the
  acromion is gone in the stage crops (`sh/cmp.png`).
- `neck_opening` (from `Character.shell(neck=...)`): a shell that keeps at least half of the
  trunk faces in the 12 cm below the neck's root (and not the head) keeps every trunk and
  neck face up to 4 cm above the root, and its contour field is capped by a distance field
  from the neck bone (open-ended upwards, round below the root; front-back × 1.1): 0.5 at
  the neck skin radius + `NECK_EASE` (1.5 cm, radius ≈ 8.4 cm), ramping over ±5 cm, so the
  isoline is a smooth ellipse through the triangles whatever their shape. Got it: DelKri
  shirt, FrnBuk cloak, IncSha unku and yacolla, MaySha tilmatli, SpnSha habit, cappa, hood
  (the hood lies on the shoulders). The drafted garments (EngKri, EngM18) are not shells.
- `HULL_WINDING`: the hull is closed but for small holes (86 open edges: the neck seam and
  the ankles), consistently wound, each piece turned outward by its volume; the rest normals
  are then the vertex normals, no bone-side flip. Flesh rest normals opposed to the surface:
  195 → 0 (DelKri).
In game (sheet, back / three-quarter / front): the shoulder teeth are gone from SpnSha's
cappa (clearly better) and from the back of DelKri's shirt, fewer on FrnBuk's cloak; the
opening dips into a small V at the back of the neck (IncSha, FrnBuk, SpnSha). But DelKri's
quiver strap now shows the shirt through it in patches and EngKri's sash bow changed shape
(the regression checks moved: the rest normals at the hips and armpits differ). `model_layers
--steps 8` poking / crossing before → after: delkri 332 / 224 → 368 / 193, spnsha 77 / 216 →
55 / 192, incsha 156 / 61 → 220 / 34, maysha 62 / 19 → 90 / 14, frnbuk 247 / 273 → 323 / 306,
irolut 121 / 59 → 102 / 65, engkri 101 / 364 → 129 / 302, engm18 179 / 432 → 209 / 435.
Not a clear improvement overall, so the flags stay off; only SpnSha (better on both counts
and in game) was copied to the showcase (old one in `backup-trisoup7/`). Next: build with
one flag at a time to find which one brings the strap and sash pokes (probably
`HULL_WINDING`: outward normals at the armpits and hips move those shells), and seat the
straps after the new shells.

**Eighth trisoup pass: one flag at a time; neck cap and drafted bodies fixed; two flags on**
(#287; VM builds of DelKri, SpnSha, IncSha, MaySha, FrnBuk, IroLut, EngKri, EngM18;
everything in `ACR-analysis/trisoup8/`: `sheet_before_after.png` (main → this pass),
`sheet_flags_back.png` and `sheet_flags_34.png` (base | hull | neck | shoulder),
`layers_*.txt`, `shots_*/`, `glb_base/`, `glb_final/`, `tools/variant.sh`). `model_layers
--steps 8`, poking / crossing:

| unit | base (main) | `HULL_WINDING` | `NECK_CUT` | `SHOULDER_REMESH` | final |
|---|---|---|---|---|---|
| delkri | 396 / 229 | 408 / 184 | 392 / 218 | 355 / 214 | 355 / 211 |
| spnsha | 111 / 202 | 83 / 173 | 81 / 201 | 103 / 202 | 79 / 192 |
| incsha | 217 / 106 | 209 / 83 | 205 / 76 | 246 / 88 | 225 / 73 |
| maysha | 62 / 19 | 99 / 21 | 64 / 18 | 55 / 21 | 53 / 15 |
| frnbuk | 314 / 305 | 323 / 286 | 295 / 299 | 329 / 300 | 299 / 296 |
| irolut | 106 / 71 | 102 / 65 | 106 / 71 | 103 / 57 | 103 / 57 |
| engkri | 109 / 360 | same | same | 143 / 252 | same as base |
| engm18 | 179 / 432 | same | same | 240 / 387 | same as base |

Which flag does what (in game, `sheet_flags_*.png`):
- `SHOULDER_REMESH` removes the shoulder teeth (SpnSha's cappa, DelKri's shirt, fewer on
  FrnBuk) and brought **EngKri's sash bow** change: under a drafted coat (`drape.dress`) the
  re-meshed body tears the coat at the back and the shoulders and moves the sash and the
  skirts (EngKri, EngM18; the only flag that changes them at all). Now
  `Character.body(shoulders=False)` for drafted bodies (civil peasants, frontier, line with
  `garments`): the flag is **on** and only the shell-dressed bodies are re-meshed.
- `NECK_CUT` brings **the V at the back of the neck** (IncSha, FrnBuk, SpnSha) and ate
  SpnSha's hood (a collar standing round the neck). Fixed: `NeckField` centres the neck's
  axis on the skin round it (the bone lies behind the middle of the neck, so the opening
  reached further down the back than the front; shift at most 4 cm) and below the neck's
  root counts the distance along the axis `NECK_DROP` (2.5) times: a flat elliptic cap
  instead of a sphere, still a linear ramp of ±5 cm (a smooth isoline); `shell(neck=False)`
  for the hood. Flag **on**: no V left, the hood is back.
- `HULL_WINDING` changes nothing visible at this distance (crossing down, MaySha's poking
  up 62 → 99): stays **off**.
- **DelKri's quiver strap showing the shirt** is not one of the flags: main shows it too
  (`shots_base/DelKri`). `bd.strap` now lifts the ribbon towards the ray's origin whatever
  the winding of the face it hits; at rest the shirt still pokes through it at the shoulder
  in 20 vertices, worst 6.5 mm (was 21 mm; `tools/strapprobe.py`), but the patches on the
  back in STAND remain: next, sample the strap's surface finer or seat it from the posed
  shirt.
Copied to the showcase: DelKri, SpnSha, IncSha, MaySha, FrnBuk, IroLut (old ones in
`showcase/backup-trisoup8/`); EngKri and EngM18 are unchanged.

**Drape pilot: shells drafted and draped instead** (#287; everything in
`ACR-analysis/drapepilot/`: `sheet_before_after.png` (per unit before | after; back,
three-quarter, front, a walker, the dead man), `delkri_close.png`, `before/`, `after/`,
`glb/`, `layers_{before,after}.txt`, `tools/` (`build.sh` VM build, `shots.py`)). New drafts
in `garments.py`, registered in `GARMENTS`/`CLOTH`/`registry.SPACING`: `tunic` (the coat's
body and sleeves let out, or open armholes, closed front and back, skirts below the waist on
the skirt bones), `cloak` (one panel per side from the hem up the back, over the shoulders
and down the front, a neck hole, the fronts knotted at the throat), `band` (a sash wound
round the waist, shrinking onto what is under it), `keyed` (a draft in a family's key);
instances `frn_blouse`, `frn_sash`, `frn_cape`, `trade_shirt`, `unku`, `yacolla`,
`tilmatli`. `drape.dress(over=...)`: clothes made before (shells, flaps) collide in the sim
and lend their weights like a draped layer beneath. Opt-in per unit, **off** in every unit
script: frontier `drape=True` (blouse, sash, cloak), native `"trade_shirt"` in `dress`,
inca `drape=True` (unku, yacolla), meso `drape=True` (tilmatli); the body is then not
shoulder-re-meshed. The ichcahuipilli was not tried. `model_layers --steps 8`, poking /
crossing, showcase → pilot: delkri 355 / 211 → 198 / 172, frnbuk 299 / 296 → 467 / 277,
incsha 225 / 73 → 128 / 182, maysha 53 / 15 → 52 / 40.

In game:
- **The cloaks fail**: placed over the shoulders they do not stay there. IncSha's yacolla
  slides off and lies on the ground beside him, MaySha's tilmatli hangs off one shoulder,
  FrnBuk's cape collapses into a crumpled red heap on the upper back. Probably the
  over-the-shoulder placement: past the fold it climbs the neck (`top_z` near x = 0 is the
  top of the neck) and then runs out in the air, so the neck hole is in the wrong place and
  nothing hooks the cloak on the shoulders. Next: place cloaks with their own yoke (a ring
  round the neck base at the shoulder top, pinned for the sew phase) instead of the coat's
  fold.
- **The unku** (IncSha) is clean: no teeth at the shoulders or armholes, the skirt
  hangs straight and walks; the old shell's zig-zag hem is gone.
- **The trade shirt** (DelKri) has no shell teeth, but it is crumpled and bulky (22 cm of
  ease is too much for linen this light), the waist seam shows a gap and the skin on the
  front, and the quiver strap still shows the shirt through it in patches (the strap is
  laid on the new surface but the shirt's folds stand above its 26 mm offset).
- FrnBuk's blouse and sash are clean in themselves; with the failed cape the unit is worse.

So the pilot does **not** yet clearly remove the teeth: no glb was copied to the showcase,
the units keep their shells, and no family-wide conversion list is proposed. Before that:
cloak placement (above), less ease and more bending for the shirts, the strap seated with
an offset over the draped folds, then repeat this pilot.

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

**Drape 2: cloaks from a pinned yoke, a fitted trade shirt** (#287; everything in
`ACR-analysis/drape2/`: `sheet_before_after.png`, `before/` (the pilot's showcase shots),
`after/`, `glb/`, `layers_*.txt`, `tools/`). Changes:
- **Cloaks** (`garments.cloak`) are placed by their own drape region `cape`: u runs round
  the neck ring (`theta`, `turn`, `span`), v is the distance from the neck ring along the
  cloak's hanging line, `Body.cape`: per 3° of azimuth the upper convex hull of the body
  (torso, the neck below the ring, the shoulder caps, the thighs) seen along the ray from the
  neck axis, offset by `gap`, then straight down from its outermost point. The cloth's rest
  shape is its placed shape, so the panel only sets the domain (hem lengths per azimuth:
  front, side, back) and the mesh density. The **yoke** (`place["pin_v"]`, the first 7 cm
  from the neck ring, the knot included) is pinned through sewing and settling. The
  tilmatli is one piece round the neck knotted on the right shoulder (`knot_at=-45`); the
  yacolla and the buccaneer's cape are two halves sewn down the centre back, knotted at the
  throat. Weights (`drape.cape_weights`): from the skin or layer under it, the limb bones
  taken out (half the arm share kept on the shoulder top), below the waist blended into
  the skirt bones (with the runtime `_cloth` amount of a coat skirt).
- **Trade shirt**: 12 cm ease at chest and waist (was 22 / 26), bending 0.8; the tunic's
  waist seam welds up to 8 cm (`close`). The quiver strap is laid with 48 samples and
  `straps.clear_folds` lifts each row until rays across its width and half way to the next
  row clear the surface by the offset.

`model_layers --steps 8`, poking / crossing, showcase → drape 2: incsha 225 / 73 → 81 / 219
(the crossings mostly the yacolla's free part against the unku in stride), maysha 53 / 15 →
67 / 136, frnbuk 299 / 296 → 539 / 558, delkri 355 / 211 → 260 / 187.

In game (blank field, ACR_LOD=0; the exe used drew the textured ground, not white):
- **The cloaks stay on**: all three hang from the shoulders, none slides off.
- **IncSha's yacolla** reads right: a black mantle down the back to the knees, open in
  front over the unku, walks and lies with the body. Left: small white patches of the
  unku on the shoulder tops. **On** (`incsha.py` `drape=True`, unku and yacolla draped).
- **MaySha's tilmatli** is on the shoulders and knotted, but crumpled and shows small dark
  holes (the hipcloth/skin poking through the folds): **off**, the shell stays.
- **FrnBuk's cape** sits on the shoulders but is ragged, with holes and the blouse through
  it: **off**.
- **DelKri's shirt** is slimmer and the hem teeth are gone, but the quiver strap still shows
  patches at the back and the shoulder seams show wisps: not clearly better, **off**.

Next: more mesh density in the cape region (`span` gives the round-the-neck resolution;
the hanging part is coarse at the hem), a larger `gap` / `margin` where a cloak lies on a
draped layer, and self-collision for the tilmatli's folds. Conversion list for the other
families: not yet (only one cloak passes).

**Drape 3: cloaks over the layers beneath; why the tilmatli and the buccaneer's cape still
fail** (#287; everything in `ACR-analysis/drape3/`: `sheet_before_after.png`, `pair_*.png`,
`wire/` (rest and STAND wireframe crops, `*_d2_d3.png`, `*_d3_arm30.png`), `before/`,
`after/`, `glb/`, `glb_arm10/`, `layers_*.txt`, `tools/` (`build.sh`, `shots.py` now framing
the walker from `state`, `wire.py`, `pair.py`, `sheet.py`); the exe built from this branch,
ground white). Changes kept:
- **The cloak's hull covers the layers under it**: `Body.cape_over` adds the rest-pose
  vertices of everything the cloak lies on (`over` and the garments draped before it in the
  same `dress`, skirts included) to the hanging-line hull, `CAPE_OVER` (1.2 cm) further out;
  so the yoke is pinned outside the blouse, the unku or the sash, not inside them.
- Cloak collision margin 9 mm (was 6); the tilmatli self-collides (`CLOTH[...]["self"]`,
  4 mm), opt-in per cloth.

Tried and dropped: a denser cape mesh (`SPACING` 2.4–2.6 cm instead of 3.0–3.5) and the
whole upper arm in the hull (`CAPE_ARM` 0.30 m instead of 0.10).

Causes, from the wireframe crops (rest pose and STAND):
- **The crumple is the arms, not the mesh resolution.** The back panels are smooth; every
  pleat sits where the hanging line, falling straight from the shoulder cap, meets the upper
  arm of the A-pose rest body. The sim pushes the cloth out round the arm and it buckles into
  pleats one triangle wide; the forearm comes out through the side. A denser mesh only makes
  more, smaller pleats; the whole upper arm in the hull makes a bell to the elbow and still
  buckles below it.
- **The dark "holes" are pleats turned inside out**: triangles of those folds face inwards
  and render black (the denser mesh gave many more of them: MaySha and FrnBuk were worse in
  game). Not skin showing through.
- **FrnBuk's blouse showed through** because the yoke was pinned inside the blouse (30 cm of
  ease stands well off the skin): fixed by the hull over the layers (the back is now red to
  the sash), but the shoulders crumple like MaySha's.
So the shoulder-cape and the tilmatli need the arm handled before they can pass: drape them
on a rest body with the arms down (a pose-space sim: the arms rotated to the STAND angle for
the sim, the result un-posed by the arm bones' inverse), or cut the cloak's sides round the
arm (an arm slit, like a poncho's). Then the pleats over the arm do not form at all.

`model_layers --steps 8`, poking / crossing, showcase (shell) → drape 2 → drape 3 (the
denser test build): maysha 53 / 15 → 67 / 136 → 81 / 84, frnbuk 299 / 296 → 539 / 558 →
627 / 607 (`layers_cloaks.txt`).

`model_layers --steps 8`, IncSha drape 2 (the showcase) → drape 3 (crossing vertices):
unku → yacolla 109 → 84 (samples 1877 → 1579), skin → yacolla 44 → 31, skin → unku
66 → 68; poking at rest 81 → 67… 76; all 219 → 183 (with the denser mesh: 167). Row by
row, the 219 of drape 2 were: the stride legs through the knee-length unku skirt in
MOTION_L/R (66; free cloth on the skirt bones, the runtime cloth moves it; the shell unku
was shorter and tighter), the yacolla's free part against the unku in the crawl and death
clips (109; both free cloths on different skirt weights) and the skin against the yacolla's
skirt in CRAWLARM / ATTACK (44). None is in STAND or the walk's upper body: harmless in game; the yacolla rows drop by a quarter with the hull over the unku.

In game:
- **IncSha** (on since drape 2), rebuilt with the drape 3 code: the yacolla now lies over the
  unku and opens a little wider in front; the white unku bits at the shoulder tops and the
  back of the neck are about the same (the unku's neckline stands above the yacolla's yoke:
  the unku's neck hole sits higher than the cloak's neck ring; next: lower the unku's neck
  or raise the yoke's ring by the unku's height there). Not clearly better: the showcase
  keeps the drape 2 glb.
- **MaySha**: smoother at the back, but the sides crumple over the arms with many black
  inverted pleats: **off**.
- **FrnBuk**: the blouse no longer shows on the back, but the cape is ragged and black-flecked
  over the shoulders: **off**.
- **DelKri**: not changed in drape 3 (the strap patches and shoulder wisps need the strap
  seated on the posed shirt): **off**.

**Drape 4: cloaks draped with the arms down** (#287; everything in `ACR-analysis/drape4/`:
`sheet_before_after.png` (MaySha, FrnBuk; showcase shell | drape 4), `pair_*.png`, `before/`,
`after/`, `glb/` (final), `glb_v1/` (without the smoothing), `wire/`, `layers_{before,v1,after}.txt`,
`tools/`; the exe built from this branch, ground white, HUD off, `ACR_LOD=0`). Changes:
- **Pose-space drape** (`drape.dress`, for cape-region garments in `ARMS_DOWN_GARMENTS` or with
  `CLOTH[g]["arms_down"]`: the tilmatli and the buccaneer's cape): the rig is posed with the
  arms of `clips.STAND_FREE` (`ARMS_DOWN`, copied: upper arms (4, ±22, 0), forearms -12°),
  the skin, the shoes, `over` and the layers draped before it are linear-blend skinned into
  that pose (`pose_objects`, restored after), `Body.pose_arms` moves the measuring body's arm
  points and bones, and the hanging-line hull takes the upper arm down to `CAPE_ARM_DOWN`
  (0.32 m: the cloak hangs over the arm at the side, like a mantle). The cloak is placed,
  sewn and settled there, then `taubin` (`POSED_SMOOTH` = 8 passes, the pinned yoke kept, the
  cut edge moving along itself) flattens the one-triangle pleats left, its weights are taken
  in the posed space (`cape_weights` from the posed skin and layers beneath), and `unpose`
  brings each vertex back by the inverse of its blended skinning matrix. The cloak's weights
  are trunk-only below the shoulder top, so almost all of it is unchanged by the inverse
  (largest move in the report `unposed_m`); it lies through the raised A-pose arm at rest,
  but that pose is never shown. The weights used for the inverse are the ones exported.
- Such a cloak is marked `acr_posed`: `hide_skin` does not push the arm's skin under it and
  `settle` does not pull arm vertices (skin, sleeves) under it (it would dent the arms).
- Arm slits were not needed (not tried).

`model_layers --steps 8`, poking / crossing, showcase (shell) → drape 4 without smoothing →
drape 4: maysha 53 / 15 → 70 / 121 → 58 / 108, frnbuk 299 / 296 → 322 / 501 → 271 / 513. The
crossings are the arms under the cloak in the raised-arm and crawl clips (CRAWLARM, ATTACK,
DEATH: the cloak has no arm weights, the arms swing under it and come out through it when
raised forward), e.g. FrnBuk's sleeves 328 of the 513; none in STAND or the walk.

In game:
- **MaySha**: no black inverted pleats left (drape 3 was covered in them; without the
  smoothing about a dozen remained on the back). The tilmatli hangs from the shoulders over
  the arms to the forearms, knotted on the right, open in front; it walks and lies with the
  body. Clearly better than the shell's flat plate: **on** (`maysha.py` `drape=True`), copied
  to the showcase (old glb in `showcase/backup-drape4/`).
- **FrnBuk**: front and three-quarter clean (the cape over the shoulders and arms, no black
  pleats, no blouse through it in front), but the back shows white patches of the blouse
  through the cape at the left side and above the hem. Likely the musketeer's stand: his arms
  are not at `STAND_FREE`, so the posed sleeves under the cape are not where the sim had them.
  **Off**. Next: drape it with the unit's own STAND arms (the family's stand clip instead of
  `STAND_FREE`), or a larger cloak margin over the sleeves.
- The walker was out of frame in the shots of both units (`shots.py` frames it from `state`).

**Drape 5: own stand arms, the yoke over the neckline, blouses and sashes** (#287; everything
in `ACR-analysis/drape5/`: `sheet_before_after.png` (FrnBuk, IncSha, UsaMil, EngTap, EngKri,
SpnKri, FrnKri; showcase | drape 5), `pair_*.png`, `before/`, `after/`, `glb/`,
`layers_{before,after}.txt`, `tools/`, the exe built from this branch in `exe/`; ground white,
HUD off, `ACR_LOD=0`, one field per driver process). Changes:
- **The cloak is draped with the unit's own stand arms**: `drape.arms_down(stand)` takes the
  clavicle, upper arm, forearm and hand rotations of `ch.stand` (set by the family before
  `dress`; `clips.STAND_FREE` without one, so MaySha's and IncSha's drape arms are unchanged). `ARMS_DOWN` is
  now `arms_down()`, not a hand copy. The frontier family sets `poses.STAND_MUSKET` (the
  musketeer's stand: left upper arm forward, the right forearm up at the musket).
- **The yoke ring over a layer's neckline** (`CAPE_NECK_OVER`, 4 cm): the layers a cloak lies
  on count in its hanging-line hull up to 4 cm above the skin's cut-off at the neck, so the
  yoke ring is raised over the unku's neckline instead of under it.
- `civil.spec(drape=True)`: the peasant's bloused shirt and sash drafted and draped (the
  frontier's `frn_blouse` and `frn_sash` in the unit's cloth) instead of their shells; the
  smock (skirts) keeps its coat and only the sash is draped.

`model_layers --steps 8`, poking / crossing, showcase → drape 5: frnbuk 290 / 304 → 207 / 520
(fshirt → fcloak 328 crossing, all in the raised-arm clips ATTACK_CUT_FINISH / CRAWLARM, none in
STAND or the walk), incsha 81 / 219 → 66 / 199, usamil 86 / 100 → 86 / 120, engtap 114 / 274 →
86 / 322, engkri 101 / 364 → 88 / 359, spnkri 122 / 150 → 128 / 170, frnkri 119 / 119 → 122 / 129.

In game:
- **FrnBuk**: the blouse patches on the back are gone (a little white at the right armpit
  and under the hem's left corner); the cape lies over the shoulders and arms, smooth, the
  strap over it; the shell's jagged back plate is gone. The draped sash is narrow and gathered
  in front. Clearly better: **on** (`drape=True`).
- **IncSha**: no white unku bits at the shoulder tops or the back of the neck any more (the
  yacolla's yoke now lies over the neckline); the collar is a little ragged in front.
  Better: **on** (rebuilt).
- **UsaMil** (blouse alone): the waistcoat is one draped piece, the shell's waistband step and
  its teeth gone, the strap on it; a few white wisps at the shoulder seams as before. **On**.
- **EngTap** (sash alone): a clean band instead of the shell's red zigzag. The hunting
  shirt (still a shell with its cape) keeps its black flecks. **On**.
- **EngKri** (sash over the smock): a clean band instead of the shell's teeth. **On**.
- **SpnKri, FrnKri** (blouse and sash): the back is better, but in front the blouse opens
  across the belly at the waist seam (skin shows in a slit above the breeches). **Off**: the
  civil blouse needs its front skirt sewn to the body over the breeches (or a lower waist
  seam) before it can go.
- The walker is in frame now for FrnBuk and EngTap (`shots.py` frames the spawn); the others
  walk out of the crop.
- MaySha not rebuilt: `STAND_FREE` and `CAPE_NECK_OVER` leave the tilmatli as it was (it
  lies over the hip cloth only).

### Roll-out list: shell garments to draft and drape

Effort: S = a draft instance of an existing `tunic` / `cloak` / `band` (an hour with the
build and shots), M = a new draft parameter or panel (half a day), L = needs a missing
mechanism (a day or more). Order within a family is the order to do them. The blocker for
every **cloak** row was the arm problem above: fixed in drape 4 (pose-space drape); a cloak
is now S + a check of its unit's stand arms; tunics and bands can go now.

| family | shell → draft | kind | effort | note |
|---|---|---|---|---|
| native (Woodlands) | `shirt` → `trade_shirt` | tunic | S per unit, after DelKri passes | strap seat on the posed shirt first (M) |
| native | `matchcoat` (blanket over one shoulder) | cloak, `knot_at` under the arm | M | like the tilmatli; add to `ARMS_DOWN_GARMENTS` |
| native | `cloak` (shaman's pelt) | cloak | S (arms down) | stiff hide: high bending |
| native | `breechcloth` + flaps | band + two hanging panels | M | flaps are modelled and clean: low priority |
| plains | `war_shirt` | tunic, open sides, fringe stays a shell strip | M | hide: thicker cloth |
| pueblo | `tunic` | tunic, no sleeves | S | |
| pueblo | `sash` | band | S | |
| pueblo | `blanket` | cloak | S (arms down) | |
| pueblo | `jerkin` | tunic, open front | M | |
| meso | `tilmatli` | cloak (`knot_at`) | done, **on** for MaySha (drape 4) | other meso units: S each |
| meso | `maya_jacket` | tunic, short | S | |
| meso | `ichcahuipilli` | tunic, quilted (thick, stiff) | M | never tried |
| meso | `maxtlatl` | band + flaps | M | |
| inca | `unku` | tunic | done (IncSha; neckline under the yoke since drape 5); S per unit | all unku variants are keys |
| inca | `yacolla` | cloak | done (IncSha); S per unit | |
| inca | `quilt` (quilted armour) | tunic, thick | M | |
| frontier | `blouse` | tunic (`frn_blouse`) | done, **on** for FrnBuk, UsaMil (drape 5) | |
| frontier | `sash` | band (`frn_sash`) | done, **on** for FrnBuk, EngTap (drape 5) | FrnBuk's gathers narrow in front |
| frontier | `cloak` / `cape` | cloak (`frn_cape`) | done, **on** for FrnBuk (drape 5) | draped with `poses.STAND_MUSKET`'s arms |
| conquista | (uses frontier's layers) | as frontier | S each | SpnS15/FrnS15 drape their own 16th-century set (`conquista.DRAPE`): not tried |
| civil | `blouse`, `sash` | tunic, band (`civil.spec(drape=True)`) | sash **on** for EngKri; blouse M | SpnKri/FrnKri off: the blouse opens at the waist seam in front |
| civil | `habit` + `habit_legs` | tunic to the ankle, wide sleeves | M | |
| civil | `cappa`, `hood` | cloak (+ a hood panel) | M | arms down |
| cavalry | `coat` (riders' coats), `waistcoat` | the uniform `coat` / `waistcoat` drafts | M | the seat: skirts split over the saddle (`gear` seat) |
| cavalry | `buffcoat` | `buffcoat` draft | M | as the infantry's 17th century |
| cavalry | `sash` | band (over the shoulder: a bandolier variant) | M | |
| cavalry | `breeches`, `boots`, cuffs, lapels | stay shells | – | tight or rigid, no teeth seen |

Next: the civil blouse's front (SpnKri, FrnKri); the pueblo tunic and sash and the maya
jacket (not done in drape 5); the other meso/inca cloaks with their own stands (`ch.stand`).

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

## Traced patterns

Historical cutting diagrams (Waugh, *The Cut of Men's Clothes*; later Reconstructing History,
J.P. Ryan) are digitised into `acr-pattern/1` JSON (schema: the `acr-garments` skill,
`references/pattern-json.md`) under `ACR_SOURCES/patterns/<source>/<garment>.json`: locally
`D:\ACR-models\patterns\`, on the VM `/home/claude/sources/patterns/`. They are copyrighted:
the JSON, the trace sidecar (`<garment>.trace.json`, scan pixels) and the overlay stay there;
the repo holds only the code and synthetic test patterns.

- `garments.load_pattern(path)` / `check_pattern(doc)`: reads and checks the JSON
  (counter-clockwise outlines, edges in range, seam links naming real edges, every grading
  measure in `base`).
- `garments.grade_pattern(doc, m)`: grades from the original wearer's `base` to the body's
  `m` and returns `Panel`s and `Seam`s for the drape. Girths (`grade.u` bands, e.g.
  `[[-200, "waist"], [8, "chest"]]`) scale u by `(m + ease) / (base + ease)` with the ease
  kept constant; a band blends in over `GRADE_BLEND` (8 cm) below its `v_from`, so widths
  have no step. Lengths (`grade.v`, one measure or bands) scale v from the waist line:
  `v' = integral of m/base`, so the waist stays put and skirts (v < 0) and the body (v > 0)
  scale by their own lengths; `place.lift` is graded the same way.
- `garments.traced(path)`: a draft `draft(m) -> (panels, seams)` (with `draft.pattern`);
  `garments.pattern_path("waugh/doublet_1635")` resolves under `ACR_SOURCES` and fails
  clearly without it.
- A grading band (u or v) named `fixed` keeps the traced size (no measure for it, or cloth
  so full that the body does not matter).
- A unit switches to a traced garment through its SPEC: `pike.musketeer_spec(...,
  coat_pattern="waugh/doublet_1635", breeches_pattern="waugh/breeches_1640")` registers
  each (`pike.traced_garment`, cloth of the JSON's `cloth`) and drapes it in place of
  `doublet` / `breeches`. `engmus.COAT_PATTERN` and `engmus.BREECHES_PATTERN` stay `None`
  (the drafted garments) until the traced ones match their layer numbers;
  `engpik.COAT_PATTERN` is `"waugh/buffcoat_1640"` (round 6). The rule for a default:
  crossing not above the drafted, poking at most 10% above, `gear_volume` not worse, the
  studio sheet passed.
- `stats.json` `drape.<garment>.worst_gap_at_cm`: the widest sewn pair's midpoint (cm, rest
  pose, model axes), so a seam gap can be placed without a render.

**Waugh Diagram VIII** (book p. 26, PDF p. 15), the linen doublet c. 1635-40 (V&A), for
EngMus. Scale from the margin ruler, 63.9 px/in at 300 dpi. Seven panels: front and back (cut
in one with their skirts in the original, split at the waist line and welded `close`), front
and back skirts, the side basque (trimmed by the 1 in the skirts overlap it), a stand collar,
the one-piece sleeve (drawn folded; straightened along its fold, v = arc length from the
wrist); fourteen seams. The sleeve head is split where its length fraction matches front and
back armhole (27.4 : 17.7 cm), so each half sews to its own armhole. Wearer read off the
pattern: chest 81, waist 70, back length 40 cm (ease 10); the 1630s waist sits 9 cm high
(`lift`). Graded to the default body: chest x1.19, waist x1.17, back x1.13.

**Waugh Diagram X** (book p. 27, PDF p. 16), Genoa velvet breeches c. 1640 (V&A). Page
deskewed by -0.95 deg; 64.0 px/in. Front, back and the waist band; eight seams. The fullness
(1.6x) is gathered into the band (seam ease 0.75). The leg runs 57 cm from the crotch to the
hem, worn bloused and gathered at the knee: v below the crotch is graded so the hem lands
1 cm below the knee; widths and the rise are `fixed` (about 28 cm over the thigh; no rise
measure yet). The waist band is pinned. Padding and pockets are not cut. The front's CF
curve follows the panel's own (inner) edge; the outer line with cross ticks is the fly strip.
The inseam slants about 20 cm across each panel from hem to crotch: the leg panels set
`place["u0_edge"]: "inseam"` (`Panel.u_on`, `drape.place`), so u is measured from the inseam
at every height and the seam stays on the inner leg instead of wrapping round the thigh
(inseam gap 83 -> 30 mm, welded 123/131 -> 131/133). Breeches under a doublet are hidden
4 mm under it (`line.make`, like the shirt): the gathered band no longer pushes the doublet
out (drafted EngMus too: 258/200 -> 261/188).

**Waugh Diagram IX** (book p. 26, PDF p. 15, lower half), buff leather doublet c. 1640
(Masner), for EngPik behind `engpik.COAT_PATTERN` (`pike.pikeman_spec(..., coat_pattern=
"waugh/buffcoat_1640")`, in place of the drafted `buffcoat`). Same ruler as VIII. Front and
back cut in one with flared skirts (drawn overlapping at the side; the side seam sewn to the
hem, the back's small step at the waist kept), the CF laced to the waist (`close`) and open
below, the CB sewn to the waist with a vent below, the folded one-piece sleeve straightened
as in VIII (keyed `dsleeve`: cloth sleeves under the armour; buff in the original). No
collar. Wearer: chest 86, waist 85 (straight body), back 39 cm with a 5 cm `lift`; ease 16;
skirt 32 cm below the waist.

Numbers on EngMus (VM, 24 Oct 2026; drafted garments in brackets):

| | welded | `gap_mm_max` | poking / crossing | sprite IoU STAND |
|---|---|---|---|---|
| drafted doublet + breeches | 193/201, 118/128 | 110, 94 | 258 / 202 | 0.596 |
| traced doublet (head as one chain, sleeve widened) | 234/234 | 35 | 298 / 256 | 0.607 |
| traced doublet (head split by length, sleeve widened) | | | 288 / 276 | |
| traced doublet (head split by length, sleeve as traced) | 231/234 | 57 (collar) | 250 / 259 | |
| traced doublet + breeches | 234/234, 123/131 | 30, 83 (inseam) | 353 / 331 | |
| drafted, shirt hidden under the doublet | | | 258 / 200 | |
| traced doublet, shirt hidden, belts 20/24 mm off | 231/234 | 57 (collar) | 237 / 239 | |
| traced doublet + breeches, same | 234/234, 123/131 | 30, 83 (inseam) | 347 / 267 | |
| traced doublet, sleeve weights smoothed 3 passes (`drape.sleeve_weights`) | | | 236 / 229 | |
| same, smoothed 6 / 12 passes | | | 235 / 242, 236 / 249 | |
| same, arm proxy 2 cm out, no smoothing / 3.5 cm and 8 passes | | | 237 / 251, 235 / 242 | |
| round 5: drafted, breeches hidden under the doublet | 193/201, 118/128 | 110, 94 | 261 / 188 | |
| round 5: traced breeches (CF retraced, `u0_edge`) | 131/133 | 64 (band) | 298 / 209 | |
| round 5: traced doublet + breeches | 234/234, 131/133 | 28, 64 (band) | 322 / 212 | |
| round 6: same, breeches back hem re-read as a curve | 234/234, 128/133 | 26, 65 (band, at the right hip, z 103 cm) | 317 / 215 | |
| round 6: same, legs lifted 2 cm, band 1 cm lower | 130/133 | 64 (band, same place), outseam 59 | 309 / 229 | |

Numbers on EngPik (round 5): drafted buff coat welded 175/187, `gap_mm_max` 89, 241 / 310;
traced Diagram IX 136/136, 32 (sleeve head), 253 / 310 (skin through the buff 70/43 -> 35/23,
cloth sleeves through the buff 22/13 -> 46/35); `gear_volume` 0 closed items in both.
Round 6: the traced buff coat is EngPik's default (`engpik.COAT_PATTERN`): crossing equal
(310), poking 5% above (253 vs 241), rebuilt twice to the same numbers. Its sleeve weights
are not the cause of the cloth sleeves through the buff (`model_layers` 46 poking, 37
crossing; drafted 22 / 10): plain transfer weights measured 272 / 313, the arm proxy 1.5 cm
out 253 / 314. The pokes sit at the sleeve head under the buff's armhole in the rest pose.
EngMus stays drafted: the traced pair measures 317 / 215 against 261 / 188; the extra
crossing is the doublet through the belts (26 vs 12) and the falling band through the belts
(36 vs 22). The band seam's widest pair is at the side of the hip, not at the CF, and
lifting the legs did not close it.

The waist band must be pinned (`place.pin` "top": tied to the doublet with points): unpinned,
the full breeches slid down into a puff at the knee, with bare thighs. That failed drape still
welded 131/131 and measured 265 / 219, so check the sheet before you trust the layer numbers.
Per pair (traced doublet against the drafted, crossing vertices): skin through the
doublet 51 vs 39 and the sleeves 42 vs 29 (strike and crawl frames: the fuller cut stands
off the skin it takes its weights from), the falling band through the bandolier 43 vs 22,
doublet through the bandolier 20 vs 12. The shirt is now hidden under `doublet` and
`dsleeve` like the line's under the coat (`line.make`, all draped pike units), and under a
traced doublet the belts lie 20/24 mm off the cloth (`pike.TRACED_STRAP_OFFSETS`,
`build["strap_offsets"]`); on the drafted doublet those offsets measured worse (213
crossing). With the breeches the doublet's poking rises at the band and belts (doublet
through fallband 64, through belts 38): the gathered waist pushes the doublet out. A traced garment's
full sleeves take their weights from the arm skin only (upper arm and forearm faces of their
side, `drape.sleeve_weights`, switched by the cloth's `sleeve_weights` that
`pike.TRACED_SLEEVE_WEIGHTS` sets), smoothed over the sleeve with the armhole and wrist edges
held at the plain transfer's weights: skin through the sleeves 42 -> 32, shirt through the
sleeves 25 -> 19 crossing. Pushing the arm proxy out to the sleeve's stand-off or smoothing
more measured worse (table): the crossings left are not a weighting problem alone, and the
traced doublet stays behind the switch (goal 202 crossing). Casting
the falling band below the belts (leaving `belts` out of its ray tree) cut poking by 36-71
but raised crossing by 10-14: not kept. Not modelled: the
lacing band, the open sleeve slit to X, padding.

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

## 17th century (and the conquistadors' 16th)

A unit chooses its own garments with `build["drape"]`: the garment names in drape order
(inner first). `drape.dress` uses that set instead of the one its family's `make` asks for,
so `line.make` and `frontier.make` drape them unchanged (`pike.MUSKETEER_DRAPE`,
`pike.PIKEMAN_DRAPE`, `officers.DRAPE17`, `conquista.DRAPE`). Patterns in `garments.py`
(`tests/test_garments.py`, `Period17`):

| garment | cut | keys | cloth |
|---|---|---|---|
| `doublet` | the coat's body and two-piece sleeves fitted closer (12 cm ease at the chest), no lapels, cuffs or skirts, the centre front sewn shut; a low stand collar; short skirts (13 cm) cut into **tabs** (slits every ~8 cm), free cloth on the skirt bones | `doublet`, sleeves `dsleeve` | wool |
| `doublet_slashed` | the same with three **slashes** down the top sleeve (inlay strips of the lining) | + `slashes` | |
| `buffcoat` | fitted (16 cm ease), hooked shut to the waist, **skirts to above the knee** (front and back, sewn at the sides, a back vent), sleeves of cloth (`sleeves="dsleeve"`), of buff (`sleeves="buff"`) or none (`buffcoat_bare`) | `buff` | thick leather: mass 0.9, tension 40, bending 14 |
| `hose` | long hose from the instep to mid-thigh, seamed up the back, the top pinned | `hose` | knit |
| `slops` | trunk hose: the breeches cut short, 26 cm of extra girth over the thigh **gathered** into a thigh band (seam ease 1.3), paned (lining strips) | `slops`, `slashes` | wool |
| `shirt17` | deep linen cuffs only (not simulated) | `shirt` | linen |

Modelled, not draped (`drape.py`, registered gear):
- **`falling_band`**: the wide linen collar. A grid round the neck (48 angles x 7 rings,
  11.5 cm deep in front, 8.5 cm behind, split at the centre front), each vertex cast
  straight down onto the draped clothes (not the skin: the chin caught the rays) and
  lifted 8 mm, a turned-down lip; skinned like the cloth under it (`finish(..., under=)`).
- **`bucket_boots`**: per leg, rings up the shin fitted per angle to the skin and the hose
  plus 1 cm, then the bucket: the top flaring 6 cm out over the knee and its edge turned
  down; skinned from the skin; key `jackboots` (painted like leather, no gaiter buttons).

Materials (`outfit.py`): `buff` has its own painter `paint_buff` (oil-tanned, napped:
a cloudy mottle where the oil took unevenly, a faint grain, greased dark at the hems and
the front edges) with a matte finish and a soft sheen (`FINISH["buff"]` = roughness 0.92,
sheen 0.30, gloss 0.02); `slashes` and `fallband` are linen cells, `jackboots` leather.
Layer ranks (`acr_render::layers`): hose 1, slops and jackboots 2, doublet and dsleeve 4,
buff 5, fallband 6.

Rebuilt and checked on the blank field (sheet `ACR-analysis/c17/sheet_c17.png`: sprite,
old, new, face): EngMus, EngPik, EngOf17, SpnS15. Known: the band is ragged along its outer
edge and hidden under the buff coat's shoulders; the doublet's collar stands above the
band; the buff coat's skirts hang flat (no pleats at the side); the slops gather little
puff (no pressure in the sim). Not yet: the 17th-c. cavalry (`cavalry.py`: EngD17,
FrnD17, SpnD17, the *KOf17), which need `buffcoat`, `falling_band` and `bucket_boots` on
the rider (their `make` drapes its own set) and the matchlock or carbine.

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
- Trisoup (#287): rest-pose pokes remain in every family (survey under "Layers");
  `drape.settle` catches only part of them; `drape.layer_order`, `cull_hidden` and
  `relax_edges` (third pass) are built but off. Not done yet: deleting covered inner faces
  checked over all clips, seam-aware edge smoothing with a turned hem, wider atlas gutters
  (`outfit.atlas_uv` packs islands with a 0.006 margin of the *whole* atlas before it is
  scaled into a painter's cell, i.e. under 1 px in small cells, and pads cells by 2 px),
  and before/after renders.
