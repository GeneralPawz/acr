"""Shrinks an exported `.glb` in place (pure Python, no `bpy`; #199).

Blender writes every attribute it does not know as float and samples every clip at every
frame. This pass rewrites the binary chunk with core glTF 2.0 types the renderer reads
(`acr-render` `model.rs`), with no extension required:

- `WEIGHTS_0` as normalised unsigned bytes, rounded so each vertex still sums to 255;
- `COLOR_0`, `_GORE`, `_FACE`, `_WIND` (values in 0..1) as normalised unsigned bytes;
- `TEXCOORD_n` as normalised unsigned shorts when every value lies in 0..1;
- rotation outputs of clips as normalised shorts;
- linear clip channels without the keys that interpolation between their neighbours
  reproduces within `ROT_TOL` / `MOVE_TOL` (the sampled frames between a clip's keys), a
  constant channel as its first and last key, and a constant channel equal to the node's
  own transform (the exported STAND pose, which the renderer falls back to) dropped while
  another channel keeps the clip's length;
- identical images, buffer views and accessors stored once.

`_PART` and `_LIMB` stay float: a vertex attribute element takes 4 bytes anyway (glTF
aligns each one to 4), and `_PART` holds part numbers, not 0..1 values. Positions, normals
and morph targets stay float (quantising them needs `KHR_mesh_quantization`).
"""

import hashlib
import json
import math
import struct
import zlib

ROT_TOL = 2e-4      # rad: a dropped rotation key is reproduced within this angle
MOVE_TOL = 2e-4     # m (and scale units) for translation and scale keys
DROP_REST = True    # drop constant channels equal to the node transform

FLOAT, BYTE, UBYTE, SHORT, USHORT, UINT = 5126, 5120, 5121, 5122, 5123, 5125
SIZE = {BYTE: 1, UBYTE: 1, SHORT: 2, USHORT: 2, UINT: 4, FLOAT: 4}
FMT = {BYTE: "b", UBYTE: "B", SHORT: "h", USHORT: "H", UINT: "I", FLOAT: "f"}
COMPONENTS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
UNIT_ATTRIBUTES = ("COLOR_0", "_GORE", "_FACE", "_WIND")
ARRAY_BUFFER = 34962


# ----------------------------------------------------------------------------- container

def read_glb(data):
    magic, _version, _length = struct.unpack_from("<4sII", data, 0)
    if magic != b"glTF":
        raise ValueError("not a glb")
    pos, doc, blob = 12, None, b""
    while pos < len(data):
        n, kind = struct.unpack_from("<II", data, pos)
        chunk = data[pos + 8:pos + 8 + n]
        if kind == 0x4E4F534A:
            doc = json.loads(chunk.decode("utf-8"))
        elif kind == 0x004E4942:
            blob = bytes(chunk)
        pos += 8 + n
    return doc, blob


def write_glb(doc, blob):
    text = json.dumps(doc, separators=(",", ":")).encode("utf-8")
    text += b" " * (-len(text) % 4)
    blob += b"\0" * (-len(blob) % 4)
    total = 12 + 8 + len(text) + (8 + len(blob) if blob else 0)
    out = struct.pack("<4sII", b"glTF", 2, total) + struct.pack("<II", len(text), 0x4E4F534A) + text
    if blob:
        out += struct.pack("<II", len(blob), 0x004E4942) + blob
    return out


# ----------------------------------------------------------------------------- accessors

def read_accessor(doc, views, a):
    """The accessor's elements as tuples of floats (normalised integers mapped to 0..1 or
    -1..1). Sparse accessors are not read (None)."""
    if "sparse" in a or "bufferView" not in a:
        return None
    ct, n, k = a["componentType"], a["count"], COMPONENTS[a["type"]]
    view = doc["bufferViews"][a["bufferView"]]
    data = views[a["bufferView"]]
    size = SIZE[ct] * k
    stride = view.get("byteStride") or size
    off = a.get("byteOffset", 0)
    fmt = "<" + FMT[ct] * k
    out = [struct.unpack_from(fmt, data, off + i * stride) for i in range(n)]
    if a.get("normalized"):
        scale = {UBYTE: 255.0, USHORT: 65535.0, BYTE: 127.0, SHORT: 32767.0}[ct]
        out = [tuple(max(v / scale, -1.0) for v in e) for e in out]
    return out


