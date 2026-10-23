"""The Pueblo on foot as one family (docs/humans-roster.md, family 10; docs/families/
pueblo.md), built on `native.py`: the same body, skin, legging and moccasin pipeline, with
the Southwest dress added by wrapping native's `dress`, `hair` and `kit` for the call (the
way `native.clean_shaven` swaps `faces.zones`), never by editing native.

Southwest dress (all shells of the MakeHuman skin or modelled panels, deforming with it):
- `kilt`: a woven cotton kilt wrapped round the hips to above the knee (a tube on the
  skirt bones), white with a black, green and red stepped border at the hem (painter
  `pue_kilt`); the colour of the ground is the palette's `pue_kilt`;
- `sash`: a woven sash at the waist (red ground, a black and green diamond band; painter
  `pue_sash`) with its end hanging at the right front;
- `bandolier`: a woven band from the right hip over the left shoulder (the shaman's stole);
- `tunic`: a sleeveless cotton shirt in a geometric weave (the archer's);
- `wraps`: knee-high wrapped leggings or boot moccasins (buckskin or red cloth wound on
  the shin, painter `pue_wraps`: the diagonal turns);
- `jerkin`: a Spanish-influenced leather jacket (`sleeves` True) or a sleeveless vest;
- `blanket`: a plaid blanket over the shoulders (native's matchcoat cut, painter `pue_plaid`);
- hair (`phair`): `bangs` (cut at the brow and at the jaw at the sides, long at the back)
  or `chongo` (the same with the back hair tied into a knot at the nape); a cloth headband
  (`headband`); necklaces (`necklace`: `turquoise`, `shell`).

Gear: native's bow, quiver and trade gun; the war club is a stone-headed club
(`stone_club`, in place of the Woodlands ball club for the call); the shaman a gourd rattle
(`gourd`). The peasant (`peasant` True) takes `civil.py`'s tools and work clips on the
Pueblo body (`build_peasant`).

SPEC: `native.spec`'s keys plus the Pueblo ones (`PUEBLO`).
"""

import math

import bmesh
from mathutils import Matrix, Vector

from . import armature as am
from . import body as bd
from . import civil, crawl, gear, heads, held, native, outfit, registry, rig
from . import clips as cl
from .humans import LAYER_OF, Character, prepare_export

PALETTE = {
    "skin": (0.74, 0.50, 0.34), "hair": (0.05, 0.04, 0.04), "leggings": (0.62, 0.10, 0.08),
    "flaps": (0.62, 0.10, 0.08), "moccasins": (0.62, 0.50, 0.34), "shirt": (0.88, 0.86, 0.80),
    "pue_kilt": (0.88, 0.86, 0.80), "pue_sash": (0.66, 0.08, 0.06), "pue_wraps": (0.62, 0.10, 0.08),
    "pue_tunic": (0.80, 0.82, 0.72), "pue_jerkin": (0.12, 0.11, 0.10), "pue_blanket": (0.55, 0.10, 0.07),
    "pue_band": (0.70, 0.08, 0.06), "turquoise": (0.25, 0.66, 0.62), "shell": (0.90, 0.86, 0.78),
    "gourd": (0.72, 0.56, 0.30),
}
PUEBLO = {"pdress": (), "phair": "bangs", "headband": True, "necklace": ("turquoise", "shell"), "sleeves": False,
          "gourd": False, "peasant": False}
FIXED = ("skin", "hair", "feather", "silver", "turquoise", "shell", "pue_kilt", "pue_sash", "pue_tunic", "pue_band")

# -- painters: the woven patterns ---------------------------------------------------------

KILT_HEM = 0.62  # the kilt's lower edge (m, rest pose)


def _np():
    import numpy as np

    from . import coat

    return np, coat


