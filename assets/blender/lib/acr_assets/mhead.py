"""The MakeHuman head (#283 pass 4, needs `bpy`, `bmesh`, `numpy`): the head and upper neck
of the MakeHuman 1.x base mesh (CC0, `assets/blender/data/makehuman/head.json`, derived by
`assets/blender/tools/makehuman_head.py`; see assets/LICENSES.md) replaces the egg of the skin hull.

- Placed at real size on skeleton H's head: the top of the skull on the hull's top, the
  skull's depth centred on the hull's, the neck narrowed or widened at the bottom so it
  sinks into the hull's neck (the hull keeps everything below the cut, the head's neck
  overlaps it by `OVERLAP`).
- Decimated (collapse, mirror-symmetric) to about `TARGET_TRIS`.
- The `face_*` morphs come from MakeHuman targets (`MorphField`): a field over the full-
  resolution head, so it reaches the decimated skin and every shell near the face.
- `canon` maps the head's landmarks (eyes, nose tip, mouth) to the head-local layout
  `faces.py` paints and zones in.
"""

import json
import math
import os

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.kdtree import KDTree

DATA = os.path.join(os.path.dirname(__file__), "..", "..", "data", "makehuman", "head.json")
TARGET_TRIS = 1300
OVERLAP = 0.018  # metres of the head's neck inside the hull's neck
# Weight-1 strength of each target (MakeHuman's full weights are caricatures).
GAIN = {"face_jaw": 0.8, "face_chin": 0.7, "face_cheekbones": 1.0, "face_nose": 0.8, "face_nose_bridge": 1.0,
        "face_brow": 0.35, "face_eyes": 0.6, "face_age": 0.8, "face_fat": 0.8, "face_thin": 0.9}
# Head-local landmarks of the painted layout (`faces.py`, `heads.paint_face`).
CANON = {"nose_tip": -0.034, "mouth": -0.0605, "chin": -0.105}

_cache = {}


def load():
    if "d" not in _cache:
        with open(DATA) as fh:
            d = json.load(fh)
        d["V"] = np.array(d["verts"], np.float64).reshape(-1, 3) * 1e-4
        d["E"] = np.array(d["eye"], bool)
        _cache["d"] = d
    return _cache["d"]


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


class Placement:
    """Rest positions of the MakeHuman head in armature space."""

    def __init__(self, V, E, cx, top, y_mid, neck):
        skin = V[~E]
        self.off = np.array([cx - 0.5 * (skin[:, 0].min() + skin[:, 0].max()),
                             y_mid - 0.5 * (skin[skin[:, 2] > skin[:, 2].max() - 0.12, 1].min()
                                            + skin[skin[:, 2] > skin[:, 2].max() - 0.12, 1].max()),
                             top - skin[:, 2].max()])
        self.bottom = skin[:, 2].min() + self.off[2]
        P = V + self.off
        # Neck fit: at the bottom ring scale x/y to the hull's neck (slightly inside).
        ring = P[(~E) & (P[:, 2] < self.bottom + 0.01)]
        mc = np.array([0.5 * (ring[:, 0].min() + ring[:, 0].max()), 0.5 * (ring[:, 1].min() + ring[:, 1].max())])
        mr = np.array([0.5 * (ring[:, 0].max() - ring[:, 0].min()), 0.5 * (ring[:, 1].max() - ring[:, 1].min())])
        hc, hr = neck
        self.neck = (mc, mr, np.array(hc), np.array(hr) * 0.96)
        self.P = self.warp(P)

    def warp(self, P):
        mc, mr, hc, hr = self.neck
        t = smooth(self.bottom + 0.045, self.bottom, P[:, 2])[:, None]  # 1 at the bottom
        xy = P[:, :2]
        fit = hc + (xy - mc) * (hr / mr)
        out = P.copy()
        out[:, :2] = xy * (1 - t) + fit * t
        return out


def landmarks(P, E):
    """Eye level and centres, face front, nose tip, mouth line, chin (armature space)."""
    eyes = P[E]
    l, r = eyes[eyes[:, 0] > np.median(eyes[:, 0])], eyes[eyes[:, 0] <= np.median(eyes[:, 0])]
    ez = 0.5 * (l[:, 2].mean() + r[:, 2].mean())
    cx = 0.5 * (l[:, 0].mean() + r[:, 0].mean())
    skin = P[~E]
    mid = skin[np.abs(skin[:, 0] - cx) < 0.004]
    face = mid[(mid[:, 2] < ez + 0.01) & (mid[:, 2] > ez - 0.14)]
    tip = face[np.argmin(face[:, 1])]
    below = face[(face[:, 2] < tip[2] - 0.012) & (face[:, 2] > tip[2] - 0.05) & (face[:, 1] < tip[1] + 0.035)]
    mouth = below[np.argmax(below[:, 1])] if len(below) else tip - (0, 0, 0.03)
    lower = face[(face[:, 2] < mouth[2] - 0.015) & (face[:, 1] < tip[1] + 0.05)]
    chin = lower[np.argmin(lower[:, 2] + 0.0 * lower[:, 1])] if len(lower) else mouth - (0, 0, 0.045)
    front = skin[(np.abs(skin[:, 0] - cx) < 0.03) & (np.abs(skin[:, 2] - ez) < 0.01)]
    return {"eye_z": float(ez), "cx": float(cx), "y_front": float(front[:, 1].min()),
            "nose_tip": float(tip[2] - ez), "mouth": float(mouth[2] - ez), "chin": float(chin[2] - ez),
            "eye_x": float(0.5 * (l[:, 0].mean() - r[:, 0].mean()))}


