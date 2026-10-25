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
    "shirt": ("linen", "linen"), "stock": ("linen", "leather"), "fallband": ("linen", "linen"),
    "shoes": ("leather", "leather"),
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
    patch = bm.faces.layers.int.get("hd_chest_patch")
    if patch:
        out["chest_patch_faces"] = sum(bool(f[patch]) for f in bm.faces)
        out["chest_patch_reversed_faces"] = sum(bool(f[patch]) and f.normal.y > 0 for f in bm.faces)
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
                               post_weld=lambda ob, garment: tailor_trim(ob, garment, body_tree))
            r = dict(r.get(g, {}), s=round(time.time() - t0, 1))
            rep[g] = r
            objs += ch.layers["clothes"]
            keys.update(made)
            print(f"[outfit_hd] {g} draped alone: {r}", flush=True)
    return objs, keys, rep


def tailor_trim(ob, garment, body_tree):
    """Keep the coat simulation unchanged; replace only its collar/cuff panels.

    Remove by authored panel ID, before material splitting: spatial masks can mistake
    a lapel for the collar or leave a stray cuff triangle behind.
    """
    import bmesh
    from . import garments

    relax_tailoring(ob, garment, body_tree)
    if garment != "coat":
        return
    hang_skirts(ob)
    patch_chest(ob, body_tree)
    names = list(garments.coat()[0])
    ids = {names.index(n) for n in ("collar", "cuff")}
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    panel = bm.faces.layers.int["panel"]
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f[panel] in ids], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(ob.data)
    bm.free()


def patch_chest(ob, body_tree):
    """Retopologise the buckled armhole fronts, sharing the settled patch boundary.

    The old sewn cap had overlapping, reversed triangles in this window. Fill the
    local patch with a connected grid and fit it from the front, so the projection
    cannot jump into the armpit. This is fitting after the single garment solve.
    """
    import bmesh
    from mathutils import Vector
    from . import garments

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bmesh.ops.remove_doubles(bm, verts=list(bm.verts), dist=1e-6)
    mark = bm.faces.layers.int.get("hd_chest_patch") or bm.faces.layers.int.new("hd_chest_patch")
    panel = bm.faces.layers.int.get("panel")
    front_id = list(garments.coat()[0]).index("coat_front")
    wool_ids = {list(garments.coat()[0]).index(n) for n in ("coat_front", "sleeve_top", "sleeve_under")}
    gone = []
    for f in bm.faces:
        p = f.calc_center_median()
        if 1.18 < p.z < 1.455 and 0.105 < abs(p.x) < 0.275 and p.y < -0.025 and (panel is None or f[panel] in wool_ids):
            gone.append(f)
    old_boundary = {e for e in bm.edges if e.is_boundary}
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bmesh.ops.delete(bm, geom=[e for e in bm.edges if not e.link_faces], context="EDGES")
    edges = [e for e in bm.edges if e.is_boundary and e not in old_boundary and
             all(1.13 < v.co.z < 1.49 and abs(v.co.x) > 0.08 for v in e.verts)]
    filled = bmesh.ops.holes_fill(bm, edges=edges, sides=0)["faces"]
    for f in filled:
        f[mark] = 1
        if panel is not None:
            f[panel] = front_id
    triangles = bmesh.ops.triangulate(bm, faces=filled)["faces"]
    for f in triangles:
        f[mark] = 1
        if panel is not None:
            f[panel] = front_id
    splits = {e for f in triangles for e in f.edges}
    if splits:
        bmesh.ops.subdivide_edges(bm, edges=list(splits), cuts=3, use_grid_fill=True)
    patch = {v for f in bm.faces if f[mark] for v in f.verts}
    for v in patch:
        hit = body_tree.ray_cast(Vector((v.co.x, -0.5, v.co.z)), Vector((0, 1, 0)), 0.65)[0]
        if hit is not None:
            v.co.y = hit.y-0.018
    bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
    # Give the new front grid consistent winding even beside a non-manifold sew.
    for f in bm.faces:
        if f[mark] and f.normal.y > 0:
            f.normal_flip()
    bm.to_mesh(ob.data)
    bm.free()
    ob.data.update()


def strip_inlay_rims(pieces, keys):
    """The lapels are inlays in the coat, so their shared red/blue edge has no cut rim."""
    import bmesh

    for ob in pieces:
        if keys[ob.name] != "facings" or ob.get("hd_trim"):
            continue
        bm = bmesh.new()
        bm.from_mesh(ob.data)
        # Draft panels are triangles. Only the generic solidify hem added quads.
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if len(f.verts) == 4], context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(ob.data)
        bm.free()


