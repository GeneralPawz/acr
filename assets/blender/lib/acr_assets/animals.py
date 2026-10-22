"""Animals from the bought WildMesh "100 MODELS" pack (#287), rigged and animated.

The pack is not in the repository (Standard licence: no redistribution, see
`assets/LICENSES.md`). Its extracted folders are found through `ACR_SOURCES` (the folder
that holds `wildmesh/<Name>/SK_<Name>.blend`; on the render VM `/home/claude/sources`).

An animal script gives a spec (`build(ctx, SPEC)`):

    blend   "wildmesh/BrownBear/SK_Bear.blend"
    height  target height of the body (metres, idle pose, ground to top; antlers excluded
            with `measure_skip`), or `length` (along the body)
    tris    triangle budget of all meshes together (decimated above it)
    clips   {".MD key": (source action, mode, sprite frames[, a, b])}

Clip modes, all keyed on sprite frames (clip frame i = sprite frame i, 24 fps as the
humans):
    loop    the source span a..b (fractions, default 0..1) spread over the frames so that the
            last frame leads into the first; root motion along the body is removed
    once    the span spread over the frames, ending on its last pose
    cycle   the span repeated at its own speed (10 sprite frames per second) over the frames
    real    the source at its own speed at 10 sprite frames per second (the original's
            tick), its last pose held to the end (deaths)
    pose    one frame: the source at fraction a (default 1 = its end)
    fall    authored death for rigs without one: from the source pose at a, the body rolls
            by b degrees about its length (90 onto its side, 180 on its back) and sinks to
            the ground within one second, then lies still
    lunge   authored attack for rigs without one: from the source pose at a, the body
            lunges b metres forward and back with the head lowered (a butt or a bite)

Other spec keys: `fbx` (instead of `blend`) with `anims` {action name: animation-only FBX
of the same skeleton}, `textures` {material name part or "*": image or (image, alpha clip)
or "#rrggbb"} for sources whose materials do not survive import, `head_bone`/`tail_bone`
and `neck_bones` for rigs with generic bone names, `lift` {clip: metres} to raise a clip
(the flying eagle).

Facing: the head is turned to -Y (south, the convention); the ground point is under the
body's centre, the lowest point of the idle pose on the ground.
"""

import math
import os

import bpy
from mathutils import Matrix, Vector

TICKS_PER_SECOND = 10


def sources():
    root = os.environ.get("ACR_SOURCES")
    if not root or not os.path.isdir(root):
        raise RuntimeError("ACR_SOURCES must name the folder with the extracted pack (wildmesh/<Name>/...)")
    return root


def _append(path):
    """Appends every object (with its meshes, materials, images) and every action of the
    .blend at `path`. Scripts in it are never run."""
    try:
        bpy.context.preferences.filepaths.use_scripts_auto_execute = False
    except Exception:
        pass
    with bpy.data.libraries.load(path, link=False) as (src, dst):
        dst.objects = list(src.objects)
        dst.actions = list(src.actions)
    coll = bpy.context.scene.collection
    for o in dst.objects:
        if o is not None and o.name not in coll.objects:
            coll.objects.link(o)
    return [o for o in dst.objects if o is not None], [a for a in dst.actions if a is not None]


def _import_fbx(path):
    """Imports an FBX; returns the new objects and actions."""
    objs0, acts0 = set(bpy.data.objects), set(bpy.data.actions)
    bpy.ops.import_scene.fbx(filepath=path)
    return [o for o in bpy.data.objects if o not in objs0], [a for a in bpy.data.actions if a not in acts0]


def _import_anim(path, name):
    """The action of an animation-only FBX (same skeleton), renamed `name`; its objects go."""
    objs, acts = _import_fbx(path)
    act = acts[0]
    act.name = name
    act.use_fake_user = True
    # Its own armature object's transform (FBX scale) does not apply to ours.
    for fc in _fcurves(act):
        if not fc.data_path.startswith("pose.bones"):
            _remove_fcurve(act, fc)
    for o in objs:
        bpy.data.objects.remove(o, do_unlink=True)
    for a in acts[1:]:
        bpy.data.actions.remove(a)
    return act


