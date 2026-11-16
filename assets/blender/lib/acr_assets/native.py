"""Native warriors on foot as one family (docs/humans-roster.md, family 3; docs/families/
native.md): the Woodlands dress of the Iroquois, Huron and Delaware, the base the Plains
(family 9) and Pueblo (family 10) units build on. A unit script is data: its `.MD` lines,
its `WEAPONKIND`s and a `SPEC` (`spec`), then `build(ctx)` calls `native.build(ctx, SPEC)`.

The dress is modelled on the body (shells of the MakeHuman skin, `Character.shell`) plus
loose modelled panels, not draped: every piece deforms with the skin.
- `breechcloth`: a band through the belt and two flaps hanging front and back (their lower
  edge follows the thighs half way, so a stride does not poke through);
- `leggings`: side-seam leggings of trade stroud from the ankle to high on the thigh, with
  the flap (the seam allowance left wide) standing out along the outer side, and garters
  under the knee;
- `moccasins`: soft one-piece moccasins over a last (`mbody.shoes` in buckskin);
- `shirt`: the trade shirt of linen, worn loose to mid-thigh (optional);
- `matchcoat`: a blanket over the left shoulder and under the right arm (optional);
- hair: `roach` (a deer-hair crest from the crown to the nape on a shaved head), `lock`
  (the scalp lock: a tuft at the crown, a braid down the back) or `long` (loose hair to
  the shoulders, Plains); feathers in it (`feathers`: how many);
- skin paint (`paint`: a set of `masks.PAINT_SETS`, one per unit type: one atlas per
  model), silver arm bands and a gorget (`silver`);
- gear: bow and quiver, trade gun (with powder horn and shot pouch), tomahawk, war club,
  spear, knife at the belt.

SPEC keys (all but `md` and `weapons` optional):
- `md`: the `.MD` animation lines by clip; `weapons`: {index: WEAPONKIND}; `active`: the
  `SETACTIVEPOINT` frame by clip (`{"ATTACK1": 44}`);
- `palette`: colours by material key (sRGB) over `PALETTE`; `mix`: the sprite share;
- `weapon`: the main weapon, `bow`, `gun`, `tomahawk`, `club` or `spear`; `melee`: the
  `clips.MELEE` kind of index 0 (`club`, `tomahawk`, `spear`; None: no strikes);
- `dress`: any of `breechcloth`, `leggings`, `moccasins`, `shirt`, `matchcoat`;
- `hair`, `feathers`, `paint`, `silver` (True: gorget and arm bands), `kit`: strapped gear
  (`quiver`, `horn`, `pouch`, `knife`);
- `drape`: pieces of `dress` drafted and draped instead of shells (#307): `leggings`
  (`garments.side_leggings`, with its garters; the flaps stay modelled), `breechcloth`
  (`garments.breechcloth`: one strip between the legs and over the belt, `_flap_weights`).
"""

import math

import bmesh
from mathutils import Matrix, Vector

from . import aim as aiming
from . import armature as am
from . import clips as cl
from . import crawl, gear, grips, heads, held, masks, mbody, outfit, poses, registry, rig, straps
from .humans import LAYER_OF, Character, prepare_export

# The Iroquois warrior's colours: a nation overrides them. The skin is a warm copper as in
# the sprites (slice 1's darker brown plus the sprite mix came out near-black on the
# field); it keeps the palette colour (`fixed`), a unit varies it a little.
# Garters: dark (worsted tape or hide; the sprites' garters read 0.15-0.43, #307).
PALETTE = {
    "skin": (0.80, 0.55, 0.38), "bone": (0.85, 0.81, 0.69), "hair": (0.06, 0.05, 0.05),
    "breechcloth": (0.16, 0.18, 0.30), "leggings": (0.50, 0.10, 0.08), "flaps": (0.50, 0.10, 0.08),
    "garter": (0.30, 0.25, 0.17), "moccasins": (0.45, 0.32, 0.20), "shirt": (0.86, 0.84, 0.78),
    "matchcoat": (0.20, 0.24, 0.36), "belt_hide": (0.30, 0.20, 0.12), "belts": (0.30, 0.20, 0.12),
    "roach": (0.62, 0.12, 0.08), "feather": (0.88, 0.85, 0.78), "silver": (0.80, 0.80, 0.82),
    "horn": (0.80, 0.72, 0.55), "pouch": (0.36, 0.24, 0.14), "hide": (0.55, 0.40, 0.26),
    "wood": (0.40, 0.25, 0.13), "metal": (0.50, 0.50, 0.52), "stone": (0.20, 0.20, 0.22),
    "cord": (0.70, 0.62, 0.48), "paint_red": (0.55, 0.12, 0.07), "paint_black": (0.05, 0.05, 0.05),
    "paint_white": (0.86, 0.84, 0.78),
}
BUILD = {"width": 0.98, "shoulders": 1.08, "limbs": 1.15, "hips": 0.98, "belly": 0.95}
DEFAULTS = {"build": BUILD, "weapon": "bow", "melee": "club", "mix": 0.6, "fixed": ("skin", "feather", "silver"),
            "dress": ("breechcloth", "leggings", "moccasins"), "hair": "roach", "feathers": 2, "paint": "red_black",
            "silver": False, "kit": ("knife",), "active": None, "gun_len": 1.45, "headdress": None, "rattle": False,
            "work": False, "drape": (), "scale": 1.0}

# Material keys and painters of the family (`registry`): stroud (trade wool) for the
# breechcloth, leggings and blanket, buckskin for moccasins and pouches.
registry.painter("stroud", like="wool", finish=(1.0, 0.40, 0.0))
registry.painter("buckskin", like="leather", finish=(0.85, 0.0, 0.05))
registry.painter("pelt", like="felt", finish=(1.0, 0.30, 0.0))  # a bear or wolf skin: matted, dull
for _key, _layer, _painter, _fb in (
        ("breechcloth", "wool", "stroud", (0.16, 0.18, 0.30)), ("leggings", "wool", "stroud", (0.50, 0.10, 0.08)),
        ("flaps", "wool", "stroud", (0.50, 0.10, 0.08)), ("matchcoat", "wool", "stroud", (0.20, 0.24, 0.36)),
        ("moccasins", "leather", "buckskin", (0.45, 0.32, 0.20)), ("belt_hide", "leather", "buckskin", (0.30, 0.20, 0.12)),
        ("pouch", "leather", "buckskin", (0.36, 0.24, 0.14)), ("roach", "hair", None, (0.62, 0.12, 0.08)),
        ("horn", "bone", None, (0.80, 0.72, 0.55)), ("silver", "metal", None, (0.80, 0.80, 0.82)),
        ("pelt", "hide", "pelt", (0.36, 0.27, 0.18)), ("gustoweh", "hide", "buckskin", (0.40, 0.28, 0.16)),
        ("turtle", "bone", None, (0.34, 0.28, 0.16))):
    registry.material(_key, _layer, painter=_painter, fallback=_fb)


# -- skin paint and face (the native variants of `masks` and `heads`) -------------------

# `masks.paint_face_half` once had no width limit (every front-facing texel from 27 cm
# below the top of the head, i.e. the shoulders and the upper arms); this name kept the
# fixed copy. The shared mask is limited to the head now (`masks._face`), so the name is an
# alias kept for the unit specs that use it.
paint_face_half = masks.paint_face_half


registry.mask("native_face_half", paint_face_half)
FIXED_MASKS = {}  # mask substitutions (none since `masks._face` is limited)
# Paint sets of the native units beyond `masks.PAINT_SETS` (one per unit type).
PAINT_SETS = {
    "black_red_chin": [("paint_face_half", "paint_black", {"side": 1.0}), ("paint_face_band", "paint_red", {"high": 0.20})],
    "red_stripes": [("paint_stripes", "paint_red", {"period": 0.09}), ("paint_face_band", "paint_red", {})],
    "white_band": [("paint_face_band", "paint_white", {"width": 0.04}), ("paint_dots", "paint_white", {"face": False})],
}


def body_paint(choice, palette):
    """`masks.body_paint` with the native masks (`FIXED_MASKS`) and sets (`PAINT_SETS`)."""
    sets = PAINT_SETS[choice] if choice in PAINT_SETS else masks.paint_set(choice)
    return [masks.entry(FIXED_MASKS.get(m, m), ("skin",), masks._colour(palette, c), **params) for m, c, params in sets]


LEGS = {"thigh_l", "thigh_r", "shin_l", "shin_r"}
TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}

# Stands (plain poses for the walk cycle; the item orientation in "world").
STAND = dict(cl.STAND_FREE)
STAND_ARMED = cl.STAND_ARMED
STAND_SPEAR = cl._with(cl.STAND_FREE, forearm_r=(-50, 0, 10), world={"socket.hand_r": (4, 0, 0)})


# The hack down at a man on the ground, leaning FORWARD (positive spine X bends forward on
# skeleton H; the shared `clips.club_strikes` stoop leans back and falls ~0.9 m short):
# a long lunge, the trunk bent over the front knee, the blade's face on a man lying 1.8 m
# ahead (the sim fences at 1.6-2.0 m; the strike layer takes the rest).
DOWN_AT = 1.8
LUNGE_DOWN = {"root_loc": (0, -0.75, -0.30), "thigh_l": (-64, -3, 0), "shin_l": (80, 0, 0), "foot_l": (-14, 0, 0),
              "thigh_r": (24, 3, 0), "shin_r": (48, 0, 0), "foot_r": (-12, 0, 0), "spine": (36, 0, 6),
              "spine1": (14, 0, 0), "spine2": (6, 0, 0), "head": (-14, 0, 0)}


