"""Broadleaf: an open-crowned deciduous tree (light green sprites 10, 11, 35, 47, 53-55).

Source: jungle_jim, "Realistic Trees Collection" (Medium011), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:35"
SPEC = {"source": "sketchfab2/realistic-trees-collection/scene.gltf", "nodes": ["Medium011"], "kind": "tree", "height": 7.0, "budget": 6000, "wind": {"trunk": 1.4, "branch": 1.8, "flutter": 2.0, "tempo": 1.0}}


def build(ctx):
    nature.build(ctx, SPEC)
