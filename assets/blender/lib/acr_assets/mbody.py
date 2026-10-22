"""The MakeHuman body (#287, needs `bpy`, `numpy`): the body of the MakeHuman 1.x base mesh
below the chin (CC0, `assets/blender/data/makehuman/body.json`, derived by
`assets/blender/tools/makehuman_body.py`; see assets/LICENSES.md) is the skin hull of
skeleton H: real feet (heel, arch, toes), hands with fingers, shoulders, neck and limbs.

- Reposed onto skeleton H's rest pose: the torso is mapped through its heights (hip joint
  onto `thigh` heads, shoulder joint onto `upperarm` heads, the neck rises with the
  shoulders); every limb segment (upper arm, forearm, hand, thigh, shin, foot) is rotated,
  stretched along its axis and moved onto its bone. The segments are blended per vertex
  by inverse distance to the MakeHuman joint segments, so the joints stay smooth (the
  MakeHuman rest pose is already close to H's A-pose).
- Decimated (collapse, mirror-symmetric) to about `TARGET_TRIS`; the fingers merge into
  mittens with a thumb at this budget.
- The head is added afterwards by `mhead.build`, which cuts the neck at its seam.
"""

import json
import os

import bmesh
import bpy
import numpy as np

from . import mhead

DATA = os.path.join(os.path.dirname(__file__), "..", "..", "data", "makehuman", "body.json")
TARGET_TRIS = 2000

_cache = {}


def load():
    if "d" not in _cache:
        with open(DATA) as fh:
            d = json.load(fh)
        d["V"] = np.array(d["verts"], np.float64).reshape(-1, 3) * 1e-4
        d["J"] = {k: np.array(v, np.float64) * 1e-4 for k, v in d["joints"].items()}
        _cache["d"] = d
    return _cache["d"]


def _rot(a, b):
    """Minimal rotation matrix turning unit vector a onto unit vector b."""
    v = np.cross(a, b)
    c = float(np.dot(a, b))
    if np.linalg.norm(v) < 1e-9:
        return np.eye(3)
    k = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.eye(3) + k + k @ k * (1.0 / (1.0 + c))


def _seg_dist(P, a, b):
    d = b - a
    t = np.clip(((P - a) @ d) / max(float(d @ d), 1e-12), 0.0, 1.0)
    return np.linalg.norm(P - (a + t[:, None] * d), axis=1)


def segments_mh(J):
    """MakeHuman joint segments per region: {region: (a, b)}; the torso has several."""
    s = {"torso": [(J["pelvis"], J["neck"]), (J["neck"], J["head"])]}
    for side, sfx in (("l", "_l"), ("r", "_r")):
        s["torso"] += [(J["neck"], J[f"{side}-shoulder"]), (J["pelvis"], J[f"{side}-upper-leg"])]
        s["upperarm" + sfx] = [(J[f"{side}-shoulder"], J[f"{side}-elbow"])]
        s["forearm" + sfx] = [(J[f"{side}-elbow"], J[f"{side}-hand"])]
        s["hand" + sfx] = [(J[f"{side}-hand"], J[f"{side}-finger-3-1"]),
                           (J[f"{side}-finger-3-1"], J[f"{side}-finger-3-4"]),
                           (J[f"{side}-hand"], J[f"{side}-finger-1-4"]),
                           (J[f"{side}-finger-2-1"], J[f"{side}-finger-2-4"]),
                           (J[f"{side}-finger-4-1"], J[f"{side}-finger-4-4"]),
                           (J[f"{side}-finger-5-1"], J[f"{side}-finger-5-4"])]
        s["thigh" + sfx] = [(J[f"{side}-upper-leg"], J[f"{side}-knee"])]
        s["shin" + sfx] = [(J[f"{side}-knee"], J[f"{side}-ankle"])]
        s["foot" + sfx] = [(J[f"{side}-ankle"], J[f"{side}-foot-1"]), (J[f"{side}-foot-1"], J[f"{side}-foot-2"])]
    return s


# H bone of each limb region and the MakeHuman joints it is mapped from.
LIMBS = {"upperarm": ("shoulder", "elbow"), "forearm": ("elbow", "hand"), "hand": ("hand", "finger-3-1"),
         "thigh": ("upper-leg", "knee"), "shin": ("knee", "ankle"), "foot": ("ankle", "foot-1")}


