"""ShpKanoe1: generated model (Tripo, from the sprite), fitted and rigged by `acr_assets.genbuild`
(9 m overall, keel 0.3 m below the waterline)."""

from acr_assets import genbuild

TYPE = "ShpKanoe1"


def build(ctx):
    genbuild.ship(ctx, TYPE, length=9.0, draft=0.3)
