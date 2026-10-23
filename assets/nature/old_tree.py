"""Old tree: a gnarled broad tree (jungle and old-growth sprites 22, 39, 42-45, 368-371).

Source: evolveduk, "Old Tree", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:42"
SPEC = {"source": "sketchfab2/old-tree/scene.gltf", "nodes": ["Tree_0_", "Tree_2_"], "kind": "tree", "height": 9.0, "budget": 8000, "leaves": ["tree_2mat"]}


def build(ctx):
    nature.build(ctx, SPEC)
