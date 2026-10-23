# Family 7: Mesoamerica (Aztec and Maya) (#287)

Module `assets/blender/lib/acr_assets/meso.py`, unit scripts `assets/units/<type>.py`, test
`assets/blender/tests/test_meso.py`. Plan and roster: [humans-roster.md](../humans-roster.md)
section 2.7. The family adds what it needs by registering it (docs/assets-humans.md,
"Family registry"); it edits none of the shared modules.

## Units

| unit | dress | head | weapons (WEAPONKIND) | clips |
|---|---|---|---|---|
| `MayVoi` macuahuitl warrior | Maya quilted jacket, hip cloth, sandals, red/black paint | `maya` turban with feather frame | macuahuitl, chimalli `rings` (MECH) | `ATTACK_CUT_MID`, `ATTACK_CUT_DOWN` (shield up) |
| `AtcMet` spear thrower | ichcahuipilli, maxtlatl, sandals | `topknot` (temillotl) | atlatl and dart (KOPIE), dart as a short spear (PIKA) | spear throw (release 28), `ATTACK_THRUST_*` |
| `AtcLut` archer | ichcahuipilli, maxtlatl, sandals, black stripes | `band` | bow, arrow, quiver (OSTRELA, STRELA), MECH | bow (release 44), `ATTACK_CUT_*` |
| `MayLut` archer | hip cloth, sandals, red half face | `band` | as AtcLut | as AtcLut |
| `MayPik` spearman | Maya jacket, hip cloth, sandals | `maya` | spear with a broad stone head (PIKA) | `ATTACK_THRUST_MID`, `_DOWN` |
| `AtcVog` chief | jaguar tlahuiztli, maxtlatl, sandals | `jaguar` helmet | macuahuitl, chimalli `fret` (MECH) | `ATTACK_CUT_*` (shield up) |
| `MayVog` chief | jaguar tlahuiztli, hip cloth, sandals | `maya` | spear (PIKA) | `ATTACK_THRUST_*` |
| `AtcSha` priest | tilmatli (black), maxtlatl, black face | `band` | heal (MEDIK), macuahuitl (MECH) | heal, `ATTACK_CUT_*` |
| `MaySha` priest | tilmatli (white), hip cloth, white dots | `maya` | heal (MEDIK), macuahuitl (MECH) | heal, `ATTACK_CUT_*` |
| `MayKri` peasant | - | - | - | not built: its `.MD` has work, carry and five weapon sets (later slice) |

## What `meso.py` adds

- **Garments** as shells over the skin (`Character.shell`, fast and robust; not draped):
  `maxtlatl` (band round the hips and a flap in front), `ichcahuipilli` (3 cm thick, into the
  upper arms), `tlahuiztli` (torso, arms, thighs and shins to the ankle), `maya_jacket`
  (sleeveless), `tilmatli` (over the back and shoulders), `hipcloth` (a short skirt tube
  weighted to hips and thighs), `sandals` (sole and ankle tie).
- **Painters:** `quilted` (diamond quilting, puffed pads), `jaguar` (rosettes from a jittered
  3D cell grid), `eagle` (rows of feathers), `plumes` (streaked feathers), `meso_hair`.
- **Headgear** (`helmet(kind)`, modelled shapes, one material key each): `jaguar` (skull,
  upper jaw and snout over the brow, ears, fangs, pelt down the nape: the face looks out
  between the jaws), `eagle` (crown and beak, crest), `crest`, `maya` (turban, a frame on the
  forehead and a fan of plumes sweeping back), `topknot`, `band`.
- **Gear:** `chimalli` (the library shield with a feather fringe and one key, the design a
  mask in the shield's own frame: `chimalli_halves` stepped fret, `chimalli_rings` feather
  down balls, `chimalli_quarters`), `atlatl` (on the `hand_r` bone, so it stays in the hand
  when the throw hides the dart's socket), `dart`; macuahuitl, spear, longbow, arrow and
  quiver from `held.py`.
- **Clips:** `unit_clips(spec)`: stand (`STAND_ARMED`, the chimalli up via
  `clips.with_shield`), walk from `poses.walk_cycle`, death from `poses.death_keys`, the
  missiles and the heal from `clips.unit_clips` with the `.MD`'s `SETACTIVEPOINT` frames,
  the melee strikes with the weapon's own grip-to-point length (macuahuitl 0.80 m, spear
  1.57 m, dart 1.10 m) so `socket.tip` meets docs/melee.md's targets on frame 2.

## References

- *Codex Mendoza* (c. 1541, Bodleian Library MS. Arch. Selden. A. 1), tribute pages ff.
  20r-55r and the warrior ranks f. 64r: tlahuiztli suits, jaguar and eagle helmets, shield
  designs (xicalcoliuhqui, feather down balls).
- Bernardino de Sahagún, *Florentine Codex* (c. 1577, Biblioteca Medicea Laurenziana; World
  Digital Library facsimile), books 8 and 9: dress of warriors and lords, the ichcahuipilli.
- J. Pohl and A. McBride, *Aztec, Mixtec and Zapotec Armies* (Osprey Men-at-Arms 239, 1991).
- R. Hassig, *Aztec Warfare* (1988): atlatl, macuahuitl, tepoztopilli, cotton armour.
- Bonampak murals, Structure 1, Room 2 (Maya, c. 790): quilted jackets, jaguar pelts,
  tall feather headdresses, spears with broad points.
- Museum reconstructions: Museo Nacional de Antropología (Mexico City), the macuahuitl
  replica after the lost Real Armería piece; the chimalli in the Weltmuseum Wien.

No images were copied; the shapes are modelled from the descriptions and proportions.