def _textures(meshes, rules):
    """Materials by hand (`rules`: {material name part or "*": (base colour, alpha clip)},
    paths below ACR_SOURCES): a Principled BSDF fed by the image; with alpha clip the image's
    alpha goes through a Round node, which the glTF exporter writes as alpha mask."""
    for m in meshes:
        for slot in m.material_slots:
            mat = slot.material
            if mat is None:
                continue
            rule = next((v for k, v in rules.items() if k != "*" and k.lower() in mat.name.lower()), rules.get("*"))
            if rule is None:
                continue
            base, clip = (rule, False) if isinstance(rule, str) else rule
            mat.use_nodes = True
            nt = mat.node_tree
            nt.nodes.clear()
            out = nt.nodes.new("ShaderNodeOutputMaterial")
            bsdf = nt.nodes.new("ShaderNodeBsdfPrincipled")
            nt.links.new(bsdf.outputs[0], out.inputs[0])
            if base.startswith("#"):
                rgb = [int(base[i:i + 2], 16) / 255 for i in (1, 3, 5)]
                bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
                continue
            tex = nt.nodes.new("ShaderNodeTexImage")
            tex.image = bpy.data.images.load(os.path.join(sources(), *base.split("/")), check_existing=True)
            nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
            if clip:
                rnd = nt.nodes.new("ShaderNodeMath")
                rnd.operation = "ROUND"
                nt.links.new(tex.outputs["Alpha"], rnd.inputs[0])
                nt.links.new(rnd.outputs[0], bsdf.inputs["Alpha"])


def _fix_images(folder):
    """Re-points images to the pack's Textures folder, keeps base colour only, and shrinks
    them to 1024 pixels."""
    tex = os.path.join(folder, "Textures")
    for im in bpy.data.images:
        if im.packed_file is None and not os.path.isfile(bpy.path.abspath(im.filepath)):
            cand = os.path.join(tex, os.path.basename(im.filepath.replace("\\", "/")))
            if os.path.isfile(cand):
                im.filepath = cand
                im.reload()
    for mat in bpy.data.materials:
        if not mat.node_tree:
            continue
        nodes = mat.node_tree.nodes
        bsdf = next((n for n in nodes if n.type == "BSDF_PRINCIPLED"), None)
        if bsdf is None:
            continue
        # Normal maps, roughness and the like: the renderer uses the base colour only.
        for name, inp in bsdf.inputs.items():
            if name != "Base Color" and name != "Alpha":
                for link in list(inp.links):
                    mat.node_tree.links.remove(link)
        for n in list(nodes):
            if n.type == "NORMAL_MAP" or (n.type == "TEX_IMAGE" and not any(lk.to_node == bsdf for o in n.outputs for lk in o.links)):
                nodes.remove(n)
        bsdf.inputs["Metallic"].default_value = 0.0
        bsdf.inputs["Roughness"].default_value = 0.9
    for im in bpy.data.images:
        if im.users == 0 or im.size[0] == 0:
            continue
        if max(im.size) > 1024:
            w, h = im.size
            k = 1024 / max(w, h)
            im.scale(max(1, int(w * k)), max(1, int(h * k)))
            im.pack()


def _jaguar(im):
    """Leopard fur to jaguar fur: a warmer, deeper gold ground and bolder rosettes (the dark
    marks grown by two pixels at 1024) with a dark dot inside each (light ground enclosed
    by marks on all four sides), the way a jaguar's coat differs from a leopard's."""
    import numpy as np

    w, h = im.size
    px = np.empty(w * h * 4, dtype=np.float32)
    im.pixels.foreach_get(px)
    px = px.reshape(h, w, 4)
    rgb = px[..., :3]
    lum = rgb @ np.array([0.299, 0.587, 0.114], dtype=np.float32)
    dark = lum < np.percentile(lum, 22)
    grown = dark.copy()
    for _ in range(2):
        g = grown.copy()
        g[1:] |= grown[:-1]
        g[:-1] |= grown[1:]
        g[:, 1:] |= grown[:, :-1]
        g[:, :-1] |= grown[:, 1:]
        grown = g
    # Ground: towards orange-gold where light and saturated enough to be fur.
    warm = np.array([1.08, 0.92, 0.70], dtype=np.float32)
    rgb[:] = np.clip(rgb * warm, 0.0, 1.0)
    ring = grown & ~dark
    rgb[ring] = rgb[ring] * 0.35
    # Dots inside the rosettes: light ground enclosed by dark marks (a pixel with dark
    # marks on all four sides within 5 pixels) gets a small dark spot at its centre.
    enclosed = ~grown

    def shift(m, sy, sx):
        out = np.zeros_like(m)
        out[max(-sy, 0):h - max(sy, 0), max(-sx, 0):w - max(sx, 0)] = m[max(sy, 0):h - max(-sy, 0), max(sx, 0):w - max(-sx, 0)]
        return out

    for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        near = np.zeros_like(grown)
        for k in range(1, 6):
            near |= shift(grown, dy * k, dx * k)
        enclosed &= near
    # Keep the centres: enclosed pixels whose 4-neighbours are enclosed too, thinned twice.
    core = enclosed.copy()
    for _ in range(2):
        c = core.copy()
        c[1:] &= core[:-1]
        c[:-1] &= core[1:]
        c[:, 1:] &= core[:, :-1]
        c[:, :-1] &= core[:, 1:]
        core = c
    rgb[core] = rgb[core] * 0.4
    px[..., :3] = rgb
    im.pixels.foreach_set(px.ravel())
    im.pack()


