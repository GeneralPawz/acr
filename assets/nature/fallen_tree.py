"""Fallen tree: a broadleaf lying on the ground with its crown (sprites 431-441, 444-446).

Source: jungle_jim, "Realistic Trees Collection" (Medium011, as broadleaf_ez), Sketchfab, CC-BY 4.0
(assets/LICENSES.md); built by `acr_assets.nature` with `lie`, half the leaf cards (`keep`) and wilting (`dry` 0.5): tipped over towards +X, the crown
squashed onto the ground, centred on its ground point. The map table gives each sprite its scale
(the sprite's width over the model's length), as the fit by height does not hold for a lying tree.
"""

from acr_assets import nature

TYPE = "TREELIST:439"
SPEC = {"source": "sketchfab2/realistic-trees-collection/scene.gltf", "nodes": ["Medium011"], "kind": "tree", "height": 7.0,
        "budget": 5000, "keep": 0.5, "dry": 0.5, "lie": {"deg": 86.0, "squash": 0.5}, "wind": {"trunk": 0.0, "branch": 0.0, "flutter": 1.2, "tempo": 1.0}}


def build(ctx):
    nature.build(ctx, SPEC)
