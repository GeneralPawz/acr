"""European line infantry of the 18th century as one family (docs/humans-roster.md): the
sewn uniform of docs/clothing.md (shirt and stock, waistcoat, coat with facings and
turnbacks, breeches, gaiters, shoes), crossbelts, a hat, strapped gear and a musket with
the musketeer clips. A unit script is data: its `.MD` clip lines and a `SPEC` of
parameters (`spec`), then `build(ctx)` calls `line.build(ctx, SPEC)`.

`SPEC` keys (all but `md` optional; `spec` fills the defaults):
- `md`: the `.MD` animation lines by clip, as `poses.musketeer_clips` keys them;
- `palette`: colours by material key (sRGB) over `PALETTE`: the nation's coat, the
  regiment's `facings`, the `lining` of the turnbacks, `waistcoat`, `breeches`, `gaiters`,
  `belts`, `hat`, ... Sprite colours are mixed in with `mix`;
- `lapels`: the material key of the coat's lapels: `facings` (the uniform's draft) or
  `lapels` (an own colour, `palette["lapels"]`: FrnM18's white lapels over the white
  waistcoat, a coat drafted through `drape.dress(patterns=...)`);
- `fixed`: palette keys that keep their palette colour (no sprite mix), e.g. FrnM18's white
  waistcoat, which the projection would tint with the coat;
- `build`: body build (`Character` build; `garments` False builds the shells of #178);
- `fit`: part sizes fitted to the sprite silhouette (hat, musket, pack, shell skirt);
- `hat`: a registered hat (`registry.hat`): `tricorn`, `slouch` or `cocked` (`gear.py`);
- `gear`: registered gear (`registry.gear`): the straps over the draped coat
  (`straps.py`: `crossbelts`, `belt_l`, `belt_r`) and strapped gear: `cartridge`, `knapsack`;
- `weapon`: `musket` (with rammer, aim, reload and charge clips), `drum` (the side drum
  on the hips bone below the `belt_r` carriage belt, sticks in both hands,
  `clips.drummer_clips`, a thrust for `0 PIKA`) or `colour` (the regimental colour on its
  pike in the right hand, `clips.colour_clips`);
- `flag`: the colour's design (`FLAGS`: `union`, `france`, `burgundy`, `stripes`), painted on
  the cloth in its own frame over `palette["flag"]` (the field);
- `sidearm`: None or `sword`, `hanger`, `tomahawk`: a second weapon drawn for melee on its
  own right-hand socket (`socket.blade_r`, like the dragoon's sabre); in the strike clips
  (`ATTACK`, `ATTACK_*`) it is shown and the musket hidden in the hand and slung on the back
  (`socket.sling`), every other clip the other way round (`add_sidearm`, `sidearm_strikes`);
- `lace`: None (no lace) or {"hat": braid width in m on the tricorn's brim, "buttonholes":
  True for bars of lace on the lapels and cuffs}, in `palette["lace"]` (`masks.py`).

The family registers its hats and strapped gear with `registry` at import (the example for
the other family modules: docs/assets-humans.md, "Family registry").
"""

import math

from mathutils import Matrix, Vector

from . import aim as aiming
from . import armature as am
from . import body as bd
from . import clips as wclips
from . import crawl, faces, gear, heads, held, masks, outfit, poses, registry, rig, straps
from .humans import LAYER_OF, Character, prepare_export

