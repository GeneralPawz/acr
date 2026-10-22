"""Broadleaf tree: the most used deciduous tree of the maps, `TREES.GP` frame 104
(`TREELIST.LST` D104: 4394 objects on 40 of the 63 maps; its look-alikes 103, 105, 106 and
110 add another 13 000), a 5 m tree with an open crown of leaf clumps on a leaning trunk.

States (node groups, extras `acr_state`): `STAND` (the tree), `FELLED` (the trunk and crown
lying on the ground next to the stump) and `STUMP`. Wind: every vertex carries `_WIND`
(vec4): x trunk bend weight (0 at the root, 1 at the crown top, quadratic), y branch weight
(distance from the trunk axis / crown radius), z phase (per clump, 0..1), w leaf flutter
(1 on leaves). A vertex shader moves `p += dir * (x * A_trunk * sin(t) + y * A_branch *
sin(2.3 t + 6.28 z)) + n * w * A_leaf * sin(7 t + 6.28 z)`. `COLOR_0` darkens the inside of
the crown (ambient occlusion baked per vertex).

The crown clumps are the k-means clusters of the sprite's crown pixels (14 clusters, screen
position and area), placed in the plane through the trunk with a seeded depth spread; the
trunk follows the sprite's bark pixels. Colours: one dominant bark and leaf colour from the
sprite and procedural detail tiles, so the tree holds up from all sides.
"""

import math
import random

import bmesh
import bpy
from mathutils import Matrix, Vector, noise

from acr_assets import detail, kit, project
from acr_assets import conventions as cv

TYPE = "TREELIST:104"
CHECKS = ["--anim STAND"]
LIKE = [103, 104, 105, 106, 110]

# Sprite measurements (screen pixels from the ground point; tools: k-means of the crown).
# (screen x, screen y, radius, pixel count)
CLUMPS = [[1.2, -95.0, 8.2, 212], [-8.7, -127.7, 8.3, 215], [-23.5, -117.2, 8.8, 245], [26.5, -75.0, 9.1, 263],
          [10.9, -84.6, 8.1, 206], [24.4, -93.6, 8.7, 238], [-7.4, -115.5, 7.8, 190], [41.4, -66.9, 8.0, 203],
          [21.8, -109.8, 9.0, 256], [-23.2, -87.5, 9.3, 274], [-16.0, -66.2, 7.6, 182], [28.5, -58.1, 6.9, 149],
          [8.6, -118.5, 8.0, 202], [41.6, -53.6, 7.3, 166]]
# Trunk centre line (screen y, screen x) from the bark pixels below the crown.
TRUNK = [(0, 1.0), (-20, 0.5), (-40, 4.0), (-55, 2.0), (-75, 0.0), (-95, -6.0)]
# Bare branches the sprite shows below and beside the crown (screen x, y of the tip).
BRANCHES = [(-38.0, -50.0), (-30.0, -78.0), (34.0, -46.0), (18.0, -40.0)]
FIT = {"clump_scale": 1.675, "depth": 25.0, "trunk_r": 3.6, "flatten": 1.1}
PALETTE = {"bark": (0.30, 0.24, 0.18), "leaves": (0.20, 0.30, 0.13), "cut": (0.72, 0.60, 0.42)}


def m(sx, sy, depth=0.0):
    """Model point (metres) of a sprite point (screen px from the ground point) at ground
    depth `depth` (game y px, + towards the viewer)."""
    h = (depth / 2.0 - sy) / cv.COS_E
    return Vector((sx * cv.METRES_PER_PIXEL, -depth * cv.METRES_PER_PIXEL, h * cv.METRES_PER_PIXEL))


def _wind(obj, values):
    a = obj.data.attributes.new("_wind", "FLOAT_COLOR", "POINT")
    for i, d in enumerate(a.data):
        d.color = values[i]


def _ao(obj, values):
    a = obj.data.color_attributes.new("ao", "FLOAT_COLOR", "POINT")
    for i, d in enumerate(a.data):
        g = values[i]
        d.color = (g, g, g, 1.0)
    obj.data.color_attributes.active_color = a


