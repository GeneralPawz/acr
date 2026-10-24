"""SpnRudSt: stone quarry (mine), scripted (`acr_assets.stonebuild`; Pixal3D filled the pit in,
#305): a funnel pit in a ring of cut blocks, a timber scaffold on one side."""

from acr_assets import stonebuild

TYPE = "SpnRudSt"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=31)
    k.pit("rubble", "pit", (0, 0), 4.2, 3.0, 1.2, 0.05, squash=0.8)
    k.heap("stone", (0, 0), 6.0, 190, 0.7, 1.5, ring=0.68, squash=0.8)
    k.heap("stone", (0, 0), 3.0, 18, 0.5, 0.0, squash=0.8)  # loose blocks on the floor
    for x, y in ((-4.5, -1.0), (-4.5, 2.0), (-1.5, 2.0), (-1.5, -1.0)):
        k.beam("timber", (x, y, 0.0), (x, y, 4.0), 0.22)
    for z in (2.2, 3.9):
        k.beam("timber", (-4.6, -1.0, z), (-1.4, -1.0, z))
        k.beam("timber", (-4.6, 2.0, z), (-1.4, 2.0, z))
        k.beam("timber", (-4.5, -1.1, z), (-4.5, 2.1, z))
        k.beam("timber", (-1.5, -1.1, z), (-1.5, 2.1, z))
    k.box("timber", (-3.0, 0.5), (3.2, 3.2), 2.2, 2.35)
    stonebuild.finish(ctx, k)
