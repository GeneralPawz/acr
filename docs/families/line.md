# Family notes: line infantry, drummers and standard bearers (families 1 and 2, #287)

Batch 1. Module `assets/blender/lib/acr_assets/line.py`; unit scripts `assets/units/`
`spnm18.py`, `usam18.py`, `{eng,frn,spn,usa}b18.py`, `{eng,frn,spn,usa}f18.py`. The main
session folds these notes into docs/humans-roster.md.

## What line.py gained

- **`weapon`** `drum` and `colour` beside `musket`:
  - `drum`: `held.drum` on the `hips` bone (a held object: one material per part) with its head's centre at `clips.DRUM_HEAD`, axis
    `clips.DRUM_ROT` (the drum hangs on the left thigh from the `belt_r` carriage belt),
    shell in the unit's `facings`; a `drumstick` on each hand socket.
  - `colour`: `held.colour` split in two held objects on `socket.hand_r`: staff and finial
    (`wood`, `brass`), and the cloth (`*.colour_cloth`, groups `cloth`/`pin` kept) in an own
    material key `flag` with its own painter cell, so the design can be painted on it.
- **`flag`** spec key: the colour's design, `FLAGS` (painted masks in the cloth's own
  frame, registered here: `flag_cross`, `flag_saltire` (with `ragged` the Cross of
  Burgundy), `flag_stripes`, `flag_canton`), coloured by the palette keys `flag` (field),
  `flag_a`, `flag_b`, `flag_c`:
  - `union` (EngF18): the King's colour, the Union of 1707-1801 (no St Patrick's saltire);
  - `france` (FrnF18): the white cross through blue quarters (Gardes françaises, no lis);
  - `burgundy` (SpnF18): the red ragged Cross of Burgundy on white;
  - `stripes` (UsaF18): thirteen red and white stripes, a plain blue canton (1775-77).