def _decimate(meshes, budget):
    tris = sum(sum(len(p.vertices) - 2 for p in m.data.polygons) for m in meshes)
    if tris <= budget:
        return tris
    ratio = budget / tris
    for m in meshes:
        mod = m.modifiers.new("decimate", "DECIMATE")
        mod.decimate_type = "COLLAPSE"
        mod.ratio = ratio
        mod.use_collapse_triangulate = True
        # Before the armature modifier: the decimated mesh keeps its weights.
        m.modifiers.move(len(m.modifiers) - 1, 0)
    return int(tris * ratio)


def _bounds(objs):
    dg = bpy.context.evaluated_depsgraph_get()
    lo, hi = Vector((1e9,) * 3), Vector((-1e9,) * 3)
    for o in objs:
        ev = o.evaluated_get(dg)
        me = ev.to_mesh()
        mw = ev.matrix_world
        for v in me.vertices:
            p = mw @ v.co
            lo = Vector(map(min, lo, p))
            hi = Vector(map(max, hi, p))
        ev.to_mesh_clear()
    return lo, hi


def _show(arm, action, frame):
    ad = arm.animation_data or arm.animation_data_create()
    ad.action = action
    if hasattr(ad, "action_slot") and action is not None and action.slots:
        ad.action_slot = action.slots[0]
    f = math.floor(frame)
    bpy.context.scene.frame_set(int(f), subframe=float(frame - f))
    bpy.context.view_layer.update()


def _sample(arm, action, frame):
    _show(arm, action, frame)
    return {pb.name: (pb.location.copy(), pb.rotation_quaternion.copy(), pb.scale.copy()) for pb in arm.pose.bones}


def _times(action, mode, n, a, b, src_fps):
    s, e = action.frame_range
    s0, e0 = s + (e - s) * a, s + (e - s) * b
    if mode == "pose":
        return [s + (e - s) * (1.0 if a is None else a)]
    if mode == "real":
        step = src_fps / TICKS_PER_SECOND
        return [min(s0 + i * step, e0) for i in range(n)]
    if mode == "loop":
        return [s0 + (e0 - s0) * i / n for i in range(n)]
    if mode == "cycle":
        step = src_fps / TICKS_PER_SECOND
        return [s0 + (i * step) % (e0 - s0) for i in range(n)]
    return [s0 + (e0 - s0) * i / max(n - 1, 1) for i in range(n)]


_LAST = {}
_NECK = []


def _fcurves(act):
    out = []
    for layer in act.layers:
        for strip in layer.strips:
            for bag in strip.channelbags:
                out += list(bag.fcurves)
    return out


def _remove_fcurve(act, fc):
    for layer in act.layers:
        for strip in layer.strips:
            for bag in strip.channelbags:
                if fc in list(bag.fcurves):
                    bag.fcurves.remove(fc)
                    return


def _top(arm):
    return [pb for pb in arm.pose.bones if pb.parent is None]


def _world_basis(arm, pb):
    """3x3 from a top-level bone's local pose space to world (with scale)."""
    return arm.matrix_world.to_3x3() @ pb.bone.matrix_local.to_3x3()


def _roll(arm, base, deg, drop):
    """Sets `base` and rolls the top-level bones by `deg` about world Y (the body's length:
    models face -Y) around their heads, moved down by `drop` metres."""
    _set(arm, base)
    q_world = Matrix.Rotation(math.radians(deg), 3, "Y")
    for pb in _top(arm):
        m = _world_basis(arm, pb)
        mn = m.normalized()
        pb.rotation_quaternion = ((mn.inverted() @ q_world @ mn) @ pb.rotation_quaternion.to_matrix()).to_quaternion()
        pb.location = pb.location + m.inverted() @ Vector((0.0, 0.0, -drop))
    bpy.context.view_layer.update()


