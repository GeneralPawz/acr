"""Runs `acr_assets.genfit` over generated items: headgear fitted to the head of a built
unit glb (rendered on it, front and 3/4), other items alone (front and 3/4); writes each
item's glb (baked colour + normal map), the renders and `genfit_stats.json`.

    blender -b --factory-startup --python assets/blender/tools/genfit_batch.py -- \
        --unit target/assets/units/engmus.glb --out /tmp/genfit pixal3d/eq_tricorn_hat.glb ...

Sources resolve through `genfit.source_path` (ACR_GENERATED or ACR_SOURCES).
"""

import json
import math
import os
import sys
import time
import traceback

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))

from acr_assets import genfit  # noqa: E402


def load_unit(path):
    """The unit as exported (STAND); its flesh in world space as the scalp, socket.hat."""
    bpy.ops.import_scene.gltf(filepath=path)
    dg = bpy.context.evaluated_depsgraph_get()
    flesh = next(o for o in bpy.data.objects if o.type == "MESH" and o.name.endswith(".flesh"))
    me = bpy.data.meshes.new_from_object(flesh.evaluated_get(dg))
    me.transform(flesh.matrix_world)
    scalp = bpy.data.objects.new("unit.scalp", me)
    bpy.context.scene.collection.objects.link(scalp)
    scalp.hide_render = True
    arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    sock = arm.matrix_world @ arm.pose.bones["socket.hat"].head
    head = arm.matrix_world @ arm.pose.bones["head"].tail
    shown = [o for o in bpy.data.objects if o.type == "MESH" and o is not scalp]
    # the unit's own hat and the gear hidden under a new hat
    for o in shown:
        if any(k in o.name.lower() for k in ("pike_broad", "tricorn", "slouch", "cocked", "morion", "pike_pot",
                                              ".caps", ".bones", "hair_loose", "hair_queue", "hair_tied")):
            o.hide_render = True
    return scalp, genfit.head_of(scalp, head), sock, [o for o in shown if not o.hide_render]


def render(objs, path, azim, center, half, res=480):
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sh = sc.display.shading
    sh.light, sh.color_type, sh.background_type = "STUDIO", "TEXTURE", "WORLD"
    if sc.world is None:
        sc.world = bpy.data.worlds.new("w")
    sc.world.color = (1, 1, 1)
    sc.render.resolution_x = sc.render.resolution_y = res
    sc.view_settings.view_transform = "Standard"
    cam = bpy.data.objects.get("genfit.cam")
    if cam is None:
        cam = bpy.data.objects.new("genfit.cam", bpy.data.cameras.new("genfit.cam"))
        sc.collection.objects.link(cam)
    cam.data.type, cam.data.ortho_scale, cam.data.clip_end = "ORTHO", 2 * half, 50
    a, e = math.radians(azim), math.radians(8)
    d = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
    cam.location = center + d * 10
    cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    sc.camera = cam
    keep = {o.name: o.hide_render for o in sc.objects if o.type == "MESH"}
    for o in sc.objects:
        if o.type == "MESH":
            o.hide_render = o not in objs
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    for n, h in keep.items():
        bpy.data.objects[n].hide_render = h


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    unit = argv[argv.index("--unit") + 1] if "--unit" in argv else None
    out = argv[argv.index("--out") + 1]
    rels = [a for i, a in enumerate(argv) if a.endswith(".glb") and (i == 0 or argv[i - 1] not in ("--unit",))]
    os.makedirs(out, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scalp = head = sock = body = None
    if unit:
        scalp, head, sock, body = load_unit(unit)
    stats = {}
    for rel in rels:
        it = genfit.item(rel)
        t0 = time.time()
        try:
            hat = it["category"] == "headgear" and scalp is not None
            ob, st = genfit.fit(rel, head=head if hat else None, socket=sock if hat else None,
                                scalp=scalp if hat else None)
        except Exception as e:  # one bad item does not stop the batch
            traceback.print_exc()
            stats[it["name"]] = {"error": repr(e)}
            continue
        st["seconds"] = round(time.time() - t0, 1)
        stats[it["name"]] = st
        print("GENFIT", it["name"], json.dumps(st))
        bpy.ops.object.select_all(action="DESELECT")
        ob.select_set(True)
        bpy.context.view_layer.objects.active = ob
        bpy.ops.export_scene.gltf(filepath=os.path.join(out, it["name"] + ".glb"), use_selection=True,
                                  export_image_format="JPEG", export_jpeg_quality=90)
        if hat:
            ob.location = sock
            bpy.context.view_layer.update()
            ctr = Vector((head["axis"][0], head["axis"][1], head["nose"].z + 0.06))
            for az in (0, 35):
                render(body + [ob], os.path.join(out, f"{it['name']}_{az:02d}.png"), az, ctr, 0.24)
        else:
            lo = Vector([min((ob.matrix_world @ Vector(c))[i] for c in ob.bound_box) for i in range(3)])
            hi = Vector([max((ob.matrix_world @ Vector(c))[i] for c in ob.bound_box) for i in range(3)])
            # grip / socket marker
            mk = bpy.data.objects.get("genfit.marker")
            if mk is None:
                bpy.ops.mesh.primitive_uv_sphere_add(radius=1, segments=12, ring_count=6)
                mk = bpy.context.active_object
                mk.name = "genfit.marker"
                m = bpy.data.materials.new("marker")
                m.diffuse_color = (1, 0, 0.6, 1)
                mk.data.materials.append(m)
            mk.scale = (0.012 * max(1.0, max(hi - lo) / 0.8),) * 3
            mk.location = (0, 0, 0)
            for az in (0, 35):
                render([ob, mk], os.path.join(out, f"{it['name']}_{az:02d}.png"), az, (lo + hi) / 2,
                       0.55 * max(hi - lo) + 0.02)
        ob.hide_render = True
        ob.location = (0, 0, 0)
    with open(os.path.join(out, "genfit_stats.json"), "w") as f:
        json.dump(stats, f, indent=1)


main()
