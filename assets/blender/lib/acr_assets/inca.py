"""The Inca (docs/humans-roster.md, family 8; docs/families/inca.md): men of the Andes in
the unku (a knee-long tunic of one folded panel, sewn up the sides) on skeleton H with the
layers of #178. A unit script is data: its `.MD` clip lines, its `WEAPONKIND`s and
`SETACTIVEPOINT` frames and a `SPEC` (`spec`); `build(ctx)` calls `inca.build(ctx, SPEC)`.

Dress (each its own material key and painter: `outfit.dress` paints one colour per painter):
- the unku: a shell over the trunk and a skirt tube standing off the legs down to the
  knee; four weaves: `check` (the army's black-and-white checkerboard with a red stepped
  yoke, Guaman Poma's soldiers), `plain` (one colour, a zigzag hem), `tocapu` (a band of
  tocapu squares at the waist), `royal` (tocapu all over: lords);
- the yacolla (cloak) over the back and shoulders, standing off; quilted cotton armour;
- sandals (meso's), the llautu (a braided band wound round the head), hair cropped with a
  fringe of locks (a thin shell over the scalp and cards at its edge, not a helmet), ear
  spools for nobles, a cane helmet, feather fans of cards.
Gear: the sling (huaraca), the star-headed mace (champi), the spear, the blowpipe, bow,
arrows and quiver, and a small round shield worn on the back.

SPEC keys (`spec` fills the defaults):
- `md`, `weapons` {attack index: WEAPONKIND}, `active` {clip: SETACTIVEPOINT frame};
- `palette` colours by material key (sRGB) over `PALETTE`; `mix` sprite mix (0..1);
- `unku` "check", "plain", "tocapu" or "royal"; `quilt` True for the cotton armour,
  `yacolla` True for the cloak;
- `head` "llautu" or "helmet", `plumes` the number of feathers in the fan (0: none),
  `plume_len` their length, `spools` True for ear spools;
- `hand_r` "sling", "champi", "spear", "blowpipe" or None; `bow` True; `shield` True (on
  the back);
- `melee` the strike clips of index 0: "club" (the champi's or sling's smash, the pipe's
  swing) or "spear" (the thrust) or None.
"""

import math

from . import clips as cl
from . import meso, poses, registry, rig

TORSO = meso.TORSO
UPPER = meso.UPPER
LEGS = meso.LEGS

PALETTE = {
    "skin": (0.55, 0.36, 0.24), "bone": (0.85, 0.81, 0.69), "inca_hair": (0.04, 0.035, 0.03),
    "unku_check": (0.78, 0.10, 0.07), "unku_plain": (0.80, 0.55, 0.12), "unku_tocapu": (0.72, 0.12, 0.10),
    "unku_royal": (0.70, 0.10, 0.12), "yacolla": (0.20, 0.10, 0.08), "inca_quilt": (0.90, 0.86, 0.74),
    "llautu": (0.80, 0.12, 0.08), "inca_plumes": (0.85, 0.15, 0.08), "inca_helmet": (0.72, 0.56, 0.30),
    "inca_shield": (0.88, 0.80, 0.62), "spool": (0.85, 0.66, 0.22), "sandals": (0.50, 0.36, 0.20),
    "wood": (0.38, 0.24, 0.12), "stone": (0.30, 0.30, 0.32), "cord": (0.80, 0.70, 0.50),
    "brass": (0.72, 0.52, 0.25), "hide": (0.60, 0.46, 0.30), "feather": (0.88, 0.85, 0.78),
}
BUILD = {"width": 1.0, "shoulders": 1.04, "limbs": 1.10, "hips": 1.0}
FIXED = ("unku_check", "unku_plain", "unku_tocapu", "unku_royal", "inca_plumes", "llautu", "inca_shield", "spool",
         "inca_hair", "yacolla", "inca_quilt", "inca_helmet")
DEFAULTS = {"weapons": {}, "active": {}, "build": BUILD, "mix": 0.25, "fixed": FIXED, "unku": "check",
            "quilt": False, "yacolla": False, "head": "llautu", "plumes": 0, "plume_len": 0.22, "spools": False,
            "hand_r": "champi", "bow": False, "shield": False, "melee": "club", "shield_design": (0.70, 0.12, 0.08)}


