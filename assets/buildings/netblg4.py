"""NetBlg4: neutral stone temple, scripted (`acr_assets.stonebuild`; Pixal3D made a smooth
lump of it, #305): a terrace, the temple block with a crenellated upper storey, a wide front
stair and a stele by it."""

from acr_assets import stonebuild

TYPE = "NetBlg4"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=44)
    k.box("stone", (0, 0), (13.0, 11.0), 0.0, 1.0)
    k.box("stone", (0, 1.5), (9.0, 7.0), 1.0, 5.0)
    k.box("stone", (0, 1.5), (9.6, 7.6), 4.8, 5.2)
    k.box("stone", (-0.5, 2.0), (6.0, 5.0), 5.2, 8.0)
    k.merlons("stone", (-0.5, 2.0), (6.0, 5.0), 8.0, 1.0, w=0.5, gap=0.4)
    k.stairs("stone", (0, -5.5), 3.2, 3.5, 0.0, 5.0, 12, 0)
    k.box("stone", (3.9, -3.2), (1.6, 1.6), 1.0, 5.0, taper=0.12)  # stele
    k.box("stone", (3.9, -3.2), (1.0, 1.0), 5.0, 5.8, taper=0.3)
    k.heap("rubble", (0, 0), 7.5, 60, 0.6, 0.5, ring=0.8, squash=0.85)
    stonebuild.finish(ctx, k)