def hang_skirts(ob):
    """Fair the settled skirts into continuous gravity lines from the sewn waist.

    Work before material splitting: the lining and red wool share the same vertices,
    and the waist anchor belongs to both bodice and skirt. Keep small baked folds.
    """
    import bmesh
    from . import garments

    names = list(garments.coat()[0])
    body_ids = {names.index(n) for n in ("coat_front", "coat_back", "lapel")}
    skirt_ids = {names.index(n) for n in ("skirt_front", "skirt_back", "turnback")}
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    panel = bm.faces.layers.int["panel"]
    drop = bm.verts.layers.float.get("hd_skirt_drop") or bm.verts.layers.float.new("hd_skirt_drop")
    skirt = {v for f in bm.faces if f[panel] in skirt_ids for v in f.verts}
    waist = [v for v in skirt if any(f[panel] in body_ids for f in v.link_faces)]
    if not waist:
        raise ValueError("HD coat has no sewn waist anchors")
    anchors = sorted((math.atan2(v.co.x, -v.co.y), math.hypot(v.co.x, v.co.y), v.co.z) for v in waist)
    angles = [a-2*math.pi for a, _r, _z in anchors]+[a for a, _r, _z in anchors]+[a+2*math.pi for a, _r, _z in anchors]
    radii = [r for _a, r, _z in anchors]*3
    heights = [z for _a, _r, z in anchors]*3
    for v in skirt:
        a = math.atan2(v.co.x, -v.co.y)
        r0, z0 = float(np.interp(a, angles, radii)), float(np.interp(a, angles, heights))
        d = max(0.0, z0-v.co.z)
        v[drop] = d
        t = min(1.0, d/0.44)
        r = math.hypot(v.co.x, v.co.y)
        residual = max(-0.003, min(0.003, (r-r0)*0.12))
        target = r0+0.045*t+residual*min(1.0, d/0.08)
        blend = min(1.0, d/0.07)
        blend = blend*blend*(3-2*blend)
        if r > 1e-6:
            v.co.x *= 1+blend*(target/r-1)
            v.co.y *= 1+blend*(target/r-1)
    bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
    bm.to_mesh(ob.data)
    bm.free()


