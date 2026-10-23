# Family 12: conquistadors on foot (`acr_assets/conquista.py`, #287)

Units: `SpnS15` (Spanish arquebusier; ENGLAND and USA field him too), `FrnS15` (French
arquebusier), `SpnAlb` (halberdier, `0 PIKA`). The mounted `SpnRei` is `cavalry.py`'s and
unchanged. Scripts: `assets/units/{spns15,frns15,spnalb}.py`; tests
`assets/blender/tests/test_frontier.py`.

## What the sprites show and the model

| unit | sprite (walk pack) | model |
|---|---|---|
| SpnS15 | steel breastplate, orange-red sleeves, red legs, grey helmet | cuirass (shell) over an orange doublet with red sleeves, ochre slops, red hose, cabasset, sword in its scabbard on the left hip, arquebus (1.38 m) on a strap |
| FrnS15 | dark red doublet, yellow-black striped slops and hose, red cap | no armour, dark red doublet, yellow slops and hose (no stripes yet), red flat cap, sword, arquebus |
| SpnAlb | full white armour, red sleeves, morion, halberd | cuirass and tassets over a doublet with red sleeves, red slops, olive hose, combed morion with a red plume, sword, halberd (2.25 m, `held.halberd`) |

The body is `frontier.make` with `conquista.spec`'s defaults, its clothes draped from the
period's own patterns (`conquista.DRAPE`, `build["drape"]`, docs/clothing.md "17th
century"): long **hose** (key `hose`, the palette's `fhose` colour) to mid-thigh, paned
**slops** gathered into a thigh band (`slops`, the lining `slashes` in the panes), linen
cuffs and a **doublet with slashed sleeves** (`doublet`, `dsleeve`, `slashes`). The
`blouse`/`slops`/`cuirass`/`tassets` shells are gone: the armour is modelled over the
draped doublet, `c_cuirass` (breast and back fitted per angle to the cloth: a peascod
breast swelling low, a medial ridge, the lower edge dipping to a point, a scooped neck,
a fauld turned out at the waist; rigid on `spine1`) and, where the layers listed
`tassets` (SpnAlb), `c_tassets` (four lapping lames on each thigh, rigid on the thigh).
Sheet `ACR-analysis/c17/sheet_c17.png` (SpnS15). Registered here: hats `morion`, `cabasset`,
`flat_cap`; gear `plume` (on the hat socket), `c_scabbard` (sheath and cup-hilt).

## Clips

- Arquebusiers: `frontier.build` (the musketeer clips of `line.py`), the release on the
  `.MD`'s `SETACTIVEPOINT #ATTACK1`.
- Halberdier (`conquista.halberd_clips`): STAND (halberd grounded at the right foot), the
  walk (MOTION_*, also as the `0` variants), PATTACK / UATTACK / PSTAND / PSTAND1 (to and
  from the levelled ready), DEATH, DEATHLIE1, crawl, and the strikes on ATTACK's frames
  (hit on frame 2): `ATTACK_THRUST_MID` (from the hip, left hand forward on the shaft),
  `ATTACK_THRUST_DOWN` = `ATTACK_DOWN` (overhand stab down at a man on the ground at
  2.0 m, the trunk bent forward: positive spine X, since the shared `STOOP` leans back).
  ATTACK plays the mid thrust. `conquista.probe` prints the point's position on the hit
  frame in the build log (`PROBE ...`).

## Known gaps

- ~~MECH of the arquebusiers uses the musket blows~~ (m2-shared-moves): SpnS15 and FrnS15
  draw a sword for melee (`frontier.build(..., make_fn=line.with_sidearm(frontier.make,
  "sword"))`, the arquebus slung); SpnS15 probe: `ATTACK_CUT_MID` 0.00, `ATTACK_THRUST_DOWN`
  0.00 at 2.0 m. The sheathed sword (`c_scabbard`) still shows beside the drawn one; no rodela.
- The halberd's own forward stoop (`halberd_strikes`) stays (a two-handed pole).
- The halberd's cut (`ATTACK_CUT_*`) is not made; only thrusts.
- FrnS15's stripes on the slops (a paint mask) are not done.
- Extra sprite stances (`ATTACK3`, `PSTAND3`) are not modelled.
