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
    # Outward rest normals: the vertex normal, flipped where it points at its own bone.
    seg = am.segments(arm_obj)
    rn = mesh.attributes.new("rest_normal", "FLOAT_VECTOR", "POINT")
    for v in mesh.vertices:
        n = v.normal.copy()
        if n.dot(v.co - seg_point(v.co, *seg[hs[v.index]])) < 0:
            n.negate()
        rn.data[v.index].vector = n
    return am.bind(obj, arm_obj)


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


def derive(src, name, keep_face, offset, border=False):
    """A clothing shell: the faces of `src` for which keep_face(centre, normal, home, part)
    holds, pushed out by `offset` metres along the rest normals; same weights, homes and
    parts. With `border` the cut edges (not the part seams of `src`) are smoothed along
    their curve and get a hem fold (`smooth_border`)."""
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


def smooth_border(bm, src, seam, offset, iterations=10, hem=0.7):
    """Cut edges of a shell made from a decimated hull zig-zag along the triangles: relax
    the open border along its own curve (vertices on the part seams of `src`, `seam`, stay
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
