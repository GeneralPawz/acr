# Family 11: frontiersmen and irregulars (`acr_assets/frontier.py`, #287)

Units: `EngTap` (trapper), `EngTapDragon` (jäger, the neutral hunter; it plays EngTap's
sprites), `UsaMil` (militia), `FrnBuk` (buccaneer). All carry a long gun (`VISTREL`) and
fight hand to hand (`MECH`; EngTap and FrnBuk list `2 PIKA` as well). Scripts:
`assets/units/{engtap,engtapdragon,usamil,frnbuk}.py`; tests
`assets/blender/tests/test_frontier.py`. `frontier.make` is family 12's base too
(docs/families/conquista.md).

## What the sprites show and the model

| unit | sprite (walk pack) | model |
|---|---|---|
| EngTap | tan fringed shirt, red at the waist, buckskin legs, light cap | linen hunting shirt to mid-thigh with a shoulder cape and a fringe band, red sash, buckskin leggings, shoes, fur cap with a tail, horn and shot pouch on a strap over the left shoulder, tomahawk and knife in the belt, long rifle (1.62 m) |
| EngTapDragon | (EngTap's) | the same in green (shirt and sash), a black round hat (sprite mix 0.15: the sprites are the trapper's) |
| UsaMil | white shirt, red-brown vest, dark hat, pale legs | shirt sleeves under a red-brown waistcoat, brown breeches, pale stockings, black tricorn, horn and pouch, musket |
| FrnBuk | red cloak, broad brown hat, dark breeches | linen shirt bloused over a red sash, dark breeches and leggings, red cloak over back and shoulders, broad hat turned up on the left, horn, pouch, knife, long buccaneer musket (1.72 m) |

## Garments

Draped like the civilians' (`drape.dress(garments=("breeches", "gaiters", "coat"))`): the
upper garment is `civil.peasant_coat` (the coat without lapels, collar, cuffs and
turnbacks, the front sewn shut): with skirts the hunting shirt, without them a shirt or
waistcoat; leggings and hose are the gaiter pattern without the garter (`frontier.legs`),
in their own cloth key. Shells over the cloth (`SPEC["layers"]`): `cape` (shoulder cape
and a wider fringe band at its edge), `blouse`, `sash`, `cloak`; family 12 adds `cuirass`,
`tassets`, `slops`.

Registered here: cloth keys `hunting`, `leggings`, `fsash`, `fur`, `horn`, `pouch`,
`fcloak`, `fshirt`, `fvest`, `fhose`, `doublet`, `dsleeve`, `slops`, `plume`, each with its
own painter (from when `outfit.dress` painted one colour per painter; now per key); gear `horn`, `pouch`, `tomahawk_belt`, `knife`; hats
`fur_cap`, `round_hat`, `buccaneer_hat`.

## Clips

The musketeer clips of `line.py` (`line._musket_clips`: present, fire and reload with the
`.MD`'s `SETACTIVEPOINT #ATTACK1`, the rammer; the bayonet / butt blows of
`poses.musketeer_strikes` on ATTACK's frames; walk, death, crawl). Only the clips the family
makes are listed in the glb (`frontier.CLIPS` plus the strikes and crawl).

## Trapper carry and gait (#347)

`EngTap` runs on its 8-frame `MOTION_L` (step 80, as the native foot: the generated run,
`poses.motion_gait`) with a short gun (0.95 m) held in both hands at the belt, muzzle forward
and down (`SPEC["carry"] = "waist"`: `poses.CARRY_WAIST` solved by `aim.hold`; the stand,
the steps and the presents start from it, the left arm stays on the gun). The `.MD`'s
`TAKERESSTAGES FOOD 8 5 3` and `RATE 4 8` make a hunter with food walk stance 5 at half
speed on `#MOTION_L4` (10 frames a step); the family now makes `MOTION_{L,R,LB,RB}4` as a
walk (`frontier.WALKS`), where the plain run clip played at half speed before (the gallop).
Sprite check (`--anim`, first frame, mean IoU of 9 directions): MOTION_L 0.508 -> 0.523,
MOTION_L4 0.484, ATTACK1 0.466 -> 0.474, ATTACK3 0.397 -> 0.398, STAND 0.549 -> 0.508.
`EngTapDragon` (same sprites) is unchanged.

## Trapper reload and load (#347)

The `#ATTACK3` sprites show the short rifle grounded and upright, the right hand rising to
head height on the rod and sinking again, twice over the 84 frames. `SPEC["reload"] =
"trapper"` (`poses.STYLES`) plays that: `poses.reload_steps(0.95)` is the musketeer's drill
with the hand distances along the short barrel (the left hand near the muzzle, the butt
nearer the man, square on instead of bladed), ATTACK3 rams twice (`TRAPPER_RAM`) and
ATTACK1, PSTAND3 and the transitions rest on the low stroke so the clips chain. The rammer
is as long as the barrel allows (`aim.rod_for`). The rammer, IK grips and `solve_holds` are
the musketeer's.

`SPEC["load"]` puts a load of pelts (`gear.pelt_load`, two rolls) on the right shoulder: the
bone `socket.load` on `clavicle_r` carries it skinned rigidly, and `frontier.key_load` scales
it to 1 in `MOTION_{L,R,LB,RB}4` and to 0 in every other clip (like the sidearm sockets). The
`MOTION_L4` sprites show a pale bundle over the right shoulder. It is skinned gear, not
held, so it does not drop at death; the death clips hide it.

Sprite check (`--anim ATTACK3 --step N`, mean IoU of 9 directions, `acr model-check --step`
now also renders the model at that frame): step 0 0.398 -> 0.434, step 21 (the hand high)
0.433 -> 0.468, step 63 0.432 -> 0.469; MOTION_L4 0.490 -> 0.489 with the load.

Not modelled: the Büffeljagd felt outfit and beaver hat. The kit has no felt garment or
beaver hat; it would take a new hat in `registry.HATS` and a felt coat pattern for
`civil.peasant_coat`, plus a mission-specific outfit switch (the sim would have to tell the
renderer which skin a unit wears), which is a subsystem of its own.

## Known gaps

- The sprites' extra stances (`MOTION_*0..4`, `ATTACK2` (PIKA), `ATTACK3` / `PSTAND3`) are
  not modelled; the game plays the plain clips.
- ~~`MECH` uses the musket's blows~~ (m2-shared-moves): EngTap, EngTapDragon, UsaMil and
  FrnBuk draw a tomahawk for melee (`line.with_sidearm(frontier.make, "tomahawk")`, the
  musket slung on the back; docs/melee.md). FrnBuk probe at frame 2: `ATTACK_CUT_MID` 0.00,
  `ATTACK_CUT_DOWN` 0.00 at 2.0 m (layer 0.04). The belt tomahawk of `tomahawk_belt` still
  shows while the drawn one is in the hand.
- No real fringe geometry: a fringe band shell at the cape's edge; no capot/tuque (no
  Canadian militia unit exists in the roster).
- Poses not fitted to the sprites (`tools/silhouette.py`).
