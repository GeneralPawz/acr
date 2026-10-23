"""Dead tree: a bare, leafless trunk with branches (sprites 1, 14, 50, 351-353, 358, 396-398).

Source: Quadra3D, "Dead Trees Demo", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:351"
SPEC = {"source": "sketchfab2/dead-trees-demo/scene.gltf", "nodes": ["Object_10"], "kind": "tree", "height": 8.0, "budget": 5000, "leaves": []}


def build(ctx):
    nature.build(ctx, SPEC)
