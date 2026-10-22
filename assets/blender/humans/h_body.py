"""Layered European body on skeleton H (needs `bpy`, `bmesh`).

Layers, innermost first:
  bones    anatomical bone mesh (skull, spine, ribs, pelvis, limb bones), rigid on H
  body     flesh/skin: a Skin-modifier hull around the rest skeleton, smooth weights
  clothes  shells derived from the body (the same vertices pushed out along the normals, the
           same weights, so they deform exactly like the skin), plus free parts (coat skirt)
  gear     separate objects parented to socket bones (hat, musket, cartridge box, knapsack)
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.geometry import intersect_point_line

import h_rig


# --------------------------------------------------------------------------- helpers

def _link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


def _seg_dist(p, a, b):
    _q, t = intersect_point_line(p, a, b)
    t = min(max(t, 0.0), 1.0)
    return (p - (a + (b - a) * t)).length


def _children(arm_obj):
    ch = {}
    for b in arm_obj.data.bones:
        if b.parent:
            ch.setdefault(b.parent.name, []).append(b.name)
    return ch


def skin_weights(obj, arm_obj, bones=h_rig.BODY_BONES, falloff=4.0):
    """Smooth weights: each vertex belongs to its nearest bone segment (home) and blends
    with home's parent and children by inverse distance^falloff. Returns {vertex: home}."""
    seg = h_rig.segments(arm_obj)
    ch = _children(arm_obj)
    parent = {b.name: (b.parent.name if b.parent else None) for b in arm_obj.data.bones}
    homes = {}
    for v in obj.data.vertices:
        p = v.co
        d = {b: _seg_dist(p, *seg[b]) for b in bones}
        home = min(d, key=d.get)
        homes[v.index] = home
        cand = [home] + [c for c in ([parent[home]] + ch.get(home, [])) if c in bones]
        w = {c: 1.0 / (d[c] + 0.01) ** falloff for c in cand}
        tot = sum(w.values())
        for c, x in w.items():
            x /= tot
            if x < 0.04:
                continue
            g = obj.vertex_groups.get(c) or obj.vertex_groups.new(name=c)
            g.add([v.index], x, "REPLACE")
    return homes


def rigid_weights(obj, bone_of_vertex):
    for vi, b in bone_of_vertex.items():
        g = obj.vertex_groups.get(b) or obj.vertex_groups.new(name=b)
        g.add([vi], 1.0, "REPLACE")


def bind(obj, arm_obj):
    obj.parent = arm_obj
    m = obj.modifiers.new("H", "ARMATURE")
    m.object = arm_obj
    return obj


def set_material(obj, mat):
    obj.data.materials.clear()
    obj.data.materials.append(mat)


# --------------------------------------------------------------------------- body

def _skin_graph(build):
    """(name, parent, position, (radius x, radius y)) in rest armature space."""
    w = build.get("width", 1.0)
    g = [
        ("pelvis", None, (0, 0.0, 0.93), (0.155 * w, 0.11)),
        ("waist", "pelvis", (0, 0.0, 1.08), (0.135 * w, 0.095 * build.get("belly", 1.0))),
        ("chest", "waist", (0, -0.005, 1.27), (0.16 * w, 0.105)),
        ("upper", "chest", (0, 0.0, 1.40), (0.165 * w, 0.095)),
        ("neck0", "upper", (0, -0.005, 1.47), (0.052, 0.055)),
        ("neck1", "neck0", (0, -0.01, 1.555), (0.05, 0.05)),
        ("jaw", "neck1", (0, -0.03, 1.60), (0.062, 0.085)),
        ("skull", "jaw", (0, -0.015, 1.665), (0.08, 0.098)),
        ("crown", "skull", (0, -0.01, 1.725), (0.05, 0.06)),
    ]
    for side, sfx in ((1, "_l"), (-1, "_r")):
        sh, elbow, wrist, hand_end, d = h_rig._arm(side)
        g += [
            ("shoulder" + sfx, "upper", tuple(sh + Vector((0.01 * side, 0, -0.01))), (0.058, 0.06)),
            ("elbow" + sfx, "shoulder" + sfx, tuple(elbow), (0.042, 0.042)),
            ("wrist" + sfx, "elbow" + sfx, tuple(wrist), (0.03, 0.026)),
            ("hand" + sfx, "wrist" + sfx, tuple(wrist + d * 0.075), (0.022, 0.045)),
            ("finger" + sfx, "hand" + sfx, tuple(hand_end + d * 0.05), (0.014, 0.036)),
            ("hip" + sfx, "pelvis", (0.095 * side, 0.0, 0.86), (0.085, 0.09)),
            ("knee" + sfx, "hip" + sfx, (0.0945 * side, 0.0, 0.50), (0.056, 0.058)),
            ("calf" + sfx, "knee" + sfx, (0.0945 * side, 0.015, 0.36), (0.055, 0.058)),
            ("ankle" + sfx, "calf" + sfx, (0.0945 * side, 0.025, 0.10), (0.035, 0.035)),
            ("heel" + sfx, "ankle" + sfx, (0.0945 * side, 0.02, 0.04), (0.04, 0.045)),
            ("toes" + sfx, "heel" + sfx, (0.099 * side, -0.16, 0.035), (0.042, 0.03)),
        ]
    return g


