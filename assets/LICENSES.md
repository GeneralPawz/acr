# Third-party assets

Assets made by others that the pipeline builds on. Everything else under `assets/` is
made for ACR. Only CC0 or CC-BY sources; CC-BY needs the credit below to ship with the
game (credits screen or bundled licence file).

| Asset | Used for | Author | Licence | Source |
|---|---|---|---|---|
| Horse Rig (`bd71883bad9845389ef4ec2801004801`) | base mesh for the dragoon horse (#170, #275); evaluated, not yet in the build | CreativeMachine (https://sketchfab.com/CreativeMachine) | CC-BY-4.0 (http://creativecommons.org/licenses/by/4.0/) | https://sketchfab.com/3d-models/horse-rig-bd71883bad9845389ef4ec2801004801 |
| Dutch Ship Large 01 (`dutch_ship_large_01`) | base of the frigate `ShpFregat` (#287) | James Ray Cock, Rico Cilliers, Nicolò Zubbini (Poly Haven) | CC0 | https://polyhaven.com/a/dutch_ship_large_01 |
| Dutch Ship Large 02 (`dutch_ship_large_02`) | base of the galleon `ShpSanta` (#287) | James Ray Cock, Rico Cilliers, Nicolò Zubbini (Poly Haven) | CC0 | https://polyhaven.com/a/dutch_ship_large_02 |
| Dutch Ship Medium (`dutch_ship_medium`) | base of the small ship `ShpPinta` (#287) | James Ray Cock, Rico Cilliers, Nicolò Zubbini (Poly Haven) | CC0 | https://polyhaven.com/a/dutch_ship_medium |
| 18th Century Cannon (`df60e8db9d56459b8d900d53f5235b10`) | barrel of the field cannon `ArtPus` (#287) | rivetech (https://sketchfab.com/rivetech) | CC-BY-4.0 (http://creativecommons.org/licenses/by/4.0/) | https://sketchfab.com/3d-models/df60e8db9d56459b8d900d53f5235b10 |

Notes:

- Horse Rig: downloaded 2026-10-22 from the Objaverse mirror of Sketchfab's public
  downloads (`huggingface.co/datasets/allenai/objaverse`, `glbs/000-125/<uid>.glb`), the
  Sketchfab glTF conversion: 38.8k + 3.2k triangles, 46-joint skin, three PNG textures.
  The licence is stated on the Sketchfab page (API: `license.slug = by`) and in the file's
  `asset.extras` (author, licence, source). Credit line: "Horse Rig" by CreativeMachine,
  CC BY 4.0, modified (decimated, refitted to rig R, repainted).
- Ships (Poly Haven, CC0, no attribution required; credited anyway): the 2k glTF downloads,
  kept outside the repo under `$ACR_SOURCES/polyhaven/<id>/` (base colour textures only).
  Authors per `SOURCE.json`: James Ray Cock (model, textures, clean-up), Rico Cilliers
  (sails), Nicolò Zubbini (original model). Decimated, re-oriented, scaled, cut at the
  waterline, rigged and animated (`assets/blender/lib/acr_assets/ships.py`).
- 18th Century Cannon: downloaded 2026-10-22 from Sketchfab (`18th-century-cannon.zip`,
  FBX + PBR PNGs), kept under `$ACR_SOURCES/sketchfab/18th-century-cannon/`. Only the barrel
  and its albedo texture are used; the field carriage and wheels are ours. Credit line:
  "18th Century Cannon" by rivetech, CC BY 4.0, modified (barrel only, on a new carriage,
  animated).
