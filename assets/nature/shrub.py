"""Shrub: a leafy low shrub (sprites 3, 4, 16, 27, 31, 33).

Source: Poly Haven, "Shrub 02" (d), CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:3"
SPEC = {"source": "polyhaven2/shrub_02/shrub_02_2k.gltf", "nodes": ["shrub_02_d"], "kind": "bush", "height": 1.3, "budget": 3000}


def build(ctx):
    nature.build(ctx, SPEC)
