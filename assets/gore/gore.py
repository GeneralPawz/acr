"""Gib pack (#262, docs/damage-model.md section 2): the loose pieces a torn body leaves,
one mesh node each, all with decay materials (`acr_decay` extras, `COLOR_0` decay mask,
`_GORE`), so they rot with the corpse.

Nodes (each mesh's origin is its resting point on the ground; the pack is laid out on a
grid for previews, the renderer takes the meshes, not the node positions):

- `intestine_0..2`: intestine coils, segmented spline tubes (loose meander, tight coil, long
  strand);
- `guts_heap`: a spilled-guts heap (coils piled on a wet mesentery mound);
- `organ_0` (liver lobe), `organ_1` (lung/heart chunk);
- `flesh_0..3`: flesh lumps (skin, fat layer, torn meat);
- `bone_0..2`: bone shards (splintered ends, marrow);
- `skull_fragment` (scalp with hair outside, bone inside), `jaw` (mandible with teeth);
- `cloth_0..1`: cloth scraps in a neutral uniform colour, material extras `acr_tint` =
  "nation" (the renderer multiplies the nation's uniform colour in);
- `stump_cap`: a disc of unit radius in the XY plane facing +Z (the limb's outward axis):
  skin rim, fat ring, red flesh, white bone in the centre with marrow, ragged outline; the
  renderer scales it with `acr_limb_radius`;
- `torn_edge`: a ring strip of unit radius around +Z, height ~0.12..0.35 with a ragged free
  edge (skin outside, meat inside), to sit on a cut around the stump cap.

Wet tissue is glossy (`acr_roughness` 0.22..0.4 in the material extras, kept by the glTF
twin), bone and cloth matte.
"""

import math
import random

import bmesh
import bpy
import numpy as np
from mathutils import Vector, noise

from acr_assets import decay, detail
from acr_assets.humans import prepare_export

# sRGB colours.
RGB = {
    "skin": (0.80, 0.58, 0.45), "fat": (0.86, 0.76, 0.50), "meat": (0.50, 0.06, 0.05),
    "gut": (0.78, 0.50, 0.46), "gut_dark": (0.55, 0.24, 0.25), "mesentery": (0.62, 0.18, 0.16),
    "liver": (0.30, 0.05, 0.04), "lung": (0.70, 0.36, 0.38), "bone": (0.88, 0.85, 0.75),
    "marrow": (0.42, 0.08, 0.06), "teeth": (0.90, 0.86, 0.70), "hair": (0.16, 0.11, 0.07),
    "cloth": (0.78, 0.77, 0.74),
}
# (decay layer, roughness, texture kind) per material key.
KINDS = {
    "skin": ("flesh", 0.45, "skin"), "fat": ("flesh", 0.30, "fat"), "meat": ("flesh", 0.24, "meat"),
    "gut": ("flesh", 0.20, "gut"), "gut_dark": ("flesh", 0.22, "gut"), "mesentery": ("flesh", 0.18, "meat"),
    "liver": ("flesh", 0.22, "liver"), "lung": ("flesh", 0.30, "lung"), "bone": ("bone", 0.60, "bone"),
    "marrow": ("flesh", 0.25, "meat"), "teeth": ("bone", 0.40, "bone"), "hair": ("hair", 0.70, "hair"),
    "cloth": ("wool", 0.90, "wool"),
}
SIZE = 64


