"""Map prop: a fitted-stone Inca wall (ruins).

Source: generated with Rodin (Hyper3D), local only (`$ACR_SOURCES/generated/rodin/prop_inca_wall.glb`,
assets/LICENSES.md); made game-ready by `acr_assets.genfit` (size, budget: its `ITEMS` row)
through `acr_assets.nature.build_generated`.
"""

from acr_assets import nature

SPEC = {"source": "rodin/prop_inca_wall.glb", "sway": None}


def build(ctx):
    nature.build_generated(ctx, SPEC)
