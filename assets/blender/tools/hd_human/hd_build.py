"""HD base human prototype (Blender 5.2, headless).

blender -b --factory-startup --python hd_build.py -- <mh data dir> <rig.blend> <maps dir> <portrait.png> <out.blend> <acr lib dir> [decimate_tris]

MakeHuman 1.x base mesh (CC0) at full resolution, young male targets, reposed onto skeleton H
(acr_assets.mbody.repose), finger bones refitted to MakeHuman's finger joints, heat weights,
Catmull-Clark level 1, one full-body UV set, 4k skin maps with the portrait baked in.
"""
import json
import math
import os
import sys

import bmesh
import bpy
import numpy as np

argv = sys.argv[sys.argv.index("--") + 1:]
MH, RIG, MAPS, PORTRAIT, OUT, LIB = argv[:6]
DECIMATE = int(argv[6]) if len(argv) > 6 else 0
sys.path.insert(0, LIB)
from acr_assets import mbody  # noqa: E402

stats = {}

# ---------------------------------------------------------------- MakeHuman base mesh
vs, vts, faces = [], [], []
g = None
with open(os.path.join(MH, "3dobjs", "base.obj")) as fh:
    for ln in fh:
        if ln.startswith("v "):
            vs.append([float(t) for t in ln.split()[1:4]])
        elif ln.startswith("vt "):
            vts.append([float(t) for t in ln.split()[1:3]])
        elif ln.startswith("g "):
            g = ln.split()[1]
        elif ln.startswith("f "):
            tok = [t.split("/") for t in ln.split()[1:]]
            faces.append((g, [int(t[0]) - 1 for t in tok], [int(t[1]) - 1 for t in tok]))
vs = np.array(vs)
for rel in ("macrodetails/caucasian-male-young.target",
            "macrodetails/universal-male-young-averagemuscle-averageweight.target"):
    with open(os.path.join(MH, "targets", rel)) as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            t = ln.split()
            vs[int(t[0])] += [float(t[1]), float(t[2]), float(t[3])]
V = np.stack([vs[:, 0], -vs[:, 2], vs[:, 1]], 1) * 0.1  # dm, y-up, +z front -> m, z-up, -y front
groups = {}
for gg, f, _t in faces:
    groups.setdefault(gg, set()).update(f)
J = {gg[6:]: V[sorted(s)].mean(0) for gg, s in groups.items() if gg.startswith("joint-")}
jn = sorted(J)

# ---------------------------------------------------------------- skeleton H with its clips
with bpy.data.libraries.load(RIG, link=False) as (src, dst):
    dst.objects = list(src.objects)
    dst.actions = list(src.actions)
arm = next(o for o in dst.objects if o and o.type == "ARMATURE" and o.name.endswith(".rig") and "." not in o.name[:-4])
bpy.context.scene.collection.objects.link(arm)
mw = arm.matrix_world.copy()
arm.parent = None
arm.matrix_world = mw
for o in dst.objects:
    if o is not None and o is not arm and o.users == 0:
        pass
stats["rig"] = arm.name
stats["actions"] = sorted(a.name for a in bpy.data.actions)
seg = {b.name: (b.head_local.copy(), b.tail_local.copy()) for b in arm.data.bones}

# Repose the whole mesh plus the joint centres onto H.
VJ = np.concatenate([V, np.array([J[k] for k in jn])])
mbody._cache["d"] = {"V": VJ, "J": J, "faces": []}
Q, _ = mbody.repose(seg)
QV, QJ = Q[:len(V)], dict(zip(jn, Q[len(V):]))

# ---------------------------------------------------------------- meshes with UVs
def make(name, keep):
    sel = [(f, t) for gg, f, t in faces if gg in keep]
    used = sorted({i for f, _t in sel for i in f})
    remap = {o: n for n, o in enumerate(used)}
    me = bpy.data.meshes.new(name)
    me.from_pydata([tuple(QV[i]) for i in used], [], [[remap[i] for i in f] for f, _t in sel])
    uv = me.uv_layers.new(name="UVMap")
    uvs = [vts[i] for _f, t in sel for i in t]
    uv.data.foreach_set("uv", np.array(uvs, np.float32).ravel())
    me.validate()
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    for p in me.polygons:
        p.use_smooth = True
    return ob


body = make("hd.body", {"body"})
eyes = make("hd.eyes", {"helper-l-eye", "helper-r-eye"})
stats["base_tris"] = sum(len(p.vertices) - 2 for p in body.data.polygons)
stats["base_quads"] = sum(1 for p in body.data.polygons if len(p.vertices) == 4)

