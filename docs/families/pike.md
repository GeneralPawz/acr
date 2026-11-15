# Family 6: pike and shot, 17th century (#287, batch 2)

`assets/blender/lib/acr_assets/pike.py`, unit scripts `assets/units/{engpik, frnpik, engmus,
spnmus, frns17, engb17, frnb17, spnb17, engf17, frnf17, spnf17}.py`; tests
`assets/blender/tests/test_pike.py`. Roster: docs/humans-roster.md, 2.6.

## Units

| Unit | `.MD` | Dress | Kit |
|---|---|---|---|
| `EngPik` | STAND, PSTAND, MOTION_* and *0, PATTACK/UATTACK 36, ATTACK 21 (`0 PIKA`), DEATH 71 | red coat, grey breeches | pot, back and breast with tassets, sword, pike |
| `FrnPik` | as EngPik | grey-white coat, blue breeches, red stockings | combed morion, back and breast, sword, pike |
| `EngMus` | STAND, MOTION_*, PATTACK/UATTACK/PSTAND, ATTACK 53 (`0 MECH`), PATTACK1/UATTACK1/PSTAND1, ATTACK1 68 (`1 VISTREL`), DEATH 40 | red coat, grey breeches, broad hat | bandolier with the apostles (`belt_l`), sword on a baldric (`belt_r`, `off_scabbard`), matchlock, match in the left hand |
| `SpnMus` | as EngMus | ochre buff coat, red breeches, broad hat | as EngMus |
| `FrnS17` | as EngMus (ATTACK1 87; REST and MOTION_*0 not built) | grey-white coat, blue breeches, red stockings | as EngMus |
| `EngB17`, `FrnB17`, `SpnB17` | STAND, REST 62 (SpnB17: PSTAND), MOTION_L/R, DEATH | red / blue faced red / yellow faced red, broad hat | side drum on `belt_r` |
| `EngF17`, `FrnF17`, `SpnF17` | STAND, REST 60, MOTION_L/R, DEATH | red / grey-white / ochre, broad hat | colour on `belt_l`: St George's cross (the `france` design in red on white), the French white cross on blue, the Cross of Burgundy |

`.MD` lines read with `acr cat <game> <TYPE>.MD`. Colours are the palette mixed half and half
with the `#STAND` sprites (`line`'s `mix`); belts, armour, match and scabbard fixed.

## How it is built

- Everything goes through `line.make` with the period's own drape set (`build["drape"]`,
  docs/clothing.md "17th century"): musketeers, drummers and standard bearers
  `MUSKETEER_DRAPE` (breeches, stockings, linen cuffs, a **doublet** with tabbed skirts in
  the coat colour); pikemen `PIKEMAN_DRAPE` (a **buff coat** with cloth sleeves in the coat
  colour, skirts to above the knee, under the corselet). All wear the **falling band**
  (`falling_band` gear, modelled over the doublet or buff coat).
- **Musketeers** carry the **matchlock** (`held.matchlock`: fishtail butt, lock plate, the
  serpentine holding a length of match over the pan; it replaces the line's flintlock
  mesh in the hand and on the sling, same frame and sockets) and the **forked rest**
  (`held.musket_rest`) in the left hand with the coiled match. `line.build` has no hook
  for its `make`: `pike.build` swaps `line.make` for the build's duration.
- Registered here: hats `pike_broad` (broad felt, brim cocked up on the left), `pike_morion`,
  `pike_pot` (both `armour.helmet` shells in steel, key `plate`); gear `pike_corselet` = `armour.corslet` (#522, shared:
  breast and back formed over the dressed trunk, medial ridge, rolled neck and arm edges, a
  waist flange (shoulder straps optional, off), all rigid on `spine1`; two tassets of 7 lames, each lame one
  weight between `spine1` and its thigh, so they hinge and slide without bending; painted
  steel, painter `plate`, metallic 1, roughness 0.4; `stats.armour`: fit gap, triangles and
  `poke` per clip), `pike_apostles` (nine charges down
  the front run of `belt_l` and a bullet bag), `pike_match` (a coil of slow match rigid on
  `hand_l`, lit ends); material keys `plate` (metal, `armour.py`), `match` (linen).
