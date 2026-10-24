"""Pine: a small stylised conifer (young pines, sprites 93, 94, 109, 111).

Source: bitgem, "Pine Tree - Proto Series - Free", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:93"
SPEC = {"source": "sketchfab2/pine-tree-proto-series-free/scene.gltf", "kind": "tree", "height": 6.0, "budget": 3000, "leaves": [], "recolour": True}


def build(ctx):
    nature.build(ctx, SPEC)