# The English musketeer's colours (`EngM18`): the defaults a nation overrides.
PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69),
    "coat": (0.66, 0.09, 0.06), "waistcoat": (0.86, 0.85, 0.80), "breeches": (0.86, 0.85, 0.80),
    "gaiters": (0.10, 0.10, 0.10), "belts": (0.85, 0.75, 0.30), "hat": (0.07, 0.07, 0.07),
    "cartridge": (0.08, 0.07, 0.06), "knapsack": (0.40, 0.27, 0.14), "wood": (0.42, 0.24, 0.12),
    "metal": (0.55, 0.55, 0.57),
    # Draped garments (docs/clothing.md): regimental facings, the lining of the turnbacks,
    # shirt, black stock and shoes.
    "facings": (0.13, 0.17, 0.36), "lining": (0.88, 0.86, 0.80), "shirt": (0.92, 0.91, 0.88),
    "stock": (0.08, 0.08, 0.08), "shoes": (0.08, 0.07, 0.06),
}
BUILD = {"width": 1.02, "shoulders": 1.12, "limbs": 1.23, "hips": 1.0, "garments": True}
FIT = {
    "hat_z": -0.02, "hat_scale": 0.925, "musket_len": 1.69,
    "skirt_top": [0.245, 0.175], "skirt_bot": [0.21, 0.15], "coat": 0.033,
    "pack": [0.32, 0.10, 0.32], "pack_y": 0.1925, "skirt_gap": 72,
}
DEFAULTS = {"build": BUILD, "fit": FIT, "hat": "tricorn", "gear": ("crossbelts", "cartridge", "knapsack"),
            "weapon": "musket", "mix": 0.5, "fixed": (), "lapels": "facings", "lace": None, "flag": None, "sidearm": None}


def _cartridge(ch, name, s):
    box = gear.box(f"{name}.cartridge_box", (0.16, 0.06, 0.12))
    gear.rigid(box, ch.arm, "hips", Matrix.Translation((-0.15, 0.17, 0.97)) @ Matrix.Rotation(math.radians(30), 4, "Z"))
    return ch.add("gear", box, "cartridge")


def _knapsack(ch, name, s):
    pack = gear.box(f"{name}.knapsack", tuple(s["fit"]["pack"]))
    gear.rigid(pack, ch.arm, "spine2", Matrix.Translation((0, s["fit"]["pack_y"], 1.28)))
    return ch.add("gear", pack, "knapsack")


registry.hat("tricorn", gear.tricorn)
registry.hat("slouch", gear.slouch_hat)
registry.hat("cocked", gear.cocked_hat)
registry.gear("cartridge", _cartridge)
registry.gear("knapsack", _knapsack)
# Lapels in an own colour: an own atlas cell, painted like the coat's broadcloth.
registry.painter("lapels", like="wool", finish=(1.0, 0.45, 0.0))
registry.material("lapels", "wool", painter="lapels", fallback=(0.86, 0.85, 0.80))


def _coat_with(key):
    """The uniform coat's draft with the lapels cut from cloth of material `key`."""
    from . import garments as gm

    def draft(m):
        panels, seams = gm.coat(m)
        panels["lapel"].key = key
        return panels, seams

    return draft
STRAPS = straps.STRAPS  # gear drawn with the cloth, before the bone layer

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}


def spec(md, palette=None, build=None, fit=None, **kw):
    """A full SPEC: the defaults with `palette`, `build`, `fit` merged key by key and the
    other keys (`hat`, `gear`, `weapon`, `mix`, `fixed`) replaced."""
    unknown = set(kw) - set(DEFAULTS)
    if unknown:
        raise KeyError(f"unknown SPEC keys {sorted(unknown)}")
    return dict(DEFAULTS, **kw, md=md, palette=dict(PALETTE, **(palette or {})), build=dict(BUILD, **(build or {})),
                fit=dict(FIT, **(fit or {})))


