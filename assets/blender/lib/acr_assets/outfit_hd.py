"""High-detail outfit on the HD body (`hdbody`): the RDR2-style recipe (docs/clothing.md,
"High-detail outfits").

1. Each garment of the 1768 drafts (`garments`) is draped *alone* on the HD master (no
   layer stack in one simulation): `drape.dress` with one garment at `SPACING_CM`, the
   short damped sim of `drape.simulate`, seams welded, weights from the skin.
2. One outfit mesh from the visible layers only (`compose`): every inner face covered by
   an outer layer (rays along the skin normal) is deleted, and so is the skin under the
   clothes; what stays of an inner layer is pushed `OFFSET` under the layer on it, so it
   cannot poke through in the rest pose and, with the weights of the surface beneath,
   in no pose. Loose parts (coat skirts, turnbacks) never hide anything: the legs swing
   behind them.
3. Woven cloth: tileable twill (wool) and plain weave (linen) albedo, normal and
   roughness maps on box-projected UVs (`weave`), brass buttons (`drape.buttons`).
4. Gore and decay: `_PART` from the weights (`rig.part_of_bone`), `_GORE` and the decay
   mask per layer (`decay.add_masks`), `acr_decay` extras on every material; the renderer
   caps cuts itself (docs/rendering-3d.md, "Caps"), so the skin inside a sleeve may go.
5. Levels: the outfit decimated to `LODS`, composed again per level against the body's
   level, written with `acr_lod` / `acr_lod_of` like the body's.

`ctx.stats["outfit"]` holds the numbers (drape per garment, culled faces, pokes, triangles
per level, seconds). Pure helpers (`weave_height`, `box_uv`, `skirt_swing`) are tested in
`tests/test_outfit_hd.py`.
"""

import math
import time
from contextlib import contextmanager

import numpy as np

GARMENTS = ("breeches", "gaiters", "shirt", "waistcoat", "coat")  # garments.ORDER, EngM18
SPACING_CM = 1.5  # panel mesh spacing for the HD drape (line units: 2-5 cm)
OFFSET = 0.003  # m: an inner layer ends this far under the layer on it
REACH = 0.05  # m: an outer layer this far out along the skin normal covers a point
NEAR = 0.03  # m: the clamp looks for an outer surface this close
POKE = 0.012  # m: ... and along the normal this far under the vertex (acr_render::layers::POKE)
LOOSE = ("coat_skirts", "turnbacks", "_skirts")  # name endings of loose parts: hide nothing
LODS = (10000, 3000, 800)  # outfit triangles per level (the body's 10k / 3k / 800)
TILE_M = 0.05  # one texture repeat on the cloth (m)
TEX = 1024  # albedo / roughness texels per repeat
TEX_NRM = 2048
FABRIC = {  # key -> (fabric, decay layer)
    "coat": ("wool", "wool"), "coat_skirts": ("wool", "wool"), "facings": ("wool", "wool"),
    "lapels": ("wool", "wool"), "lining": ("wool", "wool"), "turnbacks": ("wool", "wool"),
    "waistcoat": ("wool", "wool"), "breeches": ("wool", "wool"), "gaiters": ("wool", "wool"),
    "shirt": ("linen", "linen"), "stock": ("linen", "leather"), "shoes": ("leather", "leather"),
    "brass": ("metal", "metal"), "metal": ("metal", "metal"),
}
SHOE_LIFT = 0.006  # m: the shoe hull off the foot skin
SHOE_VOXEL = 0.004  # m: remesh of the shoe hull
SHOE_TRIS = 1600  # both shoes


# ----------------------------------------------------------------------------- pure