def tailored_trim(name, arm, body, pieces, keys, skin):
    """Closed shaped wool facings, linen stock and wristbands on the visible outfit.

    Smooth profiles replace crushed simulated facings. Solidify adds outward thickness
    and closed rims; the underlying sleeve supplies the cuff weights. Nothing here
    changes the coat above the cuff, the skirt, the body rig or its animation.
    """
    import bpy
    import bmesh
    from mathutils import Vector
    from . import armature, drape, hdbody

    made, report = [], {"profile": "standing_throat", "cuffs": {}, "pieces": {}}

    def piece(label, key, verts, faces, thickness, under=()):
        me = bpy.data.meshes.new(f"{name}.{label}")
        me.from_pydata(verts, [], faces)
        ob = bpy.data.objects.new(me.name, me)
        bpy.context.collection.objects.link(ob)
        bm = bmesh.new()
        bm.from_mesh(me)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(me)
        bm.free()
        for i, weights in enumerate(drape.transfer_weights(ob, body, arm, under=under)):
            for bone, weight in weights.items():
                (ob.vertex_groups.get(bone) or ob.vertex_groups.new(name=bone)).add([i], weight, "REPLACE")
        armature.bind(ob, arm)
        hdbody.limit_weights(ob)
        if thickness:
            solid = ob.modifiers.new("closed folded cloth", "SOLIDIFY")
            solid.thickness, solid.offset = thickness, 1.0
            solid.use_rim = True
            solid.use_even_offset = True
            # Apply before the armature, so the rim shares the surface's weights.
            ob.modifiers.move(len(ob.modifiers) - 1, 0)
            with bpy.context.temp_override(object=ob, active_object=ob):
                bpy.ops.object.modifier_apply(modifier=solid.name)
        ob["hd_trim"] = True
        bm = bmesh.new()
        bm.from_mesh(ob.data)
        measured = {"boundary_edges": sum(e.is_boundary for e in bm.edges),
                    "nonmanifold_edges": sum(not e.is_manifold for e in bm.edges),
                    "weights": hdbody.limit_weights(ob)}
        if thickness:
            n = len(verts)
            ds = sorted((ob.data.vertices[i+n].co-ob.data.vertices[i].co).length*1000 for i in range(n))
            measured["thickness_mm_range"] = [round(ds[0], 3), round(ds[-1], 3)]
        report["pieces"][label] = measured
        bm.free()
        keys[ob.name] = key
        made.append(ob)
        return ob

    def band(label, key, rings, thickness, under=()):
        n = len(rings[0])
        verts = [p for row in rings for p in row]
        faces = [(j*n+i, j*n+(i+1)%n, (j+1)*n+(i+1)%n, (j+1)*n+i)
                 for j in range(len(rings)-1) for i in range(n)]
        ob = piece(label, key, verts, faces, thickness, under)
        # Unbroken cylindrical grain on each band, including the folded rims.
        uv = ob.data.uv_layers.new(name="UVMap")
        circumference = sum((Vector(rings[0][(i+1)%n])-Vector(rings[0][i])).length for i in range(n))
        centers = [sum((Vector(p) for p in row), Vector())/n for row in rings]
        for p in ob.data.polygons:
            indices = [ob.data.loops[li].vertex_index % len(verts) for li in p.loop_indices]
            seam = any(i % n == 0 for i in indices) and any(i % n == n-1 for i in indices)
            for li, vi in zip(p.loop_indices, indices):
                j, i = divmod(vi, n)
                u = n if seam and i == 0 else i
                uv.data[li].uv = (u/n*circumference/TILE_M, (centers[j]-centers[0]).length/TILE_M)
        return ob

    def neck_point(a, z, gap):
        radial = Vector((math.sin(a), -math.cos(a), 0))
        # The HD neck is forward of the skeleton's origin. Probe from outside
        # inwards: a ray starting at y=0 can already be behind the neck and miss
        # its back half, which left the old stock with only a few surviving faces.
        hit = skin.ray_cast(Vector((0, -0.045, z))+radial*0.22, -radial, 0.44)[0]
        if hit is None:
            raise ValueError(f"HD neck profile misses skin at {z}")
        return hit + radial * gap

    # Fit three smooth elliptical sections above the shoulder transition. Projecting
    # each collar vertex onto the body independently follows the shoulder triangles
    # at the lower edge and makes a ragged, flapping rim instead of a tailored band.
    neck_sections = []
    body.data.calc_loop_triangles()
    for z in (1.540, 1.545, 1.550):
        pts = []
        for tri in body.data.loop_triangles:
            vs = [body.data.vertices[i].co for i in tri.vertices]
            if min(v.z for v in vs) > z or max(v.z for v in vs) < z:
                continue
            for p, q in zip(vs, vs[1:]+vs[:1]):
                if (p.z-z)*(q.z-z) < 0:
                    hit = p.lerp(q, (z-p.z)/(q.z-p.z))
                    if abs(hit.x) < 0.13 and -0.16 < hit.y < 0.10:
                        pts.append(hit)
        if not pts:
            raise ValueError(f"No HD neck section at {z}")
        cy = (min(p.y for p in pts)+max(p.y for p in pts))/2
        rx = max(abs(p.x) for p in pts)
        ry = (max(p.y for p in pts)-min(p.y for p in pts))/2
        scale = max(math.hypot(p.x/rx, (p.y-cy)/ry) for p in pts)
        neck_sections.append((z, cy, rx*scale, ry*scale))
    report["neck_sections_m"] = neck_sections

    def smooth_neck(a, z, gap):
        t = max(0.0, min(2.0, (z-1.540)/0.005))
        i = min(1, int(t))
        u = t-i
        p, q = neck_sections[i], neck_sections[i+1]
        cy, rx, ry = [p[k]*(1-u)+q[k]*u for k in (1, 2, 3)]
        sa, ca = math.sin(a), math.cos(a)
        den = math.hypot(sa, ca)
        return Vector(((rx+gap)*sa/den, cy-(ry+gap)*ca/den, z))

    # Both bands close at centre back. A low front clears the chin; the stock rises
    # above the blue wool, with a small woven fold rather than a raw triangular edge.
    angles = [2*math.pi*i/64 for i in range(64)]
    collar, stock = [], []
    for t in (0.0, 0.08, 0.35, 0.65, 0.92, 1.0):
        cr, sr = [], []
        for a in angles:
            back = (1-math.cos(a))/2
            notch = 0.004*max(0, math.cos(a))**12*t
            p = smooth_neck(a, 1.489 + 0.022*back + (0.051-0.012*back)*t-notch,
                            0.006 + 0.006*(1-t)**5 + 0.0005*math.sin(math.pi*t))
            if t <= 0.08:
                radial = Vector((math.sin(a), -math.cos(a), 0))
                fit = neck_point(a, p.z, 0.006)
                p += radial*max(0, (fit-p).dot(radial))
            cr.append(p)
            sr.append(smooth_neck(a, 1.509 + 0.014*back + 0.044*t,
                                 0.003 + 0.0005*math.sin(4*math.pi*t)))
        collar.append(cr)
        stock.append(sr)
    sleeves = [o for o in pieces if keys[o.name] == "coat"]
    band("collar", "facings", collar, 0.004, under=sleeves)
    band("stock", "stock", stock, 0.001)
    report["collar_front_flare_mm"] = round(math.hypot(
        collar[-1][0].x-collar[0][0].x, collar[-1][0].y-collar[0][0].y)*1000, 3)
    # The hooked upper fronts close above the waistcoat's V. A connected fitted
    # facing replaces the old alternating open diamonds at the throat.
    verts = []
    for z, width in ((1.415, 0.006), (1.43, 0.015), (1.45, 0.023),
                     (1.47, 0.026), (1.49, 0.028), (1.505, 0.025)):
        for u in (-1, -0.5, 0, 0.5, 1):
            x = width*u
            hit = skin.ray_cast(Vector((x, -0.4, z)), Vector((0, 1, 0)), 0.5)[0]
            verts.append(hit+Vector((0, -0.022, 0)))
    faces = [(j*5+i, j*5+i+1, (j+1)*5+i+1, (j+1)*5+i) for j in range(5) for i in range(4)]
    piece("throat_closure", "facings", verts, faces, 0.002, under=sleeves)
    # Rounded knot and two short folded linen ends, following the throat/chest.
    knot_z = 1.550
    knot_y = smooth_neck(0, knot_z, 0.008).y
    rings = []
    for z, width, depth in ((1.539, 0.006, 0.003), (1.544, 0.010, 0.006),
                            (1.554, 0.009, 0.007), (1.559, 0.004, 0.002)):
        rings.append([(width*math.sin(a), knot_y-depth*math.cos(a), z) for a in angles])
    band("stock_knot", "fallband", rings, 0.001)
    for side in (-1, 1):
        verts = []
        for t, width in ((0, 0.006), (0.25, 0.012), (0.85, 0.014), (1, 0.010)):
            z = 1.545 - 0.070*t
            for u in (-1, 0, 1):
                x = side*(0.006+0.012*t) + u*width
                p = neck_point(math.atan2(x, 0.065), z, 0.014+0.020*t)
                verts.append((x, p.y-0.002*(1-u*u), z))
        faces = [(j*3+i, j*3+i+1, (j+1)*3+i+1, (j+1)*3+i)
                 for j in range(3) for i in range(2)]
        piece(f"stock_end_{side}", "fallband", verts, faces, 0.001)
    report["collar_thickness_mm"] = 4.0
    report["stock_thickness_mm"] = 1.0

    sleeve_tree = _tree(sleeves)
    for side in ("l", "r"):
        bone = arm.data.bones[f"forearm_{side}"]
        wrist = bone.tail_local.copy()
        axis = (wrist-bone.head_local).normalized()
        back = Vector((0, 1, 0))
        back = (back-axis*back.dot(axis)).normalized()
        across = axis.cross(back).normalized()
        # A smooth circular profile encloses every sleeve probe. Its inner face is
        # 3 mm above the highest sleeve fold, not buried in the red sleeve surface.
        radii = []
        for t in (-0.120, -0.110, -0.100):
            center = wrist + axis*t
            ds = []
            for a in angles:
                radial = back*math.cos(a)+across*math.sin(a)
                loc = sleeve_tree.ray_cast(center+radial*0.16, -radial, 0.16)[0]
                if loc is not None:
                    ds.append((loc-center).length)
            radii.extend(ds)
        if not radii:
            raise ValueError(f"No sleeve beneath HD cuff {side}")
        radius = max(radii)+0.003
        cuff_rings = []
        for t in (-0.120, -0.118, -0.112, -0.064, -0.010, -0.004, -0.002):
            # A subtle rolled silhouette at both turned edges.
            r = radius + (0.0008 if t in (-0.118, -0.004) else 0)
            cuff_rings.append([wrist+axis*t+(back*math.cos(a)+across*math.sin(a))*r for a in angles])
        cuff = band(f"cuff_{side}", "facings", cuff_rings, 0.004, under=sleeves)
        # Remove hidden sleeve ends, including loose seam/hem triangles. A 20 mm
        # overlap under the closed cuff covers its interface in the animated poses.
        for ob in sleeves:
            keep = []
            for v in ob.data.vertices:
                rel = v.co-wrist
                t = rel.dot(axis)
                near = (rel-axis*t).length < radius+0.03 and -0.100 < t < 0.08
                keep.append(not near)
            _delete_faces(ob, keep)
        linen = []
        for t, r in ((-0.018, radius-0.002), (-0.002, radius-0.002),
                     (0.006, radius-0.009), (0.010, radius-0.014)):
            row = []
            for a in angles:
                radial = back*math.cos(a)+across*math.sin(a)
                center = wrist+axis*t
                q = center+radial*r
                if t == 0.010:
                    loc = skin.ray_cast(center+radial*0.12, -radial, 0.24)[0]
                    if loc is not None:
                        q = loc+radial*0.003
                row.append(q)
            linen.append(row)
        band(f"shirt_cuff_{side}", "shirt", linen, 0.001, under=(cuff,))
        # Three closed domed button meshes on the outer/back face of each cuff.
        out = (back+across*(0.6 if side == "l" else -0.6)).normalized()
        tangent = axis.cross(out).normalized()
        for k, t in enumerate((-0.094, -0.061, -0.028)):
            center = wrist+axis*t+out*(radius+0.0045)
            vs = [center+out*0.003]
            vs += [center+(axis*math.cos(a)+tangent*math.sin(a))*0.008 for a in angles[::4]]
            vs.append(center-out*0.001)
            fs = [(0, 1+i, 1+(i+1)%16) for i in range(16)]
            fs += [(17, 1+(i+1)%16, 1+i) for i in range(16)]
            piece(f"cuff_button_{side}_{k}", "brass", vs, fs, 0, under=(cuff,))
        report["cuffs"][side] = {"thickness_mm": 4.0, "sleeve_clearance_min_mm": 3.0,
                                  "inner_radius_mm": round(radius*1000, 2),
                                  "sleeve_overlap_mm": 20.0, "closure_seam_mm": 0.0,
                                  "buttons": 3}
    # The original shirt contains only the two wristbands and the stock.
    for ob in list(pieces):
        if keys[ob.name] in ("shirt", "stock"):
            pieces.remove(ob)
            keys.pop(ob.name)
            bpy.data.objects.remove(ob, do_unlink=True)
    pieces.extend(made)
    return report


