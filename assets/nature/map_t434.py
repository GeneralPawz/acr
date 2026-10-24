"""Map object TREELIST 434 (rubble), generated from its own sprite.

Source: Pixal3D (TencentARC) on bbv-dev from the original's TREELIST frame 434, local only
(`$ACR_SOURCES/generated/pixal3d/map_t434.glb`, assets/LICENSES.md); stood upright, fitted to
the sprite and made game-ready by `acr_assets.nature.build_generated` (`map`).
"""

from acr_assets import nature

TYPE = "TREELIST:434"
SPEC = {"source": "pixal3d/map_t434.glb", "map": "rubble"}


def build(ctx):
    nature.build_generated(ctx, SPEC)
