"""European civilians as one family (docs/humans-roster.md, family 13; docs/families/civil.md):
the peasants (`EngKri`, `FrnKri`, `SpnKri`) in sewn work clothes with their tools and work
clips, and the priest (`SpnSha`) in a Dominican habit with the heal clip.

A unit script is data: its `.MD` lines (`peasant_md` builds the peasants'), a palette and a
`SPEC` (`spec`); `build(ctx)` calls `civil.build(ctx, SPEC)`.

Peasants. The clothes are draped from the uniform's sewing patterns cut down for work
(`peasant_coat`): the coat without lapels, collar, cuffs and turnbacks, its front sewn shut,
is a shirt bloused at the waist (`skirts` False) or a smock-frock to mid-thigh (`skirts`
True); the body panels may take another cloth than the sleeves, which makes a sleeveless
waistcoat over shirt sleeves in one layer (FrnKri). Knee breeches, stockings (the gaiter
pattern without the garter), shoes, a sash band, a slouch hat.

Tools (`TOOLS`, held items as `held.Item`s registered here, not in `held.py`): felling axe
(`#ATTACK1`, wood and stone, and the club blows of `#ATTACK`), sickle (`#ATTACK3`, the
field), hammer (`#WORK`, building, kneeling), a grain sack (stance 2) and a bundle of
firewood (stance 4) on the shoulder. Each sits on its own socket bone (`socket.<tool>`,
a child of `socket.hand_r` or of `spine2`) whose scale every clip keys 0 or 1 (`show`).

Work clips follow the sprite packs (`EngKriU_Rub` chop, `EngKriU_Ubrk` reap, `EngKriU_Str`
hammer, `EngKriG_Urg` / `EngKriG_Drv` carry) and land their blows on the `.MD`'s sound
frames (`SOUND 15 #ATTACK1 TREE1`, `SOUND 5 #WORK STROIT`; `IMPACT`).

The priest: white habit to the ankle, black cappa (cloak) open at the front, a hood collar,
sandals, a red book in the left hand, a wooden cross on the chest; `clips.heal` for `#ATTACK`
(`MEDIK`). Clips with the pose data are pure Python (`peasant_clips`, `priest_clips`, tested
in `tests/test_civil.py`); the Blender side is `make` and `build`.
"""

import math

from . import clips as cl
from . import held, poses, registry, rig

# -- materials -------------------------------------------------------------------------------

# `outfit.dress` paints one colour per painter (one atlas cell), so every cloth of the family
# that must differ from another has its own painter, painted like a built-in one: the smock
# (`smock`), the sash (`band`), the waistcoat (`vest`), the stockings (`hose`), the habit and
# the cappa, the sack and the book.
for _key, _layer, _like, _fin, _rgb in (
        ("smock", "wool", "wool", (1.0, 0.40, 0.0), (0.36, 0.11, 0.09)),
        ("band", "wool", "wool", (1.0, 0.45, 0.0), (0.62, 0.07, 0.06)),
        ("vest", "wool", "kersey", (1.0, 0.30, 0.0), (0.20, 0.25, 0.31)),
        ("hose", "linen", "linen", (0.95, 0.20, 0.0), (0.78, 0.76, 0.70)),
        ("habit", "wool", "wool", (1.0, 0.40, 0.0), (0.86, 0.84, 0.78)),
        ("cappa", "wool", "wool", (1.0, 0.50, 0.0), (0.10, 0.10, 0.11)),
        ("sack", "linen", "linen", (0.95, 0.15, 0.0), (0.72, 0.64, 0.48)),
        ("book", "leather", "leather", (0.7, 0.0, 0.15), (0.55, 0.08, 0.07))):
    registry.painter(_key, like=_like, finish=_fin)
    registry.material(_key, _layer, painter=_key, fallback=_rgb)


def _slouch(name, **kw):
    from . import gear

    return gear.slouch_hat(name, **kw)


registry.hat("civil_slouch", _slouch)

# -- tools (held items) ------------------------------------------------------------------------

AXE_LEN = 0.82  # haft end to the head
AXE_TIP = 0.65  # grip to the edge (along the haft)
SICKLE_TIP = 0.30
HAMMER_TIP = 0.30


def axe(length=AXE_LEN):
    """A felling axe: ash haft, the grip 8 cm above its end, an iron head with the edge
    forward (-Y) and a flat poll behind (`socket.tip` at the middle of the edge)."""
    it = held.Item("axe")
    it.keys = {"haft": "wood", "head": "metal"}
    z1 = length - 0.10 - 0.08
    it.tube([((0, 0, -0.08), 0.017), ((0, 0, 0.30), 0.015), ((0, 0, z1 + 0.04), 0.016)], "haft", n=6)
    it.box((0, 0.015, z1), (0.032, 0.06, 0.07), "head")
    it.box((0, -0.065, z1), (0.014, 0.10, 0.06), "head", taper=(0.6, 1.0))
    it.box((0, -0.12, z1), (0.006, 0.02, 0.12), "head")
    it.sockets["socket.tip"] = (0.0, -0.13, z1)
    return it


