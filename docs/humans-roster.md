# Humans roster: every human unit type and the plan to model them (#287)

Goal: a full 3D model for every human the game makes, Europeans and Natives, on the
existing pipeline (MakeHuman CC0 bodies, skeleton H, garments cut from scripted sewing
patterns and draped, [clothing.md](clothing.md); no bought uniforms). The models capture
the sprites' look (colours, silhouette, kit) without copying them pixel for pixel.

## 1. Inventory

Source: the game's `.MD` files (root folder of the archives) and the nations' `.NDS`
`[MEMBERS]`, read with `acr extract`; German names from `Text\MDLIST.TXT` (the German
edition has no English texts; the English column is a translation). A type counts when a
nation's `.NDS` lists it (the catalog) and it is a man: animals (`Anm*`, `MonKey`), ships
(`Shp*`), the editor's `WaterFall`, `empty` and buildings are left out. Mounted: `USAGE`
`FASTHORSE`, `HARDHORCE` or `HORSE-STRELOK`, or a horse-sized `GEOMETRY` radius (>= 15),
as `acr_sim::spacing::is_rider`. Weapons are the `WEAPONKIND <index> <kind>` lines:
`MECH` melee (sabre, sword, club, bayonet butt; the cut or smash of docs/melee.md), `PIKA`
a thrust (bayonet, pike, spear, spontoon), `KOPIE` thrown spear, `TAMAGAVK` thrown
tomahawk, `KAMEN` sling stone, `VISTREL` firearm, `STRELA` bow, `OSTRELA` fire arrow,
`VAFLIA` blowpipe dart, `MEDIK` heals. 3D: a unit script exists in `assets/units/`.

