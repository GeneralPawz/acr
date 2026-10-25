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

The pack's own saddle and bridle (leather, its reins on rein bones) stay, the reins' ends
keyed into the rider's fists (`follow_reins`), the hair cards swaying in the wind
(`hair_wind`), the mesh with authored levels (`lods`); `saddlecloth`
adds a cloth in the regiment's colour under the saddle, cut from the horse's back.
"""

import math
import os

import bpy
from mathutils import Matrix, Vector

from . import animals as an
from . import armature as am

BLEND = "wildmesh/Horse/SK_Horse.blend"
WITHERS = 1.5  # metres, ground to the top of the withers in the idle pose
# Triangles per material of the pack in the master (#233): high detail first. The hide keeps
# all of the pack's 28 k triangles (decimated to 9 k it showed facets over the barrel, neck and
# croup in the free camera); hair cards, bridle and reins keep about half. `LOD_PARTS` are the
# authored levels (`lods`) the game draws further out.
PARTS = {"body": 30000, "hair": 14000, "saddle2": 9000, "saddle1": 3200, "eye": 800}
TRIS = sum(PARTS.values())
LOD_PARTS = (
    {"body": 9000, "hair": 3600, "saddle2": 2000, "saddle1": 1200, "eye": 200},
    {"body": 3200, "hair": 1400, "saddle2": 600, "saddle1": 400, "eye": 60},
    {"body": 1100, "hair": 500, "saddle2": 160, "saddle1": 120, "eye": 24},
)
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


def load(name, parent, clips, withers=WITHERS, tris=TRIS, coat=None, parts=PARTS):
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
    stats["part_triangles"] = _decimate_parts(meshes, {k: int(v * tris / TRIS) for k, v in parts.items()})
    stats["triangles"] = sum(sum(len(p.vertices) - 2 for p in m.data.polygons) for m in meshes)

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


def _tris_by_material(m):
    out = {}
    for p in m.data.polygons:
        out[p.material_index] = out.get(p.material_index, 0) + len(p.vertices) - 2
    return out


def _decimate_parts(meshes, budget):
    """Collapses each material's faces to its own triangle count `budget` = {name part:
    triangles} (edit-mode decimate of the material's faces only, so the weights stay and
    the other parts are untouched). Returns {part: [source, kept]}."""
    out = {}
    for m in meshes:
        before = _tris_by_material(m)
        mats = m.data.materials
        for o in bpy.context.view_layer.objects:
            o.select_set(False)
        m.select_set(True)
        bpy.context.view_layer.objects.active = m
        for i, n in sorted(before.items()):
            mat = mats[i] if i < len(mats) else None
            key = next((k for k in budget if mat and k in mat.name.lower()), None)
            if key is None or n <= budget[key]:
                continue
            bpy.ops.object.mode_set(mode="EDIT")
            bpy.ops.mesh.select_all(action="DESELECT")
            m.active_material_index = i
            bpy.ops.object.material_slot_select()
            bpy.ops.mesh.decimate(ratio=budget[key] / n)
            bpy.ops.object.mode_set(mode="OBJECT")
        after = _tris_by_material(m)
        for i, n in sorted(before.items()):
            mat = mats[i] if i < len(mats) else None
            name = next((k for k in budget if mat and k in mat.name.lower()), mat.name if mat else str(i))
            out[name] = [n, after.get(i, 0)]
    return out


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
    for fc in am._fcurves(act):
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    clips = list(arm.get("acr_clips", []))
    clips.append(name)
    arm["acr_clips"] = clips
    ad.action = None
    return act


def saddlecloth(horse, name, rgb, half_len=0.36, drop=0.42, offset=0.018, rows=12, cols=9):
    """A cloth under the saddle in the regiment's colour: an even grid laid on the hide
    around the saddle (within `half_len` along the back, its corners rounded inwards, down to
    `drop` below the top of the back), `offset` off the hide and skinned like the hide under
    each grid point (barycentric weights of the face hit).

    It used to be the hide's own faces copied and pushed out: after the decimation a few
    large triangles chosen by their centres, so its edge was jagged, holes showed the coat
    between the faces and the pack's girth cut through it (#233 follow-up). Writes
    `saddlecloth` = {triangles, min_gap_mm, max_gap_mm} to the horse's stats."""
    import bmesh
    from mathutils import interpolate
    from mathutils.bvhtree import BVHTree

    body = horse.meshes[0]
    sad = horse.head("saddle")
    src = bmesh.new()
    src.from_mesh(body.data)
    mats = body.data.materials
    hide = {i for i, m in enumerate(mats) if m and "body" in m.name.lower()}
    bmesh.ops.delete(src, geom=[f for f in src.faces if f.material_index not in hide], context="FACES")
    bmesh.ops.triangulate(src, faces=src.faces[:])
    src.faces.ensure_lookup_table()
    src.normal_update()
    dl = src.verts.layers.deform.active
    tree = BVHTree.FromBMesh(src)
    top = tree.ray_cast(Vector((sad.x, sad.y, sad.z + 3.0)), Vector((0, 0, -1)))[0].z
    cz = top - 0.40

    def hit(y, side, theta):
        o = Vector((sad.x + side * math.sin(theta) * 3.0, y, cz + math.cos(theta) * 3.0))
        return tree.ray_cast(o, (Vector((sad.x, y, cz)) - o).normalized())

    def reach(y, side):
        # The angle round the barrel at which the hide falls to `drop` below the top.
        lo, hi = 0.0, math.radians(150)
        for _ in range(24):
            mid = (lo + hi) / 2
            loc = hit(y, side, mid)[0]
            if loc is not None and loc.z > top - drop:
                lo = mid
            else:
                hi = mid
        return lo

    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    groups = {}
    grid = {}
    for r in range(rows + 1):
        u = 2.0 * r / rows - 1.0
        for c in range(-cols, cols + 1):
            t = abs(c) / cols
            side = 1.0 if c >= 0 else -1.0
            # Corners rounded: the lower edge shorter along the back than the top.
            y = sad.y + u * half_len * (1.0 - 0.30 * t * t)
            theta = reach(y, side) * t
            loc, nrm, idx, _ = hit(y, side, theta)
            if loc is None:
                loc, nrm, idx, _ = tree.find_nearest(Vector((sad.x, y, top)))
            f = src.faces[idx]
            w = interpolate.poly_3d_calc([v.co for v in f.verts], loc)
            n = sum((v.normal * wi for v, wi in zip(f.verts, w)), Vector()).normalized()
            vert = bm.verts.new(loc + n * offset)
            acc = {}
            for v, wi in zip(f.verts, w):
                for g, gw in v[dl].items():
                    acc[g] = acc.get(g, 0.0) + wi * gw
            best = sorted(acc.items(), key=lambda kv: (-kv[1], kv[0]))[:4]
            tot = sum(gw for _, gw in best) or 1.0
            groups[vert] = [(g, gw / tot) for g, gw in best if gw > 1e-4]
            grid[r, c] = vert
    for r in range(rows):
        for c in range(-cols, cols):
            quad = [grid[r, c], grid[r, c + 1], grid[r + 1, c + 1], grid[r + 1, c]]
            face = bm.faces.new(quad)
            face.normal_update()
            mid = sum((v.co for v in quad), Vector()) / 4
            if face.normal.dot(mid - Vector((sad.x, mid.y, cz))) < 0:
                face.normal_flip()
    src.free()
    bm.verts.index_update()
    order = list(bm.verts)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    vgs = [obj.vertex_groups.new(name=g.name) for g in body.vertex_groups]
    for i, v in enumerate(order):
        for g, gw in groups[v]:
            vgs[g].add([i], gw, "REPLACE")
    obj.parent = body.parent
    md = obj.modifiers.new("armature", "ARMATURE")
    md.object = horse.arm
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.95
    me.materials.append(mat)
    gaps = [tree.find_nearest(v.co)[3] for v in me.vertices]
    horse.stats["saddlecloth"] = {"triangles": 2 * len(me.polygons), "min_gap_mm": round(min(gaps) * 1000, 1),
                                  "max_gap_mm": round(max(gaps) * 1000, 1)}
    return obj


