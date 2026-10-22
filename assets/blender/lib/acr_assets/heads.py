"""Individual faces (#283, needs `bpy`, `bmesh`, `numpy`): a finer head on the skin hull
with a modelled base face and its own painted 512² face map, the `face_*` and `body_*` morph
targets, the `_FACE` zones and the hair and beard variants as separate smooth meshes. The
fields, curves and odds are pure Python in `faces.py`. See docs/assets-humans.md "Faces and
bodies".
"""

import math

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

from . import armature as am
from . import body as bd
from . import coat, decay, faces

HAIR_BASE = (0.80, 0.78, 0.75)  # neutral: the renderer multiplies the chosen hair colour in
FACE_SIZE = 512  # the head's own texture
STRAND_SIZE = 256


class Frame:
    """Head-local coordinates (`faces`): x from the head's centre line, y from the face
    front at eye level, z from eye level."""

    def __init__(self, cx, y_front, eye_z):
        self.cx, self.y_front, self.eye_z = cx, y_front, eye_z

    def local(self, co):
        return co[0] - self.cx, co[1] - self.y_front, co[2] - self.eye_z

    def as_list(self):
        return [self.cx, self.y_front, self.eye_z]


def frame_of(cos, cx):
    """The frame of a skin hull's head from its vertex positions (before the base face)."""
    top = max(c[2] for c in cos)
    ez = top - faces.EYE_BELOW_TOP
    near = [c[1] for c in cos if abs(c[0] - cx) < 0.03 and abs(c[2] - ez) < 0.03]
    return Frame(cx, min(near), ez)


def refine(bm, cx):
    """Hull hook (`body.hull(head=...)`): subdivides the head (twice on the face, once on
    the rest; smooth) and models the base face. Returns the head frame."""
    fr = frame_of([tuple(v.co) for v in bm.verts], cx)

    def level(f):
        x, y, z = fr.local(f.calc_center_median())
        if z < -0.14:
            return 0
        return 2 if (y < 0.05 and -0.115 < z < 0.05 and abs(x) < 0.07) else 1

    for lv in (1, 2):
        edges = {e for f in bm.faces if level(f) >= lv for e in f.edges}
        bmesh.ops.subdivide_edges(bm, edges=list(edges), cuts=1, use_grid_fill=True, smooth=1.0)
    for v in bm.verts:
        x, y, z = fr.local(v.co)
        if z < -0.2:
            continue
        d = faces.base_face(x, y, z)
        e = faces.asym(x, y, z)
        v.co.x += d[0] + e[0]
        v.co.y += d[1] + e[1]
        v.co.z += d[2] + e[2]
    return fr


def face_morphs(objs, fr, names=faces.FACE_MORPHS):
    """Shape keys `face_*` on every mesh in `objs` that the head fields move (rest
    positions in head-local coordinates; keys with no motion are left out)."""
    for o in objs:
        basis = [tuple(v.co) for v in o.data.vertices]
        for n in names:
            ds = [faces.morph(n, *fr.local(c)) for c in basis]
            if max((max(map(abs, d)) for d in ds), default=0.0) < 1e-5:
                continue
            key = bd._key(o, n)
            key.slider_min = names[n][0]
            for i, (c, d) in enumerate(zip(basis, ds)):
                key.data[i].co = (c[0] + d[0], c[1] + d[1], c[2] + d[2])


def body_morphs(ch, objs):
    """`body_stocky` / `body_thin` on `objs` (flesh and clothes): away from / towards the
    bones; the head and hands keep their size (the face has its own morphs)."""
    from . import rig

    bones = [ch.prefix + b for b in rig.BODY_BONES]
    fixed = {ch.prefix + b: 1.0 for b in ("head", "hand_l", "hand_r", "foot_l", "foot_r", "toe_l", "toe_r")}
    for o in objs:
        bd.toward_bones(o, ch.arm, "body_stocky", faces.STOCKY_KEEP, bones, fixed)
        bd.toward_bones(o, ch.arm, "body_thin", faces.THIN_KEEP, bones, fixed)


