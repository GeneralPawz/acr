"""Hands that hold their weapons (#287 m2-grips; needs `bpy`): closed fists, hand sockets
in the fist and the hand turned to the held item.

- **Fists** (`close`): the MakeHuman body merges the fingers into mittens at its budget
  (`mbody.TARGET_TRIS`), so the rig's finger bones carry no weights and cannot curl them.
  The fist is baked into the rest mesh instead (as the riders' `cavalry.close_hands`):
  every hand vertex past the knuckles rolls round the grip's axis, a cylinder `radius`
  before the hand's axis, as far round as it lies past the knuckles. The radius follows
  the diameter of the item the hand holds (`held.GRIPS`; `held.FIST` for an empty hand).
  No runtime cost and no morph target to key per clip (the hands hold something in every
  clip; a corpse's fist stays closed).
- **Sockets in the fist** (`setup`): `socket.hand_[lr]` (and `socket.club_r`,
  `socket.blade_r` beside the right one) move to the fist's centre, on the grip axis.
- **The grip frame** (`frame`): the item frame (origin at the grip, +Z the long axis, -Y
  the edge or face) in the fist at rest: +Z along the knuckle line out of the thumb side,
  -Y along the hand towards the knuckles. `gear.hold` puts an item in it.
- **Turning the hand** (`armature.pose`, on an armature with `acr_grip`): a grip socket
  never turns on its own; the item's orientation that the clip asks for ("world", or the
  socket's own key) turns the hand instead, so the weapon stays in the palm.
- **The off hand** (`aim._solve`): a left reach goal on the right item's axis (a musket's
  fore-stock, a pike's shaft) puts the left fist's centre on the axis and turns the hand
  so its knuckle line runs along it.
"""

import math

import bpy
from mathutils import Matrix, Vector

from . import held as library

KNUCKLE = 0.08  # m along the hand from the wrist to the knuckle line (the rig's hand length)
PALM = 0.012  # the grip axis lies this far before the palm surface plus the item's radius
GRIP_SOCKETS = ("socket.hand_r", "socket.hand_l", "socket.club_r", "socket.blade_r")


def radius_for(diameter):
    """The fist's curl radius (the grip axis from the hand's axis) for a grip of `diameter`."""
    return min(max(diameter / 2 + PALM, 0.022), 0.034)


def _side(name):
    return "l" if name.endswith("_l") else "r"


def _palm(d, side):
    """The palm's normal (rest): towards the thigh, square to the hand's axis `d`."""
    n = Vector((1.0 if side == "r" else -1.0, 0.0, 0.0))
    return (n - d * n.dot(d)).normalized()


def _hand(arm_obj, prefix, side):
    return arm_obj.data.bones[prefix + "hand_" + side]


def frame(arm_obj, side, prefix=""):
    """The grip frame (3x3, armature space, rest): columns the item's X, Y, Z in the fist."""
    b = _hand(arm_obj, prefix, side)
    d = (b.tail_local - b.head_local).normalized()
    t = Vector((0.0, -1.0, 0.0))
    t = (t - d * t.dot(d)).normalized()  # the knuckle line, out of the thumb side
    y = -d
    g = Matrix((y.cross(t), y, t)).transposed()
    # A long gun's wrist is gripped diagonally across the palm. The transverse
    # tool grip otherwise puts the trigger wrist directly above the stock.
    pitch = arm_obj.get(prefix + "acr_grip_pitch_" + side, 0.0)
    yaw = arm_obj.get(prefix + "acr_grip_yaw_" + side, 0.0)
    return g @ Matrix.Rotation(math.radians(pitch), 3, "X") @ Matrix.Rotation(math.radians(yaw), 3, "Y")


def centre(arm_obj, side, prefix="", radius=None):
    """The fist's centre on the grip axis (armature space, rest)."""
    b = _hand(arm_obj, prefix, side)
    d = (b.tail_local - b.head_local).normalized()
    r = radius if radius is not None else arm_obj.get("acr_fist", {}).get(side, radius_for(library.FIST))
    return b.head_local + d * KNUCKLE + _palm(d, side) * r