def make(root, name, s):
    """The soldier of SPEC `s` under `root`: body, uniform, belts, strapped and held gear."""
    build, fit = s["build"], s["fit"]
    ch = Character(root, name, build)
    ch.body()

    def front(c):
        return c.y < -0.04 and abs(c.x) < 0.075 and c.z < 1.41

    if build.get("garments"):
        # Sewing patterns draped on the body (docs/clothing.md); shoes over a last.
        from . import drape, mbody

        ch.shoes = ch.add("clothes", mbody.shoes(ch.arm, ch.flesh, f"{name}.footwear", ch.prefix, key="shoes"),
                          "shoes", symmetric=True)
        ch.drape_report = {}
        patterns = {"coat": _coat_with(s["lapels"])} if s["lapels"] != "facings" else None
        made = drape.dress(ch, report=ch.drape_report, patterns=patterns)
        cloth = [o for o in ch.layers["clothes"] if o.name in made]
        drape.hide_skin(ch, cloth)
        # Covered under-layers keep a margin below the layers on them (docs/clothing.md, "Layers").
        fixed = [o for o in cloth if not o.name.endswith(("coat_skirts", "turnbacks"))]
        for inner, outer, depth in (("waistcoat", ("coat", "facings", "lapels"), 0.006),
                                    ("shirt", ("coat", "facings", "lapels"), 0.004),
                                    ("breeches", ("waistcoat", "gaiters"), 0.004)):
            for o in fixed:
                if made[o.name] == inner:
                    drape.hide_under(ch, o, [x for x in fixed if made[x.name] in outer], depth)
        surf = drape.surface(cloth, f"{name}.belt_surface")
        drape.buttons(ch, cloth, name)
        offsets = (0.014, 0.018)
    else:
        ch.shell("waistcoat", "waistcoat", lambda c, n, h, p: h in TORSO and 0.97 < c.z < 1.47, 0.012)
        ch.shell("coat", "coat", lambda c, n, h, p: (h in TORSO and 0.97 < c.z < 1.49 and not front(c)) or h in ARMS, fit["coat"])
        ch.shell("cuffs", "coat", lambda c, n, h, p: h in {"forearm_l", "forearm_r"} and c.z < 1.02, 0.045)
        ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0) or (h in SHINS and c.z > 0.40), 0.010)
        ch.shell("gaiters", "gaiters", lambda c, n, h, p: (h in SHINS and c.z <= 0.44) or h in FEET, 0.016)
        skirt = bd.tube(ch.arm, f"{name}.coat_skirt", rig.SKIRT_TOP, rig.SKIRT_BOTTOM, fit["skirt_top"], fit["skirt_bot"],
                        gap_deg=fit["skirt_gap"], flare_back=0.03, columns=rig.SKIRT_COLUMNS)
        ch.add("clothes", skirt, "coat", symmetric=True)
        surf, offsets = ch.flesh, (0.038, 0.042)
    # Straps over the clothes (`straps.py`, registered gear): they follow the draped cloth
    # and take the weights of the garments under them (they move with the coat).
    ch.strap_surface, ch.strap_offsets = surf, offsets
    ch.strap_under = fixed if build.get("garments") else None
    registry.build_gear([g for g in s["gear"] if g in STRAPS], ch, name, s)
    ch.bone_layer()
    ch.caps()

    # Strapped gear (rest pose, rigid on deforming bones: follows `deflate`).
    registry.build_gear([g for g in s["gear"] if g not in STRAPS], ch, name, s)

    # Held gear on sockets, placed in the standing pose (drops at death).
    am.pose(ch.arm, poses.STAND_MUSKET)
    lace = s["lace"] or {}
    hat = registry.build_hat(s["hat"], f"{name}.{s['hat']}", **({"lace": lace["hat"]} if lace.get("hat") else {}))
    am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((0, 0, fit["hat_z"])))
              @ Matrix.Rotation(math.radians(-6), 4, "X") @ Matrix.Scale(fit["hat_scale"], 4))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "hat"
    # Lace (`masks.lace`): braid round the brim, bars on the buttonholes of lapels and cuffs.
    ch.paint_masks = masks.lace(s["palette"], hat=hat.name if hat.get("acr_lace") else None,
                                width=lace.get("hat") or 0.012, buttonholes=lace.get("buttonholes", False)) if lace else []
    if s["weapon"] == "drum":
        _drum(ch, name)
    elif s["weapon"] == "colour":
        _colour(ch, name, s)
    elif s["weapon"] == "musket":
        gun = gear.musket(f"{name}.musket", length=fit["musket_len"], bayonet=0.0)
        gun["acr_material_keys"] = ["wood", "metal"]
        # The socket's orientation is "upright" in its rest frame; the clips turn it.
        sock = am.world(ch.arm, "socket.hand_r")
        rest_up = sock.to_3x3() @ ch.arm.pose.bones["socket.hand_r"].bone.matrix_local.to_3x3().inverted()
        am.attach(gun, ch.arm, "socket.hand_r", Matrix.Translation(sock.translation + Vector((0.0, -0.01, 0.0))) @ rest_up.to_4x4())
        ch.held["musket"] = gun
        ch.keys[gun.name] = "wood"
    else:
        raise ValueError(f"line infantry weapon {s['weapon']!r}")
    if s.get("sidearm"):
        add_sidearm(ch, name, s["sidearm"], fit["musket_len"])
    return ch


