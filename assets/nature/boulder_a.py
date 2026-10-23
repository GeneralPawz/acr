"""Boulder: a rounded sandstone boulder.

Source: Poly Haven, "Namaqualand Boulder 02", CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "STONLIST:4"
SPEC = {"source": "polyhaven2/namaqualand_boulder_02/namaqualand_boulder_02_2k.gltf", "kind": "rock", "height": 1.4, "budget": 1600}


def build(ctx):
    nature.build(ctx, SPEC)
