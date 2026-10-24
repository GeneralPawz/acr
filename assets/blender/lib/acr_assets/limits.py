"""Anatomical joint limits of skeleton H and the contact grip solver (pure Python, no `bpy`;
unit-tested). docs/assets-humans.md, "Joint limits".

The table lives in `assets/blender/data/joint_limits.json` so the Rust ragdoll's test
(`crates/acr-physics/src/ragdoll.rs`) checks its elbow and knee hinges against the same
numbers. Ranges are degrees of the bone's local Euler XYZ (Blender's order: R = Rz Ry Rx)
relative to its rest pose, given for the left side; the right side mirrors them (x the same,
y and z negated: skeleton H's right bones are the X-mirror of the left ones). Local frames:
Y along the bone; vertical bones +Z forward, level bones +Z up (`armature.build`); fingers
and thumb +Z palmar, so flexion is +X on both hands. "cone" bones (the shoulder) are limited
by the swing of their axis from rest and the twist about it, not by Euler angles (an arm
raised overhead has no meaningful Euler decomposition).

Sources: AAOS, "Joint Motion: Method of Measuring and Recording" (1965); Norkin & White,
"Measurement of Joint Motion: A Guide to Goniometry" (5th ed., 2016).

Thumb: `thumb_01` is the first metacarpal (its head on the carpometacarpal joint, MakeHuman's
`finger-1-1`), `thumb_02` the proximal phalanx (MCP), `thumb_03` the distal (IP).
"""

import json
import math
import os

DATA = os.path.join(os.path.dirname(__file__), "..", "..", "data", "joint_limits.json")
FINGERS = ("index", "middle", "ring", "pinky")
PREFIXES = ("rider.", "c0.", "c1.", "c2.", "c3.")

_table = {}


def table():
    """The parsed `joint_limits.json`."""
    if not _table:
        with open(DATA, encoding="utf-8") as f:
            _table.update(json.load(f))
    return _table


def key_of(bone):
    """(table key, side) of a skeleton H bone name (prefixes like `rider.` dropped), or
    (None, None) for bones the table does not cover (sockets, the root, horse bones)."""
    for p in PREFIXES:
        if bone.startswith(p):
            bone = bone[len(p):]
    side = None
    if bone.endswith("_l") or bone.endswith("_r"):
        bone, side = bone[:-2], bone[-1]
    if bone.startswith("skirt_"):
        return "skirt_" + bone.rsplit("_", 1)[1], None
    for f in FINGERS:
        if bone.startswith(f + "_0"):
            return "finger_" + bone[len(f) + 1:], side
    return (bone if bone in table()["bones"] else None), side


def limit(bone):
    """The limit of `bone` with the right side mirrored: {"kind", "x", "y", "z"} or
    {"kind": "cone", "swing", "twist"}, or None."""
    key, side = key_of(bone)
    if key is None:
        return None
    lim = dict(table()["bones"][key])
    if side == "r":
        for ax in ("y", "z", "twist"):
            if ax in lim:
                lo, hi = lim[ax]
                lim[ax] = [-hi, -lo]
    return lim


# ----------------------------------------------------------------------------- rotations

def quat_matrix(q):
    """3x3 (rows) of a unit quaternion (w, x, y, z)."""
    w, x, y, z = q
    n = math.sqrt(w * w + x * x + y * y + z * z) or 1.0
    w, x, y, z = w / n, x / n, y / n, z / n
    return ((1 - 2 * (y * y + z * z), 2 * (x * y - w * z), 2 * (x * z + w * y)),
            (2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)),
            (2 * (x * z - w * y), 2 * (y * z + w * x), 1 - 2 * (x * x + y * y)))


def euler_xyz(q):
    """Blender 'XYZ' Euler angles (degrees) of quaternion `q`: R = Rz(c) Ry(b) Rx(a)."""
    m = quat_matrix(q)
    b = math.asin(max(-1.0, min(1.0, -m[2][0])))
    a = math.atan2(m[2][1], m[2][2])
    c = math.atan2(m[1][0], m[0][0])
    return tuple(math.degrees(v) for v in (a, b, c))


def swing_twist(q):
    """(swing of the local Y axis from rest, twist about it), degrees."""
    w, x, y, z = q
    n = math.sqrt(w * w + x * x + y * y + z * z) or 1.0
    w, y = w / n, y / n
    tw = 2 * math.atan2(y, w)
    if tw > math.pi:
        tw -= 2 * math.pi
    elif tw < -math.pi:
        tw += 2 * math.pi
    sw = 2 * math.acos(max(-1.0, min(1.0, math.sqrt(w * w + y * y))))
    return math.degrees(sw), math.degrees(tw)


