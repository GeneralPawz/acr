"""Crawl clips of the wounded (#262, pure Python, no `bpy`; unit-tested): CRAWL2, CRAWL1,
CRAWLARM, WRITHE as pose specs for `armature.add_clip` (see `poses.py` for the spec format).

The poses are written as bone directions in the body frame (skeleton H standing: +Z
towards the head, -Y the belly, +X the left side) and turned into the relative Euler specs
by `aim_chain`; the root then lays the body down face first (prone: belly on the ground,
head towards the facing direction -Y) or on its back (WRITHE). Every clip loops: its last
frame equals its first. Keyframed stand-ins until recorded drills replace them (#167).
"""

import math

from . import rig

# Frames per loop (10 game ticks per second).
FRAMES = {"CRAWL2": 24, "CRAWL1": 24, "CRAWLARM": 28, "WRITHE": 30}
CLIPS = tuple(FRAMES)

# Prone: belly on the ground, the spine axis 12 cm up; supine (WRITHE): back down.
PRONE_ROOT, PRONE_LOC = (90.0, 0.0, 0.0), (0.0, 0.40, 0.12)
SUPINE_ROOT, SUPINE_LOC = (-90.0, 0.0, 180.0), (0.0, 0.40, 0.12)

# Coat tails flat on the legs (as in `poses.lie_prone`).
SKIRT_FLAT = {"skirt_fl_01": (-80, 0, 0), "skirt_fr_01": (-80, 0, 0), "skirt_bl_01": (-70, 0, 0), "skirt_br_01": (-70, 0, 0)}


# ----------------------------------------------------------------------------- 3x3 maths

def _norm(v):
    n = math.sqrt(sum(x * x for x in v))
    return tuple(x / n for x in v)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def mat_mul(a, b):
    return tuple(tuple(sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)) for i in range(3))


def mat_vec(m, v):
    return tuple(sum(m[i][k] * v[k] for k in range(3)) for i in range(3))


def transpose(m):
    return tuple(tuple(m[j][i] for j in range(3)) for i in range(3))


IDENTITY = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))


def align(u, v):
    """The shortest rotation taking direction u onto direction v."""
    u, v = _norm(u), _norm(v)
    c = _dot(u, v)
    if c > 1 - 1e-12:
        return IDENTITY
    if c < -1 + 1e-12:
        a = _norm(_cross(u, (1.0, 0.0, 0.0) if abs(u[0]) < 0.9 else (0.0, 1.0, 0.0)))
        return tuple(tuple(2 * a[i] * a[j] - (1.0 if i == j else 0.0) for j in range(3)) for i in range(3))
    k = _cross(u, v)
    s = math.sqrt(_dot(k, k))
    k = tuple(x / s for x in k)
    t = 1 - c
    x, y, z = k
    return ((t * x * x + c, t * x * y - s * z, t * x * z + s * y),
            (t * x * y + s * z, t * y * y + c, t * y * z - s * x),
            (t * x * z - s * y, t * y * z + s * x, t * z * z + c))


def euler_matrix(r):
    """`armature.euler`: Rz @ Ry @ Rx of (rx, ry, rz) degrees."""
    a, b, c = (math.radians(x) for x in r)
    rx = ((1, 0, 0), (0, math.cos(a), -math.sin(a)), (0, math.sin(a), math.cos(a)))
    ry = ((math.cos(b), 0, math.sin(b)), (0, 1, 0), (-math.sin(b), 0, math.cos(b)))
    rz = ((math.cos(c), -math.sin(c), 0), (math.sin(c), math.cos(c), 0), (0, 0, 1))
    return mat_mul(rz, mat_mul(ry, rx))


def to_euler(m):
    """(rx, ry, rz) degrees with euler_matrix(r) == m."""
    b = math.asin(max(-1.0, min(1.0, -m[2][0])))
    a = math.atan2(m[2][1], m[2][2])
    c = math.atan2(m[1][0], m[0][0])
    return tuple(round(math.degrees(x), 2) for x in (a, b, c))


# ----------------------------------------------------------------------------- chains