def body(arm_obj, name="body", build=None):
    """Continuous low-poly skin around skeleton H: Skin modifier + one subdivision, applied
    (normals recalculated: the skin hull has mixed winding); smooth weights."""
    build = build or {}
    graph = _skin_graph(build)
    idx = {n: i for i, (n, *_r) in enumerate(graph)}
    me = bpy.data.meshes.new(name + ".graph")
    me.from_pydata([p for _n, _p, p, _r in graph],
                   [(idx[par], idx[n]) for n, par, _p, _r in graph if par], [])
    tmp = _link(bpy.data.objects.new(name + ".graph", me))
    sk = tmp.modifiers.new("skin", "SKIN")
    sk.use_smooth_shade = True
    sk.branch_smoothing = 0.3
    for i, (_n, _par, _p, r) in enumerate(graph):
        me.skin_vertices[0].data[i].radius = r
    me.skin_vertices[0].data[0].use_root = True
    sub = tmp.modifiers.new("sub", "SUBSURF")
    sub.levels = 1
    sub.render_levels = 1
    dg = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(tmp.evaluated_get(dg))
    mesh.name = name
    bpy.data.objects.remove(tmp)
    bpy.data.meshes.remove(me)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    obj = _link(bpy.data.objects.new(name, mesh))
    for poly in mesh.polygons:
        poly.use_smooth = True
    homes = skin_weights(obj, arm_obj)
    obj["homes"] = [homes[i] for i in range(len(mesh.vertices))]
    return bind(obj, arm_obj)


def derive(src, name, keep_face, offset):
    """A clothing shell: the faces of `src` for which keep_face(centre, normal, home) holds,
    pushed out by `offset` metres along the vertex normals; same vertex groups."""
    obj = src.copy()
    obj.data = src.data.copy()
    obj.data.name = name
    obj.name = name
    obj.modifiers.clear()
    _link(obj)
    homes = list(src["homes"])
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.normal_update()
    bm.verts.ensure_lookup_table()
    # Offset direction: the vertex normal, flipped where it points towards the vertex's own
    # bone (the Skin-modifier hull is not consistently wound, so some normals point in).
    seg = h_rig.segments(src.parent)
    normals = {}
    for v in bm.verts:
        a, b = seg[homes[v.index]]
        q, t = intersect_point_line(v.co, a, b)
        q = a + (b - a) * min(max(t, 0.0), 1.0)
        n = v.normal.copy()
        if n.dot(v.co - q) < 0:
            n.negate()
        normals[v] = n
    kill = []
    for f in bm.faces:
        votes = {}
        for v in f.verts:
            votes[homes[v.index]] = votes.get(homes[v.index], 0) + 1
        home = max(votes, key=votes.get)
        if not keep_face(f.calc_center_median(), f.normal, home):
            kill.append(f)
    bmesh.ops.delete(bm, geom=kill, context="FACES")
    for v in bm.verts:
        v.co += normals[v] * offset  # keyed by BMVert: delete() renumbers indices
    bm.to_mesh(obj.data)
    bm.free()
    for a in list(obj.data.color_attributes):
        obj.data.color_attributes.remove(a)
    if "homes" in obj:
        del obj["homes"]
    return bind(obj, src.parent)


