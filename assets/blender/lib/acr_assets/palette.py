"""Colour comparison of a model against its sprites (pure numpy, no `bpy`).

Used by `assets/blender/tools/palette_check.py`: sRGB to CIELAB (D65), CIEDE2000, a
trimmed median as the dominant colour of a region, and the region name of a glTF
material (`engm18.coat.outfit.001.gltf` -> `coat`).
"""

import re

import numpy as np

# Lightness weight of the colour difference: the sprites carry the original lighting
# (shaded albedo), the model's colours are unlit, so lightness counts half (kL = 2).
KL = 2.0
# Regions with fewer sprite pixels than this are not judged.
MIN_PIXELS = 12
# Share of a region's pixels nearest its median that make the dominant colour.
KEEP = 0.6


def srgb_to_lab(rgb):
    """CIELAB (D65) of sRGB values in 0..1, shape (..., 3)."""
    c = np.asarray(rgb, dtype=np.float64)
    lin = np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)
    m = np.array([[0.4124564, 0.3575761, 0.1804375],
                  [0.2126729, 0.7151522, 0.0721750],
                  [0.0193339, 0.1191920, 0.9503041]])
    xyz = lin @ m.T / np.array([0.95047, 1.0, 1.08883])
    e = 216 / 24389
    f = np.where(xyz > e, np.cbrt(xyz), (24389 / 27 * xyz + 16) / 116)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], -1)


def lab_to_srgb(lab):
    """Inverse of `srgb_to_lab`, clipped to 0..1."""
    lab = np.asarray(lab, dtype=np.float64)
    fy = (lab[..., 0] + 16) / 116
    fx = fy + lab[..., 1] / 500
    fz = fy - lab[..., 2] / 200
    e = 6 / 29

    def inv(f):
        return np.where(f > e, f ** 3, 3 * e * e * (f - 4 / 29))

    xyz = np.stack([inv(fx), inv(fy), inv(fz)], -1) * np.array([0.95047, 1.0, 1.08883])
    m = np.array([[3.2404542, -1.5371385, -0.4985314],
                  [-0.9692660, 1.8760108, 0.0415560],
                  [0.0556434, -0.2040259, 1.0572252]])
    lin = np.clip(xyz @ m.T, 0, 1)
    return np.where(lin <= 0.0031308, 12.92 * lin, 1.055 * lin ** (1 / 2.4) - 0.055)


def ciede2000(lab1, lab2, kl=KL):
    """CIEDE2000 difference of two CIELAB colours (arrays broadcast), lightness weight kl."""
    l1, a1, b1 = np.moveaxis(np.asarray(lab1, dtype=np.float64), -1, 0)
    l2, a2, b2 = np.moveaxis(np.asarray(lab2, dtype=np.float64), -1, 0)
    c1, c2 = np.hypot(a1, b1), np.hypot(a2, b2)
    cm = (c1 + c2) / 2
    g = 0.5 * (1 - np.sqrt(cm ** 7 / (cm ** 7 + 25.0 ** 7)))
    a1p, a2p = (1 + g) * a1, (1 + g) * a2
    c1p, c2p = np.hypot(a1p, b1), np.hypot(a2p, b2)
    h1p = np.degrees(np.arctan2(b1, a1p)) % 360
    h2p = np.degrees(np.arctan2(b2, a2p)) % 360
    dl = l2 - l1
    dc = c2p - c1p
    dh = h2p - h1p
    dh = np.where(dh > 180, dh - 360, np.where(dh < -180, dh + 360, dh))
    dh = np.where(c1p * c2p == 0, 0, dh)
    dhh = 2 * np.sqrt(c1p * c2p) * np.sin(np.radians(dh / 2))
    lm = (l1 + l2) / 2
    cmp_ = (c1p + c2p) / 2
    hs = h1p + h2p
    hm = np.where(np.abs(h1p - h2p) > 180, np.where(hs < 360, (hs + 360) / 2, (hs - 360) / 2), hs / 2)
    hm = np.where(c1p * c2p == 0, hs, hm)
    t = (1 - 0.17 * np.cos(np.radians(hm - 30)) + 0.24 * np.cos(np.radians(2 * hm))
         + 0.32 * np.cos(np.radians(3 * hm + 6)) - 0.20 * np.cos(np.radians(4 * hm - 63)))
    sl = 1 + 0.015 * (lm - 50) ** 2 / np.sqrt(20 + (lm - 50) ** 2)
    sc = 1 + 0.045 * cmp_
    sh = 1 + 0.015 * cmp_ * t
    rt = (-2 * np.sqrt(cmp_ ** 7 / (cmp_ ** 7 + 25.0 ** 7))
          * np.sin(np.radians(60 * np.exp(-(((hm - 275) / 25) ** 2)))))
    return np.sqrt((dl / (kl * sl)) ** 2 + (dc / sc) ** 2 + (dhh / sh) ** 2 + rt * (dc / sc) * (dhh / sh))


def dominant(lab, keep=KEEP):
    """Dominant colour of CIELAB samples (n, 3): the per-channel median of the share
    `keep` of samples nearest the overall median (drops outlines, neighbours and
    highlights that a misregistered region picks up)."""
    lab = np.asarray(lab, dtype=np.float64).reshape(-1, 3)
    if len(lab) == 0:
        return None
    med = np.median(lab, 0)
    d = np.linalg.norm(lab - med, axis=1)
    n = max(1, int(round(len(lab) * keep)))
    return np.median(lab[np.argsort(d, kind="stable")[:n]], 0)


_SUFFIX = re.compile(r"(\.(outfit|gltf|\d+))+$")


def region(material, unit):
    """Region name of a glTF material: the unit prefix and the `.outfit`, `.<n>`, `.gltf`
    suffixes removed (`engm18.belts.outfit.002.gltf` -> `belts`); materials of other
    sources (the horse's `M_Body`) keep their name."""
    name = material
    if name.lower().startswith(unit.lower() + "."):
        name = name[len(unit) + 1:]
    return _SUFFIX.sub("", name) or material


def erode(mask, rounds=1):
    """Boolean mask shrunk by `rounds` pixels (4-neighbourhood)."""
    m = np.asarray(mask, dtype=bool).copy()
    for _ in range(rounds):
        p = np.pad(m, 1, constant_values=False)
        m = m & p[:-2, 1:-1] & p[2:, 1:-1] & p[1:-1, :-2] & p[1:-1, 2:]
    return m