def unorm(values, ct):
    top = 255 if ct == UBYTE else 65535
    return [tuple(min(top, max(0, round(v * top))) for v in e) for e in values]


def snorm16(values):
    return [tuple(min(32767, max(-32767, round(v * 32767))) for v in e) for e in values]


def weights_u8(values):
    """Weights as bytes that sum to 255 per vertex (largest remainder), the strongest
    influence first among equals; an all-zero vertex stays zero."""
    out = []
    for w in values:
        s = sum(w)
        if s <= 0:
            out.append((0,) * len(w))
            continue
        exact = [x / s * 255 for x in w]
        base = [math.floor(x) for x in exact]
        rest = 255 - sum(base)
        order = sorted(range(len(w)), key=lambda i: (-(exact[i] - base[i]), -w[i], i))
        for i in order[:rest]:
            base[i] += 1
        out.append(tuple(base))
    return out


# ----------------------------------------------------------------------------- clips

def slerp(a, b, t):
    d = sum(x * y for x, y in zip(a, b))
    if d < 0:
        d, b = -d, tuple(-x for x in b)
    if d > 0.9995:
        wa, wb = 1 - t, t
    else:
        th = math.acos(min(1.0, d))
        s = math.sin(th)
        wa, wb = math.sin((1 - t) * th) / s, math.sin(t * th) / s
    q = [x * wa + y * wb for x, y in zip(a, b)]
    n = math.sqrt(sum(x * x for x in q)) or 1.0
    return tuple(x / n for x in q)


def quat_angle(a, b):
    d = abs(sum(x * y for x, y in zip(a, b)))
    na = math.sqrt(sum(x * x for x in a)) * math.sqrt(sum(x * x for x in b)) or 1.0
    return 2 * math.acos(min(1.0, d / na))


def _err(path, a, b):
    if path == "rotation":
        return quat_angle(a, b)
    return max(abs(x - y) for x, y in zip(a, b))


def _interp(path, a, b, t):
    if path == "rotation":
        return slerp(a, b, t)
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def reduce_keys(times, values, path, tol=None):
    """Indices of the keys to keep so that linear (slerp for rotations) interpolation
    between kept keys reproduces every dropped key within `tol`. First and last stay."""
    tol = (ROT_TOL if path == "rotation" else MOVE_TOL) if tol is None else tol
    n = len(times)
    if n <= 2:
        return list(range(n))
    keep, a = [0], 0
    while a < n - 1:
        b = a + 1
        while b + 1 < n:
            c = b + 1
            ta, tc = times[a], times[c]
            ok = True
            for i in range(a + 1, c):
                t = (times[i] - ta) / (tc - ta) if tc > ta else 0.0
                if _err(path, _interp(path, values[a], values[c], t), values[i]) > tol:
                    ok = False
                    break
            if not ok:
                break
            b = c
        keep.append(b)
        a = b
    return keep


def is_constant(values, path, tol=None):
    tol = (ROT_TOL if path == "rotation" else MOVE_TOL) if tol is None else tol
    return all(_err(path, values[0], v) <= tol for v in values[1:])


NODE_DEFAULT = {"translation": (0.0, 0.0, 0.0), "rotation": (0.0, 0.0, 0.0, 1.0), "scale": (1.0, 1.0, 1.0)}


# ----------------------------------------------------------------------------- the pass