def club_down(n, tip=cl.CLUB_TIP, at=DOWN_AT):
    """`ATTACK_CUT_DOWN` (= `ATTACK_DOWN`) leaning forward over the lunge."""
    down_r = (150, 0, 0)
    hit = cl._blow(dict(cl.STAND_FREE, **LUNGE_DOWN), cl.grip_for((0.0, -at, 0.20), down_r, tip), down_r)
    over = cl._blow(dict(cl.STAND_FREE, **dict(LUNGE_DOWN, root_loc=(0, -0.55, -0.18), spine=(14, 0, 0))),
                    (-0.14, -0.70, 1.50), (-40, 0, 0))
    return cl._strike(n, over, hit, over)


def spec(md, weapons, palette=None, build=None, **kw):
    """A full SPEC: the defaults with `palette` and `build` merged key by key."""
    unknown = set(kw) - set(DEFAULTS)
    if unknown:
        raise KeyError(f"unknown SPEC keys {sorted(unknown)}")
    return dict(DEFAULTS, **kw, md=md, weapons=dict(weapons), palette=dict(PALETTE, **(palette or {})),
                build=dict(BUILD, **(build or {})))


# -- dress ------------------------------------------------------------------------------

def _mesh(name, verts, faces, groups, part=0):
    """A skinned mesh from data: `groups` [{bone: weight}] per vertex."""
    import bpy

    from .body import link

    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    me.update()
    obj = link(bpy.data.objects.new(name, me))
    vg = {}
    for i, g in enumerate(groups):
        for b, w in g.items():
            if b not in vg:
                vg[b] = obj.vertex_groups.new(name=b)
            vg[b].add([i], w, "REPLACE")
    attr = me.attributes.new("part", "INT", "POINT")
    for d in attr.data:
        d.value = part
    return obj


def _flap(ch, name, y0, z0, z1, w0, w1, out, back=False, rows=6, cols=4):
    """A breechcloth flap from the belt (z0, y0) to its hem (z1), `w0` wide at the belt,
    `w1` at the hem, leaning `out` m away from the body at the hem; the hem follows the
    thighs half way. Two-sided (a thin slab)."""
    P = ch.prefix
    verts, groups, faces = [], [], []
    s = 1.0 if back else -1.0
    for side, dt in ((0, 0.0), (1, 0.004)):
        for r in range(rows + 1):
            t = r / rows
            z = z0 + (z1 - z0) * t
            y = y0 + s * (out * t * t + dt)
            w = w0 + (w1 - w0) * t
            for c in range(cols + 1):
                x = -w / 2 + w * c / cols
                verts.append((x, y, z))
                k = 0.5 * t * t
                g = {P + "hips": 1.0 - k}
                if k > 0:
                    g[P + "thigh_l"] = k * (0.5 + x / max(w, 1e-6))
                    g[P + "thigh_r"] = k * (0.5 - x / max(w, 1e-6))
                groups.append(g)
    n = (rows + 1) * (cols + 1)
    for side in (0, 1):
        for r in range(rows):
            for c in range(cols):
                a = side * n + r * (cols + 1) + c
                q = (a, a + 1, a + cols + 2, a + cols + 1)
                faces.append(q if (side == 0) != back else q[::-1])
    # close the sides and the hem
    for r in range(rows):
        for c in (0, cols):
            a, b = r * (cols + 1) + c, (r + 1) * (cols + 1) + c
            faces.append((a, b, b + n, a + n) if c == 0 else (a, a + n, b + n, b))
    for c in range(cols):
        a = rows * (cols + 1) + c
        faces.append((a, a + n, a + n + 1, a + 1))
    obj = _mesh(f"{ch.name}.{name}", verts, faces, groups)
    return am.bind(obj, ch.arm)


def _flap_weights(ch, obj, z0=1.03, off=0.02):
    """The draped breechcloth's flaps (the cloth standing more than `off` m off the skin, and
    all of it below the crotch) skinned like the modelled flaps (`_flap`): the hips, the hem
    going 60 % to the thighs across the width, so a stride swings the flap instead of
    poking through it. The band between the legs keeps the weights of the layer beneath."""
    from mathutils.bvhtree import BVHTree

    P, fl = ch.prefix, ch.flesh
    tree = BVHTree.FromPolygons([v.co for v in fl.data.vertices], [p.vertices for p in fl.data.polygons])
    cz = min((v.co.z for v in fl.data.vertices if abs(v.co.x) < 0.012 and v.co.z > 0.6), default=0.82)
    vs = obj.data.vertices
    z1 = min(v.co.z for v in vs)
    w = max(max(abs(v.co.x) for v in vs) * 2.0, 1e-3)
    groups = {n: obj.vertex_groups.get(P + n) or obj.vertex_groups.new(name=P + n) for n in ("hips", "thigh_l", "thigh_r")}
    flap = []
    for v in vs:
        d = tree.find_nearest(v.co)[3]
        if v.co.z < cz or (d is not None and d > off):
            flap.append(v.index)
    for g in obj.vertex_groups:
        g.remove(flap)
    # The draped leggings beneath (`ch.legging_cloth`): where the flap lies on a legging top
    # it takes the legging's weights, fully within `near[0]` m, blending out by `near[1]`
    # (past `acr_render::layers::REACH`), so a parted leg carries the flap over its top
    # instead of coming out through it (#307: 38 crossings, 40 mm, in ATTACK_CUT_FINISH).
    under = _surface_weights(getattr(ch, "legging_cloth", ()))
    near = (0.015, 0.045)
    for i in flap:
        co = vs[i].co
        t = min(max((z0 - co.z) / (z0 - z1), 0.0), 1.0)
        k = 0.6 * t  # linear: the flap follows the thighs sooner than `_flap`'s (the legging tops)
        ws = {P + "hips": 1.0 - k}
        if k > 0:
            ws[P + "thigh_l"] = k * min(max(0.5 + co.x / w, 0.0), 1.0)
            ws[P + "thigh_r"] = k * min(max(0.5 - co.x / w, 0.0), 1.0)
        if under:
            d, wl = under(co)
            s = min(max((near[1] - d) / (near[1] - near[0]), 0.0), 1.0) if wl else 0.0
            if s > 0:
                ws = {b: ws.get(b, 0.0) * (1 - s) + wl.get(b, 0.0) * s for b in set(ws) | set(wl)}
                ws = dict(sorted(ws.items(), key=lambda kv: -kv[1])[:4])
                tot = sum(ws.values()) or 1.0
                ws = {b: x / tot for b, x in ws.items()}
        for b, x in ws.items():
            if x > 0.0:
                g = groups.get(b[len(P):]) or obj.vertex_groups.get(b) or obj.vertex_groups.new(name=b)
                g.add([i], x, "REPLACE")
    return len(flap)


def _surface_weights(objs):
    """A lookup co -> (distance m, {bone: weight}) on the nearest face of `objs` (skinned,
    rest pose), the face's weights interpolated by inverse distance to its corners; None
    without objects."""
    from mathutils.bvhtree import BVHTree

    verts, polys, wts = [], [], []
    for o in objs:
        names = {g.index: g.name for g in o.vertex_groups}
        base = len(verts)
        for v in o.data.vertices:
            verts.append(v.co.copy())
            wts.append({names[e.group]: e.weight for e in v.groups if e.group in names and e.weight > 0
                        and not names[e.group].startswith("acr_")})
        polys += [[base + i for i in p.vertices] for p in o.data.polygons]
    if not polys:
        return None
    tree = BVHTree.FromPolygons(verts, polys)

    def look(co):
        loc, _n, fi, d = tree.find_nearest(co)
        if fi is None:
            return 1e9, {}
        inv = [1.0 / max((verts[i] - loc).length, 1e-5) for i in polys[fi]]
        w = {}
        for i, k in zip(polys[fi], inv):
            for b, x in wts[i].items():
                w[b] = w.get(b, 0.0) + x * k / sum(inv)
        return d, w

    return look


def _legging_flap(ch, side, z0=0.14, z1=0.76, rows=14, surface=None):
    """The legging's flap on the outer side of leg `side` (1 left, -1 right): a two-sided
    fin from the outermost skin line (sampled in bands of height, smoothed) standing
    2-4 cm out (widest at the ankle), each row skinned like its skin vertex; `surface`: the
    draped leggings to stand out from instead of the skin."""
    P, flesh = ch.prefix, surface or ch.flesh
    inset = 0.009 if surface is None else 0.007
    names = {g.index: g.name for g in flesh.vertex_groups}
    legs = {P + b for b in LEGS}
    rows_out = []
    for r in range(rows + 1):
        z = z0 + (z1 - z0) * r / rows
        best = None
        for v in flesh.data.vertices:
            if abs(v.co.z - z) > 0.025 or v.co.x * side < 0.03:
                continue
            gs = {names[g.group]: g.weight for g in v.groups if g.weight > 0.01 and names.get(g.group) in legs}
            if gs and (best is None or v.co.x * side > best[0].x * side):
                best = (v.co.copy(), gs)
        if best:
            rows_out.append((z, best[0], best[1]))
    # Smooth the line (the outermost vertex jumps a little from band to band).
    pts = [c for _, c, _ in rows_out]
    sm = [(pts[max(i - 1, 0)] + pts[i] * 2 + pts[min(i + 1, len(pts) - 1)]) / 4 for i in range(len(pts))]
    verts, groups, faces_ = [], [], []
    for (z, _c, gs), c in zip(rows_out, sm):
        t = (z - z0) / (z1 - z0)
        width = 0.040 - 0.018 * t
        for dy in (-0.002, 0.002):
            for dx in (-0.006, width):
                verts.append((c.x + side * (inset + dx), c.y + dy, z))
                groups.append(gs)
    n = len(rows_out)
    for i in range(n - 1):
        a, b = 4 * i, 4 * (i + 1)
        faces_ += [(a, a + 1, b + 1, b), (a + 2, b + 2, b + 3, a + 3), (a + 1, a + 3, b + 3, b + 1)]
    faces_ += [(0, 2, 3, 1), (4 * (n - 1), 4 * (n - 1) + 1, 4 * (n - 1) + 3, 4 * (n - 1) + 2)]
    if side < 0:
        faces_ = [f[::-1] for f in faces_]
    obj = _mesh(f"{ch.name}.legging_flap{'_l' if side > 0 else '_r'}", verts, faces_, groups)
    return am.bind(obj, ch.arm)


