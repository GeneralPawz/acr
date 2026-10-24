"""ShpKanoe3: the war canoe, ShpKanoe1's generated model (Tripo) fitted to its own, longer
sprite and rigged by `acr_assets.genbuild` (10.5 m overall, keel 0.35 m below the waterline)."""

from acr_assets import genbuild

TYPE = "ShpKanoe3"


def build(ctx):
    genbuild.ship(ctx, TYPE, length=10.5, draft=0.35, rel="tripo/ShpKanoe1_sprite.glb")
