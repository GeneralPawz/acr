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

## Garments on the master (`hd_garments.py`, prototype)

The EngM18 outfit (full linen shirt, breeches, gaiters, waistcoat, coat) from the pipeline's
1768 drafts (`garments.py`), graded to the HD skin and draped with `drape.dress` under HD
settings patched in for the run only (nothing in `acr_assets` changes): panels meshed at
1.2 cm, every layer simulated in order on the solidified layers beneath (linen 0.5 mm, wool
1.5-2.5 mm, collision distances 2.5-3.8 mm), a damped end so the cloth comes to rest (end
speed and kinetic energy in the stats), weights from the surface beneath, the skin under
the clothes pushed in (`drape.hide_skin`; an untouched `hd.flesh` copy stays for checks).

    blender -b out/hd.blend --python hd_garments.py -- <worktree>/assets/blender/lib out/hd_dressed.blend 1.2
    blender -b out/hd_dressed.blend --python hd_render.py -- out/dressed jobs.json
