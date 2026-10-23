"""Makes screenshots cheap to look at: trims them to their content and lays several into
one labelled sheet. An image costs about width x height / 750 tokens to read, so a
1600x900 frame is about 1900 tokens and the same man trimmed to 400x500 about 270.

    acr-shots trim IMG... [--max 800] [--margin 12]
        writes IMG.trim.png next to each: cropped to what differs from the corner colour
        (white, greenscreen or a flat ground), then scaled down to at most --max px.
    acr-shots sheet OUT.png IMG... [--cols 4] [--cell 400] [--labels a,b,...]
        trims every image to a common box (the union of their contents, so frames of one
        scene stay aligned), scales to --cell px and writes a grid with labels.
"""

import argparse
import os
import sys

from PIL import Image, ImageChops, ImageDraw


def content_box(im, margin, tol=24):
    rgb = im.convert("RGB")
    bg = Image.new("RGB", rgb.size, rgb.getpixel((0, 0)))
    diff = ImageChops.difference(rgb, bg).convert("L").point(lambda v: 255 if v > tol else 0)
    box = diff.getbbox()
    if not box:
        return (0, 0) + rgb.size
    x0, y0, x1, y1 = box
    w, h = rgb.size
    return (max(0, x0 - margin), max(0, y0 - margin), min(w, x1 + margin), min(h, y1 + margin))


def fit(im, size):
    s = min(1.0, size / max(im.size))
    return im if s >= 1.0 else im.resize((max(1, int(im.width * s)), max(1, int(im.height * s))), Image.LANCZOS)


def trim(paths, maxpx, margin):
    for p in paths:
        im = Image.open(p)
        out = os.path.splitext(p)[0] + ".trim.png"
        fit(im.crop(content_box(im, margin)), maxpx).save(out)
        print(out)


def sheet(out, paths, cols, cell, labels, margin):
    ims = [Image.open(p).convert("RGB") for p in paths]
    boxes = [content_box(im, margin) for im in ims]
    union = (min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes), max(b[3] for b in boxes))
    tiles = [fit(im.crop(union), cell) for im in ims]
    tw, th = max(t.width for t in tiles), max(t.height for t in tiles) + 14
    rows = (len(tiles) + cols - 1) // cols
    canvas = Image.new("RGB", (tw * min(cols, len(tiles)), th * rows), "white")
    d = ImageDraw.Draw(canvas)
    for i, t in enumerate(tiles):
        x, y = (i % cols) * tw, (i // cols) * th
        canvas.paste(t, (x, y + 14))
        d.text((x + 2, y + 1), labels[i] if i < len(labels) else os.path.basename(paths[i]), fill=(160, 0, 160))
    canvas.save(out)
    print(out, canvas.size)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("trim")
    t.add_argument("imgs", nargs="+")
    t.add_argument("--max", type=int, default=800)
    t.add_argument("--margin", type=int, default=12)
    s = sub.add_parser("sheet")
    s.add_argument("out")
    s.add_argument("imgs", nargs="+")
    s.add_argument("--cols", type=int, default=4)
    s.add_argument("--cell", type=int, default=400)
    s.add_argument("--labels", default="")
    s.add_argument("--margin", type=int, default=12)
    a = ap.parse_args()
    if a.cmd == "trim":
        trim(a.imgs, a.max, a.margin)
    else:
        sheet(a.out, a.imgs, a.cols, a.cell, [x for x in a.labels.split(",") if x], a.margin)


if __name__ == "__main__":
    sys.exit(main())
