"""Builds one asset: runs an asset script in an empty scene and exports its glTF.

    blender -b --factory-startup --python assets/blender/build.py -- \
        <asset.py> --out <file.glb> [--ref <ref.json>] [--compress none|meshopt|draco] [--blend <file.blend>]

`cargo xtask assets` calls this for every asset script. An asset script defines

    TYPE = "IncDo1"          # NAME of the .MD the sprite check compares with (optional)
    def build(ctx): ...      # ctx.ref: acr_assets.Reference or None, ctx.root: the root node

and creates its objects under `ctx.root`. With `--ref` (the output of `acr model-ref`) it
can take its colours from the sprites (`acr_assets.project`).
"""

import argparse
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "lib"))

from acr_assets import Reference, export, scene  # noqa: E402


class Context:
    def __init__(self, name, ref, root):
        self.name = name
        self.ref = ref
        self.root = root


def main(argv):
    p = argparse.ArgumentParser(prog="build.py")
    p.add_argument("asset")
    p.add_argument("--out", required=True)
    p.add_argument("--ref")
    p.add_argument("--compress", default="none", choices=export.COMPRESSION)
    p.add_argument("--blend", help="also save the scene as .blend (for inspection)")
    a = p.parse_args(argv)

    name = os.path.splitext(os.path.basename(a.asset))[0]
    spec = importlib.util.spec_from_file_location(f"asset_{name}", a.asset)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    scene.reset()
    ref = Reference(a.ref) if a.ref else None
    props = {"acr_asset": name}
    if getattr(module, "TYPE", None):
        props["acr_type"] = module.TYPE
    root = scene.root(name, **props)
    module.build(Context(name, ref, root))
    if a.blend:
        import bpy

        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(a.blend))
    size = export.export_glb(os.path.abspath(a.out), a.compress)
    print(f"ACR asset {name}: {a.out} ({size} bytes, colours {'from sprites' if ref else 'flat'})")


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    try:
        main(argv)
    except Exception:
        import traceback

        traceback.print_exc()
        sys.exit(1)
