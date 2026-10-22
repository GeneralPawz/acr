# Third-party asset data

Data checked into `assets/` that was derived from someone else's work. Everything else under
`assets/` is our own.

## MakeHuman 1.x (CC0 1.0)

- **Files:** `assets/blender/data/makehuman/head.json` (#283).
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