def skirt(arm_obj, name, top, bottom, r_top, r_bot, gap_deg=0.0, rings=6, seg=20, flare_back=0.0):
    """Coat skirt / tunic tail: a lofted tube from z=top to z=bottom with an optional front
    opening of gap_deg; weights blend hips -> thigh of the vertex's side."""
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
            verts.append((math.cos(a) * rx, y, z))
    for r in range(rings - 1):
        for c in range(cols - (1 if gap_deg > 0 else 0)):
            c2 = (c + 1) % cols
            faces.append((r * cols + c, r * cols + c2, (r + 1) * cols + c2, (r + 1) * cols + c))
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    for p in me.polygons:
        p.use_smooth = True
    obj = _link(bpy.data.objects.new(name, me))
    gh = obj.vertex_groups.new(name="hips")
    gl = obj.vertex_groups.new(name="thigh_l")
    gr = obj.vertex_groups.new(name="thigh_r")
    for v in me.vertices:
        t = (top - v.co.z) / (top - bottom)
        side = max(min(v.co.x / 0.12, 1.0), -1.0)
        leg = t * abs(side) * 0.8
        gh.add([v.index], 1.0 - leg, "REPLACE")
        (gl if side > 0 else gr).add([v.index], max(leg, 0.0), "REPLACE")
    return bind(obj, arm_obj)


def strap(arm_obj, name, surface, a, b, width, offset, samples=12):
    """A belt over `surface` (rest mesh) along the line a-b (in the XZ plane), front and
    back: rays along Y find the surface, the ribbon sits `offset` above it."""
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
    obj = _link(bpy.data.objects.new(name, m))
    skin_weights(obj, arm_obj)
    return bind(obj, arm_obj)


# --------------------------------------------------------------------------- bone layer

def _prim(bm, kind, mat, size):
    if kind == "sphere":
        r = bmesh.ops.create_uvsphere(bm, u_segments=8, v_segments=5, radius=1.0, matrix=mat)
    elif kind == "cyl":
        r = bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=1.0, radius2=1.0,
                                  depth=1.0, matrix=mat)
    else:
        r = bmesh.ops.create_cube(bm, size=1.0, matrix=mat)
    vs = r["verts"]
    inv = mat.inverted()
    for v in vs:
        v.co = mat @ Vector([c * s for c, s in zip(inv @ v.co, size)])
    return vs


def _along(a, b):
    """Matrix placing a unit primitive (axis Z) centred between a and b, and the length."""
    d = b - a
    rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
    return Matrix.Translation((a + b) / 2) @ rot, d.length


