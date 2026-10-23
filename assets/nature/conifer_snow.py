"""Snow conifer: the conifer with snow on its branches (winter maps; no original sprite).

Source: dariiiii, "Coniferous Forest Assets Pack" (tree A), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:87"
SPEC = {"source": "sketchfab2/coniferous-forest-assets-pack/scene.gltf", "nodes": ["Object_4", "Object_5"], "kind": "tree", "height": 11.0, "budget": 6000, "leaves": ["branchmat"], "snow": 0.6}


def build(ctx):
    nature.build(ctx, SPEC)