def repose(seg, prefix="", build=None):
    """Full-resolution MakeHuman body vertices in skeleton H's rest pose (armature space,
    `seg` = `armature.segments`). Returns (V, faces)."""
    b = build or {}
    d = load()
    V, J = d["V"], d["J"]
    P = prefix
    hips = np.array(seg[P + "hips"][0])
    o = hips - np.array((0.0, 0.0, 0.95))  # rig origin (rider rigs are offset)
    hip_mh = 0.5 * (J["l-upper-leg"][2] + J["r-upper-leg"][2])
    sh_mh = 0.5 * (J["l-shoulder"][2] + J["r-shoulder"][2])
    hip_h = float(seg[P + "thigh_l"][0][2])
    sh_h = float(seg[P + "upperarm_l"][0][2])
    k = (sh_h - hip_h) / (sh_mh - hip_mh)
    w = b.get("width", 1.0)
    limb_r = 1.0 + (b.get("limbs", 1.0) - 1.0) * 0.35

    def torso(p):
        q = p.copy()
        z = p[:, 2]
        q[:, 2] = np.where(z < sh_mh, hip_h + (z - hip_mh) * k, sh_h + (z - sh_mh))
        q[:, 0] = p[:, 0] * w + o[0]
        q[:, 1] = p[:, 1] + o[1]
        return q

    out = {"torso": torso(V)}
    for side, sfx in (("l", "_l"), ("r", "_r")):
        for reg, (ja, jb) in LIMBS.items():
            a, bb = J[f"{side}-{ja}"], J[f"{side}-{jb}"]
            ha, hb = (np.array(x) for x in seg[P + reg + sfx])
            if reg == "hand":  # H's hand bone ends at the knuckles' middle
                hb = ha + (hb - ha) * 1.15
            dm, dh = bb - a, hb - ha
            lm, lh = np.linalg.norm(dm), np.linalg.norm(dh)
            um, uh = dm / lm, dh / lh
            R = _rot(um, uh)
            rel = V - a
            along = rel @ um
            cross = rel - along[:, None] * um
            r = limb_r if reg in ("upperarm", "forearm", "thigh", "shin") else 1.0
            loc = along[:, None] * um * (lh / lm) + cross * r
            out[reg + sfx] = ha + loc @ R.T
    segs = segments_mh(J)
    names = list(out)
    D = np.stack([np.min([_seg_dist(V, a, c) for a, c in segs[n]], axis=0) for n in names], axis=1)
    W = 1.0 / (D + 0.012) ** 6
    # The belly and the buttocks above the hip joints stay with the torso (a seated rider's
    # thighs would otherwise drag the belly forward).
    up = mhead.smooth(hip_mh - 0.02, hip_mh + 0.06, V[:, 2])
    for i, n in enumerate(names):
        if n.startswith("thigh"):
            W[:, i] *= 1.0 - up
    W /= W.sum(1, keepdims=True)
    Q = np.zeros_like(V)
    for i, n in enumerate(names):
        Q += W[:, i:i + 1] * out[n]
    return Q, d["faces"]