def _pattern(kind, seed):
    """Brightness pattern (mean 1) of a wet tissue kind; cloth/bone/hair from `detail`."""
    if kind in ("wool", "hair"):
        return detail.pattern(kind if kind == "wool" else "hair", seed)
    rng = np.random.default_rng(seed)
    n1 = detail._value_noise(rng, SIZE, 4)
    n2 = detail._value_noise(rng, SIZE, 16)
    n3 = detail._value_noise(rng, SIZE, 32)
    y, x = np.mgrid[0:SIZE, 0:SIZE]
    if kind == "meat":  # muscle fibres along x, dark clotted blotches
        fib = 0.5 + 0.5 * np.sin(y * 2 * math.pi * 10 / SIZE + 4 * n1)
        p = 0.70 + 0.22 * fib + 0.2 * n2 - 0.25 * (n1 > 0.7)
    elif kind == "gut":  # vessels: thin dark meandering lines on a pale wall
        v = np.abs(np.sin(x * 2 * math.pi * 3 / SIZE + 6 * n1 + 3 * n2))
        p = 0.92 + 0.10 * n2 - 0.28 * (v < 0.08)
    elif kind == "liver":
        p = 0.85 + 0.2 * n2 + 0.1 * n3
    elif kind == "lung":  # spongy mottling
        p = 0.75 + 0.35 * (n3 > 0.55) + 0.15 * n2
    elif kind == "fat":
        p = 0.85 + 0.25 * (n3 > 0.5) * n2
    elif kind == "bone":
        p = 0.92 + 0.08 * n2 + 0.04 * n3
    else:  # skin
        p = 0.94 + 0.05 * n1 + 0.03 * n3
    return p / p.mean()


_MATS = {}


def mat(key):
    if key in _MATS:
        return _MATS[key]
    layer, rough, kind = KINDS[key]
    lin = np.power(np.array(RGB[key], np.float32), 2.2)
    px = np.ones((SIZE, SIZE, 4), np.float32)
    px[..., :3] = np.power(np.clip(lin[None, None, :] * _pattern(kind, len(_MATS) + 3)[..., None], 0, 1), 1 / 2.2)
    img = bpy.data.images.new(f"gore.{key}", SIZE, SIZE, alpha=False)
    img.pixels.foreach_set(px.ravel())
    img.file_format = "PNG"
    img.pack()
    m = decay.material(f"gore.{key}", layer, RGB[key], image=img, uv_map="detail", roughness=rough,
                       interpolation="Linear")
    m["acr_roughness"] = rough
    if key == "cloth":
        m["acr_tint"] = "nation"
    _MATS[key] = m
    return m


def finish(root, name, bm, keys, layer, at):
    """Mesh object `name` from `bm` (faces' material_index into `keys`), smooth, box UVs,
    decay masks, under `root` at `at`."""
    me = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    obj.parent = root
    obj.location = at
    for k in keys:
        me.materials.append(mat(k))
    detail.box_uv(obj, scale=10.0)
    decay.add_masks(obj, layer, seed=sum(map(ord, name)))
    return obj


# ----------------------------------------------------------------------------- shapes

def _frames(path):
    """Parallel-transport frames (tangent, normal, binormal) along a polyline."""
    out = []
    t0 = (path[1] - path[0]).normalized()
    n = t0.cross(Vector((0, 0, 1)))
    if n.length < 1e-4:
        n = t0.cross(Vector((1, 0, 0)))
    n.normalize()
    for i in range(len(path)):
        a, b = path[max(i - 1, 0)], path[min(i + 1, len(path) - 1)]
        t = (b - a).normalized()
        n = (n - t * n.dot(t)).normalized()
        out.append((t, n, t.cross(n)))
    return out


def sweep(bm, path, radius, seg=10, mi=0, cap=True):
    """A tube along `path` (Vectors) with radius(i, angle) per ring; closed ends."""
    fr = _frames(path)
    rings = []
    for i, (p, (_t, n, b)) in enumerate(zip(path, fr)):
        ring = []
        for k in range(seg):
            a = 2 * math.pi * k / seg
            r = radius(i, a)
            ring.append(bm.verts.new(p + (n * math.cos(a) + b * math.sin(a)) * r))
        rings.append(ring)
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(seg):
            f = bm.faces.new((r0[k], r0[(k + 1) % seg], r1[(k + 1) % seg], r1[k]))
            f.material_index = mi
    if cap:
        for ring, p, flip in ((rings[0], path[0], True), (rings[-1], path[-1], False)):
            c = bm.verts.new(p)
            for k in range(seg):
                vs = (c, ring[(k + 1) % seg], ring[k]) if flip else (c, ring[k], ring[(k + 1) % seg])
                bm.faces.new(vs).material_index = mi


