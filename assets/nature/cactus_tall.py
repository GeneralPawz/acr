"""Tall cactus: an old columnar cactus (sprites 173-187).

Source: yadrogames, "Cactus Pack" (cactus 6), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:187"
SPEC = {"source": "sketchfab2/cactus-pack/scene.gltf", "nodes": ["cactus_6_"], "kind": "tree", "height": 4.0, "budget": 1500, "leaves": []}


def build(ctx):
    nature.build(ctx, SPEC)
