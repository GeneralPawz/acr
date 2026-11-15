"""Clothing audit of a built model (docs/clothing.md "Clothing audit", #523).

Reads the exported glb (no Blender, numpy only), skins it in `STAND` and in the middle of the
walk clip the way the game would, and gives each garment numbers for what makes clothing
read as buggy mesh:

- **poke**: vertices of an inner layer (or the skin, or a stump cap) that lie outside the
  layer above them (`poke`);
- **ragged edges**: how far the open boundary loops (hems, collars, cuffs, tops) leave their
  own smoothed outline and how many teeth they have per metre (`loop_raggedness`);
- **holes**: skin not covered inside a covered region (`skin_holes`), and boundary edges of
  one garment that face each other across a gap, i.e. a seam left open (`open_seams`);
- **shell or draped**: from the triangulation (`kind`): a draped garment is a Delaunay
  lattice of near-equilateral triangles, a shell cut from the body hull or a modelled grid
  is not;
- **stretch and collapse** of the faces in the walk against `STAND`, and slivers
  (`stretch`);
- **shading soup**: the share of faces whose normal differs much from their neighbours'
  (`soup`).

`audit(path)` returns one row per garment; `tools/clothing_audit.py` runs it over folders
and writes the CSV and the ranking. The thresholds (`LIMITS`) were calibrated on the
English pikeman of 2026-11-15 (`ACR-analysis/clothing-audit/2026-11-15/verified.md`).
"""

import json
import math
import struct

import numpy as np

from . import garments

# Flag limits; a row gets the flag when its number is above the limit.
LIMITS = {
    "poke": 12,  # vertices of this layer outside the layer above (worse pose)
    "poke_skin": 4,  # skin or cap vertices outside a garment
    "rag_mm": 8.0,  # mean distance of the reversal vertices of the worst outline from their chord
    "teeth_per_m": 2.0,  # with rag_mm: teeth per metre of that outline
    "hole": 4,  # uncovered skin vertices inside a covered region
    "seam_cm": 15.0,  # length of boundary facing its own garment across a gap
    "stretch": 0.08,  # share of faces whose area changes by more than STRETCH in the walk
    "sliver": 0.25,  # share of faces with a smallest angle under SLIVER_DEG
    "soup": 0.45,  # share of faces more than SOUP_DEG off their neighbours
}
POKE_MM = (2.0, 25.0)  # outside the upper layer by this much (the depth bias hides less)
COVER_MM = 60.0  # skin counts as covered when a garment lies this close above it
STRETCH = (0.6, 1.6)  # face area in the walk / in STAND outside this range
SLIVER_DEG = 10.0
SOUP_DEG = 25.0
RAG_TURN = 15.0  # a vertex of an outline counts from this turn (degrees) ...
RAG_DEV = 0.002  # ... and this distance from its neighbours' chord (m)
RAG_MIN_LOOP = 0.15  # shorter loops are not outlines
SEAM_NEAR = 0.025  # boundary within this distance of other boundary of the same garment
SEAM_ARC = 0.06  # ... that lies at least this far along the loop
DRAPED_MIN_ANGLE = 45.5  # median smallest angle of a draped (Delaunay lattice) garment
KNN = 16

# Meshes that are neither garment nor cover.
IGNORE = ("caps", "fringe", "hair", "beard", "bones", "brow", "eye", "teeth", "tongue", "lash")
RIGID_LAYERS = ("metal",)  # armour: covers, is not audited as cloth
WALK_CLIPS = ("MOTION_L", "MOTION_R", "MOTION_L0", "PMOTION_L")

_DT = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
_NC = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
_NORM = {5120: 127.0, 5121: 255.0, 5122: 32767.0, 5123: 65535.0}


# -- glb ---------------------------------------------------------------------------------------

class Glb:
    """The JSON and binary chunk of a glb, with accessors as numpy arrays."""

    def __init__(self, path):
        with open(path, "rb") as f:
            data = f.read()
        if data[:4] != b"glTF":
            raise ValueError(f"{path}: not a glb")
        n, _ = struct.unpack_from("<II", data, 12)
        self.doc = json.loads(data[20:20 + n])
        off = 20 + n
        self.bin = data[off + 8:off + 8 + struct.unpack_from("<I", data, off)[0]] if off < len(data) else b""

    def accessor(self, i, raw=False):
        a = self.doc["accessors"][i]
        dt, nc = _DT[a["componentType"]], _NC[a["type"]]
        if "bufferView" in a:
            v = self.doc["bufferViews"][a["bufferView"]]
            start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
            item = np.dtype(dt).itemsize * nc
            stride = v.get("byteStride") or item
            if stride == item:
                out = np.frombuffer(self.bin, dt, a["count"] * nc, start).reshape(a["count"], nc)
            else:
                buf = np.frombuffer(self.bin, np.uint8, (a["count"] - 1) * stride + item, start)
                idx = np.arange(a["count"])[:, None] * stride + np.arange(item)[None, :]
                out = buf[idx].copy().view(dt).reshape(a["count"], nc)
        else:
            out = np.zeros((a["count"], nc), dt)
        if raw:
            return out
        if a.get("normalized") and a["componentType"] in _NORM:
            return np.maximum(out.astype(np.float64) / _NORM[a["componentType"]], -1.0)
        return out.astype(np.float64) if dt == np.float32 else out


def _trs(t, r, s):
    x, y, z, w = r
    m = np.eye(4)
    m[:3, :3] = np.array([
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ]) * np.asarray(s)[None, :]
    m[:3, 3] = t
    return m


