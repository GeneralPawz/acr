"""A generic ruin for a building model (#303; `bpy` only inside `add`).

`add(ctx, obj)` moves the intact mesh `obj` into a `<root>.finished` group (`acr_state`
STANDLO) and builds the `<root>.ruin` state group (`acr_state` DEATHLIE1/DEATHLIE2, the
group the renderer draws once a building has fallen in, `ruin_view.rs`) from the intact
mesh `obj` (ground at z = 0, metres, +Z up):

- **Walls:** a copy of the mesh, sliced into a grid (horizontal cuts through the standing
  band, vertical cuts every `CELL` m), cut by a ragged height field (`height_field`: 20-50 %
  of the standing height, smooth value noise plus a jagged term per cell): faces above it go,
  vertices left above it drop to it less a random fracture depth. The roof goes with it.
- **Char:** the materials are copied per height band and multiplied darker towards the top
  (`SOOT`: soot rising up the walls; same images, so no extra texture bytes).
- **Rubble:** a scorched ground patch, a heap of jittered blocks (ash, cinder, charcoal: flat
  materials) centred on the footprint with some fallen
  outside it, and charred beams leaning from the heap (`rubble_layout`, pure Python).
- Budget: the whole ruin stays within `share` (40 %) of the intact triangles; the walls are
  decimated to what the rubble leaves.

Stats `ruin`: `triangles`, `share`, `walls`, `rubble`, `height_m` (min, max of the cut),
`chunks`, `beams`. Randomness is seeded from the root name.
"""

import math
import random
import zlib

CELL = 0.5           # vertical cut spacing (m)
BAND = 0.25          # horizontal cut spacing through the standing band (m)
LOW, HIGH = 0.2, 0.5  # share of the standing height the walls keep
SOOT = (0.55, 0.38, 0.24, 0.14)  # colour factor per band, ground up to the broken top
CHAR = (0.035, 0.028, 0.022)
ASH = (0.20, 0.19, 0.18)
EMBER_ASH = (0.11, 0.095, 0.085)


def seed_of(name):
    return zlib.crc32(name.encode()) & 0xFFFFFFFF


def _lattice(seed, i, j):
    h = zlib.crc32(f"{seed}:{i}:{j}".encode()) & 0xFFFFFF
    return h / float(0xFFFFFF)


def _smooth(seed, x, y, scale):
    u, v = x / scale, y / scale
    i, j = math.floor(u), math.floor(v)
    fu, fv = u - i, v - j
    fu, fv = fu * fu * (3 - 2 * fu), fv * fv * (3 - 2 * fv)
    a, b = _lattice(seed, i, j), _lattice(seed, i + 1, j)
    c, d = _lattice(seed, i, j + 1), _lattice(seed, i + 1, j + 1)
    return (a * (1 - fu) + b * fu) * (1 - fv) + (c * (1 - fu) + d * fu) * fv


def height_field(seed, top, low=LOW, high=HIGH):
    """h(x, y): the broken top of the walls (m), in [low, high] * top: a smooth noise over
    about 3 m with a jagged term per 0.5 m cell."""
    def h(x, y):
        n = 0.7 * _smooth(seed, x, y, 3.0) + 0.3 * _lattice(seed + 7, math.floor(x / CELL), math.floor(y / CELL))
        return top * (low + (high - low) * n)
    return h


def rubble_layout(seed, half, top, chunks, beams):
    """Blocks and beams of the rubble heap over a footprint of half extents `half` (x, y)
    around the origin: [(centre, size, euler)] for each, metres."""
    rnd = random.Random(seed)
    hx, hy = half
    heap = min(0.12 * top + 0.4, 0.45 * min(hx, hy) + 0.3)
    s = max(0.18, min(0.7, 0.05 * top + 0.12))
    out = []
    for k in range(chunks):
        outside = k % 5 == 0
        if outside:  # fallen out past the walls
            a = rnd.uniform(0, math.tau)
            r = rnd.uniform(0.95, 1.3)
            x, y = hx * r * math.cos(a), hy * r * math.sin(a)
            z0 = 0.0
        else:
            x = max(-0.9, min(0.9, rnd.gauss(0, 0.4))) * hx
            y = max(-0.9, min(0.9, rnd.gauss(0, 0.4))) * hy
            rr = (x / hx) ** 2 + (y / hy) ** 2
            z0 = heap * max(0.0, 1.0 - rr) * rnd.uniform(0.3, 1.0)
        size = (s * rnd.uniform(0.5, 1.4), s * rnd.uniform(0.4, 1.1), s * rnd.uniform(0.25, 0.8))
        rot = (rnd.uniform(-0.6, 0.6), rnd.uniform(-0.6, 0.6), rnd.uniform(0, math.tau))
        out.append(("chunk", (x, y, z0 + size[2] * 0.4), size, rot))
    for _ in range(beams):
        a = rnd.uniform(0, math.tau)
        length = rnd.uniform(0.5, 1.0) * min(hx, hy) * 1.4 + 0.8
        thick = max(0.12, min(0.3, 0.02 * top + 0.1))
        r = rnd.uniform(0.0, 0.7)
        x, y = hx * r * math.cos(a), hy * r * math.sin(a)
        pitch = rnd.uniform(0.1, 0.55)
        z = math.sin(pitch) * length * 0.5 + thick * 0.5
        out.append(("beam", (x, y, z), (length, thick, thick), (rnd.uniform(-0.2, 0.2), pitch, a)))
    return out