Not in the catalog, not modelled: `SpnReiG1`, `SpnReiG2` (leftover SpnRei variants),
`EngStr`, `SpnBar` (no nation lists them). `PueVsd` *is* in the catalog: the Delaware,
Huron and Iroquois horseman (assets-humans.md said otherwise; `PueVsda` is the Pueblo's).


## 2. Families

A family shares a body, a garment set and a rig; its units differ in parameters (colours,
hat, gear, weapon, clips). Ordered by units covered and screen time (line infantry and
the formations' drummers and flags fill the battlefield; Native nations each field several
types from the same dress). Slices: agent slices of ~45 min, incl. the VM build and a
blank-field check. Clips by weapon follow docs/melee.md (strike clips
`ATTACK_<KIND>_<HEIGHT>`, kind `THRUST` / `CUT` / `SMASH` / `SHOT`).

| # | family | units | n | module | slices |
|---|---|---|---|---|---|
| 1 | European line infantry, 18th c. | EngM18 ✔, FrnM18 ✔, SpnM18, UsaM18 | 4 | `line.py` (this change) | 1 |
| 2 | Drummers and standard bearers, 18th c. (foot) | EngB18, FrnB18, SpnB18, UsaB18, EngF18, FrnF18, SpnF18, UsaF18 | 8 | `line.py` + held drum/flag | 3 |
| 3 | Woodlands warriors (Iroquois, Huron, Delaware) | IroLut, IroStr, IroTam, IroVog, IroSha, HurLut, HurS15, HurSha, HurVog, DelKri, DelLut, DelPik, DelS15, DelSha, DelVog | 15 | `native.py` (new) | 5 |
| 4 | Officers, 18th and 17th c. (foot) | EngOf17, EngOf18, FrnOf17, FrnOf18, SpnOf17, SpnOf18, UsaOf18 | 7 | `line.py` / `pikeshot.py` + sash, gorget | 2 |
| 5 | Dragoons and mounted officers | EngD17, EngD18 ✔, FrnD17, FrnD18, SpnD17, SpnD18, UsaD18, EngKOf17, EngKOf18, FrnKOf17, FrnKOf18, SpnKOf17, SpnKOf18, UsaKOf18 | 14 | `cavalry.py` specs | 3 |
| 6 | Pike and shot, 17th c. (with its drummers and flags) | EngPik, FrnPik, EngMus, SpnMus, FrnS17, EngB17, FrnB17, SpnB17, EngF17, FrnF17, SpnF17 | 11 | `pikeshot.py` (new) | 5 |
| 7 | Mesoamerica (Aztec, Maya) | AtcLut, AtcMet, AtcSha, AtcVog, MayKri, MayLut, MayPik, MaySha, MayVog, MayVoi | 10 | `meso.py` (new) | 4 |
| 8 | Inca (Texcoco also fields IncLut, IncPik) | IncLut, IncPik, IncPra, IncSha, IncTrb, IncVog | 6 | `andes.py` (new) | 3 |
| 9 | Plains (Sioux) | SiuKri, SiuLut, SiuKLut, SiuKPik, SiuKStr ✔, SiuKVog | 6 | `native.py` + `cavalry.py` | 2 |
| 10 | Pueblo (PueVsd also rides for the Woodlands nations) | PueKri, PueLut, PueS15, PueSha, PueVog, PueVsda ✔, PueVsd | 7 | `native.py` (Southwest garments) | 3 |
| 11 | Frontiersmen and irregulars | EngTap, EngTapDragon, FrnBuk, UsaMil | 4 | `frontier.py` (new) | 2 |
| 12 | Conquistadors, 16th c. | SpnS15, FrnS15, SpnAlb, SpnRei ✔ | 4 | `conquista.py` (new; morion, cuirass) | 2 |
| 13 | European civilians: peasants and the priest | EngKri ✔, FrnKri, SpnKri, SpnSha | 4 | `civil.py` (from `engkri.py`) | 2 |
| 14 | Mounted standard bearers | EngKF18 ✔, FrnKF18 ✔, SpnKF18 ✔, UsaKF18 ✔ | 4 | `cavalry.py` | done |

104 types; 11 built (✔), 93 to go, about 37 slices. The cannon (`ArtPus`) and its crew
are covered by `guncrew.py`.

### 1. European line infantry, 18th c.

- References: David Morier, the grenadier paintings (1751, Royal Collection); the British
  Clothing Warrant of 1751; Kronoskaf, *Seven Years War Project* (kronoskaf.com/syw:
  regiment pages with uniform plates for the French, Spanish and British foot); Lienhart &
  Humbert, *Les uniformes de l'armée française* (1897–1906); Mollo & McGregor, *Uniforms of
  the American Revolution* (1975); Company of Military Historians plates.
- Garments: all exist (clothing.md). Missing: hat lace (braid along the tricorn's brim,
  `gear.tricorn(lace=...)`), buttonhole lace on lapels and cuffs (a painter mask in
  `outfit`, coloured by a `lace` palette key), shoulder strap.
- Gear: crossbelts, cartridge box, knapsack, musket with rammer (done); bayonet scabbard
  and hanger for grenadier-style variants.
- Clips: `poses.musketeer_clips` + the strikes `ATTACK_THRUST_MID/DOWN`,
  `ATTACK_SMASH_MID`, `ATTACK_SHOT_DOWN` (every `line` unit gets them).
- SpnM18: white coat, blue or red facings (Kronoskaf, Spanish line). UsaM18: blue coat, red
  or buff facings (Continental Army, 1779 regulations). Half a slice each.

### 2. Drummers and standard bearers, 18th c.

- References: Morier (drummers in reversed colours with lace); Kronoskaf's colour pages.
- Garments: the line uniform; drummers in **reversed colours** (coat = facings colour) with
  lace (the lace painter of family 1). Standard bearers as line infantry without knapsack,
  with a colour belt over the left shoulder.
- Gear: drum (shell, hoops, cords, on a carriage belt) and sticks; flag on a staff in the
  nation's colours (cloth with bones like the coat skirts, or the cavalry standard of
  `cavalry.py`); a hanger.
- Clips: drumming STAND and MOTION (sticks on the hand sockets, keyed); flag carry and
  wave (from `cavalry.FLAG_R`). The 18th-c. drummers have `0 PIKA`: one generic
  `ATTACK_THRUST_MID` with the hanger.

### 3. Woodlands warriors (Iroquois, Huron, Delaware)

