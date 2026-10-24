"""In-painting blank texels of a generated base colour (pure Python + numpy; no `bpy`, unit-tested).

Pixal3D bakes the faces it found no colour for as plain white (UsaKreBase: 10 % of the
texels and 16 % of the surface at min channel >= 250, SpnKreBase 15 %, UsaKreUp 4 %), which
the game draws as glaring white patches. A real light colour ramps up to white (SiuMel,
IroVer: well under 0.1 % at >= 250); the blank ones sit on a plateau at pure white.

`blank(rgb)` marks those texels (every channel >= `LEVEL`, grown by `GROW` texels into
near-white neighbours: the lossy WebP source rings the patch edges); `fill(rgb, hole)`
replaces them from the colours around them by push-pull (a mean pyramid of the valid texels,
holes taken from the next coarser level), then smooths inside the holes so no block edges
show. Values are the image's stored 0..1 floats (`bpy` pixels), rows in any order.
"""

import numpy as np

LEVEL = 250 / 255  # every channel at least this: a blank texel
EDGE = 0.8         # neighbours of a blank texel at least this bright join it (WebP ringing)
GROW = 1           # texels the blank mask grows into such neighbours
SMOOTH = 8         # relaxation passes inside the holes after push-pull


def _grow(mask):
    out = mask.copy()
    out[1:] |= mask[:-1]
    out[:-1] |= mask[1:]
    out[:, 1:] |= mask[:, :-1]
    out[:, :-1] |= mask[:, 1:]
    return out


def blank(rgb):
    """Bool (H, W): the texels Pixal3D left white (see the module doc)."""
    lo = rgb[..., :3].min(axis=-1)
    hole = lo >= LEVEL
    near = lo >= EDGE
    for _ in range(GROW):
        hole = _grow(hole) & near
    return hole


def _down(c, w):
    h, wd = w.shape
    ph, pw = h % 2, wd % 2
    if ph or pw:
        c = np.pad(c, ((0, ph), (0, pw), (0, 0)))
        w = np.pad(w, ((0, ph), (0, pw)))
    c = c.reshape(c.shape[0] // 2, 2, c.shape[1] // 2, 2, c.shape[2]).sum(axis=(1, 3))
    w = w.reshape(w.shape[0] // 2, 2, w.shape[1] // 2, 2).sum(axis=(1, 3))
    return c, w


def fill(rgb, hole):
    """`rgb` (H, W, C float) with the `hole` texels replaced from their surroundings.
    Without any valid texel the input comes back unchanged."""
    rgb = np.asarray(rgb, np.float64)
    if not hole.any() or hole.all():
        return rgb.copy()
    w = (~hole).astype(np.float64)
    levels = [(rgb * w[..., None], w)]
    while max(levels[-1][1].shape) > 1 and (levels[-1][1] == 0).any():
        levels.append(_down(*levels[-1]))
    c, w = levels[-1]
    filled = c / np.maximum(w, 1e-12)[..., None]
    for c, w in reversed(levels[:-1]):
        up = np.repeat(np.repeat(filled, 2, axis=0), 2, axis=1)[: w.shape[0], : w.shape[1]]
        filled = np.where((w > 0)[..., None], c / np.maximum(w, 1e-12)[..., None], up)
    out = rgb.copy()
    out[hole] = filled[hole]
    for _ in range(SMOOTH):
        p = np.pad(out, ((1, 1), (1, 1), (0, 0)), mode="edge")
        avg = (p[:-2, 1:-1] + p[2:, 1:-1] + p[1:-1, :-2] + p[1:-1, 2:]) / 4
        out[hole] = avg[hole]
    return out