# -- sidearm: a second weapon drawn for melee (m2-shared-moves) ---------------------------

# Sidearm kind: (held item, melee clips of `clips.MELEE`).
SIDEARMS = {"sword": ("smallsword", "sword"), "hanger": ("hanger", "sword"), "tomahawk": ("tomahawk", "tomahawk")}
BLADE = "socket.blade_r"
SLING = "socket.sling"
# The slung musket on the back: its middle between the shoulder blades, the muzzle up over
# the right shoulder (`SLING_TILT` degrees from upright), the barrel against the coat.
SLING_AT = (0.0, 0.20, 1.18)
SLING_TILT = -28
# Sockets shown: in the strike clips and in every other clip (the ramrod's socket,
# `aim.ROD_SOCKET`, hidden with the musket in the hand).
SHOW_MELEE = {"socket.hand_r": 0, "socket.hand_l": 0, BLADE: 1, SLING: 1}
SHOW_REST = {"socket.hand_r": 1, "socket.hand_l": 1, BLADE: 0, SLING: 0}


def _add_sockets(arm_obj, prefix=""):
    """Adds `socket.blade_r` (beside `socket.hand_r`, same frame) and `socket.sling` (on
    `spine2`, on the back) to a built armature; non-deforming."""
    import bpy

    bpy.context.view_layer.objects.active = arm_obj
    bpy.ops.object.mode_set(mode="EDIT")
    ebs = arm_obj.data.edit_bones
    hs = ebs[prefix + "socket.hand_r"]
    if prefix + BLADE not in ebs:
        eb = ebs.new(prefix + BLADE)
        eb.head, eb.tail, eb.roll = hs.head.copy(), hs.tail.copy(), hs.roll
        eb.parent, eb.use_deform = hs.parent, False
    if prefix + SLING not in ebs:
        eb = ebs.new(prefix + SLING)
        eb.head, eb.tail = Vector(SLING_AT), Vector(SLING_AT) + Vector((0.0, 0.0, 0.1))
        eb.parent, eb.use_deform = ebs[prefix + "spine2"], False
    bpy.ops.object.mode_set(mode="OBJECT")


def add_sidearm(ch, name, kind, musket_len):
    """The sidearm `kind` (`SIDEARMS`) on `socket.blade_r` and a second musket slung on the
    back (`socket.sling`); the clips key which is shown (`sidearm_strikes`, `key_show`)."""
    import bpy

    item, _melee = SIDEARMS[kind]
    _add_sockets(ch.arm, ch.prefix)
    am.pose(ch.arm, {})
    w = gear.hold(ch, item, f"{name}.{kind}", ch.prefix + BLADE, slot="sidearm")
    gun = gear.musket(f"{name}.musket_slung", length=musket_len, bayonet=0.0)
    for c in list(gun.children):  # the muzzle empty: the slung musket is no weapon
        bpy.data.objects.remove(c)
    gun["acr_material_keys"] = ["wood", "metal"]
    t = math.radians(SLING_TILT)
    axis = Vector((math.sin(t), 0.0, math.cos(t)))
    grip = Vector(SLING_AT) - axis * (musket_len / 2 - 0.25)
    am.attach(gun, ch.arm, ch.prefix + SLING, Matrix.Translation(grip) @ Matrix.Rotation(t, 4, "Y")
              @ Matrix.Rotation(math.radians(180), 4, "Z"))
    ch.held["musket_slung"] = gun
    ch.keys[gun.name] = "wood"
    ch.sidearm = kind
    am.pose(ch.arm, poses.STAND_MUSKET)
    return w


def with_sidearm(make_fn, kind):
    """`make_fn(root, name, s)` (a family's `make`, e.g. `frontier.make`) with the sidearm
    `kind` added (`add_sidearm`); for `frontier.build(ctx, s, make_fn=...)`."""

    def make_armed(root, name, s):
        ch = make_fn(root, name, s)
        add_sidearm(ch, name, kind, s.get("gun") or FIT["musket_len"])
        return ch

    return make_armed


