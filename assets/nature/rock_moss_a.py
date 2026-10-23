"""Mossy rock: a dark rounded rock.

Source: Poly Haven, "Rock Moss Set 02" (rock 08), CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "STONLIST:7"
SPEC = {"source": "polyhaven2/rock_moss_set_02/rock_moss_set_02_2k.gltf", "nodes": ["rock08"], "kind": "rock", "height": 0.8, "budget": 1200}


def build(ctx):
    nature.build(ctx, SPEC)
