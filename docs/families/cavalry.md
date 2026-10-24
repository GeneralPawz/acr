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

`mount.follow_reins` keys the pack's rein ends (`reins_01_<side>`, `posFix`) into the
rider's fists (`socket.hand_<side>`) on every frame of every clip: the left rein in the left
fist, the right one in the right fist while that is on the reins and otherwise in the left
fist 3 cm to the right (aim, cut, the mounted reload); both let go as the rider's root leaves
his seat (thrown, corpses). `stats.json` `horse.reins`: the end's miss against its fist over
the held side-frames, before and after, and the frames let go per clip.

`mount.hair_wind` gives the mane, forelock and tail cards `_WIND` (branch swing and flutter
from 0 at the hide to 1 at 0.6 m off it, a phase per 6 cm clump), so the game's wind sways
them on top of the pack's tail bones. The hair stays `alphaMode: MASK`; hide, eyes and tack
are opaque.

## Not yet

- Facings, lace and sash are shells and thin geometry; at game zoom the lapels read as
  part of the waistcoat, the lace as a light edge. The colours come from the sprites
  (`mix` 1), so a facing the sprite barely shows takes the coat's or waistcoat's colour.
- The coat is still the shell coat (no draped skirts, no turnbacks); the 18th-c. carbine is a box.
- The French bonnet's turban is plain cloth, not fur.
- The saddlecloth has no lace border (the sprites show yellow borders on French and
  English cloths).
- The officers' pistol rides upright on the thigh like the carbine.
