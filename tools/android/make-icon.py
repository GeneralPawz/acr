#!/usr/bin/env python3
"""Builds the Android launcher icon resources.

`make-icon.py <monogram.svg> <res-dir> [preview.png Ac.ico]`

The original only ships its emblem (a gold "AC" monogram on a steel frame) at 32x32; the exe
resources and the menu art hold nothing larger, and upscaling it gives a blob. So the monogram is
redrawn: `monogram.svg` holds its calligraphic strokes (traced by eye from the 32x32 icon, our own
drawing), stamped here with a broad nib and shaded in brass, with a dark outline and soft shadow.
The backdrop is drawn too: a dark bronze radial gradient with a thin gold ring, in the style of
the game's frames. Output (adaptive icon, API 26+):

  drawable-nodpi/icon_background.png, icon_foreground.png, icon_monochrome.png  (432x432, 108 dp)
  mipmap-anydpi-v26/icon.xml                                                      (themed too)
  mipmap-{m,h,xh,xxh,xxxh}dpi/icon.png                                            (round, 48..192)

The preview (512 and 192 px circles, next to the old icon at 16x) is optional.
"""
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter

LAYER = 432  # 108 dp at xxxhdpi
VISIBLE = 288  # the 72 dp the launcher shows
EMBLEM = 200  # the monogram's longest side: inside the 66 dp (264 px) safe circle
LEGACY = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
GOLD = (222, 170, 62)

XML = """<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/icon_background" />
    <foreground android:drawable="@drawable/icon_foreground" />
    <monochrome android:drawable="@drawable/icon_monochrome" />
</adaptive-icon>
"""


def smoothstep(x, lo, hi):
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def centre_line(points, weights, step=0.04):
    """Catmull-Rom through the points, with the weights interpolated alongside."""
    p = np.array(points, np.float64)
    w = np.array(weights, np.float64)
    p = np.vstack([2 * p[0] - p[1], p, 2 * p[-1] - p[-2]])
    w = np.concatenate([[w[0]], w, [w[-1]]])
    out = []
    for i in range(1, len(p) - 2):
        n = max(2, int(np.hypot(*(p[i + 1] - p[i])) / step))
        for t in np.linspace(0, 1, n, endpoint=False):
            t2, t3 = t * t, t * t * t
            q = 0.5 * (2 * p[i] + (-p[i - 1] + p[i + 1]) * t + (2 * p[i - 1] - 5 * p[i] + 4 * p[i + 1] - p[i + 2]) * t2
                       + (-p[i - 1] + 3 * p[i] - 3 * p[i + 1] + p[i + 2]) * t3)
            out.append((q[0], q[1], w[i] + (w[i + 1] - w[i]) * t))
    out.append((*points[-1], weights[-1]))
    return out


def monogram(svg):
    """The SVG's strokes as an RGBA emblem, EMBLEM px on its longest side, in brass with a gloss."""
    strokes = []
    for el in ET.parse(svg).getroot().iter("{http://www.w3.org/2000/svg}polyline"):
        pts = [tuple(map(float, q.split(","))) for q in el.get("points").split()]
        strokes.append(centre_line(pts, list(map(float, el.get("w").split()))))
    ss = 4  # supersampling
    k = ss * EMBLEM / 22.0  # px per grid unit: the monogram spans about 22 units
    size = int(28 * k)
    mask = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(mask)
    ang = np.radians(-42)  # broad nib, angled up to the right
    ca, sa = np.cos(ang), np.sin(ang)
    nib = np.array([(np.cos(t) * 1.5, np.sin(t) * 0.22) for t in np.linspace(0, 2 * np.pi, 20, endpoint=False)])
    nib = nib @ np.array([[ca, sa], [-sa, ca]])
    for line in strokes:
        for x, y, w in line:
            d.polygon([((x + dx * w) * k, (y + dy * w) * k) for dx, dy in nib], fill=255)
    box = mask.getbbox()
    mask = mask.crop(box)
    mask = mask.resize((mask.width // ss, mask.height // ss), Image.LANCZOS)
    pad = 16
    canvas = Image.new("L", (mask.width + 2 * pad, mask.height + 2 * pad), 0)
    canvas.paste(mask, (pad, pad))
    # Gloss: bright along the stroke's middle, darker brass toward the edges, lit from the top left.
    soft = np.asarray(canvas.filter(ImageFilter.GaussianBlur(4.5))).astype(np.float32) / 255
    yy, xx = np.mgrid[0 : canvas.height, 0 : canvas.width].astype(np.float32)
    light = 0.18 * (1 - (xx / canvas.width + yy / canvas.height))
    lum = 60 + 175 * np.clip(soft * 1.05 - 0.12 + light, 0, 1)
    rgb = gold_ramp(lum)
    img = Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8), "RGB").convert("RGBA")
    img.putalpha(canvas)
    return img


def gold_ramp(lum):
    """The original's brass, as a clean ramp from dark to bright gold (its pixels are noisy)."""
    t = smoothstep(lum, 70, 235)[..., None]
    dark, mid, light = (128, 78, 22), (226, 170, 58), (255, 238, 170)
    lo = np.array(dark) + (np.array(mid) - np.array(dark)) * np.clip(t * 2, 0, 1)
    return lo + (np.array(light) - np.array(mid)) * np.clip(t * 2 - 1, 0, 1)


