"""ShpFregat: the frigate (LINKOR), built on Poly Haven `dutch_ship_large_01` (CC0, see assets/LICENSES.md).

Bow south, 40 m overall, keel 3.6 m below the waterline (the ground point); clips and rig: `acr_assets.ships`.
"""

from acr_assets import genbuild, ships

TYPE = "ShpFregat"

# True: the generated model (Tripo, from the sprite; `acr_assets.genbuild`) instead of the
# Poly Haven base.
GENERATED = False


def build(ctx):
    if GENERATED:
        return genbuild.ship(ctx, TYPE, length=40.0, draft=3.6)
    ships.build(ctx, "dutch_ship_large_01", length=40.0, budget=28000, draft=3.6)