def zones(obj, fr):
    """The `_FACE` attribute (glTF `_FACE`, vec4; `faces.zones`)."""
    me = obj.data
    a = me.attributes.get("_face") or me.attributes.new("_face", "FLOAT_COLOR", "POINT")
    for d, v in zip(a.data, me.vertices):
        d.color = faces.zones(*fr.local(v.co))


# --------------------------------------------------------------------------- hair, beards

def _bind_rigid(ch, obj):
    g = obj.vertex_groups.new(name=ch.prefix + "head")
    g.add(list(range(len(obj.data.vertices))), 1.0, "REPLACE")
    attr = obj.data.attributes.new("part", "INT", "POINT")
    for d in attr.data:
        d.value = 1
    return am.bind(obj, ch.arm)


def _mesh(ch, name, verts, polys, uvs, mats=None):
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], polys)
    uv = me.uv_layers.new(name="strands")
    for poly in me.polygons:
        poly.use_smooth = True
        if mats:
            poly.material_index = mats[poly.index]
        for li, vi in zip(poly.loop_indices, poly.vertices):
            uv.data[li].uv = uvs[vi]
    obj = bd.link(bpy.data.objects.new(name, me))
    return _bind_rigid(ch, obj)


def _outward(obj, centre):
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    for f in bm.faces:
        if f.normal.dot(f.calc_center_median() - centre) < 0:
            f.normal_flip()
    bm.to_mesh(me)
    bm.free()


def _centre(fr):
    return Vector((fr.cx + faces.CENTRE[0], fr.y_front + faces.CENTRE[1], fr.eye_z + faces.CENTRE[2]))


def _patch(ch, fr, tree, name):
    """One hair or beard patch (`faces.PATCHES`): a grid of rays from the head centre to
    the skin, lifted by the tapered thickness, with strand UVs (u across, v from the open
    edge); a full ring is closed at the crown with a fan."""
    pt = faces.PATCHES[name]
    c = _centre(fr)
    wrap = pt["theta"][1] - pt["theta"][0] >= 360
    verts, polys, uvs = [], [], []

    def lift(th, ph, s, tp):
        d = Vector(faces.ray_dir(th, ph))
        hit, nrm, _i, _dist = tree.ray_cast(c, d)
        if hit is None:
            hit, nrm = c + d * 0.09, d
        if nrm.dot(d) < 0:
            nrm = -nrm
        return tuple(hit + nrm * faces.thickness(name, th, s, tp))

    for side in ((1, -1) if pt.get("mirror") else (1,)):
        rows = faces.patch_grid(name)
        base = len(verts)
        ns = len(rows) - 1
        cols = len(rows[0])
        for j, row in enumerate(rows):
            for i, (th, ph, tp) in enumerate(row):
                verts.append(lift(th * side, ph, j / ns, tp))
                uvs.append((i / max(cols - 1, 1) * (6.0 if wrap else 3.0), j / ns))
        span = cols if wrap else cols - 1
        for j in range(ns):
            for i in range(span):
                a, b = base + j * cols + i, base + j * cols + (i + 1) % cols
                polys.append((a, b, b + cols, a + cols))
        if wrap:
            top = len(verts)
            verts.append(lift(0.0, 90.0, 1.0, 1.0))
            uvs.append((0.5, 1.0))
            for i in range(cols):
                a, b = base + ns * cols + i, base + ns * cols + (i + 1) % cols
                polys.append((a, b, top))
    obj = _mesh(ch, f"{ch.name}.{name}", verts, polys, uvs)
    _outward(obj, c)
    return obj