def spec(md, **kw):
    """A full SPEC: the defaults with `palette` and `build` merged key by key."""
    unknown = set(kw) - set(DEFAULTS) - {"palette"}
    if unknown:
        raise KeyError(f"unknown SPEC keys {sorted(unknown)}")
    pal = dict(PALETTE, **kw.pop("palette", {}))
    build = dict(BUILD, **kw.pop("build", {}))
    return dict(DEFAULTS, **kw, md=md, palette=pal, build=build)


# -- painters (numpy; registered at import) ------------------------------------------------

WHITE = (0.93, 0.91, 0.85)
BLACK = (0.05, 0.045, 0.04)
TOCAPU_COLOURS = ((0.80, 0.10, 0.07), (0.95, 0.72, 0.10), BLACK, WHITE, (0.10, 0.45, 0.25), (0.12, 0.22, 0.55))


def _np():
    return meso._np()


def _weave(np, coat, col, p, seed):
    """Cloth: a fine warp-faced rib and a little unevenness."""
    rib = 0.94 + 0.06 * np.abs(np.sin(p[:, 2] * math.pi / 0.004))
    return col * (rib * (0.92 + 0.08 * coat.fbm(p, 25.0, seed)))[:, None]


def _around(np, p):
    """Arc length round the body (m, 0 at the front) and height."""
    return np.arctan2(p[:, 0], -p[:, 1]) * 0.17, p[:, 2]


def _yoke(np, p, top):
    """The stepped V yoke at the neck (front and back): 1 where it is."""
    x = np.abs(p[:, 0])
    z0 = top - 0.50 + 1.4 * np.floor(x / 0.035) * 0.035
    return (p[:, 2] > z0).astype(np.float32)


def _tocapu(np, u, v, k):
    """Tocapu motif `k` (0..4) on the square's local (u, v) in 0..1."""
    out = np.zeros_like(u)
    m = k == 0
    out[m] = (u[m] > v[m])
    m = k == 1
    out[m] = (np.abs(u[m] - 0.5) < 0.16) | (np.abs(v[m] - 0.5) < 0.16)
    m = k == 2
    out[m] = (np.floor(u[m] * 3) + np.floor(v[m] * 3)) % 2
    m = k == 3
    d = np.maximum(np.abs(u[m] - 0.5), np.abs(v[m] - 0.5))
    out[m] = (d > 0.14) & (d < 0.30)
    m = k == 4
    out[m] = v[m] > np.floor(u[m] * 4) / 4
    return out


def _tocapu_grid(np, coat, s, z, size, seed):
    """Colours (linear) and border mask of a grid of tocapu squares of `size` m."""
    a, b = s / size, z / size
    ia, ib = np.floor(a).astype(np.int64), np.floor(b).astype(np.int64)
    u, v = a - ia, b - ib
    h = (ia * 73856093) ^ (ib * 19349663) ^ seed
    k = (h >> 3) % 5
    c1 = (h >> 7) % len(TOCAPU_COLOURS)
    c2 = (c1 + 1 + (h >> 11) % (len(TOCAPU_COLOURS) - 1)) % len(TOCAPU_COLOURS)
    pal = np.array([coat.lin(c) for c in TOCAPU_COLOURS], np.float32)
    m = _tocapu(np, u, v, k)
    col = pal[c1] * (1 - m)[:, None] + pal[c2] * m[:, None]
    border = (np.minimum(np.minimum(u, 1 - u), np.minimum(v, 1 - v)) < 0.08).astype(np.float32)
    return col, border


def _zigzag(np, s, z, z0, height):
    """1 inside a band of triangles standing on z0 (`height` high)."""
    f = s / height
    tri = np.abs(f - np.floor(f) - 0.5) * 2  # 0 at the middle of a tooth, 1 at its edges
    return ((z > z0) & (z < z0 + height * (1 - tri))).astype(np.float32)


def paint_unku_check(p, n, rgb, L, seed):
    """The army's unku (Guaman Poma): black-and-white checkerboard of 7 cm squares, a red
    stepped yoke at the neck (front and back)."""
    np, coat = _np()
    s, z = _around(np, p)
    chk = (np.floor(s / 0.07) + np.floor(z / 0.07)) % 2
    col = coat.mix(coat.lin(WHITE), coat.lin(BLACK), chk)
    col = coat.mix(col, coat.lin(rgb), _yoke(np, p, L["head_top"]))
    return _weave(np, coat, col, p, seed)


