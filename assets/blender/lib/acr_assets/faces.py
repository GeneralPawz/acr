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
import os

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
    "full": ["beard_full", "beard_moustache"], "moustache_sideburns": ["beard_moustache", "beard_sideburns"],
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
# Hair and beard patches (#283 pass 2): smooth meshes over the skin, built on rays from the
# head centre (`ray_dir`) in (azimuth θ, elevation φ) degrees; θ = 0 looks forward (-Y).
# Each patch: (θ range |θ| <= max or a band, the lower and upper elevation curves as
# [(|θ|, φ)] keys, thickness in metres, open edges that taper into the skin, grid).
HAIRLINE = [(0, 47), (25, 45), (50, 37), (72, 26), (92, 22), (108, 6), (128, -24), (180, -40)]
PATCHES = {
    # hair: from the hairline (s = 0, tapered) to the crown (s = 1, closed with a fan)
    "hair_short": {"theta": (-180, 180), "lo": [(t, p + 2) for t, p in HAIRLINE], "hi": [(0, 89), (180, 89)],
                   "thick": 0.0035, "open": ("lo",), "grid": (32, 6), "ridges": 0.15, "ragged": 3.0},
    "hair_tied": {"theta": (-180, 180), "lo": HAIRLINE, "hi": [(0, 89), (180, 89)],
                  "thick": 0.0050, "open": ("lo",), "grid": (32, 6), "ridges": 0.35, "ragged": 5.0},
    "hair_loose": {"theta": (-180, 180), "lo": [(0, 47), (25, 45), (50, 37), (72, 26), (92, 12), (110, -30), (135, -52), (180, -60)],
                   "hi": [(0, 89), (180, 89)], "thick": 0.0075, "open": ("lo",), "grid": (32, 7), "ridges": 0.4,
                   "ragged": 9.0},
    # beards: open on every side
    "beard_full": {"theta": (-98, 98), "lo": [(0, -80), (40, -70), (70, -48), (98, -26)],
                   "hi": [(0, -31), (18, -28), (30, -17), (45, -9), (65, -3), (85, 6), (98, 9)],
                   "thick": 0.011, "open": ("lo", "hi", "ends"), "grid": (18, 5), "ridges": 0.35, "ragged": 7.0},
    # moustache: a band over the upper lip whose lower edge hangs in drooping locks
    # (on the MakeHuman head the upper lip lies 21..30 degrees below the centre's level)
    "beard_moustache": {"theta": (-40, 40), "lo": [(0, -29), (22, -31), (40, -39)], "hi": [(0, -21), (22, -22), (40, -28)],
                        "thick": 0.014, "open": ("lo", "hi", "ends"), "grid": (12, 2), "ridges": 0.3, "ragged": 5.0},
    # sideburns: in front of the ear only (rays past about 80 degrees hit the ear and stood
    # the patch off the cheek as a flat card); fine enough to follow the cheek, ragged below
    "beard_sideburns": {"theta": (56, 80), "mirror": True, "lo": [(56, -12), (68, -20), (80, -24)],
                        "hi": [(56, 8), (80, 12)], "thick": 0.0035, "open": ("lo", "hi", "ends"), "grid": (5, 5),
                        "ridges": 0.2, "ragged": 3.0},
}
CENTRE = (0.0, 0.09, -0.01)  # ray origin, head-local
# The head's own texture (`heads.face_map`): the procedural map's size, and the largest a
# baked face from `face_source` may bring (power-of-two sizes in between are kept).
FACE_SIZE = 1024
FACE_SIZE_MAX = 2048
# Baked faces live outside the repo: `$ACR_FACES/<unit>.png`, else
# `$ACR_SOURCES/faces/<unit>.png`, an sRGB image in the head layout (`heads.unwrap_head`).
FACE_DIR = "faces"


def face_source(name, environ=None, exists=os.path.isfile):
    """Path of the baked face image of unit `name` (lower-case file stem), or None when
    there is none: the head then gets the painted procedural face."""
    env = os.environ if environ is None else environ
    folder = env.get("ACR_FACES")
    if not folder and env.get("ACR_SOURCES"):
        folder = os.path.join(env["ACR_SOURCES"], FACE_DIR)
    if not folder:
        return None
    path = os.path.join(folder, f"{name.lower()}.png")
    return path if exists(path) else None


