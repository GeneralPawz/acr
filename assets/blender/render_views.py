"""Renders a model through the original camera for every view of a sprite reference.

    blender -b --factory-startup --python assets/blender/render_views.py -- \
        --ref <ref.json> --model <file.glb> --out <dir> [--shading flat|studio]

Writes `render_<k>.png` (RGBA, transparent background, the canvas of `ref.json`) for
`acr model-check`, which compares them with `ref_<k>.png`. Workbench with flat lighting
shows the base colour (texture) unlit, which is what the sprite colours are projected into;
`studio` adds Workbench's default lighting for a look at the shapes.
"""

import argparse
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "lib"))

import bpy  # noqa: E402

from acr_assets import Reference, camera, scene  # noqa: E402
from acr_assets import conventions as cv  # noqa: E402


def main(argv):
    p = argparse.ArgumentParser(prog="render_views.py")
    p.add_argument("--ref", required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--shading", default="flat", choices=("flat", "studio"))
    a = p.parse_args(argv)
    ref = Reference(a.ref)

    s = scene.reset()
    bpy.ops.import_scene.gltf(filepath=os.path.abspath(a.model))
    pivot = bpy.data.objects.new("pivot", None)
    s.collection.objects.link(pivot)
    for obj in list(s.objects):
        if obj.parent is None and obj is not pivot:
            obj.parent = pivot
    camera.original_camera(ref.width, ref.height, ref.anchor)

    s.render.engine = "BLENDER_WORKBENCH"
    s.display.shading.light = "FLAT" if a.shading == "flat" else "STUDIO"
    s.display.shading.color_type = "TEXTURE"
    # No anti-aliasing: every pixel samples its centre, like the hard-edged sprites (with
    # AA the samples spread over neighbouring texels and blur the projected colours).
    s.display.render_aa = "OFF"
    s.render.film_transparent = True
    s.render.image_settings.file_format = "PNG"
    s.render.image_settings.color_mode = "RGBA"
    s.render.image_settings.color_depth = "8"
    s.render.dither_intensity = 0.0

    os.makedirs(a.out, exist_ok=True)
    for v in ref.views:
        pivot.rotation_euler = (0.0, 0.0, cv.yaw(v.dir))
        s.render.filepath = os.path.join(os.path.abspath(a.out), f"render_{v.k}.png")
        bpy.ops.render.render(write_still=True)
        print(f"ACR view {v.k}: RealDir {v.dir}, yaw {math.degrees(cv.yaw(v.dir)):.1f} deg")


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    try:
        main(argv)
    except Exception:
        import traceback

        traceback.print_exc()
        sys.exit(1)
