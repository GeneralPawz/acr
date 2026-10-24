"""Trees, bushes and rocks from downloaded libraries (Poly Haven CC0, Sketchfab CC-BY; needs
`bpy`). One script per model under `assets/nature/` holds a `SPEC` and calls `build(ctx, SPEC)`:

    source    path below `$ACR_SOURCES` (e.g. "sketchfab2/oak-tree/scene.gltf")
    nodes     node-name parts to keep (empty: all meshes)
    kind      "tree", "bush" or "rock"
    height    metres from the ground point to the top (uniform scale)
    budget    triangle budget of the whole model
    leaves    material-name parts of the leaf cards (alpha mask, wind flutter)
    snow      whiten the leaf cards (0..1) for a snow-laden variant
    recolour  turn red texels leaf green (atlases with an autumn crown only)
    max_width narrow a too wide crown to this many metres (x and y)
    turn      degrees about +Z (models face -Y)
    wind      the species' sway (below): {"trunk", "branch", "flutter", "tempo"} scales (default
              WIND[kind]), "bend" the trunk's exponent (2), "crown" flutter on the bark above
              this share of the height for crowns without leaf cards (birch, pine)

What it does: imports and joins the kept nodes, puts the trunk base on the ground point,
scales to `height`, cuts to `budget` (bark: collapse decimation; leaf cards: whole cards are
dropped by a seeded draw and the kept ones grow to keep the crown's cover), rebuilds every
material as one base-colour PNG (max 512 px) on a Principled BSDF, leaf cards through a Round
node on the image alpha (exported as `alphaMode: MASK`), and removes all colour attributes:
the renderer multiplies `COLOR_0` into the colour, and the sources' baked vertex colours
(black or AO) made crowns dark. Wind (`_WIND`, the same channels as `broadleaf.py`): x trunk
bend (height / top)² times min(1, top / 6 m) (a bush bends less than a tree), y branch weight
(distance from the trunk axis / crown radius), z phase per clump (leaf cards grouped in cells of
half the crown radius), w 1 on leaf cards; rocks carry zeros. The root's `extras.acr_wind`
[trunk, branch, flutter, tempo] scales the shader's sway per species (docs/rendering-3d.md
"Wind").

Stats: `source_triangles`, `triangles`, `size_m` [x, y, z], `leaf_rgb` (mean sRGB colour of
the opaque leaf texels after levelling: the crown colour the renderer starts from), `leaf_gain`
(per source material, the brightening `_level` applied), `cards` [kept, source].
"""

import math
import random

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

from . import vessels as vs

LEAF_WORDS = ("leaf", "leav", "twig", "frond", "needle", "branches", "bush", "shrub", "fern", "phong3")
MAX_TEX = 512
# Default wind scales by kind ([trunk, branch, flutter, tempo]); a SPEC's "wind" overrides keys.
WIND = {"tree": (1.0, 1.0, 1.0, 1.0), "bush": (1.0, 1.0, 1.5, 1.8), "rock": (0.0, 0.0, 0.0, 1.0)}
# Mean sRGB level of a leaf card's opaque texels after `_level` (the original's crowns: about
# 0.30-0.40 on TREES.GP).
LEAF_LEVEL = 0.33


def _keep(objs, parts):
    if not parts:
        return objs
    keep = [o for o in objs if any(p.lower() in o.name.lower() for p in parts)]
    for o in objs:
        if o not in keep:
            bpy.data.objects.remove(o, do_unlink=True)
    if not keep:
        raise SystemExit(f"no node matches {parts}")
    return keep


def _image_of(mat):
    """The image feeding the base colour (or any image in the tree), and the flat colour."""
    if mat is None or not mat.use_nodes:
        return None, (0.5, 0.5, 0.5, 1.0)
    nt = mat.node_tree
    bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
    flat = tuple(bsdf.inputs["Base Color"].default_value) if bsdf else (0.5, 0.5, 0.5, 1.0)
    if bsdf is not None:
        stack = [link.from_node for link in bsdf.inputs["Base Color"].links]
        seen = set()
        while stack:
            n = stack.pop()
            if n.name in seen:
                continue
            seen.add(n.name)
            if n.type == "TEX_IMAGE" and n.image is not None:
                return n.image, flat
            for inp in n.inputs:
                stack += [link.from_node for link in inp.links]
    return None, flat