def foreground(emblem):
    """The emblem centred on a transparent layer, over its dark outline and a soft shadow."""
    layer = Image.new("RGBA", (LAYER, LAYER), (0, 0, 0, 0))
    pos = ((LAYER - emblem.width) // 2, (LAYER - emblem.height) // 2)
    mask = Image.new("L", layer.size, 0)
    mask.paste(emblem.getchannel("A"), pos)
    outline = mask.filter(ImageFilter.MaxFilter(9)).filter(ImageFilter.GaussianBlur(1.2))
    shadow = mask.filter(ImageFilter.MaxFilter(5)).filter(ImageFilter.GaussianBlur(7))
    shadow = shadow.point(lambda v: v * 6 // 10)
    layer.paste((12, 6, 2, 255), (8, 10), shadow)
    layer.paste((46, 24, 8, 255), (0, 0), outline)
    layer.alpha_composite(emblem, pos)
    return layer


def ring(draw, centre, radius, width, colour):
    c = centre
    draw.ellipse([c - radius, c - radius, c + radius, c + radius], outline=colour, width=width)


def background():
    """Dark bronze radial gradient with a thin gold ring (2x supersampled for smooth edges)."""
    n = LAYER * 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    d = np.hypot(xx - n / 2, yy - n / 2) / (VISIBLE / 2 * 2)  # 1.0 at the visible circle's edge
    t = np.clip(d, 0, 1.3)[..., None]
    inner, outer = np.array([104, 66, 30], np.float32), np.array([30, 16, 7], np.float32)
    rgb = inner + (outer - inner) * np.clip(t, 0, 1) ** 1.3
    # Faint worn-leather noise so the flat gradient does not band.
    rng = np.random.default_rng(332)
    rgb += rng.normal(0, 1.6, (n, n, 1))
    img = Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8), "RGB").convert("RGBA")
    d = ImageDraw.Draw(img)
    c = n // 2
    ring(d, c, 2 * 132, 5, (58, 36, 14, 255))  # shadow side of the ring
    ring(d, c, 2 * 130, 7, (206, 156, 58, 255))
    ring(d, c, 2 * 130 - 4, 2, (255, 222, 130, 255))  # highlight on the inner edge
    return img.resize((LAYER, LAYER), Image.LANCZOS)


def monochrome(fg):
    """Themed icons: the emblem's silhouette, opaque white where it is, nothing else."""
    a = fg.getchannel("A")
    mask = np.asarray(a) > 200
    solid = Image.fromarray((mask * 255).astype(np.uint8), "L")
    layer = Image.new("RGBA", fg.size, (255, 255, 255, 0))
    layer.putalpha(solid.filter(ImageFilter.GaussianBlur(0.8)))
    return layer


def circle_mask(size):
    m = Image.new("L", (size * 4, size * 4), 0)
    ImageDraw.Draw(m).ellipse([0, 0, size * 4 - 1, size * 4 - 1], fill=255)
    return m.resize((size, size), Image.LANCZOS)


def masked(bg, fg, size):
    """What a launcher shows: background and foreground, the central 72 dp, cut to a circle."""
    comp = bg.copy()
    comp.alpha_composite(fg)
    off = (LAYER - VISIBLE) // 2
    comp = comp.crop((off, off, off + VISIBLE, off + VISIBLE)).resize((size, size), Image.LANCZOS)
    comp.putalpha(circle_mask(size))
    return comp


def main(svg, res, preview=None, old_icon=None):
    res = Path(res)
    fg = foreground(monogram(svg))
    bg = background()
    nodpi = res / "drawable-nodpi"
    nodpi.mkdir(parents=True, exist_ok=True)
    bg.convert("RGB").save(nodpi / "icon_background.png")
    fg.save(nodpi / "icon_foreground.png")
    monochrome(fg).save(nodpi / "icon_monochrome.png")
    anydpi = res / "mipmap-anydpi-v26"
    anydpi.mkdir(parents=True, exist_ok=True)
    (anydpi / "icon.xml").write_text(XML)
    for density, size in LEGACY.items():
        d = res / f"mipmap-{density}"
        d.mkdir(parents=True, exist_ok=True)
        masked(bg, fg, size).save(d / "icon.png")
    if preview:
        old = Image.open(old_icon).convert("RGBA").resize((512, 512), Image.NEAREST) if old_icon else None
        sheet = Image.new("RGBA", (24 + 512 + 24 + 512 * (old is not None) + 24, 512 + 2 * 24 + 192 + 24), (236, 236, 236, 255))
        sheet.alpha_composite(masked(bg, fg, 512), (24, 24))
        sheet.alpha_composite(masked(bg, fg, 192), (24, 512 + 48))
        if old:
            sheet.alpha_composite(old, (24 + 512 + 24, 24))
        sheet.save(preview)


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4, 5):
        sys.exit(__doc__)
    main(*sys.argv[1:])
