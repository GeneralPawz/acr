"""Building kit (needs `bpy`, `bmesh`): palisades, log walls, roofs, posts, platforms,
scaffolds, log piles and rubble, in the game's ground coordinates like `shapes` (world
pixels: x east, y south, heights true world pixels; angles like `RealDir` on the ground,
clockwise from east on screen). Each call returns one mesh object under `parent`; many small
parts (stakes, rafters) go into one mesh so a fort stays a few dozen draw calls.
"""

import math
import random

import bmesh
import bpy
from mathutils import Matrix, Vector

from . import scene, shapes
from .conventions import METRES_PER_PIXEL as S


def _obj(parent, name, bm):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.validate()
    obj = bpy.data.objects.new(f"{parent.name}.{name}" if not name.startswith(parent.name) else name, me)
    obj.parent = parent
    bpy.context.scene.collection.objects.link(obj)
    return obj


def _m(x, y, z=0.0):
    return Vector(shapes.m(x, y, z))


def _cyl(bm, a, b, r, sides=6, tip=None):
    """A cylinder from a to b (metres), capped, or with a pointed tip of length `tip` beyond
    b."""
    d = (b - a).normalized()
    t = Vector((0, 0, 1)) if abs(d.z) < 0.9 else Vector((1, 0, 0))
    e1 = d.cross(t).normalized()
    e2 = d.cross(e1)
    ring = lambda c: [bm.verts.new(c + (e1 * math.cos(2 * math.pi * i / sides) + e2 * math.sin(2 * math.pi * i / sides)) * r)  # noqa: E731
                      for i in range(sides)]
    lo, hi = ring(a), ring(b)
    for i in range(sides):
        j = (i + 1) % sides
        bm.faces.new((lo[i], lo[j], hi[j], hi[i]))
    bm.faces.new(list(reversed(lo)))
    if tip:
        apex = bm.verts.new(b + d * tip)
        for i in range(sides):
            bm.faces.new((hi[i], hi[(i + 1) % sides], apex))
    else:
        bm.faces.new(hi)


def palisade(parent, name, points, height, radius=4.0, spacing=8.5, jitter=0.08, closed=False, seed=1,
             keep=None, sides=5, sink=6.0):
    """Pointed stakes along the polyline `points` (game x, y), `height` true px above the
    ground (varied by `jitter`), sunk `sink` px into it. `keep(i, t)` may drop stakes (a
    stage under construction): i stake index, t position 0..1 along the whole line."""
    rnd = random.Random(seed)
    pts = list(points) + ([points[0]] if closed else [])
    lens = [math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1)]
    total = sum(lens)
    bm = bmesh.new()
    n = int(total / spacing)
    for i in range(n + 1):
        s = i * total / max(n, 1)
        t = s / total if total else 0
        acc = 0
        for k, ln in enumerate(lens):
            if s <= acc + ln or k == len(lens) - 1:
                f = (s - acc) / ln if ln else 0
                x = pts[k][0] + (pts[k + 1][0] - pts[k][0]) * f
                y = pts[k][1] + (pts[k + 1][1] - pts[k][1]) * f
                break
            acc += ln
        if keep and not keep(i, t):
            continue
        h = height * (1 + rnd.uniform(-jitter, jitter))
        r = radius * rnd.uniform(0.85, 1.15)
        _cyl(bm, _m(x, y, -sink), _m(x, y, h - 2.2 * r), r * S, sides, tip=2.2 * r * S)
    return _obj(parent, name, bm)


def logs(parent, name, a, b, count, radius, z0, step=None, spread=0.0, seed=2, sides=6):
    """A stack or row of horizontal logs from a to b (game x, y): `count` logs starting at
    height z0, each `step` higher (default 2 radius: a log wall); `spread` scatters them
    sideways (a pile)."""
    rnd = random.Random(seed)
    step = 2 * radius if step is None else step
    d = Vector((b[0] - a[0], b[1] - a[1]))
    side = Vector((-d.y, d.x)).normalized() if d.length else Vector((0, 1))
    bm = bmesh.new()
    for i in range(count):
        o = side * (rnd.uniform(-spread, spread) if spread else 0)
        z = z0 + radius + i * step
        _cyl(bm, _m(a[0] + o.x, a[1] + o.y, z), _m(b[0] + o.x, b[1] + o.y, z), radius * S, sides)
    return _obj(parent, name, bm)


