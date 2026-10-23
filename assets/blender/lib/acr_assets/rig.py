"""Rigs as data (pure Python, no `bpy`; unit-tested): skeleton H (humans) and rig R (horse),
plus the mapping of `.MD` animations to clip frames.

A rig is a list of `Bone(name, parent, head, tail, deform)` in armature space (metres,
Blender axes: +X = the character's left, -Y = forward/south, +Z up), rest pose. Sockets
(`socket.<name>`) are non-deforming bones where gear, missiles, the rider or effects attach;
the glTF exporter writes every bone as a node, so the renderer finds them by name.
`acr_assets.armature` turns a table into a Blender armature. See docs/assets-humans.md.
"""

import math
from collections import namedtuple

Bone = namedtuple("Bone", "name parent head tail deform")

# Rest pose of skeleton H: relaxed A-pose, arms 35 degrees out.
A_POSE = math.radians(35.0)


def _add(a, b):
    return tuple(x + y for x, y in zip(a, b))


def _mul(a, s):
    return tuple(x * s for x in a)


def _norm(a):
    n = math.sqrt(sum(x * x for x in a))
    return tuple(x / n for x in a)


def arm_points(side):
    """Shoulder, elbow, wrist, hand end and the arm direction of skeleton H for side +1
    (left, +X) or -1 (right), rest pose."""
    sh = (0.17 * side, 0.0, 1.43)
    d = (math.sin(A_POSE) * side, 0.0, -math.cos(A_POSE))
    elbow = _add(sh, _mul(d, 0.29))
    wrist = _add(elbow, _mul(d, 0.25))
    hand_end = _add(wrist, _mul(d, 0.09))
    return sh, elbow, wrist, hand_end, d


# Coat-skirt columns of skeleton H: (suffix, x, y) where the two-segment chains hang from
# the hips (front left/right, back left/right; the coat is open at the front).
SKIRT_COLUMNS = (("fl", 0.125, -0.115), ("fr", -0.125, -0.115), ("bl", 0.12, 0.125), ("br", -0.12, 0.125))
SKIRT_TOP, SKIRT_KNEE, SKIRT_BOTTOM = 1.0, 0.78, 0.55

# Where stuck missiles (arrows, spears) attach: (name, parent bone, surface point, outward
# direction). The renderer may use the nearest one to the hit point (#183).
HIT_SOCKETS = (
    ("head", "head", (0.0, -0.09, 1.66), (0, -1, 0)),
    ("chest", "spine2", (0.0, -0.11, 1.33), (0, -1, 0)),
    ("belly", "spine", (0.0, -0.11, 1.12), (0, -1, 0)),
    ("back", "spine2", (0.0, 0.11, 1.30), (0, 1, 0)),
    ("arm_l", "upperarm_l", None, (1, 0, 0)),
    ("arm_r", "upperarm_r", None, (-1, 0, 0)),
    ("thigh_l", "thigh_l", (0.10, -0.08, 0.72), (0, -1, 0)),
    ("thigh_r", "thigh_r", (-0.10, -0.08, 0.72), (0, -1, 0)),
)

# Bones of skeleton H that deform the body skin (fingers ride on the hand: mitten hands at
# this scale).
BODY_BONES = (
    "hips", "spine", "spine1", "spine2", "neck", "head",
    "clavicle_l", "upperarm_l", "forearm_l", "hand_l", "thigh_l", "shin_l", "foot_l", "toe_l",
    "clavicle_r", "upperarm_r", "forearm_r", "hand_r", "thigh_r", "shin_r", "foot_r", "toe_r",
)