def _fret(np, u, v):
    """A stepped triangle row (the Pueblo 'cloud terrace'): 1 inside the steps; `u` runs
    along the band (one motif per unit), `v` 0..1 across it."""
    f = u - np.floor(u)
    tri = 1.0 - np.abs(2.0 * f - 1.0)  # 0 at the edges, 1 in the middle
    return (np.floor(v * 4.0) / 4.0 < np.floor(tri * 4.0) / 4.0).astype(np.float32)


def paint_kilt(p, n, rgb, L, seed):
    """A cotton kilt: the plain-woven ground (`rgb`, a faint weave) and, 3-12 cm above the
    hem, a band of black stepped terraces between red and green lines."""
    np, coat = _np()
    base = coat.lin(rgb)
    col = base * (0.90 + 0.10 * coat.fbm(p, (400.0, 400.0, 900.0), seed, octaves=1))[:, None]
    z = p[:, 2] - KILT_HEM
    a = np.arctan2(p[:, 1], p[:, 0])
    v = np.clip((z - 0.035) / 0.07, 0.0, 1.0)
    band = coat.smooth(0.030, 0.034, z) * coat.smooth(0.110, 0.106, z)
    terrace = _fret(np, a * 12.0 / (2 * math.pi), v) * band
    col = coat.mix(col, coat.lin((0.05, 0.05, 0.05)), terrace)
    for z0, c in ((0.024, (0.62, 0.08, 0.06)), (0.116, (0.62, 0.08, 0.06)), (0.012, (0.12, 0.36, 0.20))):
        col = coat.mix(col, coat.lin(c), coat.smooth(0.006, 0.003, np.abs(z - z0)))
    return col


def paint_sash(p, n, rgb, L, seed):
    """A woven sash: the ground (`rgb`) with a band of black diamonds outlined in green
    along it and thin white edge lines (the brocaded sash, a red ground)."""
    np, coat = _np()
    base = coat.lin(rgb)
    col = base * (0.88 + 0.12 * coat.fbm(p, 500.0, seed, octaves=1))[:, None]
    u = (p[:, 0] + p[:, 1]) / 0.05
    w = p[:, 2] / 0.05
    d = np.abs(u - np.round(u)) + np.abs(w - np.round(w))
    col = coat.mix(col, coat.lin((0.12, 0.38, 0.20)), coat.smooth(0.42, 0.38, d))
    col = coat.mix(col, coat.lin((0.04, 0.04, 0.04)), coat.smooth(0.30, 0.26, d))
    stripe = np.abs((p[:, 2] / 0.09) - np.round(p[:, 2] / 0.09))
    return coat.mix(col, coat.lin((0.88, 0.86, 0.80)), coat.smooth(0.05, 0.02, stripe) * 0.8)


def paint_tunic(p, n, rgb, L, seed):
    """A sleeveless woven shirt: the light ground with rows of green and black zig-zags."""
    np, coat = _np()
    base = coat.lin(rgb)
    col = base * (0.9 + 0.1 * coat.fbm(p, 300.0, seed, octaves=1))[:, None]
    a = np.arctan2(p[:, 1], p[:, 0]) * 10.0 / (2 * math.pi)
    f = a - np.floor(a)
    zig = (1.0 - np.abs(2.0 * f - 1.0)) * 0.05
    r = (p[:, 2] - zig) / 0.10
    k = r - np.floor(r)
    col = coat.mix(col, coat.lin((0.10, 0.34, 0.20)), coat.smooth(0.18, 0.12, np.abs(k - 0.3)))
    return coat.mix(col, coat.lin((0.05, 0.05, 0.05)), coat.smooth(0.07, 0.04, np.abs(k - 0.62)))


def paint_wraps(p, n, rgb, L, seed):
    """Wound leggings: the strip's turns rising diagonally round the shin, each edge a
    little darker."""
    np, coat = _np()
    base = coat.lin(rgb)
    a = np.arctan2(p[:, 1], np.abs(p[:, 0]) - 0.09)
    t = p[:, 2] / 0.045 + a / (2 * math.pi)
    k = t - np.floor(t)
    shade = 0.80 + 0.25 * np.sin(k * math.pi)
    return base * (shade * (0.92 + 0.08 * coat.fbm(p, 300.0, seed, octaves=1)))[:, None]