def sickle():
    """A reaping sickle: a short wooden handle, the iron blade bent forward and round to the
    left (+X) in a half moon (`socket.tip` at the point)."""
    it = held.Item("sickle")
    it.keys = {"handle": "wood", "blade": "metal"}
    it.tube([((0, 0, -0.04), 0.016), ((0, 0, 0.09), 0.014)], "handle", n=6)
    pts = []
    for k in range(7):
        a = math.radians(-90 + 210 * k / 6)
        pts.append(((0.10 * (1 + math.sin(a)) * 0.9, -0.10 * math.cos(a) * 0.4, 0.10 + 0.13 * (1 + math.sin(a)) * 0.5
                     + 0.06 * k / 6), 0.008 if k < 6 else 0.0))
    it.tube(pts, "blade", n=4, scale=(1.0, 0.35))
    it.sockets["socket.tip"] = pts[-1][0]
    return it


def hammer():
    """A carpenter's hammer: a wooden handle, an iron head across it, the face forward (-Y)."""
    it = held.Item("hammer")
    it.keys = {"handle": "wood", "head": "metal"}
    it.tube([((0, 0, -0.04), 0.013), ((0, 0, 0.30), 0.012)], "handle", n=6)
    it.box((0, -0.01, 0.30), (0.03, 0.13, 0.03), "head", taper=(0.8, 0.8))
    it.sockets["socket.tip"] = (0.0, -0.075, 0.30)
    return it


def sack():
    """A tied grain sack lying across the shoulder (long axis +Z, about 60 cm)."""
    it = held.Item("sack")
    it.keys = {"sack": "sack", "tie": "cord"}
    it.tube([((0, 0, -0.30), 0.0), ((0, 0, -0.27), 0.11), ((0, 0, -0.05), 0.16), ((0, 0, 0.18), 0.15),
             ((0, 0, 0.26), 0.07), ((0, 0, 0.30), 0.03), ((0, 0, 0.36), 0.0)], "sack", n=8, scale=(1.0, 0.75))
    it.tube([((0, 0, 0.29), 0.035), ((0, 0, 0.31), 0.035)], "tie", n=6)
    return it


def bundle(logs=5, length=0.85):
    """A bundle of split firewood tied with two cords (long axis +Z)."""
    it = held.Item("bundle")
    it.keys = {"logs": "wood", "tie": "cord"}
    for k in range(logs):
        a = 2 * math.pi * k / logs
        x, y = 0.06 * math.cos(a), 0.05 * math.sin(a)
        dz = 0.04 * ((k * 7) % 3 - 1)
        it.tube([((x, y, -length / 2 + dz), 0.035), ((x, y, length / 2 + dz), 0.033)], "logs", n=5)
    for z in (-0.22, 0.22):
        it.tube([((0, 0, z - 0.01), 0.105), ((0, 0, z + 0.01), 0.105)], "tie", n=8, caps=False)
    return it


def book():
    """The priest's breviary: red leather boards, cream edges (held flat, spine to the hand)."""
    it = held.Item("book")
    it.keys = {"cover": "book", "pages": "vellum"}
    it.box((0.0, -0.02, 0.07), (0.035, 0.11, 0.16), "cover")
    it.box((0.0, -0.025, 0.07), (0.028, 0.105, 0.15), "pages")
    return it


def cross():
    """A wooden pectoral cross on a cord (hung rigid on the chest)."""
    it = held.Item("cross")
    it.keys = {"cross": "wood"}
    it.box((0, 0, 0.0), (0.016, 0.008, 0.10), "cross")
    it.box((0, 0, 0.02), (0.065, 0.008, 0.016), "cross")
    return it


ITEMS = {"axe": axe, "sickle": sickle, "hammer": hammer, "sack": sack, "bundle": bundle, "book": book, "cross": cross}
BUDGET = {"axe": 120, "sickle": 80, "hammer": 60, "sack": 140, "bundle": 160, "book": 24, "cross": 24}
TIP = {"axe": AXE_TIP, "sickle": SICKLE_TIP, "hammer": HAMMER_TIP}

# Socket bones of the tools: (parent, offset of the head from the parent socket's head or
# the bone point). Hand tools share `socket.hand_r`'s frame, so the clips' "world" turns them.
HAND_TOOLS = ("axe", "sickle", "hammer")
SHOULDER = {"sack": ("spine2", (0.15, 0.03, 1.55)), "bundle": ("spine2", (-0.15, 0.03, 1.56))}
TOOL_SOCKETS = tuple(f"socket.{t}" for t in HAND_TOOLS + tuple(SHOULDER))


