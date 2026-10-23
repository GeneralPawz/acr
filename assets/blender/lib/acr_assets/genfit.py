"""Fitting a generated model to its sprite (pure Python + numpy; no `bpy`, unit-tested).

A generated model (`genbuild`) arrives at an arbitrary scale and orientation. Its surface is
sampled into points once; for each candidate turn about +Z the points are projected through
the original camera (`conventions`: 30 px/m, elevation 30 degrees, `yaw(real_dir)`), and the
uniform scale and ground shift that put the projection onto the sprite's silhouette are found
in screen space (an orthographic camera turns a 3D scale into a 2D scale about the ground
point, and a shift on the ground into any 2D shift). The score is the IoU of the point
raster against the sprite's alpha mask, the same measure `acr model-check` reports.
"""

import math

import numpy as np

from . import conventions as cv


def rot(yaw_deg, pitch_deg=0.0, roll_deg=0.0):
    """Rotation matrix: roll about Y, then pitch about X, then yaw about Z (degrees)."""
    y, p, r = (math.radians(a) for a in (yaw_deg, pitch_deg, roll_deg))
    rz = np.array([[math.cos(y), -math.sin(y), 0], [math.sin(y), math.cos(y), 0], [0, 0, 1]])
    rx = np.array([[1, 0, 0], [0, math.cos(p), -math.sin(p)], [0, math.sin(p), math.cos(p)]])
    ry = np.array([[math.cos(r), 0, math.sin(r)], [0, 1, 0], [-math.sin(r), 0, math.cos(r)]])
    return rz @ rx @ ry


def project(points, real_dir=64.0):
    """Canvas offsets (pixels, x right, y down) of model points (metres, ground point at the
    origin) seen in `real_dir`."""
    a = cv.yaw(real_dir)
    c, s = math.cos(a), math.sin(a)
    x = c * points[:, 0] - s * points[:, 1]
    y = s * points[:, 0] + c * points[:, 1]
    u = x * cv.PIXELS_PER_METRE
    v = -(y * cv.SIN_E + points[:, 2] * cv.COS_E) * cv.PIXELS_PER_METRE
    return np.stack([u, v], axis=1)


def ground_shift(du, dv, real_dir=64.0):
    """The ground shift (metres, Blender x, y) that moves the projection by (du, dv) px."""
    qx = du / cv.PIXELS_PER_METRE
    qy = -dv / (cv.SIN_E * cv.PIXELS_PER_METRE)
    a = -cv.yaw(real_dir)
    c, s = math.cos(a), math.sin(a)
    return c * qx - s * qy, s * qx + c * qy


def raster(uv, shape):
    """Boolean mask (rows, cols) of the pixels hit by points `uv`, closed by one pixel."""
    h, w = shape
    i = np.floor(uv[:, 1]).astype(np.int64)
    j = np.floor(uv[:, 0]).astype(np.int64)
    ok = (i >= 0) & (i < h) & (j >= 0) & (j < w)
    m = np.zeros(shape, bool)
    m[i[ok], j[ok]] = True
    return _erode(_dilate(m))


def _dilate(m):
    out = m.copy()
    out[1:] |= m[:-1]
    out[:-1] |= m[1:]
    o2 = out.copy()
    o2[:, 1:] |= out[:, :-1]
    o2[:, :-1] |= out[:, 1:]
    return o2


def _erode(m):
    return ~_dilate(~m)


def iou(a, b):
    u = np.count_nonzero(a | b)
    return np.count_nonzero(a & b) / u if u else 0.0


def _box(mask):
    rows = np.nonzero(mask.any(axis=1))[0]
    cols = np.nonzero(mask.any(axis=0))[0]
    return cols[0], rows[0], cols[-1] + 1, rows[-1] + 1


def place(uv, mask):
    """Scale and shift that put the 0.05..99.95th percentile box of `uv` onto the mask's box."""
    x0, y0, x1, y1 = _box(mask)
    lo = np.percentile(uv, 0.05, axis=0)
    hi = np.percentile(uv, 99.95, axis=0)
    k = math.sqrt((x1 - x0) / max(hi[0] - lo[0], 1e-9) * (y1 - y0) / max(hi[1] - lo[1], 1e-9))
    c = (lo + hi) / 2 * k
    return k, ((x0 + x1) / 2 - c[0], (y0 + y1) / 2 - c[1])


def score(uv, mask, k, shift):
    return iou(raster(uv * k + np.asarray(shift), mask.shape), mask)


def refine(uv, mask, k, shift, scales=(0.92, 0.96, 1.0, 1.04, 1.08), radius=4, step=2):
    """Local search over scale and shift; returns (iou, k, shift)."""
    x0, y0, x1, y1 = _box(mask)
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2  # scale about the silhouette's centre
    best = (score(uv, mask, k, shift), k, shift)
    for _ in range(6):
        _, k0, s0 = best
        for f in scales:
            for dy in range(-radius, radius + 1, step):
                for dx in range(-radius, radius + 1, step):
                    cand = (k0 * f, (cx - (cx - s0[0]) * f + dx, cy - (cy - s0[1]) * f + dy))
                    sc = score(uv, mask, *cand)
                    if sc > best[0]:
                        best = (sc, *cand)
        if best[1] == k0 and best[2] == s0:
            if step == 1:
                break
            step, radius, scales = 1, 2, (0.98, 1.0, 1.02)
    return best


def ground(points):
    """`points` lifted so the lowest stands at z = 0; returns (points, lift)."""
    z0 = points[:, 2].min()
    return points - np.array([0.0, 0.0, z0]), -z0


def fit(points, mask, anchor, real_dir=64.0, yaws=range(0, 360, 10), pitch=0.0, keep=4):
    """Best turn, scale and ground shift of `points` (model metres, centred) against `mask`
    (the sprite's silhouette, anchor = ground point in mask pixels). Returns a dict:
    yaw (degrees), scale, shift (metres, Blender x, y), lift (metres before scaling), iou."""
    ax, ay = anchor
    rough = []
    for yaw in yaws:
        p, lift = ground(points @ rot(yaw, pitch).T)
        uv = project(p, real_dir)
        k_, sh = place(uv, mask)
        rough.append((score(uv, mask, k_, sh), yaw, k_, sh))
    rough.sort(key=lambda r: -r[0])
    best = None
    for _, yaw, k_, sh in rough[:keep]:
        p, lift = ground(points @ rot(yaw, pitch).T)
        uv = project(p, real_dir)
        sc, k2, sh2 = refine(uv, mask, k_, sh)
        if best is None or sc > best["iou"]:
            best = {"yaw": float(yaw), "pitch": float(pitch), "scale": float(k2), "lift": float(lift),
                    "screen": (float(sh2[0] - ax), float(sh2[1] - ay)), "iou": float(sc)}
    best["shift"] = ground_shift(*best["screen"], real_dir)
    best["rough"] = [(round(r[0], 3), r[1]) for r in rough[:keep]]
    return best
