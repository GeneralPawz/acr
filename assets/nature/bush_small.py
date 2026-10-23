"""Small bush: a low tuft (sprites 79, 80).

Source: marblesstudio, "Real Bush" (small 1), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:79"
SPEC = {"source": "sketchfab2/real-bush/scene.gltf", "nodes": ["Small_bush_1"], "kind": "bush", "height": 0.6, "budget": 600}


def build(ctx):
    nature.build(ctx, SPEC)