def skeleton():
    """Skeleton H with the tool sockets (non-deforming)."""
    table = list(rig.skeleton_h())
    hand = next(b for b in table if b.name == "socket.hand_r")
    for t in HAND_TOOLS:
        table.append(rig.Bone(f"socket.{t}", "socket.hand_r", hand.head, hand.tail, False))
    for t, (parent, p) in SHOULDER.items():
        table.append(rig.Bone(f"socket.{t}", parent, p, (p[0], p[1], p[2] + 0.1), False))
    return table


# -- poses ---------------------------------------------------------------------------------

_with = poses._with
_r, _l = cl._r, cl._l

# EngKri's #STAND (fitted to the sprite with tools/silhouette.py, head nod capped).
STAND = {
    "upperarm_l": (20, 22, 0), "forearm_l": (-2, 0, 0),
    "upperarm_r": (20, -21, 0), "forearm_r": (11, 0, 0),
    "thigh_l": (0, -4, 0), "thigh_r": (0, 1, 0), "spine": (-12, 0, 0), "head": (20, 0, 0),
}
REST_POSE = _with(STAND, head=(14, 0, 8), spine=(2, 0, 4), upperarm_l=(-2, 30, 0), upperarm_r=(-2, -30, 0))
# A bladed stance before the tree, feet apart, knees soft.
CHOP_STANCE = {"thigh_l": (-12, -8, 0), "shin_l": (10, 0, 0), "thigh_r": (6, 8, 0), "shin_r": (8, 0, 0),
               "root_loc": (0, 0, -0.03)}
# Stooped over the stalks, the left hand gathering them.
REAP_STANCE = {"root_loc": (0, 0.06, -0.20), "thigh_l": (-40, -6, 0), "shin_l": (52, 0, 0), "foot_l": (-12, 0, 0),
               "thigh_r": (-20, 8, 0), "shin_r": (40, 0, 0), "foot_r": (-16, 0, 0),
               "spine": (-42, 0, 0), "spine1": (-14, 0, 0), "spine2": (-6, 0, 0), "head": (-16, 0, 0)}
# On the right knee, the left foot forward, bent over the beam.
KNEEL = {"root_loc": (0, 0.10, -0.43), "thigh_l": (-82, -6, 0), "shin_l": (84, 0, 0), "foot_l": (-4, 0, 0),
         "thigh_r": (-4, 6, 0), "shin_r": (96, 0, 0), "foot_r": (34, 0, 0),
         "spine": (-22, 0, 0), "spine1": (-8, 0, 0), "head": (-8, 0, 0)}

# The sprite frames the blows land on (`SOUND <frame> #<clip>` in ENGKRI.MD; the field has
# no sound: three strokes spread over the 57 frames as the sprite's).
IMPACT = {"ATTACK1": (15,), "WORK": (5,), "ATTACK3": (14, 33, 52)}

# Tree (the bark the axe bites, ahead and a little left) and the beam the hammer drives a
# peg into, in armature space.
TREE = (0.18, -0.80, 1.00)
PEG = (-0.06, -0.45, 0.38)  # the top of a beam before the knee
STALKS = (0.05, -0.50, 0.40)


def _tool(base, r_point, rot, l_point=None, pole=(-0.30, 0.10, -0.30), lpole=(0.30, 0.10, -0.30)):
    p = _with(base)
    p["world"] = dict(p.get("world", {}), **{"socket.hand_r": rot})
    p["reach"] = {"r": _r(r_point, pole)}
    if l_point is not None:
        p["reach"]["l"] = _l(l_point, lpole)
    return p


def _two(base, r_point, rot, below=0.09):
    """Both hands on the haft: the right at the grip, the left `below` it towards the end."""
    return _tool(base, r_point, rot, cl.add(r_point, cl.axis(rot), -below))


