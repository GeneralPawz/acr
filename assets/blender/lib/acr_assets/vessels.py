"""Ships and guns from downloaded bases (#287; needs `bpy`).

A base model lives outside the repo under `$ACR_SOURCES/<source>/<name>/` (local builds:
`D:\\ACR-models\\extracted`, the render VM: `/home/claude/sources`); `assets/LICENSES.md`
lists each one. The helpers import it, strip its PBR maps to the base colour (the renderer
reads base colour only), shrink its textures, decimate it to a budget and rig it with a few
rigid bones: parts hang on bones, clips move the bones (rocking hull, filling sails, a
recoiling barrel), and a part is hidden by scaling its bone to nearly 0 with a step key
(intact hull to burnt wreck).
"""

import math
import os

import bmesh
import bpy
from mathutils import Euler, Matrix, Quaternion, Vector

HIDDEN = 0.001


def source(*parts):
    """A path below `$ACR_SOURCES`; fails clearly when the base is missing."""
    root = os.environ.get("ACR_SOURCES")
    if not root:
        raise SystemExit("ACR_SOURCES is not set (folder of extracted base models, see docs/assets.md)")
    p = os.path.join(root, *parts)
    if not os.path.exists(p):
        raise SystemExit(f"base model missing: {p}")
    return p


def _new_objects(before):
    return [o for o in bpy.context.scene.objects if o.name not in before]


def import_file(path):
    """Imports a glTF or FBX file; returns its mesh objects (world transforms applied)."""
    before = {o.name for o in bpy.context.scene.objects}
    if path.lower().endswith(".fbx"):
        bpy.ops.import_scene.fbx(filepath=path)
    else:
        bpy.ops.import_scene.gltf(filepath=path)
    new = _new_objects(before)
    meshes = [o for o in new if o.type == "MESH"]
    for o in meshes:
        mw = o.matrix_world.copy()
        o.parent = None
        o.matrix_world = mw
    bpy.ops.object.select_all(action="DESELECT")
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    for o in new:
        if o.type != "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    return meshes


def triangles(objs):
    return sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in objs)


def decimate(obj, target):
    """Collapse-decimates `obj` to about `target` triangles (no-op when below)."""
    n = triangles([obj])
    if n <= target:
        return n
    m = obj.modifiers.new("decimate", "DECIMATE")
    m.decimate_type = "COLLAPSE"
    m.ratio = target / n
    m.use_collapse_triangulate = True
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=m.name)
    return triangles([obj])


def join(objs, name):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    if len(objs) > 1:
        bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    o.name = name
    return o


def transform(objs, matrix):
    """Applies `matrix` (scale, turn, offset) to the meshes' data."""
    for o in objs:
        o.data.transform(matrix)
        o.data.update()


def base_colour_only(objs, max_size=1024, tint=None, keep_normal=False):
    """Every material keeps its base colour (texture or factor) and loses metal and
    roughness maps (roughness 0.85, metallic 0: the generated albedo has its shading baked
    in), and its normal map unless `keep_normal` (then the tangent Normal Map chain stays, its
    image linear: glTF normalTexture); textures are shrunk to `max_size` and go out as PNG."""
    seen = set()
    for o in objs:
        for slot in o.material_slots:
            mat = slot.material
            if mat is None or mat.name in seen or not mat.use_nodes:
                continue
            seen.add(mat.name)
            nt = mat.node_tree
            bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
            if bsdf is None:
                continue
            nmap = None
            if keep_normal and bsdf.inputs["Normal"].links:
                nmap = bsdf.inputs["Normal"].links[0].from_node
                if nmap.type != "NORMAL_MAP" or nmap.space != "TANGENT":
                    nmap = None
            for name in ("Normal", "Metallic", "Roughness", "Specular IOR Level", "Emission Color"):
                if name == "Normal" and nmap is not None:
                    continue
                inp = bsdf.inputs.get(name)
                if inp is not None:
                    for link in list(inp.links):
                        nt.links.remove(link)
            bsdf.inputs["Metallic"].default_value = 0.0
            bsdf.inputs["Roughness"].default_value = 0.85
            for n in list(nt.nodes):
                if n.type == "TEX_IMAGE" and not _feeds(n, bsdf):
                    nt.nodes.remove(n)
                elif n.type == "NORMAL_MAP" and n is not nmap:
                    nt.nodes.remove(n)
                elif n.type == "TEX_IMAGE" and nmap is not None and _feeds(n, nmap, 2):
                    n.image.colorspace_settings.name = "Non-Color"
            if tint is not None:
                _tint(nt, bsdf, tint)
    for img in bpy.data.images:
        if img.users == 0:
            continue
        if img.size[0] == 0 and img.filepath:
            img.reload()
            _ = img.pixels[0] if len(img.pixels) else None
        if img.size[0] == 0:
            continue
        w, h = img.size
        if max(w, h) > max_size:
            k = max_size / max(w, h)
            img.scale(max(1, int(w * k)), max(1, int(h * k)))
        img.file_format = "PNG"
        img.filepath_raw = os.path.splitext(img.filepath_raw)[0] + ".png"
        # Re-pack the pixels as PNG: the exporter writes a packed JPEG as it is, and the
        # renderer decodes PNG only.
        img.pack()


def _feeds(node, target, depth=6):
    """Whether `node` reaches `target` through its output links."""
    if depth == 0:
        return False
    for out in node.outputs:
        for link in out.links:
            if link.to_node == target or _feeds(link.to_node, target, depth - 1):
                return True
    return False


