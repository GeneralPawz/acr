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