class _Packer:
    def __init__(self, doc, blob):
        self.doc = doc
        self.views = []
        for v in doc.get("bufferViews", []):
            off = v.get("byteOffset", 0)
            self.views.append(blob[off:off + v["byteLength"]])

    def new_view(self, data, target=None):
        v = {"buffer": 0, "byteLength": len(data)}
        if target:
            v["target"] = target
        self.doc["bufferViews"].append(v)
        self.views.append(data)
        return len(self.views) - 1

    def put(self, a, values, ct, normalized, target=None):
        """Points accessor `a` at a new view with `values` encoded as `ct`."""
        k = COMPONENTS[a["type"]]
        fmt = "<" + FMT[ct] * k
        data = b"".join(struct.pack(fmt, *e) for e in values)
        if target == ARRAY_BUFFER:
            size = SIZE[ct] * k
            if size % 4:   # vertex elements are 4-byte aligned: give the view a stride
                pad = b"\0" * (-size % 4)
                data = b"".join(struct.pack(fmt, *e) + pad for e in values)
        a["bufferView"] = self.new_view(data, target)
        a.pop("byteOffset", None)
        a["componentType"] = ct
        if normalized:
            a["normalized"] = True
        else:
            a.pop("normalized", None)
        if target == ARRAY_BUFFER and (SIZE[ct] * k) % 4:
            self.doc["bufferViews"][-1]["byteStride"] = SIZE[ct] * k + (-(SIZE[ct] * k) % 4)
        if ct != FLOAT and "min" in a and a.get("normalized"):
            a.pop("min", None)
            a.pop("max", None)

    def put_float(self, values, type_, minmax=False):
        a = {"type": type_, "count": len(values), "componentType": FLOAT}
        if minmax:
            k = COMPONENTS[type_]
            a["min"] = [min(e[i] for e in values) for i in range(k)]
            a["max"] = [max(e[i] for e in values) for i in range(k)]
        self.put(a, values, FLOAT, False)
        self.doc["accessors"].append(a)
        return len(self.doc["accessors"]) - 1

    # --- vertex attributes
    def attributes(self, stats):
        acc = self.doc["accessors"]
        done = set()
        for mesh in self.doc.get("meshes", []):
            for prim in mesh["primitives"]:
                for sem, ai in prim["attributes"].items():
                    if ai in done:
                        continue
                    a = acc[ai]
                    if a["componentType"] != FLOAT and not (sem in UNIT_ATTRIBUTES and a.get("normalized")
                                                            and a["componentType"] == USHORT):
                        continue
                    vals = read_accessor(self.doc, self.views, a)
                    if vals is None:
                        continue
                    if sem.startswith("WEIGHTS_"):
                        self.put(a, weights_u8(vals), UBYTE, True, ARRAY_BUFFER)
                    elif sem in UNIT_ATTRIBUTES:
                        if any(v < -1e-6 or v > 1 + 1e-6 for e in vals for v in e):
                            continue
                        self.put(a, unorm(vals, UBYTE), UBYTE, True, ARRAY_BUFFER)
                    elif sem.startswith("TEXCOORD_"):
                        if any(v < 0 or v > 1 for e in vals for v in e):
                            continue
                        self.put(a, unorm(vals, USHORT), USHORT, True, ARRAY_BUFFER)
                    else:
                        continue
                    done.add(ai)
                    key = sem.rstrip("0123456789").rstrip("_")
                    stats[key] = stats.get(key, 0) + 1

    def morphs(self, stats):
        """Dense float morph targets as sparse accessors (zero base) where that is smaller:
        face morphs move the head only."""
        acc = self.doc["accessors"]
        done = set()
        for mesh in self.doc.get("meshes", []):
            for prim in mesh["primitives"]:
                for target in prim.get("targets", []):
                    for ai in target.values():
                        a = acc[ai]
                        if ai in done or a["componentType"] != FLOAT or "sparse" in a:
                            continue
                        vals = read_accessor(self.doc, self.views, a)
                        if vals is None:
                            continue
                        moved = [i for i, e in enumerate(vals) if any(e)]
                        ict, isz = (USHORT, 2) if a["count"] <= 65535 else (UINT, 4)
                        k = COMPONENTS[a["type"]]
                        if len(moved) * (isz + 4 * k) + 8 >= a["count"] * 4 * k:
                            continue
                        done.add(ai)
                        a.pop("bufferView", None)
                        a.pop("byteOffset", None)
                        if not moved:
                            continue   # all zero: no buffer at all
                        idx = struct.pack("<" + FMT[ict] * len(moved), *moved)
                        val = b"".join(struct.pack("<" + "f" * k, *vals[i]) for i in moved)
                        a["sparse"] = {"count": len(moved),
                                       "indices": {"bufferView": self.new_view(idx), "componentType": ict},
                                       "values": {"bufferView": self.new_view(val)}}
                        stats["sparse_morphs"] = stats.get("sparse_morphs", 0) + 1

    # --- clips
    def clips(self, stats):
        acc = self.doc["accessors"]
        nodes = self.doc.get("nodes", [])
        before = after = dropped = 0
        for anim in self.doc.get("animations", []):
            samplers = anim["samplers"]
            decoded = []
            for ch in anim["channels"]:
                s = samplers[ch["sampler"]]
                path = ch["target"]["path"]
                times = read_accessor(self.doc, self.views, acc[s["input"]])
                vals = read_accessor(self.doc, self.views, acc[s["output"]])
                decoded.append((ch, s, path, times, vals))
            end = max((t[-1][0] for _, _, _, t, _ in decoded if t), default=0.0)
            new_channels, new_samplers = [], []
            keeps_end = False
            pending = []
            for ch, s, path, times, vals in decoded:
                interp = s.get("interpolation", "LINEAR")
                if (times is None or vals is None or path == "weights" or interp == "CUBICSPLINE"
                        or "node" not in ch["target"]):
                    new_samplers.append(dict(s))
                    new_channels.append(dict(ch, sampler=len(new_samplers) - 1))
                    keeps_end = keeps_end or (times is not None and times and times[-1][0] >= end)
                    continue
                before += len(times)
                t = [x[0] for x in times]
                if is_constant(vals, path):
                    node = nodes[ch["target"]["node"]]
                    own = tuple(node.get(path, NODE_DEFAULT[path]))
                    if DROP_REST and _err(path, vals[0], own) <= (ROT_TOL if path == "rotation" else MOVE_TOL) / 4:
                        pending.append((ch, s, path, t, vals))
                        continue
                    keep = [0, len(t) - 1] if len(t) > 1 else [0]
                elif interp == "STEP":
                    keep = [0] + [i for i in range(1, len(t)) if vals[i] != vals[i - 1]]
                    if keep[-1] != len(t) - 1:
                        keep.append(len(t) - 1)
                else:
                    keep = reduce_keys(t, vals, path)
                new_samplers.append(self._sampler(s, path, [t[i] for i in keep], [vals[i] for i in keep]))
                new_channels.append(dict(ch, sampler=len(new_samplers) - 1))
                after += len(keep)
                keeps_end = keeps_end or t[-1] >= end
            for ch, s, path, t, vals in pending:
                if keeps_end:
                    dropped += 1
                    continue
                keep = [0, len(t) - 1] if len(t) > 1 else [0]
                new_samplers.append(self._sampler(s, path, [t[i] for i in keep], [vals[i] for i in keep]))
                new_channels.append(dict(ch, sampler=len(new_samplers) - 1))
                after += len(keep)
                keeps_end = keeps_end or t[-1] >= end
            anim["samplers"], anim["channels"] = new_samplers, new_channels
        stats["clip_keys"] = [before, after]
        stats["clip_channels_dropped"] = dropped

    def _sampler(self, s, path, times, vals):
        out = dict(s)
        out["input"] = self.put_float([(x,) for x in times], "SCALAR", minmax=True)
        if path == "rotation":
            a = {"type": "VEC4", "count": len(vals), "componentType": FLOAT}
            self.put(a, snorm16(vals), SHORT, True)
            self.doc["accessors"].append(a)
            out["output"] = len(self.doc["accessors"]) - 1
        else:
            out["output"] = self.put_float(vals, COMPONENTS_TYPE[len(vals[0])])
        return out

    # --- images
    def images(self, stats):
        seen, remap = {}, {}
        for i, img in enumerate(self.doc.get("images", [])):
            if "bufferView" not in img:
                continue
            if img.get("mimeType") == "image/png":
                small = png_shrink(self.views[img["bufferView"]])
                if small is not None:
                    stats["image_bytes"] = [a + b for a, b in zip(stats.get("image_bytes", [0, 0]),
                                                                (len(self.views[img["bufferView"]]), len(small)))]
                    self.views[img["bufferView"]] = small
            key = (img.get("mimeType"), hashlib.sha1(self.views[img["bufferView"]]).hexdigest())
            if key in seen:
                remap[i] = seen[key]
            else:
                seen[key] = i
        if remap:
            for tex in self.doc.get("textures", []):
                if tex.get("source") in remap:
                    tex["source"] = remap[tex["source"]]
        stats["images_shared"] = len(remap)

    # --- garbage collection, dedupe, write
    def finish(self):
        doc = self.doc
        acc = doc.get("accessors", [])
        used_acc = set()
        for mesh in doc.get("meshes", []):
            for prim in mesh["primitives"]:
                used_acc |= set(prim["attributes"].values())
                if "indices" in prim:
                    used_acc.add(prim["indices"])
                for t in prim.get("targets", []):
                    used_acc |= set(t.values())
        for skin in doc.get("skins", []):
            if "inverseBindMatrices" in skin:
                used_acc.add(skin["inverseBindMatrices"])
        for anim in doc.get("animations", []):
            for s in anim["samplers"]:
                used_acc |= {s["input"], s["output"]}
        view_key = {}
        new_views, new_data, vmap = [], [], {}

        def keep_view(vi):
            if vi in vmap:
                return vmap[vi]
            v = doc["bufferViews"][vi]
            data = self.views[vi]
            key = (v.get("target"), v.get("byteStride"), hashlib.sha1(data).hexdigest())
            if key in view_key:
                vmap[vi] = view_key[key]
                return vmap[vi]
            nv = {k: x for k, x in v.items() if k not in ("byteOffset",)}
            new_views.append(nv)
            new_data.append(data)
            view_key[key] = vmap[vi] = len(new_views) - 1
            return vmap[vi]

        acc_key, amap, new_acc = {}, {}, []
        for ai in sorted(used_acc):
            a = dict(acc[ai])
            if "bufferView" in a:
                a["bufferView"] = keep_view(a["bufferView"])
            if "sparse" in a:
                sp = a["sparse"] = json.loads(json.dumps(a["sparse"]))
                sp["indices"]["bufferView"] = keep_view(sp["indices"]["bufferView"])
                sp["values"]["bufferView"] = keep_view(sp["values"]["bufferView"])
            key = json.dumps(a, sort_keys=True)
            if key not in acc_key:
                new_acc.append(a)
                acc_key[key] = len(new_acc) - 1
            amap[ai] = acc_key[key]
        for img in doc.get("images", []):
            if "bufferView" in img:
                img["bufferView"] = keep_view(img["bufferView"])
        for mesh in doc.get("meshes", []):
            for prim in mesh["primitives"]:
                prim["attributes"] = {k: amap[v] for k, v in prim["attributes"].items()}
                if "indices" in prim:
                    prim["indices"] = amap[prim["indices"]]
                if "targets" in prim:
                    prim["targets"] = [{k: amap[v] for k, v in t.items()} for t in prim["targets"]]
        for skin in doc.get("skins", []):
            if "inverseBindMatrices" in skin:
                skin["inverseBindMatrices"] = amap[skin["inverseBindMatrices"]]
        for anim in doc.get("animations", []):
            for s in anim["samplers"]:
                s["input"], s["output"] = amap[s["input"]], amap[s["output"]]
        # Lay the views out, 4-byte aligned.
        blob = bytearray()
        for v, data in zip(new_views, new_data):
            blob += b"\0" * (-len(blob) % 4)
            v["byteOffset"] = len(blob)
            v["byteLength"] = len(data)
            blob += data
        doc["bufferViews"] = new_views
        doc["accessors"] = new_acc
        if doc.get("buffers"):
            doc["buffers"] = [{"byteLength": len(blob) + (-len(blob) % 4)}]
        return bytes(blob)