_REST = {b.name: b for b in rig.skeleton_h()}
_PARENT = {b.name: b.parent for b in rig.skeleton_h()}


def rest_dir(bone):
    b = _REST[bone]
    return _norm(tuple(t - h for h, t in zip(b.head, b.tail)))


def delta(spec, bone):
    """The bone's rotation from rest in the body frame (the root left out): the product of
    the spec rotations from the hips down to it."""
    chain = []
    while bone and bone != "root":
        chain.append(bone)
        bone = _PARENT[bone]
    m = IDENTITY
    for b in reversed(chain):
        if b in spec:
            m = mat_mul(m, euler_matrix(spec[b]))
    return m


def aim_chain(spec, bones, dirs):
    """Adds to `spec` the rotations that point each bone of the chain `bones` (parent
    first) along its direction in `dirs` (body frame), each one turning as little as
    possible from where its parent carries it (no twist)."""
    for bone, d in zip(bones, dirs):
        parent = delta(spec, _PARENT[bone])
        carried = mat_mul(parent, IDENTITY)
        now = mat_vec(carried, rest_dir(bone))
        want = mat_mul(align(now, d), carried)
        spec[bone] = to_euler(mat_mul(transpose(parent), want))
    return spec


def _arm(spec, side, upper, fore, hand=None):
    s = "_l" if side > 0 else "_r"
    m = lambda v: (v[0] * side, v[1], v[2])  # noqa: E731
    dirs = [m(upper), m(fore), m(hand or fore)]
    return aim_chain(spec, ["upperarm" + s, "forearm" + s, "hand" + s], dirs)


def _leg(spec, side, thigh, shin, foot):
    s = "_l" if side > 0 else "_r"
    m = lambda v: (v[0] * side, v[1], v[2])  # noqa: E731
    return aim_chain(spec, ["thigh" + s, "shin" + s, "foot" + s], [m(thigh), m(shin), m(foot)])


# Arm phases of the elbow crawl (body frame, left side; the right mirrors x). Prone, so -Y
# is the ground: an arm on the ground has y < 0, a lifted one y >= 0.
REACH = ((0.45, -0.30, 0.84), (-0.30, -0.10, 0.95), (-0.25, -0.30, 0.92))  # elbow by the head, forearm ahead
PULL = ((0.80, -0.30, -0.52), (-0.05, -0.20, 0.98), (0.0, -0.45, 0.89))  # elbow back by the ribs, hand gripping
LIFT = ((0.85, 0.15, 0.20), (-0.20, 0.25, 0.95), (-0.2, 0.0, 0.98))  # elbow swung forward over the ground
LIMP = ((0.25, -0.35, -0.90), (0.05, -0.30, -0.95), (0.0, -0.40, -0.92))  # along the side, palm down
# Legs: drag (straight, toes pointed into the ground), frog (thigh out, knee bent for the
# push), push (pressed straight).
DRAG = ((0.08, -0.04, -1.0), (0.02, -0.02, -1.0), (0.0, -0.45, -0.89))
DRAG_OUT = ((0.20, -0.04, -0.98), (0.10, -0.02, -0.99), (0.0, -0.45, -0.89))
FROG = ((0.72, -0.10, -0.69), (-0.10, -0.15, -0.98), (0.10, -0.60, -0.79))
PUSH = ((0.18, -0.08, -0.98), (0.05, -0.05, -1.0), (0.0, -0.75, -0.66))


def _prone(roll=0.0, yaw=0.0, lift=0.0, spine_y=0.0):
    """Trunk of the crawl: prone root, head raised to look ahead, the chest twisting with
    the pulling arm (`spine_y`), rolled onto one side by `roll` degrees."""
    sp = {"root": (PRONE_ROOT[0], roll, yaw), "root_loc": (PRONE_LOC[0], PRONE_LOC[1], PRONE_LOC[2] + lift),
          "spine": (0, spine_y * 0.5, 0), "spine1": (-4, spine_y * 0.5, 0), "spine2": (-6, 0, 0),
          "neck": (28, 0, 0), "head": (18, 0, -spine_y)}
    sp.update(SKIRT_FLAT)
    return sp