def weave_height(n, threads, kind, seed=11):
    """Tileable cloth relief (n x n, 0..1): `twill` diagonal ribs (2/2 wool twill) or
    `plain` over-under weave (linen), `threads` per repeat, with slubs and fibre noise."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:n, 0:n] / n
    a = 2 * math.pi * threads
    if kind == "twill":
        h = 0.5 + 0.35 * np.sin(a * (x + y)) + 0.15 * np.sin(a * x)
    else:
        h = 0.5 + 0.25 * np.sin(a * x) * np.sign(np.sin(a * y / 2)) + 0.25 * np.sin(a * y) * np.sign(np.sin(a * x / 2))
    f = np.fft.rfft2(rng.standard_normal((n, n)))
    ky = np.fft.fftfreq(n)[:, None]
    kx = np.fft.rfftfreq(n)[None, :]
    k = np.sqrt(kx ** 2 + ky ** 2) * n
    fibre = np.fft.irfft2(f * np.exp(-(k / (threads * 0.7)) ** 2), s=(n, n))
    fibre /= np.abs(fibre).max() or 1.0
    h = h + 0.18 * fibre
    return (h - h.min()) / (np.ptp(h) or 1.0)


def box_uv(co, normal, tile=TILE_M):
    """UV of a corner from its position and the face normal: the plane across the
    dominant axis, in repeats of `tile` metres (seams between the planes are hidden by the
    tiling weave)."""
    ax = max(range(3), key=lambda i: abs(normal[i]))
    u, v = [c for i, c in enumerate(co) if i != ax]
    return (u / tile, v / tile)


def skirt_swing(keys, gain=0.55, lag=1):
    """Coat-skirt bones following the thighs a key late (secondary motion in the clip):
    keys [(frame, {bone: (x, y, z)})] -> the same with `skirt_*_01/02` added where the
    clip leaves them out. The front skirts swing with their thigh, the back ones less."""
    out = []
    for i, (f, spec) in enumerate(keys):
        prev = keys[max(0, i - lag)][1]
        spec = dict(spec)
        for side in ("l", "r"):
            th = prev.get(f"thigh_{side}")
            if th is None:
                continue
            x = float(th[0])
            for col, g in (("f", gain), ("b", gain * 0.6)):
                b1, b2 = f"skirt_{col}{side}_01", f"skirt_{col}{side}_02"
                if b1 not in spec:
                    spec[b1] = (round(x * g, 2), 0.0, 0.0)
                if b2 not in spec:
                    spec[b2] = (round(x * g * 0.35, 2), 0.0, 0.0)
        out.append((f, spec))
    return out


# ----------------------------------------------------------------------------- Blender

class _Ch:
    """The bits of `humans.Character` that `drape.dress` / `drape.buttons` read."""
    prefix = ""
    shoes = None
    stand = None
    symmetric = ()

    def __init__(self, name, arm, flesh, garments):
        self.name, self.arm, self.flesh = name, arm, flesh
        self.build = {"drape": list(garments)}
        self.layers, self.keys = {"clothes": []}, {}

    def add(self, layer, obj, key, symmetric=False):
        self.layers.setdefault(layer, []).append(obj)
        self.keys[obj.name] = key
        obj["acr_key"] = key
        return obj


def _co(ob):
    me = ob.data
    a = np.zeros(len(me.vertices) * 3)
    me.vertices.foreach_get("co", a)
    return a.reshape(-1, 3)


def _tree(objs):
    from mathutils.bvhtree import BVHTree

    V, F, base = [], [], 0
    for o in objs:
        V += [v.co.copy() for v in o.data.vertices]
        F += [[base + i for i in p.vertices] for p in o.data.polygons]
        base += len(o.data.vertices)
    return BVHTree.FromPolygons(V, F) if F else None


def _tris(ob):
    return sum(len(p.vertices) - 2 for p in ob.data.polygons)


@contextmanager
def _hd_measure(body, garments, spacing):
    """`drape.Body` measures the HD skin, panels meshed at `spacing` cm (this build only)."""
    from . import drape, mbody

    V = _co(body)
    saved_repose, saved_sp = mbody.repose, {g: drape.SPACING.get(g) for g in garments}
    mbody.repose = lambda *a, **k: (V, [])
    for g in garments:
        drape.SPACING[g] = spacing
    try:
        yield
    finally:
        mbody.repose = saved_repose
        for g, s in saved_sp.items():
            if s is None:
                drape.SPACING.pop(g, None)
            else:
                drape.SPACING[g] = s


def drape_alone(name, arm, body, garments=GARMENTS, spacing=SPACING_CM):
    """Each garment draped alone on `body` (rest pose). Returns (objects, {name: key},
    {garment: drape report + seconds})."""
    from . import drape

    objs, keys, rep = [], {}, {}
    with _hd_measure(body, garments, spacing):
        for g in garments:
            ch = _Ch(name, arm, body, (g,))
            r = {}
            t0 = time.time()
            made = drape.dress(ch, garments=(g,), report=r)
            r = dict(r.get(g, {}), s=round(time.time() - t0, 1))
            rep[g] = r
            objs += ch.layers["clothes"]
            keys.update(made)
            print(f"[outfit_hd] {g} draped alone: {r}", flush=True)
    return objs, keys, rep


def shoes(name, arm, body):
    """Closed low shoes: per foot the convex hull of the foot and toe skin (lifted
    `SHOE_LIFT` along the normals: one rounded toe, a flat sole under the arch), voxel
    remeshed at `SHOE_VOXEL`, smoothed and decimated to `SHOE_TRIS`, weighted from the
    nearest foot skin. The foot inside is culled with the skin under the clothes
    (`compose`, `cull_feet`); `shoe_buckles` adds the buckles."""
    import bmesh
    import bpy
    from mathutils.kdtree import KDTree

    from . import body as bd

    names = {g.index: g.name for g in body.vertex_groups}
    me = body.data
    foot = foot_mask(body)
    idx = [i for i, f in enumerate(foot) if f]
    bm = bmesh.new()
    for side in (1, -1):
        vs = [bm.verts.new(me.vertices[i].co + me.vertices[i].normal * SHOE_LIFT) for i in idx
              if me.vertices[i].co.x * side > 0]
        res = bmesh.ops.convex_hull(bm, input=vs)
        inner = {v for v in res["geom_interior"] + res["geom_unused"] if isinstance(v, bmesh.types.BMVert)}
        bmesh.ops.delete(bm, geom=list(inner), context="VERTS")
    m2 = bpy.data.meshes.new(f"{name}.shoes")
    bm.to_mesh(m2)
    bm.free()
    ob = bpy.data.objects.new(f"{name}.shoes", m2)
    bd.link(ob)
    bpy.context.view_layer.objects.active = ob
    for typ, attrs in (("REMESH", {"mode": "VOXEL", "voxel_size": SHOE_VOXEL}),
                       ("SMOOTH", {"factor": 0.5, "iterations": 2}),
                       ("DECIMATE", {"ratio": 1.0})):
        md = ob.modifiers.new(typ.lower(), typ)
        for k, x in attrs.items():
            setattr(md, k, x)
        if typ == "DECIMATE":
            md.ratio = min(1.0, SHOE_TRIS / max(1, _tris_eval(ob)))
        bpy.ops.object.modifier_apply(modifier=md.name)
    kd = KDTree(len(idx))
    for k, i in enumerate(idx):
        kd.insert(me.vertices[i].co, k)
    kd.balance()
    groups = {}
    for v in ob.data.vertices:
        _co, k, _d = kd.find(v.co)
        for e in me.vertices[idx[k]].groups:
            gn = names[e.group]
            g = groups.get(gn) or ob.vertex_groups.new(name=gn)
            groups[gn] = g
            g.add([v.index], e.weight, "REPLACE")
    for p in ob.data.polygons:
        p.use_smooth = True
    ob.modifiers.new("Armature", "ARMATURE").object = arm
    ob.parent = arm
    ob.matrix_parent_inverse = arm.matrix_world.inverted()
    return ob


def foot_mask(ob):
    """Per vertex: its strongest bone is a foot or toe bone."""
    names = {g.index: g.name for g in ob.vertex_groups}
    out = []
    for v in ob.data.vertices:
        g = max(v.groups, key=lambda e: e.weight) if v.groups else None
        out.append(bool(g) and names[g.group].startswith(("foot", "toe")))
    return out


def cull_feet(ob):
    """Deletes the skin's foot and toe faces (inside the closed shoes); returns the count."""
    m = foot_mask(ob)
    return _delete_faces(ob, [not x for x in m])