# ---------------------------------------------------------------- refit fingers
bpy.context.view_layer.objects.active = arm
bpy.ops.object.mode_set(mode="EDIT")
eb = arm.data.edit_bones
for s, sfx in (("l", "_l"), ("r", "_r")):
    knuck = QJ[f"{s}-finger-5-1"] - QJ[f"{s}-finger-2-1"]
    for k, f in enumerate(("thumb", "index", "middle", "ring", "pinky"), start=1):
        for j in range(1, 4):
            b = eb.get(f"{f}_0{j}{sfx}")
            if b is None:
                continue
            b.use_connect = False
            b.head = QJ[f"{s}-finger-{k}-{j}"]
            b.tail = QJ[f"{s}-finger-{k}-{j + 1}"]
            d = np.array(b.tail - b.head)
            z = np.cross(d, knuck)  # back of the hand: bend about local X curls the finger
            if sfx == "_r":
                z = -z
            b.align_roll(z)
for b in eb:
    b.use_deform = b.name in mbody_bones if False else b.use_deform
bpy.ops.object.mode_set(mode="OBJECT")
BODY = ("hips", "spine", "spine1", "spine2", "neck", "head", "clavicle_l", "upperarm_l", "forearm_l",
        "hand_l", "thigh_l", "shin_l", "foot_l", "toe_l", "clavicle_r", "upperarm_r", "forearm_r", "hand_r",
        "thigh_r", "shin_r", "foot_r", "toe_r")
FING = ("thumb", "index", "middle", "ring", "pinky")
for b in arm.data.bones:
    b.use_deform = b.name in BODY or any(b.name.startswith(f) for f in FING)

# ---------------------------------------------------------------- weights
bpy.ops.object.select_all(action="DESELECT")
body.select_set(True)
arm.select_set(True)
bpy.context.view_layer.objects.active = arm
bpy.ops.object.parent_set(type="ARMATURE_AUTO")
empty = [vg.name for vg in body.vertex_groups if not any(vg.index == e.group for v in body.data.vertices[:1] for e in v.groups)]
bpy.context.view_layer.objects.active = body
bpy.ops.object.mode_set(mode="WEIGHT_PAINT")
bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
bpy.ops.object.vertex_group_normalize_all(group_select_mode="ALL", lock_active=False)
bpy.ops.object.mode_set(mode="OBJECT")
unweighted = sum(1 for v in body.data.vertices if not v.groups)
stats["unweighted_verts"] = unweighted

# Keep the unsubdivided, weighted cage for the LOD chain.
cage = body.copy()
cage.data = body.data.copy()
cage.name = "hd.cage"
bpy.context.scene.collection.objects.link(cage)
cage.hide_render = True

# ---------------------------------------------------------------- subdivide (high master)
m = body.modifiers.new("sub", "SUBSURF")
m.levels = m.render_levels = 1
m.uv_smooth = "PRESERVE_BOUNDARIES"
bpy.context.view_layer.objects.active = body
for mm in list(body.modifiers):
    if mm.type == "ARMATURE":
        body.modifiers.remove(mm)
bpy.ops.object.modifier_apply(modifier="sub")
if DECIMATE:
    d = body.modifiers.new("dec", "DECIMATE")
    d.ratio = DECIMATE / (2 * len(body.data.polygons))
    d.use_symmetry = True
    d.symmetry_axis = "X"
    bpy.ops.object.modifier_apply(modifier="dec")
am = body.modifiers.new("arm", "ARMATURE")
am.object = arm
for p in body.data.polygons:
    p.use_smooth = True
body.data.polygons.foreach_get  # noqa
stats["high_tris"] = sum(len(p.vertices) - 2 for p in body.data.polygons)

# eyes ride on the head bone
eyes.parent = arm
vg = eyes.vertex_groups.new(name="head")
vg.add(list(range(len(eyes.data.vertices))), 1.0, "REPLACE")
ae = eyes.modifiers.new("arm", "ARMATURE")
ae.object = arm
ae = cage.modifiers.get("Armature") or cage.modifiers.new("arm", "ARMATURE")
ae.object = arm

# ---------------------------------------------------------------- portrait projection
eye_z = 0.5 * (QJ["l-eye"][2] + QJ["r-eye"][2])
cx = 0.5 * (QJ["l-eye"][0] + QJ["r-eye"][0])
eye_y = 0.5 * (QJ["l-eye"][1] + QJ["r-eye"][1])
stats["eye"] = [round(float(cx), 4), round(float(eye_y), 4), round(float(eye_z), 4)]
BOX = (-0.11, 0.11, -0.16, 0.06)
CZ, AX, FADE = -0.035, (0.066, 0.098), 0.72


