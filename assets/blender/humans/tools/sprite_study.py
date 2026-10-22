"""Sprite study for the human models (pure Python, run with `python -I`).

Reads reference views written by `acr model-ref <game> <TYPE> <dir> --anim NAME` (ref.json +
ref_k.png, one per stored direction, anchored on the ground point) and prints, per view, the
extent of the opaque pixels relative to the anchor and the dominant colours; writes an
enlarged contact sheet per reference folder.

    python -I sprite_study.py OUT_DIR REF_DIR [REF_DIR ...]
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import png  # noqa: E402

SCALE = 4


def study(ref_dir):
    ref = json.load(open(os.path.join(ref_dir, "ref.json")))
    ax, ay = ref["anchor"]
    out = {"type": ref["type"], "anim": ref["anim"], "anchor": ref["anchor"], "views": []}
    colours = {}
    images = []
    for v in ref["views"]:
        w, h, rows = png.read(os.path.join(ref_dir, v["image"]))
        images.append(rows)
        xs, ys = [], []
        for y, r in enumerate(rows):
            for x in range(w):
                p = r[x * 4:x * 4 + 4]
                if p[3] >= 128:
                    xs.append(x)
                    ys.append(y)
                    q = tuple(c >> 5 << 5 for c in p[:3])
                    colours[q] = colours.get(q, 0) + 1
        if not xs:
            continue
        out["views"].append({
            "k": v["k"], "dir": v["dir"],
            "above_ground_px": ay - min(ys), "below_ground_px": max(ys) + 1 - ay,
            "left_px": ax - min(xs), "right_px": max(xs) + 1 - ax,
            "opaque_px": len(xs),
        })
    total = sum(colours.values()) or 1
    out["colours"] = [
        {"rgb": "#%02x%02x%02x" % c, "share": round(n / total, 3)}
        for c, n in sorted(colours.items(), key=lambda kv: -kv[1])[:12]
    ]
    # Contact sheet: all views side by side, enlarged, on grey, anchor row marked.
    w, h = ref["width"], ref["height"]
    sheet = png.blank(len(images) * (w + 2) * SCALE, h * SCALE, (96, 104, 96, 255))
    for i, rows in enumerate(images):
        png.paste(sheet, rows, i * (w + 2) * SCALE, 0, SCALE)
        for x in range(w * SCALE):
            sx = i * (w + 2) * SCALE + x
            sheet[ay * SCALE][sx * 4:sx * 4 + 4] = bytearray((255, 255, 0, 255))
    return out, sheet


def main():
    out_dir = sys.argv[1]
    os.makedirs(out_dir, exist_ok=True)
    report = []
    for d in sys.argv[2:]:
        r, sheet = study(d)
        report.append(r)
        name = f"{r['type']}-{r['anim'].lstrip('#')}"
        png.write(os.path.join(out_dir, name + "-sheet.png"), len(sheet[0]) // 4, len(sheet), sheet)
        print(name, "anchor", r["anchor"])
        for v in r["views"]:
            print("  k=%d dir=%3d up=%3d down=%2d left=%3d right=%3d px=%d" % (
                v["k"], v["dir"], v["above_ground_px"], v["below_ground_px"],
                v["left_px"], v["right_px"], v["opaque_px"]))
        print("  colours", " ".join(f"{c['rgb']}:{c['share']}" for c in r["colours"]))
    json.dump(report, open(os.path.join(out_dir, "sprite_study.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
