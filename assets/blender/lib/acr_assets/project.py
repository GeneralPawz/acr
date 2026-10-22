"""Colour from the sprites: projects the stored sprite views through the original camera
onto a model (needs `bpy`, `numpy`).

Every face takes its colours from the view that sees it most frontally among the views in
which it is not hidden (a ray from its centre or from a point near one of its corners
towards the camera hits nothing). The views are packed side by side into one atlas image
(UV map `sprite`, nearest filtering); transparent pixels are filled from their neighbours.
Faces no view sees get a flat colour: the dominant sprite colour of their object's seen
faces (`dominant`). Units use only that dominant colour per part (`humans.Character.colours`):
a 60-pixel sprite is too coarse to texture a model that can be zoomed and turned.

Options for units and other lit models:

- **Posed models:** the projection uses the evaluated geometry (an armature in the pose of
  the reference animation, e.g. `STAND`) and writes the UVs onto the rest mesh.
- **Mirrored directions** (`mirror`): units store only the west half (9 of 16 directions);
  the game draws the east half mirrored. For objects listed in `mirror` (the symmetric
  ones: body, clothes) the mirror images of the stored views count as extra views at
  `RealDir` 128 - d, taken from the same atlas pixels with a flipped u. Asymmetric gear
  (a musket in the right hand) must not use them.
- **Light removal** (`delight`): the sprites carry the original lighting (from the upper
  left), which a lit renderer would add a second time. The light direction and per-object
  diffuse/ambient ratio are fitted to the sprite's luminance against the model's normals
  (ray-cast per pixel, smooth normals), and the atlas is divided by that shading raised to
  `delight` (0 = keep, 1 = full removal; clamped).

Limits: a face is visible or hidden as a whole, so the hidden part of a partly covered face
gets the occluder's colours; mirrored views are exact only for symmetric parts.
"""

import math

import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree
from mathutils.interpolate import poly_3d_calc

from . import conventions as cv
from . import materials

# Faces at a flatter angle to the view than this (cosine) take no colour from it.
MIN_FACING = 0.05
# Rounds of edge dilation that push sprite colours into transparent pixels.
DILATE = 12
# A mirrored view must face a face this much better than a stored one to be chosen.
MIRROR_PENALTY = 0.8


def load(path):
    """RGBA float pixels of a PNG, top row first (values as stored, i.e. sRGB)."""
    img = bpy.data.images.load(path, check_existing=False)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1].copy()
    bpy.data.images.remove(img)
    return px


def save(path, px):
    """Writes RGBA float pixels (top row first) as PNG."""
    h, w = px.shape[:2]
    img = bpy.data.images.new("save", w, h, alpha=True)
    img.pixels.foreach_set(np.ascontiguousarray(px[::-1]).ravel())
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()
    bpy.data.images.remove(img)


def dilate(px, rounds=DILATE):
    """Fills transparent pixels with the mean of their solid neighbours, `rounds` times,
    then with the mean colour; returns an opaque image."""
    rgb = px[..., :3].copy()
    solid = px[..., 3] >= 0.5
    if not solid.any():
        return np.dstack([rgb, np.ones(solid.shape, np.float32)])
    for _ in range(rounds):
        acc = np.zeros_like(rgb)
        cnt = np.zeros(solid.shape, np.float32)
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            s = np.roll(solid, (dy, dx), (0, 1))
            acc += np.roll(rgb, (dy, dx), (0, 1)) * s[..., None]
            cnt += s
        grow = ~solid & (cnt > 0)
        rgb[grow] = acc[grow] / cnt[grow][:, None]
        solid = solid | grow
    rgb[~solid] = rgb[solid].mean(axis=0)
    return np.dstack([rgb, np.ones(solid.shape, np.float32)])


class _Scene:
    """World-space BVH of the evaluated meshes (current pose) with smooth normals."""

    def __init__(self, objects):
        dg = bpy.context.evaluated_depsgraph_get()
        verts, polys, self.owner, self.vnormals = [], [], [], []
        for oi, obj in enumerate(objects):
            ev = obj.evaluated_get(dg)
            me = ev.to_mesh()
            mw = obj.matrix_world
            nm = mw.to_3x3().inverted_safe().transposed()
            base = len(verts)
            verts += [mw @ v.co for v in me.vertices]
            self.vnormals += [(nm @ v.normal).normalized() for v in me.vertices]
            for p in me.polygons:
                polys.append([base + i for i in p.vertices])
                self.owner.append(oi)
            ev.to_mesh_clear()
        self.verts = verts
        self.poly_verts = polys
        self.tree = BVHTree.FromPolygons(verts, polys)

    def normal(self, hit, fi):
        vs = self.poly_verts[fi]
        w = poly_3d_calc([self.verts[i] for i in vs], hit)
        n = Vector((0, 0, 0))
        for wi, i in zip(w, vs):
            n += self.vnormals[i] * wi
        return n.normalized() if n.length > 1e-9 else n