def sidearm_strikes(md, kind):
    """{clip: Clip} of the sidearm's blows on ATTACK's frames (`clips.MELEE`, the weapon's
    point from `held.py`), every key showing the sidearm and the slung musket; ATTACK plays
    the first (a cut)."""
    item, melee = SIDEARMS[kind]
    if "ATTACK" not in md:
        return {}
    tip = tuple(held.build(item).sockets["socket.tip"])  # the point in the item frame
    strikes = dict(wclips.MELEE[melee](wclips.md_frames(md["ATTACK"]), tip=tip))
    strikes["ATTACK"] = strikes["ATTACK_CUT_MID"]
    memo = {}

    def drawn(pose):
        if id(pose) not in memo:
            p = dict(pose)
            world = dict(p.get("world", {}))
            if "socket.hand_r" in world:
                world[BLADE] = world["socket.hand_r"]
            p["world"] = world
            p["show"] = dict(SHOW_MELEE)
            memo[id(pose)] = p
        return memo[id(pose)]

    return {k: wclips.Clip([(f, drawn(p)) for f, p in c], c.event) for k, c in strikes.items()}


def key_show(arm_obj, act, show, prefix=""):
    """Keys the socket scales `show` ({socket: 0 | 1}) constant over action `act`."""
    arm_obj.animation_data_create()
    arm_obj.animation_data.action = act
    if hasattr(arm_obj.animation_data, "action_slot") and act.slots:
        arm_obj.animation_data.action_slot = act.slots[0]
    for sock, v in show.items():
        pb = arm_obj.pose.bones.get(prefix + sock)
        if pb is None:
            continue
        pb.scale = (float(v),) * 3
        pb.keyframe_insert("scale", frame=0, group=pb.name)
    for pb in arm_obj.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)
    arm_obj.animation_data.action = None


def unscale(arm_obj):
    """Every pose bone's scale back to 1 (a socket left hidden by the last keyed clip would
    bake a 0 scale into its held item at export: docs/melee.md, m2-melee4)."""
    for pb in arm_obj.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)


# -- drummers and standard bearers (family 2) ---------------------------------------------

# The colour's cloth in an own material and painter cell, so the nation's design can be
# painted on it (`held.colour` keeps the cloth in the shared `colour` key, flat).
registry.painter("flag", like="linen", finish=(1.0, 0.55, 0.15))
registry.material("flag", "linen", painter="flag", fallback=(0.70, 0.12, 0.10))
registry.material("flag_a", "linen", fallback=(0.92, 0.90, 0.84))  # the design's first colour
registry.material("flag_b", "linen", fallback=(0.66, 0.08, 0.08))  # its second
registry.material("flag_c", "linen", fallback=(0.10, 0.16, 0.42))  # its third

# The cloth of `held.colour` in its item frame: flying to +X from the staff, the top edge
# at FLAG_TOP, `FLAG_SIZE` (fly, hoist) m.
FLAG_TOP = 2.9 - 0.9 - 0.04
FLAG_SIZE = (1.8, 1.65)


def _flag_uv(p):
    """(u, v) on the cloth: u 0 at the staff to 1 at the fly, v 0 at the top to 1 below."""
    return (p[:, 0] - 0.02) / FLAG_SIZE[0], (FLAG_TOP - p[:, 2]) / FLAG_SIZE[1]


def _band(np, d, half, soft=0.004):
    """1 inside |d| < half, soft at the edge."""
    return np.clip((half - np.abs(d)) / soft + 0.5, 0.0, 1.0)


def flag_cross(p, n, L, seed, width=0.10):
    """An upright cross through the centre (St George's, the French white cross); `width`
    a fraction of the fly."""
    import numpy as np

    u, v = _flag_uv(p)
    h = FLAG_SIZE[1] / FLAG_SIZE[0]
    return np.maximum(_band(np, u - 0.5, width / 2), _band(np, (v - 0.5) * h, width / 2))


