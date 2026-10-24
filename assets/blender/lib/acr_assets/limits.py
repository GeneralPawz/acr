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


def hinge_angles(q):
    """(flexion about local X, twist about local Y, sideways about local Z), degrees, of a
    hinge with its own twist (the elbow and the radius rolling on the ulna): q = swing *
    twist, the twist about the bone's own axis applied first, the swing (an axis in the
    local XZ plane) split into its X and Z parts. Euler XYZ would read a bent and pronated
    forearm (Rx then the twist) as a large Z turn."""
    w, x, y, z = q
    n = math.sqrt(w * w + x * x + y * y + z * z) or 1.0
    w, x, y, z = w / n, x / n, y / n, z / n
    if w < 0:
        w, x, y, z = -w, -x, -y, -z
    t = math.sqrt(w * w + y * y)
    if t < 1e-9:  # a half turn of swing: the twist is undefined
        tw, sw = 0.0, (0.0, x, 0.0, z)
    else:
        tq = (w / t, 0.0, y / t, 0.0)
        tw = 2 * math.atan2(tq[2], tq[0])
        # swing = q * conj(twist)
        cw, cy = tq[0], -tq[2]
        sw = (w * cw - y * cy, x * cw - z * cy, y * cw + w * cy, z * cw + x * cy)
    sw_w = max(-1.0, min(1.0, sw[0]))
    ang = 2 * math.acos(abs(sw_w))
    s = math.sqrt(sw[1] ** 2 + sw[3] ** 2)
    sign = 1.0 if sw_w >= 0 else -1.0
    ax, az = (sign * sw[1] / s, sign * sw[3] / s) if s > 1e-9 else (0.0, 0.0)
    return math.degrees(ang * ax), math.degrees(tw), math.degrees(ang * az)


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
    if lim["kind"] == "hinge":
        a, b, c = hinge_angles(q)
        out = 0.0
        for v, ax in ((a, "x"), (b, "y"), (c, "z")):
            lo, hi = lim[ax]
            out = max(out, lo - v, v - hi)
        return out
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


def cloud_gap(pts, o, a, r):
    """The smallest gap (m; negative = inside) of points `pts` from the cylinder of radius
    `r` round the line (`o`, unit `a`)."""
    return min(line_distance(p, o, a) for p in pts) - r


def _turn(h, x, t, heads, frames, clouds, start):
    """Turns bones `start`.. of a chain (heads, frames, point clouds) about axis `x` through
    `h` by `t` radians, in place."""
    for j in range(start, len(heads)):
        heads[j] = _add(h, _rot(_sub(heads[j], h), x, t))
        frames[j] = tuple(_rot(v, x, t) for v in frames[j])
        if clouds[j] is not None:
            clouds[j] = [_add(h, _rot(_sub(p, h), x, t)) for p in clouds[j]]


def wrap_chain(chain, o, a, r, step=1.0, clearance=0.0, out=None):
    """Curls one finger (or the thumb) round a cylinder: `chain` = [(head, x, y, z, length,
    pad, (lo, hi)[, axis])] per bone in rest (armature space: head, unit local axes, the
    palmar pad, the flexion range in degrees), proximal first; the cylinder is the line
    (`o`, unit `a`) with radius `r`. `pad` is the pad's thickness along local +Z (m), or the
    bone's skin as a list of rest points (armature space), whose closest point then counts.
    Each joint flexes about its local X (or `axis`), in turn, from rest (or the range's
    bottom) by the least angle in range that brings its skin `clearance` m off the surface
    (the angle where the gap crosses it, refined by bisection); skin already inside opens
    the joint until it clears, down to the range's bottom; a pad that cannot reach stops at
    the range's top. Returns ([degrees per bone], [gap per bone in m at the solution]);
    a list `out` receives the posed point clouds."""
    heads = [c[0] for c in chain]
    frames = [(c[1], c[2], c[3]) for c in chain]
    clouds = [list(c[5]) if isinstance(c[5], (list, tuple)) else None for c in chain]
    angles, gaps = [], []
    for i, c in enumerate(chain):
        length, pad, (lo, hi) = c[4], c[5], c[6]
        h = heads[i]
        x, y, z = frames[i]
        if len(c) > 7 and c[7] is not None:  # a joint turning about its own axis (the CMC)
            x = c[7]
        cloud = clouds[i]

        def gap(deg):
            t = math.radians(deg)
            if cloud is not None:
                return min(line_distance(_add(h, _rot(_sub(p, h), x, t)), o, a) for p in cloud) - r - clearance
            return pad_gap(h, _rot(y, x, t), _rot(z, x, t), length, pad, o, a, r) - clearance

        def bisect(out, inside):
            for _ in range(20):
                m = 0.5 * (out + inside)
                if gap(m) > 0:
                    out = m
                else:
                    inside = m
            return out, inside

        start = min(max(0.0, lo), hi)
        deg = start
        if gap(start) > 0:
            d, found = start, None
            while d < hi:
                d2 = min(hi, d + step)
                if gap(d2) <= 0:
                    found = (d, d2)
                    break
                d = d2
            deg = hi if found is None else bisect(*found)[1]
        else:
            d, found = start, None
            while d > lo:
                d2 = max(lo, d - step)
                if gap(d2) > 0:
                    found = (d2, d)
                    break
                d = d2
            deg = lo if found is None else bisect(*found)[0]
        angles.append(deg)
        gaps.append(gap(deg) + clearance)
        _turn(h, x, math.radians(deg), heads, frames, clouds, i)
    if out is not None:
        out.extend(clouds)
    return angles, gaps


