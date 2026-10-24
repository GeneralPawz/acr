"""The high-detail base human (m2-hd-human, opt-in per unit): the master first, LODs derived.

MakeHuman 1.x base mesh (CC0) at full resolution with its UVs, young male targets, reposed
onto skeleton H (`mbody.repose`), finger bones refitted to MakeHuman's finger joints, heat
weights with the finger joints sharpened, Catmull-Clark level 1, knuckles and palm creases,
then a symmetric collapse to about `MASTER_TRIS` with the UV seams locked; weights limited to
4 per vertex and normalised after every collapse (`limit_weights`), meshes parented to the
armature. Skin: the MakeHuman CC0
diffuse upscaled to 4k with the portrait blended into the face, a physically scaled
micro-normal (`micro_height`: grain, furrows and pores in millimetres over the real texel
size, a few degrees of normal tilt, not the old cobbled bump) and a roughness with an oily
T-zone. LODs come from the weighted base cage: a free collapse to the target (locking the
source seams cannot reach 800: they alone hold 1,122 vertices), then fresh UVs cut along the
source seams (`lod_seams`) and an angle-based unwrap, and normal and albedo baked from the
master into each LOD's own atlas (512 px for the 800 crowd level).

Sources are local-only (`ACR_SOURCES/makehuman/`: `data/3dobjs/base.obj`, the two macro
targets, `young_lightskinned_male_diffuse.png`; the portrait from `generated/faces`). The
numpy parts below need no `bpy` and are tested in `tests/test_hdbody.py`.

stats (`ctx.stats["hd"]`): `master_tris`, `lod_tris`, `uv_seam_integrity` (share of
triangles per mesh whose corners and centre lie in one UV island: for the master against the source atlas, for a LOD against its own), `uv_flipped`,
`uv_islands`, `unweighted_verts`, `weights` (per mesh: max influences, max sum error),
`lod_unwrap`, `lod_seam_edges`, `micro_normal_deg` (mean and p99 tilt), `finger_verts`.
"""

import math
import os

import numpy as np

MASTER_TRIS = 78000
LODS = ((10000, 2048), (3000, 1024), (800, 512))  # triangles, texture size
SKIN_AREA_M2 = 1.9  # adult male skin area
UV_FILL = 0.62  # share of the MakeHuman atlas covered by the body islands
FINGERS = ("thumb", "index", "middle", "ring", "pinky")
# The musket grip: curl about each finger bone's bend axis (degrees, per joint 1..3).
GRIP = {"thumb": (15, 25, 20), "index": (50, 65, 40), "middle": (60, 75, 45), "ring": (62, 78, 45),
        "pinky": (65, 80, 45)}
GRIP_CLIPS_SKIP = ("DEATH", "DEATHLIE1")


# ----------------------------------------------------------------------------- skin maps

def texel_mm(n, area_m2=SKIN_AREA_M2, fill=UV_FILL):
    """Edge of one texel on the skin in millimetres for an n x n atlas."""
    return math.sqrt(area_m2 / (fill * n * n)) * 1000.0


def _blur(a, sigma):
    """Periodic gaussian blur by FFT (keeps the map tileable)."""
    fy = np.fft.fftfreq(a.shape[0])[:, None]
    fx = np.fft.fftfreq(a.shape[1])[None, :]
    g = np.exp(-2 * (np.pi * sigma) ** 2 * (fx ** 2 + fy ** 2))
    return np.real(np.fft.ifft2(np.fft.fft2(a) * g))


def _unit(a):
    a = a - a.mean()
    return a / (a.std() + 1e-12)


def micro_height(n, tex_mm, seed=306):
    """Skin micro relief in millimetres on an n x n tileable grid with texels of `tex_mm`:
    grain (0.004 mm), the furrow network between skin plates (about 0.8 mm cells, 0.012 mm
    deep) and pores (about 0.1 mm wide, 0.02 mm deep, ~1 per mm^2 on the face, sparser on
    the body: here an average). Below a texel the features average out, as they should."""
    rng = np.random.default_rng(seed)
    grain = _unit(_blur(rng.standard_normal((n, n)), 0.7)) * 0.004
    cell = max(0.8 / tex_mm, 1.0)
    furrow = -np.abs(_unit(_blur(rng.standard_normal((n, n)), cell * 0.5)))
    furrow = _unit(furrow) * 0.012
    pits = np.zeros((n, n))
    count = int(n * n * tex_mm * tex_mm * 0.6)
    idx = rng.integers(0, n, size=(2, count))
    pits[idx[0], idx[1]] = 1.0
    pore_w = max(0.1 / tex_mm, 0.6)
    pits = _blur(pits, pore_w) * (2 * math.pi * pore_w * pore_w)  # peak ~1 for an isolated pore
    return grain + furrow - 0.02 * pits


def height_to_normal(h_mm, tex_mm):
    """Tangent-space normals (n, n, 3), +Y up in the image, from a height field in mm."""
    dx = (np.roll(h_mm, -1, 1) - np.roll(h_mm, 1, 1)) / (2 * tex_mm)
    dy = (np.roll(h_mm, -1, 0) - np.roll(h_mm, 1, 0)) / (2 * tex_mm)
    nrm = np.stack([-dx, dy, np.ones_like(h_mm)], -1)
    return nrm / np.linalg.norm(nrm, axis=-1, keepdims=True)


def tilt_deg(nrm):
    """(mean, p99) angle of the normals from +Z in degrees."""
    a = np.degrees(np.arccos(np.clip(nrm[..., 2], -1, 1)))
    return float(a.mean()), float(np.percentile(a, 99))