def flag_saltire(p, n, L, seed, width=0.10, ragged=False):
    """A diagonal cross corner to corner (St Andrew's; with `ragged` the Cross of Burgundy,
    knots of cut-off branches along each arm)."""
    import numpy as np

    u, v = _flag_uv(p)
    x, y = (u - 0.5) * FLAG_SIZE[0], (v - 0.5) * FLAG_SIZE[1]
    a = math.atan2(FLAG_SIZE[1], FLAG_SIZE[0])
    d1 = x * math.sin(a) - y * math.cos(a)
    d2 = x * math.sin(a) + y * math.cos(a)
    half = width * FLAG_SIZE[0] / 2
    if not ragged:
        return np.maximum(_band(np, d1, half, 0.01), _band(np, d2, half, 0.01))
    t1 = x * math.cos(a) + y * math.sin(a)
    t2 = x * math.cos(a) - y * math.sin(a)
    half1 = half * (1.0 + 0.6 * (np.cos(t1 * 2 * math.pi / 0.21) > 0.6))
    half2 = half * (1.0 + 0.6 * (np.cos(t2 * 2 * math.pi / 0.21 + 1.0) > 0.6))
    return np.maximum(_band(np, d1, half1, 0.01), _band(np, d2, half2, 0.01))


def flag_stripes(p, n, L, seed, count=13):
    """Every other one of `count` horizontal stripes (the 2nd, 4th, ...)."""
    import numpy as np

    _u, v = _flag_uv(p)
    k = np.floor(np.clip(v, 0.0, 0.9999) * count)
    return ((k % 2) == 1).astype(np.float32)


def flag_canton(p, n, L, seed, w=0.42, h=7 / 13):
    """The upper quarter at the staff."""
    import numpy as np

    u, v = _flag_uv(p)
    return ((u < w) & (v < h)).astype(np.float32)


for _name, _fn in (("flag_cross", flag_cross), ("flag_saltire", flag_saltire), ("flag_stripes", flag_stripes),
                   ("flag_canton", flag_canton)):
    registry.mask(_name, _fn)

# Colour designs, simple and of the period: [(mask, palette key, params)] over the field
# (`palette["flag"]`).
FLAGS = {
    # The King's colour (the Union of 1707-1801, without St Patrick's saltire): blue field,
    # St Andrew's white saltire, St George's red cross fimbriated white.
    "union": [("flag_saltire", "flag_a", {"width": 0.11}), ("flag_cross", "flag_a", {"width": 0.20}),
              ("flag_cross", "flag_b", {"width": 0.12})],
    # A French regimental colour: the white cross through coloured quarters.
    "france": [("flag_cross", "flag_a", {"width": 0.13})],
    # The Spanish infantry's colour: the red ragged Cross of Burgundy on white.
    "burgundy": [("flag_saltire", "flag_b", {"width": 0.06, "ragged": True})],
    # A Continental colour (1775-77): thirteen red and white stripes, a plain blue canton.
    "stripes": [("flag_stripes", "flag_a", {}), ("flag_canton", "flag_c", {})],
}


def flag_masks(design, palette, cloth):
    """`ch.paint_masks` entries of colour design `design` on the cloth object `cloth`."""
    return [masks.entry(m, ("flag",), tuple(palette.get(k) or registry.FALLBACK[k]), frame=cloth, **params)
            for m, k, params in FLAGS[design]]


def _subset(it, parts, name):
    """The faces of item `it` in `parts` as an own `held.Item` (sockets and groups kept)."""
    out = held.Item(name)
    out.keys = {p: k for p, k in it.keys.items() if p in parts}
    remap = {}
    for f, part in zip(it.faces, it.parts):
        if part not in parts:
            continue
        for i in f:
            if i not in remap:
                remap[i] = out.v(it.verts[i])
        out.f([remap[i] for i in f], part)
    out.sockets = dict(it.sockets)
    groups = {g: [remap[i] for i in idx if i in remap] for g, idx in it.groups.items()}
    out.groups = {g: idx for g, idx in groups.items() if idx}
    return out


