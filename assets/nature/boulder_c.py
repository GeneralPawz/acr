"""Big boulder: a tall weathered rock (rock sprites 226-264, 332-349).

Source: Poly Haven, "Namaqualand Boulder 05", CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "TREELIST:256"
SPEC = {"source": "polyhaven2/namaqualand_boulder_05/namaqualand_boulder_05_2k.gltf", "kind": "rock", "height": 2.4, "budget": 2000}


def build(ctx):
    nature.build(ctx, SPEC)