- References: John Verelst, *The Four Indian Kings* (1710, Library and Archives Canada);
  Benjamin West, *The Death of General Wolfe* (1770) and his studies of Native dress;
  George Townshend's sketches (1759, National Portrait Gallery); Johnson & Hook, *American
  Woodland Indians* (Osprey MAA 228); *Handbook of North American Indians* vol. 15,
  *Northeast* (Smithsonian, 1978); Gary Zaboly's reconstructions.
- Garments (new, `garments_native.py`): breechcloth (one panel through a belt, flaps front
  and back: loose like the coat skirts), side-seam leggings with flaps and garters,
  moccasins (one piece with a puckered toe, on a last like `mbody.shoes`), trade shirt
  (linen), matchcoat or blanket over one shoulder (a draped rectangle).
- Hair and paint: scalp lock and roach (a deer-hair crest of cards on the skull),
  feathers (cards), a skin painter with body paint masks (red, black, stripes, hands),
  silver gorgets and arm bands.
- Gear: bow and quiver (`Lut`), trade musket (`Str`, `S15`), tomahawk (`Tam`), ball-headed
  war club, spear (`DelPik`), powder horn and shot pouch; shaman: rattle, fur cloak;
  chief: feather headdress (gustoweh for the Iroquois), trade silver.
- Clips: bow draw and loose (attack index 1–2, fire-arrow variant), trade musket
  (musketeer clips without the formation drill), tomahawk throw (`TAMAGAVK`), melee
  `ATTACK_CUT_*` (club, tomahawk), `ATTACK_THRUST_*` (spear), shaman's heal (`MEDIK`: a
  looping blessing), the peasant's work (as EngKri).

### 4. Officers

- References: Morier; officers' portraits by Reynolds and Gainsborough; Kronoskaf.
- Garments: the line uniform with a finer finish (gloss, lace), a sash (a draped band round
  the waist), gorget (gear), boots; 17th c. from family 6.
- Gear: smallsword or sabre, spontoon (`EngOf17` `PIKA`), pistol.
- Clips: officer STAND (hand on hip), point, `ATTACK_CUT_*` with the sword,
  `ATTACK_THRUST_*` with the spontoon.

### 5. Dragoons and mounted officers

- `cavalry.py` already builds the dragoon (EngD18) and the standard bearers from a `SPEC`:
  each remaining unit is a spec (colours, hat, coat length, boots). Mounted officers: the
  dragoon with sash, pistol and sword. 17th c.: buff coat and broad hat from family 6.
- Clips exist (index 0 sabre, index 1 shot).

### 6. Pike and shot, 17th c.

