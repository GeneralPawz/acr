"""Stone: a small flat stone.

Source: Poly Haven, "Namaqualand Stones 01" (d), CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "STONLIST:40"
SPEC = {"source": "polyhaven2/namaqualand_stones_01/namaqualand_stones_01_2k.gltf", "nodes": ["stones_01_d"], "kind": "rock", "height": 0.3, "budget": 600}


def build(ctx):
    nature.build(ctx, SPEC)