# Frontal portraits (#306): a photo-like face generated over the head's own frontal guide
# (`heads.face_map` writes `<unit>.guide.png` into `$ACR_FACE_GUIDE`), projected back
# along -Y onto the face front. The frame both share: head-local x0, x1, z0, z1 (metres),
# square, the model's left (+x) on the image's right as in a photo of the face.
PORTRAIT_BOX = (-0.11, 0.11, -0.16, 0.06)
PORTRAIT_GUIDE_PX = 768
# The face area the portrait covers: an ellipse around (0, PORTRAIT_CENTRE_Z) with these
# half axes; it fades into the painted skin from PORTRAIT_FADE of the radius outwards.
PORTRAIT_CENTRE_Z = -0.035
PORTRAIT_AXES = (0.066, 0.098)
PORTRAIT_FADE = 0.72


# Several portraits per unit (#306): up to PORTRAIT_TILES go into one face texture as a
# grid of whole head tiles (`heads.face_map`, material `extras.acr_face_tiles`); the
# renderer picks one per soldier from his look seed (`acr-render` `looks::face_tile`).
PORTRAIT_TILES = 4
# The default sets per group of peoples, read from `$ACR_SOURCES` (local-only generated
# faces; a build without them paints the face), chosen by the unit's name prefix. Only the
# groups in PORTRAIT_ON are used by default (`ACR_PORTRAIT_GROUPS=eu,wd,...` overrides it);
# `ACR_PORTRAITS=0` switches every default off.
PORTRAIT_DIR = os.path.join("generated", "faces", "aligned")
PORTRAIT_GROUPS = {
    "eu": ("eng", "frn", "spn", "usa"),  # European
    "wd": ("del", "hur", "iro"),  # Eastern Woodlands
    "pl": ("siu",),  # Plains
    "sw": ("pue",),  # Southwest (Pueblo)
    "ms": ("atc", "may"),  # Mesoamerican
    "an": ("inc",),  # Andean
}
PORTRAIT_ON = ("eu", "wd", "pl", "sw", "ms", "an")
EUROPEAN = PORTRAIT_GROUPS["eu"]


def portrait_set(group, tiles=PORTRAIT_TILES):
    """File stems of group `group`'s default portraits: `<group>1_s1` .. `<group><tiles>_s1`."""
    return tuple(f"{group}{k}_s1" for k in range(1, tiles + 1))


PORTRAIT_SET = portrait_set("eu")


def portrait_group(name):
    """The group of peoples of unit `name` (by name prefix), or None."""
    n = name.lower()
    return next((g for g, pre in PORTRAIT_GROUPS.items() if n.startswith(pre)), None)


def portrait_sources(name, environ=None, exists=os.path.isfile, tiles=PORTRAIT_TILES):
    """Paths of unit `name`'s frontal portraits, at most `tiles`, or []: first
    `$ACR_FACES/<unit>.portrait<k>.png` (k = 1..) or `<unit>.portrait.png`, else, when the
    unit's group (`portrait_group`) is switched on, that group's default set under
    `$ACR_SOURCES`."""
    env = os.environ if environ is None else environ
    n = name.lower()
    folder = env.get("ACR_FACES")
    if folder:
        found = [p for p in (os.path.join(folder, f"{n}.portrait{k}.png") for k in range(1, tiles + 1)) if exists(p)]
        one = os.path.join(folder, f"{n}.portrait.png")
        if found or exists(one):
            return found or [one]
    on = env.get("ACR_PORTRAIT_GROUPS")
    on = PORTRAIT_ON if on is None else tuple(g.strip() for g in on.split(",") if g.strip())
    group = portrait_group(n)
    if env.get("ACR_PORTRAITS", "1") == "0" or group not in on or not env.get("ACR_SOURCES"):
        return []
    return [p for p in (os.path.join(env["ACR_SOURCES"], PORTRAIT_DIR, f"{k}.png") for k in portrait_set(group, tiles))
            if exists(p)]


# Matching a portrait's colour to the painted skin (`heads._blend_portrait`): one brightness
# gain within SKIN_GAIN, and per channel at most SKIN_TINT off it, so a photo whose rim is
# dark stubble or shadow is not blown up into a red face (eu2 reached 1.15, 1.71, 1.64).
SKIN_GAIN = (0.7, 1.4)
SKIN_TINT = 0.12


