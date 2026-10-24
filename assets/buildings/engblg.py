"""EngBlg: generated model (Pixal3D, from its own `#STANDLO` sprite), stood upright and fitted
to the sprite by `acr_assets.genbuild.from_sprite`."""

from acr_assets import genbuild

TYPE = "EngBlg"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    genbuild.from_sprite(ctx, TYPE, budget=12000)
