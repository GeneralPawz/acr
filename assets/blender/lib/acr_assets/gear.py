"""Gear meshes (needs `bpy`, `bmesh`): hats, muskets, boxes, swords. Each returns an object
with its origin at the grip / crown socket; `armature.attach` puts it on a socket bone (gear
that drops at death) and `rigid` skins it to a deforming bone (strapped gear that follows the
body's `deflate`)."""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from . import armature as am
from .body import link


def from_bm(bm, name):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    return link(bpy.data.objects.new(name, me))


def tricorn(name, n=16):
    """Black felt tricorn, origin at the crown socket; brim turned up on three sides."""
    bm = bmesh.new()
    inner, outer = [], []
    for i in range(n):
        a = 2 * math.pi * i / n
        rel = a + math.pi / 2  # 0 at the front corner (-Y)
        k = (1 - math.cos(3 * rel)) / 2  # 0 at the 3 corners, 1 between
        r_out = 0.19 - 0.04 * k
        inner.append(bm.verts.new((math.cos(a) * 0.105, math.sin(a) * 0.115, -0.03)))
        outer.append(bm.verts.new((math.cos(a) * r_out, math.sin(a) * (r_out + 0.01), -0.03 + 0.09 * k + 0.01)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((inner[i], inner[j], outer[j], outer[i]))
    top = bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.1, radius2=0.085, depth=0.09,
                                matrix=Matrix.Translation((0, 0, 0.012)))
    for v in top["verts"]:
        v.co.y *= 1.1
    return from_bm(bm, name)


def slouch_hat(name, n=14):
    bm = bmesh.new()
    inner, outer = [], []
    for i in range(n):
        a = 2 * math.pi * i / n
        droop = 0.025 * (1 + math.sin(a)) / 2
        inner.append(bm.verts.new((math.cos(a) * 0.1, math.sin(a) * 0.11, -0.03)))
        outer.append(bm.verts.new((math.cos(a) * 0.19, math.sin(a) * 0.2, -0.045 - droop)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((inner[i], inner[j], outer[j], outer[i]))
    top = bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.1, radius2=0.08, depth=0.1,
                                matrix=Matrix.Translation((0, 0, 0.015)))
    for v in top["verts"]:
        v.co.y *= 1.1
    return from_bm(bm, name)


def cocked_hat(name, n=14):
    """Bicorne-like dragoon hat (a tricorn with the front corner flattened)."""
    obj = tricorn(name, n)
    for v in obj.data.vertices:
        if v.co.y < -0.12:
            v.co.y *= 0.8
    return obj


def musket(name, length=1.55, bayonet=0.0):
    """Brown Bess-like musket along +Z, origin at the grip (0.25 m above the butt);
    material 0 wood, 1 metal; empty `socket.muzzle` child at the muzzle."""
    bm = bmesh.new()
    metal = set()
    for (z0, z1, w0, w1, d0, d1) in ((0.0, 0.30, 0.04, 0.03, 0.11, 0.05),
                                     (0.30, length - 0.25, 0.03, 0.026, 0.045, 0.03)):
        r = bmesh.ops.create_cube(bm, size=1.0)
        for v in r["verts"]:
            top = v.co.z > 0
            v.co = Vector((v.co.x * (w1 if top else w0), v.co.y * (d1 if top else d0) + 0.01, z1 if top else z0))
    r = bmesh.ops.create_cone(bm, cap_ends=True, segments=5, radius1=0.011, radius2=0.009, depth=length - 0.30,
                              matrix=Matrix.Translation((0, -0.01, 0.28 + (length - 0.30) / 2)))
    metal |= set(r["verts"])
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        v.co = Vector((v.co.x * 0.035, v.co.y * 0.04 - 0.005, 0.38 + v.co.z * 0.07))
    metal |= set(r["verts"])
    if bayonet:
        r = bmesh.ops.create_cone(bm, cap_ends=True, segments=3, radius1=0.008, radius2=0.001, depth=bayonet,
                                  matrix=Matrix.Translation((0, -0.025, length - 0.02 + bayonet / 2)))
        metal |= set(r["verts"])
    for f in bm.faces:
        f.material_index = 1 if f.verts[0] in metal else 0
    for v in bm.verts:
        v.co.z -= 0.25
    obj = from_bm(bm, name)
    m = bpy.data.objects.new("socket.muzzle", None)
    m.empty_display_size = 0.05
    link(m)
    m.parent = obj
    m.location = (0, -0.01, length - 0.25)
    return obj


def rammer(name, length=0.9):
    """Steel ramrod along +Z from its tail (origin, where the hand holds it) to the head."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=4, radius1=0.0075, radius2=0.0075, depth=length,
                          matrix=Matrix.Translation((0, 0, length / 2)))
    bmesh.ops.create_cone(bm, cap_ends=True, segments=4, radius1=0.012, radius2=0.012, depth=0.03,
                          matrix=Matrix.Translation((0, 0, length - 0.015)))
    return from_bm(bm, name)


def sabre(name, length=0.95):
    """Sabre in its scabbard, along -Z from the hilt (origin)."""
    bm = bmesh.new()
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        v.co = Vector((v.co.x * 0.012, v.co.y * 0.035, -length / 2 + v.co.z * length))
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        v.co = Vector((v.co.x * 0.03, v.co.y * 0.08, 0.04 + v.co.z * 0.10))
    return from_bm(bm, name)


def box(name, size, bevel=0.2):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Diagonal((*size, 1)))
    if bevel:
        bmesh.ops.bevel(bm, geom=list(bm.edges), offset=min(size) * bevel, segments=1, affect="EDGES", profile=0.5)
    return from_bm(bm, name)


def rigid(obj, arm_obj, bone, world_matrix, part=0):
    """Skins gear rigidly to a deforming bone at `world_matrix` (rest pose of the armature
    assumed at identity world transform): it follows the body and its morph targets."""
    obj.matrix_world = world_matrix
    bpy.context.view_layer.update()
    me = obj.data
    me.transform(obj.matrix_world)
    obj.matrix_world = Matrix.Identity(4)
    g = obj.vertex_groups.new(name=bone)
    g.add(list(range(len(me.vertices))), 1.0, "REPLACE")
    attr = me.attributes.new("part", "INT", "POINT")
    for d in attr.data:
        d.value = part
    return am.bind(obj, arm_obj)
