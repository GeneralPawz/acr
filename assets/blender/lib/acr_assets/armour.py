"""Plate armour worn over the clothes (#522): the shared builder for back and breast with
tassets, and the steel they are painted in.

- `corslet(ch, name, s)` (gear `corslet`): a breast and a back plate formed over the trunk
  as it is dressed (`Field`: rays cast inward at the clothes and the body, the arms left
  out, the envelope smoothed so the plate is a hammered form, not a cast of the folds), a
  medial ridge down the breast to a point at the waist, a rolled edge at the neck and the
  arm openings, a flange turned out at the waist, shoulder straps over the coat; and two
  tassets of overlapping lames hung from the flange. `s["corslet"]` overrides `CORSLET`.
- Plate does not bend: breast, back and straps are rigid on `spine1`. Each tasset lame has
  one weight for all its vertices, between `spine1` (the flange it hangs from) and its
  thigh (`lame_weights`): the top lame stays at the flange, the lowest goes with the thigh,
  the lames slide over each other and none of them bends or stretches.
- Material key `plate` (decay layer `metal`, painter `plate`): `paint_plate` paints what
  the Lambert renderer cannot show, the sky above and the ground below mirrored in the
  steel with the dark line of the horizon between them, plus wear and tarnish; the finish
  is glossy and metallic (`registry.METALLIC`) for the renderer's highlight.
- `s["corslet"]` also fits other wearers: `origin` (a rider's ground point), `lames` 0 (no
  tassets: a horseman's back and breast), `PEASCOD` (the 16th-century form).
- `poke(ch, objs)`: clothes and body vertices standing outside the plates in the current
  pose; the pikeman's build writes it to `stats.armour` for STAND and the walk.

The shape functions (`breast_top`, `back_top`, `breast_bottom`, `ridge`, `lame_weights`,
`lame_rows`) are pure Python (assets/blender/tests/test_armour.py).
"""

import math

from . import registry

try:  # the shape functions are tested without Blender
    from mathutils import Vector
except ImportError:  # pragma: no cover
    Vector = None

# Heights are metres on skeleton H (hips 0.95, spine 1.05, spine1 1.20, spine2 1.33, neck 1.45).
CORSLET = {
    "bone": "spine1",
    "waist": 1.065,  # the breast's lower edge at the sides
    "point": 0.030,  # how far the ridge's point dips below it in front
    "clear": 0.007,  # plate's inside to the smoothed cloth envelope
    "thick": 0.003,
    "roll": 0.0045,  # radius of the rolled edges
    "flange": (0.022, 0.014),  # out, down
    "lames": 7,
    "lame": (0.056, 0.012),  # a lame's height, its overlap with the next
    "tasset": (4.0, 78.0),  # degrees from the front centre: inner and outer edge
    "straps": True,
    "ridge": 0.015,  # the medial crease's height at the waist
    "belly": 0.012,  # the breast's fullness over the belly
    "origin": (0.0, 0.0, 0.0),  # the figure's ground point (a rider's: his `origin`)
}
# A 16th-century peascod (conquistadors): a deep point, a full belly, shorter tassets.
PEASCOD = {"point": 0.050, "ridge": 0.020, "belly": 0.024, "lames": 5, "tasset": (5.0, 72.0)}
PLATE = (0.40, 0.42, 0.46)  # munition steel, kept bright with oil and sand


