"""HD garments prototype: the EngM18 outfit draped on the HD master (Blender 5.2, headless).

blender -b <hd.blend> --factory-startup --python hd_garments.py -- <acr lib dir> <out.blend> [spacing_cm]

Uses the pipeline's drafts and drape (`garments`, `drape.dress`) with HD settings patched
in for this run only (nothing in acr_assets changes):
- panels meshed at `spacing_cm` (default 1.0 cm) instead of 2-5 cm;
- the measurements (`drape.Body`) taken from the HD body instead of the low hull;
- every layer simulated (the shirt too: a full linen shirt, not just wristbands) until its
  cloth is at rest (mean vertex speed under `REST_MM_S`), not a fixed 75 frames;
- collision distances small and growing outwards; the body's collision shell 1 mm;
- real thickness per layer (solidify outwards before the next layer is draped on it);
- weights from the surface beneath (`drape.transfer_weights` with the layers under it);
- the skin under the clothes pushed in (`drape.hide_skin`); an untouched copy `hd.flesh`
  stays (hidden) for the measurements.
Order: shirt, breeches (over the shirt tails), gaiters, waistcoat, coat.
Writes <out>.stats.json: per garment sew/weld/seam gaps, sim frames, end speed and energy,
triangles and edge lengths.
"""
import json
import os
import sys
import time

import bpy
import numpy as np

argv = sys.argv[sys.argv.index("--") + 1:]
LIB, OUT = argv[0], argv[1]
SPACE = float(argv[2]) if len(argv) > 2 else 1.0
sys.path.insert(0, LIB)
from acr_assets import body as bd  # noqa: E402
from acr_assets import drape  # noqa: E402
from acr_assets import garments as gm  # noqa: E402
from acr_assets import mbody  # noqa: E402

T0 = time.time()
REST_MM_S = 4.0  # mean vertex speed (mm/s) under which the cloth counts as settled
SEW = 40  # sewing frames (no gravity)
RAMP = 30  # last frames: air damping ramps up so the jitter of the contacts dies out (quasi-static end)
MIN_SETTLE, MAX_SETTLE = 130, 130  # frames of gravity
ORDER = ("hd_shirt", "breeches", "gaiters", "waistcoat", "coat")
# thickness (m): linen 0.5 mm, wool 1.5-2.5 mm; margin (m): cloth-side collision distance
HD_CLOTH = {"hd_shirt": (0.0005, 0.0015), "breeches": (0.0015, 0.0018), "gaiters": (0.0015, 0.0018),
            "waistcoat": (0.0015, 0.0022), "coat": (0.0025, 0.0028)}
COLL_OUTER = 0.001  # collision objects' outer shell (adds to the cloth's margin)

sc = bpy.context.scene
arm = next(o for o in sc.objects if o.type == "ARMATURE")
body = bpy.data.objects["hd.body"]
for o in sc.objects:
    if o.name == "hd.cage":
        bpy.data.objects.remove(o)
arm.data.pose_position = "REST"
if arm.animation_data:
    arm.animation_data.action = None
for pb in arm.pose.bones:
    pb.matrix_basis.identity()
sc.frame_set(1)

# home bone per skin vertex (hide_skin, split_parts read it)
names = {g.index: g.name for g in body.vertex_groups}
bones = sorted(names.values())
homes = []
for v in body.data.vertices:
    w = max(v.groups, key=lambda e: e.weight) if v.groups else None
    homes.append(bones.index(names[w.group]) if w else bones.index("hips"))
a = body.data.attributes.new("home", "INT", "POINT")
a.data.foreach_set("value", homes)
body["acr_home_bones"] = bones

# measuring copy of the untouched skin
flesh = body.copy()
flesh.data = body.data.copy()
flesh.name = flesh.data.name = "hd.flesh"
sc.collection.objects.link(flesh)
flesh.hide_render = True


class Ch:
    prefix = ""
    name = "hd"
    shoes = None
    stand = None

    def __init__(self):
        self.arm, self.flesh, self.build = arm, body, {"drape": list(ORDER)}
        self.layers, self.keys, self.symmetric = {"clothes": []}, {}, []

    def add(self, layer, obj, key, symmetric=False):
        self.layers[layer].append(obj)
        self.keys[obj.name] = key
        obj["acr_key"] = key
        return obj


# ---------------------------------------------------------------- patches (this run only)
co = np.zeros(len(body.data.vertices) * 3)
body.data.vertices.foreach_get("co", co)
HDV = co.reshape(-1, 3)
mbody.repose = lambda *a_, **k_: (HDV, [])  # Body measures the HD skin
for g in ORDER:
    drape.SPACING[g] = SPACE