def albedo(src_rgb, n, h_mm, seed=307):
    """The diffuse upscaled to n (by the caller) with faint low-frequency mottling and
    a hint of the furrows (cavity), all within a few percent."""
    rng = np.random.default_rng(seed)
    mott = _unit(_blur(rng.standard_normal((n, n)), n / 170.0))[..., None]
    cav = np.clip(-h_mm / 0.02, 0, 1)[..., None]
    out = src_rgb * (1.0 + 0.025 * mott * np.array([1.0, 0.96, 0.95])) * (1.0 - 0.03 * cav)
    return np.clip(out, 0, 1)


def roughness(base, h_mm):
    """Roughness = the zone map (`zone_roughness`, baked) plus pores and furrows a little
    rougher: dull in the dips, the plates' tops a touch glossier."""
    return np.clip(base + 0.04 * np.tanh(-h_mm / 0.01), 0.2, 0.85)


def zone_roughness(p, eye, eye_sep):
    """Per point (m, armature space) roughness: 0.58 on the body, 0.40 on the T-zone
    (forehead centre, nose, chin), 0.34 on the lips. `eye`: centre between the eyes."""
    p = np.asarray(p, float)
    r = np.full(len(p), 0.58)
    s = eye_sep / 0.064
    dx = np.abs(p[:, 0] - eye[0]) / s
    dz = (p[:, 2] - eye[2]) / s
    front = p[:, 1] < eye[1] + 0.03 * s

    def soft(d, w):
        return np.clip(1.0 - d / w, 0, 1) ** 2 * (3 - 2 * np.clip(1.0 - d / w, 0, 1)) ** 0

    forehead = soft(np.hypot(dx / 0.030, (dz - 0.045) / 0.035), 1.0)
    nose = soft(np.hypot(dx / 0.016, (dz + 0.030) / 0.040), 1.0)
    chin = soft(np.hypot(dx / 0.022, (dz + 0.105) / 0.018), 1.0)
    lips = soft(np.hypot(dx / 0.024, (dz + 0.075) / 0.008), 1.0)
    t = np.maximum.reduce([forehead, nose, chin]) * front
    r = r - 0.18 * t
    r = np.minimum(r, 0.58 - 0.24 * lips * front)
    return r


# ----------------------------------------------------------------------------- UV seams

def island_labels(tri_uv, tri_v):
    """UV islands of a triangle list: corners join when they share a vertex and its UV.
    tri_uv (t, 3, 2), tri_v (t, 3). Returns one label per triangle (0..k-1) and k."""
    t = len(tri_v)
    parent = np.arange(t)

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    first = {}
    q = np.round(np.asarray(tri_uv) * 1e5).astype(np.int64)
    for i in range(t):
        for c in range(3):
            key = (int(tri_v[i][c]), int(q[i, c, 0]), int(q[i, c, 1]))
            j = first.setdefault(key, i)
            if j != i:
                a, b = find(i), find(j)
                if a != b:
                    parent[max(a, b)] = min(a, b)
    roots = np.array([find(i) for i in range(t)])
    _u, lab = np.unique(roots, return_inverse=True)
    return lab, int(lab.max()) + 1 if t else 0


def island_raster(tri_uv, labels, res=512):
    """Label image (res x res) of the UV islands: -1 where no triangle covers a texel."""
    img = np.full((res, res), -1, np.int32)
    uv = np.asarray(tri_uv, float) * res
    for k in range(len(uv)):
        a, b, c = uv[k]
        lo = np.floor(np.minimum(np.minimum(a, b), c)).astype(int).clip(0, res - 1)
        hi = np.ceil(np.maximum(np.maximum(a, b), c)).astype(int).clip(0, res - 1)
        xs, ys = np.meshgrid(np.arange(lo[0], hi[0] + 1) + 0.5, np.arange(lo[1], hi[1] + 1) + 0.5)
        d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
        if abs(d) < 1e-12:
            continue
        l1 = ((b[1] - c[1]) * (xs - c[0]) + (c[0] - b[0]) * (ys - c[1])) / d
        l2 = ((c[1] - a[1]) * (xs - c[0]) + (a[0] - c[0]) * (ys - c[1])) / d
        m = (l1 >= -0.02) & (l2 >= -0.02) & (1 - l1 - l2 >= -0.02)
        sub = img[lo[1]:hi[1] + 1, lo[0]:hi[0] + 1]
        sub[m] = labels[k]
    return img


def seam_integrity(raster, tri_uv):
    """Share of triangles whose three corners (pulled 15 % towards the centre) and centre fall
    in one source UV island: 1.0 = no triangle spans a seam; a collapse across a seam makes
    triangles that smear one island over another (the mouth of the old 800 LOD)."""
    res = raster.shape[0]
    uv = np.asarray(tri_uv, float)
    if not len(uv):
        return 1.0
    cen = uv.mean(1, keepdims=True)
    pts = np.concatenate([cen, cen + 0.85 * (uv - cen)], 1)  # (t, 4, 2)
    ij = np.clip((pts * res).astype(int), 0, res - 1)
    lab = raster[ij[..., 1], ij[..., 0]]
    ok = (lab == lab[:, :1]).all(1) & (lab[:, 0] >= 0)
    return float(ok.mean())


def flipped_index(tri_uv):
    """Indices of the triangles wound against the majority in UV space (folded UVs)."""
    uv = np.asarray(tri_uv, float)
    if not len(uv):
        return np.zeros(0, int)
    s = (uv[:, 1, 0] - uv[:, 0, 0]) * (uv[:, 2, 1] - uv[:, 0, 1]) - (uv[:, 2, 0] - uv[:, 0, 0]) * (uv[:, 1, 1] - uv[:, 0, 1])
    pos, neg = int((s > 0).sum()), int((s < 0).sum())
    return np.nonzero(s < 0 if pos >= neg else s > 0)[0]


def uv_flipped(tri_uv):
    """Count of triangles wound against the majority in UV space (folded UVs)."""
    return len(flipped_index(tri_uv))


