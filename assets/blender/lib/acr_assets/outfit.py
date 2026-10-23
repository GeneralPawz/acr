"""Realistic clothes, skin and leather for characters (needs `bpy`, `numpy`): one baked atlas
per character, painted like the horse coats (`coat.py`).

Every dressed part (skin, wool, linen, boots and gaiters, belts, felt hats, leather gear)
gets a rectangle of one atlas through a Smart-UV map `atlas`; Cycles bakes its rest-pose
position, normal and ambient occlusion, and numpy paints each texel from them: the part's
colour (from the sprites or the palette) with weave, folds, wear and grime, a face (eyes,
brows, mouth, stubble) and hair on the head, brass buttons down the coat and waistcoat,
button rows on the gaiters, creases in the leather, all multiplied by the occlusion so the
form reads under the game's flat Lambert light. Wood, metal, bones and stump caps keep their
materials. Deterministic (seeded noise).
"""

import numpy as np

from . import coat, decay, registry

SIZE = 1024
# Painter of each material key (`humans.LAYER_OF` keys); keys not listed keep their material.
# A painter's objects share its atlas cell (packed together, `atlas_uv`), each painted in
# its own key's colour (`split_by_colour`). The draped garments' facings, lining and stock
# have own painters for their own finish; `KIND` maps such painters to the one they paint
# like. `FINISH`: how each painter's cloth takes the light
# (`acr_roughness`, `acr_sheen`, `acr_gloss` in the material extras): the game's
# `model.wgsl` adds the sheen at grazing angles and the gloss as a highlight; the Blender
# previews use Principled roughness, sheen and specular. Broadcloth is fulled and shorn:
# matte, a soft sheen at the silhouette only. Kersey (the waistcoat and breeches) and linen
# are flatter; the gaiters are black linen or wool, not leather; shoes are blacked leather
# with a dull gloss. All three tables live in `registry` (families add painters there).
PAINTER = registry.PAINTER
KIND = registry.KIND
FINISH = registry.FINISH
HAIR = (0.24, 0.16, 0.10)
BRASS = (0.78, 0.62, 0.27)