def proj(ob, mask=True):
    me = ob.data
    co = np.zeros(len(me.vertices) * 3)
    me.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    no = np.zeros(len(me.vertices) * 3)
    me.vertices.foreach_get("normal", no)
    no = no.reshape(-1, 3)
    u = (co[:, 0] - cx - BOX[0]) / (BOX[1] - BOX[0])
    v = (co[:, 2] - eye_z - BOX[2]) / (BOX[3] - BOX[2])
    uvl = me.uv_layers.new(name="proj")
    li = np.zeros(len(me.loops), np.int32)
    me.loops.foreach_get("vertex_index", li)
    uvl.data.foreach_set("uv", np.stack([u[li], v[li]], 1).astype(np.float32).ravel())
    me.uv_layers.active = me.uv_layers["UVMap"] if "UVMap" in me.uv_layers else uvl
    if "UVMap" in me.uv_layers:
        me.uv_layers["UVMap"].active_render = True
    if mask:
        r = np.hypot((co[:, 0] - cx) / AX[0], (co[:, 2] - eye_z - CZ) / AX[1])
        k = np.clip((1.0 - r) / (1.0 - FADE), 0, 1)
        k = k * k * (3 - 2 * k)
        front = np.clip((-no[:, 1] - 0.15) / 0.35, 0, 1) * (co[:, 1] < eye_y + 0.04)
        a = me.color_attributes.new("pmask", "FLOAT_COLOR", "POINT")
        w = (k * front).astype(np.float32)
        a.data.foreach_set("color", np.stack([w, w, w, np.ones_like(w)], 1).ravel())
        return co, u, v, w, li


co, pu, pv, pw, li = proj(body)
proj(eyes, mask=False)

# colour-match the portrait to the skin on a ring of the face (cheeks, forehead)
alb = bpy.data.images.load(os.path.join(MAPS, "albedo_4k.png"))
por = bpy.data.images.load(PORTRAIT)
A = np.zeros(alb.size[0] * alb.size[1] * 4, np.float32)
alb.pixels.foreach_get(A)
A = A.reshape(alb.size[1], alb.size[0], 4)
P_ = np.zeros(por.size[0] * por.size[1] * 4, np.float32)
por.pixels.foreach_get(P_)
P_ = P_.reshape(por.size[1], por.size[0], 4)
uvs = np.zeros(len(body.data.loops) * 2, np.float32)
body.data.uv_layers["UVMap"].data.foreach_get("uv", uvs)
uvs = uvs.reshape(-1, 2)
ring = np.where((pw[li] > 0.35) & (pw[li] < 0.9))[0]
sa = A[(uvs[ring, 1] * (alb.size[1] - 1)).astype(int), (uvs[ring, 0] * (alb.size[0] - 1)).astype(int), :3]
sp = P_[(pv[li][ring].clip(0, 1) * (por.size[1] - 1)).astype(int), (pu[li][ring].clip(0, 1) * (por.size[0] - 1)).astype(int), :3]
gain = np.median(sa, 0) / np.maximum(np.median(sp, 0), 1e-3)
stats["portrait_gain"] = [round(float(x), 3) for x in gain]
P_[..., :3] = np.clip(P_[..., :3] * gain, 0, 1)
por2 = bpy.data.images.new("portrait_matched", por.size[0], por.size[1])
por2.pixels.foreach_set(P_.ravel())
por2.pack()

# ---------------------------------------------------------------- bake the 4k albedo
sc = bpy.context.scene
sc.render.engine = "CYCLES"
prefs = bpy.context.preferences.addons["cycles"].preferences
for dev in ("HIP", "NONE"):
    try:
        prefs.compute_device_type = dev
        prefs.get_devices()
        for d_ in prefs.devices:
            d_.use = True
        break
    except Exception:
        continue
sc.cycles.device = "GPU" if prefs.compute_device_type != "NONE" else "CPU"
sc.cycles.samples = 1
bake = bpy.data.images.new("hd_albedo_4k", 4096, 4096)
mat = bpy.data.materials.new("bake")
mat.use_nodes = True
nt = mat.node_tree
nt.nodes.clear()
o = nt.nodes.new("ShaderNodeOutputMaterial")
em = nt.nodes.new("ShaderNodeEmission")
t1 = nt.nodes.new("ShaderNodeTexImage")
t1.image = alb
u1 = nt.nodes.new("ShaderNodeUVMap")
u1.uv_map = "UVMap"
t2 = nt.nodes.new("ShaderNodeTexImage")
t2.image = por2
t2.extension = "EXTEND"
u2 = nt.nodes.new("ShaderNodeUVMap")
u2.uv_map = "proj"
at = nt.nodes.new("ShaderNodeAttribute")
at.attribute_name = "pmask"
mx = nt.nodes.new("ShaderNodeMix")
mx.data_type = "RGBA"
nt.links.new(u1.outputs[0], t1.inputs[0])
nt.links.new(u2.outputs[0], t2.inputs[0])
nt.links.new(at.outputs["Fac"], mx.inputs["Factor"])
nt.links.new(t1.outputs[0], mx.inputs["A"])
nt.links.new(t2.outputs[0], mx.inputs["B"])
nt.links.new(mx.outputs["Result"], em.inputs[0])
nt.links.new(em.outputs[0], o.inputs[0])
tgt = nt.nodes.new("ShaderNodeTexImage")
tgt.image = bake
nt.nodes.active = tgt
body.data.materials.clear()
body.data.materials.append(mat)
bpy.ops.object.select_all(action="DESELECT")
body.select_set(True)
bpy.context.view_layer.objects.active = body
body.data.uv_layers["UVMap"].active = True
sc.cycles.bake_type = "EMIT"
sc.render.bake.margin = 16
bpy.ops.object.bake(type="EMIT")
bake.filepath_raw = os.path.join(os.path.dirname(OUT), "hd_albedo_4k.png")
bake.file_format = "PNG"
bake.save()