def _set(arm, smp):
    for pb in arm.pose.bones:
        pb.location, pb.rotation_quaternion, pb.scale = smp[pb.name]
    bpy.context.view_layer.update()


def _read(arm):
    return {pb.name: (pb.location.copy(), pb.rotation_quaternion.copy(), pb.scale.copy()) for pb in arm.pose.bones}


def _fall(arm, action, n, a, deg, meshes):
    base = _sample(arm, action, action.frame_range[0] + (action.frame_range[1] - action.frame_range[0]) * a)
    arm.animation_data.action = None
    _roll(arm, base, deg, 0.0)
    lo, _hi = _bounds(meshes)
    drop = lo.z
    out = []
    k = TICKS_PER_SECOND
    for i in range(n):
        f = min(i / k, 1.0)
        f = f * f * (3 - 2 * f)
        _roll(arm, base, deg * f, drop * f)
        out.append(_read(arm))
    return out


def _lunge(arm, action, n, a, dist, meshes):
    base = _sample(arm, action, action.frame_range[0] + (action.frame_range[1] - action.frame_range[0]) * a)
    arm.animation_data.action = None
    neck = [arm.pose.bones[n] for n in _NECK] or [
        pb for pb in arm.pose.bones if any(t in pb.name.lower() for t in ("neck", "head"))]
    lateral = Vector((1.0, 0.0, 0.0))
    out = []
    for i in range(n):
        f = math.sin(math.pi * i / max(n - 1, 1)) ** 2
        _set(arm, base)
        for pb in _top(arm):
            pb.location = pb.location + _world_basis(arm, pb).inverted() @ Vector((0.0, -dist * f, -0.04 * f))
        bpy.context.view_layer.update()
        for pb in neck[:2]:
            m_arm = arm.matrix_world.to_3x3().normalized()
            axis = (m_arm.inverted() @ lateral).normalized()
            h = pb.matrix.to_translation()
            r = Matrix.Rotation(math.radians(-22 * f / len(neck[:2])), 4, axis)
            pb.matrix = Matrix.Translation(h) @ r @ Matrix.Translation(-h) @ pb.matrix
            bpy.context.view_layer.update()
        out.append(_read(arm))
    return out


def _bake(arm, name, action, mode, n, a=0.0, b=1.0, src_fps=30, meshes=()):
    if mode == "fall":
        samples = _fall(arm, action, n, a, b, meshes)
    elif mode == "lunge":
        samples = _lunge(arm, action, n, a, b, meshes)
    else:
        times = _times(action, mode, n, a, b, src_fps)
        samples = [_sample(arm, action, t) for t in times]
    if mode == "loop":
        # Root motion: the pose at the span's end minus its start, spread over the cycle.
        end = _sample(arm, action, action.frame_range[0] + (action.frame_range[1] - action.frame_range[0]) * b)
        for pb in arm.pose.bones:
            if pb.parent is not None and pb.parent.parent is not None:
                continue
            d = end[pb.name][0] - samples[0][pb.name][0]
            if d.length > 0.02 / max(arm.matrix_world.to_scale()[0], 1e-6):
                for i, smp in enumerate(samples):
                    loc, q, sc = smp[pb.name]
                    smp[pb.name] = (loc - d * (i / n), q, sc)
    return samples


def _locals(arm, mats):
    """Pose-bone (location, rotation, scale) giving the armature-space bone matrices
    `mats` (the armature object at the identity)."""
    out = {}
    for pb in arm.pose.bones:
        b = pb.bone
        if b.parent is None:
            local = b.matrix_local.inverted() @ mats[pb.name]
        else:
            rest = b.parent.matrix_local.inverted() @ b.matrix_local
            local = rest.inverted() @ mats[b.parent.name].inverted() @ mats[pb.name]
        loc, q, sc = local.decompose()
        out[pb.name] = (loc, q, sc)
    return out


