"""Conifer: a fir of the northern forests, `TREES.GP` frame 87 (look-alikes 88-92, 100-102,
107-108), a 5.6 m tree: a bark trunk with whorls of drooping branches, nine tiers narrowing to
a spike, with the trunk showing in the gaps between them.

Procedural, like `broadleaf.py` (it replaced the Sketchfab pack's tree A, which was twice the
sprite's height and one green mass). The tier layout (`acr_assets.firtiers`) is measured from
the sprite's half widths; every branch is a serrated, two-sided frond that droops from the
trunk, so the tiers stay readable from all sides. States `STAND`, `FELLED` and `STUMP` and the
`_WIND` attribute (x trunk bend, y branch, z phase, w needle flutter) are broadleaf's. Colours:
bark and needle colour from the sprite's pixels (the sprite is dark: needles about sRGB 0.11,
0.19, 0.09 with highlights to 0.27, 0.42, 0.22), the underside of every frond darker.
"""

import importlib.util
import math
import os
import random

import bmesh
import bpy
from mathutils import Matrix, Vector

from acr_assets import detail, firtiers, kit, project
from acr_assets import conventions as cv

TYPE = "TREELIST:87"
CHECKS = ["--anim STAND"]
LIKE = [87, 88, 89, 90, 91, 92, 100, 101, 102, 107, 108]

FIT = {"trunk_r": 3.2, "reach": 1.0, "frond_w": 0.40}
LEAF_LEVEL = 0.20
PALETTE = {"bark": (0.22, 0.16, 0.13), "leaves": (0.10, 0.19, 0.08), "cut": (0.72, 0.60, 0.42)}
STATIONS = 6  # per frond; each has 3 top and 3 underside vertices, interleaved


