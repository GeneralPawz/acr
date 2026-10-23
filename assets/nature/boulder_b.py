"""Boulder: a split granite boulder.

Source: Poly Haven, "Namaqualand Boulder 03", CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "STONLIST:10"
SPEC = {"source": "polyhaven2/namaqualand_boulder_03/namaqualand_boulder_03_2k.gltf", "kind": "rock", "height": 1.1, "budget": 1600}


def build(ctx):
    nature.build(ctx, SPEC)