def catmull(ctrl, per=8):
    out = []
    pts = [ctrl[0]] + list(ctrl) + [ctrl[-1]]
    for i in range(1, len(pts) - 2):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[i + 1], pts[i + 2]
        for s in range(per):
            t = s / per
            out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t
                              + (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t))
    out.append(ctrl[-1])
    return out


def intestine(bm, ctrl, r=0.017, mi=0, per=10, seg=10, pitch=0.032, seed=0):
    """A segmented gut: bulges every `pitch` metres along the path, slightly flattened
    where it lies (it sags), a little uneven."""
    path = catmull([Vector(c) for c in ctrl], per)
    s = [0.0]
    for a, b in zip(path, path[1:]):
        s.append(s[-1] + (b - a).length)
    off = Vector((seed * 3.1, seed * 1.7, 0.3))

    def rad(i, a):
        bulge = 0.82 + 0.26 * math.sin(math.pi * s[i] / pitch) ** 2
        lump = 1.0 + 0.10 * noise.noise(path[i] * 30 + off)
        return r * bulge * lump * (1.0 - 0.18 * max(0.0, -math.sin(a)))

    sweep(bm, path, rad, seg=seg, mi=mi)


def blob(bm, centre, size, rough, mi=0, seed=0, subdiv=3, flat=0.0, key_of=None):
    """A displaced icosphere (lumps, organs); `key_of(local_co, normal)` -> material index
    per face."""
    ret = bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=1.0)
    vs = ret["verts"]
    off = Vector((seed * 7.3, seed * 2.1, seed * 5.7))
    for v in vs:
        d = v.co.normalized()
        k = 1.0 + rough * noise.noise(d * 1.8 + off) + rough * 0.5 * noise.noise(d * 4.5 + off)
        v.co = Vector((d.x * size[0], d.y * size[1], d.z * size[2])) * k
        if v.co.z < -flat * size[2]:
            v.co.z = -flat * size[2] + (v.co.z + flat * size[2]) * 0.15  # rests flat on the ground
        v.co += Vector(centre)
    faces = {f for v in vs for f in v.link_faces}
    for f in faces:
        f.material_index = mi
        if key_of:
            c = f.calc_center_median() - Vector(centre)
            f.material_index = key_of(c, f.normal)
    return vs


def ragged(a, seed, amp=0.12):
    return 1.0 + amp * (0.6 * math.sin(3 * a + seed) + 0.4 * math.sin(7 * a + 2 * seed) + 0.5 * random.Random(seed * 1000 + int(a * 100)).uniform(-1, 1) * 0.5)


# ----------------------------------------------------------------------------- pieces

def coil_path(kind, rnd):
    if kind == 0:  # loose meander on the ground
        return [(x * 0.09 - 0.25, 0.08 * math.sin(x * 1.3) + rnd.uniform(-0.02, 0.02), 0.016) for x in range(7)]
    if kind == 1:  # tight pile of loops
        pts = []
        for i in range(22):
            a = i * 1.15
            rr = 0.05 + 0.035 * math.sin(i * 0.7)
            pts.append((rr * math.cos(a), rr * math.sin(a), 0.016 + 0.012 * (i / 7.0) * (1 - i / 22.0) * 3))
        return pts
    pts = []  # long strand
    for i in range(12):
        pts.append((i * 0.085 - 0.45, 0.1 * math.sin(i * 0.9) + 0.03 * math.cos(i * 2.3), 0.016))
    return pts


