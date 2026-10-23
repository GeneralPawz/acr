"""Plains warriors (docs/humans-roster.md, family 9; docs/families/plains.md): the Sioux of
the 1830s as Catlin and Bodmer drew them, built on the Woodlands family (`native.py`) by
import: a Plains unit is a `native` SPEC plus Plains keys, and `build` runs native's
pipeline with two hooks (`native.dress` and `native.hair` are wrapped for the call and
restored).

Plains SPEC keys (over `native.spec`'s):
- `warshirt`: a buckskin war shirt (a shell over the trunk to the hip and the arms to the
  wrist, key `war_shirt`) with quilled strips over the shoulders (`beads`) and down the
  top of each sleeve (`quill`);
- `fringe`: fringe as strands (narrow hanging cards, key `fringe`), not a band: along the
  underside of the sleeves (with `warshirt`) and the outer seam of the leggings;
- `braids`: two braids in front of the shoulders with cloth wraps (`wraps`), hair parted;
- `eagle`: single eagle feathers at the back of the head (white with a dark tip);
- `breastplate`: a hair-pipe breastplate (two columns of bone tubes on a hide strip);
- `bonnet`: the chief's eagle-feather war bonnet (a brow band, a fan of feather cards
  standing up and back, a short trailer) - the mounted chief's, see `mounted`.

`mounted(ctx, spec, head)` builds a rider with `cavalry.build`, swapping
`cavalry.native_head` for the Plains head (`braids`, `eagle` or `bonnet`).
"""

import math
import random

import bmesh
from mathutils import Matrix, Vector

from . import armature as am
from . import cavalry, crawl, gear, heads, native, outfit, registry, rig
from . import clips as cl
from .humans import LAYER_OF, prepare_export

# Buckskin (smoked and white-tanned), quillwork and trade-bead colours, after the shirts in
# Bodmer's plates and the Smithsonian's Sioux shirts.
PALETTE = {
    "skin": (0.78, 0.52, 0.36), "war_shirt": (0.70, 0.56, 0.38), "fringe": (0.66, 0.52, 0.34),
    "leggings": (0.62, 0.48, 0.31), "flaps": (0.62, 0.48, 0.31), "moccasins": (0.70, 0.58, 0.42),
    "breechcloth": (0.14, 0.18, 0.34), "garter": (0.55, 0.12, 0.08), "quill": (0.72, 0.22, 0.10),
    "beads": (0.30, 0.46, 0.72), "hairpipe": (0.90, 0.86, 0.74), "wraps": (0.58, 0.10, 0.07),
    "eagle_tip": (0.08, 0.06, 0.05), "feather": (0.92, 0.90, 0.85),
}
PLAINS_KEYS = {"warshirt": False, "fringe": True, "braids": True, "eagle": 1, "breastplate": False, "bonnet": False}
# The buckskin keeps its colour too: 60 % of the sprites' shaded olive turned it grey-green.
FIXED = ("skin", "feather", "silver", "eagle_tip", "hairpipe", "beads", "quill", "war_shirt", "fringe", "leggings", "flaps",
         "moccasins")

for _key, _layer, _painter, _fb in (
        ("war_shirt", "leather", "buckskin", PALETTE["war_shirt"]), ("fringe", "leather", "buckskin", PALETTE["fringe"]),
        ("quill", "hide", None, PALETTE["quill"]), ("beads", "hide", None, PALETTE["beads"]),
        ("hairpipe", "bone", None, PALETTE["hairpipe"]), ("wraps", "wool", "stroud", PALETTE["wraps"]),
        ("eagle_tip", "hair", None, PALETTE["eagle_tip"])):
    registry.material(_key, _layer, painter=_painter, fallback=_fb)


def spec(md, weapons, palette=None, build=None, **kw):
    """A Plains SPEC: `native.spec` with the Plains palette and the Plains keys. The hair is
    the Plains hair (`hair` "plains": parted, braided), made here, not by `native.hair`."""
    plains = {k: kw.pop(k, v) for k, v in PLAINS_KEYS.items()}
    kw.setdefault("hair", "plains")
    kw.setdefault("feathers", 0)
    kw.setdefault("fixed", FIXED)
    s = native.spec(md, weapons, palette=dict(PALETTE, **(palette or {})), build=build, **kw)
    s.update(plains)
    return s