def chop_keys():
    """#ATTACK1 (27 frames): the axe from over the right shoulder in a flat diagonal cut into
    the trunk (`TREE`) on frame 15, the follow-through, back to the ready."""
    base = _with(STAND, **CHOP_STANCE)
    ready_r = (60, -40, 30)
    ready = _two(_with(base, spine=(-8, 0, 0)), (-0.10, -0.30, 1.02), ready_r)
    up_r = (-40, 0, 60)  # head over the right shoulder, behind
    up = _two(_with(base, spine=(-2, 0, 20), spine1=(0, 0, 10), head=(10, 0, -12)), (-0.26, -0.02, 1.48), up_r)
    hit_r = (90, -90, 30)  # haft forward-left, the edge leading into the bark
    hit = _two(_with(base, spine=(-14, 0, -14), spine1=(-4, 0, -8), head=(16, 0, 10)),
               cl.grip_for(TREE, hit_r, AXE_TIP), hit_r)
    mid_r = (40, -60, 50)
    mid = _two(_with(base, spine=(-8, 0, 6)), (-0.24, -0.20, 1.30), mid_r)
    after = _two(_with(base, spine=(-16, 0, -18)), cl.add(cl.grip_for(TREE, hit_r, AXE_TIP), (0.05, 0.03, -0.02)), hit_r)
    return [(0, ready), (8, up), (12, mid), (15, hit), (18, after), (22, ready), (26, ready)]


def reap_keys(n=57, impacts=IMPACT["ATTACK3"]):
    """#ATTACK3 (57 frames): stooped in the field, the left hand gathering the stalks, the
    sickle drawn through them right to left, three strokes."""
    base = _with(STAND, **REAP_STANCE)
    gather = cl.add(STALKS, (0.06, 0.0, 0.0))
    back_r = (70, -60, 70)
    back = _tool(base, (-0.28, -0.38, 0.52), back_r, gather)
    cut_r = (80, -80, 20)
    cut = _tool(_with(base, spine=(-44, 0, -10)), cl.add(STALKS, (-0.06, 0.04, 0.0)), cut_r, gather)
    keys = [(0, back)]
    for f in impacts:
        keys += [(f - 5, back), (f, cut), (f + 3, _tool(base, cl.add(STALKS, (0.08, 0.02, 0.02)), cut_r,
                                                          cl.add(gather, (0.04, 0.05, 0.08))))]
    keys.append((n - 1, back))
    return keys


def hammer_keys(n=21, impact=IMPACT["WORK"][0]):
    """#WORK (21 frames): kneeling at the beam, the left hand holding a peg, one hammer blow
    landing on `PEG` on frame 5, the hammer lifted again."""
    hold_l = cl.add(PEG, (0.08, 0.02, 0.10))
    raised_r = (-50, 0, 10)
    raised = _tool(KNEEL_STAND, (-0.18, -0.30, 0.82), raised_r, hold_l)
    hit_r = (100, 0, 4)
    hit = _tool(_with(KNEEL_STAND, spine=(-28, 0, 0)), cl.grip_for(PEG, hit_r, HAMMER_TIP), hit_r, hold_l)
    lift = _tool(KNEEL_STAND, (-0.16, -0.40, 0.55), (40, 0, 6), hold_l)
    return [(0, raised), (impact, hit), (impact + 3, lift), (n - 6, raised), (n - 1, raised)]


KNEEL_STAND = _with(STAND, **KNEEL)

# Carrying: the sack on the left shoulder steadied with both hands, the bundle on the right.
CARRY_SACK = {"reach": {"l": _l((0.20, -0.10, 1.62), (0.35, 0.0, -0.30)), "r": _r((0.06, -0.20, 1.52))}}
CARRY_BUNDLE = {"reach": {"r": _r((-0.20, -0.10, 1.62), (-0.35, 0.0, -0.30)), "l": _l((-0.06, -0.20, 1.50))}}
STOOP_LIFT = _with(STAND, root_loc=(0, 0.04, -0.30), thigh_l=(-60, -6, 0), shin_l=(78, 0, 0), foot_l=(-18, 0, 0),
                   thigh_r=(-56, 6, 0), shin_r=(74, 0, 0), foot_r=(-18, 0, 0), spine=(-38, 0, 0),
                   upperarm_l=(-50, 20, 0), upperarm_r=(-50, -20, 0))


def _carry(pose, load):
    p = _with(pose)
    p["reach"] = dict((CARRY_SACK if load == "sack" else CARRY_BUNDLE)["reach"])
    return p


# -- clips ------------------------------------------------------------------------------------


def _show(pose, tools):
    """`pose` (a copy) with every tool socket's scale: 1 for `tools`, else 0."""
    p = dict(pose)
    p["show"] = {s: (1 if s.split(".", 1)[1] in tools else 0) for s in TOOL_SOCKETS}
    return p


def _clip(n, keys, tools=(), event=None):
    """A `clips.Clip` of `n` frames with the tools shown from its first key on."""
    c = cl.frames(n, keys, event)
    if c:
        c[0] = (c[0][0], _show(c[0][1], tools))
    return c


def _norm(n, keys):
    """[(fraction, pose)] keys on `n` frames (`rig.clip_keys`)."""
    return rig.clip_keys(n, keys)


