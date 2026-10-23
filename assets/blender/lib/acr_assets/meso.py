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
    # Feathers, jewellery, woven trim (fixed: never mixed with the sprite's dull pixels).
    "quetzal": (0.05, 0.50, 0.30), "macaw": (0.78, 0.10, 0.06), "jade": (0.22, 0.62, 0.45),
    "turquoise": (0.10, 0.62, 0.66), "gold": (0.85, 0.62, 0.18), "meso_trim": (0.92, 0.50, 0.10),
}
BUILD = {"width": 1.02, "shoulders": 1.06, "limbs": 1.14, "hips": 1.0}
DEFAULTS = {"weapons": {}, "active": {}, "build": BUILD, "mix": 0.35, "fixed": ("maxtlatl", "plumes", "jaguar", "eagle", "chimalli", "quetzal", "macaw", "jade", "turquoise",
                                                                             "gold", "meso_trim", "meso_hair"),
            "dress": ("maxtlatl", "ichcahuipilli", "sandals"), "suit": None, "head": "topknot", "paint": None,
            "hand_r": "macuahuitl", "atlatl": False, "bow": False, "shield": "fret", "melee": "club",
            "plume": ("quetzal", "macaw"), "jewels": ("ears", "neck", "cuffs"), "work": False}

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


def paint_hair(p, n, rgb, L, seed):
    """Black hair combed back and tied up: fine strands (streaks along z), a soft sheen."""
    np, coat = _np()
    base = coat.lin(rgb)
    fine = coat.fbm(p, (420.0, 420.0, 25.0), seed, octaves=2)
    broad = coat.fbm(p, (60.0, 60.0, 8.0), seed + 7)
    sheen = coat.smooth(0.62, 0.80, fine) * 0.35
    col = base * (0.7 + 0.5 * broad)[:, None]
    return coat.mix(col, coat.lin((0.22, 0.20, 0.22)), sheen)


# Woven borders (sRGB): red with yellow and turquoise stripes (the Codex Mendoza mantles).
BORDER = ((0.72, 0.10, 0.06), (0.92, 0.72, 0.16), (0.10, 0.55, 0.60))


def _hem(lo, hi):
    """A painter: cotton with a woven border between z = lo and hi (rest metres): red,
    yellow stripes, a turquoise stripe in the middle."""
    def fn(p, n, rgb, L, seed):
        np, coat = _np()
        col = coat.lin(rgb) * (0.9 + 0.1 * coat.fbm(p, 300.0, seed, octaves=1))[:, None]
        z = p[:, 2]
        band = coat.smooth(hi + 0.004, hi - 0.004, z) * coat.smooth(lo - 0.02, lo - 0.012, z)
        t = np.clip((z - lo) / max(hi - lo, 1e-3), 0.0, 1.0)
        red, yel, tq = (coat.lin(c) for c in BORDER)
        stripe = np.where(((t * 6.0) % 2.0 < 0.5)[:, None], yel, red)
        stripe = np.where(((t > 0.42) & (t < 0.58))[:, None], tq, stripe)
        return coat.mix(col, stripe * (0.9 + 0.1 * coat.fbm(p, 500.0, seed + 2, octaves=1))[:, None], band)
    return fn


def paint_trim(p, n, rgb, L, seed):
    """Woven band (headbands, the turban): red stripes across the colour, a fine weave."""
    np, coat = _np()
    s = (p[:, 2] * 120.0) % 2.0 < 0.5
    col = np.where(s[:, None], coat.lin(BORDER[0]), coat.lin(rgb))
    return col * (0.88 + 0.12 * coat.fbm(p, 600.0, seed, octaves=1))[:, None]


def paint_stone(p, n, rgb, L, seed):
    """Polished jade and turquoise: mottled, veined."""
    np, coat = _np()
    base = coat.lin(rgb)
    vein = coat.smooth(0.55, 0.75, coat.fbm(p, 90.0, seed))
    return coat.mix(base * (0.8 + 0.35 * coat.fbm(p, 40.0, seed + 1))[:, None], np.clip(base * 1.5, 0, 1), vein * 0.4)


def paint_gold(p, n, rgb, L, seed):
    np, coat = _np()
    return coat.lin(rgb) * (0.85 + 0.25 * coat.fbm(p, 80.0, seed))[:, None]


def paint_feather(p, n, rgb, L, seed):
    """Long tail feathers: barbs (fine noise), iridescent mottle."""
    np, coat = _np()
    base = coat.lin(rgb)
    barbs = coat.fbm(p, 500.0, seed, octaves=1)
    mott = coat.fbm(p, 25.0, seed + 3)
    return base * (0.65 + 0.35 * barbs + 0.35 * mott)[:, None]