def regions(names):
    """Atlas rectangles for the painters `names`: a grid of equal cells, the skin twice as
    wide (the face needs the detail)."""
    names = sorted(names, key=lambda n: (n != "skin", n))
    cells = [n for n in names for _ in range(2 if n == "skin" else 1)]
    cols = 3
    rows = -(-len(cells) // cols)
    out = {}
    for i, n in enumerate(cells):
        c, r = i % cols, i // cols
        rect = (c / cols, r / rows, (c + 1) / cols, (r + 1) / rows)
        if n in out:
            a = out[n]
            rect = (min(a[0], rect[0]), min(a[1], rect[1]), max(a[2], rect[2]), max(a[3], rect[3]))
        out[n] = rect
    return out


def _buttons(p, n, xs, z0, z1, step, radius, front=True):
    """Mask of round buttons in columns at |x| = xs, from z0 to z1 every `step`, on the
    front (−y) or anywhere."""
    zk = np.clip(np.round((p[:, 2] - z0) / step), 0, round((z1 - z0) / step)) * step + z0
    d = np.full(len(p), 9.0, np.float32)
    for x in xs:
        d = np.minimum(d, np.hypot(np.abs(p[:, 0]) - x, p[:, 2] - zk))
    m = coat.smooth(radius, radius * 0.6, d)
    if front:
        m = m * coat.smooth(-0.2, -0.5, n[:, 1])
    return m, d


def paint(kind, p, n, rgb, L, seed):
    if kind in registry.PAINT_FN:
        return registry.PAINT_FN[kind](p, n, rgb, L, seed)
    base = coat.lin(rgb)
    fine = coat.fbm(p, 260.0, seed, octaves=1)
    broad = coat.fbm(p, 9.0, seed + 5)
    if kind == "skin":
        col = base * (0.90 + 0.10 * coat.fbm(p, 35.0, seed) + 0.04 * fine)[:, None]
        top = L["head_top"]
        face = coat.smooth(-0.15, -0.5, n[:, 1]) * coat.smooth(top - 0.26, top - 0.20, p[:, 2])
        # Ruddy cheeks and nose, stubble on the jaw.
        cheek = face * coat.smooth(0.05, 0.02, np.abs(np.abs(p[:, 0]) - 0.04) + np.abs(p[:, 2] - (top - 0.15)))
        col = coat.mix(col, base * np.array([1.08, 0.82, 0.78], np.float32), cheek * 0.6)
        jaw = face * coat.smooth(top - 0.21, top - 0.19, p[:, 2]) * coat.smooth(top - 0.15, top - 0.17, p[:, 2])
        col = coat.mix(col, base * 0.72, jaw * 0.35 * (0.6 + 0.4 * fine))
        # Eyes (white, dark iris), brows, mouth.
        ez = top - 0.105
        for sx in (-1.0, 1.0):
            d = np.hypot(p[:, 0] - sx * 0.034, (p[:, 2] - ez) * 1.6)
            col = coat.mix(col, coat.lin((0.86, 0.84, 0.80)), face * coat.smooth(0.012, 0.009, d))
            col = coat.mix(col, coat.lin((0.10, 0.08, 0.06)), face * coat.smooth(0.0065, 0.0045, d))
            brow = np.hypot((p[:, 0] - sx * 0.034) * 0.6, p[:, 2] - (ez + 0.022))
            col = coat.mix(col, coat.lin(HAIR) * 0.6, face * coat.smooth(0.010, 0.006, brow))
        mouth = np.hypot(p[:, 0] * 0.35, p[:, 2] - (top - 0.165))
        col = coat.mix(col, coat.lin((0.42, 0.18, 0.15)), face * coat.smooth(0.0075, 0.004, mouth))
        # Hair: the back and top of the head down to the nape, tied in a queue.
        hair = coat.smooth(top - 0.13, top - 0.09, p[:, 2]) * coat.smooth(-0.02, 0.03, p[:, 1] + 0.06 * (p[:, 2] - top + 0.1) / 0.1)
        hair = np.maximum(hair, coat.smooth(top - 0.05, top - 0.03, p[:, 2]))
        # The queue: hair down the back of the head to the nape.
        hair = np.maximum(hair, coat.smooth(0.01, 0.04, p[:, 1]) * coat.smooth(top - 0.25, top - 0.21, p[:, 2]) * coat.smooth(0.07, 0.05, np.abs(p[:, 0])))
        streak = coat.fbm(p, (90.0, 90.0, 12.0), seed + 9)
        col = coat.mix(col, coat.lin(HAIR) * (0.7 + 0.5 * streak)[:, None], np.clip(hair, 0, 1))
        return col
    if kind == "wool":
        # Fulled broadcloth: no weave shows, a fine felted nap (mottle at 2..5 mm), the dye a
        # little uneven over the panel; no painted highlights (the light makes the form).
        nap = coat.fbm(p, 420.0, seed, octaves=1)
        mottle = coat.fbm(p, (90.0, 90.0, 60.0), seed + 2, octaves=2)
        col = base * (0.93 + 0.05 * broad + 0.06 * mottle + 0.05 * nap)[:, None]
        # Rubbed thin (paler, the nap worn) only on the edges that see wear.
        grime = coat.smooth(0.85, 0.55, p[:, 2]) * 0.22
        col = coat.mix(col, coat.lin((0.25, 0.20, 0.15)), grime)
        return col
    if kind == "linen":
        # Linen: slubbed threads (short streaks along the weft), a little crumpled, grubby.
        slub = coat.fbm(p, (40.0, 40.0, 380.0), seed, octaves=1)
        weave = coat.fbm(p, 520.0, seed + 1, octaves=1)
        col = base * (0.90 + 0.07 * slub + 0.05 * weave + 0.04 * broad)[:, None]
        grime = coat.smooth(0.6, 0.8, coat.fbm(p, 12.0, seed + 4)) * 0.12
        return coat.mix(col, coat.lin((0.48, 0.40, 0.30)), grime)
    if kind == "kersey":
        # Kersey (waistcoat, breeches): a coarser twilled wool, flatter than broadcloth,
        # soiled at the knees and the seat; buttons down the waistcoat front.
        nap = coat.fbm(p, 380.0, seed + 1, octaves=1)
        rib = coat.fbm(p, (160.0, 40.0, 160.0), seed + 2, octaves=1)
        col = base * (0.90 + 0.04 * rib + 0.05 * nap + 0.05 * broad)[:, None]
        grime = (coat.smooth(0.62, 0.48, p[:, 2]) * coat.smooth(0.40, 0.48, p[:, 2]) * 0.25
                 + coat.smooth(0.6, 0.8, coat.fbm(p, 12.0, seed + 4)) * 0.15)
        col = coat.mix(col, coat.lin((0.48, 0.40, 0.30)), np.clip(grime, 0, 0.4))
        b, _ = _buttons(p, n, (0.0,), 1.00, 1.42, 0.055, 0.008)
        return coat.mix(col, coat.lin(BRASS) * 0.9, b * coat.smooth(0.95, 1.0, p[:, 2]))
    if kind == "gaiters":
        # Black linen or wool gaiters: matte cloth gone rusty brown-black, mud at the foot;
        # the buttons are geometry (`drape.buttons`).
        nap = coat.fbm(p, 300.0, seed, octaves=1)
        col = base * (0.85 + 0.12 * broad + 0.10 * nap)[:, None] + coat.lin((0.035, 0.03, 0.025))
        mud = coat.smooth(0.22, 0.05, p[:, 2]) * coat.smooth(0.4, 0.7, broad)
        return coat.mix(col, coat.lin((0.30, 0.24, 0.16)), mud * 0.6)
    if kind == "boots":
        crease = coat.fbm(p, (25.0, 25.0, 4.0), seed + 2)
        col = base * (0.80 + 0.30 * crease + 0.08 * fine)[:, None]
        mud = coat.smooth(0.25, 0.05, p[:, 2]) * coat.smooth(0.4, 0.7, broad)
        col = coat.mix(col, coat.lin((0.30, 0.24, 0.16)), mud * 0.7)
        # Button row on the outer side of each gaiter.
        outer = coat.smooth(0.3, 0.7, n[:, 0] * np.sign(p[:, 0]))
        zk = np.round((p[:, 2] - 0.08) / 0.045) * 0.045 + 0.08
        d = np.hypot(p[:, 1] - 0.0, p[:, 2] - zk)
        b = outer * coat.smooth(0.009, 0.006, d) * coat.smooth(0.46, 0.42, p[:, 2]) * coat.smooth(0.06, 0.09, p[:, 2])
        return coat.mix(col, coat.lin((0.30, 0.28, 0.26)), b)
    if kind == "belts":
        col = base * (0.86 + 0.10 * broad + 0.06 * fine)[:, None]
        dirt = coat.smooth(0.55, 0.8, coat.fbm(p, 14.0, seed + 6)) * 0.3
        return coat.mix(col, coat.lin((0.40, 0.33, 0.24)), dirt)
    if kind == "felt":
        col = base * (0.85 + 0.25 * coat.fbm(p, 50.0, seed) + 0.1 * broad)[:, None]
        dust = coat.smooth(0.6, 0.85, coat.fbm(p, 10.0, seed + 7)) * 0.25
        return coat.mix(col, coat.lin((0.35, 0.32, 0.28)), dust)
    # leather
    crease = np.abs(coat.fbm(p, 22.0, seed + 8) - 0.5)
    col = base * (0.78 + 0.25 * broad + 0.08 * fine)[:, None]
    col = col * (1.0 - 0.35 * coat.smooth(0.025, 0.0, crease))[:, None]
    scuff = coat.smooth(0.7, 0.9, coat.fbm(p, 30.0, seed + 9)) * 0.3
    return coat.mix(col, base * 1.4 + 0.05, scuff)


def apply_mask(e, p, n, col, L, seed):
    """Paints mask entry `e` (`masks.entry`) over the colours `col` of texels at `p`, `n`:
    in the frame of object `e["frame"]` when set, the mask's colour with a little mottle."""
    if e.get("frame"):
        import bpy

        m = np.array(bpy.data.objects[e["frame"]].matrix_world.inverted(), np.float32)
        p = p @ m[:3, :3].T + m[:3, 3]
        n = n @ np.linalg.inv(m[:3, :3])
        n = n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-6)
    w = registry.MASKS[e["mask"]](p, n, L, seed, **e.get("params", {}))
    tint = coat.lin(e["rgb"]) * (0.88 + 0.16 * coat.fbm(p, 300.0, seed + 31, octaves=1))[:, None]
    return coat.mix(col, tint, np.clip(w, 0.0, 1.0))