def view_axes(real_dir):
    """Camera right, up and towards-camera directions in model space for a view."""
    a = -cv.yaw(real_dir)
    right = Vector(cv.rotate_z(cv.RIGHT, a))
    up = Vector(cv.rotate_z(cv.UP, a))
    towards = Vector(cv.rotate_z(tuple(-c for c in cv.FORWARD), a))
    return right, up, towards


def canvas_ray(ref, real_dir, i, j):
    """Origin and direction (model space) of the camera ray through canvas pixel (i, j)."""
    right, up, towards = view_axes(real_dir)
    s = cv.METRES_PER_PIXEL
    ax, ay = ref.anchor
    p = right * ((i + 0.5 - ax) * s) - up * ((j + 0.5 - ay) * s) + towards * 60.0
    return p, -towards


def _normals_image(scene, ref, view, solid):
    """Per solid canvas pixel: camera-space normal (x right, y up, z towards the camera) and
    the object index of the first hit (-1 none)."""
    h, w = solid.shape
    right, up, towards = view_axes(view.dir)
    normals = np.zeros((h, w, 3), np.float32)
    owner = np.full((h, w), -1, np.int32)
    for j, i in zip(*np.nonzero(solid)):
        p, d = canvas_ray(ref, view.dir, i, j)
        hit, _n, fi, _d = scene.tree.ray_cast(p, d)
        if hit is None:
            continue
        n = scene.normal(hit, fi)
        if n.dot(towards) < 0:  # a single-sided sheet seen from behind
            n = -n
        normals[j, i] = (n.dot(right), n.dot(up), n.dot(towards))
        owner[j, i] = scene.owner[fi]
    return normals, owner


def fit_light(samples):
    """samples: [(luminance, normals (n, 3), object index)]. Fits a light direction L
    (camera space) and per object lum = a + b max(0, n·L); returns (L, k) with k the median
    diffuse/ambient ratio b/a."""
    lum = np.concatenate([s[0] for s in samples])
    nrm = np.concatenate([s[1] for s in samples])
    obj = np.concatenate([s[2] for s in samples])
    ids = [o for o in np.unique(obj) if (obj == o).sum() >= 20]
    best = None
    for el in np.radians(np.arange(-30, 85, 7.5)):
        for az in np.radians(np.arange(0, 360, 15)):
            L = np.array([math.cos(el) * math.cos(az), math.cos(el) * math.sin(az), math.sin(el)])
            d = np.maximum(nrm @ L, 0.0)
            sse, ratios, weights = 0.0, [], []
            for o in ids:
                m = obj == o
                A = np.stack([np.ones(m.sum()), d[m]], axis=1)
                sol = np.linalg.lstsq(A, lum[m], rcond=None)[0]
                sse += float(((A @ sol - lum[m]) ** 2).sum())
                if sol[0] > 1e-3 and sol[1] > 0:
                    ratios.append(sol[1] / sol[0])
            if best is None or sse < best[0]:
                best = (sse, L, float(np.median(ratios)) if ratios else 0.0)
    if best is None:
        return np.array([0.0, 0.0, 1.0]), 0.0
    return best[1], min(best[2], 3.0)