def _smooth(e0, e1, x):
    t = min(1.0, max(0.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def breast_top(a):
    """Height of the breast's upper edge `a` degrees from the front centre: the neck scoop,
    up to the strap points on the collar bones, then down round the arm to under the armpit."""
    a = abs(a)
    if a < 36.0:
        return 1.385 + 0.055 * _smooth(4.0, 36.0, a)
    return 1.44 - 0.16 * _smooth(36.0, 72.0, a)


def back_top(a):
    """The back's upper edge: straight across below the nape, then down round the arm."""
    a = abs(a)
    if a < 40.0:
        return 1.445
    return 1.445 - 0.165 * _smooth(40.0, 74.0, a)


def breast_bottom(a, waist=CORSLET["waist"], point=CORSLET["point"]):
    """The breast's lower edge: level at the waist, dipping to the ridge's point in front."""
    return waist - point * max(0.0, 1.0 - abs(a) / 42.0) ** 2


def ridge(a, z, waist=CORSLET["waist"], crease_m=CORSLET["ridge"], belly_m=CORSLET["belly"]):
    """How far the breast stands out beyond the trunk's envelope (m): the medial ridge, a
    crease at the centre that grows towards the point at the waist, over a full belly."""
    a = abs(a)
    low = _smooth(1.40, waist + 0.03, z)
    crease = max(0.0, 1.0 - a / 34.0) ** 1.5
    belly = math.exp(-((z - (waist + 0.07)) / 0.11) ** 2) * math.cos(math.radians(min(a, 90.0))) ** 2
    return crease_m * crease * (0.35 + 0.65 * low) + belly_m * belly


def lame_weights(n, top=0.06, bottom=0.92):
    """Thigh weight of each of `n` tasset lames, top to bottom (the rest is the bone the
    flange is on): rising steadily, so neighbours never part by more than their overlap."""
    if n == 1:
        return [bottom]
    return [top + (bottom - top) * (k / (n - 1)) ** 0.85 for k in range(n)]


def lame_rows(n, z_top, height, overlap):
    """(top, bottom) height of each lame: every lame starts `overlap` above the end of the
    one over it."""
    out = []
    z = z_top
    for _ in range(n):
        out.append((z, z - height))
        z = z - height + overlap
    return out


def paint_plate(p, n, rgb, L, seed):
    """Steel as a painted mirror: sky from above, the dark horizon, the ground's warm
    bounce from below, fine scratches drawn down the plate, tarnish in patches."""
    import numpy as np

    from . import coat

    base = coat.lin(rgb)
    up = n[:, 2]
    sky = coat.smooth(-0.02, 0.80, up)
    horizon = np.exp(-((up + 0.10) / 0.15) ** 2)
    ground = coat.smooth(-0.15, -0.90, up)
    f = 0.62 + 1.05 * sky ** 1.4 - 0.34 * horizon + 0.22 * ground
    f *= 0.90 + 0.20 * coat.fbm(p, (55.0, 55.0, 260.0), seed, octaves=2)
    col = base[None, :] * f[:, None]
    col += (sky ** 3)[:, None] * np.array([0.07, 0.09, 0.12], np.float32)[None, :]
    col = coat.mix(col, col * np.array([1.10, 0.98, 0.84], np.float32), ground * 0.6)
    tarnish = coat.smooth(0.60, 0.78, coat.fbm(p, 16.0, seed + 2)) * 0.45
    return coat.mix(col, coat.lin((0.16, 0.11, 0.08)) * (0.6 + 0.6 * f)[:, None], tarnish)


registry.painter("plate", fn=paint_plate, finish=(0.40, 0.0, 0.55), metallic=1.0)
registry.material("plate", "metal", painter="plate", fallback=PLATE)

ARMS = ("upperarm", "forearm", "hand", "finger", "thumb", "index", "middle", "ring", "pinky")


def _arm_vertices(o):
    """Per vertex: True where an arm bone holds the largest weight."""
    names = [g.name for g in o.vertex_groups]
    arm = [any(k in nm for k in ARMS) for nm in names]
    out = []
    for v in o.data.vertices:
        best, w = None, 0.0
        for g in v.groups:
            if g.weight > w:
                best, w = g.group, g.weight
        out.append(best is not None and arm[best])
    return out


class Field:
    """The dressed trunk and thighs as radii round a vertical axis (x = 0, y = `cy`): rays
    cast inward at the clothes and the body without the arms. Angle `t` as `math.atan2`
    (front, -Y, at -90 degrees)."""

    def __init__(self, ch, z0=0.62, z1=1.52, dz=0.01, n=120, origin=(0.0, 0.0, 0.0)):
        import numpy as np
        from mathutils.bvhtree import BVHTree

        verts, polys = [], []
        self.origin = Vector(origin)
        for layer in ("clothes", "flesh"):
            for o in ch.layers[layer]:
                if o.type != "MESH":
                    continue
                arm = _arm_vertices(o)
                base = len(verts)
                m = o.matrix_world
                verts.extend(m @ v.co - self.origin for v in o.data.vertices)
                for f in o.data.polygons:
                    if not any(arm[i] for i in f.vertices):
                        polys.append([base + i for i in f.vertices])
        tree = self.tree = BVHTree.FromPolygons(verts, polys)
        ys = [v.y for v in verts if 1.05 < v.z < 1.40 and abs(v.x) < 0.19 and abs(v.y) < 0.22]
        self.cy = float(np.median(ys)) if ys else 0.0
        self.z0, self.dz, self.n = z0, dz, n
        rows = int(round((z1 - z0) / dz)) + 1
        raw = np.full((rows, n), np.nan, np.float32)
        for k in range(rows):
            z = z0 + k * dz
            for i in range(n):
                t = -math.pi + 2 * math.pi * i / n
                d = Vector((math.cos(t), math.sin(t), 0.0))
                hit = tree.ray_cast(Vector((0.0, self.cy, z)) + d * 0.45, -d, 0.45)
                if hit[0] is not None:
                    raw[k, i] = 0.45 - hit[3]
        # Misses (the arm openings, between the legs): the nearest hit along the ring, else 0.
        for k in range(rows):
            row = raw[k]
            ok = ~np.isnan(row)
            if ok.any() and not ok.all():
                idx = np.arange(n)
                good = idx[ok]
                near = good[np.argmin(np.abs((idx[:, None] - good[None, :] + n // 2) % n - n // 2), axis=1)]
                raw[k] = row[near]
        raw = np.nan_to_num(raw, nan=0.0)
        self.raw = raw
        # The envelope: the largest radius nearby, then smoothed (a plate bridges folds).
        env = raw.copy()
        for s in (-2, -1, 1, 2):
            env = np.maximum(env, np.roll(raw, s, axis=1))
        pad = np.pad(env, ((1, 1), (0, 0)), mode="edge")
        env = np.maximum(env, np.maximum(pad[:-2], pad[2:]))
        for _ in range(3):
            env = (np.roll(env, -2, 1) + np.roll(env, -1, 1) + env + np.roll(env, 1, 1) + np.roll(env, 2, 1)) / 5.0
            pad = np.pad(env, ((2, 2), (0, 0)), mode="edge")
            env = (pad[:-4] + pad[1:-3] + pad[2:-2] + pad[3:-1] + pad[4:]) / 5.0
        self.env = np.maximum(env, raw)

    def _at(self, grid, t, z):
        n = self.n
        x = ((t + math.pi) / (2 * math.pi) * n) % n
        i0 = int(math.floor(x)) % n
        fx = x - math.floor(x)
        y = min(max((z - self.z0) / self.dz, 0.0), grid.shape[0] - 1.001)
        k0 = int(math.floor(y))
        fy = y - k0
        a = grid[k0, i0] * (1 - fx) + grid[k0, (i0 + 1) % n] * fx
        b = grid[k0 + 1, i0] * (1 - fx) + grid[k0 + 1, (i0 + 1) % n] * fx
        return float(a * (1 - fy) + b * fy)

    def radius(self, t, z):
        """The smoothed envelope's radius at angle `t`, height `z`."""
        return self._at(self.env, t, z)

    def hanging(self, t, z, z_top):
        """The radius something hung from `z_top` needs at `z`: the envelope's largest
        radius between the two heights (it never tucks in under a fold going down)."""
        k = max(1, int(round((z_top - z) / self.dz)))
        return max(self._at(self.env, t, z + (z_top - z) * j / k) for j in range(k + 1))

    def point(self, t, r, z):
        return Vector((math.cos(t) * r, self.cy + math.sin(t) * r, z)) + self.origin

    def top(self, x, y, default):
        """Height (local) of the trunk's surface seen from above at local (x, y)."""
        hit = self.tree.ray_cast(Vector((x, y, 1.70)), Vector((0.0, 0.0, -1.0)), 0.6)
        return hit[0].z if hit[0] is not None else default


def _roll(bm, edge, dirs, r, steps=5, turn=250.0):
    """A rolled edge: the plate's edge turned outward and over (`dirs`: outward unit vector
    per edge vertex). Returns the last ring."""
    up = Vector((0.0, 0.0, 1.0))
    prev = edge
    for k in range(1, steps + 1):
        a = math.radians(turn * k / steps)
        ring = [bm.verts.new(v.co + (d * (1 - math.cos(a)) + up * math.sin(a)) * r) for v, d in zip(edge, dirs)]
        for i in range(len(edge) - 1):
            bm.faces.new((prev[i], prev[i + 1], ring[i + 1], ring[i]))
        prev = ring
    return prev


def _finish(bm, name, thick):
    """Outward normals, the plate's thickness (inward), smooth shading."""
    import bmesh

    from . import gear

    bmesh.ops.remove_doubles(bm, verts=bm.verts[:], dist=1e-5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    if thick:
        bmesh.ops.solidify(bm, geom=bm.faces[:], thickness=thick)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    for f in bm.faces:
        f.smooth = True
    obj = gear.from_bm(bm, name)
    obj.data.validate()
    return obj


def _plate(field, centre, half, top, bottom, swell, c, cols=56, rows=34, flange=None):
    """One plate as a bmesh: a grid over angles `centre` ± `half` degrees, between the edge
    functions `bottom(a)` and `top(a)`, standing `c["clear"] + swell(a, z)` off the envelope;
    the top edge rolled, the lower edge turned out into a `flange` (out, down)."""
    import bmesh

    bm = bmesh.new()
    grid, dirs = [], []
    for j in range(cols + 1):
        a = -half + 2.0 * half * j / cols
        t = math.radians(centre + a)
        zb, zt = bottom(a), top(a)
        col = []
        for i in range(rows + 1):
            f = i / rows
            z = zb + (zt - zb) * (1 - (1 - f) ** 1.15)  # a little denser towards the top
            r = field.radius(t, z) + c["clear"] + c["thick"] + swell(a, z)
            col.append(bm.verts.new(field.point(t, r, z)))
        grid.append(col)
        dirs.append(Vector((math.cos(t), math.sin(t), 0.0)))
    for a, b in zip(grid, grid[1:]):
        for i in range(rows):
            bm.faces.new((a[i], b[i], b[i + 1], a[i + 1]))
    _roll(bm, [col[-1] for col in grid], dirs, c["roll"])
    lip = None
    if flange:
        out, down = flange
        prev = [col[0] for col in grid]
        for k in (1, 2, 3):
            f = k / 3.0
            ring = [bm.verts.new(v.co + d * out * f ** 1.5 - Vector((0, 0, down * f))) for v, d in zip([col[0] for col in grid], dirs)]
            for i in range(cols):
                bm.faces.new((ring[i], ring[i + 1], prev[i + 1], prev[i]))
            prev = ring
        lip = prev
    return bm, lip


def _strap(bm, field, x, y0, y1, z0, z1, width=0.026, steps=24, lift=0.004):
    """A shoulder strap from the back plate's top (y1, z1) over the shoulder to the breast's
    (y0, z0) at local `x`: a thin band lying `lift` on the trunk's surface under it, its ends
    lapping 2 cm down onto the plates."""
    ys = [y0 + (y1 - y0) * k / steps for k in range(steps + 1)]
    zs = []
    for k, y in enumerate(ys):
        f = k / steps
        line = z0 + (z1 - z0) * f
        z = max(field.top(x + dx, y, line - lift) for dx in (-width / 2, 0.0, width / 2)) + lift
        zs.append(max(z, line))
    zs = [zs[0]] + [max(b, (a + b + c) / 3.0) for a, b, c in zip(zs, zs[1:], zs[2:])] + [zs[-1]]
    rows = [(y0 - 0.003, z0 - 0.02)] + list(zip(ys, zs)) + [(y1 + 0.003, z1 - 0.02)]
    vs = [(bm.verts.new(field.origin + Vector((x - width / 2, y, z))), bm.verts.new(field.origin + Vector((x + width / 2, y, z))))
          for y, z in rows]
    for (a0, b0), (a1, b1) in zip(vs, vs[1:]):
        bm.faces.new((a0, b0, b1, a1))


def _edge_at(field, centre, top, x, c):
    """Where a plate's upper edge passes local |x| = `x`: (y, z) local, 1.2 cm below the edge."""
    best = None
    for a10 in range(150, 750, 5):
        a = a10 / 10.0
        t = math.radians(centre + a)
        z = top(a) - 0.012
        r = field.radius(t, z) + c["clear"] + c["thick"] + 0.003
        d = abs(abs(math.cos(t) * r) - x)
        if best is None or d < best[0]:
            best = (d, field.cy + math.sin(t) * r, z)
    return best[1], best[2]


def _skin(obj, ch, weights):
    """Skins `obj` with per-vertex {bone: weight} (`weights[i]`); the gear's `part` 0."""
    from . import armature as am

    groups = {}
    for i, w in enumerate(weights):
        for bone, x in w.items():
            if x > 0.0:
                g = groups.get(bone) or groups.setdefault(bone, obj.vertex_groups.new(name=ch.prefix + bone))
                g.add([i], x, "REPLACE")
    attr = obj.data.attributes.new("part", "INT", "POINT")
    for d in attr.data:
        d.value = 0
    return am.bind(obj, ch.arm)


def tassets(ch, name, field, c, key="plate"):
    """Two tassets of `c["lames"]` lames hung from the breast's flange, one over each thigh.
    Every lame is a curved strip with its lower edge standing out over the next one's top;
    all its vertices share one weight between `c["bone"]` and the thigh (`lame_weights`)."""
    import bmesh
    from mathutils.kdtree import KDTree

    from . import gear

    n = c["lames"]
    height, overlap = c["lame"]
    z_top = c["waist"] - 0.003  # tucked up under the flange: no gap when the lames swing
    rows = lame_rows(n, z_top, height, overlap)
    wts = lame_weights(n)
    a0, a1 = c["tasset"]
    cols, bands = 18, 4
    bm = bmesh.new()
    made = []  # (position, lame, side) of every vertex before the thickness is added
    for side in (1, -1):  # +x is the man's left
        for k, (zt, zb) in enumerate(rows):
            grid = []
            for j in range(cols + 1):
                a = a0 + (a1 - a0) * j / cols
                t = math.radians(-90.0 + side * a)
                col = []
                for i in range(bands + 1):
                    f = i / bands
                    z = zt + (zb - zt) * f
                    # Hung from the flange and flaring a little; each lame's lower edge
                    # stands off over the next lame's top, with a turned lip at the end.
                    r = field.hanging(t, z, z_top) + c["clear"] + 0.004 + c["thick"]
                    r = max(r, field.radius(t, c["waist"]) + c["clear"] + c["thick"] + c["flange"][0] * 0.45)
                    r += 0.020 * (z_top - z) / (n * height) + 0.0075 * f + 0.003 * f ** 6
                    # Rounded corners at the outer lower edge of the lowest lame.
                    if k == n - 1 and f > 0.5:
                        z += 0.035 * (f - 0.5) * 2 * max(0.0, (j / cols - 0.75) * 4) ** 2
                    v = bm.verts.new(field.point(t, r, z))
                    made.append((v.co.copy(), k, side))
                    col.append(v)
                grid.append(col)
            for a, b in zip(grid, grid[1:]):
                for i in range(bands):
                    bm.faces.new((a[i], b[i], b[i + 1], a[i + 1]) if side > 0 else (b[i], a[i], a[i + 1], b[i + 1]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    bmesh.ops.solidify(bm, geom=bm.faces[:], thickness=c["thick"])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    for f in bm.faces:
        f.smooth = True
    # The inside's vertices belong to the lame whose outside they lie 3 mm behind.
    kd = KDTree(len(made))
    for i, (co, _k, _s) in enumerate(made):
        kd.insert(co, i)
    kd.balance()
    bm.verts.ensure_lookup_table()
    weights = []
    for v in bm.verts:
        _co, k, side = made[kd.find(v.co)[1]]
        weights.append({c["bone"]: 1.0 - wts[k], "thigh_l" if side > 0 else "thigh_r": wts[k]})
    obj = gear.from_bm(bm, f"{name}.tassets")
    obj.data.validate()
    _skin(obj, ch, weights)
    return ch.add("gear", obj, key)


def corslet(ch, name, s):
    """Back and breast with tassets over the clothes of `ch` (module docstring). Returns the
    tassets; `ch.armour` lists the plates."""
    import bmesh
    import numpy as np
    from mathutils import Matrix

    from . import gear

    c = dict(CORSLET, **(s.get("corslet") or {}))
    field = Field(ch, origin=c["origin"])
    waist, point = c["waist"], c["point"]
    parts = []
    bm, _lip = _plate(field, -90.0, 93.0, breast_top, lambda a: breast_bottom(a, waist, point),
                      lambda a, z: ridge(a, z, waist, c["ridge"], c["belly"]) + 0.0025, c, flange=c["flange"])
    breast = _finish(bm, f"{name}.breast", c["thick"])
    bm, _lip = _plate(field, 90.0, 90.0, back_top, lambda a: waist - 0.012,
                      lambda a, z: 0.004 * math.exp(-((z - 1.34) / 0.09) ** 2), c, cols=48, rows=30,
                      flange=(c["flange"][0] * 0.8, c["flange"][1] * 0.8))
    back = _finish(bm, f"{name}.back", c["thick"])
    parts += [breast, back]
    if c["straps"]:
        bm = bmesh.new()
        x = 0.105
        yf, zf = _edge_at(field, -90.0, breast_top, x, c)
        yb, zb = _edge_at(field, 90.0, back_top, x, c)
        for side in (1, -1):
            _strap(bm, field, side * x, yf, yb, zf, zb)
        parts.append(_finish(bm, f"{name}.straps", 0.0015))
    for o in parts:
        gear.rigid(o, ch.arm, ch.prefix + c["bone"], Matrix.Identity(4))
        ch.add("gear", o, "plate")
    out = tassets(ch, name, field, c) if c["lames"] else parts[0]
    ch.armour = parts + ([out] if c["lames"] else [])
    # How well it sits, unposed: the raw surface against the plate's inside.
    gap = (field.env - field.raw)[int((waist - field.z0) / field.dz):int((1.38 - field.z0) / field.dz)]
    ch.armour_report = {"fit_gap_mean_m": round(float(np.mean(gap)) + c["clear"], 4),
                        "fit_gap_max_m": round(float(np.max(gap)) + c["clear"], 4)}
    return out


registry.gear("corslet", corslet)


def poke(ch, objs=None, reach=0.03):
    """Vertices of the clothes and the body that stand outside the armour `objs` (default
    `ch.armour`) in the current pose: {"trunk": n, "arms": n}. A vertex pokes when the plate
    lies within `reach` behind it (a ray against its normal hits the armour)."""
    import bpy
    from mathutils.bvhtree import BVHTree

    dg = bpy.context.evaluated_depsgraph_get()
    verts, polys = [], []
    for o in objs or getattr(ch, "armour", ()):
        e = o.evaluated_get(dg)
        me = e.to_mesh()
        base = len(verts)
        verts.extend(e.matrix_world @ v.co for v in me.vertices)
        polys.extend([base + i for i in f.vertices] for f in me.polygons)
        e.to_mesh_clear()
    if not polys:
        return {"trunk": 0, "arms": 0}
    tree = BVHTree.FromPolygons(verts, polys)
    out = {"trunk": 0, "arms": 0}
    for layer in ("clothes", "flesh"):
        for o in ch.layers[layer]:
            if o.type != "MESH":
                continue
            arm = _arm_vertices(o)
            e = o.evaluated_get(dg)
            me = e.to_mesh()
            m = e.matrix_world
            rot = m.to_3x3()
            for v in me.vertices:
                co = m @ v.co
                nrm = (rot @ v.normal).normalized()
                if tree.ray_cast(co - nrm * 0.0005, -nrm, reach)[0] is not None:
                    out["arms" if v.index < len(arm) and arm[v.index] else "trunk"] += 1
            e.to_mesh_clear()
    return out


def poke_report(ch, clips, frames=5):
    """`poke` at STAND and through each of `clips` (`frames` samples), for `ctx.stats`."""
    import bpy

    from . import armature as am

    out = {}
    for clip in clips:
        act = bpy.data.actions.get(clip)
        if act is None:
            continue
        f0, f1 = (int(x) for x in act.frame_range)
        worst = {"trunk": 0, "arms": 0, "frame": f0}
        for k in range(frames):
            f = f0 + round((f1 - f0) * k / max(1, frames - 1))
            am.set_clip(ch.arm, clip, f)
            n = poke(ch)
            if n["trunk"] + n["arms"] >= worst["trunk"] + worst["arms"]:
                worst = dict(n, frame=f - f0)
        out[clip] = worst
    return out
