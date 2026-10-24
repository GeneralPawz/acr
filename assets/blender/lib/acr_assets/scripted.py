"""Scripted props and buildings in metres (needs `bpy`, `bmesh`, `numpy`): procedural
materials like the generated assets carry (base colour, tangent normal map, roughness; see
`genfit` and `nature`), and mesh helpers that write straight into a `bmesh` with a UV map
(`uv`): poles (tubes along a polyline), flat panels and quad grids.

Textures are 256-pixel tiles, seeded, made from the `surfaces` noise; the normal map is the
gradient of a height field drawn with the colour (bark fissures and fibres, cloth weave,
mortar joints), so the relief reads under the renderer's light from every side.
"""

import math
import random

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from . import surfaces

SIZE = 256
UV = "uv"


# ----------------------------------------------------------------------------- materials

def _image(name, rgb, colour):
    img = bpy.data.images.new(name, SIZE, SIZE, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "sRGB" if colour else "Non-Color"
    px = np.ones((SIZE, SIZE, 4), np.float32)
    px[..., :3] = np.clip(rgb, 0, 1)
    img.pixels.foreach_set(px.ravel())
    img.file_format = "PNG"
    img.pack()
    return img


def normal_map(height, strength):
    """Tangent-space normals (0..1 encoded) of a tileable height field (rows = v)."""
    gx = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) * 0.5 * strength
    gy = (np.roll(height, -1, 0) - np.roll(height, 1, 0)) * 0.5 * strength
    n = np.stack([-gx, -gy, np.ones_like(height)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n * 0.5 + 0.5


def material(name, srgb, height, roughness=0.9, strength=6.0, double=False):
    """Principled BSDF with a base-colour PNG (`srgb`, sRGB 0..1), a normal map from
    `height` (0..1) and a constant roughness; `double` turns off back-face culling (thin
    sheets: bark panels, cloth, hide)."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    mat.use_backface_culling = not double
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = 0.0
    uv = nt.nodes.new("ShaderNodeUVMap")
    uv.uv_map = UV
    col = nt.nodes.new("ShaderNodeTexImage")
    col.image = _image(f"{name}_col", srgb, True)
    nrm = nt.nodes.new("ShaderNodeTexImage")
    nrm.image = _image(f"{name}_nrm", normal_map(height, strength), False)
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nm.uv_map = UV
    for t in (col, nrm):
        nt.links.new(uv.outputs["UV"], t.inputs["Vector"])
    nt.links.new(col.outputs["Color"], bsdf.inputs["Base Color"])
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def _mix(a, b, t):
    return np.asarray(a, np.float32) * (1 - t[..., None]) + np.asarray(b, np.float32) * t[..., None]


def _srgb(lin):
    return np.power(np.clip(lin, 0, 1), 1 / 2.2)


def _lum(rgb):
    return rgb @ np.array([0.3, 0.59, 0.11], np.float32)


def bark(seed=11):
    """Elm-bark sheets: fibres and fissures along v, grey weathering, dark cracks."""
    rng = np.random.default_rng(seed)
    fib = surfaces._fbm(rng, SIZE, 4, stretch=(6, 0.5))
    fis = surfaces._fbm(rng, SIZE, 5, stretch=(5, 0.35))
    patch = surfaces._fbm(rng, SIZE, 2)
    crack = np.clip((0.36 - fis) * 7, 0, 1)
    rgb = _mix((0.27, 0.21, 0.16), (0.47, 0.42, 0.36), fib)
    rgb = _mix(rgb, (0.55, 0.53, 0.50), np.clip((patch - 0.55) * 3, 0, 0.6))
    rgb = _mix(rgb, (0.09, 0.07, 0.05), crack)
    return rgb, np.clip(0.5 + 0.4 * (fib - 0.5) - 0.5 * crack, 0, 1)


def wood(seed=12):
    """Weathered peeled saplings and posts: grain along u."""
    rng = np.random.default_rng(seed)
    g = surfaces._fbm(rng, SIZE, 4, stretch=(0.5, 6))
    knot = np.clip((surfaces._fbm(rng, SIZE, 3) - 0.66) * 6, 0, 1)
    rgb = _mix((0.33, 0.28, 0.22), (0.52, 0.47, 0.39), g)
    rgb = _mix(rgb, (0.18, 0.14, 0.10), knot)
    return rgb, np.clip(g - 0.6 * knot, 0, 1)


def hide(seed=13):
    """A tanned hide: mottled tan with darker smoke stains and fine wrinkles."""
    rng = np.random.default_rng(seed)
    m = surfaces._fbm(rng, SIZE, 3)
    w = surfaces._fbm(rng, SIZE, 10, octaves=2)
    rgb = _mix((0.40, 0.29, 0.19), (0.62, 0.50, 0.36), m)
    rgb = _mix(rgb, (0.25, 0.18, 0.12), np.clip((0.4 - m) * 3, 0, 0.7))
    return rgb, w


def canvas(seed=14):
    """Sailcloth: off-white weave, weathered stains."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)
    weave = 0.5 + 0.25 * np.sin(x * math.pi / 2) * np.sin(y * math.pi / 2)
    stain = surfaces._fbm(rng, SIZE, 3)
    rgb = _mix((0.66, 0.62, 0.53), (0.84, 0.81, 0.72), stain)
    rgb = rgb * (0.94 + 0.08 * weave[..., None])
    return rgb, weave


def from_surface(kind):
    """A `surfaces` tile (linear colours) as (sRGB colour, height from its luminance)."""
    gen = surfaces.KINDS[kind][0]
    rgb = _srgb(gen())
    h = _lum(rgb)
    return rgb, (h - h.min()) / max(1e-6, float(h.max() - h.min()))


# ----------------------------------------------------------------------------- geometry

def new_bm():
    bm = bmesh.new()
    bm.loops.layers.uv.new(UV)
    return bm


def face(bm, pts, uvs, outward=None):
    """One face over new vertices `pts` with `uvs`; flipped when its normal points away
    from `outward` (a direction, or a callable of the face centre giving one)."""
    vs = [bm.verts.new(p) for p in pts]
    f = bm.faces.new(vs)
    lay = bm.loops.layers.uv[UV]
    for lp, uv in zip(f.loops, uvs):
        lp[lay].uv = uv
    if outward is not None:
        f.normal_update()
        d = outward(f.calc_center_median()) if callable(outward) else Vector(outward)
        if f.normal.dot(d) < 0:
            f.normal_flip()
    return f


def grid(bm, at, nu, nv, outward=None, uv_scale=(1.0, 1.0), uv_offset=(0.0, 0.0)):
    """A quad grid: `at(u, v)` (u, v in 0..1) gives the point; `nu` x `nv` quads."""
    lay = bm.loops.layers.uv[UV]
    vs = [[bm.verts.new(at(i / nu, j / nv)) for i in range(nu + 1)] for j in range(nv + 1)]
    faces = []
    for j in range(nv):
        for i in range(nu):
            f = bm.faces.new((vs[j][i], vs[j][i + 1], vs[j + 1][i + 1], vs[j + 1][i]))
            for lp, (a, b) in zip(f.loops, ((i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1))):
                lp[lay].uv = (uv_offset[0] + a / nu * uv_scale[0], uv_offset[1] + b / nv * uv_scale[1])
            faces.append(f)
    if outward is not None:
        for f in faces:
            f.normal_update()
            d = outward(f.calc_center_median()) if callable(outward) else Vector(outward)
            if f.normal.dot(d) < 0:
                f.normal_flip()
    return faces


def tube(bm, pts, radius, sides=5, tile=1.0, seed=0):
    """A pole along the polyline `pts` (metres), open ended, u along its length."""
    pts = [Vector(p) for p in pts]
    lay = bm.loops.layers.uv[UV]
    rings, lengths, s = [], [], 0.0
    ref = None
    for k, p in enumerate(pts):
        a = pts[max(0, k - 1)]
        b = pts[min(len(pts) - 1, k + 1)]
        d = (b - a).normalized()
        if ref is None:
            ref = Vector((0, 0, 1)) if abs(d.z) < 0.9 else Vector((1, 0, 0))
        e1 = d.cross(ref).normalized()
        e2 = d.cross(e1)
        ref = e2  # carry the frame along so the ring does not twist
        if k:
            s += (p - pts[k - 1]).length
        lengths.append(s)
        rings.append([bm.verts.new(p + (e1 * math.cos(2 * math.pi * i / sides) + e2 * math.sin(2 * math.pi * i / sides)) * radius)
                      for i in range(sides)])
    off = random.Random(seed).random()
    for k in range(len(pts) - 1):
        for i in range(sides):
            j = (i + 1) % sides
            quad = (rings[k][i], rings[k + 1][i], rings[k + 1][j], rings[k][j])
            f = bm.faces.new(quad)
            uvs = ((lengths[k] / tile + off, i / sides), (lengths[k + 1] / tile + off, i / sides),
                   (lengths[k + 1] / tile + off, (i + 1) / sides), (lengths[k] / tile + off, (i + 1) / sides))
            for lp, uv in zip(f.loops, uvs):
                lp[lay].uv = uv
            f.normal_update()
            c = f.calc_center_median()
            axis = (pts[k] + pts[k + 1]) / 2
            if f.normal.dot(c - axis) < 0:
                f.normal_flip()


def to_object(bm, name, mat, parent):
    me = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    me.validate()
    me.materials.append(mat)
    obj = bpy.data.objects.new(name, me)
    obj.parent = parent
    bpy.context.scene.collection.objects.link(obj)
    for p in me.polygons:
        p.use_smooth = False
    return obj


def triangles(root):
    n = 0
    for o in root.children_recursive:
        if o.type == "MESH":
            o.data.calc_loop_triangles()
            n += len(o.data.loop_triangles)
    return n


def size(root):
    """[x, y, z] extent in metres of the meshes under `root`."""
    bpy.context.view_layer.update()
    pts = [o.matrix_world @ Vector(c) for o in root.children_recursive if o.type == "MESH" for c in o.bound_box]
    lo = [min(p[i] for p in pts) for i in range(3)]
    hi = [max(p[i] for p in pts) for i in range(3)]
    return [round(hi[i] - lo[i], 2) for i in range(3)]
