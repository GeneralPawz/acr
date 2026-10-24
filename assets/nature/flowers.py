"""Flowers: a low patch of leaf rosettes with a few orange, yellow and white flower heads
(sprites 327-330, about 1-1.5 m wide and 0.4 m high).

Scripted (no source): seeded rosettes of tilted leaf quads and thin stems carrying star-shaped
heads, flat colours from the sprites. Wind (`_WIND`, as `acr_assets.nature`): x bend by height,
y radial, z phase per plant, w flutter on leaves and heads.
"""

import math
import random

import bmesh
import bpy
from mathutils import Vector

from acr_assets import nature

TYPE = "TREELIST:328"
RADIUS = 0.6
TOP = 0.42
COLOURS = {"leaf": (0.30, 0.36, 0.14), "dry": (0.45, 0.38, 0.20), "stem": (0.26, 0.34, 0.12),
           "orange": (0.95, 0.52, 0.08), "yellow": (0.95, 0.82, 0.18), "white": (0.90, 0.90, 0.86)}
SLOTS = list(COLOURS)


def _mat(name, rgb):
    mat = bpy.data.materials.new(name)
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*[c ** 2.2 for c in rgb], 1.0)
    bsdf.inputs["Roughness"].default_value = 0.9
    return mat


def build(ctx):
    rnd = random.Random(327)
    bm = bmesh.new()
    wind = {}

    def face(pts, slot, phase, flutter):
        vs_ = [bm.verts.new(p) for p in pts]
        f = bm.faces.new(vs_)
        f.material_index = SLOTS.index(slot)
        for v in vs_:
            h = max(v.co.z, 0.0) / TOP
            r = Vector((v.co.x, v.co.y)).length / RADIUS
            wind[v] = (0.3 * h * h, min(1.0, r) * 0.3, phase, flutter)

    for _ in range(14):
        a, d = rnd.uniform(0, 2 * math.pi), RADIUS * math.sqrt(rnd.random())
        c = Vector((math.cos(a) * d, math.sin(a) * d, 0.0))
        ph = rnd.random()
        slot = "dry" if rnd.random() < 0.3 else "leaf"
        n, ln, w = rnd.randint(5, 7), rnd.uniform(0.12, 0.22), rnd.uniform(0.04, 0.06)
        for k in range(n):
            t = 2 * math.pi * k / n + rnd.uniform(-0.2, 0.2)
            u = Vector((math.cos(t), math.sin(t), 0.0))
            s = Vector((-u.y, u.x, 0.0))
            rise = rnd.uniform(0.25, 0.6)
            tip = c + u * ln + Vector((0, 0, ln * rise + 0.02))
            mid = c + u * ln * 0.5 + Vector((0, 0, ln * rise * 0.6 + 0.02))
            face([c + Vector((0, 0, 0.01)), mid - s * w, tip, mid + s * w], slot, ph, 1.0)
    for i in range(9):
        a, d = rnd.uniform(0, 2 * math.pi), RADIUS * 0.85 * math.sqrt(rnd.random())
        c = Vector((math.cos(a) * d, math.sin(a) * d, 0.0))
        ph = rnd.random()
        h = rnd.uniform(0.22, TOP - 0.04)
        lean = Vector((rnd.uniform(-0.05, 0.05), rnd.uniform(-0.05, 0.05), 0.0))
        head = c + lean + Vector((0, 0, h))
        face([c + Vector((-0.008, 0, 0)), c + Vector((0.008, 0, 0)), head], "stem", ph, 0.5)
        slot = ("orange", "yellow", "white")[i % 3]
        r = rnd.uniform(0.035, 0.05)
        ring = []
        for k in range(10):
            q = r * (1.0 if k % 2 == 0 else 0.45)
            t = 2 * math.pi * k / 10
            ring.append(head + Vector((math.cos(t) * q, math.sin(t) * q, 0.012 * (k % 2))))
        face(ring, slot, ph, 1.0)
    rows = [wind[v] for v in bm.verts]
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    me = bpy.data.meshes.new(f"{ctx.name}.mesh")
    bm.to_mesh(me)
    bm.free()
    for name in SLOTS:
        me.materials.append(_mat(f"{ctx.name}.{name}", COLOURS[name]))
    a = me.attributes.new("_wind", "FLOAT_COLOR", "POINT")
    for i, row in enumerate(rows):
        a.data[i].color = row
    obj = bpy.data.objects.new(ctx.name, me)
    bpy.context.scene.collection.objects.link(obj)
    obj.parent = ctx.root
    nature.white(obj)
    me.calc_loop_triangles()
    ctx.root["acr_nature"] = "bush"
    ctx.root["acr_wind"] = [1.0, 1.0, 1.2, 1.8]
    zs = [v.co.z for v in me.vertices]
    ctx.stats = {"kind": "bush", "triangles": len(me.loop_triangles),
                 "size_m": [round(2 * RADIUS, 2), round(2 * RADIUS, 2), round(max(zs), 2)]}
