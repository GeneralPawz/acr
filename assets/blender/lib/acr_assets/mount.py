"""The WildMesh horse for the cavalry (#287, #275): the bought pack's `Horse` (mesh, textures,
rig with saddle, stirrup and rein bones, gaits) as the mount of a combined horse-and-rider
unit, instead of the procedural horse on rig R (`acr_assets.horse`).

The pack is not in the repository (see `acr_assets.animals`, which this reuses: appending
the .blend from `ACR_SOURCES`, base-colour textures, black vertex colours stripped, the FBX
transform baked out, clips sampled on sprite frames, the authored `fall`).

`load` builds the horse first (it is scaled to `WITHERS` and turned to face -Y like every
model), samples its clips as armature-space bone matrices and flattens it. The unit then
builds its rider on skeleton H and `join`s the two armatures: the rider's root hangs from the
horse's `saddle` bone, so he follows the back through every gait. `key_clip` keys a combined
clip: the horse's sampled frames and the rider's pose specs.

The pack's own saddle and bridle (leather, its reins on rein bones) stay; `saddlecloth`
adds a cloth in the regiment's colour under the saddle, cut from the horse's back.
"""

import math
import os

import bpy
from mathutils import Matrix, Vector

from . import animals as an

BLEND = "wildmesh/Horse/SK_Horse.blend"
WITHERS = 1.5  # metres, ground to the top of the withers in the idle pose
TRIS = 16000
SRC_FPS = 30
# Leg bones whose fallen pose is relaxed (bent a little) instead of the standing pose's.
# Fallen legs (#287): (bone, degrees, towards): each joint bends its bone towards the tail
# ("back") or the head ("front"), the way a dead horse's legs fold: knees and fetlocks
# back, hocks forward, the upper legs a little.
RELAX = [("upperarm_{s}", 12, "front"), ("hand_{s}", 45, "back"), ("fingers_01_{s}", 30, "back"),
         ("upperleg_{s}", 15, "front"), ("foot_{s}", 35, "front"), ("toes_01_{s}", 30, "back")]


class Horse:
    def __init__(self, arm, meshes, worlds, stats):
        self.arm = arm
        self.meshes = meshes
        self.worlds = worlds  # {clip: [{bone: armature-space matrix}]}
        self.stats = stats

    def head(self, bone):
        return self.arm.data.bones[bone].head_local.copy()

    def tail(self, bone):
        return self.arm.data.bones[bone].tail_local.copy()


