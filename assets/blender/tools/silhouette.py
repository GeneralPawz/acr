"""Quick silhouette fit for an asset script (development aid, not part of the build).

    blender -b --factory-startup --python assets/blender/tools/silhouette.py -- \
        <asset.py> <ref.json> [--clip NAME] [--exec "python"] [--out DIR]

Builds the asset's model with its `make(root, name)` (no colours, no export), shows clip
NAME (default: the reference's animation) and prints the silhouette IoU per stored
direction, like `acr model-check` but in one Blender run (a few seconds), so poses and
proportions can be tuned quickly. `--exec` runs code first with the asset module as `m`
and `acr_assets.poses` as `poses` (e.g. to try another pose). `--out` writes
`fit_<k>.png` (grey both, magenta sprite only, cyan model only).
"""

import argparse
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402
import numpy as np  # noqa: E402

from acr_assets import Reference, camera, poses, scene  # noqa: E402
from acr_assets import armature as am  # noqa: E402
from acr_assets import conventions as cv  # noqa: E402
from acr_assets import project  # noqa: E402


def main(argv):
    p = argparse.ArgumentParser(prog="silhouette.py")
    p.add_argument("asset")
    p.add_argument("ref")
    p.add_argument("--clip")
    p.add_argument("--exec", dest="code")
    p.add_argument("--out")
    p.add_argument("--opt", help="pose parameters to fit: bone.i or world.bone.i or root_loc.i, comma-separated")
    p.add_argument("--step", type=float, default=8.0)
    p.add_argument("--shift", default="0,0", help="move the render by dx,dy pixels before comparing")
    p.add_argument("--fit", help="module constants to fit (rebuilds): NAME.key[.i]:step, comma-separated")
    a = p.parse_args(argv)
    name = os.path.splitext(os.path.basename(a.asset))[0]
    spec = importlib.util.spec_from_file_location(f"asset_{name}", a.asset)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    if a.code:
        exec(a.code, {"m": m, "poses": poses})
    ref = Reference(a.ref)
    clip = a.clip or ref.anim.lstrip("#@")
    holder = {}

    def rebuild():
        s = scene.reset()
        root = scene.root(name)
        model = m.make(root, name)
        if hasattr(m, "pose_for"):
            m.pose_for(model, clip)
        camera.original_camera(ref.width, ref.height, ref.anchor)
        holder.update(s=s, root=root, model=model)
        return s, root, model

    s, root, model = rebuild()
    pivot = root
    s.render.engine = "BLENDER_WORKBENCH"
    s.display.shading.light = "FLAT"
    s.display.shading.color_type = "SINGLE"
    s.display.shading.single_color = (1, 1, 1)
    s.display.render_aa = "OFF"
    s.render.film_transparent = True
    s.render.image_settings.color_mode = "RGBA"
    tmp = os.path.join(bpy.app.tempdir, "sil.png")
    sprites = [project.load(v.image)[..., 3] >= 0.5 for v in ref.views]
    dx, dy = (int(x) for x in a.shift.split(","))

    def shifted(r):
        return np.roll(r, (dy, dx), (0, 1))

    def score():
        out = []
        for v, q in zip(ref.views, sprites):
            pivot.rotation_euler = (0.0, 0.0, cv.yaw(v.dir))
            s.render.filepath = tmp
            bpy.ops.render.render(write_still=True)
            r = shifted(project.load(tmp)[..., 3] >= 0.5)
            out.append((r & q).sum() / max((r | q).sum(), 1))
        return float(np.mean(out))

    def setup_render(s):
        s.render.engine = "BLENDER_WORKBENCH"
        s.display.shading.light = "FLAT"
        s.display.shading.color_type = "SINGLE"
        s.display.shading.single_color = (1, 1, 1)
        s.display.render_aa = "OFF"
        s.render.film_transparent = True
        s.render.image_settings.color_mode = "RGBA"

    if a.fit:
        # Coordinate descent on module constants; every step rebuilds the model.
        items = []
        for it in a.fit.split(","):
            path, st, *lim = it.split(":")
            parts = path.split(".")
            lo, hi = (float(lim[0]), float(lim[1])) if lim else (-1e9, 1e9)
            items.append((parts, float(st), lo, hi))

        def ref_of(parts):
            obj = getattr(m, parts[0])
            for k in parts[1:-1]:
                obj = obj[int(k)] if isinstance(obj, list) else obj[k]
            last = parts[-1]
            return obj, (int(last) if isinstance(obj, list) else last)

        def evaluate():
            nonlocal s, pivot
            s, root_, _model = rebuild()
            setup_render(s)
            pivot = root_
            return score()

        best = evaluate()
        scale = 1.0
        while scale >= 0.25:
            improved = False
            for parts, st, lo, hi in items:
                for sgn in (1, -1):
                    obj, key = ref_of(parts)
                    old_v = obj[key]
                    new_v = old_v + sgn * st * scale
                    if not lo <= new_v <= hi:
                        continue
                    obj[key] = new_v
                    sc = evaluate()
                    if sc > best + 1e-4:
                        best, improved = sc, True
                        print(f"FITSTEP {'.'.join(parts)} {obj[key]:.4f} -> {best:.4f}")
                        break
                    obj[key] = old_v
            if not improved:
                scale /= 2
        evaluate()
        print("BESTFIT", best, {n: getattr(m, n) for n in {it[0][0] for it in items}})
        model = holder["model"]

    if a.opt:
        # Coordinate descent on the pose of `clip` (frame 0), halving the step.
        spec = getattr(model, "_pose_spec", None)
        if spec is None:
            raise SystemExit("the asset's pose_for must store the pose as model._pose_spec")
        spec = {k: ({b: list(r) for b, r in v.items()} if isinstance(v, dict) else list(v)) for k, v in spec.items()}
        params = a.opt.split(",")

        def get(sp, prm):
            parts = prm.split(".")
            i = int(parts[-1])
            if parts[0] == "world":
                return sp.setdefault("world", {}).setdefault(".".join(parts[1:-1]), [0, 0, 0]), i
            return sp.setdefault(".".join(parts[:-1]), [0, 0, 0]), i

        def apply(sp):
            am.pose(model.arm, {k: (v if isinstance(v, dict) else tuple(v)) for k, v in sp.items()})

        apply(spec)
        best = score()
        step = a.step
        while step >= 1.0:
            improved = False
            for prm in params:
                for sgn in (1, -1):
                    vec, i = get(spec, prm)
                    d = step * (0.01 if prm.startswith("root_loc") else 1.0)
                    vec[i] += sgn * d
                    apply(spec)
                    sc = score()
                    if sc > best + 1e-4:
                        best, improved = sc, True
                        print(f"OPT {prm} {vec[i]:.3f} -> {best:.4f}")
                        break
                    vec[i] -= sgn * d
            if not improved:
                step /= 2
        apply(spec)
        print("BEST", best, {k: v for k, v in spec.items()})
    ious = []
    for v in ref.views:
        pivot.rotation_euler = (0.0, 0.0, cv.yaw(v.dir))
        s.render.filepath = tmp
        bpy.ops.render.render(write_still=True)
        r = shifted(project.load(tmp)[..., 3] >= 0.5)
        q = project.load(v.image)[..., 3] >= 0.5
        inter, union = (r & q).sum(), (r | q).sum()
        ious.append(inter / max(union, 1))
        if a.out:
            os.makedirs(a.out, exist_ok=True)
            img = np.zeros(r.shape + (4,), np.float32)
            img[..., 3] = 1
            img[..., :3] = (0.28, 0.3, 0.25)
            img[r & q, :3] = 0.67
            img[q & ~r, :3] = (0.9, 0.16, 0.78)
            img[r & ~q, :3] = (0.16, 0.82, 0.9)
            project.save(os.path.join(a.out, f"fit_{v.k}.png"), img.repeat(3, 0).repeat(3, 1))
    print("FIT " + " ".join(f"{x:.3f}" for x in ious) + f" mean {np.mean(ious):.3f}")


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    try:
        main(argv)
    except Exception:
        import traceback

        traceback.print_exc()
        sys.exit(1)
