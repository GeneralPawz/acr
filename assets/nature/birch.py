"""Birch: a slim light-green tree (sprites 5, 17, 21, 49).

Source: bitgem, "Birch Tree - Proto Series - Free", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:17"
SPEC = {"source": "sketchfab2/birch-tree-proto-series-free/scene.gltf", "kind": "tree", "height": 8.0, "budget": 3000, "leaves": []}


def build(ctx):
    nature.build(ctx, SPEC)
