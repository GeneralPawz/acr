"""Skeleton H: the humanoid armature shared by all European foot units (needs `bpy`).

59 bones, metres, rest pose = relaxed A-pose (arms 35 degrees out), facing -Y (south),
character's left = +X. Names are lower case with `_l` / `_r`; sockets are `socket.<name>` (docs/assets.md)
and do not deform. The joint layout follows the usual BVH/mocap hierarchy (hips root of the
deforming chain, three spine joints, neck, head, clavicles, three-joint fingers), so a
recorded drill can be retargeted by a name map (see docs/assets-humans.md).
"""

import math

import bpy
from mathutils import Matrix, Quaternion, Vector

A_POSE = math.radians(35.0)


def _arm(side):
    """Arm chain points for side +1 (left, +X) or -1 (right)."""
    s = side
    sh = Vector((0.17 * s, 0.0, 1.43))
    d = Vector((math.sin(A_POSE) * s, 0.0, -math.cos(A_POSE)))
    elbow = sh + d * 0.29
    wrist = elbow + d * 0.25
    hand_end = wrist + d * 0.09
    return sh, elbow, wrist, hand_end, d


def bone_table():
    """[(name, parent, head, tail, deform)] in armature space (rest)."""
    b = []

    def add(name, parent, head, tail, deform=True):
        b.append((name, parent, Vector(head), Vector(tail), deform))

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
        sh, elbow, wrist, hand_end, d = _arm(side)
        add("clavicle" + sfx, "spine2", (0.03 * side, -0.01, 1.41), sh)
        add("upperarm" + sfx, "clavicle" + sfx, sh, elbow)
        add("forearm" + sfx, "upperarm" + sfx, elbow, wrist)
        add("hand" + sfx, "forearm" + sfx, wrist, wrist + d * 0.08)
        # Fingers: three joints each, fanned along -Y (palm faces the thigh).
        for i, f in enumerate(("index", "middle", "ring", "pinky")):
            base = wrist + d * 0.08 + Vector((0, -0.03 + 0.02 * i, 0))
            ln = (0.030, 0.034, 0.031, 0.025)[i]
            p = base
            parent = "hand" + sfx
            for j in range(3):
                q = p + d * ln * (1.0 - 0.2 * j)
                add(f"{f}_0{j + 1}{sfx}", parent, p, q)
                parent, p = f"{f}_0{j + 1}{sfx}", q
        p = wrist + d * 0.02 + Vector((0, -0.035, 0))
        parent = "hand" + sfx
        for j in range(3):
            q = p + (d * 0.6 + Vector((0, -0.4, 0))).normalized() * 0.03
            add(f"thumb_0{j + 1}{sfx}", parent, p, q)
            parent, p = f"thumb_0{j + 1}{sfx}", q
        add("socket.hand" + sfx, "hand" + sfx, wrist + d * 0.06 + Vector((0, -0.01, 0)),
            wrist + d * 0.06 + Vector((0, -0.11, 0)), False)
        hx = 0.09 * side
        add("thigh" + sfx, "hips", (hx, 0, 0.93), (hx * 1.05, 0.0, 0.50))
        add("shin" + sfx, "thigh" + sfx, (hx * 1.05, 0.0, 0.50), (hx * 1.05, 0.02, 0.085))
        add("foot" + sfx, "shin" + sfx, (hx * 1.05, 0.02, 0.085), (hx * 1.08, -0.11, 0.025))
        add("toe" + sfx, "foot" + sfx, (hx * 1.08, -0.11, 0.025), (hx * 1.1, -0.18, 0.02))
    return b


# Bones that deform the body skin (fingers ride on the hand: mitten hands at this scale).
BODY_BONES = {
    "hips", "spine", "spine1", "spine2", "neck", "head",
    "clavicle_l", "upperarm_l", "forearm_l", "hand_l", "thigh_l", "shin_l", "foot_l", "toe_l",
    "clavicle_r", "upperarm_r", "forearm_r", "hand_r", "thigh_r", "shin_r", "foot_r", "toe_r",
}


def build(name="H"):
    arm = bpy.data.armatures.new(name)
    obj = bpy.data.objects.new(name, arm)
    bpy.context.scene.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    for bname, parent, head, tail, deform in bone_table():
        eb = arm.edit_bones.new(bname)
        eb.head, eb.tail = head, tail
        eb.use_deform = deform
        # Rolls only matter to retargeting: poses here are given in armature axes (`pose`).
        eb.align_roll(Vector((0, -1, 0)) if abs((tail - head).normalized().z) > 0.7 else Vector((0, 0, 1)))
        if parent:
            eb.parent = arm.edit_bones[parent]
            eb.use_connect = (eb.parent.tail - head).length < 1e-4
    bpy.ops.object.mode_set(mode="OBJECT")
    return obj


def segments(arm_obj):
    """{bone: (head, tail)} in armature space (rest)."""
    return {b.name: (b.head_local.copy(), b.tail_local.copy()) for b in arm_obj.data.bones}


def pose(arm_obj, spec, frame=None):
    """Applies `spec` = {bone: (rx, ry, rz)} (degrees, XYZ Euler about the *armature* axes of
    the rest pose, relative to the parent) plus optional "root_loc": (x, y, z) in armature
    space, and keys it at `frame` if given. Unlisted bones return to rest."""
    for pb in arm_obj.pose.bones:
        rest = pb.bone.matrix_local.to_3x3()
        r = spec.get(pb.name)
        if r is None:
            q = Quaternion()
        else:
            e = Matrix.Rotation(math.radians(r[2]), 3, "Z") @ Matrix.Rotation(
                math.radians(r[1]), 3, "Y") @ Matrix.Rotation(math.radians(r[0]), 3, "X")
            q = (rest.inverted() @ e @ rest).to_quaternion()
        pb.rotation_mode = "QUATERNION"
        pb.rotation_quaternion = q
        pb.location = (0, 0, 0)
        if pb.name == "root" and "root_loc" in spec:
            pb.location = rest.inverted() @ Vector(spec["root_loc"])
        if frame is not None:
            pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
            pb.keyframe_insert("location", frame=frame, group=pb.name)
    bpy.context.view_layer.update()