def _tris_eval(ob):
    import bpy

    ev = ob.evaluated_get(bpy.context.evaluated_depsgraph_get())
    me = ev.to_mesh()
    n = sum(len(p.vertices) - 2 for p in me.polygons)
    ev.to_mesh_clear()
    return n


def shoe_buckles(name, arm, shoe, size=(0.034, 0.026, 0.004)):
    """A brass buckle on each shoe's instep: a flat box on the top of the shoe a third of
    the way from the ankle to the toe, on the `foot_*` bone."""
    import bmesh
    import bpy
    from mathutils import Matrix, Vector

    from . import body as bd

    tree = _tree([shoe])
    bm = bmesh.new()
    for side in ("l", "r"):
        b = arm.data.bones[f"foot_{side}"]
        head, tail = arm.matrix_world @ b.head_local, arm.matrix_world @ b.tail_local
        p = head.lerp(tail, 0.45)
        hit = tree.ray_cast(Vector((p.x, p.y, p.z + 0.3)), Vector((0, 0, -1)), 0.6)
        if hit[0] is None:
            continue
        n = hit[1].normalized()
        fwd = (tail - head)
        fwd.z = 0
        fwd = (fwd - n * fwd.dot(n)).normalized()
        x = fwd.cross(n).normalized()
        rot = Matrix((x, fwd, n)).transposed().to_4x4()
        geom = bmesh.ops.create_cube(bm, size=1.0)["verts"]
        bmesh.ops.transform(bm, verts=geom, matrix=Matrix.Translation(hit[0] + n * size[2] * 0.5) @ rot
                            @ Matrix.Diagonal((size[0], size[1], size[2], 1.0)))
    me = bpy.data.meshes.new(f"{name}.buckles")
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new(f"{name}.buckles", me)
    bd.link(ob)
    for side in ("l", "r"):
        g = ob.vertex_groups.new(name=f"foot_{side}")
        g.add([v.index for v in me.vertices if (v.co.x > 0) == (side == "l")], 1.0, "REPLACE")
    ob.modifiers.new("Armature", "ARMATURE").object = arm
    ob.parent = arm
    ob.matrix_parent_inverse = arm.matrix_world.inverted()
    return ob


