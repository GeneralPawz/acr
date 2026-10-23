"""Mesoamerica (Aztec and Maya) as one family (docs/humans-roster.md, family 7;
docs/families/meso.md): bare-skinned men in cotton, on skeleton H with the layers of #178.
A unit script is data: its `.MD` clip lines, its `WEAPONKIND`s and `SETACTIVEPOINT`
frames, and a `SPEC` (`spec`); `build(ctx)` calls `meso.build(ctx, SPEC)`.

Garments are shells over the skin (`Character.shell`), each its own material key and
painter (registered here):
- `maxtlatl` the loincloth (a band round the hips and a flap hanging in front),
- `ichcahuipilli` the quilted cotton armour (thick: a 3 cm shell, quilting stitched in),
- `tlahuiztli` the elite's full suit (`jaguar` rosettes or `eagle` feathers),
- `tilmatli` the cloak (over the back, knotted on the right shoulder),
- `hipcloth` the Maya wrap (a short skirt on the hips and thighs),
- `maya_jacket` the Maya sleeveless quilted jacket, `sandals`.
Headgear (held, on `socket.hat`, modelled shapes): `jaguar` and `eagle` helmets (the
warrior's face in the open jaws or beak), `crest` (Aztec quetzal feather fan), `maya`
(a turban with a tall feather frame), `topknot` (the temillotl, hair tied up), `band`.
Gear: macuahuitl, tepoztopilli (`spear` with a stone head), atlatl with darts, bow,
arrows and quiver, and the chimalli (a round shield, feather fringe, a painted design).

SPEC keys (`spec` fills the defaults):
- `md` the `.MD` lines by clip, `weapons` {attack index: WEAPONKIND}, `active` the
  `SETACTIVEPOINT` frames {clip: frame};
- `palette` colours by material key (sRGB) over `PALETTE`; `mix` sprite mix (0..1);
- `dress` the garments (names above), `suit` "jaguar" or "eagle" (tlahuiztli painter),
  `head` the headgear, `paint` a `masks.PAINT_SETS` name (body paint) or None;
- `hand_r` "macuahuitl", "spear", "dart", "arrow" or None, `atlatl` True for the thrower,
  `bow` True (bow in the left hand, quiver on the back), `shield` a chimalli design
  (`CHIMALLI`) or None;
- `melee` the strike clips of index 0: "club" (the macuahuitl's hack), "spear" (the
  thrust) or None.
"""

import math

from . import clips as cl
from . import poses, registry, rig

TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
UPPER = {"upperarm_l", "upperarm_r"}
ARMS = UPPER | {"forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}

# Defaults (the Aztec macuahuitl warrior); units override.
PALETTE = {
    "skin": (0.56, 0.37, 0.25), "bone": (0.85, 0.81, 0.69), "meso_hair": (0.05, 0.04, 0.04),
    "maxtlatl": (0.90, 0.87, 0.78), "ichcahuipilli": (0.88, 0.84, 0.72), "tilmatli": (0.80, 0.45, 0.20),
    "hipcloth": (0.85, 0.80, 0.68), "maya_jacket": (0.86, 0.80, 0.66), "sandals": (0.45, 0.32, 0.18),
    "jaguar": (0.80, 0.60, 0.25), "eagle": (0.55, 0.40, 0.25), "plumes": (0.10, 0.55, 0.35),
    "chimalli": (0.85, 0.80, 0.70), "atlatl": (0.42, 0.26, 0.13),
    "wood": (0.40, 0.25, 0.13), "stone": (0.08, 0.08, 0.10), "cord": (0.75, 0.68, 0.52),
    "metal": (0.10, 0.10, 0.12), "hide": (0.60, 0.46, 0.30), "feather": (0.85, 0.82, 0.76),
    "paint_red": (0.62, 0.14, 0.08), "paint_black": (0.06, 0.05, 0.05), "paint_white": (0.88, 0.86, 0.80),
}
BUILD = {"width": 1.02, "shoulders": 1.06, "limbs": 1.14, "hips": 1.0}
DEFAULTS = {"weapons": {}, "active": {}, "build": BUILD, "mix": 0.35, "fixed": ("maxtlatl", "plumes", "jaguar", "eagle", "chimalli"),
            "dress": ("maxtlatl", "ichcahuipilli", "sandals"), "suit": None, "head": "topknot", "paint": None,
            "hand_r": "macuahuitl", "atlatl": False, "bow": False, "shield": "fret", "melee": "club"}

# Chimalli designs (shield face, painted in the shield's own frame): colours (sRGB) and the
# mask. The Codex Mendoza's tribute shields: the xicalcoliuhqui (stepped fret) halves, the
# "down ball" rings of feather balls, quartered faces.
CHIMALLI = {
    "fret": {"base": (0.85, 0.80, 0.68), "paint": (0.12, 0.30, 0.55), "mask": "chimalli_halves"},
    "rings": {"base": (0.80, 0.18, 0.10), "paint": (0.92, 0.90, 0.84), "mask": "chimalli_rings"},
    "quarters": {"base": (0.90, 0.75, 0.25), "paint": (0.10, 0.10, 0.10), "mask": "chimalli_quarters"},
}


def spec(md, **kw):
    """A full SPEC: the defaults with `palette` and `build` merged key by key."""
    unknown = set(kw) - set(DEFAULTS) - {"palette"}
    if unknown:
        raise KeyError(f"unknown SPEC keys {sorted(unknown)}")
    pal = dict(PALETTE, **kw.pop("palette", {}))
    build = dict(BUILD, **kw.pop("build", {}))
    return dict(DEFAULTS, **kw, md=md, palette=pal, build=build)


# -- painters (numpy; registered at import) ------------------------------------------------

def _np():
    import numpy as np

    from . import coat

    return np, coat


def _cells(np, p, size, seed):
    """Distance to the nearest jittered point of a 3D grid of `size` m, and its cell hash."""
    q = p / size
    best = np.full(len(p), 9.0, np.float32)
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            for dz in (-1, 0, 1):
                c = np.floor(q) + np.array((dx, dy, dz), np.float32)
                ci = c.astype(np.int64)
                jx = ((ci[:, 0] * 73856093) ^ (ci[:, 1] * 19349663) ^ (ci[:, 2] * 83492791) ^ seed) & 1023
                jy = ((ci[:, 0] * 19349663) ^ (ci[:, 1] * 83492791) ^ (ci[:, 2] * 73856093) ^ seed) & 1023
                jz = ((ci[:, 0] * 83492791) ^ (ci[:, 1] * 73856093) ^ (ci[:, 2] * 19349663) ^ seed) & 1023
                pt = c + np.stack((jx, jy, jz), 1).astype(np.float32) / 1023.0 * 0.7 + 0.15
                best = np.minimum(best, np.linalg.norm(q - pt, axis=1))
    return best


def paint_jaguar(p, n, rgb, L, seed):
    """Jaguar pelt (the tlahuiztli of the jaguar warriors, Codex Mendoza f. 64r): tawny
    ground, paler under, black rosettes (a broken ring round a darker centre)."""
    np, coat = _np()
    base = coat.lin(rgb)
    col = base * (0.88 + 0.12 * coat.fbm(p, 30.0, seed) + 0.05 * coat.fbm(p, 300.0, seed + 1, octaves=1))[:, None]
    d = _cells(np, p, 0.055, seed)
    ring = coat.smooth(0.22, 0.28, d) * coat.smooth(0.42, 0.35, d)
    ring *= coat.smooth(0.25, 0.45, coat.fbm(p, 60.0, seed + 3))  # broken rings
    centre = coat.smooth(0.16, 0.10, d) * 0.45
    col = coat.mix(col, base * 0.55, centre)
    return coat.mix(col, coat.lin((0.05, 0.04, 0.03)), np.clip(ring * 1.4, 0, 1))


def paint_eagle(p, n, rgb, L, seed):
    """Eagle suit: rows of overlapping feathers (scallops, 3.5 cm), dark tips, a pale shaft."""
    np, coat = _np()
    base = coat.lin(rgb)
    row = p[:, 2] / 0.035
    off = np.floor(row) % 2 * 0.5
    ang = np.arctan2(p[:, 1], p[:, 0]) * 0.11 / 0.03 + off
    fx = ang - np.floor(ang) - 0.5
    fz = row - np.floor(row)
    tip = coat.smooth(0.55, 0.9, fz + 0.6 * fx * fx)
    col = base * (0.85 + 0.25 * (1.0 - fz))[:, None]
    col = coat.mix(col, base * 0.45, tip * 0.7)
    shaft = coat.smooth(0.06, 0.02, np.abs(fx)) * (1.0 - tip)
    return coat.mix(col, coat.lin((0.85, 0.80, 0.70)), shaft * 0.35)


def paint_quilted(p, n, rgb, L, seed):
    """Quilted cotton (ichcahuipilli, the Maya jacket): soft cotton, stitched in a diamond
    grid of 6 cm, each pad slightly puffed (lighter in the middle), grubby."""
    np, coat = _np()
    base = coat.lin(rgb)
    a = (p[:, 0] + p[:, 2]) / 0.06
    b = (p[:, 0] - p[:, 2]) / 0.06 + 0.5 * p[:, 1] / 0.06
    da = np.abs(a - np.round(a))
    db = np.abs(b - np.round(b))
    seam = coat.smooth(0.07, 0.02, np.minimum(da, db))
    puff = np.minimum(da, db)
    col = base * (0.86 + 0.25 * puff + 0.06 * coat.fbm(p, 400.0, seed, octaves=1))[:, None]
    col = coat.mix(col, base * 0.62, seam)
    grime = coat.smooth(0.55, 0.85, coat.fbm(p, 10.0, seed + 4)) * 0.18
    return coat.mix(col, coat.lin((0.45, 0.38, 0.28)), grime)


def paint_plumes(p, n, rgb, L, seed):
    """Feathers (quetzal and others): long streaks along the feather, iridescent mottle."""
    np, coat = _np()
    base = coat.lin(rgb)
    streak = coat.fbm(p, (160.0, 160.0, 18.0), seed)
    return base * (0.75 + 0.45 * streak + 0.08 * coat.fbm(p, 40.0, seed + 2))[:, None]


registry.painter("jaguar", fn=paint_jaguar, finish=(0.95, 0.35, 0.0))
registry.painter("eagle", fn=paint_eagle, finish=(0.9, 0.30, 0.0))
registry.painter("quilted", fn=paint_quilted, finish=(1.0, 0.10, 0.0))
registry.painter("plumes", fn=paint_plumes, finish=(0.6, 0.30, 0.10))
registry.painter("meso_hair", like="felt", finish=(0.7, 0.2, 0.1))
registry.material("maxtlatl", "linen", painter="linen", fallback=PALETTE["maxtlatl"])
registry.material("hipcloth", "linen", painter="linen", fallback=PALETTE["hipcloth"])
registry.material("tilmatli", "linen", painter="linen", fallback=PALETTE["tilmatli"])
registry.material("ichcahuipilli", "linen", painter="quilted", fallback=PALETTE["ichcahuipilli"])
registry.material("maya_jacket", "linen", painter="quilted", fallback=PALETTE["maya_jacket"])
registry.material("jaguar", "hide", painter="jaguar", fallback=PALETTE["jaguar"])
registry.material("eagle", "hide", painter="eagle", fallback=PALETTE["eagle"])
registry.material("plumes", "hair", painter="plumes", fallback=PALETTE["plumes"])
registry.material("sandals", "leather", painter="leather", fallback=PALETTE["sandals"])
registry.material("meso_hair", "hair", painter="meso_hair", fallback=PALETTE["meso_hair"])
registry.painter("chimalli", like="linen", finish=(0.95, 0.12, 0.0))
registry.material("chimalli", "hide", painter="chimalli", fallback=PALETTE["chimalli"])
registry.material("atlatl", "wood", fallback=PALETTE["atlatl"])


# -- chimalli designs (masks in the shield's frame: face -Y, x across, z up) ---------------

def _polar(p):
    import numpy as np

    return np.hypot(p[:, 0], p[:, 2]), np.arctan2(p[:, 2], p[:, 0])


def chimalli_halves(p, n, L, seed, radius=0.30):
    """The lower half painted, its edge a stepped fret (xicalcoliuhqui) across the middle."""
    import numpy as np

    from .masks import _smooth

    x = p[:, 0] / radius
    step = np.floor((x + 1.0) * 4.0) % 2 * 0.12 - 0.06
    return _smooth(np, 0.01, -0.01, p[:, 2] / radius - step) * _smooth(np, 0.0, -0.02, p[:, 1])


def chimalli_rings(p, n, L, seed, radius=0.30):
    """Seven feather down balls in a ring and one in the middle (Codex Mendoza)."""
    import numpy as np

    from .masks import _smooth

    r, a = _polar(p)
    k = np.round(a / (2 * math.pi / 7)) * (2 * math.pi / 7)
    d = np.hypot(p[:, 0] - 0.6 * radius * np.cos(k), p[:, 2] - 0.6 * radius * np.sin(k))
    d = np.minimum(d, r)
    return _smooth(np, 0.05 * radius / 0.3, 0.035 * radius / 0.3, d) * _smooth(np, 0.0, -0.02, p[:, 1])


def chimalli_quarters(p, n, L, seed, radius=0.30):
    """Quartered face: the upper left and lower right quarters painted."""
    import numpy as np

    from .masks import _smooth

    return _smooth(np, -0.004, 0.004, -p[:, 0] * p[:, 2]) * _smooth(np, 0.0, -0.02, p[:, 1])


registry.mask("chimalli_halves", chimalli_halves)
registry.mask("chimalli_rings", chimalli_rings)
registry.mask("chimalli_quarters", chimalli_quarters)


# -- items (held.Item data, pure) --------------------------------------------------------

def chimalli(radius=0.31, fringe=9):
    """A chimalli: a round shield of cane wicker faced with hide and cotton, slightly domed,
    a feather fringe hanging from the lower rim (one key: painted in one atlas cell, the
    design a mask). Origin at the hand grip, face -Y (as `held.shield`)."""
    from . import held

    it = held.shield(radius)
    it.name = "chimalli"
    it.keys = {"face": "chimalli", "rim": "chimalli", "fringe": "chimalli"}
    for i in range(fringe):
        a = math.radians(-150 + 120 * i / max(fringe - 1, 1))
        x, z = math.cos(a) * radius * 0.98, math.sin(a) * radius * 0.98
        pts = [(x, -0.055, z), (x * 1.04, -0.06, z - 0.08), (x * 1.06, -0.05, z - 0.17)]
        it.strip(pts, 0.035, "fringe", normal=(0, 1, 0))
        it.strip(list(reversed(pts)), 0.035, "fringe", normal=(0, 1, 0))
    return it


def atlatl(length=0.55):
    """An atlatl (spear thrower): a flat board along +Z from the grip (finger loops at the
    origin) to the hook at the far end that takes the dart's butt."""
    from . import held

    it = held.Item("atlatl")
    it.keys = {"board": "atlatl"}
    it.box((0, 0, length / 2 - 0.05), (0.012, 0.035, length), "board", taper=(0.8, 0.7))
    it.box((0, 0.02, length - 0.06), (0.012, 0.03, 0.02), "board")
    for s in (-1, 1):
        it.tube([((s * 0.02, -0.01, -0.02), 0.008), ((s * 0.03, -0.01, 0.02), 0.008)], "board", n=4)
    it.sockets["socket.hook"] = (0.0, 0.035, length - 0.06)
    return it


def dart(length=1.55, grip=0.55):
    """An atlatl dart: a light cane shaft, a stone point, fletched at the butt."""
    from . import held

    it = held.spear(length=length, grip=grip, head=0.09)
    it.name = "dart"
    it.keys = dict(it.keys, head="stone")
    base = -grip
    for a in (0, 120, 240):
        r = math.radians(a)
        p = [it.v((0, 0, base + 0.02)), it.v((0, 0, base + 0.16)), it.v((0.02 * math.cos(r), 0.02 * math.sin(r), base + 0.05))]
        it.f(p, "binding")
        it.f(tuple(reversed(p)), "binding")
    return it


def _dome(it, part, r, z0, z1, n=12, y=0.0, sy=1.0):
    """A dome over the skull from z0 (rim) to z1 (top)."""
    path = [((0, y, z0), r), ((0, y, z0 + (z1 - z0) * 0.45), r * 0.98), ((0, y, z0 + (z1 - z0) * 0.8), r * 0.70),
            ((0, y, z1), 0.0)]
    return it.tube(path, part, n=n, caps=False, scale=(1.0, sy))


def _fan(it, part, centre, count, length, spread, tilt, width=0.04, curve=0.25):
    """A fan of feathers from `centre`: `count` strips over `spread` degrees about the
    vertical, leaning `tilt` degrees back (+Y), drooping by `curve` at the tips."""
    for i in range(count):
        a = math.radians(-spread / 2 + spread * i / max(count - 1, 1))
        t = math.radians(tilt)
        pts = []
        for k in range(4):
            s = length * k / 3
            up = s * math.cos(t) - curve * (s / length) ** 2 * length
            back = s * math.sin(t) + curve * 0.5 * (s / length) ** 2 * length
            pts.append((centre[0] + math.sin(a) * up * 0.9, centre[1] + back, centre[2] + math.cos(a) * up))
        it.strip(pts, width, part, normal=(0, 1, 0))
        it.strip(list(reversed(pts)), width, part, normal=(0, 1, 0))


def helmet(kind):
    """Headgear as an Item in the hat frame (origin at `socket.hat`, the crown; face -Y)."""
    from . import held

    it = held.Item(kind)
    if kind == "jaguar":
        # The jaguar's head worn as a helmet: the skull over the crown, the upper jaw and
        # snout over the brow, the man's face between the jaws; ears; the pelt down the nape.
        it.keys = {"pelt": "jaguar", "fangs": "jaguar"}
        _dome(it, "pelt", 0.118, -0.07, 0.05, sy=1.08)
        it.box((0, -0.11, -0.035), (0.13, 0.10, 0.07), "pelt", taper=(0.8, 0.7), shift=(0, -0.01))
        it.box((0, -0.17, -0.06), (0.10, 0.05, 0.04), "pelt")  # snout
        for s in (-1, 1):
            it.box((s * 0.075, 0.0, 0.05), (0.035, 0.02, 0.05), "pelt", taper=(0.4, 0.6))
            it.tube([((s * 0.04, -0.17, -0.08), 0.008), ((s * 0.04, -0.17, -0.12), 0.0)], "fangs", n=4)
        it.box((0, 0.10, -0.15), (0.16, 0.03, 0.20), "pelt", taper=(1.2, 1.0))
    elif kind == "eagle":
        # The eagle's head: the crown, the upper beak over the brow, crest feathers behind.
        it.keys = {"head": "eagle", "beak": "eagle"}
        _dome(it, "head", 0.118, -0.06, 0.05, sy=1.08)
        it.tube([((0, -0.08, -0.02), 0.06), ((0, -0.16, -0.04), 0.04), ((0, -0.22, -0.08), 0.015),
                 ((0, -0.23, -0.11), 0.0)], "beak", n=6, scale=(1.0, 0.8))
        _fan(it, "head", (0, 0.06, 0.0), 5, 0.22, 50, 60, width=0.035)
    elif kind == "crest":
        # Hair tied up and a fan of long quetzal plumes standing behind the head.
        it.keys = {"hair": "meso_hair", "plumes": "plumes"}
        _dome(it, "hair", 0.112, -0.10, 0.03, sy=1.05)
        _fan(it, "plumes", (0, 0.07, -0.02), 9, 0.55, 80, 25, width=0.045, curve=0.30)
        it.keys = {"hair": "plumes", "plumes": "plumes"}
    elif kind == "maya":
        # A turban of cloth, a tall frame rising from the forehead with plumes sweeping back.
        it.keys = {"band": "plumes", "frame": "plumes", "plumes": "plumes"}
        it.tube([((0, 0.0, -0.06), 0.122), ((0, 0.0, 0.0), 0.118)], "band", n=12, caps=False, scale=(1.0, 1.08))
        it.box((0, -0.09, 0.05), (0.10, 0.05, 0.11), "frame", taper=(1.3, 1.0), shift=(0, 0.04))
        _fan(it, "plumes", (0, -0.05, 0.09), 11, 0.60, 120, 50, width=0.05, curve=0.40)
    elif kind == "topknot":
        # The temillotl: the hair gathered in a pillar on the crown, tied with a red band.
        it.keys = {"hair": "meso_hair"}
        _dome(it, "hair", 0.112, -0.11, 0.025, sy=1.05)
        it.tube([((0, 0.02, 0.0), 0.035), ((0, 0.02, 0.09), 0.03), ((0, 0.04, 0.13), 0.04), ((0, 0.05, 0.15), 0.0)],
                "hair", n=6)
    elif kind == "band":
        it.keys = {"hair": "meso_hair"}
        _dome(it, "hair", 0.112, -0.12, 0.025, sy=1.05)
        it.box((0, 0.10, -0.17), (0.14, 0.04, 0.16), "hair", taper=(0.8, 1.0))
        _fan(it, "hair", (0, 0.08, -0.02), 2, 0.20, 30, 30, width=0.03)
    else:
        raise ValueError(f"meso headgear {kind!r}")
    return it


# -- clips (pure: data for clips.apply) ---------------------------------------------------

def stance(s):
    """The unit's stand: weapon at the side, the chimalli up before the body, the bow low."""
    if s["bow"]:
        return cl.STAND_FREE
    base = cl.STAND_ARMED if s["hand_r"] else cl.STAND_FREE
    if s["hand_r"] == "spear":
        # The spear at the order: upright, the butt by the right foot.
        base = cl._with(cl.STAND_FREE, forearm_r=(-30, 0, 12), world={"socket.hand_r": (0, 0, 0)})
    return cl.with_shield(base) if s["shield"] else base


def tips(s):
    """Grip-to-point length of the right-hand weapon (from the item data)."""
    from . import held

    if s["hand_r"] == "macuahuitl":
        return held.macuahuitl().sockets["socket.tip"][2]
    if s["hand_r"] == "spear":
        return held.spear(length=2.1, grip=0.8, head=0.26).sockets["socket.tip"][2]
    if s["hand_r"] == "dart":
        return dart().sockets["socket.tip"][2]
    return cl.CLUB_TIP


def unit_clips(s):
    """{clip: [(frame, pose)]} for SPEC `s`: stand, walk (and the `0` variants), the ready
    poses, the attacks (`clips.unit_clips` by WEAPONKIND, the melee strikes at the weapon's
    own length), death and the lying frames. Returns (clips, extra clip names)."""
    md = s["md"]
    stand = stance(s)
    still = cl.bare(stand)
    lie = poses.lie_prone()
    cl_, pl, cr, pr = poses.walk_cycle(stand)
    keys = {
        "STAND": [(0.0, stand)],
        "MOTION_L": [(0.0, cl_), (0.5, pl), (1.0, cr)],
        "MOTION_R": [(0.0, cr), (0.5, pr), (1.0, cl_)],
        "MOTION_LB": [(0.0, cl_), (0.5, pr), (1.0, cr)],
        "MOTION_RB": [(0.0, cr), (0.5, pl), (1.0, cl_)],
        "DEATH": poses.death_keys(dict(still), lie),
    }
    for k in ("L", "R", "LB", "RB"):
        keys[f"MOTION_{k}0"] = keys[f"MOTION_{k}"]
    for k in ("DEATHLIE1", "DEATHLIE2", "DEATHLIE3"):
        keys[k] = [(0.0, lie)]
    for k in ("PSTAND", "PATTACK", "UATTACK", "PSTAND1", "PATTACK1", "UATTACK1", "PATTACK0", "UATTACK0"):
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
        strikes = cl.club_strikes(n, tip=tip) if s["melee"] == "club" else cl.spear_strikes(n, tip=tip)
        if s["shield"]:
            strikes = {k: cl.frames(n, [(f, cl.with_shield(p) if "l" not in p.get("reach", {}) else p) for f, p in c],
                                    c.event) for k, c in strikes.items()}
        out.update(strikes)
        out["ATTACK"] = strikes["ATTACK_CUT_MID" if "ATTACK_CUT_MID" in strikes else "ATTACK_THRUST_MID"]
        extra = [k for k in strikes if k not in md]
    return out, extra


# -- Blender ------------------------------------------------------------------------------

def _foot_z(ch):
    return ch.arm.data.bones[ch.prefix + "foot_l"].head_local.z


def dress(ch, s):
    """The garments of `s["dress"]` as shells (and the hip wrap as a skirt tube)."""
    from mathutils import Matrix

    from . import body as bd
    from . import gear

    name = ch.name
    fz = _foot_z(ch)
    d = set(s["dress"])
    if "tlahuiztli" in d:
        suit = s["suit"] or "jaguar"
        ch.shell("tlahuiztli", suit, lambda c, n, h, p: (h in TORSO and 0.88 < c.z < 1.48) or h in ARMS or h in LEGS
                 or (h in SHINS and c.z > fz + 0.06), 0.016)
    if "ichcahuipilli" in d:
        ch.shell("ichcahuipilli", "ichcahuipilli", lambda c, n, h, p: (h in TORSO and 0.90 < c.z < 1.47)
                 or (h in UPPER and c.z > 1.28), 0.030)
    if "maya_jacket" in d:
        ch.shell("maya_jacket", "maya_jacket", lambda c, n, h, p: h in TORSO and 0.93 < c.z < 1.46, 0.024)
    if "maxtlatl" in d:
        ch.shell("maxtlatl", "maxtlatl", lambda c, n, h, p: h in {"hips"} | LEGS and 0.86 < c.z < 1.0, 0.012 if "tlahuiztli" not in d else 0.024)
        # The flap hanging in front (rigid on the hips; a knotted end).
        flap = gear.box(f"{name}.maxtlatl_flap", (0.15, 0.012, 0.30), bevel=0.1)
        for v in flap.data.vertices:
            if v.co.z < 0:
                v.co.x *= 0.8
        gear.rigid(flap, ch.arm, ch.prefix + "hips", Matrix.Translation((0.0, -0.125, 0.80)) @ Matrix.Rotation(math.radians(6), 4, "X"))
        ch.add("gear", flap, "maxtlatl", symmetric=True)
    if "hipcloth" in d:
        skirt = bd.tube(ch.arm, f"{name}.hipcloth", 1.0, 0.70, (0.19, 0.15), (0.22, 0.18), gap_deg=0, rings=5, seg=18,
                        prefix=ch.prefix)
        ch.add("clothes", skirt, "hipcloth", symmetric=True)
    if "tilmatli" in d:
        ch.shell("tilmatli", "tilmatli", lambda c, n, h, p: (h in TORSO and c.y > 0.0 and 0.80 < c.z < 1.52)
                 or (h in {"clavicle_l", "clavicle_r"}), 0.036)
    if "sandals" in d:
        ch.shell("sandals", "sandals", lambda c, n, h, p: (h in FEET and c.z < fz + 0.02)
                 or (h in SHINS and fz + 0.03 < c.z < fz + 0.07), 0.007)


def _upright(ch, socket):
    from mathutils import Matrix

    from . import armature as am

    sock = am.world(ch.arm, socket)
    rest_up = sock.to_3x3() @ ch.arm.pose.bones[socket].bone.matrix_local.to_3x3().inverted()
    return Matrix.Translation(sock.translation) @ rest_up.to_4x4()


def arm(ch, s):
    """Headgear, weapons, shield and quiver (held: dropped at death), placed in the stand."""
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import gear, held

    name = ch.name
    am.pose(ch.arm, cl.bare(stance(s)))
    hat = gear.held(helmet(s["head"]), f"{name}.{s['head']}")
    am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((0, 0, -0.01))))
    ch.held["hat"] = hat
    ch.keys[hat.name] = hat["acr_material_keys"][0]
    item = s["hand_r"]
    if item == "macuahuitl":
        gear.hold(ch, "macuahuitl", f"{name}.macuahuitl")
    elif item == "spear":
        gear.hold(ch, held.spear(length=2.1, grip=0.8, head=0.26), f"{name}.spear", keys={"head": "stone"})
    elif item == "dart":
        gear.hold(ch, dart(), f"{name}.dart")
    elif item == "arrow":
        gear.hold(ch, "arrow", f"{name}.arrow", keys={"head": "stone"})
    if s["atlatl"]:
        # On the hand bone itself (not the socket the throw hides at the release).
        board = gear.held(atlatl(), f"{name}.atlatl")
        am.attach(board, ch.arm, "hand_r", _upright(ch, "socket.hand_r") @ Matrix.Translation((0, 0.03, -0.02)))
        ch.held["atlatl"] = board
        ch.keys[board.name] = "atlatl"
    if s["bow"]:
        gear.hold(ch, "longbow", f"{name}.bow", "socket.hand_l")
        q = gear.held("quiver", f"{name}.quiver")
        mouth = Vector(held.quiver().sockets["socket.mouth"])
        r = Matrix.Rotation(math.radians(-25), 4, "Y")
        gear.rigid(q, ch.arm, ch.prefix + "spine2", Matrix.Translation(Vector(cl.QUIVER_MOUTH) - (r.to_3x3() @ mouth)) @ r)
        ch.add("gear", q, "hide")
    ch.paint_masks = list(getattr(ch, "paint_masks", []))
    if s["shield"]:
        d = CHIMALLI[s["shield"]]
        sh = gear.hold(ch, chimalli(), f"{name}.chimalli", "socket.hand_l")
        s["palette"]["chimalli"] = d["base"]
        ch.paint_masks.append(_mask(d["mask"], ("chimalli",), d["paint"], sh.name))
    if s["paint"]:
        from . import masks

        ch.paint_masks += masks.body_paint(s["paint"], s["palette"])


