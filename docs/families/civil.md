# Family 13: European civilians (`acr_assets/civil.py`, #287)

Units: `EngKri`, `FrnKri`, `SpnKri` (peasants, `USAGE PEASANT`, `WEAPONKIND 0 MECH`) and
`SpnSha` (the priest every European nation fields, `MEDIK`). The game has no female
peasant unit, so all four are men. Scripts: `assets/units/{engkri,frnkri,spnkri,spnsha}.py`;
tests `assets/blender/tests/test_civil.py`.

## What the sprites show

| unit | sprite | model |
|---|---|---|
| EngKri | dark red smock to mid-thigh, red sash, light breeches, slouch hat | smock-frock (`tunic`), sash band, linen breeches, grey stockings, shoes, slouch hat |
| FrnKri | white shirt sleeves, dark slate waistcoat, blue-grey breeches, hat with a red band | one draped garment: body panels in `waistcoat`, sleeves in `shirt`; blue-grey breeches, pale stockings |
| SpnKri | white shirt, red sash, red-brown breeches | shirt bloused at the waist, sash, red-brown breeches |
| SpnSha | a Dominican: white habit, black cappa with a hood, red book, tonsure | white habit to the ankle, black cappa open at the front, hood collar, red breviary in the left hand, wooden cross on the chest |

## Garments

The peasants' clothes are draped (docs/clothing.md) from the uniform's patterns, cut down
in `civil.py` and passed to `drape.dress(patterns=...)`, so the shared drafts stay
unchanged:

- `peasant_coat(body_key, sleeve_key, skirts)`: the coat without lapels, collar, cuffs and
  turnbacks, the front sewn shut neck to hem (`cf` to its mirror, replacing the hook). With
  `skirts` a smock-frock (EngKri), without them a shirt to the waist. Body and sleeve panels
  may take different cloth: FrnKri's waistcoat over shirt sleeves in one layer (no layer
  fight between a 20 cm-ease shirt and an 8 cm-ease waistcoat).
- `stockings`: the gaiter pattern without the garter, keyed `stockings`.
- Breeches as the uniform's; shoes over the last (`mbody.shoes`); the sash is a shell band.

The priest is built from shells and lofted tubes (habit to the ankle on the skirt bones,
cappa over the back and shoulders with a 70° front opening, a hood roll at the neck).

Registered here: material keys `sack`, `habit`, `cappa`, `book`; hat `civil_slouch`.

## Tools and sockets

Tools are `held.Item`s defined in `civil.py` (`ITEMS`, `BUDGET`), not in `held.py`: `axe`
(68 tris), `sickle` (66), `hammer` (32), `sack` (100), `bundle` of firewood (112), `book`
(24), `cross` (24). Each tool hangs on its own non-deforming socket bone added to skeleton H
(`civil.skeleton()`): `socket.axe`, `socket.sickle`, `socket.hammer` (children of
`socket.hand_r`, so a clip's `world` rotation turns them), `socket.sack` (left shoulder),
`socket.bundle` (right shoulder), both under `spine2`. Every clip keys the scale of all
five sockets on its first frame (`show`: 1 for its tools, 0 for the rest), the crawl clips
too.

## Clips (ENGKRI.MD; FrnKri, SpnKri the same layout)

| clip | sprite pack | content | event |
|---|---|---|---|
| `ATTACK1` (27) | `U_Rub` | axe: wind-up over the right shoulder, flat diagonal cut into the trunk, follow-through; wood and stone (`TAKERESSTAGES ... 2`) | 15 (`SOUND 15 #ATTACK1 TREE1`) |
| `ATTACK3` (57) | `U_Ubrk` | stooped, left hand gathering, three sickle strokes | 14, 33, 52 |
| `WORK` (21) | `U_Str` | kneeling, left hand on the peg, one hammer blow | 5 (`SOUND 5 #WORK STROIT`) |
| `MOTION_*2`, `PSTAND2`, `ATTACK2` | `G_Urg` | walking with the grain sack on the left shoulder | - |
| `PATTACK2` / `UATTACK2` (86) | `P_Urg` | stoop and shoulder the sack / put it down | - |
| `MOTION_*4`, `PSTAND4`, `ATTACK4`, `PATTACK4` / `UATTACK4` | `G_Drv`, `P_Drv` | the same with the firewood on the right shoulder | - |
| `ATTACK`, `ATTACK5`, `ATTACK_CUT_MID`, `ATTACK_CUT_DOWN`, `ATTACK_DOWN` (53) | `U` | `clips.club_strikes` with the axe's edge (`AXE_TIP`) | 2 |
| `STAND`, `REST`, `MOTION_*`, `MOTION_*1/3/5` | `Stp`, `G` | stand, look round, walk (stances 1, 3, 5 walk empty-handed as the sprites) | - |
| `DEATH`, `DEATHLIE1` | `H` | `poses.death_keys`, `lie_prone` | - |

The priest: `STAND` (hands before the chest, book in the left), `REST` (reading), walk,
`ATTACK` (200 frames) = `clips.heal` (heal on frame 2, `SETACTIVEPOINT #ATTACK 1`),
`DEATH`. No flee clip exists in the `.MD` files; fleeing plays the walk.

## Known gaps

- Per-instance clothing colours: the renderer tints only skin, hair and the gun crew's
  coat per instance (`Tint`); the peasants vary by face, hair and build only.
- The sim credits stone with the axe clip (`ATTACK1`, as the original); there is no pick.
- Carry stances follow the sprites: the sim's carry stance (5 for wood and stone, 3 for
  food) walks on the plain walk sprites, so loads show only in stances 2 and 4.
- No tonsure on the priest (the head variants have no bald crown yet).
- The palette keys are the family's own (`smock`, `band`, `vest`, `hose`, `habit`, `cappa`,
  `sack`, `book`, each with its own painter): `outfit.dress` paints one colour per painter,
  so two keys sharing a painter (`tunic` and `sash` on `wool`) come out in one colour.
- The tool sockets are keyed to scale 0 by most clips; the export resets every pose bone's
  scale to 1 first, or the zero scale is baked into the tools' node transforms.
- Look (first pass, ACR-analysis/fam-civil): the clothes and the priest read well; the
  stockings show small skin holes on the shins (the gaiter pattern's seam), the sickle is
  hard to see in `ATTACK3` (small, close to the body), the reaping stoop leans sideways,
  the sack sits too far back on the shoulder, the shell edges of the priest's cappa are
  ragged. Poses were set by hand, not fitted to the sprites (`tools/silhouette.py`).