def paint_unku_plain(p, n, rgb, L, seed):
    """A plain unku: one colour, a zigzag hem in black and white."""
    np, coat = _np()
    s, z = _around(np, p)
    col = coat.lin(rgb) * np.ones((len(p), 1), np.float32)
    hem = L["head_top"] - 1.20
    col = coat.mix(col, coat.lin(BLACK), ((z > hem) & (z < hem + 0.06)).astype(np.float32))
    col = coat.mix(col, coat.lin(WHITE), _zigzag(np, s, z, hem, 0.05))
    return _weave(np, coat, col, p, seed)


def paint_unku_tocapu(p, n, rgb, L, seed):
    """An unku in one colour with a band of tocapu squares round the waist (two rows of
    5.5 cm) and the zigzag hem."""
    np, coat = _np()
    s, z = _around(np, p)
    col = paint_unku_plain(p, n, rgb, L, seed)
    z0 = L["head_top"] - 0.80
    band = ((z > z0) & (z < z0 + 0.11)).astype(np.float32)
    tc, border = _tocapu_grid(np, coat, s, z - z0, 0.055, seed)
    tc = coat.mix(tc, coat.lin(BLACK), border)
    return coat.mix(col, _weave(np, coat, tc, p, seed), band)


def paint_unku_royal(p, n, rgb, L, seed):
    """A lord's unku: tocapu squares all over (5 cm), a yoke in the ground colour."""
    np, coat = _np()
    s, z = _around(np, p)
    tc, border = _tocapu_grid(np, coat, s, z, 0.05, seed)
    col = coat.mix(tc, coat.lin(rgb), border)
    col = coat.mix(col, coat.lin(rgb), _yoke(np, p, L["head_top"]))
    return _weave(np, coat, col, p, seed)


def paint_llautu(p, n, rgb, L, seed):
    """The llautu: a braid of coloured wool, slanting stripes."""
    np, coat = _np()
    ang = np.arctan2(p[:, 0], -p[:, 1]) * 0.10
    i = np.floor((ang + p[:, 2]) / 0.010).astype(np.int64) % 3
    pal = np.array([coat.lin(rgb), coat.lin((0.95, 0.72, 0.10)), coat.lin(BLACK)], np.float32)
    return pal[i] * (0.85 + 0.15 * coat.fbm(p, 200.0, seed, octaves=1))[:, None]


def paint_plumes(p, n, rgb, L, seed):
    """Feathers in bands of colour across the fan (the unit's colour, gold, green), each
    streaked along its length and darker at the tip."""
    np, coat = _np()
    i = np.floor((p[:, 0] + 1.0) / 0.022).astype(np.int64) % 4
    pal = np.array([coat.lin(rgb), coat.lin((0.98, 0.78, 0.12)), coat.lin(rgb), coat.lin((0.10, 0.50, 0.30))], np.float32)
    streak = coat.fbm(p, (160.0, 160.0, 18.0), seed)
    return pal[i] * (0.75 + 0.40 * streak)[:, None]


def paint_helmet(p, n, rgb, L, seed):
    """A helmet of plaited cane: rows of stems, a darker gap between the rows."""
    np, coat = _np()
    row = np.abs(np.sin(p[:, 2] * math.pi / 0.011))
    ang = np.arctan2(p[:, 0], p[:, 1]) * 0.11
    plait = np.abs(np.sin((ang + np.floor(p[:, 2] / 0.011) * 0.006) * math.pi / 0.012))
    return coat.lin(rgb) * (0.62 + 0.30 * row + 0.12 * plait + 0.06 * coat.fbm(p, 60.0, seed))[:, None]


registry.painter("unku_check", fn=paint_unku_check, finish=(0.95, 0.15, 0.0))
registry.painter("unku_plain", fn=paint_unku_plain, finish=(0.95, 0.15, 0.0))
registry.painter("unku_tocapu", fn=paint_unku_tocapu, finish=(0.95, 0.15, 0.0))
registry.painter("unku_royal", fn=paint_unku_royal, finish=(0.92, 0.18, 0.0))
registry.painter("llautu", fn=paint_llautu, finish=(1.0, 0.15, 0.0))
registry.painter("inca_plumes", fn=paint_plumes, finish=(0.6, 0.30, 0.10))
registry.painter("inca_helmet", fn=paint_helmet, finish=(0.8, 0.0, 0.05))
registry.painter("inca_quilt", fn=meso.paint_quilted, finish=(1.0, 0.10, 0.0))
registry.painter("yacolla", like="wool", finish=(1.0, 0.25, 0.0))
registry.painter("inca_hair", like="felt", finish=(0.75, 0.2, 0.1))
registry.painter("inca_shield", like="linen", finish=(0.95, 0.12, 0.0))
registry.painter("spool", like="leather", finish=(0.4, 0.0, 0.3))
for _k in ("unku_check", "unku_plain", "unku_tocapu", "unku_royal", "inca_quilt", "inca_shield"):
    registry.material(_k, "linen", painter=_k, fallback=PALETTE[_k])