def _queue(ch, fr, tree, name):
    """The pigtail: a tapered, slightly flattened tube down the nape, tied with a black
    ribbon near the top (material 1); rigid on the head bone."""
    seg = 8
    c = Vector((fr.cx, fr.y_front + faces.CENTRE[1], fr.eye_z - 0.045))
    hit = tree.ray_cast(c, Vector((0, 1, 0)))[0]
    back = (hit.y - fr.y_front) if hit else 0.18
    path = faces.queue_path(back)
    verts, polys, uvs, mats = [], [], [], []
    for j, (y, z, r) in enumerate(path):
        for k in range(seg):
            a = 2 * math.pi * k / seg
            verts.append((fr.cx + math.cos(a) * r, fr.y_front + y + math.sin(a) * r * 0.75, fr.eye_z + z))
            uvs.append((k / seg * 2.0, j / (len(path) - 1)))
    for i in range(len(path) - 1):
        for k in range(seg):
            k2 = (k + 1) % seg
            polys.append((i * seg + k, i * seg + k2, (i + 1) * seg + k2, (i + 1) * seg + k))
            mats.append(1 if i == 0 else 0)
    return _mesh(ch, name, verts, polys, uvs, mats)


def variants(ch):
    """The hair and beard variants (`faces.PATCHES` and the queue) as smooth patches over
    the head skin. Call before `Character.morphs`. Returns {suffix: object}."""
    fr = ch.face_frame
    me = ch.flesh.data
    tree = BVHTree.FromPolygons([v.co.copy() for v in me.vertices], [p.vertices[:] for p in me.polygons])
    out = {n: _patch(ch, fr, tree, n) for n in faces.PATCHES}
    out["hair_queue"] = _queue(ch, fr, tree, f"{ch.name}.hair_queue")
    ch.variants = out
    return out


def _image(name, col, size):
    px = np.ones((size * size, 4), np.float32)
    px[:, :3] = coat.to_srgb(np.clip(col, 0.0, 1.0))
    img = bpy.data.images.new(name, size, size, alpha=False)
    img.pixels.foreach_set(px.ravel())
    img.file_format = "PNG"
    img.pack()
    return img


def strand_image(name, seed=29):
    """Hair texture: fine streaks along v (the strand direction), a few light strands,
    darker towards the roots (v = 0), in the neutral `HAIR_BASE` for the tint."""
    n = STRAND_SIZE
    v, u = np.mgrid[0:n, 0:n]
    p = np.stack([u / n, v / n, np.zeros(u.shape)], -1).reshape(-1, 3).astype(np.float32)
    fine = coat.fbm(p, (220.0, 5.0, 1.0), seed, octaves=2)
    broad = coat.fbm(p, (40.0, 2.0, 1.0), seed + 3)
    light = coat.smooth(0.70, 0.80, coat.fbm(p, (90.0, 3.0, 1.0), seed + 5))
    val = 0.62 + 0.45 * (fine - 0.5) + 0.30 * (broad - 0.5) + 0.12 * light
    root = 0.62 + 0.38 * coat.smooth(0.0, 0.35, p[:, 1])
    return _image(name, coat.lin(HAIR_BASE) * (val * root)[:, None], n)


def finish_variants(ch, deflate=0.65):
    """Morphs, masks, `_LIMB`, materials of the variants (after `Character.morphs`)."""
    from . import limb, rig

    bones = [ch.prefix + b for b in rig.BODY_BONES]
    objs = list(ch.variants.values())
    for o in objs:
        bd.toward_bones(o, ch.arm, "deflate", deflate, bones)
    face_morphs(objs, ch.face_frame)
    img = strand_image(f"{ch.name}.hair_strands")
    hair = decay.material(f"{ch.name}.hair_variant", "hair", HAIR_BASE, image=img, uv_map="strands",
                          interpolation="Linear")
    hair["acr_tint"] = "hair"
    hair["acr_roughness"] = 0.92  # matte: unwashed hair
    ribbon = decay.material(f"{ch.name}.hair_ribbon", "hair", (0.04, 0.04, 0.05))
    for o in objs:
        decay.add_masks(o, "hair", seed=sum(map(ord, o.name)), part_of_vertex=lambda i: 1)
        a = o.data.attributes.get("_limb") or o.data.attributes.new("_limb", "FLOAT", "POINT")
        for d in a.data:
            d.value = limb.TRUNK
        o.data.materials.clear()
        o.data.materials.append(hair)
        if o.name.endswith("hair_queue"):
            o.data.materials.append(ribbon)
    return objs