def shoes(name, arm, body, skins=()):
    """Closed low leather shoes on a last, with rigid instep buckle frames."""
    import bpy
    import bmesh
    from mathutils import Vector

    from . import armature as am, body as bd, poses

    # Seat the rigid lasts in the unchanged STAND pose. Correct geometry in bind
    # space, rather than translating the root or modifying a drill/aim stance.
    saved = [(pb, pb.matrix_basis.copy()) for pb in arm.pose.bones]
    arm.data.pose_position = "POSE"
    am.pose(arm, poses.STAND_MUSKET)
    bpy.context.view_layer.update()
    stand = {s: arm.pose.bones[f"foot_{s}"].matrix @ arm.data.bones[f"foot_{s}"].matrix_local.inverted()
             for s in ("l", "r")}
    for pb, matrix in saved:
        pb.matrix_basis = matrix
    arm.data.pose_position = "REST"

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
        gap = min((stand[side] @ Vector(p)).z for p in verts[start:]) - 0.003
        shift = stand[side].inverted().to_3x3() @ Vector((0, 0, gap))
        for i in range(start, len(verts)):
            verts[i] = Vector(verts[i]) - shift
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
                v.co = Vector((cx + dx + v.co.x*sx, -0.145 + dy + v.co.y*sy, 0.067 + v.co.z*0.005))
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