# ---------------------------------------------------------------- final skin material
def skin_mat(name, albedo_img, normal_img, rough_img, normal_strength=0.6):
    m_ = bpy.data.materials.new(name)
    m_.use_nodes = True
    nt_ = m_.node_tree
    bs = nt_.nodes["Principled BSDF"]
    uv_ = nt_.nodes.new("ShaderNodeUVMap")
    uv_.uv_map = "UVMap"
    ta = nt_.nodes.new("ShaderNodeTexImage")
    ta.image = albedo_img
    nt_.links.new(uv_.outputs[0], ta.inputs[0])
    nt_.links.new(ta.outputs[0], bs.inputs["Base Color"])
    if rough_img is not None:
        tr = nt_.nodes.new("ShaderNodeTexImage")
        tr.image = rough_img
        tr.image.colorspace_settings.name = "Non-Color"
        nt_.links.new(uv_.outputs[0], tr.inputs[0])
        nt_.links.new(tr.outputs[0], bs.inputs["Roughness"])
    if normal_img is not None:
        tn = nt_.nodes.new("ShaderNodeTexImage")
        tn.image = normal_img
        tn.image.colorspace_settings.name = "Non-Color"
        nm = nt_.nodes.new("ShaderNodeNormalMap")
        nm.uv_map = "UVMap"
        nm.inputs["Strength"].default_value = normal_strength
        nt_.links.new(uv_.outputs[0], tn.inputs[0])
        nt_.links.new(tn.outputs[0], nm.inputs["Color"])
        nt_.links.new(nm.outputs[0], bs.inputs["Normal"])
    bs.inputs["Subsurface Weight"].default_value = 0.12
    bs.inputs["Subsurface Radius"].default_value = (1.0, 0.35, 0.2)
    bs.inputs["Subsurface Scale"].default_value = 0.01
    return m_


nimg = bpy.data.images.load(os.path.join(MAPS, "normal_4k.png"))
rimg = bpy.data.images.load(os.path.join(MAPS, "rough_4k.png"))
skin = skin_mat("hd.skin", bake, nimg, rimg, 0.22)
body.data.materials.clear()
body.data.materials.append(skin)
em_ = bpy.data.materials.new("hd.eye")
em_.use_nodes = True
bs = em_.node_tree.nodes["Principled BSDF"]
te = em_.node_tree.nodes.new("ShaderNodeTexImage")
te.image = por2
te.extension = "EXTEND"
ue = em_.node_tree.nodes.new("ShaderNodeUVMap")
ue.uv_map = "proj"
em_.node_tree.links.new(ue.outputs[0], te.inputs[0])
em_.node_tree.links.new(te.outputs[0], bs.inputs["Base Color"])
bs.inputs["Roughness"].default_value = 0.08
eyes.data.materials.append(em_)
cage.data.materials.clear()
cage.data.materials.append(skin)

for a_ in bpy.data.actions:
    a_.use_fake_user = True
for im in bpy.data.images:
    if im.name not in ("Render Result", "Viewer Node") and not im.packed_file:
        im.pack()
stats["high_verts"] = len(body.data.vertices)
stats["cage_tris"] = sum(len(p.vertices) - 2 for p in cage.data.polygons)
stats["eye_tris"] = sum(len(p.vertices) - 2 for p in eyes.data.polygons)
# remove leftovers of the source unit
for ob in list(bpy.data.objects):
    if ob.name not in (body.name, eyes.name, cage.name, arm.name):
        bpy.data.objects.remove(ob)
bpy.ops.outliner.orphans_purge(do_recursive=True) if False else None
bpy.ops.wm.save_as_mainfile(filepath=OUT)
with open(os.path.splitext(OUT)[0] + ".stats.json", "w") as fh:
    json.dump(stats, fh, indent=1)
print("STATS", json.dumps(stats))
