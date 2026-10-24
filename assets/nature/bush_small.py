"""Small bush: a low tuft (sprites 79, 80).

Source: marblesstudio, "Real Bush" (small 1), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:79"
SPEC = {"source": "sketchfab2/real-bush/scene.gltf", "nodes": ["Small_bush_1"], "kind": "bush", "height": 0.6, "budget": 600, "wind": {"trunk": 2.0, "branch": 3.0, "flutter": 2.5, "tempo": 2.0}}


def build(ctx):
    nature.build(ctx, SPEC)
