# Native warriors on foot: the Woodlands family (`acr_assets/native.py`, #287)

Family 3 of [humans-roster.md](../humans-roster.md): the Iroquois, Huron and Delaware
warriors on foot. `native.py` is also the base for the Plains (family 9) and Pueblo
(family 10) units: their dress, hair and gear are SPEC keys, so a Plains or Southwest unit
adds keys (a hide shirt, a kilt) rather than a new pipeline.

## Unit scripts

A unit script is data, like `line.py`'s: the `.MD` lines (`#` lists keep only the frame
count, e.g. `"#ATTACK1 9 79"`), the `WEAPONKIND`s by attack index, the `SETACTIVEPOINT`
frames, and `native.spec(MD, WEAPONS, ...)`.

| unit | weapon | melee (index 0) | dress | hair | paint | kit |
|---|---|---|---|---|---|---|
| `IroLut` | bow (fire arrow 1, arrow 2; loosed on 44) | war club in its own socket (`socket.club_r`, the arrow hidden), `club_down` for the down hack | breechcloth, red leggings, moccasins | roach, 2 feathers | `red_black` | quiver, knife |
| `IroStr` | trade gun (`poses.musketeer_clips`, shot on 16) | musket thrust and butt | as IroLut | roach, 1 feather | `black_stripes` | strap, powder horn, shot pouch, knife |
| `IroTam` | tomahawk throw (gone on 16) | tomahawk hack (`club_strikes`) | + blue matchcoat over the left shoulder, silver gorget and arm bands | roach, 2 feathers | `red_hands` | knife |
| `DelPik` | spear (`spear_strikes`, PIKA) | the thrust | + linen trade shirt, blue leggings | scalp lock and braid, 3 feathers | `red_face_half` | knife |
| `IroVog` | trade gun | musket | + fine blue matchcoat, silver gorget and arm bands | scalp lock, **gustoweh** | `red_face_half` | strap, horn, pouch, knife |
| `HurVog` | trade gun | musket | + scarlet matchcoat, silver; green breechcloth, blue leggings | scalp lock, feather **crown** | `black_red_chin` | strap, horn, pouch, knife |
| `DelVog` | trade gun | musket | + shirt, dark blue matchcoat, silver; red breechcloth, blue leggings | scalp lock, feather **crown** | `red_stripes` | strap, horn, pouch, knife |
| `HurS15` | trade gun (SPEC over IroStr) | musket | green breechcloth, blue leggings | black roach, 1 feather | `white_dots` | strap, horn, pouch, knife |
| `DelS15` | trade gun (SPEC over IroStr) | musket | + shirt, red breechcloth, blue leggings | scalp lock, 2 feathers | `black_stripes` | strap, horn, pouch, knife |
| `HurLut` | bow (SPEC over IroLut) | war club | green breechcloth, blue leggings | black roach, 3 feathers | `black_face_half` | quiver, knife |
| `DelLut` | bow (SPEC over IroLut) | tomahawk (`socket.club_r`) | red breechcloth, blue leggings | scalp lock, 2 feathers | `red_black` | quiver, knife |
| `DelKri` | bow (loosed on 25) | war club; WORK (two-handed swing) and REST (look round) as EngKri's | + shirt | scalp lock | none | quiver, knife |
| `IroSha`, `HurSha`, `DelSha` | heal (MEDIK, `clips.heal`), turtle rattle in the left hand | war club (ATTACK, `club_strikes`, `club_down`) | + fur cloak (`pelt`; DelSha over a shirt) | long hair, 1-2 feathers | `white_band` | knife |

All 15 Woodlands units are built (slice 2). Iroquois wear red leggings and the red roach,
Huron green breechcloths, blue leggings and a black roach, Delaware a red breechcloth, blue
leggings and the scalp lock; skin tones vary a little by nation (`palette["skin"]`).

## SPEC keys

