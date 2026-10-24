"""Map object TREELIST 447 (rubble), generated from its own sprite.

Source: Pixal3D (TencentARC) on bbv-dev from the original's TREELIST frame 447, local only
(`$ACR_SOURCES/generated/pixal3d/map_t447.glb`, assets/LICENSES.md); stood upright, fitted to
the sprite and made game-ready by `acr_assets.nature.build_generated` (`map`).
"""

from acr_assets import nature

TYPE = "TREELIST:447"
SPEC = {"source": "pixal3d/map_t447.glb", "map": "rubble"}


def build(ctx):
    nature.build_generated(ctx, SPEC)