def _outward(body_tree, p):
    loc, n, _i, _d = body_tree.find_nearest(p)
    return n if loc is not None else None


def _delete_faces(ob, keep_vertex):
    """Deletes the faces none of whose corners is kept; returns the count."""
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bm.verts.ensure_lookup_table()
    gone = [f for f in bm.faces if not any(keep_vertex[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(ob.data)
    bm.free()
    return len(gone)


def _covered(ob, outer_tree, body_tree, own_normals=False):
    """Per vertex: an outer surface within `REACH` along the outward normal."""
    from mathutils import Vector

    me = ob.data
    out = []
    for v in me.vertices:
        n = v.normal if own_normals else _outward(body_tree, v.co)
        if n is None:
            out.append(False)
            continue
        hit = outer_tree.ray_cast(v.co - n * 0.002, Vector(n), REACH)
        out.append(hit[0] is not None)
    return out


def _clamp(ob, outer_tree, body_tree, own_normals=False, offset=OFFSET, rounds=3, sign=1.0):
    """Pushes vertices under the outer surface by `offset` (`sign` -1: lifts them over an
    inner surface instead, for loose parts); returns (moved, pokes left). Two probes per
    vertex: the nearest outer point (`NEAR`) and a ray along the outward normal from
    `offset` out down to `POKE` in (what the game's layer check measures)."""
    from mathutils import Vector

    moved, pokes = set(), 0
    for r in range(rounds + 1):
        pokes = 0
        for v in ob.data.vertices:
            n = v.normal if own_normals else _outward(body_tree, v.co)
            if n is None:
                continue
            n = Vector(n) * sign
            worst = None
            loc, nq, _i, _d = outer_tree.find_nearest(v.co, NEAR)
            if loc is not None:
                if nq.dot(n) < 0:
                    nq = -nq
                s = (v.co - loc).dot(nq)
                if s > -offset + 1e-5:
                    worst = (s, nq)
            hit = outer_tree.ray_cast(v.co + n * offset, -n, offset + POKE)
            if hit[0] is not None:
                s = offset - hit[3]
                if worst is None or s > worst[0]:
                    worst = (s, n)
            if worst is None:
                continue
            s, d = worst
            if r == rounds:
                pokes += s > 0.001
                continue
            v.co = v.co - d * (s + offset)
            moved.add(v.index)
        ob.data.update()
    return len(moved), pokes


def compose(body_tree, objs, keys, rank, skins=()):
    """The visible-layers-only outfit: deletes covered inner faces (and the skin under the
    clothes, `skins`), pushes what stays under the layer on it. `body_tree`: BVH of the
    whole rest skin (the outward normals); `rank(key)` -> layer rank. Returns stats."""
    import bpy

    st = {"culled_faces": {}, "moved_verts": {}, "pokes": {}}
    order = sorted(objs, key=lambda o: -rank(keys[o.name]))
    hiding = [o for o in objs if not o.name.endswith(LOOSE)]
    for o in order:
        r = rank(keys[o.name])
        outer = [x for x in hiding if rank(keys[x.name]) > r and x is not o]
        if not outer:
            continue
        tree = _tree(outer)
        cov = _covered(o, tree, body_tree)
        st["culled_faces"][o.name] = _delete_faces(o, [not c for c in cov])
        tree = _tree(outer)
        moved, pokes = _clamp(o, tree, body_tree)
        st["moved_verts"][o.name], st["pokes"][o.name] = moved, pokes
    for o in [x for x in objs if x.name.endswith(LOOSE)]:
        r = rank(keys[o.name])
        inner = [x for x in hiding if rank(keys[x.name]) < r] + list(skins)
        if inner:
            moved, pokes = _clamp(o, _tree(inner), body_tree, sign=-1.0)
            st["moved_verts"][o.name], st["pokes"][o.name] = moved, pokes
    tree = _tree(hiding)
    for sk in skins:
        sk.data.update()
        cov = _covered(sk, tree, body_tree, own_normals=True)
        st["culled_faces"][sk.name] = _delete_faces(sk, [not c for c in cov])
        moved, pokes = _clamp(sk, tree, body_tree, own_normals=True)
        st["moved_verts"][sk.name], st["pokes"][sk.name] = moved, pokes
    for o in [x for x in objs if not x.data.polygons]:
        objs.remove(o)
        bpy.data.objects.remove(o)
    st["pokes_total"] = sum(st["pokes"].values())
    return st


def weave_material(name, rgb, fabric, cache={}):
    """Cloth material with tiled albedo, normal and roughness maps (`weave_height`)."""
    from . import hdbody

    if fabric == "metal":
        import bpy

        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bs = m.node_tree.nodes["Principled BSDF"]
        bs.inputs["Base Color"].default_value = (*rgb, 1.0)
        bs.inputs["Metallic"].default_value = 1.0
        bs.inputs["Roughness"].default_value = 0.35
        return m
    kind, threads, depth_mm, rough = {"wool": ("twill", 40, 0.25, 0.9), "linen": ("plain", 48, 0.12, 0.75),
                                      "leather": ("plain", 6, 0.05, 0.55)}[fabric]
    if fabric not in cache:
        h = weave_height(TEX_NRM, threads, kind)
        nrm = hdbody.height_to_normal(h * depth_mm, TILE_M * 1000 / TEX_NRM)
        hs = h[::TEX_NRM // TEX, ::TEX_NRM // TEX]
        cache[fabric] = (hdbody._image(f"hd_{fabric}_normal", nrm * 0.5 + 0.5, non_color=True), hs,
                         hdbody._image(f"hd_{fabric}_rough", np.clip(rough + 0.08 * (hs - 0.5), 0, 1), non_color=True))
    nimg, hs, rimg = cache[fabric]
    lin = np.array([c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb])
    alb = np.clip(lin[None, None, :] * (0.72 + 0.4 * hs[..., None]), 0, 1) ** (1 / 2.2)
    aimg = hdbody._image(f"{name}.albedo", alb)
    m = hdbody.skin_material(name, aimg, nimg, rimg, 0.8)
    m.node_tree.nodes["Principled BSDF"].inputs["Subsurface Weight"].default_value = 0.0
    if fabric == "wool":
        m.node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 0.3
    return m


def _uv(ob):
    me = ob.data
    uv = me.uv_layers.get("UVMap") or me.uv_layers.new(name="UVMap")
    for p in me.polygons:
        for li in p.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            uv.data[li].uv = box_uv(co, p.normal)


def body_parts(ob):
    """`_part` of a skinned mesh's vertices from its strongest bone."""
    from . import rig

    names = {g.index: g.name for g in ob.vertex_groups}
    out = []
    for v in ob.data.vertices:
        g = max(v.groups, key=lambda e: e.weight) if v.groups else None
        out.append(rig.part_of_bone(names[g.group]) if g else 0)
    return out


def gore(ob, layer):
    """`_PART`, `_GORE` and the decay mask (COLOR_0) of `ob`."""
    from . import decay

    ps = body_parts(ob)
    decay.add_masks(ob, layer, seed=sum(map(ord, ob.name)), part_of_vertex=ps.__getitem__)


def finish_pieces(objs, keys, palette, mats, prefix="hd"):
    """UVs, one material per key (with `acr_decay`), gore masks; returns the objects."""
    from . import decay

    for o in objs:
        key = keys[o.name]
        fabric, layer = FABRIC.get(key, ("wool", "wool"))
        mkey = {"coat_skirts": "coat", "turnbacks": "lining"}.get(key, key)
        if mkey in mats:
            mats[key] = mats[mkey]
        if key not in mats:
            rgb = palette.get(key) or palette.get({"coat_skirts": "coat", "turnbacks": "lining", "brass": "belts"}.get(key, ""), (0.5, 0.5, 0.5))
            m = weave_material(f"{prefix}.{mkey}.hd", rgb, fabric)
            m["acr_decay"] = decay.extras(layer)
            mats[key] = mats[mkey] = m
        o.data.materials.clear()
        o.data.materials.append(mats[key])
        _uv(o)
        gore(o, layer)


def join(name, objs, arm):
    """One skinned object from `objs`; weights limited to 4 and normalised."""
    import bpy

    from . import hdbody

    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    ob = objs[0]
    ob.name = ob.data.name = name
    for m in list(ob.modifiers):
        if m.type != "ARMATURE":
            ob.modifiers.remove(m)
    if not any(m.type == "ARMATURE" for m in ob.modifiers):
        ob.modifiers.new("Armature", "ARMATURE").object = arm
    ob.parent = arm
    for p in ob.data.polygons:
        p.use_smooth = True
    return ob, hdbody.limit_weights(ob)


def _decimated(o, ratio):
    import bpy

    from . import body as bd

    c = o.copy()
    c.data = o.data.copy()
    c.name = c.data.name = o.name + ".dec"
    bd.link(c)
    if ratio < 1.0 and c.data.polygons:
        d = c.modifiers.new("dec", "DECIMATE")
        d.ratio = max(ratio, 0.002)
        d.use_collapse_triangulate = True
        bpy.context.view_layer.objects.active = c
        c.modifiers.move(len(c.modifiers) - 1, 0)
        bpy.ops.object.modifier_apply(modifier="dec")
    return c


def build(ctx, arm, body, body_lods=(), palette=None):
    """Dresses the HD master `body` (rest pose, skinned to `arm`) in the EngM18 outfit;
    culls its skin and that of `body_lods` (its levels 1..3) under the clothes. Returns
    (outfit, [outfit levels])."""
    import bpy

    from . import drape, garments as gm, line

    st = ctx.stats.setdefault("outfit", {})
    palette = dict(line.PALETTE, **(palette or {}))
    arm.data.pose_position = "REST"
    t0 = time.time()
    pieces, keys, st["drape"] = drape_alone(ctx.name, arm, body)
    sh = shoes(ctx.name, arm, body)
    pieces.append(sh)
    keys[sh.name] = "shoes"
    st["drape_s"] = round(time.time() - t0, 1)

    def rank(k):
        return {"coat_skirts": 5, "turnbacks": 6, "shoes": 1.5}.get(k, gm.LAYER_RANK.get(k, 5))

    st["source_tris"] = {o.name: _tris(o) for o in pieces}
    body_tree = _tree([body])
    t1 = time.time()
    # the levels see the master's pieces, decimated before the master is cut
    total = sum(_tris(o) for o in pieces)
    level_pieces = []
    for target in LODS:
        lp = [_decimated(o, target / total) for o in pieces]
        level_pieces.append(({x.name: keys[o.name] for x, o in zip(lp, pieces)}, lp))
    st["compose"] = compose(body_tree, pieces, keys, rank, skins=(body,))
    st["feet_culled"] = {sk.name: cull_feet(sk) for sk in (body, *body_lods)}
    st["compose_s"] = round(time.time() - t1, 1)
    ch = _Ch(ctx.name, arm, body, ())
    for o in pieces:
        ch.add("clothes", o, keys[o.name])
    st["buttons"] = [o.name for o in drape.buttons(ch, list(pieces), f"{ctx.name}.hd")]
    bk = shoe_buckles(ctx.name, arm, sh)
    pieces.append(bk)
    keys[bk.name] = "brass"
    st["shoes"] = {"tris": _tris(sh), "buckle_tris": _tris(bk)}
    for o in ch.layers["clothes"]:
        if o not in pieces:
            pieces.append(o)
            keys[o.name] = ch.keys[o.name]
    mats = {}
    finish_pieces(pieces, keys, palette, mats, ctx.name)
    outfit, st["weights"] = join(f"{ctx.name}.hd.outfit", pieces, arm)
    st["tris"] = {"master": _tris(outfit)}
    levels = []
    for (lk, lp), target, skin in zip(level_pieces, LODS, list(body_lods) + [None] * 3):
        c = compose(body_tree, lp, lk, rank, skins=(skin,) if skin is not None else ())
        st["compose"][f"lod{target}_pokes"] = c["pokes_total"]
        finish_pieces(lp, lk, palette, mats, ctx.name)
        lo, w = join(f"{ctx.name}.hd.outfit.lod{target}", lp, arm)
        lo["acr_lod_of"] = outfit.name
        st["weights"][str(target)] = w
        st["tris"][str(target)] = _tris(lo)
        levels.append(lo)
    if body_lods:
        st["skin_tris_after"] = {sk.name: _tris(sk) for sk in (body, *body_lods)}
    st["total_s"] = round(time.time() - t0, 1)
    arm.data.pose_position = "POSE"
    print(f"[outfit_hd] {st['tris']} in {st['total_s']} s, pokes {st['compose']['pokes_total']}", flush=True)
    return outfit, levels
