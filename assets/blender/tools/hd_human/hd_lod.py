"""LOD chain demo: the unsubdivided cage decimated to 10k / 3k / 800 triangles, normals baked
from the high master (with its micro-detail normal map), rendered beside the high mesh.

blender -b <hd.blend> --python hd_lod.py -- <out dir>
"""
import json
import math
import os
import sys

import bpy
from mathutils import Vector

OUT = sys.argv[sys.argv.index("--") + 1]
os.makedirs(OUT, exist_ok=True)
sc = bpy.context.scene
high = bpy.data.objects["hd.body"]
cage = bpy.data.objects["hd.cage"]
eyes = bpy.data.objects["hd.eyes"]
arm = next(o for o in sc.objects if o.type == "ARMATURE")
if arm.animation_data:
    arm.animation_data.action = None
for pb in arm.pose.bones:
    pb.matrix_basis.identity()

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
skin = high.data.materials[0]
stats = {"high": sum(len(p.vertices) - 2 for p in high.data.polygons)}
lods = []
for tris, px in ((10000, 2048), (3000, 1024), (800, 512)):
    ob = cage.copy()
    ob.data = cage.data.copy()
    ob.name = f"hd.lod{tris}"
    ob.hide_render = False
    sc.collection.objects.link(ob)
    for m in list(ob.modifiers):
        ob.modifiers.remove(m)
    bpy.context.view_layer.objects.active = ob
    d = ob.modifiers.new("dec", "DECIMATE")
    d.ratio = tris / stats["high"] * stats["high"] / sum(len(p.vertices) - 2 for p in ob.data.polygons)
    d.use_symmetry = True
    bpy.ops.object.modifier_apply(modifier="dec")
    for p in ob.data.polygons:
        p.use_smooth = True
    stats[f"lod{tris}"] = sum(len(p.vertices) - 2 for p in ob.data.polygons)
    img = bpy.data.images.new(f"lod{tris}_normal", px, px)
    img.colorspace_settings.name = "Non-Color"
    m = skin.copy()
    m.name = f"skin.lod{tris}"
    nt = m.node_tree
    tgt = nt.nodes.new("ShaderNodeTexImage")
    tgt.image = img
    nt.nodes.active = tgt
    ob.data.materials.clear()
    ob.data.materials.append(m)
    bpy.ops.object.select_all(action="DESELECT")
    high.select_set(True)
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob
    sc.render.bake.use_selected_to_active = True
    sc.render.bake.cage_extrusion = 0.012
    sc.render.bake.max_ray_distance = 0.04
    sc.render.bake.margin = 8
    sc.cycles.samples = 1
    bpy.ops.object.bake(type="NORMAL")
    img.filepath_raw = os.path.join(OUT, f"lod{tris}_normal.png")
    img.file_format = "PNG"
    img.save()
    # use the baked map instead of the micro-detail map
    nm = next(n for n in nt.nodes if n.type == "NORMAL_MAP")
    src = nm.inputs["Color"].links[0].from_node
    src.image = img
    nm.inputs["Strength"].default_value = 1.0
    lods.append(ob)

# renders: each mesh alone, same camera
sc.cycles.samples = 64
sc.cycles.use_denoising = True
sc.render.film_transparent = True
sc.view_settings.view_transform = "AgX"
sc.world.use_nodes = True
sc.world.node_tree.nodes["Background"].inputs[0].default_value = (0.75, 0.77, 0.8, 1)
for name, kind, loc, e, s in (("key", "AREA", (-2.2, -3.0, 3.0), 900, 2.0), ("fill", "AREA", (3.0, -2.0, 1.6), 250, 3.0),
                              ("rim", "AREA", (1.0, 3.0, 2.6), 500, 1.5)):
    ld = bpy.data.lights.new(name, kind)
    ld.energy, ld.size = e, s
    lo = bpy.data.objects.new(name, ld)
    sc.collection.objects.link(lo)
    lo.location = loc
    lo.rotation_euler = (-(Vector((0, 0, 1.0)) - Vector(loc))).to_track_quat("Z", "Y").to_euler()
cd = bpy.data.cameras.new("cam")
cam = bpy.data.objects.new("cam", cd)
sc.collection.objects.link(cam)
sc.camera = cam
cd.lens, cd.sensor_fit, cd.sensor_height = 70, "VERTICAL", 24
a, e = math.radians(-35), math.radians(6)
dv = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
c = Vector((0, 0, 0.9))
cam.location = c + dv * (1.95 * 70 / 24)
cam.rotation_euler = (-dv).to_track_quat("-Z", "Y").to_euler()
sc.render.resolution_x, sc.render.resolution_y = 600, 900
meshes = [high] + lods
for ob in meshes:
    for o2 in meshes:
        o2.hide_render = o2 is not ob
    cage.hide_render = True
    sc.render.filepath = os.path.join(OUT, f"{ob.name}.png")
    bpy.ops.render.render(write_still=True)
    # head close-up
    sc.render.resolution_x = sc.render.resolution_y = 500
    hc = Vector((0, -0.05, 1.63))
    cam.location = hc + dv * 0.75
    sc.render.filepath = os.path.join(OUT, f"{ob.name}_head.png")
    bpy.ops.render.render(write_still=True)
    sc.render.resolution_x, sc.render.resolution_y = 600, 900
    cam.location = c + dv * (1.95 * 70 / 24)
with open(os.path.join(OUT, "lod_stats.json"), "w") as fh:
    json.dump(stats, fh)
print("LOD", json.dumps(stats))
