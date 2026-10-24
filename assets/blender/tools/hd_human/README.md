# HD base human prototype (m2-hd-human)

Master-first human: MakeHuman 1.x base mesh (CC0) at full resolution, young male targets,
reposed onto skeleton H (`acr_assets.mbody.repose`), finger bones refitted to MakeHuman's
finger joints, heat weights (4 per vertex), Catmull-Clark level 1 then symmetric collapse to
~78k triangles; one full-body UV set with a 4k albedo (MakeHuman CC0 skin, portrait baked in
through a frontal projection), 4k micro-detail normal and roughness. Clips come from a built
unit's `--blend` (skeleton H with its actions). Outputs are local-only.

Run on the VM (paths are examples):

    python3 -I tex.py <skins/young_caucasian_male/young_lightskinned_male_diffuse.png> maps
    blender -b --factory-startup --python hd_build.py -- <makehuman/data> <engm18.blend> maps \
        <aligned portrait.png> out/hd.blend <worktree>/assets/blender/lib 78000
    blender -b out/hd.blend --python hd_render.py -- out/renders jobs.json
    blender -b out/hd.blend --python hd_lod.py -- out/lod

Sources: makehuman/data from github.com/makehumancommunity/makehuman; skins from
makehuman_system_assets_cc0.zip (files2.makehumancommunity.org); see assets/LICENSES.md.

Round 2: the pipeline version is `assets/blender/lib/acr_assets/hdbody.py` with the opt-in unit
`assets/units/hdman.py` (`cargo xtask assets hdman`; stats under `hd` in hdman.stats.json,
LODs as `acr_lod` nodes in hdman.glb). These prototype scripts stay for the renders.
