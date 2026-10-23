"""Individual faces (#283, needs `bpy`, `bmesh`, `numpy`): a finer head on the skin hull
with a modelled base face and its own face map (1024², one coherent UV island; or a baked
face from `faces.face_source`), the `face_*` and `body_*` morph
targets, the `_FACE` zones and the hair and beard variants as separate smooth meshes. The
fields, curves and odds are pure Python in `faces.py`. See docs/assets-humans.md "Faces and
bodies".
"""

import math
import os

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

from . import BUILD_STATS
from . import armature as am
from . import body as bd
from . import coat, decay, faces, held, mhead

HAIR_BASE = (0.80, 0.78, 0.75)  # neutral: the renderer multiplies the chosen hair colour in
FACE_SIZE = faces.FACE_SIZE  # the head's own texture (procedural)
STRAND_SIZE = 256
# `unwrap_head`: texel weight of the face front over scalp and neck, solver iterations.
FACE_WEIGHT = 1.0
UV_ITERATIONS = 20


class Frame:
    """Head-local coordinates (`faces`): x from the head's centre line, y from the face
    front at eye level, z from eye level."""

    def __init__(self, cx, y_front, eye_z):
        self.cx, self.y_front, self.eye_z = cx, y_front, eye_z
        self.keys = None  # MakeHuman head: z map to the painted layout (`mhead.canon_keys`)
        self.field = None  # MakeHuman head: `mhead.MorphField`

    def local(self, co):
        z = co[2] - self.eye_z
        if self.keys is not None:
            z = mhead.canon_z(self.keys, z)
        return co[0] - self.cx, co[1] - self.y_front, z

    def as_list(self):
        return [self.cx, self.y_front, self.eye_z]


def frame_of(cos, cx):
    """The frame of a skin hull's head from its vertex positions (before the base face)."""
    top = max(c[2] for c in cos)
    ez = top - faces.EYE_BELOW_TOP
    near = [c[1] for c in cos if abs(c[0] - cx) < 0.03 and abs(c[2] - ez) < 0.03]
    return Frame(cx, min(near), ez)


def refine(bm, cx, body=None):
    """Hull hook (`body.hull(head=...)`): replaces the hull's head with the MakeHuman head
    (`mhead.build`; pass 4). Returns the head frame (with its morph field)."""
    fr, field, _lm = mhead.build(bm, cx, Frame, body)
    fr.field = field
    return fr


def face_morphs(objs, fr, names=faces.FACE_MORPHS):
    """Shape keys `face_*` on every mesh in `objs` that the head fields move (rest
    positions in head-local coordinates; keys with no motion are left out)."""
    for o in objs:
        basis = [tuple(v.co) for v in o.data.vertices]
        if fr.field is not None:
            at = [fr.field.at(list(names), c) for c in basis]
        for n in names:
            if fr.field is not None:
                ds = [a[n] if a else (0.0, 0.0, 0.0) for a in at]
            else:
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


ZONE_RELAX = 3
BEARD_ZONE_GAIN = 0.7


def zones(obj, fr, beard=True):
    """The `_FACE` attribute (glTF `_FACE`, vec4; `faces.zones`; no beard channel without
    `beard`)."""
    me = obj.data
    a = me.attributes.get("_face") or me.attributes.new("_face", "FLOAT_COLOR", "POINT")
    z = [list(faces.zones(*fr.local(v.co), beard=beard)) for v in me.vertices]
    # The beard zone is shaded per vertex in the renderer: relax it over the mesh so the
    # decimated face's large triangles do not show as dark wedges, and keep it softer.
    nb = [[] for _ in me.vertices]
    for e in me.edges:
        i, j = e.vertices
        nb[i].append(j)
        nb[j].append(i)
    r = [c[0] for c in z]
    for _ in range(ZONE_RELAX):
        r = [0.5 * r[i] + 0.5 * (sum(r[j] for j in nb[i]) / len(nb[i])) if nb[i] else r[i] for i in range(len(r))]
    for d, c, ri in zip(a.data, z, r):
        d.color = (ri * BEARD_ZONE_GAIN, c[1], c[2], c[3])


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
        held.set_materials(o.data, [hair, ribbon] if o.name.endswith("hair_queue") else [hair])
    return objs


# --------------------------------------------------------------------------- face map

def _in_face(fr, co):
    """Above the jaw line, on the head: the face map's (the neck part may reach the jaw)."""
    x, y, z = fr.local(co)
    return z > -0.125 and faces.near(x, y, z) > 0.5