def peasant_md(rest, death, motion1_gp=0):
    """The peasants' `.MD` lines (EngKri, FrnKri and SpnKri share the layout; REST and DEATH
    lengths differ, SpnKri walks stance 1 on its own sprites)."""
    md = {
        "STAND": "#STAND 9 1 12 0", "REST": f"@REST 9 12 0 {rest}",
        "MOTION_L": "@MOTION_L 9 0 0 9", "MOTION_R": "@MOTION_R 9 0 10 19",
        "MOTION_LB": "@MOTION_LB 9 0 19 10", "MOTION_RB": "@MOTION_RB 9 0 9 0",
        "WORK": "@WORK 9 9 0 21", "ATTACK": "@ATTACK 9 7 0 53",
        "ATTACK1": "@ATTACK1 9 8 0 27", "PATTACK1": "#PATTACK1 9 1 4 85", "UATTACK1": "#UATTACK1 9 1 4 85",
        "ATTACK2": "#ATTACK2 9 1 6 0", "PATTACK2": "@PATTACK2 9 6 0 85", "UATTACK2": "@UATTACK2 9 6 85 0",
        "ATTACK3": "@ATTACK3 9 10 0 57", "PATTACK3": "#PATTACK3 9 1 6 85", "UATTACK3": "#UATTACK3 9 1 6 85",
        "ATTACK4": "#ATTACK4 9 1 8 0", "PATTACK4": "@PATTACK4 9 4 0 85", "UATTACK4": "@UATTACK4 9 4 85 0",
        "ATTACK5": "@ATTACK5 9 7 0 53", "PATTACK5": "#PATTACK5 9 1 7 0", "UATTACK5": "#UATTACK5 9 1 7 0",
        "PSTAND1": "#PSTAND1 9 1 8 0", "PSTAND2": "#PSTAND2 9 1 6 85", "PSTAND3": "#PSTAND3 9 1 10 0",
        "PSTAND4": "#PSTAND4 9 1 4 85", "PSTAND5": "#PSTAND5 9 1 7 0",
        "DEATH": f"@DEATH 9 11 0 {death}", "DEATHLIE1": f"#DEATHLIE1 9 1 11 {death}",
    }
    gps = {"1": motion1_gp, "2": 3, "3": 0, "4": 1, "5": 0}
    for s, gp in gps.items():
        md.update({f"MOTION_L{s}": f"@MOTION_L{s} 9 {gp} 0 9", f"MOTION_R{s}": f"@MOTION_R{s} 9 {gp} 10 19",
                   f"MOTION_LB{s}": f"@MOTION_LB{s} 9 {gp} 19 10", f"MOTION_RB{s}": f"@MOTION_RB{s} 9 {gp} 9 0"})
    return md


def _walk(md, stand, sfx="", load=None):
    c_l, p_l, c_r, p_r = poses.walk_cycle(stand)
    if load:
        c_l, p_l, c_r, p_r = (_carry(p, load) for p in (c_l, p_l, c_r, p_r))
    tools = (load,) if load else ()
    out = {}
    for name, keys in ((f"MOTION_L{sfx}", [(0.0, c_l), (0.5, p_l), (1.0, c_r)]),
                       (f"MOTION_R{sfx}", [(0.0, c_r), (0.5, p_r), (1.0, c_l)]),
                       (f"MOTION_LB{sfx}", [(0.0, c_l), (0.5, p_r), (1.0, c_r)]),
                       (f"MOTION_RB{sfx}", [(0.0, c_r), (0.5, p_l), (1.0, c_l)])):
        if name in md:
            n = cl.md_frames(md[name])
            out[name] = _clip(n, _norm(n, keys), tools)
    return out


