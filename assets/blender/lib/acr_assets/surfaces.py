"""Realistic building surfaces (needs `bpy`, `numpy`): tileable colour textures for log
walls, bark, shingles, hewn timber, stone, rubble and trodden earth, with their shading
(log curvature, gaps, fissures) baked in so they read under the game's simple Lambert
light, and UVs aligned to each face (rows along a roof's eaves, bark up a stake, logs along
a wall).

Buildings take these instead of the sprite projection: the look matters more than matching
the sprite pixel for pixel, the materials hold up from every side in the free camera, and a
few shared 256-pixel tiles keep the files small (`docs/assets.md`).
"""

import math

import bpy
import numpy as np

from . import materials

SIZE = 256


def _wrap_noise(rng, size, cells, stretch=(1, 1)):
    """Tileable value noise; `stretch` = cells across (u, v) multipliers (streaks)."""
    cu, cv = max(1, int(cells * stretch[0])), max(1, int(cells * stretch[1]))
    g = rng.random((cv, cu)).astype(np.float32)
    u = np.arange(size) * cu / size
    v = np.arange(size) * cv / size
    iu0 = np.floor(u).astype(int) % cu
    iv0 = np.floor(v).astype(int) % cv
    iu1, iv1 = (iu0 + 1) % cu, (iv0 + 1) % cv
    tu, tv = u - np.floor(u), v - np.floor(v)
    tu, tv = tu * tu * (3 - 2 * tu), tv * tv * (3 - 2 * tv)
    a = g[iv0][:, iu0] * (1 - tu) + g[iv0][:, iu1] * tu
    b = g[iv1][:, iu0] * (1 - tu) + g[iv1][:, iu1] * tu
    return a * (1 - tv)[:, None] + b * tv[:, None]


def _fbm(rng, size, cells, octaves=4, stretch=(1, 1)):
    out = np.zeros((size, size), np.float32)
    amp, total = 1.0, 0.0
    for o in range(octaves):
        out += amp * _wrap_noise(rng, size, cells * 2 ** o, stretch)
        total += amp
        amp *= 0.5
    return out / total


def _cells(rng, size, count):
    """Tileable Voronoi: distance to the nearest and second nearest site (pixels) and the
    nearest site's index."""
    pts = rng.random((count, 2)) * size
    y, x = np.mgrid[0:size, 0:size].astype(np.float32)
    d1 = np.full((size, size), 1e9, np.float32)
    d2 = np.full((size, size), 1e9, np.float32)
    idx = np.zeros((size, size), np.int32)
    for i, (px, py) in enumerate(pts):
        dx = np.abs(x - px)
        dy = np.abs(y - py)
        dx = np.minimum(dx, size - dx)
        dy = np.minimum(dy, size - dy)
        d = np.sqrt(dx * dx + dy * dy)
        closer = d < d1
        d2 = np.where(closer, d1, np.minimum(d2, d))
        idx = np.where(closer, i, idx)
        d1 = np.where(closer, d, d1)
    return d1, d2, idx


def _lin(rgb):
    return np.array([materials.srgb_to_linear(c) for c in rgb], np.float32)


def _mix(a, b, t):
    return a * (1 - t[..., None]) + b * t[..., None]


