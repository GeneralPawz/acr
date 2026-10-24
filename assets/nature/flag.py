"""Flag: a wooden pole with a red and yellow chequered banner (sprites 331, 471), 2.25 m high.

Scripted (no source): an 8-sided pole and a cloth grid of 10 x 6 quads hanging off it towards
+X (east, as in the sprite: map entry "turn": "sprite"), with a still ripple; the checks are a
small nearest-sampled texture in the sprite's colours. Wind (`_WIND`): the pole 0; the cloth
flutters along its normal with w = distance from the pole (0 at the hoist) and the phase z
growing along the fly (a wave running out to the free edge), and swings with y.
"""

import math

import bmesh
import bpy
import numpy as np

from acr_assets import nature

TYPE = "TREELIST:331"
POLE_H, POLE_R = 2.25, 0.03
FLY, HOIST = 0.95, 0.62  # cloth length and height (m)
NX, NZ = 10, 6
RED, YELLOW, ORANGE = (0.72, 0.10, 0.05), (0.96, 0.80, 0.12), (0.93, 0.48, 0.08)
WOOD = (0.42, 0.30, 0.17)


def _checks(name):
    w, h = 8, 6
    px = np.ones((h, w, 4), np.float32)
    for j in range(h):
        for i in range(w):
            c = (RED, YELLOW, ORANGE)[(i + 2 * j) % 3]
            px[j, i, :3] = [x ** 2.2 for x in c]
    img = bpy.data.images.new(name, w, h, alpha=False)
    img.pixels.foreach_set(px.ravel())
    img.file_format = "PNG"
    img.pack()
    return img


def _mat(name, rgb=None, img=None):
    mat = bpy.data.materials.new(name)
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.9
    if img is None:
        bsdf.inputs["Base Color"].default_value = (*[c ** 2.2 for c in rgb], 1.0)
    else:
        tex = nt.nodes.new("ShaderNodeTexImage")
        tex.image = img
        tex.interpolation = "Closest"
        nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    return mat


def build(ctx):
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    wind = {}
    ring = []
    for z in (0.0, POLE_H):
        ring.append([bm.verts.new((math.cos(a) * POLE_R, math.sin(a) * POLE_R, z))
                     for a in (2 * math.pi * k / 8 for k in range(8))])
    for k in range(8):
        f = bm.faces.new([ring[0][k], ring[0][(k + 1) % 8], ring[1][(k + 1) % 8], ring[1][k]])
        f.material_index = 0
    f = bm.faces.new(ring[1][::-1])
    f.material_index = 0
    for v in bm.verts:
        wind[v] = (0.0, 0.0, 0.0, 0.0)
    top = POLE_H - 0.05
    grid = []
    for j in range(NZ + 1):
        row = []
        for i in range(NX + 1):
            u, t = i / NX, j / NZ
            v = bm.verts.new((POLE_R + u * FLY, 0.06 * u * math.sin(u * 2.4 * math.pi + t * 0.8),
                              top - t * HOIST - 0.08 * u * u))
            wind[v] = (0.0, 0.4 * u, 0.5 * u, u)
            row.append(v)
        grid.append(row)
    for j in range(NZ):
        for i in range(NX):
            f = bm.faces.new([grid[j][i], grid[j + 1][i], grid[j + 1][i + 1], grid[j][i + 1]])
            f.material_index = 1
            for loop, (a, b) in zip(f.loops, ((i, j), (i, j + 1), (i + 1, j + 1), (i + 1, j))):
                loop[uv].uv = (a / NX, 1.0 - b / NZ)
    rows = [wind[v] for v in bm.verts]
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    me = bpy.data.meshes.new(f"{ctx.name}.mesh")
    bm.to_mesh(me)
    bm.free()
    me.materials.append(_mat(f"{ctx.name}.pole", WOOD))
    me.materials.append(_mat(f"{ctx.name}.cloth", img=_checks(f"{ctx.name}.checks")))
    for p in me.polygons:
        p.use_smooth = True
    a = me.attributes.new("_wind", "FLOAT_COLOR", "POINT")
    for i, row in enumerate(rows):
        a.data[i].color = row
    obj = bpy.data.objects.new(ctx.name, me)
    bpy.context.scene.collection.objects.link(obj)
    obj.parent = ctx.root
    nature.white(obj)
    me.calc_loop_triangles()
    ctx.root["acr_nature"] = "bush"
    ctx.root["acr_wind"] = [0.0, 1.0, 2.5, 1.6]
    ctx.stats = {"kind": "flag", "triangles": len(me.loop_triangles), "size_m": [round(FLY + POLE_R, 2), 0.12, POLE_H]}
