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
- `bone_0..3`: bone shards (splintered cortex tubes, marrow cavity: spiral fracture, split
  sliver, joint end, crushed chunk);
- `skull_fragment` (scalp with hair outside, bone inside), `jaw` (mandible with teeth);
- `cloth_0..1`: cloth scraps in a neutral uniform colour, material extras `acr_tint` =
  "nation" (the renderer multiplies the nation's uniform colour in);
- `stump_cap` (torn) and `stump_cap_clean` (sabre cut): discs of unit radius in the XY
  plane facing +Z (the limb's outward axis), see `stump_cap`; the renderer scales them with
  `acr_limb_radius`;
- `torn_edge`: a ring of unit radius around +Z with tatters of meat, skin and cloth (up to
  0.6 long) and hanging intestine loops, to sit on a cut around the stump cap; see
  `torn_edge`.

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
    "skin": (0.80, 0.58, 0.45), "fat": (0.86, 0.76, 0.50), "meat": (0.50, 0.06, 0.05), "meat_dark": (0.32, 0.03, 0.03),
    "gut": (0.78, 0.50, 0.46), "gut_dark": (0.55, 0.24, 0.25), "mesentery": (0.62, 0.18, 0.16),
    "liver": (0.30, 0.05, 0.04), "lung": (0.70, 0.36, 0.38), "bone": (0.88, 0.85, 0.75),
    "marrow": (0.42, 0.08, 0.06), "teeth": (0.90, 0.86, 0.70), "hair": (0.16, 0.11, 0.07),
    "cloth": (0.78, 0.77, 0.74), "meat_cut": (0.62, 0.09, 0.08), "fascia": (0.80, 0.62, 0.56),
}
# (decay layer, roughness, texture kind) per material key.
KINDS = {
    "skin": ("flesh", 0.45, "skin"), "fat": ("flesh", 0.30, "fat"), "meat": ("flesh", 0.24, "meat"), "meat_dark": ("flesh", 0.2, "meat"),
    "gut": ("flesh", 0.20, "gut"), "gut_dark": ("flesh", 0.22, "gut"), "mesentery": ("flesh", 0.18, "meat"),
    "liver": ("flesh", 0.22, "liver"), "lung": ("flesh", 0.30, "lung"), "bone": ("bone", 0.60, "bone"),
    "marrow": ("flesh", 0.25, "meat"), "teeth": ("bone", 0.40, "bone"), "hair": ("hair", 0.70, "hair"),
    "cloth": ("wool", 0.90, "wool"), "meat_cut": ("flesh", 0.14, "meat"), "fascia": ("flesh", 0.16, "fat"),
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

    # Flesh lumps (6..12 cm): dark wet torn muscle with fibres; some keep a patch of skin
    # (with its fat layer) or of the coat cloth.
    for k, (sz, patch) in enumerate((((0.06, 0.045, 0.035), "skin"), ((0.045, 0.035, 0.03), "cloth"),
                                     ((0.035, 0.03, 0.025), None), ((0.05, 0.03, 0.03), "skin"))):
        bm = bmesh.new()
        lump(bm, sz, seed=20 + k, patch=patch)
        objs.append(finish(root, f"flesh_{k}", bm, ["meat_dark", "fat", "cloth" if patch == "cloth" else "skin"],
                           "flesh", next(grid)))

    # Bone shards: hollow shafts (white cortex, dark marrow cavity) with jagged, splintered
    # ends: a spiral fracture with one long point, a split half-pipe sliver, an end with its
    # joint knob, a short crushed chunk.
    for k in range(4):
        bm = bmesh.new()
        bone_shard(bm, k)
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

    # Jaw: a mandible lying on its base: the horseshoe body with chin, the rami rising at
    # the back to condyle and coronoid process, a row of teeth (incisors to molars, two
    # knocked out), torn muscle at the angles.
    bm = bmesh.new()
    mandible(bm)
    objs.append(finish(root, "jaw", bm, ["bone", "teeth", "meat_dark"], "bone", next(grid)))

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

    # The unit-radius pieces stand apart from the small gibs.
    objs.append(stump_cap(root, Vector((5.0, 0, 0)), "stump_cap", torn=True))
    objs.append(stump_cap(root, Vector((5.0, 2.6, 0)), "stump_cap_clean", torn=False))
    objs.append(torn_edge(root, Vector((5.0, 5.2, 0))))

    tris = {}
    for o in objs:
        o.data.calc_loop_triangles()
        tris[o.name] = len(o.data.loop_triangles)
    prepare_export(objs, root, acr_triangles=sum(tris.values()), acr_gibs=[o.name for o in objs])
    ctx.stats = {"triangles": tris}



def _n(x, y, z=0.0):
    return noise.noise(Vector((x, y, z)))


def lump(bm, size, seed, patch=None):
    """A torn lump of muscle: displaced icosphere with fibre ridges along one direction and
    a flat underside; `patch` "skin" (with fat under it) or "cloth" covers part of the top.
    Materials: 0 meat, 1 fat, 2 skin/cloth."""
    ret = bmesh.ops.create_icosphere(bm, subdivisions=4, radius=1.0)
    rs = random.Random(seed)
    off = Vector((seed * 7.3, seed * 2.1, seed * 5.7))
    fdir = Vector((rs.uniform(-1, 1), rs.uniform(-1, 1), rs.uniform(-0.3, 0.3))).normalized()
    for v in ret["verts"]:
        d = v.co.normalized()
        k = 1.0 + 0.35 * noise.noise(d * 1.6 + off) + 0.15 * noise.noise(d * 4.0 + off)
        k += 0.035 * math.sin(d.dot(fdir) * 38 + 3 * noise.noise(d * 2 + off))  # fibres
        k += 0.03 * noise.noise(d * 11 + off)
        c = Vector((d.x * size[0], d.y * size[1], d.z * size[2])) * k
        if c.z < -0.4 * size[2]:
            c.z = -0.4 * size[2] + (c.z + 0.4 * size[2]) * 0.15
        c.z += 0.4 * size[2]
        v.co = c
    tilt = Vector((rs.uniform(-0.4, 0.4), rs.uniform(-0.4, 0.4), 1)).normalized()
    for f in bm.faces:
        f.material_index = 0
        if patch:
            c = f.calc_center_median()
            h = (c - Vector((0, 0, 0.4 * size[2]))).normalized().dot(tilt) + 0.15 * noise.noise(c * 60 + off)
            if h > 0.55:
                f.material_index = 2
            elif h > 0.42 and patch == "skin":
                f.material_index = 1


def loft(bm, rings, mi=0, cap=(True, True), closed=True):
    """Faces between consecutive rings (lists of equal length of Vectors), `mi` an index or
    a function (ring i, k, face centre) -> index; fan caps at the ends of closed rings."""
    vs = [[bm.verts.new(p) for p in r] for r in rings]
    m = len(rings[0])
    span = m if closed else m - 1
    for i in range(len(vs) - 1):
        for k in range(span):
            k1 = (k + 1) % m
            f = bm.faces.new((vs[i][k], vs[i][k1], vs[i + 1][k1], vs[i + 1][k]))
            f.material_index = mi(i, k, f.calc_center_median()) if callable(mi) else mi
    for on, ring, flip in ((cap[0], vs[0], True), (cap[1], vs[-1], False)):
        if not on or not closed:
            continue
        c = bm.verts.new(sum((v.co for v in ring), Vector()) / m)
        for k in range(m):
            q = (c, ring[(k + 1) % m], ring[k]) if flip else (c, ring[k], ring[(k + 1) % m])
            f = bm.faces.new(q)
            f.material_index = mi(0, k, c.co) if callable(mi) else mi
    return vs


def _ellipse(c, ax, ay, rx, ry, m=8):
    return [c + ax * (rx * math.cos(2 * math.pi * q / m)) + ay * (ry * math.sin(2 * math.pi * q / m)) for q in range(m)]


def bone_shard(bm, k):
    """Bone shard `k` along +X lying on the ground: a cortex tube (outer wall bone, inner
    wall marrow-red, the broken rims bone) whose ends are cut per angular column into
    jagged splinters. 0: spiral fracture with one long point; 1: a split half-pipe sliver;
    2: a shaft end with its joint knob; 3: a short crushed chunk. Materials: bone 0,
    marrow 1."""
    rs = random.Random(30 + k)
    seg = 16
    L, r0, wall = ((0.15, 0.015, 0.0045), (0.10, 0.017, 0.005), (0.13, 0.014, 0.004), (0.06, 0.019, 0.006))[k]
    closed = k != 1
    a0, a1 = (0.0, 2 * math.pi) if closed else (-0.3, math.pi + 0.5)
    cols = seg if closed else seg + 1

    def jag(spiral):
        out = []
        for c in range(cols):
            t = c / (cols if closed else cols - 1)
            e = rs.uniform(0.0, 0.010) + (0.012 if c % 2 else 0.0) * rs.uniform(0.4, 1.0)  # sharp teeth
            e += spiral * (2 * min(t, 1 - t))  # one long point at t = 0
            out.append(e)
        return out

    e0 = jag((0.0, 0.0, 0.0, 0.012)[k])
    e1 = jag((0.07, 0.035, 0.0, 0.015)[k])
    if k == 2:
        e1 = [0.0] * cols  # the joint end
    rows = 10
    outer, inner = [], []
    for j in range(rows + 1):
        orow, irow = [], []
        for c in range(cols):
            a = a0 + (a1 - a0) * c / (cols if closed else cols - 1)
            lo, hi = e0[c], L - e1[c]
            x = lo + (hi - lo) * j / rows
            ridge = 1.0 + 0.10 * math.cos(2 * a + k) + 0.05 * math.cos(3 * a)  # not a round pipe
            bend = 0.006 * math.sin(math.pi * x / L) * (1 if k % 2 else -1)
            r = r0 * ridge * (1.0 + 0.05 * math.sin(x * 60 + k))
            cz = r0 * 1.15
            orow.append(Vector((x, bend + r * math.cos(a), cz + r * math.sin(a))))
            ri = r - wall * (0.8 + 0.4 * rs.random())
            irow.append(Vector((x, bend + ri * math.cos(a), cz + ri * math.sin(a))))
        outer.append(orow)
        inner.append(irow)
    loft(bm, outer, 0, cap=(False, False), closed=closed)
    loft(bm, [list(reversed(r)) for r in inner], 1, cap=(False, False), closed=closed)
    span = cols if closed else cols - 1
    for j in (0, rows):  # the broken rims (splinter edges)
        o = [bm.verts.new(p) for p in outer[j]]
        i_ = [bm.verts.new(p) for p in inner[j]]
        for c in range(span):
            c1 = (c + 1) % cols
            q = (o[c], i_[c], i_[c1], o[c1]) if j == 0 else (o[c], o[c1], i_[c1], i_[c])
            bm.faces.new(q).material_index = 0
    if not closed:  # the split edges along the shaft
        for c in (0, cols - 1):
            vo = [bm.verts.new(outer[j][c]) for j in range(rows + 1)]
            vi = [bm.verts.new(inner[j][c]) for j in range(rows + 1)]
            for j in range(rows):
                q = (vo[j], vo[j + 1], vi[j + 1], vi[j]) if c == 0 else (vo[j], vi[j], vi[j + 1], vo[j + 1])
                bm.faces.new(q).material_index = 0
    if k == 2:  # epiphysis: the joint end, two condyles
        for q, dy in enumerate((-0.009, 0.009)):
            blob(bm, (L + 0.004, dy, r0 * 1.15), (0.013, 0.012, 0.016), 0.10, mi=0, seed=k + q, subdiv=2)
    bmesh.ops.remove_doubles(bm, verts=bm.verts[:], dist=1e-6)
    zmin = min(v.co.z for v in bm.verts)
    for v in bm.verts:  # rest on the ground
        v.co.z -= zmin


def mandible(bm):
    """A mandible (metres) lying on its base: horseshoe body with chin, rami with condyle
    and coronoid process, teeth (two knocked out), torn masseter at the angles. Materials:
    bone 0, teeth 1, meat 2."""
    W, D = 0.046, 0.060
    Z = Vector((0, 0, 1))

    def arch(t):  # t -1..1 from the left angle over the chin to the right angle
        th = t * math.pi / 2
        return Vector((W * math.sin(th), -D * math.copysign(abs(math.cos(th)) ** 0.8, 1), 0.0))

    def frame(t):
        p = arch(t)
        tan = (arch(min(t + 0.01, 1)) - arch(max(t - 0.01, -1))).normalized()
        out = tan.cross(Z)
        if out.dot(p - Vector((0, -D * 0.45, 0))) < 0:
            out = -out
        return p, tan, out

    n = 27
    rings, crest = [], []
    for i in range(n):
        t = -1.0 + 2.0 * i / (n - 1)
        p, tan, out = frame(t)
        chin = math.exp(-(t / 0.22) ** 2)
        h = 0.023 + 0.008 * chin  # taller at the symphysis
        w = 0.011 + 0.003 * chin
        ring = []
        for k in range(12):
            a = 2 * math.pi * k / 12
            up = 0.5 - 0.5 * math.cos(a)  # 0 base .. 1 alveolar crest
            lat = math.sin(a) * w / 2 * (1.0 - 0.35 * up)
            bump = 0.004 * chin * max(0.0, math.sin(a)) * (1 - up) ** 2  # mental protuberance
            ring.append(p + out * (lat + bump) + Z * (h * up))
        rings.append(ring)
        crest.append((t, p, tan, out, h))
    loft(bm, rings, 0)
    for sgn in (-1, 1):
        _t, base, _tan, out, _h = crest[0 if sgn < 0 else -1]
        back = Vector((0, 1, 0))
        # Ramus: a plate rising up and back from the angle.
        rr = []
        for j in range(6):
            v = j / 5
            c = base + back * (0.004 + 0.010 * v) + Z * (0.004 + 0.036 * v) + out * (0.003 * v)
            rr.append(_ellipse(c, back, out, 0.016 - 0.002 * v, 0.0035, 10))
        loft(bm, rr, 0)
        top = base + back * 0.014 + Z * 0.040 + out * 0.003
        # Condyle (behind, a transverse knob on a neck) and coronoid (in front, a blade
        # point); the notch between.
        loft(bm, [_ellipse(top + back * 0.009, back, out, 0.004, 0.003),
                  _ellipse(top + back * 0.012 + Z * 0.016, back, out, 0.003, 0.003)], 0)
        blob(bm, top + back * 0.012 + Z * 0.019 + out * 0.001, (0.0045, 0.010, 0.0045), 0.06, mi=0, seed=3 + sgn,
             subdiv=2)
        cv = blob(bm, (0, 0, 0), (0.003, 0.006, 0.010), 0.05, mi=0, seed=5 + sgn, subdiv=2)
        for v in cv:
            v.co = top - back * 0.008 + Z * (0.008 + v.co.z) + back * v.co.y * (1 - max(v.co.z, 0) / 0.012) + out * v.co.x
        # Torn masseter at the angle.
        blob(bm, base + back * 0.010 + Z * 0.012 + out * 0.007, (0.006, 0.013, 0.011), 0.35, mi=2, seed=8 + sgn,
             subdiv=2)
    # Teeth along the crest, per side from the midline: 2 incisors, canine, 2 premolars,
    # 3 molars.
    kinds = (("i", 0.0055), ("i", 0.006), ("c", 0.007), ("p", 0.0072), ("p", 0.0072),
             ("m", 0.011), ("m", 0.0105), ("m", 0.0095))
    missing = {(1, 3), (-1, 6)}
    arc = 0.0
    ts = [-1.0 + 2.0 * i / 400 for i in range(401)]
    lens = [0.0]
    for a, b in zip(ts, ts[1:]):
        lens.append(lens[-1] + (arch(b) - arch(a)).length)
    mid = lens[200]

    def at_len(s):  # parameter t at arc length s from the midline (signed)
        target = mid + s
        i = min(range(401), key=lambda i: abs(lens[i] - target))
        return ts[i]

    for sgn in (-1, 1):
        arc = 0.0006
        for q, (kind, wid) in enumerate(kinds):
            s_mid = arc + wid / 2
            arc += wid + 0.0004
            if (sgn, q) in missing:
                continue
            t = at_len(sgn * s_mid)
            p, tan, out = frame(t)
            chin = math.exp(-(t / 0.22) ** 2)
            h = 0.023 + 0.008 * chin
            hgt = {"i": 0.009, "c": 0.011, "p": 0.008, "m": 0.0068}[kind]
            lat = {"i": 0.0026, "c": 0.0035, "p": 0.0036, "m": 0.0048}[kind]
            vs = blob(bm, (0, 0, 0), (wid * 0.47, lat, hgt * 0.5), 0.05, mi=1, seed=q * 7 + sgn + 40, subdiv=2)
            for v in vs:
                c = v.co.copy()
                z = c.z + hgt * 0.5
                if kind == "i":  # chisel edge
                    c.y *= 1.0 - 0.6 * (z / hgt)
                elif kind == "c":  # pointed
                    c.x *= 1.0 - 0.5 * (z / hgt)
                elif z > hgt * 0.75:  # cusps
                    z += 0.0010 * math.cos(c.x / wid * 14) * math.cos(c.y / lat * 7)
                v.co = p + tan * c.x + out * c.y + Z * (h - 0.0025 + z)
    zmin = min(v.co.z for v in bm.verts)
    for v in bm.verts:
        v.co.y += D * 0.45
        v.co.z -= zmin


def _polar(c, r, a, z):
    return Vector((c[0] + r * math.cos(a), c[1] + r * math.sin(a), z))


def stump_cap(root, at, name, torn=True):
    """Unit disc facing +Z (the limb's outward axis), built from rings that follow the
    tissue borders (organic, not stepped). Clean (sabre): a wet, near-flat cut face of
    muscle bundles parted by pale fascia, an uneven fat layer, an off-centre bone with a
    marrow dot standing a little proud, the skin retracted below the cut (the fat's flank
    shows), a not quite round outline. Torn (cannonball, explosion): lumpy bulging fibrous
    muscle with dark clots, an irregular fat layer, the skin rolled back and ragged, the
    bone proud (0.25..0.55) with sharp splinters around a recessed marrow cavity.
    Materials: skin 0, fat 1, meat 2, bone 3, marrow 4, cut meat 5, fascia 6, dark meat 7."""
    bm = bmesh.new()
    n = 96
    seed = 7 if torn else 9
    rs = random.Random(seed)
    c_b = (0.09, -0.06) if not torn else (0.05, 0.04)  # the bone sits off-centre
    c_m = (c_b[0] + 0.012, c_b[1] - 0.008)

    def outer(a):
        if torn:
            return 0.92 + 0.16 * ragged(a, seed, 0.6) - 0.08 + 0.05 * _n(a * 3, 1.1)
        return 1.0 + 0.035 * math.sin(2 * a + 0.7) + 0.03 * _n(math.cos(a) * 2, math.sin(a) * 2, 0.4)

    def rb(a):  # bone radius: a rounded triangle, not a pipe
        return 0.20 * (1.0 + 0.09 * math.cos(3 * a + 0.5) + 0.04 * _n(math.cos(a) * 3, math.sin(a) * 3, 2.0))

    def rm(a):
        return 0.45 * rb(a) * (1.0 + 0.08 * _n(math.cos(a) * 4, math.sin(a) * 4, 6.0))

    def skin_w(a):
        return 0.04 if not torn else 0.05 + 0.02 * _n(a * 3.1, 5.1, seed)

    def fat_in(a):
        ca, sa = math.cos(a), math.sin(a)
        thick = 0.06 + 0.08 * (0.5 + 0.5 * _n(ca * 1.7, sa * 1.7, 2.2 + seed)) + 0.06 * max(0.0, math.sin(a - 1.0))
        thick += 0.025 * _n(ca * 6, sa * 6, 4.4 + seed)  # an organic, wavy border
        return outer(a) - skin_w(a) - max(thick, 0.03)

    def fat_out(a):
        return outer(a) - skin_w(a)

    spikes = [(rs.uniform(0, 2 * math.pi), rs.uniform(0.12, 0.4), rs.uniform(0.06, 0.32)) for _ in range(11)]

    def splinter(a):
        h = 0.0
        for a0, w, ln in spikes:
            d = abs((a - a0 + math.pi) % (2 * math.pi) - math.pi)
            h = max(h, ln * max(0.0, 1.0 - d / w) ** 1.4)
        return h

    # Muscle bundles: perturbed Voronoi cells in the meat annulus; fascia where two meet.
    seeds = []
    while len(seeds) < 20:
        a, r = rs.uniform(0, 2 * math.pi), math.sqrt(rs.uniform(0.25 ** 2, 0.78 ** 2))
        p = (r * math.cos(a), r * math.sin(a))
        if math.dist(p, c_b) > 0.25 and all(math.dist(p, q) > 0.13 for q, _ in seeds):
            seeds.append((p, rs.random() < 0.35))

    def cells(x, y):
        wx, wy = x + 0.05 * _n(x * 5, y * 5, 1.3), y + 0.05 * _n(x * 5, y * 5, 7.7)
        ds = sorted((math.dist((wx, wy), p), dark) for p, dark in seeds)
        return ds[1][0] - ds[0][0], ds[0][1]

    def meat_z(x, y, t):
        """Height of the muscle at (x, y), t 0 at the bone .. 1 at the fat."""
        if not torn:
            gap, _ = cells(x, y)
            return 0.004 + 0.010 * min(1.0, gap / 0.08) + 0.002 * _n(x * 20, y * 20)
        dome = 0.17 * (1 - t ** 1.6)
        lumps = 0.08 * _n(x * 3.2, y * 3.2, seed) + 0.035 * _n(x * 8, y * 8, seed + 1)
        phi = 0.6 * _n(x * 1.5, y * 1.5, 9.0)
        fib = 0.012 * math.sin((x * math.cos(phi) + y * math.sin(phi)) * 48 + 3 * _n(x * 4, y * 4))
        return max(dome + lumps + fib, 0.02) * (1 - 0.6 * t ** 3)

    def meat_key(_i, _k, c):
        if not torn:
            gap, dark = cells(c.x, c.y)
            return 6 if gap < 0.022 else (2 if dark else 5)
        return 7 if _n(c.x * 7, c.y * 7, 3.0) > 0.3 else 2

    proud = 0.25 if torn else 0.016
    fns = []  # (ring function a -> Vector, material of the band from this ring to the next)
    fns.append((lambda a: _polar(c_m, 0.5 * rm(a), a, proud - (0.15 if torn else 0.005)), 4))
    fns.append((lambda a: _polar(c_m, rm(a), a, proud - (0.10 if torn else 0.004)), 3))
    if torn:  # the splintered cortex: inner wall, jagged rim, outer wall down to the meat
        fns.append((lambda a: _polar(c_b, rm(a) * 1.08 + 0.01, a, proud + 0.8 * splinter(a + 0.05)), 3))
        fns.append((lambda a: _polar(c_b, rb(a), a, proud + splinter(a)), 3))
    else:
        fns.append((lambda a: _polar(c_b, rb(a), a, proud), 3))
    fns.append((lambda a: _polar(c_b, rb(a) * 1.03, a, meat_z(*_polar(c_b, rb(a) * 1.03, a, 0)[:2], 0.0)
                                  - (0.0 if torn else 0.004)), meat_key))
    M = 16 if not torn else 14

    def meat_ring(j):
        def f(a):
            t = j / M
            p = _polar(c_b, rb(a) * 1.03, a, 0).lerp(_polar((0, 0), fat_in(a), a, 0), t)
            if torn and j < M:
                p *= 1.0 + 0.02 * _n(a * 4, t * 5)
            p.z = meat_z(p.x, p.y, t)
            return p
        return f

    for j in range(1, M + 1):
        fns.append((meat_ring(j), meat_key if j < M else 1))

    def fat_ring(t):
        def f(a):
            r = fat_in(a) + (fat_out(a) - fat_in(a)) * t
            x, y = r * math.cos(a), r * math.sin(a)
            z = (0.007 + 0.004 * _n(x * 9, y * 9)) if not torn else 0.02 + 0.02 * _n(x * 6, y * 6)
            return Vector((x, y, z * (1 - 0.3 * t)))
        return f

    for t in (0.35, 0.7, 1.0):
        fns.append((fat_ring(t), 1))
    if not torn:
        # The skin is retracted: the fat's flank shows, then the skin's pale cut edge, then
        # the limb's side going down.
        fns.append((lambda a: _polar((0, 0), fat_out(a) + 0.004, a, -0.035 + 0.008 * _n(a * 5, 1.0)), 1))
        fns.append((lambda a: _polar((0, 0), fat_out(a) + 0.012, a, -0.032 + 0.008 * _n(a * 5, 1.0)), 0))
        fns.append((lambda a: _polar((0, 0), outer(a), a, -0.045), 0))
        fns.append((lambda a: _polar((0, 0), outer(a) * 1.005, a, -0.15), 0))
    else:
        for t in (0.3, 0.6, 1.0):
            def skin(a, t=t):
                r = fat_out(a) + (outer(a) - fat_out(a)) * t
                x, y = r * math.cos(a), r * math.sin(a)
                lip = 0.05 * math.sin(math.pi * min(t / 0.6, 1.0))
                drop = 0.16 * max(0.0, t - 0.6) / 0.4
                return Vector((x, y, lip - drop + 0.03 * _n(x * 9, y * 9)))
            fns.append((skin, 0))
    rings = [[f(2 * math.pi * k / n) for k in range(n)] for f, _ in fns]
    centre = bm.verts.new((c_m[0], c_m[1], proud - (0.16 if torn else 0.006)))
    vs = [[bm.verts.new(p) for p in r] for r in rings]
    for k in range(n):
        bm.faces.new((centre, vs[0][k], vs[0][(k + 1) % n])).material_index = 4
    for i in range(len(vs) - 1):
        key = fns[i][1]
        for k in range(n):
            k1 = (k + 1) % n
            f = bm.faces.new((vs[i][k], vs[i + 1][k], vs[i + 1][k1], vs[i][k1]))
            f.material_index = key(i, k, f.calc_center_median()) if callable(key) else key
    return finish(root, name, bm, ["skin", "fat", "meat", "bone", "marrow", "meat_cut", "fascia", "meat_dark"],
                  "flesh", at)


def _flap(bm, amid, w, length, r0, th, droop, hang, twist, keys, rs, cols=5, rows=10):
    """One tatter from the ring at angle `amid` (`w` radians wide at the root): it rises
    along +Z, then curls outwards and back over the rim (`droop` = (start, end) angle from
    +Z), sags towards -Y by `hang`, narrows and ends in a ragged tip. `keys` = (outer side,
    inner side, torn edges) material indices."""
    tips = [rs.uniform(0.45, 1.0) for _ in range(cols + 1)]
    tips[rs.randrange(cols + 1)] = 1.0
    off = rs.uniform(0, 50)
    ov, iv = [], []
    for j in range(rows + 1):
        orow, irow = [], []
        for c in range(cols + 1):
            u = c / cols - 0.5
            vmax = tips[c]
            ang = amid + u * w * (1.0 - 0.45 * (j / rows) * vmax)
            R = Vector((math.cos(ang), math.sin(ang), 0.0))
            p = Vector((r0 * R.x, r0 * R.y, 0.03))
            th_c = droop[0]
            ds = length * vmax / rows
            for s in range(j):
                th_c = droop[0] + (droop[1] - droop[0]) * ((s + 0.5) / rows) ** 1.2 + twist * u
                p = p + (R * math.sin(th_c) + Vector((0, 0, math.cos(th_c)))) * ds
            v = j / rows * vmax
            p = p + Vector((0, -hang * length * v * v, 0))
            nrm = R * math.cos(th_c) - Vector((0, 0, math.sin(th_c)))
            p = p + nrm * 0.03 * _n(ang * 6 + off, v * 4) * v  # wrinkles
            orow.append(bm.verts.new(p))
            irow.append(bm.verts.new(p - nrm * th))
        ov.append(orow)
        iv.append(irow)
    for j in range(rows):
        for c in range(cols):
            bm.faces.new((ov[j][c], ov[j][c + 1], ov[j + 1][c + 1], ov[j + 1][c])).material_index = keys[0]
            bm.faces.new((iv[j][c], iv[j + 1][c], iv[j + 1][c + 1], iv[j][c + 1])).material_index = keys[1]
    for c in range(cols):  # the torn tip
        bm.faces.new((ov[rows][c], ov[rows][c + 1], iv[rows][c + 1], iv[rows][c])).material_index = keys[2]
    for c, flip in ((0, False), (cols, True)):  # the torn sides
        for j in range(rows):
            q = (ov[j][c], iv[j][c], iv[j + 1][c], ov[j + 1][c])
            bm.faces.new(tuple(reversed(q)) if flip else q).material_index = keys[2]


def torn_edge(root, at):
    """Ring of unit radius around +Z (the waist of a body torn in two; -Y is down when it
    lies): a low ragged band (skin outside, meat inside, fat on the cut) from which hang
    tatters in three overlapping layers, meat inside, skin, coat cloth outside, of uneven
    width and length (0.1..0.6), rising, then curling outwards and back over the rim, the
    ones on the lower side longest and sagging most; four intestine loops spill over the
    rim and hang. Materials: skin 0, meat 1, cloth 2, gut 3, fat 4."""
    bm = bmesh.new()
    rs = random.Random(265)
    n = 64
    o0, o1, i0, i1 = [], [], [], []
    for k in range(n):
        a = 2 * math.pi * k / n
        h = 0.05 + 0.035 * _n(a * 3, 0.5) + 0.02 * _n(a * 9, 2.5)
        r = 1.0 + 0.02 * _n(a * 4, 1.5)
        o0.append(bm.verts.new((r * math.cos(a), r * math.sin(a), -0.06)))
        o1.append(bm.verts.new((r * math.cos(a), r * math.sin(a), h)))
        i0.append(bm.verts.new((0.93 * r * math.cos(a), 0.93 * r * math.sin(a), -0.06)))
        i1.append(bm.verts.new((0.93 * r * math.cos(a), 0.93 * r * math.sin(a), h - 0.01)))
    for k in range(n):
        k1 = (k + 1) % n
        bm.faces.new((o0[k], o0[k1], o1[k1], o1[k])).material_index = 0
        bm.faces.new((i0[k], i1[k], i1[k1], i0[k1])).material_index = 1
        bm.faces.new((o1[k], o1[k1], i1[k1], i1[k])).material_index = 4
    for layer, (r0, th, keys, lens, count) in enumerate((
            (0.90, 0.06, (1, 1, 1), (0.10, 0.30), 7),
            (0.99, 0.025, (0, 1, 1), (0.25, 0.60), 9),
            (1.04, 0.012, (2, 2, 2), (0.30, 0.60), 6))):
        a = rs.uniform(0, 2 * math.pi)
        for _ in range(count):
            w = rs.uniform(0.25, 0.75)
            a += w * (rs.uniform(0.6, 0.9) if layer == 1 else rs.uniform(0.7, 1.4))
            down = max(0.0, -math.sin(a))  # the lower side hangs
            length = min(rs.uniform(*lens) * (0.8 + 0.4 * down) * rs.choice((0.75, 1.0, 1.15)), 0.62)
            droop = (rs.uniform(0.15, 0.6), rs.uniform(1.4, 2.6) if layer else rs.uniform(0.6, 1.4))
            _flap(bm, a, w, length, r0, th, droop, 0.35 * down + 0.08, rs.uniform(-0.4, 0.4), keys, rs)
    for q, (a, ln) in enumerate(((4.3, 0.55), (5.1, 0.42), (3.6, 0.35), (1.2, 0.2))):
        pts = []
        for s, (r, z) in enumerate(((0.5, 0.0), (0.75, 0.08), (1.0, 0.11), (1.12 + 0.3 * ln, 0.0),
                                    (1.18 + 0.4 * ln, -ln * 0.35), (1.1 + 0.3 * ln, -ln * 0.45), (1.06, -0.1),
                                    (1.0, 0.07), (0.8, 0.06), (0.55, 0.0))):
            aa = a + 0.35 * s / 9
            sag = -0.35 * ln * max(0.0, -math.sin(aa)) * (1 if 3 <= s <= 6 else 0)
            pts.append((r * math.cos(aa), r * math.sin(aa) + sag, z))
        intestine(bm, pts, r=0.06, mi=3, per=6, seg=10, pitch=0.13, seed=q + 20)
    return finish(root, "torn_edge", bm, ["skin", "meat", "cloth", "gut", "fat"], "flesh", at)
