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
# The draped garments' facings, lining and stock have own painters, so that they get their
# own atlas cells and colours (a painter's cell has one colour); `KIND` maps such painters
# to the one they paint like. `FINISH`: how each painter's cloth takes the light
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
            for o in os_:
                coat.atlas_uv(o, rects[name])
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
    for i, (name, os_) in enumerate(sorted(groups.items())):
        u0, v0, u1, v1 = rects[name]
        m = cov & (u >= u0) & (u < u1) & (v >= v0) & (v < v1)
        rgb = colours.get(os_[0].name) or (0.5, 0.5, 0.5)
        out[m] = paint(KIND.get(name, name), flat_p[m], flat_n[m], rgb, L, seed=11 + 7 * i)
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