# ----------------------------------------------------------------------------- reins, hair, LODs

# The pack's reins end in bones on the saddle (`reins_01_<side>`, its `inner` child the end in
# the fist, `posFix` a second grip bone); their middle hangs from the neck and the front from
# the head (the bit ring).
REIN_BONES = {sd: (f"reins_01_{sd}", f"reins_01_posFix_{sd}") for sd in ("l", "r")}
REIN_TAKE = (0.10, 0.22)  # right fist off its rein end (m): the left fist takes both reins
REIN_DROP = (0.35, 0.60)  # rider's root off his seat on the saddle (m): dropped (thrown, dead)
REIN_REACH = (0.25, 0.40)  # rein fist off its place in the seat (m): the rein dropped slack
REIN_SPLIT = 0.03  # both reins in the left fist: the right one this far to the right


def _smooth(x, a, b):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def _slack_points(arm, hide, ahead=0.10):
    """Where each dropped rein end lies: on the neck's crest `ahead` m in front of its rest
    end, 2 cm off the hide; as saddle-local points (rest), so they ride the back."""
    from mathutils.bvhtree import BVHTree

    me = hide.data
    mats = me.materials
    faces = [tuple(p.vertices) for p in me.polygons if mats[p.material_index] and "body" in mats[p.material_index].name.lower()]
    tree = BVHTree.FromPolygons([v.co for v in me.vertices], faces)
    sad = arm.data.bones["saddle"].matrix_local.inverted()
    out = {}
    for sd in ("l", "r"):
        e = arm.data.bones["reins_01_inner_" + sd].head_local
        o = Vector((e.x * 0.5, e.y - ahead, e.z + 1.0))
        hit = tree.ray_cast(o, Vector((0.0, 0.0, -1.0)))[0]
        p = (hit if hit is not None else Vector((e.x * 0.5, e.y - ahead, e.z - 0.05))) + Vector((0.0, 0.0, 0.02))
        out[sd] = sad @ p
    return out


