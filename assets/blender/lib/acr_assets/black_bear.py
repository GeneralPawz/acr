"""The WildMesh brown bear reshaped and recoloured into an American black bear (`AnmMedSm`).

A black bear is not a small grizzly:
- black fur with a tan-brown muzzle and a small white chest blaze;
- no shoulder hump: the back runs straight from withers to rump;
- a straight ("Roman") facial profile instead of the grizzly's dished face;
- larger, more upright ears;
- shorter claws;
- a leaner body on longer-looking legs (narrower and shallower trunk).

All edits work on the rest mesh in armature space (the source rig: -Y forward, Z up,
centimetres), weighted by the bones' vertex groups, so the rig and every clip stay as they
are. Colours are painted into the base-colour images through the UVs of the faces of each
region (the renderer reads base colour only; COLOR_0 is the decay mask).
"""

import numpy as np

TORSO = ("RigPelvis", "RigSpine1", "RigSpine2", "RigChest", "RigNeck1", "RigNeck2")
EARS = ("RigRigLEar1", "RigRigREar1")
FRONT_DIGITS = ("RigLFLegDigit11", "RigRFLegDigit11")
BACK_DIGITS = ("RigLBLegDigit11", "RigRBLegDigit11")

BLACK = np.array([0.045, 0.040, 0.038], dtype=np.float32)
TAN = np.array([0.50, 0.36, 0.22], dtype=np.float32)
BLAZE = np.array([0.80, 0.77, 0.70], dtype=np.float32)


def _smooth(t):
    t = np.clip(t, 0.0, 1.0)
    return t * t * (3 - 2 * t)


def _weights(mesh, names):
    """Summed weight of the vertex groups `names` per vertex."""
    idx = {g.index for g in mesh.vertex_groups if g.name in names}
    w = np.zeros(len(mesh.data.vertices), dtype=np.float32)
    for v in mesh.data.vertices:
        for g in v.groups:
            if g.group in idx:
                w[v.index] += g.weight
    return w


def _top_profile(p, ys, half_width):
    """The highest Z near the midline per Y bin (NaN where empty)."""
    out = np.full(len(ys) - 1, np.nan)
    mid = np.abs(p[:, 0]) < half_width
    for i in range(len(ys) - 1):
        sel = mid & (p[:, 1] >= ys[i]) & (p[:, 1] < ys[i + 1])
        if sel.any():
            out[i] = p[sel, 2].max()
    return out


