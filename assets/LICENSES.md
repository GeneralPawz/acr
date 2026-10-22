# Third-party assets

Assets made by others that the pipeline builds on. Everything else under `assets/` is
made for ACR. Only CC0 or CC-BY sources; CC-BY needs the credit below to ship with the
game (credits screen or bundled licence file).

| Asset | Used for | Author | Licence | Source |
|---|---|---|---|---|
| Horse Rig (`bd71883bad9845389ef4ec2801004801`) | base mesh for the dragoon horse (#170, #275); evaluated, not yet in the build | CreativeMachine (https://sketchfab.com/CreativeMachine) | CC-BY-4.0 (http://creativecommons.org/licenses/by/4.0/) | https://sketchfab.com/3d-models/horse-rig-bd71883bad9845389ef4ec2801004801 |

Notes:

- Horse Rig: downloaded 2026-10-22 from the Objaverse mirror of Sketchfab's public
  downloads (`huggingface.co/datasets/allenai/objaverse`, `glbs/000-125/<uid>.glb`), the
  Sketchfab glTF conversion: 38.8k + 3.2k triangles, 46-joint skin, three PNG textures.
  The licence is stated on the Sketchfab page (API: `license.slug = by`) and in the file's
  `asset.extras` (author, licence, source). Credit line: "Horse Rig" by CreativeMachine,
  CC BY 4.0, modified (decimated, refitted to rig R, repainted).