def excess(bone, q):
    """How far (degrees, 0 = inside) the local rotation `q` of `bone` lies outside its
    limits: the worst axis. None for bones without limits."""
    lim = limit(bone)
    if lim is None or lim["kind"] == "free":
        return None
    if lim["kind"] == "cone":
        sw, tw = swing_twist(q)
        lo, hi = lim["twist"]
        return max(0.0, sw - lim["swing"], lo - tw, tw - hi)
    a, b, c = euler_xyz(q)

    def wrap(v):
        return (v + 180.0) % 360.0 - 180.0

    # The same rotation has two Euler triples, (a, b, c) and (a+180, 180-b, c+180): judge
    # the one nearer the range.
    best = None
    for e in ((a, b, c), (wrap(a + 180), wrap(180 - b), wrap(c + 180))):
        out = 0.0
        for v, ax in zip(e, "xyz"):
            lo, hi = lim[ax]
            out = max(out, lo - v, v - hi)
        best = out if best is None else min(best, out)
    return best


def clamp_x(bone, deg):
    """A flexion angle (local X, degrees) clamped to `bone`'s range."""
    lo, hi = limit(bone)["x"]
    return max(lo, min(hi, deg))


# ----------------------------------------------------------------------------- vectors

def _sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def _add(a, b):
    return tuple(x + y for x, y in zip(a, b))


def _mul(a, s):
    return tuple(x * s for x in a)


def _dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _rot(v, axis, ang):
    """`v` turned by `ang` radians about unit `axis` (Rodrigues)."""
    c, s = math.cos(ang), math.sin(ang)
    return _add(_add(_mul(v, c), _mul(_cross(axis, v), s)), _mul(axis, _dot(axis, v) * (1 - c)))


def line_distance(p, o, a):
    """Distance of point `p` from the line through `o` along unit `a`."""
    w = _sub(p, o)
    return math.sqrt(max(0.0, _dot(w, w) - _dot(w, a) ** 2))


def pad_gap(head, y, z, length, pad, o, a, r, samples=9):
    """The smallest gap (m; negative = inside) between the palmar pad of a phalanx (the
    segment from `head` along unit `y` of `length`, its skin `pad` metres along unit `z`)
    and the cylinder of radius `r` round the line (`o`, unit `a`)."""
    best = None
    for i in range(samples):
        t = length * (0.15 + 0.8 * i / (samples - 1))
        p = _add(_add(head, _mul(y, t)), _mul(z, pad))
        g = line_distance(p, o, a) - r
        best = g if best is None or g < best else best
    return best


def wrap_chain(chain, o, a, r, step=1.0):
    """Curls one finger (or the thumb) round a cylinder: `chain` = [(head, x, y, z, length,
    pad, (lo, hi))] per bone in rest (armature space: head, unit local axes, the palmar pad
    thickness, the flexion range in degrees), proximal first; the cylinder is the line
    (`o`, unit `a`) with radius `r`. Each joint flexes about its local X, in turn, by the
    least angle in range that brings its pad onto the surface (the angle where the gap
    crosses zero, refined by bisection); a pad that cannot reach stops at the range's top.
    Returns ([degrees per bone], [gap per bone in m at the solution])."""
    heads = [c[0] for c in chain]
    frames = [(c[1], c[2], c[3]) for c in chain]
    angles, gaps = [], []
    for i, c in enumerate(chain):
        length, pad, (lo, hi) = c[4], c[5], c[6]
        h = heads[i]
        x, y, z = frames[i]
        if len(c) > 7 and c[7] is not None:  # a joint turning about its own axis (the CMC)
            x = c[7]

        def gap(deg):
            t = math.radians(deg)
            return pad_gap(h, _rot(y, x, t), _rot(z, x, t), length, pad, o, a, r)

        deg = lo
        g0 = gap(lo)
        if g0 > 0:
            d = lo
            found = None
            while d < hi:
                d2 = min(hi, d + step)
                if gap(d2) <= 0:
                    found = (d, d2)
                    break
                d = d2
            if found is None:
                deg = hi
            else:
                a0, a1 = found
                for _ in range(20):
                    m = 0.5 * (a0 + a1)
                    if gap(m) > 0:
                        a0 = m
                    else:
                        a1 = m
                deg = a1
        angles.append(deg)
        gaps.append(gap(deg))
        # Turn the rest of the chain with this joint.
        t = math.radians(deg)
        for j in range(i + 1, len(chain)):
            heads[j] = _add(h, _rot(_sub(heads[j], h), x, t))
            frames[j] = tuple(_rot(v, x, t) for v in frames[j])
    return angles, gaps
