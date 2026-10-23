"""Tall bush: a loose upright bush (sprites 56-58, 75-83).

Source: marblesstudio, "Real Bush" (tall 1), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:56"
SPEC = {"source": "sketchfab2/real-bush/scene.gltf", "nodes": ["tall_bush_1"], "kind": "bush", "height": 1.8, "budget": 1000}


def build(ctx):
    nature.build(ctx, SPEC)
