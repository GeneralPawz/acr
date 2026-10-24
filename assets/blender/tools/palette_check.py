"""Colour check of built models against their sprites (development aid, not part of the build).

    blender -b --factory-startup --python assets/blender/tools/palette_check.py -- \
        <list.txt> --out DIR

Each line of the list is `<unit> <model.glb> <ref.json> <ref_other_player.json>`: the two
references are `acr model-ref <game> <TYPE> <dir> [--player N]` of the same type for two
players, so pixels that differ between them are team colour and are left out. Per unit and
stored direction it renders the model in its `STAND` pose with the original camera twice
(Workbench, flat light, no anti-aliasing): once with the textures unlit
and once with one flat id colour per material. The id render maps each material's region
(`palette.region`: coat, facings, leggings ...) into the sprite; per region the dominant
colour (`palette.dominant`, CIELAB) of the sprite pixels under it (region and sprite eroded
by a pixel, so outlines and neighbours drop out) is compared with the model's colour there
(CIEDE2000, lightness weighted half, `palette.KL`). Writes `DIR/<unit>.json` and
`DIR/<unit>.png` (per view: sprite, model colours, region ids).
"""

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402
import numpy as np  # noqa: E402

from acr_assets import Reference, camera, project, scene  # noqa: E402
from acr_assets import conventions as cv  # noqa: E402
from acr_assets import palette  # noqa: E402

sys.path.insert(0, os.path.join(HERE, ".."))
import render_views  # noqa: E402

# Materials that are not cloth or skin seen in STAND (gore, hidden stumps).
SKIP = ("stump_meat", "bones")


