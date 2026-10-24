"""Cycles proof renders of high-detail gear (m2-hd-musket): imports a glb, hides the
authored levels (`extras.acr_lod`), and renders named views on a light grey studio.

blender -b --factory-startup -P hd_gear_sheet.py -- <glb> <out dir> <mode> [clip ...]
mode `kit`: the laid-out kit from above and the musket in 4 views plus the lock and the butt.
mode `man`: the unit in each named clip from the front-left, full figure: clip frame 0, or
`CLIP:FRAME`. Poses go through `acr_assets.posecheck` (export pose, then only that clip; checked
against `<name>.poses.json` beside the glb). FIRE_AIMED, FIRE_HIP and ATTACK1 present from
frame 0 to the shot; their middle is the reload, not the aim."""

import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))
from acr_assets import posecheck  # noqa: E402

argv = sys.argv[sys.argv.index("--") + 1:]
glb, out, mode = argv[0], argv[1], argv[2]
clips = argv[3:]
os.makedirs(out, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
posecheck.import_glb(glb)
for ob in bpy.data.objects:
    if "acr_lod" in ob.keys():
        ob.hide_render = True
sc = bpy.context.scene
sc.render.engine = "CYCLES"
try:
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = "HIP"
    prefs.get_devices()
    for d in prefs.devices:
        d.use = True
    sc.cycles.device = "GPU"
except Exception:
    pass
sc.cycles.samples = 48
sc.cycles.use_denoising = True
sc.render.resolution_x, sc.render.resolution_y = 1200, 800
w = bpy.data.worlds.new("w")
sc.world = w
w.use_nodes = True
w.node_tree.nodes["Background"].inputs[0].default_value = (0.8, 0.8, 0.82, 1)
w.node_tree.nodes["Background"].inputs[1].default_value = 0.6
for rot, e in (((50, 0, 30), 3.0), ((60, 0, 200), 1.2)):
    s = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    s.data.energy = e
    s.data.angle = math.radians(8)
    s.rotation_euler = [math.radians(a) for a in rot]
    sc.collection.objects.link(s)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
sc.collection.objects.link(cam)
sc.camera = cam


def shot(name, target, direction, size, ortho=True, up=(0.0, 0.0, 1.0)):
    cam.data.type = "ORTHO" if ortho else "PERSP"
    cam.data.ortho_scale = size
    d = Vector(direction).normalized()
    cam.location = Vector(target) + d * 6.0
    u = Vector(up)
    if abs(u.normalized().dot(d)) > 0.99:
        u = Vector((0.0, 1.0, 0.0))
    x = u.cross(d).normalized()
    y = d.cross(x)
    from mathutils import Matrix
    cam.rotation_euler = Matrix((x, y, d)).transposed().to_euler()
    cam.data.clip_end = 20
    sc.render.filepath = os.path.join(out, name + ".png")
    bpy.ops.render.render(write_still=True)


def world_of(suffix):
    for ob in bpy.data.objects:
        if ob.type == "MESH" and ob.name.endswith(suffix) and not ob.hide_render:
            return ob
    raise KeyError(suffix)


if mode == "kit":
    gun = world_of(".musket")
    mw = gun.matrix_world
    c = mw @ Vector((0, 0, 0.44))
    ax = (mw.to_3x3() @ Vector((0, 0, 1))).normalized()
    up = (mw.to_3x3() @ Vector((0, 1, 0))).normalized()
    side = ax.cross(up)
    shot("kit_layout", (0.2, -0.1, 0.0), (0.0, -0.35, 1.0), 2.0, up=(0.0, 1.0, 0.0))
    keep = {gun.name} | {c.name for c in gun.children_recursive}
    for ob in bpy.data.objects:
        if ob.type == "MESH" and ob.name not in keep:
            ob.hide_render = True
    shot("musket_right", c, side, 1.6, up=up)  # the lock side (-X)
    shot("musket_left", c, -side, 1.6, up=up)
    shot("musket_top", c, up, 1.6, up=-side)
    shot("musket_three_quarter", c, side + up * 0.6 - ax * 0.5, 1.6, up=up)
    lock = mw @ Vector((-0.03, 0.03, 0.15))
    shot("musket_lock", lock, side + up * 0.25, 0.24, up=up)
    butt = mw @ Vector((0.0, -0.05, -0.22))
    shot("musket_butt", butt, side - ax * 0.6 + up * 0.2, 0.30, up=up)
else:
    poser = posecheck.Poser(sc)
    poses = posecheck.poses_path(glb)
    for job in clips:
        clip, _, f = job.partition(":")
        f = int(f or 0)
        dumped = poses and str(f) in posecheck.load(poses)["clips"].get(clip, {})
        poser.play(clip, f, check=poses if dumped else None)
        clip = clip if not f else f"{clip}_{f}"
        shot(f"man_{clip}", (0.0, 0.0, 1.0), (-0.7, -1.0, 0.25), 2.3)
        shot(f"man_{clip}_hands", (0.0, -0.2, 1.3), (-0.7, -1.0, 0.25), 0.9)
        shot(f"man_{clip}_side", (0.0, 0.0, 1.1), (1.0, -0.15, 0.1), 2.3)
