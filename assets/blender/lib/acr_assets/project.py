"""Colour from the sprites: projects the stored sprite views through the original camera
onto a model (needs `bpy`, `numpy`).

Every face takes its colours from the view that sees it most frontally among the views in
which it is not hidden (a ray from its centre or from a point near one of its corners
towards the camera hits nothing). The
views are packed side by side into one atlas image, so the model gets one UV map
(`sprite`) and one material. Faces no view sees get a flat material in the mean sprite
colour of their object's visible faces (or `fallback`).

Limits: the sprite colours include the original lighting and shadows (they are used as
albedo); a face counts as visible or hidden as a whole, so the hidden part of a partly
covered face gets the occluder's colours; the mirrored half of a unit's directions is not
stored and adds nothing.
"""

import bpy
import numpy as np
from mathutils import Vector

from . import conventions as cv
from . import materials

# Faces at a flatter angle to the view than this (cosine) take no colour from it.
MIN_FACING = 0.05
# Rounds of edge dilation that push sprite colours into transparent pixels, so texture
# filtering and slightly larger models do not pick up the black of empty pixels.
DILATE = 12


def _load(view):
    img = bpy.data.images.load(view.image, check_existing=False)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1]  # top row first
    bpy.data.images.remove(img)
    return px


def _dilate(px):
    """Fills transparent pixels with the mean of their solid neighbours, DILATE times, then
    with the mean colour; returns an opaque image."""
    rgb = px[..., :3].copy()
    solid = px[..., 3] >= 0.5
    if not solid.any():
        return np.dstack([rgb, np.ones(solid.shape, np.float32)])
    for _ in range(DILATE):
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


def project(objects, ref, views=None, name="sprite", fallback=(0.5, 0.5, 0.5)):
    """Projects `views` (default: all views of `ref`, an `acr_assets.Reference`) onto the
    mesh objects `objects`, which stand at the model origin (root not rotated)."""
    views = views or ref.views
    w, h, n = ref.width, ref.height, len(views)
    sprites = [_load(v) for v in views]
    atlas = np.concatenate([_dilate(p) for p in sprites], axis=1)
    image = bpy.data.images.new(name, w * n, h, alpha=False)
    image.pixels.foreach_set(atlas[::-1].ravel())
    image.file_format = "PNG"
    image.pack()
    mat = materials.textured(name, image, "sprite")

    scene = bpy.context.scene
    depsgraph = bpy.context.evaluated_depsgraph_get()
    # Direction towards the camera in model space, per view.
    towards = [Vector(cv.rotate_z(tuple(-c for c in cv.FORWARD), -cv.yaw(v.dir))) for v in views]

    for obj in objects:
        mesh = obj.data
        mw = obj.matrix_world
        nm = mw.to_3x3().inverted_safe().transposed()
        uv = mesh.uv_layers.get("sprite") or mesh.uv_layers.new(name="sprite")
        samples = []
        hidden = []
        for poly in mesh.polygons:
            centre = mw @ poly.center
            normal = (nm @ poly.normal).normalized()
            # The face centre and its corners pulled 20 % towards it: a face is seen if any
            # of them is.
            probes = [centre] + [centre.lerp(mw @ mesh.vertices[vi].co, 0.8) for vi in poly.vertices]
            best, best_facing = None, MIN_FACING
            for i, t in enumerate(towards):
                facing = normal.dot(t)
                if facing <= best_facing:
                    continue
                if any(not scene.ray_cast(depsgraph, p + t * 1e-3, t)[0] for p in probes):
                    best, best_facing = i, facing
            if best is None:
                hidden.append(poly.index)
                for li in poly.loop_indices:
                    uv.data[li].uv = (0.0, 0.0)
                continue
            v = views[best]
            for li in poly.loop_indices:
                co = mw @ mesh.vertices[mesh.loops[li].vertex_index].co
                sx, sy = cv.to_canvas(tuple(co), ref.anchor, v.dir)
                uv.data[li].uv = ((best * w + sx) / (w * n), 1.0 - sy / h)
            sx, sy = cv.to_canvas(tuple(centre), ref.anchor, v.dir)
            x, y = int(sx), int(sy)
            if 0 <= x < w and 0 <= y < h and sprites[best][y, x, 3] >= 0.5:
                samples.append(sprites[best][y, x, :3])
        obj.data.materials.clear()
        obj.data.materials.append(mat)
        if hidden:
            # The sprite PNG is sRGB; the mean is taken in sRGB like the flat colours.
            rgb = tuple(np.mean(samples, axis=0)) if samples else fallback
            obj.data.materials.append(materials.flat(f"{obj.name}.unseen", rgb))
            for i in hidden:
                mesh.polygons[i].material_index = 1
    return mat