def _drum(ch, name):
    """The side drum on the hips (its head at `clips.DRUM_HEAD`, hanging from the
    `belt_r` carriage belt on the left thigh) and a drumstick in each hand."""
    import bpy

    am.pose(ch.arm, {})
    it = held.build("drum")
    drum = gear.held(it, f"{name}.drum")
    for c in list(drum.children):  # the item's socket empties: not exported
        bpy.data.objects.remove(c)
    r = am.euler(wclips.DRUM_ROT)
    m = Matrix.Translation(Vector(wclips.DRUM_HEAD) - r @ Vector(it.sockets["socket.head"])) @ r.to_4x4()
    # On the hips bone like a held item (one material per part: shell, hoops, vellum, cords;
    # rigid gear would take one colour); it falls with the man.
    am.attach(drum, ch.arm, "hips", m)
    ch.held["drum"] = drum
    ch.keys[drum.name] = "facings"
    for side in ("r", "l"):
        gear.hold(ch, "drumstick", f"{name}.stick_{side}", f"socket.hand_{side}", slot=f"stick_{side}")


def _colour(ch, name, s):
    """The regimental colour on its pike in the right hand: staff and finial one held
    object, the cloth another (key `flag`, the nation's design painted on it)."""
    am.pose(ch.arm, {})
    it = held.build("colour")
    staff = _subset(it, {"staff", "finial"}, "colour")
    cloth = _subset(it, {"cloth"}, "colour_cloth")
    cloth.keys = {"cloth": "flag"}
    gear.hold(ch, staff, f"{name}.colour", "socket.hand_r", slot="colour")
    flag = gear.hold(ch, cloth, f"{name}.colour_cloth", "socket.hand_r", slot="colour_cloth")
    ch.flag = flag.name


def support_clips(md, active=None):
    """{clip: Clip} of a drummer (`weapon` drum) or standard bearer (colour): the clips.py
    builders, the backward walk (the forward clips reversed, as the `.MD` plays them), the
    `0 PIKA` stances (the stand) and the fall from the free stand."""
    drummer = "MOTION_L0" in md or "PSTAND" in md
    out = wclips.drummer_clips(md) if drummer else wclips.colour_clips(md)
    stand = out["STAND"][0][1]
    for sfx in ("", "0"):
        for back, fwd in (("MOTION_LB", "MOTION_R"), ("MOTION_RB", "MOTION_L")):
            if f"{back}{sfx}" in md and f"{fwd}{sfx}" in out:
                c = out[f"{fwd}{sfx}"]
                last = c[-1][0]
                out[f"{back}{sfx}"] = [(last - f, p) for f, p in reversed(c)]
    if drummer:
        # `0 PIKA`: a thrust on ATTACK's frames (the stick jabs: the drummer's hanger stays
        # sheathed) and the strike clips of docs/melee.md with it.
        out.update(wclips.unit_clips(md, {0: "PIKA"}, melee="sword", active=active))
        for k in ("PSTAND", "PATTACK", "UATTACK"):
            if k in md:
                out[k] = wclips.frames(wclips.md_frames(md[k]), [(0, stand)])
    lie = poses.lie_prone()
    for k, keys in (("DEATH", poses.death_keys(wclips.STAND_FREE, lie)), ("DEATHLIE1", [(0.0, lie)])):
        if k in md:
            out[k] = rig.clip_keys(wclips.md_frames(md[k]), keys)
    return out