def paint_plaid(p, n, rgb, L, seed):
    """A trade blanket in a plaid: the ground (`rgb`) crossed by black bands and thin
    yellow lines, 12 cm apart."""
    np, coat = _np()
    base = coat.lin(rgb)
    col = base * (0.9 + 0.1 * coat.fbm(p, 200.0, seed, octaves=1))[:, None]
    for axis in (0, 2):
        q = p[:, axis] / 0.12
        d = np.abs(q - np.round(q))
        col = coat.mix(col, coat.lin((0.05, 0.04, 0.04)), coat.smooth(0.16, 0.12, d) * 0.85)
        col = coat.mix(col, coat.lin((0.80, 0.62, 0.12)), coat.smooth(0.03, 0.015, np.abs(d - 0.32)))
    return col


registry.painter("pue_kilt", fn=paint_kilt, finish=(1.0, 0.12, 0.0))
registry.painter("pue_sash", fn=paint_sash, finish=(1.0, 0.12, 0.0))
registry.painter("pue_tunic", fn=paint_tunic, finish=(1.0, 0.12, 0.0))
registry.painter("pue_wraps", fn=paint_wraps, finish=(1.0, 0.30, 0.0))
registry.painter("pue_plaid", fn=paint_plaid, finish=(1.0, 0.40, 0.0))
for _key, _layer, _painter in (
        ("pue_kilt", "linen", "pue_kilt"), ("pue_sash", "linen", "pue_sash"), ("pue_tunic", "linen", "pue_tunic"),
        ("pue_wraps", "wool", "pue_wraps"), ("pue_jerkin", "leather", "buckskin"), ("pue_blanket", "wool", "pue_plaid"),
        ("pue_band", "linen", "linen"), ("turquoise", "bone", None), ("shell", "bone", None), ("gourd", "wood", None)):
    registry.material(_key, _layer, painter=_painter, fallback=PALETTE[_key])


def spec(md, weapons, palette=None, build=None, **kw):
    """A full SPEC: `native.spec` with the Pueblo palette and keys (`PUEBLO`)."""
    extra = {k: kw.pop(k, v) for k, v in PUEBLO.items()}
    kw.setdefault("fixed", FIXED)
    kw.setdefault("hair", "long")  # the scalp painted (`native.clean_shaven(scalp=True)`)
    kw.setdefault("feathers", 0)
    s = native.spec(md, weapons, palette=dict(PALETTE, **(palette or {})), build=build, **kw)
    s.update(extra)
    return s


# -- held items ---------------------------------------------------------------------------

def stone_club(length=0.62):
    """A stone-headed war club: a straight hardwood handle and a grooved, egg-shaped stone
    head lashed across its end with rawhide, the striking end forward (-Y, `socket.tip`
    `clips.CLUB_TIP` from the grip). Origin at the grip near the handle's end."""
    it = held.Item("war_club")
    it.keys = {"club": "wood", "head": "stone", "lash": "hide"}
    it.tube([((0, 0, -0.10), 0.015), ((0, 0, length - 0.10), 0.017)], "club", n=6)
    z = length - 0.13
    path = [((0, 0.05, z), 0.0), ((0, 0.04, z), 0.022), ((0, 0.0, z), 0.036), ((0, -0.05, z), 0.034),
            ((0, -0.10, z), 0.024), ((0, -0.125, z), 0.0)]
    it.tube(path, "head", n=8)
    it.tube([((0, 0.01, z - 0.045), 0.020), ((0, 0.01, z + 0.045), 0.020)], "lash", n=6)
    it.sockets["socket.tip"] = (0.0, -0.125, z)
    return it


