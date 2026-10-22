# Asset sources: free professional bases for the M2 3D models

Issue #287. A survey of free, professionally made base models and materials for each
category in the 3D inventory (`ACR-analysis/3d/inventory.json`: 252 types). The plan is to
start from a good free base, then build on it in our scripted Blender pipeline
([assets.md](assets.md)) so that the result reads as American Conquest, only more realistic.

Surveyed 2026-10-22. Licences were checked on each source's licence page or, for Sketchfab,
on the model's own licence field (read through the public Sketchfab API, `/v3/models/<uid>`).
Polycounts are the site's own figures: Poly Haven reports vertices or triangles of the
highest LOD, Sketchfab reports faces. No asset files were downloaded.

## Rules this survey applies

- **CC0 first.** CC-BY is fine with attribution. NC, ND, editorial-only, unclear,
  purchase-only and login-only assets are excluded, or listed under
  [Needs the user's decision](#needs-the-users-decision).
- **Sketchfab needs a free account to download.** This holds for every Sketchfab model,
  CC0 ones included ([Download API](https://sketchfab.com/developers/download-api): downloads
  need "users to be authenticated with a Sketchfab account"). All Sketchfab picks below
  therefore need **the user's decision on a free account**. Their licences are fine (CC-BY
  or CC0).
- **Our pipeline commits no generated files** ([assets.md](assets.md)). Imported bases go into
  a local, git-ignored source cache, the same way as the MakeHuman base. That cache is fetched
  by a script that pins the URL and a SHA-256 for each file, and the asset scripts read from
  it. CC-BY bases get a line in a `CREDITS` file (author, title, URL, licence) that ships with
  the game.

## Sources at a glance

| Source | Licence (page checked) | Login | Verdict |
|---|---|---|---|
| **Poly Haven** | CC0 for everything ([polyhaven.com/license](https://polyhaven.com/license)) | no | **Top source.** 522 models, about 700 textures, all with PBR maps and several resolutions. Has the Smugglers Cove set (17th-century Dutch ships, a pinnace, a rigged ship's cannon, crates, barrels), the Pine Forest set (pine, fir, saplings, stumps, ferns, moss, grass) and many rock and cliff scans. |
| **ambientCG** | CC0 ([docs.ambientcg.com/license](https://docs.ambientcg.com/license/)) | no | **Top source for materials.** Ground, snow, footsteps, thatch, planks, wood siding, bark, plaster, clay roof tiles, leaf-card atlases (`LeafSet001`–`030`), pine needles. Public JSON API. |
| Blender demo files | per bundle ([blender.org/download/demo-files](https://www.blender.org/download/demo-files/)) | no | Human Base Meshes (CC0) and Animal Fur Examples (CC0) are useful. The Hair Styles demo is CC-BY-SA, so we avoid it. |
| Blender Studio | assets need a paid subscription ([studio.blender.org](https://studio.blender.org/welcome/)) | paid | Excluded. |
| **Sketchfab** | per model: CC0 / CC-BY picks only | free account | **Best source for animated animals, specific trees and historic scans.** Quality varies, so check each model. |
| OpenGameArt | per asset, mixed (CC0 / CC-BY / GPL / CC-BY-SA) | no | Mostly game-ready low or mid poly. Nothing beats Poly Haven or Sketchfab for our categories; only worth a look for effect sprites. |
| Smithsonian 3D | **only items marked CC0 are free**; the others are "non-commercial, educational and personal use" | no | The Gunboat *Philadelphia* (1776) scan is **not CC0** (3D terms are non-commercial), so it is excluded. The open-access API needs a key, and searching with `DEMO_KEY` hit its rate limit, so no CC0 cannon or animal scan was confirmed. Worth a manual search on 3d.si.edu with the "CC0" filter on (cannon, ordnance, bison and elk skeletons). |
| MakeHuman | system assets CC0; community assets have a licence per asset (mostly CC0 or CC-BY) | no | Humans already chosen. Community site: [static.makehumancommunity.org](https://static.makehumancommunity.org/about.html). No 18th-century clothing confirmed (see Humans). |
| Quixel Megascans (Fab) | Fab Standard Licence: any engine allowed, commercial use allowed; **the free period ended in 2024**, and most assets are now paid | Epic account | **Needs the user's decision.** Excellent scans, but purchase or login only. |
| Kenney | CC0 ([kenney.nl/support](https://kenney.nl/support)) | no | Stylised, so not a base. The Particle Pack is usable for small spark and puff sprites. |
| Quaternius | CC0 ([Ultimate Animated Animals](https://quaternius.com/packs/ultimateanimatedanimals.html)) | no | Low-poly and stylised, so not a visual base. Its CC0 rigs and clips (walk, gallop, attack, death) are a useful **animation reference** or a fallback for retargeting. |
| Scan the World (MyMiniFactory) | licence per object; **the default is non-commercial** ([FAQ](https://www.myminifactory.com/scantheworld/faq/)) | account | Excluded. Use only if an object is explicitly CC0. |
| JangaFX free VDBs | CC0 ([free VDB animations](https://jangafx.com/software/embergen/download/free-vdb-animations)) | no | **Top source for smoke and fire.** 15 simulations (camp fire, fire plume, smoke plume, chimney smoke, ground explosion, dust impact, and more) as OpenVDB. We render them in Blender into flipbooks. |
| Unity Labs VFX flipbooks | CC0 ([Unity blog](https://unity.com/blog/engine-platform/free-vfx-image-sequences-flipbooks)) | no | Ready-made smoke, fire and explosion image sequences and flipbook sheets (EXR / TGA). |

## Humans (covered: MakeHuman CC0)

Bodies come from MakeHuman CC0, as already decided. For 18th-century dress:

- **MakeHuman community clothes.** Each asset has its own licence. This survey found **no
  confirmed 18th-century items** (tricorne, frock coat, waistcoat, breeches, gaiters). The
  community site could not be searched from here: www.makehumancommunity.org serves a TLS
  certificate that does not match its name. Check by hand in MPFB2's asset browser, which
  shows the licence, and accept CC0 or CC-BY only.
- **Blender Human Base Meshes v1.4.1 (CC0).** Clean quad bodies, heads, hands and feet.
  Useful as a topology reference and as a source for sculpting clothes.
- **Fabric and leather materials: ambientCG (CC0).** `Fabric0xx` (wool, linen, canvas),
  leather, and Poly Haven `rough_linen`. These texture our own scripted clothes (#217
  `acr_assets.outfit`).
- **Hair: Blender's Animal Fur Examples (CC0)** for the geometry-nodes setup. Avoid the Hair
  Styles demo (CC-BY-SA). Queues, braids and Native hairstyles are scripted.
- **Gap:** no free, professional, era-correct clothing exists that fits our bodies.
  **Fallback:** keep building the clothes in our scripts (cloth shells from the body, as we
  do now), with ambientCG cloth and leather PBR. Tricorne, bicorne, cartridge box,
  powder horn and musket are small scripted props.

## Horses (another agent owns this; my top candidates)

| Candidate | Licence | Quality | Notes |
|---|---|---|---|
| [Horse, henry_7](https://sketchfab.com/3d-models/a6f860e43e364619bccb174a1ac7d0c9) | CC-BY | 30.7k faces, rigged, **28 animations** | Best free, realistic, animated all-rounder found. Retarget its clips to our horse rig. |
| [Realistic Animated Donkey, WildMesh_3D](https://sketchfab.com/3d-models/2b4086035df94d9e88fd0f18d34b3872) | CC-BY | 19.4k faces, **117 animations** | Base for a **mule or draught animal** (limbers, carts). The author notes "known rigging problems". |
| [Horse Rigged (Game Ready), abhayexe](https://sketchfab.com/3d-models/bc64f4ff7966474ca9bacd42fa73a754) | CC-BY | 2.9k faces, rigged | Low poly; could serve as an LOD or a reference only. |
| Quaternius Ultimate Animated Animals (horse) | CC0 | stylised | Clip reference only. |

## Animals

What each AC animal is, identified from the sprites (rendered with `acr gp-sheet`):

| Type | Animal | Sprite evidence |
|---|---|---|
| AnmBiz | American bison | name |
| **AnmCro / AnmCroSm** | **crocodilian: American alligator (or American crocodile in Central America)**; Sm = small or juvenile | long, low, green-olive body with a ridged tail; neither a cougar nor a crow |
| AnmEgl | eagle (bald eagle) | name |
| AnmJug | jaguar by name. At sprite size the coat reads **tan and unspotted, like a cougar** | Plan one big-cat mesh with two coats (jaguar rosettes, cougar tan) |
| AnmMed / AnmMedSm | brown or grizzly bear; Sm = cub or smaller bear | heavy, brown, with a shoulder hump |
| AnmOln / AnmOlns | **elk (wapiti) stag**, and a hind or doe without antlers | antlered stag with a pale rump; Olns has no antlers |
| MonKey (Other) | monkey | name |

The game needs for every animal: STAND, REST, MOTION (walk and run), ATTACK, DEATH, and
DEATHLIE1–3 (lying dead).

| Animal | Pick 1 | Pick 2 | Work on top |
|---|---|---|---|
| Elk stag + hind | [Realistic Animated Elk, WildMesh_3D](https://sketchfab.com/3d-models/787834f9caa2474d9f1814b807c072d7): CC-BY, 7.9k faces, **53 animations** | [Realistic Animated Doe, WildMesh_3D](https://sketchfab.com/3d-models/b6172618aa304473a4cf7b9246c0a218): CC-BY, 5.2k faces, 55 animations | Fix the rig (the author warns of "known rigging problems"); retarget to our skeleton conventions; re-texture as wapiti (tan body, dark mane, pale rump); add a fur card shell. |
| Bear | [Realistic Animated Bear, WildMesh_3D](https://sketchfab.com/3d-models/bffc3c87d2d148ff8533e1cc8a11c9f1): CC-BY, 7.5k faces, **81 animations** | none better found | Same rig clean-up; grizzly coat and shoulder hump; scale down for the cub. |
| Bison | [Animated Free Bison, AneesAnimates](https://sketchfab.com/3d-models/be2dcd97db1c4b9c91a7728d9bc89925): CC-BY, 61k faces, 1 animation | [American Bison, jondameron5](https://sketchfab.com/3d-models/88fbc7cb875444669bd45ac3597bc579): CC-BY, 858 faces, baked lighting (silhouette reference only) | Decimate to about 10k faces; rig it ourselves (the WildMesh bear or elk skeleton is a good start); write all clips except one. |
| Big cat (jaguar / cougar) | [Cougar, GalileoGB](https://sketchfab.com/3d-models/d9dc5171a1a14674a0313df212d12c87): CC-BY, 17.5k faces, 1 animation. The author bars NFT or crypto use, which does not affect us. | [jaguar, Ear.Rodriguez](https://sketchfab.com/3d-models/91c61c329d2a4668816f81f08dfcd492): CC-BY, 25.9k faces, 1 animation (made for an AR app) | Shared mesh and rig; write the clips ourselves; two coats. |
| Alligator | [Mississippi Alligator, andrey.](https://sketchfab.com/3d-models/f0cc55fd5e4448e6bbd2ee65b2f1ea66): CC-BY, 275k faces, 1 animation | none | Decimate heavily (a low, flat animal seen from above); re-rig with a spine chain; write a crawl, a lunge, a death roll and lying clips. |
| Eagle | [Bald Eagle, ucdavisterc](https://sketchfab.com/3d-models/8874cfcf83a54575ba8068376f8a63a8): CC-BY, 15.8k faces, 1 animation | [Bald Eagle - Med poly, Urielgc](https://sketchfab.com/3d-models/31c6a60a650745faa173ce488a542b58): CC-BY, 2.3k faces | Flapping, gliding and diving clips; feather cards on the wing edges. |
| Monkey | **gap** (only stylised or film-character models found) | | Fallback: scripted, starting from a MakeHuman or Blender human base mesh with altered proportions, or a stylised CC0 base. |

**Animals, gaps:** only the elk, hind and bear come with usable animation sets. All the
others need **our own clips**. Fallback: Rigify animal metarigs (the quadruped and bird
rigs that ship with Blender) plus scripted gait cycles. The Quaternius CC0 clips can serve
as timing references.

## Trees and vegetation

| Need | Pick | Licence | Quality | Work on top |
|---|---|---|---|---|
| Pines and firs (north) | Poly Haven [pine_tree_01](https://polyhaven.com/a/pine_tree_01), [fir_tree_01](https://polyhaven.com/a/fir_tree_01), [pine_sapling_medium](https://polyhaven.com/a/pine_sapling_medium), [fir_sapling](https://polyhaven.com/a/fir_sapling) | CC0 | photo-scanned, 8K maps, **ships with LODs**; the top LOD is huge (pine 17M) | Use the lowest LOD or bake impostors; build a geometry-nodes variant generator that reuses its bark and needle textures. |
| Oaks | [Mighty Oak Trees, Jagobo](https://sketchfab.com/3d-models/4f6ab5594a8a415aba3f958682b9ced5) (3 trees); also [Animated Oak Trees](https://sketchfab.com/3d-models/fcb0374d91334eb7a119bfc316167f7e) | CC-BY | 100–140k triangles per tree, UVs, bark with colour, normal and roughness | Decimate the branches, convert the foliage to cards with an ambientCG `LeafSet` atlas, add LODs. |
| Maples | [Maple Trees, Jagobo](https://sketchfab.com/3d-models/9f51e08e56cd4e44b35fd02a3ad8d3a6) (red autumn) or [Maple trees pack with LODs, lolipop_1707](https://sketchfab.com/3d-models/b5d2833c258f4054a01ee2b4ef85adf0) (12 trees, LODs, 2K atlases) | CC-BY | the pack is game-ready | Recolour to summer green; the autumn colour is optional per season. |
| Aspen and birch | [Quaking Aspen Trees, Jagobo](https://sketchfab.com/3d-models/4b5716cb8e3845928e94c93cac6dd526) | CC-BY | about 47k faces per tree | Doubles as birch with a paper-birch bark texture. |
| Generic deciduous trees, coast | Poly Haven [island_tree_01–03](https://polyhaven.com/a/island_tree_01), [tree_small_02](https://polyhaven.com/a/tree_small_02), [jacaranda_tree](https://polyhaven.com/a/jacaranda_tree), dead trunks, stumps | CC0 | scans with LODs | Mix in for variety. |
| Palms and jungle (Central America) | [Tropical Plants Pack M02P, mozzarellaARC](https://sketchfab.com/3d-models/2f093afb792742438f0f7ba7eaab90f0) (15 plants); [Coconut Palm, evolveduk](https://sketchfab.com/3d-models/26e787f2ff2e4c0fb004c3b0210805a3); Poly Haven [pachira_aquatica_01](https://polyhaven.com/a/pachira_aquatica_01), [calathea_orbifolia_01](https://polyhaven.com/a/calathea_orbifolia_01) | CC-BY / CC0 | mid poly, PBR | **Weakest area.** No free, realistic canopy jungle tree (ceiba, mahogany) was found; script these. |
| Bushes, ferns, grass | Poly Haven shrub_01–04, fern_02, grass_medium_01/02, moss_01, dry_branches, root clusters, nettle; ambientCG `Foliage001–008` and `LeafSet001–030` atlases | CC0 | scanned | Grass becomes cards or instanced clumps; the shader does the wind. |
| Crops (maize, wheat) | [Maize Corn Plant, gilles.schaeck](https://sketchfab.com/3d-models/5fd3b104d8104519b061469c365d4974) (3.1k faces) | CC-BY | simple | Instanced in rows for FIELD and FIELDP; wheat is scripted from cards. |

**Trees, gap and fallback:** all trees need an **LOD and impostor pass**, since an RTS view
shows hundreds of them. Fallback for missing species (ceiba, cypress, mesquite, cactus):
Blender's bundled Sapling Tree Gen or our own geometry nodes, with Poly Haven and ambientCG
bark (`Bark001–015`) and `LeafSet` atlases.

## Rocks, cliffs, stones

| Pick | Licence | Notes |
|---|---|---|
| Poly Haven [rock_moss_set_01/02](https://polyhaven.com/a/rock_moss_set_01), [boulder_01](https://polyhaven.com/a/boulder_01), [rock_07](https://polyhaven.com/a/rock_07), [rock_09](https://polyhaven.com/a/rock_09), [stone_01](https://polyhaven.com/a/stone_01), rock_face_01/02, the namaqualand_boulder and namaqualand_cliff sets | CC0 | 20k–360k triangles, **with LODs**; scans with PBR maps. The moss set suits the eastern forest; namaqualand suits Pueblo and the dry south-west. |
| Poly Haven [coastal_cliff_01/02/04](https://polyhaven.com/a/coastal_cliff_01), [mountainside](https://polyhaven.com/a/mountainside), the coast_rocks sets | CC0 | 0.3–2.9M triangles, so decimate them. Shore and cliff pieces for coastal maps. |
| ambientCG `Rock0xx`, `Rocks0xx`; Poly Haven `rock_wall_*`, `rocky_terrain*` | CC0 | Tiling materials for cliffs placed on the terrain. |

Work: decimate to 2–8k triangles, bake normals from the scan, re-tint per biome. No gap.

## Terrain materials (PBR)

All CC0 and tiling, with colour, normal, roughness and displacement maps:

| Surface | Poly Haven | ambientCG |
|---|---|---|
| Grass | grass_path_2/3, aerial_grass_rock, grass_ground | Grass001–008, Ground037 |
| Dirt | dirt, dirt_floor, dry_ground_01, dry_ground_rocks | Ground0xx (178 results) |
| Mud | brown_mud(_02/_03/_dry/_leaves_01/_rocks_01), mud_forest, **muddy_tracks** | Ground054, 071, 087, 106, 109 |
| Sand | sand_01–03, coast_sand_01–05, aerial_sand, gravelly_sand | Ground0xx sand variants |
| Snow | snow_01–05, snow_field_aerial, snow_floor | Snow001–015, **Footsteps001–006** (trampled snow) |
| Rock | rocky_terrain(_02/_03), rocky_trail, rock_ground | Rock0xx, Rocks0xx |
| Forest floor | forest_floor, forest_ground_04–06, forest_leaves_02–04, leaves_forest_ground | ScatteredLeaves001–009, PineNeedles001 |
| Trampled ground | muddy_tracks, burned_ground_01 (burnt ground) | Footsteps001–006, Pathway002 |

Work: pack the maps into our terrain atlas or array layout; reduce to 1K or 2K; blend with
height. No gap.

## Buildings

Free *finished* buildings that fit the era are rare and uneven, so **materials plus our
scripted kit (`kit.py`) are the base**. Finished models serve as references only.

**Materials (all CC0):**

| Material | Source |
|---|---|
| Thatch | ambientCG `ThatchedRoof001A/B`, `ThatchedRoof002A/B`; Poly Haven `thatch_roof_angled` |
| Wood planks, siding, shingles | ambientCG `Planks0xx` (86 results), `WoodSiding005–009`, `Wood0xx`; Poly Haven `wood_planks`, `wood_plank_wall`, `roof_planks`, `rough_pine_door`, `wooden_gate`, `wood_shutter` |
| Logs and bark (log cabins, palisades, longhouse bark) | Poly Haven `wood_trunk_wall`, `bark_brown_01/02`, `bark_willow`; ambientCG `Bark001–015`, `TreeEnd001–004` (log ends) |
| Adobe and plaster (Spanish, Pueblo) | Poly Haven `clay_plaster`, `clay_block_wall`, `plastered_wall_*`, `rough_plaster_*`; ambientCG `Plaster001–007`, `Clay001–009` |
| Clay roof tiles (Spanish, Portuguese) | Poly Haven `clay_roof_tiles(_02/_03)`, `roof_tiles`; ambientCG `RoofingTiles011–015` |
| Stone and brick (English, Dutch, French, forts, Maya, Aztec) | Poly Haven `stone_wall_*`, `castle_brick_*`, `brick_wall_*`, `sandstone_blocks_04/05/08`, `rough_block_wall`, `mossy_stone_wall`; ambientCG `Bricks0xx`, `PavingStones0xx` |
| Slate | Poly Haven `roof_slates_02/03`, `castle_wall_slates` |

**Model references (CC-BY, Sketchfab, free account):**

| Building type | Reference | Use |
|---|---|---|
| Spanish mission | [San Xavier del Bac, Thomas.Elliott](https://sketchfab.com/3d-models/2712c63b8f9d4f869c09212d5610cfb3) (1.4M faces, real building) | Proportions and detail reference for SpnSkl and MisPal; then rebuild. |
| English or colonial Georgian house | [Mount Clare Museum House, katiewolfe](https://sketchfab.com/3d-models/01c0cd3d7cf943b3bcf1bd22bd1392de) (1754 Georgian scan); [American Colonial house, Blueberry12](https://sketchfab.com/3d-models/3e536217e25545448762004ec55f1786) (78k faces) | References. |
| Wigwam | [Bark Covered Wigwam, bclspecialcollections](https://sketchfab.com/3d-models/1c57f95a341e456b83bf0884222e04dc) (scan of a museum miniature); [Wigwam Base Model, Giizhig](https://sketchfab.com/3d-models/6486e2daf6d54b13937e9da3a1624b07) | Shape reference; rebuild with bark materials. |
| Tipi | [Tipi, golovecky](https://sketchfab.com/3d-models/98cbc7ecf0ac4093928822771876d7f2) (16k faces) | Simple; scripting is easier. |
| Aztec or Maya stone | [Aztec Style Temple Kit, zaxel](https://sketchfab.com/3d-models/c7c5c028570f4e90a88814d9264cc31e) (392k faces, modular) | Kit-piece ideas; stepped platforms are easy to script. |
| Palisade | [Free Palisade logs, joanlahots](https://sketchfab.com/3d-models/e05e9b816eac4d5f88f1f0afd2ff7121) (334 polygons, 4K textures) | Directly usable as a log piece in `kit.py` palisades. |
| Log cabin | [Log Cabin (Free Download), klessgyzen](https://sketchfab.com/3d-models/2657189dbdff49fdb20ba33d6ebd5c52) | Reference. |

**Buildings, gaps:** Iroquois longhouse, Pueblo multi-storey blocks, Dutch gabled houses,
windmills, water mills and star forts: **nothing free and good was found.** Fallback: our
scripted kit with the CC0 materials above. Poly Haven's Smugglers Cove and The Shed props
(barrels, buckets, lanterns, crates, `gate_latch_01`, `large_castle_door`) dress the
buildings.

## Ships

| Pick | Licence | Quality | Fit |
|---|---|---|---|
| Poly Haven [dutch_ship_large_01](https://polyhaven.com/a/dutch_ship_large_01), [dutch_ship_large_02](https://polyhaven.com/a/dutch_ship_large_02), [dutch_ship_medium](https://polyhaven.com/a/dutch_ship_medium) | CC0 | 70–110k triangles, 4K PBR, ornate weathered hulls, rigging and sails | **Excellent.** 17th-century Dutch ships, the frigate (ShpFregat) and galleon (ShpSanta) class. Re-flag and re-paint them per nation; the sails take wind morphs. |
| Poly Haven [ship_pinnace](https://polyhaven.com/a/ship_pinnace) | CC0 | 184k triangles, **rigged** | Small ship (ShpPinta) or a transport. |
| [Aleksandr class archipelago frigate, Museovirasto](https://sketchfab.com/3d-models/f82f40ca04d84a24868a453c4e3a16c2) | CC-BY | 587k faces, scan of a museum model (1790) | Reference for an 18th-century frigate. |
| [galleon model, edds](https://sketchfab.com/3d-models/6c02dbf602ee47f99606a3f575620467) | CC-BY | 400k faces, UVs, **no textures** (Golden Hind) | Galleon reference or base if the Dutch ships are not enough. |
| Canoes | Sketchfab dugout scans: [SCIAA_MRD](https://sketchfab.com/3d-models/e50c2dae43f44192aa838e55c02480d8), [Red River Dugout, dostalc](https://sketchfab.com/3d-models/98c5b235d0f94b5f8b5736e45d9f80bc) (CC-BY, 1–1.6M faces) | CC-BY | Dugout shape and texture reference for ShpKanoe1/3; **no birch-bark canoe** found, so script that one (a lofted hull plus a bark material). |

Rowing boat (ShpLodka) and ferry (ShpParom): no free base found that matches; script
them from plank materials, with Poly Haven ship parts as a style match. Work for all ships:
decimate to 10–25k triangles, LODs, damage states, nation colours, a sail cloth shader.

## Artillery

| Pick | Licence | Quality | Fit |
|---|---|---|---|
| Poly Haven [cannon_01](https://polyhaven.com/a/cannon_01) | CC0 | 41k triangles, **rigged**, cast-iron barrel, weathered wooden truck carriage, PBR | A naval or garrison gun. The barrel is directly reusable; for a field gun, put it on a scripted two-wheel field carriage. |
| [18th Century Cannon, rivetech](https://sketchfab.com/3d-models/df60e8db9d56459b8d900d53f5235b10) | CC-BY | 11.9k faces | A field-gun base (ArtPus). |
| [Old Mortar Cannon, Mister_Fish](https://sketchfab.com/3d-models/160f87185c2745688f9aab52766cda44) | CC-BY | 8.2k faces, game-ready, modelled on a gun displayed in Lviv Castle | Mortar (HolArt, UsaArt). |

**Gap:** limbers and caissons. The only free one found ([Limber, kmkrtchyan70](https://sketchfab.com/3d-models/331c4d1c2ea3430fa05e856d363e9286))
is **AI-generated with Meshy** and 820k faces, so we reject it. Fallback: script the limber
(wheels, axle, ammunition chest, pole). The wheel is shared with the carts.

## Props

| Prop | Pick (CC0 unless noted) |
|---|---|
| Barrels | Poly Haven [wooden_barrels_01](https://polyhaven.com/a/wooden_barrels_01), wine_barrel_01, Barrel_01/02, barrel_03 |
| Crates and chests | Poly Haven [wooden_crate_01/02](https://polyhaven.com/a/wooden_crate_01), treasure_chest, old_military_crate |
| Buckets, baskets, lanterns | Poly Haven wooden_bucket_01/02, wicker_basket_01/02, wooden_lantern_01, spinning_wheel_01, tables, stools |
| Carts | [Old wooden cart, GreenG](https://sketchfab.com/3d-models/c06d6248b9d64173b376926ee212fd6f) (CC-BY, 57k faces, 4K Substance bake); [Medieval wooden cart, rakutin](https://sketchfab.com/3d-models/c4ee19920e9b45aaa6ebe37839b1ad2a) (CC-BY, 11k faces) |
| Fences | Scripted (split-rail and picket fences are trivial to script) with Poly Haven `wooden_gate` and plank materials; the free fence packs are modern |
| Fields and crops | See Trees (maize); fields as decals over `muddy_tracks` or ploughed earth (ambientCG Ground0xx) |
| Firewood, logs | ambientCG `Logs001`; Poly Haven tree_stump_01/02, dead_tree_trunk |

## Effects textures

| Effect | Pick | Licence | Work |
|---|---|---|---|
| Smoke (musket and cannon smoke, chimneys) | JangaFX free VDBs (smoke plume, chimney smoke, dust impact, dust shockwave); Unity Labs smoke flipbooks (wispy, cloud, disc) | CC0 | Render the VDBs in Blender into flipbook atlases lit from the RTS camera angle; Unity's sheets work as they are. |
| Fire (burning buildings, camp fires) | JangaFX small camp fire, simple fire, fire plume; Unity Labs flames and fireballs | CC0 | Same treatment; add emission. |
| Explosions (cannon impacts, powder magazines) | JangaFX ground explosion, building implosion; Unity Labs explosions | CC0 | Same. |
| Blood | **gap.** No professional CC0 blood decal or flipbook set was found. | | Fallback: generate it in Blender (metaball or fluid splats rendered to decals and a small flipbook). It matches our wound and decay work anyway. |
| Sparks, small puffs | Kenney Particle Pack | CC0 | Usable as sprites. |

## Needs the user's decision

| Item | Why | Value |
|---|---|---|
| **A free Sketchfab account** | Needed to download any Sketchfab model, CC0 included | Unlocks the animated animals (elk, hind, bear), the oaks, maples and aspens, the field cannon, the mortar, the carts and all the historic references. **Recommended.** |
| Quixel Megascans on Fab | The free period ended in 2024; now mostly paid; Epic account; Fab Standard Licence (any engine allowed, commercial use allowed) | Best-in-class scans of trees, ground and rocks. Not needed: Poly Haven and ambientCG cover the same ground for free. |
| Smithsonian 3D items not marked CC0 (for example Gunboat *Philadelphia*) | Non-commercial terms | Reference only; do not ship them. |
| WildMesh 2.0 animal pack (Patreon) | Paid | Fixed rigs for the elk, hind and bear plus about 100 more animals. Only if the clean-up of the free versions proves too costly. |

## Recommendation and order of work

1. **Materials first (no login, CC0, highest impact):** a fetch script with pinned hashes
   for ambientCG and Poly Haven terrain, building, bark and leaf sets, at 1K and 2K. This
   upgrades the terrain and every scripted building at once.
2. **Poly Haven models:** the Dutch ships and the pinnace (ShpFregat, ShpSanta, ShpPinta),
   cannon_01, props (barrels, crates, buckets, lanterns), rocks and cliffs, pines and firs.
   Work: decimate, LODs, nation re-paint. CC0, so there is no attribution.
3. **Smoke and fire:** JangaFX VDBs and Unity flipbooks, baked to our flipbook format.
   Muskets and cannons need smoke early, because it sells the period look.
4. **Sketchfab, once the user approves a free account:** first the elk, hind and bear from
   WildMesh (rig clean-up, then retarget to our clip names), then the cougar or jaguar,
   bison, alligator and eagle (clips written by us), oaks, maples and aspens (foliage cards,
   LODs), the field cannon and the mortar. Keep `CREDITS` up to date with every CC-BY import.
5. **Scripted fallbacks, in parallel:** limber and cart wheels, fences, rowing boat, ferry,
   birch-bark canoe, longhouse, Pueblo blocks, Dutch gables, mills, star forts, jungle
   canopy trees, monkey, and blood decals.

## Gaps and fallbacks (summary)

| Gap | Fallback |
|---|---|
| Animations for the bison, big cat, alligator, eagle and monkey | Rigify animal metarigs plus scripted gait cycles; Quaternius CC0 clips as timing reference |
| Monkey model | Scripted (adapt a base mesh) |
| 18th-century clothing | Our scripted cloth shells (#217) with ambientCG fabrics |
| Limber or caisson | Scripted (shares the wheel with the carts) |
| Birch-bark canoe, rowing boat, ferry | Scripted lofted hulls with plank and bark materials |
| Longhouse, Pueblo, Dutch houses, mills, star forts, fences | `kit.py` plus CC0 materials |
| Jungle canopy trees (ceiba, mahogany), cypress, cactus | Sapling or geometry-nodes trees with CC0 bark and leaf atlases |
| Blood decals | Rendered in Blender |
