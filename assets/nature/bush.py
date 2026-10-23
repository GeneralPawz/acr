"""Bush: a round medium bush (sprites 125-130, 132).

Source: marblesstudio, "Real Bush" (medium 1), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:126"
SPEC = {"source": "sketchfab2/real-bush/scene.gltf", "nodes": ["Medium_bush_1"], "kind": "bush", "height": 1.1, "budget": 1000}


def build(ctx):
    nature.build(ctx, SPEC)
