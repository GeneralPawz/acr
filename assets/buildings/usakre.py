"""UsaKre: generated model (Pixal3D, from its own `#STANDLO` sprite), stood upright and fitted
to the sprite by `acr_assets.genbuild.from_sprite`. The game draws one model per finished building, so it is the `Base` model
(`#STANDLO` with every section in its first state); the `Up` model (all sections built) waits
for a renderer that swaps models by section state."""

from acr_assets import genbuild

TYPE = "UsaKre"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    genbuild.from_sprite(ctx, TYPE, budget=12000, rel="pixal3d/buildings/UsaKreBase_sprite.glb")
