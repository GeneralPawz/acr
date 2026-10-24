"""NetBlg8: neutral stone tower (a square ruin), scripted (`acr_assets.stonebuild`; Pixal3D
made a smooth lump of it, #305): a masonry block, a pierced parapet, a stair on one side."""

from acr_assets import stonebuild

TYPE = "NetBlg8"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=38)
    k.box("stone", (0, 0), (9.0, 9.0), 0.0, 5.2)
    k.box("stone", (0, 0), (9.4, 9.4), 5.0, 5.4)
    k.merlons("stone", (0, 0), (9.0, 9.0), 5.4, 1.4, w=0.7, gap=0.35, depth=0.5)
    k.box("stone", (0, 0), (9.0, 9.0), 6.6, 6.9)
    k.heap("rubble", (0, 0), 3.0, 30, 0.6, 0.0)  # fallen stones on the roof
    k.stairs("stone", (6.8, -2.6), 1.8, 2.2, 0.0, 2.2, 7, 90)
    k.heap("rubble", (0, 0), 7.0, 60, 0.5, 0.4, ring=0.9)
    stonebuild.finish(ctx, k)