def _is_leaf(name, words):
    n = name.lower()
    return any(w in n for w in words)


def _pixels(img):
    px = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(px)
    return px.reshape(-1, 4)


def _put(img, px, suffix):
    out = bpy.data.images.new(img.name + suffix, img.size[0], img.size[1], alpha=True)
    out.pixels.foreach_set(np.clip(px, 0, 1).ravel())
    return out


def _whiten(img, amount):
    """Snow on the leaf cards: towards blue-white, more on the bright (lit, upper) texels."""
    px = _pixels(img)
    lum = px[:, :3].mean(axis=1, keepdims=True)
    k = np.clip(amount * (0.6 + lum), 0, 1)
    px[:, :3] = px[:, :3] * (1 - k) + np.array([0.80, 0.84, 0.90], np.float32) * k
    return _put(img, px, ".snow")


def _green(img):
    """Red texels (r well above g and b) become leaf green: the Proto Series atlases carry
    only their red (autumn) crown strip."""
    px = _pixels(img)
    r, g, b = px[:, 0].copy(), px[:, 1].copy(), px[:, 2].copy()
    red = (r > 1.3 * g) & (r > 1.3 * b)
    px[red, 0] = g[red] * 0.6 + 0.05
    px[red, 1] = r[red] * 0.62
    px[red, 2] = b[red] * 0.5
    return _put(img, px, ".green")


def _level(img, target, opaque):
    """Brightens the image so its mean (sRGB, over the opaque texels of a card) reaches
    `target`: the sources' leaf atlases are often dark (oak 0.16), and the renderer's Lambert
    with half ambient darkens them further. Never darkens; gain at most 2.5. The transparent
    texels take the mean opaque colour: their source colours (black, magenta) bled into the
    mip levels and turned distant or grazing crowns dark and purple."""
    px = _pixels(img)
    sel = px[:, 3] > 0.5 if opaque else np.ones(len(px), bool)
    if not sel.any():
        return _put(img, px, ".png"), 1.0
    mean = px[sel, :3].mean(axis=0)
    gain = min(2.5, max(1.0, target / max(float(mean.mean()), 1e-3)))
    px[:, :3] *= gain
    px[~sel, :3] = mean * gain
    return _put(img, px, ".lit"), gain


def _materials(obj, leaf_words, snow, level, gains, recolour=False):
    """Rebuilds every slot as image -> Principled (leaf cards: alpha -> Round -> Alpha)."""
    leaf_slots = set()
    done = {}
    for i, slot in enumerate(obj.material_slots):
        src = slot.material
        name = src.name if src else f"mat{i}"
        leaf = _is_leaf(name, leaf_words)
        if leaf:
            leaf_slots.add(i)
        if name in done:
            slot.material = done[name]
            continue
        img, flat = _image_of(src)
        if img is not None and img.size[0] == 0 and img.filepath:
            img.reload()
        if img is not None and max(img.size) > MAX_TEX:
            k = MAX_TEX / max(img.size)
            img.scale(max(1, int(img.size[0] * k)), max(1, int(img.size[1] * k)))
        if img is not None and recolour:
            img = _green(img)
        if img is not None and leaf:
            img, gains[name] = _level(img, level, True)
            if snow:
                img = _whiten(img, snow)
        elif img is not None:
            img = _put(img, _pixels(img), ".png")
        mat = bpy.data.materials.new(f"{obj.name}.{'leaves' if leaf else 'bark'}.{i}")
        mat.use_nodes = True
        nt = mat.node_tree
        nt.nodes.clear()
        out = nt.nodes.new("ShaderNodeOutputMaterial")
        bsdf = nt.nodes.new("ShaderNodeBsdfPrincipled")
        nt.links.new(bsdf.outputs[0], out.inputs[0])
        bsdf.inputs["Roughness"].default_value = 0.9
        bsdf.inputs["Metallic"].default_value = 0.0
        if img is not None:
            tex = nt.nodes.new("ShaderNodeTexImage")
            tex.image = img
            nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
            if leaf:
                rnd = nt.nodes.new("ShaderNodeMath")
                rnd.operation = "ROUND"
                nt.links.new(tex.outputs["Alpha"], rnd.inputs[0])
                nt.links.new(rnd.outputs[0], bsdf.inputs["Alpha"])
        else:
            bsdf.inputs["Base Color"].default_value = flat
        mat["acr_nature"] = "leaves" if leaf else "bark"
        done[name] = mat
        slot.material = mat
    for img in bpy.data.images:
        if img.users and img.size[0]:
            img.file_format = "PNG"
            img.filepath_raw = (img.filepath_raw.rsplit(".", 1)[0] if img.filepath_raw else img.name) + ".png"
            img.pack()
    return leaf_slots


