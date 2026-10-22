"""The horse on rig R (needs `bpy`, `bmesh`): body hull lofted from cross sections, legs,
mane and tail, the bone layer for decay (#178), saddle and saddlecloth, and placeholder
poses/clips (stand, rest, walk, trot, fall, lying).

Cross sections are (y, z of the centre, half width, half height) along the body and neck in
rig R's rest space (metres, facing -Y); the legs are tubes along their bones. Weights:
nearest bone segment of rig R blended with its neighbours (`body.skin_weights`).
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from . import armature as am
from . import body as bd
from . import rig

HORSE_PART = 48  # `_part` of every horse vertex (the rider's cuts are 0..41)

# (y, z, half width, half height) from the buttock to the muzzle.
SECTIONS = [
    (0.84, 1.26, 0.09, 0.12), (0.72, 1.27, 0.20, 0.25), (0.48, 1.24, 0.25, 0.30), (0.18, 1.19, 0.25, 0.31),
    (-0.14, 1.20, 0.25, 0.32), (-0.44, 1.25, 0.23, 0.31), (-0.62, 1.32, 0.18, 0.26),
    (-0.74, 1.50, 0.12, 0.20), (-0.86, 1.68, 0.10, 0.15), (-0.97, 1.85, 0.08, 0.11),
    (-1.04, 1.89, 0.08, 0.10), (-1.16, 1.79, 0.075, 0.09), (-1.27, 1.66, 0.06, 0.07), (-1.33, 1.60, 0.045, 0.05),
]
# Leg radii from the top joint down (front: scapula..hoof, back: femur..hoof).
LEG_R = {"front": [0.11, 0.09, 0.07, 0.045, 0.04, 0.05], "back": [0.13, 0.10, 0.06, 0.045, 0.05]}

DEFORM = [b.name for b in rig.rig_r() if b.deform]


def _ring(c, d, hw, hh, n):
    """Ring of n points around centre c, perpendicular to direction d (in the YZ plane for
    the body), half width hw along X and half height hh across."""
    d = d.normalized()
    side = Vector((1, 0, 0))
    up = d.cross(side).normalized()
    if up.z < 0:
        up = -up
    return [c + side * math.cos(2 * math.pi * k / n) * hw + up * math.sin(2 * math.pi * k / n) * hh for k in range(n)]


def _loft(bm, centres, radii, n, dirs=None):
    rings = []
    for i, (c, (hw, hh)) in enumerate(zip(centres, radii)):
        d = (centres[min(i + 1, len(centres) - 1)] - centres[max(i - 1, 0)]) if dirs is None else dirs[i]
        rings.append([bm.verts.new(p) for p in _ring(c, d, hw, hh, n)])
    for a, b in zip(rings, rings[1:]):
        for k in range(n):
            bm.faces.new((a[k], a[(k + 1) % n], b[(k + 1) % n], b[k]))
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])
    return rings


def hull(arm_obj, name, scale=1.0, n=10, sections=None):
    sections = sections or SECTIONS
    bm = bmesh.new()
    cs = [Vector((0, y, z)) for y, z, _w, _h in sections]
    _loft(bm, cs, [(w * scale, h * scale) for _y, _z, w, h in sections], n)
    for leg, (_parent, chain) in rig.HORSE_LEGS.items():
        for side in (1, -1):
            pts = [Vector((h[0] * side, h[1], h[2])) for _n, h, _t in chain] + [Vector((chain[-1][2][0] * side, chain[-1][2][1], 0.0))]
            rs = LEG_R[leg] + [LEG_R[leg][-1]]
            # Down the leg: rings perpendicular to the bone, round.
            dirs = [pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)] for i in range(len(pts))]
            rings = []
            for i, (c, r) in enumerate(zip(pts, rs)):
                d = dirs[i].normalized()
                e1 = d.cross(Vector((1, 0, 0))).normalized()
                e2 = d.cross(e1).normalized()
                rings.append([bm.verts.new(c + (e1 * math.cos(2 * math.pi * k / 7) + e2 * math.sin(2 * math.pi * k / 7)) * r * scale)
                              for k in range(7)])
            for a, b in zip(rings, rings[1:]):
                for k in range(7):
                    bm.faces.new((a[k], a[(k + 1) % 7], b[(k + 1) % 7], b[k]))
            bm.faces.new(rings[-1])
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bd.link(bpy.data.objects.new(name, me))
    for p in me.polygons:
        p.use_smooth = True
    bmo = bmesh.new()
    bmo.from_mesh(me)
    bmesh.ops.recalc_face_normals(bmo, faces=bmo.faces)
    bmo.to_mesh(me)
    bmo.free()
    bd.skin_weights(obj, arm_obj, DEFORM, falloff=3.0)
    rn = me.attributes.new("rest_normal", "FLOAT_VECTOR", "POINT")
    for v in me.vertices:
        rn.data[v.index].vector = v.normal
    part = me.attributes.new("part", "INT", "POINT")
    for d in part.data:
        d.value = HORSE_PART
    return am.bind(obj, arm_obj)


def mane_tail(arm_obj, name, sections=None):
    """Mane along the neck and a tail: thin fins/tubes."""
    sections = sections or SECTIONS
    bm = bmesh.new()
    neck = [Vector((0, y, z + h * 0.9)) for y, z, _w, h in sections[6:11]]
    for a, b in zip(neck, neck[1:]):
        vs = [bm.verts.new(p) for p in (a, b, b + Vector((0, 0.05, 0.10)), a + Vector((0, 0.05, 0.10)))]
        bm.faces.new(vs)
    tail = [Vector(p) for p in ((0, 0.86, 1.30), (0, 0.94, 1.10), (0, 0.98, 0.85), (0, 1.0, 0.62))]
    rs = [0.05, 0.07, 0.08, 0.06]
    rings = []
    for i, (c, r) in enumerate(zip(tail, rs)):
        d = (tail[min(i + 1, 3)] - tail[max(i - 1, 0)]).normalized()
        e1 = Vector((1, 0, 0))
        e2 = d.cross(e1).normalized()
        rings.append([bm.verts.new(c + (e1 * math.cos(2 * math.pi * k / 5) + e2 * math.sin(2 * math.pi * k / 5)) * r) for k in range(5)])
    for a, b in zip(rings, rings[1:]):
        for k in range(5):
            bm.faces.new((a[k], a[(k + 1) % 5], b[(k + 1) % 5], b[k]))
    bm.faces.new(rings[-1])
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bd.link(bpy.data.objects.new(name, me))
    bd.skin_weights(obj, arm_obj, DEFORM, falloff=3.0)
    part = me.attributes.new("part", "INT", "POINT")
    for d in part.data:
        d.value = HORSE_PART
    return am.bind(obj, arm_obj)


def bones(arm_obj, name):
    """Horse skeleton for decay: skull, vertebrae, ribs, pelvis, leg bones."""
    B = bd.BoneMesh(arm_obj)
    seg = B.seg
    h, t = seg["head"]
    m, ln = bd.along(h, t)
    B.add("cube", m, (0.07, 0.09, ln * 0.9), "head")
    for bone in ("spine_01", "spine_02", "chest", "neck_01", "neck_02", "pelvis", "tail_01"):
        B.long_bone(bone, 0.025, 0.035, shrink=0.05)
    for i, y in enumerate((-0.55, -0.42, -0.29, -0.16, -0.03, 0.10)):
        bone = "chest" if y < -0.35 else "spine_02" if y < -0.05 else "spine_01"
        r = bmesh.ops.create_circle(B.bm, cap_ends=False, segments=8, radius=1.0,
                                    matrix=Matrix.Translation((0, y, 1.18)) @ Matrix.Rotation(math.pi / 2, 4, "X"))
        vs = r["verts"]
        for v in vs:
            v.co.x *= 0.22
            v.co.z = 1.18 + (v.co.z - 1.18) * 0.27
        ext = bmesh.ops.extrude_edge_only(B.bm, edges=list({e for v in vs for e in v.link_edges}))
        nv = [g for g in ext["geom"] if isinstance(g, bmesh.types.BMVert)]
        for v in nv:
            v.co.y += 0.03
        for v in vs + nv:
            v[B.lay] = B.names.index(bone)
    B.add("cube", Matrix.Translation((0, 0.55, 1.25)), (0.22, 0.25, 0.12), "pelvis")
    for leg, (_p, chain) in rig.HORSE_LEGS.items():
        for sfx in ("_l", "_r"):
            for n, *_r in chain:
                B.long_bone(f"{leg}_{n}{sfx}", 0.02, 0.03, shrink=0.05, segments=4)
    return B.finish(name, lambda b: HORSE_PART)


def saddlecloth(horse_hull, name, offset=0.025):
    """A shell over the back and flanks (saddlecloth) derived from the horse's hull."""
    def keep(c, n, home, part):
        return -0.45 < c.y < 0.30 and c.z > 1.05 and home in ("spine_01", "spine_02", "chest", "pelvis")
    return bd.derive(horse_hull, name, keep, offset)