def logwall(seed=1, logs=6):
    """Horizontal logs (rows along u) with rounded shading, grain and dark chinking."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)
    row = y * logs / SIZE
    k = np.floor(row).astype(int)
    f = row - k
    per_log = rng.random(logs + 1).astype(np.float32)[k % logs]
    grain = _fbm(rng, SIZE, 2, 3, stretch=(1, 8))
    streak = _wrap_noise(rng, SIZE, 3, stretch=(1, 24))
    wood = _mix(_lin((0.52, 0.42, 0.30)), _lin((0.34, 0.26, 0.18)), np.clip(0.35 * per_log + 0.45 * grain + 0.25 * streak, 0, 1))
    # Weathered grey on the upper half of each log.
    wood = _mix(wood, _lin((0.52, 0.50, 0.46)), np.clip((0.5 - f) * 0.9 * (0.4 + 0.6 * per_log), 0, 0.45))
    shade = 0.55 + 0.45 * np.sin(np.clip(f, 0.06, 0.94) * math.pi) ** 0.6
    rgb = wood * shade[..., None]
    gap = (f < 0.07) | (f > 0.95)
    chink = _lin((0.20, 0.18, 0.15)) * (0.8 + 0.3 * _wrap_noise(rng, SIZE, 16)[..., None])
    return np.where(gap[..., None], chink, rgb)


def bark(seed=2):
    """Bark of round stakes and logs: fissures running along v."""
    rng = np.random.default_rng(seed)
    ridges = _fbm(rng, SIZE, 12, 3, stretch=(1, 0.08))
    fiss = np.clip((ridges - 0.40) * 2.5, 0, 1)
    base = _mix(_lin((0.20, 0.15, 0.10)), _lin((0.46, 0.38, 0.29)), fiss)
    lichen = np.clip((_fbm(rng, SIZE, 5, 3) - 0.62) * 6, 0, 1)
    base = _mix(base, _lin((0.50, 0.52, 0.42)), lichen * 0.6)
    return base * (0.75 + 0.25 * fiss)[..., None]


def shingles(seed=3, rows=8, tint=(0.50, 0.41, 0.31)):
    """Wooden shingles in rows along u (the eaves), staggered, each with its own tone, the
    lower edge of a row casting a shadow, moss in patches."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)
    row = y * rows / SIZE
    k = np.floor(row).astype(int)
    f = row - k
    width = SIZE / 10
    offset = (k % 2) * width / 2 + (rng.random(rows + 1)[k % rows] * width * 0.3)
    col = (x + offset) / width
    c = np.floor(col).astype(int)
    fc = col - c
    tone = rng.random((rows, 16)).astype(np.float32)[k % rows, c % 16]
    grain = _wrap_noise(rng, SIZE, 4, stretch=(0.5, 12))
    rgb = _lin(tint) * (0.72 + 0.35 * tone + 0.12 * grain)[..., None]
    rgb = _mix(rgb, _lin((0.62, 0.60, 0.55)), np.clip(0.25 * tone, 0, 0.25))
    edge = np.clip(f / 0.18, 0, 1) ** 0.7
    split = np.clip(np.minimum(fc, 1 - fc) / 0.04, 0, 1)
    rgb = rgb * (0.22 + 0.78 * edge * (0.35 + 0.65 * split))[..., None]
    moss = np.clip((_fbm(rng, SIZE, 3, 4) - 0.66) * 5, 0, 1)
    return _mix(rgb, _lin((0.30, 0.36, 0.18)) * (0.6 + 0.4 * edge[..., None]), moss * 0.35)


def timber(seed=4):
    """Hewn beams and boards: grain along u, a few cracks."""
    rng = np.random.default_rng(seed)
    grain = _fbm(rng, SIZE, 3, 4, stretch=(0.3, 6))
    rgb = _mix(_lin((0.36, 0.28, 0.19)), _lin((0.55, 0.45, 0.33)), grain)
    cracks = np.clip((np.abs(_wrap_noise(rng, SIZE, 6, stretch=(0.2, 3)) - 0.5) < 0.012) * 1.0, 0, 1)
    rgb = rgb * (1 - 0.6 * cracks)[..., None]
    weather = np.clip(_fbm(rng, SIZE, 2, 3) - 0.3, 0, 1)
    return _mix(rgb, _lin((0.50, 0.48, 0.44)), weather * 0.5)


def stone(seed=5, count=26, tint=(0.56, 0.54, 0.50)):
    """Fieldstone masonry: irregular stones with dark mortar joints."""
    rng = np.random.default_rng(seed)
    d1, d2, idx = _cells(rng, SIZE, count)
    tone = rng.random(count).astype(np.float32)[idx]
    rgb = _lin(tint) * (0.70 + 0.40 * tone + 0.15 * _fbm(rng, SIZE, 8, 3))[..., None]
    joint = np.clip((d2 - d1) / 5.0, 0, 1)
    bevel = np.clip((d2 - d1) / 18.0, 0, 1)
    rgb = rgb * (0.55 + 0.45 * bevel)[..., None]
    return _mix(_lin((0.18, 0.17, 0.15)), rgb, joint)


