"""Scripted masonry buildings and mine pits in metres (needs `bpy`, `bmesh`, `numpy`): the
types Pixal3D melts (#305: the neutral stone temples `NetBlg1/3/4/7/8`, the mines
`SpnRudCo/Ir/St`, the Aztec fort `AtcKre`).

A unit script fills a `Kit` with blocks (`box`, tapered to a frustum with `taper`),
`stairs`, `merlons` along a rim, `beam`s between two points (timber frames), jittered
rubble (`heap`, on a disc or a ring) and a `pit` (a funnel down to a dark floor; the floor
stays above the ground, the terrain is not cut). All blocks go into one mesh, one material
slot per surface kind (`surfaces.KINDS`, or a flat colour for `FLAT` kinds).

`finish(ctx, kit)` centres it, fits turn, uniform scale and ground shift to the `#STANDLO`
sprite (`spritefit.fit`, like `genbuild.building`), lays the surface UVs aligned to each
face in metres per tile of its kind (after the fit, so tiles keep their real size), and adds
the generic ruin (`ruin.add`). Stats: `fit`, `iou` (the `acr model-check` raster),
`triangles`, `size_m`, `blocks`, and `ruin`.
"""

import math
import random

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

from . import materials, surfaces

CELL = 0.6  # metres: grid size of the block faces

# Flat colours (sRGB) for kinds that need no texture.
FLAT = {
    "pit": (0.09, 0.08, 0.07),
    "ore": (0.16, 0.15, 0.14),
    "plaster": (0.80, 0.74, 0.60),
}


