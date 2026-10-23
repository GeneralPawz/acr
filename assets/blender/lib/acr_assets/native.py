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
  (`quiver`, `horn`, `pouch`, `knife`).
"""

import math

import bmesh
from mathutils import Matrix, Vector

from . import aim as aiming
from . import armature as am
from . import clips as cl
from . import crawl, gear, heads, held, masks, mbody, outfit, poses, registry, rig, straps
from .humans import LAYER_OF, Character, prepare_export

# The Iroquois warrior's colours: a nation overrides them. The skin is a warm copper as in
# the sprites (slice 1's darker brown plus the sprite mix came out near-black on the
# field); it keeps the palette colour (`fixed`), a unit varies it a little.
PALETTE = {
    "skin": (0.80, 0.55, 0.38), "bone": (0.85, 0.81, 0.69), "hair": (0.06, 0.05, 0.05),
    "breechcloth": (0.16, 0.18, 0.30), "leggings": (0.50, 0.10, 0.08), "flaps": (0.50, 0.10, 0.08),
    "garter": (0.70, 0.55, 0.20), "moccasins": (0.45, 0.32, 0.20), "shirt": (0.86, 0.84, 0.78),
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
            "work": False}

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


def _legging_flap(ch, side, z0=0.14, z1=0.76, rows=14):
    """The legging's flap on the outer side of leg `side` (1 left, -1 right): a two-sided
    fin from the outermost skin line (sampled in bands of height, smoothed) standing
    2-4 cm out (widest at the ankle), each row skinned like its skin vertex."""
    P, flesh = ch.prefix, ch.flesh
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
                verts.append((c.x + side * (0.009 + dx), c.y + dy, z))
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


def dress(ch, s):
    """The garments of SPEC `s` as shells and modelled panels (the clothes layer)."""
    name, d = ch.name, s["dress"]
    knee = ch.arm.data.bones[ch.prefix + "shin_l"].head_local.z
    if "moccasins" in d:
        ch.shoes = ch.add("clothes", mbody.shoes(ch.arm, ch.flesh, f"{name}.footwear", ch.prefix, offset=0.005, sole=0.006,
                                                 heel=0.002, key="moccasins"), "moccasins", symmetric=True)
        ch.shoes["acr_material_keys"] = ["moccasins", "moccasins"]  # a tie, not a brass buckle
    if "leggings" in d:
        ch.shell("leggings", "leggings", lambda c, n, h, p: h in LEGS and 0.10 < c.z < 0.80, 0.009)
        # The flap: the seam allowance standing out along the outer side of each leg, a
        # thin modelled fin (slice 1's offset shell of the outer faces read lumpy).
        for side in (1.0, -1.0):
            ch.add("clothes", _legging_flap(ch, side), "flaps")
        ch.shell("garters", "garter", lambda c, n, h, p: h in {"shin_l", "shin_r"} and knee - 0.08 < c.z < knee - 0.05, 0.016)
    if "shirt" in d:
        ch.shell("shirt", "shirt", lambda c, n, h, p: (h in TORSO and 0.80 < c.z < 1.50) or h in ARMS
                 or (h in {"thigh_l", "thigh_r"} and c.z > 0.74), 0.013)
    belt_off = 0.024 if "shirt" in d else 0.012
    if "breechcloth" in d:
        ch.shell("belt", "belt_hide", lambda c, n, h, p: h in {"hips", "spine"} and 0.98 < c.z < 1.03, belt_off + 0.004)
        ch.shell("breechcloth", "breechcloth", lambda c, n, h, p: h in {"hips", "thigh_l", "thigh_r"} and 0.80 < c.z < 0.99
                 and abs(c.x) < 0.075, belt_off)
        # The flaps hang over the belt, front and back (back a little longer).
        y_front = -0.115 - (0.012 if "shirt" in d else 0.0)
        y_back = 0.105 + (0.012 if "shirt" in d else 0.0)
        ch.add("clothes", _flap(ch, "flap_front", y_front, 1.03, 0.60, 0.22, 0.25, 0.05), "breechcloth")
        ch.add("clothes", _flap(ch, "flap_back", y_back, 1.03, 0.56, 0.24, 0.27, 0.05, back=True), "breechcloth")
    if "matchcoat" in d:
        # A blanket over the left shoulder and under the right arm: the trunk above a line
        # from the right hip to the left armpit, and the left upper arm.
        ch.shell("matchcoat", "matchcoat", lambda c, n, h, p: (h in TORSO and c.z > 1.02 + 0.9 * (0.18 - c.x) * (c.z < 1.6)
                                                                and c.z < 1.52 and not (c.x < -0.12 and c.z > 1.25))
                 or (h == "upperarm_l") or (h in {"thigh_l"} and c.z > 0.70 and c.x > 0.0), 0.030)
    if "cloak" in d:
        # The shaman's fur cloak: a pelt over both shoulders and down the back to the
        # seat, open in front below the collarbones; over the upper arms to mid-arm.
        ch.shell("cloak", "pelt", lambda c, n, h, p: (h in TORSO and 0.92 < c.z < 1.56
                                                       and (c.y > -0.02 or c.z > 1.40 or abs(c.x) > 0.16))
                 or (h in {"upperarm_l", "upperarm_r"} and c.z > 1.22), 0.034)
    if s.get("silver"):
        ch.shell("armbands", "silver", lambda c, n, h, p: h in {"upperarm_l", "upperarm_r"}
                 and abs(c.z - (ch.arm.data.bones[ch.prefix + "upperarm_l"].head_local.z - 0.12)) < 0.018, 0.010)


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
        gear.rigid(q, ch.arm, ch.prefix + "spine2", Matrix.Translation(at) @ rot)
        out["quiver"] = ch.add("gear", q, "hide")
    if "horn" in k:
        h = _horn(f"{name}.powder_horn")
        h["acr_material_keys"] = ["horn"]
        gear.rigid(h, ch.arm, ch.prefix + "hips", Matrix.Translation((-0.17, 0.02, 0.98)) @ Matrix.Rotation(math.radians(-70), 4, "Y")
                   @ Matrix.Rotation(math.radians(90), 4, "Z"))
        out["horn"] = ch.add("gear", h, "horn")
    if "pouch" in k:
        p = gear.box(f"{name}.shot_pouch", (0.13, 0.04, 0.12))
        gear.rigid(p, ch.arm, ch.prefix + "hips", Matrix.Translation((-0.15, -0.05, 0.90)) @ Matrix.Rotation(math.radians(25), 4, "Z"))
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
    ch.body()
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
        sock = am.world(ch.arm, "socket.hand_r")
        rest_up = sock.to_3x3() @ ch.arm.pose.bones["socket.hand_r"].bone.matrix_local.to_3x3().inverted()
        am.attach(gun, ch.arm, "socket.hand_r", Matrix.Translation(sock.translation + Vector((0.0, -0.01, 0.0))) @ rest_up.to_4x4())
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
        sock = am.world(ch.arm, "socket.hand_l")
        rest_up = sock.to_3x3() @ ch.arm.pose.bones["socket.hand_l"].bone.matrix_local.to_3x3().inverted()
        am.attach(r, ch.arm, "socket.hand_l", Matrix.Translation(sock.translation) @ rest_up.to_4x4())
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
    c_l, p_l, c_r, p_r = poses.walk_cycle(walk)
    lie = poses.lie_prone()
    keys = {
        "STAND": [(0.0, stand)],
        "MOTION_L": [(0.0, c_l), (0.5, p_l), (1.0, c_r)],
        "MOTION_R": [(0.0, c_r), (0.5, p_r), (1.0, c_l)],
        "MOTION_LB": [(0.0, c_l), (0.5, p_r), (1.0, c_r)],
        "MOTION_RB": [(0.0, c_r), (0.5, p_l), (1.0, c_l)],
        "DEATH": poses.death_keys(cl.bare(stand), lie),
        "DEATHLIE1": [(0.0, lie)],
    }
    for k in ("PSTAND", "PSTAND0", "PSTAND1", "PSTAND2", "PATTACK", "UATTACK", "PATTACK0", "UATTACK0", "PATTACK1",
              "UATTACK1", "PATTACK2", "UATTACK2"):
        keys[k] = [(0.0, stand)]
    for k in ("MOTION_L", "MOTION_R", "MOTION_LB", "MOTION_RB"):
        for sfx in "0123456789":
            keys[k + sfx] = keys[k]
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
                if club:
                    show["socket.hand_r"] = 0
                show.setdefault("socket.hand_r", 1)
                memo[k] = dict(pose, show=show)
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
    if s["weapon"] == "gun":
        rod = gear.rammer(f"{ctx.name}.rammer")
        am.attach(rod, ch.arm, aiming.ROD_SOCKET, am.world(ch.arm, aiming.ROD_SOCKET))
        ch.held["rammer"] = rod
        ch.keys[rod.name] = "metal"
        aim = aiming.hold(ch.arm, poses.AIM_MUSKET, pocket=(0.06, 0.0, -0.02), elbow_r=(0.35, 0.10, -0.06))
        ctx.aim_errors = aiming.errors(ch.arm, aim)
        reload = aiming.drill(ch.arm, poses.RELOAD_STEPS)
        charge = aiming.hold(ch.arm, poses.CHARGE_MUSKET, pocket=(0.02, -0.12, -0.32), left_along=0.30)
        clips = poses.musketeer_clips(md, aim=aim, charge=charge, reload=reload)
        for k in ("MOTION_L", "MOTION_R", "MOTION_LB", "MOTION_RB"):
            for sfx in ("0", "1"):
                if k + sfx in md:
                    clips[k + sfx] = clips[k]
        clips = {k: v for k, v in clips.items() if k in md or k.startswith("ATTACK_")}
        for clip, keys in clips.items():
            am.add_clip(ch.arm, clip, keys)
            if clip == "ATTACK1":
                aiming.bake_musket(ch.arm, clip, keys, rod_phases=poses.ROD_PHASES)
            else:
                aiming.stow_rammer(ch.arm, clip)
            names.append(clip)
        stand = poses.STAND_MUSKET
    else:
        for clip, keys in unit_clips(s).items():
            cl.apply(ch.arm, clip, keys)
            names.append(clip)
        stand = stand_of(s)
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
    if "socket.club_r" in ch.arm.pose.bones:
        ch.arm.pose.bones["socket.club_r"].scale = (0.0, 0.0, 0.0)
    if s["weapon"] == "gun":
        ch.arm.pose.bones[aiming.ROD_SOCKET].matrix = ch.arm.pose.bones["socket.hand_r"].matrix @ aiming.ROD_STOWED
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=names,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), acr_family="native")
    ctx.stats = {"triangles": tris, "light": report, "colours": colours,
                 "aim_errors_m": getattr(ctx, "aim_errors", None)}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    keys = unit_clips(s)[clip] if s["weapon"] != "gun" else poses.musketeer_clips(s["md"])[clip]
    am.pose(ch.arm, cl.bare(keys[-1][1] if clip == "DEATH" else keys[0][1]))


_ = (held, straps)  # they register material keys (`hide`, `feather`, ...) and the straps (`belt_l`)
