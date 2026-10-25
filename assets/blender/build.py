"""Builds one asset: runs an asset script in an empty scene and exports its glTF.

    PYTHONHASHSEED=0 blender -b --factory-startup --python-use-system-env --python assets/blender/build.py -- \
        <asset.py> --out <file.glb> [--ref <ref.json>] [--compress none|meshopt|draco] [--blend <file.blend>]

`cargo xtask assets` calls this for every asset script. An asset script defines

    TYPE = "IncDo1"          # NAME of the .MD the sprite check compares with (optional)
    def build(ctx): ...      # ctx.ref: acr_assets.Reference or None, ctx.root: the root node

and creates its objects under `ctx.root`. With `--ref` (the output of `acr model-ref`) it
can take its colours from the sprites (`acr_assets.project`).
"""

import argparse
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "lib"))

from acr_assets import BUILD_STATS, Reference, export, scene  # noqa: E402


class Context:
    def __init__(self, name, ref, root):
        self.name = name
        self.ref = ref
        self.root = root
        # Numbers an asset wants in `<name>.stats.json` next to its glTF (triangles, ...).
        self.stats = {}
        # References by label (`STANDLO-0`, `BUILDLO_2`, ...) from --refs.
        self.refs = {}


def _audit_limits(ctx):
    """Every clip of every skeleton-H armature against the joint limits (`armature.
    audit_limits`, docs/assets-humans.md "Joint limits") into ctx.stats["limits"]."""
    import bpy

    from acr_assets import armature, limits

    out = None
    for ob in sorted(bpy.data.objects, key=lambda o: o.name):
        if ob.type != "ARMATURE" or not ob.get("acr_clips"):
            continue
        if not any(limits.key_of(b.name)[0] == "forearm" for b in ob.data.bones):
            continue
        a = armature.audit_limits(ob)
        if out is None:
            out = a
            continue
        out["frames"] += a["frames"]
        out["violations"] += a["violations"]
        out["by_bone"].update(a["by_bone"])
        out["by_clip"].update(a["by_clip"])
        if a["worst"] and (out["worst"] is None or a["worst"][3] > out["worst"][3]):
            out["worst"] = a["worst"]
    if out is not None:
        ctx.stats["limits"] = out


def main(argv):
    p = argparse.ArgumentParser(prog="build.py")
    p.add_argument("asset")
    p.add_argument("--out", required=True)
    p.add_argument("--ref")
    p.add_argument("--refs", help="folder of references <label>/ref.json (one per CHECKS entry)")
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
    ctx = Context(name, ref, root)
    if a.refs and os.path.isdir(a.refs):
        for label in sorted(os.listdir(a.refs)):
            p = os.path.join(a.refs, label, "ref.json")
            if os.path.isfile(p):
                ctx.refs[label] = Reference(p)
    if sys.flags.hash_randomization:
        # Sets of names iterate in a new order every run: the build is not repeatable.
        print("ACR warning: string hashes are randomised; run through `cargo xtask assets` "
              "(PYTHONHASHSEED=0 --python-use-system-env) for a repeatable build")
    module.build(ctx)
    _audit_limits(ctx)
    for k, v in BUILD_STATS.items():
        ctx.stats.setdefault(k, v)
    if a.blend:
        import bpy

        os.makedirs(os.path.dirname(os.path.abspath(a.blend)), exist_ok=True)
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(a.blend))
    # The limit constraints (`armature.apply_limits`) stay in the .blend; the clips already
    # respect them, and the exporter would bake every constrained bone's channels.
    import bpy

    for ob in bpy.data.objects:
        if ob.type == "ARMATURE":
            for pb in ob.pose.bones:
                for c in [c for c in pb.constraints if c.name.startswith("acr_limit")]:
                    pb.constraints.remove(c)
    # The export pose, before the exporter runs; the clip poses after it (`posecheck`).
    from acr_assets import posecheck

    poser = posecheck.Poser() if any(o.type == "ARMATURE" for o in bpy.data.objects) and bpy.data.actions else None
    size = export.export_glb(os.path.abspath(a.out), a.compress, getattr(module, "IMAGE_FORMAT", "AUTO"))
    ctx.stats["glb_bytes"] = size
    if poser is not None:
        # What proof renders of the glb check their pose against (`posecheck.verify`).
        posecheck.dump(os.path.abspath(a.out), poser)
        # The step cycles as the game's gait layer measures them (docs/body-sim.md).
        g = posecheck.gait(poser)
        if g:
            ctx.stats["gait"] = g
    if export.LAST_PACK:
        ctx.stats["pack"] = export.LAST_PACK
    with open(os.path.splitext(os.path.abspath(a.out))[0] + ".stats.json", "w", encoding="utf-8") as f:
        json.dump(ctx.stats, f, indent=1)
    print(f"ACR asset {name}: {a.out} ({size} bytes, colours {'from sprites' if ref else 'flat'})")


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    try:
        main(argv)
    except Exception:
        import traceback

        traceback.print_exc()
        sys.exit(1)