def build(bm, seg, prefix="", build=None, tris=TARGET_TRIS):
    """Replaces the hull in `bm` with the reposed, decimated MakeHuman body."""
    Q, faces = repose(seg, prefix, build)
    vs, fs = mhead.decimate(Q, faces, tris)
    bm.clear()
    base = [bm.verts.new(v) for v in vs]
    bm.verts.ensure_lookup_table()
    for f in fs:
        try:
            bm.faces.new([base[i] for i in f])
        except ValueError:
            pass
    # The decimated trapezius and acromion stand up in points: relax the shoulder line.
    sh = float(seg[prefix + "upperarm_l"][0][2])
    zone = [v for v in bm.verts if v.co.z > sh - 0.04 and abs(v.co.x - seg[prefix + "hips"][0][0]) < 0.26]
    for _ in range(4):
        bmesh.ops.smooth_vert(bm, verts=zone, factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    _cache["tris"] = sum(len(f) - 2 for f in fs)
    return bm


def belly_weights(obj, arm_obj, prefix=""):
    """The lower belly and the buttocks above the hip joints belong to the hips, not the
    thighs: fades the thigh weights out from 3 cm below to 6 cm above the thigh's head and
    gives the rest to `hips` (a seated rider's belly would bulge forward)."""
    from . import armature as am

    seg = am.segments(arm_obj)
    hips = obj.vertex_groups.get(prefix + "hips")
    for sfx in ("_l", "_r"):
        g = obj.vertex_groups.get(prefix + "thigh" + sfx)
        if g is None or hips is None:
            continue
        z0 = seg[prefix + "thigh" + sfx][0].z
        for v in obj.data.vertices:
            w = next((e.weight for e in v.groups if e.group == g.index), 0.0)
            if w <= 0.0:
                continue
            k = float(mhead.smooth(z0 - 0.03, z0 + 0.06, v.co.z))
            if k <= 0.0:
                continue
            h = next((e.weight for e in v.groups if e.group == hips.index), 0.0)
            g.add([v.index], w * (1.0 - k), "REPLACE")
            hips.add([v.index], h + w * k, "REPLACE")


FOOTWEAR = ("gaiters", "boots", "shoes", "boots_high", "stockings")


def shoes(arm_obj, flesh, name, prefix="", offset=0.007, sole=0.012, heel=0.012):
    """Both shoes as one closed mesh over a last: per side the convex hull of the skin of
    the foot below the ankle (heel, arch, toe box in one smooth shape), `offset` above the
    skin, a `sole` under it and a low `heel` at the back. Smooth weights on shin, foot and
    toe. The skin inside the shoes is deleted (no toe can show through)."""
    from mathutils import Vector

    from . import armature as am
    from . import body as bd
    from . import rig

    seg = am.segments(arm_obj)
    bones = list(flesh["acr_home_bones"])
    hs = [bones[d.value] for d in flesh.data.attributes["home"].data]
    bm = bmesh.new()
    hulls = []
    for sfx in ("_l", "_r"):
        foot, toe, shin = prefix + "foot" + sfx, prefix + "toe" + sfx, prefix + "shin" + sfx
        ankle = seg[foot][0].z + 0.055
        pts = [v.co.copy() for v in flesh.data.vertices
               if hs[v.index] in (foot, toe) or (hs[v.index] == shin and v.co.z < ankle)]
        pts = [p for p in pts if p.z < ankle]
        if len(pts) < 8:
            continue
        lo = min(p.z for p in pts)
        back = max(p.y for p in pts)
        tmp = bmesh.new()
        vs = [tmp.verts.new(p) for p in pts]
        out = bmesh.ops.convex_hull(tmp, input=vs)
        bmesh.ops.delete(tmp, geom=list({g for g in out["geom_interior"] + out["geom_unused"] if isinstance(g, bmesh.types.BMVert)}),
                         context="VERTS")
        bmesh.ops.recalc_face_normals(tmp, faces=tmp.faces)
        tmp.normal_update()
        planes = [(f.verts[0].co.copy(), f.normal.copy()) for f in tmp.faces]
        moved = {v: v.co + v.normal * offset for v in tmp.verts}
        for v, c in moved.items():
            v.co = c
            if v.co.z < lo + 0.012:  # the sole, thicker under the heel
                h = max(0.0, min(1.0, (v.co.y - (back - 0.09)) / 0.04))
                v.co.z = lo - sole - heel * h
        hulls.append((planes, offset + 0.002))
        me = bpy.data.meshes.new("shoe.tmp")
        tmp.to_mesh(me)
        tmp.free()
        bm.from_mesh(me)
        bpy.data.meshes.remove(me)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = False
    obj = bd.link(bpy.data.objects.new(name, me))
    fb = [prefix + b for b in ("shin_l", "foot_l", "toe_l", "shin_r", "foot_r", "toe_r")]
    hs2 = bd.skin_weights(obj, arm_obj, fb)
    attr = me.attributes.new("part", "INT", "POINT")
    for i, h in enumerate(hs2):
        attr.data[i].value = rig.part_of_bone(h[len(prefix):])
    # Delete the skin inside the shoes.
    fm = flesh.data
    fbm = bmesh.new()
    fbm.from_mesh(fm)

    def inside(co):
        return any(all((co - a).dot(n) < tol for a, n in planes) for planes, tol in hulls)

    gone = [f for f in fbm.faces if all(inside(Vector(v.co)) for v in f.verts)]
    bmesh.ops.delete(fbm, geom=gone, context="FACES")
    bmesh.ops.delete(fbm, geom=[v for v in fbm.verts if not v.link_faces], context="VERTS")
    fbm.to_mesh(fm)
    fbm.free()
    fm.update()
    return am.bind(obj, arm_obj)