def fit_gaiters(name, arm, body, pieces, keys):
    """Sew the draped ankle into a vamp over the shoe, with an outer placket and stirrup.

    This is a fitted extension of the independently settled gaiter, with shared
    vertices at the ankle. Its weights blend continuously into the rigid foot.
    """
    import bpy
    import bmesh
    from mathutils import Vector
    from . import armature, drape, hdbody

    gaiters = [o for o in pieces if keys[o.name] == "gaiters"]
    shoe_tree = _tree([o for o in pieces if keys[o.name] == "shoes"])
    report = {"instep_clearance_mm": [], "stirrup_sides": 2, "outer_seams": 2}
    for ob in gaiters:
        bm = bmesh.new()
        bm.from_mesh(ob.data)
        # Part borders are coincident duplicates for cutting; merge the ankle's
        # duplicates before extending it so the new vamp has shared smooth normals.
        ankle = [v for v in bm.verts if v.co.z < 0.28]
        bmesh.ops.remove_doubles(bm, verts=ankle, dist=1e-6)
        deform = bm.verts.layers.deform.verify()
        # A horizontal sewing line inside the ankle, with no doubled rim.
        bmesh.ops.bisect_plane(bm, geom=list(bm.verts)+list(bm.edges)+list(bm.faces),
                               plane_co=(0, 0, 0.22), plane_no=(0, 0, 1),
                               dist=1e-6, clear_inner=True)
        edges = [e for e in bm.edges if e.is_boundary and all(abs(v.co.z-0.22)<1e-5 for v in e.verts)]
        boundary = {v for e in edges for v in e.verts}
        rows = {v: [v] for v in boundary}
        for v in boundary:
            side = "l" if v.co.x > 0 else "r"
            cx = arm.data.bones[f"foot_{side}"].head_local.x
            a = math.atan2(v.co.x-cx, -v.co.y)
            y = -0.020 - (0.110 if math.cos(a)>0 else 0.090)*math.cos(a)
            # Read the actual shoe last, including its STAND seating correction.
            width = 0.044 if y > -0.055 else 0.050
            x = cx + width*0.92*math.sin(a)
            hit = shoe_tree.ray_cast(Vector((x, y, 0.30)), Vector((0, 0, -1)), 0.40)[0]
            if hit is None:
                hit = shoe_tree.find_nearest(Vector((x, y, 0.08)))[0]
            q = hit + Vector((0, 0, 0.003))
            report["instep_clearance_mm"].append(round((q-hit).length*1000, 3))
            weights = dict(v[deform])
            foot = ob.vertex_groups.get(f"foot_{side}") or ob.vertex_groups.new(name=f"foot_{side}")
            for t in (0.2, 0.4, 0.6, 0.8, 1.0):
                co = v.co.lerp(q, t*t)
                co.z = v.co.z*(1-t)+q.z*t
                p = bm.verts.new(co)
                share = t*t*(3-2*t)
                for group, w in weights.items():
                    p[deform][group] = w*(1-share)
                p[deform][foot.index] = p[deform].get(foot.index, 0)+share
                rows[v].append(p)
        for e in edges:
            loop = e.link_loops[0]
            a, b = loop.vert, loop.link_loop_next.vert
            for j in range(5):
                bm.faces.new((rows[b][j], rows[a][j], rows[a][j+1], rows[b][j+1]))
        bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
        bm.to_mesh(ob.data)
        bm.free()
        hdbody.limit_weights(ob)

    def detail(label, verts, faces, thickness=0, rigid_side=None):
        me = bpy.data.meshes.new(f"{name}.{label}")
        me.from_pydata(verts, [], faces)
        ob = bpy.data.objects.new(me.name, me)
        bpy.context.collection.objects.link(ob)
        if rigid_side:
            ob.vertex_groups.new(name=f"foot_{rigid_side}").add(list(range(len(verts))), 1, "REPLACE")
        else:
            for i, w in enumerate(drape.transfer_weights(ob, body, arm, under=gaiters)):
                for b, x in w.items():
                    (ob.vertex_groups.get(b) or ob.vertex_groups.new(name=b)).add([i], x, "REPLACE")
        if thickness:
            solid = ob.modifiers.new("closed leather overlap", "SOLIDIFY")
            solid.thickness, solid.offset = thickness, 1
            with bpy.context.temp_override(object=ob, active_object=ob):
                bpy.ops.object.modifier_apply(modifier=solid.name)
        armature.bind(ob, arm)
        hdbody.limit_weights(ob)
        ob["hd_trim"] = True
        pieces.append(ob)
        keys[ob.name] = "gaiters"

    tree = _tree(gaiters)
    for side, sx in (("l", 1), ("r", -1)):
        cx = arm.data.bones[f"foot_{side}"].head_local.x
        verts = []
        # Overlap seam on the outside; the existing pewter button row seats on it.
        for z in np.linspace(0.12, 0.49, 38):
            for y in (-0.007, 0.007):
                p = tree.ray_cast(Vector((cx+sx*0.25, y, z)), Vector((-sx, 0, 0)), 0.30)[0]
                if p is None:
                    p = tree.find_nearest(Vector((cx+sx*0.05, y, z)))[0]
                verts.append(p+Vector((sx*0.0015, 0, 0)))
        detail(f"gaiter_outer_seam_{side}", verts,
               [(2*j, 2*j+1, 2*j+3, 2*j+2) for j in range(37)], 0.0015)
        # A closed 18 mm leather stirrup: round both edges and under the sole.
        sole = shoe_tree.ray_cast(Vector((cx, -0.051, -0.10)), Vector((0, 0, 1)), 0.30)[0].z
        path = [(cx-0.045, 0.080), (cx-0.052, 0.045), (cx-0.052, sole+0.005),
                (cx-0.045, sole-0.001), (cx, sole-0.001), (cx+0.045, sole-0.001),
                (cx+0.052, sole+0.005), (cx+0.052, 0.045), (cx+0.045, 0.080)]
        verts = [(x, y, z) for x, z in path for y in (-0.060, -0.042)]
        detail(f"gaiter_stirrup_{side}", verts,
               [(2*j, 2*j+1, 2*j+3, 2*j+2) for j in range(len(path)-1)], 0.0015, side)
    ds = report["instep_clearance_mm"]
    report["instep_clearance_mm"] = [min(ds), max(ds)]
    return report

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
    from mathutils.kdtree import KDTree

    # Exact seam weights from the bodice, fading into the underlying breeches over
    # the upper 10 cm. A waist vertex must not suddenly switch to the thigh.
    coat_verts, coat_weights = [], []
    for source in pieces:
        if keys[source.name] != "coat" or source.name.endswith(LOOSE):
            continue
        names = {g.index: g.name for g in source.vertex_groups}
        for v in source.data.vertices:
            if v.co.z < 1.18:
                coat_verts.append(v.co.copy())
                coat_weights.append({names[e.group]: e.weight for e in v.groups})
    seam = KDTree(len(coat_verts))
    for i, p in enumerate(coat_verts):
        seam.insert(p, i)
    seam.balance()

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
    # A hanging panel clears the widest part of the breeches below its column.
    # Following each local thigh bump independently creates the old forward belly
    # half-way down the skirt. The support line is constant down the grain.
    azimuths = np.linspace(-math.pi, math.pi, 65)
    envelope = []
    for a in azimuths:
        radial = Vector((math.sin(a), -math.cos(a), 0))
        radii = []
        for z in np.linspace(0.60, 1.00, 13):
            hit = inner.ray_cast(radial*0.5+Vector((0, 0, z)), -radial, 0.5)[0]
            if hit is not None:
                radii.append(hit.dot(radial))
        envelope.append(max(radii, default=0.16)+0.025)
    for ob in pieces:
        if not ob.name.endswith(LOOSE):
            continue
        # The split garment's generic hem also rimmed its sewn waist. Remove that
        # internal rim, leaving the actual shared sewing line continuous.
        import bmesh

        bm = bmesh.new()
        bm.from_mesh(ob.data)
        drop = bm.verts.layers.float.get("hd_skirt_drop")
        if drop:
            bmesh.ops.delete(bm, geom=[f for f in bm.faces if all(v[drop] < 0.006 for v in f.verts)], context="FACES")
            bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(ob.data)
        bm.free()
        drops = ob.data.attributes.get("hd_skirt_drop")
        ws = []
        for v in ob.data.vertices:
            drop = drops.data[v.index].value if drops else max(0, 1.05-v.co.z)
            blend = min(1.0, drop/0.10)
            blend = blend*blend*(3-2*blend)
            _p, si, _d = seam.find(v.co)
            a = math.atan2(v.co.x, -v.co.y)
            r = math.hypot(v.co.x, v.co.y)
            hanging = float(np.interp(a, azimuths, envelope))+0.01*drop
            shape_blend = min(1.0, drop/0.075)
            shape_blend = shape_blend*shape_blend*(3-2*shape_blend)
            if r > 1e-6:
                v.co.x *= 1+shape_blend*(hanging/r-1)
                v.co.y *= 1+shape_blend*(hanging/r-1)
            side = 1 if v.co.x > 0 else -1
            leg = max(0.0, min(1.0, (0.97 - v.co.z) / 0.12))
            outward = Vector((v.co.x - side * 0.09 * leg, v.co.y, 0)).normalized()
            loc, _n, ti, distance = inner.ray_cast(v.co + outward * 0.06, -outward, 0.14)
            if loc is None:
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
            # Remap leg weights only after blending the common waist influence.
            w = {b: x*blend for b, x in w.items()}
            for b, x in coat_weights[si].items():
                w[b] = w.get(b, 0)+x*(1-blend)
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


