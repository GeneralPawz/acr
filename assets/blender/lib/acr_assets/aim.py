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


def _solve(arm_obj, spec, goals, iterations=3):
    """Solves the arm chains of `goals` = {"r"|"l": (point, pole)} by IK on the posed
    `spec` so the right hand's grip point (the head of `socket.hand_r`) or the left palm
    reaches `point` (armature space), the elbow bending towards `pole`; writes the angles
    back into `spec` (updated in place and returned)."""
    arm = arm_obj.pose.bones
    chain = {"r": "forearm_r", "l": "forearm_l"}

    def offset(side):
        if side == "r":
            return arm["socket.hand_r"].head - arm["forearm_r"].tail
        return (arm["hand_l"].tail - arm["forearm_l"].tail) * 0.5

    am.pose(arm_obj, spec)
    # A socket placed in armature space ("world_loc") does not follow the hand: measure
    # the hand's own grip point from a pose without it.
    loose = {k: v for k, v in spec.items() if k != "world_loc"}
    bones = [b for side in goals for b in (f"upperarm_{side}", f"forearm_{side}")]
    empties = []
    for _ in range(iterations):
        am.pose(arm_obj, loose)
        offs = {side: offset(side) for side in goals}
        for o in empties:
            bpy.data.objects.remove(o)
        empties = []
        for side, (point, pole) in goals.items():
            t = _empty(f"ik.{side}", arm_obj.matrix_world @ (Vector(point) - offs[side]))
            p = _empty(f"pole.{side}", arm_obj.matrix_world @ Vector(pole))
            empties += [t, p]
            c = arm[chain[side]].constraints.new("IK")
            c.name = "acr_ik"
            c.target = t
            c.pole_target = p
            c.pole_angle = math.radians(-90)
            c.chain_count = 2
            c.use_tail = True
        bpy.context.view_layer.update()
        solved = _spec_angles(arm_obj, bones)
        for side in goals:
            pb = arm[chain[side]]
            pb.constraints.remove(pb.constraints["acr_ik"])
        spec.update(solved)
        loose.update(solved)
    for o in empties:
        bpy.data.objects.remove(o)
    am.pose(arm_obj, spec)
    return spec


def hold(arm_obj, base, pocket=(0.03, -0.05, -0.04), left_along=0.34, left_drop=0.035, iterations=3,
         elbow_r=(0.35, -0.15, -0.35), elbow_l=0.45):
    """`base` (a pose spec with the musket socket's "world" orientation) with both arms
    solved so the musket's butt sits in the right shoulder pocket (offset `pocket` from the
    right shoulder joint: inwards, forwards, down, metres), the right hand holds the grip
    and the left hand the barrel `left_along` m further on, `left_drop` m below it. The
    right elbow points `elbow_r` (outwards, backwards along the musket, up) from the
    shoulder, the left elbow `elbow_l` m straight below the left hand. Returns the new spec."""
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
    flat = Vector((axis.x, axis.y, 0)).normalized()
    butt = sh + inward * pocket[0] + Vector((0, 0, pocket[2])) + flat * pocket[1] * -1
    grip = butt + axis * GRIP_TO_BUTT
    left = grip + axis * left_along + Vector((0, 0, -left_drop))
    goals = {
        "r": (grip, sh - inward * elbow_r[0] - flat * elbow_r[1] + Vector((0, 0, elbow_r[2]))),
        "l": (left, left + Vector((0, 0, -elbow_l)) + flat * 0.10),
    }
    return _solve(arm_obj, spec, goals, iterations)


def place(arm_obj, base, grip, right="grip", left=0.40, elbow_r=(-0.30, 0.15, -0.30), elbow_l=(0.25, 0.0, -0.35),
          butt=False):
    """A loading pose: the musket (orientation from `base`'s "world" socket rotation) with
    its grip at `grip` (armature space), whichever hand holds it. `right`/`left` say where
    each hand goes: "grip", a distance in metres along the musket from the grip (the left
    palm under the stock, the right hand on the rammer or the muzzle), a point in armature
    space (`(x, y, z)`), or None to leave the arm as `base` has it. The elbows bend towards
    the shoulder + `elbow_r` / `elbow_l` (x, y, z). With `butt`, `grip` is where the butt
    stands instead (the musket grounded). Returns the new spec."""
    spec = dict(base)
    am.pose(arm_obj, spec)
    arm = arm_obj.pose.bones
    axis = (arm["socket.hand_r"].matrix.to_3x3() @ Vector((0, 0, 1))).normalized()
    g = Vector(grip) + (axis * GRIP_TO_BUTT if butt else Vector())
    spec["world_loc"] = {"socket.hand_r": tuple(round(c, 4) for c in g)}

    def where(t):
        if t == "grip":
            return g
        if isinstance(t, (int, float)):
            return g + axis * t
        return Vector(t)

    goals = {}
    for side, t, el in (("r", right, elbow_r), ("l", left, elbow_l)):
        if t is not None:
            goals[side] = (where(t), arm[f"upperarm_{side}"].head + Vector(el))
    _solve(arm_obj, spec, goals)
    # How far each hand ended from its goal (metres; reported by the build).
    am.pose(arm_obj, {k: v for k, v in spec.items() if k != "world_loc"})
    at = {"r": arm["socket.hand_r"].head, "l": (arm["hand_l"].tail + arm["forearm_l"].tail) * 0.5}
    spec_err = {s: round((at[s] - p).length, 3) for s, (p, _) in goals.items()}
    am.pose(arm_obj, spec)
    print("ACR place errors (m):", spec_err)
    return spec


def drill(arm_obj, steps):
    """{name: spec} for the loading steps {name: {"base", "grip", ...}} (`place` arguments)."""
    return {name: place(arm_obj, **kw) for name, kw in steps.items()}


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