def reshape(arm, mesh, log=print):
    me = mesh.data
    to_arm = arm.matrix_world.inverted() @ mesh.matrix_world
    n = len(me.vertices)
    co = np.empty(n * 3, dtype=np.float32)
    me.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    m = np.array(to_arm, dtype=np.float32)
    p = co @ m[:3, :3].T + m[:3, 3]
    bones = arm.data.bones

    # 1. No hump: over the withers (y -90 .. 40) the back comes down to a level line at the
    # height of the rump; what rises above it comes down, fading with depth below the top
    # and towards the neck. The top profile takes every vertex of an 8 cm slice.
    ys = np.arange(-100.0, 60.0, 8.0)
    top = _top_profile(p, ys, 1e9)
    yc = (ys[:-1] + ys[1:]) / 2
    ok = ~np.isnan(top)
    ztop = np.interp(p[:, 1], yc[ok], top[ok])
    rump = float(np.interp(30.0, yc[ok], top[ok]))
    level = rump + 2.0
    span = _smooth((p[:, 1] + 95.0) / 25.0) * _smooth((40.0 - p[:, 1]) / 15.0)
    excess = np.clip(ztop - level, 0.0, None) * span
    depth = _smooth((p[:, 2] - (ztop - 40.0)) / 40.0)
    p[:, 2] -= excess * depth
    log(f"black bear: hump down by {excess.max():.1f} cm to {level:.0f}")
    z0 = z1 = level
    y0, y1 = -60.0, 55.0

    # 2. A lean trunk: narrower (x 0.86) and shallower from below (the belly up by 14 % of
    # its depth under the back line), so the legs show longer.
    torso = np.clip(_weights(mesh, TORSO), 0.0, 1.0)
    back = z0 + (p[:, 1] - y0) / (y1 - y0) * (z1 - z0)
    p[:, 0] -= p[:, 0] * 0.14 * torso
    below = np.clip(back - p[:, 2], 0.0, None)
    p[:, 2] += below * 0.14 * torso

    # 3. A straight (Roman) face: the top of the head from the brow to the nose tip on a
    # line, with a slight bulge, instead of the grizzly's dish.
    head = bones["RigHead"].head_local
    face = p[:, 1] < head.y - 8.0
    hy = np.arange(p[face, 1].min() - 1, head.y - 7.9, 2.0)
    htop = _top_profile(p[face], hy, 4.0)
    hyc = (hy[:-1] + hy[1:]) / 2
    hok = ~np.isnan(htop)
    nose_y, brow_y = hyc[hok][0] + 2.0, hyc[hok][-1]
    nose_z = np.interp(nose_y, hyc[hok], htop[hok])
    brow_z = np.interp(brow_y, hyc[hok], htop[hok])
    t = np.clip((p[:, 1] - nose_y) / (brow_y - nose_y), 0.0, 1.0)
    roman = nose_z + t * (brow_z - nose_z) + 1.5 * np.sin(np.pi * t)
    hz = np.interp(p[:, 1], hyc[hok], htop[hok])
    lift = np.clip(roman - hz, 0.0, None) * face
    # Only the upper face rises, fading to nothing at the jaw.
    p[:, 2] += lift * _smooth((p[:, 2] - (hz - 12.0)) / 12.0)
    log(f"black bear: face straightened by up to {lift.max():.1f} cm")

    # 4. Larger, more upright ears: x1.45 about the ear root, tipped up.
    for name in EARS:
        w = _weights(mesh, (name,))
        root = np.array(bones[name].head_local, dtype=np.float32)
        d = p - root
        d *= 1.0 + 0.45 * w[:, None]
        # Upright: lean the ear back towards +Z (rotate its forward part up).
        fwd = np.clip(-d[:, 1], 0.0, None)
        d[:, 2] += fwd * 0.35 * w
        d[:, 1] += fwd * 0.2 * w
        p[:] = root + d
    # 5. Shorter claws: what reaches past the toes forward comes back by half.
    for names in (FRONT_DIGITS, BACK_DIGITS):
        for name in names:
            w = np.clip(_weights(mesh, (name,)), 0.0, 1.0)
            toe = bones[name].head_local
            sel = (w > 0.3) & (p[:, 2] < toe.z + 9.0)
            front = p[:, 1].copy()
            reach = np.clip((toe.y - 4.0) - front, 0.0, None)
            p[:, 1] += reach * 0.5 * sel
            # A little curl: the claw tips come down to the ground line.
            p[:, 2] -= reach * 0.15 * sel

    inv = np.linalg.inv(m)
    co = p @ inv[:3, :3].T + inv[:3, 3]
    me.vertices.foreach_set("co", co.ravel())
    me.update()
    return p


