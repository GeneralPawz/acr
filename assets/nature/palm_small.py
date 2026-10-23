"""Small palm: a young palm (sprites 276-286).

Source: yadrogames, "Palm Trees", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:280"
SPEC = {"source": "sketchfab2/palm-trees/scene.gltf", "nodes": ["Bark_1_Bark", "Leaf_1_Leaf"], "kind": "tree", "height": 5.0, "budget": 3000}


def build(ctx):
    nature.build(ctx, SPEC)