- The sword on its baldric is `belt_r` + officers' `off_scabbard` (registered gear, reused).
- **Musketeers** keep the line's musket path: present, fire and the line's reload drill
  (`aim.drill`, `poses.RELOAD_STEPS`); the musket is the line's flintlock shape. `ATTACK`
  (`0 MECH`, the sprite's butt stroke) plays the line's bayonet lunge. The stance-3 clips
  (`PSTAND3`, `PATTACK3`, `ATTACK3`, `TRANSX3`: the musket rest) are dropped from `MD`.
- **Drummers** without `PSTAND` or `MOTION_L0` (EngB17, FrnB17) get a one-frame `PSTAND`
  so `line.support_clips` keys them as drummers, not as standard bearers.
- **Pikemen** are built with the musket and disarmed (as officers), then hold **two
  pikes**, one per hand socket, shown one at a time by the socket's scale (`show`):
  - `socket.hand_l`: the pike gripped 1.30 m from the butt: ordered (STAND, upright at the
    left side) and shouldered on the left for the march (MOTION_*);
  - `socket.hand_r`: gripped **0.45 m from the butt** (#511; the point 4.71 m beyond the
    hand): the charge (PSTAND, MOTION_*0) and the push (ATTACK, ATTACK_THRUST_MID). The pike
    is level at the right hip (89°), beside the body: the right hand 0.32 m right of the
    centre line (`pike.SIDE`, `clips.beside`), the left 0.58 m before it on the shaft, the
    left shoulder leading (`pike.TWIST`). The push lands on the chest of a standing man
    **5.0 m away, centre to centre** (`pike.STANCE`, 150 px; his surface 4.83 m ahead): the
    body goes from the rear foot (root +0.12 m) onto the front foot (−0.28 m), the right hand
    staying just behind the hip, the point travelling 0.34 m. Until #511 this pike was
    gripped 3.35 m from the butt and fought at sword distance (1.67 m).
  - `socket.blade_r` (added beside `socket.hand_r` by `line._add_sockets`): a third pike
    gripped 3.35 m from the butt (the point 1.81 m beyond the hand), for the thrust down at a
    man on the ground 2.0 m ahead (0.44 m right of the centre line, past the bent head and
    the helmet's brim) and BRACE: with the butt grip the point cannot come down on a man that
    near. The strike layer and the audit take the shown right-hand socket.
  - Every pose-bone scale is reset to 1 before export (the hidden-socket bug); the crawl
    clips hide both pikes.

## Worn kit (m2-kit-more)

Musketeers (EngMus, SpnMus, FrnS17: the units on the musketeer rig; the Units table's
sprite reading gives all three the bandolier over the left shoulder and the sword on a
baldric) wear `line_kit.KIT17` through
SPEC `kit` (`musketeer_spec`), the line's worn-kit rules (docs/families/line.md "Worn kit"):
solid straps at a fixed offset, trunk weights only, at most 4; each item rigid on one bone and
seated on the surface under it (`line_kit.seat_back`); `acr_decay` on every material; no
`acr_lod` node.
- **Baldric**, buff, 65 mm, right shoulder to left hip, with the sword's leather scabbard
  (0.70 m) at the hip, as the line's hanger hangs. No hilt in it: the sword is drawn for melee
  (`sidearm="sword"`, shown by its socket), and a rigid hilt could not be hidden then.
- **Bandolier**, 45 mm, left shoulder to right hip over the baldric, with the **twelve
  apostles** (`line_kit.apostle_mesh`: turned wooden bottles with leather caps; five down the
  front, seven down the back, `APOSTLES`, all rigid on `spine`: on the strap's own bone each,
  neighbours on `spine1` and `spine2` swung into each other), the **bullet bag** at its end (front of the right
  hip) and the **priming flask** beside it (horn, brass nozzle).
- The **falling band** is built before the straps so they lie over it (`line_kit.UNDER_KIT`).

## Clips

| clip | content |
|---|---|
| STAND | pike ordered at the left side |
| MOTION_L/R/LB/RB | `poses.walk_cycle` with the pike shouldered (left) |
| PSTAND, MOTION_*0 | the charge: pike level (89°) at the right hip, the right hand near the butt, both hands on it, the body back on the rear foot |
| PATTACK / UATTACK | order to charge / back (the pikes swap between two neighbouring frames at mid-clip) |
| ATTACK = ATTACK_THRUST_MID | the push: the weight onto the front foot, the point 0.34 m forward onto the chest of a man 5.0 m away (4.83 m, 1.15 m) |
| ATTACK_THRUST_DOWN = ATTACK_DOWN | the shortened pike (`socket.blade_r`): lunge (officers' `LUNGE_DOWN`), trunk over the man, pike 50° down, point at 2.0 m, 0.2 m |
| BRACE (extra) | charge for horse: kneeling, the pike raised at a rider's chest (not played by the sim yet) |
| DEATH, DEATHLIE1, crawl | as the line's; through the fall the pike keeps the corpse's lie (level, turned out) instead of following the hand through the right leg |

Every frame of every clip is keyed (`clips.apply(..., dense=True)`, `clips.solved`): between
two keys the hands are solved again for the blended goals, so they stay on the shaft (the
exporter's interpolation of arm angles carried the left hand up to 0.66 m off it).

Clip audit (#509, `ACR-analysis/clip-audit/pole1/audit.csv` against `2026-11-15`), EngPik and
FrnPik alike: through the body 12 clips -> 0, touching 1 -> 0, `grip_left` fails 2 -> 0 (worst
0.658 -> 0.107 m), reach 4 of 4 pass (ATTACK 0.111 -> 0.000 m, ATTACK_DOWN 0.050 -> 0.042 m).

**The shared stoop leans back** (`clips.STOOP`, spine X negative): the family's down thrust
bends the trunk forward with positive spine X (`DOWN_BASE`, `PUSH_BASE`); `clips.py` is
untouched.

## Measured

Probe (`strike_view::probe_clips`, frame 2, point to target in m, clip alone / with the
layer; `ACR-analysis/fam-pike/probe/probe_pik2.txt`), EngPik and FrnPik alike:

| Clip | Target (fwd, up) | Clip alone | With the layer |
|---|---|---|---|
| `ATTACK_THRUST_MID` | 1.50, 1.15 | 0.07 | 0.06 |
| `ATTACK_THRUST_DOWN` | 2.00, 0.20 | **0.01** | 0.02 |
| `ATTACK_THRUST_DOWN` | 2.57, 0.20 (walked up) | 0.58 | 0.01 |
| `ATTACK_THRUST_DOWN` | 1.60, 0.20 | 0.39 | 0.21 |

The first build gripped the charge pike 3.00 m from the butt (point 2.16 m beyond the hand):
THRUST_MID was 0.45 / 0.43 off, the hand 0.44 m short of its goal behind the body; the grip
moved to 3.35 m.

Field (blank white field, HUD off; `ACR-analysis/fam-pike/sheet_field_{a,b,c}.png`): STAND
front and back rows and close-ups of all eleven. The strike clips were not filmed in game. Rebuilt on 9883ad7 (held gear keeps its material
slots): the pike's steel head and the musket's barrel now show apart from the wood
(`sheet_field_d.png`).

## Not yet

- ~~17th-c. garments, the matchlock's serpentine and the musket rest~~ (m2-c17, sheet
  `ACR-analysis/c17/sheet_c17.png`): doublet, buff coat, falling band, matchlock, rest. Still missing:
  the broad hat's plume, de Gheyn's longer drill, clips that use the rest (it is carried,
  not set under the barrel), slashed sleeves on the pike units (the option exists:
  `doublet_slashed`). The falling band is ragged at its outer edge and hides under the
  buff coat's shoulders; the doublet's red stand collar shows above it.
- The stance-3 rest clips of the musketeers; FrnS17's REST.
- ~~The musketeer's `0 MECH` is the bayonet lunge~~ (m2-shared-moves): EngMus, SpnMus and
  FrnS17 draw a sword (`pike.musketeer_spec(..., sidearm="sword")`, the musket slung);
  EngMus probe: `ATTACK_CUT_MID` / `_HIGH` / `ATTACK_THRUST_MID` 0.00, `ATTACK_THRUST_DOWN`
  0.00 at 2.0 m (0.57 at 2.57 m, layer 0.00). The pikemen's `DOWN_BASE` could now use
  `clips.STOOP`.
- `SETACTIVEPOINT #ATTACK1` (EngMus 13, SpnMus 12, FrnS17 19) is not passed to the musket
  path (the line's own fire frame is used).

**Reach (#511).** Pike 4.9 m (`PIKE_LEN`; 5.16 m to the point). Clip alone, the point is on a
standing man's chest at the hit frame from 5.0 m centre to centre = 150 px (clip audit with the
type's distance set to 5.0 m: reach 0.000 m, through 0, grip fails 0; `ACR-analysis/clip-audit/
pike-reach`). The sim must bring him there: `melee::STANCES` 150 px for EngPik and FrnPik, and a
blow that lands from that far (`ATTACK_RADIUS` max is 100 px in `ENGPIK.MD`); at the old
distances the strike layer cannot close the gap (1.20 m off at 3.33 m, 2.02 m at 2.50 m). There
is no finishing clip: on a man 0.9 m ahead even the shortened pike's grip would be 1.85 m up.
The native and Meso-American spears (2.0-2.1 m, the point 1.4-1.6 m from the hand) keep their
grips: a butt grip would gain about 0.5 m and needs their carry redone.

**The close blow (#511).** `ATTACK_THRUST_CLOSE` / `ATTACK_CLOSE` (`pike.pike_close`): the
shortened pike (`socket.blade_r`, the point 1.81 m before the right hand) level beside the right
ribs at a standing man 1.67 m off (`clips.TARGET_MID`), both hands on the shaft; the 3.35 m
behind the right hand run back level at rib height, the butt about 0.9 m right of his line and
3.6 m behind him (through the ranks behind, as the long pike's does in the charge). The view
plays it (`strike_view::close_blow`) for a type whose `melee::STANCES` entry is beyond its `.MD`
reach when the standing victim is nearer than half the stance (75 px for the pikemen). Audit
(`ACR-analysis/clip-audit/pike-close`): reach 0.000 m at 1.67 m, through 0, hands 0.050 / 0.051 m.