def gourd():
    """A gourd rattle: a round gourd on a short wooden handle, painted plain. Origin at
    the grip, +Z up the handle."""
    it = held.Item("gourd")
    it.keys = {"handle": "wood", "gourd": "gourd"}
    it.tube([((0, 0, -0.06), 0.012), ((0, 0, 0.10), 0.012)], "handle", n=6)
    path = [((0, 0, 0.09), 0.0)] + [((0, 0, 0.16 - 0.065 * math.cos(math.pi * k / 6)), 0.065 * math.sin(math.pi * k / 6))
                                    for k in range(1, 6)] + [((0, 0, 0.225), 0.0)]
    it.tube(path, "gourd", n=10)
    return it


# -- dress --------------------------------------------------------------------------------

def _strip(ch, name, x, y, z0, z1, w, out):
    """A hanging end (sash, belt) at (x, y): native's breechcloth flap, narrow, moved off
    the middle."""
    obj = native._flap(ch, name, y, z0, z1, w, w * 0.9, out, rows=4, cols=2)
    for v in obj.data.vertices:
        v.co.x += x
    return obj


def dress(ch, s):
    """The Southwest garments of SPEC `s` (after native's)."""
    d = s["pdress"]
    knee = ch.arm.data.bones[ch.prefix + "shin_l"].head_local.z
    shirt = "shirt" in s["dress"]
    if "wraps" in d:
        ch.shell("wraps", "pue_wraps", lambda c, n, h, p: h in {"shin_l", "shin_r", "foot_l", "foot_r"}
                 and 0.07 < c.z < knee - 0.02, 0.011)
    if "tunic" in d:
        ch.shell("tunic", "pue_tunic", lambda c, n, h, p: h in native.TORSO and 0.90 < c.z < 1.50
                 and not (h in {"clavicle_l", "clavicle_r"} and abs(c.x) > 0.17), 0.012)
    if "jerkin" in d:
        sleeves = s["sleeves"]
        ch.shell("jerkin", "pue_jerkin", lambda c, n, h, p: (h in native.TORSO and 0.86 < c.z < 1.50
                                                              and not (c.y < -0.05 and abs(c.x) < 0.035 and c.z < 1.40))
                 or (sleeves and h in native.ARMS and c.z > 1.02), 0.026 if shirt else 0.014)
    if "kilt" in d:
        kilt = bd.tube(ch.arm, f"{ch.name}.kilt", 1.01, KILT_HEM, (0.158, 0.122), (0.205, 0.165),
                       columns=rig.SKIRT_COLUMNS, prefix=ch.prefix)
        ch.add("clothes", kilt, "pue_kilt", symmetric=True)
    if "sash" in d:
        off = 0.040 if "kilt" in d else (0.026 if shirt else 0.014)
        ch.shell("sash", "pue_sash", lambda c, n, h, p: h in {"hips", "spine"} and 0.96 < c.z < 1.06, off)
        ch.add("clothes", _strip(ch, "sash_end", -0.07, -0.135 - (0.03 if "kilt" in d else 0.0), 1.00, 0.66, 0.07, 0.04),
               "pue_sash")
    if "bandolier" in d:
        ch.shell("bandolier", "pue_sash", lambda c, n, h, p: h in native.TORSO | {"upperarm_l"}
                 and abs((c.z - 1.0) - 0.75 * (c.x + 0.17)) < 0.045 and c.z > 0.94, 0.040)
    if "blanket" in d:
        # A blanket round the shoulders and down the back to the knee, open in front.
        ch.shell("blanket", "pue_blanket", lambda c, n, h, p: (h in native.TORSO and 0.80 < c.z < 1.55
                                                                and (c.y > -0.03 or abs(c.x) > 0.15 or c.z > 1.42))
                 or (h in {"upperarm_l", "upperarm_r"}) or (h in {"thigh_l", "thigh_r"} and c.y > 0.0 and c.z > 0.55),
                 0.040)