def _region_mask(mesh, p, image, select, size):
    """A mask (h, w) of the image's pixels covered by the UVs of the faces using `image`
    whose arm-space centroid satisfies `select` (returns a 0..1 weight per face)."""
    me = mesh.data
    w_px, h_px = size
    mask = np.zeros((h_px, w_px), dtype=np.float32)
    uv = me.uv_layers.active.data
    mats = [s.material for s in mesh.material_slots]
    for poly in me.polygons:
        mat = mats[poly.material_index] if poly.material_index < len(mats) else None
        if mat is None or not mat.node_tree or not any(
            n.type == "TEX_IMAGE" and n.image == image for n in mat.node_tree.nodes
        ):
            continue
        c = p[list(poly.vertices)].mean(axis=0)
        k = float(select(c))
        if k <= 0.0:
            continue
        loops = list(poly.loop_indices)
        pts = np.array([uv[i].uv for i in loops], dtype=np.float32) * [w_px, h_px]
        for j in range(1, len(pts) - 1):
            tri = np.array([pts[0], pts[j], pts[j + 1]])
            x0, y0 = np.floor(tri.min(axis=0)).astype(int) - 1
            x1, y1 = np.ceil(tri.max(axis=0)).astype(int) + 1
            x0, y0 = max(x0, 0), max(y0, 0)
            x1, y1 = min(x1, w_px - 1), min(y1, h_px - 1)
            if x1 < x0 or y1 < y0:
                continue
            gx, gy = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
            (ax, ay), (bx, by), (cx, cy) = tri
            den = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
            if abs(den) < 1e-9:
                continue
            l1 = ((by - cy) * (gx - cx) + (cx - bx) * (gy - cy)) / den
            l2 = ((cy - ay) * (gx - cx) + (ax - cx) * (gy - cy)) / den
            inside = (l1 >= -0.02) & (l2 >= -0.02) & (1 - l1 - l2 >= -0.02)
            sub = mask[y0:y1 + 1, x0:x1 + 1]
            sub[inside] = np.maximum(sub[inside], k)
    # Soft edges.
    for _ in range(4):
        mask[1:-1, 1:-1] = (mask[1:-1, 1:-1] * 4 + mask[:-2, 1:-1] + mask[2:, 1:-1] + mask[1:-1, :-2] + mask[1:-1, 2:]) / 8
    return mask


def recolour(arm, mesh, p, log=print):
    """Black fur, a tan muzzle, a small white chest blaze."""
    import bpy

    jaw = arm.data.bones["RigJaw"].head_local
    eye = arm.data.bones["RigLEyelid"].head_local
    nose = float(p[:, 1].min())
    for im in bpy.data.images:
        if not im.users or im.size[0] == 0 or "teeth" in im.name.lower() or "teef" in im.name.lower():
            continue
        w, h = im.size
        px = np.empty(w * h * 4, dtype=np.float32)
        im.pixels.foreach_get(px)
        px = px.reshape(h, w, 4)
        rgb = px[..., :3]
        lum = rgb @ np.array([0.299, 0.587, 0.114], dtype=np.float32)
        # Keep the fur's detail as relative brightness around its median.
        rel = np.clip(lum / max(1e-4, float(np.median(lum))), 0.3, 2.2)[..., None]
        black = BLACK * (0.55 + 0.45 * rel) + 0.035 * (rel - 1.0).clip(0, None)
        # The muzzle: the snout in front of the eyes, fading out towards the brow.
        def muzzle(c):
            # In front of the eyes, below the brow; the nose pad stays dark.
            if c[1] < nose + 3.0:
                return 0.0
            ahead = np.clip((eye.y - 9.0 - c[1]) / 5.0, 0.0, 1.0)
            low = np.clip((eye.z + 2.0 - c[2]) / 4.0, 0.0, 1.0)
            return float(ahead * low)

        mz = _region_mask(mesh, p, im, muzzle, (w, h))[..., None]
        tan = TAN * (0.6 + 0.4 * rel)
        out = black * (1 - mz) + tan * mz

        # The chest blaze: a small pale patch low on the front of the chest.
        def blaze(c):
            if abs(c[0]) > 9.0:
                return 0.0
            if not (jaw.y + 45.0 < c[1] < jaw.y + 62.0 and 55.0 < c[2] < 80.0):
                return 0.0
            return 0.85

        bz = _region_mask(mesh, p, im, blaze, (w, h))[..., None]
        out = out * (1 - bz) + BLAZE * (0.7 + 0.3 * np.clip(rel, 0, 1.3)) * bz
        rgb[:] = np.clip(out, 0.0, 1.0)
        im.pixels.foreach_set(px.ravel())
        im.pack()
        log(f"black bear: recoloured {im.name} (muzzle {float(mz.mean()):.3f}, blaze {float(bz.mean()):.4f})")


def apply(arm, meshes, log=print):
    import bpy

    bpy.context.view_layer.update()
    for mesh in meshes:
        if not mesh.vertex_groups.get("RigHead"):
            continue
        p = reshape(arm, mesh, log)
        recolour(arm, mesh, p, log)
