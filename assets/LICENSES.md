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

## MakeHuman 1.x (CC0 1.0)

- **Files:** `assets/blender/data/makehuman/head.json` (#283) and
  `assets/blender/data/makehuman/body.json` (#287).
- **Source:** <https://github.com/makehumancommunity/makehuman>, commit
  `a8bc2d54ff0ac92e78ff71431b1023eda42bf482`:
  - the base mesh `makehuman/data/3dobjs/base.obj` (groups `body`, `helper-l-eye`,
    `helper-r-eye`): the head and upper neck, with the eyeballs;
  - the targets under `makehuman/data/targets/` (`macrodetails/caucasian-male-*`,
    `macrodetails/universal-male-*-averagemuscle-*`, `chin`, `cheek`, `nose`, `eyebrows`,
    `forehead`, `eyes`, `head`). The file lists the targets behind each `face_*` morph.
- **Licence:** CC0 1.0 Universal. The repository's `LICENSE.md`, section C, says "The base
  mesh and proxies, Targets and modifiers, Textures, Clothes, Poses and expressions ...
  have been released under CC0 1.0 Universal". The full text is in `LICENSE.ASSETS.md`, and
  each file's header says "This asset was explicitly released as CC0 in september 2020".
  The copyright holders at release were Data Collection AB, Joel Palmius and Jonas
  Hauquier.
- **How it was derived:**
  - The base mesh was set to a young adult man (macro targets
    `caucasian-male-young` and `universal-male-young-averagemuscle-averageweight`, both at
    weight 1).
  - It was cut about 4 cm below the chin and converted to metres with z up and -y
    forward, in 0.1 mm integers.
  - The listed targets were summed into the ten `face_*` morphs and limited to the
    head's vertices.
  - Made by `assets/blender/tools/makehuman_head.py`. The build decimates the head (`acr_assets/mhead.py`).
  - No MakeHuman code is used. The build does not need the MakeHuman download.
  - `body.json`: the same young adult man's `body` group below the chin (no head, no
    helpers) and the centres of the base mesh's `joint-*` helper cubes, in the same units.
    Made by `assets/blender/tools/makehuman_body.py`; the build reposes it onto skeleton H
    and decimates it (`acr_assets/mbody.py`).