def head_weights(ch):
    """The MakeHuman head is rigid on the head bone down to the jaw, then blends into the
    neck over 4 cm (the hull's proximity weights give the jaw to the neck). Call after
    `body.hull`, before `split_parts`."""
    fr, ob = ch.face_frame, ch.flesh
    if fr is None or fr.field is None:
        return
    gh = ob.vertex_groups.get(ch.prefix + "head") or ob.vertex_groups.new(name=ch.prefix + "head")
    gn = ob.vertex_groups.get(ch.prefix + "neck") or ob.vertex_groups.new(name=ch.prefix + "neck")
    for v in ob.data.vertices:
        x, y, z = fr.local(v.co)
        if faces.near(x, y, z) < 0.5 or z < -0.17:
            continue
        w = float(mhead.smooth(-0.165, -0.125, z))
        for g in ob.vertex_groups:
            if g.index not in (gh.index, gn.index):
                try:
                    g.remove([v.index])
                except RuntimeError:
                    pass
        gh.add([v.index], w, "REPLACE")
        if w < 1.0:
            gn.add([v.index], 1.0 - w, "REPLACE")
        else:
            gn.remove([v.index])


def face_uv(fr, co):
    """Cylindrical face-map UV of a rest position: the face front (u = 0.5) gets the
    most texels (u = 0.5 + 0.5 sin(θ/2)), v from below the chin to the crown."""
    x, y, z = fr.local(co)
    th = math.atan2(x, -(y - faces.CENTRE[1]))
    return 0.5 + 0.5 * math.sin(th / 2.0), (z + 0.17) / 0.30


def unwrap_head(obj, fr, margin=0.004):
    """The head's own UV layout in the active UV map of `obj` (a copy holding only the head:
    face, scalp, ears, upper neck): one coherent island, cut by a seam down the back of the
    head from the nape to the crown (midline edges behind `faces.CENTRE`), minimum-stretch
    unwrap weighted towards the face front and the ears (`FACE_WEIGHT`); the eyeballs are
    small islands beside it. The layout is turned so the face
    stands upright (crown up, the model's left, +x, to the right) and scaled into the unit
    square, so a baked face reads like a portrait. Deterministic: a baker can rely on the
    same layout for the same head. Returns its numbers for `stats.json`."""
    from bpy_extras import mesh_utils

    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    loc = {v: fr.local(v.co) for v in bm.verts}
    seams = 0
    for e in bm.edges:
        e.seam = all(abs(loc[v][0]) < 2e-4 and loc[v][1] > faces.CENTRE[1] for v in e.verts)
        seams += e.seam
    bm.to_mesh(me)
    bm.free()
    # Texel importance: the face front (brows to chin) gets FACE_WEIGHT more area, the ears
    # half of it; scalp and neck the rest.
    g = obj.vertex_groups.new(name="uv_importance")
    for v in me.vertices:
        x, y, z = fr.local(v.co)
        front = (1.0 - mhead.smooth(0.02, 0.06, y)) * mhead.smooth(-0.15, -0.12, z) * (1.0 - mhead.smooth(0.05, 0.08, z))
        ear = faces.ear(x, y, z)
        g.add([v.index], float(max(front, 0.5 * ear)), "REPLACE")
    vl = bpy.context.view_layer
    for o in vl.objects:
        o.select_set(False)
    vl.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.unwrap(method="MINIMUM_STRETCH", fill_holes=True, correct_aspect=True, margin=margin,
                      no_flip=True, iterations=UV_ITERATIONS, use_weights=True, weight_group="uv_importance",
                      weight_factor=FACE_WEIGHT)
    bpy.ops.object.mode_set(mode="OBJECT")
    obj.select_set(False)
    layer = me.uv_layers.active
    n = len(me.loops)
    uv = np.empty(n * 2, np.float32)
    layer.data.foreach_get("uv", uv)
    uv = uv.reshape(-1, 2).astype(np.float64)
    vi = np.empty(n, np.int32)
    me.loops.foreach_get("vertex_index", vi)
    L = np.array([fr.local(v.co) for v in me.vertices], np.float64)[vi]
    islands = sorted(mesh_utils.mesh_linked_uv_islands(me), key=len, reverse=True)
    main = np.zeros(n, bool)
    for pi in islands[0]:
        p = me.polygons[pi]
        main[p.loop_start:p.loop_start + p.loop_total] = True
    # Upright: fit uv ~ A (x, z, 1) over the face front of the main island, turn +z to +v,
    # mirror u when +x points left.
    sel = main & (L[:, 1] < 0.05)
    A = np.linalg.lstsq(np.c_[L[sel][:, 0], L[sel][:, 2], np.ones(int(sel.sum()))], uv[sel], rcond=None)[0]
    th = math.pi / 2 - math.atan2(A[1][1], A[1][0])
    R = np.array([[math.cos(th), -math.sin(th)], [math.sin(th), math.cos(th)]])
    uv = uv @ R.T
    if (R @ A[0])[0] < 0:
        uv[:, 0] = -uv[:, 0]
    lo, hi = uv.min(0), uv.max(0)
    k = (1.0 - 2 * margin) / float((hi - lo).max())
    uv = (uv - lo) * k + margin + (1.0 - 2 * margin - (hi - lo) * k) / 2
    layer.data.foreach_set("uv", uv.astype(np.float32).ravel())
    # Numbers: covered share of the square, folds (flipped polygons of the main island),
    # texel density on the face front (per metre at a 1 px texture).
    area_uv = area_3d = used = 0.0
    sign = []
    for p in me.polygons:
        ls = list(range(p.loop_start, p.loop_start + p.loop_total))
        u = uv[ls]
        a = 0.5 * float(np.sum(u[:, 0] * np.roll(u[:, 1], -1) - np.roll(u[:, 0], -1) * u[:, 1]))
        used += abs(a)
        if main[p.loop_start]:
            sign.append(a)
            if all(L[i][1] < 0.05 for i in ls) and L[p.loop_start][2] > -0.12:
                area_uv += abs(a)
                area_3d += p.area
    sign = np.array(sign)
    flipped = int(min((sign > 0).sum(), (sign < 0).sum()))
    return {"islands": len(islands), "main_polys": len(islands[0]), "seam_edges": int(seams),
            "used": round(used, 3), "flipped": flipped,
            "front_per_m": math.sqrt(area_uv / max(area_3d, 1e-12))}