def _flatten(arm, meshes, parent):
    """Bakes the armature's and the meshes' world transforms (the FBX origin's rotation,
    the 0.01 scale, the turn and size of the build) into the mesh vertices and the bone
    rest pose, so that the exported nodes are identities like the human units'. Returns
    the uniform scale: pose locations (bone space) must be multiplied by it."""
    bpy.context.view_layer.update()
    w = arm.matrix_world.copy()
    scale = w.to_scale()[0]
    bone_of = {m.name: m.parent_bone for m in meshes if m.parent_type == "BONE"}
    for m in meshes:
        mw = m.matrix_world.copy()
        m.parent = None
        m.parent_type = "OBJECT"
        m.data.transform(mw)
        m.matrix_world = Matrix.Identity(4)
    arm.parent = None
    arm.matrix_world = Matrix.Identity(4)
    arm.data.transform(w)
    arm.parent = parent
    arm.matrix_parent_inverse = Matrix.Identity(4)
    arm.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    for m in meshes:
        m.parent = arm
        m.matrix_parent_inverse = Matrix.Identity(4)
        m.matrix_basis = Matrix.Identity(4)
        if m.name in bone_of:
            # Rigid parts on a bone (the elk's antlers): skinned to it with full weight.
            g = m.vertex_groups.new(name=bone_of[m.name])
            g.add(list(range(len(m.data.vertices))), 1.0, "REPLACE")
            if not any(md.type == "ARMATURE" for md in m.modifiers):
                md = m.modifiers.new("armature", "ARMATURE")
                md.object = arm
    bpy.context.view_layer.update()
    return scale


def _keys(arm, name, samples):
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    _LAST[name] = samples[-1]
    ad = arm.animation_data
    ad.action = act
    for i, smp in enumerate(samples):
        for pb in arm.pose.bones:
            loc, q, sc = smp[pb.name]
            pb.location, pb.rotation_quaternion, pb.scale = loc, q, sc
            pb.keyframe_insert("location", frame=i, group=pb.name)
            pb.keyframe_insert("rotation_quaternion", frame=i, group=pb.name)
            pb.keyframe_insert("scale", frame=i, group=pb.name)
    ad.action = None
    return act


def _head_dir(arm, head_name=None, tail_name=None):
    """Unit vector (world, horizontal) from the pelvis to the head in the rest pose."""
    bones = arm.data.bones
    head = bones.get(head_name or "") or next((b for b in bones if b.name.lower().endswith("head")), None) or next(
        b for b in bones if "head" in b.name.lower())
    tail = bones.get(tail_name or "") or next((b for b in bones if "tail" in b.name.lower()), None) or next(
        (b for b in bones if "pelvis" in b.name.lower() or "hips" in b.name.lower()), bones[0])
    v = arm.matrix_world @ head.head_local - arm.matrix_world @ tail.head_local
    v.z = 0
    return v.normalized()