class Kit:
    def __init__(self, seed=0):
        self.bm = bmesh.new()
        self.kinds = []
        self.rnd = random.Random(seed)
        self.blocks = 0

    def _slot(self, kind):
        if kind not in self.kinds:
            self.kinds.append(kind)
        return self.kinds.index(kind)

    def _quad(self, corners, slot):
        """One quad as a grid of about `CELL` m cells (finer faces see the sprite better: a
        face takes the projection's colours only when it is visible as a whole)."""
        a, b, c, d = (Vector(p) for p in corners)
        nu = max(1, min(24, round(max((b - a).length, (c - d).length) / CELL)))
        nv = max(1, min(24, round(max((d - a).length, (c - b).length) / CELL)))
        grid = [[self.bm.verts.new(a.lerp(b, i / nu).lerp(d.lerp(c, i / nu), j / nv)) for i in range(nu + 1)]
                for j in range(nv + 1)]
        for j in range(nv):
            for i in range(nu):
                f = self.bm.faces.new((grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]))
                f.material_index = slot

    def _hull(self, kind, pts):
        """Box from 8 corners: bottom 0-3, top 4-7, counter-clockwise seen from above. The
        bottom face is left out (it stands on the ground or on another block)."""
        slot = self._slot(kind)
        for q in ((4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)):
            self._quad([pts[i] for i in q], slot)
        self.blocks += 1

    def box(self, kind, c, size, z0, z1, yaw=0.0, taper=0.0):
        """Block centred on `c` (x, y), `size` (sx, sy), from `z0` to `z1`, turned `yaw`
        degrees; the top is inset by `taper` (share of the size) on every side."""
        a = math.radians(yaw)
        ca, sa = math.cos(a), math.sin(a)
        pts = []
        for z, k in ((z0, 1.0), (z1, 1.0 - 2 * taper)):
            for ux, uy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                x, y = ux * size[0] / 2 * k, uy * size[1] / 2 * k
                pts.append((c[0] + x * ca - y * sa, c[1] + x * sa + y * ca, z))
        self._hull(kind, pts)

    def tiers(self, kind, c, sizes, z0, heights, taper=0.0):
        """Stacked blocks, one per (sx, sy) in `sizes` and height in `heights`."""
        z = z0
        for s, h in zip(sizes, heights):
            self.box(kind, c, s, z, z + h, taper=taper)
            z += h
        return z

    def stairs(self, kind, foot, width, run, z0, z1, n, facing):
        """A flight of `n` steps from `z0` at `foot` (x, y; the bottom step's front edge
        centre) up to `z1`, climbing over `run` metres against `facing` (degrees: the
        direction the stairs face, 0 = -Y, 90 = +X)."""
        a = math.radians(facing)
        d = (math.sin(a), -math.cos(a))  # the direction they face (down the flight)
        step = run / n
        for i in range(n):
            s = (i + 0.5) * step
            # each step a block from the ground to its tread, stepping back up the flight
            cx, cy = foot[0] - d[0] * s, foot[1] - d[1] * s
            self.box(kind, (cx, cy), (width, step * 1.02), z0, z0 + (z1 - z0) * (i + 1) / n, yaw=facing)

    def cornice(self, kind, c, size, z, h=0.3, out=0.25):
        """A moulding band `h` high standing `out` proud of a block of `size` at height `z`."""
        self.box(kind, c, (size[0] + 2 * out, size[1] + 2 * out), z, z + h)

    def door(self, kind, c, face_y, w=1.4, h=2.4, out=0.3, jamb=0.4, z0=0.0):
        """A doorway on the south face (y = `face_y`) of a block, centred at x = `c`: two
        jambs and a lintel standing `out` proud of the wall, the opening between them in the
        wall's own plane (the sprite's dark doorway projects onto it)."""
        y = face_y - out / 2
        for sx in (-1, 1):
            self.box(kind, (c + sx * (w / 2 + jamb / 2), y), (jamb, out), z0, z0 + h)
        self.box(kind, (c, y), (w + 2 * jamb + 0.2, out + 0.05), z0 + h, z0 + h + 0.45)

    def merlons(self, kind, c, size, z, h, w=0.5, gap=0.5, depth=0.45):
        """Merlons of width `w` and gap `gap` standing on the rim of the rectangle `size`."""
        sx, sy = size
        for (ax, ay), (bx, by) in (((-1, -1), (1, -1)), ((1, -1), (1, 1)), ((1, 1), (-1, 1)), ((-1, 1), (-1, -1))):
            p = Vector((c[0] + ax * sx / 2, c[1] + ay * sy / 2))
            q = Vector((c[0] + bx * sx / 2, c[1] + by * sy / 2))
            length = (q - p).length
            n = max(1, int((length + gap) / (w + gap)))
            yaw = math.degrees(math.atan2(q.y - p.y, q.x - p.x))
            inward = Vector((-(q - p).y, (q - p).x)).normalized() * depth / 2
            for i in range(n):
                t = (i + 0.5) / n
                m = p.lerp(q, t) + inward
                self.box(kind, (m.x, m.y), (w, depth), z, z + h, yaw=yaw)

    def beam(self, kind, a, b, t=0.18):
        """A square timber of side `t` from point `a` to point `b`."""
        a, b = Vector(a), Vector(b)
        d = (b - a).normalized()
        ref = Vector((0, 0, 1)) if abs(d.z) < 0.9 else Vector((1, 0, 0))
        e1 = d.cross(ref).normalized() * t / 2
        e2 = d.cross(e1).normalized() * t / 2
        pts = [tuple(p + s1 * e1 + s2 * e2) for p in (a, b) for s1, s2 in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
        self._hull(kind, pts)

    def heap(self, kind, c, radius, n, size, top, ring=0.0, squash=1.0, arc=None):
        """`n` jittered blocks (edge `size` x 0.5..1.3) scattered over a disc of `radius`
        (or the ring from `ring` x radius out), stacked into a mound `top` high at its
        middle line; `squash` scales the y radius; `arc` (start, end degrees) limits it."""
        rnd = self.rnd
        for _ in range(n):
            ang = math.radians(rnd.uniform(*arc)) if arc else rnd.uniform(0, 2 * math.pi)
            u = rnd.uniform(ring, 1.0) if ring else math.sqrt(rnd.random())
            mid = (ring + 1.0) / 2 if ring else 0.0
            hump = max(0.0, 1.0 - abs(u - mid) / max(1e-6, 1.0 - mid))
            x = c[0] + math.cos(ang) * u * radius
            y = c[1] + math.sin(ang) * u * radius * squash
            e = size * rnd.uniform(0.5, 1.3)
            z0 = rnd.uniform(0.0, 0.7) * top * hump
            self.box(kind, (x, y), (e, e * rnd.uniform(0.6, 1.2)), z0, z0 + e * rnd.uniform(0.5, 0.9),
                     yaw=rnd.uniform(0, 90), taper=rnd.uniform(0.0, 0.15))

    def pit(self, wall, floor, c, r_top, r_floor, z_top, z_floor, sides=20, squash=1.0):
        """A funnel from the rim (`r_top` at `z_top`) down to a flat floor (`r_floor` at
        `z_floor`), faces turned inward (seen from above and inside); `floor` None leaves
        the bottom open (the ground shows, as in the coal mine's sprite)."""
        ws = self._slot(wall)
        top, bot = [], []
        for i in range(sides):
            a = 2 * math.pi * i / sides
            j = 1.0 + 0.08 * math.sin(3 * a + 1.3) + self.rnd.uniform(-0.04, 0.04)
            top.append(Vector((c[0] + math.cos(a) * r_top * j, c[1] + math.sin(a) * r_top * j * squash, z_top)))
            bot.append(self.bm.verts.new((c[0] + math.cos(a) * r_floor * j, c[1] + math.sin(a) * r_floor * j * squash, z_floor)))
        for i in range(sides):
            k = (i + 1) % sides
            self._quad([top[i], top[k], bot[k].co.copy(), bot[i].co.copy()], ws)
        if floor is None:
            self.blocks += 1
            return
        f = self.bm.faces.new(list(bot))
        f.material_index = self._slot(floor)
        f.normal_update()
        if f.normal.z < 0:
            f.normal_flip()
        self.blocks += 1


def _material(kind, tint=None, uv="surface"):
    """The material of a surface kind; with `tint` (sRGB) its tile is scaled to that mean
    colour (the sprite's colour for the faces no sprite view sees)."""
    if kind in FLAT:
        return materials.flat(f"stone.{kind}" + (".unseen" if tint else ""), tint or FLAT[kind])
    if tint is None:
        return surfaces.material(kind)
    gen, _tile, rough = surfaces.KINDS[kind]
    rgb = gen()
    lin = np.array([materials.srgb_to_linear(c) for c in tint], np.float32)
    rgb = rgb * (lin / np.maximum(rgb.reshape(-1, 3).mean(axis=0), 1e-4))
    n = surfaces.SIZE
    px = np.ones((n, n, 4), np.float32)
    px[..., :3] = np.clip(rgb, 0, 1)
    img = bpy.data.images.new(f"stone_{kind}_unseen", n, n, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "sRGB"
    img.pixels.foreach_set(np.power(px, [1 / 2.2, 1 / 2.2, 1 / 2.2, 1.0]).astype(np.float32).ravel())
    img.file_format = "PNG"
    img.pack()
    return materials.textured(f"stone_{kind}_unseen", img, uv, roughness=rough, closest=False)


def _uvs(me, kinds):
    """Surface UVs aligned to each face (`surfaces.aligned_uv`), per kind's tile size
    (`kinds`: the kind of each polygon)."""
    uv = me.uv_layers.get("surface") or me.uv_layers.new(name="surface")
    for poly in me.polygons:
        kind = kinds[poly.index]
        k = 1.0 / (surfaces.KINDS[kind][1] if kind in surfaces.KINDS else 1.0)
        n = poly.normal
        if abs(n.z) > 0.97:
            t, b = (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)
        else:
            tl = math.hypot(n.x, n.y)
            t = (-n.y / tl, n.x / tl, 0.0)
            b = (n.y * t[2] - n.z * t[1], n.z * t[0] - n.x * t[2], n.x * t[1] - n.y * t[0])
            if b[2] < 0:
                b = (-b[0], -b[1], -b[2])
        for li in poly.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            uv.data[li].uv = ((co.x * t[0] + co.y * t[1] + co.z * t[2]) * k,
                              (co.x * b[0] + co.y * b[1] + co.z * b[2]) * k)
    me.uv_layers.active = uv


def finish(ctx, kit, yaws=range(0, 360, 10)):
    """The kit as one mesh under `ctx.root`, fitted to the sprite, coloured, with its ruin.

    Colour: the faces a sprite view sees take the sprite's pixels through the original
    camera (`project.project_uvs`, as the scripted `incdo1`; at game zoom the model reads
    like the sprite, carving and weathering included); the faces it does not see (backs,
    hidden ledges) take their kind's procedural tile scaled to the dominant sprite colour
    instead of a flat one. Without a reference every face keeps its procedural tile."""
    from . import genbuild, project, ruin, spritefit
    from . import vessels as vs

    me = bpy.data.meshes.new(ctx.root.name)
    kit.bm.to_mesh(me)
    kit.bm.free()
    me.validate()
    kind_of = np.zeros(len(me.polygons), np.int32)
    me.polygons.foreach_get("material_index", kind_of)
    obj = bpy.data.objects.new(ctx.root.name, me)
    bpy.context.scene.collection.objects.link(obj)
    v, _ = genbuild._arrays(obj)
    c = (v.min(axis=0) + v.max(axis=0)) / 2
    vs.transform([obj], Matrix.Translation((-c[0], -c[1], -float(v[:, 2].min()))))
    _uvs(me, [kit.kinds[k] for k in kind_of])
    index = kind_of.copy()
    if ctx.ref is not None:
        view = ctx.ref.views[0]
        m = genbuild.mask(view)
        f = spritefit.fit(genbuild.sample(obj, 80000, seed=1), m, ctx.ref.anchor, view.dir, yaws=yaws)
        vs.transform([obj], genbuild._matrix(f))
        ctx.stats["fit"] = {k: (round(x, 4) if isinstance(x, float) else x) for k, x in f.items() if k not in ("screen", "rough")}
        ctx.stats["fit"]["shift"] = [round(x, 3) for x in f["shift"]]
        ctx.stats["iou"] = genbuild._final_iou(obj, m, ctx.ref.anchor, view.dir)
        _uvs(me, [kit.kinds[k] for k in kind_of])  # tiles at the fitted size
        image, info = project.project_uvs([obj], ctx.ref)
        hidden, rgb, _samples = info[obj.name]
        me.materials.append(materials.textured("sprite", image, "sprite"))
        for kind in kit.kinds:
            me.materials.append(_material(kind, tuple(rgb) if rgb else None, uv="sprite"))
        hid = np.zeros(len(me.polygons), bool)
        hid[list(hidden)] = True
        # The renderer reads one UV set: the unseen faces carry their tile UVs in the
        # `sprite` map too (they do not use the atlas), and the `surface` map goes.
        sp, su = me.uv_layers["sprite"], me.uv_layers["surface"]
        for i in np.nonzero(hid)[0]:
            for li in me.polygons[int(i)].loop_indices:
                sp.data[li].uv = su.data[li].uv
        me.uv_layers.remove(su)
        me.uv_layers.active = me.uv_layers["sprite"]
        index = np.where(hid, kind_of + 1, 0).astype(np.int32)
        ctx.stats["seen_share"] = round(1.0 - float(hid.mean()), 3)
    else:
        for kind in kit.kinds:
            me.materials.append(_material(kind))
    me.polygons.foreach_set("material_index", index)
    for p in me.polygons:
        p.use_smooth = False
    ctx.stats["blocks"] = kit.blocks
    ctx.stats["triangles"] = vs.triangles([obj])
    ctx.stats["size_m"] = genbuild._size(obj)
    obj.parent = ctx.root
    ruin.add(ctx, obj)
    return obj
