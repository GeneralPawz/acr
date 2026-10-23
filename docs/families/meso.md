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
| `AtcLut` archer | ichcahuipilli, maxtlatl, sandals, black stripes | `band` | bow, arrow, quiver (OSTRELA, STRELA), MECH | bow (release 44), MECH the bow strike (`ATTACK_CUT_*`) |
| `MayLut` archer | hip cloth, sandals, red half face, arm bands | `band` | as AtcLut | as AtcLut |
| `MayPik` spearman | Maya jacket, hip cloth, sandals | `maya` | spear with a broad stone head (PIKA) | `ATTACK_THRUST_MID`, `_DOWN` |
| `AtcVog` chief | jaguar tlahuiztli, maxtlatl, sandals | `jaguar` helmet | macuahuitl, chimalli `fret` (MECH) | `ATTACK_CUT_*` (shield up) |
| `MayVog` chief | jaguar tlahuiztli, hip cloth, sandals | `maya` | spear (PIKA) | `ATTACK_THRUST_*` |
| `AtcSha` priest | tilmatli (black), maxtlatl, black face | `band` | heal (MEDIK), macuahuitl (MECH) | heal, `ATTACK_CUT_*` |
| `MaySha` priest | tilmatli (white), hip cloth, white dots | `maya` | heal (MEDIK), macuahuitl (MECH) | heal, `ATTACK_CUT_*` |
| `MayKri` peasant | maxtlatl, sandals, jade collar and ear spools | `topknot` | stone axe (MECH) | REST, WORK and ATTACK1 (the axe at a trunk), ATTACK3 (hacking the field), PATTACK4/UATTACK4/ATTACK4 (the load picked up, carried on the tumpline, set 4 walks with it), ATTACK5 and ATTACK the hack; every numbered walk and ready line |

## What `meso.py` adds

- **Garments** as shells over the skin (`Character.shell`, fast and robust; not draped),
  standing off where the real thing is stiff: `ichcahuipilli` (a 4.2 cm shell, into the
  upper arms, and a hem tube flaring over the loincloth), `maxtlatl` (band round the hips,
  the two ends hanging to the knee before and behind, rigid on the hips), `tlahuiztli`
  (torso, arms, thighs and shins), `maya_jacket` (sleeveless), `tilmatli` (the knot over the
  shoulders as a shell, the cloak itself a stiff sheet hanging down the back from the
  shoulders, rigid on `spine2`, widening towards the hem), `hipcloth` (a short skirt tube),
  `sandals`. Not draped: the drape pipeline is the shared module's and these cottons are
  stiff; a draped tilmatli that swings with the legs is a later step.
- **Painters:** `quilted` (diamond quilting), woven borders (`_hem`: red with yellow and
  turquoise stripes, at the hem of the maxtlatl ends, the hip cloth and the tilmatli), `jaguar`, `eagle`, `meso_hair` (strands with a sheen), `meso_trim` (striped
  bands), `feather` (barbs and mottle), `meso_stone` (veined jade and turquoise),
  `meso_gold`. One painter per key (keys sharing a painter share an atlas cell, so a
  colour: the long and short feathers came out one colour until each had its own). The bright keys (`quetzal`, `macaw`, `jade`, `turquoise`, `gold`,
  `meso_trim`, `meso_hair`, `maxtlatl`) are `fixed`: never dulled by the sprite mix.
- **Head** (`head_parts(kind, plume)`: Items skinned rigidly to the head, one key each, not
  dropped at death): hair as a cap close over the skull from a high hairline at the
  forehead to the nape, so the face reads, and tied up (`topknot`: the temillotl with a
  woven tie and two red feathers; `band`: a woven headband, the hair clubbed at the back,
  red and green feathers); `maya`/`crest`: a woven turban, a jade disc on the brow, a fan
  of 13 long quetzal plumes with 11 shorter macaw feathers before it. Every feather is its
  own modelled vane (`feather`: narrow at the quill, pointed, folded along the rachis,
  drooping, twisted a little, both sides), so the fan reads as feathers from any side.
  Not alpha cards: the humans' atlas has no alpha channel and the shared `outfit` is not
  ours to change; the outline is geometry instead (about 1.3k triangles for the Maya fan).
  `plume` = (long, short) keys per unit (MayPik, MayVog: red long, green short).
  The animal helmets (`helmet`: `jaguar`, `eagle`) stay held items (dropped at death), with a
  plume crest on the head.
- **Jewellery:** jade ear spools, a jade bead collar with a turquoise pendant (on
  `spine2`), gold wrist cuffs and turquoise arm bands (not under a tlahuiztli; no arm
  bands under the ichcahuipilli sleeves). `jewels` per unit.
- **Gear:** `chimalli` (the library shield with a feather fringe and one key, the design a
  mask in the shield's own frame: `chimalli_halves`, `chimalli_rings`,
  `chimalli_quarters`), `atlatl` (on `hand_r`), `dart`, the family's own `macuahuitl` (a
  paddle 11 cm wide and 3 cm thick, 8 obsidian blades a side as prisms 4 cm proud: serrated
  from the face and the side), the peasant's stone `axe`; spear, longbow, arrow and quiver
  from `held.py`. Multi-material held items (macuahuitl blades, spear head) render in one
  colour until m2-heldmat's fix lands; nothing here works round it.
- **Clips:** `unit_clips(spec)`: stand, walk, death, the missiles and the heal from
  `clips.unit_clips`; the melee strikes (`strikes`) at the weapon's own grip-to-point length,
  leaning the trunk forward (positive spine X; the shared `STOOP` bends back and stops
  ~0.45 m short, the officers' finding): the down blows at a man on the ground at 2.0 m
  (officers' `TARGET_DOWN`) from a long lunge. `club` (macuahuitl, axe), `spear` (two hands),
  `bow` (the archers' MECH: the `.MD`s give the archers no other weapon, so the bow is
  chopped down in both hands like a staff, the bow hand at the grip). `work_clips` for the
  peasant.

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
