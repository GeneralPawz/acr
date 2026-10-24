"""Large broadleaf: a wide old deciduous tree (sprites 24, 30, 36).

Source: jungle_jim, "Realistic Trees Collection" (Large), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:24"
SPEC = {"source": "sketchfab2/realistic-trees-collection/scene.gltf", "nodes": ["EZTree0.Large_"], "kind": "tree", "height": 10.0, "budget": 8000, "wind": {"trunk": 1.3, "branch": 1.8, "flutter": 2.0, "tempo": 0.95}}


def build(ctx):
    nature.build(ctx, SPEC)
