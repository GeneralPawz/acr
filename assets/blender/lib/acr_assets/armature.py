"""Armatures from rig tables (`acr_assets.rig`), posing and placeholder clips (needs `bpy`).

Poses are given as Euler angles about the *armature* axes of the rest pose (degrees,
relative to the parent), so they read the same whatever the bone rolls: +X tips a bone's far
end backwards (+Y), -X forwards; +Y swings a left limb inwards; +Z turns about the vertical.
A clip is a Blender action named after the `.MD` key (`STAND`, `DEATH`, ...); the glTF
exporter writes one animation per action.
"""

import math

import bpy
from mathutils import Matrix, Quaternion, Vector


def build(name, table, parent=None):
    """An armature object from [rig.Bone]; linked to the scene, optionally under `parent`."""
    arm = bpy.data.armatures.new(name)
    obj = bpy.data.objects.new(name, arm)
    bpy.context.scene.collection.objects.link(obj)
    if parent is not None:
        obj.parent = parent
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    for b in table:
        eb = arm.edit_bones.new(b.name)
        eb.head, eb.tail = Vector(b.head), Vector(b.tail)
        eb.use_deform = b.deform
        d = (eb.tail - eb.head).normalized()
        eb.align_roll(_roll_axis(b.name, d))
        if b.parent:
            eb.parent = arm.edit_bones[b.parent]
            eb.use_connect = (eb.parent.tail - eb.head).length < 1e-4
    bpy.ops.object.mode_set(mode="OBJECT")
    apply_limits(obj)
    return obj


HAND_BONES = ("thumb_", "index_", "middle_", "ring_", "pinky_")


def _clamp(bone, q):
    """`q` (local) with its Euler XYZ angles clamped to `bone`'s limits (fingers and thumb:
    every authored hand pose stays anatomical)."""
    from . import limits

    lim = limits.limit(bone)
    if lim is None or lim["kind"] not in ("euler", "hinge"):
        return q
    e = q.to_euler("XYZ")
    for i, ax in enumerate("xyz"):
        lo, hi = (math.radians(v) for v in lim[ax])
        e[i] = min(hi, max(lo, e[i]))
    return e.to_quaternion()


def _roll_axis(name, d):
    """The direction local +Z takes (`limits`): palmar for finger and thumb bones (towards
    the thigh, so flexion is +X on both hands), else forward for vertical bones and up for
    level ones."""
    from . import limits

    key, side = limits.key_of(name)
    if key and key.startswith(("finger_", "thumb_")):
        n = Vector((1.0 if side == "r" else -1.0, 0.0, 0.0))
        return n - d * n.dot(d)
    return Vector((0, -1, 0)) if abs(d.z) > 0.7 else Vector((0, 0, 1))


def apply_limits(arm_obj, hands_only=True):
    """LIMIT_ROTATION constraints (local space, `limits` table) on the deforming bones of
    skeleton H in `arm_obj`: by default on the finger and thumb bones only (the clips of the
    other bones are audited, not clamped: `audit_limits`). Cone bones get none."""
    from . import limits

    n = 0
    for pb in arm_obj.pose.bones:
        key, _side = limits.key_of(pb.name)
        lim = limits.limit(pb.name)
        if lim is None or lim["kind"] not in ("euler", "hinge"):
            continue
        if hands_only and not key.startswith(("finger_", "thumb_")):
            continue
        c = pb.constraints.new("LIMIT_ROTATION")
        c.name = "acr_limit"
        c.owner_space = "LOCAL"
        c.euler_order = "XYZ"
        for ax in "xyz":
            lo, hi = lim[ax]
            setattr(c, f"use_limit_{ax}", True)
            setattr(c, f"min_{ax}", math.radians(lo))
            setattr(c, f"max_{ax}", math.radians(hi))
        n += 1
    return n


def audit_limits(arm_obj):
    """Every frame of every clip of `arm_obj` against the joint limits (`limits.excess`,
    beyond the table's tolerance), read from the actions' F-curves: {"frames", "violations",
    "worst": [clip, bone, frame, degrees], "by_bone": {bone: [count, max degrees]}}."""
    from . import limits

    tol = limits.table()["tolerance_deg"]
    bones = [b.name for b in arm_obj.data.bones if b.use_deform and limits.limit(b.name)]
    # A rider's or crew's skeleton shares the armature with a horse or gun: only its bones.
    if any(b.startswith(limits.PREFIXES) for b in bones):
        bones = [b for b in bones if b.startswith(limits.PREFIXES)]
    frames = viol = 0
    worst = None
    by_bone = {}
    for clip in arm_obj.get("acr_clips", []):
        act = bpy.data.actions.get(clip)
        if act is None:
            continue
        curves = {}
        for fc in _fcurves(act):
            if fc.data_path.endswith("rotation_quaternion"):
                curves.setdefault(fc.data_path.split('"')[1], {})[fc.array_index] = fc
        f0, f1 = (int(round(v)) for v in act.frame_range)
        for f in range(f0, f1 + 1):
            frames += 1
            for b in bones:
                fc = curves.get(b)
                if not fc:
                    continue
                q = tuple(fc[i].evaluate(f) if i in fc else (1.0 if i == 0 else 0.0) for i in range(4))
                e = limits.excess(b, q)
                if e is None or e <= tol:
                    continue
                viol += 1
                cnt, mx = by_bone.get(b, (0, 0.0))
                by_bone[b] = [cnt + 1, round(max(mx, e), 1)]
                if worst is None or e > worst[3]:
                    worst = [clip, b, f, round(e, 1)]
    return {"frames": frames, "violations": viol, "worst": worst,
            "by_bone": dict(sorted(by_bone.items()))}


