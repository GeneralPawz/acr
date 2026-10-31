"""Fir branch tiers (`assets/nature/conifer.py`): the layout of a conifer's whorls of
branches, pure Python (no Blender). Screen pixels from the sprite's ground point, as the
nature scripts measure them: `sy` up is negative.

The silhouette of `TREES.GP` frame 87 is a stack of whorls: every tier is a ring of drooping
branches whose reach is the sprite's half width at that height; the gaps between tiers show
the trunk, which is what makes the fir read as tiers rather than one blob.
"""

import math
import random

# (screen y of the whorl, reach in px): half widths of the sprite's silhouette.
TIERS = [(-36, 33), (-50, 30), (-63, 27), (-76, 24), (-90, 21), (-104, 18), (-118, 14), (-131, 10), (-144, 5)]
TOP = -150
TRUNK_BASE = (0, 0.0)


def branches(seed=7, per_tier=(9, 9, 8, 8, 7, 7, 6, 5, 4)):
    """Branch list `(tier, angle, reach_px, sy, droop)`: each tier is a ring of evenly spaced
    branches (angle in radians around the trunk, rotated per tier so the tiers interlock),
    reach and height jittered a little."""
    rnd = random.Random(seed)
    out = []
    for i, ((sy, reach), n) in enumerate(zip(TIERS, per_tier)):
        turn = rnd.uniform(0, 2 * math.pi)
        for k in range(n):
            a = turn + 2 * math.pi * k / n + rnd.uniform(-0.18, 0.18)
            out.append((i, a, reach * rnd.uniform(0.88, 1.08), sy + rnd.uniform(-2.5, 2.5), rnd.uniform(0.30, 0.45)))
    return out


def silhouette_halfwidth(sy):
    """Half width in px of the layout at screen height `sy`: the widest branch reach of the
    tiers whose drooping branches cover that height (a branch spans `reach * droop` below its
    root)."""
    best = 0.0
    for tsy, reach in TIERS:
        drop = reach * 0.4
        if tsy - 3 <= sy <= tsy + drop:
            t = (sy - tsy) / drop if drop else 0.0
            best = max(best, reach * (1.0 - 0.35 * max(t, 0.0)))
    return best