# Body parts for the cuts of #183 (cannonballs tear limbs off): part id -> (name, bones).
# A cut runs between a part and its parent part; JOINTS lists them as (joint name, parent
# part, child part, the child's first bone).
PARTS = (
    (0, "torso", ("hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r")),
    (1, "head", ("neck", "head")),
    (2, "upperarm_l", ("upperarm_l",)),
    (3, "forearm_l", ("forearm_l", "hand_l")),
    (4, "upperarm_r", ("upperarm_r",)),
    (5, "forearm_r", ("forearm_r", "hand_r")),
    (6, "thigh_l", ("thigh_l",)),
    (7, "shin_l", ("shin_l", "foot_l", "toe_l")),
    (8, "thigh_r", ("thigh_r",)),
    (9, "shin_r", ("shin_r", "foot_r", "toe_r")),
)
JOINTS = (
    ("neck", 0, 1, "neck"),
    ("shoulder_l", 0, 2, "upperarm_l"),
    ("elbow_l", 2, 3, "forearm_l"),
    ("shoulder_r", 0, 4, "upperarm_r"),
    ("elbow_r", 4, 5, "forearm_r"),
    ("hip_l", 0, 6, "thigh_l"),
    ("knee_l", 6, 7, "shin_l"),
    ("hip_r", 0, 8, "thigh_r"),
    ("knee_r", 8, 9, "shin_r"),
)


def part_of_bone(bone):
    """Part id of a deforming bone of skeleton H (fingers belong to their hand's part,
    skirt bones to the torso)."""
    for pid, _name, bones in PARTS:
        if bone in bones:
            return pid
    for side in ("_l", "_r"):
        if bone.endswith(side) and any(bone.startswith(f) for f in ("thumb", "index", "middle", "ring", "pinky")):
            return part_of_bone("hand" + side)
    return 0


def skeleton_h(prefix="", offset=(0.0, 0.0, 0.0)):
    """Skeleton H: 75 bones (60 deforming). `prefix` is put before every name and `offset`
    added to every point (a rider's bones in a combined rig: `rider.hips` above the
    saddle)."""
    b = []

    def add(name, parent, head, tail, deform=True):
        b.append(Bone(prefix + name, prefix + parent if parent else None, _add(head, offset), _add(tail, offset), deform))

    add("root", None, (0, 0, 0), (0, 0, 0.2), False)
    add("hips", "root", (0, 0, 0.95), (0, 0, 1.05))
    add("spine", "hips", (0, 0, 1.05), (0, 0, 1.20))
    add("spine1", "spine", (0, 0, 1.20), (0, 0, 1.33))
    add("spine2", "spine1", (0, 0, 1.33), (0, 0, 1.45))
    add("neck", "spine2", (0, 0, 1.45), (0, -0.01, 1.55))
    add("head", "neck", (0, -0.01, 1.55), (0, -0.01, 1.75))
    add("socket.hat", "head", (0, -0.01, 1.70), (0, -0.01, 1.82), False)
    add("socket.back", "spine2", (0, 0.13, 1.30), (0, 0.13, 1.42), False)
    add("socket.hip_l", "hips", (0.16, 0.02, 0.92), (0.16, 0.02, 0.82), False)
    add("socket.hip_r", "hips", (-0.14, 0.09, 0.95), (-0.14, 0.09, 0.85), False)
    for side, sfx in ((1, "_l"), (-1, "_r")):
        sh, elbow, wrist, hand_end, d = arm_points(side)
        add("clavicle" + sfx, "spine2", (0.03 * side, -0.01, 1.41), sh)
        add("upperarm" + sfx, "clavicle" + sfx, sh, elbow)
        add("forearm" + sfx, "upperarm" + sfx, elbow, wrist)
        add("hand" + sfx, "forearm" + sfx, wrist, _add(wrist, _mul(d, 0.08)))
        # Fingers: three joints each, fanned along -Y (palm faces the thigh).
        for i, f in enumerate(("index", "middle", "ring", "pinky")):
            p = _add(_add(wrist, _mul(d, 0.08)), (0, -0.03 + 0.02 * i, 0))
            ln = (0.030, 0.034, 0.031, 0.025)[i]
            parent = "hand" + sfx
            for j in range(3):
                q = _add(p, _mul(d, ln * (1.0 - 0.2 * j)))
                add(f"{f}_0{j + 1}{sfx}", parent, p, q)
                parent, p = f"{f}_0{j + 1}{sfx}", q
        p = _add(_add(wrist, _mul(d, 0.02)), (0, -0.035, 0))
        parent = "hand" + sfx
        t = _norm(_add(_mul(d, 0.6), (0, -0.4, 0)))
        for j in range(3):
            q = _add(p, _mul(t, 0.03))
            add(f"thumb_0{j + 1}{sfx}", parent, p, q)
            parent, p = f"thumb_0{j + 1}{sfx}", q
        g = _add(_add(wrist, _mul(d, 0.06)), (0, -0.01, 0))
        add("socket.hand" + sfx, "hand" + sfx, g, _add(g, (0, -0.10, 0)), False)
        hx = 0.09 * side
        add("thigh" + sfx, "hips", (hx, 0, 0.93), (hx * 1.05, 0.0, 0.50))
        add("shin" + sfx, "thigh" + sfx, (hx * 1.05, 0.0, 0.50), (hx * 1.05, 0.02, 0.085))
        add("foot" + sfx, "shin" + sfx, (hx * 1.05, 0.02, 0.085), (hx * 1.08, -0.11, 0.025))
        add("toe" + sfx, "foot" + sfx, (hx * 1.08, -0.11, 0.025), (hx * 1.1, -0.18, 0.02))
    # Coat skirt: four two-segment chains (#168: the skirt has its own bones).
    for s, x, y in SKIRT_COLUMNS:
        add(f"skirt_{s}_01", "hips", (x, y, SKIRT_TOP), (x * 1.08, y * 1.1, SKIRT_KNEE))
        add(f"skirt_{s}_02", f"skirt_{s}_01", (x * 1.08, y * 1.1, SKIRT_KNEE), (x * 1.16, y * 1.2, SKIRT_BOTTOM))
    # Hit sockets for stuck missiles (#183).
    for name, parent, point, out in HIT_SOCKETS:
        if point is None:  # mid upper arm, outside
            side = 1 if parent.endswith("_l") else -1
            sh, elbow, *_ = arm_points(side)
            point = _add(_mul(_add(sh, elbow), 0.5), (0.04 * side, 0, 0))
        add(f"socket.hit_{name}", parent, point, _add(point, _mul(out, 0.1)), False)
    return b