# -- seams and fringe -------------------------------------------------------------------

def _seam(ch, bone, pick, bins=10, t0=0.08, t1=0.94, min_w=0.6):
    """Points along `bone` on the flesh: per bin of the bone's length the vertex `pick`
    scores highest (the underside, the outer side), with its bone weights."""
    P, flesh = ch.prefix, ch.flesh
    b = ch.arm.data.bones[P + bone]
    head, tail = b.head_local, b.tail_local
    axis = tail - head
    ln2 = axis.length_squared
    names = {g.index: g.name for g in flesh.vertex_groups}
    best = [None] * bins
    for v in flesh.data.vertices:
        gs = {names[g.group]: g.weight for g in v.groups if g.weight > 0.01}
        if gs.get(P + bone, 0.0) < min_w:
            continue
        t = (v.co - head).dot(axis) / ln2
        if not t0 <= t <= t1:
            continue
        i = min(int((t - t0) / (t1 - t0) * bins), bins - 1)
        score = pick(v.co)
        if best[i] is None or score > best[i][0]:
            best[i] = (score, v.co.copy(), gs)
    return [(c, gs) for x in best if x for _, c, gs in [x]]


def _strands(ch, name, points, hang, length, out, per=3, width=0.007, seed=1):
    """Fringe as strands: `per` narrow two-segment cards from each seam point, hanging along
    `hang` (bent a little at the tip), each skinned like its seam vertex."""
    rng = random.Random(seed)
    verts, groups, faces = [], [], []
    hang = hang.normalized()
    for (c, gs), (c2, _) in zip(points, points[1:] + points[-1:]):
        step = (c2 - c) if c2 != c else Vector((0, 0, 0))
        for k in range(per):
            base = c + out + step * (k / per)
            side = hang.cross(Vector((0, 1, 0)) if abs(hang.y) < 0.9 else Vector((1, 0, 0))).normalized() * width / 2
            ln = length * (0.8 + 0.4 * rng.random())
            i0 = len(verts)
            for t in (0.0, 0.55, 1.0):
                p = base + hang * (ln * t) + Vector((0, 0, -0.01 * t * t))
                verts += [p - side, p + side]
                groups += [gs, gs]
            faces += [(i0, i0 + 1, i0 + 3, i0 + 2), (i0 + 2, i0 + 3, i0 + 5, i0 + 4)]
    obj = native._mesh(f"{ch.name}.{name}", verts, faces, groups)
    return am.bind(obj, ch.arm)


def fringe(ch, s):
    """Sleeve fringe (underside of the arms, with the war shirt) and legging fringe (the
    outer seam, off the edge of the legging flap)."""
    made = []
    if s["warshirt"]:
        for sd in ("l", "r"):
            for bone, n in ((f"upperarm_{sd}", 8), (f"forearm_{sd}", 7)):
                pts = _seam(ch, bone, lambda c: -c.z, bins=n, t1=0.80 if bone.startswith("fore") else 0.94)
                if pts:
                    made.append(ch.add("clothes", _strands(ch, f"fringe_{bone}", pts, Vector((0, 0.15, -1)), 0.10,
                                                           Vector((0, 0, -0.016)), seed=len(made)), "fringe"))
    if "leggings" in s["dress"]:
        for sd, side in (("l", 1.0), ("r", -1.0)):
            for bone, n in ((f"thigh_{sd}", 7), (f"shin_{sd}", 9)):
                pts = _seam(ch, bone, lambda c, side=side: c.x * side, bins=n, t0=0.25 if bone.startswith("thigh") else 0.05)
                pts = [(c, gs) for c, gs in pts if 0.12 < c.z < 0.78]
                if pts:
                    made.append(ch.add("clothes", _strands(ch, f"fringe_{bone}", pts, Vector((side * 0.9, 0, -1)), 0.08,
                                                           Vector((side * 0.040, 0, 0)), per=2, seed=len(made) + 7), "fringe"))
    return made


# -- dress and hair (the hooks into native.make) -----------------------------------------