registry.painter("jaguar", fn=paint_jaguar, finish=(0.95, 0.35, 0.0))
registry.painter("eagle", fn=paint_eagle, finish=(0.9, 0.30, 0.0))
registry.painter("quilted", fn=paint_quilted, finish=(1.0, 0.10, 0.0))
registry.painter("plumes", fn=paint_plumes, finish=(0.6, 0.30, 0.10))
registry.painter("meso_hair", fn=paint_hair, finish=(0.55, 0.35, 0.05))
registry.painter("maxtlatl_hem", fn=_hem(0.57, 0.64), finish=(0.95, 0.12, 0.0))
registry.painter("hipcloth", fn=_hem(0.70, 0.77), finish=(0.95, 0.12, 0.0))
registry.painter("tilmatli", fn=_hem(0.80, 0.88), finish=(0.95, 0.12, 0.0))
registry.painter("meso_trim", fn=paint_trim, finish=(0.95, 0.15, 0.0))
# One painter per key: keys sharing a painter share an atlas cell and so a colour.
registry.painter("jade", fn=paint_stone, finish=(0.35, 0.60, 0.0))
registry.painter("turquoise", fn=paint_stone, finish=(0.35, 0.60, 0.0))
registry.painter("meso_gold", fn=paint_gold, finish=(0.35, 0.50, 0.9))
registry.painter("quetzal", fn=paint_feather, finish=(0.6, 0.35, 0.10))
registry.painter("macaw", fn=paint_feather, finish=(0.6, 0.35, 0.10))
registry.material("maxtlatl", "linen", painter="maxtlatl_hem", fallback=PALETTE["maxtlatl"])
registry.material("hipcloth", "linen", painter="hipcloth", fallback=PALETTE["hipcloth"])
registry.material("tilmatli", "linen", painter="tilmatli", fallback=PALETTE["tilmatli"])
registry.material("ichcahuipilli", "linen", painter="quilted", fallback=PALETTE["ichcahuipilli"])
registry.material("quetzal", "hair", painter="quetzal", fallback=PALETTE["quetzal"])
registry.material("macaw", "hair", painter="macaw", fallback=PALETTE["macaw"])
registry.material("jade", "bone", painter="jade", fallback=PALETTE["jade"])
registry.material("turquoise", "bone", painter="turquoise", fallback=PALETTE["turquoise"])
registry.material("gold", "metal", painter="meso_gold", fallback=PALETTE["gold"])
registry.material("meso_trim", "linen", painter="meso_trim", fallback=PALETTE["meso_trim"])
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
    """A fan of feather strips from `centre` (the eagle helmet's crest): `count` strips over
    `spread` degrees about the vertical, leaning `tilt` degrees back (+Y)."""
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


def _unit(v):
    m = math.sqrt(sum(c * c for c in v)) or 1.0
    return tuple(c / m for c in v)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def feather(it, part, base, d, length, width, twist=0.0, droop=0.15, n=7):
    """One long feather: a vane (narrow at the quill, pointed at the tip) folded along the
    rachis (the rib stands out of the vane), drooping by `droop` of its length,
    double-sided. Modelled outline rather than an alpha card: the humans' atlas has no
    alpha channel, and the fold keeps it a feather edge-on."""
    d = _unit(d)
    ref = (0.0, 1.0, 0.0) if abs(d[1]) < 0.9 else (1.0, 0.0, 0.0)
    side = _unit(_cross(ref, d))
    nrm = _cross(d, side)
    c, s_ = math.cos(twist), math.sin(twist)
    side, nrm = (tuple(c * a + s_ * b for a, b in zip(side, nrm)), tuple(c * b - s_ * a for a, b in zip(side, nrm)))
    for back in (0.0, -0.0008):  # the two sides as their own vertices (valid mesh)
        rows = []
        for k in range(n + 1):
            t = k / n
            w = width * math.sin(math.pi * (0.12 + 0.88 * t)) ** 0.6 * (0.55 if t < 0.15 else 1.0)
            c0 = tuple(b + dd * length * t - (droop * length * t * t if i == 2 else 0.0) + nn * back
                       for i, (b, dd, nn) in enumerate(zip(base, d, nrm)))
            rib = tuple(cc + nn * w * 0.18 for cc, nn in zip(c0, nrm))
            rows.append((it.v(tuple(cc - ss * w / 2 for cc, ss in zip(c0, side))), it.v(rib),
                         it.v(tuple(cc + ss * w / 2 for cc, ss in zip(c0, side)))))
        for a, b in zip(rows, rows[1:]):
            for q in ((a[0], b[0], b[1], a[1]), (a[1], b[1], b[2], a[2])):
                it.f(q if back == 0.0 else tuple(reversed(q)), part)


