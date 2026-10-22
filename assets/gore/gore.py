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
- `stump_cap` (torn) and `stump_cap_clean` (sabre cut): discs of unit radius in the XY plane facing +Z (the limb's outward axis):
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
    "skin": (0.80, 0.58, 0.45), "fat": (0.86, 0.76, 0.50), "meat": (0.50, 0.06, 0.05), "meat_dark": (0.32, 0.03, 0.03),
    "gut": (0.78, 0.50, 0.46), "gut_dark": (0.55, 0.24, 0.25), "mesentery": (0.62, 0.18, 0.16),
    "liver": (0.30, 0.05, 0.04), "lung": (0.70, 0.36, 0.38), "bone": (0.88, 0.85, 0.75),
    "marrow": (0.42, 0.08, 0.06), "teeth": (0.90, 0.86, 0.70), "hair": (0.16, 0.11, 0.07),
    "cloth": (0.78, 0.77, 0.74),
}
# (decay layer, roughness, texture kind) per material key.
KINDS = {
    "skin": ("flesh", 0.45, "skin"), "fat": ("flesh", 0.30, "fat"), "meat": ("flesh", 0.24, "meat"), "meat_dark": ("flesh", 0.2, "meat"),
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

    # Flesh lumps (6..12 cm): dark wet torn muscle with fibres; some keep a patch of skin
    # (with its fat layer) or of the coat cloth.
    for k, (sz, patch) in enumerate((((0.06, 0.045, 0.035), "skin"), ((0.045, 0.035, 0.03), "cloth"),
                                     ((0.035, 0.03, 0.025), None), ((0.05, 0.03, 0.03), "skin"))):
        bm = bmesh.new()
        lump(bm, sz, seed=20 + k, patch=patch)
        objs.append(finish(root, f"flesh_{k}", bm, ["meat_dark", "fat", "cloth" if patch == "cloth" else "skin"],
                           "flesh", next(grid)))

    # Bone shards: a hollow shaft with splintered ends, marrow showing.
    for k in range(3):
        bm = bmesh.new()
        ln = 0.10 + 0.05 * k
        r0 = 0.014 + 0.002 * k
        path = [Vector((-ln / 2 + ln * i / 8, 0.004 * math.sin(i * 0.8 + k), r0)) for i in range(9)]
        rs = random.Random(30 + k)
        spikes = [rs.uniform(0.0, 1.0) ** 1.5 for _ in range(12)]

        def rad(i, a, r0=r0, k=k):
            ridge = 1.0 + 0.12 * math.cos(2 * a) + 0.06 * math.cos(3 * a + k)  # not a round pipe
            return r0 * ridge * (1.0 + 0.04 * math.sin(i * 1.7))

        mark = len(bm.faces)
        sweep(bm, path, rad, seg=12, mi=0, cap=True)
        # Splinter the ends into jagged points around a recessed marrow core (the cap
        # centres); bone_2 keeps its knobbed joint end.
        for end, sgn in ((-ln / 2, -1), (ln / 2, 1)):
            if k == 2 and sgn > 0:
                continue
            for v in bm.verts:
                if abs(v.co.x - end) < 1e-6:
                    if abs(v.co.y - path[0].y if sgn < 0 else v.co.y - path[-1].y) < 1e-9 and abs(v.co.z - r0) < 1e-9:
                        v.co.x -= sgn * 0.008  # the marrow core sits in the hollow
                        continue
                    a = math.atan2(v.co.z - r0, v.co.y)
                    v.co.x += sgn * 0.035 * spikes[int((a + math.pi) / (2 * math.pi) * 11.99)]
        for f in list(bm.faces)[mark:]:
            if len(f.verts) == 3:
                f.material_index = 1
                if k == 2 and f.calc_center_median().x > 0:
                    f.material_index = 0
        if k == 2:  # epiphysis: the joint knob of the unbroken end
            blob(bm, (ln / 2 + 0.008, 0.0, r0), (0.016, 0.022, 0.02), 0.12, mi=0, seed=k, subdiv=2)
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


def stump_cap(root, at, name, torn=True):
    """Unit disc facing +Z (the limb's outward axis) on a polar grid. Torn (cannonball,
    explosion): lumpy bulging fibrous muscle, an irregular fat layer, the skin rim rolled
    back and ragged, the bone standing proud (0.3..0.55 of the radius: 2..4 cm on a limb)
    with a splintered end and recessed marrow. Clean (sabre): a near-flat cut face, the bone
    flush, a smooth rolled rim. Materials: skin 0, fat 1, meat 2, bone 3, marrow 4."""
    bm = bmesh.new()
    n = 56
    seed = 7 if torn else 9
    rs = random.Random(seed)
    spikes = [rs.uniform(0.0, 1.0) for _ in range(9)]
    mar, bone = 0.105, 0.245
    rings = [0.0, 0.05, 0.10, 0.11, 0.17, 0.24, 0.25, 0.30, 0.36, 0.42, 0.48, 0.54, 0.60, 0.65, 0.69, 0.73,
             0.77, 0.81, 0.85, 0.88, 0.91, 0.94, 0.97, 1.0]

    def outer(a):
        if torn:
            return 0.92 + 0.16 * ragged(a, seed, 0.6) - 0.08 + 0.05 * _n(a * 3, 1.1)
        return 1.0 + 0.025 * _n(a * 2, 0.4)

    def fat_in(a):
        return 0.70 + 0.07 * _n(a * 2.5, 3.3, seed)

    def fat_out(a):
        return 0.82 + 0.04 * _n(a * 3.1, 5.1, seed)

    def proud(a):
        if not torn:
            return 0.012
        i = (a / (2 * math.pi)) * len(spikes)
        j = int(i) % len(spikes)
        t = i - int(i)
        sp = spikes[j] * (1 - t) + spikes[(j + 1) % len(spikes)] * t
        return 0.30 + 0.25 * sp ** 2

    def height(s, a, x, y):
        if s <= mar:
            return proud(a) - (0.12 if torn else 0.01)
        if s <= bone:
            return proud(a) - (0.0 if s > 0.2 else 0.015)
        fo = fat_out(a)
        if s < fo:
            if torn:
                t = (s - bone) / (fo - bone)
                dome = 0.17 * (1 - t ** 1.6)
                lumps = 0.08 * _n(x * 3.2, y * 3.2, seed) + 0.035 * _n(x * 8, y * 8, seed + 1)
                phi = 0.6 * _n(x * 1.5, y * 1.5, 9.0)
                fib = 0.012 * math.sin((x * math.cos(phi) + y * math.sin(phi)) * 48 + 3 * _n(x * 4, y * 4))
                z = dome + lumps + fib
                if s > fat_in(a):
                    z = min(z, 0.03 + 0.02 * _n(x * 6, y * 6))  # the fat layer sits lower
                return z
            fib = 0.004 * math.sin(x * 55 + 2 * _n(x * 3, y * 3))
            return 0.012 * (1 - (s - bone) / (fo - bone)) + 0.008 * _n(x * 5, y * 5) + fib
        # Skin: rolled back into a lip, then down and away (ragged when torn).
        t = (s - fo) / (1.0 - fo)
        lip = (0.05 if torn else 0.025) * math.sin(math.pi * min(t / 0.6, 1.0))
        drop = (0.16 if torn else 0.05) * max(0.0, t - 0.6) / 0.4
        return lip - drop + (0.03 * _n(x * 9, y * 9) if torn else 0.0)

    grid = []
    for s in rings:
        row = []
        for k in range(1 if s == 0.0 else n):
            a = 2 * math.pi * k / n
            r = s * outer(a) * (1.0 + (0.03 * _n(a * 4, s * 5) if torn and bone < s < 0.9 else 0.0))
            x, y = r * math.cos(a), r * math.sin(a)
            row.append(bm.verts.new((x, y, height(s, a, x, y))))
        grid.append(row)

    def key(s, a):
        if s <= mar:
            return 4
        if s <= bone + 0.004:
            return 3
        if s < fat_in(a):
            return 2
        if s < fat_out(a):
            return 1
        return 0

    for k in range(n):
        a = 2 * math.pi * (k + 0.5) / n
        bm.faces.new((grid[0][0], grid[1][k], grid[1][(k + 1) % n])).material_index = key(0.02, a)
    for i in range(1, len(rings) - 1):
        sm = 0.5 * (rings[i] + rings[i + 1])
        for k in range(n):
            a = 2 * math.pi * (k + 0.5) / n
            k1 = (k + 1) % n
            f = bm.faces.new((grid[i][k], grid[i + 1][k], grid[i + 1][k1], grid[i][k1]))
            f.material_index = key(sm, a)
    return finish(root, name, bm, ["skin", "fat", "meat", "bone", "marrow"], "flesh", at)


def torn_edge(root, at):
    """Ring of unit radius around +Z: a low band of skin (meat inside) from which hang
    tatters of skin and of cloth of uneven width and length (0.08..0.5), curling outwards
    and twisting, with ragged tips. Materials: skin 0, meat 1, cloth 2."""
    bm = bmesh.new()
    rs = random.Random(265)
    n, base = 48, 0.06
    o0, o1, i0, i1 = [], [], [], []
    for k in range(n):
        a = 2 * math.pi * k / n
        h = base + 0.02 * _n(a * 3, 0.5)
        o0.append(bm.verts.new((math.cos(a), math.sin(a), 0.0)))
        o1.append(bm.verts.new((math.cos(a), math.sin(a), h)))
        i0.append(bm.verts.new((0.95 * math.cos(a), 0.95 * math.sin(a), 0.0)))
        i1.append(bm.verts.new((0.95 * math.cos(a), 0.95 * math.sin(a), h)))
    for k in range(n):
        k1 = (k + 1) % n
        bm.faces.new((o0[k], o0[k1], o1[k1], o1[k])).material_index = 0
        bm.faces.new((i0[k], i1[k], i1[k1], i0[k1])).material_index = 1
        bm.faces.new((o1[k], o1[k1], i1[k1], i1[k])).material_index = 1
    a = rs.uniform(0, 0.3)
    while a < 2 * math.pi - 0.2:
        w = rs.uniform(0.18, 0.55)
        if rs.random() < 0.2:  # a gap
            a += w * 0.6
            continue
        length = rs.choice((rs.uniform(0.08, 0.2), rs.uniform(0.15, 0.35), rs.uniform(0.3, 0.5)))
        cloth = rs.random() < 0.35
        curl = rs.uniform(0.2, 0.9)
        twist = rs.uniform(-0.5, 0.5)
        cols, rows = 3, 6
        tips = [rs.uniform(0.6, 1.0) for _ in range(cols + 1)]
        th = 0.012 if cloth else 0.025
        outer_v, inner_v = [], []
        for j in range(rows + 1):
            v = j / rows
            orow, irow = [], []
            for c in range(cols + 1):
                u = c / cols - 0.5
                vv = v * (tips[c] if j == rows else 1.0)
                taper = 1.0 - 0.55 * vv
                ang = a + w / 2 + u * w * taper + twist * vv * 0.2
                lz = vv * length
                r = 1.0 + curl * lz * vv + 0.02 * _n(ang * 7, lz * 9)
                z = base * 0.6 + lz * (1.0 - 0.4 * curl * vv)
                orow.append(bm.verts.new((r * math.cos(ang), r * math.sin(ang), z)))
                irow.append(bm.verts.new(((r - th) * math.cos(ang), (r - th) * math.sin(ang), z)))
            outer_v.append(orow)
            inner_v.append(irow)
        for j in range(rows):
            for c in range(cols):
                q = (outer_v[j][c], outer_v[j][c + 1], outer_v[j + 1][c + 1], outer_v[j + 1][c])
                bm.faces.new(q).material_index = 2 if cloth else 0
                q = (inner_v[j][c], inner_v[j + 1][c], inner_v[j + 1][c + 1], inner_v[j][c + 1])
                bm.faces.new(q).material_index = 2 if cloth else 1
        for c in range(cols):  # the torn tip edge
            q = (outer_v[rows][c], outer_v[rows][c + 1], inner_v[rows][c + 1], inner_v[rows][c])
            bm.faces.new(q).material_index = 2 if cloth else 1
        a += w * rs.uniform(0.75, 1.0)
    return finish(root, "torn_edge", bm, ["skin", "meat", "cloth"], "flesh", at)