- **Clips** (`support_clips`): `clips.drummer_clips` / `clips.colour_clips`, the backward
  walk (`MOTION_LB`/`_RB` and `_LB0`/`_RB0`: the forward clips played backwards, as the
  `.MD` plays them), the drummer's `0 PIKA` from `clips.unit_clips(md, {0: "PIKA"},
  melee="sword")` (`ATTACK` = `ATTACK_THRUST_MID`, plus `ATTACK_CUT_*`,
  `ATTACK_THRUST_DOWN`, `ATTACK_DOWN`), `PSTAND`/`PATTACK`/`UATTACK` as the stand, `DEATH`
  from `clips.STAND_FREE` to `poses.lie_prone`, the crawl clips. Keyed with `clips.apply`
  (the reach goals IK-solved).
- The musket path is unchanged (moved into `_musket_clips`).

## Units

| unit | dress | gear | notes |
|---|---|---|---|
| SpnM18 | white coat, blue cuffs, collar, lapels and waistcoat, white turnbacks and breeches | crossbelts, cartridge, knapsack, musket | coat, lining, waistcoat `fixed`; hat lace white |
| UsaM18 | blue coat faced red (1779, Middle States), white turnbacks, waistcoat, breeches | as EngM18 | waistcoat, lining fixed; hat lace white |
| EngB18 | royal livery: red faced blue, yellow lace on lapels and cuffs | `belt_r`, drum, sticks | |
| FrnB18 | King's livery: blue faced red, white lapels, white lace | `belt_r`, drum, sticks | |
| SpnB18 | Bourbon livery: blue faced red, red waistcoat, white lace | `belt_r`, drum, sticks | |
| UsaB18 | reversed colours (1779): red faced blue, white lace | `belt_r`, drum, sticks | |
| EngF18 | EngM18's uniform | `belt_l`, colour `union` | no knapsack, no cartridge box |
| FrnF18 | FrnM18's uniform | `belt_l`, colour `france` | |
| SpnF18 | SpnM18's uniform | `belt_l`, colour `burgundy` | |
| UsaF18 | UsaM18's uniform | `belt_l`, colour `stripes` | |

## Worn kit (m2-line-kit)

SPEC `kit` (`line_kit.py`, the `gear_hd` kit at regular-unit scale, draped garments only):
solid crossbelts laid over the coat at a fixed offset (`kit_hd.lay_strap`, 3 samples across (pack straps 2),
a row every 3 cm, trunk weights only) and the `gear_hd` items collapsed to `line_kit.TRIS`,
coloured by the unit's palette (`line_kit.MAT_KEY`: leather `cartridge`, buff `belts`,
`brass`, `metal`, canvas `knapsack`), each rigid on one trunk bone. They replace the
ribbons and boxes of `gear` (`line_kit.COVERS`). No authored LOD nodes: one `acr_lod` node
would switch the whole soldier to authored levels and drop its far meshes
(`model_renderer::prepare_model`). Per nation (`line_kit.KITS`):

| nation | pieces | why |
|---|---|---|
| Eng | crossbelts, oval belt plate, cartridge box, bayonet scabbard, knapsack, tin canteen | the battalion man after the 1768 warrant: no hanger, the plate on the bayonet belt; the issued kidney-shaped tin canteen |
| Frn | crossbelts, cartridge box, bayonet scabbard, hanger (sabre-briquet), knapsack, tin canteen | the fusilier's sabre on the waist belt, buckles, no plate; the tin `bidon` |
| Spn | as Frn, wooden canteen | sword and bayonet on the waist belt; a small wooden cask |
| Usa | crossbelts, cartridge box, bayonet scabbard, knapsack, wooden canteen | Continental line, no sword; the cooper-made "cheese-box" canteen |

The hip items rest on the first surface under their back, the coat skirt included
(`line_kit.seat_back`, turned onto it at most 35°, 4 mm off on the 30 % quantile of the rays:
on the outermost ray the pleats at the side vent held the box 0.136 m out; now 0.070 m,
median gap 0.029 m). The knapsack stays rigid on `spine2`: with `kit_hd.follow_back` the
items met in 234 vertices against 9. `collisions.coat_in_pack` lists the deepest coat
vertices: the 0.089 m one (rest y 0.094, 8 cm in front of the pack) is a false hit of the
nearest-face test on the pack's open strap sweeps.

**Canteens** (`canteen`: the `gear_hd` cheese box; `canteen_tin`: `line_kit.tin_canteen_mesh`,
a flat oval tin) hang behind the left hip under the pack on their own narrow strap
(`CANTEEN_STRAP`, 22 mm, over the right shoulder nearer the neck than the waist belt, laid
last over everything), seated on the skirt with `seat_back`. On the back of the hip the
skirt swings with the thigh (the HD kit: 98 coat vertices in the canteen in walk): the build
tries the canteen rigid on each of `CANTEEN_BONES` (`hips`, `thigh_l`), counts the clothes'
vertices inside it over `MOTION_L`/`MOTION_R` and keeps the fewer
(`kit.collisions.canteen` in stats.json). Numbers: `stats.json` `kit` (triangles per piece, seat shifts, collisions
per clip).

## Open points

- The drummer's jab is made with the right stick (the hanger is not modelled: a held item
  cannot be swapped mid-clip without a socket for it).
- The colour cloth is a flat sheet with a static wave; the runtime cloth is still to come
  (groups `cloth`/`pin` are kept on the cloth object).
- **The drum renders in one colour in game** (the shell's `facings` everywhere: no pale
  vellum head, no wooden hoops), both as rigid gear and as a held object on `hips`; the
  sprites show a white head and a brown shell. Next to check: how the renderer handles
  several material slots on a held object.
- ~~In the blank-field idle the drummers stand in `STAND` (both sticks raised)~~
  (m2-shared-moves): `STAND` is `clips.drum_ready`, both sticks low over the head, the
  tips 3.5 cm above it (the sim plays STAND nearly always, motion.rs). The drummer's jab
  (`0 PIKA`) uses `sword_strikes` with the sword's length on a 0.28 m stick: 0.58 m short
  (layer 0.01–0.05); a hanger sidearm would fit it.
- `line.spec(..., sidearm=...)` gives gun units a drawn sidearm (docs/assets-humans.md). While walking, the right arm reaches forward
  nearly straight. Check `drum_march` on this rig.
- Sprite check: SpnM18's sprite reads as white with **red** facings; the model follows
  Kronoskaf's blue. The USA drummer's sprite is mostly white/blue, not the reversed red
  coat of the 1779 regulations. Both are kept as period choices.
- Strike probe (EngB18, the stick as weapon, `strike_view::probe`): ATTACK_THRUST_MID is
  0.58 m short in the clip alone and 0.05 m off with the layer; ATTACK_CUT_MID 0.69 / 0.13;
  ATTACK_THRUST_DOWN 0.94 / 0.32 (the stick is short, 0.28 m from the hand).
- Triangles: drummers 11 792, standard bearers 11 550, SpnM18/UsaM18 11 688.