def load(name, parent, clips, withers=WITHERS, tris=TRIS, coat=None):
    """The horse under `parent` with `clips` = {key: (source action, mode, frames[, a, b])}
    (modes of `acr_assets.animals`), its coat tinted by `coat` (`COAT`). Returns a `Horse`."""
    path = os.path.join(an.sources(), *BLEND.split("/"))
    objs, actions = an._append(path)
    an._fix_images(os.path.dirname(path))
    for im in bpy.data.images:
        for part, rgb in (coat or COAT).items():
            if im.users and part in im.name.lower():
                _tint(im, rgb)
    cards = _alpha_cards()
    arm = next(o for o in objs if o.type == "ARMATURE")
    meshes = [o for o in objs if o.type == "MESH" and (o.parent == arm or any(
        m.type == "ARMATURE" and m.object == arm for m in o.modifiers))]
    stand_src = bpy.data.actions[clips["STAND"][0]]
    an._show(arm, stand_src, stand_src.frame_range[0])
    mw = arm.matrix_world.copy()
    for act in actions:
        for fc in an._fcurves(act):
            if not fc.data_path.startswith("pose.bones"):
                an._remove_fcurve(act, fc)
    keep = {arm, *meshes}
    for o in objs:
        if o not in keep:
            bpy.data.objects.remove(o, do_unlink=True)
    arm.parent = None
    arm.matrix_world = mw
    arm.animation_data_create()
    arm.name = f"{name}.horse_rig"
    for m in meshes:
        m.name = f"{name}.horse_{m.name.replace('.', '_').lower()}"
        for attr in list(m.data.color_attributes):
            m.data.color_attributes.remove(attr)
        while len(m.data.uv_layers) > 1:
            m.data.uv_layers.remove(m.data.uv_layers[-1])
    stats = {"source_bones": len(arm.data.bones), "alpha_cards": cards}
    _prune(arm, meshes)
    stats["bones"] = len(arm.data.bones)
    stats["source_triangles"] = sum(sum(len(p.vertices) - 2 for p in m.data.polygons) for m in meshes)
    stats["triangles"] = an._decimate(meshes, tris)
    for m in meshes:
        _apply_decimate(m)

    an._show(arm, stand_src, stand_src.frame_range[0])
    h = an._head_dir(arm)
    yaw = math.atan2(-1.0, 0.0) - math.atan2(h.y, h.x)
    wrap = bpy.data.objects.new(name + ".horse_wrap", None)
    bpy.context.scene.collection.objects.link(wrap)
    wrap.parent = parent
    arm.parent = wrap
    arm.matrix_parent_inverse = Matrix.Identity(4)
    arm.matrix_basis = mw
    wrap.rotation_euler = (0.0, 0.0, yaw)
    bpy.context.view_layer.update()
    k = withers / _withers_height(arm, meshes)
    wrap.scale = (k, k, k)
    bpy.context.view_layer.update()
    lo, hi = an._bounds(meshes)
    # Centre: the saddle over the origin (the rider's position), the hooves on the ground.
    sad = arm.matrix_world @ arm.pose.bones["saddle"].head
    wrap.location = (wrap.location.x - sad.x, wrap.location.y - sad.y, wrap.location.z - lo.z)
    bpy.context.view_layer.update()
    lo, hi = an._bounds(meshes)
    stats["size_m"] = [round(hi[i] - lo[i], 2) for i in range(3)]
    stats["withers_m"] = round(_withers_height(arm, meshes), 2)

    # Ground covered by one source cycle of each gait (metres, its root motion), against
    # which the game's speed is matched (`cavalry.MOTIONS`).
    stats["stride_m"] = {}
    for src in sorted({c[0] for c in clips.values() if c[1] in ("loop", "cycles")}):
        act = bpy.data.actions[src]
        ends = []
        for t in act.frame_range:
            an._show(arm, act, t)
            ends.append(arm.matrix_world @ arm.pose.bones["pelvis"].head)
        stats["stride_m"][src] = [round((ends[1] - ends[0]).length, 2), round((act.frame_range[1] - act.frame_range[0]) / SRC_FPS, 2)]
    baked = {}
    for key, clip in clips.items():
        src, mode, n = clip[0], clip[1], clip[2]
        a = clip[3] if len(clip) > 3 else (None if mode == "pose" else 0.0)
        b = clip[4] if len(clip) > 4 else 1.0
        if mode == "cycles":
            baked[key] = _cycles(arm, bpy.data.actions[src], n, a, b)
        else:
            baked[key] = an._bake(arm, key, bpy.data.actions[src], mode, n, a, b, SRC_FPS, meshes)
        if mode == "fall":
            _relax(arm, baked[key])
    arm.animation_data.action = None
    worlds = {}
    for key, samples in baked.items():
        worlds[key] = []
        for smp in samples:
            an._set(arm, smp)
            # Without the armature's own scale (it goes into the rest pose), with the bones'.
            unscale = Matrix.Scale(1.0 / arm.matrix_world.to_scale()[0], 4)
            worlds[key].append({pb.name: arm.matrix_world @ pb.matrix @ unscale for pb in arm.pose.bones})
    for pb in arm.pose.bones:
        pb.location, pb.rotation_quaternion, pb.scale = Vector(), (1, 0, 0, 0), Vector((1, 1, 1))
    an._flatten(arm, meshes, parent)
    bpy.data.objects.remove(wrap, do_unlink=True)
    # The pack's own actions are sampled: the exporter must not write them.
    for act in actions:
        bpy.data.actions.remove(act)
    return Horse(arm, meshes, worlds, stats)