registry.material("yacolla", "wool", painter="yacolla", fallback=PALETTE["yacolla"])
registry.material("llautu", "wool", painter="llautu", fallback=PALETTE["llautu"])
registry.material("inca_plumes", "hair", painter="inca_plumes", fallback=PALETTE["inca_plumes"])
registry.material("inca_hair", "hair", painter="inca_hair", fallback=PALETTE["inca_hair"])
registry.material("inca_helmet", "wood", painter="inca_helmet", fallback=PALETTE["inca_helmet"])
registry.material("spool", "metal", painter="spool", fallback=PALETTE["spool"])
del _k


# -- items (held.Item data, pure) --------------------------------------------------------

CHAMPI = 0.62  # the champi's grip to the star


def champi(length=0.78, grip=0.16, points=6):
    """The champi: a hardwood haft and a bronze star of `points` points (a flat ring with
    pyramids round it). Origin at the grip, +Z up the haft; `socket.tip` the star."""
    from . import held

    it = held.Item("champi")
    it.keys = {"haft": "wood", "star": "brass"}
    top = length - grip
    it.tube([((0, 0, -grip), 0.016), ((0, 0, top + 0.03), 0.014)], "haft", n=6)
    it.tube([((0, 0, top - 0.03), 0.032), ((0, 0, top + 0.03), 0.032)], "star", n=8)
    for i in range(points):
        a = 2 * math.pi * i / points
        c, s = math.cos(a), math.sin(a)
        base = [it.v((0.03 * c - 0.016 * s, 0.03 * s + 0.016 * c, top + dz)) for dz in (-0.022, 0.022)]
        base += [it.v((0.03 * c + 0.016 * s, 0.03 * s - 0.016 * c, top + dz)) for dz in (0.022, -0.022)]
        tip = it.v((0.085 * c, 0.085 * s, top))
        for j in range(4):
            it.f((base[j], base[(j + 1) % 4], tip), "star")
    it.sockets["socket.tip"] = (0.0, 0.0, top)
    return it


def back_shield(radius=0.24, fringe=7):
    """The small round shield of cane and cotton, worn on the back: one key (the design a
    mask in its own frame), a fringe of feathers or cloth from the lower rim."""
    from . import held

    it = held.shield(radius)
    it.name = "inca_shield"
    it.keys = {k: "inca_shield" for k in it.keys}
    for i in range(fringe):
        a = math.radians(-150 + 120 * i / max(fringe - 1, 1))
        x, z = math.cos(a) * radius * 0.97, math.sin(a) * radius * 0.97
        pts = [(x, -0.05, z), (x * 1.03, -0.055, z - 0.07), (x * 1.05, -0.05, z - 0.14)]
        it.strip(pts, 0.03, "fringe", normal=(0, 1, 0))
        it.strip(list(reversed(pts)), 0.03, "fringe", normal=(0, 1, 0))
        it.keys["fringe"] = "inca_shield"
    return it


def band():
    """The llautu wound round the head (hat frame: origin at the crown, face -Y), lying on
    the brow over the cropped hair."""
    from . import held

    it = held.Item("llautu")
    it.keys = {"band": "llautu"}
    it.tube([((0, 0.008, -0.062), 0.104), ((0, 0.008, -0.045), 0.108), ((0, 0.008, -0.028), 0.102)], "band", n=16,
            caps=False, scale=(1.0, 1.10))
    return it


def helmet():
    """A cane helmet: a dome to the brow with a rolled rim (hat frame)."""
    from . import held

    it = held.Item("inca_helmet")
    it.keys = {"dome": "inca_helmet"}
    meso._dome(it, "dome", 0.122, -0.085, 0.045, n=14, sy=1.10)
    it.tube([((0, 0, -0.095), 0.124), ((0, 0, -0.080), 0.128), ((0, 0, -0.068), 0.124)], "dome", n=14, caps=False,
            scale=(1.0, 1.10))
    return it


