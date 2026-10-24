"""Conifer: a fir or spruce of the northern forests (sprites 87-94, 100-102, 107-109).

Source: dariiiii, "Coniferous Forest Assets Pack" (tree A), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:87"
SPEC = {"source": "sketchfab2/coniferous-forest-assets-pack/scene.gltf", "nodes": ["Object_4", "Object_5"], "kind": "tree", "height": 11.0, "budget": 6000, "leaves": ["branchmat"], "wind": {"trunk": 0.8, "branch": 0.4, "flutter": 0.6, "tempo": 0.8, "bend": 2.8}}


def build(ctx):
    nature.build(ctx, SPEC)