- References: Jacob de Gheyn, *Wapenhandelinghe* (1607: a plate per posture of the musket
  and pike drill, also the clips' reference); Haythornthwaite, *The English Civil War* and
  Osprey's *English Civil War Armies*; Royal Armouries pikeman's armour (pot, breast and
  back, tassets).
- Garments (new): doublet (short, tabbed waist), full breeches to the knee, stockings, buff
  coat (thick leather: stiff in bending), falling band collar; broad-brimmed hat.
- Gear: pikeman's pot, corselet and tassets (rigid gear), 5 m pike, matchlock with rest and
  bandolier, sword.
- Clips: pike port, charge and push (`ATTACK_THRUST_*`, two-handed), musket with de Gheyn's
  longer drill, drum and flag as family 2.

### 7. Mesoamerica (Aztec, Maya)

- References: *Codex Mendoza* (c. 1541, Bodleian MS. Arch. Selden. A. 1: tribute pages with
  warrior suits); the *Florentine Codex* (Sahagún, World Digital Library); Pohl & McBride,
  *Aztec, Mixtec and Zapotec Armies* (Osprey MAA 239); Hassig, *Aztec Warfare* (1988); the
  Bonampak murals (Maya, Structure 1).
- Garments (new): maxtlatl (loincloth), tilmatli (cloak knotted on the shoulder),
  ichcahuipilli (quilted cotton armour: thick, stiff cloth), tlahuiztli (full-body suit,
  jaguar or eagle painter), sandals; Maya hip cloth and cotton jacket.
- Gear: chimalli (feathered round shield), macuahuitl (obsidian-edged club: `MayVoi`),
  atlatl and darts (`AtcMet` `KOPIE`), spear, bow; feather headdresses, ear spools.
- Clips: `ATTACK_CUT_*` (macuahuitl), atlatl throw, bow, heal, peasant work.

### 8. Inca

- References: Felipe Guaman Poma de Ayala, *El primer nueva corónica y buen gobierno*
  (1615, Royal Danish Library facsimile: warriors, slingers); Pohl & Robinson, *Armies of
  the Aztec and Inca Empires* (Osprey, 2001).
- Garments (new): unku (tunic: one panel folded, side seams, a tocapu painter), yacolla
  (cloak), quilted armour, sandals; llautu headband, cane or wooden helmets.
- Gear: sling (`IncPra` `KAMEN`), star-headed mace (champi), blowpipe (`IncTrb` `VAFLIA`),
  spear, bow, round shield.
- Clips: sling whirl and cast, blowpipe, `ATTACK_SMASH_*` (mace), `ATTACK_THRUST_*`.

### 9. Plains (Sioux)

- References: George Catlin's Sioux portraits (1832, Smithsonian American Art Museum);
  Karl Bodmer's plates for Maximilian zu Wied, *Reise in das innere Nord-America*
  (1839–41); *Handbook of North American Indians* vol. 13, *Plains*.
- Garments: family 3's kit in the Plains cut: hide shirt with fringes and quillwork strips,
  leggings, breechcloth, buffalo robe; war bonnet for the chief.
- Gear: shield, lance (`SiuKPik`), bow from horseback, gun (done for SiuKStr).
- Clips: `cavalry.py` mounted bow and lance; SiuKri and SiuLut on foot as family 3.

### 10. Pueblo

- References: *Handbook of North American Indians* vol. 9, *Southwest* (1979); Edward S.
  Curtis, *The North American Indian* (photographs, Northwestern University digital
  collection).
- Garments: cotton kilt and sash, manta, wrapped leggings and moccasins, headband; a
  leather jerkin for the warriors.
- Gear: bow, trade musket, war club, shield; the shaman a generic painted mask and rattle
  (no sacred objects reproduced).

### 11. Frontiersmen and irregulars

- References: Robert Rogers, *Journals* (1765) and Zaboly's ranger reconstructions; the
  riflemen's hunting shirt (Mollo & McGregor); Exquemelin, *De Americaensche Zee-Roovers*
  (1678) for the buccaneer (`FrnBuk`).
- Garments: hunting shirt (fringed smock pattern), Native leggings (family 3), waistcoat
  and breeches (family 1), Scotch bonnet, round hat or fur cap; the buccaneer: shirt, wide
  sailor's breeches, sash.
- Gear: long rifle or musket, powder horn, shot pouch, tomahawk, knife; cutlass.
- Clips: musketeer clips without the formation drill; `ATTACK_CUT_*` (tomahawk, cutlass).

### 12. Conquistadors

- References: Pohl & Hook, *The Conquistador 1492–1550* (Osprey Warrior 40); morions and
  cabassets of the Royal Armouries and the Real Armería; the Lienzo de Tlaxcala (c. 1550).
- Garments: doublet, slashed breeches, hose; buff jerkin; quilted escaupil (from family 7).
- Gear: morion and cabasset, breastplate, gorget (rigid gear), arquebus with match (`S15`),
  halberd (`SpnAlb`), sword. SpnRei (mounted) exists.
- Clips: arquebus fire and reload, `ATTACK_THRUST_*` and `ATTACK_CUT_*` (the halberd does both).

### 13. European civilians

- References: Paul Sandby's drawings of labourers (Royal Collection); for the priest the
  habits of the Franciscans and Jesuits.
- Garments: `engkri.py`'s tunic and slouch hat become a `civil.py` family (smock,
  breeches, stockings, apron); the priest: cassock or habit (a long robe: the coat-skirt
  pattern to the ankle) with a cord, broad hat, cross.
- Clips: work (axe, pick, sickle), the priest's heal (`MEDIK`, like the shaman's), club blow.

## 3. Batches (parallel agents without file conflicts)

Shared code first, then one agent per family; each family has its own module and unit
scripts, so the agents of a batch touch disjoint files. The shared files (`garments.py`,
`outfit.py`, `poses.py`, `gear.py`, `drape.py`) change only in batch 0.