def plumes(count, length, base_z=-0.06, front=True):
    """A fan of feather cards standing up from the band at the front of the head (hat
    frame), or a crest from the helmet's top (`front` False)."""
    from . import held

    it = held.Item("inca_plumes")
    it.keys = {"plumes": "inca_plumes"}
    if front:
        meso._fan(it, "plumes", (0, -0.10, base_z + 0.02), count, length, 70, 12, width=0.034, curve=0.18)
    else:
        meso._fan(it, "plumes", (0, 0.02, base_z), count, length, 90, 25, width=0.04, curve=0.30)
    return it


# -- clips (pure: data for clips.apply) ---------------------------------------------------

STAND_LONG = cl._with(cl.STAND_FREE, forearm_r=(-30, 0, 12), world={"socket.hand_r": (0, 0, 0)})

# Down strikes leaning FORWARD (positive spine X bends forward on skeleton H; the shared
# STOOP leans back): a long lunge, the trunk over the front knee, the striking point on a
# man lying DOWN_AT ahead (the sim fences a man on the ground at about 2.0 m).
DOWN_AT = 1.8
LUNGE_DOWN = {"root_loc": (0, -0.70, -0.30), "thigh_l": (-64, -3, 0), "shin_l": (80, 0, 0), "foot_l": (-14, 0, 0),
              "thigh_r": (24, 3, 0), "shin_r": (48, 0, 0), "foot_r": (-12, 0, 0), "spine": (32, 0, 6),
              "spine1": (12, 0, 0), "spine2": (6, 0, 0), "head": (-14, 0, 0)}
LEAN_DOWN = {"root_loc": (0, -0.30, -0.10), "thigh_l": (-34, -3, 0), "shin_l": (30, 0, 0), "thigh_r": (14, 3, 0),
             "shin_r": (12, 0, 0), "spine": (22, 0, -6), "spine1": (6, 0, 0)}


def club_down(n, tip, at=DOWN_AT):
    """`ATTACK_CUT_DOWN` (= `ATTACK_DOWN`): the smash down over the lunge."""
    down_r = (150, 0, 0)
    target = (0.0, -at, 0.20)
    hit = cl._blow(dict(cl.STAND_FREE, **LUNGE_DOWN), cl.grip_for(target, down_r, tip), down_r)
    over = cl._blow(dict(cl.STAND_FREE, **dict(LUNGE_DOWN, root_loc=(0, -0.50, -0.18), spine=(12, 0, 0))),
                    (-0.14, -0.62, 1.55), (-40, 0, 0))
    return cl._strike(n, over, hit, over)


def spear_down(n, tip, at=DOWN_AT, left_along=0.45):
    """`ATTACK_THRUST_DOWN` (= `ATTACK_DOWN`): both hands drive the point down, leaning in."""
    down_r = (122, 0, 0)

    def two(base, r_point, r):
        return cl._blow(base, r_point, r, l_point=cl.add(r_point, cl.axis(r), left_along))

    target = (0.0, -at, 0.20)
    hit = two(dict(cl.STAND_FREE, **LEAN_DOWN), cl.grip_for(target, down_r, tip), down_r)
    up = two(dict(cl.STAND_FREE, spine=(8, 0, 0)), cl.add(hit["reach"]["r"][0], (0, 0.30, 0.22)), (112, 0, 0))
    return cl._strike(n, up, hit, up)


def stance(s):
    if s["bow"]:
        return cl.STAND_FREE
    if s["hand_r"] in ("spear", "blowpipe"):
        return STAND_LONG
    if s["hand_r"] == "champi":
        return cl.STAND_ARMED
    return cl.STAND_FREE


def tips(s):
    """Grip-to-striking-point length of the right-hand weapon (from the item data)."""
    from . import held

    item = s["hand_r"]
    if item == "spear":
        return spear_item().sockets["socket.tip"][2]
    if item == "champi":
        return champi().sockets["socket.tip"][2]
    if item == "blowpipe":
        return held.blowpipe().sockets["socket.muzzle"][2]
    if item == "sling":
        return 0.10  # the fist round the loaded pouch: the sling hangs and is no lever
    return cl.CLUB_TIP


def spear_item():
    from . import held

    return held.spear(length=2.2, grip=0.85, head=0.24)


