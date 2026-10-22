"""Individual faces and bodies (#283, pure Python, no Blender): the head-local fields of the
base face and the `face_*` morph targets, the zones of the `_FACE` attribute, the regions of
the hair and beard variants, and the reference choice of one soldier's look from a seed.

Head-local coordinates (metres): x across the face (the model's left is +x), y depth from
the face front (0 on the face surface at the eyes, growing towards the back of the head;
the model faces -Y, so y = Y - Y_front), z up from eye level. Every field is smooth, zero
away from the head and mirror-symmetric in x (displacements in x are odd, the rest even).
See docs/assets-humans.md "Faces and bodies".
"""

import math

# Morph targets: name -> (lowest weight, highest weight). Signed ones go both ways (narrow
# to wide jaw); the renderer mixes them per soldier (`choose`).
FACE_MORPHS = {
    "face_jaw": (-1.0, 1.0),          # jaw width at the angles
    "face_chin": (-1.0, 1.0),         # chin forward and longer / receding
    "face_cheekbones": (-1.0, 1.0),   # high, wide cheekbones / flat
    "face_nose": (-1.0, 1.0),         # nose length (tip forward and down) / short snub
    "face_nose_bridge": (-1.0, 1.0),  # aquiline hump / flat, broken bridge
    "face_brow": (0.0, 1.0),          # heavy brow ridge
    "face_eyes": (-1.0, 1.0),         # eye spacing (wide-set / close-set)
    "face_age": (0.0, 1.0),           # sagging: jowls, hollow cheeks, eye bags, nose tip down
    "face_fat": (0.0, 1.0),           # fleshy face and double chin
    "face_thin": (0.0, 1.0),          # gaunt: hollow cheeks and temples
}
# Body build morphs (flesh and clothes move away from / towards their bones).
BODY_MORPHS = {"body_stocky": (0.0, 1.0), "body_thin": (0.0, 1.0)}
STOCKY_KEEP = 1.14  # distance to the bone kept at weight 1 (body_stocky)
THIN_KEEP = 0.88

HEIGHT_M = (1.58, 1.80)  # per-soldier height range: uniform scale of the instance
HEIGHT_MEAN_M = 1.69     # 18th-century recruits: about 5 ft 6 in
HEIGHT_SD_M = 0.055

# Eye level below the top of the head (the skin painter's eyes sit here, `outfit.paint`).
EYE_BELOW_TOP = 0.105

# Hair colours (sRGB), the tint multiplied into the hair material (`acr_tint` "hair"); the
# last ones are greys for older men.
HAIR_COLOURS = {
    "black": (0.07, 0.06, 0.05), "dark_brown": (0.16, 0.11, 0.07), "brown": (0.28, 0.19, 0.11),
    "chestnut": (0.38, 0.21, 0.11), "dark_blond": (0.50, 0.40, 0.24), "ginger": (0.55, 0.27, 0.11),
    "grizzled": (0.36, 0.33, 0.30), "grey": (0.55, 0.53, 0.50),
}
GREYS = ("grizzled", "grey")
# How common each (non-grey) colour is among the recruits.
HAIR_ODDS = {"black": 0.12, "dark_brown": 0.30, "brown": 0.25, "chestnut": 0.12, "dark_blond": 0.15, "ginger": 0.06}
# Skin tints (sRGB multipliers on the painted skin, `acr_tint` "skin") per recruitment.
SKIN_TONES = {
    "english": {
        "fair": (1.05, 1.02, 0.99), "pale_ruddy": (1.04, 0.95, 0.91), "ruddy": (1.03, 0.88, 0.80),
        "weathered": (0.95, 0.85, 0.76), "sunburnt": (1.00, 0.82, 0.70), "tanned": (0.86, 0.73, 0.61),
        "olive": (0.88, 0.82, 0.68),
    },
}

# Variants: choice -> node suffixes (`<name>.<suffix>`); "none" shows nothing.
HAIR_VARIANTS = {
    "short": ["hair_short"], "tied": ["hair_tied", "hair_queue"], "loose": ["hair_loose"],
}
BEARD_VARIANTS = {
    "none": [], "moustache": ["beard_moustache"], "sideburns": ["beard_sideburns"],
    "full": ["beard_full"], "moustache_sideburns": ["beard_moustache", "beard_sideburns"],
}
# Odds per style: line infantry shaved (stubble of a few days, now and then sideburns or a
# moustache; the queue was regulation), civilians and irregulars did as they liked.
STYLES = {
    "regular": {
        "hair": {"tied": 0.75, "short": 0.25},
        "beard": {"none": 0.78, "sideburns": 0.14, "moustache": 0.05, "moustache_sideburns": 0.03},
        "stubble": (0.15, 1.0),
    },
    "civilian": {
        "hair": {"loose": 0.45, "tied": 0.25, "short": 0.30},
        "beard": {"none": 0.40, "full": 0.28, "moustache": 0.12, "sideburns": 0.12, "moustache_sideburns": 0.08},
        "stubble": (0.3, 1.0),
    },
}
# Shell offsets (m) of the hair and beard meshes over the skin.
OFFSETS = {"hair_short": 0.0045, "hair_tied": 0.008, "hair_loose": 0.012, "beard_full": 0.008,
           "beard_moustache": 0.0055, "beard_sideburns": 0.005}


