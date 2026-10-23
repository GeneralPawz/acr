"""Mossy rock: a blocky grey rock.

Source: Poly Haven, "Rock Moss Set 02" (rock 12), CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "STONLIST:22"
SPEC = {"source": "polyhaven2/rock_moss_set_02/rock_moss_set_02_2k.gltf", "nodes": ["rock12"], "kind": "rock", "height": 0.7, "budget": 1200}


def build(ctx):
    nature.build(ctx, SPEC)
