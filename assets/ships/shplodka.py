"""ShpLodka: generated model (Tripo, from the sprite), fitted and rigged by `acr_assets.genbuild`
(7 m overall, keel 0.6 m below the waterline)."""

from acr_assets import genbuild

TYPE = "ShpLodka"


def build(ctx):
    genbuild.ship(ctx, TYPE, length=7.0, draft=0.6)
