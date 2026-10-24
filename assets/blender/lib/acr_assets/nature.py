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
    dry       0..1: leaf cards towards a darker straw brown (dead, dry bushes; wilting crowns)
    keep      share of the leaf cards kept, without growing the kept ones (a sparse crown)
    lie       a fallen tree: {"deg": tip about +Y, crown towards +X, "squash": height factor
              of the lying crown (broken branches)}; centred on its ground point, trunk and
              branch sway 0, the leaves' flutter kept
    wind      the species' sway (below): {"trunk", "branch", "flutter", "tempo"} scales (default
              WIND[kind]), "bend" the trunk's exponent (2), "crown" flutter on the bark above
              this share of the height for crowns without leaf cards (birch, pine)

What it does: imports and joins the kept nodes, puts the trunk base on the ground point,
scales to `height`, cuts to `budget` (bark: collapse decimation; leaf cards: whole cards are
dropped by a seeded draw and the kept ones grow to keep the crown's cover), rebuilds every
material as one base-colour PNG (max 512 px) on a Principled BSDF (roughness 0.9, metallic 0,
the source's tangent normal map kept as a linear PNG of at most 512 px where it has one:
`normal_maps` in the stats; the collapse decimation keeps the uvs), leaf cards through a Round
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
from mathutils.bvhtree import BVHTree

from . import breaks, detail
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


def _normal_of(mat):
    """(image, strength) of the source's tangent-space normal map (the image behind a Normal
    Map node on the Principled Normal input), or (None, 1.0)."""
    if mat is None or not mat.use_nodes:
        return None, 1.0
    bsdf = next((n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
    if bsdf is None or not bsdf.inputs["Normal"].links:
        return None, 1.0
    nm = bsdf.inputs["Normal"].links[0].from_node
    if nm.type != "NORMAL_MAP" or nm.space != "TANGENT" or not nm.inputs["Color"].links:
        return None, 1.0
    tex = nm.inputs["Color"].links[0].from_node
    if tex.type != "TEX_IMAGE" or tex.image is None:
        return None, 1.0
    return tex.image, float(nm.inputs["Strength"].default_value)


def normal_png(img, size, name):
    """A packed linear PNG copy of normal map `img`, at most `size` px (glTF normalTexture:
    OpenGL convention, green +Y, which is what Blender bakes and glTF sources carry)."""
    if img.size[0] == 0 and img.filepath:
        img.reload()
    w, h = img.size
    if w == 0:
        return None
    px = _pixels(img)
    out = bpy.data.images.new(name, w, h, alpha=False)
    out.colorspace_settings.name = "Non-Color"
    out.pixels.foreach_set(np.clip(px, 0, 1).ravel())
    if max(w, h) > size:
        k = size / max(w, h)
        out.scale(max(1, int(w * k)), max(1, int(h * k)))
    out.file_format = "PNG"
    out.filepath_raw = name + ".png"
    out.pack()
    return out


def bind_normal(nt, bsdf, img, strength=1.0):
    """Normal Map (tangent) of `img` into the Principled Normal: exported as normalTexture
    with `scale` = strength."""
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    tex.interpolation = "Linear"
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nm.inputs["Strength"].default_value = strength
    nt.links.new(tex.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])


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


def _dry(img, amount):
    """Dead, dry foliage: towards a darker straw brown (TREES.GP 143-172: about sRGB 0.35, 0.28,
    0.18 on the twigs); the whole card darkens by up to 40 % with `amount` (`_level` only
    brightens)."""
    px = _pixels(img)
    lum = (px[:, :3] @ np.array([0.3, 0.55, 0.15], np.float32))[:, None]
    brown = lum * np.array([1.25, 1.0, 0.72], np.float32)
    px[:, :3] = (px[:, :3] * (1 - amount) + brown * amount) * (1 - 0.4 * amount)
    return _put(img, px, ".dry")


def _materials(obj, leaf_words, snow, level, gains, recolour=False, normal_sizes=None, dry=0.0):
    """Rebuilds every slot as image -> Principled (leaf cards: alpha -> Round -> Alpha)."""
    leaf_slots = set()
    done = {}
    normals = {} if normal_sizes is None else normal_sizes
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
        nimg, nstr = _normal_of(src)
        if img is not None and img.size[0] == 0 and img.filepath:
            img.reload()
        if img is not None and max(img.size) > MAX_TEX:
            k = MAX_TEX / max(img.size)
            img.scale(max(1, int(img.size[0] * k)), max(1, int(img.size[1] * k)))
        if img is not None and recolour:
            img = _green(img)
        if img is not None and leaf:
            img, gains[name] = _level(img, level, True)
            if dry:
                img = _dry(img, dry)
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
        if nimg is not None:
            # the source's normal map, kept through the decimation (collapse keeps the uvs)
            n = normal_png(nimg, MAX_TEX, f"{obj.name}.{i}.nrm")
            if n is not None:
                bind_normal(nt, bsdf, n, nstr)
                normals[name] = list(n.size)
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


def _thin_cards(obj, leaf_slots, target, seed, grow=True):
    """Drops whole leaf cards until about `target` leaf triangles remain; the kept cards grow
    by sqrt(source / kept) (at most 1.8) about their centre unless `grow` is off. Returns
    (kept, source) cards."""
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
    grow = min(1.8, math.sqrt(len(parts) / keep_n)) if grow else 1.0
    drop = []
    for k, p in enumerate(parts):
        if k in keep:
            vs_ = list(dict.fromkeys(v for f in p for v in f.verts))
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


def _lay_down(obj, lie):
    """A fallen tree (TREES.GP 431-446): tipped `deg` about +Y around a cut 0.3 m up (the crown
    towards +X), the height times `squash` (the crown breaks on the ground), whatever would go
    into the ground squashed onto it, then centred on its ground point (the middle of the
    footprint, as the sprites' DX DY). Trunk and branch sway 0, the leaves keep their flutter."""
    me = obj.data
    me.transform(Matrix.Rotation(math.radians(lie.get("deg", 88.0)), 4, "Y") @ Matrix.Translation((0, 0, -0.3)))
    k = lie.get("squash", 1.0)
    for v in me.vertices:
        z = v.co.z
        if z < 0.03:
            z = 0.03 + 0.1 * (z - 0.03) / (1 + abs(z))
        v.co.z = 0.03 + (z - 0.03) * k if z > 0.03 else z
    xs = [v.co.x for v in me.vertices]
    ys = [v.co.y for v in me.vertices]
    lo = min(v.co.z for v in me.vertices)
    me.transform(Matrix.Translation((-(min(xs) + max(xs)) / 2, -(min(ys) + max(ys)) / 2, -lo - 0.02)))
    for d in me.attributes["_wind"].data:
        c = d.color
        d.color = (0.0, 0.0, c[2], c[3])


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
    normals = {}
    leaf_slots = _materials(obj, leaf_words, spec.get("snow", 0.0), spec.get("leaf_level", LEAF_LEVEL), gains,
                            spec.get("recolour", False), normals, spec.get("dry", 0.0))
    budget = spec.get("budget", 6000)
    cards = None
    if leaf_slots and spec.get("keep"):
        leaf_tris = sum(len(p.vertices) - 2 for p in obj.data.polygons if p.material_index in leaf_slots)
        sparse = _thin_cards(obj, leaf_slots, int(leaf_tris * spec["keep"]), spec.get("seed", 7) + 1, grow=False)
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
    if spec.get("lie"):
        _lay_down(obj, spec["lie"])
    white(obj)
    struck = None
    if kind == "tree" and not spec.get("lie"):
        struck = _tree_states(ctx, obj, spec.get("seed", 7))
    elif kind == "rock":
        struck = _chip(ctx, obj, spec.get("seed", 7))
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
        "sparse_cards": sparse if leaf_slots and spec.get("keep") else None,
        "leaf_gain": {k: round(v, 2) for k, v in sorted(gains.items())},
        "materials": sorted(s.material.name for s in obj.material_slots if s.material),
        "normal_maps": {k: v for k, v in sorted(normals.items())},
        "struck": struck,
    }


# Fresh split wood (sRGB): pale, yellowish, as oak and pine look just after a break.
FRESH_WOOD = (0.82, 0.70, 0.50)


def _tile_material(name, rgb, kind):
    """A Principled material on a `detail` tile of colour `rgb` (sRGB), on uv map `detail`."""
    mat = bpy.data.materials.new(name)
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = detail.tile(name, rgb, kind)
    uv = nt.nodes.new("ShaderNodeUVMap")
    uv.uv_map = "detail"
    nt.links.new(uv.outputs["UV"], tex.inputs["Vector"])
    nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.9
    mat["acr_colour"] = list(rgb)
    return mat


def _part(ctx, name, bm, mats):
    """Object `name` under the root from bmesh `bm` (freed): materials `mats` by face
    `material_index`, no sway, white `COLOR_0`, `detail` uvs, flat shaded."""
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for mt in mats:
        me.materials.append(mt)
    obj = bpy.data.objects.new(name, me)
    obj.parent = ctx.root
    bpy.context.scene.collection.objects.link(obj)
    a = me.attributes.new("_wind", "FLOAT_COLOR", "POINT")
    for d in a.data:
        d.color = (0.0, 0.0, 0.0, 0.0)
    white(obj)
    detail.box_uv(obj, scale=6.0)
    for p in me.polygons:
        p.use_smooth = False
    return obj


def _copy_cut(ctx, obj, name, z, above, keep=None):
    """A copy of `obj` (materials, uvs, attributes) cut by the plane at height `z`, the part
    above it (`above`) or below, and of that only the faces `keep(face)` accepts."""
    me = obj.data.copy()
    me.name = name
    bm = bmesh.new()
    bm.from_mesh(me)
    geom = list(bm.verts) + list(bm.edges) + list(bm.faces)
    bmesh.ops.bisect_plane(bm, geom=geom, dist=1e-5, plane_co=(0, 0, z), plane_no=(0, 0, 1),
                           clear_inner=above, clear_outer=not above)
    drop = [f for f in bm.faces if keep is not None and not keep(f)]
    if drop:
        bmesh.ops.delete(bm, geom=drop, context="FACES")
    bm.to_mesh(me)
    bm.free()
    out = bpy.data.objects.new(name, me)
    out.parent = ctx.root
    bpy.context.scene.collection.objects.link(out)
    return out


def _texel_rgb(obj, pick, opaque=True, texel=None):
    """Mean sRGB colour of the texels under the faces `pick(poly)` accepts (their loops' uvs
    and the face centre's in the base colour image of each face's material); None without any.
    `opaque` skips texels with alpha under 0.5 (leaf cards); bark images often carry something
    else in alpha. `texel(rgb)` may reject texels by colour."""
    me = obj.data
    uv = me.uv_layers.active
    if uv is None:
        return None
    pixels = {}
    tot, n = [0.0, 0.0, 0.0], 0
    for p in me.polygons:
        if not pick(p):
            continue
        img, _flat = _image_of(obj.material_slots[p.material_index].material if obj.material_slots else None)
        if img is None or not img.size[0]:
            continue
        if img.name not in pixels:
            pixels[img.name] = np.array(img.pixels[:], np.float32).reshape(img.size[1], img.size[0], 4)
        px = pixels[img.name]
        uvs = [uv.data[li].uv for li in p.loop_indices]
        mid = (sum(u for u, _v in uvs) / len(uvs), sum(v for _u, v in uvs) / len(uvs))
        for u, v in uvs + [mid]:
            x = int((u % 1.0) * (px.shape[1] - 1))
            y = int((v % 1.0) * (px.shape[0] - 1))
            if (not opaque or px[y, x, 3] > 0.5) and (texel is None or texel(px[y, x, :3])):
                for c in range(3):
                    tot[c] += float(px[y, x, c])
                n += 1
    return [round(t / n, 3) for t in tot] if n else None


def _srgb(c):
    """Linear (a Principled base colour value) to sRGB. (Byte images' `pixels` are already
    sRGB.)"""
    c = max(0.0, min(1.0, c))
    return 12.92 * c if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055


BARK_WORDS = ("bark", "branch", "trunk", "wood", "stem")


def _bark_rgb(obj, cut, cx, cy, r):
    """The bark colour of a tree's new break parts and where it came from: the texels under
    the trunk's bark faces that reach the band 0..2 cut (centre within 2 r of the trunk axis),
    else a bark-named image's (`BARK_WORDS`) opaque texels, else the opaque ones under any trunk face there (a shared bark and leaf atlas), else under
    every bark face, else the bark material's flat colour."""
    def is_bark(p):
        slots = obj.material_slots
        mat = slots[p.material_index].material if slots else None
        return mat is not None and mat.get("acr_nature") == "bark"

    zs = [v.co.z for v in obj.data.vertices]

    def trunk(p):
        lo = min(zs[i] for i in p.vertices)
        hi = max(zs[i] for i in p.vertices)
        return lo <= 2.0 * cut and hi >= 0.0 and math.hypot(p.center.x - cx, p.center.y - cy) < 2.0 * r

    rgb = _texel_rgb(obj, lambda p: trunk(p) and is_bark(p), opaque=False)
    if rgb:
        return rgb, "trunk"
    # A bark image in a slot taken for leaves (broadleaf_ez's `branches.011`): its opaque
    # texels.
    named = {i for i, sl in enumerate(obj.material_slots)
             if (img := _image_of(sl.material)[0]) is not None
             and any(w in img.name.lower() for w in BARK_WORDS)}
    rgb = _leaf_rgb(obj, named)
    if rgb:
        return rgb, "bark_image"
    # One atlas for bark and leaves: its opaque texels under the trunk that are not leaf
    # green.
    rgb = _texel_rgb(obj, trunk, texel=lambda c: c[1] <= 1.08 * c[0])
    if rgb:
        return rgb, "trunk_atlas"
    rgb = _texel_rgb(obj, is_bark, opaque=False)
    if rgb:
        return rgb, "bark"
    for slot in obj.material_slots:
        if slot.material is not None and slot.material.get("acr_nature") == "bark":
            img, flat = _image_of(slot.material)
            if img is None:
                return [round(_srgb(c), 3) for c in flat[:3]], "flat"
    return [0.30, 0.24, 0.18], "default"


def _spike(bm, base, tip, width, side, mat):
    """A thin splinter: a three-sided wedge from `base` to `tip`, `width` wide along `side`."""
    a = bm.verts.new(base - side * width)
    b = bm.verts.new(base + side * width)
    up = (tip - base).normalized().cross(side).normalized() * width * 0.6
    c = bm.verts.new(base + up)
    t = bm.verts.new(tip)
    for f in ((a, b, t), (b, c, t), (c, a, t)):
        bm.faces.new(f).material_index = mat


def _tree_states(ctx, obj, seed):
    """The struck states of a map tree (rule `ball_terrain`, docs/assets.md "Struck map
    objects"), derived from the standing tree `obj` so they match it:

    - `<name>.stump` (the tree's own faces below a jagged break 0.35..0.8 m up) and
      `<name>.stump_break` (the break: bark wall, pale fresh wood on top, standing splinters);
    - `<name>.felled` (the rest of the tree, crown and all) and `<name>.felled_break` (its torn
      foot), lying `breaks.FELL_DEG` towards +X about the ground point, whatever would go into
      the ground crushed onto it; trunk and branch sway 0, the leaves keep their flutter;
    - `<name>.scar` (bark torn off on the -X side for 0.5 m above the break, splinters out) for a
      tree that stopped a ball.
    The renderer turns +X to the ball's flight. Returns the `struck` stats."""
    me = obj.data
    name = ctx.name
    hi = max(v.co.z for v in me.vertices)
    cut = min(0.8, max(0.35, 0.09 * hi))
    # The trunk: vertices of a band under the break (above the root flare, below the first
    # forks), near their centre.
    band = [v.co for v in me.vertices if 0.15 <= v.co.z <= cut]
    if len(band) < 8:
        band = [v.co for v in me.vertices if v.co.z <= cut * 1.6]
    cx = sum(c.x for c in band) / len(band)
    cy = sum(c.y for c in band) / len(band)
    # Twice: the first centre and median pull towards low branches and roots.
    r = 0.45
    for _ in range(3):
        near = [c for c in band if math.hypot(c.x - cx, c.y - cy) < 2.0 * r] or band
        cx = sum(c.x for c in near) / len(near)
        cy = sum(c.y for c in near) / len(near)
        d = sorted(math.hypot(c.x - cx, c.y - cy) for c in near)
        r = min(0.4, max(0.06, 0.012 * hi, d[len(d) // 4]))
    rough = max(0.05, 0.6 * r)
    n = 16
    prof = breaks.profile(n, seed, rough)
    foot_prof = breaks.profile(n, seed + 1, rough * 0.8)
    # The bark colour of the new parts: the texels under the trunk band's faces.
    bark_rgb, bark_from = _bark_rgb(obj, cut, cx, cy, r)
    bark = _tile_material(f"{name}.bark_break", tuple(bark_rgb), "wood")
    fresh = _tile_material(f"{name}.fresh_wood", FRESH_WOOD, "wood")

    def by_trunk(f):
        return all(math.hypot(v.co.x - cx, v.co.y - cy) < 3.0 * r for v in f.verts)

    stump = _copy_cut(ctx, obj, f"{name}.stump", cut, False, by_trunk)
    felled = _copy_cut(ctx, obj, f"{name}.felled", cut, True)
    for dd in stump.data.attributes["_wind"].data:
        dd.color = (0.0, 0.0, 0.0, 0.0)
    rnd = random.Random(seed + 11)
    centre = Vector((cx, cy, 0.0))

    def ring_pt(k, rad, z):
        a = 2 * math.pi * k / n
        return centre + Vector((math.cos(a) * rad, math.sin(a) * rad, z))

    # The stump's break: a bark wall from under the break up to the jagged rim, fresh wood
    # from the rim down to a rougher core; four splinters stand up from the rim.
    bm = bmesh.new()
    low = [bm.verts.new(ring_pt(k, r * 1.04, cut - 0.08)) for k in range(n)]
    rim = [bm.verts.new(ring_pt(k, r * 0.98, cut + prof[k])) for k in range(n)]
    core = [bm.verts.new(ring_pt(k, r * 0.5, cut + 0.35 * prof[k] + rnd.uniform(0, 0.03))) for k in range(n)]
    mid = bm.verts.new(centre + Vector((rnd.uniform(-0.2, 0.2) * r, rnd.uniform(-0.2, 0.2) * r, cut + 0.04)))
    for k in range(n):
        j = (k + 1) % n
        bm.faces.new((low[k], low[j], rim[j], rim[k])).material_index = 0
        bm.faces.new((rim[k], rim[j], core[j], core[k])).material_index = 1
        bm.faces.new((core[k], core[j], mid)).material_index = 1
    for k in sorted(rnd.sample(range(n), 4)):
        a = 2 * math.pi * k / n
        out = Vector((math.cos(a), math.sin(a), 0.0))
        base = ring_pt(k, r * 0.8, cut + prof[k] * 0.6)
        tip = base + Vector((0, 0, rough * rnd.uniform(1.5, 3.0))) + out * r * rnd.uniform(0.1, 0.5)
        _spike(bm, base, tip, r * 0.12, Vector((-out.y, out.x, 0.0)), 1)
    stump_break = _part(ctx, f"{name}.stump_break", bm, [bark, fresh])

    # The felled trunk's foot: the matching tear, a bark wall over the cut hanging down into a
    # ragged lip, fresh wood under it, splinters pointing down.
    bm = bmesh.new()
    top = [bm.verts.new(ring_pt(k, r * 1.04, cut + 0.08)) for k in range(n)]
    lip = [bm.verts.new(ring_pt(k, r * 0.98, cut - foot_prof[k])) for k in range(n)]
    core = [bm.verts.new(ring_pt(k, r * 0.5, cut - 0.35 * foot_prof[k] - rnd.uniform(0, 0.03))) for k in range(n)]
    foot = bm.verts.new(centre + Vector((0, 0, cut - 0.03)))
    for k in range(n):
        j = (k + 1) % n
        bm.faces.new((top[j], top[k], lip[k], lip[j])).material_index = 0
        bm.faces.new((lip[j], lip[k], core[k], core[j])).material_index = 1
        bm.faces.new((core[j], core[k], foot)).material_index = 1
    for k in sorted(rnd.sample(range(n), 3)):
        a = 2 * math.pi * k / n
        out = Vector((math.cos(a), math.sin(a), 0.0))
        base = ring_pt(k, r * 0.75, cut - foot_prof[k] * 0.5)
        tip = base - Vector((0, 0, rough * rnd.uniform(1.2, 2.4))) + out * r * rnd.uniform(0.1, 0.4)
        _spike(bm, base, tip, r * 0.1, Vector((-out.y, out.x, 0.0)), 1)
    felled_break = _part(ctx, f"{name}.felled_break", bm, [bark, fresh])

    # Lying: turned about +Y through the ground point, crown east; whatever went into the
    # ground crushed onto it. A lying tree does not sway; its leaves still flutter.
    for o in (felled, felled_break):
        m = o.data
        for v in m.vertices:
            x, y, z = breaks.lay(tuple(v.co))
            v.co = Vector((x, y, breaks.ground(z)))
        for dd in m.attributes["_wind"].data:
            c = dd.color
            dd.color = (0.0, 0.0, c[2], c[3])

    # The scar: a torn patch of fresh wood just outside the bark on the -X side (where a ball
    # flying +X meets the trunk), ragged at its edges, with splinters sticking out.
    z0 = cut + 0.1
    side = sorted(math.hypot(v.co.x - cx, v.co.y - cy) for v in me.vertices if z0 <= v.co.z <= z0 + 0.5
                  and math.hypot(v.co.x - cx, v.co.y - cy) < 1.5 * r
                  and math.cos(math.atan2(v.co.y - cy, v.co.x - cx) - math.pi) > 0.8)
    rs = (side[int(len(side) * 0.75)] if side else r) + 0.012
    bm = bmesh.new()
    na, nz = 6, 5
    grid = []
    for i in range(nz):
        z = z0 + 0.5 * i / (nz - 1)
        row = []
        for k in range(na):
            a = math.pi + math.radians(-48 + 96 * k / (na - 1))
            edge = i in (0, nz - 1) or k in (0, na - 1)
            rad = rs * (0.98 if edge else 1.0 - 0.06 * rnd.random())
            zz = z + (rnd.uniform(-0.08, 0.08) if edge else 0.0)
            row.append(bm.verts.new(centre + Vector((math.cos(a) * rad, math.sin(a) * rad, zz))))
        grid.append(row)
    for i in range(nz - 1):
        for k in range(na - 1):
            bm.faces.new((grid[i][k], grid[i][k + 1], grid[i + 1][k + 1], grid[i + 1][k])).material_index = 0
    for _j in range(3):
        a = math.pi + rnd.uniform(-0.5, 0.5)
        out = Vector((math.cos(a), math.sin(a), 0.0))
        base = centre + out * rs + Vector((0, 0, z0 + rnd.uniform(0.1, 0.4)))
        tip = base + out * rnd.uniform(0.12, 0.3) + Vector((0, 0, rnd.uniform(-0.1, 0.15)))
        _spike(bm, base, tip, 0.025, Vector((0, 0, 1.0)), 0)
    scar = _part(ctx, f"{name}.scar", bm, [fresh])

    def tris(objs):
        return sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in objs)

    lying = [v.co for o in (felled, felled_break) for v in o.data.vertices]
    return {
        "cut_m": round(cut, 3),
        "trunk_r_m": round(r, 3),
        "stump_triangles": tris([stump, stump_break]),
        "felled_triangles": tris([felled, felled_break]),
        "scar_triangles": tris([scar]),
        "felled_min_z_m": round(min(c.z for c in lying), 3),
        "felled_length_m": round(max(c.x for c in lying) - min(c.x for c in lying), 2),
        "felled_height_m": round(max(c.z for c in lying), 2),
        "bark_rgb": bark_rgb,
        "bark_from": bark_from,
    }


CHIP_SIDES = 8  # chip marks built per rock, one for each ball heading (renderer: `CHIP_SIDES`)


def _chip(ctx, obj, seed):
    """A big rock's chip marks (rule `ball_terrain`): `<name>.chip.<k>` for k in
    0..CHIP_SIDES, a ragged facet of fresh, lighter stone at half the height where a ball
    flying at 2 pi k / CHIP_SIDES from +X (towards +Y) first meets the rock, a little proud of
    the surface. The renderer shows the one nearest the ball's heading in the rock's frame.
    Returns the `struck` stats."""
    me = obj.data
    bvh = BVHTree.FromObject(obj, bpy.context.evaluated_depsgraph_get())
    xs = [v.co.x for v in me.vertices]
    ys = [v.co.y for v in me.vertices]
    hi = max(v.co.z for v in me.vertices)
    cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    reach = max(max(xs) - min(xs), max(ys) - min(ys)) + 1.0
    rgb = _leaf_rgb(obj, set(range(len(obj.material_slots)))) or [0.5, 0.48, 0.45]
    pale = tuple(round(min(1.0, 0.45 * c + 0.55 * 0.80), 3) for c in rgb)
    mat = _tile_material(f"{ctx.name}.chip", pale, "plain")
    rnd = random.Random(seed + 5)
    size = min(max(xs) - min(xs), max(ys) - min(ys), hi)
    made, tris = [], 0
    for k in range(CHIP_SIDES):
        a = 2 * math.pi * k / CHIP_SIDES
        way = Vector((math.cos(a), math.sin(a), 0.0))
        start = Vector((cx, cy, hi * 0.5)) - way * reach
        hit, nrm, _i, _d = bvh.ray_cast(start, way)
        bm = bmesh.new()
        if hit is not None:
            nrm = nrm.normalized()
            t1 = nrm.cross(Vector((0, 0, 1)))
            t1 = t1.normalized() if t1.length > 1e-4 else Vector((1, 0, 0))
            t2 = nrm.cross(t1).normalized()
            rad = size * 0.22
            centre = bm.verts.new(hit + nrm * 0.004)
            ring = []
            for j in range(9):
                b = 2 * math.pi * j / 9
                ring.append(bm.verts.new(hit + nrm * 0.015
                                         + (t1 * math.cos(b) + t2 * math.sin(b)) * rad * rnd.uniform(0.6, 1.0)))
            for j in range(9):
                bm.faces.new((centre, ring[j], ring[(j + 1) % 9]))
            made.append(round(rad, 3))
        else:
            made.append(None)
        chip = _part(ctx, f"{ctx.name}.chip.{k}", bm, [mat])
        tris += len(chip.data.polygons)
    return {"chip_r_m": made, "chip_triangles": tris, "chip_rgb": list(pale)}


# Map objects generated from their own sprite (`map`): triangle budget and texture per kind.
MAP_KINDS = {"ruin": {"tris": 4000, "tex": 1024}, "rubble": {"tris": 1000, "tex": 512}}
LEVEL_KEEP = 0.01  # `_map_fit` keeps the levelling unless it costs more sprite IoU than this
MAP_GAIN_MAX = 2.0  # `_map_tone` brightens a baked map texture at most this much
# The game's model light (acr-render shadow::LIGHT, east-south-up, here Blender's east-north-up)
# and its unlit share (model_renderer AMBIENT): `_map_tone` matches the sprite as the game draws it.
GAME_LIGHT = (-15.0, -14.0, 30.0)
GAME_AMBIENT = 0.5


def _map_fit(low, m, anchor, real_dir):
    """Stands a model made from the game's view of a map sprite upright and fits it to that
    sprite (`spritefit`): the camera turn (`camera_upright`, minus the 30-degree elevation
    about X; azimuth 0: the sprite is the north view, `RealDir` 64), then the residual tilt
    (`level`: long axis of a pillar, else the flattest base), then the sprite fit with the
    turn held at 0 (scale and ground shift; the model came from this very frame). Returns
    stats: `iou_raw` (the same fit without the upright turns), `iou_fit`, `iou`
    (after the transform, denser sample), `iou_camera` (camera turn only; the levelling is
    undone when it scores `LEVEL_KEEP` worse), `upright` {camera_deg, how, tilt_deg}, `fit`."""
    from . import genbuild, spritefit

    def centre():
        v, _ = genbuild._arrays(low)
        c = (v.min(axis=0) + v.max(axis=0)) / 2
        vs.transform([low], Matrix.Translation((-c[0], -c[1], -c[2])))

    def m4(r):
        return Matrix([list(row) for row in r]).to_4x4()

    centre()
    raw = spritefit.fit(genbuild.sample(low, 60000, seed=1), m, anchor, real_dir, yaws=(0,))
    cam = spritefit.camera_upright()
    vs.transform([low], m4(cam))
    centre()
    cam_only = spritefit.fit(genbuild.sample(low, 60000, seed=1), m, anchor, real_dir, yaws=(0,))
    lv, how, tilt = spritefit.level(genbuild.sample(low, 30000, seed=2))
    vs.transform([low], m4(lv))
    centre()
    f = spritefit.fit(genbuild.sample(low, 60000, seed=1), m, anchor, real_dir, yaws=(0,))
    if f["iou"] < cam_only["iou"] - LEVEL_KEEP:
        # the levelling made the match worse (a heap without a flat base): undo it
        vs.transform([low], m4(lv.T))
        centre()
        f, how, tilt = cam_only, f"{how} undone", 0.0
    vs.transform([low], genbuild._matrix(f))
    return {
        "iou_raw": round(raw["iou"], 3),
        "iou_camera": round(cam_only["iou"], 3),
        "iou_fit": round(f["iou"], 3),
        "iou": genbuild._final_iou(low, m, anchor, real_dir),
        "upright": {"camera_deg": round(math.degrees(math.asin(cam[2, 1])), 1), "how": how, "tilt_deg": tilt},
        "fit": {"scale": round(f["scale"], 4), "shift": [round(x, 3) for x in f["shift"]]},
    }


def _map_tone(low, view):
    """Brings the baked base colour of a map model up to its sprite's brightness. Pixal3D
    returns PBR materials and reads some stone sprites as metal: pillars and blocks
    (TREELIST 454, 456, 458, 459, 463, 469) came with metallic 0.17-0.78 and a base colour
    up to a third darker than the sprite, which a PBR viewer lights with reflections but the
    game, which reads base colour only (Lambert, half ambient), drew nearly black. Compares
    the mean luminance (sRGB) of the sprite's opaque pixels with the model as the game
    shades it on the faces the sprite camera sees (area seen along `conventions.FORWARD`,
    the model already turned and fitted to the sprite view): base colour times the game's
    light (`GAME_LIGHT`, `GAME_AMBIENT`; linear light, so lit^(1/2.2) in sRGB). Brightens by
    their ratio, never darkens, at most `MAP_GAIN_MAX`. Returns stats {sprite, model (the
    unlit base colour), shaded, gain}."""
    from . import conventions as cv

    me = low.data
    bsdf = next(n for n in me.materials[0].node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    node = bsdf.inputs["Base Color"].links[0].from_node
    img = node.image
    w, h = img.size
    px = _pixels(img)
    lum_w = np.array([0.3, 0.55, 0.15], np.float32)
    ref = bpy.data.images.load(view.image)
    rp = _pixels(ref)
    bpy.data.images.remove(ref)
    sprite = float((rp[rp[:, 3] >= 0.5, :3] @ lum_w).mean())
    me.calc_loop_triangles()
    uv = np.empty(len(me.loops) * 2, np.float32)
    me.uv_layers.active.data.foreach_get("uv", uv)
    uv = uv.reshape(-1, 2)
    loops = np.empty(len(me.loop_triangles) * 3, np.int32)
    me.loop_triangles.foreach_get("loops", loops)
    loops = loops.reshape(-1, 3)
    co = np.empty(len(me.vertices) * 3, np.float32)
    me.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    verts = np.empty(len(me.loop_triangles) * 3, np.int32)
    me.loop_triangles.foreach_get("vertices", verts)
    t = co[verts.reshape(-1, 3)]
    fn = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
    seen = np.clip(-(fn @ np.asarray(cv.FORWARD, np.float32)), 0, None)
    light = np.asarray(GAME_LIGHT, np.float32) / np.linalg.norm(GAME_LIGHT)
    nrm = fn / np.maximum(np.linalg.norm(fn, axis=1, keepdims=True), 1e-12)
    lit = GAME_AMBIENT + (1.0 - GAME_AMBIENT) * np.clip(nrm @ light, 0, None)
    c = uv[loops].mean(axis=1)
    tex = px.reshape(h, w, 4)[np.clip((c[:, 1] * h).astype(int), 0, h - 1), np.clip((c[:, 0] * w).astype(int), 0, w - 1), :3]
    alb = tex @ lum_w
    total = max(float(seen.sum()), 1e-9)
    model = float((alb * seen).sum() / total)
    shaded = float((alb * lit ** (1 / 2.2) * seen).sum() / total)
    gain = min(MAP_GAIN_MAX, max(1.0, sprite / max(shaded, 1e-3)))
    if gain > 1.0:
        px[:, :3] *= gain
        node.image = _put(img, px, ".tone")
        node.image.pack()
        bpy.data.images.remove(img)
    return {"sprite": round(sprite, 3), "model": round(model, 3), "shaded": round(shaded, 3), "gain": round(gain, 3)}


def build_generated(ctx, spec):
    """A map prop from a generated model (Pixal3D or Rodin, `$ACR_SOURCES/generated/<source>`,
    local only) through `genfit.fit` (prop category: backdrop dropped, decimated to the
    item's `tris`, base colour baked, standing on z = 0, facing -Y at its `size`). SPEC:

        source  path below the generated sources ("pixal3d/prop_ruin_wall.glb")
        sway    None (hard props: zero `_WIND`), "stalk" (sways with height, leaves flutter:
                maize, dry bushes) or "green" (green texels flutter: pumpkin leaves);
                `genfit.prop_wind`; the root's `acr_wind` scales: `WIND["rock"]` for hard
                props, `WIND["bush"]` otherwise
        sink    metres below the ground point (default 0.03; on slopes nothing floats)
        map     "ruin" or "rubble": a map object generated from its own TREELIST sprite in
                the game's view (`MAP_KINDS` budget, built at the sprite's size, stood
                upright and fitted to that sprite: `_map_fit`, its base colour brought up to
                the sprite's brightness: `_map_tone`; needs the game folder)
        turn    degrees about +Z

    The baked normal map is kept (`normal_map_px`), roughness 0.85, metallic 0. Stats: the
    `genfit.fit` numbers (`source_triangles`, `triangles`, `deviation`, `tex_unbaked_frac`,
    `backdrop_faces_removed`, `size_m`) plus `wind`, `flutter_frac`."""
    from . import genfit

    mp = spec.get("map")
    row = ref = sil = None
    if mp:
        from . import conventions as cv
        from . import genbuild

        ref = genbuild._need_ref(ctx)
        sil = genbuild.mask(ref.views[0])
        rows, cols = np.nonzero(sil.any(axis=1))[0], np.nonzero(sil.any(axis=0))[0]
        # the sprite's size: width, or height through the 30-degree camera
        size = max((cols[-1] - cols[0] + 1), (rows[-1] - rows[0] + 1) / cv.COS_E) / cv.PIXELS_PER_METRE
        row = dict(MAP_KINDS[mp], category="prop", size=round(float(size), 3), layer="stone")
    low, st = genfit.fit(spec["source"], name=f"{ctx.name}.mesh", row=row)
    me = low.data
    # the baked normal map stays (normalTexture; genfit.CATEGORIES "nrm" px)
    nrm = _normal_of(me.materials[0])[0]
    st["normal_map_px"] = list(nrm.size) if nrm is not None else None
    img = _image_of(me.materials[0])[0]
    px = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    st["tex_black_frac"] = round(float((px[:, :3].max(1) < 0.05).mean()), 3)
    st["tex_mean_rgb"] = [round(float(c), 3) for c in px[:, :3].mean(0)]
    me.validate()
    if mp:
        st["map_kind"] = mp
        st.update(_map_fit(low, sil, ref.anchor, ref.views[0].dir))
        st["tone"] = _map_tone(low, ref.views[0])
    sink = spec.get("sink", 0.03)
    vs.transform([low], Matrix.Rotation(math.radians(spec.get("turn", 0.0)), 4, "Z") @ Matrix.Translation((0, 0, -sink)))
    low.parent = ctx.root
    # per-vertex baked colour (for "green" flutter)
    mode = spec.get("sway")
    rgb = None
    if mode == "green":
        img = _image_of(me.materials[0])[0]
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
    ctx.root["acr_wind"] = [float(k) for k in WIND["bush" if mode else "rock"]]
    xs = [v.co.x for v in me.vertices]
    ys = [v.co.y for v in me.vertices]
    zs = [v.co.z for v in me.vertices]
    st.update({
        "kind": "prop",
        "source": spec["source"],
        "size_m": [round(max(xs) - min(xs), 2), round(max(ys) - min(ys), 2), round(max(zs), 2)],
        "sway": mode,
        "flutter_frac": round(flutter / max(1, len(me.vertices)), 3),
    })
    ctx.stats = st