def _smooth(e0, e1, x):
    """Smoothstep from e0 (0) to e1 (1); e0 > e1 runs downwards."""
    if e0 == e1:
        return 1.0 if x >= e1 else 0.0
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def _g(*pairs):
    """exp(-sum(((v - c) / s)^2)) over (v, c, s) triples."""
    return math.exp(-sum(((v - c) / s) ** 2 for v, c, s in pairs))


def _sign(x):
    return 1.0 if x > 0 else -1.0 if x < 0 else 0.0


def front(y, depth=0.045):
    """1 on the face surface (y <= 0.01), 0 deeper than `depth`."""
    return _smooth(depth, 0.01, y)


def near(x, y, z):
    """1 in and around the head, 0 a little outside it (keeps every field off the body,
    the hat and a rider's horse)."""
    r = math.sqrt((x / 0.11) ** 2 + ((y - 0.09) / 0.14) ** 2 + ((z + 0.02) / 0.14) ** 2)
    return _smooth(1.25, 1.0, r)


def _local(field):
    def wrapped(*a):
        *pre, x, y, z = a
        k = near(x, y, z)
        if k == 0.0:
            return (0.0, 0.0, 0.0)
        d = field(*pre, x, y, z)
        return tuple(c * k for c in d)

    wrapped.__doc__ = field.__doc__
    wrapped.__name__ = field.__name__
    return wrapped


@_local
def base_face(x, y, z):
    """Displacement (dx, dy, dz) that turns the egg of the skin hull into a face: nose,
    eye sockets, brow ridge, cheekbones, lips, chin and ears."""
    ax, s = abs(x), _sign(x)
    f = front(y)
    dx = dy = dz = 0.0
    # Nose: a ridge from the bridge (z 0.005) to the tip (z -0.032), narrow, then the base.
    prof = _smooth(0.012, 0.0, z) * (0.006 + 0.013 * _smooth(0.0, -0.032, z)) * _smooth(-0.050, -0.036, z)
    dy -= prof * math.exp(-(x / 0.010) ** 2) * f
    wings = _g((ax, 0.012, 0.007), (z, -0.038, 0.006)) * f
    dy -= 0.005 * wings
    dx += s * 0.002 * wings
    # Eye sockets in, brow ridge out, cheekbones out and forward.
    dy += 0.0075 * _g((ax, 0.033, 0.015), (z, 0.002, 0.010)) * f
    dy -= 0.0055 * _g((ax, 0.030, 0.026), (z, 0.020, 0.008)) * f
    cb = _g((ax, 0.050, 0.016), (z, -0.024, 0.012)) * front(y, 0.07)
    dy -= 0.004 * cb
    dx += s * 0.003 * cb
    # Hollow under the cheekbone, lips, the groove under the lip, chin.
    dy += 0.003 * _g((ax, 0.040, 0.014), (z, -0.050, 0.012)) * f
    dy -= 0.0035 * _g((x, 0.0, 0.020), (z, -0.060, 0.007)) * f
    dy += 0.002 * _g((x, 0.0, 0.016), (z, -0.072, 0.005)) * f
    dy -= 0.006 * _g((x, 0.0, 0.018), (z, -0.088, 0.012)) * f
    # Ears on the sides, behind the cheeks.
    dx += s * 0.011 * _g((y, 0.100, 0.016), (z, -0.012, 0.022)) * _smooth(0.045, 0.065, ax)
    return dx, dy, dz


