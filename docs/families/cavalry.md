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

## Standing in the stirrups (m2-cav-cut)

Both sabre cuts down (`SABRE_DOWN_R`, `SABRE_DOWN_L`; ATTACK plays the right one) stand in
the stirrups: `cavalry.stirrup_legs` moves the root by the pose's `root_loc` (12 cm up,
24 cm towards the cut's side) and solves each leg (thigh X/Y and the knee hinge, damped
least squares) so the ankle stays where the seat puts it, the foot kept level in its
stirrup (miss under 1 mm, `stats.json` `seat.stirrup_miss_m`). The bend starts at the
pelvis (`hips`, a free joint: the hips flex and the pelvis leans out over the stirrup on the
cut's side, 30-48°), so spine, spine1 and spine2 stay inside `limits.py`. m2-melee6's fixed
legs (thigh -42, knee 56) with the root lowered 2-10 cm lifted the feet out of the stirrups
and sat the hips into the saddle; its spine (X 60, side lean 38) was 15-23° over. The left
fist rests on the pommel's top (`pommel` +0.19 m; at +0.12 it sank 64 vertices in). The war
club (`CLUB_DOWN_R`) keeps the old legs.

`stats.json` `seat` (`cavalry_kit.rider_clear`): on every frame of STAND and the cut clips,
the rider's vertices more than 3 mm inside hide, saddle or saddlecloth, split by the bone
that moves them: `trunk` (hips, spine, skirts: the seat), `arms` (fists on reins and pommel)
and `legs` (thighs and boots straddle the barrel in every seated pose, STAND included: 197
EngD18, 269 SpnRei); the worst frame per clip, `trunk_frames` per frame, `rise_m` the hips
against STAND. `strike_off_m`: each strike pose's point off its target.

| EngD18 (SpnRei) | before | after |
|---|---|---|
| hips at the blow, against STAND | -0.10 to +0.01 m | +0.12 to +0.13 m |
| trunk inside at the blow: CUT_DOWN f2, CUT_DOWN_L f2, ATTACK f27 | worst frame 81, 154, 88 (fists counted in; STAND 73) | 4, 1, 4 (9, 6, 9) |
| spine joints past their limit, all clips | 81 frames, max 23° | 16, 15° (the reload) |
| point off its target, down R / down L | 0.08 / 0.02 m | 0.05 / 0.07 m (0.055 / 0.079) |
| scabbard push, cut R | 15 cm, 30°, 4 vertices inside | 7 cm (14), 30°, 0-1 inside |
| kit collisions, all clips | 121 (45) | 70 (25) |
| ATTACK sprite IoU | 0.648 (0.626) | 0.648 (0.626) |

Proof (`render_pose.py`, exact poses): `ACR-analysis/cav-cut/proof/sheet_cuts.png`.

## Renders

`ACR-analysis/fam-cavalry/` (blank white field, HUD off): `sheet_dragoons.png` and
`sheet_officers.png` (each unit: three sprite views, then side, front and close-up), `sheet_cuts.png` (EngD18 and UsaKOf18
attacking a crawling man, frames 8-38), `lineup/`, `lineup2/` and `melee/` the frames.
In that scene the officer (`ATTACK_RADIUS 0 20 60`, shorter than the dragoon's 40 80)
rides up onto the man and the cut is hard to see from the side; the dragoon's cut ends at
the man's body.

## Mounted bow and club (m2-shared-moves)

`held` "bow": the bow in the left fist (the fist turned upright: `bow_pose` sets the hand's
world orientation so the item's +Z is the bow), an arrow in the right fist (nock at the
fist), a quiver rigid on `spine2`. `bow_poses` turns the trunk left (spine Z -18/-16/-12°,
the head +40° back to the target), the bow arm out at shoulder height; `bow_keys` keys
ready, half, full draw, loose on `spec["release"]` (the `.MD`'s SETACTIVEPOINT rounded up),
follow through, lower, the hand to the quiver, draw, nock, ready; the ready stances hold
the nocked bow. `blade` "club" draws a `held.war_club` on `socket.blade_r`. Units: SiuKLut
(bow, club), SiuKVog (club), PueVsd (`hat` "band").

## 17th century (m2-cav17)

EngD17, FrnD17, SpnD17, EngKOf17, FrnKOf17, SpnKOf17 and SpnRei wear `dress` "buff17"
(`cavalry.dress17`): breeches, a buff coat (a shell keyed `buff`, hooked shut, the skirt a
tube to 0.60 m above the rider's origin, i.e. above the knee), sleeves of cloth (`dsleeve`,
troopers: the sprite's coat colour) or of buff (`sleeves` "buff": officers, the reiter), a
waist belt, the officers' sash (`uniform`), bucket-top boots (`drape.bucket_boots`) and a
falling band (`drape.falling_band`) modelled on the shells. `paint` runs `outfit.dress`
(the buff painter, linen, leather); `fixed` (`cavalry.FIXED17`: buff, band, boots) keeps
those palette colours against the sprites (`cavalry.PALETTE17`).

| key | values |
|---|---|
| `dress` | `"buff17"` |
| `sleeves` | `"dsleeve"` (troopers) or `"buff"` |
| `gun` | `"wheellock"`: a short long arm (`held.matchlock(length=0.95)`) held like the pistol, its muzzle socket dropped (`strike_view::weapon`) |
| `hat` | `"zischagge"`: lobster-pot helmet (skull, low comb, peak with three face bars, cheek pieces, four-lame tail; `plume`: red and white plumes) |
| `arms` | steel arm defences (`metal` shell over the sleeves) |
| `holster` | the second of a pair of pistols in the left saddle holster (`holster_pistol`, rigid on the horse's `saddle` bone) |
| `paint`, `fixed` | the realistic atlas; palette keys that keep their colour |

| type | hat | arms |
|---|---|---|
| EngD17, FrnD17 | stocking cap (the sprites show one, no helmet) | wheellock carbine, sword drawn for the cuts |
| SpnD17 | grey broad hat | wheellock carbine, sword |
| *KOf17 | broad hat; buff sleeves, sash | pistol, sword |
| SpnRei | zischagge with plumes; cuirass, arm defences, buff sleeves | a pair of pistols (hand, left holster), sword drawn for the cuts |

The sprites show no cuirass on the dragoons and no helmet but the reiter's, so the troopers
have none. SpnRei's sprite shows bright steel (not black), so its `metal` stays steel.
Before/after sheet: `ACR-analysis/cav17/sheet_cav17.png` (sprites, old, new, head).

Known: the falling band is ragged along its edge (as on foot); the buff skirt is a tube
over the saddle, not draped; the 18th-c. ones keep `dress` "coat".

## The reload in the saddle

The dragoons' `.MD` comments out `SLOWRECHARGE`: the original loads mounted inside
`@ATTACK1` (156 frames, shot on frame 20). Every rider with `gun` "carbine" or "wheellock"
and no `held` item (`reloads`) gets `poses.MOUNTED_RELOAD` there (`reload_clip`): across
the body to half-cock, take, bite, prime and shut; upright by the horse's right shoulder to
pour, draw the rammer, ram once, return it; poise and present. The reins stay in the left
hand, which holds the fore-end; the horse plays `Idle`. `stats.json` `reload` (fists from
their goals as played). Details in `docs/assets-humans.md`, "Reload drill". Sheet:
`ACR-analysis/genai/dragoon-reload/engd18_reload_sheet.png`.

## The horse: detail, reins, hair, levels (#233)

`mount.PARTS` is the master: the pack's hide whole (28.3 k triangles; at 9 k it showed
facets over barrel, neck and croup), hair cards 14 k, bridle, bit rings and reins
(`saddle2`) 9 k. `mount.lods` adds three authored levels of the horse mesh
(`mount.LOD_PARTS`: 14.8 k, 5.3 k, 1.9 k; `extras.acr_lod` / `acr_lod_of`, materials shared,
docs/rendering-3d.md "Authored levels"); `stats.json` `horse.lod_triangles`.

`mount.follow_reins` keys the pack's rein ends (`reins_01_<side>`, `posFix`) on every frame
of every clip: the left rein in the left fist (`socket.hand_l`), the right one in the right
fist while that is on the reins, else in the left fist 3 cm to the right. A fist more than
0.25-0.40 m from its place in the seat (aiming, the mounted reload, a two-handed swing) or a
rider out of the saddle (thrown, corpses) drops the rein slack on the neck's crest, 10 cm
ahead of its rest end, so a rein never stretches to a raised hand. `stats.json`
`horse.reins`: the end's miss against its fist over the held side-frames before and after,
the frames dropped per clip, and the longest span from the rein's middle on the neck to its
end (engd18: 0.65 m at rest, 0.75 m keyed). Blender keeps channels a clip does not key from
the last clip shown, while the exporter samples each clip from rest: `follow_reins` and
`posecheck.Poser` (opened `--blend`) reset the bones to rest first.

`mount.hair_wind` gives the mane, forelock and tail cards `_WIND` (branch swing and flutter
from 0 at the hide to 1 at 0.6 m off it, a phase per 6 cm clump), so the game's wind sways
them on top of the pack's tail bones. The hair stays `alphaMode: MASK`; hide, eyes and tack
are opaque.

## Worn and saddle kit (m2-cav-kit)

SPEC `kit` (`cavalry_kit.KITS`: `d18` the 18th-century dragoons, `d17` EngD17/FrnD17/SpnD17,
`rei` SpnRei) puts the foot units' kit rules (`line_kit`, `kit_hd`) on the rider and his horse:

| piece | what | skinned |
|---|---|---|
| `carbine_belt` | solid buff belt, left shoulder to right hip (70 mm), laid over the coat at `kit_hd.BELT_OFFSET`, its inner face dropped; a steel swivel at its end | the garment under it, arms left out (`line_kit._weigh`, 4 bones); swivel rigid on `hips` |
| `waist_belt` | round the waist (`kit_hd.ring_path`, 45 mm), a buckle over its open ends | as above; buckle on `hips` |
| `pouch` | `gear_hd` cartridge box (0.85 scale) over the right hip, seated on the coat on its lowest gap (`line_kit.seat_back`, quantile 0) | rigid `hips` |
| `scabbard` | `gear_hd.scabbard_mesh` (sabre 0.88 m, sword 0.82 m) from the left of the waist belt, 40° back and 12° out, seated `SADDLE_GAP` outside hide, saddle, saddlecloth and the rider's leg in the seated pose; the hilt (`gear_hd` hanger hilt) in its mouth on `socket.hip_l`, hidden with the sheathed blade while the drawn one shows | rigid on its hanger bone `rider.kit_scabbard` (`hang`; under `hips`, goes with the thrown rider) |
| `portmanteau`, `roll` | the valise (0.50 x 0.095 m) and the rolled blanket on it (0.58 x 0.06 m), lying across the croup behind the cantle, laid down in `STAND` (`cavalry_kit.sag`, `reseat`: stiff, bending at most `BEND` 15 mm; not decimated, which crumpled them) | rigid on their own horse bone `kit_valise` (under `saddle`), keyed from the croup on every frame (`follow_croup`) |

**Valise bone (`cavalry_kit.follow_croup`).** The croup flexes 6–11 cm against any one back
bone in the gallop and the reiter's trot. The valise and roll sit on their own bone, keyed on
every frame of every clip. The key is the rigid fit (`rigid_fit`, Kabsch) of the hide vertices
under their footprint (163 vertices; the patch reaches `PATCH_MARGIN` 6 cm round it) to the
same vertices in `STAND` frame 0, where `reseat` lays them down. In a frame where the croup
bends under the fit to within `LIFT_CLEAR` (4 mm), the bone lifts along the fit's up; the lift
spreads to the frames beside it (`spread_max`). `stats.json` `kit.valise`: lift, least gap and
vertices inside per clip. Tried and dropped: lifting the valise clear of every frame (6–11 cm
off the croup at STAND), and skinning it like the hide under it (it swung into the rider).
`kit_hd.rigid` stacked one armature modifier per call, so the five re-rigs of the old bone
choice deformed the valise five times over in Blender. The export skinned it once, so the old
`kit.collisions` for it were wrong.

**Scabbard hanger (`cavalry_kit.hang`).** The scabbard and the hilt's `socket.hip_l` hang on
`rider.kit_scabbard`, whose head is the belt hook (`HOOK`, 7 cm above the seated mouth). It is
keyed on every frame of the live clips. It goes with the hips at the hook but keeps its `STAND`
angle against the horse. Where it would meet the hide, saddle or saddlecloth, the straps let it
out sideways (up to `STRAP` 15 cm) or swing it out about the hook (`HANG_OUT`, up to 30°),
whichever moves it less. Both spread over neighbouring frames. `stats.json`
`kit.scabbard_hang.pushed`: the push per frame (cm) and the vertices still inside. Swinging it
about its mouth (the old `swing`) did not help: the mouth itself met the saddle.

The kit replaces the ribbon `belt` (`bd.strap`) and the box sabre (`gear.sabre`). The saddle's
pistol holsters stay (`military_saddle`). No `acr_lod` nodes; every material through the
palette keys (`cavalry_kit.MAT_KEY`). `stats.json` `kit`: triangles and gaps per piece,
`valise`, `scabbard_hang`, and `collisions` per clip (`cavalry_kit.collide`: rider items against coat,
hide, saddle, saddlecloth; saddle kit against hide and rider). `ACR_NO_KIT=1` builds a unit
without it and measures the old sabre box the same way (before/after). `kit_hd.is_arm` and
`trunk_weights` now read the `rider.` prefix (the strap weights kept the arm bones before).

## Not yet

- Facings, lace and sash are shells and thin geometry; at game zoom the lapels read as
  part of the waistcoat, the lace as a light edge. The colours come from the sprites
  (`mix` 1), so a facing the sprite barely shows takes the coat's or waistcoat's colour.
- The coat is still the shell coat (no draped skirts, no turnbacks); the 18th-c. carbine is a box.
- The French bonnet's turban is plain cloth, not fur.
- The saddlecloth has no lace border (the sprites show yellow borders on French and
  English cloths).
- The officers' pistol rides upright on the thigh like the carbine.
- The officers (`*KOf*`) and standard bearers carry no kit yet.
- In the cut down right (`ATTACK`, `ATTACK_CUT_DOWN`) the rider stands clear of the seat, but
  his pelvis moves 24 cm over the right stirrup, so the left hip's scabbard hook comes over
  the saddle: it still swings the full 30° out and the straps let it out 7 cm (EngD18) to
  14 cm (17th c., SpnRei) (`kit.scabbard_hang.pushed`). A rider standing that far out would
  lay it over his left thigh.
- The sword wrist in the cuts is bent past `limits.py` (hand_r 50-74°, the blade's line
  against the forearm from `solve_strike`), as before; the forearm already rolls
  (`armature.settle_arm`).