def _leaf_rgb(obj, leaf_slots):
    """Mean sRGB colour of the opaque texels of the leaf images."""
    tot, n = [0.0, 0.0, 0.0], 0
    for i in sorted(leaf_slots):
        img, _flat = _image_of(obj.material_slots[i].material)
        if img is None:
            continue
        px = img.pixels[:]
        for j in range(0, len(px), 4 * 7):
            if px[j + 3] > 0.5:
                for c in range(3):
                    tot[c] += px[j + c]
                n += 1
    return [round(t / n, 3) for t in tot] if n else None


def _islands(bm, faces):
    """Loose parts among `faces` (lists of faces)."""
    left = set(faces)
    out = []
    while left:
        f = left.pop()
        part, stack = [f], [f]
        while stack:
            g = stack.pop()
            for e in g.edges:
                for h in e.link_faces:
                    if h in left:
                        left.remove(h)
                        part.append(h)
                        stack.append(h)
        out.append(part)
    return out


def _thin_cards(obj, leaf_slots, target, seed):
    """Drops whole leaf cards until about `target` leaf triangles remain; the kept cards grow
    by sqrt(source / kept) (at most 1.8) about their centre. Returns (kept, source) cards."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    leaf = [f for f in bm.faces if f.material_index in leaf_slots]
    tris = sum(len(f.verts) - 2 for f in leaf)
    parts = _islands(bm, leaf)
    if tris <= target or len(parts) < 4:
        bm.free()
        return len(parts), len(parts)
    rnd = random.Random(seed)
    parts.sort(key=lambda p: min(f.index for f in p))
    order = list(range(len(parts)))
    rnd.shuffle(order)
    keep_n = max(3, int(len(parts) * target / tris))
    keep = set(order[:keep_n])
    grow = min(1.8, math.sqrt(len(parts) / keep_n))
    drop = []
    for k, p in enumerate(parts):
        if k in keep:
            vs_ = {v for f in p for v in f.verts}
            c = sum((v.co for v in vs_), Vector()) / len(vs_)
            for v in vs_:
                v.co = c + (v.co - c) * grow
        else:
            drop += p
    bmesh.ops.delete(bm, geom=drop, context="FACES")
    bm.to_mesh(obj.data)
    bm.free()
    return keep_n, len(parts)


def _split_decimate(obj, leaf_slots, target):
    """Collapse-decimates the bark faces only (separated, decimated, joined back)."""
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="DESELECT")
    bpy.ops.object.mode_set(mode="OBJECT")
    bark_faces = 0
    for p in obj.data.polygons:
        p.select = p.material_index not in leaf_slots
        bark_faces += p.select
    if not bark_faces or vs.triangles([obj]) <= 0:
        return obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.separate(type="SELECTED")
    bpy.ops.object.mode_set(mode="OBJECT")
    parts = [o for o in bpy.context.selected_objects if o != obj]
    if not parts:
        return obj
    bark = parts[0]
    vs.decimate(bark, target)
    return vs.join([obj, bark], obj.name)


def _wind(obj, kind, leaf_slots, seed, bend_pow=2.0, crown=None):
    me = obj.data
    a = me.attributes.new("_wind", "FLOAT_COLOR", "POINT")
    if kind == "rock":
        for d in a.data:
            d.color = (0.0, 0.0, 0.0, 0.0)
        return 0.0
    top = max(v.co.z for v in me.vertices) or 1.0
    leafy = set()
    for p in me.polygons:
        if p.material_index in leaf_slots:
            leafy.update(p.vertices)
    pool = [me.vertices[i].co for i in leafy] or [v.co for v in me.vertices]
    crown_r = max(0.3, max(Vector((c.x, c.y)).length for c in pool))
    bend = min(1.0, top / 6.0)
    cell = crown_r / 2.0
    rnd = random.Random(seed)
    phases = {}
    # Phase per clump: the cell of the vertex's position (whole cards share one, as their
    # vertices are close; a card across a cell edge splits, which only shifts its flutter).
    for i, v in enumerate(me.vertices):
        co = v.co
        key = (int(math.floor(co.x / cell)), int(math.floor(co.y / cell)), int(math.floor(co.z / cell)))
        if key not in phases:
            phases[key] = None
    for key in sorted(phases):
        phases[key] = rnd.random()
    for i, v in enumerate(me.vertices):
        co = v.co
        key = (int(math.floor(co.x / cell)), int(math.floor(co.y / cell)), int(math.floor(co.z / cell)))
        horiz = Vector((co.x, co.y)).length
        h = min(max(co.z, 0.0) / top, 1.0)
        x = bend * h ** bend_pow
        y = min(horiz / crown_r, 1.0) * min(1.0, crown_r / 2.5)
        w = 1.0 if i in leafy else 0.0
        if crown is not None and not leafy:
            # No leaf cards (one bark material): the fine twigs of the upper crown flutter.
            w = min(1.0, max(0.0, (h - crown) / (1.0 - crown)) * 1.5) * min(1.0, horiz / crown_r * 2.0)
        a.data[i].color = (x, y, phases[key] if w else 0.0, w)
    return crown_r


def white(obj):
    """A white `COLOR_0`, active and render-active. `_wind` is a colour attribute too: as the
    only one it became `COLOR_0`, which the renderer multiplies into the colour (rocks black,
    crowns tinted purple by the phase and flutter channels)."""
    me = obj.data
    a = me.color_attributes.new("col", "BYTE_COLOR", "POINT")
    for d in a.data:
        d.color = (1.0, 1.0, 1.0, 1.0)
    me.color_attributes.active_color = a
    me.color_attributes.render_color_index = me.color_attributes.find("col")


def build(ctx, spec):
    kind = spec.get("kind", "tree")
    objs = vs.import_file(vs.source(*spec["source"].split("/")))
    objs = _keep(objs, spec.get("nodes", []))
    source_tris = vs.triangles(objs)
    obj = vs.join(objs, f"{ctx.name}.mesh")
    for attr in list(obj.data.color_attributes):
        obj.data.color_attributes.remove(attr)
    leaf_words = spec.get("leaves", LEAF_WORDS if kind != "rock" else ())
    gains = {}
    leaf_slots = _materials(obj, leaf_words, spec.get("snow", 0.0), spec.get("leaf_level", LEAF_LEVEL), gains,
                            spec.get("recolour", False))
    budget = spec.get("budget", 6000)
    cards = None
    if leaf_slots:
        leaf_tris = sum(len(p.vertices) - 2 for p in obj.data.polygons if p.material_index in leaf_slots)
        bark_tris = vs.triangles([obj]) - leaf_tris
        leaf_target = max(budget - min(bark_tris, int(budget * spec.get("bark_share", 0.4))), budget // 3)
        cards = _thin_cards(obj, leaf_slots, leaf_target, spec.get("seed", 7))
        obj = _split_decimate(obj, leaf_slots, budget - min(leaf_target, leaf_tris))
    else:
        vs.decimate(obj, budget)
    me = obj.data
    # Ground point: the centre of the lowest tenth (the trunk base), lowest vertex at 0.
    lo = min(v.co.z for v in me.vertices)
    hi = max(v.co.z for v in me.vertices)
    base = [v.co for v in me.vertices if v.co.z < lo + 0.1 * (hi - lo)]
    cx = sum(c.x for c in base) / len(base)
    cy = sum(c.y for c in base) / len(base)
    if kind == "rock":
        xs = [v.co.x for v in me.vertices]
        ys = [v.co.y for v in me.vertices]
        cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    k = spec["height"] / (hi - lo)
    # A crown wider than `max_width` (m) narrows to it (x and y only).
    width = max(max(v.co.x for v in me.vertices) - min(v.co.x for v in me.vertices),
                max(v.co.y for v in me.vertices) - min(v.co.y for v in me.vertices)) * k
    kxy = min(1.0, spec.get("max_width", 1e9) / width)
    sink = spec.get("sink", 0.03 if kind != "rock" else 0.08)
    vs.transform([obj], Matrix.Rotation(math.radians(spec.get("turn", 0.0)), 4, "Z")
                 @ Matrix.Diagonal((k * kxy, k * kxy, k, 1.0))
                 @ Matrix.Translation((-cx, -cy, -lo - sink / k)))
    obj.parent = ctx.root
    for p in me.polygons:
        p.use_smooth = kind == "rock" or p.material_index not in leaf_slots
    wind = dict(zip(("trunk", "branch", "flutter", "tempo"), WIND[kind]))
    wind.update({k: v for k, v in spec.get("wind", {}).items() if k in wind})
    crown_r = _wind(obj, kind, leaf_slots, spec.get("seed", 7), spec.get("wind", {}).get("bend", 2.0),
                    spec.get("wind", {}).get("crown"))
    white(obj)
    me.calc_loop_triangles()
    xs = [v.co.x for v in me.vertices]
    ys = [v.co.y for v in me.vertices]
    zs = [v.co.z for v in me.vertices]
    ctx.root["acr_nature"] = kind
    wind_k = [round(float(wind[k]), 3) for k in ("trunk", "branch", "flutter", "tempo")]
    ctx.root["acr_wind"] = wind_k
    ctx.stats = {
        "kind": kind,
        "source": spec["source"],
        "source_triangles": source_tris,
        "triangles": len(me.loop_triangles),
        "size_m": [round(max(xs) - min(xs), 2), round(max(ys) - min(ys), 2), round(max(zs), 2)],
        "crown_r_m": round(crown_r, 2),
        "wind": wind_k,
        "leaf_rgb": _leaf_rgb(obj, leaf_slots),
        "cards": cards,
        "leaf_gain": {k: round(v, 2) for k, v in sorted(gains.items())},
        "materials": sorted(s.material.name for s in obj.material_slots if s.material),
    }


def build_generated(ctx, spec):
    """A map prop from a generated model (Pixal3D or Rodin, `$ACR_SOURCES/generated/<source>`,
    local only) through `genfit.fit` (prop category: backdrop dropped, decimated to the
    item's `tris`, base colour baked, standing on z = 0, facing -Y at its `size`). SPEC:

        source  path below the generated sources ("pixal3d/prop_ruin_wall.glb")
        wind    None (hard props: zero `_WIND`), "stalk" (sways with height, leaves flutter:
                maize, dry bushes) or "green" (green texels flutter: pumpkin leaves);
                `genfit.prop_wind`
        sink    metres below the ground point (default 0.03; on slopes nothing floats)
        turn    degrees about +Z

    The baked normal map is dropped (the renderer reads base colour only). Stats: the
    `genfit.fit` numbers (`source_triangles`, `triangles`, `deviation`, `tex_unbaked_frac`,
    `backdrop_faces_removed`, `size_m`) plus `wind`, `flutter_frac`."""
    from . import genfit

    low, st = genfit.fit(spec["source"], name=f"{ctx.name}.mesh")
    me = low.data
    # base colour only: drop the normal-map chain and its image
    for m in me.materials:
        nt = m.node_tree
        for n in list(nt.nodes):
            if n.type == "NORMAL_MAP" or (n.type == "TEX_IMAGE" and n.image and n.image.name.endswith(".nrm")):
                if n.type == "TEX_IMAGE":
                    img = n.image
                    nt.nodes.remove(n)
                    bpy.data.images.remove(img)
                else:
                    nt.nodes.remove(n)
    img = next(n.image for n in me.materials[0].node_tree.nodes if n.type == "TEX_IMAGE" and n.image)
    px = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    st["tex_black_frac"] = round(float((px[:, :3].max(1) < 0.05).mean()), 3)
    st["tex_mean_rgb"] = [round(float(c), 3) for c in px[:, :3].mean(0)]
    me.validate()
    sink = spec.get("sink", 0.03)
    vs.transform([low], Matrix.Rotation(math.radians(spec.get("turn", 0.0)), 4, "Z") @ Matrix.Translation((0, 0, -sink)))
    low.parent = ctx.root
    # per-vertex baked colour (for "green" flutter)
    mode = spec.get("wind")
    rgb = None
    if mode == "green":
        img = next(n.image for n in me.materials[0].node_tree.nodes if n.type == "TEX_IMAGE" and n.image)
        w, h = img.size
        px = np.empty(w * h * 4, np.float32)
        img.pixels.foreach_get(px)
        px = px.reshape(h, w, 4)
        uv = me.uv_layers.active.data
        acc = np.zeros((len(me.vertices), 4))
        for lp in me.loops:
            u, v = uv[lp.index].uv
            c = px[min(h - 1, max(0, int(v * h))), min(w - 1, max(0, int(u * w))), :3]
            acc[lp.vertex_index, :3] += c
            acc[lp.vertex_index, 3] += 1
        rgb = acc[:, :3] / np.maximum(acc[:, 3:], 1)
    top = max(v.co.z for v in me.vertices)
    reach = max(0.3, max(Vector((v.co.x, v.co.y)).length for v in me.vertices))
    cell = reach / 2.0
    rnd = random.Random(spec.get("seed", 7))
    keys = {}
    for v in me.vertices:
        keys[(int(math.floor(v.co.x / cell)), int(math.floor(v.co.y / cell)), int(math.floor(v.co.z / cell)))] = None
    for k in sorted(keys):
        keys[k] = rnd.random()
    a = me.attributes.new("_wind", "FLOAT_COLOR", "POINT")
    flutter = 0
    for i, v in enumerate(me.vertices):
        co = v.co
        ph = keys[(int(math.floor(co.x / cell)), int(math.floor(co.y / cell)), int(math.floor(co.z / cell)))]
        ch = genfit.prop_wind(co.z, top, Vector((co.x, co.y)).length, reach, mode,
                              tuple(rgb[i]) if rgb is not None else None, ph)
        a.data[i].color = ch
        flutter += ch[3] > 0
    white(low)
    ctx.root["acr_nature"] = "prop"
    ctx.root["acr_wind"] = "_WIND: x trunk bend, y branch, z phase, w leaf flutter"
    xs = [v.co.x for v in me.vertices]
    ys = [v.co.y for v in me.vertices]
    zs = [v.co.z for v in me.vertices]
    st.update({
        "kind": "prop",
        "source": spec["source"],
        "size_m": [round(max(xs) - min(xs), 2), round(max(ys) - min(ys), 2), round(max(zs), 2)],
        "wind": mode,
        "flutter_frac": round(flutter / max(1, len(me.vertices)), 3),
    })
    ctx.stats = st