@_local
def morph(name, x, y, z):
    """Displacement (dx, dy, dz) of the morph target `name` at weight 1."""
    ax, s = abs(x), _sign(x)
    f = front(y)
    fw = front(y, 0.12)
    if name == "face_jaw":
        w = _g((z, -0.072, 0.024)) * _smooth(0.015, 0.055, ax) * fw
        return s * 0.010 * w, 0.0, -0.002 * w
    if name == "face_chin":
        w = _g((x, 0.0, 0.022), (z, -0.090, 0.016)) * f
        return 0.0, -0.008 * w, -0.006 * w
    if name == "face_cheekbones":
        w = _g((ax, 0.050, 0.016), (z, -0.022, 0.012)) * front(y, 0.07)
        return s * 0.006 * w, -0.004 * w, 0.001 * w
    if name == "face_nose":
        w = math.exp(-(x / 0.012) ** 2) * _g((z, -0.032, 0.012)) * f
        return 0.0, -0.007 * w, -0.006 * w
    if name == "face_nose_bridge":
        w = _g((x, 0.0, 0.009), (z, -0.012, 0.010)) * f
        return 0.0, -0.006 * w, 0.0
    if name == "face_brow":
        w = _g((ax, 0.030, 0.028), (z, 0.020, 0.009)) * f
        return 0.0, -0.006 * w, -0.002 * w
    if name == "face_eyes":
        w = _g((ax, 0.034, 0.018), (z, 0.0, 0.016)) * f
        return s * 0.005 * w, 0.0, 0.0
    if name == "face_age":
        jowl = _g((ax, 0.045, 0.020), (z, -0.075, 0.018)) * fw
        hollow = _g((ax, 0.044, 0.015), (z, -0.045, 0.012)) * f
        bags = _g((ax, 0.033, 0.014), (z, -0.012, 0.007)) * f
        tip = math.exp(-(x / 0.012) ** 2) * _g((z, -0.034, 0.010)) * f
        temple = _g((ax, 0.068, 0.012), (y, 0.04, 0.025), (z, 0.030, 0.018))
        dx = s * (0.003 * jowl - 0.003 * temple)
        dy = 0.004 * hollow - 0.0015 * bags
        dz = -0.008 * jowl - 0.0025 * bags - 0.003 * tip - 0.002 * hollow
        return dx, dy, dz
    if name == "face_fat":
        cheeks = _g((z, -0.060, 0.030)) * _smooth(0.0, 0.035, ax) * fw * _smooth(0.0, -0.03, z)
        chin2 = _g((x, 0.0, 0.035), (z, -0.105, 0.020), (y, 0.04, 0.035))
        return s * 0.012 * cheeks, -0.004 * cheeks - 0.008 * chin2, -0.010 * chin2
    if name == "face_thin":
        cheek = _g((ax, 0.046, 0.020), (z, -0.050, 0.020)) * fw
        temple = _g((ax, 0.068, 0.012), (y, 0.04, 0.025), (z, 0.030, 0.018))
        return -s * (0.007 * cheek + 0.003 * temple), 0.004 * cheek, 0.0
    raise KeyError(name)


def zones(x, y, z):
    """The `_FACE` attribute (vec4): R beard (where stubble and beard grow), G wrinkles
    (forehead, crow's feet, nasolabial folds), B pock marks and scars (cheeks, forehead,
    nose), A the face front (1) for everything the renderer paints per soldier."""
    ax = abs(x)
    f = front(y)
    fw = front(y, 0.12)
    lips = _g((x, 0.0, 0.022), (z, -0.060, 0.008))
    beard = _smooth(-0.020, -0.040, z) * fw * (1.0 - lips) * _smooth(-0.15, -0.11, z)
    beard = max(beard, _g((x, 0.0, 0.03), (z, -0.049, 0.006)) * f * (1.0 - lips))
    wrink = max(_g((ax, 0.0, 0.05), (z, 0.050, 0.014)) * f,
                _g((ax, 0.058, 0.010), (z, 0.002, 0.010)) * fw,
                _g((ax, 0.026, 0.008), (z, -0.050, 0.014)) * f)
    pock = max(_g((ax, 0.040, 0.026), (z, -0.030, 0.026)) * front(y, 0.07),
               _g((ax, 0.0, 0.05), (z, 0.045, 0.02)) * f)
    k = near(x, y, z)
    return (min(beard, 1.0) * k, min(wrink, 1.0) * k, min(pock, 1.0) * k, f * k)


def region(variant, x, y, z):
    """Whether a skin face at head-local (x, y, z) belongs to the hair/beard `variant`."""
    ax = abs(x)
    if variant in ("hair_short", "hair_tied", "hair_loose"):
        # Hairline: high on the forehead, over the temples, round behind the ears to the
        # nape; the face and the ears stay bare.
        if y < 0.03:
            scalp = z > 0.062
        elif y < 0.085:
            scalp = z > 0.030 - 0.25 * (y - 0.03)
        else:
            scalp = z > -0.035 - 0.6 * (y - 0.085)
        ear = ax > 0.055 and 0.080 < y < 0.125 and -0.04 < z < 0.02
        scalp = scalp and not ear
        if variant == "hair_short":
            return scalp and z > -0.075
        if variant == "hair_tied":
            return scalp and z > -0.09
        return scalp or (y > 0.12 and z > -0.22)  # loose: down the back of the neck
    if variant == "beard_full":
        lips = abs(x) < 0.022 and -0.068 < z < -0.052
        return z < -0.022 and y < 0.115 and z > -0.13 and not lips
    if variant == "beard_moustache":
        return ax < 0.032 and -0.057 < z < -0.040 and y < 0.035
    if variant == "beard_sideburns":
        return ax > 0.05 and 0.045 < y < 0.105 and -0.065 < z < 0.015
    raise KeyError(variant)