def canon_keys(lm):
    """Piecewise-linear z map (head z -> painted layout z) through the landmarks."""
    src = [-1.0, lm["chin"], lm["mouth"], lm["nose_tip"], 0.0, 1.0]
    dst = [-1.0 - lm["chin"] + CANON["chin"], CANON["chin"], CANON["mouth"], CANON["nose_tip"], 0.0, 1.0]
    return src, dst


def canon_z(keys, z):
    """Head z (from eye level) to the painted layout's z; linear outside the keys."""
    src, dst = keys
    return float(np.interp(z, src, dst))


def decimate(verts, faces, tris=TARGET_TRIS):
    """Collapse-decimates (mirror in x) a mesh; returns (verts, faces) as lists."""
    me = bpy.data.meshes.new("mh_head.tmp")
    me.from_pydata([tuple(v) for v in verts], [], faces)
    me.validate()
    ob = bpy.data.objects.new("mh_head.tmp", me)
    bpy.context.scene.collection.objects.link(ob)
    n = sum(len(f) - 2 for f in faces)
    mod = ob.modifiers.new("dec", "DECIMATE")
    mod.decimate_type = "COLLAPSE"
    mod.ratio = min(1.0, tris / n)
    mod.use_symmetry = True
    mod.symmetry_axis = "X"
    mod.use_collapse_triangulate = False
    dg = bpy.context.evaluated_depsgraph_get()
    out = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    vs = [tuple(v.co) for v in out.vertices]
    fs = [tuple(p.vertices) for p in out.polygons]
    bpy.data.objects.remove(ob)
    bpy.data.meshes.remove(me)
    bpy.data.meshes.remove(out)
    return vs, fs


class MorphField:
    """`face_*` displacements at weight 1 anywhere near the head: inverse-distance mix of
    the nearest full-resolution MakeHuman vertices' target deltas, fading out 4..20 mm off
    the skin and towards the bottom of the neck (the seam stays put)."""

    def __init__(self, P, targets, bottom, mirror_x=None):
        self.P = P
        self.bottom = bottom
        self.D = {}
        for name, t in targets.items():
            d = np.zeros_like(P)
            if t["i"]:
                d[np.array(t["i"])] = np.array(t["d"], np.float64).reshape(-1, 3) * 1e-4 * GAIN.get(name, 1.0)
            self.D[name] = d
        self.kd = KDTree(len(P))
        for i, p in enumerate(P):
            self.kd.insert(p, i)
        self.kd.balance()

    def at(self, names, co):
        """{name: (dx, dy, dz)} at armature-space point `co`."""
        near = self.kd.find_n(co, 4)
        if not near or near[0][2] > 0.02:
            return None
        k = float(smooth(0.02, 0.004, near[0][2])) * float(smooth(self.bottom + OVERLAP, self.bottom + OVERLAP + 0.03, co[2]))
        if k <= 0.0:
            return None
        w = np.array([1.0 / max(d, 1e-4) for _c, _i, d in near])
        w = w / w.sum() * k
        ids = [i for _c, i, _d in near]
        return {n: tuple((self.D[n][ids] * w[:, None]).sum(0)) for n in names}


def skull(bm, cx):
    """Top and the depth centre of the hull's skull: where the MakeHuman head goes."""
    hv = np.array([tuple(v.co) for v in bm.verts])
    top = hv[:, 2].max()
    headv = hv[(hv[:, 2] > top - 0.16) & (np.abs(hv[:, 0] - cx) < 0.15)]
    return float(top), float(0.5 * (headv[:, 1].min() + headv[:, 1].max()))


def build(bm, cx, frame_cls, body=None):
    """Hull hook: removes the hull's head above the cut and puts in the decimated MakeHuman
    head. `body(bm)` may replace the hull below (`mbody.build`, #287) once the skull's
    place is measured. Returns (frame, field, landmarks)."""
    d = load()
    top, y_mid = skull(bm, cx)
    if body:
        body(bm)
    hv = np.array([tuple(v.co) for v in bm.verts])
    V, E = d["V"], d["E"]
    skin = V[~E]
    bottom = top - (skin[:, 2].max() - skin[:, 2].min())
    cut = bottom + OVERLAP
    band = hv[np.abs(hv[:, 2] - cut) < 0.012]
    band = band[np.abs(band[:, 0] - cx) < 0.12]
    neck = ((0.5 * (band[:, 0].min() + band[:, 0].max()), 0.5 * (band[:, 1].min() + band[:, 1].max())),
            (0.5 * (band[:, 0].max() - band[:, 0].min()), 0.5 * (band[:, 1].max() - band[:, 1].min())))
    pl = Placement(V, E, cx, top, y_mid, neck)
    P = pl.P
    # Remove the hull above the cut.
    gone = [f for f in bm.faces if f.calc_center_median().z > cut]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context="VERTS")
    vs, fs = decimate(P, d["faces"])
    base = [bm.verts.new(v) for v in vs]
    bm.verts.ensure_lookup_table()
    for f in fs:
        try:
            bm.faces.new([base[i] for i in f])
        except ValueError:
            pass
    lm = landmarks(P, E)
    fr = frame_cls(lm["cx"], lm["y_front"], lm["eye_z"])
    fr.keys = canon_keys(lm)
    field = MorphField(P, d["targets"], pl.bottom)
    _cache["tris"] = sum(len(f) - 2 for f in fs)
    _cache["landmarks"] = lm
    return fr, field, lm
