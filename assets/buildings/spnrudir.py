"""SpnRudIr: iron mine, scripted (`acr_assets.stonebuild`; Pixal3D filled the pit in, #305): a
funnel pit in a ring of rubble, a timber headframe over the shaft, barrels and crates."""

from acr_assets import stonebuild

TYPE = "SpnRudIr"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=32)
    k.pit("earth", "pit", (0, 0), 3.4, 1.9, 1.0, 0.05, squash=0.75)
    k.heap("rubble", (0, 0), 5.2, 150, 0.6, 1.2, ring=0.66, squash=0.75)
    for x, y in ((-1.4, -0.4), (-1.4, 1.2), (0.6, 1.2), (0.6, -0.4)):
        k.beam("timber", (x, y, 0.0), (x, y, 3.6), 0.22)
    for z in (1.8, 3.5):
        k.beam("timber", (-1.5, -0.4, z), (0.7, -0.4, z))
        k.beam("timber", (-1.5, 1.2, z), (0.7, 1.2, z))
    k.beam("timber", (-0.4, -0.5, 3.6), (-0.4, 1.3, 3.6), 0.28)  # the winding beam
    for x, y in ((-2.2, -3.2), (-1.4, -3.4), (1.8, -3.0), (2.4, -2.5)):
        k.box("timber", (x, y), (0.7, 0.7), 0.0, 0.85, taper=0.05)  # barrels and crates
    stonebuild.finish(ctx, k)
