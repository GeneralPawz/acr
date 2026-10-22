# Third-party assets

Assets made by others that the pipeline builds on. Everything else under `assets/` is
made for ACR. Only CC0 or CC-BY sources; CC-BY needs the credit below to ship with the
game (credits screen or bundled licence file). Exception: the bought WildMesh pack below,
used locally only (its files and the models built from it are never committed).

| Asset | Used for | Author | Licence | Source |
|---|---|---|---|---|
| Horse Rig (`bd71883bad9845389ef4ec2801004801`) | base mesh for the dragoon horse (#170, #275); evaluated, not yet in the build | CreativeMachine (https://sketchfab.com/CreativeMachine) | CC-BY-4.0 (http://creativecommons.org/licenses/by/4.0/) | https://sketchfab.com/3d-models/horse-rig-bd71883bad9845389ef4ec2801004801 |
| WildMesh 3D "100 MODELS" (Realistic Animals: BrownBear, Buffalo, AmericanAligator, Leopard, Elk, Doe, ...) | the animals `assets/animals/*.py` (#287) | WildMesh 3D (https://www.patreon.com/wildmesh), partly with AnimalMesh 3D | WildMesh Standard License v1.1 (bought, `LICENSE-STANDARD.txt` next to the pack) | purchase, kept outside the repo; scripts read it via `ACR_SOURCES` |

Notes:

- WildMesh Standard License v1.1 (read 2026-10-22), in short: for licensees with at most
  USD 100,000 revenue or funding in 12 months (above that a Studio License is required);
  non-exclusive, perpetual, commercial use allowed; may be modified, retextured,
  re-animated, optimised (s. 3d); end products may ship them only in compiled, packaged or
  otherwise non-extractable form (s. 3b). Not allowed: redistributing the assets or
  substantial derivatives as files (originals or modified), putting them in open
  repositories or download links, training AI/ML on them, claiming authorship (s. 4).
  Attribution not required; suggested credit: "3D Animals by WildMesh 3D —
  https://www.patreon.com/wildmesh". Consequences for ACR: the pack and the `.glb` files
  built from it stay out of git and out of any public download (a game build that ships
  loose `.glb` files would count as extractable: needs packing or a decision before any
  release); the repository holds only the build scripts.

- Horse Rig: downloaded 2026-10-22 from the Objaverse mirror of Sketchfab's public
  downloads (`huggingface.co/datasets/allenai/objaverse`, `glbs/000-125/<uid>.glb`), the
  Sketchfab glTF conversion: 38.8k + 3.2k triangles, 46-joint skin, three PNG textures.
  The licence is stated on the Sketchfab page (API: `license.slug = by`) and in the file's
  `asset.extras` (author, licence, source). Credit line: "Horse Rig" by CreativeMachine,
  CC BY 4.0, modified (decimated, refitted to rig R, repainted).