def _musket_clips(ctx, ch, md):
    """The musket's rammer and the musketeer clips (aim, reload, charge, crawl)."""
    # Ramrod (in its pipes; drawn in the reload of ATTACK1): on the free left-hand socket,
    # placed on every frame by `aim.bake_musket` / `aim.stow_rammer`.
    rod = gear.rammer(f"{ctx.name}.rammer")
    am.attach(rod, ch.arm, aiming.ROD_SOCKET, am.world(ch.arm, aiming.ROD_SOCKET))
    ch.held["rammer"] = rod
    ch.keys[rod.name] = "metal"
    heads.variants(ch)  # hair and beard variants (#283), before the morphs
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    # Present with both hands: butt in the shoulder, left hand under the barrel (IK).
    # Right elbow raised out to the side near shoulder height, left elbow under the barrel.
    aim = aiming.hold(ch.arm, poses.AIM_MUSKET, pocket=(0.06, 0.0, -0.02), elbow_r=(0.35, 0.10, -0.06))
    ctx.aim_errors = aiming.errors(ch.arm, aim)
    print("ACR aim errors (m):", ctx.aim_errors)
    # The reload drill after the shot (ATTACK1), each step IK-solved.
    reload = aiming.drill(ch.arm, poses.RELOAD_STEPS)
    # Charge bayonets: the same grip with the butt at the right hip.
    charge = aiming.hold(ch.arm, poses.CHARGE_MUSKET, pocket=(0.02, -0.12, -0.32), left_along=0.30)
    clips = poses.musketeer_clips(md, aim=aim, charge=charge, reload=reload)
    kind = getattr(ch, "sidearm", None)
    drawn = sidearm_strikes(md, kind) if kind else {}
    if drawn:
        # The musket's blows give way to the sidearm's (`add_sidearm`).
        clips = {k: v for k, v in clips.items() if k != "ATTACK" and not k.startswith("ATTACK_")}
    for clip, keys in clips.items():
        act = am.add_clip(ch.arm, clip, keys)
        if clip == "ATTACK1":
            aiming.bake_musket(ch.arm, clip, keys, rod_phases=poses.ROD_PHASES)
        else:
            aiming.stow_rammer(ch.arm, clip)
        if kind:
            key_show(ch.arm, act, SHOW_REST, ch.prefix)
    for clip, keys in drawn.items():
        wclips.apply(ch.arm, clip, keys)
    # Crawl clips of the wounded (#262): keyframed, looping.
    for clip, keys in crawl.clips().items():
        act = am.add_clip(ch.arm, clip, keys)
        aiming.stow_rammer(ch.arm, clip)
        if kind:
            key_show(ch.arm, act, SHOW_REST, ch.prefix)
    if kind:
        unscale(ch.arm)


def build(ctx, s):
    """Builds, animates, paints and prepares for export the soldier of SPEC `s`."""
    md = s["md"]
    ch = make(ctx.root, ctx.name, s)
    musket = s["weapon"] == "musket"
    extra = []
    if musket:
        _musket_clips(ctx, ch, md)
    else:
        if s["weapon"] == "colour" and s["flag"]:
            ch.paint_masks = list(ch.paint_masks) + flag_masks(s["flag"], s["palette"], ch.flag)
        heads.variants(ch)
        ch.morphs(extra_deflate=ch.layers["gear"])
        ch.masks()
        ctx.aim_errors = None
        for clip, keys in support_clips(md, s.get("active")).items():
            wclips.apply(ch.arm, clip, keys)
            if clip not in md:
                extra.append(clip)
        for clip, keys in crawl.clips().items():
            am.add_clip(ch.arm, clip, keys)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    # Realistic cloth, skin and leather painted over those colours (#217).
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)  # face and body morphs, `_FACE`, variants (#283)
    # The exported scene pose (glTF node transforms) is STAND.
    ch.arm.animation_data.action = None
    unscale(ch.arm)
    if musket:
        am.pose(ch.arm, poses.STAND_MUSKET)
        ch.arm.pose.bones[aiming.ROD_SOCKET].matrix = ch.arm.pose.bones["socket.hand_r"].matrix @ aiming.ROD_STOWED
    else:
        am.pose(ch.arm, wclips.bare(wclips.solve(ch.arm, support_clips(md)["STAND"][0][1])))
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=list(md) + extra + list(crawl.CLIPS),
                   acr_limb_radius=ch.limb_radius,
                   acr_light=report.get("light_dir_camera", []), acr_layers=list(ch.layers),
                   **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours, "aim_errors_m": ctx.aim_errors,
                 "drape": getattr(ch, "drape_report", None)}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    if s["weapon"] == "musket":
        keys = poses.musketeer_clips(s["md"])[clip]
    else:
        keys = support_clips(s["md"])[clip]
    pose = keys[-1][1] if clip == "DEATH" else keys[0][1]
    ch._pose_spec = pose if s["weapon"] == "musket" else wclips.bare(wclips.solve(ch.arm, pose))
    am.pose(ch.arm, ch._pose_spec)
