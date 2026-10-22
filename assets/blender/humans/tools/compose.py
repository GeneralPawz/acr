"""Comparison sheets and silhouette IoU for the human renders (pure Python, `python -I`).

    python -I compose.py REF_DIR OUT_DIR

REF_DIR: `acr model-ref` folders <Type>-<ANIM>; OUT_DIR: build_humans.py output. Writes
compare-<Type>-<ANIM>.png (per direction: sprite x4 | model at sprite size x4 | model
rendered at 4x), decay-strip.png (sprite #DEATHLIE1 then the decay stages, 3x) and
compare.json (silhouette IoU per view, alpha >= 128).
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import png  # noqa: E402

BG = (96, 104, 96, 255)


def iou(a, b):
    inter = union = 0
    for ra, rb in zip(a, b):
        for x in range(3, len(ra), 4):
            p, q = ra[x] >= 128, rb[x] >= 128
            inter += p and q
            union += p or q
    return inter / union if union else 0.0


def compare(ref_dir, out_dir, typ, anim, ks):
    ref = json.load(open(os.path.join(ref_dir, f"{typ}-{anim}", "ref.json")))
    w, h = ref["width"], ref["height"]
    rows, scores = [], {}
    for k in ks:
        spr = png.read(os.path.join(ref_dir, f"{typ}-{anim}", f"ref_{k}.png"))[2]
        m1 = os.path.join(out_dir, f"{typ}-{anim}-k{k}-x1.png")
        m4 = os.path.join(out_dir, f"{typ}-{anim}-k{k}-x4.png")
        if not os.path.exists(m1):
            continue
        r1 = png.read(m1)[2]
        r4 = png.read(m4)[2]
        scores[k] = round(iou(spr, r1), 3)
        rows.append((spr, r1, r4))
    if not rows:
        return {}
    gap = 8
    W, H = 3 * w * 4 + 4 * gap, len(rows) * (h * 4 + gap) + gap
    sheet = png.blank(W, H, BG)
    for i, (spr, r1, r4) in enumerate(rows):
        y = gap + i * (h * 4 + gap)
        png.paste(sheet, spr, gap, y, 4)
        png.paste(sheet, r1, 2 * gap + w * 4, y, 4)
        png.paste(sheet, r4, 3 * gap + 2 * w * 4, y, 1)
    png.write(os.path.join(out_dir, f"compare-{typ}-{anim}.png"), W, H, sheet)
    return scores


def strip(ref_dir, out_dir, n=8, k=2, scale=3):
    ref = json.load(open(os.path.join(ref_dir, "EngM18-DEATHLIE1", "ref.json")))
    w, h = ref["width"], ref["height"]
    spr = png.read(os.path.join(ref_dir, "EngM18-DEATHLIE1", f"ref_{k}.png"))[2]
    gap = 6
    W, H = (n + 1) * (w * scale + gap) + gap, h * scale + 2 * gap
    sheet = png.blank(W, H, BG)
    png.paste(sheet, spr, gap, gap, scale)
    for i in range(n):
        p = os.path.join(out_dir, f"decay-{i}-x{scale}.png")
        if os.path.exists(p):
            png.paste(sheet, png.read(p)[2], gap + (i + 1) * (w * scale + gap), gap, 1)
    png.write(os.path.join(out_dir, "decay-strip.png"), W, H, sheet)


def turntable(out_dir, n=8):
    imgs = [png.read(os.path.join(out_dir, f"turn-{i}.png")) for i in range(n)
            if os.path.exists(os.path.join(out_dir, f"turn-{i}.png"))]
    if not imgs:
        return
    w, h = imgs[0][0], imgs[0][1]
    sheet = png.blank(w * len(imgs), h, BG)
    for i, (_w, _h, rows) in enumerate(imgs):
        png.paste(sheet, rows, i * w, 0, 1)
    png.write(os.path.join(out_dir, "turntable.png"), w * len(imgs), h, sheet)


def main():
    ref_dir, out_dir = sys.argv[1], sys.argv[2]
    report = {}
    for typ, anim in (("EngM18", "STAND"), ("EngKri", "STAND"), ("EngM18", "DEATHLIE1")):
        report[f"{typ}-{anim}"] = compare(ref_dir, out_dir, typ, anim, (0, 2, 4, 6, 8))
    strip(ref_dir, out_dir)
    turntable(out_dir)
    json.dump(report, open(os.path.join(out_dir, "compare.json"), "w"), indent=1)
    print(json.dumps(report))


if __name__ == "__main__":
    main()