def plume_fan(it, part, centre, count, length, spread, tilt, width=0.045, droop=0.18, seed=1):
    """A fan of `count` feathers radiating from `centre` (`spread` degrees left to right,
    leaning `tilt` degrees back), each a little longer or shorter and twisted."""
    for i in range(count):
        u = i / max(count - 1, 1)
        a = math.radians(-spread / 2 + spread * u)
        t = math.radians(tilt + 6 * math.sin(i * 2.3 + seed))
        d = (math.sin(a), math.sin(t) * math.cos(a), math.cos(t) * math.cos(a))
        ln = length * (0.88 + 0.12 * math.cos(math.pi * (u - 0.5)) + 0.04 * math.sin(i * 3.7 + seed))
        feather(it, part, centre, d, ln, width, twist=math.radians(25 * math.sin(i * 1.9 + seed)), droop=droop)


def _cap(it, part, r, sy, top, front, back, zc=-0.065, rings=5, n=16):
    """Hair close over the skull: an ellipsoid cap from the hairline (`front`, high on the
    forehead) round to the nape (`back`, low) up to `top` above the crown."""
    grid = []
    for j in range(rings):
        u = j / rings
        ring = []
        for i in range(n):
            a = -math.pi / 2 + 2 * math.pi * i / n
            rim = (front + back) / 2 + (back - front) / 2 * math.sin(a)
            z = rim + (top - rim) * (1 - (1 - u) ** 1.6)
            k = 1.0 if z <= zc else max(0.0, 1 - ((z - zc) / (top - zc)) ** 2) ** 0.5
            ring.append(it.v((math.cos(a) * r * k, math.sin(a) * r * sy * k, z)))
        grid.append(ring)
    apex = it.v((0.0, 0.01, top))
    for ra, rb in zip(grid, grid[1:]):
        for i in range(n):
            it.f((ra[i], ra[(i + 1) % n], rb[(i + 1) % n], rb[i]), part)
    for i in range(n):
        it.f((grid[-1][i], grid[-1][(i + 1) % n], apex), part)


def helmet(kind):
    """The animal helmets as held Items in the hat frame (origin at `socket.hat`, the crown;
    face -Y), one key each: `jaguar` and `eagle`."""
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
    else:
        raise ValueError(f"meso helmet {kind!r}")
    return it


HELMETS = ("jaguar", "eagle")
HEADS = ("jaguar", "eagle", "crest", "maya", "topknot", "band")


def head_parts(kind, plume=("quetzal", "macaw")):
    """The head's own pieces (skinned rigidly to the head, never dropped) as [(Item, key)]
    in the hat frame (origin at `socket.hat`, the crown; face -Y), one key per Item: hair
    (`meso_hair`, combed back and tied up, the face free), woven bands (`meso_trim`),
    feathers (`plume` = (long, short) keys: quetzal green, macaw red) and jade."""
    from . import held

    if kind not in HEADS:
        raise ValueError(f"meso headgear {kind!r}")
    long_, short = plume
    out = []

    def item(name, key):
        it = held.Item(name)
        it.keys = {"p": key}
        out.append((it, key))
        return it

    hair = None
    if kind not in HELMETS:
        hair = item("hair", "meso_hair")
        _cap(hair, "p", 0.106, 1.10, 0.016, -0.040, -0.150)
    if kind == "topknot":
        # The temillotl: hair gathered in a pillar on the crown, tied with a woven band,
        # two red feathers in the knot.
        hair.tube([((0, 0.03, 0.0), 0.034), ((0, 0.03, 0.08), 0.030), ((0, 0.05, 0.12), 0.040), ((0, 0.07, 0.15), 0.0)],
                  "p", n=8)
        tie = item("tie", "meso_trim")
        tie.tube([((0, 0.03, 0.025), 0.037), ((0, 0.03, 0.06), 0.035)], "p", n=8, caps=False)
        f = item("feathers", short)
        plume_fan(f, "p", (0, 0.05, 0.10), 3, 0.24, 40, 35, width=0.035, droop=0.25, seed=3)
    elif kind == "band":
        # A woven headband, the hair tied in a club at the back, red and green feathers.
        band = item("band", "meso_trim")
        band.tube([((0, 0.0, -0.075), 0.111), ((0, 0.0, -0.040), 0.110)], "p", n=16, caps=False, scale=(1.0, 1.11))
        hair.tube([((0, 0.10, -0.08), 0.03), ((0, 0.13, -0.10), 0.04), ((0, 0.14, -0.14), 0.025), ((0, 0.13, -0.17), 0.0)],
                  "p", n=8)
        f = item("feathers", short)
        plume_fan(f, "p", (0, 0.11, -0.06), 5, 0.26, 70, 30, width=0.035, droop=0.20, seed=1)
        g = item("feathers_long", long_)
        plume_fan(g, "p", (0, 0.115, -0.05), 2, 0.34, 20, 25, width=0.035, droop=0.15, seed=5)
    elif kind in ("maya", "crest"):
        # A turban of woven cloth, a jade disc on the brow, a tall fan of long quetzal
        # plumes on the crown with red macaw feathers before it (Bonampak; the Aztec
        # quetzalapanecayotl for `crest`).
        band = item("band", "meso_trim")
        band.tube([((0, 0.0, -0.075), 0.116), ((0, 0.0, -0.02), 0.114), ((0, 0.0, 0.01), 0.10)], "p", n=16,
                  scale=(1.0, 1.10))
        jade = item("jade", "jade")
        jade.tube([((0, -0.120, -0.045), 0.026), ((0, -0.132, -0.045), 0.024)], "p", n=10)
        g = item("plumes", long_)
        tall = 0.62 if kind == "maya" else 0.55
        plume_fan(g, "p", (0, 0.04, 0.02), 13, tall, 150, 28, width=0.05, droop=0.22, seed=2)
        f = item("feathers", short)
        plume_fan(f, "p", (0, -0.06, -0.005), 11, tall * 0.48, 170, 6, width=0.05, droop=0.10, seed=4)
    elif kind == "jaguar":
        # A crest of green plumes rising from the back of the jaguar helmet.
        g = item("plumes", long_)
        plume_fan(g, "p", (0, 0.08, 0.05), 7, 0.42, 80, 40, width=0.045, droop=0.25, seed=6)
    elif kind == "eagle":
        f = item("feathers", short)
        plume_fan(f, "p", (0, 0.08, 0.04), 5, 0.30, 60, 45, width=0.04, droop=0.2, seed=7)
    return out


