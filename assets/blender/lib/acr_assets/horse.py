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
from . import coat, decay, rig

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
    for tip, base in ears(sections):
        _ear(bm, base, tip)
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


POLL = 10  # section index of the poll (top of the head behind the ears)


def _frame(sections, i, t=0.0):
    """Centre, half width, half height and the ring's up vector of the head/body section
    i (+ t towards i + 1)."""
    a, b = sections[i], sections[min(i + 1, len(sections) - 1)]
    y, z, hw, hh = (a[k] + (b[k] - a[k]) * t for k in range(4))
    cs = [Vector((0, s[0], s[1])) for s in sections]
    d = (cs[min(i + 1, len(cs) - 1)] - cs[max(i - 1, 0)]).normalized()
    up = d.cross(Vector((1, 0, 0))).normalized()
    if up.z < 0:
        up = -up
    return Vector((0, y, z)), hw, hh, up


def surface(sections, i, t, angle_deg, side=1):
    """A point on the section ellipse: angle 0 = side, 90 = top."""
    c, hw, hh, up = _frame(sections, i, t)
    a = math.radians(angle_deg)
    return c + Vector((side * math.cos(a) * hw, 0, 0)) + up * math.sin(a) * hh


def ears(sections):
    """(tip, base) of both ears: on the poll, pricked up and a little forward."""
    out = []
    for side in (1, -1):
        base = surface(sections, POLL, 0.15, 62, side)
        out.append((base + Vector((0.035 * side, -0.05, 0.15)), base))
    return out


def _ear(bm, base, tip, r=0.034, n=5):
    d = (tip - base).normalized()
    e1 = d.cross(Vector((0, 1, 0))).normalized()
    e2 = d.cross(e1).normalized()
    ring = [bm.verts.new(base + (e1 * math.cos(2 * math.pi * k / n) * 0.8 + e2 * math.sin(2 * math.pi * k / n)) * r)
            for k in range(n)]
    mid = [bm.verts.new(base + (tip - base) * 0.55 + (v.co - base) * 0.75) for v in ring]
    t = bm.verts.new(tip)
    for k in range(n):
        j = (k + 1) % n
        bm.faces.new((ring[k], ring[j], mid[j], mid[k]))
        bm.faces.new((mid[k], mid[j], t))
    bm.faces.new(list(reversed(ring)))


def landmarks(sections):
    """Rest-space points the coat painter needs (coat.paint_hide)."""
    eyes = [tuple(surface(sections, POLL, 0.55, 38, side)) for side in (1, -1)]
    tip = sections[-1]
    bits = [tuple(surface(sections, len(sections) - 2, 0.5, -25, side)) for side in (1, -1)]
    return {"knee": 0.50, "hoof": 0.075, "eyes": eyes, "ears": [tuple(t) for t, _b in ears(sections)],
            "muzzle": tip[0] - 0.03, "brass": [(x, y, z, 0.03) for x, y, z in bits]}


def bridle(arm_obj, name, sections, width=0.024, push=0.012):
    """Headstall: crown piece behind the ears, noseband, cheek pieces; bit rings at the
    mouth corners. Skinned like the hull, so it follows the head."""
    bm = bmesh.new()

    def band(i, t, n=12):
        c, hw, hh, up = _frame(sections, i, t)
        fwd = up.cross(Vector((1, 0, 0))).normalized() * (width / 2)
        rings = []
        for off in (-fwd, fwd):
            rings.append([bm.verts.new(c + off + Vector((math.cos(2 * math.pi * k / n) * (hw + push), 0, 0))
                                       + up * math.sin(2 * math.pi * k / n) * (hh + push)) for k in range(n)])
        for k in range(n):
            j = (k + 1) % n
            bm.faces.new((rings[0][k], rings[0][j], rings[1][j], rings[1][k]))

    def strap(a, b, out):
        d = (b - a).normalized()
        w = d.cross(out).normalized() * (width / 2)
        vs = [bm.verts.new(p) for p in (a - w + out * push, b - w + out * push, b + w + out * push, a + w + out * push)]
        bm.faces.new(vs)

    n = len(sections)
    band(POLL, 0.35)
    band(n - 3, 0.6)
    for side in (1, -1):
        a = surface(sections, POLL, 0.35, 0, side)
        b = surface(sections, n - 3, 0.6, 0, side)
        strap(a, b, Vector((side, 0, 0)))
        bit = surface(sections, n - 2, 0.5, -25, side)
        bmesh.ops.create_icosphere(bm, subdivisions=1, radius=0.022,
                                   matrix=Matrix.Translation(bit + Vector((side * 0.008, 0, 0))))
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bd.link(bpy.data.objects.new(name, me))
    bd.skin_weights(obj, arm_obj, DEFORM, falloff=3.0)
    part = me.attributes.new("part", "INT", "POINT")
    for d in part.data:
        d.value = HORSE_PART
    return am.bind(obj, arm_obj)


# Atlas rectangles of the horse's parts (coat.REGIONS names the painter of each).
PARTS = {"hull": "hide", "mane": "hair", "saddle": "tack", "bridle": "bridle", "cloth": "cloth"}


