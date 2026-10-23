"""Palm: a tall coconut palm (sprites 266-296, 303-321).

Source: NextSpring, "Realistic Palm Tree 2 (Free)", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:313"
SPEC = {"source": "sketchfab2/realistic-palm-tree-2-free/scene.gltf", "nodes": ["M_Palm_P2_31"], "kind": "tree", "height": 9.0, "budget": 4000}


def build(ctx):
    nature.build(ctx, SPEC)
