"""SpnRudCo: coal mine, scripted (`acr_assets.stonebuild`; Pixal3D filled the pit in, #305): an
open funnel pit in a ring of pale rubble, a plank shed on the back of the rim, steps down one side."""

from acr_assets import stonebuild

TYPE = "SpnRudCo"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    k = stonebuild.Kit(seed=33)
    k.pit("rubble", None, (0, 0), 2.9, 2.0, 1.3, 0.0, squash=0.85)
    k.heap("rubble", (0, 0), 4.6, 150, 0.7, 1.6, ring=0.62, squash=0.85)
    k.box("timber", (1.6, 3.6), (3.2, 2.6), 0.0, 2.6)
    k.box("shingles", (1.6, 3.6), (3.6, 3.0), 2.6, 3.2, taper=0.3)
    k.stairs("stone", (4.9, 0.6), 1.4, 2.0, 0.0, 1.5, 6, 90)
    stonebuild.finish(ctx, k)