def ear_spools(ear=0.084):
    """Jade ear spools in the hat frame."""
    from . import held

    it = held.Item("ear_spools")
    it.keys = {"p": "jade"}
    for s in (-1, 1):
        it.tube([((s * (ear - 0.004), 0.008, -0.105), 0.019), ((s * (ear + 0.010), 0.008, -0.105), 0.021)], "p", n=10)
    return it


def necklace(centre, rx=0.10, ry=0.095, drop=0.07, beads=22):
    """A collar of jade beads round the neck base (world rest, ring centre `centre`): low
    on the chest in front; a turquoise pendant. Returns [(Item, key)]."""
    from . import held

    it = held.Item("necklace")
    it.keys = {"p": "jade"}
    for i in range(beads):
        a = 2 * math.pi * i / beads
        y = math.sin(a)
        p = (centre[0] + math.cos(a) * rx, centre[1] + y * ry, centre[2] - drop * max(0.0, -y) ** 1.5)
        it.box(p, (0.022, 0.022, 0.022), "p")
    pend = held.Item("pendant")
    pend.keys = {"p": "turquoise"}
    pend.tube([((centre[0], centre[1] - ry - 0.006, centre[2] - drop - 0.02), 0.03),
               ((centre[0], centre[1] - ry - 0.018, centre[2] - drop - 0.02), 0.028)], "p", n=10)
    return [(it, "jade"), (pend, "turquoise")]


def cuff(a, b, r, key="gold"):
    """A band round a limb from point a to b (world rest), radius r."""
    from . import held

    it = held.Item("cuff")
    it.keys = {"p": key}
    it.tube([(a, r), (b, r * 1.03)], "p", n=12)
    return it


def macuahuitl(length=0.92, blades=8):
    """A macuahuitl after the Real Armeria piece: a broad flat paddle of hard wood (11 cm
    wide, 3 cm thick), a round handle with a knob, and obsidian blades set close along both
    edges (`stone`): prisms 4 cm proud with thickness, so the edge reads serrated from the
    side and the face. `socket.tip` at the end of the paddle."""
    from . import held

    it = held.Item("macuahuitl")
    it.keys = {"club": "wood", "blades": "stone"}
    it.tube([((0, 0, -0.10), 0.017), ((0, 0, 0.10), 0.018)], "club", n=8)
    it.tube([((0, 0, -0.13), 0.0), ((0, 0, -0.12), 0.024), ((0, 0, -0.10), 0.019)], "club", n=8, caps=False)
    w, top = 0.11, length - 0.08
    it.box((0, 0, (0.08 + top) / 2), (0.030, w, top - 0.08), "club", taper=(0.85, 0.75))
    for s in (-1, 1):
        for i in range(blades):
            z = 0.15 + (top - 0.17) * (i + 0.5) / blades
            ww = w * (0.75 + 0.25 * (z - 0.08) / (top - 0.08)) / 2
            it.box((0, s * (ww + 0.012), z), (0.012, 0.045, 0.06), "blades", taper=(0.6, 0.5))
    it.sockets["socket.tip"] = (0.0, 0.0, top)
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


def axe():
    """The peasant's stone axe: the trade tomahawk's shape, the head in stone."""
    from . import held

    it = held.tomahawk(0.55)
    it.name = "stone_axe"
    it.keys = dict(it.keys, head="stone")
    return it


def tips(s):
    """Grip-to-point length of the right-hand weapon (from the item data); for the bow
    strike the bow's grip to its upper tip."""
    from . import held

    if s["melee"] == "bow":
        return BOW_TIP
    if s["hand_r"] == "macuahuitl":
        return macuahuitl().sockets["socket.tip"][2]
    if s["hand_r"] == "spear":
        return held.spear(length=2.1, grip=0.8, head=0.26).sockets["socket.tip"][2]
    if s["hand_r"] == "dart":
        return dart().sockets["socket.tip"][2]
    return cl.CLUB_TIP


