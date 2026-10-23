"""Stone: a pale field stone.

Source: Poly Haven, "Namaqualand Stones 01" (a), CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "STONLIST:18"
SPEC = {"source": "polyhaven2/namaqualand_stones_01/namaqualand_stones_01_2k.gltf", "nodes": ["stones_01_a"], "kind": "rock", "height": 0.45, "budget": 800}


def build(ctx):
    nature.build(ctx, SPEC)