gm.CLOTH["hd_shirt"] = dict(gm.CLOTH["shirt"], bending=0.15)
gm.EASE["hd_shirt"] = 24.0
for g, (t, mg) in HD_CLOTH.items():
    gm.CLOTH[g] = dict(gm.CLOTH[g], thickness=t, margin=mg)
drape.SOLIDIFY = True


def solidify(obj, thickness):
    """`drape.solidify` without even offset (it throws spikes metres long at the welded seam
    fans) and with the offset clamped to the local edge length."""
    mod = obj.modifiers.new("acr_solid", "SOLIDIFY")
    mod.thickness = thickness
    mod.offset = 1.0
    mod.use_rim = True
    mod.use_even_offset = False
    mod.thickness_clamp = 1.0
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    obj.modifiers.remove(mod)
    old = obj.data
    obj.data = me
    bpy.data.meshes.remove(old)


drape.solidify = solidify
bd.split_parts = lambda obj, *a_, **k_: obj  # one surface per garment on the master (parts come with the LODs)
_coll = drape._collision


def collision(obj, on=True, thickness=0.004):
    _coll(obj, on, COLL_OUTER)
    if on:
        obj.collision.thickness_inner = 0.002
        obj.collision.damping = 0.7
        obj.collision.cloth_friction = 8.0


drape._collision = collision
SIMLOG = {}