def crawl2_keys():
    """Elbow crawl: elbows pull alternately, legs drag. (fraction, spec) keys."""
    def key(lead, trail, lead_side, twist):
        s = _prone(roll=4 * lead_side, spine_y=twist * lead_side)
        _arm(s, lead_side, *lead)
        _arm(s, -lead_side, *trail)
        _leg(s, lead_side, *DRAG)
        _leg(s, -lead_side, *DRAG_OUT)
        return s

    a = key(REACH, PULL, 1, 6)      # left reaches, right has pulled
    b = key(PULL, LIFT, 1, -4)      # left pulls, right swings forward
    c = key(REACH, PULL, -1, 6)     # mirrored
    d = key(PULL, LIFT, -1, -4)
    return [(0.0, a), (0.25, b), (0.5, c), (0.75, d), (1.0, a)]


def crawl1_keys():
    """One leg (the left) pushes, both arms pull together, the right leg drags."""
    def key(arms, leg, lift):
        s = _prone(lift=lift)
        _arm(s, 1, *arms)
        _arm(s, -1, *arms)
        _leg(s, 1, *leg)
        _leg(s, -1, *DRAG)
        return s

    reach = key(REACH, FROG, 0.0)
    pull = key(PULL, PUSH, 0.015)
    recover = key(LIFT, FROG, 0.01)
    return [(0.0, reach), (0.45, pull), (0.75, recover), (1.0, reach)]


def crawlarm_keys():
    """One-armed drag: the left arm reaches and hauls, the right arm limp, legs dragging,
    the body rolling towards the working arm."""
    def key(arm, roll, twist):
        s = _prone(roll=roll, spine_y=twist)
        _arm(s, 1, *arm)
        _arm(s, -1, *LIMP)
        _leg(s, 1, *DRAG)
        _leg(s, -1, *DRAG_OUT)
        return s

    reach = key(REACH, 10, 8)
    pull = key(PULL, 3, -6)
    lift = key(LIFT, 6, 0)
    return [(0.0, reach), (0.55, pull), (0.8, lift), (1.0, reach)]


# WRITHE (on the back, body frame: +Y is now the ground): knees drawn up in turn, hands
# clutching the belly, rolling and arching, the head tossing.
CLUTCH = ((0.30, -0.45, -0.84), (-0.93, -0.10, -0.35), (-0.80, -0.30, -0.50))
KNEE_UP = ((0.10, -0.70, -0.70), (0.0, 0.70, -0.70), (0.0, -0.10, -0.99))
LEG_FLAT = ((0.10, 0.02, -1.0), (0.03, 0.02, -1.0), (0.0, -0.60, -0.80))


def _supine(roll, arch, head):
    return {"root": (SUPINE_ROOT[0], roll, SUPINE_ROOT[2]), "root_loc": SUPINE_LOC,
            "spine": (arch, 0, 0), "spine1": (arch * 0.6, 0, 0), "neck": (-arch * 0.5 - 10, 0, 0), "head": (-10, 0, head)}


def writhe_keys():
    def key(roll, arch, head, left_up, right_up):
        s = _supine(roll, arch, head)
        _arm(s, 1, *CLUTCH)
        _arm(s, -1, *CLUTCH)
        _leg(s, 1, *(KNEE_UP if left_up else LEG_FLAT))
        _leg(s, -1, *(KNEE_UP if right_up else LEG_FLAT))
        return s

    a = key(-14, -6, 25, True, False)
    b = key(0, 8, 0, True, True)
    c = key(14, -6, -25, False, True)
    d = key(4, 10, 10, False, False)
    return [(0.0, a), (0.3, b), (0.55, c), (0.8, d), (1.0, a)]


def clips():
    """{clip: [(frame, spec)]}, each looping (last frame = first, `FRAMES` + 1 frames)."""
    keys = {"CRAWL2": crawl2_keys(), "CRAWL1": crawl1_keys(), "CRAWLARM": crawlarm_keys(), "WRITHE": writhe_keys()}
    return {name: rig.clip_keys(FRAMES[name] + 1, ks) for name, ks in keys.items()}
