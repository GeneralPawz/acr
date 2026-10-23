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

- Everything goes through `line.make` (draped coat, breeches, stockings for gaiters,
  shoes). The coat's lapels take the coat's colour (`facings` defaults to `coat`), so the
  coat reads closed and long-skirted. 17th-c. garments of the roster (doublet with tabbed
  waist, falling band, buff coat cut) are **not** drafted yet: the line coat stands in.
- Registered here: hats `pike_broad` (broad felt, brim cocked up on the left), `pike_morion`,
  `pike_pot`; gear `pike_corselet` (breast and back fitted per angle to the draped cloth
  1.00-1.42 m, a peascod ridge, four tassets on `hips`), `pike_apostles` (nine charges down
  the front run of `belt_l` and a bullet bag), `pike_match` (a coil of slow match rigid on
  `hand_l`, lit ends); material keys `armour` (metal), `match` (linen).
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
  - `socket.hand_r`: gripped 3.35 m from the butt (the point 1.81 m beyond the hand): the
    charge (PSTAND, MOTION_*0, the strikes). One grip cannot serve both: ordered, a hand
    1.3 m up the shaft; in the push, the point must land at the fencing distance (1.5 m)
    and the butt rides high behind the head (overhand, as the push of pike was fought).
  - Every pose-bone scale is reset to 1 before export (the hidden-socket bug); the crawl
    clips hide both pikes.

## Clips

| clip | content |
|---|---|
| STAND | pike ordered at the left side |
| MOTION_L/R/LB/RB | `poses.walk_cycle` with the pike shouldered (left) |
| PSTAND, MOTION_*0 | the charge: pike levelled overhand, point down 8° |
| PATTACK / UATTACK | order to charge / back (pikes swap at mid-clip) |
| ATTACK = ATTACK_THRUST_MID | the push: long step, trunk thrown forward, the point at the chest (1.5 m, 1.15 m) |
| ATTACK_THRUST_DOWN = ATTACK_DOWN | lunge (officers' `LUNGE_DOWN`), trunk over the man, pike 50° down, point at 2.0 m, 0.2 m |
| BRACE (extra) | charge for horse: kneeling, the pike raised at a rider's chest (not played by the sim yet) |
| DEATH, DEATHLIE1, crawl | as the line's |

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
front and back rows and close-ups of all eleven. The strike clips were not filmed in game.

## Not yet

- 17th-c. garments (doublet, falling band, buff coat, full breeches, boots), the broad hat's
  plume; the matchlock's serpentine and the musket rest; de Gheyn's longer drill.
- The stance-3 rest clips of the musketeers; FrnS17's REST.
- The musketeer's `0 MECH` is the bayonet lunge, not a butt stroke or the sword.
- `SETACTIVEPOINT #ATTACK1` (EngMus 13, SpnMus 12, FrnS17 19) is not passed to the musket
  path (the line's own fire frame is used).
- Multi-material held items may render in one colour (the pike's head as the shaft).