def peasant_clips(md):
    """{clip: clips.Clip} of a peasant: stand, rest, walks (stance 2 with the sack, 4 with the
    firewood), chop, reap, hammer, the carry transitions, the club blows with the axe
    (`ATTACK`, `ATTACK5`, `ATTACK_CUT_*`), death."""
    n = {k: cl.md_frames(v) for k, v in md.items()}
    out = {}
    out["STAND"] = _clip(1, [(0, STAND)])
    out["REST"] = _clip(n["REST"], _norm(n["REST"], [(0.0, STAND), (0.4, REST_POSE), (0.7, REST_POSE), (1.0, STAND)]))
    for sfx in ("", "1", "3", "5"):
        out.update(_walk(md, STAND, sfx))
    out.update(_walk(md, STAND, "2", "sack"))
    out.update(_walk(md, STAND, "4", "bundle"))
    chop = chop_keys()
    out["ATTACK1"] = _clip(n["ATTACK1"], chop, ("axe",), event=IMPACT["ATTACK1"][0])
    for k in ("PSTAND1", "PATTACK1", "UATTACK1"):
        out[k] = _clip(1, [(0, chop[0][1])], ("axe",))
    reap = reap_keys(n["ATTACK3"])
    out["ATTACK3"] = _clip(n["ATTACK3"], reap, ("sickle",), event=IMPACT["ATTACK3"][0])
    for k in ("PSTAND3", "PATTACK3", "UATTACK3"):
        out[k] = _clip(1, [(0, reap[0][1])], ("sickle",))
    out["WORK"] = _clip(n["WORK"], hammer_keys(n["WORK"]), ("hammer",), event=IMPACT["WORK"][0])
    for s, load in (("2", "sack"), ("4", "bundle")):
        carry = _carry(STAND, load)
        m = n[f"PATTACK{s}"]
        lift = _with(STOOP_LIFT)
        out[f"PATTACK{s}"] = _clip(m, [(0, lift), (int(m * 0.45), _carry(STOOP_LIFT, load)), (m - 1, carry)], (load,))
        out[f"UATTACK{s}"] = _clip(m, [(0, carry), (int(m * 0.55), _carry(STOOP_LIFT, load)), (m - 1, lift)], (load,))
        out[f"PSTAND{s}"] = _clip(1, [(0, carry)], (load,))
        out[f"ATTACK{s}"] = _clip(1, [(0, carry)], (load,))
    # Club blows with the axe (MECH): the shared hack, the axe's edge on the target.
    strikes = cl.club_strikes(n["ATTACK"], tip=AXE_TIP)
    for k, c in strikes.items():
        out[k] = _clip(n["ATTACK"], list(c), ("axe",), event=c.event)
    out["ATTACK"] = out["ATTACK_CUT_MID"]
    out["ATTACK5"] = out["ATTACK_CUT_MID"]
    out["PSTAND5"] = out["PATTACK5"] = out["UATTACK5"] = _clip(1, [(0, strikes["ATTACK_CUT_MID"][-1][1])], ("axe",))
    lie = poses.lie_prone()
    out["DEATH"] = _clip(n["DEATH"], _norm(n["DEATH"], poses.death_keys(STAND, lie)))
    out["DEATHLIE1"] = _clip(1, [(0, lie)])
    return {k: v for k, v in out.items() if k in md or k.startswith("ATTACK_")}


# The priest: hands before the chest, the book in the left.
PRIEST_STAND = _with(cl.STAND_FREE, spine=(-4, 0, 0), head=(8, 0, 0))
PRIEST_STAND["reach"] = {"l": _l((0.07, -0.24, 1.12), (0.30, 0.10, -0.30)), "r": _r((-0.04, -0.23, 1.10))}
PRIEST_REST = _with(PRIEST_STAND, head=(26, 0, 4), spine=(-8, 0, 0))
PRIEST_REST["reach"] = {"l": _l((0.05, -0.28, 1.18)), "r": _r((-0.02, -0.27, 1.16))}


def priest_clips(md, active=1):
    """{clip: clips.Clip} of the priest: stand, rest (reading his book), walk, the heal
    (`clips.heal`, landing on `release_frame(active)`), death."""
    n = {k: cl.md_frames(v) for k, v in md.items()}
    out = {"STAND": _clip(1, [(0, PRIEST_STAND)])}
    if "REST" in md:
        out["REST"] = _clip(n["REST"], _norm(n["REST"], [(0.0, PRIEST_STAND), (0.25, PRIEST_REST), (0.8, PRIEST_REST),
                                                         (1.0, PRIEST_STAND)]))
    out.update(_walk(md, PRIEST_STAND))
    heal = cl.unit_clips(md, {0: "MEDIK"}, active={"ATTACK": active})["ATTACK"]
    out["ATTACK"] = _clip(n["ATTACK"], list(heal), event=heal.event)
    lie = poses.lie_prone()
    out["DEATH"] = _clip(n["DEATH"], _norm(n["DEATH"], poses.death_keys(cl.bare(PRIEST_STAND), lie)))
    out["DEATHLIE1"] = _clip(1, [(0, lie)])
    return out


# -- garments ---------------------------------------------------------------------------------

DROP_ALWAYS = ("lapel", "turnback", "cuff", "collar")


def peasant_coat(body_key, sleeve_key, skirts):
    """The coat draft cut down to a work garment: no lapels, collar, cuffs or turnbacks, the
    front sewn shut from the neck to the hem; with `skirts` a smock-frock to mid-thigh,
    without them a shirt or jacket to the waist. Body panels in `body_key`, sleeves in
    `sleeve_key`."""
    from . import garments as gm

    def draft(m):
        panels, seams = gm.coat(m)
        drop = set(DROP_ALWAYS) | (set() if skirts else {"skirt_front", "skirt_back"})
        panels = {k: p for k, p in panels.items() if k not in drop}
        for k, p in panels.items():
            p.key = sleeve_key if k.startswith("sleeve") else body_key
        keep = [s for s in seams if not any(x[0] in drop for x in s.a + s.b)
                and not (s.a[0][0] == "coat_front" and s.a[0][1] == "hook")]
        keep.append(gm.Seam([("coat_front", "cf")], [("coat_front", "cf", False, "R")], "coat"))
        return panels, keep

    return draft