def saddle(name):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, -0.10, 1.53)) @ Matrix.Diagonal((0.36, 0.55, 0.10, 1)))
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, -0.34, 1.60)) @ Matrix.Diagonal((0.30, 0.08, 0.14, 1)))
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, 0.15, 1.59)) @ Matrix.Diagonal((0.32, 0.08, 0.12, 1)))
    for sx in (-1, 1):
        bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0.24 * sx, -0.10, 1.12)) @ Matrix.Diagonal((0.02, 0.10, 0.06, 1)))
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    return bd.link(bpy.data.objects.new(name, me))


# ----------------------------------------------------------------------------- poses

# Standing (fitted to EngD18's #STAND with tools/silhouette.py).
STAND = {"head": (2, 0, 0), "neck_01": (-4, 0, 0), "neck_02": (4, 0, 0), "front_humerus_l": (11, 0, 0),
         "front_humerus_r": (11, 0, 0), "back_femur_l": (3, 0, 0), "back_femur_r": (4, 0, 0), "tail_01": (-13, 0, 0)}
REST = dict(STAND, neck_01=(12, 0, 0), neck_02=(10, 0, 0), head=(10, 0, 0), tail_02=(0, 0, 15))


def gait(phase, trot=True):
    """A gait pose at phase 0..1: diagonal pairs (trot) or four beats (walk)."""
    def swing(p, amp, lift):
        s = math.sin(2 * math.pi * p)
        knee = max(0.0, math.cos(2 * math.pi * p)) * lift
        return s * amp, knee

    out = {}
    offs = {"front_l": 0.0, "back_r": 0.0, "front_r": 0.5, "back_l": 0.5} if trot else \
        {"front_l": 0.0, "back_r": 0.25, "front_r": 0.5, "back_l": 0.75}
    amp, lift = (24, 50) if trot else (16, 35)
    for leg, off in offs.items():
        kind, sfx = leg.split("_")
        a, knee = swing(phase + off, amp, lift)
        if kind == "front":
            out[f"front_humerus_{sfx}"] = (-a, 0, 0)
            out[f"front_radius_{sfx}"] = (-knee * 0.9, 0, 0)
            out[f"front_cannon_{sfx}"] = (knee * 0.9, 0, 0)
        else:
            out[f"back_femur_{sfx}"] = (-a, 0, 0)
            out[f"back_tibia_{sfx}"] = (knee * 0.6, 0, 0)
            out[f"back_hock_{sfx}"] = (-knee * 0.6, 0, 0)
    bob = (0.03 if trot else 0.012) * math.cos(4 * math.pi * phase)
    out["root_loc"] = (0, 0, bob)
    out["neck_01"] = (4 * math.sin(4 * math.pi * phase), 0, 0)
    return out


def fallen():
    """Lying on its side, legs out (#DEATHLIE1 of the dragoon; fitted to its sprite)."""
    return {"root": (0, -112, 230), "root_loc": (-1.32, 1.24, 0.31),
            "neck_01": (0, 0, 38), "neck_02": (0, 0, -8), "head": (100, 0, 0),
            "front_humerus_l": (-16, 0, 0), "front_humerus_r": (-35, 0, 0),
            "back_femur_l": (4, 0, 0), "back_femur_r": (30, 0, 0),
            "tail_01": (0, 0, 20)}


def falling():
    return {"root": (12, -40, 0), "root_loc": (-0.4, 0.0, -0.25),
            "front_humerus_l": (-50, 0, 0), "front_radius_l": (-60, 0, 0), "front_humerus_r": (-40, 0, 0),
            "front_radius_r": (-70, 0, 0), "back_femur_l": (-20, 0, 0), "back_femur_r": (-25, 0, 0),
            "neck_01": (-20, 0, 0), "head": (-20, 0, 0)}
