# Humans: skeleton H, layered bodies, decomposition and gore

European foot units (#169), horses and riders (#170), decomposing corpses (#178) and deaths
by cause (#183). The first run (PR #181) was a self-contained prototype; its pieces now live
in the pipeline's library and the units are normal assets built by `cargo xtask assets`
([assets.md](assets.md); the vertical slice's numbers: [assets-slice.md](assets-slice.md)).
Conventions as in assets.md (30 px/m, +X east, +Y north, +Z up, models face south,
`socket.<name>`).

| module (`assets/blender/lib/acr_assets/`) | |
|---|---|
| `rig.py` | skeleton H and horse rig R as data (pure Python, unit-tested), body parts and cut joints, `.MD` clip frame counts |
| `armature.py` | armature from a table, `pose` in armature axes (+ `world` / `world_loc` targets), clips as actions |
| `body.py` | skin hull, smooth weights, part split, clothing shells, skirt on its bones, straps, bone layer, stump caps, morph targets |
| `humans.py` | `Character`: layers, finishing (morphs, masks, colours), export preparation |
| `horse.py` | horse hull, mane/tail, bone layer, saddlecloth, saddle, gait and fall poses (the procedural horse, `ACR_HORSE=procedural`) |
| `cavalry.py` | mounted units on the WildMesh horse (#287): rider dress (coat, armour, native), military saddle with holsters or a blanket and rope bridle, reins IK, a fist round the standard or lance, natives' hair, feathers and shield, guns, morion, clips (attack index 0 the cold weapon, index 1 the shot, as the .MD), the fall ending over the unit's origin and the thrown rider beside it; gait and strides matched to the game's speed (`MOTIONS`) |
| `mount.py` | the WildMesh pack's horse as a mount (#287): loaded, tinted bay, pruned to its weighted bones and decimated, scaled to 1.5 m at the withers, its gaits sampled on sprite frames (`animals.py`); `join` hangs the rider's root from its `saddle` bone; a saddlecloth cut from its back |
| `gear.py`, `poses.py`, `decay.py`, `detail.py` | gear meshes; placeholder poses/clips; decay layers and masks; procedural detail tiles |

Units (every human type and the plan for the rest: [humans-roster.md](humans-roster.md)):
`assets/units/engm18.py` (musketeer) and `frnm18.py` (French fusilier), data over the line
infantry family `acr_assets/line.py`; `engkri.py` (peasant), `engd18.py` (dragoon),
`engkf18.py`, `frnkf18.py`, `spnkf18.py`, `usakf18.py` (standard bearers), `spnrei.py` (reiter),
`siukstr.py` (Sioux horseman with a gun), `puevsda.py` (Pueblo lancer). Not built, because
the game never makes them (no nation's .NDS lists them, so they are not in the catalog):
SpnReiG1/SpnReiG2 (leftover variants of SpnRei with other body sprites, no LIFE, DAMAGE 1).
PueVsd (PueVsda without `MAXINDEPO`, same sprites) *is* in the catalog: the Delaware, Huron
and Iroquois horseman. The game has no mules, donkeys,
pack animals, carts or limbers (no such object type among its .MD files; the cannon `ArtPus`
is pushed by its crew, modelled as part of the gun in `acr_assets/guncrew.py`), so the pack's Donkey has no unit to serve.
Tools: `assets/blender/tools/silhouette.py` (fits poses and proportions to the sprite
silhouettes in one Blender session) and `tools/preview.py` (lit renders, free views and decay
stages from the glTF alone, with the decay shader rebuilt from the extras).

## Export contract (what the renderer reads)

- File `<md name lower case>.glb`; one armature; clips = actions named after the `.MD` keys
  (`STAND`, `MOTION_L`, ..., `DEATHLIE1`), keyed on sprite frames at 24 fps; the exported
  scene pose is `STAND` (not the bind pose).
- `COLOR_0` of every mesh with an `acr_decay` material = the decay mask (R blotch noise, G
  fray noise, B layer code / 8); materials: base colour texture (a small detail tile times
  the part's colour) on `TEXCOORD_0`, extras `acr_decay` (layer windows) and
  `acr_base_rgb`.
- Custom attributes (`export_attributes`): `_GORE` (vec4: wound, blood, dirt, burn noise,
  each rank-normalised) and `_PART` (body part 0..9 of `rig.PARTS`, stump caps 16 + joint
  / 32 + joint, horse 48).
- Morph targets: `deflate` (flesh, clothes and strapped gear collapse onto the bones),
  `emaciated` (hunger, #183), the per-soldier `face_*` and `body_*` targets (see "Faces and
  bodies" below). The coat skirt has bones (`skirt_*`) instead of the first
  run's `lie_flat` morph: the `DEATHLIE*` clips lay it flat.
- Nodes: `socket.*` bones (hat, back, hips, hands, hit sockets `socket.hit_*` for stuck
  missiles, horse `socket.saddle`); gear that drops at death (hat, musket, sabre) is a
  separate node on its socket, strapped gear (knapsack, cartridge box, saddle) is skinned.
- The mesh `<name>.caps` holds the stump caps: hidden unless a joint is cut. Cutting joint
  j: draw the body without the parts below j plus the parent-side cap (`_PART` 16 + j);
  draw the cut-off part with its child-side cap (32 + j) and, in that copy's palette, every
  bone outside the part locked to the part's root bone (so the seam vertices weighted to
  the parent follow the part).
- `_LIMB` (float, #262, `acr_assets/limb.py`): the limb level 0..1 (level / 255) along the
  chain shoulder → elbow → wrist → tip of the middle finger (arm) or hip → knee → ankle →
  toe tip (leg), linear per segment so that the joints land exactly on elbow 110, wrist
  205, knee 120, ankle 215 (/255). Skin, bones and stump caps project their rest position
  onto the segments of their own `_PART` (upper arm/thigh: the first segment only;
  forearm+hand/shin+foot: the other two); clothes and strapped gear take the level of the
  nearest skin vertex of the same limb; trunk, head, held gear and the horse are −1.
- Root extras `acr_limb_radius`: `{"arm_l": [16], "arm_r": [16], "leg_l": [16], "leg_r":
  [16]}`, the outer radius (metres, 75th percentile of the skin and clothes distances to
  the chain; rigid strapped gear left out) at the levels k/15, k = 0..15; empty windows
  interpolated; a sample is capped at 1.12x the median of its neighbours within +-2, so
  armpit, crotch and coat-skirt vertices near the trunk joint don't bulge the root. Stump caps and the gib pack's unit caps are scaled with it.
- Crawl clips (#262, `acr_assets/crawl.py`; engm18, engkri and the engd18 rider, who crawls
  where DEATHLIE1 throws him beside the fallen horse; listed in `acr_clips`):
  `CRAWL2` (24 frames: face down, head first, elbows pulling alternately, legs dragging),
  `CRAWL1` (24: both arms pull while the left leg pushes, the right drags), `CRAWLARM`
  (28: the left arm hauls, the right limp), `WRITHE` (30: on the back, knees drawn up in
  turn, hands on the belly, rolling). Each loops: its last frame (24/28/30) equals frame 0.
  Keyframed stand-ins until recorded drills replace them (#167). Held gear is not keyed:
  the renderer drops it before a crawl.
- Gib pack `gore.glb` (`assets/gore/gore.py`): `intestine_0..2`, `guts_heap`, `organ_0..1`,
  `flesh_0..3`, `bone_0..3` (splintered cortex tubes with the marrow cavity showing: a
  spiral fracture with one long point, a split half-pipe sliver, a shaft end with its
  joint knob, a short crushed chunk), `skull_fragment`, `jaw` (a mandible on its base:
  body with chin, rami with condyle and coronoid, 14 teeth, torn masseter), `cloth_0..1`
  (material extras `acr_tint` = "nation": multiply the uniform colour in), `stump_cap`
  (torn: cannonball, explosion) and `stump_cap_clean` (sabre cut), unit discs in XY facing
  +Z built from rings that follow the tissue borders (torn: lumpy fibrous muscle with dark
  clots, an organic fat layer, the skin rolled back and ragged, bone proud by 0.25..0.55
  with sharp splinters around a recessed marrow cavity; clean: a wet flat face of muscle
  bundles parted by fascia, an uneven fat layer, an off-centre bone with a marrow dot, the
  skin retracted below the cut, a not quite round outline; materials skin, fat, meat,
  bone, marrow, cut meat, fascia, dark meat), and `torn_edge` (unit-radius ring around
  +Z, -Y down when the body lies: a low ragged band with three layers of tatters, meat,
  skin and coat cloth, 0.1..0.6 long, curling out and back over the rim, longest and
  sagging on the lower side, and four intestine loops hanging over the rim). Decay materials and masks as above
  (`COLOR_0`, `_GORE`, `_PART` 0); wet tissue carries `acr_roughness` (0.18..0.45) in its
  material extras and the glTF roughness. Each mesh's origin is its resting point; the
  node positions only lay the pack out on a grid.

### Faces and bodies (#283, `acr_assets/faces.py` pure, `heads.py`)

Every foot soldier and rider (engm18, engkri, engd18) carries the material for an
individual look; the renderer picks one per soldier (a later slice). Until it does, it skips
the variant meshes, so the game looks as before.

- **Head (pass 4, `acr_assets/mhead.py`):** the head and upper neck of the MakeHuman 1.x
  base mesh (CC0, `assets/blender/data/makehuman/head.json`; source, commit and licence in
  `assets/LICENSES.md`) replace the egg of the skin hull.
  - It is a young adult man at real size with the low-poly MakeHuman eyeballs, decimated
    (collapse, mirror-symmetric) to about 1300 tris.
  - The top of the skull sits on the hull's top. The neck is narrowed to the hull's neck
    at the cut and overlaps it by 18 mm. The hull keeps everything below the cut.
  - Hull weights: the head bone, blending into the neck.
  - The skull and jaw of the bone layer stay shrunk inside the face.
  - Head-local z is mapped through the head's landmarks (nose tip, mouth line, chin) onto
    the painted layout, so the face map, `_FACE` zones and hair curves of passes 1–3 land
    on the real features (`Frame.local`).
- **Body (#287, `acr_assets/mbody.py`):** below the head the skin hull is the MakeHuman 1.x
  base body (CC0, `assets/blender/data/makehuman/body.json`, from
  `assets/blender/tools/makehuman_body.py`), not the Skin-modifier hull: feet with heel,
  arch and toes, hands with fingers, real shoulders, neck and limbs.
  - Reposed onto skeleton H: the torso is mapped through its heights (hip joints onto the
    `thigh` heads, shoulder joints onto the `upperarm` heads); each limb segment (upper arm,
    forearm, hand, thigh, shin, foot) is rotated, stretched along its axis and moved onto
    its bone, blended by distance to the MakeHuman joint segments. `build["limbs"]` thickens
    the long limbs by a third of its excess, `build["width"]` widens the torso.
  - Decimated (collapse, mirror-symmetric) to about 2000 tris: with the head about 3200
    tris of flesh. The shoulder line is relaxed after the decimation.
  - The Skin-modifier hull is still built first: it gives the skull's place to the head.
    Weights, parts, shells and caps are made from the new hull as before. The thigh
    weights fade out above the hip joints (`mbody.belly_weights`), so a seated rider's
    belly stays flat.
  - Shells cut from the hull (`Character.shell`, `body.derive(border=True)`) relax their
    cut edges along the border curve, put them back on the skin and fold a hem inwards
    (`body.smooth_border`); part seams stay put.
  - Footwear (`gaiters`, `boots`, `shoes`, `boots_high`, `stockings`) leaves the feet to
    one closed `<unit>.footwear` mesh (`mbody.shoes`): per foot the convex hull of the skin
    below the ankle (a last: heel, arch, toe box) lifted 7 mm, with a sole and a low heel,
    the toe tapered and sprung (`TOE_TAPER`, `TOE_SPRING`), a buckle on the instep
    (material keys `[footwear, "brass"]`),
    weighted to shin, foot and toe. The skin inside the shoes is deleted.
  - `build["mh_body"] = False` keeps the old hull.
- **Face map:** the head part of the skin has its own material (second primitive of the
  flesh, `acr_tint` "skin") with a 512² texture on `TEXCOORD_0`.
  - The UV is a smart-project unwrap of the head (`heads.unwrap`, pass 5; the earlier
    cylinder folded the chin's underside onto the neck). The bake is dilated into the
    island margins (`heads.dilate`), so filtering shows no seams.
  - Pass 5 toned the paint down (dirt, pores, blotches, occlusion) so the face matches the
    body's skin; the stubble is fine and even (a shaved shadow and sparse hair points); the
    brows are full tapered bands. The beard zone (`_FACE` R) is relaxed over the mesh and
    scaled by 0.7, so the renderer's per-vertex stubble shows no wedges.
  - Painted from a Cycles bake: weathered skin, eyes with whites, iris, lids, lashes and
    crease, brows, nostrils, lips and the mouth line, folds, a cropped scalp along the
    hairline, a faint beard shadow.
  - Ageing for every man: uneven tone, sun spots, grime in the pores, forehead creases,
    frown lines, crow's feet, eye-bag lines, deep nasolabial folds, days of stubble.
  - Clean-shaven faces: `heads.finish(ch, beard=False)` leaves out the beard channel of
    `_FACE` (`faces.zones(..., beard=False)`; the renderer's per-soldier stubble) and the
    painted beard shadow; `scalp=False` paints no cropped scalp along the hairline
    (`faces.hairline(scalp)`), for shaved and plucked heads (a roach, a scalp lock). The
    natives, Plains and Pueblo pass these (before, `native.clean_shaven` swapped
    `faces.zones` and `faces.HAIRLINE` for the call).
  - The rest of the body keeps the shared outfit atlas. The dragoon rider, not dressed by
    `outfit`, gets the face map in his detail UV map.
- **Asymmetry:** `faces.asym` (nose knocked aside, one brow and cheek higher) is added to
  the base face; the morph fields stay mirror-symmetric.
- **Face morph targets** (flesh, and every shell or variant the fields reach: collar,
  hair, beards), weights `[lo, hi]` in root extras `acr_face_morphs`. Since pass 4 they are
  sums of MakeHuman targets (age and weight from the macro targets, the rest from the
  chin, cheek, nose, eyebrow, forehead, eye and head targets), scaled by `mhead.GAIN`.
  A field over the full-resolution head carries them to the decimated skin and to the
  shells: the nearest MakeHuman vertices' deltas, fading out 4 to 20 mm off the skin and
  above the neck seam.
  - `face_jaw`, `face_chin`, `face_cheekbones`, `face_nose` (length), `face_nose_bridge`,
    `face_eyes` (spacing): −1..1;
  - `face_brow`, `face_age` (jowls, hollow cheeks, eye bags, nose tip down), `face_fat`,
    `face_thin`: 0..1.

  10 face targets plus `body_stocky` and `body_thin`: 14 targets on the flesh with `deflate`
  and `emaciated`. All are position-only. A shell gets only the targets that move it.
- **Body:** `body_stocky` (flesh and clothes 14 % further from their bones) and `body_thin`
  (12 % nearer). Head, hands and feet stay fixed. Height is a uniform instance scale to
  `acr_height_m` = [1.58, 1.80]; the model is built at about 1.73 m.
- **`_FACE`** (vec4, flesh only):
  - R: the beard zone, where stubble grows;
  - G: the wrinkle zone (forehead, crow's feet, nasolabial folds);
  - B: pock marks and scars (cheeks, forehead);
  - A: the face front.

  The renderer shades stubble, wrinkles, pocks and a scar procedurally from these zones with
  per-soldier uniforms, as `ACR-analysis/faces/peek_faces.py` does.
- **Variants:** separate meshes `<name>.hair_short`, `.hair_tied`, `.hair_queue`,
  `.hair_loose`, `.beard_full`, `.beard_moustache` and `.beard_sideburns`.
  - Each is a smooth patch (`faces.PATCHES`): a grid of rays from the head centre to the
    skin, bounded by smooth curves (hairline across the forehead and temples, beard edge
    along the cheeks). It is lifted by a thickness that tapers to the skin on the open edges
    and has combed ridges; the open edge of hair, beard and moustache is ragged (every other
    column a lock reaching past the line, `faces.lock`). The queue is a tapered tube with a
    black ribbon (material 1). The hair material is matte (`acr_roughness` 0.92).
  - All are rigid on the head bone and carry strand UVs (`TEXCOORD_0`: u across, v from the
    root edge) on a neutral 256² streak texture, darker at the roots.
  - "full" shows `beard_full` and `beard_moustache`.
  - Drawn per soldier: at most about 460 tris (tied hair + queue + full beard). engkri lost
    its fixed `hair` shell; until the renderer picks variants, peasants show the painted
    cropped scalp.
  - They carry the decay layer `hair`, `_PART` 1, `_LIMB` −1, `deflate` and the face targets.
  - Root extras `acr_hair_variants` / `acr_beard_variants` map a choice to its nodes:
    `{"tied": ["hair_tied", "hair_queue"], "none": [], ...}`. `acr_look_odds` gives their
    odds: style `regular` for engm18 and engd18, `civilian` for engkri.
- **Tints:** material extras `acr_tint`: `"skin"` on the flesh material and `"hair"` on the
  variants' material (neutral base `(0.80, 0.78, 0.75)`). The renderer multiplies in a
  per-soldier colour from `acr_skin_tones` (sRGB multipliers per recruitment, `english`) or
  `acr_hair_colours` (sRGB; for hair, divide by the neutral base first).
- **`acr_face_frame`** `[x, y_front, eye_z]` (root extras): the head-local origin of the
  fields, in armature space.
- **Choice:** `faces.choose(seed, style)` is the reference. It gives the morph weights, hair,
  beard, hair colour (greys rise with age), skin tone, stubble, pocks, scar and height, all
  from a 32-bit hash (`_mix32`, `Rng`) that the renderer ports to Rust to get the same look
  per unit id.

## Family registry and held gear (#287, batch 0a)

### Family registry (`acr_assets/registry.py`, pure Python)

A family module (`line.py`, later `native.py`, `meso.py`, ...) registers what it adds at
import time instead of editing the shared tables. The shared modules read the registry's
dicts (`humans.LAYER_OF`, `humans.FALLBACK`, `outfit.PAINTER`, `outfit.KIND`,
`outfit.FINISH` and `drape.SPACING` *are* `registry.LAYER_OF`, ...). Registering a name
again with the same value is allowed; another value raises `ValueError`.

| call | adds |
|---|---|
| `registry.material(key, layer, painter=None, fallback=None)` | a material key: decay layer (`flesh hair bone wool linen leather wood metal hide`), outfit painter (None: keeps the detail tile), colour when the palette leaves it out |
| `registry.painter(name, fn=None, like=None, finish=(r, sheen, gloss))` | an outfit painter: `fn(p, n, rgb, L, seed) -> (N, 3)` linear colours (as `outfit.paint`), or `like` a built-in one (own atlas cell and colour, painted like it) |
| `registry.garment(name, draft, cloth, ease=None, spacing=3.5)` | a sewing pattern in `garments.GARMENTS` (`draft(measure) -> (panels, seams)`) with its sim settings (`garments.CLOTH` keys); drape it with `drape.dress(ch, garments=(..., name))` |
| `drape.dress(ch, garments=..., patterns={name: draft})` | a one-off draft that overrides or adds a garment for one unit |
| `registry.hat(name, fn)` / `registry.build_hat(name, obj_name)` | a hat builder, origin at the crown socket |
| `registry.gear(name, fn)` / `registry.build_gear(names, ch, name, spec)` | strapped gear, `fn(ch, name, spec) -> object` |

`line.py` is the example: it registers its hats (`tricorn`, `slouch`, `cocked`), its
strapped gear (`cartridge`, `knapsack`) and the `lapels` key and painter (lapels in an own
cloth, drafted through `patterns`). Its spec has two new keys: `fixed` (palette keys that keep
the palette colour: `Character.colours(fixed=...)` skips the sprite mix for them) and
`lapels` (`facings` or `lapels`). FrnM18 uses both: the sprite's chest is white, and the
uniform draft had cut the lapels from the red facings (the "red waistcoat"); now they are
white cloth, the waistcoat and lapels fixed. EngM18 rebuilds pixel-identical (blank field,
front and back; `ACR-analysis/humans-b0a/frames/{old,new}`, `frnm18_before_after.png`).

### Held gear (`acr_assets/held.py` pure, `gear.held` / `gear.hold`)

Library items as mesh data: origin at the grip (drum: the carriage hook; sling: the finger
loop; shield: the hand grip), long axis +Z, edge or face -Y. `gear.held(item, name,
keys={part: key})` makes the object (one material slot per key, `acr_material_keys`,
sockets as empties `<name>.socket.*`, vertex groups); `gear.hold(ch, item, name, socket,
matrix)` puts it on a socket as held gear (dropped at death). New material keys:
`vellum`, `cord`, `colour`, `hide`, `stone`, `feather`. Tests: `tests/test_held.py`
(sockets, tips, budgets, sizes); contact sheets `tools/held_sheet.py --row short|long`.

| item | tris (budget) | sockets |
|---|---|---|
| `drum` (side drum, shell in `facings`, rope cords) | 284 (400) | `head` |
| `drumstick` | 46 (50) | `tip` |
| `colour` (6 ft 6 in silk on a 9 ft 10 in pike; groups `cloth`, `pin`) | 130 (200) | `tip`, `top` |
| `smallsword` | 98 (150) | `tip` |
| `hanger` (1751, curved, brass hilt) | 86 (120) | `tip` |
| `officer_sword` (spadroon) | 90 (150) | `tip` |
| `spontoon` | 74 (120) | `tip` |
| `halberd` | 92 (150) | `tip` |
| `longbow` (Woodlands self bow, braced) | 152 (160) | `nock` |
| `arrow` | 32 (40) | `tip` |
| `quiver` (with 6 arrows) | 98 (220) | `mouth` |
| `spear` (head `metal`, remap to `stone`) | 62 (100) | `tip` |
| `war_club` (ball-headed, spike) | 150 (160) | `tip` |
| `tomahawk` | 56 (80) | `tip` |
| `macuahuitl` (obsidian blades, `stone`) | 78 (140) | `tip` |
| `sling` | 28 (60) | `pouch`, `release` |
| `shield` (round hide) | 138 (180) | `arm` |
| `pistol` | 92 (100) | `muzzle` |

All scripted Blender meshes, no third-party models (so nothing new in
`assets/LICENSES.md`); a CC0 replacement (Poly Haven, Kenney, Quaternius) can swap in
behind the same `Item` data later.

### Clips by weapon (`acr_assets/clips.py` pure, `clips.apply` in Blender; batch 0b)

Reusable clips for skeleton H, named as the `.MD` files name them; a family calls
`clips.unit_clips(md, weapons, melee=..., active=...)` (by `WEAPONKIND` index: index i plays
`ATTACK<i>`, `ATTACK` for 0), `clips.drummer_clips(md)` or `clips.colour_clips(md)` and keys
each with `clips.apply(arm, name, clip)`. Stand, walk and death stay the family's
(`poses.walk_cycle`, `poses.death_keys`). Each clip is a `Clip` ([(frame, pose)]) with
`event`: the frame of the release, heal or hit.

- **Timing.** A missile leaves (and a heal lands) on `SETACTIVEPOINT #ATTACKn f` rounded up
  to even (`clips.release_frame`: `combat.rs` (A), `max(f, 1)`, odd + 1); a melee blow on
  `poses.STRIKE_HIT_FRAME` (2). The family passes the active frames from its `.MD`.
- **Poses** are `poses.py` specs plus `reach` ({"r"|"l": (grip point, elbow pole)}, solved by
  `aim._solve`) and `show` ({socket: 0|1}: a socket's scale, keyed as a one-frame step: the
  thrown tomahawk, the loosed arrow are gone until the next is drawn). The grip points are
  computed from the item geometry (`held.py`): the stick's tip on the drum head, the arrow
  from the string through the bow's grip, the weapon's `socket.tip` on the target at the hit.
- **Item placement** the clips assume: bow on `socket.hand_l`, arrow, tomahawk, spear, sling,
  blowpipe, colour, sword, club on `socket.hand_r` (`gear.hold` default: upright in the
  socket's rest frame); drumsticks on both hands; the drum rigid on `hips` with its head's
  centre at `clips.DRUM_HEAD`, rotation `clips.DRUM_ROT` (the sheet tool shows how);
  shield on `socket.hand_l`.

| clip (builder) | units | frames | event |
|---|---|---|---|
| `ATTACK1`/`ATTACK2` bow: nocked low, raise and draw, full draw, loose, follow through, reach to the quiver, nock (`bow`) | `*Lut` (STRELA, OSTRELA) | 79 (IroLut) | release 44 |
| `PSTAND<i>`, `PATTACK<i>`, `UATTACK<i>` (bow ready) | `*Lut` | .MD | - |
| `ATTACK1` tomahawk throw: wind-up over the shoulder, cast, follow through, draw the next (`throw`) | IroTam (TAMAGAVK) | 50 | 16 |
| `ATTACK1` spear throw (`throw(..., "spear")`) | AtcMet (KOPIE) | 55 | 28 |
| `ATTACK1` sling: load, whirl overhead (a quarter turn every 2 frames), overhand cast (`sling`) | IncPra (KAMEN) | 68 | 56 |
| `ATTACK1` blowpipe: load, raise to the lips, blow (`blowpipe`; new item `held.blowpipe`) | IncTrb (VAFLIA) | 61 | 18 |
| `ATTACK`/`ATTACK1` heal: hands out over the man, then a slow rise and sink (`heal`) | SpnSha, `*Sha` (MEDIK) | 200 / 75-101 | 2 |
| `REST` drumming: alternate strokes every 2 frames, a flam every 8th (`drum_roll`) | `*B18` | 70 | - |
| `MOTION_L`/`_R` (and `_L0`/`_R0`) marching with the drum (`drum_march`) | `*B18` | 10 | - |
| `STAND`, `REST` colour waved (figure of eight, a sweep every 30 frames), `MOTION_L`/`_R` carried (`colour_*`) | `*F18`, `*F17` | 1 / 120 / 10 | - |
| `ATTACK_CUT_MID`, `ATTACK_CUT_HIGH`, `ATTACK_THRUST_MID` (lunge), `ATTACK_THRUST_DOWN` = `ATTACK_DOWN` (`sword_strikes`) | officers (MECH) | ATTACK's (52) | hit 2 |
| `ATTACK_CUT_MID`, `ATTACK_CUT_DOWN` = `ATTACK_DOWN` club or tomahawk hack (`club_strikes`) | natives (MECH) | ATTACK's | hit 2 |
| `ATTACK_THRUST_MID`, `ATTACK_THRUST_DOWN` = `ATTACK_DOWN` two-handed spear (`spear_strikes`) | DelPik, MayPik, IncPik (PIKA) | ATTACK's | hit 2 |
| shield block: `clips.with_shield(pose)` (a guard pose, no clip) | shields | - | - |

The melee index also sets `ATTACK` to the first strike clip (the `.MD` fallback). Targets
(docs/melee.md): a standing man's surface 1.50 m ahead (chest 1.15 m, head 1.55 m up), a man
on the ground at 1.6 m; the tests check the point at frame 2 within 1 cm of them (the clip's
data; the IK's reach is the next check, `strike_view::probe` on a built model: no unit with
these clips is built yet). Tests: `tests/test_clips.py`. Contact sheet:
`tools/clips_sheet.py --part clips|masks` (the MakeHuman test figure at each key frame).

### Lace and body paint (`acr_assets/masks.py`, `outfit.apply_mask`; batch 0b)

Painted masks over the outfit atlas, registered with `registry.mask(name, fn)`
(`fn(p, n, L, seed, **params) -> weights`). A character lists what it wears in
`ch.paint_masks` (entries from `masks.entry`, `masks.lace`, `masks.body_paint`);
`outfit.dress` paints each over the cells of the painters of its material `keys`, after the
cloth, in the lace or paint colour with a little mottle; with `frame` (an object name) in that
object's own frame. No entries, no change: EngM18 and FrnM18 are unchanged.
The face masks (`paint_face_half`, `paint_face_band`, the face dots) cover the face only:
down to the jaw (`masks.FACE_DOWN`, 23.5 cm below the top of the head) and 9.5 cm either
side of the nose (`FACE_HALF_WIDTH`); before, they reached every front-facing texel from
27 cm below the top up (the shoulders and upper arms), and native.py registered a limited
copy (`native_face_half`, now an alias).

- **Palette keys** (registered, with fallbacks): `lace` (gilt `(0.80, 0.64, 0.28)`; worsted
  white for soldiers via the palette), `paint_red` (red ochre), `paint_black`, `paint_white`.
- **Lace:** `hat_lace` (braid round the tricorn's brim edge, `gear.tricorn(lace=width)` /
  `cocked_hat(lace=...)` sets `acr_lace`; painted in the hat's frame), `buttonhole_lace`
  (bars across the lapels every 5.5 cm and across the cuffs along the forearm, on the
  `facings`/`lapels` cloth). Line spec key `lace`: `{"hat": 0.012, "buttonholes": True}`
  (default None); `registry.build_hat(name, obj, **kw)` passes `lace` on.
- **Body paint:** `paint_face_half`, `paint_face_band` (across the eyes), `paint_stripes`
  (chest and upper arms), `paint_hands` (hand prints on the chest), `paint_dots` (cheeks,
  chest); sets in `masks.PAINT_SETS` (`red_face_half`, `black_face_half`, `red_black`,
  `black_stripes`, `red_hands`, `white_dots`), picked by name or by an integer seed
  (`masks.paint_set(seed)`: deterministic). Per-instance paint in the game would need atlas
  variants (one atlas per model now): a family picks per unit, or builds variants.

### Straps (`acr_assets/straps.py`; batch 0b)

The crossbelts moved out of `line.py` into registered gear (`registry.gear`): `crossbelts`
(EngM18's two buff belts), `belt_l` (over the left shoulder to the right hip: the colour
belt, a pouch strap), `belt_r` (over the right shoulder to the left hip: the drum's carriage
belt). The family sets `ch.strap_surface`, `ch.strap_offsets`, `ch.strap_under` and builds
the names of `straps.STRAPS` its spec lists with the cloth, before `bone_layer`
(`line.make`). EngM18 and FrnM18 rebuild unchanged: glTF JSON, vertices and textures byte-identical to batch 0a's build; the only bytes that differ (262 and 348) are the triangle order of the hidden bone layer, the same triangles (Blender's nondeterminism; batch 0a saw 344). Builds in `ACR-analysis/humans-b0b/build.log`, sheets `sheet_clips.png`, `sheet_masks.png` there.

## 1. What the sprites tell

`ENGM18.MD` packs: `USERLC 1 EngM18G` (walk, `MOTION_*`), `2 EngM18B` (not referenced by any
animation line), `3 EngM18V` (`ATTACK1`/melee and the `TRANS*`/`PSTAND3` stance change, 1.5 MB,
the largest), `4 EngM18H` (`@DEATH 9 4 0 70` = 71 frames, `#DEATHLIE1 9 1 4 70` = its last
frame), `5 EngM18P` (`STAND` frame 0, `PATTACK`/`UATTACK`: reloading), `6 EngM18U`
(`@ATTACK 9 6 0 70`: firing). `EngKri`: `#STAND` in `EngKriStp`, `@DEATH 9 11 0 45` and
`#DEATHLIE1` frame 45 in `EngKriH` (46 frames); work packs `_Drv`, `_Str`, `_Urg`, `_Rub`, `_Ubrk`.

| | EngM18 #STAND | EngKri #STAND |
|---|---|---|
| top of head (hat) above ground point | 45–47 screen px → 52–54 world px → **1.75–1.8 m** | 45 px in all 9 views → 52 px → **1.73 m** |
| musket muzzle | 50–70 px (vertical in front views) | – |
| width (front) | 22 px ≈ 0.73 m incl. sleeves and musket | 21 px |
| below the ground point | 4–6 px (feet nearer the camera) | 5–7 px |

- **Musketeer:** red coat with long skirts open at the front, white waistcoat and breeches,
  black gaiters to below the knee, black tricorn, buff crossbelts (a yellow X on the chest,
  dominant at sprite size), brown knapsack, black cartridge box on the right hip. Musket in
  the right hand at the right side, slanted backwards. Dominant colours (`#a00000`, `#600000`,
  `#602000`, near-black, white `#e0e0e0` in the lying views).
- **Peasant:** dark red-brown tunic to mid-thigh with a red sash, light breeches/stockings,
  brown shoes, a brown slouch hat over long brown hair; arms held a little away from the body.
- **Death and lying:** both fall **forward, prone, head towards the facing direction**; the
  feet stay near the ground point (EngM18 `#DEATHLIE1` at k=4/west: 42 px forward, 14 px
  back), arms flung out above the head, the musket lies beside the body, the tricorn falls off
  (black blob near the head). EngKri lies the same way with arms forward.
- `#DEATHLIE2`/`#DEATHLIE3` exist for animals and a few humans (e.g. `EngStr`, `EngMus`); for
  `EngStr` all three are the same frame: they only keep the corpse longer (removal at 2000
  instead of 1200), they do not show decay stages.
- **What they don't tell:** faces, hands and anything smaller than ~2 px (a hand is 2–3 px);
  the far side (only 9 directions, east mirrored, so asymmetric gear swaps sides when
  mirrored); the underside and how the body lies on the ground; any anatomy; the colours
  include baked lighting from the upper left; the death frames are motion-blurred poses, so a
  3D clip matches only in timing and the start/end poses.

## 2. Skeleton H

59 bones (52 deforming), metres, rest pose = relaxed A-pose (arms 35° out), facing −Y, built
by `h_rig.build()`:

```
root (ground point, no deform)
└ hips ─ spine ─ spine1 ─ spine2 ─ neck ─ head ─ socket.hat
  │                         ├ socket.back
  │                         └ clavicle_[lr] ─ upperarm ─ forearm ─ hand ─ socket.hand_[lr]
  │                                                               ├ thumb_01..03
  │                                                               └ index/middle/ring/pinky_01..03
  ├ thigh_[lr] ─ shin ─ foot ─ toe
  ├ socket.hip_l (bayonet/sword)   socket.hip_r (cartridge box)
```

Retargeting a BVH drill (CMU/Mixamo-like) is a name map: Hips→hips, Spine/Spine1/Spine2→
spine/spine1/spine2 (Chest), Neck→neck, Head→head, LeftShoulder→clavicle_l,
LeftArm→upperarm_l, LeftForeArm→forearm_l, LeftHand→hand_l, LeftUpLeg→thigh_l, LeftLeg→shin_l,
LeftFoot→foot_l, LeftToeBase→toe_l, finger chains by name; the root motion goes to `root`.
Bone rolls are deterministic but not yet matched to a particular BVH convention: a retarget
must go through rest-pose world rotations (as Blender's retarget tools and `h_rig.pose` do).
`h_rig.pose()` takes Euler angles about the *armature* axes of the rest pose (relative to the
parent), which made the poses readable without caring about rolls.

## 3. Layers

| layer | made how | weights | tris (EngM18 / EngKri) |
|---|---|---|---|
| bones | stylised primitives: skull + jaw, 13 vertebrae, 5 rib hoops + sternum, pelvis ring, limb bones with knob ends, hand/foot blocks | **rigid**, one bone per part | 1892 / 1892 |
| flesh | Skin modifier around a 31-node graph of the rest skeleton, one subdivision, applied | smooth: nearest bone + parent/children by 1/d⁴ | 1836 / 1836 (+88 hair) |
| clothes | **shells derived from the body**: a face subset (by height and the face's bone), pushed out along the normals by 1–4 cm; straps ray-cast onto the body; skirt as a lofted tube | copied from the body (straps: same smooth rule) | 1796 / 1636 |
| gear | separate meshes parented to socket bones | – (bone parent) | 236 / 84 |

Total EngM18 **5760 tris**, EngKri 5536 tris, no textures (vertex colour + flat material
colours). `engm18.glb` (uncompressed, 3 clips sampled, 12 meshes, 13 materials): **601 KB**,
of which the JSON chunk is 151 KB.

Every decaying mesh carries the colour attribute `decay_mask` (glTF `COLOR_0`): R
low-frequency noise, rank-normalised per mesh (threshold x removes a fraction x of the
surface, so dissolving runs evenly in time), G per-vertex white noise (frays the edges),
B layer code / 8, A 1. Two shape keys (glTF morph targets) belong to the decay too:
`deflate` (flesh and clothes: every vertex moves 35 % towards its bone; the clothes collapse
onto the skeleton as the flesh goes) and `lie_flat` (coat/tunic skirt: squashes the rigid tube
so it lies flat on a prone corpse; 1 while lying).

### Realistic clothes, skin and leather (`acr_assets.outfit`, #217)

A realistic look matters more than matching the sprites (#214). After `Character.colours`
picks a colour per part (sprite and palette blended 50/50: the sprite colours alone came
out too dark in the game), `outfit.dress` paints one 1024² atlas for the dressed parts
the way the horse coats are painted (`coat.py`): each painter gets a rectangle through a
Smart-UV map `atlas` (the skin a double one), Cycles bakes rest-pose position, normal and
ambient occlusion per texel, and numpy paints:

- All objects of one painter are unwrapped together (`outfit.atlas_uv`), so their islands
  sit side by side in its cell. (Unwrapped one by one, each filled the whole cell and the
  bake kept the last one's texels: a native's scalp lock, key `hair` on the `skin` painter,
  painted its dark hair over the trunk, arms and legs.)
- Each object keeps its own colour (its key's, `Character.colours`) in its painter's style:
  the cell's texels are labelled by the UV triangles that own them (`raster_labels`,
  `grow_labels` into the bake margin) and painted per colour (`split_by_colour`). Two keys
  on one painter (`tunic` and `sash` on `wool`) no longer need a painter each.

| Painter | Parts | Look |
|---|---|---|
| `skin` | flesh, hair | tone variation, ruddy cheeks, stubble on the jaw, eyes (white, iris), brows, mouth, hair on the back and top of the head and a queue to the nape |
| `wool` | coat, tunic, sash, cloak | weave, vertical folds, worn patches, grime towards the hem, brass buttons down the coat front |
| `linen` | waistcoat, breeches, shirt, stockings | fine weave, grime towards the knees, small buttons down the waistcoat |
| `boots` | gaiters, boots, shoes | creases, mud at the feet, a button row on the outer side of the gaiters |
| `belts` | crossbelts | pipeclay with dirt |
| `felt` | hats, caps | felt grain and dust |
| `leather` | cartridge box, knapsack, saddle | creases and scuffs |

The colours are multiplied by the occlusion (`0.55 + 0.45 · ao^0.8`) so the form reads
under the game's flat light. Wood, metal (the musket), bones and stump caps keep their
materials. `EngM18` and `EngKri` use it; the glb grows by the atlas (about 1.9 MB).

### Two-handed aiming (`acr_assets.aim`)

The present/fire pose is solved with inverse kinematics instead of being hand-set: after
the body takes `poses.AIM_MUSKET` (torso turned so the left shoulder comes forward, head
bowed along the musket, the musket socket level along the facing), `aim.hold` puts the
butt in the right shoulder pocket, the right hand on the grip (the musket's origin, 0.25 m
in front of the butt) and the left hand under the barrel 0.34 m further on, lets Blender's
IK solve both arms (chain of two, elbow poles out and down), and writes the result back as
plain pose angles. `PATTACK`, `PSTAND` and `ATTACK` use it. `EngM18` reports the residual
errors in its stats (`aim_errors_m`: butt to shoulder joint ~8 cm, the pocket offset; left
hand ~2 mm).

## 4. Decay

One parameter `decay` ∈ [0, 1] per corpse; each material has its layer's windows
(`h_decay.LAYERS`, also in the glTF material `extras.acr_decay`):

| layer | discolour (tint) | dissolve | notes |
|---|---|---|---|
| flesh | 0.00–0.22 pale grey-green | 0.20–0.52 | dark rotten edge 0.07 |
| hair | 0.20–0.50 | 0.42–0.70 | |
| linen (waistcoat, breeches) | 0.05–0.45 dirty | 0.36–0.74 | |
| wool (coat, tunic, sash) | 0.10–0.55 darker | 0.44–0.84 | tatters |
| leather (gaiters, belts, hat, boxes) | 0.20–0.70 dark | 0.74–0.95 | last to go |
| wood / metal (musket) | 0.30–0.95 dark / rust | never | sinks with the rest |
| bone | 0.50–0.85 bleached | 0.94–1.00 | after sinking, avoids a pop |
| hide (animal coat, mane, tail; code 8) | 0.04–0.50 darker, browner | 0.30–0.72 | rot darkens a coat, never bleaches it (#212) |

Plus `deflate` weight 0.18–0.55 and sinking 0.78–1.00 by 0.20 m (the whole corpse incl.
dropped gear; the terrain hides it). Shader (Blender: `h_decay.material`; the same in WGSL):

```
mask   = 0.75 * color.r + 0.25 * color.g
colour = mix(base, tint, k * ramp(decay, d0, d1))
p      = ramp(decay, s0, s1);  th = p * (1 + edge)
if (mask <= th) { discard; }                  // alpha test, no blending, no sorting
if (p > 0 && mask - th < edge) { colour = edge_rgb; }
```

`ramp(x, a, b) = clamp((x - a) / (b - a), 0, 1)`. Per corpse the renderer needs one uniform
(or instance attribute) `decay`, the morph weights `deflate = ramp(decay, .18, .55)` and
`lie_flat`, and a vertical offset. All of it is WebGL2-friendly: vertex colours, two morph
targets, `discard` in the fragment shader; layers stay separate meshes (or are merged with the
layer code in B and a per-layer parameter table in a uniform array). The sprite renderer keeps
the original half-transparent fade (sprite-mode fallback).

**From the death counter** (docs/simulation.md "Death"; +1 per tick in `#DEATH`, +2 per tick
lying, 10 ticks/s; EngM18 lies at about counter 71 after its 71-frame `#DEATH`):
`decay = clamp((counter − 100) / (remove − 100))` with `remove` = 1200 (no `#DEATHLIE2`) or
2000; decay starts where the original turns the corpse half transparent.

| decay | counter (remove 1200) | ≈ s after death | stage (strip image) |
|---|---|---|---|
| 0 | ≤ 100 | ≤ 8.5 | `#DEATH` clip, then fresh lying, gear dropped (1) |
| 0.15 | 265 | 17 | discoloured skin, dirtier clothes (2) |
| 0.30 | 430 | 25 | face and hands dissolving, clothes start to collapse (3) |
| 0.42 | 562 | 32 | flesh mostly gone, linen tattering (4) |
| 0.55 | 705 | 39 | coat in tatters, skull and ribs show (5) |
| 0.68 | 848 | 46 | skeleton in rags, boots and belts left (6) |
| 0.80 | 980 | 53 | bones, leather going, sinking starts (7) |
| 0.90 | 1090 | 58 | bones half in the ground (8) |
| 1 | 1200 | 64 | removed, as in the original |

`#DEATHLIE2`/`#DEATHLIE3` need no pose change for humans (same frame); they only stretch the
timeline to 2000.

## 5. Results

Sprite check (`acr model-check`, EngM18 #STAND, 9 directions): **mean silhouette IoU 0.595**
(0.56–0.64), anchor shift (0..1, 3..4) px, render area 12 % smaller than the sprite.
EngKri #STAND IoU 0.46–0.70, EngM18 #DEATHLIE1 0.27–0.45 (pose only roughly matched).
Colours are flat material colours, not projected from the sprites (`acr_assets.project` is the
next step for the look).

Renders (in `--out`): `compare-EngM18-STAND.png`, `compare-EngKri-STAND.png`,
`compare-EngM18-DEATHLIE1.png` (per direction k = 0, 2, 4, 6, 8: sprite ×4 | model at sprite
size ×4 | model rendered at 4×), `decay-strip.png` (sprite `#DEATHLIE1` k=2, then the 8
stages above from the original camera, 3×), `decay-close.png`, `free-view.png`,
`turntable.png` (8 angles, 15° up), `engm18.glb`, `stats.json`, `compare.json`.

## 6. Insights and problems

1. **Scale fits:** 30 px/m gives 1.73–1.8 m for both figures; the peasant measures 45 screen
   px in all nine directions, a clean calibration target. Feet show 4–7 px below the ground
   point, which the model reproduces.
2. **Derived clothing shells solve interpenetration for free:** the same vertices with the
   same weights deform exactly like the skin, so clothes never cut into the body in any pose.
   Only free parts (coat skirt) and rigid parts can interpenetrate: the skirt is a stiff tube
   that bulged up 20 cm on a prone corpse until the `lie_flat` morph target, and it will cut
   into the legs when walking (needs a few skirt bones or a cloth bake).
3. **The decay reads well only with collapse:** dissolving flesh under intact clothes is
   invisible from the original camera; clothes must deflate onto the bones (morph target)
   and tatter later. Visually it reads as: discolour → face/hands go → clothes sag → rags on
   a skeleton → sink. Order of materials (flesh < linen < wool < leather < wood/metal) gives a
   natural progression without extra keys.
4. **Rank-normalised masks** make the dissolve proceed linearly in time; with raw noise it
   stalls and then jumps. Per-vertex masks on a ~1–2k tri mesh are coarse but enough at
   sprite size; the G channel frays the edges.
5. **Bone mesh is the most expensive layer** (1892 tris, a third of the model) and only
   visible in the second half of the decay: halve it (6-segment spheres → 4, fewer
   vertebrae) or make it a separate LOD swapped in at decay ≥ 0.3. Rigid bones stayed inside
   the flesh in standing and lying poses; at strongly bent knees/elbows the knobs come close
   to the skin (no poke-through seen yet with the placeholder clips).
6. **Strapped gear floats** once the body deflates (the knapsack hovers over the skeleton in
   `decay-close.png`): socket offsets must follow the `deflate` weight (or strapped gear drops
   to the ground like held gear). Held gear (musket, hat) drops at death as separate objects:
   the renderer detaches the socket child at the landing frame and keeps its world transform.
7. **No z-fighting in practice** with 1–4 cm shell offsets in Blender; in the game, depth
   precision at RTS distances is fine, but closely stacked shells (waistcoat 1.2 cm under the
   coat) should be culled where fully covered (saves overdraw and tris). Flesh under
   clothes is only ever seen through holes; an engine could skip it.
8. **Skin-modifier hull** gives a usable continuous body in 30 lines, but its winding is
   inconsistent (normals had to be recalculated) and junctions (armpits, crotch) are crude.
   Two script bugs worth remembering: `bmesh.ops.delete` renumbers vertex indices (key data
   by `BMVert`, not index), and Blender 5.2 creates new shape keys at value 1.0.
9. **Sprites vs model:** silhouette IoU ~0.6 with flat primitives and no tuning; the sprite
   figure is broader (sleeves, cuffs, knapsack) and the musket stands more upright. The pose
   (especially lying) dominates IoU more than proportions; a model check per animation frame
   will need the user's recorded clips.
10. **Speed:** full build (2 figures, ~70 Cycles CPU renders, glTF) in ~30 s; Blender headless
    on Windows ARM64 works without GPU. EEVEE was not needed.

## 7. Recommendations

- **#162 (pipeline):** add to `acr_assets` the bits this run needed: a rig module (skeleton H
  as data + armature build + `pose` in armature axes), body/shell derivation, strap casting,
  a decay-material helper that writes `extras.acr_decay`, and an export option for colour
  attributes + morph targets (`export_vertex_color`, `export_morph`). Per-asset budget for a
  foot soldier: ≤ 6k tris all layers, ≤ 4k without the bone layer, ≤ 600 KB uncompressed glb
  (meshopt should take it well under 200 KB; most of the JSON is sampled animation).
- **#166 (animation states):** clip names = `.MD` keys (`STAND`, `DEATH`, `DEATHLIE1`, ...);
  key placeholder clips on sprite frame numbers so `#DEATH`'s 71 frames map 1:1. Events
  needed: `land` (detach held gear, set `lie_flat`), `shot`, `impact`.
- **#168 (vertical slice):** keep skeleton H and the derived-shell approach for EngM18;
  project sprite colours onto the shells; add skirt bones; tune the lying pose per stored
  direction against `#DEATHLIE1`.
- **#178 (decomposition):** adopt the parameter set above (one `decay` uniform, mask in
  `COLOR_0`, `deflate`/`lie_flat` morph targets, sink offset), drive it from the death counter
  with the type's removal tick, keep the sprite fade as fallback. Horses: same scheme, bone
  mesh on rig R. Open: strapped gear following `deflate`, optional ground decals after
  removal, tuning the timing once corpses are seen in game.
