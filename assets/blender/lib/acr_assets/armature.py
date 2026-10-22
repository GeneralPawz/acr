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
        eb.align_roll(Vector((0, -1, 0)) if abs(d.z) > 0.7 else Vector((0, 0, 1)))
        if b.parent:
            eb.parent = arm.edit_bones[b.parent]
            eb.use_connect = (eb.parent.tail - eb.head).length < 1e-4
    bpy.ops.object.mode_set(mode="OBJECT")
    return obj


def segments(arm_obj):
    """{bone: (head, tail)} in armature space (rest)."""
    return {b.name: (b.head_local.copy(), b.tail_local.copy()) for b in arm_obj.data.bones}


def euler(r):
    return (Matrix.Rotation(math.radians(r[2]), 3, "Z") @ Matrix.Rotation(math.radians(r[1]), 3, "Y")
            @ Matrix.Rotation(math.radians(r[0]), 3, "X"))


def pose(arm_obj, spec, frame=None, prefix=""):
    """Applies `spec` = {bone: (rx, ry, rz)} plus optional "root_loc": (x, y, z) (armature
    space) and "world": {bone: (rx, ry, rz)} (see below), and keys it at `frame`. Bone names in `spec` get `prefix`; bones of the armature
    without the prefix are left alone, unlisted bones with it return to rest."""
    for pb in arm_obj.pose.bones:
        if not pb.name.startswith(prefix):
            continue
        key = pb.name[len(prefix):]
        rest = pb.bone.matrix_local.to_3x3()
        r = spec.get(key)
        q = Quaternion() if r is None else (rest.inverted() @ euler(r) @ rest).to_quaternion()
        pb.rotation_mode = "QUATERNION"
        pb.rotation_quaternion = q
        pb.location = (0, 0, 0)
        if key == "root" and "root_loc" in spec:
            pb.location = rest.inverted() @ Vector(spec["root_loc"])
        if frame is not None and key not in spec.get("world", {}):
            pb.keyframe_insert("rotation_quaternion", frame=frame, group=pb.name)
            pb.keyframe_insert("location", frame=frame, group=pb.name)
    bpy.context.view_layer.update()
    # "world": {bone: (rx, ry, rz)}: the bone's armature-space orientation = that rotation
    # applied to its rest orientation, whatever its parents do (a socket that keeps a
    # musket upright or aimed while the hand moves).
    for key, r in spec.get("world", {}).items():
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


def add_clip(arm_obj, name, keys, prefix_keys=None):
    """Creates the action `name` from [(frame, spec)] (and for a combined rig
    `prefix_keys` = {prefix: [(frame, spec)]}); keeps it with a fake user. Returns it."""
    arm_obj.animation_data_create()
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    arm_obj.animation_data.action = act
    groups = prefix_keys or {"": keys}
    for prefix, ks in groups.items():
        for f, spec in ks:
            pose(arm_obj, spec, frame=f, prefix=prefix)
    for fc in _fcurves(act):
        for kp in fc.keyframe_points:
            kp.interpolation = "BEZIER"
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
