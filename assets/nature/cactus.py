"""Cactus: a branching green cactus (sprites 188-216).

Source: yadrogames, "Cactus Pack" (cactus 4), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:189"
SPEC = {"source": "sketchfab2/cactus-pack/scene.gltf", "nodes": ["cactus_4_"], "kind": "bush", "height": 2.2, "budget": 1500, "leaves": [], "wind": {"trunk": 0.0, "branch": 0.0, "flutter": 0.0}}


def build(ctx):
    nature.build(ctx, SPEC)