def _skin_openings(sk, covered, arm):
    """Keep skin across the collar/wrist openings, including their hidden overlap.

    Cutting at individual cloth-ray hits makes triangular holes above a moving hem.
    The short intact overlap stays inside the closed trim and follows its weights.
    """
    wrists = [arm.data.bones[f"hand_{s}"].head_local for s in ("l", "r")]
    return [c and not (v.co.z > 1.44 and abs(v.co.x) < 0.14) and
            not any((v.co-w).length < 0.07 for w in wrists)
            for c, v in zip(covered, sk.data.vertices)]


def torso_weights(z):
    """One continuous trunk field shared by fitted coat fronts and their underlap."""
    knots = ((1.04, "hips"), (1.20, "spine"), (1.34, "spine1"), (1.48, "spine2"))
    if z <= knots[0][0]:
        return {knots[0][1]: 1.0}
    for (lo, a), (hi, b) in zip(knots, knots[1:]):
        if z < hi:
            t = (z-lo)/(hi-lo)
            return {a: 1-t, b: t}
    return {knots[-1][1]: 1.0}


def front_weights(ob, underlap=False):
    """Keep the bodice off arm influences; blend into sleeve weights at the armhole."""
    names = {g.index: g.name for g in ob.vertex_groups}
    weights = []
    for v in ob.data.vertices:
        w = {names[e.group]: e.weight for e in v.groups}
        t = 1.0 if underlap else max(0.0, min(1.0, (0.28-abs(v.co.x))/0.08, (0.04-v.co.y)/0.04))
        if not underlap and not 0.98 < v.co.z < 1.51:
            t = 0.0
        result = {b: x*(1-t) for b, x in w.items()}
        for b, x in torso_weights(v.co.z).items():
            result[b] = result.get(b, 0)+x*t
        weights.append(result)
    for g in list(ob.vertex_groups):
        ob.vertex_groups.remove(g)
    for i, w in enumerate(weights):
        for b, x in w.items():
            if x > 1e-6:
                (ob.vertex_groups.get(b) or ob.vertex_groups.new(name=b)).add([i], x, "REPLACE")