# --------------------------------------------------------------------------- face map

def face_uv(fr, co):
    """Cylindrical face-map UV of a rest position: the face front (u = 0.5) gets the
    most texels (u = 0.5 + 0.5 sin(θ/2)), v from below the chin to the crown."""
    x, y, z = fr.local(co)
    th = math.atan2(x, -(y - faces.CENTRE[1]))
    return 0.5 + 0.5 * math.sin(th / 2.0), (z + 0.17) / 0.30


def face_map(ch, skin_rgb):
    """The head's own material and 512² texture: the head part of the skin gets its own
    material slot and, in the `atlas` UV map, the cylindrical `face_uv`; Cycles bakes
    position, normal and occlusion of a copy of the head, numpy paints it (`paint_face`)."""
    fr = ch.face_frame
    flesh = ch.flesh
    me = flesh.data
    # The texture layer of the skin: `atlas` when dressed by `outfit`, else the detail map
    # (the dragoon rider).
    uvname = "atlas" if "atlas" in me.uv_layers else me.uv_layers[0].name
    parts = [d.value for d in me.attributes["part"].data]
    uv = me.uv_layers[uvname]
    head = [p.index for p in me.polygons if all(parts[v] == 1 for v in p.vertices)]
    for pi in head:
        p = me.polygons[pi]
        us = [face_uv(fr, me.vertices[v].co) for v in p.vertices]
        if max(u for u, _ in us) - min(u for u, _ in us) > 0.5:  # across the seam at the back
            us = [(u + 1.0 if u < 0.5 else u, v) for u, v in us]
        for li, t in zip(p.loop_indices, us):
            uv.data[li].uv = t
    # Bake on a copy holding only the head.
    tmp = bpy.data.objects.new(f"{ch.name}.face_bake", me.copy())
    tmp.matrix_world = flesh.matrix_world
    bd.link(tmp)
    if tmp.data.shape_keys:
        tmp.shape_key_clear()
    bm = bmesh.new()
    bm.from_mesh(tmp.data)
    keep = set(head)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.index not in keep], context="FACES")
    bm.to_mesh(tmp.data)
    bm.free()
    tmp.data.uv_layers.active = tmp.data.uv_layers[uvname]
    size = coat.SIZE
    coat.SIZE = FACE_SIZE
    try:
        pos, nrm, ao, covered = coat.bake([tmp], FACE_SIZE)
    finally:
        coat.SIZE = size
    data = tmp.data
    bpy.data.objects.remove(tmp)
    bpy.data.meshes.remove(data)
    flat_p, flat_n = pos.reshape(-1, 3), nrm.reshape(-1, 3)
    loc = np.stack([flat_p[:, 0] - fr.cx, flat_p[:, 1] - fr.y_front, flat_p[:, 2] - fr.eye_z], -1).astype(np.float32)
    cov = covered.reshape(-1)
    col = np.tile(coat.lin(skin_rgb) * 0.85, (len(loc), 1)).astype(np.float32)
    col[cov] = paint_face(loc[cov], flat_n[cov], skin_rgb)
    occ = np.clip(ao.reshape(-1), 0.0, 1.0)
    col[cov] *= (0.5 + 0.5 * occ[cov] ** 0.9)[:, None]
    img = _image(f"{ch.name}.face", col, FACE_SIZE)
    m = decay.material(f"{ch.name}.face", "flesh", skin_rgb, image=img, uv_map=uvname, interpolation="Linear")
    m["acr_colour_from"] = "outfit"
    m["acr_tint"] = "skin"
    me.materials.append(m)
    slot = len(me.materials) - 1
    for pi in head:
        me.polygons[pi].material_index = slot
    return img


