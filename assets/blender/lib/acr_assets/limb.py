"""Limb levels (#262, pure Python, no `bpy`; unit-tested): the coordinate 0..1 along a limb
that the damage model cuts, breaks and wounds at.

A limb is the chain shoulder -> elbow -> wrist -> fingertip (arm) or hip -> knee -> ankle ->
toe tip (leg). The joints sit at fixed levels (`ARM_LEVELS`, `LEG_LEVELS`, out of 255; the
renderer's `render/body.rs` uses the same table); in between the level runs linearly along
each segment, so a joint lands exactly on its level. A vertex gets the level of its rest
position projected onto the segments of its own body part (an upper-arm vertex never takes a
forearm level), trunk vertices get `TRUNK` (-1). Clothes and strapped gear take the level of
the nearest skin vertex of the same limb (`Character.masks` does that with a KD tree).

`radius_profile` samples the outer radius (metres) of a limb at `SAMPLES` levels; the glTF
root extras `acr_limb_radius` = {limb: [SAMPLES radii]} scale the stump caps.
"""

import math

# Joint levels out of 255: shoulder, elbow, wrist, fingertip / hip, knee, ankle, toe tip.
ELBOW, WRIST, KNEE, ANKLE = 110, 205, 120, 215
ARM_LEVELS = (0, ELBOW, WRIST, 255)
LEG_LEVELS = (0, KNEE, ANKLE, 255)
TRUNK = -1.0
SAMPLES = 16

# Limb name -> (levels, chain bones: (bone, end) per chain point of skeleton H).
LIMBS = {
    "arm_l": (ARM_LEVELS, (("upperarm_l", "head"), ("forearm_l", "head"), ("hand_l", "head"), ("middle_03_l", "tail"))),
    "arm_r": (ARM_LEVELS, (("upperarm_r", "head"), ("forearm_r", "head"), ("hand_r", "head"), ("middle_03_r", "tail"))),
    "leg_l": (LEG_LEVELS, (("thigh_l", "head"), ("shin_l", "head"), ("foot_l", "head"), ("toe_l", "tail"))),
    "leg_r": (LEG_LEVELS, (("thigh_r", "head"), ("shin_r", "head"), ("foot_r", "head"), ("toe_r", "tail"))),
}
LIMB_ORDER = ("arm_l", "arm_r", "leg_l", "leg_r")

# Body part (`rig.PARTS`) -> (limb, chain segments the part may project onto).
PART_LIMB = {
    2: ("arm_l", (0,)), 3: ("arm_l", (1, 2)),
    4: ("arm_r", (0,)), 5: ("arm_r", (1, 2)),
    6: ("leg_l", (0,)), 7: ("leg_l", (1, 2)),
    8: ("leg_r", (0,)), 9: ("leg_r", (1, 2)),
}


def body_part(part, joints):
    """The body part of a `_PART` value: stump caps (16 + j parent side, 32 + j child side of
    `joints[j]`) map to the part they close; anything else (horse 48) is returned as is."""
    if 16 <= part < 16 + len(joints):
        return joints[part - 16][1]
    if 32 <= part < 32 + len(joints):
        return joints[part - 32][2]
    return part


def _sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def _dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def project(p, a, b):
    """(t, distance): the clamped parameter 0..1 of the point of segment a-b nearest to p and
    the distance to it."""
    ab = _sub(b, a)
    ll = _dot(ab, ab)
    t = 0.0 if ll == 0 else min(max(_dot(_sub(p, a), ab) / ll, 0.0), 1.0)
    q = tuple(x + y * t for x, y in zip(a, ab))
    return t, math.sqrt(_dot(_sub(p, q), _sub(p, q)))


def level(p, points, levels, segments=None):
    """(level 0..1, distance to the chain) of point p on the chain `points` with joint
    `levels` (out of 255), projecting onto `segments` (indices; default all) only."""
    best = None
    for i in segments if segments is not None else range(len(points) - 1):
        t, d = project(p, points[i], points[i + 1])
        if best is None or d < best[1] - 1e-12:
            best = ((levels[i] + (levels[i + 1] - levels[i]) * t) / 255.0, d)
    return best


def vertex_level(p, part, chains):
    """(limb or None, level, distance) of a vertex at rest position p with body part `part`
    (already through `body_part`); `chains` = {limb: chain points}. Trunk: (None, TRUNK, 0)."""
    if part not in PART_LIMB:
        return None, TRUNK, 0.0
    limb, segs = PART_LIMB[part]
    lv, d = level(p, chains[limb], LIMBS[limb][0], segs)
    return limb, lv, d


def radius_profile(samples, n=SAMPLES, quantile=0.95):
    """n radii at the levels k / (n - 1) from (level, radius) samples of one limb: per
    window of +-1/(2(n-1)) the `quantile` of the radii (the outer surface, robust against a
    stray flap); empty windows are interpolated from their neighbours (ends copy the
    nearest). Returns [0.0] * n without samples."""
    half = 0.5 / (n - 1)
    out = []
    for k in range(n):
        c = k / (n - 1)
        rs = sorted(r for lv, r in samples if abs(lv - c) <= half + 1e-9)
        out.append(rs[min(int(quantile * len(rs)), len(rs) - 1)] if rs else None)
    known = [i for i, r in enumerate(out) if r is not None]
    if not known:
        return [0.0] * n
    for i in range(n):
        if out[i] is not None:
            continue
        lo = max((k for k in known if k < i), default=None)
        hi = min((k for k in known if k > i), default=None)
        if lo is None:
            out[i] = out[hi]
        elif hi is None:
            out[i] = out[lo]
        else:
            out[i] = out[lo] + (out[hi] - out[lo]) * (i - lo) / (hi - lo)
    return [round(r, 4) for r in out]
