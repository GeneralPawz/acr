"""Pure geometry of struck map objects (rule `ball_terrain`; no `bpy`): the jagged break of a
cannon-felled trunk, the lying pose of the felled tree and the soft ground contact. Used by
`nature._tree_states` and `nature._chip`; tested in `tests/test_breaks.py`.

The felled state lies `FELL_DEG` about the model's +Y axis through the ground point, crown
towards +X: the renderer (`map_models::felled_transform`, FALL_ANGLE 1.48 rad) turns +X to the
fall direction and plays the fall by turning it back up, so the two angles must agree.
"""

import math
import random

# Degrees a felled tree lies from upright (= acr-game map_models FALL_ANGLE, 1.48 rad).
FELL_DEG = math.degrees(1.48)
# Lowest height (m) of anything lying on the ground, and the share of a depth below it kept
# (crushed branches spread flat instead of going into the ground).
FLOOR = 0.03
CRUSH = 0.12


def profile(n, seed, rough, spikes=3):
    """`n` heights (m, >= 0) around a broken trunk, angle k / n turns: a ragged break of
    about `rough` metres with `spikes` tall splinters (2..3.5 times as high). Seeded."""
    rnd = random.Random(seed)
    out = [rough * (0.15 + 0.55 * rnd.random()) for _ in range(n)]
    # Neighbours pull together a little: a break tears in strips, not in single-vertex noise.
    out = [0.5 * out[k] + 0.25 * (out[k - 1] + out[(k + 1) % n]) for k in range(n)]
    for k in rnd.sample(range(n), min(spikes, n)):
        out[k] = rough * rnd.uniform(2.0, 3.5)
    return out


def at_angle(heights, angle):
    """The break height of `profile` at `angle` (radians), linear between its samples."""
    n = len(heights)
    t = (angle / (2 * math.pi)) % 1.0 * n
    k = int(t) % n
    f = t - int(t)
    return heights[k] * (1 - f) + heights[(k + 1) % n] * f


def lay(p, deg=FELL_DEG):
    """Point `p` (x, y, z) of the standing tree turned `deg` about +Y through the origin, the
    top towards +X."""
    a = math.radians(deg)
    x, y, z = p
    return (x * math.cos(a) + z * math.sin(a), y, -x * math.sin(a) + z * math.cos(a))


def ground(z, floor=FLOOR, crush=CRUSH):
    """Height `z` of a lying vertex after it met the ground: above `floor` unchanged, below
    it squashed onto the floor (`crush` of the depth kept, never more than 0.1 m under)."""
    if z >= floor:
        return z
    return floor + max(crush * (z - floor), -0.1)
