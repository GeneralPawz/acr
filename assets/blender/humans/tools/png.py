"""Minimal RGBA PNG reader/writer in pure Python (no PIL), for the sprite study and sheets."""

import struct
import zlib


def read(path):
    """Returns (w, h, rows) with rows a list of bytearrays of RGBA (8 bit) pixels."""
    d = open(path, "rb").read()
    i = 8
    idat = b""
    plte = None
    trns = None
    while i < len(d):
        n, t = struct.unpack(">I4s", d[i:i + 8])
        c = d[i + 8:i + 8 + n]
        if t == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", c[:10])
        elif t == b"PLTE":
            plte = c
        elif t == b"tRNS":
            trns = c
        elif t == b"IDAT":
            idat += c
        i += 12 + n
    if depth != 8:
        raise ValueError(f"{path}: bit depth {depth} not supported")
    bpp = {6: 4, 2: 3, 3: 1, 0: 1, 4: 2}[ctype]
    raw = zlib.decompress(idat)
    s = w * bpp
    rows = []
    prev = bytearray(s)
    for y in range(h):
        f = raw[y * (s + 1)]
        line = bytearray(raw[y * (s + 1) + 1:(y + 1) * (s + 1)])
        for x in range(s):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        out = bytearray(w * 4)
        for x in range(w):
            if ctype == 6:
                out[x * 4:x * 4 + 4] = line[x * 4:x * 4 + 4]
            elif ctype == 2:
                out[x * 4:x * 4 + 4] = line[x * 3:x * 3 + 3] + b"\xff"
            elif ctype == 3:
                k = line[x]
                out[x * 4:x * 4 + 3] = plte[k * 3:k * 3 + 3]
                out[x * 4 + 3] = trns[k] if trns and k < len(trns) else 255
            elif ctype == 0:
                out[x * 4:x * 4 + 4] = bytes((line[x],) * 3) + b"\xff"
            else:
                out[x * 4:x * 4 + 4] = bytes((line[x * 2],) * 3) + bytes((line[x * 2 + 1],))
        rows.append(out)
    return w, h, rows


def write(path, w, h, rows):
    raw = b"".join(b"\x00" + bytes(r) for r in rows)

    def chunk(t, c):
        return struct.pack(">I", len(c)) + t + c + struct.pack(">I", zlib.crc32(t + c) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    open(path, "wb").write(data)


def blank(w, h, rgba=(0, 0, 0, 0)):
    return [bytearray(bytes(rgba) * w) for _ in range(h)]


def paste(dst, src, x0, y0, scale=1, over=True):
    """Nearest-neighbour paste of src rows (RGBA) into dst rows at (x0, y0), scaled."""
    sh = len(src)
    sw = len(src[0]) // 4 if sh else 0
    dh = len(dst)
    dw = len(dst[0]) // 4
    for y in range(sh * scale):
        dy = y0 + y
        if not 0 <= dy < dh:
            continue
        srow = src[y // scale]
        drow = dst[dy]
        for x in range(sw * scale):
            dx = x0 + x
            if not 0 <= dx < dw:
                continue
            p = srow[(x // scale) * 4:(x // scale) * 4 + 4]
            if over and p[3] < 128:
                continue
            drow[dx * 4:dx * 4 + 4] = p