def gable_roof(parent, name, c, angle, length, width, z_eave, z_ridge, overhang=8.0, thickness=3.0):
    """A gable roof: ridge along `length` (game px), eaves on both long sides, closed gable
    triangles at the ends (z_eave down to the wall top)."""
    a = math.radians(angle)
    u = Vector((math.cos(a), math.sin(a)))
    v = Vector((-u.y, u.x))
    L, W = length / 2 + overhang, width / 2 + overhang
    cc = Vector(c)
    p = lambda su, sv, z: _m(*(cc + u * su + v * sv), z)  # noqa: E731
    bm = bmesh.new()
    vs = [bm.verts.new(x) for x in (
        p(-L, -W, z_eave), p(L, -W, z_eave), p(L, 0, z_ridge), p(-L, 0, z_ridge),
        p(-L, W, z_eave), p(L, W, z_eave),
        p(-L, -W, z_eave - thickness), p(L, -W, z_eave - thickness), p(-L, W, z_eave - thickness), p(L, W, z_eave - thickness),
    )]
    bm.faces.new((vs[0], vs[1], vs[2], vs[3]))
    bm.faces.new((vs[3], vs[2], vs[5], vs[4]))
    bm.faces.new((vs[6], vs[7], vs[1], vs[0]))
    bm.faces.new((vs[4], vs[5], vs[9], vs[8]))
    bm.faces.new((vs[7], vs[6], vs[8], vs[9]))
    # Gable ends (wall triangles under the roof, set in by the overhang).
    for su in (-length / 2, length / 2):
        g = [bm.verts.new(x) for x in (p(su, -width / 2, z_eave - thickness), p(su, width / 2, z_eave - thickness),
                                      p(su, 0, z_ridge - thickness * 2))]
        bm.faces.new(g)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _obj(parent, name, bm)