COMPONENTS_TYPE = {1: "SCALAR", 2: "VEC2", 3: "VEC3", 4: "VEC4"}


def pack_bytes(data):
    """The packed glb and what changed."""
    doc, blob = read_glb(data)
    if not doc.get("bufferViews"):
        return data, {}
    p = _Packer(doc, blob)
    stats = {}
    p.attributes(stats)
    p.morphs(stats)
    p.clips(stats)
    p.images(stats)
    blob = p.finish()
    return write_glb(doc, blob), stats


def pack_file(path):
    with open(path, "rb") as f:
        data = f.read()
    out, stats = pack_bytes(data)
    with open(path, "wb") as f:
        f.write(out)
    stats["bytes"] = [len(data), len(out)]
    return stats


# ----------------------------------------------------------------------------- PNG

def _chunks(data):
    pos = 8
    while pos < len(data):
        n, kind = struct.unpack_from(">I4s", data, pos)
        yield kind, data[pos + 8:pos + 8 + n]
        pos += 12 + n


def png_decode(data):
    """8-bit, non-interlaced greyscale, RGB or RGBA PNG -> uint8 array (h, w, c), else None."""
    import numpy as np

    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    idat, head = [], None
    for kind, body in _chunks(data):
        if kind == b"IHDR":
            head = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"tRNS" or (kind[:1].isupper() and kind != b"IEND"):
            return None     # a palette or transparency key: leave the image as it is
        # Other ancillary chunks (gamma, profile, EXIF, offsets) go: the renderer's decoder
        # (`png` with `normalize_to_color8`) ignores them.
    w, h, depth, ctype, _c, _f, interlace = head
    chans = {0: 1, 2: 3, 4: 2, 6: 4}.get(ctype)
    if depth != 8 or interlace or chans is None:
        return None
    raw = np.frombuffer(zlib.decompress(b"".join(idat)), np.uint8).reshape(h, 1 + w * chans)
    out = np.zeros((h, w * chans), np.uint8)
    prev = np.zeros(w * chans, np.int16)
    for y in range(h):
        f, line = raw[y, 0], raw[y, 1:].astype(np.int16)
        if f == 0:
            cur = line
        elif f == 2:
            cur = (line + prev) & 255
        elif f == 1:
            cur = line.reshape(w, chans).cumsum(axis=0, dtype=np.int64).reshape(-1) & 255
        else:
            cur = np.zeros_like(line)
            lst, pv = line.tolist(), prev.tolist()
            c = cur.tolist()
            for i in range(len(lst)):
                a = c[i - chans] if i >= chans else 0
                if f == 3:
                    c[i] = (lst[i] + ((a + pv[i]) >> 1)) & 255
                else:
                    b, cc = pv[i], pv[i - chans] if i >= chans else 0
                    p = a + b - cc
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - cc)
                    pr = a if pa <= pb and pa <= pc else (b if pb <= pc else cc)
                    c[i] = (lst[i] + pr) & 255
            cur = np.array(c, np.int16)
        out[y] = cur
        prev = cur.astype(np.int16)
    return out.reshape(h, w, chans)