# The pack's horse is grey (its textures are a grey coat): tinted to a bay with a dark mane.
COAT = {"body": (0.62, 0.40, 0.24), "hair": (0.30, 0.21, 0.14)}


# Materials of the pack drawn as alpha-tested strand cards (mane, tail, forelock).
CARDS = ("hair",)


def _alpha_cards(parts=CARDS):
    """The pack's mane and tail are strand cards whose texture is 75 % transparent, but its
    material leaves the alpha unplugged, so they exported opaque: dark sheets instead of
    strands (#233). Plugs the image alpha through a Round node into the BSDF's alpha, which
    the exporter writes as `alphaMode: MASK` (cutoff 0.5). Returns the materials changed."""
    done = []
    for mat in sorted(bpy.data.materials, key=lambda m: m.name):
        if not mat.users or not mat.node_tree or not any(k in mat.name.lower() for k in parts):
            continue
        nt = mat.node_tree
        bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
        if bsdf is None or not bsdf.inputs["Base Color"].links:
            continue
        tex = bsdf.inputs["Base Color"].links[0].from_node
        if tex.type != "TEX_IMAGE":
            continue
        for link in list(bsdf.inputs["Alpha"].links):
            nt.links.remove(link)
        rnd = nt.nodes.new("ShaderNodeMath")
        rnd.operation = "ROUND"
        nt.links.new(tex.outputs["Alpha"], rnd.inputs[0])
        nt.links.new(rnd.outputs[0], bsdf.inputs["Alpha"])
        done.append(mat.name)
    return done


def _tint(im, rgb):
    import numpy as np

    w, h = im.size
    px = np.empty(w * h * 4, dtype=np.float32)
    im.pixels.foreach_get(px)
    px = px.reshape(h, w, 4)
    px[..., :3] = np.clip(px[..., :3] * np.array(rgb, dtype=np.float32), 0.0, 1.0)
    im.pixels.foreach_set(px.ravel())
    im.pack()


def _apply_decimate(m):
    """Bakes the decimation into the mesh (keeping its vertex groups): otherwise every
    sampled frame of the export decimates the full mesh again."""
    dec = [md for md in m.modifiers if md.type == "DECIMATE"]
    if not dec or (m.data.shape_keys and len(m.data.shape_keys.key_blocks)):
        return
    others = [md for md in m.modifiers if md.type != "DECIMATE"]
    for md in others:
        md.show_viewport = False
    bpy.context.view_layer.update()
    ev = m.evaluated_get(bpy.context.evaluated_depsgraph_get())
    me = bpy.data.meshes.new_from_object(ev, preserve_all_data_layers=True, depsgraph=bpy.context.evaluated_depsgraph_get())
    old = m.data
    m.data = me
    for md in dec:
        m.modifiers.remove(md)
    for md in others:
        md.show_viewport = True
    if old.users == 0:
        bpy.data.meshes.remove(old)
    bpy.context.view_layer.update()


KEEP = ("saddle", "stirrup_02_l", "stirrup_02_r", "reins_01_l", "reins_01_r")


def _prune(arm, meshes):
    """Drops the bones that move no vertex (ends, helpers): of the pack's 547 bones only the
    weighted ones, their parents and the saddle's sockets are needed, and every bone costs
    keys in every clip."""
    used = set()
    for m in meshes:
        idx = {g.index: g.name for g in m.vertex_groups}
        for v in m.data.vertices:
            for g in v.groups:
                if g.weight > 1e-4 and g.group in idx:
                    used.add(idx[g.group])
    used |= {b for b in KEEP if b in arm.data.bones}
    need = set()
    for b in arm.data.bones:
        if b.name in used:
            while b is not None and b.name not in need:
                need.add(b.name)
                b = b.parent
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    for eb in list(arm.data.edit_bones):
        if eb.name not in need:
            arm.data.edit_bones.remove(eb)
    bpy.ops.object.mode_set(mode="OBJECT")
    for m in meshes:
        for g in list(m.vertex_groups):
            if g.name not in need:
                m.vertex_groups.remove(g)