def _luma(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def skin_match(painted, photo):
    """Per-channel factors (r, g, b) that bring the mean colour `photo` towards `painted`
    (linear rgb triples): the brightness ratio clamped to SKIN_GAIN, each channel's own
    ratio at most SKIN_TINT off it."""
    g0 = _luma(painted) / max(_luma(photo), 1e-4)
    g = min(max(g0, SKIN_GAIN[0]), SKIN_GAIN[1])
    return tuple(g * min(max(p / max(q, 1e-4) / g0, 1.0 - SKIN_TINT), 1.0 + SKIN_TINT) for p, q in zip(painted, photo))


def tile_grid(n):
    """Tiles per side of the face texture holding `n` head tiles (1 for one)."""
    g = 1
    while g * g < n:
        g += 1
    return g


def portrait_px(x, z, w, h):
    """Image coordinates (column, row; row 0 at the top) of head-local (x, z) in a
    `w` x `h` portrait framed by PORTRAIT_BOX. Plain arithmetic: floats or numpy arrays."""
    x0, x1, z0, z1 = PORTRAIT_BOX
    return (x - x0) / (x1 - x0) * w, (z1 - z) / (z1 - z0) * h


def portrait_radius(x, z):
    """Elliptic radius of (x, z) in the portrait's face area: 1 on its rim."""
    ax, az = PORTRAIT_AXES
    return ((x / ax) ** 2 + ((z - PORTRAIT_CENTRE_Z) / az) ** 2) ** 0.5


def ear(x, y, z):
    """1 on the ears (the box the hairline leaves bare in `region`), else 0."""
    return 1.0 if abs(x) > 0.055 and 0.080 < y < 0.125 and -0.04 < z < 0.02 else 0.0


def face_size(src=None):
    """Texture size of the head: `FACE_SIZE`, or a baked face's own size (its larger side,
    rounded down to a power of two) between `FACE_SIZE` and `FACE_SIZE_MAX`."""
    if not src:
        return FACE_SIZE
    n = 1 << (max(int(src), 1).bit_length() - 1)
    return max(FACE_SIZE, min(FACE_SIZE_MAX, n))


# Variant meshes that are not patches.
OTHER_VARIANTS = ("hair_queue",)


def curve(keys, theta):
    """Elevation at |θ| from (|θ|, φ) keys, smoothstep between keys (θ in degrees)."""
    a = abs(theta)
    if a <= keys[0][0]:
        return keys[0][1]
    for (t0, p0), (t1, p1) in zip(keys, keys[1:]):
        if a <= t1:
            return p0 + (p1 - p0) * _smooth(t0, t1, a)
    return keys[-1][1]


def ray_dir(theta, phi):
    """Unit direction (head-local) of azimuth θ, elevation φ (degrees)."""
    t, p = math.radians(theta), math.radians(phi)
    return (math.sin(t) * math.cos(p), -math.cos(t) * math.cos(p), math.sin(p))


def patch_grid(name):
    """The patch's parameter grid: rows of [(θ, φ, taper)] from the lower to the upper
    curve; taper 0 on an open edge, 1 inside (the thickness fades into the skin)."""
    pt = PATCHES[name]
    nt, ns = pt["grid"]
    t0, t1 = pt["theta"]
    wrap = t1 - t0 >= 360
    cols = nt if wrap else nt + 1
    rows = []
    for j in range(ns + 1):
        sj = j / ns
        row = []
        for i in range(cols):
            ti = i / nt
            th = t0 + (t1 - t0) * ti
            lo, hi = curve(pt["lo"], th), curve(pt["hi"], th)
            d = []
            if "lo" in pt["open"]:
                d.append(sj)
            if "hi" in pt["open"]:
                d.append(1.0 - sj)
            if "ends" in pt["open"]:
                d.append(min(ti, 1.0 - ti) * (2.0 if not pt.get("mirror") else 1.0))
            taper = _smooth(0.0, 0.34, min(d)) if d else 1.0
            ph = lo + (hi - lo) * sj
            if j == 0 and pt.get("ragged"):
                # A ragged edge: every other column a lock reaching past the line.
                ph -= pt["ragged"] * lock(i) * (i % 2) * (1.0 if pt["open"][0] == "lo" else 0.0)
            row.append((th, ph, taper))
        rows.append(row)
    return rows


def lock(i):
    """Deterministic length 0.35..1 of the lock at edge column i."""
    f = math.sin(i * 12.9898 + 4.1) * 43758.5453
    return 0.35 + 0.65 * (f - math.floor(f))


def asym(x, y, z):
    """A small, fixed asymmetry of the base face (a nose knocked aside, one brow and one
    cheek higher): faces are never mirror images."""
    f = front(y)
    nose = math.exp(-(x / 0.012) ** 2) * _g((z, -0.025, 0.018)) * f
    brow = _g((x, 0.030, 0.022), (z, 0.020, 0.010)) * f
    cheek = _g((x, -0.048, 0.018), (z, -0.030, 0.016)) * f
    k = near(x, y, z)
    return (0.0025 * nose * k, 0.0, (0.0015 * brow + 0.0012 * cheek) * k)


def thickness(name, theta, s, taper):
    """Patch thickness at a grid point: tapered edges, combed ridges along the strands and
    a few clumps (deterministic)."""
    pt = PATCHES[name]
    ridge = 1.0 + pt["ridges"] * math.sin(math.radians(theta) * 23.0)
    clump = 1.0 + 0.18 * math.sin(theta * 0.31 + s * 5.0) * math.sin(theta * 0.13 + 1.7)
    # 1.2 mm keeps the chords of the grid over the curved scalp clear of the skin.
    return 0.0012 * min(1.0, taper * 4.0) + pt["thick"] * (0.15 + 0.85 * taper) * ridge * clump


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
    # Upper lids over the eyeballs, the nasolabial folds from the nose wings to the
    # mouth corners.
    dy -= 0.0028 * _g((ax, 0.033, 0.012), (z, 0.008, 0.0045)) * f
    t = min(max(((ax - 0.016) * 0.012 + (z + 0.040) * -0.025) / (0.012 ** 2 + 0.025 ** 2), 0.0), 1.0)
    fold = math.hypot(ax - (0.016 + 0.012 * t), z - (-0.040 - 0.025 * t))
    dy -= 0.0022 * math.exp(-(fold / 0.0045) ** 2) * f
    # A hard jaw line over the neck, the jaw angles, the Adam's apple.
    under = _g((y, 0.055, 0.035), (z, -0.112, 0.010)) * _smooth(0.075, 0.03, ax)
    dz += 0.004 * under
    dy += 0.003 * under
    angle = _g((ax, 0.058, 0.012), (y, 0.075, 0.020), (z, -0.085, 0.014))
    dx += s * 0.004 * angle
    dy -= 0.004 * _g((x, 0.0, 0.010), (y, 0.05, 0.03), (z, -0.150, 0.012))
    # Ears on the sides, behind the cheeks, with a hollow bowl.
    side = _smooth(0.045, 0.065, ax)
    dx += s * 0.011 * _g((y, 0.100, 0.016), (z, -0.012, 0.022)) * side
    dx -= s * 0.004 * _g((y, 0.097, 0.007), (z, -0.010, 0.010)) * side
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


def zones(x, y, z, beard=True):
    """The `_FACE` attribute (vec4): R beard (where stubble and beard grow; 0 everywhere for a
    clean-shaven face, `beard` False), G wrinkles (forehead, crow's feet, nasolabial folds),
    B pock marks and scars (cheeks, forehead, nose), A the face front (1) for everything the
    renderer paints per soldier."""
    ax = abs(x)
    f = front(y)
    fw = front(y, 0.12)
    lips = _g((x, 0.0, 0.022), (z, -0.060, 0.008))
    grow = beard
    beard = _smooth(-0.020, -0.040, z) * fw * (1.0 - lips) * _smooth(-0.15, -0.11, z)
    beard = max(beard, _g((x, 0.0, 0.03), (z, -0.049, 0.006)) * f * (1.0 - lips))
    wrink = max(_g((ax, 0.0, 0.05), (z, 0.050, 0.014)) * f,
                _g((ax, 0.058, 0.010), (z, 0.002, 0.010)) * fw,
                _g((ax, 0.026, 0.008), (z, -0.050, 0.014)) * f)
    pock = max(_g((ax, 0.040, 0.026), (z, -0.030, 0.026)) * front(y, 0.07),
               _g((ax, 0.0, 0.05), (z, 0.045, 0.02)) * f)
    k = near(x, y, z)
    return (min(beard, 1.0) * k if grow else 0.0, min(wrink, 1.0) * k, min(pock, 1.0) * k, f * k)


def hairline(scalp=True):
    """The hairline (`HAIRLINE`: (θ, φ) degrees) under which the face map paints the cropped
    scalp; without a `scalp` (shaved or plucked heads: a roach, a scalp lock) it is lifted
    over the crown (φ 120°), so no scalp is painted."""
    return list(HAIRLINE) if scalp else [(a, 120.0) for a, _ in HAIRLINE]


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
        ear_ = ear(x, y, z) > 0.0
        scalp = scalp and not ear_
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
