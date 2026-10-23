"""Painted masks over the outfit atlas (docs/assets-humans.md, "Lace and body paint"): lace
on the hat's brim and the buttonholes of lapels and cuffs, body paint on the skin of Native
warriors. Pure Python to import (numpy only inside the mask functions, which run in
Blender's `outfit.dress`).

A mask is registered with `registry.mask(name, fn)`: `fn(p, n, L, seed, **params)` returns a
weight 0..1 per texel from the texel's rest-pose position `p` and normal `n` ((N, 3) numpy,
armature space; or the frame of an object, below) and `L` (`head_top`). A character asks for
masks in `ch.paint_masks`, a list of entries (`entry`):
`{"mask": name, "keys": (material keys...), "rgb": sRGB, "params": {...}, "frame": object
name or None}`. `outfit.dress` paints each over the atlas cells of the painters of `keys`,
in that object's local frame when `frame` is set (the hat's own brim), after the cloth or
skin is painted. Without entries nothing changes (EngM18 and FrnM18 stay as they were).

Colours are palette keys registered here: `lace` (worsted or gilt braid; the family's
palette sets it), `paint_red` (red ochre), `paint_black` (charcoal), `paint_white` (white
clay).
"""

import math

from . import registry

registry.material("lace", "wool", fallback=(0.80, 0.64, 0.28))
registry.material("paint_red", "flesh", fallback=(0.52, 0.13, 0.07))
registry.material("paint_black", "flesh", fallback=(0.05, 0.05, 0.05))
registry.material("paint_white", "flesh", fallback=(0.86, 0.84, 0.78))

PAINT_COLOURS = ("paint_red", "paint_black", "paint_white")


