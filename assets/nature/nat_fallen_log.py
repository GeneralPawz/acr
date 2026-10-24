"""Map prop: a fallen tree trunk.

Source: generated with Pixal3D (TencentARC) on bbv-dev, local only (`$ACR_SOURCES/generated/pixal3d/nat_fallen_log.glb`,
assets/LICENSES.md); made game-ready by `acr_assets.genfit` (size, budget: its `ITEMS` row)
through `acr_assets.nature.build_generated`.
"""

from acr_assets import nature

TYPE = "TREELIST:439"
SPEC = {"source": "pixal3d/nat_fallen_log.glb", "sway": None}


def build(ctx):
    nature.build_generated(ctx, SPEC)