def dress(ch, s):
    """The war shirt with its quilled strips, then native's dress."""
    if s["warshirt"]:
        ch.shell("war_shirt", "war_shirt", lambda c, n, h, p: (h in native.TORSO and 0.86 < c.z < 1.50) or h in native.ARMS
                 and c.z > 0.0, 0.014)
        # Quilled strips: over each shoulder (front and back) and along the top of each sleeve.
        ch.shell("shoulder_strips", "beads", lambda c, n, h, p: h in native.TORSO and 1.28 < c.z < 1.50
                 and abs(abs(c.x) - 0.105) < 0.022, 0.019)
        ch.shell("sleeve_strips", "quill", lambda c, n, h, p: h in native.ARMS and n.z > 0.80, 0.019)
    native_dress(ch, s)
    fringe(ch, s)


def _front(ch, z, x=0.0, band=0.03):
    """The front of the trunk (min y) near height `z` and across `x`."""
    ys = [v.co.y for v in ch.flesh.data.vertices if abs(v.co.z - z) < band and abs(v.co.x - x) < 0.02]
    return min(ys) if ys else -0.12


def breastplate(ch, s):
    """The hair-pipe breastplate: two columns of bone tubes on a hide strip, from the
    collarbones to the lower ribs, rigid on the chest (`spine2`)."""
    bm = bmesh.new()
    hide = bmesh.new()
    z_top, rows = 1.40, 11
    off = 0.030 if s["warshirt"] else 0.016
    for r in range(rows):
        z = z_top - r * 0.022
        for sd in (-1.0, 1.0):
            x = sd * 0.055
            y = _front(ch, z, x) - off
            m = Matrix.Translation((x, y, z)) @ Matrix.Rotation(math.radians(90), 4, "Y")
            bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.0075, radius2=0.0075, depth=0.085, matrix=m)
        native._box(hide, Vector((0, _front(ch, z) - off + 0.004, z)), (0.022, 0.004, 0.024))
    for key, b, nm in (("hairpipe", bm, "breastplate"), ("belt_hide", hide, "breastplate_strip")):
        o = gear.from_bm(b, f"{ch.name}.{nm}")
        gear.rigid(o, ch.arm, ch.prefix + "spine2", Matrix.Identity(4), part=1)
        ch.add("gear", o, key)


def _feather(bm, tip, base, up, side, length, width, bend=None):
    """An eagle feather as two cards: the white quill (`bm`) and its dark tip (`tip`)."""
    native._card(bm, base, up, side, length * 0.72, width, taper=0.85, bend=bend)
    end = base + up.normalized() * (length * 0.72) + ((bend or Vector()) * 0.52)
    native._card(tip, end, up, side, length * 0.28, width * 0.85, taper=0.35)


def hair(ch, s):
    """Plains hair (parted, two braids in front of the shoulders with wraps), single eagle
    feathers; then native's hair (the `feathers` and `headdress` keys)."""
    crown, head = native._skull(ch)
    centre = Vector((0.0, head.y + 0.01, (crown.z + head.z) / 2 + 0.01))
    r = crown.z - centre.z
    made = []
    if s["hair"] == "plains":
        bm, wraps = bmesh.new(), bmesh.new()
        # The hair over the back of the head, a thin cap (the face map paints the scalp).
        native._box(bm, Vector((0, centre.y + 0.07, centre.z - 0.04)), (0.14, 0.035, 0.12))
        if s["braids"]:
            for sd in (-1.0, 1.0):
                top = Vector((sd * 0.078, centre.y + 0.01, centre.z - 0.05))
                bot = Vector((sd * 0.098, centre.y - 0.075, centre.z - 0.40))
                n = 6
                for i in range(n):
                    a, b = top.lerp(bot, i / n), top.lerp(bot, (i + 1) / n)
                    mid, d = (a + b) / 2, (b - a)
                    rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
                    w = 0.026 - 0.008 * i / n
                    native._box(bm, mid, (w, w * 0.8, d.length * 1.05), rot)
                for t in (0.30, 0.75):  # the wraps (trade cloth or otter fur)
                    c = top.lerp(bot, t)
                    rot = (bot - top).to_track_quat("Z", "Y").to_matrix().to_4x4()
                    native._box(wraps, c, (0.032, 0.028, 0.05), rot)
        made += [("hair", gear.from_bm(bm, f"{ch.name}.braids")), ("wraps", gear.from_bm(wraps, f"{ch.name}.hair_wraps"))]
    if s["eagle"]:
        quill, tip = bmesh.new(), bmesh.new()
        for k in range(s["eagle"]):
            dx = (k - (s["eagle"] - 1) / 2) * 0.03
            base = centre + Vector((dx, 0.07, r * 0.75))
            _feather(quill, tip, base, Vector((dx * 3, 0.55, 0.85 - 0.25 * k)), Vector((1, 0, 0)), 0.26, 0.04,
                     bend=Vector((0, 0.03, -0.03)))
        made += [("feather", gear.from_bm(quill, f"{ch.name}.eagle")), ("eagle_tip", gear.from_bm(tip, f"{ch.name}.eagle_tip"))]
    for key, o in made:
        gear.rigid(o, ch.arm, ch.prefix + "head", Matrix.Identity(4), part=1)
        ch.add("gear", o, key)
    if s["breastplate"]:
        breastplate(ch, s)
    return native_hair(ch, s) + made