def follow_reins(arm, clips, prefix="rider.", hide=None):
    """Keys the reins' ends on every frame of `clips` (#233): the left rein in the left fist,
    the right one in the right fist while it is on the reins, else in the left fist beside the
    left rein. A fist out of the reins' reach (`REIN_REACH` from its place in the seat: aiming,
    the mounted reload, a two-handed swing) or a rider out of the saddle (`REIN_DROP`: thrown,
    the corpses) drops the rein slack on the neck's crest (`_slack_points` on `hide`), never
    stretched to the hand. The pack's reins stayed on the saddle bones, missing the fists.
    Returns the fist miss over the held side-frames before and after, the frames dropped per
    clip and the longest span from the rein's middle (on the neck) to its end, before/after."""
    pb = arm.pose.bones
    if "reins_01_inner_l" not in pb or prefix + "socket.hand_l" not in pb:
        return {}
    gap = {"before": [], "after": []}
    span = {"before": 0.0, "after": 0.0}
    dropped = {}
    slack = _slack_points(arm, hide) if hide is not None else None

    def rest():
        # A channel a clip does not key plays at rest in the game (the exporter resets the
        # bones before it samples a clip), but keeps in Blender what the last clip shown left
        # there (STAND's rider 1.4 m off his seat after an ATTACK).
        for p in pb:
            p.location, p.scale = Vector(), Vector((1.0, 1.0, 1.0))
            p.rotation_quaternion, p.rotation_euler = (1.0, 0.0, 0.0, 0.0), (0.0, 0.0, 0.0)

    def middle(sd):
        b = pb.get("reins_02_" + sd)
        return b.head if b is not None else pb["head"].head

    rest()
    am.set_clip(arm, "STAND", 0)
    inv = pb["saddle"].matrix.inverted()
    seat = inv @ pb[prefix + "root"].head
    # The fists on the reins in the seat (saddle-local): the reach is measured from there.
    home = {sd: inv @ pb[prefix + "socket.hand_" + sd].head for sd in ("l", "r")}
    for name in clips:
        act = bpy.data.actions.get(name)
        if act is None:
            continue
        s, e = (int(round(x)) for x in act.frame_range)
        rest()
        for f in range(s, e + 1):
            am.set_clip(arm, name, f)
            sad = pb["saddle"].matrix.copy()
            fist = {sd: pb[prefix + "socket.hand_" + sd].head.copy() for sd in ("l", "r")}
            end = {sd: pb["reins_01_inner_" + sd].head.copy() for sd in ("l", "r")}
            for sd in ("l", "r"):
                span["before"] = max(span["before"], (end[sd] - middle(sd)).length)
            take = _smooth((fist["r"] - end["r"]).length, *REIN_TAKE)
            hand = {"l": fist["l"], "r": fist["r"].lerp(fist["l"] + Vector((-REIN_SPLIT, 0.0, 0.0)), take)}
            inv = sad.inverted()
            seated = 1.0 - _smooth((inv @ pb[prefix + "root"].head - seat).length, *REIN_DROP)
            near = {"l": home["l"], "r": home["r"].lerp(home["l"] + Vector((-REIN_SPLIT, 0.0, 0.0)), take)}
            want = {}
            held = []
            for sd in ("l", "r"):
                hold = seated * (1.0 - _smooth((inv @ hand[sd] - near[sd]).length, *REIN_REACH))
                drop = sad @ slack[sd] if slack else end[sd]
                want[sd] = drop.lerp(hand[sd], hold)
                if hold > 0.999:
                    gap["before"].append((hand[sd] - end[sd]).length)
                    held.append(sd)
                else:
                    dropped[name] = dropped.get(name, 0) + 1
                d = want[sd] - end[sd]
                for bone in REIN_BONES[sd]:
                    if bone not in pb:
                        continue
                    m = pb[bone].matrix.copy()
                    m.translation += d
                    pb[bone].matrix = m
                    pb[bone].keyframe_insert("location", frame=f, group=bone)
            bpy.context.view_layer.update()
            for sd in ("l", "r"):
                span["after"] = max(span["after"], (pb["reins_01_inner_" + sd].head - middle(sd)).length)
            for sd in held:
                gap["after"].append((hand[sd] - pb["reins_01_inner_" + sd].head).length)
        for fc in am._fcurves(act):
            for kp in fc.keyframe_points:
                kp.interpolation = "LINEAR"
    arm.animation_data.action = None
    rest()

    def summ(v):
        return {"mean_m": round(sum(v) / len(v), 4), "max_m": round(max(v), 4)} if v else {}

    return {"held": len(gap["before"]), "dropped": dropped, "before": summ(gap["before"]), "after": summ(gap["after"]),
            "span_max_m": {k: round(v, 3) for k, v in span.items()}}


