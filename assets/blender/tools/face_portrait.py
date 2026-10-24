"""Aligns a generated frontal portrait to the head's frontal guide (#306).

    python face_portrait.py GUIDE.png PHOTO.png OUT.png

`heads.face_map` writes the guide (`$ACR_FACE_GUIDE/<unit>.guide.png`, the painted face seen
from the front in `faces.PORTRAIT_BOX`); an image model turns it into a photo, which keeps
the layout only roughly (a few per cent of scale and offset). This finds the scale, aspect
and shift that lay the photo's dark features (brows, eyes, nostrils, mouth line) over the
guide's inside the face area, by FFT cross-correlation over a small grid of scales, and
writes the photo warped into the guide's frame: ready as `$ACR_FACES/<unit>.portrait.png`.
Needs numpy and Pillow only.
"""

import os
import sys

import numpy as np
from PIL import Image, ImageFilter

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))
from acr_assets import faces  # noqa: E402

N = 192  # working size


def features(im):
    """Dark detail: how much darker than its surroundings each pixel is."""
    g = im.convert("L").resize((N, N), Image.BILINEAR)
    a = np.asarray(g, np.float32) / 255.0
    b = np.asarray(g.filter(ImageFilter.BoxBlur(6)), np.float32) / 255.0
    return np.clip(b - a, 0.0, None)


def face_mask(n, grow=1.1):
    r, c = np.mgrid[0:n, 0:n] + 0.5
    x0, x1, z0, z1 = faces.PORTRAIT_BOX
    x = x0 + c / n * (x1 - x0)
    z = z1 - r / n * (z1 - z0)
    return (faces.portrait_radius(x, z) < grow).astype(np.float32)


def warp(im, s, a, dx, dy, size):
    """`im` scaled by s*a across, s/a down about the centre, then shifted by (dx, dy)."""
    w, h = size
    sx, sy = s * a, s / a
    cx, cy = w / 2, h / 2
    data = (1 / sx, 0, cx - (cx + dx) / sx, 0, 1 / sy, cy - (cy + dy) / sy)
    return im.resize(size, Image.BICUBIC).transform(size, Image.AFFINE, data, Image.BICUBIC, fillcolor=(255, 255, 255))


def align(guide, photo):
    fg = features(guide) * face_mask(N)
    fg -= fg.mean()
    Fg = np.fft.rfft2(fg)
    best = (-1.0, 1.0, 1.0, 0, 0)
    lim = N // 5
    for s in np.arange(0.80, 1.25, 0.02):
        for a in (0.94, 0.97, 1.0, 1.03, 1.06):
            fp = features(warp(photo, s, a, 0, 0, photo.size))
            fp -= fp.mean()
            cc = np.fft.irfft2(np.conj(Fg) * np.fft.rfft2(fp), s=(N, N))
            cc = np.roll(np.roll(cc, lim, 0), lim, 1)[: 2 * lim, : 2 * lim]
            i = int(np.argmax(cc))
            v = float(cc.flat[i]) / (np.linalg.norm(fp) + 1e-6)
            if v > best[0]:
                r, c = divmod(i, 2 * lim)
                best = (v, float(s), a, lim - c, lim - r)
    return best


def main():
    gpath, ppath, out = sys.argv[1:4]
    guide = Image.open(gpath).convert("RGB")
    photo = Image.open(ppath).convert("RGB")
    v, s, a, dx, dy = align(guide, photo)
    k = guide.size[0] / N
    res = warp(photo, s, a, dx * k, dy * k, guide.size)
    res.save(out)
    print(f"{os.path.basename(ppath)}: score {v:.3f} scale {s:.2f} aspect {a:.2f} shift {dx * k:.0f},{dy * k:.0f} px")


if __name__ == "__main__":
    main()