def stockings(m):
    """Knitted stockings: the gaiter pattern without its garter, in `hose`."""
    from . import garments as gm

    panels, seams = gm.gaiters(m)
    panels = {k: p for k, p in panels.items() if k != "garter"}
    panels["gaiter"].key = "hose"
    return panels, seams


# -- Blender side ---------------------------------------------------------------------------------

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}

PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69), "hair": (0.45, 0.33, 0.20),
    "smock": (0.36, 0.11, 0.09), "band": (0.62, 0.07, 0.06), "breeches": (0.78, 0.74, 0.60),
    "shirt": (0.86, 0.84, 0.78), "vest": (0.22, 0.26, 0.32), "hose": (0.78, 0.76, 0.70),
    "shoes": (0.22, 0.14, 0.08), "hat": (0.47, 0.36, 0.22), "wood": (0.50, 0.34, 0.18), "metal": (0.50, 0.50, 0.52),
    "sack": (0.72, 0.64, 0.48), "cord": (0.60, 0.52, 0.38),
    "habit": (0.86, 0.84, 0.78), "cappa": (0.10, 0.10, 0.11), "book": (0.55, 0.08, 0.07), "vellum": (0.86, 0.80, 0.66),
}
BUILD = {"width": 0.97, "belly": 1.05, "limbs": 1.1}


def spec(md, kind="peasant", palette=None, body="shirt", sleeves="shirt", skirts=False, sash=True, mix=0.5,
         fixed=(), build=None):
    """A unit's SPEC: `kind` peasant or priest; for peasants the draped garment's `body` and
    `sleeves` keys and `skirts`, a `sash`; colours over `PALETTE` (sprite mix `mix`, `fixed`
    keys unmixed)."""
    return {"md": md, "kind": kind, "palette": dict(PALETTE, **(palette or {})), "body": body, "sleeves": sleeves,
            "skirts": skirts, "sash": sash, "mix": mix, "fixed": tuple(fixed), "build": dict(BUILD, **(build or {}))}


def _hold_tools(ch, name, items):
    from mathutils import Matrix

    from . import armature as am
    from . import gear

    for t in items:
        sock = f"socket.{t}"
        w = am.world(ch.arm, sock)
        if t in HAND_TOOLS:
            rest_up = w.to_3x3() @ ch.arm.pose.bones[sock].bone.matrix_local.to_3x3().inverted()
            m = Matrix.Translation(w.translation) @ rest_up.to_4x4()
        else:
            # Across the shoulder: the long axis left-right, a little forward-down.
            m = Matrix.Translation(w.translation) @ Matrix.Rotation(math.radians(90), 4, "Y") \
                @ Matrix.Rotation(math.radians(8), 4, "X")
        gear.hold(ch, ITEMS[t](), f"{name}.{t}", socket=sock, matrix=m, slot=t)


