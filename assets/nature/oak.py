"""Oak: a broad, dense deciduous tree (dark green sprites 355-357, 381-395).

Source: greg-l, "Oak Tree", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:355"
SPEC = {"source": "sketchfab2/oak-tree/scene.gltf", "kind": "tree", "height": 9.0, "budget": 7000}


def build(ctx):
    nature.build(ctx, SPEC)
