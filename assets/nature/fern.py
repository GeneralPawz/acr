"""Fern: a low fan of fronds (palmetto and fern sprites 265, 297-302).

Source: Poly Haven, "Fern 02" (b), CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:298"
SPEC = {"source": "polyhaven2/fern_02/fern_02_2k.gltf", "nodes": ["fern_02_b"], "kind": "bush", "height": 0.9, "budget": 2400, "wind": {"trunk": 2.0, "branch": 3.0, "flutter": 2.5, "tempo": 1.7}}


def build(ctx):
    nature.build(ctx, SPEC)