def dress(ch, sections, cloth_rgb, key_layer, coat_rgb=None):
    """Realistic materials for the horse of `ch` (`ch.horse` objects): one baked atlas
    (coat.atlas) for hide, mane and tail, saddle, bridle and saddlecloth; replaces their
    colours from `Character.colours`. `key_layer` maps a material key to its decay layer."""
    H = ch.horse
    parts = {}
    for part, region in PARTS.items():
        if part in H:
            coat.atlas_uv(H[part], coat.REGIONS[region])
            parts.setdefault(region, []).append(H[part])
    L = landmarks(sections)
    zs = [v.co.z for v in H["cloth"].data.vertices]
    ys = [v.co.y for v in H["cloth"].data.vertices]
    L["cloth_edges"] = (min(zs), min(ys), max(ys))
    img = coat.atlas(f"{ch.name}.horse_atlas", parts, L, cloth_rgb)
    rep = {"hide": coat.BAY["body"], "hair": coat.BAY["mane"], "tack": (0.27, 0.15, 0.07),
           "bridle": (0.27, 0.15, 0.07), "cloth": cloth_rgb}
    for region, objs in parts.items():
        for o in objs:
            key = ch.keys[o.name]
            m = decay.material(f"{ch.name}.{key}.coat", key_layer[key], rep[region], image=img, uv_map="atlas",
                               interpolation="Linear")
            m["acr_colour_from"] = "coat"
            o.data.materials.clear()
            o.data.materials.append(m)
    return img


def mane_tail(arm_obj, name, sections=None):
    """Mane along the neck and a tail: thin fins/tubes."""
    sections = sections or SECTIONS
    bm = bmesh.new()
    # Mane: a crest along the top of the neck and a curtain falling to the off (right) side.
    rows = []
    for y, z, w, h in sections[6:11]:
        top = Vector((0, y, z + h * 0.92))
        rows.append((top + Vector((0.01, 0.02, 0.0)), top + Vector((0, 0.03, 0.07)),
                     Vector((-w * 0.75, y + 0.03, z + h * 0.62)), Vector((-w * 1.02, y + 0.05, z + h * 0.05))))
    for a, b in zip(rows, rows[1:]):
        for k in range(3):
            bm.faces.new([bm.verts.new(p) for p in (a[k], b[k], b[k + 1], a[k + 1])] if k else
                         [bm.verts.new(p) for p in (a[1], b[1], b[0], a[0])])
    # Forelock between the ears.
    c, _hw, hh, up = _frame(sections, POLL, 0.25)
    f0 = c + up * hh * 0.95
    bm.faces.new([bm.verts.new(p) for p in (f0 + Vector((-0.04, 0, 0)), f0 + Vector((0.04, 0, 0)),
                                             f0 + Vector((0.03, -0.10, -0.08)), f0 + Vector((-0.03, -0.10, -0.08)))])
    tail = [Vector(p) for p in ((0, 0.86, 1.30), (0, 0.94, 1.10), (0, 0.98, 0.85), (0, 1.0, 0.62))]
    rs = [0.05, 0.08, 0.10, 0.08]
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


def bones(arm_obj, name, sections=None):
    """Horse skeleton for decay: skull, vertebrae, ribs, pelvis, leg bones; `sections` = the
    fitted hull's sections (the trunk follows them)."""
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
    if sections is not None:
        # The trunk, neck and head bones follow a fitted hull: same height within the
        # section as in the unfitted one, so no vertebra pokes out of a lowered neck (#212).
        for v in B.bm.verts:
            if B.names[v[B.lay]].startswith(("front_", "back_")):
                continue
            yo, zo, _wo, ho = _section_at(SECTIONS, v.co.y)
            _yf, zf, _wf, hf = _section_at(sections, v.co.y)
            v.co.z = zf + (v.co.z - zo) * hf / ho
    return B.finish(name, lambda b: HORSE_PART)


def _section_at(sections, y):
    """Section (y, z, half width, half height) interpolated at y (clamped at the ends)."""
    ys = [s[0] for s in sections]
    if y >= ys[0]:
        return sections[0]
    for a, b in zip(sections, sections[1:]):
        if b[0] <= y <= a[0]:
            t = (a[0] - y) / (a[0] - b[0])
            return tuple(a[k] + (b[k] - a[k]) * t for k in range(4))
    return sections[-1]


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
    """Dead on its side (#DEATHLIE1 of the dragoon): rolled a quarter turn (not past it, so
    the legs lie on the ground instead of sticking up), legs relaxed and bent at knee and
    hock, the underneath pair straighter, neck and head stretched out along the ground
    (a head bent far back left the rigid skull of the bone layer out of the folded hull:
    the white rectangle by the head, #275)."""
    return {"root": (0, -90, 230), "root_loc": (-1.32, 1.24, 0.34),
            "neck_01": (0, 0, 14), "neck_02": (0, 0, -4), "head": (14, 0, 0),
            "front_humerus_l": (-14, 0, 0), "front_radius_l": (-38, 0, 0), "front_cannon_l": (12, 0, 0),
            "front_humerus_r": (-24, 0, 0), "front_radius_r": (-58, 0, 0), "front_cannon_r": (16, 0, 0),
            "back_femur_l": (8, 0, 0), "back_tibia_l": (22, 0, 0), "back_hock_l": (-18, 0, 0),
            "back_femur_r": (22, 0, 0), "back_tibia_r": (34, 0, 0), "back_hock_r": (-30, 0, 0),
            "tail_01": (0, 0, 20), "tail_02": (0, 0, 10)}


def falling():
    return {"root": (12, -40, 0), "root_loc": (-0.4, 0.0, -0.25),
            "front_humerus_l": (-50, 0, 0), "front_radius_l": (-60, 0, 0), "front_humerus_r": (-40, 0, 0),
            "front_radius_r": (-70, 0, 0), "back_femur_l": (-20, 0, 0), "back_femur_r": (-25, 0, 0),
            "neck_01": (-20, 0, 0), "head": (-20, 0, 0)}
