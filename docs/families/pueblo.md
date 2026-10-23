# The Pueblo on foot (`acr_assets/pueblo.py`, #287)

Family 10 of [humans-roster.md](../humans-roster.md). `pueblo.py` builds on
[native.py](native.md) by import: the body, skin, moccasins, hair-free face
(`heads.finish(ch, beard=False, scalp=True)`: the scalp painted), bow, quiver, trade gun, the archers' `socket.club_r`, `club_down`
and the paint sets are native's. The Southwest dress is added by wrapping native's
`dress`, `hair` and `kit` for the length of a native call (`pueblo.hooks`), and the Woodlands ball club is swapped for a stone-headed club in
`held.ITEMS` for that call. Nothing in native or the shared modules is edited.

## Unit scripts

Generated from the `.MD` files by `ACR-analysis/fam-pueblo/gen_units.py` (the `#` lists
keep only their frame count). `PUEKRI.MD` lists `MOTION_RB` twice; the second (frames 15
to 8) is taken as `MOTION_LB`.

| unit | weapon | melee (index 0) | dress | hair | paint |
|---|---|---|---|---|---|
| `PueKri` | `civil.py` tools and work clips (chop, reap, build, carry) | the axe (`civil.peasant_clips`) | black leather vest over a white cotton shirt, white kilt, red sash, red wraps | bangs, chongo | none |
| `PueLut` | bow (fire arrow 1, arrow 2; loosed on 44) | stone club (`socket.club_r`, `club_down`) | woven sleeveless shirt (green and black zig-zags), dark kilt, red sash, red wraps | bangs, loose | `red_black` |
| `PueS15` | trade gun (shot on 15, `poses.musketeer_clips`) | musket thrust and butt | long white cotton shirt, red sash with the end in front, dark kilt, buckskin boot moccasins | bangs, chongo | none |
| `PueSha` | heal (MEDIK, `clips.heal`), gourd rattle in the left hand | stone club | white pelt over the shoulders (native `cloak`), red woven stole across the chest, dark kilt, red wraps | bangs, loose | `white_band` (generic) |
| `PueVog` | stone club (MECH) | stone club | Spanish-cut buckskin jacket with sleeves, red and black plaid blanket, red sash, dark kilt, red wraps | bangs, loose | `red_face_half` |

All wear the red cloth headband and turquoise and shell necklaces. `PueVsda` (the
horseman) is `cavalry.py`'s and was left as it is; `PueVsd` has the identical `.MD` and
sprites and was not given its own script in this slice.

## SPEC keys (beyond native's)

- `pdress`: any of `kilt`, `sash`, `bandolier`, `tunic`, `wraps`, `jerkin`, `blanket`;
  `sleeves`: the jerkin with sleeves (a jacket) or without (a vest).
- `phair`: `bangs` (cut at the brow and at the jaw at the sides, loose at the back) or
  `chongo` (the back hair tied into a knot at the nape with a red tie); `headband`.
- `necklace`: `turquoise`, `shell` (bead strings on the chest, the shell string with a
  pendant).
- `gourd`: the shaman's gourd rattle (native's turtle rattle is off).
- `peasant`: build with `civil.skeleton()` (the tool sockets), `civil._hold_tools` and
  `civil.peasant_clips` on the Pueblo body (`build_peasant`).
- `pueblo.spec` sets native's `hair` to `long` (the scalp painted) and `feathers` to 0;
  native's `hair` is called with `hair="none"` so only the Pueblo hair is modelled.

## How the dress is made

- **Kilt:** a lofted tube on the skirt bones (`body.tube` with `rig.SKIRT_COLUMNS`) from the
  waist to 62 cm, so a stride swings it; painter `pue_kilt`: the plain-woven ground and,
  3 to 12 cm above the hem, black stepped terraces between red and green lines.
- **Sash:** a shell at the waist over the kilt's top edge (painter `pue_sash`: black diamonds
  outlined in green on the ground, white lines) and its end, native's breechcloth flap made
  narrow and moved to the right front.
- **Stole (`bandolier`):** a shell band from the right hip over the left shoulder.
- **Woven shirt (`tunic`):** a sleeveless torso shell (painter `pue_tunic`).
- **Wraps:** a shin shell from the ankle to under the knee (painter `pue_wraps`: the strip's
  turns rising round the leg); red cloth, or buckskin for PueS15's boot moccasins. Native's
  side-seam leggings are not used: the knee between the kilt and the wraps is bare.
- **Jerkin:** a torso shell open down the front, with sleeves for the chief's jacket; key
  `pue_jerkin`, painter `buckskin`.
- **Blanket:** over the shoulders and down the back to the knee, open in front (painter
  `pue_plaid`).
- **Hair and headband:** boxes rigid on the head (bangs, sides, crown, back hair or knot) and
  a cloth band round the brow with a knot and two ends at the left side (key `pue_band`).
- **Stone club:** a straight handle, an egg-shaped stone head lashed across the end, its
  striking end at `clips.CLUB_TIP`'s distance (`socket.tip`).

## References

- *Handbook of North American Indians*, vol. 9, *Southwest* (Smithsonian, 1979): Pueblo dress
  (kilt, sash, wrapped leggings, the chongo), weapons (bow, stone club, shield).
- Edward S. Curtis, *The North American Indian*, vols. 12, 16, 17 (1922-1926), Northwestern
  University digital collection: hair in bangs and the chongo, headbands, the cotton shirt,
  wrapped leggings and boot moccasins, turquoise and shell necklaces.
- Kate Peck Kent, *Pueblo Indian Textiles* (School of American Research, 1983): the woven
  kilt with its embroidered border, the brocaded sash, the manta.
- John L. Kessell, *Pueblos, Spaniards, and the Kingdom of New Mexico* (2008), on the 1680
  Revolt: Spanish leather jackets (cueras) and arms taken up by Pueblo fighters.

No images were copied; the meshes and patterns are scripted. No sacred objects (katsina
masks, altar pieces) are reproduced: the shaman has generic face paint and a plain gourd.

## Results

Frames on the blank field (white ground, HUD off) in `ACR-analysis/fam-pueblo/frames`, the
sheet `ACR-analysis/fam-pueblo/sheet_pueblo.png` (sprite, front, back, side, two frames
against an enemy pikeman). glb sizes 3.3-4.5 MB (PueKri carries civil's five tools).

The fight frames are weak: the pikeman (EngPik, owner 1) kills PueKri and PueSha before the
camera reaches them; PueLut draws, PueS15 presents, PueVog lunges with the club.

## Open points

- Hair is modelled as boxes (bangs, sides, crown, back or knot): it reads blocky in close-up;
  cards or a hair shell would be better.
- The shared skin bugs (darker than stored, trunk patches) show on these builds too.
- The melee strikes were not probed with `strike_view` in this slice.
- No shield or rabbit stick on the foot units (the sprites show none); the painted hide
  shield is PueVsda's.
- ~~`PueVsd` needs a cavalry script of its own~~ (m2-shared-moves): `units/puevsd.py`,
  PueVsda's rider with the Pueblo look (hair cut to the jaw, a knot at the nape, a red cloth
  band round the brow: `cavalry` `hat` "band"); lance `ATTACK_THRUST_DOWN` 0.03 (layer 0.00).
