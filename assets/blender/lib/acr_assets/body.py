"""Layered bodies on a rig (needs `bpy`, `bmesh`): skin hull, body-derived clothing shells,
straps, a skirt on its own bones, the bone layer, cuts with stump caps and morph targets.

Layers, innermost first (#178): bones (rigid anatomical primitives) < flesh (a Skin-modifier
hull around the rest skeleton, smooth weights) < clothes (shells derived from the flesh:
the same vertices pushed out along the normals with the same weights, so they deform
exactly like the skin) < gear (rigid meshes on deforming bones, or objects on sockets that
drop at death). Cuts (#183): the flesh is split at the joints of `rig.JOINTS` before the
shells are derived, so every layer has the same seams; `_part` tells the renderer which
part a vertex belongs to and `caps()` makes the hidden stump caps. See docs/assets-humans.md.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.geometry import intersect_point_line

from . import armature as am
from . import rig


def link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


def seg_point(p, a, b):
    """Nearest point to p on segment a-b."""
    _q, t = intersect_point_line(p, a, b)
    return a + (b - a) * min(max(t, 0.0), 1.0)


def _children(arm_obj):
    ch = {}
    for b in arm_obj.data.bones:
        if b.parent:
            ch.setdefault(b.parent.name, []).append(b.name)
    return ch


def skin_weights(obj, arm_obj, bones, falloff=4.0):
    """Smooth weights: each vertex belongs to its nearest bone segment (home) and blends with
    home's parent and children by inverse distance^falloff. Stores the home's index in
    `bones` as the INT attribute `home`; returns the list of homes."""
    seg = am.segments(arm_obj)
    ch = _children(arm_obj)
    parent = {b.name: (b.parent.name if b.parent else None) for b in arm_obj.data.bones}
    bones = list(bones)
    homes = []
    for v in obj.data.vertices:
        p = v.co
        d = {b: (p - seg_point(p, *seg[b])).length for b in bones}
        home = min(d, key=d.get)
        homes.append(home)
        cand = [home] + [c for c in ([parent[home]] + ch.get(home, [])) if c in bones]
        w = {c: 1.0 / (d[c] + 0.01) ** falloff for c in cand}
        tot = sum(w.values())
        for c, x in w.items():
            x /= tot
            if x < 0.04:
                continue
            g = obj.vertex_groups.get(c) or obj.vertex_groups.new(name=c)
            g.add([v.index], x, "REPLACE")
    attr = obj.data.attributes.get("home") or obj.data.attributes.new("home", "INT", "POINT")
    for i, h in enumerate(homes):
        attr.data[i].value = bones.index(h)
    obj["acr_home_bones"] = bones
    return homes


def rigid_weights(obj, bone_of_vertex):
    for vi, b in bone_of_vertex.items():
        g = obj.vertex_groups.get(b) or obj.vertex_groups.new(name=b)
        g.add([vi], 1.0, "REPLACE")


def set_material(obj, mat):
    obj.data.materials.clear()
    obj.data.materials.append(mat)


def homes(obj):
    bones = list(obj["acr_home_bones"])
    return [bones[d.value] for d in obj.data.attributes["home"].data]


# --------------------------------------------------------------------------- hull

def hull(arm_obj, name, graph, bones=rig.BODY_BONES, subdivide=1, head=None):
    """Continuous low-poly skin around a rest skeleton: `graph` = [(node, parent, position,
    (radius x, radius y))]; Skin modifier + subdivision, applied; smooth weights on `bones`.
    `head(bm)` may refine and shape the head before the weights (`heads.refine`, #283).
    The rest normals are kept in the attribute `rest_normal` (shells use them, so seams
    split later stay closed)."""
    idx = {n: i for i, (n, *_r) in enumerate(graph)}
    me = bpy.data.meshes.new(name + ".graph")
    me.from_pydata([p for _n, _p, p, _r in graph], [(idx[par], idx[n]) for n, par, _p, _r in graph if par], [])
    tmp = link(bpy.data.objects.new(name + ".graph", me))
    sk = tmp.modifiers.new("skin", "SKIN")
    sk.use_smooth_shade = True
    sk.branch_smoothing = 0.3
    for i, (_n, _par, _p, r) in enumerate(graph):
        me.skin_vertices[0].data[i].radius = r
    me.skin_vertices[0].data[0].use_root = True
    if subdivide:
        sub = tmp.modifiers.new("sub", "SUBSURF")
        sub.levels = subdivide
        sub.render_levels = subdivide
    dg = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(tmp.evaluated_get(dg))
    mesh.name = name
    bpy.data.objects.remove(tmp)
    bpy.data.meshes.remove(me)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    if head:
        head(bm)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    obj = link(bpy.data.objects.new(name, mesh))
    for poly in mesh.polygons:
        poly.use_smooth = True
    hs = skin_weights(obj, arm_obj, bones)
    seg = am.segments(arm_obj)
    rn = mesh.attributes.new("rest_normal", "FLOAT_VECTOR", "POINT")
    if HULL_WINDING and _closed_outward(mesh):
        # A closed hull: the winding (made outward by volume) gives the outward normal; the
        # bone-side flip misfires at the armpits, hands and crotch (#287 trisoup7).
        mesh.update()
        for v in mesh.vertices:
            rn.data[v.index].vector = v.normal.copy()
    else:
        # Outward rest normals: the vertex normal, flipped where it points at its own bone.
        for v in mesh.vertices:
            n = v.normal.copy()
            if n.dot(v.co - seg_point(v.co, *seg[hs[v.index]])) < 0:
                n.negate()
            rn.data[v.index].vector = n
    return am.bind(obj, arm_obj)


HULL_WINDING = False  # closed hull: rest normals from the outward winding, no bone-side flip


def _closed_outward(mesh, holes=0.03):
    """True when `mesh` is closed but for small holes (at most `holes` of its edges on one
    face) and consistently wound; each connected piece is then turned so that the volume it
    encloses (about its centre) is positive: an outward winding."""
    bm = bmesh.new()
    bm.from_mesh(mesh)
    open_ = sum(len(e.link_faces) == 1 for e in bm.edges)
    if not bm.faces or any(len(e.link_faces) > 2 for e in bm.edges) or open_ > holes * len(bm.edges):
        bm.free()
        return False
    for e in bm.edges:
        if len(e.link_faces) == 2:
            l1, l2 = (next(lp for lp in f.loops if lp.edge is e) for f in e.link_faces)
            if l1.vert is l2.vert:  # both faces run the edge the same way
                bm.free()
                return False
    seen = set()
    turned = False
    for f0 in bm.faces:
        if f0.index in seen:
            continue
        piece, stack = [], [f0]
        seen.add(f0.index)
        while stack:
            f = stack.pop()
            piece.append(f)
            for e in f.edges:
                for g in e.link_faces:
                    if g.index not in seen:
                        seen.add(g.index)
                        stack.append(g)
        c = sum((v.co for f in piece for v in f.verts), Vector()) / sum(len(f.verts) for f in piece)
        vol = 0.0
        for f in piece:
            vs = [v.co - c for v in f.verts]
            for i in range(1, len(vs) - 1):
                vol += vs[0].dot(vs[i].cross(vs[i + 1]))
        if vol < 0:
            turned = True
            for f in piece:
                f.normal_flip()
    if turned:
        bm.to_mesh(mesh)
    bm.free()
    return True


# --------------------------------------------------------------------------- shoulder line

SHOULDER_REMESH = True  # relax the decimated MakeHuman shoulder line into even triangles (#287 trisoup7)
SLIVER = 4.0  # aspect ratio (longest edge / its height) above which a triangle is a sliver


def _aspect(f):
    a = f.calc_area()
    if a < 1e-12:
        return 1e9
    return max(e.calc_length() for e in f.edges) ** 2 / (2.0 * a)


def _opposite(e, f):
    return next(v for v in f.verts if v not in e.verts)


def _flip_wanted(e):
    """Delaunay test on the two triangles of `e`, guarded against folds: flip when the
    angles opposite `e` sum to more than 180 degrees and the new edge is shorter."""
    f1, f2 = e.link_faces
    c, d = _opposite(e, f1), _opposite(e, f2)
    a, b = e.verts
    if c is d or any(d in w.verts for w in c.link_edges):
        return False
    ang = (a.co - c.co).angle(b.co - c.co, 0.0) + (a.co - d.co).angle(b.co - d.co, 0.0)
    if ang <= math.pi + 1e-3:
        return False
    if f1.normal.dot(f2.normal) < 0.5:  # a crease (armpit, collar bone): keep it
        return False
    # Both new triangles must keep the orientation of the old ones.
    n = (f1.normal + f2.normal).normalized()
    for p, q, r in ((c.co, d.co, a.co), (d.co, c.co, b.co)):
        if (q - p).cross(r - p).dot(n) == 0:
            return False
    s1 = (d.co - c.co).cross(a.co - c.co).dot(n)
    s2 = (b.co - c.co).cross(d.co - c.co).dot(n)
    return (s1 > 0) == (s2 > 0) and abs(s1) > 1e-9 and abs(s2) > 1e-9


def remesh_shoulders(bm, seg, prefix="", iterations=4):
    """Re-meshes the shoulder line of the decimated MakeHuman body (in `bm`, before the
    weights): the faces above the armpits and below the neck seam, from the neck out over the
    acromion, are triangulated, their long edges split, Delaunay edge flips remove the slivers
    and the vertices are relaxed tangentially and put back on the original surface. Torso and
    upper-arm triangles then no longer interleave in a fan across the trapezius (the shell
    tops cut through it in thin shards, docs/clothing.md "Sixth trisoup pass")."""
    P = prefix
    sh = seg[P + "upperarm_l"][0]
    neck = seg[P + "neck"][0]
    cx = seg[P + "hips"][0][0]
    lo, hi = sh[2] - 0.12, neck[2] + 0.08
    reach = abs(sh[0] - cx) + 0.14

    def inside(co):
        return lo < co.z < hi and abs(co.x - cx) < reach

    tree = BVHTree.FromBMesh(bm)
    faces = [f for f in bm.faces if all(inside(v.co) for v in f.verts)]
    if not faces:
        return {}
    before = sum(_aspect(t) > SLIVER for t in faces if len(t.verts) == 3)
    bmesh.ops.triangulate(bm, faces=faces)

    def region():
        return [f for f in bm.faces if all(inside(v.co) for v in f.verts)]

    lens = sorted(e.calc_length() for f in region() for e in f.edges)
    target = lens[len(lens) // 2]
    splits = flips = 0
    for _ in range(iterations):
        fs = set(region())
        # Split the long edges (both faces in the region, so the border stays put).
        long = [e for e in {e for f in fs for e in f.edges}
                if e.calc_length() > 1.6 * target and len(e.link_faces) == 2 and all(g in fs for g in e.link_faces)]
        if long:
            splits += len(long)
            bmesh.ops.subdivide_edges(bm, edges=long, cuts=1, use_grid_fill=False)
            bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 3])
        for _f in range(6):
            fs = set(region())
            changed = 0
            for e in [e for f in fs for e in f.edges]:
                if not e.is_valid or len(e.link_faces) != 2 or not all(g in fs and len(g.verts) == 3 for g in e.link_faces):
                    continue
                for g in e.link_faces:
                    g.normal_update()
                if _flip_wanted(e):
                    ne = bmesh.utils.edge_rotate(e, True)
                    if ne is not None:
                        changed += 1
                        for g in ne.link_faces:
                            fs.add(g)
                            g.normal_update()
            flips += changed
            if not changed:
                break
        # Tangential relaxation, back onto the original surface.
        fs = set(region())
        free = {v for f in fs for v in f.verts if not v.is_boundary and all(g in fs for g in v.link_faces)}
        for _r in range(2):
            new = {}
            for v in free:
                ns = [e.other_vert(v).co for e in v.link_edges]
                c = sum(ns, Vector()) / len(ns)
                v.normal_update()
                d = c - v.co
                d -= v.normal * d.dot(v.normal)
                new[v] = v.co + d * 0.5
            for v, c in new.items():
                loc, _n, _i, _d = tree.find_nearest(c)
                v.co = loc if loc is not None else c
    # The points the decimation left standing up over the acromion and trapezius: a light
    # full smoothing of the free vertices (a few mm off the convex shoulder).
    fs = set(region())
    free = [v for f in fs for v in f.verts if not v.is_boundary and all(g in fs for g in v.link_faces)]
    for _ in range(2):
        bmesh.ops.smooth_vert(bm, verts=list(set(free)), factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    bm.normal_update()
    fs = region()
    after = sum(_aspect(t) > SLIVER for t in fs)
    return {"slivers": (before, after), "faces": len(fs), "edge": round(target, 3), "splits": splits,
            "flips": flips, "zone": (round(lo, 3), round(hi, 3), round(reach, 3))}


def split_parts(obj, part_of_bone=rig.part_of_bone):
    """Splits the mesh along the borders of the body parts (each face goes to the part of
    the majority of its vertices' homes) and writes the INT attribute `part`."""
    hs = homes(obj)
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    fpart = bm.faces.layers.int.new("fpart")
    for f in bm.faces:
        votes = {}
        for v in f.verts:
            p = part_of_bone(hs[v.index])
            votes[p] = votes.get(p, 0) + 1
        f[fpart] = max(votes, key=lambda k: (votes[k], -k))
    seams = [e for e in bm.edges if len(e.link_faces) == 2 and e.link_faces[0][fpart] != e.link_faces[1][fpart]]
    bmesh.ops.split_edges(bm, edges=seams)
    vpart = bm.verts.layers.int.get("part") or bm.verts.layers.int.new("part")
    for f in bm.faces:
        for v in f.verts:
            v[vpart] = f[fpart]
    bm.faces.layers.int.remove(fpart)
    bm.to_mesh(me)
    bm.free()
    me.update()
    return obj


def parts(obj):
    a = obj.data.attributes.get("part")
    return [d.value for d in a.data] if a else [0] * len(obj.data.vertices)


def derive(src, name, keep_face, offset, border=False, field=None):
    """A clothing shell: the faces of `src` for which keep_face(centre, normal, home, part)
    holds, pushed out by `offset` metres along the rest normals; same weights, homes and
    parts. With `border` (and `CONTOUR`) the shell is cut along an isoline through the
    triangles of a smoothed selection field (`contour_cut`), pushed out along its own
    normals and given a turned hem (`hem_border`); without `CONTOUR` whole faces are cut
    and the border is smoothed (`smooth_border`). `field(co)` (0..1, or None) caps the
    selection field per vertex (the neck opening, `neck_opening`)."""
    obj = src.copy()
    obj.data = src.data.copy()
    obj.data.name = name
    obj.name = name
    obj.modifiers.clear()
    link(obj)
    hs = homes(src)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    rn = bm.verts.layers.float_vector.get("rest_normal")
    vpart = bm.verts.layers.int.get("part")
    kill = []
    for f in bm.faces:
        votes = {}
        for v in f.verts:
            votes[hs[v.index]] = votes.get(hs[v.index], 0) + 1
        home = max(votes, key=votes.get)
        part = f.verts[0][vpart] if vpart else 0
        if not keep_face(f.calc_center_median(), f.normal, home, part):
            kill.append(f)
    if border and CONTOUR:
        # A narrow selection (a diagonal strap, a strip of quills) can fade below the isoline
        # when smoothed: then smooth less (at 0 passes the field is still cut through the
        # triangles, only less round).
        dead = {f.index for f in kill}
        want = sum(f.calc_area() for f in bm.faces if f.index not in dead)
        for passes in (CONTOUR_PASSES, 1, 0):
            cut = bm.copy()
            cut.faces.index_update()
            contour_cut(cut, dead, passes, field)
            if sum(f.calc_area() for f in cut.faces) >= 0.7 * want or passes == 0:
                break
            cut.free()
        bm.free()
        bm = cut
        rn = bm.verts.layers.float_vector.get("rest_normal")
        for v in bm.verts:
            v.co += Vector(v[rn]) * offset
        relax_border(bm)
        hem_border(bm, offset)
    else:
        # Push before deleting (keyed by BMVert; delete() renumbers the indices).
        seam = {frozenset(e.verts) for e in bm.edges if e.is_boundary}
        for v in bm.verts:
            v.co += Vector(v[rn]) * offset
        bmesh.ops.delete(bm, geom=kill, context="FACES")
        if border:
            smooth_border(bm, src, seam, offset)
    bm.to_mesh(obj.data)
    bm.free()
    for a in list(obj.data.color_attributes):
        obj.data.color_attributes.remove(a)
    obj["acr_home_bones"] = list(src["acr_home_bones"])
    return am.bind(obj, src.parent)


# The neck opening of the trunk shells (#287 trisoup7, docs/clothing.md "Seventh trisoup pass").
NECK_CUT = True  # cut the neck opening by distance from the neck, not by part and height
NECK_EASE = 0.015  # m: default opening, this far outside the skin of the neck
NECK_ASPECT = 1.1  # front-back distances count this much more (a wider-than-deep ellipse)
NECK_RAMP = 0.05  # m: half-width of the distance ramp (wider than the triangles: a smooth isoline)
NECK_DEPTH = 0.12  # m below the neck bone's head where the opening's zone ends
NECK_TOP = 0.04  # m above the neck bone's head: higher faces are not forced into the shell
NECK_DROP = 2.5  # below the neck's root, distances along the neck's axis count this much more
TRUNK = ("spine1", "spine2", "clavicle_l", "clavicle_r", "neck")


class NeckField:
    """Selection cap round the neck: 0.5 at `radius` from the neck's axis (the bone moved
    onto the centre of the neck skin by `shift`; front-back distances scaled by
    `NECK_ASPECT`), ramping to 1 outward; None below the zone. Below the neck's root the
    distance along the axis counts `NECK_DROP` times: a flat ellipsoid, so the opening does
    not run down the back where the spine lies close under the skin (round 7's V)."""

    def __init__(self, axis_a, axis_b, radius, low, shift=None):
        shift = Vector() if shift is None else shift
        self.a, self.b = axis_a + shift, axis_b + shift
        self.radius, self.low = radius, low
        self.u = (self.b - self.a).normalized()

    def offset(self, co):
        # From the neck's axis, open-ended upwards: a tube round the neck above its root,
        # a flattened cap below it.
        return co - seg_point(co, self.a, self.b + (self.b - self.a) * 4.0)

    def dist(self, co):
        d = self.offset(co)
        ax = d.dot(self.u)
        p = d - self.u * ax
        return math.sqrt(p.x * p.x + (p.y * NECK_ASPECT) ** 2 + p.z * p.z + (ax * NECK_DROP) ** 2)

    def __call__(self, co):
        if co.z < self.low:
            return None
        return min(1.0, max(0.0, 0.5 + (self.dist(co) - self.radius) / (2.0 * NECK_RAMP)))


def neck_opening(flesh, arm_obj, keep, prefix="", ease=None):
    """For a shell that covers the top of the trunk (at least half of the trunk faces in the
    zone below the neck kept by `keep`) and not the head (a hood): (keep', field) where keep' keeps every trunk and
    neck face of the zone (the trapezius whole) and `field` cuts the opening at `ease` metres
    outside the neck skin. None for other shells."""
    P = prefix
    seg = am.segments(arm_obj)
    a, b = seg[P + "neck"]
    low = a.z - NECK_DEPTH
    hs = homes(flesh)
    me = flesh.data
    probe = NeckField(a, b, 0.0, low)
    zone, kept, head, hooded = [], 0, 0, 0
    for p in me.polygons:
        c = p.center
        votes = {}
        for i in p.vertices:
            votes[hs[i]] = votes.get(hs[i], 0) + 1
        h = max(votes, key=votes.get)
        if c.z > b.z and h[len(P):] == "head":
            head += 1
            hooded += bool(keep(c, p.normal, h, 0))
        if c.z < low or c.z > b.z:
            continue
        if h[len(P):] not in TRUNK:
            continue
        zone.append(p.index)
        if h[len(P):] != "neck" and keep(c, p.normal, h, 0):
            kept += 1
    trunk = [i for i in zone if hs[me.polygons[i].vertices[0]][len(P):] != "neck"]
    if not trunk or kept < 0.5 * len(trunk) or hooded > 0.1 * max(head, 1):
        return None  # not on the top of the trunk, or over the head too (a hood)
    # The skin of the neck: its distance from the axis midway up the neck bone.
    zs = (a.z + 0.2 * (b.z - a.z), a.z + 0.5 * (b.z - a.z))
    ring = [v.co for v in me.vertices if zs[0] < v.co.z < zs[1]]
    # The bone lies behind the middle of the neck (the vertebrae): centre the axis on the
    # skin round it, else the opening reaches further down the back than the front.
    shift = Vector()
    if ring:
        offs = [probe.offset(co) for co in ring]
        shift = Vector((sorted(o.x for o in offs)[len(offs) // 2], sorted(o.y for o in offs)[len(offs) // 2], 0.0))
        if shift.length > 0.04:
            shift *= 0.04 / shift.length
    probe = NeckField(a, b, 0.0, low, shift)
    dists = sorted(probe.dist(co) for co in ring)
    r = dists[len(dists) // 2] if dists else 0.06
    field = NeckField(a, b, r + (NECK_EASE if ease is None else ease), low, shift)
    top = a.z + NECK_TOP  # the neck's faces above this (the chin) stay with the head

    def keep2(c, n, h, p):
        if low <= c.z <= top and h[len(P):] in TRUNK:
            return True
        return keep(c, n, h, p)

    return keep2, field


# Shells cut along a contour (#287 trisoup5, docs/clothing.md "Fifth trisoup pass").
CONTOUR = True  # cut along an isoline of the smoothed face selection, not along whole faces
CONTOUR_PASSES = 3  # smoothing passes of the selection field (welded over the hull's part seams)
CONTOUR_SNAP = 0.1  # field values this close to the isoline snap onto it (no sliver triangles)
STRIP_HEIGHT = 0.16  # m: a selection no taller than this is a strip (sash, belt, garter): cut level
BORDER_MOVE = 0.015  # m: the most a border vertex moves when the border is relaxed along its curve
HEM = 0.7  # the hem folds this share of the offset back towards the skin


def _pkey(co):
    return (round(co.x, 5), round(co.y, 5), round(co.z, 5))


def selection_field(bm, kill, passes=CONTOUR_PASSES, field=None):
    """{BMVert: 0..1}: the share of kept faces round each vertex (area weighted, welded over
    the part seams of the hull by position; `kill` holds the indices of the cut faces),
    smoothed `passes` times over the mesh. Its 0.5
    isoline follows the face selection without the teeth. A strip (kept faces within
    `STRIP_HEIGHT` of height: sashes, belts, garters) is cut level instead: there the field
    is a ramp in z through the band's top and bottom (still bounded sideways by the smoothed
    selection), so both edges are level lines through the triangles, a clean ribbon."""
    group = {}
    for v in bm.verts:
        group.setdefault(_pkey(v.co), []).append(v)
    gid = {v: k for k, vs in group.items() for v in vs}
    acc = {k: [0.0, 0.0] for k in group}
    for f in bm.faces:
        a = f.calc_area()
        s = 0.0 if f.index in kill else 1.0
        for v in f.verts:
            acc[gid[v]][0] += a * s
            acc[gid[v]][1] += a
    val = {k: (s / w if w > 0 else 0.0) for k, (s, w) in acc.items()}
    nbr = {k: set() for k in group}
    for e in bm.edges:
        a, b = gid[e.verts[0]], gid[e.verts[1]]
        if a != b:
            nbr[a].add(b)
            nbr[b].add(a)
    for _ in range(passes):
        val = {k: 0.5 * x + 0.5 * (sum(val[j] for j in nbr[k]) / len(nbr[k]) if nbr[k] else x)
               for k, x in val.items()}
    kept = [f for f in bm.faces if f.index not in kill]
    if kept:
        zs = [f.calc_center_median().z for f in kept]
        lo, hi = min(zs), max(zs)
        if hi - lo < STRIP_HEIGHT:
            edge = sum(e.calc_length() for f in kept for e in f.edges) / (3 * len(kept))
            lo, hi = lo - 0.2 * edge, hi + 0.2 * edge
            mid, half, ramp = (lo + hi) / 2, (hi - lo) / 2, max(0.25 * edge, 0.005)
            for k, x in val.items():
                tent = min(1.0, max(0.0, 0.5 + (half - abs(k[2] - mid)) / (2 * ramp)))
                val[k] = min(tent, min(1.0, 2.0 * x))
    if field is not None:
        for k in val:
            cap = field(Vector(k))
            if cap is not None:
                val[k] = min(val[k], cap)
    return {v: val[gid[v]] for v in bm.verts}


def contour_cut(bm, kill, passes=CONTOUR_PASSES, field=None):
    """Cuts the shell along the 0.5 isoline of `selection_field`: each edge it crosses is
    split where it crosses (weights and rest normals interpolated, home and part from the
    nearer end), the split points are joined through each triangle and the outer side is
    deleted. Twin edges on a part seam of the hull are split alike, so the seams stay closed."""
    fld = selection_field(bm, kill, passes, field)
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    for v, x in fld.items():
        if abs(x - 0.5) < CONTOUR_SNAP:
            fld[v] = 0.5
    home = bm.verts.layers.int.get("home")
    vpart = bm.verts.layers.int.get("part")
    rn = bm.verts.layers.float_vector.get("rest_normal")
    for e in [e for e in bm.edges if (fld[e.verts[0]] - 0.5) * (fld[e.verts[1]] - 0.5) < 0]:
        a, b = e.verts
        t = (fld[a] - 0.5) / (fld[a] - fld[b])
        ca, cb, na, nb = a.co.copy(), b.co.copy(), Vector(a[rn]), Vector(b[rn])
        near = a if t < 0.5 else b
        h = near[home] if home else 0
        p = near[vpart] if vpart else 0
        _e, nv = bmesh.utils.edge_split(e, a, t)
        nv.co = ca.lerp(cb, t)
        if home:
            nv[home] = h
        if vpart:
            nv[vpart] = p
        n = na.lerp(nb, t)
        nv[rn] = n.normalized() if n.length > 1e-9 else na
        fld[nv] = 0.5
    for f in list(bm.faces):
        on = [v for v in f.verts if fld[v] == 0.5]
        if len(on) == 2 and any(fld[v] > 0.5 for v in f.verts) and any(fld[v] < 0.5 for v in f.verts):
            if not any(on[1] in e.verts for e in on[0].link_edges):
                bmesh.ops.connect_verts(bm, verts=on)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if any(fld[v] < 0.5 for v in f.verts)], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")


def outer_border(bm):
    """The shell's open border: boundary edges without a twin at the same place (a twin is
    the other side of a part seam of the hull, kept in the shell as well)."""
    by = {}
    for e in bm.edges:
        if e.is_boundary:
            by.setdefault(frozenset(_pkey(v.co) for v in e.verts), []).append(e)
    return [es[0] for es in by.values() if len(es) == 1]


def relax_border(bm, iterations=4):
    """Relaxes the outer border along its own curve (stretches running along a jagged part
    seam of the hull come out smooth too). A vertex moves at most `BORDER_MOVE` and is never
    projected onto another surface (the round-4 chin spikes); vertices that also lie on a
    kept part seam stay put, so the seam stays closed."""
    border = set(outer_border(bm))
    nb = {}
    for e in border:
        a, b = e.verts
        nb.setdefault(a, []).append(b)
        nb.setdefault(b, []).append(a)
    free = {v: ns for v, ns in nb.items()
            if len(ns) == 2 and not any(e.is_boundary and e not in border for e in v.link_edges)}
    start = {v: v.co.copy() for v in free}
    for _ in range(iterations):
        new = {v: v.co * 0.5 + (ns[0].co + ns[1].co) * 0.25 for v, ns in free.items()}
        for v, c in new.items():
            v.co = c
    for v, c in start.items():
        d = v.co - c
        if d.length > BORDER_MOVE:
            v.co = c + d * (BORDER_MOVE / d.length)


def hem_border(bm, offset, hem=HEM):
    """A turned hem: the outer border extruded `hem` * offset back towards the skin along
    the shell's own rest normals (never onto another part's skin), so the edge reads as
    cloth or plate with a thickness."""
    cut = outer_border(bm)
    if not cut or hem <= 0 or offset <= 0:
        return
    rn = bm.verts.layers.float_vector.get("rest_normal")
    ext = bmesh.ops.extrude_edge_only(bm, edges=cut)
    for v in (g for g in ext["geom"] if isinstance(g, bmesh.types.BMVert)):
        v.co -= Vector(v[rn]) * (offset * hem)
    # Face the hem away from the cloth it folds under (seen from below the edge).
    for f in (g for g in ext["geom"] if isinstance(g, bmesh.types.BMFace)):
        nbr = [o for e in f.edges for o in e.link_faces if o is not f]
        if nbr:
            f.normal_update()
            if f.normal.dot(f.calc_center_median() - nbr[0].calc_center_median()) < 0:
                f.normal_flip()


def smooth_border(bm, src, seam, offset, iterations=10, hem=0.7):
    """The whole-face cut's border (`CONTOUR` off) zig-zags along the triangles: relax the
    open border along its own curve (vertices on the part seams of `src`, `seam`, stay
    put), relax the ring inside it, put both back `offset` above the skin of `src`, then
    fold a hem inwards (`hem` * offset) so the edge has a thickness."""
    me = src.data
    tree = BVHTree.FromPolygons([v.co for v in me.vertices], [p.vertices for p in me.polygons])

    def lift(vs):
        for v in vs:
            loc, n, _i, _d = tree.find_nearest(v.co)
            if loc is not None:
                v.co = loc + Vector(n) * offset

    def cut_edge(e):
        return e.is_boundary and frozenset(e.verts) not in seam

    # Vertices on a cut edge move; those also on a part seam's border stay put.
    edge = [v for v in bm.verts if any(cut_edge(e) for e in v.link_edges)
            and not any(e.is_boundary and not cut_edge(e) for e in v.link_edges)]
    if not edge:
        return
    nb = {v: [e.other_vert(v) for e in v.link_edges if cut_edge(e)] for v in edge}
    for _ in range(iterations):
        new = {v: (v.co * 0.5 + (ns[0].co + ns[1].co) * 0.25) for v, ns in nb.items() if len(ns) == 2}
        for v, c in new.items():
            v.co = c
    lift(edge)
    eset = set(edge)
    ring = list({w for v in edge for f in v.link_faces for w in f.verts if w not in eset and not w.is_boundary})
    for _ in range(2):
        bmesh.ops.smooth_vert(bm, verts=ring, factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    lift(ring)
    # Hem: the cut edges extruded towards the skin.
    cut = [e for e in bm.edges if cut_edge(e)]
    if not cut or hem <= 0:
        return
    ext = bmesh.ops.extrude_edge_only(bm, edges=cut)
    for v in (g for g in ext["geom"] if isinstance(g, bmesh.types.BMVert)):
        loc, n, _i, _d = tree.find_nearest(v.co)
        if loc is not None:
            v.co = loc + Vector(n) * (offset * (1.0 - hem))
    # Face the hem away from the cloth it folds under (seen from below the edge).
    for f in (g for g in ext["geom"] if isinstance(g, bmesh.types.BMFace)):
        nbr = [o for e in f.edges for o in e.link_faces if o is not f]
        if nbr:
            f.normal_update()
            if f.normal.dot(f.calc_center_median() - nbr[0].calc_center_median()) < 0:
                f.normal_flip()


def tube(arm_obj, name, top, bottom, r_top, r_bot, gap_deg=0.0, rings=6, seg=20, flare_back=0.0,
         columns=None, centre=(0.0, 0.0), prefix=""):
    """A lofted tube from z=top to z=bottom with an optional front opening of gap_deg (coat
    skirt, tunic tail). With `columns` (rig.SKIRT_COLUMNS) it is weighted to the skirt bones
    (hips at the top, `skirt_<c>_01/_02` below, blended between the two nearest columns);
    otherwise to hips and the thigh of its side. `prefix` names a rider's bones
    (`rider.hips`); `centre` and the heights are in armature space."""
    verts, faces = [], []
    half = math.radians(gap_deg) / 2.0
    cols = seg + (1 if gap_deg > 0 else 0)
    for r in range(rings):
        t = r / (rings - 1)
        z = top + (bottom - top) * t
        rx = r_top[0] + (r_bot[0] - r_top[0]) * t
        ry = r_top[1] + (r_bot[1] - r_top[1]) * t
        for c in range(cols):
            if gap_deg > 0:
                a = -math.pi / 2 + half + (2 * math.pi - 2 * half) * c / seg
            else:
                a = -math.pi / 2 + 2 * math.pi * c / seg
            y = math.sin(a) * ry
            if y > 0:
                y += flare_back * t
            verts.append((centre[0] + math.cos(a) * rx, centre[1] + y, z))
    for r in range(rings - 1):
        for c in range(cols - (1 if gap_deg > 0 else 0)):
            c2 = (c + 1) % cols
            faces.append((r * cols + c, r * cols + c2, (r + 1) * cols + c2, (r + 1) * cols + c))
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    for p in me.polygons:
        p.use_smooth = True
    obj = link(bpy.data.objects.new(name, me))
    groups = {}

    def add(bone, vi, w):
        if w <= 0.001:
            return
        bone = prefix + bone
        g = groups.get(bone) or obj.vertex_groups.new(name=bone)
        groups[bone] = g
        g.add([vi], w, "ADD")

    for v in me.vertices:
        t = (top - v.co.z) / (top - bottom)
        if columns:
            ang = math.atan2(v.co.y - centre[1], v.co.x - centre[0])
            cs = sorted(columns, key=lambda c: abs(_adiff(ang, math.atan2(c[2], c[1]))))[:2]
            d0, d1 = (abs(_adiff(ang, math.atan2(c[2], c[1]))) for c in cs)
            w0 = d1 / (d0 + d1) if d0 + d1 > 1e-6 else 1.0
            h = max(0.0, 1.0 - t / 0.3)
            s2 = min(max((t - 0.45) / 0.35, 0.0), 1.0)
            s1 = max(0.0, 1.0 - h - s2)
            add("hips", v.index, h)
            for c, wc in ((cs[0], w0), (cs[1], 1.0 - w0)):
                add(f"skirt_{c[0]}_01", v.index, s1 * wc)
                add(f"skirt_{c[0]}_02", v.index, s2 * wc)
        else:
            side = max(min((v.co.x - centre[0]) / 0.12, 1.0), -1.0)
            leg = t * abs(side) * 0.8
            add("hips", v.index, 1.0 - leg)
            add("thigh_l" if side > 0 else "thigh_r", v.index, leg)
    attr = me.attributes.new("part", "INT", "POINT")
    for d in attr.data:
        d.value = 0
    orient_outward(obj, arm_obj, [prefix + b for b in rig.BODY_BONES if prefix + b in arm_obj.data.bones])
    return am.bind(obj, arm_obj)


def _adiff(a, b):
    d = (a - b) % (2 * math.pi)
    return d - 2 * math.pi if d > math.pi else d


def orient_outward(obj, arm_obj, bones=rig.BODY_BONES):
    """Flips the faces of an open surface (skirt, strap) whose normal points towards the
    nearest of `bones`."""
    seg = am.segments(arm_obj)
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    for f in bm.faces:
        c = f.calc_center_median()
        q = min((seg_point(c, *seg[b]) for b in bones), key=lambda p: (p - c).length)
        if f.normal.dot(c - q) < 0:
            f.normal_flip()
    bm.to_mesh(me)
    bm.free()
    me.update()


def strap(arm_obj, name, surface, a, b, width, offset, samples=12, bones=rig.BODY_BONES):
    """A belt over `surface` (rest mesh) along the line a-b in the XZ plane, front and back:
    rays along Y find the surface, the ribbon sits `offset` above it; smooth weights."""
    me = surface.data
    tree = BVHTree.FromPolygons([v.co for v in me.vertices], [p.vertices for p in me.polygons])
    along = (b - a).normalized()
    side = along.cross(Vector((0, 1, 0))).normalized() * (width / 2)
    verts, faces = [], []
    for sign in (-1, 1):  # front (-Y), back (+Y)
        row = []
        for i in range(samples + 1):
            c = a + (b - a) * (i / samples)
            pair = []
            for e in (-side, side):
                p = c + e
                hit, n, _i, _d = tree.ray_cast(Vector((p.x, sign * 1.0, p.z)), Vector((0, -sign, 0)))
                if hit is None:
                    break
                if n.y * sign < 0:  # a face wound inward: lift towards the ray's origin all the same
                    n = -n
                pair.append(hit + n * offset)
            if len(pair) == 2:
                row.append(pair)
        base = len(verts)
        for pair in row:
            verts += [tuple(pair[0]), tuple(pair[1])]
        for i in range(len(row) - 1):
            j = base + 2 * i
            faces.append((j, j + 1, j + 3, j + 2) if sign < 0 else (j, j + 2, j + 3, j + 1))
    m = bpy.data.meshes.new(name)
    m.from_pydata(verts, [], faces)
    obj = link(bpy.data.objects.new(name, m))
    hs = skin_weights(obj, arm_obj, bones)
    orient_outward(obj, arm_obj, bones)
    attr = m.attributes.new("part", "INT", "POINT")
    for i, d in enumerate(attr.data):
        d.value = rig.part_of_bone(hs[i])
    return am.bind(obj, arm_obj)


# --------------------------------------------------------------------------- bone layer

def prim(bm, kind, mat, size, segments=6):
    """A unit primitive (sphere, cylinder along Z, cube) scaled by `size` in its own axes
    and placed by `mat`; returns its vertices."""
    if kind == "sphere":
        r = bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=max(3, segments * 2 // 3),
                                      radius=1.0, matrix=mat)
    elif kind == "cyl":
        r = bmesh.ops.create_cone(bm, cap_ends=True, segments=segments, radius1=1.0, radius2=1.0,
                                  depth=1.0, matrix=mat)
    else:
        r = bmesh.ops.create_cube(bm, size=1.0, matrix=mat)
    vs = r["verts"]
    inv = mat.inverted()
    for v in vs:
        v.co = mat @ Vector([c * s for c, s in zip(inv @ v.co, size)])
    return vs


def along(a, b):
    """Matrix placing a unit primitive (axis Z) centred between a and b, and the length."""
    d = b - a
    rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
    return Matrix.Translation((a + b) / 2) @ rot, d.length


class BoneMesh:
    """Collects rigid primitives per bone into one mesh (`finish`)."""

    def __init__(self, arm_obj):
        self.arm = arm_obj
        self.seg = am.segments(arm_obj)
        self.bm = bmesh.new()
        self.lay = self.bm.verts.layers.int.new("bone_id")
        self.names = [b.name for b in arm_obj.data.bones]

    def add(self, kind, mat, size, bone, segments=6):
        for v in prim(self.bm, kind, mat, size, segments):
            v[self.lay] = self.names.index(bone)

    def long_bone(self, bone, r, knob, shrink=0.12, segments=5):
        a, b = self.seg[bone]
        d = b - a
        a2, b2 = a + d * shrink, b - d * shrink
        m, ln = along(a2, b2)
        self.add("cyl", m, (r, r, ln), bone, segments)
        for p in (a2, b2):
            self.add("sphere", Matrix.Translation(p), (knob, knob, knob * 0.9), bone, 5)

    def ring(self, z, rx, ry, height, bone, y=0.0, widen=1.0, segments=8):
        r = bmesh.ops.create_circle(self.bm, cap_ends=False, segments=segments, radius=1.0,
                                    matrix=Matrix.Translation((0, y, z)))
        vs = r["verts"]
        for v in vs:
            v.co.x *= rx
            v.co.y = y + (v.co.y - y) * ry
        ext = bmesh.ops.extrude_edge_only(self.bm, edges=list({e for v in vs for e in v.link_edges}))
        nv = [g for g in ext["geom"] if isinstance(g, bmesh.types.BMVert)]
        for v in nv:
            v.co.z += height
            v.co.x *= widen
        for v in vs + nv:
            v[self.lay] = self.names.index(bone)

    def finish(self, name, part_of_bone=rig.part_of_bone):
        me = bpy.data.meshes.new(name)
        self.bm.to_mesh(me)
        self.bm.free()
        obj = link(bpy.data.objects.new(name, me))
        ids = [d.value for d in me.attributes["bone_id"].data]
        rigid_weights(obj, {i: self.names[b] for i, b in enumerate(ids)})
        attr = me.attributes.new("part", "INT", "POINT")
        for i, b in enumerate(ids):
            attr.data[i].value = part_of_bone(self.names[b])
        me.attributes.remove(me.attributes["bone_id"])
        return am.bind(obj, self.arm)


def bones_h(arm_obj, name, prefix=""):
    """Stylised anatomical skeleton for skeleton H (skull, jaw, spine, ribs, pelvis, limb
    bones with knob ends, hand and foot blocks), every part rigid on one bone."""
    B = BoneMesh(arm_obj)
    P = prefix
    seg = B.seg
    o = seg[P + "hips"][0] - Vector((0, 0, 0.95))  # rig origin offset (rider rigs)

    def at(x, y, z):
        return Matrix.Translation(o + Vector((x, y, z)))

    # Kept well inside the modelled face (#283: eye sockets, chin) so no bone shows.
    B.add("sphere", at(0, -0.006, 1.660), (0.062, 0.076, 0.058), P + "head", 6)
    B.add("cube", at(0, -0.04, 1.597), (0.062, 0.045, 0.026), P + "head")
    z = 0.98
    while z < 1.53:
        bone = ("hips" if z < 1.05 else "spine" if z < 1.20 else "spine1" if z < 1.33 else "spine2" if z < 1.45 else "neck")
        B.add("cube", at(0, 0.035 if z < 1.45 else 0.01, z), (0.032, 0.03, 0.026), P + bone)
        z += 0.07
    for i, zz in enumerate((1.22, 1.28, 1.34, 1.39)):
        B.ring(o.z + zz, 0.095 + 0.012 * math.sin(i / 3 * math.pi), 0.072 - 0.004 * i, 0.018,
               P + ("spine1" if zz < 1.33 else "spine2"), y=o.y)
    B.add("cube", at(0, -0.072, 1.31), (0.025, 0.012, 0.15), P + "spine2")
    B.ring(o.z + 0.93, 0.12, 0.085, 0.07, P + "hips", y=o.y + 0.005, widen=1.15)
    for s in ("_l", "_r"):
        B.long_bone(P + "clavicle" + s, 0.009, 0.014, shrink=0.05, segments=4)
        B.long_bone(P + "upperarm" + s, 0.014, 0.024)
        B.long_bone(P + "forearm" + s, 0.012, 0.02)
        h, t = seg[P + "hand" + s]
        m, ln = along(h, t + (t - h) * 0.6)
        B.add("cube", m, (0.018, 0.05, ln), P + "hand" + s)
        B.long_bone(P + "thigh" + s, 0.019, 0.032)
        B.long_bone(P + "shin" + s, 0.016, 0.026)
        h, t = seg[P + "foot" + s]
        m, ln = along(h + Vector((0, 0.04, -0.01)), seg[P + "toe" + s][1])
        B.add("cube", m, (0.045, 0.02, ln * 0.95), P + "foot" + s)
    return B.finish(name, lambda b: rig.part_of_bone(b[len(P):]))


# --------------------------------------------------------------------------- cuts

def caps(arm_obj, name, radii, joints=rig.JOINTS, segments=6, prefix=""):
    """Hidden stump caps (#183): per joint of `joints` two discs across the child bone's
    head, the parent side (facing the child; `_part` 16 + joint index, rigid on the parent
    part's bone) and the child side (facing the parent; 32 + index). Material 0 meat, 1 the
    bone end. `radii` = {joint name: radius in metres}. The renderer shows them only on a
    cut joint."""
    seg = am.segments(arm_obj)
    bones = {b.name: b for b in arm_obj.data.bones}
    bm = bmesh.new()
    lay_b = bm.verts.layers.int.new("bone_id")
    lay_p = bm.verts.layers.int.new("part")
    names = [b.name for b in arm_obj.data.bones]
    for j, (jn, _pp, _cp, child) in enumerate(joints):
        child = prefix + child
        head, tail = seg[child]
        d = (tail - head).normalized()
        parent = bones[child].parent.name
        r = radii.get(jn, 0.05)
        q = d.to_track_quat("Z", "Y")
        for side, bone, pid, sgn in (("p", parent, 16 + j, 1.0), ("c", child, 32 + j, -1.0)):
            centre = head + d * (0.004 * sgn)
            m = Matrix.Translation(centre) @ q.to_matrix().to_4x4()
            ring = []
            inner = []
            for i in range(segments):
                a = 2 * math.pi * i / segments
                ring.append(bm.verts.new(m @ Vector((math.cos(a) * r, math.sin(a) * r, 0))))
                inner.append(bm.verts.new(m @ Vector((math.cos(a) * r * 0.3, math.sin(a) * r * 0.3, 0.003 * sgn))))
            for v in ring + inner:
                v[lay_b] = names.index(bone)
                v[lay_p] = pid
            # Facing: the parent side looks along +d (towards the child), the child side back.
            order = (lambda s: s) if sgn > 0 else (lambda s: list(reversed(s)))
            f = bm.faces.new(order(inner))
            f.material_index = 1
            for i in range(segments):
                k = (i + 1) % segments
                quad = [ring[i], ring[k], inner[k], inner[i]]
                f = bm.faces.new(quad if sgn > 0 else list(reversed(quad)))
                f.material_index = 0
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = link(bpy.data.objects.new(name, me))
    ids = [d.value for d in me.attributes["bone_id"].data]
    rigid_weights(obj, {i: names[b] for i, b in enumerate(ids)})
    me.attributes.remove(me.attributes["bone_id"])
    obj["acr_caps"] = [jn for jn, *_ in joints]
    return am.bind(obj, arm_obj)


# --------------------------------------------------------------------------- morphs

def _key(obj, name):
    if obj.data.shape_keys is None:
        obj.shape_key_add(name="Basis", from_mix=False)
    key = obj.shape_key_add(name=name, from_mix=False)
    key.value = 0.0  # Blender 5.2 creates new keys at 1.0
    return key


def toward_bones(obj, arm_obj, name, keep, bones=rig.BODY_BONES, per_bone=None):
    """Shape key (glTF morph target) `name`: every vertex moves towards the nearest point of
    the nearest of `bones`, keeping `keep` (or per_bone[bone]) of its distance.
    `deflate` (#178: flesh and clothes collapse onto the bones, strapped gear follows) and
    `emaciated` (#183: hunger) are made this way."""
    seg = am.segments(arm_obj)
    key = _key(obj, name)
    for v in obj.data.vertices:
        best, q, bb = None, None, None
        for b in bones:
            p = seg_point(v.co, *seg[b])
            dist = (v.co - p).length
            if best is None or dist < best:
                best, q, bb = dist, p, b
        k = per_bone.get(bb, keep) if per_bone else keep
        key.data[v.index].co = q + (v.co - q) * k
    return key


def finish_mesh(obj):
    """Removes build-time attributes that must not reach the glTF file."""
    me = obj.data
    for n in ("home", "rest_normal"):
        if n in me.attributes:
            me.attributes.remove(me.attributes[n])
    for k in ("acr_home_bones", "acr_feet_relaxed"):
        if k in obj:
            del obj[k]
