"""UsaKreUp: the upgraded American fort (`UsaKre` with its section built, `ADDSECTION 0`):
generated model (Pixal3D, from the `#STANDLO` sprite with every section done), stood
upright and fitted to that sprite (`acr model-ref --sections 2`) by
`acr_assets.genbuild.from_sprite`. The renderer draws it instead of `usakre.glb` once every
section of the fort is done (#302, docs/rendering-3d.md "Construction and upgrades")."""

from acr_assets import genbuild

TYPE = "UsaKre"
CHECKS = ["--anim STANDLO --sections 2"]


def build(ctx):
    ctx.ref = ctx.refs.get("STANDLO-2", ctx.ref)
    genbuild.from_sprite(ctx, TYPE, budget=12000, rel="pixal3d/buildings/UsaKreUp_sprite.glb")
