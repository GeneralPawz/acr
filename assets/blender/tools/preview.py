"""Lit preview renders of a built model (development aid and report screenshots).

    blender -b --factory-startup --python assets/blender/tools/preview.py -- \
        <model.glb> <out-dir> [--ref ref.json] [--clip NAME] [--views 0,2,4,6,8] [--scale 4]
        [--free N] [--decay 0,0.3,0.6,0.9] [--state NAME] [--sections S] [--distance M]

Imports the glTF as the game would get it and renders with Eevee, a sun from the upper left
and soft sky light:

- `orig_<k>.png`: the original camera (reference canvas, `--scale` times larger) for the
  stored directions `--views`, model in clip `--clip` (default: the reference's animation);
- `free_<i>.png`: `--free` views around the model from a free camera 35 degrees up;
- `decay_<d>.png`: with `--decay`, the clip's pose at those decay values, the decay shader
  rebuilt from the material extras `acr_decay` and the `COLOR_0` mask, the way the game's
  renderer would (a check that the exported data suffices). Stump caps stay hidden.
"""

import argparse
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, os.path.join(HERE, ".."))

import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

from acr_assets import Reference, decay, scene  # noqa: E402
from acr_assets import conventions as cv  # noqa: E402
import render_views  # noqa: E402


def lights(s, strength=1.0):
    w = bpy.data.worlds.new("sky")
    s.world = w
    w.use_nodes = True
    bg = w.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.55, 0.6, 0.7, 1)
    bg.inputs["Strength"].default_value = 0.55 * strength
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 3.0 * strength
    sun.angle = math.radians(6)
    o = bpy.data.objects.new("sun", sun)
    # From the upper left of the original view, a little towards the viewer.
    o.rotation_euler = (math.radians(40), math.radians(-25), math.radians(-35))
    s.collection.objects.link(o)


def decay_materials():
    """Rebuilds every material with `acr_decay` extras as a decay shader on the object
    property `decay` (the WGSL of docs/assets-humans.md in nodes)."""
    for mat in list(bpy.data.materials):
        ex = mat.get("acr_decay")
        if not ex or not mat.node_tree:
            continue
        nt = mat.node_tree
        nodes, links = nt.nodes, nt.links
        bsdf = next((n for n in nodes if n.type == "BSDF_PRINCIPLED"), None)
        out = next((n for n in nodes if n.type == "OUTPUT_MATERIAL"), None)
        if bsdf is None or out is None:
            continue
        # The importer multiplies COLOR_0 into the base colour; it is the decay mask here.
        tex = next((n for n in nodes if n.type == "TEX_IMAGE"), None)
        for lk in [lk for lk in links if lk.to_socket == bsdf.inputs["Base Color"]]:
            links.remove(lk)
        base = tex.outputs["Color"] if tex else decay.lin(tuple(mat.get("acr_base_rgb", (0.5, 0.5, 0.5))))
        t = nodes.new("ShaderNodeAttribute")
        t.attribute_type = "OBJECT"
        t.attribute_name = "decay"
        t = t.outputs["Fac"]
        vc = nodes.new("ShaderNodeAttribute")
        vc.attribute_name = "Color"
        sep = nodes.new("ShaderNodeSeparateColor")
        links.new(vc.outputs["Color"], sep.inputs["Color"])
        mask = decay._math(nodes, links, "MULTIPLY_ADD", sep.outputs[0], 0.75)
        links.new(decay._math(nodes, links, "MULTIPLY", sep.outputs[1], 0.25), mask.node.inputs[2])
        d0, d1 = ex["discolour"]
        dfac = decay._math(nodes, links, "MULTIPLY", decay._ramp(nodes, links, t, d0, d1), ex["tint_strength"])
        col = decay._mix(nodes, links, dfac, base, decay.lin(tuple(ex["tint"])))
        keep = None
        if len(ex["dissolve"]):
            s0, s1 = ex["dissolve"]
            prog = decay._ramp(nodes, links, t, s0, s1)
            th = decay._math(nodes, links, "MULTIPLY", prog, 1.0 + ex["edge"] + 0.001)
            keep = decay._math(nodes, links, "GREATER_THAN", mask, th)
            if ex["edge"] > 0:
                above = decay._math(nodes, links, "SUBTRACT", mask, th)
                e = decay._math(nodes, links, "LESS_THAN", above, ex["edge"])
                e = decay._math(nodes, links, "MULTIPLY", e, decay._math(nodes, links, "GREATER_THAN", prog, 0.0))
                col = decay._mix(nodes, links, e, col, decay.lin(tuple(ex["edge_rgb"])))
        links.new(col, bsdf.inputs["Base Color"])
        if keep is not None:
            for lk in [lk for lk in links if lk.to_socket == out.inputs["Surface"]]:
                links.remove(lk)
            tr = nodes.new("ShaderNodeBsdfTransparent")
            mix = nodes.new("ShaderNodeMixShader")
            links.new(keep, mix.inputs[0])
            links.new(tr.outputs[0], mix.inputs[1])
            links.new(bsdf.outputs[0], mix.inputs[2])
            links.new(mix.outputs[0], out.inputs["Surface"])
        if hasattr(mat, "surface_render_method"):
            mat.surface_render_method = "DITHERED"