def make(root, name, s):
    """The civilian of SPEC `s` under `root`: body, clothes, hat or hood, tools."""
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import body as bd
    from . import drape, gear, mbody
    from .humans import Character

    ch = Character(root, name, s["build"], table=skeleton())
    priest = s["kind"] == "priest"
    ch.body(shoulders=priest)  # the others wear a drafted coat
    ch.shoes = ch.add("clothes", mbody.shoes(ch.arm, ch.flesh, f"{name}.footwear", ch.prefix, key="shoes"),
                      "shoes", symmetric=True)
    if priest:
        # Habit (white wool) to the ankle, the black cappa over it open at the front, a hood
        # collar round the shoulders.
        ch.shell("habit", "habit", lambda c, n, h, p: (h in TORSO and c.z > 0.95) or h in ARMS, 0.020)
        ch.shell("habit_legs", "habit", lambda c, n, h, p: (h in {"hips"} | LEGS) or (h in SHINS and c.z > 0.14), 0.024)
        robe = bd.tube(ch.arm, f"{name}.habit_skirt", rig.SKIRT_TOP, 0.07, (0.19, 0.15), (0.27, 0.23),
                       columns=rig.SKIRT_COLUMNS)
        ch.add("clothes", robe, "habit", symmetric=True)

        def cloak(c, n, h, p):
            return (h in TORSO and c.z > 0.98 and not (c.y < -0.03 and abs(c.x) < 0.10)) \
                or (h in {"upperarm_l", "upperarm_r"} and c.z > 1.15)

        ch.shell("cappa", "cappa", cloak, 0.040)
        ch.shell("hood", "cappa", lambda c, n, h, p: (h in {"spine2", "clavicle_l", "clavicle_r", "neck"} and c.z > 1.38
                                                      and c.y > -0.06), 0.070, neck=False)
        cape = bd.tube(ch.arm, f"{name}.cappa_skirt", rig.SKIRT_TOP + 0.02, 0.16, (0.22, 0.18), (0.31, 0.27),
                       gap_deg=70, columns=rig.SKIRT_COLUMNS)
        ch.add("clothes", cape, "cappa", symmetric=True)
    else:
        ch.drape_report = {}
        made = drape.dress(ch, garments=("breeches", "gaiters", "coat"), report=ch.drape_report,
                           patterns={"coat": peasant_coat(s["body"], s["sleeves"], s["skirts"]), "gaiters": stockings})
        cloth = [o for o in ch.layers["clothes"] if o.name in made]
        drape.hide_skin(ch, cloth)
        upper = {s["body"], s["sleeves"]}
        fixed = [o for o in cloth if made[o.name] in upper | {"hose", "breeches"}]
        for o in fixed:
            if made[o.name] == "breeches":
                drape.hide_under(ch, o, [x for x in fixed if made[x.name] in upper | {"hose"}], 0.004)
        if not s["skirts"]:
            # The shirt or waistcoat bloused over the waistband (the cut-down coat ends at
            # the natural waist, above the breeches).
            ch.shell("blouse", s["body"], lambda c, n, h, p: h in TORSO and 0.90 < c.z < 1.22, 0.050)
        if s["sash"]:
            ch.shell("sash", "band", lambda c, n, h, p: h in TORSO and 0.97 < c.z < 1.07, 0.048)
    ch.bone_layer()
    ch.caps()
    am.pose(ch.arm, cl.bare(STAND))
    if not priest:
        hat = registry.build_hat("civil_slouch", f"{name}.hat")
        am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation
                                                                + Vector((0, 0.005, 0.03)))
                  @ Matrix.Rotation(math.radians(-8), 4, "X"))
        ch.held["hat"] = hat
        ch.keys[hat.name] = "hat"
        _hold_tools(ch, name, HAND_TOOLS + tuple(SHOULDER))
    else:
        gear.hold(ch, book(), f"{name}.book", socket="socket.hand_l", slot="book")
        cr = gear.held(cross(), f"{name}.cross")
        gear.rigid(cr, ch.arm, "spine2", Matrix.Translation((0.0, -0.135, 1.27)))
        ch.add("gear", cr, "wood")
    return ch


def clips_of(s):
    if s["kind"] == "priest":
        return priest_clips(s["md"])
    return peasant_clips(s["md"])


def build(ctx, s):
    """Builds, animates, paints and prepares for export the civilian of SPEC `s`."""
    from . import armature as am
    from . import crawl, faces, heads, outfit
    from .humans import LAYER_OF, prepare_export

    ch = make(ctx.root, ctx.name, s)
    heads.variants(ch)  # hair and beard variants (#283), before the morphs
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    clips = clips_of(s)
    for clip, keys in clips.items():
        cl.apply(ch.arm, clip, keys)
    # Crawl clips of the wounded (#262), the tools hidden.
    for clip, keys in crawl.clips().items():
        keys = list(keys)
        keys[0] = (keys[0][0], _show(keys[0][1], ()))
        cl.apply(ch.arm, clip, keys)
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)  # face and body morphs, `_FACE`, variants (#283)
    ch.arm.animation_data.action = None
    # The clips key the tool sockets' scale; the export pose has them all at 1 (a zero scale
    # here would be baked into the tools' own node transforms).
    for pb in ch.arm.pose.bones:
        pb.scale = (1.0, 1.0, 1.0)
    am.pose(ch.arm, cl.bare(STAND) if s["kind"] != "priest" else cl.bare(PRIEST_STAND))
    tris = ch.triangles()
    names = list(clips) + list(crawl.CLIPS)
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()), acr_clips=names,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), **faces.extras("civilian"))
    ctx.stats = {"triangles": tris, "colours": colours, "drape": getattr(ch, "drape_report", None),
                 "events": {k: c.event for k, c in clips.items() if getattr(c, "event", None) is not None}}


def pose_for(ch, s, clip):
    """Shows the first frame of `clip` (the last of DEATH) for the silhouette tool."""
    from . import armature as am

    keys = clips_of(s)[clip]
    ch._pose_spec = cl.bare((keys[-1] if clip == "DEATH" else keys[0])[1])
    am.pose(ch.arm, ch._pose_spec)