def _filtered(rows, bpp):
    """Each row filtered with the type (0..4) that minimises the sum of absolute
    differences, the usual libpng heuristic."""
    import numpy as np

    x = rows.astype(np.int16)
    up = np.vstack([np.zeros((1, x.shape[1]), np.int16), x[:-1]])
    left = np.hstack([np.zeros((x.shape[0], bpp), np.int16), x[:, :-bpp]])
    ul = np.hstack([np.zeros((x.shape[0], bpp), np.int16), up[:, :-bpp]])
    p = left + up - ul
    pa, pb, pc = np.abs(p - left), np.abs(p - up), np.abs(p - ul)
    paeth = np.where((pa <= pb) & (pa <= pc), left, np.where(pb <= pc, up, ul))
    cand = [x, x - left, x - up, x - ((left + up) >> 1), x - paeth]
    cand = [(c & 255).astype(np.uint8) for c in cand]
    cost = np.stack([np.minimum(c, 256 - c.astype(np.int16)).sum(axis=1) for c in cand])
    best = cost.argmin(axis=0)
    out = np.empty((x.shape[0], x.shape[1] + 1), np.uint8)
    out[:, 0] = best
    for k in range(5):
        sel = best == k
        out[sel, 1:] = cand[k][sel]
    return out


def png_encode(pixels, palette=None, alpha=None):
    """PNG bytes for (h, w, c) uint8 pixels, or (h, w) indices into `palette` (n, 3) with
    optional per-entry `alpha`; adaptive filters, zlib level 9."""
    import numpy as np

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    h, w = pixels.shape[:2]
    if palette is not None:
        head = struct.pack(">IIBBBBB", w, h, 8, 3, 0, 0, 0)
        rows = np.hstack([np.zeros((h, 1), np.uint8), pixels.astype(np.uint8)])
        extra = chunk(b"PLTE", palette.astype(np.uint8).tobytes())
        if alpha is not None:
            extra += chunk(b"tRNS", alpha.astype(np.uint8).tobytes())
    else:
        c = pixels.shape[2]
        head = struct.pack(">IIBBBBB", w, h, 8, {1: 0, 2: 4, 3: 2, 4: 6}[c], 0, 0, 0)
        rows = _filtered(pixels.reshape(h, w * c), c)
        extra = b""
    body = zlib.compress(rows.tobytes(), 9)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", head) + extra + chunk(b"IDAT", body) + chunk(b"IEND", b"")