def _withers_height(arm, meshes):
    """Ground to the top of the body over the front of the back (spine_04), current pose."""
    dg = bpy.context.evaluated_depsgraph_get()
    pb = arm.pose.bones.get("spine_04") or arm.pose.bones.get("spine_03")
    p = arm.matrix_world @ pb.head
    scale = arm.matrix_world.to_scale()[0]
    near = 12.0 * scale  # 12 source units (cm) around the bone, horizontally
    top, lo = -1e9, 1e9
    for o in meshes:
        ev = o.evaluated_get(dg)
        me = ev.to_mesh()
        mw = ev.matrix_world
        body = {i for i, m in enumerate(o.data.materials) if m and "body" in m.name.lower()}
        vs = set()
        for poly in me.polygons:
            if poly.material_index in body:
                vs.update(poly.vertices)
        for i in vs:
            w = mw @ me.vertices[i].co
            lo = min(lo, w.z)
            if (w.x - p.x) ** 2 + (w.y - p.y) ** 2 < near * near:
                top = max(top, w.z)
        ev.to_mesh_clear()
    return top - lo


def _cycles(arm, action, n, p0, p1):
    """Mode `cycles`: the gait from phase `p0` to `p1` (in cycles of the source, may be
    more than one) over `n` frames, wrapped, its root motion removed; so a sprite cycle can
    hold several strides (the stride matched to the game's speed)."""
    s, e = action.frame_range
    start = an._sample(arm, action, s)
    end = an._sample(arm, action, e)
    top = [pb.name for pb in arm.pose.bones if pb.parent is None or pb.parent.parent is None]
    out = []
    for i in range(n):
        t = p0 + (p1 - p0) * i / n
        frac = t - math.floor(t)
        smp = an._sample(arm, action, s + (e - s) * frac)
        for name in top:
            d = end[name][0] - start[name][0]
            loc, q, sc = smp[name]
            smp[name] = (loc - d * frac, q, sc)
        out.append(smp)
    return out


def _relax(arm, samples):
    """Bends the fallen horse's legs over the fall (`RELAX`); the last frames hold it."""
    m3 = arm.matrix_world.to_3x3().inverted()
    back = (m3 @ Vector((0.0, 1.0, 0.0))).normalized()
    joints = [(j.format(s=sd), deg, back if to == "back" else -back) for j, deg, to in RELAX for sd in ("l", "r")]
    joints = [j for j in joints if j[0] in arm.pose.bones]
    for i, smp in enumerate(samples):
        f = min(i / an.TICKS_PER_SECOND, 1.0)
        f = f * f * (3 - 2 * f)
        an._set(arm, smp)
        for name, deg, to in joints:
            pb = arm.pose.bones[name]
            d = (pb.tail - pb.head).normalized()
            axis = d.cross(to)
            if axis.length < 1e-4:
                continue
            h = pb.head.copy()
            r = Matrix.Rotation(math.radians(deg * f), 4, axis.normalized())
            pb.matrix = Matrix.Translation(h) @ r @ Matrix.Translation(-h) @ pb.matrix
            bpy.context.view_layer.update()
        samples[i] = an._read(arm)


def join(rider_arm, horse, rider_root, socket="saddle"):
    """Moves the horse's bones into `rider_arm` (the unit's armature) and hangs
    `rider_root` from the horse's `socket` bone. The horse's meshes are skinned to it."""
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    horse.arm.select_set(True)
    rider_arm.select_set(True)
    bpy.context.view_layer.objects.active = rider_arm
    bpy.ops.object.join()
    bpy.context.view_layer.objects.active = rider_arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = rider_arm.data.edit_bones
    eb[rider_root].parent = eb[socket]
    eb[rider_root].use_connect = False
    bpy.ops.object.mode_set(mode="OBJECT")
    for m in horse.meshes:
        m.parent = rider_arm
        m.matrix_parent_inverse = Matrix.Identity(4)
        m.matrix_basis = Matrix.Identity(4)
        for md in m.modifiers:
            if md.type == "ARMATURE":
                md.object = rider_arm
    horse.arm = rider_arm
    bpy.context.view_layer.update()


