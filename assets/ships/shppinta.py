"""ShpPinta: the small ship, built on Poly Haven `dutch_ship_medium` (CC0, see assets/LICENSES.md).

Bow south, 28 m overall, waterline at the ground point; clips and rig: `acr_assets.ships`.
"""

from acr_assets import ships

TYPE = "ShpPinta"


def build(ctx):
    ships.build(ctx, "dutch_ship_medium", length=28.0, budget=20000)