def tree(parent, name, top):
    """Trunk with branches and the crown; returns (trunk, crown)."""
    rnd = random.Random(4)
    pts = [m(x, y) for y, x in TRUNK]
    bm = bmesh.new()
    wind = []
    r0 = FIT["trunk_r"] * cv.METRES_PER_PIXEL
    rings = []
    for i, p in enumerate(pts):
        t = i / (len(pts) - 1)
        r = r0 * (1.0 - 0.55 * t) * (1.3 if i == 0 else 1.0)
        ring = [bm.verts.new(p + Vector((math.cos(a) * r, math.sin(a) * r, 0))) for a in
                (2 * math.pi * k / 7 for k in range(7))]
        rings.append(ring)
    for a, b in zip(rings, rings[1:]):
        for k in range(7):
            bm.faces.new((a[k], a[(k + 1) % 7], b[(k + 1) % 7], b[k]))
    # Branches from the upper trunk towards the larger clumps.
    clumps = []
    for i, (sx, sy, rad, _n) in enumerate(CLUMPS):
        d = rnd.uniform(-FIT["depth"], FIT["depth"])
        clumps.append((m(sx, sy, d), rad * FIT["clump_scale"] * cv.METRES_PER_PIXEL, i))
    fork = pts[-2]
    for k, (bx, by) in enumerate(BRANCHES):
        base = pts[2] if by > -60 else pts[3]
        kit._cyl(bm, base, m(bx, by, (k % 2 - 0.5) * 20), r0 * 0.3, sides=4)
    for c, rad, i in clumps:
        if rad < 8 * cv.METRES_PER_PIXEL:
            continue
        kit._cyl(bm, fork, fork.lerp(c, 0.85), r0 * 0.35, sides=4)
    me = bpy.data.meshes.new(f"{name}.trunk")
    bm.to_mesh(me)
    bm.free()
    trunk = bpy.data.objects.new(f"{name}.trunk", me)
    trunk.parent = parent
    bpy.context.scene.collection.objects.link(trunk)
    centre = sum((c for c, _r, _i in clumps), Vector()) / len(clumps)
    crown_r = max((c - centre).length + r for c, r, _i in clumps)
    for v in me.vertices:
        wind.append((min(v.co.z / top, 1.0) ** 2, 0.0, 0.0, 0.0))
    _wind(trunk, wind)
    _ao(trunk, [0.8] * len(me.vertices))
    # Crown: noisy low-poly spheres.
    bm = bmesh.new()
    phase = bm.verts.layers.float.new("phase")
    for c, rad, i in clumps:
        ph = rnd.random()
        # Two overlapping, noisy sub-clumps per cluster: a lumpier crown than one ball.
        for k in range(2):
            off3 = Vector((rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(-0.6, 0.6))) * rad * 0.45
            cc = c + off3
            r = rad * (0.8 if k == 0 else 0.62)
            res = bmesh.ops.create_icosphere(bm, subdivisions=1, radius=r, matrix=Matrix.Translation(cc))
            for v in res["verts"]:
                off = noise.noise(v.co * 11.0 + Vector((i, k, 0)))
                v.co = cc + (v.co - cc) * (1.0 + 0.45 * off)
                v.co.z = cc.z + (v.co.z - cc.z) * FIT["flatten"]
                v[phase] = ph
    me = bpy.data.meshes.new(f"{name}.crown")
    bm.to_mesh(me)
    bm.free()
    crown = bpy.data.objects.new(f"{name}.crown", me)
    crown.parent = parent
    bpy.context.scene.collection.objects.link(crown)
    ph = [d.value for d in me.attributes["phase"].data]
    me.attributes.remove(me.attributes["phase"])
    wind, ao = [], []
    for v, p in zip(me.vertices, ph):
        rel = v.co - centre
        horiz = Vector((rel.x, rel.y, 0)).length
        wind.append((min(v.co.z / top, 1.0) ** 2, min(horiz / crown_r, 1.0), p, 1.0))
        ao.append(0.7 + 0.3 * min(rel.length / crown_r, 1.0))
    _wind(crown, wind)
    _ao(crown, ao)
    for p in me.polygons:
        p.use_smooth = True
    return trunk, crown