def segments(arm_obj):
    """{bone: (head, tail)} in armature space (rest)."""
    return {b.name: (b.head_local.copy(), b.tail_local.copy()) for b in arm_obj.data.bones}


def euler(r):
    return (Matrix.Rotation(math.radians(r[2]), 3, "Z") @ Matrix.Rotation(math.radians(r[1]), 3, "Y")
            @ Matrix.Rotation(math.radians(r[0]), 3, "X"))


def spec_bones(spec):
    """Bones a pose spec moves (rotation) and the ones it places (location)."""
    rot = {k for k in spec if k not in ("root_loc", "world", "world_loc")}
    rot |= set(spec.get("world", {})) | set(spec.get("world_loc", {}))
    loc = set(spec.get("world_loc", {})) | ({"root"} if "root_loc" in spec else set())
    return rot, loc


def pose(arm_obj, spec, frame=None, prefix="", keyed=None):
    """Applies `spec` = {bone: (rx, ry, rz)} plus optional "root_loc": (x, y, z) (armature
    space) and "world": {bone: (rx, ry, rz)} (see below), and keys it at `frame`: all
    bones, or with `keyed` = (rotated, located) only those (unprefixed names). Bone names in `spec` get `prefix`; bones of the armature
    without the prefix are left alone, unlisted bones with it return to rest."""
    for pb in arm_obj.pose.bones:
        if not pb.name.startswith(prefix):
            continue
        key = pb.name[len(prefix):]
        rest = pb.bone.matrix_local.to_3x3()
        r = spec.get(key)
        q = Quaternion() if r is None else (rest.inverted() @ euler(r) @ rest).to_quaternion()
        if r is not None and key.startswith(HAND_BONES):
            q = _clamp(key, q)
        pb.rotation_mode = "QUATERNION"
        pb.rotation_quaternion = q
        pb.location = (0, 0, 0)
        if key == "root" and "root_loc" in spec:
            pb.location = rest.inverted() @ Vector(spec["root_loc"])
        if frame is not None and key not in spec.get("world", {}) and key not in spec.get("world_loc", {}):
            if keyed is None or key in keyed[0]:
                pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
            if keyed is None or key in keyed[1]:
                pb.keyframe_insert("location", frame=frame, group=pb.name)
    bpy.context.view_layer.update()
    world = spec.get("world", {})
    if arm_obj.get("acr_grip"):
        _grip_hands(arm_obj, spec, world, frame, prefix)
    # "world": {bone: (rx, ry, rz)}: the bone's armature-space orientation = that rotation
    # applied to its rest orientation, whatever its parents do (a socket that keeps a
    # musket upright or aimed while the hand moves).
    for key, r in world.items():
        if arm_obj.get("acr_grip") and key in arm_obj.get("acr_grip_held", ()):
            continue
        pb = arm_obj.pose.bones[prefix + key]
        target = euler(r) @ pb.bone.matrix_local.to_3x3()
        par = pb.parent
        rel = par.bone.matrix_local.to_3x3().inverted() @ pb.bone.matrix_local.to_3x3()
        basis = (par.matrix.to_3x3() @ rel).inverted() @ target
        pb.rotation_mode = "QUATERNION"
        pb.rotation_quaternion = basis.to_quaternion()
        if frame is not None:
            pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
            pb.keyframe_insert("location", frame=frame, group=pb.name)
        bpy.context.view_layer.update()
    # "world_loc": {bone: (x, y, z)}: the bone's head in armature space (a rider thrown off
    # the horse lies on the ground wherever the horse is).
    for key, loc in spec.get("world_loc", {}).items():
        pb = arm_obj.pose.bones[prefix + key]
        m = pb.matrix.copy()
        m.translation = Vector(loc)
        pb.matrix = m
        bpy.context.view_layer.update()
        if frame is not None:
            pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
            pb.keyframe_insert("location", frame=frame, group=pb.name)