def _smooth(np, e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def _bars(np, s, period, width):
    """1 on bars `width` wide every `period` along coordinate `s` (soft edges)."""
    d = np.abs((s / period) - np.round(s / period)) * period
    return _smooth(np, width / 2, width / 2 * 0.6, d)


# -- lace --------------------------------------------------------------------------------

def tricorn_brim(a):
    """The tricorn's outer brim radius at angle `a` (`gear.tricorn`): (rx, ry)."""
    rel = a + math.pi / 2
    k = (1 - math.cos(3 * rel)) / 2
    r = 0.19 - 0.04 * k
    return r, r + 0.01


def hat_lace(p, n, L, seed, width=0.012):
    """Braid round the edge of the tricorn's brim, `width` m wide (in the hat's frame)."""
    import numpy as np

    a = np.arctan2(p[:, 1], p[:, 0])
    rel = a + math.pi / 2
    k = (1 - np.cos(3 * rel)) / 2
    rx = 0.19 - 0.04 * k
    rho = np.hypot(p[:, 0] / rx, p[:, 1] / (rx + 0.01))
    edge = _smooth(np, 1.0 - 1.15 * width / rx, 1.0 - 0.85 * width / rx, rho)
    return edge * _smooth(np, -0.045, -0.035, p[:, 2] - 0.0) * _smooth(np, 0.55, 0.6, rho)


def buttonhole_lace(p, n, L, seed, period=0.055, bar=0.011, cuff_period=0.045):
    """Bars of lace across the lapels (horizontal, every `period` m down the chest) and
    across the cuffs (along the forearm, every `cuff_period` m), on the facings cloth."""
    import numpy as np

    x, y, z = np.abs(p[:, 0]), p[:, 1], p[:, 2]
    front = _smooth(np, 0.1, -0.3, n[:, 1])
    chest = front * _smooth(np, 0.17, 0.15, x) * _smooth(np, 0.02, 0.035, x) * _smooth(np, 1.02, 1.06, z) \
        * _smooth(np, 1.47, 1.43, z) * _smooth(np, 0.13, 0.11, x)
    lapels = chest * _bars(np, z - 1.06, period, bar)
    # The arms hang 35° out in the rest pose: s runs along the forearm, down to the wrist.
    s = x * math.sin(math.radians(35)) - z * math.cos(math.radians(35))
    cuff = _smooth(np, 0.2, 0.25, x) * _smooth(np, 0.4, -0.2, n[:, 1]) * _bars(np, s, cuff_period, bar)
    return np.clip(lapels + cuff, 0.0, 1.0)


# -- body paint ------------------------------------------------------------------------

def _front(np, n):
    return _smooth(np, 0.0, -0.35, n[:, 1])


def _face(np, p, n, L):
    top = L["head_top"]
    return _front(np, n) * _smooth(np, top - 0.27, top - 0.22, p[:, 2]) * _smooth(np, top - 0.02, top - 0.05, p[:, 2])


def paint_face_half(p, n, L, seed, side=1.0):
    """One half of the face painted (left `side` 1, right -1), a soft edge down the nose."""
    import numpy as np

    return _face(np, p, n, L) * _smooth(np, -0.004, 0.004, side * p[:, 0])


def paint_face_band(p, n, L, seed, high=0.10, width=0.035):
    """A band across the eyes (`high` m below the top of the head)."""
    import numpy as np

    z = p[:, 2] - (L["head_top"] - high)
    return _face(np, p, n, L) * _smooth(np, width / 2, width / 2 * 0.6, np.abs(z))


def paint_stripes(p, n, L, seed, period=0.07, width=0.022, chest=True, arms=True):
    """Horizontal stripes across the chest and round the upper arms."""
    import numpy as np

    x, z = np.abs(p[:, 0]), p[:, 2]
    w = np.zeros(len(p), np.float32)
    if chest:
        w += _front(np, n) * _smooth(np, 0.19, 0.16, x) * _smooth(np, 1.02, 1.06, z) * _smooth(np, 1.44, 1.40, z) \
            * _bars(np, z, period, width)
    if arms:
        s = x * math.sin(math.radians(35)) - z * math.cos(math.radians(35))
        upper = _smooth(np, 0.20, 0.24, x) * _smooth(np, 0.42, 0.36, x)
        w += upper * _bars(np, s, period * 0.7, width)
    return np.clip(w, 0.0, 1.0)


def paint_hands(p, n, L, seed, at=((0.07, 1.30), (-0.09, 1.20)), scale=1.0):
    """Hand prints on the chest: a palm and four fingers and a thumb, at (x, z) each."""
    import numpy as np

    w = np.zeros(len(p), np.float32)
    f = _front(np, n)
    for cx, cz in at:
        dx, dz = (p[:, 0] - cx) / scale, (p[:, 2] - cz) / scale
        palm = np.hypot(dx / 0.042, dz / 0.050)
        m = _smooth(np, 1.0, 0.85, palm)
        for i, (fx, ln) in enumerate(((-0.030, 0.07), (-0.010, 0.08), (0.010, 0.075), (0.028, 0.06))):
            fz = 0.05 + ln / 2
            d = np.hypot((dx - fx) / 0.0095, (dz - fz) / (ln / 2))
            m = np.maximum(m, _smooth(np, 1.0, 0.8, d))
        d = np.hypot((dx + 0.055) / 0.011, (dz - 0.01) / 0.03)
        m = np.maximum(m, _smooth(np, 1.0, 0.8, d))
        w = np.maximum(w, m)
    return w * f


def paint_dots(p, n, L, seed, step=0.035, radius=0.008, face=True, chest=True):
    """Rows of dots (a half-step offset every other row) over the cheeks and the chest."""
    import numpy as np

    x, z = p[:, 0], p[:, 2]
    row = np.round(z / step)
    xs = x + (row % 2) * step / 2
    d = np.hypot(xs - np.round(xs / step) * step, z - row * step)
    dots = _smooth(np, radius, radius * 0.6, d)
    region = np.zeros(len(p), np.float32)
    if face:
        top = L["head_top"]
        region = np.maximum(region, _face(np, p, n, L) * _smooth(np, top - 0.13, top - 0.15, z) * _smooth(np, 0.02, 0.03, np.abs(x)))
    if chest:
        region = np.maximum(region, _front(np, n) * _smooth(np, 0.17, 0.14, np.abs(x)) * _smooth(np, 1.20, 1.24, z)
                            * _smooth(np, 1.44, 1.40, z))
    return dots * region


for _name, _fn in (("hat_lace", hat_lace), ("buttonhole_lace", buttonhole_lace), ("paint_face_half", paint_face_half),
                   ("paint_face_band", paint_face_band), ("paint_stripes", paint_stripes),
                   ("paint_hands", paint_hands), ("paint_dots", paint_dots)):
    registry.mask(_name, _fn)

# Body paint sets: (mask, colour key, params) each. `paint_set` picks one by name or seed.
PAINT_SETS = {
    "red_face_half": [("paint_face_half", "paint_red", {"side": 1.0})],
    "black_face_half": [("paint_face_half", "paint_black", {"side": -1.0}), ("paint_dots", "paint_white", {"chest": False})],
    "red_black": [("paint_face_half", "paint_red", {"side": 1.0}), ("paint_face_band", "paint_black", {})],
    "black_stripes": [("paint_stripes", "paint_black", {}), ("paint_face_band", "paint_black", {})],
    "red_hands": [("paint_hands", "paint_red", {}), ("paint_face_band", "paint_red", {"width": 0.03})],
    "white_dots": [("paint_dots", "paint_white", {}), ("paint_stripes", "paint_red", {"chest": False})],
}


def paint_set(choice):
    """The paint set `choice` (a name in `PAINT_SETS`), or one picked by an integer seed
    (deterministic: a unit or an instance seed gets the same set every build)."""
    if isinstance(choice, str):
        return PAINT_SETS[choice]
    names = sorted(PAINT_SETS)
    return PAINT_SETS[names[(int(choice) * 2654435761 >> 7) % len(names)]]


def entry(mask, keys, rgb, frame=None, **params):
    """A `ch.paint_masks` entry."""
    if mask not in registry.MASKS:
        raise KeyError(f"mask {mask!r} is not registered (have {sorted(registry.MASKS)})")
    return {"mask": mask, "keys": tuple(keys), "rgb": tuple(rgb), "params": params, "frame": frame}


def _colour(palette, key):
    return tuple((palette or {}).get(key) or registry.FALLBACK[key])


def body_paint(choice, palette=None, keys=("skin",)):
    """Entries for paint set `choice` on the skin (colours from `palette` or the fallbacks)."""
    return [entry(m, keys, _colour(palette, c), **params) for m, c, params in paint_set(choice)]


def lace(palette=None, hat=None, width=0.012, buttonholes=True, facings=("facings", "lapels")):
    """Entries for lace in the palette's `lace` colour: round the brim of `hat` (the hat
    object's name, painted in its own frame) and on the buttonholes of `facings`."""
    rgb = _colour(palette, "lace")
    out = []
    if hat:
        out.append(entry("hat_lace", ("hat",), rgb, frame=hat, width=width))
    if buttonholes:
        out.append(entry("buttonhole_lace", facings, rgb))
    return out