# Blows that lean the trunk forward (on skeleton H positive spine X bends forward; the
# shared `clips.STOOP` and the strikes' lean bend back and stop ~0.45 m short: the officers
# family's finding, 93f5839). A man on the ground as the sim leaves him: about 2.0 m centre
# to centre (2.57 m walking up; the strike layer's step makes up the rest).
TARGET_DOWN = (0.0, -2.0, 0.20)
LUNGE_DOWN = {"thigh_l": (-76, -3, 0), "shin_l": (84, 0, 0), "foot_l": (8, 0, 0),
              "thigh_r": (40, 3, 0), "shin_r": (22, 0, 0), "foot_r": (-30, 0, 0)}
LEAN_IN = {"spine": (8, 0, -14), "spine1": (4, 0, 0), "root_loc": (0, -0.24, -0.05), "thigh_l": (-30, -3, 0),
           "shin_l": (22, 0, 0)}
BOW_TIP = 0.70  # longbow grip to its upper tip (the bow strike)


def _stoop_down():
    return dict(cl.STAND_FREE, **dict(cl.STOOP, root_loc=(0, -0.80, -0.27), spine=(30, 0, -8), spine1=(10, 0, 0),
                                      spine2=(4, 0, 0), head=(18, 0, 0), **LUNGE_DOWN))


def _raised(hit, r_rot, lift=(0, 0.20, 0.70), hand="r"):
    base = cl._with(hit, root_loc=(0, -0.40, -0.10), spine=(12, 0, -4), spine1=(4, 0, 0),
                    thigh_l=(-40, -3, 0), shin_l=(40, 0, 0), thigh_r=(20, 3, 0), shin_r=(14, 0, 0))
    base.pop("reach", None)
    return _hit(base, cl.add(hit["reach"][hand][0], lift), r_rot, hand=hand, two=len(hit["reach"]) > 1)


def _hit(base, point, rot, hand="r", two=False, along=0.40):
    """`base` with `hand`'s weapon at world rotation `rot`, its grip at `point`; with `two`
    the other hand on the haft `along` below (spear) or above (bow) the grip."""
    if hand == "r":
        p = cl._blow(base, point, rot, l_point=cl.add(point, cl.axis(rot), along) if two else None)
        return p
    p = cl._with(base)
    p["world"] = dict(p.get("world", {}), **{"socket.hand_l": rot})
    p["reach"] = {"l": cl._l(point)}
    if two:
        p["reach"]["r"] = cl._r(cl.add(point, cl.axis(rot), -along))
    return p


def strikes(s, n):
    """The melee strikes of index 0 at the weapon's own length, leaning in: `club` (the
    macuahuitl or axe hack, mid and down), `spear` (the thrust, two hands, mid and down),
    `bow` (the archers' MECH: the bow in both hands chopped down like a staff, the bow
    hand at the grip)."""
    tip = tips(s)
    lean = dict(cl.STAND_FREE, **LEAN_IN)
    if s["melee"] == "spear":
        out = cl.spear_strikes(n, tip=tip)
        level, down_r = (88, 0, 0), (130, 0, 0)
        mid = _hit(lean, cl.grip_for(cl.TARGET_MID, level, tip), level, two=True, along=0.45)
        back = _hit(dict(cl.STAND_FREE, spine=(0, 0, 12)), cl.add(mid["reach"]["r"][0], (0, 0.38, -0.05)), level,
                    two=True, along=0.45)
        down = _hit(_stoop_down(), cl.grip_for(TARGET_DOWN, down_r, tip), down_r, two=True, along=0.45)
        up = _raised(down, (122, 0, 0), lift=(0, 0.30, 0.30))
        out["ATTACK_THRUST_MID"] = cl._strike(n, back, mid, back)
        out["ATTACK_THRUST_DOWN"] = out["ATTACK_DOWN"] = cl._strike(n, up, down, up)
        return out
    hand = "l" if s["melee"] == "bow" else "r"
    two = hand == "l"
    side = (0.12 if hand == "l" else -0.16)
    mid_r, down_r = ((78, 0, -6) if hand == "l" else (78, 0, 6)), (150, 0, 0)
    over = _hit(dict(cl.STAND_FREE, spine=(4, 0, 12 if hand == "r" else -12)), (side, 0.12, 1.86), (-75, 0, 0),
                hand=hand, two=two, along=0.30)
    ready = _hit(cl.STAND_FREE, (side, -0.22, 1.12), (30, 0, 0), hand=hand, two=two, along=0.30)
    mid = _hit(lean, cl.grip_for(cl.add(cl.TARGET_MID, (0, 0, 0.12)), mid_r, tip), mid_r, hand=hand, two=two, along=0.30)
    down = _hit(_stoop_down(), cl.grip_for(TARGET_DOWN, down_r, tip), down_r, hand=hand, two=two, along=0.30)
    up = _raised(down, (-40, 0, 0), hand=hand)
    out = {"ATTACK_CUT_MID": cl._strike(n, over, mid, ready)}
    out["ATTACK_CUT_DOWN"] = out["ATTACK_DOWN"] = cl._strike(n, up, down, up)
    return out


