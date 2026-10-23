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

## Open points

- The drummer's jab is made with the right stick (the hanger is not modelled: a held item
  cannot be swapped mid-clip without a socket for it).
- The colour cloth is a flat sheet with a static wave; the runtime cloth is still to come
  (groups `cloth`/`pin` are kept on the cloth object).
- **The drum renders in one colour in game** (the shell's `facings` everywhere: no pale
  vellum head, no wooden hoops), both as rigid gear and as a held object on `hips`; the
  sprites show a white head and a brown shell. Next to check: how the renderer handles
  several material slots on a held object.
- In the blank-field idle the drummers stand in `STAND` (both sticks raised). The idle
  frames did not show `REST` (drumming). While walking, the right arm reaches forward
  nearly straight. Check `drum_march` on this rig.
- Sprite check: SpnM18's sprite reads as white with **red** facings; the model follows
  Kronoskaf's blue. The USA drummer's sprite is mostly white/blue, not the reversed red
  coat of the 1779 regulations. Both are kept as period choices.
- Strike probe (EngB18, the stick as weapon, `strike_view::probe`): ATTACK_THRUST_MID is
  0.58 m short in the clip alone and 0.05 m off with the layer; ATTACK_CUT_MID 0.69 / 0.13;
  ATTACK_THRUST_DOWN 0.94 / 0.32 (the stick is short, 0.28 m from the hand).
- Triangles: drummers 11 792, standard bearers 11 550, SpnM18/UsaM18 11 688.
