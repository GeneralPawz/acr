"""Cycles close-ups of the right hand of a built skeleton-H model (`--blend`): open, a fist at
the joint limits, and the contact grip on its stock in STAND and FIRE_AIMED, from three
angles each (docs/assets-humans.md "Contact grip").

blender -b target/assets/units/hdman.blend --python assets/blender/tools/hand_closeups.py -- <out dir>
"""

import math
import os
import sys

import bpy
from mathutils import Quaternion, Vector

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))
from acr_assets import grips, limits  # noqa: E402

OUT = sys.argv[sys.argv.index("--") + 1]
os.makedirs(OUT, exist_ok=True)
sc = bpy.context.scene
arm = next(o for o in sc.objects if o.type == "ARMATURE")
for o in sc.objects:
    if o.type == "MESH" and ("acr_lod" in o or ".lod" in o.name.lower()):
        o.hide_render = True

sc.render.engine = "CYCLES"
try:
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = "HIP"
    prefs.get_devices()
    for d in prefs.devices:
        d.use = True
    sc.cycles.device = "GPU"
except Exception:
    sc.cycles.device = "CPU"
sc.cycles.samples = 48
sc.cycles.use_denoising = True
sc.view_settings.view_transform = "AgX"
sc.world = sc.world or bpy.data.worlds.new("w")
sc.world.use_nodes = True
bg = sc.world.node_tree.nodes["Background"]
bg.inputs[0].default_value = (0.92, 0.92, 0.92, 1)
bg.inputs[1].default_value = 0.6
for name, loc, e in (("key", (-1.5, -2.0, 2.5), 500), ("fill", (2.0, -1.0, 1.2), 150), ("rim", (0.5, 2.0, 2.0), 250)):
    ld = bpy.data.lights.new(name, "AREA")
    ld.energy, ld.size = e, 1.5
    ob = bpy.data.objects.new(name, ld)
    sc.collection.objects.link(ob)
    ob.location = loc
    ob.rotation_euler = (-Vector(loc)).to_track_quat("-Z", "Y").to_euler()
cam_d = bpy.data.cameras.new("cam")
cam = bpy.data.objects.new("cam", cam_d)
sc.collection.objects.link(cam)
sc.camera = cam
cam_d.lens = 85
sc.render.resolution_x = sc.render.resolution_y = 600

wood = bpy.data.materials.new("stock")
wood.use_nodes = True
wood.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.28, 0.14, 0.06, 1)


def shoot(tag, stock=False):
    bpy.context.view_layer.update()
    st = None
    if stock:
        d = arm.get("acr_stock", {}).get("r", 0.036)
        bpy.ops.mesh.primitive_cylinder_add(radius=d / 2, depth=0.30, vertices=48)
        st = bpy.context.active_object
        st.data.materials.append(wood)
        st.matrix_world = arm.matrix_world @ grips.hold_matrix(arm, "socket.hand_r")
    pb = arm.pose.bones["middle_01_r"]
    c = arm.matrix_world @ pb.head
    for v, (az, el) in zip("abc", ((80, 15), (-30, 30), (170, -20))):
        a, e = math.radians(az), math.radians(el)
        dv = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
        cam.location = c + dv * 0.45
        cam.rotation_euler = (-dv).to_track_quat("-Z", "Y").to_euler()
        sc.render.filepath = os.path.join(OUT, f"hand_{tag}_{v}.png")
        bpy.ops.render.render(write_still=True)
    if st is not None:
        bpy.data.objects.remove(st)


def clip(name, frame=None):
    ad = arm.animation_data or arm.animation_data_create()
    act = bpy.data.actions[name]
    ad.action = act
    if hasattr(ad, "action_slot") and act.slots:
        ad.action_slot = act.slots[0]
    f0, f1 = (int(x) for x in act.frame_range)
    sc.frame_set((f0 + f1) // 2 if frame is None else frame)


def hand_pose(fn):
    if arm.animation_data:
        arm.animation_data.action = None
    for pb in arm.pose.bones:
        key, _s = limits.key_of(pb.name)
        pb.rotation_mode = "QUATERNION"
        pb.rotation_quaternion = fn(pb.name, key) if key and key.startswith(("finger", "thumb")) else Quaternion()


hand_pose(lambda n, k: Quaternion())
shoot("open")
hand_pose(lambda n, k: Quaternion((1, 0, 0), math.radians(limits.limit(n)["x"][1] * (0.6 if k.startswith("thumb") else 0.95))))
shoot("fist")
clip("STAND")
shoot("grip_stand", stock=True)
clip("FIRE_AIMED")
shoot("grip_fire_aimed", stock=True)
print("ACR hand close-ups in", OUT)
