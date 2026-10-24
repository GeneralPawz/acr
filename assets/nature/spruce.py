"""Spruce: a dense dark conifer (sprite 131).

Source: dariiiii, "Coniferous Forest Assets Pack" (tree C), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:131"
SPEC = {"source": "sketchfab2/coniferous-forest-assets-pack/scene.gltf", "nodes": ["Object_13", "Object_14"], "kind": "tree", "height": 12.0, "budget": 6000, "leaves": ["branchmat"], "max_width": 5.5, "wind": {"trunk": 0.8, "branch": 0.4, "flutter": 0.6, "tempo": 0.8, "bend": 2.8}}


def build(ctx):
    nature.build(ctx, SPEC)
