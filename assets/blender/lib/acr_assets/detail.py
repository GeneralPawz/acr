"""Procedural surface detail (needs `bpy`, `numpy`): small tileable textures that multiply a
flat colour with cloth, leather, wood or skin variation, and a box-projected UV map
`detail` for them. Units take one dominant colour per part from the sprites
(`project.dominant`) and get their close-up look from these tiles, so a model holds up when
zoomed and turned (free camera) and still matches the sprite at game zoom.
"""

import math

import bpy
import numpy as np

SIZE = 64
# Texture tiles per metre of surface (a 64-pixel tile covers 1 / TILES m).
TILES = 4.0


def _value_noise(rng, size, cells):
    g = rng.random((cells, cells)).astype(np.float32)
    x = np.arange(size) * cells / size
    i0 = np.floor(x).astype(int) % cells
    i1 = (i0 + 1) % cells
    t = x - np.floor(x)
    t = t * t * (3 - 2 * t)
    rows = g[i0][:, i0] * (1 - t)[None, :] + g[i0][:, i1] * t[None, :]
    rows2 = g[i1][:, i0] * (1 - t)[None, :] + g[i1][:, i1] * t[None, :]
    return rows * (1 - t)[:, None] + rows2 * t[:, None]


def pattern(kind, seed=1):
    """A SIZE x SIZE brightness pattern around 1.0 for a material kind (layer name of
    `decay.LAYERS`, or `plain`)."""
    rng = np.random.default_rng(seed)
    n1 = _value_noise(rng, SIZE, 4)
    n2 = _value_noise(rng, SIZE, 16)
    fine = rng.random((SIZE, SIZE)).astype(np.float32)
    y, x = np.mgrid[0:SIZE, 0:SIZE]
    if kind == "wool":
        weave = 0.5 + 0.5 * np.sin(x * math.pi / 2) * np.sin(y * math.pi / 2)
        p = 0.90 + 0.06 * n1 + 0.04 * n2 + 0.03 * weave + 0.02 * fine
    elif kind == "linen":
        weave = (np.sin(x * math.pi / 1.5) + np.sin(y * math.pi / 1.5)) * 0.5
        p = 0.94 + 0.03 * n1 + 0.02 * weave + 0.02 * fine
    elif kind == "leather":
        p = 0.86 + 0.12 * n1 + 0.05 * n2 + 0.02 * fine
    elif kind == "wood":
        grain = 0.5 + 0.5 * np.sin(x * 2 * math.pi * 6 / SIZE + 3 * n1)
        p = 0.82 + 0.12 * grain + 0.05 * n2
    elif kind == "metal":
        p = 0.92 + 0.08 * n2
    elif kind in ("flesh", "hair"):
        p = 0.95 + 0.04 * n1 + 0.02 * n2
    else:
        p = 0.94 + 0.04 * n1 + 0.02 * n2
    return p / p.mean()


def tile(name, rgb, kind, seed=1):
    """A packed RGB image: colour `rgb` (sRGB) times the pattern of `kind`, computed in
    linear light."""
    lin = np.power(np.array(rgb, np.float32), 2.2)
    p = pattern(kind, seed)
    px = np.ones((SIZE, SIZE, 4), np.float32)
    px[..., :3] = np.power(np.clip(lin[None, None, :] * p[..., None], 0, 1), 1 / 2.2)
    img = bpy.data.images.new(name, SIZE, SIZE, alpha=False)
    img.pixels.foreach_set(px.ravel())
    img.file_format = "PNG"
    img.pack()
    return img


def box_uv(obj, name="detail", scale=TILES, from_mesh=None):
    """UV map `name` by box projection of the rest positions: each face uses the two axes
    across its normal's dominant axis, `scale` tiles per metre."""
    me = obj.data
    uv = me.uv_layers.get(name) or me.uv_layers.new(name=name)
    for poly in me.polygons:
        n = poly.normal
        ax = max(range(3), key=lambda i: abs(n[i]))
        a, b = [(1, 2), (0, 2), (0, 1)][ax]
        for li in poly.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            uv.data[li].uv = (co[a] * scale, co[b] * scale)
    return uv