def _carry():
    """A load carried on the back with the tumpline: both hands up at the strap on the brow."""
    p = cl._with(cl.STAND_FREE, spine=(10, 0, 0), spine1=(4, 0, 0))
    p["reach"] = {"r": cl._r((-0.08, -0.12, 1.66)), "l": cl._l((0.08, -0.12, 1.66))}
    return p


def work_clips(s, md):
    """The peasant's work sets (`work`): WORK and ATTACK1 the axe at wood (an overhead chop
    at a trunk), ATTACK3 the field (hacking the ground before the feet), ATTACK4 the load
    (picked up, carried with both hands on the tumpline), ATTACK5 the hack; every walk and
    ready line of a set from the stand (the carry for set 4)."""
    stand = stance(s)
    carry = _carry()
    over = _hit(dict(cl.STAND_FREE, spine=(4, 0, 12)), (-0.16, 0.10, 1.84), (-70, 0, 0))
    chop = _hit(dict(cl.STAND_FREE, **LEAN_IN), (-0.10, -0.55, 1.05), (95, 0, 30))
    low = _hit(dict(cl.STAND_FREE, spine=(28, 0, 0), spine1=(10, 0, 0), root_loc=(0, -0.05, -0.08),
                    thigh_l=(-20, -3, 0), shin_l=(24, 0, 0), thigh_r=(-6, 3, 0), shin_r=(14, 0, 0)),
               (-0.10, -0.50, 0.55), (150, 0, 0))
    lift = _hit(dict(cl.STAND_FREE, spine=(14, 0, 0)), (-0.14, -0.10, 1.55), (-30, 0, 0))
    pick = cl._with(_stoop_down(), root_loc=(0, -0.10, -0.30), thigh_l=(-60, -3, 0), shin_l=(80, 0, 0),
                    thigh_r=(-60, 3, 0), shin_r=(80, 0, 0), foot_l=(-20, 0, 0), foot_r=(-20, 0, 0), spine=(40, 0, 0))
    pick["reach"] = {"r": cl._r((-0.15, -0.50, 0.25)), "l": cl._l((0.15, -0.50, 0.25))}
    keys = {
        "REST": [(0.0, stand), (0.5, cl._with(stand, head=(14, 0, 8), spine=(2, 0, 4))), (1.0, stand)],
        "WORK": [(0.0, stand), (0.4, over), (0.6, chop), (1.0, stand)],
        "ATTACK1": [(0.0, over), (0.45, chop), (0.6, chop), (1.0, over)],
        "ATTACK3": [(0.0, lift), (0.45, low), (0.6, low), (1.0, lift)],
        "PATTACK4": [(0.0, stand), (0.5, pick), (1.0, carry)],
        "UATTACK4": [(0.0, stand), (0.5, pick), (1.0, carry)],
        "ATTACK4": [(0.0, carry)],
    }
    walk = poses.walk_cycle(stand)
    walk4 = poses.walk_cycle(carry)
    out = {}
    for k, line in md.items():
        w = walk4 if k.endswith("4") else walk
        cl_, pl, cr, pr = w
        if k in keys:
            ks = keys[k]
        elif k.startswith("MOTION_"):
            d = k[len("MOTION_"):].rstrip("012345")
            ks = {"L": [(0.0, cl_), (0.5, pl), (1.0, cr)], "R": [(0.0, cr), (0.5, pr), (1.0, cl_)],
                  "LB": [(0.0, cl_), (0.5, pr), (1.0, cr)], "RB": [(0.0, cr), (0.5, pl), (1.0, cl_)]}[d]
        elif k[:1] in "PU" and "ATTACK" in k or k.startswith("PSTAND") or k.startswith("ATTACK") and k[-1].isdigit():
            ks = [(0.0, carry if k.endswith("4") else stand)]
        else:
            continue
        out[k] = rig.clip_keys(rig.md_clip_frames(line)[0], ks)
    return out