def _bead_string(bm, n, half_w, drop, y0, z0, size):
    for i in range(n):
        t = -1.0 + 2.0 * i / (n - 1)
        x = half_w * math.sin(t * math.pi / 2)
        z = z0 + drop * (1.0 - t * t) * -1.0
        y = y0 + 0.07 * t * t
        native._box(bm, Vector((x, y, z)), (size, size, size))


def hair(ch, s):
    """Bangs (cut at the brow and the jaw at the sides), the back hair loose or tied in a
    chongo at the nape, a cloth headband: rigid on the head (gear layer, part 1)."""
    crown, head = native._skull(ch)
    centre = Vector((0.0, head.y + 0.01, (crown.z + head.z) / 2 + 0.01))
    made = []
    bm = bmesh.new()
    # Bangs over the brow, the sides to the jaw, the back hair to the shoulders (sizes as
    # native's `long` hair, whose box sits on the same head).
    y0, z0 = centre.y, centre.z
    native._box(bm, Vector((0, y0 - 0.088, z0 + 0.055)), (0.15, 0.035, 0.05))
    native._box(bm, Vector((0, y0 + 0.005, z0 + 0.075)), (0.165, 0.18, 0.05))
    for x in (-1, 1):
        native._box(bm, Vector((x * 0.078, y0 + 0.005, z0 - 0.005)), (0.028, 0.15, 0.13))
    if s["phair"] == "chongo":
        knot = Vector((0, y0 + 0.095, z0 - 0.06))
        native._box(bm, knot, (0.10, 0.06, 0.075))
        native._box(bm, Vector((0, y0 + 0.075, z0 + 0.0)), (0.15, 0.04, 0.10))
    else:
        native._box(bm, Vector((0, y0 + 0.075, z0 - 0.10)), (0.17, 0.05, 0.30))
    made.append(("hair", gear.from_bm(bm, f"{ch.name}.pueblo_hair")))
    if s["phair"] == "chongo":
        bm = bmesh.new()
        native._box(bm, Vector((0, centre.y + 0.097, centre.z - 0.06)), (0.105, 0.065, 0.02))
        made.append(("pue_band", gear.from_bm(bm, f"{ch.name}.chongo_tie")))
    if s["headband"]:
        bm = bmesh.new()
        segs, rb, zb = 16, 0.104, centre.z + 0.035
        for k in range(segs):
            a0, a1 = 2 * math.pi * k / segs, 2 * math.pi * (k + 1) / segs
            for rr, flip in ((rb, False), (rb - 0.004, True)):
                ring = [bm.verts.new((centre.x + math.sin(a) * rr, centre.y - math.cos(a) * rr * 1.12, zb + dz))
                        for a in (a0, a1) for dz in (-0.024, 0.024)]
                q = (ring[0], ring[2], ring[3], ring[1])
                bm.faces.new(q[::-1] if flip else q)
        # The knot at the left side and its two ends hanging.
        side = centre + Vector((rb, 0.02, zb - centre.z))
        native._box(bm, side, (0.02, 0.04, 0.035))
        for dy in (-0.012, 0.015):
            native._card(bm, side + Vector((0.006, dy, -0.01)), Vector((0.15, 0.2, -1)), Vector((0, 1, 0)), 0.13, 0.03,
                         taper=0.8)
        made.append(("pue_band", gear.from_bm(bm, f"{ch.name}.headband")))
    for key, o in made:
        gear.rigid(o, ch.arm, ch.prefix + "head", Matrix.Identity(4), part=1)
        ch.add("gear", o, key)
    return made