def dilate(img, mask, steps=12):
    """Grows the painted texels (`mask`) into the empty ones, so filtering and mip maps
    do not pull the background into the island edges."""
    img = img.copy()
    m = mask.copy()
    for _ in range(steps):
        acc = np.zeros_like(img)
        cnt = np.zeros(m.shape, np.float32)
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            mm = np.roll(np.roll(m, dy, 0), dx, 1)
            acc += np.roll(np.roll(img, dy, 0), dx, 1) * mm[..., None]
            cnt += mm
        grow = (~m) & (cnt > 0)
        img[grow] = acc[grow] / cnt[grow][:, None]
        m = m | grow
    if (~m).any():
        img[~m] = img[m].mean(0)
    return img


def face_map(ch, skin_rgb, beard=True, scalp=True):
    """The head's own material and texture: the head part of the skin gets its own material
    slot and, in the `atlas` UV map, its own layout (`unwrap_head`: one coherent island) on
    a separate image (`faces.FACE_SIZE`, 1024²). Cycles bakes position, normal and occlusion
    of a copy of the head, numpy paints it (`paint_face`). A baked face for the unit
    (`faces.face_source`: `$ACR_SOURCES/faces/<unit>.png` in this layout) replaces the
    painted colours (paint masks still go on top); without one the face is painted."""
    fr = ch.face_frame
    flesh = ch.flesh
    me = flesh.data
    src_path = faces.face_source(ch.name)
    src = bpy.data.images.load(os.path.abspath(src_path)) if src_path else None
    size_px = faces.face_size(max(src.size) if src else None)
    # The texture layer of the skin: `atlas` when dressed by `outfit`, else the detail map
    # (the dragoon rider).
    uvname = "atlas" if "atlas" in me.uv_layers else me.uv_layers[0].name
    parts = [d.value for d in me.attributes["part"].data]
    uv = me.uv_layers[uvname]
    face = [_in_face(fr, me.vertices[v].co) for v in range(len(me.vertices))]
    head = [p.index for p in me.polygons if all(parts[v] == 1 or face[v] for v in p.vertices)]
    # Unwrap and bake on a copy holding only the head (#283 pass 5: a cylinder folded the
    # chin's underside onto the neck), the UVs copied back.
    tmp = bpy.data.objects.new(f"{ch.name}.face_bake", me.copy())
    tmp.matrix_world = flesh.matrix_world
    bd.link(tmp)
    if tmp.data.shape_keys:
        tmp.shape_key_clear()
    bm = bmesh.new()
    bm.from_mesh(tmp.data)
    orig = bm.faces.layers.int.new("orig")
    for f in bm.faces:
        f[orig] = f.index
    keep = set(head)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.index not in keep], context="FACES")
    bm.to_mesh(tmp.data)
    bm.free()
    tmp.data.uv_layers.active = tmp.data.uv_layers[uvname]
    st = unwrap_head(tmp, fr)
    st["front_px_per_cm"] = round(st.pop("front_per_m") * size_px / 100.0, 1)
    st.update(texture_px=size_px, baked_face=bool(src))
    BUILD_STATS.setdefault("face_uv", {})[ch.name] = st
    src_uv = tmp.data.uv_layers[uvname]
    origs = [d.value for d in tmp.data.attributes["orig"].data]
    for tp in tmp.data.polygons:
        p = me.polygons[origs[tp.index]]
        for li, tl in zip(p.loop_indices, tp.loop_indices):
            uv.data[li].uv = src_uv.data[tl].uv
    size = coat.SIZE
    coat.SIZE = size_px
    try:
        pos, nrm, ao, covered = coat.bake([tmp], size_px)
    finally:
        coat.SIZE = size
    data = tmp.data
    bpy.data.objects.remove(tmp)
    bpy.data.meshes.remove(data)
    flat_p, flat_n = pos.reshape(-1, 3), nrm.reshape(-1, 3)
    loc = np.stack([flat_p[:, 0] - fr.cx, flat_p[:, 1] - fr.y_front, flat_p[:, 2] - fr.eye_z], -1).astype(np.float32)
    cov = covered.reshape(-1)
    if src is not None:
        # The baked face as it is (its own shading); sRGB texels to linear.
        if tuple(src.size) != (size_px, size_px):
            src.scale(size_px, size_px)
        px = np.array(src.pixels[:], np.float32).reshape(-1, 4)[:, :3]
        bpy.data.images.remove(src)
        col = coat.lin(px).astype(np.float32)
        col[cov] = face_masks(ch, flat_p[cov], flat_n[cov], col[cov])
    else:
        col = np.tile(coat.lin(skin_rgb) * 0.85, (len(loc), 1)).astype(np.float32)
        col[cov] = paint_face(loc[cov], flat_n[cov], skin_rgb, beard=beard, scalp=scalp)
        col[cov] = face_masks(ch, flat_p[cov], flat_n[cov], col[cov])
        occ = np.clip(ao.reshape(-1), 0.0, 1.0)
        col[cov] *= (0.72 + 0.28 * occ[cov] ** 0.9)[:, None]
        col = dilate(col.reshape(size_px, size_px, 3), cov.reshape(size_px, size_px)).reshape(-1, 3)
    img = _image(f"{ch.name}.face", col, size_px)
    m = decay.material(f"{ch.name}.face", "flesh", skin_rgb, image=img, uv_map=uvname, interpolation="Linear")
    m["acr_colour_from"] = "outfit"
    m["acr_tint"] = "skin"
    me.materials.append(m)
    slot = len(me.materials) - 1
    for pi in head:
        me.polygons[pi].material_index = slot
    return img