def unit_clips(s):
    """{clip: [(frame, pose)]} for SPEC `s`: stand, walk (and the `0` variants), the ready
    poses, the attacks (`clips.unit_clips` by WEAPONKIND, the melee strikes at the weapon's
    own length, `strikes`), the work sets (`work_clips`), death and the lying frames.
    Returns (clips, extra clip names)."""
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
    if s["work"]:
        out.update(work_clips(s, md))
    for k, ks in keys.items():
        if k in md:
            out[k] = rig.clip_keys(rig.md_clip_frames(md[k])[0], ks)
    weapons = s["weapons"]
    out.update(cl.unit_clips(md, {i: w for i, w in weapons.items() if i > 0}, active=s["active"]))
    extra = []
    if 0 in weapons and "ATTACK" in md and s["melee"]:
        n = rig.md_clip_frames(md["ATTACK"])[0]
        made = strikes(s, n)
        if s["shield"]:
            made = {k: cl.frames(n, [(f, cl.with_shield(p) if "l" not in p.get("reach", {}) else p) for f, p in c],
                                 c.event) for k, c in made.items()}
        out.update(made)
        out["ATTACK"] = made["ATTACK_CUT_MID" if "ATTACK_CUT_MID" in made else "ATTACK_THRUST_MID"]
        if s["work"] and "ATTACK5" in md:
            n5 = rig.md_clip_frames(md["ATTACK5"])[0]
            out["ATTACK5"] = strikes(s, n5)["ATTACK_CUT_MID"]
        extra = [k for k in made if k not in md]
    return out, extra


# -- Blender ------------------------------------------------------------------------------

def _foot_z(ch):
    return ch.arm.data.bones[ch.prefix + "foot_l"].head_local.z


def _mesh(it, name):
    import bpy

    from . import gear

    me = bpy.data.meshes.new(name)
    me.from_pydata(it.verts, [], it.faces)
    me.update()
    return gear.link(bpy.data.objects.new(name, me))


def _rigid(ch, it, key, bone, matrix):
    """Item `it` skinned rigidly to `bone` at `matrix` (rest world), one material key."""
    from . import gear

    obj = _mesh(it, f"{ch.name}.{it.name}")
    gear.rigid(obj, ch.arm, ch.prefix + bone, matrix)
    ch.add("gear", obj, key)
    return obj


def cape(top=1.47, bottom=0.80, spread=70, rows=6, cols=10):
    """The tilmatli hanging down the back from the shoulders: a sheet curved round the back
    (+Y), widening and standing off towards the hem (stiff cotton), double-sided."""
    from . import held

    it = held.Item("tilmatli_cape")
    it.keys = {"p": "tilmatli"}
    grid = []
    for j in range(rows):
        t = j / (rows - 1)
        z = top + (bottom - top) * t
        rx = 0.19 + 0.05 * min(t * 4, 1.0) - 0.02 * t
        ry = 0.12 + 0.03 * min(t * 4, 1.0) + 0.09 * t
        row = []
        for i in range(cols + 1):
            a = math.radians(-spread + 2 * spread * i / cols)
            row.append(it.v((math.sin(a) * rx, 0.01 + math.cos(a) * ry, z)))
        grid.append(row)
    inner = [[it.v(tuple(c * 0.985 if k < 2 else c for k, c in enumerate(it.verts[v]))) for v in row] for row in grid]
    for g, flip in ((grid, False), (inner, True)):
        for a, b in zip(g, g[1:]):
            for i in range(cols):
                q = (a[i], a[i + 1], b[i + 1], b[i])
                it.f(tuple(reversed(q)) if flip else q, "p")
    return it


def dress(ch, s):
    """The garments of `s["dress"]` as shells, the hems and hanging cloths as tubes and
    rigid sheets standing off the body."""
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
        # Quilted cotton two fingers thick standing off the body, its hem flaring stiffly
        # over the loincloth.
        ch.shell("ichcahuipilli", "ichcahuipilli", lambda c, n, h, p: (h in TORSO and 0.90 < c.z < 1.47)
                 or (h in UPPER and c.z > 1.28), 0.042)
        hem = bd.tube(ch.arm, f"{name}.ichca_hem", 0.97, 0.86, (0.205, 0.165), (0.24, 0.20), gap_deg=0, rings=3, seg=20,
                      prefix=ch.prefix)
        ch.add("clothes", hem, "ichcahuipilli", symmetric=True)
    if "maya_jacket" in d:
        ch.shell("maya_jacket", "maya_jacket", lambda c, n, h, p: h in TORSO and 0.93 < c.z < 1.46, 0.030)
    if "maxtlatl" in d:
        ch.shell("maxtlatl", "maxtlatl", lambda c, n, h, p: h in {"hips"} | LEGS and 0.86 < c.z < 1.0, 0.012 if "tlahuiztli" not in d else 0.024)
        # The ends hanging before and behind to the knee (rigid on the hips), woven borders.
        for y, tilt, tag in ((-0.125, 6, "front"), (0.13, -8, "back")):
            flap = gear.box(f"{name}.maxtlatl_{tag}", (0.16, 0.010, 0.42), bevel=0.1)
            for v in flap.data.vertices:
                if v.co.z < 0:
                    v.co.x *= 0.85
            gear.rigid(flap, ch.arm, ch.prefix + "hips", Matrix.Translation((0.0, y, 0.80)) @ Matrix.Rotation(math.radians(tilt), 4, "X"))
            ch.add("gear", flap, "maxtlatl", symmetric=True)
    if "hipcloth" in d:
        skirt = bd.tube(ch.arm, f"{name}.hipcloth", 1.0, 0.70, (0.19, 0.15), (0.225, 0.185), gap_deg=0, rings=5, seg=18,
                        prefix=ch.prefix)
        ch.add("clothes", skirt, "hipcloth", symmetric=True)
    if "tilmatli" in d:
        ch.shell("tilmatli", "tilmatli", lambda c, n, h, p: (h in {"clavicle_l", "clavicle_r"}) or
                 (h in TORSO and c.y > 0.0 and 1.30 < c.z < 1.52), 0.036)
        _rigid(ch, cape(), "tilmatli", "spine2", Matrix.Identity(4))
    if "sandals" in d:
        ch.shell("sandals", "sandals", lambda c, n, h, p: (h in FEET and c.z < fz + 0.02)
                 or (h in SHINS and fz + 0.03 < c.z < fz + 0.07), 0.007)