def seam_vertices(tri_uv, tri_v):
    """Vertex indices on a UV seam or a mesh border: a vertex with more than one UV, or on an
    edge used by one triangle only."""
    q = np.round(np.asarray(tri_uv) * 1e5).astype(np.int64)
    uvs = {}
    edges = {}
    for i, tv in enumerate(tri_v):
        for c in range(3):
            uvs.setdefault(int(tv[c]), set()).add((int(q[i, c, 0]), int(q[i, c, 1])))
            e = tuple(sorted((int(tv[c]), int(tv[(c + 1) % 3]))))
            edges[e] = edges.get(e, 0) + 1
    out = {v for v, s in uvs.items() if len(s) > 1}
    for (a, b), k in edges.items():
        if k == 1:
            out.update((a, b))
    return sorted(out)


def bary_uv(p, tri_p, tri_uv):
    """UV at point `p` from one triangle (corner positions (3, 3), corner UVs (3, 2)): `p`
    projected onto the triangle's plane, barycentric weights (extrapolated outside it)."""
    a, b, c = (np.asarray(x, float) for x in tri_p)
    v0, v1, v2 = b - a, c - a, np.asarray(p, float) - a
    d00, d01, d11 = v0 @ v0, v0 @ v1, v1 @ v1
    d20, d21 = v2 @ v0, v2 @ v1
    den = d00 * d11 - d01 * d01
    if abs(den) < 1e-18:
        return np.asarray(tri_uv, float).mean(0)
    v = (d11 * d20 - d01 * d21) / den
    w = (d00 * d21 - d01 * d20) / den
    t = np.asarray(tri_uv, float)
    return (1 - v - w) * t[0] + v * t[1] + w * t[2]


SEAM_JUMP = 0.04  # UV distance (atlas units) that marks a source seam between two LOD faces


def limit_influences(groups, limit=4):
    """[(group, weight)] of one vertex: the `limit` strongest, summing to 1 (the renderer
    skins 4 joints; the exporter would otherwise drop the weakest silently)."""
    g = sorted(((int(k), float(w)) for k, w in groups if w > 0), key=lambda t: (-t[1], t[0]))[:limit]
    s = sum(w for _k, w in g)
    return [(k, w / s) for k, w in g] if s > 0 else []


def chain_weights(s, joints, band):
    """Weights over a bone chain from the position `s` along it (one value per point):
    `joints` are the positions of the chain's joints (len = bones - 1) and `band` the half
    width of each blend; a narrow band gives a crisp finger joint instead of the soft
    heat-weight bend. Returns (points, bones)."""
    s = np.asarray(s, float)
    nb = len(joints) + 1
    w = np.zeros((len(s), nb))
    prev = np.ones(len(s))
    for j, (x, b) in enumerate(zip(joints, band)):
        t = np.clip((s - (x - b)) / (2 * b), 0, 1)
        t = t * t * (3 - 2 * t)
        w[:, j] = prev * (1 - t)
        prev = prev * t
    w[:, nb - 1] = prev
    return w / w.sum(1, keepdims=True)


# ----------------------------------------------------------------------------- Blender

def _sources():
    return os.environ.get("ACR_SOURCES", os.path.expanduser("~/sources"))


def _tri_arrays(me):
    """(tri_uv, tri_v) of a mesh's loop triangles on its first UV map."""
    me.calc_loop_triangles()
    lt = np.zeros(len(me.loop_triangles) * 3, np.int32)
    me.loop_triangles.foreach_get("loops", lt)
    lv = np.zeros(len(me.loop_triangles) * 3, np.int32)
    me.loop_triangles.foreach_get("vertices", lv)
    uv = np.zeros(len(me.loops) * 2, np.float32)
    me.uv_layers["UVMap"].data.foreach_get("uv", uv)
    uv = uv.reshape(-1, 2)
    return uv[lt].reshape(-1, 3, 2), lv.reshape(-1, 3)


def _tris(ob):
    return sum(len(p.vertices) - 2 for p in ob.data.polygons)


def load_makehuman(root):
    """Vertices (m, z up, facing -Y), UVs, faces [(group, verts, uvs)] and joint centres."""
    vs, vts, faces, g = [], [], [], None
    with open(os.path.join(root, "3dobjs", "base.obj")) as fh:
        for ln in fh:
            if ln.startswith("v "):
                vs.append([float(t) for t in ln.split()[1:4]])
            elif ln.startswith("vt "):
                vts.append([float(t) for t in ln.split()[1:3]])
            elif ln.startswith("g "):
                g = ln.split()[1]
            elif ln.startswith("f "):
                tok = [t.split("/") for t in ln.split()[1:]]
                faces.append((g, [int(t[0]) - 1 for t in tok], [int(t[1]) - 1 for t in tok]))
    vs = np.array(vs)
    for rel in ("macrodetails/caucasian-male-young.target",
                "macrodetails/universal-male-young-averagemuscle-averageweight.target"):
        with open(os.path.join(root, "targets", rel)) as fh:
            for ln in fh:
                if ln.startswith("#") or not ln.strip():
                    continue
                t = ln.split()
                vs[int(t[0])] += [float(t[1]), float(t[2]), float(t[3])]
    V = np.stack([vs[:, 0], -vs[:, 2], vs[:, 1]], 1) * 0.1
    groups = {}
    for gg, f, _t in faces:
        groups.setdefault(gg, set()).update(f)
    J = {gg[6:]: V[sorted(s)].mean(0) for gg, s in groups.items() if gg.startswith("joint-")}
    return V, vts, faces, J