def png_shrink(data):
    """The smallest lossless re-encoding of a PNG (constant opaque alpha dropped, an indexed
    image when it has at most 256 colours), or None if none is smaller or numpy is missing."""
    try:
        import numpy as np
    except ImportError:
        return None
    px = png_decode(data)
    if px is None:
        return None
    if px.shape[2] in (2, 4) and (px[..., -1] == 255).all():
        px = px[..., :-1]
    best = [data, png_encode(px)]
    if px.shape[2] in (3, 4):
        flat = px.reshape(-1, px.shape[2])
        key = np.zeros(len(flat), np.uint64)
        for k in range(px.shape[2]):
            key = key * 256 + flat[:, k].astype(np.uint64)
        colours, index = np.unique(key, return_inverse=True)
        if len(colours) <= 256:
            pal = np.array([[(int(c) >> (8 * (px.shape[2] - 1 - k))) & 255 for k in range(px.shape[2])]
                            for c in colours], np.uint8)
            idx = index.reshape(px.shape[:2])
            best.append(png_encode(idx, pal[:, :3], pal[:, 3] if px.shape[2] == 4 else None))
    small = min(best, key=len)
    return None if small is data else small


# ----------------------------------------------------------------------------- Blender import

REST_CLIP = "__REST"


def with_rest_clip(data):
    """The glb with one more clip, `REST_CLIP`, that holds every animated node at its own
    transform. Blender's importer leaves a bone a clip does not key in whatever pose the
    last evaluated clip gave it, where the renderer uses the node's transform (the STAND
    pose); playing `REST_CLIP` first restores that (`render_views.show_state`)."""
    doc, blob = read_glb(data)
    anims = doc.get("animations", [])
    if not anims:
        return data
    targets = sorted({(c["target"]["node"], c["target"]["path"]) for a in anims for c in a["channels"]
                      if "node" in c["target"] and c["target"]["path"] != "weights"})
    blob = bytearray(blob)

    def add(values, type_, minmax=False):
        k = COMPONENTS[type_]
        data = struct.pack("<" + "f" * (k * len(values)), *[x for e in values for x in e])
        blob.extend(b"\0" * (-len(blob) % 4))
        doc["bufferViews"].append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob.extend(data)
        a = {"bufferView": len(doc["bufferViews"]) - 1, "componentType": FLOAT, "count": len(values), "type": type_}
        if minmax:
            a["min"] = [min(e[i] for e in values) for i in range(k)]
            a["max"] = [max(e[i] for e in values) for i in range(k)]
        doc["accessors"].append(a)
        return len(doc["accessors"]) - 1

    t = add([(0.0,)], "SCALAR", minmax=True)
    samplers, channels = [], []
    for node, path in targets:
        own = tuple(doc["nodes"][node].get(path, NODE_DEFAULT[path]))
        samplers.append({"input": t, "output": add([own], COMPONENTS_TYPE[len(own)]), "interpolation": "LINEAR"})
        channels.append({"sampler": len(samplers) - 1, "target": {"node": node, "path": path}})
    anims.append({"name": REST_CLIP, "samplers": samplers, "channels": channels})
    doc["buffers"] = [{"byteLength": len(blob) + (-len(blob) % 4)}]
    return write_glb(doc, bytes(blob))
