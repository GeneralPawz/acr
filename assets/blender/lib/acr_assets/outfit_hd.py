"""High-detail outfit on the HD body (`hdbody`): the RDR2-style recipe (docs/clothing.md,
"High-detail outfits").

1. Each garment of the 1768 drafts (`garments`) is draped *alone* on the HD master (no
   layer stack in one simulation): `drape.dress` with one garment at `SPACING_CM`, the
   short damped sim of `drape.simulate`, seams welded, weights from the skin.
2. One outfit mesh from the visible layers only (`compose`): every inner face covered by
   an outer layer (rays along the skin normal) is deleted, and so is the skin under the
   clothes; what stays of an inner layer is pushed `OFFSET` under the layer on it, so it
   cannot poke through in the rest pose and, with the weights of the surface beneath,
   in no pose. Loose panels use the breeches' weights on matching skirt chains, so
   their covered breeches can also be deleted without exposing holes in a stride.
3. Woven cloth: tileable twill (wool) and plain weave (linen) albedo, normal and
   roughness maps on box-projected UVs (`weave`), brass buttons (`drape.buttons`).
4. Gore and decay: `_PART` from the weights (`rig.part_of_bone`), `_GORE` and the decay
   mask per layer (`decay.add_masks`), `acr_decay` extras on every material; the renderer
   caps cuts itself (docs/rendering-3d.md, "Caps"), so the skin inside a sleeve may go.
5. Levels: the finished visible outfit decimated to `LODS`, with material and gore
   attributes preserved, written with `acr_lod` / `acr_lod_of` like the body's.

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


def mesh_quality(ob):
    """Rest-space edge length and shoulder dihedral RMS, in a fixed anatomical window."""
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bm.normal_update()
    angles = []
    for e in bm.edges:
        p = (e.verts[0].co + e.verts[1].co) / 2
        if len(e.link_faces) == 2 and 1.30 < p.z < 1.48 and 0.08 < abs(p.x) < 0.29:
            if all(".coat." in ob.data.materials[f.material_index].name for f in e.link_faces):
                angles.append(math.degrees(e.calc_face_angle()))
    out = {"longest_edge_m": max((e.calc_length() for e in bm.edges), default=0.0),
           "shoulder_edges": len(angles),
           "shoulder_rms_deg": (sum(a * a for a in angles) / len(angles)) ** 0.5 if angles else 0.0}
    bm.free()
    return out


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
    body_tree = _tree([body])
    with _hd_measure(body, garments, spacing):
        for g in garments:
            ch = _Ch(name, arm, body, (g,))
            r = {}
            t0 = time.time()
            made = drape.dress(ch, garments=(g,), report=r,
                               weld_limit=0.12 if g != "shirt" else None,
                               hem_even=False,
                               post_weld=lambda ob, garment: relax_tailoring(ob, garment, body_tree))
            r = dict(r.get(g, {}), s=round(time.time() - t0, 1))
            rep[g] = r
            objs += ch.layers["clothes"]
            keys.update(made)
            print(f"[outfit_hd] {g} draped alone: {r}", flush=True)
    return objs, keys, rep


def shoes(name, arm, body, skins=()):
    """Closed low leather shoes on a last, with rigid instep buckle frames."""
    import bpy
    import bmesh
    from mathutils import Vector

    from . import armature as am, body as bd

    verts, faces, sides = [], [], []
    # Back to toe: rounded heel, high instep/tongue, broad ball, closed rounded toe.
    rings = ((0.085, 0.010, 0.065), (0.075, 0.034, 0.095),
             (0.030, 0.041, 0.130), (-0.015, 0.045, 0.115),
             (-0.060, 0.050, 0.080), (-0.115, 0.052, 0.060),
             (-0.170, 0.043, 0.050), (-0.195, 0.028, 0.040),
             (-0.205, 0.005, 0.025))
    seg = 20
    for side in ("l", "r"):
        cx = arm.data.bones[f"foot_{side}"].head_local.x
        start = len(verts)
        for y, width, height in rings:
            for i in range(seg):
                a = 2 * math.pi * i / seg
                z = 0.012 + height * (0.5 + 0.5 * math.sin(a))
                # A flat sole, with a small heel under the rear quarter.
                z = max(0.008 if y > 0.025 else 0.014, z)
                verts.append((cx + width * math.cos(a), y, z))
                sides.append(side)
        for r in range(len(rings) - 1):
            for i in range(seg):
                a = start + r * seg + i
                b = start + r * seg + (i + 1) % seg
                faces.append((a, b, b + seg, a + seg))
        faces.append(tuple(start + i for i in reversed(range(seg))))
        faces.append(tuple(start + (len(rings)-1)*seg + i for i in range(seg)))
    me = bpy.data.meshes.new(f"{name}.shoes")
    me.from_pydata(verts, [], faces)
    ob = bd.link(bpy.data.objects.new(me.name, me))
    bm = bmesh.new(); bm.from_mesh(me)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(me); bm.free()
    for side in ("l", "r"):
        vg = ob.vertex_groups.new(name=f"foot_{side}")
        vg.add([i for i, s in enumerate(sides) if s == side], 1.0, "REPLACE")
    am.bind(ob, arm)
    bm = bmesh.new()
    for side in ("l", "r"):
        cx = arm.data.bones[f"foot_{side}"].head_local.x
        for dx, dy, sx, sy in ((-0.020, 0, 0.005, 0.030), (0.020, 0, 0.005, 0.030),
                              (0, -0.013, 0.045, 0.004), (0, 0.013, 0.045, 0.004)):
            for v in bmesh.ops.create_cube(bm, size=1.0)["verts"]:
                v.co = Vector((cx + dx + v.co.x*sx, -0.030 + dy + v.co.y*sy, 0.116 + v.co.z*0.005))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(f"{name}.shoe_buckles")
    bm.to_mesh(me); bm.free()
    buckle = bd.link(bpy.data.objects.new(me.name, me))
    for side in ("l", "r"):
        vg = buckle.vertex_groups.new(name=f"foot_{side}")
        vg.add([v.index for v in me.vertices if (v.co.x > 0) == (side == "l")], 1.0, "REPLACE")
    am.bind(buckle, arm)
    for sk in (body, *skins):
        _delete_faces(sk, [v.co.z >= 0.145 for v in sk.data.vertices])
    return [(ob, "shoes"), (buckle, "brass")]

def relax_tailoring(ob, garment, body_tree):
    """Remove solver-scale buckling before splitting panels, keeping the simulated cut.

    Broad wool shoulders need a smooth surface; this is mesh fairing after the one
    simulation, with the skin as a clearance constraint, never another cloth solve.
    """
    import bmesh
    from mathutils import Vector

    if garment != "coat":
        return
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    # Only the authored sewing pairs are welded by drape.dress. Merging nearby
    # vertices here would glue the two walls of a buckled sleeve to one another.
    bm.verts.ensure_lookup_table()
    original = {v: v.co.copy() for v in bm.verts}
    selected = [v for v in bm.verts if 1.18 < v.co.z < 1.47 or
                (v.co.z >= 1.47 and abs(v.co.x) > 0.10)]
    for _ in range(45):
        nxt = {}
        for v in selected:
            nb = [e.other_vert(v) for e in v.link_edges]
            if not nb:
                continue
            avg = sum((x.co for x in nb), Vector()) / len(nb)
            blend = min(1.0, max(0.0, (original[v].z - 1.18) / 0.12))
            q = v.co.lerp(avg, 0.5 * blend)
            loc, normal, _i, _d = body_tree.find_nearest(q)
            if loc is not None:
                d = (q - loc).dot(normal)
                if d < 0.012:
                    q += normal * (0.012 - d)
            nxt[v] = q
        for v, q in nxt.items():
            v.co = q
    for v in selected:
        # The simulated cut supplies the topology and silhouette. Set the fitted
        # wool's stand-off on the smooth HD skin, rather than preserving tiny
        # inside-out folds left by a crushed sleeve cap.
        loc, n, _i, _d = body_tree.find_nearest(v.co)
        if loc is not None:
            blend = min(1.0, max(0.0, (original[v].z - 1.18) / 0.12))
            v.co = v.co.lerp(loc + n * 0.018, blend)
    # The standing collar is a fitted band, not a continuation of the shoulder's
    # smoothing domain. Project its radius onto the neck and preserve its height.
    for v in bm.verts:
        if original[v].z >= 1.47 and abs(original[v].x) < 0.10:
            z = min(original[v].z, 1.535)
            radial = Vector((original[v].x, original[v].y, 0)).normalized()
            loc, n, _i, _d = body_tree.ray_cast(Vector((0, 0, z)), radial, 0.20)
            if loc is not None:
                v.co = loc + radial * 0.014
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(ob.data)
    bm.free()
    ob.data.update()


def skirt_rig(arm):
    """Independent skirt chains share the leg chains' bind pivots and axes.

    Matching transforms, rather than delayed rotations about displaced pivots, keep a
    fixed clearance to the breeches throughout interpolated walk, drill and death clips.
    """
    import bpy

    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    for col in ("fl", "fr", "bl", "br"):
        side = col[-1]
        for level, source in ((1, "thigh"), (2, "shin")):
            dst = arm.data.edit_bones[f"skirt_{col}_0{level}"]
            src = arm.data.edit_bones[f"{source}_{side}"]
            dst.matrix = src.matrix.copy()
            dst.length = src.length
    bpy.ops.object.mode_set(mode="OBJECT")


def skirt_keys(keys):
    """Leg-following skirt bones in every clip, including run, drills and deaths."""
    out = []
    for frame, spec in keys:
        spec = dict(spec)
        for col in ("fl", "fr", "bl", "br"):
            for level, source in ((1, "thigh"), (2, "shin")):
                spec[f"skirt_{col}_0{level}"] = spec.get(f"{source}_{col[-1]}", (0, 0, 0))
        out.append((frame, spec))
    return out


def fit_stock(pieces, keys, body_tree):
    """Fit the drafted stock to a smooth closed neck band, without inward clamping."""
    from mathutils import Vector

    for ob in pieces:
        if keys[ob.name] != "stock":
            continue
        lo = min(v.co.z for v in ob.data.vertices)
        hi = max(v.co.z for v in ob.data.vertices)
        for v in ob.data.vertices:
            z = 1.475 + 0.045 * (v.co.z - lo) / max(hi - lo, 1e-6)
            radial = Vector((v.co.x, v.co.y, 0)).normalized()
            loc, _n, _i, _d = body_tree.ray_cast(Vector((0, 0, z)), radial, 0.20)
            if loc is not None:
                v.co = loc + radial * 0.006
        for vg in list(ob.vertex_groups):
            ob.vertex_groups.remove(vg)
        ob.vertex_groups.new(name="neck").add(list(range(len(ob.data.vertices))), 1.0, "REPLACE")
        ob.data.update()


def lay_skirts(pieces, keys):
    """Lay loose panels over the breeches and copy the underlying surface's weights.

    The corresponding leg influences use the skirt bones, which have identical bind
    pivots and animation. The coat remains a separate cloth chain for cuts/ragdolls.
    """
    from mathutils.bvhtree import BVHTree
    from mathutils import Vector

    breeches = [o for o in pieces if keys[o.name] == "breeches"]
    verts, tris, source_weights = [], [], []
    for source in breeches:
        names = {g.index: g.name for g in source.vertex_groups}
        base = len(verts)
        verts.extend(v.co.copy() for v in source.data.vertices)
        source_weights.extend({names[e.group]: e.weight for e in v.groups} for v in source.data.vertices)
        source.data.calc_loop_triangles()
        tris.extend(tuple(base + i for i in t.vertices) for t in source.data.loop_triangles)
    inner = BVHTree.FromPolygons(verts, tris, all_triangles=True)
    for ob in pieces:
        if not ob.name.endswith(LOOSE):
            continue
        ws = []
        for v in ob.data.vertices:
            side = 1 if v.co.x > 0 else -1
            leg = max(0.0, min(1.0, (0.97 - v.co.z) / 0.12))
            outward = Vector((v.co.x - side * 0.09 * leg, v.co.y, 0)).normalized()
            loc, _n, ti, distance = inner.ray_cast(v.co + outward * 0.06, -outward, 0.14)
            if loc is not None:
                gap = 0.06 - distance
                if gap < 0.025:
                    v.co += outward * min(0.05, 0.025 - gap)
            else:
                loc, _n, ti, _dist = inner.find_nearest(v.co)
            ia, ib, ic = tris[ti]
            a, b, c = verts[ia], verts[ib], verts[ic]
            u, vv, q = b - a, c - a, loc - a
            uu, uv, vvv, qu, qv = u.dot(u), u.dot(vv), vv.dot(vv), q.dot(u), q.dot(vv)
            den = max(uu * vvv - uv * uv, 1e-20)
            wb, wc = (vvv * qu - uv * qv) / den, (uu * qv - uv * qu) / den
            w = {}
            for vi, share in ((ia, 1 - wb - wc), (ib, wb), (ic, wc)):
                for bone, value in source_weights[vi].items():
                    w[bone] = w.get(bone, 0.0) + value * max(0, share)
            ws.append(w)
        ob.data.update()
        for vg in list(ob.vertex_groups):
            ob.vertex_groups.remove(vg)
        for v, weights in zip(ob.data.vertices, ws):
            for bone, weight in weights.items():
                if bone.startswith(("thigh_", "shin_")):
                    col = ("f" if v.co.y < 0 else "b") + bone[-1]
                    bone = f"skirt_{col}_0{1 if bone.startswith('thigh_') else 2}"
                vg = ob.vertex_groups.get(bone) or ob.vertex_groups.new(name=bone)
                vg.add([v.index], weight, "REPLACE")

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
        if keys[o.name] in ("shoes", "brass", "coat", "facings", "lapels") or o.name.endswith(LOOSE):
            continue
        outer = [x for x in hiding if rank(keys[x.name]) > r and x is not o]
        if keys[o.name] == "breeches":
            # These panels now have the same deformation as the breeches. Covered
            # breeches stay covered during the clips and can be removed safely.
            outer += [x for x in objs if x.name.endswith(LOOSE)]
        if not outer:
            continue
        tree = _tree(outer)
        if keys[o.name] == "waistcoat":
            # Only the open coat front exposes the waistcoat. Its back and
            # shoulders are covered in every drill, even when a ray misses a hem.
            _delete_faces(o, [v.co.y < -0.035 and abs(v.co.x) < 0.12 and 1.05 <= v.co.z < 1.44
                              for v in o.data.vertices])
        cov = _covered(o, tree, body_tree)
        if keys[o.name] == "breeches":
            cov = [c or tree.find_nearest(v.co)[3] < 0.006 for c, v in zip(cov, o.data.vertices)]
        # A tiny remaining corner of a hidden face is the white fleck on the coat.
        # Trim that one boundary triangle too, rather than retaining its whole area.
        import bmesh

        bm = bmesh.new()
        bm.from_mesh(o.data)
        bm.verts.ensure_lookup_table()
        gone = [f for f in bm.faces if any(cov[v.index] for v in f.verts)]
        st["culled_faces"][o.name] = len(gone)
        bmesh.ops.delete(bm, geom=gone, context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(o.data)
        bm.free()
        tree = _tree(outer)
        moved, pokes = (0, 0) if keys[o.name] == "stock" else _clamp(o, tree, body_tree)
        st["moved_verts"][o.name], st["pokes"][o.name] = moved, pokes
    # Loose panels were already laid over the breeches with their fixed clearance.
    # Repeated nearest-point pushes here buckle the waist onto unrelated body faces.
    tree = _tree(hiding)
    for sk in skins:
        sk.data.update()
        cov = _covered(sk, tree, body_tree, own_normals=True)
        st["culled_faces"][sk.name] = _delete_faces(sk, [not c for c in cov])
        # Never move the exposed skin boundary: doing so tears the neck into spikes.
        st["moved_verts"][sk.name], st["pokes"][sk.name] = 0, 0
    for o in [x for x in objs if not x.data.polygons]:
        objs.remove(o)
        bpy.data.objects.remove(o)
    st["pokes_total"] = sum(st["pokes"].values())
    return st


def hide_tailored_skin(skins, arm):
    """The uniform completely covers trunk and legs, including gaps between ray probes.

    Keep the head, neck and exposed wrists/hands. Sleeve interiors are closed by the
    game's cut caps, so hidden arm skin is not required for a severed sleeve.
    """
    counts = {}
    for ob in skins:
        names = {g.index: g.name for g in ob.vertex_groups}
        keep = []
        for v in ob.data.vertices:
            bone = names[max(v.groups, key=lambda e: e.weight).group] if v.groups else "hips"
            visible = bone in ("head", "neck") or bone.startswith(("hand_", "thumb_", "index_", "middle_", "ring_", "pinky_"))
            if bone.startswith("forearm_"):
                wrist = arm.data.bones[f"hand_{bone[-1]}"].head_local
                visible = (v.co - wrist).length < 0.07
            keep.append(visible)
        counts[ob.name] = _delete_faces(ob, keep)
    return counts


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
    ob.data.validate(clean_customdata=False)
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
    from . import drape, garments as gm, hdbody, line

    st = ctx.stats.setdefault("outfit", {})
    palette = dict(line.PALETTE, **(palette or {}))
    arm.data.pose_position = "REST"
    t0 = time.time()
    pieces, keys, st["drape"] = drape_alone(ctx.name, arm, body)
    body_tree = _tree([body])
    fit_stock(pieces, keys, body_tree)
    lay_skirts(pieces, keys)
    for sh, key in shoes(ctx.name, arm, body, body_lods):
        pieces.append(sh)
        keys[sh.name] = key
    st["drape_s"] = round(time.time() - t0, 1)

    def rank(k):
        return {"coat_skirts": 5, "turnbacks": 6, "shoes": 1.5}.get(k, gm.LAYER_RANK.get(k, 5))

    st["source_tris"] = {o.name: _tris(o) for o in pieces}
    t1 = time.time()
    st["compose"] = compose(body_tree, pieces, keys, rank, skins=(body,))
    st["hidden_skin_faces"] = hide_tailored_skin((body, *body_lods), arm)
    clothing_tree = _tree(pieces)
    for skin in body_lods:
        covered = _covered(skin, clothing_tree, body_tree, own_normals=True)
        st["compose"]["culled_faces"][skin.name] = _delete_faces(skin, [not c for c in covered])
    st["compose_s"] = round(time.time() - t1, 1)
    ch = _Ch(ctx.name, arm, body, ())
    for o in pieces:
        ch.add("clothes", o, keys[o.name])
    st["buttons"] = [o.name for o in drape.buttons(ch, list(pieces), f"{ctx.name}.hd")]
    for o in ch.layers["clothes"]:
        if o not in pieces:
            pieces.append(o)
            keys[o.name] = ch.keys[o.name]
    mats = {}
    finish_pieces(pieces, keys, palette, mats, ctx.name)
    outfit, st["weights"] = join(f"{ctx.name}.hd.outfit", pieces, arm)
    st["quality"] = mesh_quality(outfit)
    st["tris"] = {"master": _tris(outfit)}
    levels = []
    for target in LODS:
        # Simplify the visible outfit, not separate uncut layers. Culling a coarse
        # triangle around a small facing otherwise removes a large patch of coat.
        lo = _decimated(outfit, target / _tris(outfit))
        lo.name = lo.data.name = f"{ctx.name}.hd.outfit.lod{target}"
        lo.data.validate(clean_customdata=False)
        w = hdbody.limit_weights(lo)
        parts = body_parts(lo)
        attr = lo.data.attributes.get("_part")
        if attr:
            for d, part in zip(attr.data, parts):
                d.value = part
        lo["acr_lod_of"] = outfit.name
        st["weights"][str(target)] = w
        st["tris"][str(target)] = _tris(lo)
        levels.append(lo)
    st["lods_from_visible_master"] = True
    if body_lods:
        st["skin_tris_after"] = {sk.name: _tris(sk) for sk in (body, *body_lods)}
    st["total_s"] = round(time.time() - t0, 1)
    arm.data.pose_position = "POSE"
    print(f"[outfit_hd] {st['tris']} in {st['total_s']} s, pokes {st['compose']['pokes_total']}", flush=True)
    return outfit, levels