def atlas_uv(objs, region, name="atlas", angle=66.0):
    """One Smart-UV map `name` for all of `objs` together, fitted into `region`: their
    islands are packed side by side, never on top of each other (`coat.atlas_uv` unwraps one
    object; the objects of one painter share its cell, and unwrapped one by one they all
    filled it, so the bake kept the last object's texels and the others sampled them: the
    scalp lock's hair painted over a native's trunk)."""
    import math

    import bpy

    if len(objs) == 1:
        return coat.atlas_uv(objs[0], region, name, angle)
    for o in objs:
        me = o.data
        while len(me.uv_layers):
            me.uv_layers.remove(me.uv_layers[0])
        me.uv_layers.active = me.uv_layers.new(name=name)
    vl = bpy.context.view_layer
    for o in vl.objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
    vl.objects.active = objs[0]
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(angle), island_margin=0.006, scale_to_bounds=True)
    bpy.ops.object.mode_set(mode="OBJECT")
    u0, v0, u1, v1 = region
    pad = 2.0 / coat.SIZE
    for o in objs:
        for d in o.data.uv_layers[name].data:
            u, v = d.uv
            d.uv = (u0 + pad + u * (u1 - u0 - 2 * pad), v0 + pad + v * (v1 - v0 - 2 * pad))
        o.select_set(False)