- `weapon`: `bow` (longbow on `socket.hand_l`, arrow on `socket.hand_r`), `gun` (the line's
  musket, rammer, aim, reload; `gun_len`), `tomahawk`, `club`, `spear` (on `socket.hand_r`);
  `melee`: the `clips.MELEE` kind of index 0 or None.
- `dress`: any of `breechcloth`, `leggings`, `moccasins`, `shirt`, `matchcoat`.
- `hair`: `roach`, `lock`, `long`; `feathers`: how many; `headdress`: `gustoweh` (a cap on a
  splint band, short feathers over the crown, one eagle feather upright) or `crown` (a band
  with a ring of upright feathers); `paint`: a `masks.PAINT_SETS` or `native.PAINT_SETS` name
  (one per unit type: one atlas per model); `silver`: gorget and arm bands.
- `rattle`: the shaman's turtle-shell rattle in the left hand; `work`: the peasant's WORK and
  REST clips; `dress` may add `cloak` (the shaman's fur, material key `pelt`).
- `kit`: `quiver` (48 cm, 8 cm across: `held.quiver` shrunk), `belt_l` (the strap over the left shoulder, `straps.py`), `horn`,
  `pouch`, `knife`.
- `palette`, `mix` (sprite share, 0.6), `fixed` (skin, feathers and silver keep the palette).
- A bow unit with a `melee` gets a second right-hand socket `socket.club_r` (`club_socket`)
  holding a war club (or tomahawk): every clip keys both sockets' scale, the melee clips
  (`ATTACK`, `ATTACK_*`) show the club and hide the arrow, all others the reverse.

## How the dress is made

Shells of the MakeHuman skin (`Character.shell`) and modelled panels, not draped cloth:
everything deforms with the body, so nothing cuts through in any clip, and the build
needs no cloth simulation.

- **Breechcloth:** a band through a hide belt and two flaps (`_flap`: a thin slab, 22–27 cm
  wide, hanging 43–47 cm) front and back over the belt, leaning out at the hem; the hem's
  weights go half way to the thighs (left and right across the width), so a stride swings
  the flap instead of poking through it.
- **Leggings:** stroud from ankle to high thigh, with the side-seam flap standing out along
  the outer side of each leg and garters under the knee. The flap is a thin modelled fin
  (`_legging_flap`): the outermost skin line sampled in 14 bands from 14 to 76 cm, smoothed,
  the fin 4 cm wide at the ankle, 2 cm at the thigh, each row skinned like its skin vertex
  (slice 1's 2.6 cm offset shell of the outer faces read lumpy).
- **Fur cloak (shamans):** a shell 3.4 cm off the skin over both shoulders, the back to the
  seat and the upper arms, open in front below the collarbones; key `pelt` (painter `pelt`,
  like felt: matted, dull).
- **Headdress (chiefs):** `gustoweh` or `crown` (above), rigid on the head.
- **Rattle:** a turtle shell (flattened dome) on a handle in the left hand, key `turtle`.
- **Moccasins:** `mbody.shoes` over the last with a thin sole and no heel, in buckskin; the
  instep buckle's slot takes the buckskin too (a tie).
- **Trade shirt:** linen, loose to mid-thigh, sleeves to the wrist; belt and flaps go over it.
- **Matchcoat:** a blanket shell over the left shoulder and down under the right arm.
- **Hair:** the roach is two rows of tapered cards from the forehead line over the crown to
  the nape, in dyed deer hair (`roach`, red); the scalp lock a tuft of cards at the crown and
  a braid down the back. Feathers are cards set at the crown, leaning back. All rigid on the
  head bone (gear layer, part 1). The face map is `heads.face_map` painted clean-shaven
  (`native.clean_shaven`: `faces.zones`' beard channel zeroed for the call, so neither the
  painted stubble nor `_FACE`'s beard zone, which the renderer turns into stubble, is left;
  for shaved and plucked heads, `roach` and `lock`, no cropped scalp along the hairline).
- **Skin:** a warm copper, `(0.80, 0.55, 0.38)` for the Iroquois, a little darker for the
  Huron and lighter for the Delaware, kept from the palette (`fixed`): slice 1 mixed 40 % of
  the sprites' shaded skin into a brown palette and came out near-black. The clothes take
  60 % of the sprite colour (they are brighter in the sprites).
- **Body paint:** the face-half paint goes through `native.paint_face_half`. Bug in
  `masks.paint_face_half` (for the shared slice): its region `masks._face` has no width
  limit, so every front-facing texel from 27 cm below the head's top up on the painted side
  is painted, which on these builds takes in the shoulder, the top of the chest and the
  upper arm ("half the trunk" in slice 1). The native mask keeps it above the jaw (23 cm
  below the top) and within 9 cm of the nose.
- **Materials (registered):** painters `stroud` (like wool) and `buckskin` (like leather);
  keys `breechcloth`, `leggings`, `flaps`, `matchcoat` (wool), `moccasins`, `belt_hide`,
  `pouch` (leather), `roach` (hair), `horn` (bone), `silver` (metal).
- **No European looks:** the build does not make the hair and beard variants of `heads`
  nor the `acr_face_morphs` extras, so the renderer gives no queue, beard or English skin
  tone to these men. A native skin-tone table in `faces.py` is for a shared slice.

## References

- John Verelst, *The Four Indian Kings* (1710), Library and Archives Canada: roach, tattoo
  and paint, matchcoat and stroud breechcloth, the war club.
- Benjamin West, *The Death of General Wolfe* (1770), National Gallery of Canada: the seated
  warrior's scalp lock, leggings with flaps and garters, moccasins, paint, powder horn.
- George Townshend, sketches of Native warriors at Quebec (1759), National Portrait Gallery,
  London: matchcoat, roach and feathers, gun and tomahawk.
- Michael G. Johnson & Richard Hook, *American Woodland Indians* (Osprey Men-at-Arms 228,
  1990): breechcloth, side-seam leggings, puckered-toe moccasins, trade shirts.
- *Handbook of North American Indians*, vol. 15, *Northeast* (Smithsonian, 1978): dress of
  the Iroquois, Huron and Delaware.
- Gary Zaboly's reconstructions of Woodlands warriors of the Seven Years' War.

No images were copied; the meshes are scripted.

## Results (slice 1)

| unit | triangles (all layers) | glb |
|---|---|---|
| IroLut | 5 610 | 3.10 MB |
| IroStr | 5 604 | 3.50 MB |
| IroTam | 6 024 | 3.03 MB |
| DelPik | 6 228 | 3.20 MB |

Blank field (white ground, HUD off) frames in `ACR-analysis/fam-woodlands/frames/{a,b}`,
the sheet `ACR-analysis/fam-woodlands/sheet_woodlands.png` (sprite, front, back, close-up,
clips in action). `strike_view` probe on DelPik (`ACR-analysis/fam-woodlands/probe.txt`):
`ATTACK_THRUST_MID` clip tip 1 cm off the target (layer 0), `ATTACK_THRUST_DOWN` 3 cm
(layer 5 cm); at 2.0 m (the sim's distance at a man on the ground) the clip is 43 cm
short, the layer 4 cm. The shared `club_strikes` down hack leans the trunk back (negative
spine X) and fell 0.92 m short on IroTam; `native.club_down` replaces `ATTACK_CUT_DOWN` /
`ATTACK_DOWN` for club and tomahawk units with a forward lunge (spine +36°): clip tip 7 cm
off at 1.6 m and 43 cm at 2.0 m (the arm cannot reach further), layer 2–3 cm; IroLut's 20 cm
/ 4 cm at 2.0 m. Drop it once `clips.py` leans forward.

Slice 1's open points (dark skin, paint on the trunk, stubble, lumpy flaps, the archers'
melee with the arrow hand, the big quiver) are fixed in slice 2 (above).