def kit(ch, s):
    """Necklaces (rigid on the chest) and the shaman's gourd rattle."""
    out = {}
    name = ch.name
    shirt = "shirt" in s["dress"] or "tunic" in s["pdress"]
    lift = 0.012 if shirt else 0.0
    for i, k in enumerate(s["necklace"]):
        bm = bmesh.new()
        if k == "turquoise":
            _bead_string(bm, 15, 0.075, 0.075, -0.128 - lift, 1.455, 0.011)
        else:
            _bead_string(bm, 19, 0.085, 0.105, -0.132 - lift, 1.46, 0.012)
            native._box(bm, Vector((0, -0.136 - lift, 1.338)), (0.035, 0.008, 0.035))  # a shell pendant
        o = gear.from_bm(bm, f"{name}.necklace_{k}")
        gear.rigid(o, ch.arm, ch.prefix + "spine2", Matrix.Identity(4))
        out[k] = ch.add("gear", o, k)
    if s["gourd"]:
        g = gear.held(gourd(), f"{name}.gourd")
        sock = am.world(ch.arm, "socket.hand_l")
        rest_up = sock.to_3x3() @ ch.arm.pose.bones["socket.hand_l"].bone.matrix_local.to_3x3().inverted()
        am.attach(g, ch.arm, "socket.hand_l", Matrix.Translation(sock.translation) @ rest_up.to_4x4())
        ch.held["rattle"] = g
        ch.keys[g.name] = "gourd"
    return out


class hooks:
    """native's `dress`, `hair` and `kit` with the Pueblo parts added, and the stone club
    in place of the Woodlands war club, for the duration of a native call."""

    def __enter__(self):
        self.saved = native.dress, native.hair, native.kit, held.ITEMS["war_club"]
        dress0, hair0, kit0 = self.saved[:3]

        def d(ch, s):
            dress0(ch, s)
            dress(ch, s)

        def h(ch, s):
            made = hair0(ch, dict(s, hair="none"))
            return made + hair(ch, s)

        def k(ch, s):
            out = kit0(ch, s)
            out.update(kit(ch, s))
            return out

        native.dress, native.hair, native.kit = d, h, k
        held.ITEMS["war_club"] = stone_club
        return self

    def __exit__(self, *exc):
        native.dress, native.hair, native.kit, held.ITEMS["war_club"] = self.saved
        return False


def make(root, name, s):
    if s["peasant"]:
        return make_peasant(root, name, s)
    with hooks():
        return native.make(root, name, s)


def build(ctx, s):
    """Builds the Pueblo unit of SPEC `s` (the peasant by `build_peasant`)."""
    if s["peasant"]:
        return build_peasant(ctx, s)
    with hooks():
        native.build(ctx, s)


def pose_for(ch, s, clip):
    if s["peasant"]:
        return civil.pose_for(ch, dict(s, kind="peasant"), clip)
    native.pose_for(ch, s, clip)


# -- the peasant: native body, civil's tools and work clips --------------------------------

def make_peasant(root, name, s):
    ch = Character(root, name, s["build"], table=civil.skeleton())
    ch.body()
    with hooks():
        native.dress(ch, s)
        ch.bone_layer()
        ch.caps()
        native.hair(ch, s)
        native.kit(ch, s)
    am.pose(ch.arm, cl.bare(civil.STAND))
    civil._hold_tools(ch, name, civil.HAND_TOOLS + tuple(civil.SHOULDER))
    ch.paint_masks = native.body_paint(s["paint"], s["palette"]) if s["paint"] else []
    return ch


def build_peasant(ctx, s):
    ch = make_peasant(ctx.root, ctx.name, s)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    clips = civil.peasant_clips(s["md"])
    for clip, keys in clips.items():
        cl.apply(ch.arm, clip, keys)
    for clip, keys in crawl.clips().items():
        keys = list(keys)
        keys[0] = (keys[0][0], civil._show(keys[0][1], ()))
        cl.apply(ch.arm, clip, keys)
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    with native.clean_shaven(scalp=True):
        heads.finish(ch)
    ch.arm.animation_data.action = None
    for pb in ch.arm.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)
    am.pose(ch.arm, cl.bare(civil.STAND))
    tris = ch.triangles()
    names = list(clips) + list(crawl.CLIPS)
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=names,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), acr_family="native")
    ctx.stats = {"triangles": tris, "light": report, "colours": colours}