def queue_path(top_y):
    """Centre line (y, z, radius) of the queue down the nape, from the back of the head
    (`top_y` = the head's depth at the tie)."""
    return [(top_y + 0.004, -0.045, 0.014), (top_y + 0.016, -0.085, 0.012), (top_y + 0.020, -0.125, 0.010),
            (top_y + 0.018, -0.165, 0.008), (top_y + 0.014, -0.200, 0.004)]


# --------------------------------------------------------------------------- choice

def _mix32(x):
    """A 32-bit integer hash (lowbias32); the renderer ports it to pick the same look."""
    x &= 0xFFFFFFFF
    x ^= x >> 16
    x = (x * 0x7FEB352D) & 0xFFFFFFFF
    x ^= x >> 15
    x = (x * 0x846CA68B) & 0xFFFFFFFF
    x ^= x >> 16
    return x


class Rng:
    """Deterministic stream from a seed (the unit id): `u()` in [0, 1)."""

    def __init__(self, seed):
        self.state = _mix32(seed ^ 0x9E3779B9)

    def u(self):
        self.state = _mix32(self.state + 0x6D2B79F5)
        return self.state / 4294967296.0

    def tri(self):
        """Triangular in (-1, 1), peaked at 0."""
        return self.u() - self.u()

    def pick(self, odds):
        r = self.u() * sum(odds.values())
        for k, w in odds.items():
            r -= w
            if r < 0:
                return k
        return k

    def normal(self):
        return sum(self.u() for _ in range(6)) - 3.0  # ~N(0, sqrt(0.5))


def choose(seed, style="regular", nation="english"):
    """One soldier's look: morph weights, hair, beard, colours, stubble, marks, height."""
    r = Rng(seed)
    st = STYLES[style]
    age = r.u() ** 1.6  # most recruits young
    w = {}
    for name, (lo, _hi) in FACE_MORPHS.items():
        if lo < 0:
            w[name] = round(max(-1.0, min(1.0, 1.3 * r.tri())), 3)
    w["face_brow"] = round(r.u() ** 1.5, 3)
    w["face_age"] = round(age, 3)
    fat = r.tri() * 1.2
    w["face_fat"] = round(min(1.0, max(0.0, fat)), 3)
    w["face_thin"] = round(min(1.0, max(0.0, -fat)), 3)
    build = r.tri() * 1.3
    w["body_stocky"] = round(min(1.0, max(0.0, build)), 3)
    w["body_thin"] = round(min(1.0, max(0.0, -build)), 3)
    hair = r.pick(st["hair"])
    beard = r.pick(st["beard"])
    colour = r.pick(HAIR_ODDS)
    if r.u() < max(0.0, age - 0.45) * 1.6:
        colour = r.pick({g: 1.0 for g in GREYS})
    lo, hi = st["stubble"]
    stubble = round(lo + (hi - lo) * r.u(), 3)
    tones = SKIN_TONES[nation]
    tone = r.pick({k: 1.0 for k in tones})
    height = HEIGHT_MEAN_M + HEIGHT_SD_M * r.normal() * math.sqrt(2.0)
    height = round(min(HEIGHT_M[1], max(HEIGHT_M[0], height)), 3)
    return {
        "morphs": w, "hair": hair, "beard": beard, "hair_colour": colour, "skin": tone,
        "stubble": stubble, "pocks": round(0.3 + 0.7 * r.u(), 3) if r.u() < 0.2 else 0.0,
        "scar": r.u() < 0.15, "height_m": height,
    }


def nodes(look):
    """The hair and beard node suffixes `look` shows."""
    return HAIR_VARIANTS[look["hair"]] + BEARD_VARIANTS[look["beard"]]


def extras(style="regular", nation="english"):
    """Root extras for the renderer (docs/assets-humans.md "Faces and bodies")."""
    return {
        "acr_face_morphs": {k: list(v) for k, v in {**FACE_MORPHS, **BODY_MORPHS}.items()},
        "acr_hair_variants": {k: list(v) for k, v in HAIR_VARIANTS.items()},
        "acr_beard_variants": {k: list(v) for k, v in BEARD_VARIANTS.items()},
        "acr_look_odds": {"hair": dict(STYLES[style]["hair"]), "beard": dict(STYLES[style]["beard"]),
                          "stubble": list(STYLES[style]["stubble"])},
        "acr_hair_colours": {k: list(v) for k, v in HAIR_COLOURS.items()},
        "acr_skin_tones": {k: list(v) for k, v in SKIN_TONES[nation].items()},
        "acr_height_m": list(HEIGHT_M),
    }
