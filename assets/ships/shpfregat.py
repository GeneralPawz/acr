"""ShpFregat: the frigate (LINKOR), built on Poly Haven `dutch_ship_large_01` (CC0, see assets/LICENSES.md).

Bow south, 40 m overall, waterline at the ground point; clips and rig: `acr_assets.ships`.
"""

from acr_assets import ships

TYPE = "ShpFregat"


def build(ctx):
    ships.build(ctx, "dutch_ship_large_01", length=40.0, budget=28000)