def bones_mesh(arm_obj, name="bones"):
    """Stylised anatomical skeleton, every part rigid on one bone of H."""
    seg = h_rig.segments(arm_obj)
    bm = bmesh.new()
    lay = bm.verts.layers.int.new("bone_id")
    names = [b.name for b in arm_obj.data.bones]

    def own(verts, bone):
        for v in verts:
            v[lay] = names.index(bone)

    def add(kind, mat, size, bone):
        own(_prim(bm, kind, mat, size), bone)

    def long_bone(bone, r, knob, shrink=0.12):
        a, b = seg[bone]
        d = b - a
        a2, b2 = a + d * shrink, b - d * shrink
        m, ln = _along(a2, b2)
        add("cyl", m, (r, r, ln), bone)
        for p in (a2, b2):
            add("sphere", Matrix.Translation(p), (knob, knob, knob * 0.9), bone)

    def ring(z, rx, ry, height, bone, y=0.0, widen=1.0):
        r = bmesh.ops.create_circle(bm, cap_ends=False, segments=10, radius=1.0,
                                    matrix=Matrix.Translation((0, y, z)))
        vs = r["verts"]
        for v in vs:
            v.co.x *= rx
            v.co.y = y + (v.co.y - y) * ry
        ext = bmesh.ops.extrude_edge_only(bm, edges=list({e for v in vs for e in v.link_edges}))
        nv = [g for g in ext["geom"] if isinstance(g, bmesh.types.BMVert)]
        for v in nv:
            v.co.z += height
            v.co.x *= widen
        own(vs + nv, bone)

    # Skull + jaw.
    add("sphere", Matrix.Translation((0, -0.012, 1.665)), (0.068, 0.085, 0.072), "head")
    add("cube", Matrix.Translation((0, -0.05, 1.595)), (0.07, 0.05, 0.03), "head")
    # Spine: vertebrae from the pelvis to the skull.
    z = 0.98
    while z < 1.53:
        bone = ("hips" if z < 1.05 else "spine" if z < 1.20 else "spine1" if z < 1.33
                else "spine2" if z < 1.45 else "neck")
        add("cube", Matrix.Translation((0, 0.035 if z < 1.45 else 0.01, z)), (0.032, 0.03, 0.026), bone)
        z += 0.045
    # Ribcage: open elliptical hoops and a sternum.
    for i, z in enumerate((1.22, 1.26, 1.30, 1.34, 1.38)):
        ring(z, 0.095 + 0.012 * math.sin(i / 4 * math.pi), 0.072 - 0.004 * i, 0.016,
             "spine1" if z < 1.33 else "spine2")
    add("cube", Matrix.Translation((0, -0.072, 1.31)), (0.025, 0.012, 0.15), "spine2")
    # Pelvis: a flaring ring.
    ring(0.93, 0.12, 0.085, 0.07, "hips", y=0.005, widen=1.15)
    for s in ("_l", "_r"):
        long_bone("clavicle" + s, 0.009, 0.014, shrink=0.05)
        long_bone("upperarm" + s, 0.014, 0.024)
        long_bone("forearm" + s, 0.012, 0.02)
        h, t = seg["hand" + s]
        m, ln = _along(h, t + (t - h) * 0.6)
        add("cube", m, (0.018, 0.05, ln), "hand" + s)
        long_bone("thigh" + s, 0.019, 0.032)
        long_bone("shin" + s, 0.016, 0.026)
        h, t = seg["foot" + s]
        m, ln = _along(h + Vector((0, 0.04, -0.01)), seg["toe" + s][1])
        add("cube", m, (0.045, 0.02, ln * 0.95), "foot" + s)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = _link(bpy.data.objects.new(name, me))
    ids = me.attributes["bone_id"].data
    rigid_weights(obj, {i: names[ids[i].value] for i in range(len(me.vertices))})
    me.attributes.remove(me.attributes["bone_id"])
    return bind(obj, arm_obj)


# --------------------------------------------------------------------------- gear

def _mesh_from_bm(bm, name):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    return _link(bpy.data.objects.new(name, me))