def skin_maps(diffuse_path, n=4096):
    """(albedo, normal, roughness micro term) arrays at n; stats."""
    import bpy

    tex = texel_mm(n)
    h = micro_height(n, tex)
    nrm = height_to_normal(h, tex)
    im = bpy.data.images.load(diffuse_path)
    im.scale(n, n)  # Blender's own resampler (no PIL in Blender's Python)
    src = np.array(im.pixels[:], np.float32).reshape(n, n, 4)[::-1, :, :3]
    bpy.data.images.remove(im)
    alb = albedo(src, n, h)
    mean, p99 = tilt_deg(nrm)
    return alb, nrm, h, {"texel_mm": round(tex, 3), "micro_normal_deg": [round(mean, 2), round(p99, 2)]}


def _image(name, arr, non_color=False):
    import bpy

    n = arr.shape[0]
    img = bpy.data.images.new(name, n, n, alpha=False, float_buffer=False)
    if non_color:
        img.colorspace_settings.name = "Non-Color"
    if arr.ndim == 2:
        arr = np.repeat(arr[..., None], 3, -1)
    rgba = np.concatenate([arr[::-1], np.ones(arr.shape[:2] + (1,))], -1).astype(np.float32)
    img.pixels.foreach_set(rgba.ravel())
    img.pack()
    return img


def _cycles():
    import bpy

    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    prefs = bpy.context.preferences.addons["cycles"].preferences
    try:
        prefs.compute_device_type = "HIP"
        prefs.get_devices()
        for d in prefs.devices:
            d.use = True
        sc.cycles.device = "GPU"
    except Exception:
        sc.cycles.device = "CPU"
    sc.cycles.samples = 1
    sc.render.bake.margin = 8
    return sc


def _emit_bake(ob, img, build_nodes):
    """Bakes an emission shader made by `build_nodes(nt) -> socket` into `img`."""
    import bpy

    mat = bpy.data.materials.new(f"bake.{img.name}")
    mat.use_nodes = True
    nt = mat.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(build_nodes(nt), em.inputs[0])
    nt.links.new(em.outputs[0], out.inputs[0])
    tgt = nt.nodes.new("ShaderNodeTexImage")
    tgt.image = img
    nt.nodes.active = tgt
    keep = list(ob.data.materials)
    ob.data.materials.clear()
    ob.data.materials.append(mat)
    bpy.ops.object.select_all(action="DESELECT")
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob
    bpy.context.scene.render.bake.use_selected_to_active = False
    bpy.ops.object.bake(type="EMIT")
    ob.data.materials.clear()
    for m in keep:
        ob.data.materials.append(m)
    bpy.data.materials.remove(mat)


def skin_material(name, alb, nrm=None, rough=None, strength=1.0):
    import bpy

    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    bs = nt.nodes["Principled BSDF"]
    ta = nt.nodes.new("ShaderNodeTexImage")
    ta.image = alb
    nt.links.new(ta.outputs[0], bs.inputs["Base Color"])
    if rough is not None:
        tr = nt.nodes.new("ShaderNodeTexImage")
        tr.image = rough
        nt.links.new(tr.outputs[0], bs.inputs["Roughness"])
    if nrm is not None:
        tn = nt.nodes.new("ShaderNodeTexImage")
        tn.image = nrm
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nm.inputs["Strength"].default_value = strength
        nt.links.new(tn.outputs[0], nm.inputs["Color"])
        nt.links.new(nm.outputs[0], bs.inputs["Normal"])
    bs.inputs["Subsurface Weight"].default_value = 0.15
    bs.inputs["Subsurface Radius"].default_value = (1.0, 0.35, 0.2)
    bs.inputs["Subsurface Scale"].default_value = 0.01
    return m


def _refit_fingers(arm, QJ):
    """Finger bones onto MakeHuman's finger joints; roll so local +Z is the back of the hand
    (checked against the thumb, which sits on the palm side of the knuckle line)."""
    import bpy

    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    for s, sfx in (("l", "_l"), ("r", "_r")):
        knuck = QJ[f"{s}-finger-5-1"] - QJ[f"{s}-finger-2-1"]
        mid = 0.5 * (QJ[f"{s}-finger-5-1"] + QJ[f"{s}-finger-2-1"])
        thumb = QJ[f"{s}-finger-1-2"] - mid
        for k, f in enumerate(FINGERS, start=1):
            for j in range(1, 4):
                b = eb.get(f"{f}_0{j}{sfx}")
                if b is None:
                    continue
                b.use_connect = False
                b.head = QJ[f"{s}-finger-{k}-{j}"]
                b.tail = QJ[f"{s}-finger-{k}-{j + 1}"]
                d = np.array(b.tail - b.head)
                z = np.cross(d, knuck)
                if f != "thumb" and float(np.dot(z, thumb)) > 0:
                    z = -z
                if f == "thumb":
                    z = np.cross(d, np.array(QJ[f"{s}-finger-2-1"] - QJ[f"{s}-finger-1-1"]))
                    z = z if float(np.dot(z, np.cross(knuck, d))) >= 0 else -z
                b.align_roll(z)
    bpy.ops.object.mode_set(mode="OBJECT")