def lean_to(parent, name, c, angle, length, width, z_low, z_high, overhang=8.0, thickness=3.0):
    """A shed roof falling from the `-width/2` side (z_high) to the `+width/2` side (z_low)."""
    a = math.radians(angle)
    u = Vector((math.cos(a), math.sin(a)))
    v = Vector((-u.y, u.x))
    L, W = length / 2 + overhang, width / 2 + overhang
    cc = Vector(c)
    p = lambda su, sv, z: _m(*(cc + u * su + v * sv), z)  # noqa: E731
    bm = bmesh.new()
    top = [bm.verts.new(x) for x in (p(-L, -W, z_high), p(L, -W, z_high), p(L, W, z_low), p(-L, W, z_low))]
    bot = [bm.verts.new(x) for x in (p(-L, -W, z_high - thickness), p(L, -W, z_high - thickness),
                                      p(L, W, z_low - thickness), p(-L, W, z_low - thickness))]
    bm.faces.new(top)
    bm.faces.new(list(reversed(bot)))
    for i in range(4):
        j = (i + 1) % 4
        bm.faces.new((top[i], bot[i], bot[j], top[j]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _obj(parent, name, bm)


def hip_roof(parent, name, c, angle, length, width, z_eave, z_ridge, overhang=8.0, inset=None):
    inset = width / 2 + overhang if inset is None else inset
    obj = shapes.hip_roof(parent, name, c, angle, length + 2 * overhang, width + 2 * overhang, z_eave, z_ridge, inset)
    return obj


def box(parent, name, c, angle, length, width, z0, z1):
    return shapes.box(parent, name, c, angle, length, width, z0, z1)


def posts(parent, name, points, z0, z1, radius=3.0, sides=6):
    bm = bmesh.new()
    for x, y in points:
        _cyl(bm, _m(x, y, z0), _m(x, y, z1), radius * S, sides)
    return _obj(parent, name, bm)


def beams(parent, name, segments, radius=2.0, sides=5):
    """Round beams between (game x, y, z) point pairs: rafters, scaffolds, ladders."""
    bm = bmesh.new()
    for a, b in segments:
        _cyl(bm, _m(*a), _m(*b), radius * S, sides)
    return _obj(parent, name, bm)


def rafters(c, angle, length, width, z_eave, z_ridge, count, overhang=6.0):
    """Beam segments of an uncovered gable roof frame (for `beams`)."""
    a = math.radians(angle)
    u = Vector((math.cos(a), math.sin(a)))
    v = Vector((-u.y, u.x))
    cc = Vector(c)
    out = []
    for i in range(count):
        s = -length / 2 + length * i / max(count - 1, 1)
        top = cc + u * s
        for sv in (-1, 1):
            e = cc + u * s + v * sv * (width / 2 + overhang)
            out.append(((e.x, e.y, z_eave), (top.x, top.y, z_ridge)))
    r0, r1 = cc - u * length / 2, cc + u * length / 2
    out.append(((r0.x, r0.y, z_ridge), (r1.x, r1.y, z_ridge)))
    return out


def scaffold(c, angle, length, width, z_top, levels=2, bays=3):
    """Beam segments of a scaffold around a rectangle: corner and bay posts, ledgers per level."""
    a = math.radians(angle)
    u = Vector((math.cos(a), math.sin(a)))
    v = Vector((-u.y, u.x))
    cc = Vector(c)
    corners = [cc + u * su * (length / 2 + 6) + v * sv * (width / 2 + 6) for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    out = []
    for i in range(4):
        p, q = corners[i], corners[(i + 1) % 4]
        for b in range(bays):
            t = b / bays
            x = p + (q - p) * t
            out.append(((x.x, x.y, 0), (x.x, x.y, z_top + 10)))
        for lv in range(1, levels + 1):
            z = z_top * lv / levels
            out.append(((p.x, p.y, z), (q.x, q.y, z)))
    return out


def rubble(parent, name, c, radius, count, size=(6, 14), seed=3, height=20.0):
    """A heap of stones (low boxes) around c (game px) within `radius`, up to `height`."""
    rnd = random.Random(seed)
    bm = bmesh.new()
    for _ in range(count):
        r = radius * math.sqrt(rnd.random())
        a = rnd.uniform(0, 2 * math.pi)
        x, y = c[0] + r * math.cos(a), c[1] + r * math.sin(a)
        s = rnd.uniform(*size)
        z = height * (1 - r / radius) * rnd.random()
        mat = Matrix.Translation(_m(x, y, z + s * 0.3)) @ Matrix.Rotation(rnd.uniform(0, 6.3), 4, "Z") @ Matrix.Rotation(rnd.uniform(-0.4, 0.4), 4, "X")
        bmesh.ops.create_cube(bm, size=1.0, matrix=mat @ Matrix.Diagonal((s * S, s * S * 0.8, s * S * 0.6, 1)))
    return _obj(parent, name, bm)


def ground_patch(parent, name, points, z=1.0):
    """A flat polygon slightly above the ground (trodden earth around the walls)."""
    bm = bmesh.new()
    vs = [bm.verts.new(_m(x, y, z)) for x, y in points]
    f = bm.faces.new(vs)
    if f.normal.z < 0:
        f.normal_flip()
    return _obj(parent, name, bm)


def group(parent, name, **props):
    """An empty grouping a state's parts; custom properties (acr_state, acr_section, ...)
    become glTF extras the renderer and the sprite check read."""
    obj = bpy.data.objects.new(f"{parent.name}.{name}", None)
    obj.parent = parent
    bpy.context.scene.collection.objects.link(obj)
    for k, v in props.items():
        obj[k] = v
    return obj
