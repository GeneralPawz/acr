"""Cycles renders of the HD human: poses x views, and close-ups.

blender -b <hd.blend> --python hd_render.py -- <out dir> <jobs.json>
jobs: [{"name", "action" (or null = rest A-pose), "t" (0..1 of the clip range), "view"
("front"|"3q"|"back"|"side"|[az_deg, el_deg]), "focus" (null = whole body | bone name),
"dist", "res": [w, h]}]
"""
import json
import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "lib"))
from acr_assets import armature as am  # noqa: E402
from acr_assets import posecheck  # noqa: E402

argv = sys.argv[sys.argv.index("--") + 1:]
OUT, JOBS = argv[0], json.load(open(argv[1]))
os.makedirs(OUT, exist_ok=True)
sc = bpy.context.scene
arm = next(o for o in sc.objects if o.type == "ARMATURE")
body = bpy.data.objects["hd.body"]
# Every pose starts from the saved pose; only the asked clip applies (`posecheck`).
poser = posecheck.Poser(sc)

prefs = bpy.context.preferences.addons["cycles"].preferences
try:
    prefs.compute_device_type = "HIP"
    prefs.get_devices()
    for d in prefs.devices:
        d.use = True
    sc.cycles.device = "GPU"
except Exception:
    sc.cycles.device = "CPU"
sc.render.engine = "CYCLES"
sc.cycles.samples = 96
sc.cycles.use_denoising = True
sc.render.film_transparent = True
sc.view_settings.view_transform = "AgX"
sc.world = sc.world or bpy.data.worlds.new("w")
sc.world.use_nodes = True
sc.world.node_tree.nodes["Background"].inputs[0].default_value = (0.75, 0.77, 0.8, 1)
sc.world.node_tree.nodes["Background"].inputs[1].default_value = 0.55


def light(name, kind, loc, energy, size=1.0):
    ld = bpy.data.lights.new(name, kind)
    ld.energy = energy
    if kind == "AREA":
        ld.size = size
    ob = bpy.data.objects.new(name, ld)
    sc.collection.objects.link(ob)
    ob.location = loc
    tr = ob.constraints.new("TRACK_TO")
    tgt = bpy.data.objects.get("lt_target") or bpy.data.objects.new("lt_target", None)
    if tgt.name not in sc.collection.objects:
        sc.collection.objects.link(tgt)
    tgt.location = (0, 0, 1.0)
    tr.target = tgt
    return ob


light("key", "AREA", (-2.2, -3.0, 3.0), 900, 2.0)
light("fill", "AREA", (3.0, -2.0, 1.6), 250, 3.0)
light("rim", "AREA", (1.0, 3.0, 2.6), 500, 1.5)

cam_d = bpy.data.cameras.new("cam")
cam = bpy.data.objects.new("cam", cam_d)
sc.collection.objects.link(cam)
sc.camera = cam

# Relaxed fingers (not keyed by the clips); FIRE_AIMED closes them round the stock.
CURL = {"relaxed": (12, 22, 14, 8), "grip": (55, 70, 45, 30)}


def fingers(mode):
    c = CURL[mode]
    for pb in arm.pose.bones:
        n = pb.name
        for f in ("index", "middle", "ring", "pinky"):
            if n.startswith(f + "_0"):
                j = int(n[len(f) + 2]) - 1
                pb.rotation_mode = "XYZ"
                pb.rotation_euler = (math.radians(c[j]), 0, 0)
        if n.startswith("thumb_0"):
            j = int(n[7]) - 1
            pb.rotation_mode = "XYZ"
            pb.rotation_euler = (math.radians(c[3] * (0.6 if j else 0.3)), 0, 0)


def set_pose(action, t):
    if action is None:
        poser.reset()
        for pb in arm.pose.bones:
            pb.matrix_basis.identity()
        sc.frame_set(1)
        return
    f0, f1 = poser.clip_range(action)
    if t == "aim":  # the frame with the left hand furthest forward at chest height or above (levelled)
        best = None
        for f in range(0, f1 - f0 + 1):
            poser.play(action, f)
            p = arm.matrix_world @ arm.pose.bones["hand_l"].head
            z = -p.y if p.z > 1.25 else -9
            if best is None or z > best[0] + 1e-4:
                best = (z, f)
        poser.play(action, best[1])
        return
    poser.play(action, int(round((f1 - f0) * t)))


def keys_fingers(action):
    return action is not None and any('"index_01' in fc.data_path for fc in am._fcurves(bpy.data.actions[action]))


def bbox():
    dg = bpy.context.evaluated_depsgraph_get()
    ev = body.evaluated_get(dg)
    me = ev.to_mesh()
    import numpy as np
    co = np.zeros(len(me.vertices) * 3)
    me.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3) @ np.array(ev.matrix_world.to_3x3()).T + np.array(ev.matrix_world.translation)
    ev.to_mesh_clear()
    return Vector(co.min(0)), Vector(co.max(0))


VIEWS = {"front": (0, 4), "3q": (-40, 8), "back": (180, 6), "side": (-90, 4)}
for j in JOBS:
    set_pose(j.get("action"), j.get("t", 0.0))
    if not keys_fingers(j.get("action")):  # a clip with finger keys (the contact grip) keeps them
        fingers("grip" if j.get("action") in ("FIRE_AIMED", "ATTACK1", "ATTACK") else "relaxed")
    bpy.context.view_layer.update()
    w, h = j.get("res", (720, 1080))
    sc.render.resolution_x, sc.render.resolution_y = w, h
    az, el = VIEWS[j["view"]] if isinstance(j["view"], str) else j["view"]
    if j.get("focus"):
        pb = arm.pose.bones[j["focus"]]
        c = arm.matrix_world @ ((pb.head + pb.tail) * 0.5)
        dist = j.get("dist", 0.5)
        cam_d.lens = 85
    else:
        lo, hi = bbox()
        c = (lo + hi) * 0.5
        cam_d.lens = 70
        size = max(hi.z - lo.z, (hi.x - lo.x) * h / w, (hi.y - lo.y) * h / w) * 1.08
        dist = size / (36.0 / 70.0) * (1.0 if h >= w else w / h)
        cam_d.sensor_fit = "VERTICAL"
        dist = size / (cam_d.sensor_height / cam_d.lens) if False else size * 70.0 / 24.0
    cam_d.sensor_fit = "VERTICAL"
    cam_d.sensor_height = 24.0
    a, e = math.radians(az), math.radians(el)
    d = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
    cam.location = c + d * dist
    cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    sc.render.filepath = os.path.join(OUT, j["name"] + ".png")
    bpy.ops.render.render(write_still=True)
    print("RENDERED", j["name"], j.get("action"), sc.frame_current)