def drop_faces(horse, materials):
    """Removes the faces of the pack's materials whose name contains one of `materials`
    (`saddle1`: the western saddle; `saddle2`: girth, stirrups, bridle and reins)."""
    import bmesh

    for m in horse.meshes:
        bad = {i for i, mat in enumerate(m.data.materials) if mat and any(k in mat.name.lower() for k in materials)}
        if not bad:
            continue
        bm = bmesh.new()
        bm.from_mesh(m.data)
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index in bad], context="FACES")
        loose = [v for v in bm.verts if not v.link_faces]
        bmesh.ops.delete(bm, geom=loose, context="VERTS")
        bm.to_mesh(m.data)
        bm.free()
        horse.stats["triangles"] = sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in horse.meshes)


def horse_bones(arm, prefix="rider."):
    return [pb for pb in arm.pose.bones if not pb.name.startswith(prefix)]


def set_horse(arm, mats, prefix="rider."):
    """Poses the horse's bones from armature-space matrices `mats` (one sampled frame)."""
    for pb in horse_bones(arm, prefix):
        b = pb.bone
        if b.parent is None:
            local = b.matrix_local.inverted() @ mats[pb.name]
        else:
            rest = b.parent.matrix_local.inverted() @ b.matrix_local
            local = rest.inverted() @ mats[b.parent.name].inverted() @ mats[pb.name]
        loc, q, sc = local.decompose()
        pb.rotation_mode = "QUATERNION"
        pb.location, pb.rotation_quaternion, pb.scale = loc, q, sc
    bpy.context.view_layer.update()


def key_clip(arm, name, horse_frames, rider_keys, pose, prefix="rider."):
    """The combined clip `name`: the horse keyed on every frame of `horse_frames` (sampled
    matrices), the rider's specs `rider_keys` = [(frame, spec)] posed by `pose(arm, spec,
    frame)` after the horse (his world-placed bones depend on the saddle) and keyed there."""
    arm.animation_data_create()
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    ad = arm.animation_data
    ad.action = act
    hb = horse_bones(arm, prefix)
    rider_at = dict(rider_keys)
    frames = sorted(set(range(len(horse_frames))) | set(rider_at))
    for f in frames:
        set_horse(arm, horse_frames[min(f, len(horse_frames) - 1)], prefix)
        if f < len(horse_frames):
            for pb in hb:
                pb.keyframe_insert("location", frame=f, group=pb.name)
                pb.keyframe_insert("rotation_quaternion", frame=f, group=pb.name)
                pb.keyframe_insert("scale", frame=f, group=pb.name)
        if f in rider_at:
            pose(arm, rider_at[f], f)
    from . import armature as am

    for fc in am._fcurves(act):
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    clips = list(arm.get("acr_clips", []))
    clips.append(name)
    arm["acr_clips"] = clips
    ad.action = None
    return act


def saddlecloth(horse, name, rgb, half_len=0.36, drop=0.42, offset=0.018):
    """A cloth under the saddle in the regiment's colour: the horse's body faces around the
    saddle (within `half_len` along the back, down to `drop` below the seat) copied and
    pushed out by `offset`, skinned like the hide under it."""
    import bmesh

    body = horse.meshes[0]
    sad = horse.head("saddle")
    bm = bmesh.new()
    bm.from_mesh(body.data)
    mats = body.data.materials
    keep = [f for f in bm.faces if mats[f.material_index] and "body" in mats[f.material_index].name.lower()
            and abs(f.calc_center_median().y - sad.y) < half_len * (1.0 - 0.35 * max(0.0, (sad.z - f.calc_center_median().z) / drop))
            and f.calc_center_median().z > sad.z - drop]
    gone = [f for f in bm.faces if f not in set(keep)]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    for v in bm.verts:
        v.co += v.normal * offset
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    for g in body.vertex_groups:
        obj.vertex_groups.new(name=g.name)
    obj.parent = body.parent
    md = obj.modifiers.new("armature", "ARMATURE")
    md.object = horse.arm
    me.materials.clear()
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.95
    me.materials.append(mat)
    return obj
