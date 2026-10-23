"""Reeds: a clump of reeds or tall grass (sprites 322-326).

Source: daniel.2U, "Reeds Low Poly", Sketchfab, CC-BY 4.0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:323"
SPEC = {"source": "sketchfab2/reeds-low-poly/scene.gltf", "kind": "bush", "height": 1.4, "budget": 1000, "leaves": ["material.03"]}


def build(ctx):
    nature.build(ctx, SPEC)