def _sharpen_fingers(body, arm):
    """Finger vertices re-weighted along their chain (hand, 01, 02, 03) with a narrow blend
    at each joint, so a curled finger bends at the knuckle instead of along its length."""
    vgs = {vg.name: vg for vg in body.vertex_groups}
    me = body.data
    co = np.array([v.co for v in me.vertices])
    count = 0
    for sfx in ("_l", "_r"):
        hand = f"hand{sfx}"
        chains = []
        for f in FINGERS:
            bones = [arm.data.bones.get(f"{f}_0{j}{sfx}") for j in (1, 2, 3)]
            if any(b is None for b in bones):
                continue
            chains.append((f, bones))
        fing_idx = {vgs[b.name].index for _f, bs in chains for b in bs if b.name in vgs}
        hand_idx = vgs[hand].index if hand in vgs else None
        for v in me.vertices:
            wf = sum(g.weight for g in v.groups if g.group in fing_idx)
            if wf < 0.25:
                continue
            p = co[v.index]
            best = None
            for f, bs in chains:
                for b in bs:
                    a, c = np.array(b.head_local), np.array(b.tail_local)
                    d = c - a
                    t = np.clip(np.dot(p - a, d) / np.dot(d, d), 0, 1)
                    dist = np.linalg.norm(p - (a + t * d))
                    if best is None or dist < best[0]:
                        best = (dist, f, bs)
            _d, f, bs = best
            a = np.array(bs[0].head_local)
            axis = np.array(bs[-1].tail_local) - a
            L = np.linalg.norm(axis)
            axis = axis / L
            s = float(np.dot(p - a, axis))
            joints = [0.0] + [float(np.dot(np.array(b.head_local) - a, axis)) for b in bs[1:]]
            seglen = [bs[0].length] + [b.length for b in bs]
            band = [max(0.18 * min(seglen[i], seglen[i + 1]), 0.003) for i in range(3)]
            w = chain_weights([s], joints, band)[0]
            for g in list(v.groups):
                body.vertex_groups[g.group].remove([v.index])
            for name, wt in zip([hand] + [b.name for b in bs], w):
                if wt > 1e-4 and name in vgs:
                    vgs[name].add([v.index], float(wt), "REPLACE")
            count += 1
    return count


def _knuckles(body, arm):
    """Knuckle bumps on the back of the hand and finger joints, creases on the palm side."""
    me = body.data
    co = np.array([v.co for v in me.vertices])
    no = np.array([v.normal for v in me.vertices])
    off = np.zeros(len(co))
    for sfx in ("_l", "_r"):
        for f in FINGERS[1:]:
            for j, (bump, crease) in zip((1, 2, 3), ((0.0022, 0.0009), (0.0014, 0.0008), (0.0009, 0.0006))):
                b = arm.data.bones[f"{f}_0{j}{sfx}"]
                h = np.array(b.head_local)
                m = b.matrix_local.to_3x3()
                y = np.array(m.col[1])
                z = np.array(m.col[2])
                rel = co - h
                r = np.linalg.norm(rel, axis=1)
                along = rel @ y
                dors = no @ z
                k = np.clip(1 - r / 0.010, 0, 1) ** 2
                off += bump * k * np.clip(dors, 0, 1) * np.exp(-(along / 0.004) ** 2)
                band = np.exp(-(along / 0.0012) ** 2) * (r < 0.012)
                off -= crease * band * np.clip(-dors, 0, 1)
    co = co + no * off[:, None]
    me.vertices.foreach_set("co", co.ravel())
    me.update()
    return int((np.abs(off) > 1e-4).sum())


def grip_spec(arm, sides=("_l", "_r")):
    """Pose spec (armature-axes Euler degrees, `armature.pose`) curling the fingers round a
    musket stock: each finger bone turned about its own bend axis (local X; +Z is the back of
    the hand, so the curl is negative)."""
    from mathutils import Matrix

    out = {}
    for sfx in sides:
        for f, angles in GRIP.items():
            for j, ang in zip((1, 2, 3), angles):
                b = arm.data.bones.get(f"{f}_0{j}{sfx}")
                if b is None:
                    continue
                rest = b.matrix_local.to_3x3()
                rel = (b.parent.matrix_local.to_3x3().inverted() @ rest) if b.parent else rest
                # `armature.pose`: q = rest^-1 @ euler(r) @ rest with rest the bone's
                # armature-space orientation; we want q = rotation about local X.
                loc = Matrix.Rotation(math.radians(-ang), 3, "X")
                e = (rest @ loc @ rest.inverted()).to_euler("XYZ")
                del rel
                out[b.name] = tuple(round(math.degrees(a), 3) for a in e)
    return out