# ----------------------------------------------------------------------------- horse rig R

# Proportions of the warhorse (metres): withers 1.55 m, body 1.6 m from chest to buttock.
HORSE_LEGS = {
    # leg: (parent, [(bone, head, tail)]) for the left side; the right mirrors x.
    "front": ("chest", [
        ("scapula", (0.13, -0.62, 1.42), (0.15, -0.70, 1.08)),
        ("humerus", (0.15, -0.70, 1.08), (0.14, -0.62, 0.86)),
        ("radius", (0.14, -0.62, 0.86), (0.13, -0.64, 0.48)),
        ("cannon", (0.13, -0.64, 0.48), (0.13, -0.64, 0.18)),
        ("pastern", (0.13, -0.64, 0.18), (0.13, -0.69, 0.07)),
        ("hoof", (0.13, -0.69, 0.07), (0.13, -0.73, 0.0)),
    ]),
    "back": ("pelvis", [
        ("femur", (0.15, 0.62, 1.25), (0.16, 0.52, 0.86)),
        ("tibia", (0.16, 0.52, 0.86), (0.14, 0.70, 0.52)),
        ("hock", (0.14, 0.70, 0.52), (0.13, 0.68, 0.18)),
        ("pastern", (0.13, 0.68, 0.18), (0.13, 0.64, 0.07)),
        ("hoof", (0.13, 0.64, 0.07), (0.13, 0.60, 0.0)),
    ]),
}


