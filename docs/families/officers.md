# Family 4: officers on foot (#287, batch 1)

`assets/blender/lib/acr_assets/officers.py`, unit scripts `assets/units/{engof17, engof18,
frnof17, frnof18, spnof17, spnof18, usaof18}.py`. Roster: docs/humans-roster.md, 2.4.

## Units

| Unit | `.MD` (frames) | Weapon | Kit | Lace |
|---|---|---|---|---|
| `EngOf17` | STAND, REST 120, MOTION, PATTACK/UATTACK 33, PSTAND, ATTACK 70, DEATH 71 | spontoon (`PIKA`) | sash, gorget | gold, cocked hat |
| `EngOf18` | STAND, REST 43, MOTION, ATTACK 52, DEATH 48 | officer's sword (spadroon) | sash, gorget, scabbard | gold, tricorn |
| `FrnOf17` | STAND, REST 41, MOTION, ATTACK 61, DEATH 46 | smallsword | white sash, gorget, scabbard | gold, cocked hat |
| `FrnOf18` | STAND, REST 53, MOTION, ATTACK 52, DEATH 48 | smallsword | gorget, scabbard (no sash: French officers wore none) | gold, tricorn |
| `SpnOf17` | STAND, REST 40, MOTION, ATTACK 60, DEATH 46 | officer's sword | red sash, gorget, scabbard | gold, cocked hat |
| `SpnOf18` | STAND, REST 53, MOTION, ATTACK 52, DEATH 49 | officer's sword | red sash, silver gorget, scabbard | silver, tricorn |
| `UsaOf18` | STAND, REST 121, MOTION, ATTACK 52, DEATH 46 | smallsword | crimson sash, gorget, scabbard | gold, tricorn |

The `.MD` lines were read from the game archives (`acr cat <game> ENGOF18.MD`). Colours are
the palette mixed half and half with the `#STAND` sprites (`line`'s `mix`), the sash, lace,
gorget and scabbard fixed.

## How it is built

- **Through `line.make`.** The officer is the line soldier (`line.spec` keys: palette,
  build, fit, hat, lace, lapels, fixed) with `gear` replaced by the officer's kit. `line.make`
  only knows the musket (`weapon` other than `musket` raises), so `officers.make` builds
  with the musket and then removes it and holds the officer's weapon on `socket.hand_r`
  (`gear.hold`, `held` item). A hook in `line.py` for another held weapon would remove the
  detour.
- **Finer cloth:** the coat's material key becomes `officer_coat`, painted by the registered
  painter `officer_wool` (like `wool`, finish `(0.82, 0.60, 0.06)`: smoother, more sheen).
- **Lace:** `line`'s `lace` key, default `{"hat": 0.014, "buttonholes": True}` in the
  palette's `lace` (gilt `(0.83, 0.67, 0.30)`, SpnOf18 silver).
- **Kit** (registered gear, rigid after the bone layer): `off_sash` (a band round the waist
  over the coat, fitted per angle to the draped cloth between 0.97 and 1.09 m with the arms
  left out, a knot and two hanging ends on the left hip), `off_gorget` (a gilt crescent below
  the stock, fitted to the chest), `off_scabbard` (empty, on the left hip, hanging back 38°).
  New material keys: `officer_coat` (wool), `scabbard` (leather, painted like boots).

## Clips

STAND: sword upright at the side (`clips.STAND_ARMED`), the left fist on the hip (IK);
EngOf17 grounds the spontoon. REST (the command): the arm raised forward, the blade pointing
ahead and up, held, lowered. MOTION_*: `poses.walk_cycle` (left hand stays on the hip).
DEATH, DEATHLIE1 as the line's; the crawl clips.

Strike clips on ATTACK's frames, the blow on frame 2 (`officers.sword_strikes`,
`clips.spear_strikes` for the spontoon): `ATTACK_CUT_MID`, `ATTACK_CUT_HIGH`,
`ATTACK_THRUST_MID` (lunge), `ATTACK_THRUST_DOWN` = `ATTACK_DOWN`. ATTACK plays the cut.