def set_decay(s, value):
    for o in s.objects:
        if o.type != "MESH":
            continue
        o["decay"] = value
        keys = o.data.shape_keys
        if keys and "deflate" in keys.key_blocks:
            keys.key_blocks["deflate"].value = decay.deflate_weight(value)
    s.frame_set(s.frame_current)


def render(s, path):
    s.render.filepath = path
    bpy.ops.render.render(write_still=True)


def main(argv):
    p = argparse.ArgumentParser(prog="preview.py")
    p.add_argument("model")
    p.add_argument("out")
    p.add_argument("--ref")
    p.add_argument("--clip")
    p.add_argument("--views", default="0,2,4,6,8")
    p.add_argument("--scale", type=int, default=4)
    p.add_argument("--free", type=int, default=4)
    p.add_argument("--decay", default="")
    p.add_argument("--state")
    p.add_argument("--sections", type=int, default=-1)
    p.add_argument("--distance", type=float, default=0.0)
    p.add_argument("--ground", action="store_true")
    a = p.parse_args(argv)
    os.makedirs(a.out, exist_ok=True)
    s = scene.reset()
    bpy.ops.import_scene.gltf(filepath=os.path.abspath(a.model))
    pivot = bpy.data.objects.new("pivot", None)
    s.collection.objects.link(pivot)
    for obj in list(s.objects):
        if obj.parent is None and obj is not pivot:
            obj.parent = pivot

    class R:
        pass

    ref = Reference(a.ref) if a.ref else None
    st = R()
    st.anim = a.clip or a.state or (ref.anim if ref else "STAND")
    st.sections = a.sections if a.sections >= 0 else (ref.sections if ref else -1)
    render_views.show_state(s, st)
    for o in s.objects:
        if o.name.endswith(".caps"):
            o.hide_render = True
    # Always: the importer multiplies COLOR_0 (the decay mask) into the base colour.
    decay_materials()
    for o in s.objects:
        if o.type == "MESH":
            o["decay"] = 0.0
    s.render.engine = "BLENDER_EEVEE"
    s.render.film_transparent = not a.ground
    s.render.image_settings.file_format = "PNG"
    s.render.image_settings.color_mode = "RGBA"
    s.view_settings.view_transform = "Standard"
    lights(s)
    if a.ground:
        me = bpy.data.meshes.new("ground")
        me.from_pydata([(-40, -40, 0), (40, -40, 0), (40, 40, 0), (-40, 40, 0)], [], [(0, 1, 2, 3)])
        g = bpy.data.objects.new("ground", me)
        s.collection.objects.link(g)
        m = bpy.data.materials.new("ground")
        m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = decay.lin((0.36, 0.38, 0.22))
        me.materials.append(m)
    # Size of the model for the free camera.
    pts = [o.matrix_world @ Vector(c) for o in s.objects
           if o.type == "MESH" and not o.hide_render and "." in o.name for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    centre, radius = (lo + hi) / 2, max((hi - lo).length / 2, 0.5)

    if ref:
        cam = bpy.data.objects.new("orig", bpy.data.cameras.new("orig"))
        s.collection.objects.link(cam)
        loc, rot, ortho = cv.camera_setup(ref.width, ref.height, ref.anchor)
        cam.data.type = "ORTHO"
        cam.data.ortho_scale = ortho
        cam.data.clip_end = 1000
        cam.location, cam.rotation_euler = loc, rot
        s.camera = cam
        s.render.resolution_x, s.render.resolution_y = ref.width * a.scale, ref.height * a.scale
        for v in ref.views:
            if str(v.k) not in a.views.split(","):
                continue
            pivot.rotation_euler = (0, 0, cv.yaw(v.dir))
            render(s, os.path.join(os.path.abspath(a.out), f"orig_{v.k}.png"))
        pivot.rotation_euler = (0, 0, 0)
    fcam = bpy.data.objects.new("free", bpy.data.cameras.new("free"))
    s.collection.objects.link(fcam)
    fcam.data.lens = 50
    fcam.data.clip_end = 2000
    s.camera = fcam
    s.render.resolution_x, s.render.resolution_y = 800, 800
    dist = a.distance or radius * 3.2
    for i in range(a.free):
        ang = math.radians(-30 + 360 * i / max(a.free, 1))
        el = math.radians(35)
        fcam.location = centre + Vector((math.sin(ang) * math.cos(el), -math.cos(ang) * math.cos(el), math.sin(el))) * dist
        fcam.rotation_euler = (centre - fcam.location).to_track_quat("-Z", "Y").to_euler()
        render(s, os.path.join(os.path.abspath(a.out), f"free_{i}.png"))
    for d in [float(x) for x in a.decay.split(",") if x]:
        set_decay(s, d)
        pivot.location.z = decay.sink_offset(d)
        fcam.location = centre + Vector((0.9, -1.6, 1.2)) * (radius * 0.9)
        fcam.rotation_euler = (centre - fcam.location).to_track_quat("-Z", "Y").to_euler()
        render(s, os.path.join(os.path.abspath(a.out), f"decay_{d:.2f}.png"))


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    try:
        main(argv)
    except Exception:
        import traceback

        traceback.print_exc()
        sys.exit(1)