def raster_labels(tris, size):
    """Label image (size², int32, -1 where empty) of UV triangles `tris`: a list of
    (label, (3, 2) uv array) each; texel centres inside a triangle take its label."""
    lab = np.full((size, size), -1, np.int32)
    for k, t in tris:
        t = np.asarray(t, np.float64) * size - 0.5
        x0, y0 = np.floor(t.min(0)).astype(int)
        x1, y1 = np.ceil(t.max(0)).astype(int)
        x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, size - 1), min(y1, size - 1)
        if x1 < x0 or y1 < y0:
            continue
        ys, xs = np.mgrid[y0:y1 + 1, x0:x1 + 1]
        (ax, ay), (bx, by), (cx, cy) = t
        den = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
        if abs(den) < 1e-12:
            continue
        w0 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / den
        w1 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / den
        inside = (w0 >= -1e-6) & (w1 >= -1e-6) & (1.0 - w0 - w1 >= -1e-6)
        sub = lab[y0:y1 + 1, x0:x1 + 1]
        sub[inside] = k
    return lab


def grow_labels(lab, want, steps=12):
    """Spreads the labels of `lab` into the texels of `want` that have none (the bake's
    margin round each island); what is left takes the most common label."""
    lab = lab.copy()
    for _ in range(steps):
        miss = want & (lab < 0)
        if not miss.any():
            break
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            nb = np.roll(np.roll(lab, dy, 0), dx, 1)
            take = miss & (lab < 0) & (nb >= 0)
            lab[take] = nb[take]
    miss = want & (lab < 0)
    if miss.any():
        have = lab[lab >= 0]
        lab[miss] = np.bincount(have).argmax() if have.size else 0
    return lab