def rig_r():
    """Horse rig R: root, pelvis/spine/chest, neck, head, tail, four legs, sockets
    (`socket.saddle` where the rider's root sits, `socket.head`, `socket.holster_l/r`). Faces
    -Y like skeleton H; the ground point is under the girth (the middle of the hooves)."""
    b = []

    def add(name, parent, head, tail, deform=True):
        b.append(Bone(name, parent, tuple(head), tuple(tail), deform))

    add("root", None, (0, 0, 0), (0, 0, 0.3), False)
    add("pelvis", "root", (0, 0.55, 1.38), (0, 0.25, 1.40))
    add("spine_01", "pelvis", (0, 0.25, 1.40), (0, -0.05, 1.38))
    add("spine_02", "spine_01", (0, -0.05, 1.38), (0, -0.35, 1.42))
    add("chest", "spine_02", (0, -0.35, 1.42), (0, -0.62, 1.48))
    add("neck_01", "chest", (0, -0.62, 1.48), (0, -0.82, 1.70))
    add("neck_02", "neck_01", (0, -0.82, 1.70), (0, -0.98, 1.92))
    add("head", "neck_02", (0, -0.98, 1.92), (0, -1.30, 1.62))
    add("tail_01", "pelvis", (0, 0.78, 1.38), (0, 0.90, 1.18))
    add("tail_02", "tail_01", (0, 0.90, 1.18), (0, 0.96, 0.92))
    add("tail_03", "tail_02", (0, 0.96, 0.92), (0, 0.98, 0.62))
    for side, sfx in ((1, "_l"), (-1, "_r")):
        for leg, (parent, chain) in HORSE_LEGS.items():
            p = parent
            for name, head, tail in chain:
                bname = f"{leg}_{name}{sfx}"
                add(bname, p, (head[0] * side, head[1], head[2]), (tail[0] * side, tail[1], tail[2]))
                p = bname
    add("socket.saddle", "spine_02", (0, -0.10, 1.58), (0, -0.10, 1.70), False)
    add("socket.head", "head", (0, -1.16, 1.86), (0, -1.16, 1.96), False)
    add("socket.holster_l", "spine_02", (0.24, -0.36, 1.30), (0.24, -0.36, 1.20), False)
    add("socket.holster_r", "spine_02", (-0.24, -0.36, 1.30), (-0.24, -0.36, 1.20), False)
    return b


# ----------------------------------------------------------------------------- clips

def md_clip_frames(spec):
    """Frames of an `.MD` animation line as written in the file, e.g. `@DEATH 9 4 0 70`
    -> (71, False): a `@` range runs from first to last (backwards if last < first), a `#`
    list names its frames. Returns (frame count, backwards)."""
    words = spec.split()
    kind, nums = words[0][0], [int(w) for w in words[1:]]
    if kind == "@":
        first, last = nums[2], nums[3]
        return abs(last - first) + 1, last < first
    return nums[1], False


def clip_keys(n_frames, keys):
    """Keyframes for a placeholder clip of `n_frames` sprite frames: `keys` holds
    (fraction 0..1, pose); returns [(frame, pose)] with frame = round(fraction * (n - 1)),
    so the clip's frame i is the sprite's frame i (1 frame = 1 game tick at 10 per second
    unless the type's speed changes it)."""
    last = max(n_frames - 1, 0)
    out = []
    for t, pose in keys:
        f = int(round(t * last))
        if out and out[-1][0] == f:
            out[-1] = (f, pose)
        else:
            out.append((f, pose))
    return out


# The share of a horse's DEATH clip over which its body is slid to where the sprites' corpse
# lies (`cavalry.centre_fall`): the stumble, until the body is down (its root has turned
# onto its side by 0.42 s of 1.54 s). Spread over the whole clip, the dead horse kept
# sliding on the ground for a second after it had fallen.
FALL_SLIDE_END = 0.3


def fall_slide(t, end=FALL_SLIDE_END):
    """Share (0..1) of the death slide done at `t` (0..1 through the clip): eased in and
    out over the first `end` of the clip, complete from there on."""
    k = min(max(t / end, 0.0), 1.0)
    return k * k * (3 - 2 * k)