# -- authored legwear (#523): tubes, bands and panels with a thickness, each made once and
# laid over what is under it with a fixed offset (instead of shells cut out of the skin) ----

def _solid(verts, faces, thickness, centred=False):
    """The surface (`faces` wound outwards) given a thickness: vertex i stays the inner side
    (lying where it was placed), i + n is the outer one, the open edges closed by a rim.
    `centred`: half the thickness to each side (a free-standing panel)."""
    bm = bmesh.new()
    vs = [bm.verts.new(v) for v in verts]
    for f in faces:
        bm.faces.new([vs[i] for i in f])
    bm.verts.index_update()
    bm.normal_update()
    n, k0 = len(verts), (-0.5 * thickness if centred else 0.0)
    out = [tuple(v.co + v.normal * k0) for v in vs] + [tuple(v.co + v.normal * (k0 + thickness)) for v in vs]
    rim = []
    for e in bm.edges:
        if len(e.link_faces) == 1:
            lp = next(lp for lp in e.link_faces[0].loops if lp.edge is e)
            a, b = lp.vert.index, lp.link_loop_next.vert.index
            rim.append((a, b, b + n, a + n))
    bm.free()
    return out, [tuple(reversed(f)) for f in faces] + [tuple(i + n for i in f) for f in faces] + rim


def _trees(objs):
    from mathutils.bvhtree import BVHTree

    return [BVHTree.FromPolygons([v.co.copy() for v in o.data.vertices], [tuple(p.vertices) for p in o.data.polygons])
            for o in objs if o is not None and o.type == "MESH" and len(o.data.polygons)]


def _from_inside(trees, origin, direction, limit):
    """Distance from `origin` (inside the limb) out to the furthest of the trees' first
    surfaces within `limit`; None without one."""
    best = None
    for t in trees:
        d = t.ray_cast(origin, direction, limit)[3]
        if d is not None and (best is None or d > best):
            best = d
    return best


def _from_outside(trees, origin, direction, limit=1.0):
    """The first point any tree shows to a ray coming in from outside; None without one."""
    best = None
    for t in trees:
        loc, _n, _i, d = t.ray_cast(origin, direction, limit)
        if d is not None and (best is None or d < best[0]):
            best = (d, loc)
    return best[1] if best else None


def _authored(ch, name, key, verts, faces, groups, made, symmetric=False, smooth=0):
    """`smooth`: the first so many faces (the two sides of `_solid`, not its rim) shaded smooth."""
    obj = am.bind(_mesh(f"{ch.name}.{name}", verts, faces, groups), ch.arm)
    for p in obj.data.polygons[:smooth]:
        p.use_smooth = True
    obj["acr_garment"] = f"authored:native.{made}"
    return ch.add("clothes", obj, key, symmetric)


def _skin_groups(ch, pts, sided=True):
    """Each point's weights from the skin's nearest face; `sided`: never a bone of the other
    side (the inside of a thigh lies nearest the other thigh)."""
    look = _surface_weights([ch.flesh])
    out = []
    for p in pts:
        w = look(Vector(p))[1]
        if sided and abs(p[0]) > 1e-4:
            drop = "_r" if p[0] > 0 else "_l"
            w = {b: x for b, x in w.items() if not b.endswith(drop)} or w
        w = dict(sorted(w.items(), key=lambda kv: -kv[1])[:4])
        tot = sum(w.values()) or 1.0
        out.append({b: x / tot for b, x in w.items()})
    return out


def _crotch_z(ch):
    return min((v.co.z for v in ch.flesh.data.vertices if abs(v.co.x) < 0.012 and v.co.z > 0.6), default=0.82)


def _leggings(ch, n=28, step=0.025, offset=0.007, thickness=0.003, garter=None):
    """Side-seam leggings (#523): per leg one tube of hide from the ankle to the upper thigh,
    its top cut on the slant (`garments.legging_top`: highest at the outer hip, where a thong
    ties it to the belt, below the crotch inside), `offset` m off the skin and the moccasin,
    loosened over the hollows of the leg (`garments.loosen`), `thickness` thick with closed
    rims. The left one is fitted to the body and mirrored. `garter`: a key for a tied band
    below the knee. Returns the object and the outer line of the left tube (the side seam,
    bottom to top) for `_seam_flap`."""
    from . import garments as gm

    P = ch.prefix
    seg = am.segments(ch.arm)
    trees = _trees([ch.flesh, getattr(ch, "shoes", None)])
    knee, ankle = seg[P + "shin_l"]
    hip = seg[P + "thigh_l"][0]
    cz = _crotch_z(ch)
    z_a = max(0.10, ankle.z + 0.03)
    inner, outer = cz - 0.035, min(cz + 0.10, 0.95)

    def centre(z):
        if z <= knee.z:
            return ankle.lerp(knee, (z - ankle.z) / max(knee.z - ankle.z, 1e-6))
        return knee.lerp(hip, min((z - knee.z) / max(hip.z - knee.z, 1e-6), 1.0))

    rows = max(int(math.ceil((outer - z_a) / step)), 4)
    th = [2 * math.pi * i / n for i in range(n)]
    tops = [gm.legging_top(t, inner, outer) for t in th]
    R = [[0.0] * n for _ in range(rows + 1)]
    for i, t in enumerate(th):
        col, last = [], 0.05
        for j in range(rows + 1):
            z = z_a + (tops[i] - z_a) * j / rows
            c = centre(z)
            d = _from_inside(trees, Vector((c.x, c.y, z)), Vector((math.cos(t), math.sin(t), 0.0)), 0.17)
            last = d if d is not None else last
            col.append(last)
        for j, r in enumerate(gm.loosen(col, passes=3)):
            R[j][i] = r
    R = [gm.loosen(ring, passes=1, closed=True) for ring in R]

    def at(i, j, extra=0.0, z=None):
        f = j / rows if z is None else min(max((z - z_a) / (tops[i] - z_a), 0.0), 1.0)
        zz = z_a + (tops[i] - z_a) * f
        k = min(int(f * rows), rows - 1)
        u = f * rows - k
        r = R[k][i] * (1 - u) + R[k + 1][i] * u + offset + extra
        c = centre(zz)
        return (c.x + r * math.cos(th[i]), c.y + r * math.sin(th[i]), zz)

    verts = [at(i, j) for j in range(rows + 1) for i in range(n)]
    faces = [(j * n + i, j * n + (i + 1) % n, (j + 1) * n + (i + 1) % n, (j + 1) * n + i)
             for j in range(rows) for i in range(n)]
    # The thong from the top of the side seam up to the belt, flat on the hip.
    top = at(0, rows)
    side_tree = _trees([ch.flesh])
    base = len(verts)
    tie = [top[2] - 0.02 + (0.995 - top[2] + 0.02) * k / 4 for k in range(5)]
    for z in tie:
        for dy in (-0.007, 0.0, 0.007):
            hit = _from_outside(side_tree, Vector((top[0] + 0.10, top[1] + dy, z)), Vector((-1.0, 0.0, 0.0)), 0.3)
            verts.append(((hit.x if hit is not None else top[0]) + offset + 0.003, top[1] + dy, z))
    faces += [(base + 3 * k + c, base + 3 * k + c + 1, base + 3 * (k + 1) + c + 1, base + 3 * (k + 1) + c)
              for k in range(4) for c in range(2)]
    m = len(verts)
    verts += [(-x, y, z) for x, y, z in verts]
    faces += [tuple(i + m for i in reversed(f)) for f in faces]
    sv, sf = _solid(verts, faces, thickness)
    obj = _authored(ch, "leggings", "leggings", sv, sf, _skin_groups(ch, verts) * 2, "_leggings", symmetric=True)
    if garter:
        zg = knee.z - 0.065
        gv = [at(i, 0, extra=thickness + 0.001, z=zg + dz) for dz in (-0.014, 0.0, 0.014) for i in range(n)]
        gf = [(j * n + i, j * n + (i + 1) % n, (j + 1) * n + (i + 1) % n, (j + 1) * n + i) for j in range(2) for i in range(n)]
        gv += [(-x, y, z) for x, y, z in gv]
        gf += [tuple(i + 3 * n for i in reversed(f)) for f in gf]
        sv, sf = _solid(gv, gf, 0.003)
        _authored(ch, "garters", garter, sv, sf, _skin_groups(ch, gv) * 2, "_leggings", symmetric=True)
    return obj, [at(0, j, extra=thickness) for j in range(rows + 1)]


def _seam_flap(ch, side, line, width=(0.044, 0.030), thickness=0.006):
    """The legging's side seam (#523): the two edges of the hide sewn together and left
    standing along the outer side of leg `side` (1 left, -1 right), a panel `thickness`
    thick from the tube's outer `line` (`_leggings`), `width` m wide at the ankle and at
    the top, its ends rounded off; every row moves with its point on the leg."""
    n = len(line)
    out = Vector((side * 0.975, 0.22, 0.0)).normalized()
    verts, groups = [], []
    rows = _skin_groups(ch, [(side * p[0], p[1], p[2]) for p in line])
    for j, p in enumerate(line):
        f = j / (n - 1)
        end = min(1.0, math.sqrt(min(f, 1.0 - f) / 0.06))
        w = (width[0] + (width[1] - width[0]) * f) * (0.4 + 0.6 * end)
        for k in (-0.005, 0.5 * w, w):
            verts.append((side * p[0] + out.x * k, p[1] + out.y * k, p[2]))
            groups.append(rows[j])
    faces = [(3 * j + c, 3 * j + c + 1, 3 * (j + 1) + c + 1, 3 * (j + 1) + c) for j in range(n - 1) for c in range(2)]
    sv, sf = _solid(verts, faces, thickness, centred=True)
    return _authored(ch, f"legging_flap{'_l' if side > 0 else '_r'}", "flaps", sv, sf, groups * 2, "_seam_flap")


