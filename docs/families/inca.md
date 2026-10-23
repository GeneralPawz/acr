# Family 8: Inca (#287)

Module `assets/blender/lib/acr_assets/inca.py`, unit scripts `assets/units/inc*.py`, test
`assets/blender/tests/test_inca.py`. Plan and roster: [humans-roster.md](../humans-roster.md)
section 2.8. The family registers what it needs (docs/assets-humans.md, "Family registry")
and edits no shared module; it borrows meso's sandals, quilting painter, `_fan` and `_dome`.
(`incdo1` is the Inca house, a building in `assets/buildings/`, not a man.)

## Units

| unit | dress | head | weapons (WEAPONKIND) | clips |
|---|---|---|---|---|
| `IncPra` slinger | `check` unku (black and white checkerboard, red stepped yoke), sandals | llautu, cropped hair | sling (KAMEN, release 56), shield on the back, MECH | sling whirl and cast, `ATTACK_CUT_*` |
| `IncPik` spearman | quilted cotton armour over a red `tocapu` unku | cane helmet, feather crest | spear with a bronze point (PIKA), shield on the back | `ATTACK_THRUST_MID`, `_DOWN` |
| `IncTrb` blowpipe warrior | green `plain` unku (zigzag hem) | llautu, 4 feathers | blowpipe (VAFLIA, release 17), MECH (the pipe swung) | blowpipe, `ATTACK_CUT_*` |
| `IncVog` chief | `royal` unku (tocapu all over), yacolla, gold ear spools | llautu, a fan of 11 feathers | spear (PIKA), shield on the back | `ATTACK_THRUST_*` |
| `IncLut` archer | yellow `plain` unku | llautu, 2 feathers | bow, arrows, quiver (OSTRELA, STRELA, release 24), MECH | bow, `ATTACK_CUT_*` |
| `IncSha` priest | white `tocapu` unku, black yacolla, ear spools | llautu, a fan of 7 feathers | heal (MEDIK), champi (MECH) | heal, `ATTACK_CUT_*` |

## What `inca.py` adds

- **Garments:** the unku is a shell over the trunk (2 cm off the skin, over the shoulders)
  and a skirt tube standing off the legs from the hips to the knee (19.5/15 cm at the top,
  24.5/19.5 cm at the hem), one key per weave; quilted armour (3.6 cm shell); the yacolla
  (5.5 cm off the back, shoulders and the backs of the thighs).
- **Painters** (one per key: `outfit.dress` paints one colour per painter): `unku_check`,
  `unku_plain`, `unku_tocapu`, `unku_royal` (tocapu squares from five motifs, diagonal,
  cross, chequer, nested square, steps, in six colours, black borders), `llautu` (a slanting
  braid of the unit colour, gold and black), `inca_plumes` (bands of colour across the fan,
  so one key still gives red, gold and green feathers), `inca_helmet` (plaited cane),
  `inca_quilt` (meso's quilting), `yacolla`, `inca_hair`, `inca_shield`, `spool`.
- **Head:** hair cropped to a bowl line (brow, over the ears, the nape) as a 7 mm shell over
  the scalp and a fringe of 28 locks (cards) along its edge, so it reads as hair, not as a
  helmet; the llautu over it; the feather fans are cards (`meso._fan`); ear spools are
  discs found from the flesh's ear points.
- **Gear:** `champi` (haft and a six-pointed bronze star, `socket.tip` 0.62 m), the spear
  (2.2 m, bronze head), the library sling and blowpipe, bow and quiver, and `back_shield`
  (a 24 cm round shield with a cloth fringe, rigid on `spine2`, the stepped-fret design
  painted in its own frame through a temporary empty).
- **Clips:** `unit_clips(spec)` as meso's, plus forward-leaning down strikes: the shared
  `STOOP` leans backwards, so `club_down` and `spear_down` bend the trunk forward
  (positive spine X) over a lunge and put the striking point on a man lying 1.8 m ahead.
  `build` writes `strike_points` (the weapon point on each strike's hit frame) into
  `<unit>.stats.json` as the probe. Pose-bone scales are reset to 1 before export.

## References

- Felipe Guaman Poma de Ayala, *El primer nueva corónica y buen gobierno* (1615, Royal
  Danish Library, GKS 2232 4°): the captains and soldiers in checkerboard unku with the
  stepped yoke, slings, star maces, round shields on the back, feather headdresses.
- J. Pohl and A. McBride / Robinson, *Armies of the Aztec and Inca Empires* (Osprey).
- The checkerboard "Inca key" tunics and the all-over tocapu tunic (Dumbarton Oaks
  collection) for the weaves; star-headed bronze mace heads (museum pieces, e.g. Museo
  Larco).

No images were copied; shapes and patterns are modelled from the descriptions.