def unit_clips(s):
    """{clip: [(frame, pose)]} for SPEC `s` and the extra strike clip names (as meso's)."""
    md = s["md"]
    stand = stance(s)
    still = cl.bare(stand)
    lie = poses.lie_prone()
    c_l, p_l, c_r, p_r = poses.walk_cycle(stand)
    keys = {
        "STAND": [(0.0, stand)],
        "MOTION_L": [(0.0, c_l), (0.5, p_l), (1.0, c_r)],
        "MOTION_R": [(0.0, c_r), (0.5, p_r), (1.0, c_l)],
        "MOTION_LB": [(0.0, c_l), (0.5, p_r), (1.0, c_r)],
        "MOTION_RB": [(0.0, c_r), (0.5, p_l), (1.0, c_l)],
        "DEATH": poses.death_keys(dict(still), lie),
    }
    for k in ("L", "R", "LB", "RB"):
        keys[f"MOTION_{k}0"] = keys[f"MOTION_{k}"]
    for k in ("DEATHLIE1", "DEATHLIE2", "DEATHLIE3"):
        keys[k] = [(0.0, lie)]
    for k in ("PSTAND", "PATTACK", "UATTACK", "PSTAND1", "PATTACK1", "UATTACK1", "PATTACK0", "UATTACK0", "PSTAND2"):
        keys[k] = [(0.0, stand)]
    out = {}
    for k, ks in keys.items():
        if k in md:
            out[k] = rig.clip_keys(rig.md_clip_frames(md[k])[0], ks)
    weapons = s["weapons"]
    out.update(cl.unit_clips(md, {i: w for i, w in weapons.items() if i > 0}, active=s["active"]))
    extra = []
    if 0 in weapons and "ATTACK" in md and s["melee"]:
        n = rig.md_clip_frames(md["ATTACK"])[0]
        tip = tips(s)
        if s["melee"] == "club":
            strikes = cl.club_strikes(n, tip=tip)
            strikes["ATTACK_CUT_DOWN"] = strikes["ATTACK_DOWN"] = club_down(n, tip)
        else:
            strikes = cl.spear_strikes(n, tip=tip)
            strikes["ATTACK_THRUST_DOWN"] = strikes["ATTACK_DOWN"] = spear_down(n, tip)
        out.update(strikes)
        out["ATTACK"] = strikes["ATTACK_CUT_MID" if "ATTACK_CUT_MID" in strikes else "ATTACK_THRUST_MID"]
        extra = [k for k in strikes if k not in md]
    return out, extra


# -- Blender ------------------------------------------------------------------------------

def _head(ch):
    """Crown (socket.hat at rest), the skull's centre and the ear points (from the flesh)."""
    from mathutils import Vector

    P = ch.prefix
    crown = ch.arm.data.bones[P + "socket.hat"].head_local.copy()
    head = ch.arm.data.bones[P + "head"].head_local.copy()
    centre = Vector((0.0, head.y + 0.01, (crown.z + head.z) / 2 + 0.01))
    band = [v.co for v in ch.flesh.data.vertices if crown.z - 0.16 < v.co.z < crown.z - 0.09 and abs(v.co.y - centre.y) < 0.05]
    x = max((abs(c.x) for c in band), default=0.075)
    ear_z = sum(c.z for c in band if abs(c.x) > x - 0.01) / max(1, sum(1 for c in band if abs(c.x) > x - 0.01))
    ear_y = sum(c.y for c in band if abs(c.x) > x - 0.01) / max(1, sum(1 for c in band if abs(c.x) > x - 0.01))
    return crown, centre, x, ear_y, ear_z


def dress(ch, s):
    """The unku (shell and skirt), cotton armour, yacolla and sandals."""
    from . import body as bd

    name = ch.name
    key = f"unku_{s['unku']}"
    ch.shell("unku", key, lambda c, n, h, p: (h in TORSO and 0.90 < c.z < 1.52) or (h in UPPER and c.z > 1.36), 0.020)
    # The skirt: the panel hangs straight from the hips to the knee, standing off the legs.
    skirt = bd.tube(ch.arm, f"{name}.unku_skirt", 1.00, 0.50, (0.195, 0.150), (0.245, 0.195), gap_deg=0, rings=6, seg=20,
                    prefix=ch.prefix)
    ch.add("clothes", skirt, key, symmetric=True)
    if s["quilt"]:
        ch.shell("quilt", "inca_quilt", lambda c, n, h, p: (h in TORSO and 0.95 < c.z < 1.50) or (h in UPPER and c.z > 1.30),
                 0.036)
    if s["yacolla"]:
        ch.shell("yacolla", "yacolla", lambda c, n, h, p: (h in TORSO and c.y > -0.02 and 0.72 < c.z < 1.54)
                 or h in {"clavicle_l", "clavicle_r"} or (h in UPPER and c.z > 1.30 and c.y > -0.04)
                 or (h in LEGS and c.y > 0.02 and c.z > 0.55), 0.055)
    meso.dress(ch, {"dress": ("sandals",)})