def _belt(ch, off, over=(), n=40, z=(0.982, 1.006, 1.030), thickness=0.004):
    """The belt (#523): a closed band of hide round the hips, `off` m outside the skin and
    the layers `over`, bridging the hollow of the back, `thickness` thick."""
    from . import garments as gm

    trees = _trees([ch.flesh, *over])
    seg = am.segments(ch.arm)
    c = seg[ch.prefix + "hips"][0]
    verts = []
    for zz in z:
        ring = []
        for i in range(n):
            d = Vector((math.cos(2 * math.pi * i / n), math.sin(2 * math.pi * i / n), 0.0))
            hit = _from_outside(trees, Vector((c.x, c.y, zz)) + d * 0.32, -d, 0.32)
            ring.append((hit - Vector((c.x, c.y, zz))).length if hit is not None else 0.12)
        ring = gm.loosen(ring, passes=1, closed=True)
        ring = [0.5 * (ring[i] + ring[(n // 2 - i) % n]) for i in range(n)]  # left = right
        verts += [(c.x + (r + off) * math.cos(2 * math.pi * i / n), c.y + (r + off) * math.sin(2 * math.pi * i / n), zz)
                  for i, r in enumerate(ring)]
    faces = [(j * n + i, j * n + (i + 1) % n, (j + 1) * n + (i + 1) % n, (j + 1) * n + i)
             for j in range(len(z) - 1) for i in range(n)]
    sv, sf = _solid(verts, faces, thickness)
    return _authored(ch, "belt", "belt_hide", sv, sf, _skin_groups(ch, verts, sided=False) * 2, "_belt", symmetric=True)


def _breech_strip(ch, gap, over=(), cols=6, width=(0.20, 0.07), thickness=0.003):
    """The breechcloth between its flaps (#523): the strip from under the belt in front,
    down the belly, between the legs and up the seat to the belt behind, `gap` m off the
    skin and the layers `over`, `width` m wide at the belt and between the legs."""
    trees, skin = _trees([ch.flesh, *over]), _trees([ch.flesh])
    cz = _crotch_z(ch)
    zp = cz + 0.06
    zs = [1.030 + (zp - 1.030) * k / 8 for k in range(9)]
    f0 = _from_outside(skin, Vector((0.0, -0.6, zp)), Vector((0.0, 1.0, 0.0)))
    b0 = _from_outside(skin, Vector((0.0, 0.6, zp)), Vector((0.0, -1.0, 0.0)))
    yc = 0.5 * (f0.y + b0.y) if f0 is not None and b0 is not None else 0.0
    phis = [math.pi * k / 10 for k in range(1, 10)]
    path = [("f", z) for z in zs] + [("u", p) for p in phis] + [("b", z) for z in zs[::-1]]
    verts = []
    for kind, a in path:
        if kind == "u":
            w = width[1] + (width[0] - width[1]) * 0.25 * (1.0 - math.sin(a))
        else:
            w = width[0] + (width[1] + (width[0] - width[1]) * 0.25 - width[0]) * (1.030 - a) / (1.030 - zp)
        for c in range(cols + 1):
            x = -0.5 * w + w * c / cols
            if kind == "u":
                d = Vector((0.0, -math.cos(a), -math.sin(a)))
                o = Vector((x, yc, zp))
                r = _from_inside(skin, o, d, 0.4) or 0.08
                g = gap * (1.0 - 0.4 * math.sin(a))
                p = o + d * (r + g)
                verts.append((x, p.y, max(p.z, cz - g - 0.004)))
            else:
                s = -1.0 if kind == "f" else 1.0
                hit = _from_outside(trees, Vector((x, s * 0.6, a)), Vector((0.0, -s, 0.0)))
                verts.append((x, (hit.y if hit is not None else yc + s * 0.1) + s * gap, a))
    n = cols + 1
    faces = [(r * n + c, (r + 1) * n + c, (r + 1) * n + c + 1, r * n + c + 1) for r in range(len(path) - 1) for c in range(cols)]
    sv, sf = _solid(verts, faces, thickness)
    return _authored(ch, "breechcloth", "breechcloth", sv, sf, _skin_groups(ch, verts, sided=False) * 2, "_breech_strip",
                     symmetric=True)


def _apron(ch, name, z1, w0, w1, over, back=False, lean=0.04, clear=0.012, rows=18, cols=16, thickness=0.004):
    """A flap of the breechcloth (#523): the end of the strip come up under the belt, turned
    over its top and hanging free to the hem `z1`, `w0` m wide at the belt and `w1` at the
    hem. Each column hangs plumb (`garments.plumb`) from the furthest thing it lies on (the
    belt, the belly or seat, the legging tops: the layers `over` and the skin, `clear` m
    off), leaning `lean` m out at the hem, so a modest follow clears the thigh; skinned
    `garments.flap_follow` (one smooth blend across the width, less at the hem), with
    enough columns that the bend is a curve; shaded smooth."""
    from . import garments as gm

    P = ch.prefix
    trees = _trees([ch.flesh, *over])
    pivot = am.segments(ch.arm)[P + "thigh_l"][0].z
    s = 1.0 if back else -1.0
    zs = [1.040, 1.030] + [1.030 + (z1 - 1.030) * (r / rows) for r in range(1, rows + 1)]
    grid = []
    for c in range(cols + 1):
        u = c / cols - 0.5
        ys = []
        for k, z in enumerate(zs):
            x = (w0 + (w1 - w0) * (1.030 - z) / (1.030 - z1)) * u
            hit = _from_outside(trees, Vector((x, s * 0.6, min(z, 1.028))), Vector((0.0, -s, 0.0)))
            ys.append(None if hit is None else hit.y + s * (clear if k else -0.003))
        hang = gm.plumb(ys[1:], out=s)
        grid.append([ys[0] if ys[0] is not None else hang[0]] + hang)
    verts, groups = [], []
    for k, z in enumerate(zs):
        t = min(max((1.030 - z) / (1.030 - z1), 0.0), 1.0)
        for c in range(cols + 1):
            x = (w0 + (w1 - w0) * t) * (c / cols - 0.5)
            verts.append((x, grid[c][k] + s * lean * t * t, z))
            groups.append({P + b: w for b, w in gm.flap_follow(x, z, pivot).items() if w > 0.0})
    n = cols + 1
    faces = [(r * n + c, r * n + c + 1, (r + 1) * n + c + 1, (r + 1) * n + c) for r in range(len(zs) - 1) for c in range(cols)]
    if not back:
        faces = [f[::-1] for f in faces]
    sv, sf = _solid(verts, faces, thickness)
    return _authored(ch, name, "breechcloth", sv, sf, groups * 2, "_apron", smooth=2 * len(faces))


def _skin_cloth(ch, cuts, keep, ease, passes=6, floor=0.7):
    """Cloth cut out of the skin's own surface (#523): the flesh cut clean by the planes
    `cuts` [(point, normal towards the side cut away, region(co) -> bool the cut is confined
    to)], the faces of the bones `keep` kept, every vertex pushed out along its (smoothed) normal by
    `ease(co, groups)` m and the surface relaxed (`passes`), never closer to the skin than
    `floor` of its ease; rim vertices stay in their plane. Returns the cloth points, the skin
    points under them, the faces, the skin's weights per vertex and the rims {cut: [vertex,
    ...] in face order}."""
    cuts = [(co, no.normalized(), inside) for co, no, inside in cuts]
    bm = bmesh.new()
    bm.from_mesh(ch.flesh.data)
    n0 = len(bm.verts)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)  # the skin's split seams
    print(f"[{ch.name}] skin cloth: {n0} skin points, {n0 - len(bm.verts)} welded")
    for co, no, inside in cuts:
        fs = [f for f in bm.faces if inside(f.calc_center_median())]
        geom = list({x for f in fs for x in (f, *f.edges, *f.verts)})
        bmesh.ops.bisect_plane(bm, geom=geom, dist=1e-6, plane_co=co, plane_no=no)
        dead = [f for f in bm.faces if (f.calc_center_median() - co).dot(no) > 0 and inside(f.calc_center_median())]
        bmesh.ops.delete(bm, geom=dead, context="FACES")
    # Only the trunk and arms (a face goes with the bone that holds most of it).
    dl = bm.verts.layers.deform.verify()
    names = {g.index: g.name[len(ch.prefix):] for g in ch.flesh.vertex_groups}
    dead = []
    for f in bm.faces:
        tot = {}
        for v in f.verts:
            for gi, w in v[dl].items():
                tot[names.get(gi, "")] = tot.get(names.get(gi, ""), 0.0) + w
        if not tot or max(tot, key=tot.get) not in keep:
            dead.append(f)
    bmesh.ops.delete(bm, geom=dead, context="FACES")
    bm.verts.index_update()
    bm.normal_update()
    vs = list(bm.verts)
    org = [v.co.copy() for v in vs]
    nrm = [v.normal.copy() for v in vs]
    nb = [[e.other_vert(v).index for e in v.link_edges] for v in vs]
    rim, nxt = {}, {}
    for e in bm.edges:
        if len(e.link_faces) == 1:
            for v in e.verts:
                rim[v.index] = next((k for k, (co, no, _i) in enumerate(cuts)
                                     if abs((v.co - co).dot(no.normalized())) < 1e-4), -1)
            lp = e.link_loops[0]
            nxt[lp.vert.index] = lp.link_loop_next.vert.index
    faces = [tuple(v.index for v in f.verts) for f in bm.faces]
    bm.free()
    dirs = nrm
    for _ in range(3):
        dirs = [(dirs[i] * 2.0 + sum((dirs[j] for j in nb[i]), Vector()) / max(len(nb[i]), 1)).normalized()
                for i in range(len(vs))]
    for i, k in rim.items():
        if k >= 0:
            no = cuts[k][1].normalized()
            d = dirs[i] - no * dirs[i].dot(no)
            dirs[i] = d.normalized() if d.length > 1e-6 else dirs[i]
    groups = _skin_groups(ch, org)
    es = [ease(org[i], groups[i]) for i in range(len(vs))]
    pos = [org[i] + dirs[i] * es[i] for i in range(len(vs))]
    for _ in range(passes):
        pos = [pos[i] if i in rim or not nb[i] else pos[i] * 0.5 + sum((pos[j] for j in nb[i]), Vector()) * (0.5 / len(nb[i]))
               for i in range(len(vs))]
    for i in range(len(vs)):
        if i not in rim:
            d = (pos[i] - org[i]).dot(nrm[i])
            if d < floor * es[i]:
                pos[i] = pos[i] + nrm[i] * (floor * es[i] - d)
    loops, seen = {}, set()
    for a in sorted(nxt):
        if a in seen:
            continue
        chain, v = [], a
        while v is not None and v not in seen:
            seen.add(v)
            chain.append(v)
            v = nxt.get(v)
        if len(chain) > len(loops.get(rim.get(a, -1), ())):
            loops[rim.get(a, -1)] = chain
    return pos, org, faces, groups, loops


def _shirt(ch, name="shirt", key="shirt", hem=0.76, offset=0.012, thickness=0.003, step=0.025, cuff=0.035, waist=1.02,
           ease=(0.010, 0.036, 0.013)):
    """A shirt that follows the body (#523): above the `waist` it is the skin's own surface
    (`_skin_cloth`) cut clean at the neck and `cuff` m above the wrists and pushed out by an
    ease that varies smoothly, `ease` m at (shoulders and chest, the waist, the sleeves), so
    the shoulders are round and the sleeves run into the body without a step; below the
    waist the skirt hangs straight from that edge to the `hem`, pushed out only by what is
    under it (`offset` m off the legwear already there). `thickness` thick with closed rims
    at hem, cuffs and neck, shaded smooth. Returns the object and the hem's outer ring with
    its weights [(point, groups)] (for a cut fringe)."""
    from . import garments as gm

    P = ch.prefix
    seg = am.segments(ch.arm)
    neck, sh = seg[P + "neck"][0], seg[P + "upperarm_l"][0]
    up = Vector((0.0, 0.0, 1.0))
    cuts = [(Vector((0.0, 0.0, waist)), -up, lambda c: abs(c.x) < 0.24 and c.z < waist + 0.10),
            (Vector((0.0, 0.0, neck.z + 0.03)), up,
             lambda c: c.z > neck.z - 0.02 and c.x * c.x + (c.y - neck.y) ** 2 < 0.15 ** 2)]
    for sd in "lr":
        el, wr = seg[P + f"forearm_{sd}"]
        ax = (wr - el).normalized()
        cuts.append((wr - ax * cuff, ax, lambda c, el=el, ax=ax, ln=(wr - el).length:
                     (c - el).dot(ax) > 0.4 * ln and ((c - el) - ax * (c - el).dot(ax)).length < 0.11))
    chest = sh.z - 0.10

    def ease_at(c, g):
        a = min(sum(w for b, w in g.items() if b[len(P):].split("_")[0] in ("upperarm", "forearm", "hand")), 1.0)
        t = min(max((c.z - waist) / max(chest - waist, 1e-6), 0.0), 1.0)
        t = t * t * (3.0 - 2.0 * t)
        return (ease[1] + (ease[0] - ease[1]) * t) * (1.0 - a) + ease[2] * a

    pos, org, faces, groups, loops = _skin_cloth(ch, cuts, TORSO | ARMS | {"neck"}, ease_at)
    verts, wpts = [tuple(p) for p in pos], [tuple(p) for p in org]
    # The skirt: every vertex of the waist edge hangs plumb to the hem, pushed out by what is
    # under it (never in again), the rings loosened round.
    loop = loops.get(0, [])
    under = _trees([ch.flesh] + [o for o in ch.layers["clothes"] if o.type == "MESH" and o is not getattr(ch, "shoes", None)
                                 and ch.keys.get(o.name) != "flaps"])
    rows = max(int(math.ceil((waist - hem) / step)), 2)
    cy = sum(pos[i].y for i in loop) / max(len(loop), 1)
    dirs, cur = [], []
    for i in loop:
        d = Vector((pos[i].x, pos[i].y - cy, 0.0))
        cur.append(d.length)
        dirs.append(d.normalized())
    L, base = len(loop), len(verts)
    for j in range(1, rows + 1):
        z = waist + (hem - waist) * j / rows
        c = Vector((0.0, cy, z))
        for i, d in enumerate(dirs):
            hit = _from_outside(under, c + d * 0.32, -d, 0.32)
            if hit is not None:
                cur[i] = max(cur[i], (hit - c).length + offset)
        cur = gm.loosen(cur, passes=2, closed=True)
        ring = [(c.x + d.x * r, c.y + d.y * r, z) for d, r in zip(dirs, cur)]
        verts += ring
        wpts += ring
    idx = lambda j, i: loop[i % L] if j == 0 else base + (j - 1) * L + i % L  # noqa: E731
    faces = faces + [(idx(j, i + 1), idx(j, i), idx(j + 1, i), idx(j + 1, i + 1)) for j in range(rows) for i in range(L)]
    # The skirt hangs free: from the skin's weights at the waist to the flaps' own field
    # (`garments.flap_follow`) at the hip joint, so skirt and flap move alike in the stride.
    pivot = seg[P + "thigh_l"][0].z
    for p, g in zip(wpts[base:], _skin_groups(ch, wpts[base:])):
        u = min(max((waist - p[2]) / max(waist - pivot, 1e-6), 0.0), 1.0)
        w = {b: x * (1.0 - u) for b, x in g.items()}
        for b, x in gm.flap_follow(p[0], p[2], pivot).items():
            w[P + b] = w.get(P + b, 0.0) + x * u
        w = dict(sorted(w.items(), key=lambda kv: -kv[1])[:4])
        tot = sum(w.values()) or 1.0
        groups.append({b: x / tot for b, x in w.items() if x > 0.0})
    sv, sfc = _solid(verts, faces, thickness)
    obj = _authored(ch, name, key, sv, sfc, groups * 2, "_shirt", symmetric=True, smooth=2 * len(faces))
    zs = [v[2] for v in verts]
    print(f"[{ch.name}] {name}: {len(verts)} points, waist ring {L}, z {min(zs):.3f}..{max(zs):.3f}, rims {sorted(loops)}")
    ch.shirt_ease = lambda c, g: ease_at(c, g) + thickness
    k = len(verts)
    return obj, [(Vector(sv[k + base + (rows - 1) * L + i]), groups[base + (rows - 1) * L + i]) for i in range(L)]


def _matchcoat(ch, over, thickness=0.006, gap=0.016):
    """A matchcoat as one cloth (#523): a blanket over the left shoulder and upper arm and
    under the right arm, the skin's surface (`_skin_cloth`) cut clean along the diagonal
    from the right armpit to the right of the neck, along its slanted lower edge above the
    belt, round the neck and above the left elbow, `gap` m outside the shirt under it
    (`over(co, groups)`: that layer's ease), `thickness` thick with closed rims."""
    P = ch.prefix
    seg = am.segments(ch.arm)
    neck, sh = seg[P + "neck"][0], seg[P + "upperarm_l"][0]
    el = seg[P + "upperarm_l"][1]
    up = Vector((0.0, 0.0, 1.0))
    pit, by = Vector((-sh.x + 0.02, 0.0, sh.z - 0.10)), Vector((-0.07, 0.0, neck.z + 0.03))
    along = (by - pit).normalized()
    diag = Vector((-along.z, 0.0, along.x))
    if diag.x > 0:
        diag = -diag  # towards the right shoulder, which stays bare
    ax = (el - sh).normalized()
    ln = (el - sh).length
    cuts = [(pit, diag, lambda c: c.z > pit.z - 0.03 and c.x < 0.12),
            (Vector((0.0, 0.0, neck.z + 0.03)), up,
             lambda c: c.z > neck.z - 0.02 and c.x * c.x + (c.y - neck.y) ** 2 < 0.15 ** 2),
            (Vector((0.18, 0.0, 1.05)), Vector((-0.5, 0.0, -1.0)), lambda c: abs(c.x) < 0.26 and c.z < sh.z - 0.06),
            (sh + ax * (0.78 * ln), ax, lambda c: (c - sh).dot(ax) > 0.4 * ln and ((c - sh) - ax * (c - sh).dot(ax)).length < 0.12)]
    pos, org, faces, groups, loops = _skin_cloth(ch, cuts, TORSO | {"upperarm_l"}, lambda c, g: over(c, g) + gap, floor=0.9)
    sv, sf = _solid([tuple(p) for p in pos], faces, thickness)
    print(f"[{ch.name}] matchcoat: {len(pos)} points, rims {sorted(loops)}")
    return _authored(ch, "matchcoat", "matchcoat", sv, sf, groups * 2, "_matchcoat", smooth=2 * len(faces))


def dress(ch, s):
    """The garments of SPEC `s`: authored legwear (#523), shells and modelled panels (the
    clothes layer)."""
    name, d = ch.name, s["dress"]
    if "moccasins" in d:
        ch.shoes = ch.add("clothes", mbody.shoes(ch.arm, ch.flesh, f"{name}.footwear", ch.prefix, offset=0.005, sole=0.006,
                                                 heel=0.002, key="moccasins"), "moccasins", symmetric=True)
        ch.shoes["acr_material_keys"] = ["moccasins", "moccasins"]  # a tie, not a brass buckle
    if "leggings" in d and "leggings" in s["drape"]:
        # Drafted side-seam leggings with their garters, draped on the legs and moccasins;
        # the flap stands out over the draped cloth.
        from . import drape

        ch.drape_report = getattr(ch, "drape_report", {})
        made = drape.dress(ch, garments=("nat_leggings",), report=ch.drape_report)
        drape.hide_skin(ch, [o for o in ch.layers["clothes"] if o.name in made])
        leg = next(o for o in ch.layers["clothes"] if o.name in made and made[o.name] == "leggings")
        ch.legging_cloth = [o for o in ch.layers["clothes"] if o.name in made]
        for side in (1.0, -1.0):
            ch.add("clothes", _legging_flap(ch, side, surface=leg), "flaps")
    elif "leggings" in d:
        # Authored (#523): two tubes with a slanted top tied to the belt, the garters round
        # them, and the side seam standing out as a panel.
        _leg, line = _leggings(ch, garter="garter")
        for side in (1.0, -1.0):
            _seam_flap(ch, side, line)
    if "shirt" in d:  # `trade_shirt`: drafted and draped over the rest (below)
        _shirt(ch)  # authored (#523): body, yoke and sleeves with a clean hem, cuffs and neck
    belt_off = 0.024 if "shirt" in d else 0.012
    if "breechcloth" in d and "breechcloth" in s["drape"]:
        # One drafted strip between the legs and over the belt, its ends the flaps (#307).
        from . import drape

        ch.shell("belt", "belt_hide", lambda c, n, h, p: h in {"hips", "spine"} and 0.98 < c.z < 1.03, belt_off + 0.004)
        over = [o for o in ch.layers["clothes"] if o.type == "MESH" and o is not getattr(ch, "shoes", None)]
        ch.drape_report = getattr(ch, "drape_report", {})
        made = drape.dress(ch, garments=("nat_breechcloth",), over=over, report=ch.drape_report)
        drape.hide_skin(ch, [o for o in ch.layers["clothes"] if o.name in made])
        for o in ch.layers["clothes"]:
            if o.name in made:
                _flap_weights(ch, o)
    elif "breechcloth" in d:
        # Authored (#523): the strip between the legs, the belt as a closed band over it,
        # and its two ends turned over the belt and hanging free (the back a little longer),
        # each laid on what is already there (skin, leggings, shirt, belt).
        under = [o for o in ch.layers["clothes"] if o.type == "MESH" and o is not getattr(ch, "shoes", None)]
        under.append(_breech_strip(ch, belt_off, over=[o for o in under if ch.keys.get(o.name) == "shirt"]))
        under.append(_belt(ch, belt_off + 0.004, over=[o for o in under if ch.keys.get(o.name) == "shirt"]))
        _apron(ch, "flap_front", 0.60, 0.22, 0.25, under)
        _apron(ch, "flap_back", 0.56, 0.24, 0.27, under, back=True)
    if "matchcoat" in d:
        # A blanket over the left shoulder and under the right arm: the trunk above a line
        # from the right hip to the left armpit, and the left upper arm; over the left hip
        # unless the breechcloth is draped (then it stays above the belt and the flaps).
        if "shirt" in d:  # authored (#523): one clean-edged cloth with a thickness over the shirt
            _matchcoat(ch, ch.shirt_ease)
            d = tuple(x for x in d if x != "matchcoat")
    if "matchcoat" in d:
        low = 0.06 if "breechcloth" in s["drape"] else 0.0
        hip = not low
        ch.shell("matchcoat", "matchcoat", lambda c, n, h, p: (h in TORSO and c.z > 1.02 + low + 0.9 * (0.18 - c.x) * (c.z < 1.6)
                                                                and c.z < 1.52 and not (c.x < -0.12 and c.z > 1.25))
                 or (h == "upperarm_l") or (hip and h in {"thigh_l"} and c.z > 0.70 and c.x > 0.0), 0.030)
    if "cloak" in d:
        # The shaman's fur cloak: a pelt over both shoulders and down the back to the
        # seat, open in front below the collarbones; over the upper arms to mid-arm.
        ch.shell("cloak", "pelt", lambda c, n, h, p: (h in TORSO and 0.92 < c.z < 1.56
                                                       and (c.y > -0.02 or c.z > 1.40 or abs(c.x) > 0.16))
                 or (h in {"upperarm_l", "upperarm_r"} and c.z > 1.22), 0.034)
    if "trade_shirt" in d:
        # The linen trade shirt drafted and draped (#287 drape pilot) over the breechcloth,
        # belt and flaps, which it collides with and takes its weights from.
        from . import drape

        over = [o for o in ch.layers["clothes"] if o.type == "MESH" and o is not getattr(ch, "shoes", None)]
        ch.drape_report = getattr(ch, "drape_report", {})
        made = drape.dress(ch, garments=("trade_shirt",), over=over, report=ch.drape_report)
        drape.hide_skin(ch, [o for o in ch.layers["clothes"] if o.name in made])
    if s.get("silver"):
        ch.shell("armbands", "silver", lambda c, n, h, p: h in {"upperarm_l", "upperarm_r"}
                 and abs(c.z - (ch.arm.data.bones[ch.prefix + "upperarm_l"].head_local.z - 0.12)) < 0.018, 0.021 if "shirt" in d else 0.010)


# -- hair and feathers ------------------------------------------------------------------

def _card(bm, base, up, side, length, width, taper=0.35, bend=None):
    """A flat card (feather, hair lock) from `base` along `up`, `width` across `side`."""
    up, side = up.normalized(), side.normalized()
    pts = []
    for t in (0.0, 0.5, 1.0):
        w = width * (1.0 - (1.0 - taper) * t) * (0.6 if t == 0 else 1.0)
        c = base + up * (length * t) + ((bend or Vector()) * (t * t))
        pts.append((bm.verts.new(c - side * w / 2), bm.verts.new(c + side * w / 2)))
    for (a, b), (c, d) in zip(pts, pts[1:]):
        bm.faces.new((a, b, d, c))


def _box(bm, centre, size, rot=None):
    m = Matrix.Translation(centre) @ (rot or Matrix.Identity(4)) @ Matrix.Diagonal((*size, 1.0))
    bmesh.ops.create_cube(bm, size=1.0, matrix=m)


def _skull(ch):
    """The crown point (socket.hat in the rest pose) and the head's centre."""
    crown = ch.arm.data.bones[ch.prefix + "socket.hat"].head_local.copy()
    head = ch.arm.data.bones[ch.prefix + "head"].head_local.copy()
    return crown, head


def hair(ch, s):
    """The roach, scalp lock or long hair and the feathers (rigid on the head)."""
    kind, n_feathers = s["hair"], s["feathers"]
    crown, head = _skull(ch)
    centre = Vector((0.0, head.y + 0.01, (crown.z + head.z) / 2 + 0.01))
    r = crown.z - centre.z
    made = []
    if kind == "roach":
        # A crest of stiff, dyed deer hair from the crown to the nape, in cards.
        bm = bmesh.new()
        for i in range(9):
            a = math.radians(-20 + i * 14)  # 0 at the top, back is positive (+Y)
            base = centre + Vector((0, math.sin(a) * r * 0.98, math.cos(a) * r * 0.98))
            normal = (base - centre).normalized()
            h = 0.075 + 0.025 * math.sin(math.pi * i / 8)
            _card(bm, base - Vector((0.012, 0, 0)), normal + Vector((0.18, 0, 0)), Vector((0, 1, 0)), h, 0.045, taper=0.5)
            _card(bm, base + Vector((0.012, 0, 0)), normal + Vector((-0.18, 0, 0)), Vector((0, 1, 0)), h, 0.045, taper=0.5)
        made.append(("roach", gear.from_bm(bm, f"{ch.name}.roach")))
    if kind in ("roach", "lock"):
        # The scalp lock: a tuft at the crown and a braid down the back of the head.
        bm = bmesh.new()
        top = centre + Vector((0, 0.035, r * 0.98))
        for k in range(5):
            a = 2 * math.pi * k / 5
            _card(bm, top, Vector((math.sin(a) * 0.6, 0.6 + math.cos(a) * 0.4, 0.5)), Vector((math.cos(a), -math.sin(a), 0)),
                  0.07, 0.03, bend=Vector((0, 0.02, -0.03)))
        _box(bm, top + Vector((0, 0.07, -0.12)), (0.022, 0.018, 0.22), Matrix.Rotation(math.radians(-20), 4, "X"))
        made.append(("hair", gear.from_bm(bm, f"{ch.name}.scalp_lock")))
    elif kind == "long":
        bm = bmesh.new()
        _box(bm, Vector((0, centre.y + 0.07, centre.z - 0.10)), (0.17, 0.05, 0.30))
        made.append(("hair", gear.from_bm(bm, f"{ch.name}.long_hair")))
    if n_feathers:
        bm = bmesh.new()
        top = centre + Vector((0, 0.05, r * 0.95))
        for k in range(n_feathers):
            dx = (k - (n_feathers - 1) / 2) * 0.03
            lean = Vector((dx * 4, 0.9, 0.35 + 0.2 * (k % 2)))
            _card(bm, top + Vector((dx, 0, 0)), lean, Vector((1, 0, 0)), 0.20, 0.035, taper=0.3,
                  bend=Vector((0, 0.03, -0.04)))
        made.append(("feather", gear.from_bm(bm, f"{ch.name}.feathers")))
    head_kind = s.get("headdress")
    if head_kind:
        # `gustoweh` (Iroquois): a fitted cap on a splint band, short feathers over the
        # crown and one eagle feather standing up; `crown` (Huron, Delaware): a band with a
        # ring of upright feathers. Band at the brow, rigid on the head.
        bm = bmesh.new()
        segs, rb, zb = 12, r * 1.02 + 0.012, centre.z + 0.01
        for k in range(segs):
            a0, a1 = 2 * math.pi * k / segs, 2 * math.pi * (k + 1) / segs
            for rr, flip in ((rb, False), (rb - 0.004, True)):  # outer and inner face of the band
                ring = [bm.verts.new((centre.x + math.sin(a) * rr, centre.y - math.cos(a) * rr * 1.08, zb + dz))
                        for a in (a0, a1) for dz in (-0.025, 0.030)]
                q = (ring[0], ring[2], ring[3], ring[1])
                bm.faces.new(q[::-1] if flip else q)
        if head_kind == "gustoweh":
            for k in range(10):  # short feathers lying back over the crown
                a = 2 * math.pi * k / 10
                base = centre + Vector((math.sin(a) * r * 0.7, -math.cos(a) * r * 0.7, r * 0.75))
                _card(bm, base, Vector((math.sin(a) * 0.5, 0.6 - math.cos(a) * 0.4, 0.5)), Vector((math.cos(a), math.sin(a), 0)),
                      0.07, 0.03, taper=0.4)
        made.append(("gustoweh", gear.from_bm(bm, f"{ch.name}.headband")))
        bm = bmesh.new()
        if head_kind == "gustoweh":
            _card(bm, centre + Vector((0, 0.01, r * 0.98)), Vector((0, 0.15, 1)), Vector((1, 0, 0)), 0.26, 0.04, taper=0.3,
                  bend=Vector((0, 0.05, 0)))
        else:
            for k in range(11):
                a = math.radians(-100 + 200 * k / 10)
                base = Vector((centre.x + math.sin(a) * rb, centre.y - math.cos(a) * rb * 1.08, zb + 0.02))
                out = Vector((math.sin(a) * 0.25, -math.cos(a) * 0.25, 1.0))
                _card(bm, base, out, Vector((math.cos(a), math.sin(a), 0)), 0.20 + 0.04 * math.sin(math.pi * k / 10), 0.035,
                      taper=0.3)
        made.append(("feather", gear.from_bm(bm, f"{ch.name}.headdress")))
    for key, o in made:
        gear.rigid(o, ch.arm, ch.prefix + "head", Matrix.Identity(4), part=1)
        ch.add("gear", o, key)
    return made


# -- strapped gear ----------------------------------------------------------------------

def _horn(name, length=0.30):
    """A powder horn: a curved cone with a wooden plug, origin at its middle, along +X."""
    bm = bmesh.new()
    rings, segs = 7, 8
    vs = []
    for i in range(rings + 1):
        t = i / rings
        r = 0.035 * (1 - t) + 0.008 * t
        cx, cz = (t - 0.5) * length, -0.06 * (t - 0.5) ** 2 * 4
        vs.append([bm.verts.new((cx, r * math.cos(2 * math.pi * k / segs), cz + r * math.sin(2 * math.pi * k / segs)))
                   for k in range(segs)])
    for a, b in zip(vs, vs[1:]):
        for k in range(segs):
            bm.faces.new((a[k], a[(k + 1) % segs], b[(k + 1) % segs], b[k]))
    bm.faces.new(vs[0][::-1])
    bm.faces.new(vs[-1])
    return gear.from_bm(bm, name)


# The quiver's strap: over the right shoulder, across the chest to the left hip (XZ line,
# `straps.carry`), along the quiver on the back.
QUIVER_STRAP = ((-0.10, 1.47), (0.16, 1.00))


def kit(ch, s):
    """Strapped gear (rigid on deforming bones): quiver on the back, powder horn and shot
    pouch at the right hip under the strap, knife at the belt."""
    name, k = ch.name, s["kit"]
    out = {}
    if "quiver" in k:
        # Smaller than `held.quiver`'s default (slice 1's read as big as the trunk): 48 cm,
        # 8 cm across.
        q = gear.held(held.quiver(arrows=6, length=0.48), f"{name}.quiver")
        shrink = Matrix.Diagonal((0.72, 0.72, 0.86, 1.0))
        q.data.transform(shrink)
        mouth = shrink.to_3x3() @ Vector(next(e for e in q.children if e.name.endswith("socket.mouth")).location)
        rot = Matrix.Rotation(math.radians(-28), 4, "Y") @ Matrix.Rotation(math.radians(12), 4, "X")
        at = Vector(cl.QUIVER_MOUTH) - (rot.to_3x3() @ mouth)
        for e in list(q.children):
            import bpy

            bpy.data.objects.remove(e)
        # On its strap (over the right shoulder, across the chest to the left hip), seated on
        # the back and skinned to the blended back frame (`gear.seat`).
        straps.carry(ch, f"{name}.quiver_strap", QUIVER_STRAP[0], QUIVER_STRAP[1], key="hide", samples=48)
        gear.seat(ch, q, Matrix.Translation(at) @ rot, (0.0, -1.0, 0.0))
        out["quiver"] = ch.add("gear", q, "hide")
    if "horn" in k:
        h = _horn(f"{name}.powder_horn")
        h["acr_material_keys"] = ["horn"]
        gear.seat(ch, h, Matrix.Translation((-0.17, 0.02, 0.98)) @ Matrix.Rotation(math.radians(-70), 4, "Y")
                  @ Matrix.Rotation(math.radians(90), 4, "Z"), (1.0, -0.1, 0.0), turn=False)
        out["horn"] = ch.add("gear", h, "horn")
    if "pouch" in k:
        p = gear.box(f"{name}.shot_pouch", (0.13, 0.04, 0.12))
        gear.seat(ch, p, Matrix.Translation((-0.15, -0.05, 0.90)) @ Matrix.Rotation(math.radians(25), 4, "Z"), (0.95, 0.3, 0.0))
        out["pouch"] = ch.add("gear", p, "pouch")
    if "knife" in k:
        kn = gear.box(f"{name}.knife_sheath", (0.035, 0.022, 0.20), bevel=0.4)
        for v in kn.data.vertices:
            if v.co.z < 0:
                v.co.x *= 0.45
        gear.rigid(kn, ch.arm, ch.prefix + "hips", Matrix.Translation((0.09, -0.11, 0.93)) @ Matrix.Rotation(math.radians(-12), 4, "Y"))
        out["knife"] = ch.add("gear", kn, "pouch")
    if s.get("silver"):
        # A crescent gorget of trade silver on the breast.
        bm = bmesh.new()
        segs = 9
        outer = [bm.verts.new((0.075 * math.cos(a), 0, -0.045 * math.sin(a))) for a in
                 (math.pi * i / segs for i in range(segs + 1))]
        inner = [bm.verts.new((0.05 * math.cos(a), 0, -0.018 * math.sin(a))) for a in
                 (math.pi * i / segs for i in range(segs + 1))]
        for i in range(segs):
            bm.faces.new((outer[i], outer[i + 1], inner[i + 1], inner[i]))
        g = gear.from_bm(bm, f"{name}.gorget")
        gear.rigid(g, ch.arm, ch.prefix + "spine2", Matrix.Translation((0, -0.135, 1.36)) @ Matrix.Rotation(math.radians(12), 4, "X"))
        out["gorget"] = ch.add("gear", g, "silver")
    return out


# -- character --------------------------------------------------------------------------

def _rattle(name):
    """A turtle-shell rattle: the shell (a flattened dome over a flat plastron) on a
    wooden handle (the turtle's stretched neck). Origin at the grip, +Z up the handle."""
    bm = bmesh.new()
    _box(bm, Vector((0, 0, 0.04)), (0.018, 0.018, 0.16))
    ret = bmesh.ops.create_uvsphere(bm, u_segments=10, v_segments=6, radius=1.0)
    sv = [v for v in ret["verts"]]
    for v in sv:
        v.co = Vector((v.co.x * 0.065, v.co.y * (0.030 if v.co.y < 0 else 0.012), v.co.z * 0.085 + 0.18))
    return gear.from_bm(bm, name)


def club_socket(table, prefix=""):
    """The skeleton with a second right-hand socket `socket.club_r` (the archer's club,
    shown only in the melee clips; the arrow's `socket.hand_r` is hidden then)."""
    hs = next(b for b in table if b.name == prefix + "socket.hand_r")
    return table + [rig.Bone(prefix + "socket.club_r", hs.parent, hs.head, hs.tail, False)]


def make(root, name, s):
    """The warrior of SPEC `s` under `root`: body, dress, hair, strapped and held gear."""
    two = s["weapon"] == "bow" and bool(s["melee"])
    table = club_socket(rig.skeleton_h("", (0, 0, 0))) if two else None
    ch = Character(root, name, s["build"], table=table)
    w = s["weapon"]
    grip = held.fists("arrow", "longbow") if w == "bow" else         held.fists({"gun": "musket", "club": "war_club"}.get(w, w))
    ch.body(shoulders="trade_shirt" not in s["dress"], grip=grip)  # draped cloth tears over the re-meshed shoulders
    dress(ch, s)
    if "belt_l" in s["kit"]:
        ch.strap_surface, ch.strap_offsets, ch.strap_under = ch.flesh, (0.026 if "shirt" in s["dress"] else 0.010,) * 2, None
        registry.build_gear(["belt_l"], ch, name, s)
    ch.bone_layer()
    ch.caps()
    hair(ch, s)
    kit(ch, s)
    w = s["weapon"]
    if w == "bow":
        gear.hold(ch, "longbow", f"{name}.bow", "socket.hand_l", slot="bow")
        gear.hold(ch, "arrow", f"{name}.arrow", "socket.hand_r", slot="arrow")
        if two:
            # The archer's melee weapon (`.MD` index 0, MECH): a war club or, with
            # `melee` "tomahawk", a tomahawk, in its own socket (`club_socket`).
            item = "tomahawk" if s["melee"] == "tomahawk" else "war_club"
            gear.hold(ch, item, f"{name}.{item}", "socket.club_r", slot="club")
    elif w == "gun":
        am.pose(ch.arm, poses.STAND_MUSKET)
        gun = gear.musket(f"{name}.musket", length=s["gun_len"], bayonet=0.0)
        gun["acr_material_keys"] = ["wood", "metal"]
        am.attach(gun, ch.arm, "socket.hand_r", grips.hold_matrix(ch.arm, "socket.hand_r", (0.0, 0.01, 0.0)))
        ch.held["musket"] = gun
        ch.keys[gun.name] = "wood"
        am.pose(ch.arm, {})
    elif w in ("tomahawk", "club", "spear"):
        item = {"tomahawk": "tomahawk", "club": "war_club", "spear": "spear"}[w]
        gear.hold(ch, item, f"{name}.{item}", "socket.hand_r", slot=w)
    else:
        raise ValueError(f"native weapon {w!r}")
    if s.get("rattle"):
        r = _rattle(f"{name}.rattle")
        am.attach(r, ch.arm, "socket.hand_l", grips.hold_matrix(ch.arm, "socket.hand_l"))
        ch.held["rattle"] = r
        ch.keys[r.name] = "turtle"
    ch.paint_masks = body_paint(s["paint"], s["palette"]) if s["paint"] else []
    return ch


def stand_of(s):
    return {"bow": cl.bow_ready(), "spear": STAND_SPEAR, "gun": poses.STAND_MUSKET}.get(s["weapon"], STAND_ARMED)


def unit_clips(s):
    """{clip: [(frame, pose)]} for the non-gun units: stand, walk (and the `0` variants),
    the weapon clips by index (`clips.unit_clips`), death."""
    md = s["md"]
    stand = stand_of(s)
    walk = STAND if s["weapon"] == "bow" else stand
    lie = poses.lie_prone()
    keys = {
        "STAND": [(0.0, stand)],
        # Every MOTION_* line: a run on the 8-frame running sprites, a walk on 10 (#295).
        **poses.step_keys(md, walk),
        "DEATH": poses.death_keys(cl.bare(stand), lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    for k in ("PSTAND", "PSTAND0", "PSTAND1", "PSTAND2", "PATTACK", "UATTACK", "PATTACK0", "UATTACK0", "PATTACK1",
              "UATTACK1", "PATTACK2", "UATTACK2"):
        keys[k] = [(0.0, stand)]
    if s["work"]:
        # The peasant's work (DelKri, as EngKri): REST, a look round; WORK, a two-handed
        # swing (hoe, axe) from over the head down to the ground in front.
        up = cl._with(STAND, upperarm_l=(-150, 10, 0), upperarm_r=(-150, -10, 0), forearm_l=(-20, 0, 0),
                      forearm_r=(-20, 0, 0), spine=(-8, 0, 0))
        down = cl._with(STAND, upperarm_l=(-50, 30, 0), upperarm_r=(-50, -30, 0), forearm_l=(-10, 0, 0),
                        forearm_r=(-10, 0, 0), spine=(30, 0, 0), spine1=(10, 0, 0), thigh_l=(-10, -4, 0),
                        thigh_r=(-10, 4, 0), shin_l=(15, 0, 0), shin_r=(15, 0, 0))
        look = cl._with(STAND, head=(14, 0, 8), spine=(2, 0, 4), upperarm_l=(-2, 30, 0), upperarm_r=(-2, -30, 0))
        keys["WORK"] = [(0.0, STAND), (0.45, up), (0.65, down), (1.0, STAND)]
        keys["REST"] = [(0.0, STAND), (0.4, look), (0.7, look), (1.0, STAND)]
    out = {k: rig.clip_keys(rig.md_clip_frames(md[k])[0], v) for k, v in keys.items() if k in md}
    for k in ("DEATHLIE2", "DEATHLIE3"):
        if k in md:
            out[k] = out["DEATHLIE1"]
    weapons = cl.unit_clips(md, s["weapons"], melee=s["melee"], active=s["active"],
                            thrown="tomahawk")
    if s["melee"] in ("club", "tomahawk") and "ATTACK" in md:
        down = club_down(cl.md_frames(md["ATTACK"]))
        weapons["ATTACK_CUT_DOWN"] = weapons["ATTACK_DOWN"] = down
    melee = {k for k in weapons if k == "ATTACK" or k.startswith("ATTACK_")}
    out.update({k: list(v) for k, v in weapons.items()})
    if s["weapon"] == "bow" and s["melee"]:
        # The archer strikes with his club (`socket.club_r`), the arrow hidden; every other
        # clip hides the club. Every key carries both scales.
        memo = {}

        def shown(pose, club):
            k = (id(pose), club)
            if k not in memo:
                show = dict(pose.get("show") or {}, **{"socket.club_r": club})
                world = dict(pose.get("world") or {})
                if club:
                    show["socket.hand_r"] = 0
                    # The strike clips turn `socket.hand_r`: the club's socket turns with it
                    # (m2-shared-moves: it stayed in the hand's rest frame, 0.75 m off).
                    if "socket.hand_r" in world:
                        world["socket.club_r"] = world["socket.hand_r"]
                show.setdefault("socket.hand_r", 1)
                memo[k] = dict(pose, show=show, world=world)
            return memo[k]

        out = {name: [(f, shown(p, 1 if name in melee else 0)) for f, p in keys_] for name, keys_ in out.items()}
    return out


def build(ctx, s):
    """Builds, animates, paints and prepares for export the warrior of SPEC `s`."""
    md = s["md"]
    ch = make(ctx.root, ctx.name, s)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    names = []
    pole = s["weapon"] == "spear" and s["melee"] == "spear"
    if s["weapon"] == "gun":
        rod = gear.rammer(f"{ctx.name}.rammer")
        am.attach(rod, ch.arm, aiming.ROD_SOCKET, am.world(ch.arm, aiming.ROD_SOCKET))
        ch.held["rammer"] = rod
        ch.keys[rod.name] = "metal"
        aim, hip, present = aiming.fire_poses(ch)  # from the shoulder and the hip (#296)
        ctx.aim_errors = aiming.errors(ch.arm, aim)
        infos = {}
        reload = aiming.drill(ch.arm, poses.RELOAD_STEPS, infos)
        charge = aiming.hold(ch.arm, poses.CHARGE_MUSKET, pocket=(0.02, -0.12, -0.32), left_along=0.30)
        clips = poses.musketeer_clips(md, aim=aim, charge=charge, reload=reload, hip=hip)
        # The native riflemen run on their 8-frame sprites like the rest of the foot (#295).
        for k, ks in poses.step_keys(md, poses.STAND_MUSKET).items():
            clips[k] = rig.clip_keys(rig.md_clip_frames(md[k])[0], ks)
        clips = {k: v for k, v in clips.items() if k in md or k.startswith(("ATTACK_", "FIRE_"))}
        # The hands on the musket and the rammer on every frame between two loading steps (#255).
        rods = poses.rod_phases(md)
        held, plan = aiming.solve_holds(ch.arm, {k: clips[k] for k in rods if k in clips}, reload, infos)
        clips.update(held)
        for clip, keys in clips.items():
            am.add_clip(ch.arm, clip, keys)
            if clip in rods:
                aiming.bake_musket(ch.arm, clip, keys, rod_phases=rods[clip])
            else:
                aiming.stow_rammer(ch.arm, clip)
            names.append(clip)
        ctx.fire = aiming.fire_stats(ch, present)
        ctx.reload = aiming.reload_report(ch.arm, plan)
        stand = poses.STAND_MUSKET
    else:
        # A spear fought with both hands (#510): every frame keyed so the hands stay on the
        # shaft (`clips.solved`), and the spear dropped for the crawl.
        made = unit_clips(s)
        if pole:
            cl.keep_shown(made)
        for clip, keys in made.items():
            cl.apply(ch.arm, clip, keys, dense=pole)
            names.append(clip)
        stand = stand_of(s)
    if pole:
        cl.apply_crawl(ch.arm)
        names.extend(crawl.clips())
    else:
        for clip, keys in crawl.clips().items():
            am.add_clip(ch.arm, clip, keys)
            if s["weapon"] == "gun":
                aiming.stow_rammer(ch.arm, clip)
            names.append(clip)
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    # Clean-shaven; the shaved and plucked heads (roach, scalp lock) without a cropped scalp.
    heads.finish(ch, beard=False, scalp=s["hair"] == "long")
    ch.arm.animation_data.action = None
    am.pose(ch.arm, cl.solve(ch.arm, stand))
    # Every socket at scale 1 for the export: a 0 here bakes a 0 scale into the club's own
    # node and hides it in every clip (m2-shared-moves; docs/melee.md, m2-melee4). The
    # clips key the sockets' scales.
    for pb in ch.arm.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)
    if s["weapon"] == "gun":
        ch.arm.pose.bones[aiming.ROD_SOCKET].matrix = ch.arm.pose.bones["socket.hand_r"].matrix @ aiming.ROD_STOWED
    tris = ch.triangles()
    # `scale`: the whole man about his ground point, for a unit whose sprites draw him
    # taller than the family's build (DelKri, #196).
    ctx.root.scale = (s["scale"],) * 3
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=names,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), acr_family="native")
    ctx.stats = {"triangles": tris, "light": report, "colours": colours,
                 "aim_errors_m": getattr(ctx, "aim_errors", None), "fire": getattr(ctx, "fire", None),
                 "reload": getattr(ctx, "reload", None), "drape": getattr(ch, "drape_report", None)}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    keys = unit_clips(s)[clip] if s["weapon"] != "gun" else poses.musketeer_clips(s["md"])[clip]
    am.pose(ch.arm, cl.bare(keys[-1][1] if clip == "DEATH" else keys[0][1]))


_ = (held, straps)  # they register material keys (`hide`, `feather`, ...) and the straps (`belt_l`)
