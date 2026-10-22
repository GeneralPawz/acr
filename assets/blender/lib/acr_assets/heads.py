"""Individual faces (#283, needs `bpy`, `bmesh`): a finer head on the skin hull with a
modelled base face, the `face_*` and `body_*` morph targets, the `_FACE` zones and the
hair and beard variants as separate small skinned meshes. The fields and odds are pure
Python in `faces.py`. See docs/assets-humans.md "Faces and bodies".
"""

import math

import bmesh
import bpy

from . import armature as am
from . import body as bd
from . import decay, detail, faces

HAIR_BASE = (0.80, 0.78, 0.75)  # neutral: the renderer multiplies the chosen hair colour in


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
        return 2 if (y < 0.075 and -0.13 < z < 0.08 and abs(x) < 0.085) else 1

    for lv in (1, 2):
        edges = {e for f in bm.faces if level(f) >= lv for e in f.edges}
        bmesh.ops.subdivide_edges(bm, edges=list(edges), cuts=1, use_grid_fill=True, smooth=1.0)
    for v in bm.verts:
        x, y, z = fr.local(v.co)
        if z < -0.2:
            continue
        d = faces.base_face(x, y, z)
        v.co.x += d[0]
        v.co.y += d[1]
        v.co.z += d[2]
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


def _queue(ch, fr, name):
    """The pigtail: a tapered tube down the nape from the back of the head, tied with a
    black ribbon near the top (material 1); rigid on the head bone."""
    seg = 8
    back = max(fr.local(v.co)[1] for v in ch.flesh.data.vertices
               if abs(fr.local(v.co)[0]) < 0.02 and abs(fr.local(v.co)[2] + 0.045) < 0.02)
    path = faces.queue_path(back)
    verts, faces_, mats = [], [], []
    for (y, z, r) in path:
        for k in range(seg):
            a = 2 * math.pi * k / seg
            verts.append((fr.cx + math.cos(a) * r, fr.y_front + y + math.sin(a) * r * 0.8, fr.eye_z + z))
    for i in range(len(path) - 1):
        for k in range(seg):
            k2 = (k + 1) % seg
            faces_.append((i * seg + k, i * seg + k2, (i + 1) * seg + k2, (i + 1) * seg + k))
            mats.append(1 if i == 0 else 0)
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces_)
    for p, m in zip(me.polygons, mats):
        p.use_smooth = True
        p.material_index = m
    obj = bd.link(bpy.data.objects.new(name, me))
    g = obj.vertex_groups.new(name=ch.prefix + "head")
    g.add(list(range(len(verts))), 1.0, "REPLACE")
    attr = me.attributes.new("part", "INT", "POINT")
    for d in attr.data:
        d.value = 1
    return am.bind(obj, ch.arm)


def variants(ch):
    """The hair and beard variants (`faces.HAIR_VARIANTS`, `BEARD_VARIANTS`): shells of the
    head skin over each region, plus the queue. Call before `Character.morphs` (the copies
    must not carry the skin's shape keys). Returns {suffix: object}."""
    fr = ch.face_frame
    P = ch.prefix
    out = {}
    for v, off in faces.OFFSETS.items():
        homes = {P + "head"} | ({P + "neck"} if v == "hair_loose" else set())

        def keep(c, n, h, p, v=v, homes=homes):
            return h in homes and faces.region(v, *fr.local(c))

        out[v] = bd.derive(ch.flesh, f"{ch.name}.{v}", keep, off)
    out["hair_queue"] = _queue(ch, fr, f"{ch.name}.hair_queue")
    ch.variants = out
    return out


def finish_variants(ch, deflate=0.65):
    """Morphs, masks, `_LIMB`, materials of the variants (after `Character.morphs`)."""
    from . import limb, rig

    bones = [ch.prefix + b for b in rig.BODY_BONES]
    objs = list(ch.variants.values())
    for o in objs:
        bd.toward_bones(o, ch.arm, "deflate", deflate, bones)
    face_morphs(objs, ch.face_frame)
    img = detail.tile(f"{ch.name}.hair_variant", HAIR_BASE, "hair", seed=29)
    hair = decay.material(f"{ch.name}.hair_variant", "hair", HAIR_BASE, image=img, uv_map="detail")
    hair["acr_tint"] = "hair"
    ribbon = decay.material(f"{ch.name}.hair_ribbon", "hair", (0.05, 0.05, 0.05))
    for o in objs:
        decay.add_masks(o, "hair", seed=sum(map(ord, o.name)), part_of_vertex=lambda i: 1)
        a = o.data.attributes.get("_limb") or o.data.attributes.new("_limb", "FLOAT", "POINT")
        for d in a.data:
            d.value = limb.TRUNK
        o.data.materials.clear()
        o.data.materials.append(hair)
        if o.name.endswith("hair_queue"):
            o.data.materials.append(ribbon)
        detail.box_uv(o)
    return objs


def finish(ch, objs=None, skin=None):
    """Face morphs on `objs` (default: flesh and clothes; a rider passes his own, not the
    horse's), `_FACE` on the `skin` meshes (default: `ch.flesh`), body morphs, the
    variants' finishing; skin materials get `acr_tint` "skin". Call after
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
    finish_variants(ch)
    ch.root["acr_face_frame"] = fr.as_list()