def tricorn(name="tricorn"):
    """Black felt tricorn, origin at the crown socket. Brim turned up on three sides."""
    bm = bmesh.new()
    n = 24
    inner, outer = [], []
    for i in range(n):
        a = 2 * math.pi * i / n
        rel = a + math.pi / 2  # 0 at the front corner (-Y)
        k = (1 - math.cos(3 * rel)) / 2  # 0 at the 3 corners, 1 between
        r_out = 0.185 - 0.04 * k
        inner.append(bm.verts.new((math.cos(a) * 0.105, math.sin(a) * 0.115, -0.03)))
        outer.append(bm.verts.new((math.cos(a) * r_out, math.sin(a) * (r_out + 0.01),
                                   -0.03 + 0.085 * k + 0.01)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((inner[i], inner[j], outer[j], outer[i]))
    top = bmesh.ops.create_cone(bm, cap_ends=True, segments=12, radius1=0.1, radius2=0.085,
                                depth=0.09, matrix=Matrix.Translation((0, 0, 0.012)))
    for v in top["verts"]:
        v.co.y *= 1.1
    return _mesh_from_bm(bm, name)


def slouch_hat(name="slouch_hat"):
    bm = bmesh.new()
    n = 20
    inner, outer = [], []
    for i in range(n):
        a = 2 * math.pi * i / n
        droop = 0.025 * (1 + math.sin(a)) / 2
        inner.append(bm.verts.new((math.cos(a) * 0.1, math.sin(a) * 0.11, -0.03)))
        outer.append(bm.verts.new((math.cos(a) * 0.19, math.sin(a) * 0.2, -0.045 - droop)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((inner[i], inner[j], outer[j], outer[i]))
    top = bmesh.ops.create_cone(bm, cap_ends=True, segments=12, radius1=0.1, radius2=0.08,
                                depth=0.1, matrix=Matrix.Translation((0, 0, 0.015)))
    for v in top["verts"]:
        v.co.y *= 1.1
    return _mesh_from_bm(bm, name)


def musket(name="musket"):
    """Brown Bess-like musket, 1.55 m along +Z; materials 0 wood, 1 metal. Origin at the grip
    (0.25 m above the butt)."""
    bm = bmesh.new()
    metal = set()
    for (z0, z1, w0, w1, d0, d1) in ((0.0, 0.30, 0.04, 0.03, 0.11, 0.05),
                                     (0.30, 1.30, 0.03, 0.026, 0.045, 0.03)):
        r = bmesh.ops.create_cube(bm, size=1.0)
        for v in r["verts"]:
            top = v.co.z > 0
            v.co = Vector((v.co.x * (w1 if top else w0), v.co.y * (d1 if top else d0) + 0.01,
                           z1 if top else z0))
    r = bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.011, radius2=0.009,
                              depth=1.25, matrix=Matrix.Translation((0, -0.01, 0.28 + 0.625)))
    metal |= set(r["verts"])
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        v.co = Vector((v.co.x * 0.035, v.co.y * 0.04 - 0.005, 0.38 + v.co.z * 0.07))
    metal |= set(r["verts"])
    for f in bm.faces:
        f.material_index = 1 if f.verts[0] in metal else 0
    for v in bm.verts:
        v.co.z -= 0.25
    return _mesh_from_bm(bm, name)


def box(name, size):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Diagonal((*size, 1)))
    bmesh.ops.bevel(bm, geom=list(bm.edges), offset=min(size) * 0.2, segments=1,
                    affect="EDGES", profile=0.5)
    return _mesh_from_bm(bm, name)


def attach(obj, arm_obj, bone, world_matrix):
    """Parents `obj` to `bone` (armature in its current pose) so that it sits at
    `world_matrix` now and follows the bone afterwards."""
    obj.parent = arm_obj
    obj.parent_type = "BONE"
    obj.parent_bone = bone
    bpy.context.view_layer.update()
    pb = arm_obj.pose.bones[bone]
    tail = arm_obj.matrix_world @ pb.matrix @ Matrix.Translation((0, pb.bone.length, 0))
    obj.matrix_parent_inverse = tail.inverted()
    obj.matrix_basis = world_matrix
    obj["acr_socket"] = bone
    return obj


# --------------------------------------------------------------------------- shape keys

def add_deflate(obj, arm_obj, keep=0.65):
    """Shape key (glTF morph target) `deflate`: every vertex moves towards the nearest point
    of its bone, keeping `keep` of its distance. Clothes collapse onto the bones while the
    flesh under them dissolves; driven by the decay (h_decay.DEFLATE_WINDOW)."""
    seg = h_rig.segments(arm_obj)
    if obj.data.shape_keys is None:
        obj.shape_key_add(name="Basis", from_mix=False)
    key = obj.shape_key_add(name="deflate", from_mix=False)
    key.value = 0.0  # Blender 5.2 creates new keys at 1.0
    for v in obj.data.vertices:
        best, q = None, None
        for b in h_rig.BODY_BONES:
            a, c = seg[b]
            p, t = intersect_point_line(v.co, a, c)
            p = a + (c - a) * min(max(t, 0.0), 1.0)
            dist = (v.co - p).length
            if best is None or dist < best:
                best, q = dist, p
        key.data[v.index].co = q + (v.co - q) * keep
    return key


def add_lie_flat(obj, back=0.7, front=0.6, widen=1.12):
    """Shape key `lie_flat` for free-hanging parts (coat skirt): squashes the rest-pose
    front/back depth so a stiff tube lies flat on a prone corpse; weight 1 once lying."""
    if obj.data.shape_keys is None:
        obj.shape_key_add(name="Basis", from_mix=False)
    key = obj.shape_key_add(name="lie_flat", from_mix=False)
    key.value = 0.0
    for v in obj.data.vertices:
        c = v.co.copy()
        c.y *= back if c.y > 0 else front
        c.x *= widen
        key.data[v.index].co = c
    return key
