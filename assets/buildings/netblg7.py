"""NetBlg7: neutral stone temple with two towers, scripted (`acr_assets.stonebuild`; Pixal3D
made a smooth lump of it, #305): a three-step platform pyramid, a tall and a lower tower on
it, a front stair to the top."""

from acr_assets import stonebuild

TYPE = "NetBlg7"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=47)
    z = k.tiers("stone", (0, 0), [(13.0, 13.0), (11.0, 11.0), (9.0, 9.0)], 0.0, [2.2, 2.0, 1.8], taper=0.03)
    k.box("stone", (-2.0, 1.8), (3.8, 3.8), z, z + 7.5)
    k.box("stone", (-2.0, 1.8), (4.2, 4.2), z + 7.5, z + 8.0)
    k.box("stone", (2.4, 2.2), (3.4, 3.4), z, z + 5.0)
    k.merlons("stone", (2.4, 2.2), (3.4, 3.4), z + 5.0, 0.9, w=0.5, gap=0.4)
    k.stairs("stone", (0.6, -6.5), 3.4, 5.0, 0.0, z, 14, 0)
    k.heap("rubble", (0, 0), 8.0, 70, 0.6, 0.5, ring=0.82)
    stonebuild.finish(ctx, k)
