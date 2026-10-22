"""Simple parametric meshes (needs `bpy`): boxes, hip roofs, posts, quads.

Plans are given in the game's ground coordinates (world pixels, x east, y south) because
that is what one measures on a sprite; heights in world pixels too (true lengths, not
screen pixels: see `conventions`). Everything is converted to metres here.
"""

import math

from . import conventions as cv
from . import scene


def m(x, y, z=0.0):
    """Game world pixels (x east, y south, z up) to Blender metres."""
    s = cv.METRES_PER_PIXEL
    return (x * s, -y * s, z * s)


def rect(c, angle, length, width):
    """Corners of a rectangle centred on c (game x, y), its length along `angle` (degrees,
    measured like `RealDir` on the ground: clockwise from east on screen)."""
    a = math.radians(angle)
    u = (math.cos(a), math.sin(a))
    v = (-u[1], u[0])
    return [
        (c[0] + su * length / 2 * u[0] + sv * width / 2 * v[0], c[1] + su * length / 2 * u[1] + sv * width / 2 * v[1])
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1))
    ]


def along(c, angle, s):
    """The point s pixels from c along `angle`."""
    a = math.radians(angle)
    return (c[0] + s * math.cos(a), c[1] + s * math.sin(a))


def box(root, name, c, angle, length, width, z0, z1):
    base = rect(c, angle, length, width)
    verts = [m(x, y, z0) for x, y in base] + [m(x, y, z1) for x, y in base]
    faces = [(3, 2, 1, 0), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return scene.mesh_object(root, name, scene.from_pydata(name, verts, faces))


def hip_roof(root, name, c, angle, length, width, z_eave, z_ridge, inset):
    """A hip roof over a length x width rectangle; the ridge runs along the length and ends
    `inset` pixels from the gable ends (inset = width / 2 gives 45 degree hips)."""
    base = rect(c, angle, length, width)
    r0, r1 = along(c, angle, -(length / 2 - inset)), along(c, angle, length / 2 - inset)
    verts = [m(x, y, z_eave) for x, y in base] + [m(*r0, z_ridge), m(*r1, z_ridge)]
    # base corners: 0 (-u,-v), 1 (+u,-v), 2 (+u,+v), 3 (-u,+v); ridge 4 (-u), 5 (+u)
    faces = [(3, 2, 1, 0), (0, 1, 5, 4), (1, 2, 5), (2, 3, 4, 5), (3, 0, 4)]
    return scene.mesh_object(root, name, scene.from_pydata(name, verts, faces))


def post(root, name, p, z0, z1, radius=2.0, sides=6):
    """A vertical round post at ground point p (game x, y)."""
    ring = [(p[0] + radius * math.cos(2 * math.pi * i / sides), p[1] + radius * math.sin(2 * math.pi * i / sides)) for i in range(sides)]
    verts = [m(x, y, z0) for x, y in ring] + [m(x, y, z1) for x, y in ring]
    n = sides
    faces = [tuple(range(n)), tuple(range(2 * n - 1, n - 1, -1))]
    faces += [(i, n + i, n + (i + 1) % n, (i + 1) % n) for i in range(n)]
    return scene.mesh_object(root, name, scene.from_pydata(name, verts, faces))


def beam(root, name, a, b, radius=1.5, sides=6):
    """A round beam between two points (game x, y, z in world pixels)."""
    ax, ay, az = m(*a)
    bx, by, bz = m(*b)
    d = (bx - ax, by - ay, bz - az)
    ln = math.sqrt(sum(c * c for c in d))
    d = tuple(c / ln for c in d)
    # Two unit vectors perpendicular to d.
    t = (0.0, 0.0, 1.0) if abs(d[2]) < 0.9 else (1.0, 0.0, 0.0)
    e1 = (d[1] * t[2] - d[2] * t[1], d[2] * t[0] - d[0] * t[2], d[0] * t[1] - d[1] * t[0])
    l1 = math.sqrt(sum(c * c for c in e1))
    e1 = tuple(c / l1 for c in e1)
    e2 = (d[1] * e1[2] - d[2] * e1[1], d[2] * e1[0] - d[0] * e1[2], d[0] * e1[1] - d[1] * e1[0])
    r = radius * cv.METRES_PER_PIXEL
    verts = []
    for o in ((ax, ay, az), (bx, by, bz)):
        for i in range(sides):
            ca, sa = math.cos(2 * math.pi * i / sides), math.sin(2 * math.pi * i / sides)
            verts.append(tuple(o[k] + r * (ca * e1[k] + sa * e2[k]) for k in range(3)))
    n = sides
    faces = [tuple(range(n - 1, -1, -1)), tuple(range(n, 2 * n))]
    faces += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    return scene.mesh_object(root, name, scene.from_pydata(name, verts, faces))


def quad(root, name, corners):
    """A two-sided quad through four points (game x, y, z in world pixels)."""
    verts = [m(*p) for p in corners] * 2
    mesh = scene.from_pydata(name, verts, [(0, 1, 2, 3), (7, 6, 5, 4)], recalc=False)
    return scene.mesh_object(root, name, mesh)
