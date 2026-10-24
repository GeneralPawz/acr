"""NetBlg3: neutral stone tower (a tall temple tower), scripted (`acr_assets.stonebuild`;
Pixal3D made a smooth lump of it, #305): plinth, tower in three set-back storeys with
string courses, a crenellated crown, a stair up one side."""

from acr_assets import stonebuild

TYPE = "NetBlg3"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=43)
    k.box("stone", (0, 0), (10.0, 10.0), 0.0, 1.0)
    z = k.tiers("stone", (0, 0), [(7.6, 7.6), (7.0, 7.0), (6.2, 6.2)], 1.0, [4.0, 3.6, 3.0])
    for zz in (5.0, 8.6):
        k.box("stone", (0, 0), (8.0 - (zz > 6) * 0.6, 8.0 - (zz > 6) * 0.6), zz - 0.2, zz + 0.2)
    k.box("stone", (0, 0), (6.8, 6.8), z, z + 0.4)
    k.merlons("stone", (0, 0), (6.6, 6.6), z + 0.4, 1.2, w=0.6, gap=0.5)
    k.box("stone", (0, 0), (3.0, 3.0), z, z + 2.0)
    k.door("stone", 0.0, -3.8, w=1.4, h=2.6, z0=1.0)
    k.cornice("stone", (0, 0), (6.2, 6.2), z - 0.3, 0.3, 0.3)
    k.stairs("stone", (6.8, -3.0), 1.8, 3.0, 0.0, 3.0, 9, 90)
    k.box("stone", (4.6, -3.0), (1.6, 2.2), 0.0, 3.0)  # landing
    k.heap("rubble", (0, 0), 6.0, 40, 0.5, 0.3, ring=0.85)
    stonebuild.finish(ctx, k)