def _mask(mask, keys, rgb, frame):
    from . import masks

    return masks.entry(mask, keys, rgb, frame=frame)


def make(root, name, s):
    from .humans import Character

    ch = Character(root, name, s["build"])
    ch.body()
    dress(ch, s)
    ch.bone_layer()
    ch.caps()
    arm(ch, s)
    return ch


def build(ctx, s):
    """Builds, animates, paints and prepares for export the man of SPEC `s`."""
    from . import armature as am
    from . import crawl, faces, heads, outfit
    from .humans import LAYER_OF, prepare_export

    ch = make(ctx.root, ctx.name, s)
    heads.variants(ch)
    ch.morphs(extra_deflate=ch.layers["gear"])
    ch.masks()
    made, extra = unit_clips(s)
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
    ch.arm.animation_data.action = None
    am.pose(ch.arm, cl.solve(ch.arm, stance(s)))
    tris = ch.triangles()
    prepare_export(ch.all_meshes(), ctx.root, acr_triangles=sum(tris.values()),
                   acr_clips=[c for c in made if c in s["md"]] + list(crawl.CLIPS), acr_extra_clips=extra,
                   acr_limb_radius=ch.limb_radius, acr_light=report.get("light_dir_camera", []),
                   acr_layers=list(ch.layers), **faces.extras("regular"))
    ctx.stats = {"triangles": tris, "light": report, "colours": colours}