def build(ctx):
    root = ctx.root
    rnd = random.Random(262)
    grid = iter([Vector((x * 0.7, y * 0.7, 0)) for y in range(5) for x in range(5)])
    objs = []

    for k in range(3):
        bm = bmesh.new()
        intestine(bm, coil_path(k, rnd), r=0.017 if k != 2 else 0.014, mi=0, seed=k)
        objs.append(finish(root, f"intestine_{k}", bm, ["gut" if k != 1 else "gut_dark"], "flesh", next(grid)))

    # Spilled guts: a wet mound with three coils piled on it.
    bm = bmesh.new()
    blob(bm, (0, 0, 0.02), (0.17, 0.13, 0.05), 0.25, mi=1, seed=4, flat=0.4)
    for i in range(3):
        a = i * 2.1
        pts = []
        for j in range(16):
            t = a + j * 0.9
            rr = 0.06 + 0.06 * (j / 16)
            pts.append((rr * math.cos(t) + 0.03 * i, rr * math.sin(t) * 0.8, 0.05 + 0.02 * i + 0.015 * math.sin(j)))
        intestine(bm, pts, r=0.016, mi=0, seed=i + 5)
    objs.append(finish(root, "guts_heap", bm, ["gut", "mesentery"], "flesh", next(grid)))

    bm = bmesh.new()
    blob(bm, (0, 0, 0.03), (0.11, 0.07, 0.035), 0.22, seed=11, flat=0.5)
    objs.append(finish(root, "organ_0", bm, ["liver"], "flesh", next(grid)))
    bm = bmesh.new()
    blob(bm, (0, 0, 0.04), (0.07, 0.06, 0.045), 0.35, seed=12, flat=0.4,
         key_of=lambda c, n: 1 if c.x > 0.03 else 0)
    objs.append(finish(root, "organ_1", bm, ["lung", "meat"], "flesh", next(grid)))

    # Flesh lumps: skin on top, a fat layer under it, torn meat below.
    for k in range(4):
        sz = (0.05 + 0.015 * k, 0.04 + 0.01 * (k % 2), 0.03 + 0.005 * k)

        def key_of(c, n, sz=sz):
            z = c.z / sz[2]
            return 0 if (z > 0.55 and n.z > 0.3) else (1 if z > 0.35 else 2)

        bm = bmesh.new()
        blob(bm, (0, 0, sz[2] * 0.6), sz, 0.32, seed=20 + k, flat=0.5, key_of=key_of)
        objs.append(finish(root, f"flesh_{k}", bm, ["skin", "fat", "meat"], "flesh", next(grid)))

    # Bone shards: a hollow shaft with splintered ends, marrow showing.
    for k in range(3):
        bm = bmesh.new()
        ln = 0.09 + 0.05 * k
        path = [Vector((-ln / 2 + ln * i / 6, 0, 0.014)) for i in range(7)]
        rs = random.Random(30 + k)
        spikes = [rs.uniform(0.3, 1.0) for _ in range(7)]

        def rad(i, a, ln=ln):
            return 0.012 * (1.0 + 0.15 * math.cos(2 * a)) * (0.75 if i in (0, 6) else 1.0)

        sweep(bm, path, rad, seg=7, mi=0, cap=False)
        # Splinter the ends: pull the end rings out into jagged points.
        for v in bm.verts:
            for end, sgn in ((-ln / 2, -1), (ln / 2, 1)):
                if abs(v.co.x - end) < 1e-6:
                    a = math.atan2(v.co.z - 0.014, v.co.y)
                    v.co.x += sgn * 0.02 * spikes[int((a + math.pi) / (2 * math.pi) * 6.99)]
        for end, sgn in ((-ln / 2, -1), (ln / 2, 1)):
            blob(bm, (end + sgn * 0.004, 0, 0.014), (0.004, 0.008, 0.008), 0.2, mi=1, seed=k, subdiv=1)
        objs.append(finish(root, f"bone_{k}", bm, ["bone", "marrow"], "bone", next(grid)))

    # Skull fragment: a curved shell patch, scalp and hair outside, bone inside and on the
    # broken rim.
    bm = bmesh.new()
    R, th = 0.095, 0.007
    n_a, n_b = 9, 7
    outer, inner = {}, {}
    for i in range(n_a + 1):
        for j in range(n_b + 1):
            u, v = (i / n_a - 0.5) * 1.6, (j / n_b - 0.5) * 1.2
            edge = ragged(math.atan2(v, u), 3, 0.18)
            d = Vector((math.sin(u * edge), math.sin(v * edge), math.cos(u * edge) * math.cos(v * edge))).normalized()
            outer[i, j] = bm.verts.new(d * R)
            inner[i, j] = bm.verts.new(d * (R - th))
    for i in range(n_a):
        for j in range(n_b):
            q = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
            bm.faces.new([outer[x] for x in q]).material_index = 0
            bm.faces.new([inner[x] for x in reversed(q)]).material_index = 1
    border = [(i, 0) for i in range(n_a)] + [(n_a, j) for j in range(n_b)] + \
             [(i, n_b) for i in range(n_a, 0, -1)] + [(0, j) for j in range(n_b, 0, -1)]
    for a, b in zip(border, border[1:] + border[:1]):
        bm.faces.new((outer[b], outer[a], inner[a], inner[b])).material_index = 1
    for v in bm.verts:  # lying on its side, the cut down
        v.co = Vector((v.co.x, v.co.z - R * 0.55, v.co.y + R * 0.75))
    objs.append(finish(root, "skull_fragment", bm, ["hair", "bone"], "bone", next(grid)))

    # Jaw: a U-shaped mandible with rami and a row of teeth.
    bm = bmesh.new()
    path = []
    for i in range(15):
        a = math.pi * (i / 14)
        path.append(Vector((0.045 * math.cos(a), -0.05 * math.sin(a), 0.012)))
    path = [path[0] + Vector((0.005, 0.03, 0.02))] + path + [path[-1] + Vector((-0.005, 0.03, 0.02))]
    sweep(bm, path, lambda i, a: 0.008 * (1.0 + 0.6 * abs(math.sin(a))), seg=8, mi=0)
    for i in range(2, 15):
        p = path[i] + Vector((0, 0, 0.011))
        bmesh.ops.create_cube(bm, size=0.006, matrix=__import__("mathutils").Matrix.Translation(p))
    for f in bm.faces:
        if f.calc_center_median().z > 0.0195:
            f.material_index = 1
    objs.append(finish(root, "jaw", bm, ["bone", "teeth"], "bone", next(grid)))

    # Cloth scraps: ragged, crumpled, two-sided.
    for k in range(2):
        bm = bmesh.new()
        n = 14
        c = bm.verts.new((0, 0, 0.006))
        rim = []
        for i in range(n):
            a = 2 * math.pi * i / n
            r = (0.10 + 0.04 * k) * ragged(a, 40 + k, 0.3)
            rim.append(bm.verts.new((r * math.cos(a) * 1.3, r * math.sin(a), 0.004 + 0.012 * abs(math.sin(3 * a + k)))))
        for i in range(n):
            bm.faces.new((c, rim[i], rim[(i + 1) % n]))
        bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=1, use_grid_fill=True)
        for v in bm.verts:
            v.co.z += 0.006 * noise.noise(v.co * 25)
        bmesh.ops.solidify(bm, geom=bm.faces[:], thickness=0.003)
        objs.append(finish(root, f"cloth_{k}", bm, ["cloth"], "wool", next(grid)))

    objs.append(stump_cap(root, next(grid)))
    objs.append(torn_edge(root, next(grid)))

    tris = {}
    for o in objs:
        o.data.calc_loop_triangles()
        tris[o.name] = len(o.data.loop_triangles)
    prepare_export(objs, root, acr_triangles=sum(tris.values()), acr_gibs=[o.name for o in objs])
    ctx.stats = {"triangles": tris}


