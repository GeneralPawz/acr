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

Tools are `held.Item`s defined in `civil.py` (`ITEMS`, `BUDGET`), not in `held.py`. The
hand tools are seen close up and keep to the gear budget of 0.5-1.5k triangles (#205,
`test_hand_tools_in_gear_budget`): `axe` (640 tris: turned haft with a butt knob, a head
lofted from the poll through the eye to a flared bit, `_loft`), `pick` (732: haft, eye block, a
long point forward curving down, a flat chisel behind; `PICK_TIP`), `sickle` (518: turned
handle, ferrule, a thin half-moon blade), `hammer` (568: round face, neck, split claw); then
`sack` (100), `bundle` of firewood (112), `book` (24), `cross` (24). They have no textures
or normal maps (flat palette colours like the other procedural gear). The European peasants
also wear a `dagger` (236) in a leather sheath at the left hip (`_belt_dagger`, seated on the
smock or breeches with `gear.seat`; `stats.seat`), not dropped at death. Each tool hangs on its own non-deforming socket bone added to skeleton H
(`civil.skeleton()`): `socket.axe`, `socket.pick`, `socket.sickle`, `socket.hammer` (children of
`socket.hand_r`, so a clip's `world` rotation turns them), `socket.sack` (left shoulder),
`socket.bundle` (right shoulder), both under `spine2`. Every clip keys the scale of all
sockets on its first frame (`show`: 1 for its tools, 0 for the rest), the crawl clips
too. Each tool node carries extras `acr_state`: the clips that show it (`tool_states`, from
the `show` keys; `stats.tool_states` counts them), so `render_views.py` and a renderer that
reads states on units hide it elsewhere.

## Clips (ENGKRI.MD; FrnKri, SpnKri the same layout)

| clip | sprite pack | content | event |
|---|---|---|---|
| `ATTACK1` (27) | `U_Rub` | axe: wind-up over the right shoulder, flat diagonal cut into the trunk, follow-through; wood and stone (`TAKERESSTAGES ... 2`); keyed on every frame with the left fist put back on the haft (`_dense_two`; `stats.hands.grips.ATTACK1`) | 15 (`SOUND 15 #ATTACK1 TREE1`) |
| `ATTACK1_STONE` (28) | none (the `.MD`'s `#WORKSTONE` is commented out) | pick: raised behind the right shoulder, weight back, swung down with trunk and knees, the point into the stone before the feet (`ROCK`), a lever back; left fist kept on the haft per frame (`stats.hands.grips`) | 15 (`SOUND 15 #WORKSTONE STONE1`) |
| `ATTACK3` (57) | `U_Ubrk` | stooped, left hand gathering, three sickle strokes | 14, 33, 52 |
| `WORK` (21) | `U_Str` | kneeling 0.38 m ahead of the ground point as the sprite (`WORK_FWD`), left hand on the peg, one hammer blow | 5 (`SOUND 5 #WORK STROIT`) |
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
- Work states of the original (ENGKRI.MD `TAKERESSTAGES`): wood and stone work stance 2
  (`#ATTACK1`, `U_Rub`), food stance 4 (`#ATTACK3`, `U_Ubrk`, harvest with the sickle),
  building `#WORK` (`U_Str`). The game plays `ATTACK1` for stone (`gather.rs`
  `Attack(work - 1)`); the renderer plays `ATTACK1_STONE` instead while the man's load
  (`carry_type`) credits stone or gold and the model has the clip (`three_d::stone_clip`,
  render only). No hoe: no sprite or state uses one. The stance clips `PSTAND1`/`PATTACK1`
  between blows still show the axe for a stone worker (no stone variants).
- Tools by state: the renderer reads each tool node's `acr_state` (`ModelData::states`, men
  of skeleton H only) and collapses a tool in every clip that does not name it
  (`ModelData::pose`, test `tools_show_in_their_clips`), on top of the socket scale.
- Tools at death: `three_d::held_gear` knows `axe`, `pick`, `sickle`, `hammer`; a killed or
  crawling man drops the tool his last clip alive showed (`tool_clips`, `holds`), laid
  beside him from the landing on (#206). The death clips hide the tools, so the tool is not
  in his hand while he falls (asset: key the held tool in `DEATH`).
- The tools have no LOD levels and no baked textures (flat palette colours with the
  outfit's decay materials).
- Spine X: positive bends the trunk forward (docs/assets-humans.md). The stoops of the
  reap, kneel and lift poses had it negative (leaning back); fixed with the pick clip
  (`--check` IoU ATTACK3 0.35 -> 0.59, WORK 0.23 -> 0.60). The chop keeps its values (its
  sprite check drops with a forward lean).
- Carry stances follow the sprites: the sim's carry stance (5 for wood and stone, 3 for
  food) walks on the plain walk sprites, so loads show only in stances 2 and 4.
- No tonsure on the priest (the head variants have no bald crown yet).
- The palette keys are the family's own (`smock`, `band`, `vest`, `hose`, `habit`, `cappa`,
  `sack`, `book`, each with its own painter). Since the shared look slice `outfit.dress`
  paints one colour per object (key), so two keys sharing a painter keep their colours; the
  own painters stay for their finish.
- The tool sockets are keyed to scale 0 by most clips; the export resets every pose bone's
  scale to 1 first, or the zero scale is baked into the tools' node transforms.
- Look (first pass, ACR-analysis/fam-civil): the clothes and the priest read well; the
  stockings showed small skin holes on the shins (the gaiter pattern's seam paired by
  sample count; arc length since the shared look slice, `garments.seam_fractions`), the sickle is
  hard to see in `ATTACK3` (small, close to the body),
  the sack sits too far back on the shoulder, the shell edges of the priest's cappa are
  ragged. Poses were set by hand, not fitted to the sprites (`tools/silhouette.py`).
