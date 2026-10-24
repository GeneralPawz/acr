"""Dry bush: a dead, twiggy bush with a few brown leaves (sprites 143-172).

Source: marblesstudio, "Real Bush" (medium 2), Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`
with `dry` (the leaf atlas turned straw brown) and `keep` (most of the leaf cards, not grown).
"""

from acr_assets import nature

TYPE = "TREELIST:159"
SPEC = {"source": "sketchfab2/real-bush/scene.gltf", "nodes": ["Medium_bush_2"], "kind": "bush", "height": 1.4, "budget": 700,
        "dry": 0.9, "keep": 0.7, "leaf_level": 0.22, "wind": {"trunk": 1.5, "branch": 2.5, "flutter": 1.5, "tempo": 1.9}}


def build(ctx):
    nature.build(ctx, SPEC)