def _broadleaf():
    """The broadleaf script's helpers (`_wind`, `_ao`, `stump`, `_material`, `pose_for`, `m`)."""
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "broadleaf.py")
    spec = importlib.util.spec_from_file_location("asset_broadleaf_helpers", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


B = _broadleaf()
m = B.m


def _frond(bm, root, a, reach, droop, width):
    """One drooping, serrated frond from `root` along azimuth `a`: `STATIONS` stations of
    three vertices (edge, raised midrib, edge) and an underside copy 3 cm lower for thickness."""
    d = Vector((math.cos(a), math.sin(a), 0.0))
    side = Vector((-d.y, d.x, 0.0))
    top, bot = [], []
    for s in range(STATIONS):
        t = s / (STATIONS - 1)
        # Widest near the root, a tip at the end; alternate stations are narrower: the
        # serrated outline of a needled twig.
        w = width * math.sin(math.pi * (0.12 + 0.88 * t) * 0.9) * (0.62 if s % 2 else 1.0)
        c = root + d * (reach * t) + Vector((0, 0, -reach * droop * t * t))
        row = (c + side * w - Vector((0, 0, w * 0.25)), c + Vector((0, 0, w * 0.35 * (1 - t))),
               c - side * w - Vector((0, 0, w * 0.25)))
        tv, bv = [], []
        for p in row:
            tv.append(bm.verts.new(p))
            bv.append(bm.verts.new(p - Vector((0, 0, 0.03))))
        top.append(tv)
        bot.append(bv)
    for i in range(STATIONS - 1):
        for k in range(2):
            bm.faces.new((top[i][k], top[i][k + 1], top[i + 1][k + 1], top[i + 1][k]))
            bm.faces.new((bot[i][k], bot[i + 1][k], bot[i + 1][k + 1], bot[i][k + 1]))


def tree(parent, name, top):
    """Trunk and the tiered crown; returns (trunk, crown)."""
    rnd = random.Random(11)
    mpp = cv.METRES_PER_PIXEL
    r0 = FIT["trunk_r"] * mpp
    # Trunk: slim, slightly bent, from the ground to the spike.
    h = m(0, firtiers.TOP).z
    bm = bmesh.new()
    rings = []
    steps = 9
    for i in range(steps + 1):
        t = i / steps
        r = r0 * (1.0 - 0.8 * t) * (1.25 if i == 0 else 1.0)
        c = Vector((math.sin(t * 2.2) * 0.04, 0.0, h * t))
        rings.append([bm.verts.new(c + Vector((math.cos(a) * r, math.sin(a) * r, 0)))
                      for a in (2 * math.pi * k / 7 for k in range(7))])
    for a, b in zip(rings, rings[1:]):
        for k in range(7):
            bm.faces.new((a[k], a[(k + 1) % 7], b[(k + 1) % 7], b[k]))
    me = bpy.data.meshes.new(f"{name}.trunk")
    bm.to_mesh(me)
    bm.free()
    trunk = bpy.data.objects.new(f"{name}.trunk", me)
    trunk.parent = parent
    bpy.context.scene.collection.objects.link(trunk)
    B._wind(trunk, [(min(v.co.z / top, 1.0) ** 2, 0.0, 0.0, 0.0) for v in me.vertices])
    B._ao(trunk, [0.8] * len(me.vertices))

    # Crown: one frond per branch of the layout.
    bm = bmesh.new()
    spans = []
    for tier, a, reach_px, sy, droop in firtiers.branches():
        root = m(0, sy)
        reach = reach_px * mpp * FIT["reach"]
        n0 = len(bm.verts)
        _frond(bm, root, a, reach, droop * 0.75, reach * FIT["frond_w"])
        spans.append((n0, len(bm.verts), tier, root, rnd.random(), reach))
    me = bpy.data.meshes.new(f"{name}.crown")
    bm.to_mesh(me)
    bm.free()
    crown = bpy.data.objects.new(f"{name}.crown", me)
    crown.parent = parent
    bpy.context.scene.collection.objects.link(crown)
    wind, ao = [None] * len(me.vertices), [None] * len(me.vertices)
    last = len(firtiers.TIERS) - 1
    for n0, n1, tier, root, ph, reach in spans:
        for i in range(n0, n1):
            v = me.vertices[i].co
            rel = min(Vector((v.x - root.x, v.y - root.y, 0)).length / reach, 1.0)
            wind[i] = (min(v.z / top, 1.0) ** 2, rel, ph, 0.5 * rel)
            under = (i - n0) % 6 % 2 == 1  # the underside copy of each vertex
            ao[i] = (0.55 if under else 0.9) * (0.8 + 0.2 * tier / last) * (0.7 + 0.3 * rel)
    B._wind(crown, wind)
    B._ao(crown, ao)
    return trunk, crown


def make(root, name):
    groups = {}
    for state in ("STAND", "FELLED", "STUMP"):
        groups[state] = kit.group(root, state.lower(), acr_state=state)
    top = m(0, firtiers.TOP).z + 0.3
    trunk, crown = tree(groups["STAND"], f"{name}.stand", top)
    st = B.stump(groups["STUMP"], f"{name}.stump")
    # Felled: as broadleaf, the tree lying east of its stump.
    t2, c2 = tree(groups["FELLED"], f"{name}.felled", top)
    st2 = B.stump(groups["FELLED"], f"{name}.felled_stump")
    pivot = (Matrix.Translation((0.15, 0, 0.12)) @ Matrix.Rotation(math.radians(-86), 4, "Y")
             @ Matrix.Translation((0, 0, -0.4)))
    for o in (t2, c2):
        o.data.transform(pivot @ Matrix.Rotation(math.radians(15), 4, "Z"))
        for v in o.data.vertices:
            if v.co.z < 0.03:
                v.co.z = 0.03 + 0.1 * (v.co.z - 0.03) / (1 + abs(v.co.z))
        for d in o.data.attributes["_wind"].data:
            d.color = (0.0, 0.0, 0.0, 0.0)
    return {"groups": groups, "trunks": [trunk, t2], "crowns": [crown, c2], "stumps": [st, st2]}


def build(ctx):
    model = make(ctx.root, ctx.name)
    B.pose_for(model, "STAND")
    colours = dict(PALETTE)
    if ctx.ref is not None:
        objs = model["trunks"][:1] + model["crowns"][:1]
        _img, info = project.project_uvs(objs, ctx.ref, name=f"{ctx.name}.sprite")
        colours["bark"] = project.guided(info[objs[0].name][2], PALETTE["bark"], radius=0.25)
        colours["leaves"] = project.guided(info[objs[1].name][2], PALETTE["leaves"], radius=0.25)
        for o in objs:
            o.data.uv_layers.remove(o.data.uv_layers["sprite"])
    lv = sum(colours["leaves"]) / 3
    if lv < LEAF_LEVEL:
        colours["leaves"] = tuple(min(1.0, c * LEAF_LEVEL / lv) for c in colours["leaves"])
    B.pose_for(model, None)
    for o in ctx.root.children_recursive:
        o.hide_render = False
    bark = B._material(f"{ctx.name}.bark", colours["bark"], "wood")
    leaves = B._material(f"{ctx.name}.leaves", colours["leaves"], "leather")
    cut = B._material(f"{ctx.name}.cut", colours["cut"], "wood")
    for o in model["trunks"]:
        o.data.materials.append(bark)
    for o in model["crowns"]:
        o.data.materials.append(leaves)
    for o in model["stumps"]:
        o.data.materials.append(bark)
        o.data.materials.append(cut)
    for o in model["trunks"] + model["crowns"] + model["stumps"]:
        detail.box_uv(o, scale=6.0)
    tris = 0
    for o in ctx.root.children_recursive:
        if o.type == "MESH":
            o.data.calc_loop_triangles()
            tris += len(o.data.loop_triangles)
    ctx.root["acr_like_frames"] = LIKE
    ctx.root["acr_wind"] = "_WIND: x trunk bend, y branch, z phase, w needle flutter"
    ctx.stats = {"triangles": tris, "colours": colours}
