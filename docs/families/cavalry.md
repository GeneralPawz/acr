# Family 5: dragoons and mounted officers (#287)

All fourteen types of [humans-roster.md](../humans-roster.md) §2 family 5 have a unit script
on the WildMesh horse (`assets/blender/lib/acr_assets/cavalry.py`): EngD18 (existing) and
the thirteen new ones below. A unit script is data: its `.MD` lines (from the game's
`.MD`, the `MOTION_*0` copies left out; FrnKOf17's long `#REST`/`#ATTACK` frame lists are
written as ranges of the same length) and a spec for `cavalry.build`.

## Spec (new keys)

| key | values |
|---|---|
| `hat` | new: `"tricorne"` (now the line's `gear.tricorn` instead of a box), `"bonnet"` (dragoon's stocking cap on a fur turban, the bag falling back), `"helmet"` (light dragoon's leather helmet, brass comb, horsehair crest), `"broad"` (17th-c. broad-brimmed hat, brim up on the left, an ostrich plume) |
| `plume` | tricorne: a feather edging round the brim (`crest`) |
| `facings` | cuffs and lapels (shells over the coat) in `palette["facings"]` |
| `lace` | officers: bands of lace on cuffs and lapel edges, a braid round the tricorne's brim (`palette["lace"]`); geometry, since the rider's clothes are shells and not painted by `outfit`, so `masks.lace` does not reach them |
| `sash` | officers: a waist sash (`sash`) |
| `blade` | the drawn blade (`drawn`): `"sabre"` (dragoons) or `"sword"` (officers, `held.officer_sword` on `socket.blade_r`) |
| `gun` | `"pistol"` is now `held.pistol` (officers: index 1 is `VISTREL`); the carbine is unchanged |
| `coat` | horse coats `cavalry.HORSE_COATS`: `black`, `bay`, `grey`, `dapple` (by the sprites) |

New material key `crest` (registered, layer `hair`); `lace` registered as `masks.py` does.
The held pistol's `socket.muzzle` is dropped: under `socket.hand_r` the strike layer takes a
muzzle before a `socket.tip` (`strike_view::weapon`) and would ignore the drawn sword.

## Units

Officers: tricorne (18th c.) or broad hat (17th c.), lace, sash, pistol and officer's sword
(no carbine). Dragoons: carbine and sabre. Colours guided by the palette, taken from the
sprites (`model-ref` views in `ACR-analysis/fam-cavalry/refs_sheet.png`).

| type | rider | hat | horse |
|---|---|---|---|
| FrnD18 | red coat, blue facings | grey bonnet, black turban | black, blue saddlecloth |
| SpnD18 | green coat, red facings and waistcoat | red bonnet | bay |
| UsaD18 | blue coat, white facings and waistcoat | leather helmet, white crest | bay |
| EngKOf18 | red, blue facings, gold lace, crimson sash | laced tricorne | grey |
| FrnKOf18 | red, blue facings, gold lace, white sash | tricorne with white feather edging | bay, blue saddlecloth |
| SpnKOf18 | dark green, red facings, gold lace, red sash | laced tricorne | black, red-orange saddlecloth |
| UsaKOf18 | blue, red facings, buff, gold lace, crimson sash | laced tricorne | black |
| EngD17 | red coat | red bonnet | dark dapple grey |
| FrnD17 | blue coat, red cuffs | blue bonnet | bay |
| SpnD17 | buff coat, red cuffs | grey broad hat, white plume | bay |
| EngKOf17 | scarlet, gold lace, crimson sash | black broad hat, white plume | bay |
| FrnKOf17 | light blue, gold lace, white sash | blue broad hat, blue plume | grey |
| SpnKOf17 | buff, gold lace, red sash | black broad hat, white plume | grey, green saddlecloth |

## The left cut down (`ATTACK_CUT_DOWN_L`)

m2-melee6's pose leaned out to the left (armature Euler Y) with the sabre in the right hand,
so the right shoulder stayed ~1.95 m up: 0.54 m short. It also bent the trunk with a
negative X, which turns out to lean *back* (positive X bends forward). The new pose
(`cavalry.SABRE_DOWN_L`): standing in the stirrups, the seat 0.25 m left and 0.15 m forward,
the trunk bent far forward (X 60/25/12° on spine, spine1, spine2), turned left (Z 25/15/10°)
with a little side lean (Y 20/10/5°): the right shoulder comes down and forward over the
horse's left shoulder (0.36 left, 0.51 ahead, 1.59 m up) and the blade goes ~80° down past
the neck: a cross-body cut. `cavalry.solve_strike` (was the `strike` closure) is module
level so a pose can be tried without a full build (`ACR-analysis/fam-cavalry/tools/pose_probe.py`:
one `make`, then trunk variants).

Probe (`strike_view::probe_clips`, frame 2, metres right/forward/up of the attacker):

| clip | target | clip alone: point, off | with the layer |
|---|---|---|---|
| `ATTACK_CUT_DOWN_L` (all dragoons, EngD18 rebuilt) | -0.61/0.73/0.20 | -0.60/0.77/0.24, **0.06** (was 0.54) | 0.01 |
| `ATTACK_CUT_DOWN` (=`_R`, unchanged) | 0.92/0.25/0.20 | 0.89/0.26/0.32, 0.12 | 0.07 |
| `ATTACK_CUT_DOWN_L`, officers (sword) | -0.61/0.73/0.20 | -0.60/0.73/0.24, **0.05** | 0.02 |
| `ATTACK_CUT_DOWN`, officers | 0.92/0.25/0.20 | 0.88/0.23/0.31, 0.12 | 0.07 |

## Renders

`ACR-analysis/fam-cavalry/` (blank white field, HUD off): `sheet_dragoons.png` and
`sheet_officers.png` (each unit: three sprite views, then side, front and close-up), `sheet_cuts.png` (EngD18 and UsaKOf18
attacking a crawling man, frames 8-38), `lineup/`, `lineup2/` and `melee/` the frames.
In that scene the officer (`ATTACK_RADIUS 0 20 60`, shorter than the dragoon's 40 80)
rides up onto the man and the cut is hard to see from the side; the dragoon's cut ends at
the man's body.

## Not yet

- Facings, lace and sash are shells and thin geometry; at game zoom the lapels read as
  part of the waistcoat, the lace as a light edge. The colours come from the sprites
  (`mix` 1), so a facing the sprite barely shows takes the coat's or waistcoat's colour.
- The coat is still the shell coat (no draped skirts, no turnbacks); the carbine is a box.
- The French bonnet's turban is plain cloth, not fur; the 17th-c. officers wear the
  18th-c. coat cut in buff or colour (no buff coat with sleeves of a different colour).
- The saddlecloth has no lace border (the sprites show yellow borders on French and
  English cloths).
- The officers' pistol rides upright on the thigh like the carbine.