def _g(*pairs):
    e = 0.0
    for v, c, s in pairs:
        e = e + ((v - c) / s) ** 2
    return np.exp(-e)


def paint_face(p, n, rgb, seed=41):
    """Colours of head texels from head-local positions `p` (N, 3) and normals: weathered
    skin, eyes with whites, iris, lids and lashes, brows, nostrils, lips and the mouth line,
    nasolabial folds, a cropped scalp along the hairline, a faint beard shadow."""
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    ax = np.abs(x)
    base = coat.lin(rgb)
    fine = coat.fbm(p, 300.0, seed, octaves=1)
    col = base * (0.86 + 0.12 * coat.fbm(p, 30.0, seed + 1) + 0.06 * fine)[:, None]
    # Weathering: red blotches, sun-darkened front and top, a few moles.
    blot = coat.smooth(0.58, 0.78, coat.fbm(p, 16.0, seed + 2)) * 0.35
    col = coat.mix(col, base * np.array([1.06, 0.78, 0.70], np.float32), blot)
    front = coat.smooth(0.045, 0.01, y)
    sun = np.clip(-n[:, 1], 0, 1) * front * 0.12 + np.clip(n[:, 2], 0, 1) * 0.08
    col *= (1.0 - sun)[:, None]
    mole = coat.smooth(0.90, 0.92, coat.fbm(p, 90.0, seed + 3, octaves=1)) * 0.5
    col = coat.mix(col, base * np.array([0.45, 0.32, 0.25], np.float32), mole)
    # Campaign skin: uneven tone, sun spots, grime in the pores and the creases.
    tone = coat.fbm(p, 7.0, seed + 6)
    col *= (0.92 + 0.16 * tone)[:, None] * np.array([1.0, 0.97, 0.94], np.float32)
    exposed = np.clip(front + np.clip(n[:, 2], 0, 1), 0, 1)
    spots = coat.smooth(0.70, 0.78, coat.fbm(p, 55.0, seed + 7)) * exposed
    col = coat.mix(col, base * np.array([0.68, 0.52, 0.40], np.float32), spots * 0.45)
    pores = coat.smooth(0.60, 0.72, coat.fbm(p, 900.0, seed + 8, octaves=1))
    col *= (1.0 - 0.22 * pores)[:, None]
    dirt = coat.smooth(0.45, 0.75, coat.fbm(p, 9.0, seed + 9)) * 0.30
    col = coat.mix(col, coat.lin((0.30, 0.24, 0.17)), dirt)
    cheek = (_g((ax, 0.046, 0.016), (z, -0.030, 0.016)) + _g((x, 0.0, 0.010), (z, -0.030, 0.012))) * front
    col = coat.mix(col, base * np.array([1.10, 0.74, 0.68], np.float32), np.clip(cheek, 0, 1) * 0.45)
    under = _g((ax, 0.033, 0.013), (z, -0.011, 0.005)) * front
    col = coat.mix(col, base * np.array([0.72, 0.64, 0.66], np.float32), under * 0.45)
    # Eyes: almond opening, dirty white, iris, pupil, upper lid shadow, lashes, crease.
    eu = (ax - 0.033) / 0.0135
    ev = (z - 0.0015) / 0.0052
    rim = np.clip(1.0 - eu ** 2, 0.0, 1.0)
    edge = np.abs(ev) - rim
    inside = (np.abs(eu) < 1.0).astype(np.float32)
    opening = coat.smooth(0.12, -0.12, edge) * inside * front
    col = coat.mix(col, coat.lin((0.74, 0.70, 0.64)), opening)
    ir = np.hypot(ax - 0.0325, z - 0.001)
    col = coat.mix(col, coat.lin((0.20, 0.15, 0.10)), opening * coat.smooth(0.0056, 0.0046, ir))
    col = coat.mix(col, coat.lin((0.03, 0.025, 0.02)), opening * coat.smooth(0.0022, 0.0016, ir))
    col = coat.mix(col, col * 0.55, opening * coat.smooth(0.2, 0.75, ev))
    lash = coat.smooth(0.30, 0.0, np.abs(edge)) * (ev > 0) * (np.abs(eu) < 1.1) * front
    col = coat.mix(col, coat.lin((0.05, 0.035, 0.03)), lash * 0.9)
    low = coat.smooth(0.25, 0.0, np.abs(edge)) * (ev < 0) * inside * front
    col = coat.mix(col, base * 0.6, low * 0.45)
    crease = coat.smooth(0.35, 0.0, np.abs(ev - 1.9 * rim - 0.3)) * (np.abs(eu) < 1.1) * front
    col = coat.mix(col, base * 0.62, crease * 0.5)
    # Brows: an arc of short hairs, thinning outwards.
    bz = 0.019 + 0.004 * (1.0 - ((ax - 0.036) / 0.022) ** 2)
    hp = np.stack([x + z * 0.6, z, y], -1).astype(np.float32)
    hairs = coat.fbm(hp, (500.0, 140.0, 10.0), seed + 4, octaves=1)
    brow = coat.smooth(0.0040, 0.0022, np.abs(z - bz)) * coat.smooth(0.064, 0.050, ax) * coat.smooth(0.004, 0.010, ax) * front
    col = coat.mix(col, coat.lin((0.09, 0.065, 0.045)), brow * (0.55 + 0.4 * coat.smooth(0.4, 0.6, hairs)))
    # Wrinkles: forehead creases, frown lines, crow's feet, the line under the eye bags.
    wav = 0.0025 * np.sin(x * 95.0 + 1.3) + 0.0012 * np.sin(x * 260.0)
    fh = np.zeros_like(x)
    for zk in (0.036, 0.046, 0.057):
        fh = np.maximum(fh, coat.smooth(0.0010, 0.0002, np.abs(z - zk - wav)) * coat.smooth(0.052, 0.035, ax))
    col = coat.mix(col, base * 0.55, fh * front * 0.55)
    frown = coat.smooth(0.0009, 0.0002, np.abs(ax - 0.0055 - 0.08 * (z - 0.012))) * coat.smooth(0.0, 0.012, z) * coat.smooth(0.034, 0.024, z)
    col = coat.mix(col, base * 0.55, frown * front * 0.5)
    ca = np.arctan2(z - 0.001, ax - 0.047)
    cr = np.hypot(ax - 0.047, z - 0.001)
    feet = coat.smooth(0.55, 0.9, np.cos(ca * 9.0)) * coat.smooth(0.003, 0.006, cr) * coat.smooth(0.018, 0.010, cr) * (ax > 0.047)
    col = coat.mix(col, base * 0.58, feet * 0.5)
    bag = coat.smooth(0.0011, 0.0003, np.abs(z - (-0.0115 - 0.0035 * np.clip(1 - ((ax - 0.034) / 0.016) ** 2, 0, 1)))) * coat.smooth(0.054, 0.044, ax) * coat.smooth(0.018, 0.026, ax)
    col = coat.mix(col, base * 0.6, bag * front * 0.5)
    # Nostrils.
    nost = _g((ax, 0.0075, 0.0034), (z, -0.0415, 0.0019)) * front
    col = coat.mix(col, coat.lin((0.10, 0.045, 0.035)), np.clip(nost * 1.2, 0, 0.9))
    # Lips and the mouth line.
    mw = 0.024
    lu = np.clip(1.0 - (x / mw) ** 2, 0.0, 1.0)
    zm = -0.0605 + 0.0015 * (x / mw) ** 2
    upper = coat.smooth(0.0007, -0.0007, z - (zm + 0.0055 * lu - 0.0012 * _g((x, 0.0, 0.004)))) * (z > zm)
    lower = coat.smooth(0.0007, -0.0007, (zm - 0.0075 * np.sqrt(lu)) - z) * (z <= zm)
    lips = (upper + lower) * (ax < mw) * front
    col = coat.mix(col, coat.lin((0.50, 0.27, 0.24)) * (0.9 + 0.2 * fine)[:, None], lips * 0.75)
    line = coat.smooth(0.0013, 0.0004, np.abs(z - zm)) * coat.smooth(mw + 0.002, mw - 0.004, ax) * front
    col = coat.mix(col, coat.lin((0.13, 0.05, 0.045)), line * 0.9)
    chinsh = _g((x, 0.0, 0.012), (z, -0.0735, 0.0028)) * front
    col = coat.mix(col, base * 0.7, chinsh * 0.4)
    # Nasolabial folds: a soft crease from the nose wing to the mouth corner.
    t = np.clip(((ax - 0.018) * 0.012 + (z + 0.042) * -0.022) / (0.012 ** 2 + 0.022 ** 2), 0, 1)
    fold = np.hypot(ax - (0.018 + 0.012 * t), z - (-0.042 - 0.022 * t))
    col = coat.mix(col, base * 0.62, coat.smooth(0.0035, 0.0, fold) * front * 0.6)
    # Scalp: cropped hair above the hairline (under every hair variant), a soft edge.
    cy, cz = faces.CENTRE[1], faces.CENTRE[2]
    th = np.degrees(np.arctan2(x, -(y - cy)))
    ph = np.degrees(np.arctan2(z - cz, np.hypot(x, y - cy)))
    line_ph = np.interp(np.abs(th), [k for k, _ in faces.HAIRLINE], [v for _, v in faces.HAIRLINE])
    scalp = coat.smooth(-6.0, -1.0, ph - line_ph)  # dark under the tapered hair edge
    stip = coat.smooth(0.35, 0.65, fine)
    col = coat.mix(col, coat.lin((0.11, 0.08, 0.06)) * (0.8 + 0.4 * stip)[:, None], scalp * (0.6 + 0.3 * stip))
    # A faint beard shadow (the renderer adds the soldier's stubble from `_FACE`).
    beard = np.fromiter((faces.zones(float(a), float(b), float(c))[0] for a, b, c in p), np.float32, len(p))
    # Unshaven: days of dark stubble in the beard zone (the renderer adds more per soldier).
    speck = coat.smooth(0.48, 0.62, coat.fbm(p, 1400.0, seed + 10, octaves=1))
    col = coat.mix(col, coat.lin((0.07, 0.06, 0.055)), beard * (0.22 + 0.38 * speck))
    return col


def finish(ch, objs=None, skin=None):
    """Face morphs on `objs` (default: flesh and clothes; a rider passes his own, not the
    horse's), `_FACE` on the `skin` meshes (default: `ch.flesh`), the head's face map,
    body morphs, the variants' finishing; skin materials get `acr_tint` "skin". Call after
    `Character.morphs` and `outfit.dress`."""
    fr = ch.face_frame
    objs = objs if objs is not None else ch.layers["flesh"] + ch.layers["clothes"]
    face_morphs(objs, fr)
    body_morphs(ch, objs)
    for o in skin or [ch.flesh]:
        zones(o, fr)
        for m in o.data.materials:
            if m is not None:
                m["acr_tint"] = "skin"
    m0 = ch.flesh.data.materials[0]
    face_map(ch, tuple(m0.get("acr_base_rgb", (0.8, 0.6, 0.48))))
    finish_variants(ch)
    ch.root["acr_face_frame"] = fr.as_list()