def project_uvs(objects, ref, views=None, mirror=(), delight=0.0, occluders=None, name="sprite", report=None,
                crop=False):
    """Writes the UV map `sprite` on `objects` (in the pose of the reference) and returns
    (image, {object name: (hidden polygon indices, dominant sRGB of its seen faces or None,
    the colour samples)}).
    `mirror`: objects that may use mirrored views; `occluders`: the objects that hide faces
    and give the normals for `delight` (default: all rendered meshes of the scene); `crop`
    cuts the atlas down to the pixels the UVs use (a building state that covers part of the
    canvas)."""
    views = views or ref.views
    w, h, n = ref.width, ref.height, len(views)
    ax, ay = ref.anchor
    # Each view gets a slot with the anchor in its middle, so mirror images stay inside.
    half = int(math.ceil(max(ax, w - ax)))
    slot = 2 * half
    x_in = half - int(ax)
    padded = []
    for v in views:
        p = load(v.image)
        q = np.zeros((h, slot, 4), np.float32)
        q[:, x_in:x_in + w] = p
        padded.append(q)
    scene_objs = occluders or [o for o in bpy.context.scene.objects if o.type == "MESH" and not o.hide_render]
    sc = _Scene(scene_objs)
    # Per object: the sprite pixels it covers in the stored views (ray-cast ownership,
    # inner pixels only), for its dominant colour.
    covered = {}
    want = {o.name for o in objects}
    for v, q in zip(views, padded):
        sp = q[:, x_in:x_in + w]
        solid = sp[..., 3] >= 0.5
        inner = solid.copy()
        inner[1:, :] &= solid[:-1, :]
        inner[:-1, :] &= solid[1:, :]
        inner[:, 1:] &= solid[:, :-1]
        inner[:, :-1] &= solid[:, 1:]
        _n, owner = _normals_image(sc, ref, v, inner)
        for oi, o in enumerate(scene_objs):
            if o.name in want:
                m = owner == oi
                if m.any():
                    covered.setdefault(o.name, []).extend(sp[m][:, :3])
    if delight > 0:
        samples, per_view = [], []
        for v, q in zip(views, padded):
            sp = q[:, x_in:x_in + w]
            solid = sp[..., 3] >= 0.5
            normals, owner = _normals_image(sc, ref, v, solid)
            lin = np.power(np.clip(sp[..., :3], 0, 1), 2.2)
            lum = lin @ np.array([0.2126, 0.7152, 0.0722], np.float32)
            m = (owner >= 0) & solid
            samples.append((lum[m], normals[m], owner[m]))
            per_view.append((normals, m))
        L, k = fit_light(samples)
        dots = np.concatenate([np.maximum(s[1] @ L, 0) for s in samples]) if samples else np.zeros(1)
        mean_s = 1.0 + k * float(dots.mean())
        for (normals, m), q in zip(per_view, padded):
            shade = (1.0 + k * np.maximum(normals @ L, 0.0)) / mean_s
            factor = np.clip(shade, 0.55, 1.8) ** -delight
            sp = q[:, x_in:x_in + w]
            lin = np.power(np.clip(sp[..., :3], 0, 1), 2.2) * np.where(m, factor, 1.0)[..., None]
            sp[..., :3] = np.power(np.clip(lin, 0, 1), 1 / 2.2)
        if report is not None:
            report.update({"light_dir_camera": [round(float(c), 3) for c in L], "diffuse_ratio": round(k, 3),
                           "delight": delight})
    atlas = np.concatenate([dilate(q) for q in padded], axis=1)

    # Candidate views: (atlas slot, RealDir, mirrored).
    cands = [(i, v.dir, False) for i, v in enumerate(views)]
    for i, v in enumerate(views):
        md = (128.0 - v.dir) % 256.0
        if all(abs(((md - c[1] + 128) % 256) - 128) > 4 for c in cands):
            cands.append((i, md, True))
    towards = [view_axes(d)[2] for _i, d, _m in cands]
    mirror = {o.name for o in mirror}
    dg = bpy.context.evaluated_depsgraph_get()
    out = {}
    for obj in objects:
        ev = obj.evaluated_get(dg)
        me_ev = ev.to_mesh()
        mw = obj.matrix_world
        nm = mw.to_3x3().inverted_safe().transposed()
        mesh = obj.data
        uv = mesh.uv_layers.get("sprite") or mesh.uv_layers.new(name="sprite")
        samples, hidden = [], []
        allow_mirror = obj.name in mirror
        for poly in me_ev.polygons:
            centre = mw @ poly.center
            normal = (nm @ poly.normal).normalized()
            probes = [centre] + [centre.lerp(mw @ me_ev.vertices[vi].co, 0.8) for vi in poly.vertices]
            best, best_facing = None, MIN_FACING
            for ci, (_slot_i, _d, mirrored) in enumerate(cands):
                if mirrored and not allow_mirror:
                    continue
                t = towards[ci]
                facing = normal.dot(t) * (MIRROR_PENALTY if mirrored else 1.0)
                if facing <= best_facing:
                    continue
                if any(sc.tree.ray_cast(p + t * 1e-3, t)[0] is None for p in probes):
                    best, best_facing = ci, facing
            if best is None:
                hidden.append(poly.index)
                for li in poly.loop_indices:
                    uv.data[li].uv = (0.0, 0.0)
                continue
            slot_i, d, mirrored = cands[best]
            x0 = slot_i * slot

            def canvas(co):
                sx, sy = cv.to_canvas(tuple(co), (half, ay), d)
                return (2 * half - sx if mirrored else sx), sy

            for li in poly.loop_indices:
                sx, sy = canvas(mw @ me_ev.vertices[me_ev.loops[li].vertex_index].co)
                uv.data[li].uv = ((x0 + sx) / (slot * n), 1.0 - sy / h)
            # Colour samples for the dominant colour: the face centre and its probes, only
            # sprite pixels whose four neighbours are solid too (edges bleed).
            q = padded[slot_i]
            for pt in probes:
                sx, sy = canvas(pt)
                x, y = int(sx), int(sy)
                if 1 <= x < slot - 1 and 1 <= y < h - 1 and min(q[y, x, 3], q[y - 1, x, 3], q[y + 1, x, 3],
                                                                q[y, x - 1, 3], q[y, x + 1, 3]) >= 0.5:
                    samples.append(q[y, x, :3].copy())
        ev.to_mesh_clear()
        pix = covered.get(obj.name) or samples
        out[obj.name] = (hidden, dominant(pix), pix)
    # The atlas, optionally cropped to the used UVs (plus a margin for filtering).
    aw, ah = slot * n, h
    x0, y0, x1, y1 = 0, 0, aw, ah
    if crop:
        us, vs = [], []
        for obj in objects:
            uvl = obj.data.uv_layers["sprite"]
            hidden = set(out[obj.name][0])
            for poly in obj.data.polygons:
                if poly.index in hidden:
                    continue
                for li in poly.loop_indices:
                    u, v = uvl.data[li].uv
                    us.append(u * aw)
                    vs.append((1.0 - v) * ah)
        if us:
            x0, x1 = max(int(min(us)) - 4, 0), min(int(max(us)) + 5, aw)
            y0, y1 = max(int(min(vs)) - 4, 0), min(int(max(vs)) + 5, ah)
            for obj in objects:
                for d in obj.data.uv_layers["sprite"].data:
                    u, v = d.uv
                    d.uv = ((u * aw - x0) / (x1 - x0), 1.0 - ((1.0 - v) * ah - y0) / (y1 - y0))
    sub = atlas[y0:y1, x0:x1]
    image = bpy.data.images.new(name, x1 - x0, y1 - y0, alpha=False)
    image.pixels.foreach_set(np.ascontiguousarray(sub[::-1]).ravel())
    image.file_format = "PNG"
    image.pack()
    return image, out