def hair(ch):
    """Cropped hair: a thin shell over the scalp to a bowl line (brow at the front, over the
    ears at the sides, the nape behind), with a fringe of locks (cards) along its edge."""
    from mathutils import Matrix

    from . import gear, held

    crown, centre, _ex, _ey, _ez = _head(ch)
    top = crown.z

    def line(dy):  # the hair's lower edge: brow in front, low behind
        return top - 0.065 - 0.085 * max(0.0, min(1.0, (dy + 0.06) / 0.14))

    ch.shell("hair", "inca_hair", lambda c, n, h, p: h in {"head", "neck"} and c.z > line(c.y - centre.y)
             and not (n.y < -0.6 and c.z < top - 0.05), 0.007)
    it = held.Item("fringe")
    it.keys = {"locks": "inca_hair"}
    r = top - centre.z
    for i in range(28):
        a = 2 * math.pi * i / 28  # 0 at the front
        dx, dy = math.sin(a), -math.cos(a)
        z = line(dy * r * 1.05) + 0.012
        base = (centre.x + dx * r * 1.02, centre.y + dy * r * 1.08, z)
        tip = (base[0] + dx * 0.012, base[1] + dy * 0.012, z - 0.035 - 0.01 * (i % 2))
        mid = ((base[0] + tip[0]) / 2 + dx * 0.004, (base[1] + tip[1]) / 2 + dy * 0.004, (z + tip[2]) / 2)
        it.strip([base, mid, tip], 0.022, "locks", normal=(dx, dy, 0))
        it.strip([tip, mid, base], 0.022, "locks", normal=(dx, dy, 0))
    obj = gear.held(it, f"{ch.name}.fringe")
    gear.rigid(obj, ch.arm, ch.prefix + "head", Matrix.Identity(4), part=1)
    ch.add("gear", obj, "inca_hair")


def spools(ch):
    """Gold ear spools: thick discs in the lobes."""
    from mathutils import Matrix

    from . import gear, held

    _crown, _centre, ear_x, ear_y, ear_z = _head(ch)
    it = held.Item("spools")
    it.keys = {"disc": "spool"}
    for s in (-1, 1):
        x = s * (ear_x + 0.004)
        it.tube([((x, ear_y, ear_z - 0.025), 0.026), ((x + s * 0.012, ear_y, ear_z - 0.025), 0.026)], "disc", n=10)
    obj = gear.held(it, f"{ch.name}.spools")
    gear.rigid(obj, ch.arm, ch.prefix + "head", Matrix.Identity(4), part=1)
    ch.add("gear", obj, "spool")


def _hat(ch, item, slot, lift=0.0):
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import gear

    obj = gear.held(item, f"{ch.name}.{slot}")
    am.attach(obj, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((0, 0, lift))))
    ch.held[slot] = obj
    ch.keys[obj.name] = obj["acr_material_keys"][0]
    return obj