def build(ctx, spec):
    if "fbx" in spec:
        path = os.path.join(sources(), *spec["fbx"].split("/"))
        objs, actions = _import_fbx(path)
        for key, rel in spec.get("anims", {}).items():
            actions.append(_import_anim(os.path.join(sources(), *rel.split("/")), key))
    else:
        path = os.path.join(sources(), *spec["blend"].split("/"))
        objs, actions = _append(path)
    if "textures" in spec:
        _textures([o for o in objs if o.type == "MESH"], spec["textures"])
    _fix_images(os.path.dirname(path))
    if spec.get("recolour") == "jaguar":
        for im in bpy.data.images:
            if im.users and "body" in im.name.lower():
                _jaguar(im)
    arm = next(o for o in objs if o.type == "ARMATURE")
    # Keep the armature and its meshes; drop helper empties (bone ends) and stray objects.
    meshes = [o for o in objs if o.type == "MESH" and (o.parent == arm or any(
        m.type == "ARMATURE" and m.object == arm for m in o.modifiers))]
    # The pack's actions also key the armature object (FBX: rotation and scale, the same on
    # every frame): take the object's transform from the STAND source, then drop those
    # channels so that only the bones animate.
    stand_src = bpy.data.actions[spec["clips"]["STAND"][0]]
    _show(arm, stand_src, stand_src.frame_range[0])
    mw = arm.matrix_world.copy()
    for act in actions:
        for fc in _fcurves(act):
            if not fc.data_path.startswith("pose.bones"):
                _remove_fcurve(act, fc)
    keep = {arm, *meshes}
    # Meshes whose parent goes (the FBX root empty) keep their place.
    loose = {m: m.matrix_world.copy() for m in meshes if m.parent not in keep}
    for o in objs:
        if o not in keep:
            bpy.data.objects.remove(o, do_unlink=True)
    arm.parent = None
    arm.matrix_world = mw
    for m, m_w in loose.items():
        m.parent = None
        m.matrix_world = m_w
    arm.animation_data_create()
    # Parts named `<asset>.<part>` (convention), the original name kept as the part.
    arm.name = f"{ctx.name}.rig"
    for m in meshes:
        m.name = f"{ctx.name}.{m.name.replace('.', '_').lower()}"
        # The pack's vertex colours (black) would tint the texture (COLOR_0); one UV map.
        for attr in list(m.data.color_attributes):
            m.data.color_attributes.remove(attr)
        while len(m.data.uv_layers) > 1:
            m.data.uv_layers.remove(m.data.uv_layers[-1])
    ctx.stats["source_triangles"] = sum(sum(len(p.vertices) - 2 for p in m.data.polygons) for m in meshes)
    ctx.stats["triangles"] = _decimate(meshes, spec.get("tris", 12000))

    # Pose for measuring: the first frame of the STAND source.
    stand_src = bpy.data.actions[spec["clips"]["STAND"][0]]
    _show(arm, stand_src, stand_src.frame_range[0])
    # Turn the head to -Y around the armature origin.
    h = _head_dir(arm, spec.get("head_bone"), spec.get("tail_bone"))
    yaw = math.atan2(-1.0, 0.0) - math.atan2(h.y, h.x)
    wrap = bpy.data.objects.new(ctx.name + ".body", None)
    bpy.context.scene.collection.objects.link(wrap)
    wrap.parent = ctx.root
    arm.parent = wrap
    arm.matrix_parent_inverse = Matrix.Identity(4)
    arm.matrix_basis = mw
    wrap.rotation_euler = (0.0, 0.0, yaw)
    bpy.context.view_layer.update()
    skip = spec.get("measure_skip", ())
    body = [m for m in meshes if not any(s.lower() in m.name.lower() for s in skip)] or meshes
    lo, hi = _bounds(body)
    size = spec.get("height") and (hi.z - lo.z, spec["height"]) or (hi.y - lo.y, spec["length"])
    k = size[1] / size[0]
    wrap.scale = (k, k, k)
    bpy.context.view_layer.update()
    lo, hi = _bounds(meshes)
    centre = (lo + hi) / 2
    wrap.location = (wrap.location.x - centre.x, wrap.location.y - centre.y, wrap.location.z - lo.z)
    bpy.context.view_layer.update()
    lo, hi = _bounds(meshes)
    ctx.stats["size_m"] = [round(hi[i] - lo[i], 2) for i in range(3)]

    src_fps = spec.get("fps", 30)
    _NECK[:] = spec.get("neck_bones", [])
    baked = {}
    for key, clip in spec["clips"].items():
        src, mode, n = clip[0], clip[1], clip[2]
        a = clip[3] if len(clip) > 3 else (None if mode == "pose" else 0.0)
        b = clip[4] if len(clip) > 4 else 1.0
        baked[key] = _bake(arm, key, bpy.data.actions[src], mode, n, a, b, src_fps, meshes)
    # The lying corpse: DEATH's last pose, unless the spec names its own.
    for key in ("DEATHLIE1", "DEATHLIE2", "DEATHLIE3"):
        if key not in baked and "DEATH" in baked:
            baked[key] = [baked["DEATH"][-1]]
    # Every pose as world matrices of the bones, then back to bone
    # space against the flattened rest pose.
    arm.animation_data.action = None
    worlds = {}
    for key, samples in baked.items():
        worlds[key] = []
        for smp in samples:
            _set(arm, smp)
            up = Matrix.Translation((0.0, 0.0, spec.get("lift", {}).get(key, 0.0)))
            # Without the armature's own scale (it goes into the rest pose), with the bones'.
            unscale = Matrix.Scale(1.0 / arm.matrix_world.to_scale()[0], 4)
            worlds[key].append({pb.name: up @ arm.matrix_world @ pb.matrix @ unscale for pb in arm.pose.bones})
    for pb in arm.pose.bones:
        pb.location, pb.rotation_quaternion, pb.scale = Vector(), (1, 0, 0, 0), Vector((1, 1, 1))
    _flatten(arm, meshes, ctx.root)
    bpy.data.objects.remove(wrap, do_unlink=True)
    made = []
    for key, mats in worlds.items():
        made.append(_keys(arm, key, [_locals(arm, m) for m in mats]))
    for act in list(bpy.data.actions):
        if act not in made:
            bpy.data.actions.remove(act)
    arm["acr_clips"] = [a.name for a in made]
    _show(arm, bpy.data.actions["STAND"], 0)
    lo, hi = _bounds(meshes)
    ctx.stats["size_m_flat"] = [round(hi[i] - lo[i], 2) for i in range(3)]
    ctx.root["acr_animal"] = True
    ctx.stats["clips"] = {a.name: int(a.frame_range[1] - a.frame_range[0] + 1) for a in made}