native_dress, native_hair = native.dress, native.hair


class _hooks:
    """`native.dress` and `native.hair` swapped for the Plains ones during `native.make`."""

    def __enter__(self):
        native.dress, native.hair = dress, hair
        return self

    def __exit__(self, *exc):
        native.dress, native.hair = native_dress, native_hair
        return False


def make(root, name, s):
    with _hooks():
        return native.make(root, name, s)


# -- clips ------------------------------------------------------------------------------

def unit_clips(s):
    """native's clips, with the Plains `.MD` extras: stance clips 3-5 standing (PSTAND3..5,
    P/UATTACK1/4/5, ATTACK4), the peasant's ATTACK1 (the work swing on its own frames) and
    ATTACK5 (a club blow, ATTACK's strike), and a melee ATTACK made from ATTACK5 when the
    `.MD` has no ATTACK of its own (SiuKri's index 0 plays ATTACK5)."""
    md = dict(s["md"])
    if "ATTACK" not in md and "ATTACK5" in md:
        md["ATTACK"] = md["ATTACK5"].replace("ATTACK5", "ATTACK")
    out = native.unit_clips(dict(s, md=md))
    stand = native.stand_of(s)
    for k in ("PSTAND3", "PSTAND4", "PSTAND5", "PATTACK1", "UATTACK1", "PATTACK4", "UATTACK4", "PATTACK5", "UATTACK5",
              "ATTACK4", "PSTAND1"):
        if k in md and k not in out:
            out[k] = rig.clip_keys(rig.md_clip_frames(md[k])[0], [(0.0, stand)])
    if "ATTACK5" in md and "ATTACK_CUT_MID" in out:
        out["ATTACK5"] = list(out["ATTACK_CUT_MID"])
    if s["work"] and "ATTACK1" in md and "WORK" in out and s["weapons"].get(1) is None:
        n = rig.md_clip_frames(md["ATTACK1"])[0]
        w = rig.md_clip_frames(md["WORK"])[0]
        out["ATTACK1"] = [(round(f * (n - 1) / max(w - 1, 1)), p) for f, p in out["WORK"]]
    return out


def build(ctx, s):
    """native.build's path for the non-gun units, with the Plains `make` and clips."""
    ch = make(ctx.root, ctx.name, s)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    names = []
    for clip, keys in unit_clips(s).items():
        cl.apply(ch.arm, clip, keys)
        names.append(clip)
    for clip, keys in crawl.clips().items():
        am.add_clip(ch.arm, clip, keys)
        names.append(clip)
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch, beard=False, scalp=True)
    ch.arm.animation_data.action = None
    am.pose(ch.arm, cl.solve(ch.arm, native.stand_of(s)))
    if "socket.club_r" in ch.arm.pose.bones:
        ch.arm.pose.bones["socket.club_r"].scale = (0.0, 0.0, 0.0)
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=names,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), acr_family="native")
    ctx.stats = {"triangles": tris, "light": report, "colours": colours}


def pose_for(ch, s, clip):
    keys = unit_clips(s)[clip]
    am.pose(ch.arm, cl.bare(keys[-1][1] if clip == "DEATH" else keys[0][1]))