def arm(ch, s):
    """Headgear, weapons, quiver and the shield on the back, placed in the stand."""
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import gear, held, masks

    name = ch.name
    am.pose(ch.arm, cl.bare(stance(s)))
    if s["head"] == "helmet":
        _hat(ch, helmet(), "helmet")
        if s["plumes"]:
            _hat(ch, plumes(s["plumes"], s["plume_len"], base_z=0.03, front=False), "plumes")
    else:
        _hat(ch, band(), "llautu")
        if s["plumes"]:
            _hat(ch, plumes(s["plumes"], s["plume_len"]), "plumes")
    item = s["hand_r"]
    if item == "champi":
        gear.hold(ch, champi(), f"{name}.champi")
    elif item == "spear":
        gear.hold(ch, spear_item(), f"{name}.spear", keys={"head": "brass"})
    elif item == "sling":
        gear.hold(ch, "sling", f"{name}.sling")
    elif item == "blowpipe":
        gear.hold(ch, "blowpipe", f"{name}.blowpipe")
    if s["bow"]:
        gear.hold(ch, "longbow", f"{name}.bow", "socket.hand_l")
        q = gear.held("quiver", f"{name}.quiver")
        mouth = Vector(held.quiver().sockets["socket.mouth"])
        r = Matrix.Rotation(math.radians(-25), 4, "Y")
        # On its strap across the chest, seated on the back (`gear.seat`, `straps.carry`).
        from . import native, straps

        straps.carry(ch, f"{name}.quiver_strap", *native.QUIVER_STRAP, key="hide")
        gear.seat(ch, q, Matrix.Translation(Vector(cl.QUIVER_MOUTH) - (r.to_3x3() @ mouth)) @ r, (0.0, -1.0, 0.0))
        ch.add("gear", q, "hide")
    ch.paint_masks = list(getattr(ch, "paint_masks", []))
    if s["shield"]:
        import bpy

        # On the back between the shoulder blades, its face outwards (+Y), a little tilted.
        at = Matrix.Translation((0.0, 0.17, 1.24)) @ Matrix.Rotation(math.radians(180), 4, "Z") \
            @ Matrix.Rotation(math.radians(-8), 4, "X")
        sh = gear.held(back_shield(), f"{name}.inca_shield")
        frame = bpy.data.objects.new(f"{name}.shield_frame", None)
        bpy.context.scene.collection.objects.link(frame)
        frame.matrix_world = at
        gear.seat(ch, sh, at, (0.0, -1.0, 0.0), turn=False)
        frame.matrix_world = ch.seat_moves[sh.name] @ at  # the design's frame moves with it
        ch.add("gear", sh, "inca_shield")
        ch.paint_masks.append(masks.entry("chimalli_halves", ("inca_shield",), s["shield_design"], frame=frame.name,
                                          radius=0.24))
        ch.shield_frame = frame


def make(root, name, s):
    from .humans import Character

    ch = Character(root, name, s["build"])
    ch.body()
    dress(ch, s)
    hair(ch)
    if s["spools"]:
        spools(ch)
    ch.bone_layer()
    ch.caps()
    arm(ch, s)
    return ch


def _probe(ch, s, made):
    """Where the striking point is on each strike's hit frame (world, m): the strike
    layer's check that the blow reaches docs/melee.md's targets."""
    import bpy

    from . import armature as am

    weapon = ch.held.get(s["hand_r"] or "")
    if weapon is None:
        return {}
    sock = next((c for c in weapon.children if c.name.endswith(("socket.tip", "socket.muzzle", "socket.pouch"))), None)
    out = {}
    for k, clip in made.items():
        if not k.startswith("ATTACK_"):
            continue
        pose = dict(clip)[cl.STRIKE_HIT_FRAME] if cl.STRIKE_HIT_FRAME in dict(clip) else clip[1][1]
        am.pose(ch.arm, cl.solve(ch.arm, pose))
        bpy.context.view_layer.update()
        p = (sock or weapon).matrix_world.translation
        out[k] = [round(p.x, 3), round(p.y, 3), round(p.z, 3)]
    return out


def build(ctx, s):
    """Builds, animates, paints and prepares for export the man of SPEC `s`."""
    import bpy

    from . import armature as am
    from . import crawl, heads, outfit
    from .humans import LAYER_OF, prepare_export

    ch = make(ctx.root, ctx.name, s)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    made, extra = unit_clips(s)
    probe = _probe(ch, s, made)
    for clip, keys in made.items():
        cl.apply(ch.arm, clip, keys)
    for clip, keys in crawl.clips().items():
        am.add_clip(ch.arm, clip, keys)
    am.set_clip(ch.arm, "STAND")
    ch.arm.animation_data.action = None
    am.pose(ch.arm, cl.solve(ch.arm, stance(s)))
    report = {}
    colours = ch.colours(ctx.ref, s["palette"], report=report, mix=s["mix"], fixed=s["fixed"])
    outfit.dress(ch, colours, LAYER_OF)
    heads.finish(ch)
    frame = getattr(ch, "shield_frame", None)
    if frame is not None:
        bpy.data.objects.remove(frame)
    ch.arm.animation_data.action = None
    am.pose(ch.arm, cl.solve(ch.arm, stance(s)))
    for pb in ch.arm.pose.bones:  # the "show" keys leave socket scales behind
        pb.scale = (1.0, 1.0, 1.0)
    am.pose(ch.arm, cl.solve(ch.arm, stance(s)))
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()),
                   acr_clips=[c for c in made if c in s["md"]] + list(crawl.CLIPS), acr_extra_clips=extra,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), acr_family="inca")
    ctx.stats = {"triangles": tris, "light": report, "colours": colours, "strike_points": probe}