# Stump cap rings (radius fractions): bone centre, flesh, fat, skin rim.
MARROW, BONE, FAT, SKIN = 0.07, 0.22, 0.80, 0.88


def stump_cap(root, at):
    """Unit disc facing +Z; materials skin 0, fat 1, meat 2, bone 3, marrow 4."""
    bm = bmesh.new()
    n = 32
    rings = (0.0, MARROW, BONE, BONE + 0.08, 0.40, 0.55, 0.70, FAT, SKIN, 1.0)
    keys = {0: 4, 1: 3, 2: 2, 3: 2, 4: 2, 5: 2, 6: 1, 7: 0, 8: 0}

    def height(r, a):
        if r <= BONE:  # the bone stands proud of the flesh
            return 0.10 if r > MARROW else 0.07
        if r < FAT:  # flesh bulges and is torn unevenly
            return 0.06 * (1 - (r - BONE) / (FAT - BONE)) + 0.025 * noise.noise(Vector((r * 6, a * 2, 1.7)))
        return -0.03 * (r - FAT) / (1.0 - FAT)

    verts = []
    centre = bm.verts.new((0, 0, height(0.0, 0.0)))
    for ri, r in enumerate(rings[1:], 1):
        row = []
        for k in range(n):
            a = 2 * math.pi * k / n
            jag = ragged(a, 7, 0.10) if ri >= len(rings) - 3 else 1.0 + 0.03 * noise.noise(Vector((a, r * 3, 0.3)))
            rr = r * jag
            row.append(bm.verts.new((rr * math.cos(a), rr * math.sin(a), height(r, a))))
        verts.append(row)
    for k in range(n):
        bm.faces.new((centre, verts[0][k], verts[0][(k + 1) % n])).material_index = keys[0]
    for ri in range(len(verts) - 1):
        for k in range(n):
            f = bm.faces.new((verts[ri][k], verts[ri + 1][k], verts[ri + 1][(k + 1) % n], verts[ri][(k + 1) % n]))
            f.material_index = keys[ri + 1]
    return finish(root, "stump_cap", bm, ["skin", "fat", "meat", "bone", "marrow"], "flesh", at)


