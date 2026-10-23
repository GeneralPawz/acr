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

## Known gaps

- The sprites' extra stances (`MOTION_*0..4`, `ATTACK2` (PIKA), `ATTACK3` / `PSTAND3`) are
  not modelled; the game plays the plain clips.
- `MECH` uses the musket's blows; the tomahawk stays in the belt (no held tomahawk clip,
  `line.make`'s musket path has no second hand weapon).
- No real fringe geometry: a fringe band shell at the cape's edge; no capot/tuque (no
  Canadian militia unit exists in the roster).
- Poses not fitted to the sprites (`tools/silhouette.py`).