def compose(body_tree, objs, keys, rank, skins=(), arm=None):
    """The visible-layers-only outfit: deletes covered inner faces (and the skin under the
    clothes, `skins`), pushes what stays under the layer on it. `body_tree`: BVH of the
    whole rest skin (the outward normals); `rank(key)` -> layer rank. Returns stats."""
    import bpy

    st = {"culled_faces": {}, "moved_verts": {}, "pokes": {}}
    order = sorted(objs, key=lambda o: -rank(keys[o.name]))
    hiding = [o for o in objs if not o.name.endswith(LOOSE)]
    for o in order:
        r = rank(keys[o.name])
        if o.get("hd_trim") or keys[o.name] in ("shoes", "brass", "coat", "facings", "lapels") or o.name.endswith(LOOSE):
            continue
        outer = [x for x in hiding if rank(keys[x.name]) > r and x is not o]
        if keys[o.name] in ("breeches", "waistcoat"):
            # These panels now have the same deformation as the breeches. Covered
            # breeches stay covered during the clips and can be removed safely.
            outer += [x for x in objs if x.name.endswith(LOOSE)]
        if not outer:
            continue
        tree = _tree(outer)
        if keys[o.name] == "waistcoat":
            # Only the open coat front exposes the waistcoat. Its back and
            # shoulders are covered in every drill, even when a ray misses a hem.
            _delete_faces(o, [v.co.y < -0.015 and abs(v.co.x) < 0.14 and 0.98 <= v.co.z < 1.50
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
        if keys[o.name] == "waistcoat":
            hidden = {f for f in bm.faces if all(cov[v.index] for v in f.verts)}
            # Two face rings of hidden overlap prevent an oblique view seeing
            # past a cut edge into the gap between fitted layers.
            for _ in range(2):
                hidden = {f for f in hidden if all(g in hidden for e in f.edges for g in e.link_faces)}
            gone = list(hidden)
        else:
            gone = [f for f in bm.faces if any(cov[v.index] for v in f.verts)]
        st["culled_faces"][o.name] = len(gone)
        bmesh.ops.delete(bm, geom=gone, context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(o.data)
        bm.free()
        tree = _tree(outer)
        moved, pokes = (0, 0) if keys[o.name] == "stock" else _clamp(
            o, tree, body_tree, offset=0.015 if keys[o.name] == "waistcoat" else OFFSET)
        if keys[o.name] == "waistcoat":
            front_weights(o, underlap=True)
        st["moved_verts"][o.name], st["pokes"][o.name] = moved, pokes
    # Loose panels were already laid over the breeches with their fixed clearance.
    # Repeated nearest-point pushes here buckle the waist onto unrelated body faces.
    tree = _tree(hiding)
    for sk in skins:
        sk.data.update()
        cov = _covered(sk, tree, body_tree, own_normals=True)
        if arm is not None:
            cov = _skin_openings(sk, cov, arm)
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
        if o.get("hd_trim"):
            # White linen over the blue collar, independent of the historical black
            # stock palette used by the ordinary line-unit draft.
            trimkey = f"{key}.trim"
            if trimkey not in mats:
                rgb = palette.get("shirt", (0.90, 0.87, 0.78)) if key in ("stock", "fallband") else palette.get(key, (0.5, 0.5, 0.5))
                m = weave_material(f"{prefix}.{key}.trim", rgb, fabric)
                m["acr_decay"] = decay.extras(layer)
                mats[trimkey] = m
            o.data.materials.clear()
            o.data.materials.append(mats[trimkey])
            if not o.data.uv_layers:
                _uv(o)
            gore(o, layer)
            continue
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


def stitch_waist(ob):
    """Join the sewn bodice/skirt vertices, preserving the torso's gore part.

    Keeping coincident pieces separate leaves a hard normal line even after the
    internal hem is removed. Only the coat's torso seam is welded here.
    """
    import bmesh
    from mathutils import Vector

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    part = bm.verts.layers.float.get("_part")
    candidates = [v for v in bm.verts if 0.97 < v.co.z < 1.15 and
                  any(".coat." in ob.data.materials[f.material_index].name for f in v.link_faces) and
                  (part is None or v[part] == 0)]
    before = len(bm.verts)
    bmesh.ops.remove_doubles(bm, verts=candidates, dist=0.0003)
    joined = before-len(bm.verts)
    # Fair the tangent across the sewing line too. The separate simulations leave
    # a tiny shelf at the waist even when the endpoints agree exactly.
    band = [v for v in bm.verts if 0.96 < v.co.z < 1.17 and
            any(".coat." in ob.data.materials[f.material_index].name for f in v.link_faces) and
            (part is None or v[part] == 0)]
    for _ in range(24):
        moved = {}
        for v in band:
            nbr = [e.other_vert(v) for e in v.link_edges]
            if nbr:
                avg = sum((q.co for q in nbr), Vector())/len(nbr)
                strength = min(1.0, (v.co.z-0.96)/0.05, (1.17-v.co.z)/0.05)
                q = v.co.lerp(avg, 0.45*strength)
                q.z = v.co.z
                moved[v] = q
        for v, q in moved.items():
            v.co = q
    bm.to_mesh(ob.data)
    bm.free()
    ob.data.update()
    return joined


def stitch_front_inlays(ob):
    """Smooth shared wool/inlay seams while retaining integer torso part IDs."""
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    part = bm.verts.layers.float.get("_part")
    vs = [v for v in bm.verts if 1.03 < v.co.z < 1.49 and abs(v.co.x)<0.15 and
          (part is None or v[part] == 0) and any(
              any(k in ob.data.materials[f.material_index].name for k in (".coat.", ".facings.")) for f in v.link_faces)]
    before = len(bm.verts)
    bmesh.ops.remove_doubles(bm, verts=vs, dist=1e-5)
    count = before-len(bm.verts)
    bm.to_mesh(ob.data)
    bm.free()
    return count


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
    strip_inlay_rims(pieces, keys)
    fit_stock(pieces, keys, body_tree)
    lay_skirts(pieces, keys)
    for piece in pieces:
        if keys[piece.name] in ("coat", "facings"):
            front_weights(piece)
    for sh, key in shoes(ctx.name, arm, body, body_lods):
        pieces.append(sh)
        keys[sh.name] = key
    st["footwear"] = fit_gaiters(ctx.name, arm, body, pieces, keys)
    st["drape_s"] = round(time.time() - t0, 1)

    # Compose/cull against the actual new trim, not the discarded simulated bands.
    # Otherwise their old jagged silhouettes leave holes in the exposed neck/wrists.
    st["collar_cuffs"] = tailored_trim(ctx.name, arm, body, pieces, keys, body_tree)

    def rank(k):
        return {"coat_skirts": 5, "turnbacks": 6, "shoes": 1.5}.get(k, gm.LAYER_RANK.get(k, 5))

    st["source_tris"] = {o.name: _tris(o) for o in pieces}
    t1 = time.time()
    st["compose"] = compose(body_tree, pieces, keys, rank, skins=(body,), arm=arm)
    st["hidden_skin_faces"] = hide_tailored_skin((body, *body_lods), arm)
    clothing_tree = _tree(pieces)
    for skin in body_lods:
        covered = _covered(skin, clothing_tree, body_tree, own_normals=True)
        covered = _skin_openings(skin, covered, arm)
        st["compose"]["culled_faces"][skin.name] = _delete_faces(skin, [not c for c in covered])
    st["compose_s"] = round(time.time() - t1, 1)
    ch = _Ch(ctx.name, arm, body, ())
    for o in pieces:
        ch.add("clothes", o, keys[o.name])
    st["buttons"] = [o.name for o in drape.buttons(ch, [o for o in pieces if not o.get("hd_trim")], f"{ctx.name}.hd")]
    for o in ch.layers["clothes"]:
        if o not in pieces:
            pieces.append(o)
            keys[o.name] = ch.keys[o.name]
    mats = {}
    finish_pieces(pieces, keys, palette, mats, ctx.name)
    outfit, st["weights"] = join(f"{ctx.name}.hd.outfit", pieces, arm)
    st["waist_vertices_joined"] = stitch_waist(outfit)
    st["inlay_vertices_joined"] = stitch_front_inlays(outfit)
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
