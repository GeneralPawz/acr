"""Map prop: a clump of maize.

Source: generated with Rodin (Hyper3D), local only (`$ACR_SOURCES/generated/rodin/nat_maize.glb`,
assets/LICENSES.md); made game-ready by `acr_assets.genfit` (size, budget: its `ITEMS` row)
through `acr_assets.nature.build_generated`.
"""

from acr_assets import nature

SPEC = {"source": "rodin/nat_maize.glb", "sway": "stalk"}


def build(ctx):
    nature.build_generated(ctx, SPEC)