# -- mounted (cavalry.py riders with the Plains head) ------------------------------------

def _plains_head(kind):
    """A `cavalry.native_head` replacement: the hair and braids, and `kind` "eagle" (one
    eagle feather at the back) or "bonnet" (the war bonnet)."""

    def head_fn(ch, name, _kind, cy):
        P = cavalry.P
        hd = am.world(ch.arm, P + "head").translation
        sock = am.world(ch.arm, P + "socket.hat").translation
        bm, wraps = bmesh.new(), bmesh.new()
        native._box(bm, Vector((0, cy + 0.07, hd.z + 0.02)), (0.15, 0.045, 0.17))
        for sd in (-1.0, 1.0):
            top = Vector((sd * 0.078, cy + 0.02, hd.z - 0.02))
            bot = Vector((sd * 0.098, cy - 0.065, hd.z - 0.37))
            rot = (bot - top).to_track_quat("Z", "Y").to_matrix().to_4x4()
            native._box(bm, (top + bot) / 2, (0.024, 0.02, (bot - top).length), rot)
            for t in (0.30, 0.75):
                native._box(wraps, top.lerp(bot, t), (0.032, 0.028, 0.05), rot)
        cavalry._held(ch, f"{name}_hair", "hair", bm, "head", Matrix.Identity(4))
        cavalry._held(ch, f"{name}_wraps", "wraps", wraps, "head", Matrix.Identity(4))
        quill, tip = bmesh.new(), bmesh.new()
        if kind == "bonnet":
            band = bmesh.new()
            c = Vector((sock.x, cy + 0.005, sock.z - 0.06))
            # Brow band (beaded) and a fan of feathers from ear to ear standing up and back;
            # a short trailer of feathers down the back to the waist.
            for k in range(14):
                a0, a1 = math.radians(-110 + 220 * k / 14), math.radians(-110 + 220 * (k + 1) / 14)
                ring = [Vector((c.x + math.sin(a) * 0.098, c.y - math.cos(a) * 0.108, c.z + dz)) for a in (a0, a1)
                        for dz in (-0.02, 0.02)]
                vs = [band.verts.new(p) for p in ring]
                band.faces.new((vs[0], vs[2], vs[3], vs[1]))
            n = 22
            for k in range(n):
                a = math.radians(-105 + 210 * k / (n - 1))
                base = Vector((c.x + math.sin(a) * 0.10, c.y - math.cos(a) * 0.10 + 0.02, c.z + 0.01))
                up = Vector((math.sin(a) * 0.55, 0.55 + 0.25 * abs(math.cos(a)), 1.0 - 0.35 * abs(math.sin(a))))
                _feather(quill, tip, base, up, Vector((math.cos(a), math.sin(a) * 0.3, 0)), 0.30, 0.042,
                         bend=Vector((0, 0.04, -0.02)))
            for k in range(6):  # the trailer: feathers hanging down the back on a red strip
                base = Vector((c.x, c.y + 0.12 + 0.01 * k, c.z - 0.06 - 0.075 * k))
                _feather(quill, tip, base, Vector((0, 0.35, -1)), Vector((1, 0, 0)), 0.16, 0.05)
                native._box(band, base + Vector((0, 0.005, 0)), (0.07, 0.006, 0.08))
            cavalry._held(ch, f"{name}_bonnet_band", "quill", band, "socket.hat", Matrix.Identity(4))
        else:
            base = Vector((sock.x, cy + 0.085, sock.z - 0.02))
            _feather(quill, tip, base, Vector((0, 0.45, 0.9)), Vector((1, 0, 0)), 0.26, 0.04)
        cavalry._held(ch, f"{name}_eagle_tip", "eagle_tip", tip, "socket.hat", Matrix.Identity(4))
        return cavalry._held(ch, f"{name}_feathers", "cap", quill, "socket.hat", Matrix.Identity(4))

    return head_fn


def mounted(ctx, spec, head="eagle"):
    """`cavalry.build` with the Plains head (`_plains_head`) for the `feathers`/`crown` hats."""
    saved = cavalry.native_head
    cavalry.native_head = _plains_head(head)
    try:
        cavalry.build(ctx, spec)
    finally:
        cavalry.native_head = saved