def _labels(objs, size, covered):
    """Which object (index into `objs`) owns each texel of the atlas (`atlas` UVs)."""
    tris = []
    for k, o in enumerate(objs):
        me = o.data
        me.calc_loop_triangles()
        uv = me.uv_layers["atlas"].data
        for lt in me.loop_triangles:
            tris.append((k, [tuple(uv[li].uv) for li in lt.loops]))
    return grow_labels(raster_labels(tris, size), covered.reshape(size, size)).reshape(-1)


def split_by_colour(os_, colours, owner, every, m):
    """The texels `m` of one painter's cell split by colour: [(mask, sRGB)], one entry per
    distinct colour of the objects `os_` (`colours`: object name -> sRGB; `owner`: texel ->
    index into `every`, or None when every painter has one object)."""
    rgbs = [tuple(colours.get(o.name) or (0.5, 0.5, 0.5)) for o in os_]
    if owner is None or len(set(rgbs)) == 1:
        return [(m, rgbs[0])]
    out = {}
    for o, rgb in zip(os_, rgbs):
        sel = m & (owner == every.index(o))
        out[rgb] = out[rgb] | sel if rgb in out else sel
    # Texels of the cell no object claims (none, after `grow_labels`) keep the first colour.
    rest = m.copy()
    for sel in out.values():
        rest &= ~sel
    first = rgbs[0]
    out[first] = out[first] | rest
    return [(sel, rgb) for rgb, sel in out.items()]


def dress(ch, colours, layer_of, size=SIZE):
    """Paints one atlas for the dressed parts of character `ch` (`ch.keys`: object → material
    key, `colours`: object → sRGB from `Character.colours`) and gives them decaying
    materials on it. Returns the image, or None if nothing is dressed."""
    objs = ch.layers["flesh"] + ch.layers["clothes"] + ch.layers["gear"] + list(ch.held.values())
    groups = {}
    for o in objs:
        if o.type != "MESH":
            continue
        keys = list(o.get("acr_material_keys") or [ch.keys.get(o.name)])
        if len(keys) != 1 or keys[0] not in PAINTER:
            continue
        groups.setdefault(PAINTER[keys[0]], []).append(o)
    if not groups:
        return None
    rects = regions(groups)
    coat_size = coat.SIZE
    coat.SIZE = size
    try:
        for name, os_ in groups.items():
            atlas_uv(os_, rects[name])
        flesh = [v.co.z for o in ch.layers["flesh"] for v in o.data.vertices]
        L = {"head_top": max(flesh) if flesh else 1.75}
        pos, nrm, ao, covered = coat.bake([o for os_ in groups.values() for o in os_], size)
    finally:
        coat.SIZE = coat_size
    flat_p, flat_n = pos.reshape(-1, 3), nrm.reshape(-1, 3)
    out = np.zeros((size * size, 3), np.float32)
    v, u = np.mgrid[0:size, 0:size]
    u, v = ((u + 0.5) / size).reshape(-1), ((v + 0.5) / size).reshape(-1)
    cov = covered.reshape(-1)
    # Which object owns each texel: each gets its own colour (its material key's), painted
    # in its painter's style; one painter's objects share its cell.
    every = [o for _n, os_ in sorted(groups.items()) for o in os_]
    owner = _labels(every, size, covered) if any(len(os_) > 1 for os_ in groups.values()) else None
    for i, (name, os_) in enumerate(sorted(groups.items())):
        u0, v0, u1, v1 = rects[name]
        m = cov & (u >= u0) & (u < u1) & (v >= v0) & (v < v1)
        for sel, rgb in split_by_colour(os_, colours, owner, every, m):
            if sel.any():
                out[sel] = paint(KIND.get(name, name), flat_p[sel], flat_n[sel], rgb, L, seed=11 + 7 * i)
        for e in getattr(ch, "paint_masks", ()):
            if name in {PAINTER.get(k) for k in e["keys"]} and m.any():
                out[m] = apply_mask(e, flat_p[m], flat_n[m], out[m], L, seed=11 + 7 * i)
    occ = np.clip(ao.reshape(-1), 0.0, 1.0)
    out *= (0.55 + 0.45 * occ ** 0.8)[:, None]
    px = np.ones((size * size, 4), np.float32)
    px[:, :3] = coat.to_srgb(out)
    import bpy

    img = bpy.data.images.new(f"{ch.name}.outfit", size, size, alpha=False)
    img.pixels.foreach_set(px.ravel())
    img.file_format = "PNG"
    img.pack()
    for os_ in groups.values():
        for o in os_:
            key = ch.keys[o.name]
            m = decay.material(f"{ch.name}.{key}.outfit", layer_of[key], colours.get(o.name, (0.5, 0.5, 0.5)),
                               image=img, uv_map="atlas", interpolation="Linear")
            m["acr_colour_from"] = "outfit"
            painter = PAINTER[key]
            m["acr_roughness"], m["acr_sheen"], m["acr_gloss"] = FINISH.get(painter, (0.9, 0.0, 0.0))
            o.data.materials.clear()
            o.data.materials.append(m)
    return img


