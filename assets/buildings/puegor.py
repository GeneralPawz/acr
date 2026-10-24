"""PueGor: generated model (Pixal3D, from its own `#STANDLO` sprite), stood upright and fitted
to the sprite by `acr_assets.genbuild.from_sprite`. The game draws one model per finished building, so it is the `Base` model
(`#STANDLO` with every section in its first state); the `Up` model (all sections built) is
`puegorup.py`, drawn once every section is done (#302)."""

from acr_assets import genbuild

TYPE = "PueGor"
CHECKS = ["--anim STANDLO"]


def build(ctx):
    genbuild.from_sprite(ctx, TYPE, budget=12000, rel="pixal3d/buildings/PueGorBase_sprite.glb")
