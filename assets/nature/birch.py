"""Birch: a slim light-green tree (sprites 5, 17, 21, 49). The Proto model's blob crown is replaced
by leaf cards in the sprites' crown green (`crown_cards`, sRGB 0.27, 0.38, 0.14 on TREES.GP 17/21).

Source: bitgem, "Birch Tree - Proto Series - Free", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:17"
SPEC = {"source": "sketchfab2/birch-tree-proto-series-free/scene.gltf", "kind": "tree", "height": 8.0, "budget": 3000, "leaves": [], "recolour": True, "crown_cards": {"cards": 900, "size": 0.075, "depth": 0.08, "low": 0.12, "rgb": [0.27, 0.38, 0.14]}, "wind": {"trunk": 1.8, "branch": 2.0, "flutter": 2.5, "tempo": 1.2, "crown": 0.35}}


def build(ctx):
    nature.build(ctx, SPEC)