# -- 17th century (docs/clothing.md, "17th century") -------------------------------------

def paint_buff(p, n, rgb, L, seed):
    """Buff leather: oil-tanned hide with a napped (suede-like) face, matte; a soft cloudy
    mottle where the oil took unevenly, a faint grain, darkened with grease and dirt on the
    front edges, the cuffs and the skirt hems. Its finish (`FINISH["buff"]`) gives the nap
    a soft sheen at grazing angles and no gloss."""
    base = coat.lin(rgb)
    cloud = coat.fbm(p, (18.0, 18.0, 12.0), seed, octaves=3)
    grain = coat.fbm(p, 260.0, seed + 1, octaves=1)
    col = base * (0.88 + 0.16 * cloud + 0.05 * grain)[:, None]
    hem = coat.smooth(0.70, 0.55, p[:, 2]) * 0.35
    front = coat.smooth(0.05, 0.0, np.abs(p[:, 0])) * coat.smooth(-0.08, -0.14, p[:, 1]) * 0.30
    grease = np.clip(hem + front + coat.smooth(0.65, 0.85, coat.fbm(p, 9.0, seed + 3)) * 0.18, 0.0, 0.5)
    return coat.mix(col, base * 0.55 + coat.lin((0.05, 0.04, 0.02)), grease)


registry.painter("buff", fn=paint_buff, finish=(0.92, 0.30, 0.02))
registry.material("buff", "leather", painter="buff", fallback=(0.74, 0.58, 0.36))
# The lining showing through slashed sleeves and paned slops (silk or linen).
registry.painter("slashes", like="linen", finish=(0.80, 0.35, 0.05))
registry.material("slashes", "linen", painter="slashes", fallback=(0.86, 0.80, 0.62))
# The falling band: starched white linen.
registry.painter("fallband", like="linen", finish=(0.92, 0.15, 0.0))
registry.material("fallband", "linen", painter="fallband", fallback=(0.92, 0.91, 0.86))
# Bucket-top boots: brown or black waxed leather (no gaiter buttons: painted like leather).
registry.painter("jackboots", like="leather", finish=(0.6, 0.0, 0.18))
registry.material("jackboots", "leather", painter="jackboots", fallback=(0.20, 0.13, 0.08))
