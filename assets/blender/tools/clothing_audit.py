"""Clothing audit over built models (docs/clothing.md "Clothing audit", #523).

    python3 assets/blender/tools/clothing_audit.py <glb or folder>... --out <dir> [--jobs N]
        [--stats <folder with <unit>.stats.json>]

Plain Python with numpy, no Blender: reads each glb, skins it in `STAND` and in the middle of
the walk clip, and writes

- `audit.csv`: one row per unit and garment (`acr_assets.clothaudit.COLUMNS`);
- `shape.csv`: the cut of the legwear per unit (`clothaudit.leg_shape`: girths along the
  left leg, fullness above the knee band, closure at it, the bare gap);
- `ranking.md`: the worst units, the worst garments counted across units, and the flag
  counts.

With `--stats`, a garment named in a unit's `stats.json` `drape` is `draped` whatever its
triangles look like (the glb does not say how a garment was made).
"""

import argparse
import csv
import json
import os
import sys
from concurrent.futures import ProcessPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

from acr_assets import clothaudit  # noqa: E402


def one(job):
    path, stats = job
    unit = os.path.splitext(os.path.basename(path))[0]
    try:
        shape = {}
        rows, note = clothaudit.audit(path, unit, shape)
    except Exception as e:  # a broken file must not stop the run
        return unit, [], f"error: {type(e).__name__}: {e}", {}
    sj = os.path.join(stats, unit + ".stats.json") if stats else None
    if sj and os.path.exists(sj):
        with open(sj, encoding="utf-8") as f:
            draped = set((json.load(f).get("drape") or {}).keys())
        for r in rows:
            if r["garment"] in draped:
                r["kind"] = "draped"
    return unit, rows, note, shape


def table(head, rows):
    out = ["| " + " | ".join(head) + " |", "|" + "---|" * len(head)]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out)


def ranking(rows, notes, top=25):
    """Markdown: worst units, worst garments across units, flag counts."""
    units, garm, flag_n = {}, {}, {}
    for r in rows:
        u = units.setdefault(r["unit"], {"score": 0.0, "flags": 0, "worst": ("", 0.0)})
        u["score"] += r["score"]
        u["flags"] += len(r["flags"].split(";")) if r["flags"] else 0
        if r["score"] > u["worst"][1]:
            u["worst"] = (r["garment"], r["score"])
        if r["kind"] == "skin":
            continue
        g = garm.setdefault(r["garment"], {"units": 0, "flagged": 0, "score": 0.0, "kinds": set(), "flags": {}})
        g["units"] += 1
        g["score"] += r["score"]
        g["kinds"].add(r["kind"])
        if r["flags"]:
            g["flagged"] += 1
            for f in r["flags"].split(";"):
                g["flags"][f] = g["flags"].get(f, 0) + 1
                flag_n[f] = flag_n.get(f, 0) + 1
    md = [f"Units with garments: {len(units)}; rows: {len(rows)}; without separable garments: "
          f"{sum(1 for n in notes.values() if n and not n.startswith('no skin mesh'))}.", "",
          "## Worst units (sum of the garment scores)", "",
          table(["unit", "score", "flags", "worst garment"],
                [(k, round(v["score"], 1), v["flags"], f"{v['worst'][0]} ({v['worst'][1]})")
                 for k, v in sorted(units.items(), key=lambda kv: -kv[1]["score"])[:top]]), "",
          "## Worst garments across units (sum of scores)", "",
          table(["garment", "kind", "units", "flagged", "score sum", "mean", "flags (units)"],
                [(k, "/".join(sorted(v["kinds"])), v["units"], v["flagged"], round(v["score"], 1),
                  round(v["score"] / v["units"], 1),
                  ", ".join(f"{f} {n}" for f, n in sorted(v["flags"].items(), key=lambda fn: -fn[1])))
                 for k, v in sorted(garm.items(), key=lambda kv: -kv[1]["score"])[:top + 15]]), "",
          "## Flags", "", table(["flag", "rows"], sorted(flag_n.items(), key=lambda fn: -fn[1])), ""]
    return "\n".join(md)


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--out", required=True)
    ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    ap.add_argument("--stats", default=None)
    a = ap.parse_args(argv)
    files = []
    for p in a.paths:
        p = os.path.abspath(p)
        if os.path.isdir(p):
            files += sorted(os.path.join(p, f) for f in os.listdir(p) if f.lower().endswith(".glb"))
        else:
            files.append(p)
    os.makedirs(a.out, exist_ok=True)
    rows, notes, shapes = [], {}, []
    with ProcessPoolExecutor(a.jobs) as ex:
        for unit, r, note, shape in ex.map(one, [(f, a.stats) for f in files]):
            rows += r
            notes[unit] = note
            if shape:
                shapes.append(shape)
    with open(os.path.join(a.out, "shape.csv"), "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, clothaudit.SHAPE_COLUMNS)
        w.writeheader()
        w.writerows(sorted(shapes, key=lambda s: s["unit"]))
    rows.sort(key=lambda r: (r["unit"], r["garment"]))
    with open(os.path.join(a.out, "audit.csv"), "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, clothaudit.COLUMNS)
        w.writeheader()
        w.writerows(rows)
    with open(os.path.join(a.out, "skipped.txt"), "w", encoding="utf-8") as f:
        for u, n in sorted(notes.items()):
            if n:
                f.write(f"{u}\t{n}\n")
    with open(os.path.join(a.out, "ranking.md"), "w", encoding="utf-8") as f:
        f.write(ranking(rows, notes))
    print(f"clothing audit: {len(files)} models, {len(rows)} rows, {sum(1 for r in rows if r['flags'])} flagged "
          f"-> {a.out}")


if __name__ == "__main__":
    main(sys.argv[1:])
