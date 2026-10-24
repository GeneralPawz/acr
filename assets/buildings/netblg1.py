"""NetBlg1: neutral stone palace ruin, scripted (`acr_assets.stonebuild`; Pixal3D made a
smooth lump of it, #305): a wide terrace ringed by a low wall with pillars, the palace
block with a stepped roof and a roof comb, a front stair."""

from acr_assets import stonebuild

TYPE = "NetBlg1"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=41)
    k.box("stone", (0, 0), (17.0, 15.0), 0.0, 1.0)
    for (x, y), (sx, sy) in (((0, -7.2), (16.0, 0.6)), ((0, 7.2), (16.0, 0.6)),
                             ((-8.2, 0), (0.6, 14.0)), ((8.2, 0), (0.6, 14.0))):
        k.box("stone", (x, y), (sx, sy), 1.0, 2.4)
    for x in (-8.2, -4.0, 4.0, 8.2):
        for y in (-7.2, 7.2):
            k.box("stone", (x, y), (0.9, 0.9), 1.0, 4.4)
    for y in (-3.0, 3.0):
        k.box("stone", (-8.2, y), (0.9, 0.9), 1.0, 4.4)
        k.box("stone", (8.2, y), (0.9, 0.9), 1.0, 4.4)
    z = k.tiers("stone", (0.5, 1.0), [(8.0, 6.5), (8.6, 7.1), (6.8, 5.4), (5.0, 3.6)], 1.0, [3.6, 0.4, 1.0, 1.0])
    k.box("stone", (0.5, 1.0), (4.0, 0.6), z, z + 1.6)  # roof comb
    k.box("stone", (0.5, 1.0), (3.0, 0.5), z + 1.6, z + 2.4, taper=0.1)
    k.cornice("stone", (0.5, 1.0), (8.0, 6.5), 3.2, 0.3, 0.2)
    for x in (-1.8, 0.5, 2.8):
        k.door("stone", x, -2.25, w=1.0, h=2.0, z0=1.0)
    k.stairs("stone", (0.5, -8.6), 3.0, 1.6, 0.0, 1.0, 4, 0)
    k.stairs("stone", (0.5, -2.2), 2.6, 2.8, 1.0, 4.6, 8, 0)
    k.heap("rubble", (0, 0), 7.0, 50, 0.6, 0.3, ring=0.4, squash=0.85)
    stonebuild.finish(ctx, k)