def _tint(nt, bsdf, rgb):
    """Multiplies the base colour by `rgb` (a factor the exporter writes as
    `baseColorFactor` when the texture goes straight in)."""
    inp = bsdf.inputs["Base Color"]
    if inp.links:
        src = inp.links[0].from_socket
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        mix.inputs["Factor"].default_value = 1.0
        nt.links.new(src, mix.inputs["A"])
        mix.inputs["B"].default_value = (*rgb, 1.0)
        nt.links.new(mix.outputs["Result"], inp)
    else:
        c = inp.default_value
        inp.default_value = (c[0] * rgb[0], c[1] * rgb[1], c[2] * rgb[2], 1.0)


def burnt_copy(obj, name, target, rgb=(0.030, 0.024, 0.019)):
    """A charred copy of `obj` (fewer triangles, darkened materials)."""
    dup = obj.copy()
    dup.data = obj.data.copy()
    dup.name = name
    bpy.context.scene.collection.objects.link(dup)
    for i, slot in enumerate(dup.material_slots):
        if slot.material is not None:
            m = slot.material.copy()
            m.name = slot.material.name + "_burnt"
            dup.material_slots[i].material = m
            bsdf = next((n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
            if bsdf is not None:
                bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
                for link in list(bsdf.inputs["Base Color"].links):
                    m.node_tree.links.remove(link)
    decimate(dup, target)
    return dup


def cut_below(obj, z):
    """Removes the part of `obj` below the plane at height `z` (a hull under the waterline:
    the game draws water as a tint on the ground, so nothing hides it)."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=(0, 0, z), plane_no=(0, 0, 1), clear_inner=True)
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def flat_material(name, rgb):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.9
    return m


# --- rig and clips ------------------------------------------------------------------------


def rig(root, name, bones):
    """An armature under `root` from [(bone, head, parent)]; every bone points up 0.5 m so
    its local axes are fixed: local X = +X, local Y = +Z, local Z = -Y."""
    arm = bpy.data.armatures.new(name)
    obj = bpy.data.objects.new(name, arm)
    bpy.context.scene.collection.objects.link(obj)
    obj.parent = root
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    for b, head, parent in bones:
        eb = arm.edit_bones.new(b)
        eb.head = Vector(head)
        eb.tail = Vector(head) + Vector((0, 0, 0.5))
        eb.roll = 0.0
        eb.use_deform = False
        if parent:
            eb.parent = arm.edit_bones[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return obj


def attach(obj, arm, bone):
    """Hangs `obj` rigidly on `bone`, keeping where it is."""
    mw = obj.matrix_world.copy()
    obj.parent = arm
    obj.parent_type = "BONE"
    obj.parent_bone = bone
    bpy.context.view_layer.update()
    obj.matrix_world = mw


_TO_LOCAL = Matrix(((1, 0, 0), (0, 0, 1), (0, -1, 0)))  # armature axes -> bone axes


def _local(rot=(0, 0, 0), loc=(0, 0, 0), scale=(1, 1, 1)):
    q = Euler([math.radians(a) for a in rot], "XYZ").to_quaternion()
    rq = (_TO_LOCAL @ q.to_matrix() @ _TO_LOCAL.inverted()).to_quaternion()
    lv = _TO_LOCAL @ Vector(loc)
    sx, sy, sz = scale
    return rq, lv, Vector((sx, sz, sy))


def clip(arm, name, keys, step=()):
    """Action `name` from [(frame, {bone: dict(rot=(deg xyz), loc=(m), scale=(xyz))})] in
    armature axes (relative to the rest pose); each bone is keyed only where it is named.
    Bones in `step` hold their scale keys (show/hide)."""
    arm.animation_data_create()
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    arm.animation_data.action = act
    for f, spec in keys:
        for b in spec:
            pb = arm.pose.bones[b]
            pb.rotation_mode = "QUATERNION"
            q, lv, sv = _local(**spec.get(b, {}))
            pb.rotation_quaternion, pb.location, pb.scale = q, lv, sv
            for path in ("rotation_quaternion", "location", "scale"):
                pb.keyframe_insert(path, frame=f, group=b)
    from .armature import _fcurves

    for fc in _fcurves(act):
        stepped = any(f'"{b}"' in fc.data_path for b in step) and fc.data_path.endswith("scale")
        for kp in fc.keyframe_points:
            kp.interpolation = "CONSTANT" if stepped else "BEZIER"
    arm.animation_data.action = None
    for pb in arm.pose.bones:
        pb.rotation_quaternion, pb.location, pb.scale = Quaternion(), Vector(), Vector((1, 1, 1))
    return act


def swell(period_frames, amp_roll, amp_pitch, heave, steps=8):
    """Keys of a gentle rocking cycle for the `hull` bone: [(frame, spec)]."""
    out = []
    for i in range(steps + 1):
        t = i / steps * 2 * math.pi
        out.append((round(i * period_frames / steps), {
            "hull": {"rot": (amp_pitch * math.sin(t + 1.1), amp_roll * math.sin(t), 0),
                     "loc": (0, 0, heave * math.sin(t + 0.6))}}))
    return out


def merge(*key_lists):
    """Merges [(frame, spec)] lists by frame."""
    by = {}
    for ks in key_lists:
        for f, spec in ks:
            by.setdefault(f, {}).update(spec)
    return sorted(by.items())
