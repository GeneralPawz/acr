"""Gear meshes (needs `bpy`, `bmesh`): hats, muskets, boxes, swords. Each returns an object
with its origin at the grip / crown socket; `armature.attach` puts it on a socket bone (gear
that drops at death) and `rigid` skins it to a deforming bone (strapped gear that follows the
body's `deflate`)."""

import math

import bmesh
import bpy
from mathutils import Matrix, Quaternion, Vector

from . import armature as am
from .body import link


def from_bm(bm, name):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    return link(bpy.data.objects.new(name, me))


def tricorn(name, n=16, lace=None):
    """Black felt tricorn, origin at the crown socket; brim turned up on three sides. `lace`:
    the width (m) of braid round the brim's edge, painted in the palette's `lace` colour
    (`masks.hat_lace`; the object carries `acr_lace`, `masks.lace` makes the entry)."""
    bm = bmesh.new()
    inner, outer = [], []
    for i in range(n):
        a = 2 * math.pi * i / n
        rel = a + math.pi / 2  # 0 at the front corner (-Y)
        k = (1 - math.cos(3 * rel)) / 2  # 0 at the 3 corners, 1 between
        r_out = 0.19 - 0.04 * k
        inner.append(bm.verts.new((math.cos(a) * 0.105, math.sin(a) * 0.115, -0.03)))
        outer.append(bm.verts.new((math.cos(a) * r_out, math.sin(a) * (r_out + 0.01), -0.03 + 0.09 * k + 0.01)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((inner[i], inner[j], outer[j], outer[i]))
    top = bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.1, radius2=0.085, depth=0.09,
                                matrix=Matrix.Translation((0, 0, 0.012)))
    for v in top["verts"]:
        v.co.y *= 1.1
    obj = from_bm(bm, name)
    if lace:
        obj["acr_lace"] = float(lace)
    return obj


def slouch_hat(name, n=14):
    bm = bmesh.new()
    inner, outer = [], []
    for i in range(n):
        a = 2 * math.pi * i / n
        droop = 0.025 * (1 + math.sin(a)) / 2
        inner.append(bm.verts.new((math.cos(a) * 0.1, math.sin(a) * 0.11, -0.03)))
        outer.append(bm.verts.new((math.cos(a) * 0.19, math.sin(a) * 0.2, -0.045 - droop)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((inner[i], inner[j], outer[j], outer[i]))
    top = bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.1, radius2=0.08, depth=0.1,
                                matrix=Matrix.Translation((0, 0, 0.015)))
    for v in top["verts"]:
        v.co.y *= 1.1
    return from_bm(bm, name)


def cocked_hat(name, n=14, lace=None):
    """Bicorne-like dragoon hat (a tricorn with the front corner flattened)."""
    obj = tricorn(name, n, lace=lace)
    for v in obj.data.vertices:
        if v.co.y < -0.12:
            v.co.y *= 0.8
    return obj


def musket(name, length=1.55, bayonet=0.0):
    """Brown Bess-like musket along +Z, origin at the grip (0.25 m above the butt);
    material 0 wood, 1 metal; empty `socket.muzzle` child at the muzzle."""
    bm = bmesh.new()
    metal = set()
    for (z0, z1, w0, w1, d0, d1) in ((0.0, 0.30, 0.04, 0.03, 0.11, 0.05),
                                     (0.30, length - 0.25, 0.03, 0.026, 0.045, 0.03)):
        r = bmesh.ops.create_cube(bm, size=1.0)
        for v in r["verts"]:
            top = v.co.z > 0
            v.co = Vector((v.co.x * (w1 if top else w0), v.co.y * (d1 if top else d0) + 0.01, z1 if top else z0))
    r = bmesh.ops.create_cone(bm, cap_ends=True, segments=5, radius1=0.011, radius2=0.009, depth=length - 0.30,
                              matrix=Matrix.Translation((0, -0.01, 0.28 + (length - 0.30) / 2)))
    metal |= set(r["verts"])
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        v.co = Vector((v.co.x * 0.035, v.co.y * 0.04 - 0.005, 0.38 + v.co.z * 0.07))
    metal |= set(r["verts"])
    if bayonet:
        r = bmesh.ops.create_cone(bm, cap_ends=True, segments=3, radius1=0.008, radius2=0.001, depth=bayonet,
                                  matrix=Matrix.Translation((0, -0.025, length - 0.02 + bayonet / 2)))
        metal |= set(r["verts"])
    for f in bm.faces:
        f.material_index = 1 if f.verts[0] in metal else 0
    for v in bm.verts:
        v.co.z -= 0.25
    obj = from_bm(bm, name)
    m = bpy.data.objects.new("socket.muzzle", None)
    m.empty_display_size = 0.05
    link(m)
    m.parent = obj
    m.location = (0, -0.01, length - 0.25)
    return obj


def rammer(name, length=0.9):
    """Steel ramrod along +Z from its tail (origin, where the hand holds it) to the head."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=4, radius1=0.0075, radius2=0.0075, depth=length,
                          matrix=Matrix.Translation((0, 0, length / 2)))
    bmesh.ops.create_cone(bm, cap_ends=True, segments=4, radius1=0.012, radius2=0.012, depth=0.03,
                          matrix=Matrix.Translation((0, 0, length - 0.015)))
    return from_bm(bm, name)


def sabre(name, length=0.95):
    """Sabre in its scabbard, along -Z from the hilt (origin)."""
    bm = bmesh.new()
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        v.co = Vector((v.co.x * 0.012, v.co.y * 0.035, -length / 2 + v.co.z * length))
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        v.co = Vector((v.co.x * 0.03, v.co.y * 0.08, 0.04 + v.co.z * 0.10))
    return from_bm(bm, name)


def box(name, size, bevel=0.2):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Diagonal((*size, 1)))
    if bevel:
        bmesh.ops.bevel(bm, geom=list(bm.edges), offset=min(size) * bevel, segments=1, affect="EDGES", profile=0.5)
    return from_bm(bm, name)


def rigid(obj, arm_obj, bone, world_matrix, part=0):
    """Skins gear rigidly to a deforming bone at `world_matrix` (rest pose of the armature
    assumed at identity world transform): it follows the body and its morph targets."""
    obj.matrix_world = world_matrix
    bpy.context.view_layer.update()
    me = obj.data
    me.transform(obj.matrix_world)
    obj.matrix_world = Matrix.Identity(4)
    g = obj.vertex_groups.new(name=bone)
    g.add(list(range(len(me.vertices))), 1.0, "REPLACE")
    attr = me.attributes.new("part", "INT", "POINT")
    for d in attr.data:
        d.value = part
    return am.bind(obj, arm_obj)


def held(item, name, keys=None):
    """A library item (`held.Item` or its name in `held.ITEMS`) as an object `name`: one
    mesh in the item frame (origin at the grip, +Z the long axis), a material slot per
    material key (`acr_material_keys`; `keys` {part: key} remaps parts, e.g. a spear's
    head to `stone`), its sockets as empties `<name>.socket.*` under it (the strike layer
    finds `*socket.tip`) and its vertex groups (`cloth`, `pin`). Materials come from
    `Character.colours` like the musket's."""
    from . import held as library

    it = library.build(item) if isinstance(item, str) else item
    if keys:
        it.keys = dict(it.keys, **keys)
    slots, face_slot = it.material_keys()
    me = bpy.data.meshes.new(name)
    me.from_pydata(it.verts, [], it.faces)
    for p, s in zip(me.polygons, face_slot):
        p.material_index = s
    me.update()
    obj = link(bpy.data.objects.new(name, me))
    obj["acr_material_keys"] = slots
    obj["acr_held_item"] = it.name
    for g, idx in it.groups.items():
        obj.vertex_groups.new(name=g).add(idx, 1.0, "REPLACE")
    for s, co in it.sockets.items():
        e = bpy.data.objects.new(f"{name}.{s}", None)
        e.empty_display_size = 0.05
        link(e)
        e.parent = obj
        e.location = co
    return obj


def hold(ch, item, name, socket="socket.hand_r", matrix=None, keys=None, slot=None):
    """Puts library item `item` on `socket` of character `ch` as held gear (dropped at
    death): the item frame at `matrix` (world, rest of the clip's pose; default the
    socket's own frame, upright like `line`'s musket). Returns the object."""
    obj = held(item, name, keys)
    if matrix is None:
        sock = am.world(ch.arm, socket)
        rest_up = sock.to_3x3() @ ch.arm.pose.bones[socket].bone.matrix_local.to_3x3().inverted()
        matrix = Matrix.Translation(sock.translation) @ rest_up.to_4x4()
    am.attach(obj, ch.arm, socket, matrix)
    ch.held[slot or name.split(".", 1)[-1]] = obj
    ch.keys[obj.name] = obj["acr_material_keys"][0]
    return obj


# --------------------------------------------------------------------------- seated gear
# Carried gear (knapsack, quiver, back shield, horn, pouches) seated on the body's outermost
# surface (docs/assets-humans.md, "Seated gear"): cast from the item's vertices onto the
# clothes, skin and the straps already on them; laid against them with a small gap, turned to
# the surface's slope and skinned like the surface under it.

SEAT_GAP = 0.005  # m between the item's contact face and the surface under it
SEAT_TURN = math.radians(25)  # most the item turns to lie along the surface
SEAT_LEAD = 0.12  # m the rays start outside the item (in front of the contact face)
# Bones (name prefixes, without the character prefix) whose surface carries no gear: a ray
# hitting the head or an arm first is ignored, and seated gear takes none of their weights
# (nor the coat skirt's): it rides on the trunk, not on a swinging arm or skirt.
SEAT_FREE = ("head", "neck", "upperarm", "forearm", "hand", "skirt", "socket")


def _free(bone, prefix=""):
    return bone[len(prefix):].startswith(SEAT_FREE)


def _seat_sources(ch, skip=()):
    """(object, BVH, per-vertex weights) of the flesh, clothes and gear laid so far (world
    rest pose); shoes and `skip` left out."""
    from mathutils.bvhtree import BVHTree

    out = []
    for o in ch.layers["flesh"] + ch.layers["clothes"] + ch.layers["gear"]:
        if o.type != "MESH" or o in skip or ch.keys.get(o.name) == "shoes" or not o.data.polygons:
            continue
        m = o.matrix_world
        me = o.data
        tree = BVHTree.FromPolygons([m @ v.co for v in me.vertices], [p.vertices for p in me.polygons])
        names = {g.index: g.name for g in o.vertex_groups}
        bones = ch.arm.data.bones
        vw = [{names[e.group]: e.weight for e in v.groups if e.group in names and names[e.group] in bones and e.weight > 0}
              for v in me.vertices]
        out.append((me, tree, vw))
    return out


def _cast(srcs, co, into, prefix=""):
    """The nearest surface hit along `into` from `SEAT_LEAD` in front of `co`: (gap, loc,
    normal against `into`, weights at the hit) or None. A negative gap: `co` is inside."""
    best = None
    for me, tree, vw in srcs:
        loc, n, fi, t = tree.ray_cast(co - into * SEAT_LEAD, into, SEAT_LEAD + 0.25)
        if loc is not None and (best is None or t < best[0]):
            best = (t, loc, n, fi, me, vw)
    if best is None:
        return None
    t, loc, n, fi, me, vw = best
    poly = me.polygons[fi]
    inv = [1.0 / max((me.vertices[i].co - loc).length, 1e-5) for i in poly.vertices]
    s = sum(inv)
    w = {}
    for i, k in zip(poly.vertices, inv):
        for b, x in vw[i].items():
            w[b] = w.get(b, 0.0) + x * k / s
    if w and _free(max(w, key=w.get), prefix):
        return None  # the head or an arm in front of the trunk
    w = {b: x for b, x in w.items() if not _free(b, prefix)}
    return t - SEAT_LEAD, loc, (n if n.dot(into) < 0 else -n), w


def _norm(w, keep=4):
    top = sorted(w.items(), key=lambda kv: -kv[1])[:keep]
    tot = sum(x for _b, x in top) or 1.0
    return {b: x / tot for b, x in top if x / tot > 0.02}


def _gaps(obj, srcs, into, prefix=""):
    hits = [(i, _cast(srcs, v.co, into, prefix)) for i, v in enumerate(obj.data.vertices)]
    return [(i, h) for i, h in hits if h is not None]


def seat(ch, obj, world_matrix, into, gap=SEAT_GAP, soft=False, part=0, turn=True, skip=(), lag=True):
    """Seats carried gear `obj` (item frame) placed at `world_matrix` (rest pose) on the
    body's outermost surface: rays along `into` (world, towards the body) from its vertices
    find the clothes, skin and gear under it; the item turns (at most `SEAT_TURN`) so its
    contact side lies along the surface's mean slope and moves along `into` until its
    nearest vertex is `gap` off it. Skinned like the surface under it: `soft` each vertex
    with the weights under it (a knapsack bends with the spine), else every vertex with the
    contact area's blended weights (a rigid frame between spine2, spine1 and the shoulders).
    `lag`: the item's share of its main bone goes to an own `gear.*` bone (`lag_bone`) the
    game swings a little behind the body (`ingame/gear_view.rs`).
    Returns the bound object; `ch.seat_report[obj.name]` = (gap before, gap after) (m)."""
    into = Vector(into).normalized()
    obj.matrix_world = world_matrix
    bpy.context.view_layer.update()
    me = obj.data
    me.transform(obj.matrix_world)
    obj.matrix_world = Matrix.Identity(4)
    srcs = _seat_sources(ch, skip=(obj, *skip))
    move = Matrix.Identity(4)
    hits = _gaps(obj, srcs, into, ch.prefix)
    before = min((h[0] for _i, h in hits), default=None)
    if hits and turn:
        near = [h for _i, h in hits if h[0] < before + 0.03]
        n = sum((h[2] for h in near), Vector()).normalized()
        q = into.rotation_difference(-n)
        if q.angle > SEAT_TURN:
            q = Quaternion(q.axis, SEAT_TURN)
        c = sum((v.co for v in me.vertices), Vector()) / len(me.vertices)
        move = Matrix.Translation(c) @ q.to_matrix().to_4x4() @ Matrix.Translation(-c)
        me.transform(move)
        into = q @ into
        hits = _gaps(obj, srcs, into, ch.prefix)
    if hits:
        d = min(h[0] for _i, h in hits) - gap
        me.transform(Matrix.Translation(into * d))
        move = Matrix.Translation(into * d) @ move
        hits = [(i, (h[0] - d, h[1], h[2], h[3])) for i, h in hits]
    after = min((h[0] for _i, h in hits), default=None)
    # Weights: the contact area's blend (rigid), or the surface under each vertex (soft).
    contact = [h for _i, h in hits if h[0] < (after or 0.0) + 0.04] or [h for _i, h in hits]
    blend = {}
    for h in contact:
        for b, x in h[3].items():
            blend[b] = blend.get(b, 0.0) + x / len(contact)
    blend = _norm(blend) or {ch.prefix + "spine2": 1.0}
    per = {i: _norm(h[3]) for i, h in hits if h[3]} if soft else {}
    for g in list(obj.vertex_groups):
        if g.name in ch.arm.data.bones:
            obj.vertex_groups.remove(g)
    for i in range(len(me.vertices)):
        for b, x in (per.get(i) or blend).items():
            g = obj.vertex_groups.get(b) or obj.vertex_groups.new(name=b)
            g.add([i], x, "REPLACE")
    attr = me.attributes.get("part") or me.attributes.new("part", "INT", "POINT")
    for a in attr.data:
        a.value = part
    if lag:
        lag_bone(ch, obj, blend, into)
    if not hasattr(ch, "seat_report"):
        ch.seat_report, ch.seat_moves = {}, {}
    ch.seat_report[obj.name] = (before, after)
    ch.seat_moves[obj.name] = move
    print(f"seat {obj.name}: gap {before if before is None else round(before, 4)} -> "
          f"{after if after is None else round(after, 4)} m, frame {sorted(blend.items(), key=lambda kv: -kv[1])[:3]}")
    return am.bind(obj, arm_obj=ch.arm)


GEAR_BONE = "gear."  # prefix of the lag bones (`lag_bone`; the game finds them by it)


LAG_SHARE = 0.6  # of every vertex's weight the lag bone takes
LAG_TRUNK = ("hips", "spine", "spine1", "spine2")  # its parent: the trunk bone carrying most


def lag_bone(ch, obj, blend, into, share=LAG_SHARE):
    """Gives carried gear `obj` (bound, world rest pose, skinned by `seat`) an own deforming
    bone `gear.<item>` under the trunk bone that carries most of it (`blend`, the contact
    frame; a clavicle counts for spine2), keyed at rest by every clip so it follows that
    bone, and moves `share` of each vertex's weights onto it (the rest keeps its three
    largest): the game turns it about its head, the top of the item's contact side (where
    the strap holds it), so the item lags and bobs (`ingame/gear_view.rs`). Returns the
    bone's name."""
    me = obj.data
    into = Vector(into).normalized()
    cos = [v.co for v in me.vertices]
    c = sum(cos, Vector()) / len(cos)
    top = max(v.z for v in cos)
    contact = max(v.dot(into) for v in cos)
    head = c + Vector((0.0, 0.0, top - c.z)) + into * (contact - c.dot(into))
    pre = ch.prefix
    trunk = {}
    for b, x in blend.items():
        k = b[len(pre):]
        k = "spine2" if k.startswith("clavicle") else k
        if k in LAG_TRUNK:
            trunk[k] = trunk.get(k, 0.0) + x
    parent = pre + (max(trunk, key=trunk.get) if trunk else "spine2")
    name = pre + GEAR_BONE + obj.name.split(".", 1)[-1]
    arm_obj = ch.arm
    active = bpy.context.view_layer.objects.active
    bpy.context.view_layer.objects.active = arm_obj
    bpy.ops.object.mode_set(mode="EDIT")
    ebs = arm_obj.data.edit_bones
    eb = ebs.get(name) or ebs.new(name)
    eb.head, eb.tail = head, head + Vector((0.0, 0.0, -0.1))
    eb.parent, eb.use_deform = ebs[parent], True
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.context.view_layer.objects.active = active
    bones = arm_obj.data.bones
    groups = {g.index: g for g in obj.vertex_groups if g.name in bones}
    lag = obj.vertex_groups.get(name) or obj.vertex_groups.new(name=name)
    for v in me.vertices:
        ws = sorted(((groups[e.group], e.weight) for e in v.groups if e.group in groups), key=lambda gw: -gw[1])
        tot = sum(w for _g, w in ws[:3]) or 1.0
        for k, (g, w) in enumerate(ws):
            if k < 3:
                g.add([v.index], (1.0 - share) * w / tot, "REPLACE")
            else:
                g.remove([v.index])
        lag.add([v.index], share if ws else 1.0, "REPLACE")
    return name


def seat_offset(ch, obj, world_matrix, into, gap=SEAT_GAP):
    """The move along `into` that seats `obj` at `world_matrix` `gap` off the outermost
    surface (rigid gear on a socket bone: the slung musket); records the gaps in
    `ch.seat_report` like `seat`."""
    into = Vector(into).normalized()
    srcs = _seat_sources(ch, skip=(obj,))
    gaps = [h[0] for h in (_cast(srcs, world_matrix @ v.co, into, ch.prefix) for v in obj.data.vertices)
            if h is not None]
    if not gaps:
        return Vector()
    before = min(gaps)
    if not hasattr(ch, "seat_report"):
        ch.seat_report, ch.seat_moves = {}, {}
    ch.seat_report[obj.name] = (before, gap)
    print(f"seat {obj.name}: gap {round(before, 4)} -> {gap} m (socket moved)")
    return into * (before - gap)