def build_master(ctx, arm, portrait=None, master_tris=MASTER_TRIS):
    """The weighted master body and eyes under `arm` (skeleton H, rest pose); returns
    (body, eyes, cage). Fills ctx.stats["hd"]."""
    import bpy

    from . import mbody

    st = ctx.stats.setdefault("hd", {})
    root = os.path.join(_sources(), "makehuman")
    V, vts, faces, J = load_makehuman(os.path.join(root, "data"))
    jn = sorted(J)
    seg = {b.name: (b.head_local.copy(), b.tail_local.copy()) for b in arm.data.bones}
    VJ = np.concatenate([V, np.array([J[k] for k in jn])])
    saved = mbody._cache.get("d")
    mbody._cache["d"] = {"V": VJ, "J": J, "faces": []}
    Q, _ = mbody.repose(seg)
    if saved is not None:
        mbody._cache["d"] = saved
    else:
        mbody._cache.pop("d", None)
    QV, QJ = Q[:len(V)], dict(zip(jn, Q[len(V):]))

    def make(name, keep):
        sel = [(f, t) for gg, f, t in faces if gg in keep]
        used = sorted({i for f, _t in sel for i in f})
        remap = {o: n for n, o in enumerate(used)}
        me = bpy.data.meshes.new(name)
        me.from_pydata([tuple(QV[i]) for i in used], [], [[remap[i] for i in f] for f, _t in sel])
        uv = me.uv_layers.new(name="UVMap")
        uv.data.foreach_set("uv", np.array([vts[i] for _f, t in sel for i in t], np.float32).ravel())
        me.validate()
        ob = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(ob)
        ob.parent = ctx.root
        for p in me.polygons:
            p.use_smooth = True
        return ob

    body = make(f"{ctx.name}.hd.body", {"body"})
    eyes = make(f"{ctx.name}.hd.eyes", {"helper-l-eye", "helper-r-eye"})
    _refit_fingers(arm, QJ)
    for b in arm.data.bones:
        b.use_deform = b.use_deform and not b.name.startswith("socket.")
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")  # parented to the armature (glTF skin)
    st["finger_verts"] = _sharpen_fingers(body, arm)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.mode_set(mode="WEIGHT_PAINT")
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(group_select_mode="ALL", lock_active=False)
    bpy.ops.object.mode_set(mode="OBJECT")
    st["unweighted_verts"] = sum(1 for v in body.data.vertices if not v.groups)

    cage = body.copy()
    cage.data = body.data.copy()
    cage.name = f"{ctx.name}.hd.cage"
    bpy.context.scene.collection.objects.link(cage)
    st["cage_tris"] = _tris(cage)

    for mm in list(body.modifiers):
        body.modifiers.remove(mm)
    m = body.modifiers.new("sub", "SUBSURF")
    m.levels = m.render_levels = 1
    m.uv_smooth = "PRESERVE_BOUNDARIES"
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.modifier_apply(modifier="sub")
    st["knuckle_verts"] = _knuckles(body, arm)
    st["uv_flipped_subsurf"] = uv_flipped(_tri_arrays(body.data)[0])
    st["uv_flipped_collapse"] = _collapse_unfolded(body, master_tris)
    for p in body.data.polygons:
        p.use_smooth = True
    for ob in (body, cage):
        am_ = ob.modifiers.get("Armature") or ob.modifiers.new("Armature", "ARMATURE")
        am_.object = arm
    vg = eyes.vertex_groups.new(name="head")
    vg.add(list(range(len(eyes.data.vertices))), 1.0, "REPLACE")
    eyes.modifiers.new("Armature", "ARMATURE").object = arm
    eyes.parent = arm
    eyes.matrix_parent_inverse = arm.matrix_world.inverted()
    st["master_tris"] = _tris(body)
    st["weights"] = {"master": limit_weights(body)}

    # ---- skin maps (albedo with the portrait, micro-normal, zone roughness)
    alb, nrm, h, ms = skin_maps(os.path.join(root, "young_lightskinned_male_diffuse.png"))
    st.update(ms)
    alb_img = _image("hd_albedo_src", alb)
    nrm_img = _image("hd_micro_normal_4k", nrm * 0.5 + 0.5, non_color=True)
    eye = 0.5 * (QJ["l-eye"] + QJ["r-eye"])
    sep = float(np.linalg.norm(QJ["l-eye"] - QJ["r-eye"]))
    sc = _cycles()
    me = body.data
    co = np.array([v.co for v in me.vertices])
    zr = zone_roughness(co, eye, sep)
    ca = me.color_attributes.new("zone", "FLOAT_COLOR", "POINT")
    ca.data.foreach_set("color", np.stack([zr, zr, zr, np.ones_like(zr)], 1).astype(np.float32).ravel())
    zimg = bpy.data.images.new("hd_zone", 4096, 4096)
    zimg.colorspace_settings.name = "Non-Color"

    def zone_nodes(nt):
        a = nt.nodes.new("ShaderNodeAttribute")
        a.attribute_name = "zone"
        return a.outputs["Color"]

    _emit_bake(body, zimg, zone_nodes)
    zr_img = np.array(zimg.pixels[:], np.float32).reshape(4096, 4096, 4)[::-1, :, 0]
    rough_img = _image("hd_rough_4k", roughness(zr_img, h), non_color=True)
    me.color_attributes.remove(me.color_attributes["zone"])
    st["roughness_t_zone"] = round(float(zr.min()), 3)

    final = alb_img
    if portrait and os.path.exists(portrait):
        final = _portrait_bake(body, eyes, alb_img, portrait, QJ, st)
    skin = skin_material(f"{ctx.name}.hd.skin", final, nrm_img, rough_img, 1.0)
    body.data.materials.append(skin)
    cage.data.materials.append(skin)
    em = bpy.data.materials.new(f"{ctx.name}.hd.eye")
    em.use_nodes = True
    em.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.85, 0.83, 0.8, 1)
    em.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.08
    eyes.data.materials.append(em)
    del sc
    tri_uv, tri_v = _tri_arrays(cage.data)
    lab, k = island_labels(tri_uv, tri_v)
    st["uv_islands"] = k
    raster = island_raster(tri_uv, lab)
    mt_uv, _mv = _tri_arrays(body.data)
    st["uv_seam_integrity"] = {"master": round(seam_integrity(raster, mt_uv), 4)}
    st["uv_flipped"] = {"master": uv_flipped(mt_uv)}
    ctx._hd_raster = raster
    return body, eyes, cage