def close(ch, diameters=None):
    """Curls both hands of `ch` round grips of `diameters` {"r"|"l": m} (default
    `held.FIST`). Returns the number of vertices moved."""
    P = ch.prefix
    me = ch.flesh.data
    moved = 0
    radii = {}
    for sd in ("l", "r"):
        g = ch.flesh.vertex_groups.get(P + "hand_" + sd)
        if g is None:
            continue
        r = radius_for((diameters or {}).get(sd, library.FIST))
        radii[sd] = r
        b = _hand(ch.arm, P, sd)
        d = (b.tail_local - b.head_local).normalized()
        n = _palm(d, sd)
        ax = frame(ch.arm, sd, P).col[2].normalized()
        n = (n - ax * n.dot(ax)).normalized()
        bend = n.cross(ax).normalized()
        if bend.dot(d) < 0:
            ax, bend = -ax, -bend
        k = b.head_local + d * KNUCKLE
        c = k + n * r
        for v in me.vertices:
            if next((e.weight for e in v.groups if e.group == g.index), 0.0) < 0.5:
                continue
            s = (v.co - k).dot(bend)
            if s > 0.0:
                v.co = c + Matrix.Rotation(s / r, 3, ax) @ (v.co - bend * s - c)
                moved += 1
    me.update()
    ch.arm["acr_fist"] = radii
    return moved


def setup(ch):
    """Moves the hand sockets of `ch` into the fists (`close` first) and turns on the grip
    mode of `armature.pose`."""
    arm_obj, P = ch.arm, ch.prefix
    # The socket's rest frame turns with the item from "upright" (the old rest: the item
    # frame = the socket's pose @ old rest⁻¹) into the fist, so code that reads the socket's
    # matrix as the item's (`aim`: the musket's axis, the stowed ramrod) keeps working.
    want = {}
    for name in GRIP_SOCKETS:
        b = arm_obj.data.bones.get(P + name)
        if b is not None:
            sd = _side(name)
            want[name] = (centre(arm_obj, sd, P), frame(arm_obj, sd, P) @ b.matrix_local.to_3x3())
    bpy.context.view_layer.objects.active = arm_obj
    bpy.ops.object.mode_set(mode="EDIT")
    ebs = arm_obj.data.edit_bones
    for name, (c, m) in want.items():
        eb = ebs[P + name]
        eb.head, eb.tail = c, c + m.col[1] * 0.10
        eb.align_roll(m.col[2])
    bpy.ops.object.mode_set(mode="OBJECT")
    arm_obj["acr_grip"] = 1
    bpy.context.view_layer.update()


def is_grip(arm_obj, key):
    """Whether pose bone name `key` is a grip socket of an armature in grip mode."""
    return bool(arm_obj.get("acr_grip")) and any(key.endswith(s) for s in GRIP_SOCKETS)


def item_in_socket(arm_obj, key):
    """C (3x3): the item frame in the socket bone's rest frame (`setup`: the inverse of the
    old upright rest)."""
    pb = arm_obj.pose.bones[key]
    prefix = key[: len(key) - len(next(s for s in GRIP_SOCKETS if key.endswith(s)))]
    c = pb.bone.matrix_local.to_3x3().inverted() @ frame(arm_obj, _side(key), prefix)
    return c.to_quaternion().to_matrix()


def hold_matrix(arm_obj, socket, offset=(0.0, 0.0, 0.0)):
    """The world matrix an item takes on `socket` now: in grip mode the grip frame (the item
    in the fist), else upright in the socket's rest frame (the old rule). `offset` moves it
    (item frame in grip mode)."""
    from . import armature as am

    sock = am.world(arm_obj, socket)
    pb = arm_obj.pose.bones[socket]
    if is_grip(arm_obj, socket):
        held = list(arm_obj.get("acr_grip_held", ()))
        if socket not in held:
            arm_obj["acr_grip_held"] = held + [socket]
        rot = sock.to_3x3() @ item_in_socket(arm_obj, socket)
        return Matrix.Translation(sock.translation + rot @ Vector(offset)) @ rot.to_4x4()
    rest_up = sock.to_3x3() @ pb.bone.matrix_local.to_3x3().inverted()
    return Matrix.Translation(sock.translation + Vector(offset)) @ rest_up.to_4x4()