def face_masks(ch, p, n, col):
    """The character's paint masks on skin (`ch.paint_masks` entries whose keys are painted
    by the `skin` painter: face paint of `masks.py`) over the head's own texels: the head
    has its own texture (`face_map`), so the masks painted on the outfit atlas never reached
    the face. `p`, `n`: rest-pose positions and normals (armature space, as the atlas)."""
    from . import outfit, registry

    entries = [e for e in getattr(ch, "paint_masks", ()) or ()
               if "skin" in {registry.PAINTER.get(k) for k in e["keys"]}]
    if not entries or not len(p):
        return col
    flesh = [v.co.z for o in ch.layers["flesh"] for v in o.data.vertices]
    L = {"head_top": max(flesh) if flesh else 1.75}
    for i, e in enumerate(entries):
        col = outfit.apply_mask(e, p, n, col, L, seed=53 + 7 * i)
    return col


def _g(*pairs):
    e = 0.0
    for v, c, s in pairs:
        e = e + ((v - c) / s) ** 2
    return np.exp(-e)


def paint_face(p, n, rgb, seed=41, beard=True, scalp=True):
    """Colours of head texels from head-local positions `p` (N, 3) and normals: weathered
    skin, eyes with whites, iris, lids and lashes, brows, nostrils, lips and the mouth line,
    nasolabial folds, a cropped scalp along the hairline (`scalp`), a faint beard shadow
    (`beard`)."""
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    ax = np.abs(x)
    base = coat.lin(rgb)
    fine = coat.fbm(p, 300.0, seed, octaves=1)
    col = base * (0.86 + 0.12 * coat.fbm(p, 30.0, seed + 1) + 0.06 * fine)[:, None]
    # Weathering: red blotches, sun-darkened front and top, a few moles.
    blot = coat.smooth(0.62, 0.80, coat.fbm(p, 16.0, seed + 2)) * 0.18
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
    col = coat.mix(col, base * np.array([0.68, 0.52, 0.40], np.float32), spots * 0.25)
    pores = coat.smooth(0.60, 0.72, coat.fbm(p, 900.0, seed + 8, octaves=1))
    col *= (1.0 - 0.08 * pores)[:, None]
    dirt = coat.smooth(0.55, 0.80, coat.fbm(p, 9.0, seed + 9)) * 0.10
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
    # A full band, thickest over the inner eye and tapering out, with fine hair texture.
    width = 0.0050 - 0.0022 * coat.smooth(0.030, 0.060, ax)
    brow = coat.smooth(width, width * 0.45, np.abs(z - bz)) * coat.smooth(0.062, 0.052, ax) * coat.smooth(0.005, 0.011, ax) * front
    col = coat.mix(col, coat.lin((0.10, 0.075, 0.055)), brow * (0.78 + 0.18 * coat.smooth(0.3, 0.7, hairs)))
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
    hl = faces.hairline(scalp)
    line_ph = np.interp(np.abs(th), [k for k, _ in hl], [v for _, v in hl])
    scalp = coat.smooth(-6.0, -1.0, ph - line_ph)  # dark under the tapered hair edge
    stip = coat.smooth(0.35, 0.65, fine)
    col = coat.mix(col, coat.lin((0.11, 0.08, 0.06)) * (0.8 + 0.4 * stip)[:, None], scalp * (0.6 + 0.3 * stip))
    # A faint beard shadow (the renderer adds the soldier's stubble from `_FACE`).
    if not beard:
        return col
    beard = np.fromiter((faces.zones(float(a), float(b), float(c))[0] for a, b, c in p), np.float32, len(p))
    # Unshaven: days of dark stubble in the beard zone (the renderer adds more per soldier).
    # Fine even stubble: a soft grey-blue shadow of shaved skin and sparse hair points,
    # faded at the zone's edge (no blotches).
    speck = coat.smooth(0.55, 0.70, coat.fbm(p, 2200.0, seed + 10, octaves=1))
    beard = coat.smooth(0.15, 0.85, beard)
    col = coat.mix(col, col * np.array([0.80, 0.80, 0.84], np.float32), beard * 0.45)
    col = coat.mix(col, coat.lin((0.10, 0.09, 0.085)), beard * speck * 0.35)
    return col