def _portrait_bake(body, eyes, alb_img, portrait, QJ, st):
    """The aligned portrait projected frontally onto the face and blended into the albedo."""
    import bpy

    eye_z = 0.5 * (QJ["l-eye"][2] + QJ["r-eye"][2])
    cx = 0.5 * (QJ["l-eye"][0] + QJ["r-eye"][0])
    eye_y = 0.5 * (QJ["l-eye"][1] + QJ["r-eye"][1])
    BOX = (-0.11, 0.11, -0.16, 0.06)
    CZ, AX, FADE = -0.035, (0.066, 0.098), 0.72
    me = body.data
    co = np.array([v.co for v in me.vertices])
    no = np.array([v.normal for v in me.vertices])
    u = (co[:, 0] - cx - BOX[0]) / (BOX[1] - BOX[0])
    v = (co[:, 2] - eye_z - BOX[2]) / (BOX[3] - BOX[2])
    li = np.zeros(len(me.loops), np.int32)
    me.loops.foreach_get("vertex_index", li)
    pl = me.uv_layers.new(name="proj")
    pl.data.foreach_set("uv", np.stack([u[li], v[li]], 1).astype(np.float32).ravel())
    me.uv_layers["UVMap"].active = True
    me.uv_layers["UVMap"].active_render = True
    r = np.hypot((co[:, 0] - cx) / AX[0], (co[:, 2] - eye_z - CZ) / AX[1])
    k = np.clip((1.0 - r) / (1.0 - FADE), 0, 1)
    k = k * k * (3 - 2 * k)
    w = (k * np.clip((-no[:, 1] - 0.15) / 0.35, 0, 1) * (co[:, 1] < eye_y + 0.04)).astype(np.float32)
    a = me.color_attributes.new("pmask", "FLOAT_COLOR", "POINT")
    a.data.foreach_set("color", np.stack([w, w, w, np.ones_like(w)], 1).ravel())
    por = bpy.data.images.load(portrait)
    out = bpy.data.images.new("hd_albedo_4k", 4096, 4096)

    def nodes(nt):
        t1 = nt.nodes.new("ShaderNodeTexImage")
        t1.image = alb_img
        u1 = nt.nodes.new("ShaderNodeUVMap")
        u1.uv_map = "UVMap"
        t2 = nt.nodes.new("ShaderNodeTexImage")
        t2.image = por
        t2.extension = "EXTEND"
        u2 = nt.nodes.new("ShaderNodeUVMap")
        u2.uv_map = "proj"
        at = nt.nodes.new("ShaderNodeAttribute")
        at.attribute_name = "pmask"
        mx = nt.nodes.new("ShaderNodeMix")
        mx.data_type = "RGBA"
        nt.links.new(u1.outputs[0], t1.inputs[0])
        nt.links.new(u2.outputs[0], t2.inputs[0])
        nt.links.new(at.outputs["Fac"], mx.inputs["Factor"])
        nt.links.new(t1.outputs[0], mx.inputs["A"])
        nt.links.new(t2.outputs[0], mx.inputs["B"])
        return mx.outputs["Result"]

    _emit_bake(body, out, nodes)
    out.pack()
    me.uv_layers.remove(me.uv_layers["proj"])
    me.color_attributes.remove(me.color_attributes["pmask"])
    st["portrait"] = os.path.basename(portrait)
    return out


def seam_lock(ob):
    """Vertex group `lock`: 0 on UV seams and mesh borders, 1 elsewhere (the decimator's
    vertex-group weight makes a collapse touching a 0 vertex very costly)."""
    tri_uv, tri_v = _tri_arrays(ob.data)
    seam = set(seam_vertices(tri_uv, tri_v))
    vg = ob.vertex_groups.new(name="lock")
    vg.add([i for i in range(len(ob.data.vertices)) if i not in seam], 1.0, "REPLACE")
    vg.add(sorted(seam), 0.0, "REPLACE")
    return vg, len(seam)


def _collapse_unfolded(body, target, rounds=4, radius=0.012):
    """Symmetric collapse of `body` to about `target` triangles with the UV seams locked;
    where a collapse folds the UVs, the vertices round the fold are locked as well and the
    collapse is redone from the subdivided mesh. Returns the fold count per round."""
    import bpy
    from mathutils.kdtree import KDTree

    src = body.data.copy()
    kd = KDTree(len(src.vertices))
    for v in src.vertices:
        kd.insert(v.co, v.index)
    kd.balance()
    extra, history = set(), []
    for _r in range(rounds):
        if _r:
            old = body.data
            body.data = src.copy()
            bpy.data.meshes.remove(old)
        vg, _n = seam_lock(body)
        if extra:
            vg.add(sorted(extra), 0.0, "REPLACE")
        bpy.context.view_layer.objects.active = body
        d = body.modifiers.new("dec", "DECIMATE")
        d.ratio = min(1.0, target / max(_tris(body), 1))
        d.use_symmetry = True
        d.symmetry_axis = "X"
        d.vertex_group = vg.name
        d.vertex_group_factor = 1000.0
        bpy.ops.object.modifier_apply(modifier="dec")
        body.vertex_groups.remove(body.vertex_groups["lock"])
        tri_uv, tri_v = _tri_arrays(body.data)
        bad = flipped_index(tri_uv)
        history.append(len(bad))
        if not len(bad):
            break
        co = body.data.vertices
        for i in bad:
            for vi in tri_v[int(i)]:
                p = co[int(vi)].co
                for _c, j, _d in kd.find_range(p, radius):
                    extra.add(j)
                    # the mirror side too (the collapse is symmetric)
                for _c, j, _d in kd.find_range((-p.x, p.y, p.z), radius):
                    extra.add(j)
    bpy.data.meshes.remove(src)
    return history


def limit_weights(ob, limit=4):
    """At most `limit` influences per vertex, normalised (`limit_influences`); returns the
    measured {max_influences, max_sum_error} after."""
    vgs = ob.vertex_groups
    for v in ob.data.vertices:
        cur = [(g.group, g.weight) for g in v.groups]
        new = limit_influences(cur, limit)
        keep = {k for k, _w in new}
        for k, _w in cur:
            if k not in keep:
                vgs[k].remove([v.index])
        for k, w in new:
            vgs[k].add([v.index], w, "REPLACE")
    most, err = 0, 0.0
    for v in ob.data.vertices:
        ws = [g.weight for g in v.groups if g.weight > 0]
        most = max(most, len(ws))
        err = max(err, abs(sum(ws) - 1.0) if ws else 1.0)
    return {"max_influences": most, "max_sum_error": round(err, 5)}


def lod_seams(ob, cage):
    """Marks as UV seams the LOD edges where the source atlas jumps: each LOD face takes the
    UVs of its nearest cage triangle, and an edge whose midpoint gets UVs further apart than
    `SEAM_JUMP` from its two faces lies on a source seam. Mesh borders are seams too.
    Returns the seam edge count."""
    import bmesh
    from mathutils.bvhtree import BVHTree

    cme = cage.data
    tri_uv, tri_v = _tri_arrays(cme)
    cp = np.array([v.co for v in cme.vertices])
    bvh = BVHTree.FromPolygons([v.co.copy() for v in cme.vertices], [tuple(int(i) for i in t) for t in tri_v])
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    near = {}
    for f in bm.faces:
        near[f.index] = bvh.find_nearest(f.calc_center_median())[2]
    n = 0
    for e in bm.edges:
        lf = list(e.link_faces)
        seam = len(lf) != 2
        if not seam:
            a, b = near[lf[0].index], near[lf[1].index]
            if a != b:
                m = np.array((e.verts[0].co + e.verts[1].co) * 0.5)
                ua = bary_uv(m, cp[tri_v[a]], tri_uv[a])
                ub = bary_uv(m, cp[tri_v[b]], tri_uv[b])
                seam = float(np.linalg.norm(ua - ub)) > SEAM_JUMP
        e.seam = seam
        n += seam
    bm.to_mesh(ob.data)
    bm.free()
    return n