def hair_wind(horse, reach=0.6, seed=233, cell=0.06):
    """`_WIND` on the mane, forelock and tail cards (docs/rendering-3d.md "Wind"): the branch
    swing `y` and the flutter `w` grow from 0 at the roots (on the hide) to the tips `reach`
    metres off it, a phase `z` per clump of cards; no trunk bend. The game sways them with the
    trees' wind on top of the clips' own tail and mane motion. Returns the hair vertices."""
    import random

    from mathutils.bvhtree import BVHTree

    body = horse.meshes[0]
    me = body.data
    mats = me.materials
    hide = [p for p in me.polygons if mats[p.material_index] and "body" in mats[p.material_index].name.lower()]
    hair = {i for p in me.polygons if mats[p.material_index] and any(k in mats[p.material_index].name.lower() for k in CARDS)
            for i in p.vertices}
    tree = BVHTree.FromPolygons([v.co for v in me.vertices], [tuple(p.vertices) for p in hide])
    active = me.color_attributes.active_color_name
    a = me.attributes.new("_wind", "FLOAT_COLOR", "POINT")
    # Not COLOR_0: that is the decay mask (the horse has none).
    me.color_attributes.active_color_name = active or "none"
    rnd = random.Random(seed)

    def key(co):
        return tuple(int(math.floor(c / cell)) for c in co)

    phases = {k: None for k in (key(me.vertices[i].co) for i in hair)}
    for k in sorted(phases):
        phases[k] = rnd.random()
    top = 0.0
    for i, v in enumerate(me.vertices):
        if i not in hair:
            a.data[i].color = (0.0, 0.0, 0.0, 0.0)
            continue
        t = min(tree.find_nearest(v.co)[3] / reach, 1.0)
        y = 0.8 * t ** 1.3
        top = max(top, y)
        a.data[i].color = (0.0, y, phases[key(v.co)], t)
    horse.stats["hair_wind"] = {"vertices": len(hair), "max_branch": round(top, 3)}
    return len(hair)


def lods(horse, levels=LOD_PARTS):
    """The authored levels of the horse's mesh (docs/rendering-3d.md "Authored levels"): copies
    collapsed per material to `levels` (weights, `_part` and `_wind` kept, the master's
    materials shared), as `extras.acr_lod` = 1.. / `acr_lod_of`. Returns the level objects."""
    m = horse.meshes[0]
    out = []
    for k, budget in enumerate(levels, 1):
        lo = m.copy()
        lo.data = m.data.copy()
        for col in m.users_collection:
            col.objects.link(lo)
        lo.name = f"{m.name}.lod{k}"
        _decimate_parts([lo], budget)
        lo["acr_lod"] = k
        lo["acr_lod_of"] = m.name
        lo["acr_lod_tris"] = sum(_tris_by_material(lo).values())
        out.append(lo)
    horse.stats["lod_triangles"] = [lo["acr_lod_tris"] for lo in out]
    return out