def torn_edge(root, at):
    """Ring strip of unit radius around +Z: skin outside (0), meat inside (1), a ragged
    free edge of flaps 0.12..0.35 high."""
    bm = bmesh.new()
    n, rows = 40, 4
    outer, inner = [], []
    for j in range(rows + 1):
        o_row, i_row = [], []
        for k in range(n):
            a = 2 * math.pi * k / n
            top = 0.12 + 0.23 * abs(math.sin(5 * a + 1.3) * math.sin(3 * a)) + 0.04 * math.sin(17 * a)
            z = top * j / rows
            flare = 1.0 + 0.10 * (j / rows) ** 2  # flaps curl outwards
            o_row.append(bm.verts.new((flare * math.cos(a), flare * math.sin(a), z)))
            i_row.append(bm.verts.new((0.93 * flare * math.cos(a), 0.93 * flare * math.sin(a), z)))
        outer.append(o_row)
        inner.append(i_row)
    for j in range(rows):
        for k in range(n):
            k1 = (k + 1) % n
            bm.faces.new((outer[j][k], outer[j][k1], outer[j + 1][k1], outer[j + 1][k])).material_index = 0
            bm.faces.new((inner[j][k], inner[j + 1][k], inner[j + 1][k1], inner[j][k1])).material_index = 1
    for k in range(n):
        k1 = (k + 1) % n
        bm.faces.new((outer[rows][k], outer[rows][k1], inner[rows][k1], inner[rows][k])).material_index = 1
    return finish(root, "torn_edge", bm, ["skin", "meat"], "flesh", at)