def _bone_point(ch, bone, t):
    b = ch.arm.data.bones[ch.prefix + bone]
    return tuple(b.head_local + (b.tail_local - b.head_local) * t)


def adorn(ch, s):
    """The head's own pieces (hair, bands, feathers: `head_parts`) skinned to the head, the
    jewellery (`jewels`: jade ear spools, a jade collar with a turquoise pendant, gold
    wrist cuffs and turquoise arm bands) skinned to their bones (rest pose)."""
    from mathutils import Matrix

    bones = ch.arm.data.bones
    hat = bones.get("socket.hat") or bones[ch.prefix + "socket.hat"]
    at_hat = Matrix.Translation(hat.head_local + type(hat.head_local)((0, 0, -0.01)))
    for it, key in head_parts(s["head"], s["plume"]):
        _rigid(ch, it, key, "head", at_hat)
    j = set(s["jewels"])
    if "ears" in j and s["head"] not in HELMETS:
        _rigid(ch, ear_spools(), "jade", "head", at_hat)
    if "neck" in j:
        nb = bones.get(ch.prefix + "neck")
        c = tuple(nb.head_local) if nb else tuple(hat.head_local - type(hat.head_local)((0, 0, 0.25)))
        for it, key in necklace((c[0], c[1] - 0.005, c[2] - 0.02)):
            _rigid(ch, it, key, "spine2", Matrix.Identity(4))
    if "cuffs" in j and "tlahuiztli" not in s["dress"]:
        for sd in ("l", "r"):
            a, b = _bone_point(ch, f"forearm_{sd}", 0.74), _bone_point(ch, f"forearm_{sd}", 0.86)
            _rigid(ch, cuff(a, b, 0.040, "gold"), "gold", f"forearm_{sd}", Matrix.Identity(4))
            if "ichcahuipilli" not in s["dress"]:
                a, b = _bone_point(ch, f"upperarm_{sd}", 0.62), _bone_point(ch, f"upperarm_{sd}", 0.70)
                _rigid(ch, cuff(a, b, 0.050, "turquoise"), "turquoise", f"upperarm_{sd}", Matrix.Identity(4))


def _upright(ch, socket):
    from mathutils import Matrix

    from . import armature as am

    sock = am.world(ch.arm, socket)
    rest_up = sock.to_3x3() @ ch.arm.pose.bones[socket].bone.matrix_local.to_3x3().inverted()
    return Matrix.Translation(sock.translation) @ rest_up.to_4x4()


def arm(ch, s):
    """Helmets, weapons, shield and quiver (held: dropped at death), placed in the stand."""
    from mathutils import Matrix, Vector

    from . import armature as am
    from . import gear, held

    name = ch.name
    am.pose(ch.arm, cl.bare(stance(s)))
    if s["head"] in HELMETS:
        hat = gear.held(helmet(s["head"]), f"{name}.{s['head']}")
        am.attach(hat, ch.arm, "socket.hat", Matrix.Translation(am.world(ch.arm, "socket.hat").translation + Vector((0, 0, -0.01))))
        ch.held["hat"] = hat
        ch.keys[hat.name] = hat["acr_material_keys"][0]
    item = s["hand_r"]
    if item == "macuahuitl":
        gear.hold(ch, macuahuitl(), f"{name}.macuahuitl")
    elif item == "axe":
        gear.hold(ch, axe(), f"{name}.axe")
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
        # On its strap across the chest, seated on the back (`gear.seat`, `straps.carry`).
        from . import native, straps

        straps.carry(ch, f"{name}.quiver_strap", *native.QUIVER_STRAP, key="hide")
        gear.seat(ch, q, Matrix.Translation(Vector(cl.QUIVER_MOUTH) - (r.to_3x3() @ mouth)) @ r, (0.0, -1.0, 0.0))
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
    adorn(ch, s)
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