def turn_hand(arm_obj, key, item_rot):
    """Turns the hand that carries grip socket `key` so the item in it takes armature-space
    orientation `item_rot` (3x3); the socket keeps its rest pose. Returns the hand's name."""
    pb = arm_obj.pose.bones[key]
    hand = pb.parent
    c = item_in_socket(arm_obj, key)
    s_rest = pb.bone.matrix_local.to_3x3()
    target = item_rot @ c.inverted() @ s_rest.inverted() @ hand.bone.matrix_local.to_3x3()
    par = hand.parent
    rel = par.bone.matrix_local.to_3x3().inverted() @ hand.bone.matrix_local.to_3x3()
    hand.rotation_mode = "QUATERNION"
    hand.rotation_quaternion = ((par.matrix.to_3x3() @ rel).inverted() @ target).to_quaternion()
    pb.rotation_mode = "QUATERNION"
    pb.rotation_quaternion = (1.0, 0.0, 0.0, 0.0)
    return hand.name


def item_axis(arm_obj, key):
    """(origin, +Z) of the item in grip socket `key` (armature space, current pose)."""
    pb = arm_obj.pose.bones[key]
    rot = pb.matrix.to_3x3() @ item_in_socket(arm_obj, key)
    return pb.head.copy(), (rot @ Vector((0.0, 0.0, 1.0))).normalized()


def align_off_hand(arm_obj, side, axis, prefix=""):
    """Turns hand `side` (from its current pose) the least so its knuckle line runs along
    `axis` (either way). Returns the hand's rotation from rest (3x3, armature space: its
    "world" rotation)."""
    hand = arm_obj.pose.bones[prefix + "hand_" + side]
    g = frame(arm_obj, side, prefix)
    cur = hand.matrix.to_3x3() @ hand.bone.matrix_local.to_3x3().inverted()
    t = cur @ g.col[2]
    a = Vector(axis).normalized()
    if t.dot(a) < 0:
        a = -a
    aligned = t.rotation_difference(a).to_matrix() @ cur
    # Roll the supporting palm towards the forearm's neutral direction. Keeping
    # its old A-pose roll put the wrist above the fore-end and the cuff in it.
    fore = arm_obj.pose.bones[prefix + "forearm_" + side]
    rest_d = hand.bone.matrix_local.to_3x3().col[1]
    neutral = fore.matrix.to_3x3() @ fore.bone.matrix_local.to_3x3().inverted() @ rest_d
    want = neutral - a * neutral.dot(a)
    have = aligned @ rest_d
    have -= a * have.dot(a)
    if want.length > 1e-5 and have.length > 1e-5:
        want.normalize()
        have.normalize()
        angle = math.atan2(a.dot(have.cross(want)), have.dot(want))
        aligned = Matrix.Rotation(angle, 3, a) @ aligned
    return aligned


def palm_errors(arm_obj, prefix=""):
    """{side: (distance from the fist centre to the nearest grip axis, item)} for the
    current pose: each hand's fist centre against the axes of the items held (its own
    item, or for a free hand the other hand's two-handed item)."""
    out = {}
    held = {}
    for k in GRIP_SOCKETS:
        if prefix + k in arm_obj.pose.bones:
            pb = arm_obj.pose.bones[prefix + k]
            if pb.scale.x > 0.5 and any(o.get("acr_held_item") or o.get("acr_material_keys")
                                        for o in arm_obj.children if o.parent_bone == pb.name):
                held[k] = item_axis(arm_obj, prefix + k)
    for sd in ("r", "l"):
        c = arm_obj.pose.bones[prefix + "socket.hand_" + sd].head
        best = None
        for k, (o, z) in held.items():
            v = c - o
            dist = (v - z * v.dot(z)).length
            if best is None or dist < best[0]:
                best = (dist, k)
        if best:
            out[sd] = best
    return out
