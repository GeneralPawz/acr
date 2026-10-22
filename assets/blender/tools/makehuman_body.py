"""Derive the compact MakeHuman body data for ACR (reads the MakeHuman checkout as data).

The body group of the base mesh below the chin (the head comes from `head.json`), as a
young adult man like `makehuman_head.py`, with the centres of the joint helper
cubes (the rest pose `acr_assets.mbody` reposes onto skeleton H).

usage: python -I assets/blender/tools/makehuman_body.py <makehuman/data dir> <out.json> <commit>
"""
import json
import os
import sys

src, out, commit = sys.argv[1], sys.argv[2], sys.argv[3]


def read_obj(path):
    vs, vts, faces, g = [], [], [], None
    with open(path) as fh:
        for ln in fh:
            if ln.startswith("v "):
                vs.append([float(t) for t in ln.split()[1:4]])
            elif ln.startswith("vt "):
                vts.append([float(t) for t in ln.split()[1:3]])
            elif ln.startswith("g "):
                g = ln.split()[1]
            elif ln.startswith("f "):
                tok = [t.split("/") for t in ln.split()[1:]]
                faces.append((g, [int(t[0]) - 1 for t in tok], [int(t[1]) - 1 if len(t) > 1 and t[1] else -1 for t in tok]))
    return vs, vts, faces


def read_target(rel):
    d = {}
    with open(os.path.join(src, "targets", rel)) as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            t = ln.split()
            d[int(t[0])] = (float(t[1]), float(t[2]), float(t[3]))
    return d


vs, vts, faces = read_obj(os.path.join(src, "3dobjs", "base.obj"))
for rel in ("macrodetails/caucasian-male-young.target",
            "macrodetails/universal-male-young-averagemuscle-averageweight.target"):
    for i, d in read_target(rel).items():
        for k in range(3):
            vs[i][k] += d[k]


def conv(p):
    # MakeHuman: decimetres, y up, facing +z; ACR: metres, z up, facing -y. Stored in 0.1 mm.
    return [round(p[0] * 1000), round(-p[2] * 1000), round(p[1] * 1000)]


groups = {}
for g, f, _t in faces:
    groups.setdefault(g, set()).update(f)
joints = {}
for g, s in groups.items():
    if g.startswith("joint-") and not any(k in g for k in ("tongue", "lid", "eye")):
        joints[g[6:]] = conv([sum(vs[i][k] for i in s) / len(s) for k in range(3)])

body = [(f, t) for g, f, t in faces if g == "body"]
top = max(vs[i][1] for f, _t in body for i in f)
skull = [vs[i] for f, _t in body for i in f if vs[i][1] > top - 1.2]
CUT = top - 2.45  # decimetres: the chin; head.json overlaps from 2.75 below the top
keep = [(f, t) for f, t in body if min(vs[i][1] for i in f) < CUT]
used = sorted({i for f, _t in keep for i in f})
remap = {o: n for n, o in enumerate(used)}

data = {
    "source": "MakeHuman 1.x base mesh (makehuman/data/3dobjs/base.obj, group body below the chin, "
              "joint helper centres) with the young male macro targets",
    "repo": "https://github.com/makehumancommunity/makehuman", "commit": commit, "licence": "CC0 1.0",
    "units": "0.1 mm; x the model's left, -y forward, z up",
    "head_top": conv([0.0, top, 0.0])[2],
    "skull_y": conv([0.0, 0.0, 0.5 * (min(p[2] for p in skull) + max(p[2] for p in skull))])[1],
    "joints": joints,
    "verts": [c for o in used for c in conv(vs[o])],
    "faces": [[remap[i] for i in f] for f, _t in keep],
}
with open(out, "w") as fh:
    json.dump(data, fh, separators=(",", ":"))
print("verts", len(used), "faces", len(keep), "tris", sum(len(f) - 2 for f, _t in keep), "bytes", os.path.getsize(out))
