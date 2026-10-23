"""Contact sheet of the held gear library (`acr_assets.held`): every item in its item
frame, side by side with its name and triangle count and a pink marker on every socket,
rendered with an orthographic camera; one row of the long (pole) items or the short ones.

    blender -b --factory-startup --python assets/blender/tools/held_sheet.py -- --row short|long --out sheet.png
"""

import math
import os
import sys

import bpy
from mathutils import Euler, Vector

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import gear, held, registry  # noqa: E402

COLOURS = {"wood": (0.42, 0.24, 0.12), "metal": (0.62, 0.62, 0.66), "brass": (0.78, 0.62, 0.27),
           "facings": (0.62, 0.08, 0.07), "colour": (0.70, 0.12, 0.10)}


def material(key):
    m = bpy.data.materials.get(key)
    if m:
        return m
    m = bpy.data.materials.new(key)
    m.use_nodes = True
    rgb = COLOURS.get(key) or registry.FALLBACK.get(key, (0.5, 0.5, 0.5))
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Base Color"].default_value = (*[c ** 2.2 for c in rgb], 1.0)
    p.inputs["Roughness"].default_value = 0.35 if key in ("metal", "brass", "stone") else 0.75
    p.inputs["Metallic"].default_value = 0.8 if key in ("metal", "brass") else 0.0
    return m


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = argv[argv.index("--out") + 1] if "--out" in argv else "held_sheet.png"
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    # One row: the long items (`--row long`, over 1.2 m) or the short ones (`--row short`).
    row_kind = argv[argv.index("--row") + 1] if "--row" in argv else "short"

    def length(n):
        lo, hi = held.build(n).bounds()
        return hi[2] - lo[2]

    row = [n for n in held.ITEMS if (length(n) > 1.2) == (row_kind == "long")]
    step = 0.7 if row_kind == "long" else 0.42
    width = len(row) * step + (1.4 if row_kind == "long" else 0.0)
    x = -width / 2
    for n in row:
        it = held.build(n)
        obj = gear.held(it, n)
        for k in obj["acr_material_keys"]:
            obj.data.materials.append(material(k))
        lo, hi = it.bounds()
        w = max(step, hi[0] - lo[0] + 0.25)
        cx = x + w / 2
        x += w
        obj.location = Vector((cx - (lo[0] + hi[0]) / 2, 0.0, -lo[2] + 0.02))
        for s in obj.children:
            # A small pink marker on every socket.
            bpy.ops.mesh.primitive_uv_sphere_add(radius=0.012 if row_kind == "short" else 0.02, segments=8, ring_count=6)
            mk = bpy.context.object
            mk.data.materials.append(material("socket_marker"))
            mk.data.materials[0].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (1, 0, 0.6, 1)
            mk.parent = s
            mk.location = (0, 0, 0)
        bpy.ops.object.text_add(location=(cx, -0.3, 0.005))
        t = bpy.context.object
        t.data.body = f"{n}\n{it.triangles()} tris"
        t.data.size = 0.045 if row_kind == "short" else 0.09
        t.data.align_x = "CENTER"
        t.data.materials.append(material("label"))
        t.data.materials[0].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.02, 0.02, 0.02, 1)
    top = max(length(n) for n in row)
    bpy.ops.mesh.primitive_plane_add(size=40, location=(0, 0, 0))
    ground = bpy.context.object
    ground.data.materials.append(material("ground"))
    ground.data.materials[0].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.9, 0.9, 0.88, 1)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    sc.collection.objects.link(cam)
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = width + 0.2
    cam.rotation_euler = Euler((math.radians(80), 0, math.radians(-10)))
    d = cam.rotation_euler.to_matrix() @ Vector((0, 0, 1))
    cam.location = Vector((0, 0, top / 2 - 0.05)) + d * 10
    sc.camera = cam
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.5
    sun.rotation_euler = Euler((math.radians(40), math.radians(20), math.radians(-30)))
    sc.collection.objects.link(sun)
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.8, 0.8, 0.8, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.6
    sc.world = world
    sc.render.engine = "CYCLES"
    sc.cycles.samples = 48
    sc.cycles.device = "GPU" if os.environ.get("ACR_GPU", "1") == "1" else "CPU"
    sc.render.resolution_x = 2400
    sc.render.resolution_y = int(2400 * (top + 0.45) / cam.data.ortho_scale)
    sc.render.filepath = out
    bpy.ops.render.render(write_still=True)
    print("ACR held sheet:", out)


main()