**The shared stoop leaned back.** `clips.STOOP` (and the cut's `lean` in
`clips.sword_strikes`) bend the spine with negative X, which on skeleton H leans the trunk
*back*: the right shoulder ended 0.09 m in front of the hips instead of over the man, and
the IK left the hand 0.45 m short. The first EngOf18 build probed the stab down 0.46 m off
the target with the clip alone. The family keeps its own blows (`stab_down`, `stab_raised`,
`cut`) with the trunk bent forward (spine +25..30°). m2-shared-moves made these the shared
`clips.STOOP` / `clips.LEAN` (same numbers): `stab_down`/`cut` could call
`clips.sword_strikes` now; officers.py keeps its musket-and-swap build (`line.make` with
the musket, then the weapon on `socket.hand_r`); `line.add_sidearm` does not simplify it
(the officer has no musket to sling).

**The stab down** (`ATTACK_THRUST_DOWN`): wind-up with the hilt raised high, the point
down; at frame 2 a long lunge (left foot ~1.3 m forward, right leg straight behind, hips
0.78 m up), the trunk over the man, the blade 50° below horizontal, the point on
`officers.TARGET_DOWN` = 2.0 m ahead, 0.2 m up. 2.0 m is where the sim leaves the man:
the bench (`ACR_STRIKE_DEBUG`, "victim at") measured 2.0 m when the officer starts 2 m off
and 2.57 m when he walks up from 5 m; the strike layer's step (0.5 m) covers the rest.

## Measured

Blender (local, shell body, `fam-officers/scripts/strike_solve.py`): every strike's point
within 0.4 cm of its target (the spontoon 1.1 cm).

Probe (`strike_view::probe_clips` on the built `.glb`s, frame 2, the point's distance to the
target in metres; `ACR-analysis/fam-officers/probe/probe_*.txt`):

| Clip | Target (fwd, up) | Clip alone | With the layer |
|---|---|---|---|
| EngOf18 `ATTACK_THRUST_DOWN` (first build, shared `STOOP`) | 1.60, 0.20 | 0.46 | 0.02 |
| EngOf18 `ATTACK_THRUST_DOWN` | 2.00, 0.20 | **0.00** | 0.03 |
| EngOf18 `ATTACK_THRUST_DOWN` | 2.57, 0.20 (walked up from 5 m) | 0.57 | 0.00 |
| EngOf18 `ATTACK_THRUST_DOWN` | 1.60, 0.20 | 0.40 | 0.09 |
| EngOf18 `ATTACK_CUT_MID` / `_HIGH` / `ATTACK_THRUST_MID` | 1.50, 1.15 / 1.55 / 1.15 | 0.00 / 0.00 / 0.00 | 0.00 / 0.00 / 0.01 |
| FrnOf17, FrnOf18, SpnOf17, SpnOf18, UsaOf18 `ATTACK_THRUST_DOWN` | 2.00, 0.20 | 0.00 | 0.03 |
| the same, `ATTACK_CUT_MID` | 1.50, 1.15 | 0.00 | 0.00 |
| EngOf17 (spontoon) `ATTACK_THRUST_DOWN` | 1.60, 0.20 / 2.00, 0.20 | 0.01 / 0.41 | 0.03 / 0.03 |
| EngOf17 `ATTACK_THRUST_MID` | 1.50, 1.15 | 0.01 | 0.00 |

In game (blank white field, `ACR-analysis/fam-officers/run*/of_crawl`): EngOf18 against a
crawler without arms (`life` 2000, both `hold_fire`) plays `ATTACK_THRUST_DOWN`, lunges and
puts the point into the man on the ground.

## Not yet

- The spontoon's stab down (EngOf17) uses the shared `spear_strikes` at 1.6 m.
- 17th-century officers wear the 18th-century line cut with a cocked hat; the long-skirted
  coat, falling band and baldric of family 6 (`pikeshot.py`) are not there yet.
- No boots (the line's gaiters), no pistol, no hand-on-hip in the walk's arm swing.
- The sash is rigid on `hips`: it does not follow the trunk's bend in the stab.