def _unwrap(ob, px):
    """Fresh UVs for a LOD: angle-based unwrap along `lod_seams`, packed with a margin of a
    few texels; folded faces are cut free and unwrapped again (up to 3 times), else a smart
    projection. Returns the method used."""
    import bpy

    bpy.ops.object.select_all(action="DESELECT")
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob
    margin = 6.0 / px
    for method in ("ANGLE_BASED",) * 4 + ("SMART",):
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        if method == "SMART":
            bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=margin)
        else:
            bpy.ops.uv.unwrap(method=method, margin=margin)
        bpy.ops.object.mode_set(mode="OBJECT")
        me = ob.data
        tri_uv, _tv = _tri_arrays(me)
        bad = flipped_index(tri_uv)
        if not len(bad):
            break
        # Cut the folded faces and their neighbours free (they sit where a chart wraps
        # round a limb the seam detection left closed) and unwrap again.
        polys = {me.loop_triangles[int(i)].polygon_index for i in bad}
        verts = {v for p in polys for v in me.polygons[p].vertices}
        for e in me.edges:
            if e.vertices[0] in verts or e.vertices[1] in verts:
                e.use_seam = True
    return method


def build_lods(ctx, body, cage, arm, lods=LODS):
    """LOD meshes from the cage: a free symmetric collapse to the target, fresh UVs cut along
    the source seams (`lod_seams`, `_unwrap`), weights limited to 4, then normal and albedo
    baked from the master into each LOD's own atlas. Returns the LOD objects (rest pose
    required). `uv_seam_integrity` / `uv_flipped` of a LOD are measured on its own atlas."""
    import bpy

    st = ctx.stats["hd"]
    sc = _cycles()
    out = []
    st["lod_tris"] = {}
    st["lod_seam_edges"] = {}
    st["lod_unwrap"] = {}
    mods = [(o, m) for o in (body, cage) for m in o.modifiers if m.type == "ARMATURE"]
    for _o, m in mods:
        m.show_render = m.show_viewport = False
    for tris, px in lods:
        ob = cage.copy()
        ob.data = cage.data.copy()
        ob.name = f"{ctx.name}.hd.lod{tris}"
        bpy.context.scene.collection.objects.link(ob)
        for m in list(ob.modifiers):
            ob.modifiers.remove(m)
        bpy.context.view_layer.objects.active = ob
        d = ob.modifiers.new("dec", "DECIMATE")
        d.ratio = tris / _tris(ob)
        d.use_symmetry = True
        d.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier="dec")
        for p in ob.data.polygons:
            p.use_smooth = True
        st["lod_seam_edges"][str(tris)] = lod_seams(ob, cage)
        st["lod_unwrap"][str(tris)] = _unwrap(ob, px)
        st["weights"][str(tris)] = limit_weights(ob)
        st["lod_tris"][str(tris)] = _tris(ob)
        tri_uv, tri_v = _tri_arrays(ob.data)
        lab, _k = island_labels(tri_uv, tri_v)
        st["uv_seam_integrity"][str(tris)] = round(seam_integrity(island_raster(tri_uv, lab, px), tri_uv), 4)
        st["uv_flipped"][str(tris)] = uv_flipped(tri_uv)
        sc.render.bake.margin = max(2, px // 128)
        nimg = bpy.data.images.new(f"lod{tris}_normal", px, px)
        nimg.colorspace_settings.name = "Non-Color"
        aimg = bpy.data.images.new(f"lod{tris}_albedo", px, px)
        mat = body.data.materials[0].copy()
        mat.name = f"{ctx.name}.hd.skin.lod{tris}"
        nt = mat.node_tree
        tgt = nt.nodes.new("ShaderNodeTexImage")
        nt.nodes.active = tgt
        ob.data.materials.clear()
        ob.data.materials.append(mat)
        bpy.ops.object.select_all(action="DESELECT")
        body.select_set(True)
        ob.select_set(True)
        bpy.context.view_layer.objects.active = ob
        sc.render.bake.use_selected_to_active = True
        sc.render.bake.cage_extrusion = 0.012
        sc.render.bake.max_ray_distance = 0.04
        tgt.image = nimg
        bpy.ops.object.bake(type="NORMAL")
        tgt.image = aimg
        sc.render.bake.use_pass_direct = False
        sc.render.bake.use_pass_indirect = False
        sc.render.bake.use_pass_color = True
        bpy.ops.object.bake(type="DIFFUSE")
        nt.nodes.remove(tgt)
        nimg.pack()
        aimg.pack()
        lm = skin_material(f"{ctx.name}.hd.skin.lod{tris}", aimg, nimg, None, 1.0)
        lm.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.55
        ob.data.materials.clear()
        ob.data.materials.append(lm)
        bpy.data.materials.remove(mat)
        ob.modifiers.new("Armature", "ARMATURE").object = arm
        ob.parent = arm
        ob.matrix_parent_inverse = arm.matrix_world.inverted()
        ob["acr_lod_of"] = body.name
        ob["acr_lod_tris"] = tris
        out.append(ob)
    for _o, m in mods:
        m.show_render = m.show_viewport = True
    sc.render.bake.margin = 8
    bpy.data.objects.remove(cage)
    return out