- **Batch 0 (sequential, ~4 slices):**
  1. `line.py`, the family pattern (done here).
  2. Hooks so families add without editing shared tables: `outfit.register(painter,
     kind, finish)`; garment patterns passed from a family module
     (`drape.dress(ch, garments=..., patterns=...)`); held gear in `gear.py` (drum, flag on
     a staff, sword, bow, quiver, spear, club).
  3. A clips module `clips.py`: bow, thrown weapon, sling, blowpipe, heal, drum, flag carry,
     one-handed cut and thrust on foot (docs/melee.md names).
  4. Lace and paint masks in `outfit` (lace on the facings, body paint on the skin).
- **Batch 1 (6 agents in parallel):** family 1 rest (SpnM18, UsaM18) + family 2; family 3
  Woodlands; family 4 officers; family 5 cavalry specs; family 7 Mesoamerica; family 13
  civilians.
- **Batch 2 (5 agents in parallel):** family 6 pike and shot; family 8 Inca; family 9
  Plains (on family 3's `native.py`); family 10 Pueblo (on `native.py`); families 11 + 12.

## 4. The line family (`assets/blender/lib/acr_assets/line.py`)

The musketeer pipeline (sewn uniform, belts, gear, musket, aim, reload and charge clips,
crawl clips, faces, outfit painting, export) moved from `engm18.py` into `line.py`. A unit
script is data: its `.MD` lines and `line.spec(MD, palette=..., build=..., fit=...,
hat=..., gear=..., weapon=..., mix=...)`. `hat` is `tricorn`, `slouch` or `cocked`;
`gear` any of `crossbelts`, `cartridge`, `knapsack`; `weapon` `musket`.
`assets/units/frnm18.py` is the proof (about 50 lines, most of them the `.MD` table):
FrnM18 as the sprite shows him, a dark blue coat with red cuffs and turnbacks, white
breeches, black gaiters. `engm18.py` keeps `make`, `build`, `pose_for` and the module
dicts `PALETTE`, `BUILD`, `FIT` (the silhouette tool tunes them in place).

Check: the rebuilt `engm18.glb` has the size of the one built before the change (4 493 464
bytes; 344 bytes differ) and its blank-field frame (white ground, HUD off) is
pixel-identical. Frames in `ACR-analysis/humans/frames/{old,new}/`, the sheet
`ACR-analysis/humans/sheet_line_before_after.png`.

Open: FrnM18's waistcoat takes the sprite's red (the projection's pick at `mix` 0.5); a
list of palette-only keys in the spec would keep it white.

## 5. Roster
Human types: **104** (25 mounted, 79 on foot), **11** with a 3D model; the cannon's crew counted apart.

### England (17)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `EngB17` | Trommler 17. Jhd. | drummer, 17th c. | foot | none | ENGLAND, USA | - |
| `EngB18` | Trommler 18. Jhd. | drummer, 18th c. | foot | 0 PIKA (thrust) | ENGLAND | - |
| `EngD17` | Dragoner 17. Jhd. | dragoon, 17th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | ENGLAND, USA | - |
| `EngD18` | Dragoner 18. Jhd. | dragoon, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | ENGLAND | yes |
| `EngF17` | Flaggenträger 17. Jhd. | standard bearer, 17th c. | foot | none | ENGLAND, USA | - |
| `EngF18` | Flaggenträger 18. Jhd. | standard bearer, 18th c. | foot | none | ENGLAND | - |
| `EngKF18` | Ber. Flaggenträger 18. Jhd. | mounted standard bearer, 18th c. | mounted | none | ENGLAND | yes |
| `EngKOf17` | Berittener Off. 17. Jhd. | mounted officer, 17th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | ENGLAND, USA | - |
| `EngKOf18` | Berittener Off. 18. Jhd. | mounted officer, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | ENGLAND | - |
| `EngKri` | Bauer | peasant | foot | 0 MECH (melee) | ENGLAND, USA | yes |
| `EngM18` | Füsilier | fusilier | foot | 0 PIKA (thrust), 1 VISTREL (firearm) | ENGLAND | yes |
| `EngMus` | Musketier | musketeer | foot | 0 MECH (melee), 1 VISTREL (firearm) | ENGLAND, USA | - |
| `EngOf17` | Offizier 17. Jhd. | officer, 17th c. | foot | 0 PIKA (thrust) | ENGLAND, USA | - |
| `EngOf18` | Offizier 18. Jhd. | officer, 18th c. | foot | 0 MECH (melee) | ENGLAND | - |
| `EngPik` | Pikenier | pikeman | foot | 0 PIKA (thrust) | ENGLAND, USA | - |
| `EngTap` | Trapper | trapper | foot | 0 MECH (melee), 1 VISTREL (firearm), 2 PIKA (thrust) | ENGLAND, USA | - |
| `EngTapDragon` | Jäger | hunter (jäger) | foot | 0 MECH (melee), 1 VISTREL (firearm), 2 PIKA (thrust) | NEUTRAL | - |

### France (17)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `FrnB17` | Trommler 17. Jhd. | drummer, 17th c. | foot | none | FRANCE | - |
| `FrnB18` | Trommler 18. Jhd. | drummer, 18th c. | foot | 0 PIKA (thrust) | FRANCE | - |
| `FrnBuk` | Freibeuter | buccaneer | foot | 0 MECH (melee), 1 VISTREL (firearm), 2 PIKA (thrust) | FRANCE | - |
| `FrnD17` | Dragoner 17. Jhd. | dragoon, 17th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | FRANCE | - |
| `FrnD18` | Dragoner 18. Jhd. | dragoon, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | FRANCE | - |
| `FrnF17` | Flaggenträger 17. Jhd. | standard bearer, 17th c. | foot | none | FRANCE | - |
| `FrnF18` | Flaggenträger 18. Jhd. | standard bearer, 18th c. | foot | none | FRANCE | - |
| `FrnKF18` | Ber. Flaggenträger 18. Jhd. | mounted standard bearer, 18th c. | mounted | none | FRANCE | yes |
| `FrnKOf17` | Berittener Off. 17. Jhd. | mounted officer, 17th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | FRANCE | - |
| `FrnKOf18` | Berittener Off. 18. Jhd. | mounted officer, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | FRANCE | - |
| `FrnKri` | Bauer | peasant | foot | 0 MECH (melee) | FRANCE | - |
| `FrnM18` | Füsilier | fusilier | foot | 0 PIKA (thrust), 1 VISTREL (firearm) | FRANCE | yes |
| `FrnOf17` | Offizier 17. Jhd. | officer, 17th c. | foot | 0 MECH (melee) | FRANCE | - |
| `FrnOf18` | Offizier 18. Jhd. | officer, 18th c. | foot | 0 MECH (melee) | FRANCE | - |
| `FrnPik` | Pikenier | pikeman | foot | 0 PIKA (thrust) | FRANCE | - |
| `FrnS15` | Hakenbüchsenschütze | arquebusier | foot | 0 MECH (melee), 1 VISTREL (firearm) | FRANCE | - |
| `FrnS17` | Musketier | musketeer | foot | 0 MECH (melee), 1 VISTREL (firearm) | FRANCE | - |

### Spain (18)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `SpnAlb` | Hellebardier | halberdier | foot | 0 PIKA (thrust) | SPAIN | - |
| `SpnB17` | Trommler 17. Jhd. | drummer, 17th c. | foot | none | SPAIN | - |
| `SpnB18` | Trommler 18. Jhd. | drummer, 18th c. | foot | 0 PIKA (thrust) | SPAIN | - |
| `SpnD17` | Dragoner 17. Jhd. | dragoon, 17th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | SPAIN | - |
| `SpnD18` | Dragoner 18. Jhd. | dragoon, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | SPAIN | - |
| `SpnF17` | Flaggenträger 17. Jhd. | standard bearer, 17th c. | foot | none | SPAIN | - |
| `SpnF18` | Flaggenträger 18. Jhd. | standard bearer, 18th c. | foot | none | SPAIN | - |
| `SpnKF18` | Ber. Flaggenträger 18. Jhd. | mounted standard bearer, 18th c. | mounted | none | SPAIN | yes |
| `SpnKOf17` | Berittener Off. 17. Jhd. | mounted officer, 17th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | SPAIN | - |
| `SpnKOf18` | Berittener Off. 18. Jhd. | mounted officer, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | SPAIN | - |
| `SpnKri` | Bauer | peasant | foot | 0 MECH (melee) | SPAIN | - |
| `SpnM18` | Füsilier | fusilier | foot | 0 PIKA (thrust), 1 VISTREL (firearm) | SPAIN | - |
| `SpnMus` | Musketier | musketeer | foot | 0 MECH (melee), 1 VISTREL (firearm) | SPAIN | - |
| `SpnOf17` | Offizier 17. Jhd. | officer, 17th c. | foot | 0 MECH (melee) | SPAIN | - |
| `SpnOf18` | Offizier 18. Jhd. | officer, 18th c. | foot | 0 MECH (melee) | SPAIN | - |
| `SpnRei` | Conquistador 16. Jhd. | conquistador, 16th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | SPAIN | yes |
| `SpnS15` | Hakenbüchsenschütze | arquebusier | foot | 0 MECH (melee), 1 VISTREL (firearm) | ENGLAND, SPAIN, USA | - |
| `SpnSha` | Priester | priest | foot | 0 MEDIK (heals) | ENGLAND, FRANCE, SPAIN, USA | - |

### USA (8)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `UsaB18` | Trommler 18. Jhd. | drummer, 18th c. | foot | 0 PIKA (thrust) | USA | - |
| `UsaD18` | Dragoner 18. Jhd. | dragoon, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | USA | - |
| `UsaF18` | Flaggenträger 18. Jhd. | standard bearer, 18th c. | foot | none | USA | - |
| `UsaKF18` | Ber. Flaggenträger 18. Jhd. | mounted standard bearer, 18th c. | mounted | none | USA | yes |
| `UsaKOf18` | Berittener Off. 18. Jhd. | mounted officer, 18th c. | mounted | 0 MECH (melee), 1 VISTREL (firearm) | USA | - |
| `UsaM18` | Füsilier | fusilier | foot | 0 PIKA (thrust), 1 VISTREL (firearm) | USA | - |
| `UsaMil` | Miliz | militia | foot | 0 MECH (melee), 1 VISTREL (firearm) | USA | - |
| `UsaOf18` | Offizier 18. Jhd. | officer, 18th c. | foot | 0 MECH (melee) | USA | - |

### Aztecs (4)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `AtcLut` | Bogenschütze | archer | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | ACTEC | - |
| `AtcMet` | Speerwerfer | spear thrower | foot | 0 PIKA (thrust), 1 KOPIE (thrown spear) | ACTEC | - |
| `AtcSha` | Schamane | shaman | foot | 0 MECH (melee), 1 MEDIK (heals) | ACTEC | - |
| `AtcVog` | Häuptling | chief | foot | 0 MECH (melee) | ACTEC | - |

### Maya (6)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `MayKri` | Bauer | peasant | foot | 0 MECH (melee) | ACTEC, INCAS, MAY | - |
| `MayLut` | Bogenschütze | archer | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | MAY | - |
| `MayPik` | Lanzenkämpfer | spearman | foot | 0 PIKA (thrust) | MAY | - |
| `MaySha` | Schamane | shaman | foot | 0 MECH (melee), 1 MEDIK (heals) | MAY | - |
| `MayVog` | Häuptling | chief | foot | 0 PIKA (thrust) | MAY | - |
| `MayVoi` | Macamba-Krieger | macuahuitl warrior | foot | 0 MECH (melee) | MAY | - |

### Inca (6)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `IncLut` | Bogenschütze | archer | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | INCAS, TESCOCO | - |
| `IncPik` | Lanzenkämpfer | spearman | foot | 0 PIKA (thrust) | INCAS, TESCOCO | - |
| `IncPra` | Steinschleuder-Krieger | slinger | foot | 0 MECH (melee), 1 KAMEN (sling) | INCAS | - |
| `IncSha` | Schamane | shaman | foot | 0 MECH (melee), 1 MEDIK (heals) | INCAS | - |
| `IncTrb` | Krieger mit Blasrohr | blowpipe warrior | foot | 0 MECH (melee), 1 VAFLIA (blowpipe) | INCAS | - |
| `IncVog` | Häuptling | chief | foot | 0 PIKA (thrust) | INCAS | - |

### Pueblo (7)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `PueKri` | Bauer | peasant | foot | 0 MECH (melee) | PUEBLO | - |
| `PueLut` | Bogenschütze | archer | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | PUEBLO | - |
| `PueS15` | Schütze | rifleman | foot | 0 MECH (melee), 1 VISTREL (firearm) | PUEBLO | - |
| `PueSha` | Schamane | shaman | foot | 0 MECH (melee), 1 MEDIK (heals) | PUEBLO | - |
| `PueVog` | Häuptling | chief | foot | 0 MECH (melee) | PUEBLO | - |
| `PueVsd` | Reiter | horseman | mounted | 0 PIKA (thrust) | DELAVAR, HURONI, IROKEZ | - |
| `PueVsda` | Reiter | horseman | mounted | 0 PIKA (thrust) | PUEBLO | yes |

### Sioux (6)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `SiuKLut` | Berittener Bogenschütze | mounted archer | mounted | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | SIU | - |
| `SiuKPik` | Berittener Lanzenkämpfer | mounted spearman | mounted | 0 PIKA (thrust) | SIU | - |
| `SiuKri` | Arbeiter (Bauer) | worker (peasant) | foot | 0 MECH (melee) | SIU | - |
| `SiuKStr` | Berittener Schütze | mounted rifleman | mounted | 0 MECH (melee), 1 VISTREL (firearm) | SIU | yes |
| `SiuKVog` | Berittener Häuptling | mounted chief | mounted | 0 MECH (melee) | SIU | - |
| `SiuLut` | Bogenschütze (Jäger) | archer (hunter) | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | SIU | - |

### Delaware (6)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `DelKri` | Bauer | peasant | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | DELAVAR, HURONI, IROKEZ | - |
| `DelLut` | Bogenschütze | archer | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | DELAVAR | - |
| `DelPik` | Lanzenkämpfer | spearman | foot | 0 PIKA (thrust) | DELAVAR | - |
| `DelS15` | Schütze | rifleman | foot | 0 MECH (melee), 1 VISTREL (firearm) | DELAVAR | - |
| `DelSha` | Schamane | shaman | foot | 0 MECH (melee), 1 MEDIK (heals) | DELAVAR, SIU | - |
| `DelVog` | Häuptling | chief | foot | 0 MECH (melee), 1 VISTREL (firearm) | DELAVAR | - |

### Huron (4)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `HurLut` | Bogenschütze | archer | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | HURONI | - |
| `HurS15` | Schütze | rifleman | foot | 0 MECH (melee), 1 VISTREL (firearm) | HURONI | - |
| `HurSha` | Schamane | shaman | foot | 0 MECH (melee), 1 MEDIK (heals) | HURONI | - |
| `HurVog` | Häuptling | chief | foot | 0 MECH (melee), 1 VISTREL (firearm) | HURONI | - |

### Iroquois (5)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `IroLut` | Bogenschütze | archer | foot | 0 MECH (melee), 1 OSTRELA (fire arrow), 2 STRELA (bow) | IROKEZ | - |
| `IroSha` | Schamane | shaman | foot | 0 MECH (melee), 1 MEDIK (heals) | IROKEZ | - |
| `IroStr` | Schütze | rifleman | foot | 0 MECH (melee), 1 VISTREL (firearm) | IROKEZ | - |
| `IroTam` | Tomahawkwerfer | tomahawk thrower | foot | 0 MECH (melee), 1 TAMAGAVK (thrown tomahawk) | HURONI, IROKEZ | - |
| `IroVog` | Häuptling | chief | foot | 0 MECH (melee), 1 VISTREL (firearm) | IROKEZ | - |

### Artillery (all European nations) (1)

| .MD | German | English | foot/mounted | weapons (`WEAPONKIND`) | nations | 3D |
|---|---|---|---|---|---|---|
| `ArtPus` | Kanone | cannon (crew) | mounted | 4 IADRO (ball), 5 KARTECH (canister) | ENGLAND, FRANCE, SPAIN, USA | - |
