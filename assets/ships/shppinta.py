"""ShpPinta: the small ship, built on Poly Haven `dutch_ship_medium` (CC0, see assets/LICENSES.md).

Bow south, 28 m overall, keel 2.6 m below the waterline (the ground point); clips and rig: `acr_assets.ships`.
"""

from acr_assets import genbuild, ships

TYPE = "ShpPinta"

# True: the generated model (Tripo, from the sprite; `acr_assets.genbuild`) instead of the
# Poly Haven base.
GENERATED = False


def build(ctx):
    if GENERATED:
        return genbuild.ship(ctx, TYPE, length=28.0, draft=2.6)
    ships.build(ctx, "dutch_ship_medium", length=28.0, budget=20000, draft=2.6)