def simulate(obj, settings, sew=(), frames=None):
    """`drape.simulate` run until the cloth is at rest; logs speed and energy per frame."""
    mod = obj.modifiers.new("acr_cloth", "CLOTH")
    cs = mod.settings
    cs.quality = 8
    cs.mass = settings["mass"] * 0.5
    cs.tension_stiffness = cs.compression_stiffness = settings["tension"]
    cs.shear_stiffness = settings["tension"] * 0.4
    cs.bending_stiffness = settings["bending"] * 0.5
    cs.use_sewing_springs = True
    if obj.vertex_groups.get("acr_pin"):
        cs.vertex_group_mass = "acr_pin"
        cs.pin_stiffness = 1.0
    cs.air_damping = 10.0
    cs.tension_damping = cs.compression_damping = cs.shear_damping = 15.0
    cc = mod.collision_settings
    cc.distance_min = settings.get("margin", 0.002)
    cc.use_self_collision = bool(settings.get("self"))
    cc.self_distance_min = settings.get("self_gap", 0.002)
    cc.collision_quality = 4
    last = SEW + 8 + MAX_SETTLE
    mod.point_cache.frame_start = 1
    mod.point_cache.frame_end = last
    base = 'modifiers["acr_cloth"].settings.'
    s1 = SEW
    drape._key(obj, base + "effector_weights.gravity", [(1, 0.0), (s1, 0.0), (s1 + 8, 1.0)])
    drape._key(obj, base + "sewing_force_max", [(1, 5.0), (s1 // 3, drape.SEW_FORCE), (s1, drape.SEW_FORCE),
                                                 (s1 + 8, 15.0)])
    drape._key(obj, base + "air_damping", [(1, 10.0), (last - RAMP, 10.0), (last - 5, 120.0)])
    drape._key(obj, base + "shrink_min", [(1, 0.0), (s1, 0.0), (s1 + 8, settings["shrink"])])
    t0 = time.time()
    gaps, prev, speeds, f = [], None, [], 0
    n = len(obj.data.vertices)
    buf = np.zeros(n * 3)
    for f in range(1, last + 1):
        sc.frame_set(f)
        dg = bpy.context.evaluated_depsgraph_get()
        ev = obj.evaluated_get(dg).data
        if f == s1 and sew:
            gaps = [(ev.vertices[a_].co - ev.vertices[b_].co).length for a_, b_ in sew]
        if f >= s1 + 8:
            ev.vertices.foreach_get("co", buf)
            p = buf.reshape(-1, 3).copy()
            if prev is not None:
                v = np.linalg.norm(p - prev, axis=1) * sc.render.fps
                speeds.append((f, float(v.mean() * 1000), float(np.percentile(v, 99) * 1000),
                               float(0.5 * cs.mass * (v ** 2).sum())))
                if f >= last - 5 and all(s[1] < REST_MM_S for s in speeds[-4:]):
                    break
            prev = p
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    dt = time.time() - t0
    SIMLOG[obj.name] = {"frames": f, "sim_s": round(dt, 1), "verts": n,
                        "end_mean_mm_s": round(speeds[-1][1], 2), "end_p99_mm_s": round(speeds[-1][2], 1),
                        "end_kinetic_J": round(speeds[-1][3], 6), "peak_kinetic_J": round(max(s[3] for s in speeds), 4),
                        "curve_mm_s": [round(s[1], 1) for s in speeds[::10]]}
    print("SIM", obj.name, SIMLOG[obj.name], flush=True)
    obj.modifiers.remove(mod)
    obj.animation_data_clear()
    old_me = obj.data
    obj.data = me
    bpy.data.meshes.remove(old_me)
    sc.frame_set(1)
    return dt, gaps


drape.simulate = simulate

# ---------------------------------------------------------------- dress
ch = Ch()
report = {}
made = drape.dress(ch, garments=ORDER, report=report,
                   patterns={"hd_shirt": lambda m=gm.MEASURE: gm.tunic(m, "hd_shirt", "shirt", chest=24.0,
                                                                        waist=30.0, below=14.0, flare=2.0)})
t_dress = time.time() - T0
clothes = ch.layers["clothes"]
hidden = drape.hide_skin(ch, clothes, depth=0.012, reach=0.06)


# ---------------------------------------------------------------- materials (look only)
def wool(name, rgb, rough=0.92, sheen=0.35, bump=0.25, scale=900.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    bs = nt.nodes["Principled BSDF"]
    bs.inputs["Base Color"].default_value = (*rgb, 1)
    bs.inputs["Roughness"].default_value = rough
    bs.inputs["Sheen Weight"].default_value = sheen
    tx = nt.nodes.new("ShaderNodeTexNoise")
    tx.inputs["Scale"].default_value = scale
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = bump
    bp.inputs["Distance"].default_value = 0.0004
    nt.links.new(tx.outputs["Fac"], bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], bs.inputs["Normal"])
    return m


LOOK = {"shirt": wool("hd.linen", (0.86, 0.84, 0.78), 0.8, 0.1, 0.15, 1400.0),
        "waistcoat": wool("hd.red2", (0.42, 0.03, 0.03)),
        "breeches": wool("hd.white", (0.80, 0.77, 0.68)),
        "gaiters": wool("hd.black", (0.025, 0.025, 0.028), 0.75),
        "coat": wool("hd.red", (0.45, 0.025, 0.025)),
        "facings": wool("hd.facing", (0.08, 0.10, 0.30)),
        "lining": wool("hd.turnback", (0.80, 0.77, 0.68))}
for o in clothes:
    k = ch.keys[o.name]
    o.data.materials.clear()
    o.data.materials.append(LOOK.get(k, LOOK["coat"]))
    o.data.polygons.foreach_set("material_index", [0] * len(o.data.polygons))

# ---------------------------------------------------------------- numbers
stats = {"spacing_cm": SPACE, "skin_verts_hidden": hidden, "measure": report.get("measure"), "garments": {},
         "objects": {}}
for g in ORDER:
    r = report.get(g, {})
    stats["garments"][g] = {k: r.get(k) for k in ("sew", "welded", "gap_mm_p95", "gap_mm_max", "gap_mm_max_after_sewing",
                                                  "within_5mm", "worst_seams_mm", "verts_sim", "longest_edge_m")}
    stats["garments"][g]["sim"] = SIMLOG.get(f"hd.{g}.drape")
for o in clothes:
    me = o.data
    co = np.zeros(len(me.vertices) * 3)
    me.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    ev = np.zeros(len(me.edges) * 2, np.int32)
    me.edges.foreach_get("vertices", ev)
    el = np.linalg.norm(co[ev[0::2]] - co[ev[1::2]], axis=1)
    stats["objects"][o.name] = {"key": ch.keys[o.name], "tris": sum(len(p.vertices) - 2 for p in me.polygons),
                                "verts": len(me.vertices), "edge_mm_mean": round(float(el.mean()) * 1000, 1),
                                "edge_mm_p99": round(float(np.percentile(el, 99)) * 1000, 1)}
stats["dress_s"] = round(t_dress, 1)
stats["total_s"] = round(time.time() - T0, 1)
arm.data.pose_position = "POSE"
for a_ in bpy.data.actions:
    a_.use_fake_user = True
bpy.ops.wm.save_as_mainfile(filepath=OUT)
with open(os.path.splitext(OUT)[0] + ".stats.json", "w") as fh:
    json.dump(stats, fh, indent=1)
print("STATS", json.dumps({k: v for k, v in stats.items() if k != "measure"}))