def _flat(name, rgb):
    import bpy
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes.get("Principled BSDF")
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = 0.95
    return m


def _walls(ctx, obj, group, seed, top, target):
    import bmesh
    import bpy
    from . import vessels as vs

    me = obj.data.copy()
    walls = bpy.data.objects.new(f"{group.name}.walls", me)
    bpy.context.scene.collection.objects.link(walls)
    walls.parent = group
    h = height_field(seed, top)
    rnd = random.Random(seed + 1)

    bm = bmesh.new()
    bm.from_mesh(me)
    # Drop everything above the highest cut first, then slice the band into a grid.
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=(0, 0, HIGH * top + 0.05),
                           plane_no=(0, 0, 1), clear_outer=True)
    xs = [v.co.x for v in bm.verts]
    ys = [v.co.y for v in bm.verts]
    planes = []
    z = LOW * top
    while z < HIGH * top:
        planes.append(((0, 0, z), (0, 0, 1)))
        z += BAND
    for axis, lo, hi in ((0, min(xs), max(xs)), (1, min(ys), max(ys))):
        n = min(40, int((hi - lo) / CELL))
        for k in range(1, n):
            c = [0.0, 0.0, 0.0]
            c[axis] = lo + (hi - lo) * k / n
            nrm = [0.0, 0.0, 0.0]
            nrm[axis] = 1.0
            planes.append((tuple(c), tuple(nrm)))
    for co, no in planes:
        bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=co, plane_no=no)
    gone = []
    for f in bm.faces:
        c = f.calc_center_median()
        if c.z > h(c.x, c.y):
            gone.append(f)
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    for v in sorted(bm.verts, key=lambda v: (round(v.co.x, 4), round(v.co.y, 4), round(v.co.z, 4))):
        cap = h(v.co.x, v.co.y)
        if v.co.z > cap:
            v.co.z = cap - rnd.uniform(0.0, 0.25) * BAND * 2
    # Soot bands: faces take a darker copy of their material the higher they stand.
    mats = list(me.materials)
    slots = {}
    for f in bm.faces:
        c = f.calc_center_median()
        band = min(len(SOOT) - 1, int(len(SOOT) * max(0.0, c.z) / (HIGH * top)))
        key = (f.material_index, band)
        if key not in slots:
            slots[key] = len(slots)
        f.material_index = slots[key]
    bm.to_mesh(me)
    bm.free()
    index = [p.material_index for p in me.polygons]
    me.materials.clear()
    for (mi, band), _ in sorted(slots.items(), key=lambda kv: kv[1]):
        src = mats[mi] if mi < len(mats) else None
        if src is None:
            me.materials.append(_flat("ruin.char", CHAR))
            continue
        m = src.copy()
        m.name = f"{src.name}.soot{band}"
        k = SOOT[band]
        bsdf = next((n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None) if m.use_nodes else None
        if bsdf is not None:
            vs._tint(m.node_tree, bsdf, (k * 0.95, k * 0.9, k * 0.85))
        me.materials.append(m)
    # materials.clear() drops material_index (pitfall 1): put it back.
    _restore_index(me, index)
    walls_tris = vs.decimate(walls, target)
    return walls, walls_tris


def _restore_index(me, index):
    me.polygons.foreach_set("material_index", index)
    me.update()


def _box(bm, centre, size, rot, jitter, rnd, mat):
    from mathutils import Euler, Matrix, Vector
    m = Matrix.Translation(Vector(centre)) @ Euler(rot, "XYZ").to_matrix().to_4x4() @ Matrix.Diagonal(
        (size[0], size[1], size[2], 1.0))
    res = __import__("bmesh").ops.create_cube(bm, size=1.0, matrix=m)
    for v in res["verts"]:
        v.co += Vector((rnd.uniform(-jitter, jitter), rnd.uniform(-jitter, jitter), rnd.uniform(-jitter, jitter)))
    faces = {f for v in res["verts"] for f in v.link_faces}
    for f in faces:
        f.material_index = mat


def _patch(bm, half, seed):
    """A scorched patch a little wider than the footprint (a jittered 16-gon fan)."""
    from mathutils import Vector
    rnd = random.Random(seed + 3)
    centre = bm.verts.new(Vector((0, 0, 0.03)))
    ring = []
    for k in range(16):
        a = math.tau * k / 16
        r = rnd.uniform(1.05, 1.3)
        ring.append(bm.verts.new(Vector((half[0] * r * math.cos(a), half[1] * r * math.sin(a), 0.03))))
    for k in range(16):
        bm.faces.new((centre, ring[k], ring[(k + 1) % 16])).material_index = 0


def add(ctx, obj, share=0.4):
    """Builds the ruin group of the building mesh `obj` under `ctx.root`."""
    import bmesh
    import bpy
    import numpy as np
    from . import kit
    from . import vessels as vs

    intact = vs.triangles([obj])
    me = obj.data
    v = np.empty(len(me.vertices) * 3)
    me.vertices.foreach_get("co", v)
    v = v.reshape(-1, 3)
    lo, hi = v.min(axis=0), v.max(axis=0)
    top = float(hi[2])
    centre = ((lo + hi) / 2)[:2]
    half = (float(hi[0] - lo[0]) / 2 * 0.92, float(hi[1] - lo[1]) / 2 * 0.92)
    seed = seed_of(ctx.root.name)
    # The intact mesh goes into a `finished` group: the renderer draws ungrouped meshes in
    # every state, the ruin included.
    finished = kit.group(ctx.root, "finished", acr_state="STANDLO")
    obj.parent = finished
    group = kit.group(ctx.root, "ruin", acr_state=["DEATHLIE1", "DEATHLIE2"])
    budget = int(share * intact)

    # Rubble first: it takes a fixed share, the walls get the rest.
    chunks = max(20, min(140, int(budget * 0.22 / 12)))
    beams = max(6, min(24, chunks // 5))
    rnd = random.Random(seed + 2)
    rme = bpy.data.meshes.new(f"{group.name}.rubble")
    bm = bmesh.new()
    _patch(bm, half, seed)
    s = max(0.18, min(0.7, 0.05 * top + 0.12))
    for kind, c, size, rot in rubble_layout(seed, half, top, chunks, beams):
        c = (c[0] + centre[0], c[1] + centre[1], c[2])
        if kind == "beam":
            _box(bm, c, size, rot, 0.02, rnd, 3)
        else:
            _box(bm, c, size, rot, 0.18 * s, rnd, rnd.choice((1, 1, 2, 3)))
    bm.to_mesh(rme)
    bm.free()
    index = [p.material_index for p in rme.polygons]
    for name, rgb in (("ruin.ash_ground", EMBER_ASH), ("ruin.ash", ASH), ("ruin.cinder", (0.07, 0.06, 0.055)),
                      ("ruin.char", CHAR)):
        rme.materials.append(_flat(name, rgb))
    _restore_index(rme, index)
    rubble = bpy.data.objects.new(f"{group.name}.rubble", rme)
    bpy.context.scene.collection.objects.link(rubble)
    rubble.parent = group
    rubble_tris = vs.triangles([rubble])

    walls, _ = _walls(ctx, obj, group, seed, top, max(200, budget - rubble_tris))
    walls_tris = vs.triangles([walls])
    total = walls_tris + rubble_tris
    hz = [p.co.z for p in walls.data.vertices]
    ctx.stats["ruin"] = {
        "triangles": total, "share": round(total / max(1, intact), 3), "walls": walls_tris,
        "rubble": rubble_tris, "height_m": [round(LOW * top, 2), round(max(hz) if hz else 0.0, 2)],
        "chunks": chunks, "beams": beams,
    }
    return group
