"""ShpSanta: the galleon (LINKOR), built on Poly Haven `dutch_ship_large_02` (CC0, see assets/LICENSES.md).

Bow south, 34 m overall, keel 3.2 m below the waterline (the ground point); clips and rig: `acr_assets.ships`.
"""

from acr_assets import genbuild, ships

TYPE = "ShpSanta"

# True: the generated model (Tripo, from the sprite; `acr_assets.genbuild`) instead of the
# Poly Haven base.
GENERATED = False


def build(ctx):
    if GENERATED:
        return genbuild.ship(ctx, TYPE, length=34.0, draft=3.2)
    ships.build(ctx, "dutch_ship_large_02", length=34.0, budget=24000, draft=3.2)
