"""Two-handed musket poses solved with inverse kinematics (needs `bpy`): the butt in the
right shoulder, the right hand at the grip, the left hand under the barrel, the cheek down
on the stock. The arms are solved by Blender's IK on the posed body and written back as
ordinary pose angles (`armature.pose` specs), so the clips stay plain data.

The musket hangs on `socket.hand_r` with its origin at the grip, 0.25 m in front of the butt
(`gear.musket`); the spec's "world" orientation of the socket sets where it points.
"""

import math

import bpy
from mathutils import Vector

from . import armature as am

GRIP_TO_BUTT = 0.25


def _empty(name, at):
    e = bpy.data.objects.new(name, None)
    bpy.context.scene.collection.objects.link(e)
    e.location = at
    return e


def _spec_angles(arm_obj, bones):
    """The current visual pose of `bones` as spec angles (degrees about the armature axes of
    the rest pose, relative to the parent: the inverse of `armature.pose`)."""
    out = {}
    for name in bones:
        pb = arm_obj.pose.bones[name]
        rest = pb.bone.matrix_local.to_3x3()
        par = pb.parent
        if par is None:
            basis = rest.inverted() @ pb.matrix.to_3x3()
        else:
            rel = par.bone.matrix_local.to_3x3().inverted() @ rest
            basis = (par.matrix.to_3x3() @ rel).inverted() @ pb.matrix.to_3x3()
        r = (rest @ basis @ rest.inverted()).to_euler("XYZ")
        out[name] = tuple(round(math.degrees(a), 2) for a in r)
    return out


def hold(arm_obj, base, pocket=(0.03, -0.05, -0.04), left_along=0.34, left_drop=0.035, iterations=3):
    """`base` (a pose spec with the musket socket's "world" orientation) with both arms
    solved so the musket's butt sits in the right shoulder pocket (offset `pocket` from the
    right shoulder joint: inwards, forwards, down, metres), the right hand holds the grip
    and the left hand the barrel `left_along` m further on, `left_drop` m below it. Returns
    the new spec."""
    spec = dict(base)
    arm = arm_obj.pose.bones
    am.pose(arm_obj, spec)
    sh = arm["upperarm_r"].head.copy()
    centre = (arm["upperarm_r"].head + arm["upperarm_l"].head) / 2
    inward = (centre - sh)
    inward.z = 0
    inward = inward.normalized() if inward.length > 1e-6 else Vector((0, 0, 0))
    # Where the musket points: the socket's +Z (the musket runs along it from the grip).
    sock = arm["socket.hand_r"]
    axis = (sock.matrix.to_3x3() @ Vector((0, 0, 1))).normalized()
    butt = sh + inward * pocket[0] + Vector((0, 0, pocket[2])) + Vector((axis.x, axis.y, 0)).normalized() * pocket[1] * -1
    grip = butt + axis * GRIP_TO_BUTT
    left = grip + axis * left_along + Vector((0, 0, -left_drop))
    out_r = -inward
    goals = {
        # chain end bone: (target point, pole point)
        "forearm_r": (grip, sh + out_r * 0.35 + Vector((0, 0, -0.35)) - axis * 0.15),
        "forearm_l": (left, arm["upperarm_l"].head + Vector((0, 0, -0.45)) - out_r * 0.15),
    }
    # Effector offsets: from the wrist (forearm tail) to where the hand holds.
    hold_r = arm["socket.hand_r"].head - arm["forearm_r"].tail
    hold_l = (arm["hand_l"].tail - arm["forearm_l"].tail) * 0.5
    empties = []
    for _ in range(iterations):
        for o in empties:
            bpy.data.objects.remove(o)
        empties = []
        for bone, (point, pole) in goals.items():
            off = hold_r if bone == "forearm_r" else hold_l
            t = _empty(f"ik.{bone}", arm_obj.matrix_world @ (point - off))
            p = _empty(f"pole.{bone}", arm_obj.matrix_world @ pole)
            empties += [t, p]
            pb = arm[bone]
            c = pb.constraints.get("acr_ik") or pb.constraints.new("IK")
            c.name = "acr_ik"
            c.target = t
            c.pole_target = p
            c.pole_angle = math.radians(-90)
            c.chain_count = 2
            c.use_tail = True
        bpy.context.view_layer.update()
        solved = _spec_angles(arm_obj, ["upperarm_r", "forearm_r", "upperarm_l", "forearm_l"])
        for bone in goals:
            arm[bone].constraints.remove(arm[bone].constraints["acr_ik"])
        spec.update(solved)
        am.pose(arm_obj, spec)
        hold_r = arm["socket.hand_r"].head - arm["forearm_r"].tail
        hold_l = (arm["hand_l"].tail - arm["forearm_l"].tail) * 0.5
    for o in empties:
        bpy.data.objects.remove(o)
    bpy.context.view_layer.update()
    return spec


def errors(arm_obj, spec, left_along=0.34, left_drop=0.035):
    """How far (metres) the right hand's socket is from where the butt meets the shoulder
    (grip − 0.25 m along the musket), and the left hand from its place on the barrel."""
    am.pose(arm_obj, spec)
    arm = arm_obj.pose.bones
    sock = arm["socket.hand_r"]
    axis = (sock.matrix.to_3x3() @ Vector((0, 0, 1))).normalized()
    butt = sock.head - axis * GRIP_TO_BUTT
    left_target = sock.head + axis * left_along + Vector((0, 0, -left_drop))
    hand_l = (arm["hand_l"].head + arm["hand_l"].tail) / 2
    return (butt - arm["upperarm_r"].head).length, (hand_l - left_target).length