def sample(times, values, t, interp="LINEAR"):
    """One animation sampler at time `t`: linear (quaternions normalised lerp), step, or the
    value part of a cubic spline."""
    times = np.asarray(times, float).ravel()
    values = np.asarray(values, float)
    if interp == "CUBICSPLINE":
        values = values.reshape(len(times), 3, -1)[:, 1]
    if t <= times[0] or len(times) == 1:
        return values[0]
    if t >= times[-1]:
        return values[-1]
    i = int(np.searchsorted(times, t, side="right")) - 1
    if interp == "STEP":
        return values[i]
    f = (t - times[i]) / (times[i + 1] - times[i])
    a, b = values[i], values[i + 1]
    if len(a) == 4:
        if float(a @ b) < 0.0:
            b = -b
        q = a + (b - a) * f
        return q / np.linalg.norm(q)
    return a + (b - a) * f


class Model:
    """A glb's nodes, clips and meshes; `pose(clip, where)` gives world matrices."""

    def __init__(self, path):
        self.glb = g = Glb(path)
        self.doc = d = g.doc
        self.nodes = d.get("nodes", [])
        self.parent = {}
        for i, n in enumerate(self.nodes):
            for c in n.get("children", []):
                self.parent[c] = i
        self.clips = {a.get("name", str(i)): a for i, a in enumerate(d.get("animations", []))}

    def clip_length(self, clip):
        a = self.clips[clip]
        return max(float(self.doc["accessors"][s["input"]]["max"][0]) for s in a["samplers"])

    def pose(self, clip=None, where=0.0):
        """World matrix per node, with `clip` sampled at `where` (0..1 of its length).
        Channels a clip does not key keep the node's own transform (the exported `STAND`)."""
        local = []
        for n in self.nodes:
            local.append({"t": n.get("translation", [0, 0, 0]), "r": n.get("rotation", [0, 0, 0, 1]),
                          "s": n.get("scale", [1, 1, 1]), "m": n.get("matrix")})
        if clip is not None:
            a = self.clips[clip]
            t = self.clip_length(clip) * where
            for ch in a["channels"]:
                tg = ch["target"]
                key = {"translation": "t", "rotation": "r", "scale": "s"}.get(tg["path"])
                if key is None or "node" not in tg:
                    continue
                s = a["samplers"][ch["sampler"]]
                local[tg["node"]][key] = sample(self.glb.accessor(s["input"]), self.glb.accessor(s["output"]), t,
                                                s.get("interpolation", "LINEAR"))
        mats = [np.array(l["m"], float).reshape(4, 4).T if l["m"] else _trs(l["t"], l["r"], l["s"]) for l in local]
        world = [None] * len(mats)

        def w(i):
            if world[i] is None:
                p = self.parent.get(i)
                world[i] = mats[i] if p is None else w(p) @ mats[i]
            return world[i]

        return [w(i) for i in range(len(mats))]

    def mesh_nodes(self):
        return [(i, n) for i, n in enumerate(self.nodes) if "mesh" in n]

    def geometry(self, ni):
        """(raw positions, triangles, joints, weights, material keys) of a mesh node, its
        primitives joined. Positions are as stored (quantised or float)."""
        n = self.nodes[ni]
        pos, tri, jo, we, keys, base = [], [], [], [], [], 0
        for p in self.doc["meshes"][n["mesh"]]["primitives"]:
            if p.get("mode", 4) != 4 or "POSITION" not in p["attributes"]:
                continue
            at = p["attributes"]
            v = self.glb.accessor(at["POSITION"])
            f = self.glb.accessor(p["indices"]).reshape(-1, 3).astype(np.int64) if "indices" in p \
                else np.arange(len(v)).reshape(-1, 3)
            pos.append(v)
            tri.append(f + base)
            if "JOINTS_0" in at:
                jo.append(self.glb.accessor(at["JOINTS_0"]).astype(np.int64))
                we.append(self.glb.accessor(at["WEIGHTS_0"]).astype(float))
            base += len(v)
            if "material" in p:
                keys.append(material_key(self.doc["materials"][p["material"]].get("name", "")))
        if not pos:
            return None
        return (np.concatenate(pos), np.concatenate(tri), np.concatenate(jo) if jo else None,
                np.concatenate(we) if we else None, keys)

    def skinned(self, ni, geo, world):
        """World positions of a mesh node in a pose."""
        pos, _tri, jo, we, _ = geo
        n = self.nodes[ni]
        h = np.concatenate([pos, np.ones((len(pos), 1))], axis=1)
        if "skin" in n and jo is not None:
            skin = self.doc["skins"][n["skin"]]
            ibm = self.glb.accessor(skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
            jm = np.stack([world[j] @ ibm[k] for k, j in enumerate(skin["joints"])])
            we = we / np.maximum(we.sum(axis=1, keepdims=True), 1e-9)
            out = np.zeros((len(pos), 3))
            for c in range(jo.shape[1]):
                out += we[:, c:c + 1] * np.einsum("nij,nj->ni", jm[jo[:, c]], h)[:, :3]
            return out
        return (h @ world[ni].T)[:, :3]

    def dominant_bones(self, ni, geo):
        """Name of the bone with the largest weight, per vertex (None for unskinned)."""
        n = self.nodes[ni]
        if "skin" not in n or geo[2] is None:
            return None
        joints = self.doc["skins"][n["skin"]]["joints"]
        j = geo[2][np.arange(len(geo[2])), geo[3].argmax(axis=1)]
        return [self.nodes[joints[k]].get("name", "") for k in j]


def material_key(name):
    """`engpik.breeches.outfit.gltf` -> `breeches`; `engpik.shoes.1.gltf` -> `shoes`."""
    parts = name.split(".")
    return parts[1] if len(parts) > 1 else name


def mesh_key(name):
    """`engpik.fallband` -> `fallband` (the part after the unit's name)."""
    return name.split(".", 1)[1] if "." in name else name


def classify(key, layer, mat_keys, skinned):
    """(role, rank) of a mesh: `skin`, `cap`, `garment` (audited), `cover` (only hides what
    is under it), or `None`."""
    low = key.lower()
    if not skinned or ".lod" in low or low.startswith("lod"):
        return None, 0
    if low in ("flesh", "skin", "body"):
        return "skin", 0
    if any(low.startswith(p) for p in IGNORE):
        return None, 0
    ranks = [garments.LAYER_RANK[k] for k in [low] + list(mat_keys) if k in garments.LAYER_RANK]
    if layer in RIGID_LAYERS:
        return "cover", 5
    if low == "footwear":
        return "garment", 2
    if not ranks:
        return None, 0
    rank = garments.LAYER_RANK.get(low, max(ranks))
    if rank == 0:
        return "skin", 0
    return ("garment" if rank <= 6 else "cover"), rank


# -- pure geometry -------------------------------------------------------------------------------

def weld(raw):
    """Vertices at the same stored position share one index: (index per vertex, count).
    The exporter splits vertices at normals, UV seams and part borders."""
    _u, inv = np.unique(np.ascontiguousarray(raw), axis=0, return_inverse=True)
    inv = inv.ravel()
    return inv, int(inv.max()) + 1 if len(inv) else 0


def welded_triangles(tri, inv):
    t = inv[tri]
    return t[(t[:, 0] != t[:, 1]) & (t[:, 1] != t[:, 2]) & (t[:, 0] != t[:, 2])]


def edges_of(tri):
    """(sorted edge array (m, 2), edge index per triangle corner (n, 3), triangles per edge)."""
    e = np.sort(np.stack([tri[:, [0, 1]], tri[:, [1, 2]], tri[:, [2, 0]]], axis=1).reshape(-1, 2), axis=1)
    u, inv, cnt = np.unique(e, axis=0, return_inverse=True, return_counts=True)
    return u, inv.ravel().reshape(-1, 3), cnt


def boundary_loops(tri):
    """Chains of boundary edges (used by one triangle) as vertex index lists. A closed loop
    repeats nothing; `closed` tells whether its ends meet. Chains break where more than two
    boundary edges meet."""
    if len(tri) == 0:
        return []
    u, _inv, cnt = edges_of(tri)
    b = u[cnt == 1]
    adj = {}
    for a, c in b.tolist():
        adj.setdefault(a, []).append(c)
        adj.setdefault(c, []).append(a)
    used, loops = set(), []

    def walk(start, nxt):
        chain = [start, nxt]
        used.add((min(start, nxt), max(start, nxt)))
        while True:
            cur = chain[-1]
            if len(adj[cur]) != 2:
                return chain, False
            a, c = adj[cur]
            n = a if c == chain[-2] else c
            k = (min(cur, n), max(cur, n))
            if k in used:
                return chain, n == chain[0]
            used.add(k)
            if n == chain[0]:
                return chain, True
            chain.append(n)

    for v in sorted(adj):  # open chains first, from their ends and junctions
        if len(adj[v]) != 2:
            for n in adj[v]:
                if (min(v, n), max(v, n)) not in used:
                    loops.append(walk(v, n))
    for v in sorted(adj):
        for n in adj[v]:
            if (min(v, n), max(v, n)) not in used:
                loops.append(walk(v, n))
    return loops


def resample(points, closed, step=0.01):
    """A polyline resampled at even arc length; returns (points, length)."""
    p = np.asarray(points, float)
    if closed:
        p = np.concatenate([p, p[:1]])
    seg = np.linalg.norm(np.diff(p, axis=0), axis=1)
    s = np.concatenate([[0.0], np.cumsum(seg)])
    length = float(s[-1])
    n = max(int(length / step), 4)
    at = np.linspace(0.0, length, n, endpoint=not closed)
    return np.stack([np.interp(at, s, p[:, k]) for k in range(3)], axis=1), length


def loop_raggedness(points, closed):
    """How jagged an outline is: `(rag_mm, teeth_per_m, length_m)`.

    A clean hem turns one way round the limb; a torn-looking one zigzags. A vertex counts
    when the outline turns there by more than `RAG_TURN` degrees and it lies more than
    `RAG_DEV` off the chord of its neighbours; a tooth is two reversals of the turning
    direction between such vertices. `rag_mm` is the mean distance from the chord at the
    reversals (0 without any), so it does not grow with a coarse but convex outline."""
    p = np.asarray(points, float)
    n = len(p)
    seg = np.linalg.norm(np.diff(np.concatenate([p, p[:1]]) if closed else p, axis=0), axis=1)
    length = float(seg.sum())
    if n < 4 or length <= 0.0:
        return 0.0, 0.0, length
    prev, nxt = np.roll(p, 1, axis=0), np.roll(p, -1, axis=0)
    e0, e1 = p - prev, nxt - p
    turn = np.cross(e0, e1)
    cs = np.sum(e0 * e1, axis=1) / np.maximum(np.linalg.norm(e0, axis=1) * np.linalg.norm(e1, axis=1), 1e-15)
    ang = np.degrees(np.arccos(np.clip(cs, -1.0, 1.0)))
    dev = np.linalg.norm(p - 0.5 * (prev + nxt), axis=1)
    ok = (ang > RAG_TURN) & (dev > RAG_DEV)
    if not closed:
        ok[0] = ok[-1] = False
    idx = np.nonzero(ok)[0]
    if len(idx) < 2:
        return 0.0, 0.0, length
    pairs = list(zip(idx[:-1], idx[1:])) + ([(idx[-1], idx[0])] if closed else [])
    flips = [j for i, j in pairs if float(turn[i] @ turn[j]) < 0.0]
    if not flips:
        return 0.0, 0.0, length
    return float(dev[flips].mean() * 1000.0), len(flips) / 2.0 / length, length


def face_normals(pos, tri):
    n = np.cross(pos[tri[:, 1]] - pos[tri[:, 0]], pos[tri[:, 2]] - pos[tri[:, 0]])
    a = np.linalg.norm(n, axis=1)
    return n / np.maximum(a, 1e-15)[:, None], 0.5 * a


def min_angles(pos, tri):
    """Smallest corner angle of each triangle, degrees."""
    a, b, c = pos[tri[:, 0]], pos[tri[:, 1]], pos[tri[:, 2]]
    out = np.full(len(tri), 180.0)
    for p, q, r in ((a, b, c), (b, c, a), (c, a, b)):
        u, v = q - p, r - p
        cs = np.sum(u * v, axis=1) / np.maximum(np.linalg.norm(u, axis=1) * np.linalg.norm(v, axis=1), 1e-15)
        out = np.minimum(out, np.degrees(np.arccos(np.clip(cs, -1.0, 1.0))))
    return out


def soup(pos, tri):
    """Share of faces whose normal is more than `SOUP_DEG` off the mean normal of the faces
    across their edges. A smooth curve, however coarse, tilts its neighbours to both sides
    and scores 0; a crumpled or folded-over face stands against its neighbours."""
    if len(tri) < 2:
        return 0.0
    n, _a = face_normals(pos, tri)
    _u, fe, cnt = edges_of(tri)
    order = np.argsort(fe.ravel(), kind="stable")
    e_sorted = fe.ravel()[order]
    f_sorted = order // 3
    pair = np.nonzero((e_sorted[1:] == e_sorted[:-1]))[0]
    pair = pair[cnt[e_sorted[pair]] == 2]
    fa, fb = f_sorted[pair], f_sorted[pair + 1]
    tot, num = np.zeros((len(tri), 3)), np.zeros(len(tri))
    np.add.at(tot, fa, n[fb])
    np.add.at(tot, fb, n[fa])
    np.add.at(num, fa, 1)
    np.add.at(num, fb, 1)
    ln = np.linalg.norm(tot, axis=1)
    ok = (num >= 2) & (ln > 1e-9)
    if not ok.any():
        return 0.0
    cs = np.sum(tot * n, axis=1) / np.maximum(ln, 1e-9)
    return float(np.mean(np.degrees(np.arccos(np.clip(cs[ok], -1.0, 1.0))) > SOUP_DEG))


def stretch(pos_a, pos_b, tri):
    """Face areas in pose b against pose a: (share outside `STRETCH`, share collapsed below
    0.4, share of slivers in a, in b)."""
    if len(tri) == 0:
        return 0.0, 0.0, 0.0, 0.0
    _n, a = face_normals(pos_a, tri)
    _n, b = face_normals(pos_b, tri)
    r = b / np.maximum(a, 1e-12)
    ok = a > 1e-9
    return (float(np.mean(((r < STRETCH[0]) | (r > STRETCH[1]))[ok])) if ok.any() else 0.0,
            float(np.mean((r < 0.4)[ok])) if ok.any() else 0.0,
            float(np.mean(min_angles(pos_a, tri) < SLIVER_DEG)), float(np.mean(min_angles(pos_b, tri) < SLIVER_DEG)))


def kind(pos, tri):
    """`draped` for a Delaunay lattice (median smallest angle of at least
    `DRAPED_MIN_ANGLE`), else `shell` (cut from the body hull or a modelled grid); with the
    median angle."""
    if len(tri) == 0:
        return "shell", 0.0
    m = float(np.median(min_angles(pos, tri)))
    return ("draped" if m >= DRAPED_MIN_ANGLE else "shell"), m


def nearest(points, centres, k=KNN, chunk=1024):
    """Indices (n, k) of the `k` nearest `centres` of each point (brute force, chunked)."""
    k = min(k, len(centres))
    c = centres.astype(np.float32)
    c2 = np.sum(c * c, axis=1)
    out = np.empty((len(points), k), np.int64)
    for i in range(0, len(points), chunk):
        p = points[i:i + chunk].astype(np.float32)
        d = c2[None, :] - 2.0 * (p @ c.T)
        out[i:i + chunk] = np.argpartition(d, k - 1, axis=1)[:, :k] if k < len(centres) else np.argsort(d, axis=1)
    return out


def project(points, pos, tri, cand, tol=0.05):
    """Each point projected along the normal of its candidate triangles `cand` (n, k):
    (signed height above the triangle (n, k), whether the foot lies inside it)."""
    a, b, c = pos[tri[cand, 0]], pos[tri[cand, 1]], pos[tri[cand, 2]]
    v0, v1 = b - a, c - a
    nrm = np.cross(v0, v1)
    nrm /= np.maximum(np.linalg.norm(nrm, axis=2, keepdims=True), 1e-15)
    v2 = points[:, None, :] - a
    h = np.sum(v2 * nrm, axis=2)
    v2 = v2 - h[..., None] * nrm
    d00, d01, d11 = np.sum(v0 * v0, axis=2), np.sum(v0 * v1, axis=2), np.sum(v1 * v1, axis=2)
    d20, d21 = np.sum(v2 * v0, axis=2), np.sum(v2 * v1, axis=2)
    den = np.where(np.abs(d00 * d11 - d01 * d01) < 1e-18, 1e-18, d00 * d11 - d01 * d01)
    v = (d11 * d20 - d01 * d21) / den
    w = (d00 * d21 - d01 * d20) / den
    return h, (v >= -tol) & (w >= -tol) & (v + w <= 1.0 + tol), nrm


def cover_heights(points, pos, tri, k=KNN):
    """For each point the triangle it projects into with the smallest |height|:
    (height in m, triangle index, triangle normal); height is nan where none of the `k`
    nearest triangles is under or over the point."""
    n = len(points)
    if n == 0 or len(tri) == 0:
        return np.full(n, np.nan), np.zeros(n, np.int64), np.zeros((n, 3))
    cen = pos[tri].mean(axis=1)
    cand = nearest(points, cen, k)
    h, inside, nrm = project(points, pos, tri, cand)
    ah = np.where(inside, np.abs(h), np.inf)
    best = ah.argmin(axis=1)
    rows = np.arange(n)
    hh = np.where(np.isfinite(ah[rows, best]), h[rows, best], np.nan)
    return hh, cand[rows, best], nrm[rows, best]


def vertex_normals(pos, tri):
    n = np.cross(pos[tri[:, 1]] - pos[tri[:, 0]], pos[tri[:, 2]] - pos[tri[:, 0]])
    out = np.zeros_like(pos)
    for c in range(3):
        np.add.at(out, tri[:, c], n)
    return out / np.maximum(np.linalg.norm(out, axis=1, keepdims=True), 1e-15)


def poke(points, normals, pos, tri, owner):
    """Inner vertices outside the layer above: the vertex projects into an upper triangle
    that faces the same way and lies `POKE_MM` above it. Returns (mask, height mm, owner of
    the triangle, mask of the vertices that are covered at all)."""
    h, t, nrm = cover_heights(points, pos, tri)
    same = np.sum(nrm * normals, axis=1) > 0.2
    mm = h * 1000.0
    covered = np.isfinite(h) & same & (np.abs(mm) < COVER_MM)
    out = covered & (mm > POKE_MM[0]) & (mm < POKE_MM[1])
    return out, np.where(out, mm, 0.0), owner[t] if len(tri) else t, covered


def skin_holes(points, normals, pos, tri, owner, radius=0.04, share=0.85):
    """Skin vertices no garment covers, although most of the skin round them is covered:
    (mask of holes, owner of the cover round each hole or -1)."""
    h, t, nrm = cover_heights(points, pos, tri)
    mm = h * 1000.0
    covered = np.isfinite(h) & (mm < 3.0) & (mm > -COVER_MM) & (np.sum(nrm * normals, axis=1) > 0.0)
    own = np.where(covered, owner[t] if len(tri) else 0, -1)
    holes, who = np.zeros(len(points), bool), np.full(len(points), -1)
    idx = np.nonzero(~covered)[0]
    for i in idx:
        near = np.nonzero(np.sum((points - points[i]) ** 2, axis=1) < radius * radius)[0]
        near = near[near != i]
        if len(near) >= 6 and covered[near].mean() >= share:
            holes[i] = True
            o = own[near][own[near] >= 0]
            who[i] = np.bincount(o).argmax() if len(o) else -1
    return holes, who


def open_seams(pos, loops, tri=None):
    """Length (m) of boundary that faces other boundary of the same garment within
    `SEAM_NEAR` but at least `SEAM_ARC` away along the outline, and the widest such gap
    (m): a seam that did not close, a slit, or a crack between two pieces. Boundary
    vertices joined by an edge of `tri` do not count (the two sides of a narrow strip)."""
    pts, lid, arc, vid, lens = [], [], [], [], []
    for k, (chain, closed) in enumerate(loops):
        p, length = resample(pos[chain], closed, 0.01)
        pts.append(p)
        lid.append(np.full(len(p), k))
        arc.append(np.linspace(0.0, length, len(p), endpoint=not closed))
        ch = np.asarray(chain)
        d = np.linalg.norm(p[:, None, :] - pos[ch][None, :, :], axis=2)
        vid.append(ch[d.argmin(axis=1)])
        lens.append((length, closed))
    if not pts:
        return 0.0, 0.0
    p, lid, arc, vid = np.concatenate(pts), np.concatenate(lid), np.concatenate(arc), np.concatenate(vid)
    uniq, local = np.unique(vid, return_inverse=True)
    joined = np.eye(len(uniq), dtype=bool)
    if tri is not None and len(tri):
        e = edges_of(tri)[0]
        e = e[np.isin(e[:, 0], uniq) & np.isin(e[:, 1], uniq)]
        a, b = np.searchsorted(uniq, e[:, 0]), np.searchsorted(uniq, e[:, 1])
        joined[a, b] = joined[b, a] = True
        joined = (joined.astype(np.int32) @ joined.astype(np.int32)) > 0  # and one vertex further
    total, widest = 0.0, 0.0
    for i in range(0, len(p), 512):
        sl = slice(i, i + 512)
        d = np.linalg.norm(p[sl, None, :] - p[None, :, :], axis=2)
        da = np.abs(arc[sl, None] - arc[None, :])
        ln = np.array([lens[k][0] for k in lid[sl]])[:, None]
        cl = np.array([lens[k][1] for k in lid[sl]])[:, None]
        da = np.where(cl, np.minimum(da, ln - da), da)
        far = ((lid[sl, None] != lid[None, :]) | (da > SEAM_ARC)) & ~joined[local[sl]][:, local]
        d = np.where(far, d, np.inf)
        m = d.min(axis=1)
        hit = (m < SEAM_NEAR) & (m > 0.003)
        total += 0.01 * int(hit.sum())
        if hit.any():
            widest = max(widest, float(m[hit].max()))
    return total / 2.0, widest  # both sides of a gap are counted


def hull_girth(pts):
    """Perimeter of the convex hull of 2D points (m)."""
    p = sorted(set(map(tuple, np.round(np.asarray(pts, float), 6).tolist())))
    if len(p) < 3:
        return 0.0

    def half(seq):
        out = []
        for q in seq:
            while len(out) >= 2 and ((out[-1][0] - out[-2][0]) * (q[1] - out[-2][1])
                                     - (out[-1][1] - out[-2][1]) * (q[0] - out[-2][0])) <= 0:
                out.pop()
            out.append(q)
        return out

    h = half(p)[:-1] + half(p[::-1])[:-1]
    return float(sum(math.dist(h[i], h[(i + 1) % len(h)]) for i in range(len(h))))


def girth_profile(points, a, b, step=0.02, slab=0.015):
    """Convex girths (m) of `points` in slabs across the axis a -> b, every `step`:
    [(distance from a, girth)]; slabs with fewer than 5 points are left out."""
    a, b = np.asarray(a, float), np.asarray(b, float)
    ax = (b - a) / np.linalg.norm(b - a)
    u = np.cross(ax, [1.0, 0.0, 0.0] if abs(ax[0]) < 0.9 else [0.0, 1.0, 0.0])
    u /= np.linalg.norm(u)
    v = np.cross(ax, u)
    rel = np.asarray(points, float) - a
    t = rel @ ax
    out = []
    for s in np.arange(0.0, float(np.linalg.norm(b - a)) + 1e-9, step):
        sel = np.abs(t - s) < slab
        if sel.sum() >= 5:
            out.append((round(float(s), 3), hull_girth(np.stack([rel[sel] @ u, rel[sel] @ v], axis=1))))
    return out


SHAPE_COLUMNS = ["unit", "legwear", "thigh_cm", "above_knee_cm", "band_cm", "calf_cm", "fullness", "closure",
                 "band_gap_cm"]
LEGWEAR_RANKS = (1, 2)


def leg_shape(m, parts, world, pose="stand", side="l"):
    """The cut of the legwear as numbers (STAND, left leg), or None without leg bones:

    - `thigh_cm`: girth of the legwear at mid thigh; `above_knee_cm`: its widest girth 4-16 cm
      above the knee; `band_cm`: its narrowest 1-7 cm below the knee (where a knee band
      closes); `calf_cm`: widest between there and the ankle;
    - `fullness` = above_knee / band: about 1.0 is a tube, 1.3 and more reads as breeches
      gathered into a band;
    - `closure` = band / the skin's girth at the same place: 1.0-1.15 lies on the leg, more
      stands off as an open pipe;
    - `band_gap_cm`: the longest stretch from 10 cm above the knee to the ankle where no
      legwear vertex lies (bare skin between breeches and stocking)."""
    idx = {n.get("name", ""): i for i, n in enumerate(m.nodes)}
    names = [f"thigh_{side}", f"shin_{side}", f"foot_{side}"]
    if any(n not in idx for n in names):
        return None
    hip, knee, ankle = (world[idx[n]][:3, 3] for n in names)
    leg_bones = set(names[:2])
    cloth, skin, worn = [], [], []
    for p in parts:
        if not p["bones"]:
            continue
        on = np.array([b in leg_bones for b in p["bones"]])
        if p["role"] == "skin":
            skin.append(p[pose][on])
        elif p["role"] == "garment" and p["rank"] in LEGWEAR_RANKS and p["key"] != "footwear" and on.any():
            cloth.append(p[pose][on])
            worn.append(p["key"])
    if not cloth or not skin:
        return None
    cloth, skin = np.concatenate(cloth), np.concatenate(skin)
    up = dict((round(float(np.linalg.norm(knee - hip)) - s, 3), g) for s, g in girth_profile(cloth, hip, knee))
    down = dict(girth_profile(cloth, knee, ankle))
    sdown = dict(girth_profile(skin, knee, ankle))
    above = [g for s, g in up.items() if 0.04 <= s <= 0.16]
    mid = [g for s, g in up.items() if abs(s - 0.5 * float(np.linalg.norm(knee - hip))) < 0.03]
    band = [(g, s) for s, g in down.items() if 0.01 <= s <= 0.07]
    if not above or not band:
        return {"legwear": "+".join(sorted(set(worn))), "thigh_cm": 0, "above_knee_cm": 0, "band_cm": 0, "calf_cm": 0,
                "fullness": 0, "closure": 0, "band_gap_cm": 99.0}
    bg, bs = min(band)
    sk = sdown.get(bs) or min(sdown.values(), default=bg)
    sk = sk if sk > 0.05 else bg  # a degenerate skin slab: no closure number
    calf = [g for s, g in down.items() if s > bs]
    # Bare stretch along the leg: sorted distances of the legwear vertices along hip -> ankle.
    def along(a, b, pts):
        ax = (b - a) / np.linalg.norm(b - a)
        return (pts - a) @ ax

    lk, la = float(np.linalg.norm(knee - hip)), float(np.linalg.norm(ankle - knee))
    t = np.sort(np.concatenate([along(hip, knee, cloth)[along(hip, knee, cloth) <= lk] - lk,
                                along(knee, ankle, cloth)[along(knee, ankle, cloth) > 0]]))
    t = np.concatenate([[-0.10], t[(t > -0.10) & (t < la - 0.03)], [la - 0.03]])
    gap = float(np.diff(t).max()) if len(t) > 1 else la
    return {"legwear": "+".join(sorted(set(worn))), "thigh_cm": round(100 * max(mid, default=0.0), 1),
            "above_knee_cm": round(100 * max(above), 1), "band_cm": round(100 * bg, 1),
            "calf_cm": round(100 * max(calf, default=0.0), 1), "fullness": round(max(above) / bg, 2),
            "closure": round(bg / max(sk, 1e-6), 2), "band_gap_cm": round(100 * gap, 1)}


# -- the audit ---------------------------------------------------------------------------------

COLUMNS = ["unit", "garment", "rank", "layer", "kind", "made_by", "tri_angle", "tris", "poke_stand", "poke_walk",
           "poke_share", "poke_mm", "poke_over", "poke_at", "rag_mm", "teeth_per_m", "rag_loop_cm", "rag_at", "holes_stand",
           "holes_walk", "seam_cm", "seam_gap_mm", "stretch", "collapse", "sliver_stand", "sliver_walk", "soup",
           "flags", "score"]


def flags(row):
    """Names of the limits a row breaks, and a score: the sum of number / limit over them."""
    out, score = [], 0.0

    def add(name, value, limit):
        nonlocal score
        if value > limit:
            out.append(name)
            score += min(value / limit, 10.0)

    pk = max(row["poke_stand"], row["poke_walk"])
    add("poke", pk, LIMITS["poke_skin"] if row["kind"] == "skin" else LIMITS["poke"])
    if row["rag_mm"] > LIMITS["rag_mm"] and row["teeth_per_m"] > LIMITS["teeth_per_m"]:
        out.append("ragged")
        score += min(row["rag_mm"] / LIMITS["rag_mm"] * row["teeth_per_m"] / LIMITS["teeth_per_m"], 10.0)
    add("hole", max(row["holes_stand"], row["holes_walk"]), LIMITS["hole"])
    add("seam", row["seam_cm"], LIMITS["seam_cm"])
    add("stretch", row["stretch"], LIMITS["stretch"])
    add("sliver", max(row["sliver_stand"], row["sliver_walk"]), LIMITS["sliver"])
    add("soup", row["soup"], LIMITS["soup"])
    return out, round(score, 2)


def walk_clip(model):
    for c in WALK_CLIPS:
        if c in model.clips:
            return c
    for c in sorted(model.clips):
        if "MOTION" in c:
            return c
    return None


def audit(path, unit=None, shape=None):
    """One row (dict, `COLUMNS`) per garment of the glb at `path`, plus `flesh` (holes and
    skin poking out) and `caps` (stump caps showing). Returns (rows, note); rows is empty
    with a note when the model has no separable garments."""
    import os

    unit = unit or os.path.splitext(os.path.basename(path))[0]
    m = Model(path)
    if not m.doc.get("skins"):
        return [], "no skin"
    walk = walk_clip(m)
    stand_w = m.pose("STAND" if "STAND" in m.clips else None, 0.0)
    walk_w = m.pose(walk, 0.5) if walk else stand_w
    parts = []
    for ni, n in m.mesh_nodes():
        ex = n.get("extras") or {}
        if "acr_lod" in ex or "acr_held_item" in ex or "acr_socket" in ex:
            continue
        geo = m.geometry(ni)
        if geo is None:
            continue
        key = mesh_key(n.get("name", ""))
        role, rank = classify(key, ex.get("acr_layer"), geo[4], "skin" in n and geo[2] is not None)
        if role is None:
            continue
        inv, _nv = weld(geo[0])
        tri = geo[1]
        parts.append({"key": key, "role": role, "rank": rank, "layer": ex.get("acr_layer", ""), "tri": tri,
                      "made": str(ex.get("acr_garment") or ""),
                      "wtri": welded_triangles(tri, inv), "inv": inv, "bones": m.dominant_bones(ni, geo),
                      "stand": m.skinned(ni, geo, stand_w), "walk": m.skinned(ni, geo, walk_w)})
    garm = [p for p in parts if p["role"] == "garment"]
    # Vertices that coincide with another garment's: seams between the objects of one sewn
    # garment (body and sleeves, coat and facings), not open edges.
    if garm:
        allp = np.round(np.concatenate([p["stand"] for p in garm]), 4)
        gid = np.concatenate([np.full(len(p["stand"]), i) for i, p in enumerate(garm)])
        _u, inv_all = np.unique(allp, axis=0, return_inverse=True)
        inv_all = inv_all.ravel()
        lo, hi = np.full(inv_all.max() + 1, 10 ** 9), np.full(inv_all.max() + 1, -1)
        np.minimum.at(lo, inv_all, gid)
        np.maximum.at(hi, inv_all, gid)
        shared = lo[inv_all] != hi[inv_all]
        at = 0
        for p in garm:
            sh = np.zeros(int(p["inv"].max()) + 1, bool)
            sh[p["inv"][shared[at:at + len(p["stand"])]]] = True
            p["shared"] = sh
            at += len(p["stand"])
    if not garm:
        return [], "no separable garments (one mesh per garment with a known material key is needed)"
    if not any(p["role"] == "skin" for p in parts):
        note = "no skin mesh: holes not checked"
    else:
        note = ""
    if shape is not None:
        sh = leg_shape(m, parts, stand_w)
        if sh:
            shape.update(sh, unit=unit)
    rows = {}
    for p in parts:
        if p["role"] == "cover":
            continue
        row = dict.fromkeys(COLUMNS, 0)
        row.update(unit=unit, garment=p["key"], rank=p["rank"], layer=p["layer"], kind="", poke_over="", poke_at="", rag_at="", made_by="",
                   tris=len(p["tri"]))
        rows[p["key"]] = row
        if p["role"] != "garment":
            row["kind"] = p["role"]
            continue
        # Welded positions: the mean over the split copies (they coincide at rest).
        wp = {}
        for pose in ("stand", "walk"):
            acc = np.zeros((int(p["inv"].max()) + 1, 3))
            cnt = np.zeros(len(acc))
            np.add.at(acc, p["inv"], p[pose])
            np.add.at(cnt, p["inv"], 1)
            wp[pose] = acc / np.maximum(cnt, 1)[:, None]
        row["kind"], row["tri_angle"] = kind(wp["stand"], p["wtri"])
        row["tri_angle"] = round(row["tri_angle"], 1)
        if p["made"]:  # the build's own word (node extra `acr_garment` = "<made>:<builder>")
            row["kind"], _sep, row["made_by"] = p["made"].partition(":")
        st = stretch(wp["stand"], wp["walk"], p["wtri"])
        row["stretch"], row["collapse"], row["sliver_stand"], row["sliver_walk"] = (round(x, 4) for x in st)
        row["soup"] = round(max(soup(wp["stand"], p["wtri"]), soup(wp["walk"], p["wtri"])), 4)
        loops = [(ch, cl) for ch, cl in boundary_loops(p["wtri"]) if p["shared"][ch].mean() < 0.5]
        # An outline under a layer above it (the breeches' waist under the coat) is not seen.
        above = [q for q in parts if q["role"] in ("garment", "cover") and q["rank"] > p["rank"]]
        if above and loops:
            apos = np.concatenate([q["stand"] for q in above])
            off = np.cumsum([0] + [len(q["stand"]) for q in above])
            atri = np.concatenate([q["tri"] + off[i] for i, q in enumerate(above)])
            h, _t, _n = cover_heights(wp["stand"], apos, atri)
            hidden = np.isfinite(h) & (h < 0.002) & (h > -COVER_MM / 1000.0)
            loops = [(ch, cl) for ch, cl in loops if hidden[ch].mean() < 0.6]
        worst = (0.0, 0.0, 0.0, None)
        for chain, closed in loops:
            for pose in ("stand", "walk"):
                rag, teeth, length = loop_raggedness(wp[pose][chain], closed)
                if length < RAG_MIN_LOOP or len(chain) < 8:
                    continue
                if rag * teeth > worst[0] * worst[1] or worst[3] is None:
                    worst = (rag, teeth, length, chain)
        if worst[3] is not None:
            row["rag_mm"], row["teeth_per_m"] = round(worst[0], 2), round(worst[1], 1)
            row["rag_loop_cm"] = round(worst[2] * 100.0, 1)
            first = np.nonzero(np.isin(p["inv"], worst[3]))[0]
            bones = [p["bones"][i] for i in first] if p["bones"] else []
            bone = max(set(bones), key=bones.count) if bones else ""
            row["rag_at"] = f"{bone} h={wp['stand'][worst[3]][:, 1].mean():.2f}"
        seam = max(open_seams(wp[pose], loops, p["wtri"]) for pose in ("stand", "walk"))
        row["seam_cm"], row["seam_gap_mm"] = round(seam[0] * 100.0, 1), round(seam[1] * 1000.0, 1)

    # Layers: every garment, the skin and the caps against what lies above them.
    for pose in ("stand", "walk"):
        cover = [p for p in parts if p["role"] in ("garment", "cover")]
        for p in parts:
            if p["role"] == "cover":
                continue
            above = [q for q in cover if q is not p and q["rank"] > p["rank"]]
            if not above:
                continue
            pos = np.concatenate([q[pose] for q in above])
            off = np.cumsum([0] + [len(q[pose]) for q in above])
            tri = np.concatenate([q["tri"] + off[i] for i, q in enumerate(above)])
            owner = np.concatenate([np.full(len(q["tri"]), i) for i, q in enumerate(above)])
            lo, hi = p[pose].min(axis=0) - 0.08, p[pose].max(axis=0) + 0.08
            cen = pos[tri].mean(axis=1)
            keep = np.all((cen > lo) & (cen < hi), axis=1)
            tri, owner = tri[keep], owner[keep]
            if len(tri) == 0:
                continue
            nrm = vertex_normals(p[pose], p["tri"])
            row = rows[p["key"]]
            mask, mm, own, covered = poke(p[pose], nrm, pos, tri, owner)
            row["poke_" + pose] = int(mask.sum())
            row["poke_mm"] = round(max(row["poke_mm"], float(mm.max()) if len(mm) else 0.0), 1)
            if mask.any():
                if pose == "walk" or not row["poke_over"]:
                    row["poke_over"] = above[int(np.bincount(own[mask]).argmax())]["key"]
                    bones = [p["bones"][i] for i in np.nonzero(mask)[0]]
                    row["poke_at"] = max(sorted(set(bones)), key=bones.count)
                row["poke_share"] = round(max(row["poke_share"], mask.sum() / max(int(covered.sum()), 1)), 4)
            if p["role"] == "skin":
                holes, who = skin_holes(p[pose], nrm, pos, tri, owner)
                row["holes_" + pose] = int(holes.sum())
                for i in np.unique(who[holes]):
                    if i >= 0 and above[i]["key"] in rows:
                        rows[above[i]["key"]]["holes_" + pose] += int((who[holes] == i).sum())
    out = []
    for row in rows.values():
        fl, row["score"] = flags(row)
        row["flags"] = ";".join(fl)
        out.append(row)
    return out, note