def id_colour(i):
    """A distinct sRGB id colour for material index i (6 levels per channel)."""
    i += 1
    return np.array([(i // 36) % 6, (i // 6) % 6, i % 6], np.float64) / 5.0


def setup(ref):
    s = bpy.context.scene
    camera.original_camera(ref.width, ref.height, ref.anchor)
    s.render.engine = "BLENDER_WORKBENCH"
    s.display.shading.light = "FLAT"
    s.display.render_aa = "OFF"
    s.view_settings.view_transform = "Standard"
    s.view_settings.look = "None"
    s.render.film_transparent = True
    s.render.dither_intensity = 0.0
    s.render.image_settings.file_format = "PNG"
    s.render.image_settings.color_mode = "RGBA"
    s.render.image_settings.color_depth = "8"
    return s


def render(s, path):
    s.render.filepath = path
    bpy.ops.render.render(write_still=True)
    return project.load(path)


def fit(img, sprite_solid, anchor):
    """The model render scaled about the anchor (nearest pixel) so its silhouette area
    matches the sprite's: the sprites draw men taller than 30 px/m, and the regions must
    land on the same parts."""
    area = (img[..., 3] >= 0.5).sum()
    if area == 0:
        return img, 1.0
    k = float(np.sqrt(sprite_solid.sum() / area))
    h, w = img.shape[:2]
    ax, ay = anchor
    ys = np.clip(np.round(ay + (np.arange(h) - ay) / k).astype(int), 0, h - 1)
    xs = np.clip(np.round(ax + (np.arange(w) - ax) / k).astype(int), 0, w - 1)
    return img[ys][:, xs], k


def check_unit(unit, glb, ref0, ref1, out):
    sc = scene.reset()
    render_views.import_glb(glb)
    pivot = bpy.data.objects.new("pivot", None)
    bpy.context.scene.collection.objects.link(pivot)
    for ob in list(bpy.data.objects):
        if ob is not pivot and ob.parent is None:
            ob.parent = pivot
    ref, other = Reference(ref0), Reference(ref1)
    render_views.show_state(sc, ref)
    s = setup(ref)
    mats = sorted({m.name for ob in bpy.data.objects if ob.type == "MESH" for m in ob.data.materials if m})
    names = {m: palette.region(m.removesuffix(".gltf"), unit) for m in mats}
    tmp = os.path.join(bpy.app.tempdir, "pc.png")
    views = []
    for v, w in zip(ref.views, other.views):
        pivot.rotation_euler = (0.0, 0.0, cv.yaw(v.dir))
        views.append({"v": v, "w": w})
    # Pass 1: own colours (textures unlit, as `acr model-check` renders them).
    s.display.shading.color_type = "TEXTURE"
    for d in views:
        pivot.rotation_euler = (0.0, 0.0, cv.yaw(d["v"].dir))
        d["albedo"] = render(s, tmp)
    # Pass 2: one flat id colour per material.
    ids = {m: id_colour(i) for i, m in enumerate(mats)}
    for m in mats:
        # Workbench shows diffuse_color as linear: store the id's linear value.
        lin = np.where(ids[m] <= 0.04045, ids[m] / 12.92, ((ids[m] + 0.055) / 1.055) ** 2.4)
        bpy.data.materials[m].diffuse_color = (*lin, 1.0)
    s.display.shading.color_type = "MATERIAL"
    for d in views:
        pivot.rotation_euler = (0.0, 0.0, cv.yaw(d["v"].dir))
        d["ids"] = render(s, tmp)
    regions = {}
    for r in sorted(set(names.values())):
        if any(k in r for k in SKIP):
            continue
        regions[r] = {"sprite": [], "model": []}
    table = np.array([ids[m] for m in mats])
    strips, scales, teams = [], [], []
    for d in views:
        sp = project.load(d["v"].image)
        sq = project.load(d["w"].image)
        solid = sp[..., 3] >= 0.5
        team = np.abs(sp[..., :3] - sq[..., :3]).max(-1) > 0.02
        inner = palette.erode(solid)
        teams.append(float((team & solid).sum() / max(solid.sum(), 1)))
        idp, k = fit(d["ids"], solid, ref.anchor)
        alb, _ = fit(d["albedo"], solid, ref.anchor)
        scales.append(k)
        has = idp[..., 3] >= 0.5
        dist = np.linalg.norm(idp[..., None, :3] - table[None, None], axis=-1)
        idx = np.where(has, dist.argmin(-1), -1)
        for mi, m in enumerate(mats):
            r = names[m]
            if r not in regions:
                continue
            own = idx == mi
            if not own.any():
                continue
            regions[r]["model"].append(palette.srgb_to_lab(alb[own][:, :3]))
            core = palette.erode(own)
            # Thin parts (belts, faces) vanish when eroded: keep them whole.
            sel = (core if core.sum() >= 0.4 * own.sum() else own) & inner & ~team
            if sel.any():
                regions[r]["sprite"].append(palette.srgb_to_lab(sp[sel][:, :3]))
        strips.append(np.concatenate([sp, alb, np.dstack([idp[..., :3], has.astype(np.float32)])], 1))
    res = {}
    for r, d in regions.items():
        if not d["model"]:
            continue
        ml = np.concatenate(d["model"])
        sl = np.concatenate(d["sprite"]) if d["sprite"] else np.zeros((0, 3))
        mc = palette.dominant(ml)
        e = {"n_model": int(len(ml)), "n_sprite": int(len(sl)),
             "model": [round(float(x), 4) for x in palette.lab_to_srgb(mc)], "model_lab": [round(float(x), 1) for x in mc]}
        if len(sl) >= palette.MIN_PIXELS:
            sc = palette.dominant(sl)
            e["sprite"] = [round(float(x), 4) for x in palette.lab_to_srgb(sc)]
            e["sprite_lab"] = [round(float(x), 1) for x in sc]
            e["de"] = round(float(palette.ciede2000(sc, mc)), 1)
        res[r] = e
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, f"{unit}.json"), "w", encoding="utf-8") as f:
        json.dump({"unit": unit, "type": ref.type, "views": len(views), "scale": round(float(np.mean(scales)), 3), "team_share": round(float(np.mean(teams)), 3), "regions": res}, f, indent=1, sort_keys=True)
    project.save(os.path.join(out, f"{unit}.png"), np.concatenate(strips, 0).astype(np.float32))
    print(f"PALETTE {unit}: " + ", ".join(f"{r} {e.get('de', '-')}" for r, e in sorted(res.items())))


def main(argv):
    p = argparse.ArgumentParser(prog="palette_check.py")
    p.add_argument("list")
    p.add_argument("--out", required=True)
    a = p.parse_args(argv)
    with open(a.list, encoding="utf-8") as f:
        rows = [ln.split() for ln in f if ln.strip() and not ln.startswith("#")]
    for unit, glb, r0, r1 in rows:
        try:
            check_unit(unit, glb, r0, r1, a.out)
        except Exception as e:  # keep going over the roster
            print(f"PALETTE {unit}: FAILED {e!r}")


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
