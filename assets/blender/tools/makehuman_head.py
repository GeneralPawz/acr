"""Derive the compact MakeHuman head data for ACR (reads the MakeHuman checkout as data).

usage: python -I assets/blender/tools/makehuman_head.py <makehuman/data dir> <out.json> <commit>
"""
import json
import os
import sys

src, out, commit = sys.argv[1], sys.argv[2], sys.argv[3]


def read_obj(path):
    vs, faces, g = [], [], None
    with open(path) as fh:
        for ln in fh:
            if ln.startswith("v "):
                vs.append([float(t) for t in ln.split()[1:4]])
            elif ln.startswith("g "):
                g = ln.split()[1]
            elif ln.startswith("f "):
                faces.append((g, [int(t.split("/")[0]) - 1 for t in ln.split()[1:]]))
    return vs, faces


def read_target(rel):
    d = {}
    with open(os.path.join(src, "targets", rel)) as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            t = ln.split()
            d[int(t[0])] = (float(t[1]), float(t[2]), float(t[3]))
    return d


def comb(*terms):
    """Sum of weight * target."""
    acc = {}
    for w, rel in terms:
        for i, d in read_target(rel).items():
            a = acc.setdefault(i, [0.0, 0.0, 0.0])
            for k in range(3):
                a[k] += w * d[k]
    return acc


vs, faces = read_obj(os.path.join(src, "3dobjs", "base.obj"))
# A young adult man: race caucasian, gender 1, age 25, average muscle and weight.
for i, d in comb((1.0, "macrodetails/caucasian-male-young.target"),
                 (1.0, "macrodetails/universal-male-young-averagemuscle-averageweight.target")).items():
    for k in range(3):
        vs[i][k] += d[k]

body = [f for g, f in faces if g == "body"]
top = max(vs[i][1] for f in body for i in f)
CUT = top - 2.75  # decimetres: about 4 cm below the chin
head = [f for f in body if all(vs[i][1] > CUT for i in f)]
eyes = [f for g, f in faces if g in ("helper-l-eye", "helper-r-eye")]
used = sorted({i for f in head + eyes for i in f})
remap = {o: n for n, o in enumerate(used)}
eye_set = {i for f in eyes for i in f}


def conv(p):
    # MakeHuman: decimetres, y up, facing +z, the model's left +x.
    # ACR/Blender: metres, z up, facing -y, the model's left +x. Stored in 0.1 mm.
    return [round(p[0] * 1000), round(-p[2] * 1000), round(p[1] * 1000)]


T = {
    "face_jaw": [(0.5, "chin/chin-bones-incr.target"), (-0.5, "chin/chin-bones-decr.target"),
                 (0.25, "head/head-square.target")],
    "face_chin": [(0.5, "chin/chin-prominent-incr.target"), (-0.5, "chin/chin-prominent-decr.target"),
                  (0.25, "chin/chin-height-incr.target"), (-0.25, "chin/chin-height-decr.target")],
    "face_cheekbones": [(0.5, "cheek/l-cheek-bones-incr.target"), (-0.5, "cheek/l-cheek-bones-decr.target"),
                        (0.5, "cheek/r-cheek-bones-incr.target"), (-0.5, "cheek/r-cheek-bones-decr.target")],
    "face_nose": [(0.5, "nose/nose-scale-vert-incr.target"), (-0.5, "nose/nose-scale-vert-decr.target"),
                  (0.35, "nose/nose-scale-depth-incr.target"), (-0.35, "nose/nose-scale-depth-decr.target")],
    "face_nose_bridge": [(0.5, "nose/nose-hump-incr.target"), (-0.5, "nose/nose-hump-decr.target")],
    "face_brow": [(1.0, "eyebrows/eyebrows-trans-forward.target"), (0.5, "eyebrows/eyebrows-trans-down.target"),
                  (0.5, "forehead/forehead-nubian-incr.target")],
    "face_eyes": [(0.5, "eyes/l-eye-trans-out.target"), (-0.5, "eyes/l-eye-trans-in.target"),
                  (0.5, "eyes/r-eye-trans-out.target"), (-0.5, "eyes/r-eye-trans-in.target")],
    "face_age": [(1.0, "macrodetails/caucasian-male-old.target"), (-1.0, "macrodetails/caucasian-male-young.target"),
                 (1.0, "macrodetails/universal-male-old-averagemuscle-averageweight.target"),
                 (-1.0, "macrodetails/universal-male-young-averagemuscle-averageweight.target"),
                 (0.6, "head/head-age-incr.target"),
                 (0.6, "eyes/l-eye-bag-incr.target"), (0.6, "eyes/r-eye-bag-incr.target")],
    "face_fat": [(1.0, "macrodetails/universal-male-young-averagemuscle-maxweight.target"),
                 (-1.0, "macrodetails/universal-male-young-averagemuscle-averageweight.target"),
                 (0.5, "head/head-fat-incr.target")],
    "face_thin": [(1.0, "macrodetails/universal-male-young-averagemuscle-minweight.target"),
                  (-1.0, "macrodetails/universal-male-young-averagemuscle-averageweight.target"),
                  (0.5, "head/head-fat-decr.target"),
                  (0.5, "cheek/l-cheek-volume-decr.target"), (0.5, "cheek/r-cheek-volume-decr.target")],
}
targets = {}
for name, terms in T.items():
    acc = comb(*terms)
    idx, dl = [], []
    for o, d in sorted(acc.items()):
        if o not in remap:
            continue
        q = conv(d)
        if max(map(abs, q)) == 0:
            continue
        idx.append(remap[o])
        dl.extend(q)
    targets[name] = {"i": idx, "d": dl, "sources": [r for _w, r in terms]}

data = {
    "source": "MakeHuman 1.x base mesh (makehuman/data/3dobjs/base.obj, groups body, helper-l-eye, "
              "helper-r-eye) with macro and face targets (makehuman/data/targets)",
    "repo": "https://github.com/makehumancommunity/makehuman", "commit": commit, "licence": "CC0 1.0",
    "units": "0.1 mm; x the model's left, -y forward, z up; a young adult man (caucasian, male, age 25, "
             "average muscle and weight); cut about 4 cm below the chin",
    "verts": [c for o in used for c in conv(vs[o])],
    "eye": [1 if o in eye_set else 0 for o in used],
    "faces": [[remap[i] for i in f] for f in head + eyes],
    "targets": targets,
}
with open(out, "w") as fh:
    json.dump(data, fh, separators=(",", ":"))
print("verts", len(used), "faces", len(head), "+eyes", len(eyes),
      "tris", sum(len(f) - 2 for f in head + eyes), "bytes", os.path.getsize(out))
for n, t in targets.items():
    print(n, len(t["i"]), max(map(abs, t["d"])) if t["d"] else 0)
