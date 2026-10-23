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
| `IroLut` | bow (fire arrow 1, arrow 2; loosed on 44) | club strikes with the arrow hand (`club_down` for the down hack) | breechcloth, red leggings, moccasins | roach, 2 feathers | `red_black` | quiver, knife |
| `IroStr` | trade gun (`poses.musketeer_clips`, shot on 16) | musket thrust and butt | as IroLut | roach, 1 feather | `black_stripes` | strap, powder horn, shot pouch, knife |
| `IroTam` | tomahawk throw (gone on 16) | tomahawk hack (`club_strikes`) | + blue matchcoat over the left shoulder, silver gorget and arm bands | roach, 2 feathers | `red_hands` | knife |
| `DelPik` | spear (`spear_strikes`, PIKA) | the thrust | + linen trade shirt, blue leggings | scalp lock and braid, 3 feathers | `red_face_half` | knife |

Left for the next slices: IroVog, IroSha, HurLut, HurS15, HurSha, HurVog, DelKri, DelLut,
DelS15, DelSha, DelVog. HurLut/DelLut and HurS15/DelS15 are a SPEC each over IroLut/IroStr
(colours, hair, paint); the chiefs need a gustoweh (Iroquois) or a feather headdress, the
shamans a fur cloak and rattle and a looping `MEDIK` clip (`clips.heal`), DelKri the
peasant's work clips (as EngKri).

## SPEC keys

- `weapon`: `bow` (longbow on `socket.hand_l`, arrow on `socket.hand_r`), `gun` (the line's
  musket, rammer, aim, reload; `gun_len`), `tomahawk`, `club`, `spear` (on `socket.hand_r`);
  `melee`: the `clips.MELEE` kind of index 0 or None.
- `dress`: any of `breechcloth`, `leggings`, `moccasins`, `shirt`, `matchcoat`.
- `hair`: `roach`, `lock`, `long`; `feathers`: how many; `paint`: a `masks.PAINT_SETS` name
  (one per unit type: one atlas per model); `silver`: gorget and arm bands.
- `kit`: `quiver`, `belt_l` (the strap over the left shoulder, `straps.py`), `horn`,
  `pouch`, `knife`.
- `palette`, `mix` (sprite share, 0.4), `fixed` (feathers and silver keep the palette).

## How the dress is made

Shells of the MakeHuman skin (`Character.shell`) and modelled panels, not draped cloth:
everything deforms with the body, so nothing cuts through in any clip, and the build
needs no cloth simulation.

- **Breechcloth:** a band through a hide belt and two flaps (`_flap`: a thin slab, 22–27 cm
  wide, hanging 43–47 cm) front and back over the belt, leaning out at the hem; the hem's
  weights go half way to the thighs (left and right across the width), so a stride swings
  the flap instead of poking through it.
- **Leggings:** stroud from ankle to high thigh, with the side-seam flap standing out along
  the outer side of each leg (a shell of the outward-facing faces, 2.6 cm off the skin) and
  garters under the knee.
- **Moccasins:** `mbody.shoes` over the last with a thin sole and no heel, in buckskin; the
  instep buckle's slot takes the buckskin too (a tie).
- **Trade shirt:** linen, loose to mid-thigh, sleeves to the wrist; belt and flaps go over it.
- **Matchcoat:** a blanket shell over the left shoulder and down under the right arm.
- **Hair:** the roach is two rows of tapered cards from the forehead line over the crown to
  the nape, in dyed deer hair (`roach`, red); the scalp lock a tuft of cards at the crown and
  a braid down the back. Feathers are cards set at the crown, leaning back. All rigid on the
  head bone (gear layer, part 1). The face map is the shared one (`heads.face_map`).
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

Open: the skin comes out darker than the sprites (sprite mix on a dark palette; lighten
`skin` or raise `fixed`); `red_black` paints half the trunk, not only the face (to check in
`masks.paint_face_half`); the shared face map paints a European stubble; the leggings'
flap shells read bulky at close range; archers hack with the arrow hand.