def stump(parent, name, height=12.0):
    bm = bmesh.new()
    p0, p1 = m(TRUNK[0][1], 0), m(TRUNK[0][1], 0) + Vector((0, 0, height * cv.METRES_PER_PIXEL))
    kit._cyl(bm, p0 - Vector((0, 0, 0.05)), p1, FIT["trunk_r"] * 1.3 * cv.METRES_PER_PIXEL, sides=7)
    me = bpy.data.meshes.new(f"{name}")
    bm.to_mesh(me)
    bm.free()
    for poly in me.polygons:
        poly.material_index = 1 if poly.normal.z > 0.9 else 0
    obj = bpy.data.objects.new(name, me)
    obj.parent = parent
    bpy.context.scene.collection.objects.link(obj)
    _wind(obj, [(0, 0, 0, 0)] * len(me.vertices))
    _ao(obj, [0.85] * len(me.vertices))
    return obj


def _material(name, rgb, kind):
    img = detail.tile(name, rgb, kind)
    mat = bpy.data.materials.new(name)
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    uv = nt.nodes.new("ShaderNodeUVMap")
    uv.uv_map = "detail"
    nt.links.new(uv.outputs["UV"], tex.inputs["Vector"])
    nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.9
    mat["acr_colour"] = list(rgb)
    return mat


def make(root, name):
    groups = {}
    for state in ("STAND", "FELLED", "STUMP"):
        groups[state] = kit.group(root, state.lower(), acr_state=state)
    top = max(m(x, y).z for x, y, _r, _n in CLUMPS) + 0.3
    trunk, crown = tree(groups["STAND"], f"{name}.stand", top)
    st = stump(groups["STUMP"], f"{name}.stump")
    # Felled: a copy of the tree lying towards the east, next to its stump.
    t2, c2 = tree(groups["FELLED"], f"{name}.felled", top)
    st2 = stump(groups["FELLED"], f"{name}.felled_stump")
    # Cut 0.4 m above the ground, fallen east; whatever would go into the ground is
    # squashed onto it (branches break).
    pivot = (Matrix.Translation((0.15, 0, 0.12)) @ Matrix.Rotation(math.radians(-86), 4, "Y")
             @ Matrix.Translation((0, 0, -0.4)))
    for o in (t2, c2):
        o.data.transform(pivot @ Matrix.Rotation(math.radians(15), 4, "Z"))
        for v in o.data.vertices:
            if v.co.z < 0.03:
                v.co.z = 0.03 + 0.1 * (v.co.z - 0.03) / (1 + abs(v.co.z))
    return {"groups": groups, "trunks": [trunk, t2], "crowns": [crown, c2], "stumps": [st, st2]}


def pose_for(model, clip):
    for state, g in model["groups"].items():
        for o in [g] + list(g.children_recursive):
            o.hide_render = state != clip


def build(ctx):
    model = make(ctx.root, ctx.name)
    pose_for(model, "STAND")
    colours = dict(PALETTE)
    if ctx.ref is not None:
        objs = model["trunks"][:1] + model["crowns"][:1]
        _img, info = project.project_uvs(objs, ctx.ref, name=f"{ctx.name}.sprite")
        colours["bark"] = project.guided(info[objs[0].name][2], PALETTE["bark"], radius=0.25)
        colours["leaves"] = project.guided(info[objs[1].name][2], PALETTE["leaves"], radius=0.25)
        for o in objs:
            o.data.uv_layers.remove(o.data.uv_layers["sprite"])
    pose_for(model, None)
    for o in ctx.root.children_recursive:
        o.hide_render = False
    bark = _material(f"{ctx.name}.bark", colours["bark"], "wood")
    leaves = _material(f"{ctx.name}.leaves", colours["leaves"], "leather")
    cut = _material(f"{ctx.name}.cut", colours["cut"], "wood")
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
    ctx.root["acr_wind"] = "_WIND: x trunk bend, y branch, z phase, w leaf flutter"
    ctx.stats = {"triangles": tris, "colours": colours}
