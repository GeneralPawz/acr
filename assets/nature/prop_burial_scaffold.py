"""Map prop: a native burial scaffold.

Source: generated with Pixal3D (TencentARC) on bbv-dev, local only (`$ACR_SOURCES/generated/pixal3d/prop_burial_scaffold.glb`,
assets/LICENSES.md); made game-ready by `acr_assets.genfit` (size, budget: its `ITEMS` row)
through `acr_assets.nature.build_generated`.
"""

from acr_assets import nature

SPEC = {"source": "pixal3d/prop_burial_scaffold.glb", "sway": None}


def build(ctx):
    nature.build_generated(ctx, SPEC)
