# Plains warriors: the Sioux (`acr_assets/plains.py`, #287)

Family 9 of [humans-roster.md](../humans-roster.md). `plains.py` builds on the Woodlands
family ([native.md](native.md)) by import: a Plains unit is a `native` SPEC plus Plains keys,
and `plains.build` runs native's pipeline with `native.dress` and `native.hair` wrapped for
the call (restored afterwards). The riders
use `cavalry.build` with `cavalry.native_head` swapped for the Plains head (`plains.mounted`).
No shared module was edited.

## Units

| unit | built on | dress | head | gear | clips |
|---|---|---|---|---|---|
| `SiuLut` (archer) | `plains.build` | bare chest, hair-pipe breastplate, dark trade-cloth breechcloth, buckskin leggings with flaps and strand fringe, moccasins; `red_black` paint | two wrapped braids, one eagle feather | bow, quiver, knife; war club on `socket.club_r` for the melee | native's bow (arrow on 44), club strikes and `club_down`; PSTAND3/4 stand |
| `SiuKri` (villager) | `plains.build` | buckskin war shirt with beaded shoulder strips, quilled sleeve strips and sleeve fringe; red breechcloth, fringed leggings, moccasins | two wrapped braids | stone-headed war club, knife | WORK and REST (native), ATTACK1 = the work swing on its 43 frames, ATTACK5 = the club blow; the `.MD` has no ATTACK, so the strikes are made on ATTACK5's frames |
| `SiuKPik` (mounted lancer) | `plains.mounted` | cavalry `native` dress, buckskin shirt, blue breechcloth | braids with wraps, one eagle feather | lance (couched, thrust), round hide shield, blanket and rope bridle | cavalry lance clips, gallop |
| `SiuKVog` (mounted chief) | `plains.mounted` | cavalry `native` dress, white buckskin shirt, red breechcloth | **war bonnet**: beaded brow band, 22 black-tipped eagle feathers fanned up and back, a 6-feather trailer down the back | round hide shield | cavalry cut (unarmed: cavalry has no war club), gallop |
| `SiuKStr` | its own script (`cavalry.py`), not rebuilt | | | | |
| `SiuKLut` (mounted archer) | **not built**: `cavalry.py` has no bow from horseback | | | | |

## Plains SPEC keys (over native's)

- `warshirt`: a buckskin shell over the trunk to the hip and the arms, with strips of quill
  (`quill`, down the top of each sleeve: arm faces with normal z > 0.8) and beads (`beads`,
  over each shoulder, 10.5 cm out from the middle).
- `fringe` (default on): **strands**, not a band. `_seam` samples the flesh along a bone
  (per bin of its length the vertex with the highest score: the underside of the arm, the
  outer side of the leg) and `_strands` hangs 2-3 narrow two-segment cards (7 mm wide,
  8-10 cm long, lengths jittered) from each point, each skinned like its seam vertex. Sleeve
  fringe comes with the war shirt; the legging fringe sits 4 cm out, at the flap's edge.
- `braids` (default on): two tapered braids in front of the shoulders with two cloth wraps
  each (`wraps`), a thin hair mass at the back; the scalp keeps the face map's hairline
  (`heads.finish(ch, beard=False, scalp=True)`), no beard.
- `eagle`: single eagle feathers at the back of the head, each a white quill card
  (`feather`) and a dark tip card (`eagle_tip`).
- `breastplate`: two columns of 11 bone tubes (`hairpipe`) on a hide strip, rigid on
  `spine2`.
- The palette keeps the buckskin (`war_shirt`, `fringe`, `leggings`, `flaps`,
  `moccasins`) and the quill and bead colours (`fixed`): 60 % of the sprites' shaded olive
  turned the leggings grey-green.

## References

- George Catlin, Sioux portraits of 1832 (Smithsonian American Art Museum), e.g. *Eagle's
  Ribs* and *One Horn*: quilled war shirts with hair-lock and buckskin fringe, braids with
  wraps, the bonnet.
- Karl Bodmer, plates for Maximilian zu Wied, *Reise in das innere Nord-America* (1839-41):
  Sioux and Mandan dress, leggings with fringe, hide shields, bow cases.
- *Handbook of North American Indians*, vol. 13, *Plains* (Smithsonian, 2001).
- Museum pieces: Lakota war shirts and hair-pipe breastplates (National Museum of the
  American Indian), cited for construction only.

No images were copied; the meshes are scripted.

## Results (slice 1)

Glbs: siulut 3.5 MB, siukri 3.9 MB, siukpik 11.8 MB, siukvog 11.9 MB (the horse). Sheet:
`ACR-analysis/fam-plains/sheet_plains.png` (sprite, front, back, side, close-up, one frame
of the action field), frames in `ACR-analysis/fam-plains/frames`.

Open points:
- `SiuKLut` not built (no mounted bow in `cavalry.py`; a shared slice).
- SiuLut's chest reads as a dark slab on the field: the known trunk-patch bug darkens the
  bare trunk and the hair-pipe breastplate does not read through it yet.
- The sprites are more colourful (red and white trade cloth, SiuKri's long dress reads as a
  woman's); the 3D keeps buckskin.
- The action field (each unit facing an enemy at 90 px) did not start a fight in 36 ticks,
  so the sheet's last column is a stand frame; the melee strikes were not probed
  (`strike_view` picks `socket.hand_r` for archers, a known shared bug).
- The mounted close-ups frame the horse, cutting the rider's head.