def _centroid(pts):
    n = float(len(pts))
    return tuple(sum(p[k] for p in pts) / n for k in range(3))


def oppose_thumb(chain, o, a, r, target, cmc, step=10.0, clearance=0.0):
    """The thumb round the far side of a cylinder: `chain` as `wrap_chain` (metacarpal,
    proximal, distal phalanx; skin as point clouds), `cmc` = ((lo, hi) about local X
    (flexion), Y (opposition: the axial turn), Z (abduction)) of the carpometacarpal joint.
    Searches the CMC angles on a grid (then a finer one round the best), wraps MCP and IP
    round the cylinder for each (`wrap_chain`) and keeps the pose whose phalanges touch the
    cylinder without entering it and whose tip lies nearest `target` (a
    point, e.g. over the index finger's middle phalanx). Returns ((x, y, z) CMC degrees,
    [MCP, IP] degrees, [gap per bone in m], tip distance in m)."""
    h0 = chain[0][0]
    x0, y0, z0 = chain[0][1], chain[0][2], chain[0][3]

    def at(ex, ey, ez):
        heads = [c[0] for c in chain]
        frames = [(c[1], c[2], c[3]) for c in chain]
        clouds = [list(c[5]) if isinstance(c[5], (list, tuple)) else None for c in chain]
        for ax, deg in ((x0, ex), (y0, ey), (z0, ez)):  # local XYZ Euler: X first, about the rest axes
            _turn(h0, ax, math.radians(deg), heads, frames, clouds, 0)
        g0 = (cloud_gap(clouds[0], o, a, r) if clouds[0] is not None else
              pad_gap(heads[0], frames[0][1], frames[0][2], chain[0][4], chain[0][5], o, a, r))
        rest = [(heads[k], *frames[k], chain[k][4], clouds[k] if clouds[k] is not None else chain[k][5],
                 chain[k][6]) for k in (1, 2)]
        cc = []
        ang, gaps = wrap_chain(rest, o, a, r, step=2.0, clearance=clearance, out=cc)
        tip = _centroid(cc[1]) if cc[1] is not None else heads[2]
        dist = math.sqrt(sum(v * v for v in _sub(tip, target)))
        deep = min([g0] + gaps)
        score = dist + 3 * sum(max(g, 0.0) for g in gaps) + (1.0 + 100 * -deep if deep < -0.001 else 0.0)
        return score, ang, [g0] + gaps, dist

    def grid(rx, ry, rz, st):
        best = None
        def steps(lo, hi):
            n = max(1, int(round((hi - lo) / st)))
            return [lo + (hi - lo) * k / n for k in range(n + 1)]
        for ex in steps(*rx):
            for ey in steps(*ry):
                for ez in steps(*rz):
                    s = at(ex, ey, ez)
                    if best is None or s[0] < best[0][0]:
                        best = (s, (ex, ey, ez))
        return best

    (rx, ry, rz) = cmc
    best = grid(rx, ry, rz, step)
    e = best[1]
    clip = [(max(lo, v - step / 2), min(hi, v + step / 2)) for v, (lo, hi) in zip(e, cmc)]
    fine = grid(*clip, step / 4)
    if fine[0][0] < best[0][0]:
        best = fine
    (score, ang, gaps, dist), e = best
    return tuple(round(v, 2) for v in e), ang, gaps, dist