def earth(seed=6):
    """Trodden earth with pebbles and a little grass at random."""
    rng = np.random.default_rng(seed)
    n = _fbm(rng, SIZE, 4, 5)
    rgb = _mix(_lin((0.27, 0.21, 0.15)), _lin((0.45, 0.37, 0.27)), n)
    d1, d2, idx = _cells(rng, SIZE, 90)
    pebble = (d1 < 3.2) & (rng.random(90)[idx] < 0.5)
    rgb = np.where(pebble[..., None], _lin((0.55, 0.52, 0.47)) * (0.8 + 0.4 * n[..., None]), rgb)
    grass = np.clip((_fbm(rng, SIZE, 6, 3) - 0.62) * 5, 0, 1)
    return _mix(rgb, _lin((0.28, 0.33, 0.16)), grass * 0.6)


# kind → (generator, metres per tile, roughness)
KINDS = {
    "logwall": (logwall, 1.6, 0.9),
    "bark": (bark, 0.35, 0.95),
    "shingles": (shingles, 1.4, 0.85),
    "slate": (lambda: shingles(seed=7, rows=10, tint=(0.40, 0.40, 0.42)), 1.2, 0.7),
    "timber": (timber, 1.0, 0.85),
    "stone": (stone, 1.8, 0.9),
    "rubble": (lambda: stone(seed=8, count=14, tint=(0.50, 0.47, 0.42)), 0.8, 0.95),
    "earth": (earth, 4.0, 1.0),
}

_cache = {}


def material(kind):
    """The shared material of a surface kind (texture generated once per build)."""
    if kind in _cache:
        return _cache[kind]
    gen, _tile, rough = KINDS[kind]
    rgb = gen()
    px = np.ones((SIZE, SIZE, 4), np.float32)
    # Blender images are linear float; the glTF exporter writes them as sRGB PNG.
    px[..., :3] = np.clip(rgb, 0, 1)
    img = bpy.data.images.new(f"surface_{kind}", SIZE, SIZE, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "sRGB"
    img.pixels.foreach_set(np.power(px, [1 / 2.2, 1 / 2.2, 1 / 2.2, 1.0]).astype(np.float32).ravel())
    img.file_format = "PNG"
    img.pack()
    mat = materials.textured(f"surface_{kind}", img, "surface", roughness=rough, closest=False)
    _cache[kind] = mat
    return mat


def aligned_uv(obj, metres_per_tile, name="surface"):
    """UV map `name` aligned to each face: u along the face's horizontal direction (a roof's
    eaves, a wall's length), v up its slope; horizontal faces use the ground axes. Scaled to
    `metres_per_tile`."""
    me = obj.data
    uv = me.uv_layers.get(name) or me.uv_layers.new(name=name)
    k = 1.0 / metres_per_tile
    for poly in me.polygons:
        n = poly.normal
        if abs(n.z) > 0.97:
            t = (1.0, 0.0, 0.0)
            b = (0.0, 1.0, 0.0)
        else:
            # Horizontal tangent along the face, bitangent up the face.
            tl = math.hypot(n.x, n.y)
            t = (-n.y / tl, n.x / tl, 0.0)
            b = (n.y * t[2] - n.z * t[1], n.z * t[0] - n.x * t[2], n.x * t[1] - n.y * t[0])
            if b[2] < 0:
                b = (-b[0], -b[1], -b[2])
        for li in poly.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            uv.data[li].uv = ((co.x * t[0] + co.y * t[1] + co.z * t[2]) * k,
                              (co.x * b[0] + co.y * b[1] + co.z * b[2]) * k)
    me.uv_layers.active = uv
    return uv


def kind_of(name, rules):
    """The surface kind for an object name: the first (substring, kind) rule that matches."""
    low = name.lower()
    for key, kind in rules:
        if key in low:
            return kind
    return None


def apply(objs, rules, default="timber"):
    """Gives every mesh in `objs` the surface its name selects (`rules`: (substring, kind)
    pairs, first match wins) with aligned UVs; returns {object name: kind}."""
    out = {}
    for o in objs:
        if o.type != "MESH":
            continue
        kind = kind_of(o.name, rules) or default
        aligned_uv(o, KINDS[kind][1])
        materials.assign(o, material(kind))
        out[o.name] = kind
    return out