def finish(ch, objs=None, skin=None, beard=True, scalp=True):
    """Face morphs on `objs` (default: flesh and clothes; a rider passes his own, not the
    horse's), `_FACE` on the `skin` meshes (default: `ch.flesh`), the head's face map,
    body morphs, the variants' finishing; skin materials get `acr_tint` "skin". Call after
    `Character.morphs` and `outfit.dress`.

    A clean-shaven face (the natives) passes `beard=False`: no beard channel in `_FACE` (the
    renderer's stubble) and no painted beard shadow; `scalp=False` paints no cropped scalp
    along the hairline (shaved or plucked heads: a roach, a scalp lock)."""
    fr = ch.face_frame
    objs = objs if objs is not None else ch.layers["flesh"] + ch.layers["clothes"]
    face_morphs(objs, fr)
    body_morphs(ch, objs)
    for o in skin or [ch.flesh]:
        zones(o, fr, beard=beard)
        for m in o.data.materials:
            if m is not None:
                m["acr_tint"] = "skin"
    m0 = ch.flesh.data.materials[0]
    face_map(ch, tuple(m0.get("acr_base_rgb", (0.8, 0.6, 0.48))), beard=beard, scalp=scalp)
    finish_variants(ch)
    ch.root["acr_face_frame"] = fr.as_list()