def _grip_hands(arm_obj, spec, world, frame, prefix):
    """Grip mode (`grips.py`): a held item's socket keeps its rest pose in the fist and the
    hand turns instead, so the item takes the orientation the pose asks for: its "world"
    rotation, or the one the socket's own key gives (the item upright in the socket's rest
    frame turned with it, the rule before the fists). One socket per hand: an explicit
    "world" first, then `socket.hand_*`."""
    from . import grips

    held = [k for k in arm_obj.get("acr_grip_held", ()) if prefix + k in arm_obj.pose.bones]
    if not held:
        return
    targets = {}
    for key in held:
        pb = arm_obj.pose.bones[prefix + key]
        rest = pb.bone.matrix_local.to_3x3()
        rot = euler(world[key]) if key in world else pb.matrix.to_3x3() @ rest.inverted()
        rank = (key in world, key.startswith("socket.hand"))
        hand = pb.parent.name
        if hand not in targets or rank > targets[hand][0]:
            targets[hand] = (rank, key, rot)
    for hand, (_rank, key, rot) in sorted(targets.items()):
        grips.turn_hand(arm_obj, prefix + key, rot)
        bpy.context.view_layer.update()
        if frame is not None:
            for name in (hand, prefix + key):
                arm_obj.pose.bones[name].keyframe_insert("rotation_quaternion", frame=frame, group=name)
    for key in held:
        pb = arm_obj.pose.bones[prefix + key]
        if pb.rotation_quaternion != Quaternion():
            pb.rotation_quaternion = Quaternion()
            if frame is not None:
                pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
    bpy.context.view_layer.update()


def add_clip(arm_obj, name, keys, prefix_keys=None):
    """Creates the action `name` from [(frame, spec)] (and for a combined rig
    `prefix_keys` = {prefix: [(frame, spec)]}); keeps it with a fake user. Returns it."""
    arm_obj.animation_data_create()
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    arm_obj.animation_data.action = act
    groups = prefix_keys or {"": keys}
    # Key every bone but fingers and sockets that no key of the clip moves: their channels
    # would be constant and bloat the file.
    keyed = {}
    still = ("index_", "middle_", "ring_", "pinky_", "thumb_", "socket.")
    names = [b.name for b in arm_obj.data.bones]
    for prefix, ks in groups.items():
        own = [n[len(prefix):] for n in names if n.startswith(prefix) and not any(
            n.startswith(q) for q in groups if len(q) > len(prefix) and q.startswith(prefix))]
        rot, loc = {n for n in own if not any(n.startswith(t) for t in still)}, set()
        for _f, spec in ks:
            r, lo = spec_bones(spec)
            rot |= r
            loc |= lo
        keyed[prefix] = (rot, loc)
    for prefix, ks in groups.items():
        for f, spec in ks:
            pose(arm_obj, spec, frame=f, prefix=prefix, keyed=keyed[prefix])
    for fc in _fcurves(act):
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    clips = list(arm_obj.get("acr_clips", []))
    clips.append(name)
    arm_obj["acr_clips"] = clips
    arm_obj.animation_data.action = None
    return act


def _fcurves(act):
    """F-curves of an action (Blender 5 keeps them in layered channelbags)."""
    if hasattr(act, "fcurves") and len(getattr(act, "fcurves", [])):
        return list(act.fcurves)
    out = []
    for layer in getattr(act, "layers", []):
        for strip in layer.strips:
            for bag in getattr(strip, "channelbags", []):
                out += list(bag.fcurves)
    return out


def set_clip(arm_obj, name, frame=0):
    """Shows clip `name` at `frame`."""
    arm_obj.animation_data_create()
    act = bpy.data.actions[name]
    arm_obj.animation_data.action = act
    if hasattr(arm_obj.animation_data, "action_slot") and act.slots:
        arm_obj.animation_data.action_slot = act.slots[0]
    bpy.context.scene.frame_set(frame)
    bpy.context.view_layer.update()


def world(arm_obj, bone):
    """World matrix of a pose bone (current pose)."""
    return arm_obj.matrix_world @ arm_obj.pose.bones[bone].matrix


def attach(obj, arm_obj, bone, world_matrix):
    """Parents `obj` to `bone` (current pose) so that it sits at `world_matrix` now and
    follows the bone afterwards (glTF: a child node of the joint)."""
    obj.parent = arm_obj
    obj.parent_type = "BONE"
    obj.parent_bone = bone
    bpy.context.view_layer.update()
    pb = arm_obj.pose.bones[bone]
    tail = arm_obj.matrix_world @ pb.matrix @ Matrix.Translation((0, pb.bone.length, 0))
    obj.matrix_parent_inverse = tail.inverted()
    obj.matrix_basis = world_matrix
    obj["acr_socket"] = bone
    return obj


def bind(obj, arm_obj):
    """Skins `obj` to `arm_obj` (armature modifier; vertex groups named after bones)."""
    obj.parent = arm_obj
    m = obj.modifiers.new("armature", "ARMATURE")
    m.object = arm_obj
    return obj
