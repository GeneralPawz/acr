"""Mossy rock: a flat dark rock.

Source: Poly Haven, "Rock Moss Set 02" (rock 10), CC0 (assets/LICENSES.md); built by `acr_assets.nature`.
"""

from acr_assets import nature

TYPE = "STONLIST:37"
SPEC = {"source": "polyhaven2/rock_moss_set_02/rock_moss_set_02_2k.gltf", "nodes": ["rock10"], "kind": "rock", "height": 0.6, "budget": 1200}


def build(ctx):
    nature.build(ctx, SPEC)