def guided(samples, prior, radius=0.3, minimum=6):
    """Dominant colour of the samples within `radius` (sRGB distance) of the expected colour
    `prior`: a part takes the sprite's shade of its own colour and ignores pixels of its
    neighbours that the geometry overlaps by a pixel or two. `prior` when too few match."""
    if not len(samples):
        return tuple(prior)
    a = np.array(samples, np.float32)
    near = a[np.linalg.norm(a - np.array(prior, np.float32), axis=1) < radius]
    return dominant(near) if len(near) >= minimum else tuple(prior)


def dominant(samples, keep=0.6):
    """Robust dominant colour (sRGB) of sprite samples: the per-channel median of the `keep`
    fraction nearest to the median (outliers, deep shadows and highlights left out; the
    median minimises the mean absolute difference `acr model-check` reports); None without
    samples."""
    if not len(samples):
        return None
    a = np.clip(np.array(samples, np.float32), 0, 1)
    med = np.median(a, axis=0)
    d = np.linalg.norm(a - med, axis=1)
    k = max(1, int(len(a) * keep))
    return tuple(float(c) for c in np.median(a[np.argsort(d)[:k]], axis=0))


def project(objects, ref, views=None, name="sprite", fallback=(0.5, 0.5, 0.5), mirror=(), delight=0.0, crop=False):
    """Projects the views of `ref` onto `objects` and gives them one textured material (plus
    a flat one for the faces no view sees). Returns the material."""
    image, info = project_uvs(objects, ref, views, mirror=mirror, delight=delight, name=name, crop=crop)
    mat = materials.textured(name, image, "sprite")
    for obj in objects:
        hidden, rgb, _samples = info[obj.name]
        obj.data.materials.clear()
        obj.data.materials.append(mat)
        if hidden:
            obj.data.materials.append(materials.flat(f"{obj.name}.unseen", rgb or fallback))
            for i in hidden:
                obj.data.polygons[i].material_index = 1
    return mat
