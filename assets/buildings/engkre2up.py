"""EngKre2Up: the upgraded `EngKre2` (every `ADDSECTION` built), generated (Pixal3D, from the
`#STANDLO` sprite with every section done) and fitted to that sprite (`acr model-ref
--sections 2`) by `acr_assets.genbuild.from_sprite`, like `usakreup.py`. Drawn instead of
`engkre2.glb` once every section is done (#302, docs/rendering-3d.md "Construction and upgrades")."""

from acr_assets import genbuild

TYPE = "EngKre2"
CHECKS = ["--anim STANDLO --sections 2"]


def build(ctx):
    ctx.ref = ctx.refs.get("STANDLO-2", ctx.ref)
    genbuild.from_sprite(ctx, TYPE, budget=12000, rel="pixal3d/buildings/EngKre2Up_sprite.glb")
