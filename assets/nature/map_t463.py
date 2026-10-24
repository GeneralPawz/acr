"""Map object TREELIST 463 (ruin), generated from its own sprite.

Source: Pixal3D (TencentARC) on bbv-dev from the original's TREELIST frame 463, local only
(`$ACR_SOURCES/generated/pixal3d/map_t463.glb`, assets/LICENSES.md); stood upright, fitted to
the sprite and made game-ready by `acr_assets.nature.build_generated` (`map`).
"""

from acr_assets import nature

TYPE = "TREELIST:463"
SPEC = {"source": "pixal3d/map_t463.glb", "map": "ruin"}


def build(ctx):
    nature.build_generated(ctx, SPEC)
