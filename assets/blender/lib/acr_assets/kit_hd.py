"""The `gear_hd` kit worn on an HD outfit (`outfit_hd`; m2-hd-kit-worn).

- **Tricorn** fitted to the HD head: the band slice of the skin `BAND_BELOW` under the crown
  of the head gives the crown's inner oval (plus `EASE`); a separate node on `socket.hat`
  named `<unit>.tricorn` (its levels `<unit>.lod<k>.tricorn`), so it drops at death (#206).
- **Crossbelts** (`lay_strap`): solid straps (`BELT_THICK`, box section, closed), each laid
  over the layer below it with a fixed offset, no cloth sim: the pouch belt left shoulder to
  right hip on the outfit, the waist (bayonet) belt right shoulder to left hip over the
  outfit and the pouch belt, the knapsack straps over both; one piece from the front hip over
  the shoulder to the back hip, a row every `STRAP_LEVELS` step with samples across the
  width, the inner face `BELT_OFFSET` off the surface along the strap's own (smoothed)
  normal, pits narrower than the strap bridged. Weights from the surface right under each
  section (`weigh_strap`: `drape.transfer_weights`, 4 bones), so the strap moves with it;
  `_PART` from the strongest trunk bone (a severed arm leaves the belts on the trunk).
  Levels are laid again with fewer samples, not decimated (a thin solid does not decimate).
- **Strapped items** (cartridge box at the right hip, bayonet and hanger scabbards with the
  hanger's hilt at the left hip, knapsack and canteen at the back, belt plate and buckle on
  the chest), seated by `seat`: pushed along the outward axis until every vertex lies `CLEAR`
  outside the trunk of the outfit and the body (rays from outside; the arms are left out of
  the test surface). Each rides one bone, except the knapsack (`ON_BACK`), which takes the
  coat's weights under it (`follow_back`) so it bends with the back. Their baked materials
  carry `acr_decay` from creation (`gear_hd.build_item(layer=...)`), since they carry a
  decay mask as `COLOR_0`.

`ctx.stats["kit_hd"]`: per item its bone and seat shift, the belts' clearance from the coat
per clip (`clearance`), item-in-item and outfit-in-item vertices per clip (`collisions`),
kit materials without `acr_decay` (`decay_missing`), the hat's fit.
"""

import math

ARM_PREFIXES = ("upperarm", "forearm", "hand", "thumb", "index", "middle", "ring", "pinky")
BAND_BELOW = 0.075  # m: hat band under the top of the head
EASE = 0.006  # m: crown ease round the band slice
CLEAR = 0.004  # m: strapped items this far outside the trunk
BELT_OFFSET = 0.004  # m: a strap's inner face off the layer below it
BELT_THICK = 0.003  # m: buff leather
BELT_W = 0.058
PACK_W = 0.04
# (samples across, row step m) of the master and its levels 1..3 (the master's twin first).
STRAP_LEVELS = ((5, 0.01), (5, 0.01), (3, 0.02), (2, 0.04))
UNDER_M = 0.001  # m: a belt vertex closer than this to the coat (or under it) counts


def _prefix(bone):
    """A shared armature's bone prefix (`rider.`, `c0.`), or ""."""
    return bone[:bone.index(".") + 1] if "." in bone and not bone.startswith("socket.") else ""


def is_arm(bone):
    """An arm or hand bone, with or without a shared armature's prefix (`rider.upperarm_l`)."""
    return bone[len(_prefix(bone)):].startswith(ARM_PREFIXES)


def trunk_weights(groups):
    """{bone: w} without the arm bones, normalised (`{"spine1": 1}`, with the groups' bone
    prefix, when nothing is left)."""
    w = {b: x for b, x in groups.items() if not is_arm(b) and x > 0.0}
    s = sum(w.values())
    if s <= 0.0:
        return {next((_prefix(b) for b in groups), "") + "spine1": 1.0}
    top = sorted(w.items(), key=lambda e: -e[1])[:4]
    s = sum(x for _b, x in top)
    return {b: x / s for b, x in top}


def resample(pts, step):
    """Points every `step` m along the polyline `pts` (3-tuples), both ends kept; returns
    (points, index of the source segment each lies on)."""
    seg = [math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1)]
    total = sum(seg)
    n = max(1, round(total / step))
    out, src, i, acc = [], [], 0, 0.0
    for k in range(n + 1):
        d = total * k / n
        while i < len(seg) - 1 and acc + seg[i] < d:
            acc += seg[i]
            i += 1
        t = 0.0 if seg[i] <= 0 else min(1.0, max(0.0, (d - acc) / seg[i]))
        out.append(tuple(a + (b - a) * t for a, b in zip(pts[i], pts[i + 1])))
        src.append(i)
    return out, src


def strap_faces(rows, across):
    """Faces of a solid strap: `rows` cross-sections of `across` inner then `across` outer
    vertices each (vertex `r * 2 * across + side * across + j`), walls round the section
    and a cap at both ends; closed (every edge in two faces)."""
    k = 2 * across

    def ring(r):  # round the section: outer left to right, inner right to left
        return [r * k + across + j for j in range(across)] + [r * k + j for j in reversed(range(across))]

    faces = []
    for r in range(rows - 1):
        a, b = ring(r), ring(r + 1)
        for j in range(k):
            faces.append((a[j], a[(j + 1) % k], b[(j + 1) % k], b[j]))
    faces.append(tuple(reversed(ring(0))))
    faces.append(tuple(ring(rows - 1)))
    return faces


def fill_pits(heights, nbrs, drop=0.004, passes=4):
    """Heights raised to at least the mean of their neighbours less `drop` (a strap bridges
    a gap between two layers instead of sinking into it)."""
    h = list(heights)
    for _ in range(passes):
        h = [max(x, sum(h[j] for j in nbrs[i]) / len(nbrs[i]) - drop) if nbrs[i] else x for i, x in enumerate(h)]
    return h


# ----------------------------------------------------------------------------- bpy helpers

def _dominant(ob):
    names = {g.index: g.name for g in ob.vertex_groups}
    return [names[max(v.groups, key=lambda e: e.weight).group] if v.groups else "" for v in ob.data.vertices]


def trunk_tree(objs):
    """BVH of the faces of `objs` (rest, world) whose vertices are not arm-weighted."""
    from mathutils.bvhtree import BVHTree

    V, F, base = [], [], 0
    for o in objs:
        dom = _dominant(o)
        mw = o.matrix_world
        V += [mw @ v.co for v in o.data.vertices]
        F += [[base + i for i in p.vertices] for p in o.data.polygons if not any(is_arm(dom[i]) for i in p.vertices)]
        base += len(o.data.vertices)
    return BVHTree.FromPolygons(V, F)


def surface_tree(objs):
    """BVH of all faces of `objs` (rest, world)."""
    from mathutils.bvhtree import BVHTree

    V, F, base = [], [], 0
    for o in objs:
        mw = o.matrix_world
        V += [mw @ v.co for v in o.data.vertices]
        F += [[base + i for i in p.vertices] for p in o.data.polygons]
        base += len(o.data.vertices)
    return BVHTree.FromPolygons(V, F)


def strap_path(tree, a, b, over_top=True, n=80):
    """Centre line of a strap from hip `b` up the front to shoulder `a`, over the top of the
    shoulder (`over_top`; rays round the shoulder in the YZ plane) and down the back to `b`:
    [(point on the surface, outward guide)]."""
    from mathutils import Vector

    def side(sign):
        out = []
        for i in range(n + 1):
            c = a + (b - a) * (i / n)
            hit = tree.ray_cast(Vector((c.x, sign * 1.0, c.z)), Vector((0, -sign, 0)), 2.0)[0]
            if hit is not None:
                out.append((hit.copy(), Vector((0, sign, 0))))
        return out

    front, back = side(-1), side(1)
    arc = []
    if over_top and front and back:
        o = Vector((a.x, (front[0][0].y + back[0][0].y) / 2, a.z - 0.06))
        for k in range(1, 16):
            th = math.radians(-90 + 180 * k / 16)
            d = Vector((0.0, math.sin(th), math.cos(th)))
            hit = tree.ray_cast(o + d * 0.5, -d, 0.5)[0]
            if hit is not None:
                arc.append((hit.copy(), d))
    return [*reversed(front), *arc, *back] if over_top else front


def ring_path(tree, centre, z, n=96, start_deg=-80.0, span_deg=340.0):
    """Centre line of a belt round the body at height `z` about the vertical through
    `centre` (x, y): rays in from outside every `span_deg / n` degrees from `start_deg`
    (0: -Y, the front; positive towards +X), [(point on the surface, outward guide)]. The
    ends are left open (`span_deg` < 360) where a buckle covers them."""
    from mathutils import Vector

    out = []
    for k in range(n + 1):
        th = math.radians(start_deg + span_deg * k / n)
        d = Vector((math.sin(th), -math.cos(th), 0.0))
        o = Vector((centre[0], centre[1], z))
        hit = tree.ray_cast(o + d * 0.6, -d, 0.6)[0]
        if hit is not None:
            out.append((hit.copy(), d))
    return out


def lay_strap(name, tree, a, b, width, offset, thick, across=5, step=0.01, over_top=True, path=None):
    """A solid strap (`thick`, box section, closed) laid over the surface of `tree`: its
    inner face `offset` off the surface along the strap's own normal, `across` samples over
    the width, a row every `step` m; pits narrower than the strap are bridged. `path`
    ([(point, outward guide)], e.g. `ring_path`) replaces the shoulder-to-hip line `a`, `b`."""
    import bpy
    from mathutils import Vector

    from . import body as bd

    path = path or strap_path(tree, a, b, over_top)
    pts, src = resample([tuple(p) for p, _g in path], step)
    guides = [path[i][1] for i in src]
    P = [Vector(p) for p in pts]
    N = []
    for p, g in zip(P, guides):
        _loc, nrm, _i, _d = tree.find_nearest(p)
        nrm = nrm if nrm is not None else g
        N.append(nrm if nrm.dot(g) >= 0 else -nrm)
    R = len(P)
    sm = []
    for k in range(R):  # the strap's normal: smooth along it, so it does not follow folds
        acc = Vector()
        for j in range(max(0, k - 4), min(R, k + 5)):
            acc += N[j]
        sm.append(acc.normalized())
    us = [(j / (across - 1) - 0.5) * width for j in range(across)]
    base, frame = [], []
    for k in range(R):
        t = (P[min(R - 1, k + 1)] - P[max(0, k - 1)]).normalized()
        n = (sm[k] - t * sm[k].dot(t)).normalized()
        s = t.cross(n).normalized()
        frame.append((n, s))
        for u in us:
            q = P[k] + s * u
            hit = tree.ray_cast(q + n * 0.08, -n, 0.16)[0]
            base.append((q, (hit - q).dot(n) if hit is not None else None))
    known = [h for _q, h in base if h is not None]
    floor = min(known) if known else 0.0
    hs = [floor if h is None else h for _q, h in base]
    nbrs = []
    for k in range(R):
        for j in range(across):
            nb = [(k + dk) * across + j + dj for dk, dj in ((-1, 0), (1, 0), (0, -1), (0, 1))
                  if 0 <= k + dk < R and 0 <= j + dj < across]
            nbrs.append(nb)
    hs = fill_pits(hs, nbrs)
    verts = []
    for k in range(R):
        n, _s = frame[k]
        row = [base[k * across + j][0] + n * (hs[k * across + j] + offset) for j in range(across)]
        verts += [tuple(v) for v in row] + [tuple(v + n * thick) for v in row]
    m = bpy.data.meshes.new(name)
    m.from_pydata(verts, [], strap_faces(R, across))
    m.validate()
    ob = bd.link(bpy.data.objects.new(name, m))
    for p in m.polygons:
        p.use_smooth = True
    return ob


def weigh_strap(ob, outfit, under, across):
    """Each section's inner vertex takes the weights of the surface right under it (the
    outfit or a strap laid before, `drape.transfer_weights`, 4 bones), its outer twin the
    same; returns the per-vertex weights."""
    from . import drape

    ws = drape.transfer_weights(ob, outfit, None, keep=4, under=under)
    k = 2 * across
    for i in range(len(ws)):
        if (i % k) >= across:
            ws[i] = ws[i - across]
    ob.vertex_groups.clear()
    groups = {}
    for i, w in enumerate(ws):
        for bname, x in sorted(w.items()):
            if bname not in groups:
                groups[bname] = ob.vertex_groups.new(name=bname)
            groups[bname].add([i], x, "REPLACE")
    return ws


def seat(objs, tree, matrix, axis, clear=CLEAR):
    """Places `objs` (a master and its levels, rest at the origin) by `matrix`, then moves
    them along `axis` so the master's vertices lie `clear` outside `tree` (`tree` None: no
    move); returns (shift, the final matrix)."""
    from mathutils import Matrix, Vector

    a = Vector(axis).normalized()
    need = None
    if tree is not None:
        for v in objs[0].data.vertices:
            p = matrix @ v.co
            hit = tree.ray_cast(p + a * 0.6, -a, 1.2)[0]
            if hit is not None:
                s = (hit - p).dot(a) + clear
                need = s if need is None else max(need, s)
    shift = need or 0.0
    full = Matrix.Translation(a * shift) @ matrix
    for o in objs:
        o.data.transform(full)
        o.matrix_world = Matrix.Identity(4)
    return round(shift, 4), full


def rigid(ob, arm, bone):
    """Skins `ob` wholly to `bone`."""
    from . import armature as am

    ob.vertex_groups.clear()
    g = ob.vertex_groups.new(name=bone)
    g.add(list(range(len(ob.data.vertices))), 1.0, "REPLACE")
    am.bind(ob, arm)


def follow_back(ob, arm, src):
    """Skins `ob` (a soft pack on the back, rest, world) with the weights of `src` (the
    outfit) at the same height and width on the side of `ob` facing it: the pack bends with
    the back under it, as the coat does, instead of riding one bone into the hips in a deep
    bend. Only the spine's bones (`BACK_BONES`): with the clavicles or the sleeves the top
    corners swung 0.15 m into the coat when aiming."""
    from mathutils import Vector
    from mathutils.bvhtree import BVHTree

    from . import armature as am

    me = src.data
    mw = src.matrix_world
    tree = BVHTree.FromPolygons([mw @ v.co for v in me.vertices], [p.vertices for p in me.polygons])
    names = {g.index: g.name for g in src.vertex_groups}
    vw = [{names[e.group]: e.weight for e in v.groups} for v in me.vertices]
    inner = min(v.co.y for v in ob.data.vertices)
    ob.vertex_groups.clear()
    groups = {}
    for v in ob.data.vertices:
        loc, _n, fi, _d = tree.find_nearest(Vector((v.co.x, inner, v.co.z)))
        w = {}
        if fi is not None:
            poly = me.polygons[fi]
            inv = [1.0 / max((mw @ me.vertices[i].co - loc).length, 1e-5) for i in poly.vertices]
            for i, k in zip(poly.vertices, inv):
                for b, x in vw[i].items():
                    w[b] = w.get(b, 0.0) + x * k / sum(inv)
        w = {b: x for b, x in w.items() if b in BACK_BONES}
        tot = sum(w.values())
        w = {b: x / tot for b, x in w.items()} if tot > 0 else {"spine2": 1.0}
        for b, x in sorted(trunk_weights(w).items()):
            if b not in groups:
                groups[b] = ob.vertex_groups.new(name=b)
            groups[b].add([v.index], x, "REPLACE")
    am.bind(ob, arm)


def strap_gore(ob):
    """Decay mask and `_gore` of a strap; `_part` from its strongest trunk bone (a severed
    arm leaves the belts on the trunk)."""
    from . import decay, rig

    names = {g.index: g.name for g in ob.vertex_groups}
    parts = []
    for v in ob.data.vertices:
        w = trunk_weights({names[e.group]: e.weight for e in v.groups})
        parts.append(rig.part_of_bone(max(w, key=w.get)))
    decay.add_masks(ob, "leather", seed=sum(map(ord, ob.name)), part_of_vertex=parts.__getitem__)


def head_band(body):
    """(centre x, centre y, band z, half width, half depth) of the bald head at the hat band."""
    import numpy as np

    co = np.array([v.co[:] for v in body.data.vertices])
    head = co[(np.abs(co[:, 0]) < 0.13) & (co[:, 2] > 1.5)]
    top = head[:, 2].max()
    z = top - BAND_BELOW
    s = head[np.abs(head[:, 2] - z) < 0.006]
    cx = float((s[:, 0].max() + s[:, 0].min()) / 2)
    cy = float((s[:, 1].max() + s[:, 1].min()) / 2)
    return cx, cy, float(z), float(s[:, 0].max() - cx), float(s[:, 1].max() - cy), float(top)


def _material(name, rgb, layer, metal=0.0, rough=0.8):
    import bpy

    from . import decay

    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bs = m.node_tree.nodes["Principled BSDF"]
    bs.inputs["Base Color"].default_value = (*rgb, 1.0)
    bs.inputs["Metallic"].default_value = metal
    bs.inputs["Roughness"].default_value = rough
    m["acr_decay"] = decay.extras(layer)
    return m


# ----------------------------------------------------------------------------- worn kit

# item -> (bone, rotation (Euler degrees, order ZXY: turned about Z first, then tilted about
# X), centre, outward axis). The scabbards hang flat on the hip, tips back.
ITEMS = {
    "cartridge_box": ("hips", (0.0, 0.0, 19.0), (-0.12, 0.10, 0.93), (-0.35, 1.0, 0.0)),
    "hanger_scabbard": ("hips", (28.0, 0.0, 90.0), (0.17, 0.05, 0.93), (1.0, 0.25, 0.0)),
    "bayonet_scabbard": ("hips", (22.0, 0.0, 90.0), (0.16, -0.01, 0.92), (1.0, 0.1, 0.0)),
    "knapsack": ("spine2", (0.0, 0.0, 0.0), (0.0, 0.16, 1.24), (0.0, 1.0, 0.0)),
    "canteen": ("hips", (0.0, 0.0, 0.0), (0.10, 0.16, 0.80), (0.0, 1.0, 0.0)),  # spout clear under the pack
}
BACK_BONES = ("hips", "spine", "spine1", "spine2")
ON_BACK = ("knapsack",)  # skinned from the coat under it (`follow_back`), not one bone
FOLLOW = {"hanger_hilt": ("hanger_scabbard", (0.0, 0.0, 0.012))}  # seated with another item


def wear(ctx, arm, body, outfit, levels_of_unit=3):
    """Builds and wears the kit (rest pose); returns the objects made (masters and levels)."""
    import bpy
    from mathutils import Euler, Matrix, Vector

    from . import armature as am
    from . import gear_hd, outfit_hd, straps

    st = ctx.stats.setdefault("kit_hd", {})
    arm.data.pose_position = "REST"
    bpy.context.view_layer.update()
    made = []
    # Tricorn.
    cx, cy, z, hx, hy, top = head_band(body)
    rx, ry = hx + EASE, hy + EASE
    crown = (top - z) + 0.035
    hat, hat_lods = gear_hd.build_item(ctx, "tricorn", rx=rx, ry=ry, crown=crown)

    at = Matrix.Translation((cx, cy, z)) @ Euler((0.0, 0.0, 0.0)).to_matrix().to_4x4()
    for o in [hat, *hat_lods]:
        am.attach(o, arm, "socket.hat", at)
    st["tricorn"] = {"band_z": round(z, 3), "crown_rx_ry": [round(rx, 3), round(ry, 3)], "crown_h": round(crown, 3),
                     "head_top": round(top, 3)}
    made += [hat, *hat_lods]
    # Belts on the coat: solid straps, each laid over the layer below it (the outfit, then
    # the straps laid before) at a fixed offset, weights from that layer (4 bones).
    buff = _material(f"{ctx.name}.belts.hd", (0.74, 0.66, 0.48), "leather", rough=0.85)
    specs = [(straps.LEFT_SHOULDER, straps.RIGHT_HIP, BELT_W), (straps.RIGHT_SHOULDER, straps.LEFT_HIP, BELT_W)]
    # The pack straps end on the chest above the armpit: further down the sleeve swings
    # through them (0.21 m deep in DEATH with the old end at x 0.17, z 1.22).
    specs += [(Vector((0.09 * sx, 0, 1.50)), Vector((0.135 * sx, 0, 1.30)), PACK_W) for sx in (1, -1)]
    levels = {}
    for level, (across, step) in enumerate(STRAP_LEVELS):
        laid = []
        for i, (frm, to, w) in enumerate(specs):
            tree = surface_tree([outfit, *laid])
            b = lay_strap(f"{ctx.name}.strap{level}_{i}", tree, frm, to, w, BELT_OFFSET, BELT_THICK, across, step)
            weigh_strap(b, outfit, laid, across)
            laid.append(b)
        for b in laid:
            b.data.materials.append(buff)
        ob, _w = outfit_hd.join(f"{ctx.name}.belts" if level == 0 else f"{ctx.name}.lod{level}.belts", laid, arm)
        levels[level] = ob
    belt = levels[0]
    belt_lods = []
    for level in sorted(levels)[1:]:
        lo = levels[level]
        lo["acr_lod"] = level
        lo["acr_lod_of"] = belt.name
        belt_lods.append(lo)
    for o in (belt, *belt_lods):
        strap_gore(o)
    st["belts_tris"] = [len(belt.data.polygons)] + [len(lo.data.polygons) for lo in belt_lods]
    st["belt_weights"] = _w
    made += [belt, *belt_lods]
    # Strapped items.
    tree = trunk_tree([outfit, body])
    belt_tree = trunk_tree([outfit, body, belt])
    seats = {}
    cross_z = straps.LEFT_SHOULDER.z + (straps.RIGHT_HIP.z - straps.LEFT_SHOULDER.z) * 0.12 / 0.29
    placed = dict(ITEMS)
    placed["belt_plate"] = ("spine1", (0.0, 0.0, 0.0), (0.0, -0.10, cross_z), (0.0, -1.0, 0.0))
    ang = math.degrees(math.atan2(straps.LEFT_SHOULDER.z - straps.RIGHT_HIP.z, straps.LEFT_SHOULDER.x - straps.RIGHT_HIP.x))
    bx = straps.LEFT_SHOULDER.x + (straps.RIGHT_HIP.x - straps.LEFT_SHOULDER.x) * 0.22
    bz = straps.LEFT_SHOULDER.z + (straps.RIGHT_HIP.z - straps.LEFT_SHOULDER.z) * 0.22
    placed["buckle"] = ("spine2", (0.0, 90.0 - ang, 0.0), (bx, -0.10, bz), (0.0, -1.0, 0.0))
    finals, items = {}, {}
    for key in [*placed, *FOLLOW]:
        ob, lods = gear_hd.build_item(ctx, key, layer=gear_hd.LAYERS[key])
        if key in FOLLOW:
            other, offset = FOLLOW[key]
            bone, axis = placed[other][0], (0.0, 0.0, 1.0)
            shift, _full = seat([ob, *lods], None, finals[other] @ Matrix.Translation(offset), axis)
        else:
            bone, rot, centre, axis = placed[key]
            m = Matrix.Translation(centre) @ Euler([math.radians(r) for r in rot], "ZXY").to_matrix().to_4x4()
            shift, finals[key] = seat([ob, *lods], belt_tree if key in ("belt_plate", "buckle") else tree, m, axis)
        for o in [ob, *lods]:  # the materials carry `acr_decay` from `gear_hd.finish`
            if key in ON_BACK:
                follow_back(o, arm, outfit)
            else:
                rigid(o, arm, bone)
            outfit_hd.gore(o, gear_hd.LAYERS[key])
        seats[key] = {"bone": "coat" if key in ON_BACK else bone, "shift_m": shift}
        items[key] = ob
        made += [ob, *lods]
    st["items"] = seats
    arm.data.pose_position = "POSE"
    return made, belt, items


def clearance(arm, belt, outfit, clips, steps=6):
    """Belt vertices under (or within `UNDER_M` of) the coat per clip, sampled `steps` times."""
    import bpy
    from mathutils.bvhtree import BVHTree

    from . import armature as am

    from mathutils.geometry import barycentric_transform

    def tri_co(dg):
        me = outfit.evaluated_get(dg).data
        me.calc_loop_triangles()
        ow = outfit.matrix_world
        co = [ow @ v.co for v in me.vertices]
        return co, [tuple(t.vertices) for t in me.loop_triangles]

    # Each belt vertex is tied to the coat triangle under it at rest (the point on it and the
    # side it lies on), then measured against that same triangle in every pose: a layer
    # check, not "the nearest cloth", so a sleeve swinging past does not count.
    arm.data.pose_position = "REST"
    bpy.context.view_layer.update()
    co, tris = tri_co(bpy.context.evaluated_depsgraph_get())
    tree = BVHTree.FromPolygons(co, tris)
    anchor = []
    for v in belt.data.vertices:
        p = belt.matrix_world @ v.co
        hit, nrm, ti, _d = tree.find_nearest(p, 0.05)
        anchor.append(None if hit is None else (ti, hit.copy(), math.copysign(1.0, (p - hit).dot(nrm))))
    arm.data.pose_position = "POSE"
    rest = [co[i] for t in tris for i in t]
    out = {}
    worst = (1.0, None)
    for clip in clips:
        if clip not in bpy.data.actions:
            continue
        f0, f1 = (int(round(x)) for x in bpy.data.actions[clip].frame_range)
        n_under, lo = 0, 1.0
        for f in sorted({int(round(f0 + (f1 - f0) * k / max(1, steps - 1))) for k in range(steps)}):
            am.set_clip(arm, clip, f)
            dg = bpy.context.evaluated_depsgraph_get()
            pco, _t = tri_co(dg)
            bm_ = belt.evaluated_get(dg).data
            bw = belt.matrix_world
            for v in bm_.vertices:
                a = anchor[v.index]
                if a is None:
                    continue
                ti, hit, side = a
                A, B, C = (pco[i] for i in tris[ti])
                q = barycentric_transform(hit, *rest[3 * ti:3 * ti + 3], A, B, C)
                n = (B - A).cross(C - A)
                if n.length < 1e-12:
                    continue
                s = (bw @ v.co - q).dot(n.normalized()) * side
                lo = min(lo, s)
                if s < UNDER_M:
                    n_under += 1
            if lo < worst[0]:
                worst = (lo, [clip, f])
        out[clip] = {"under": n_under, "min_m": round(lo, 4)}
    arm.animation_data.action = None
    return {"clips": out, "under_total": sum(c["under"] for c in out.values()), "steps": steps,
            "min_m": round(worst[0], 4), "worst": worst[1], "belt_verts": len(belt.data.vertices)}


# Item pairs built to touch (the hilt sits in its scabbard's throat, the scabbards share the
# frog on the waist belt).
TOUCHING = {("hanger_hilt", "hanger_scabbard"), ("bayonet_scabbard", "hanger_scabbard"), ("bayonet_scabbard", "hanger_hilt")}
INSIDE_M = 0.003  # m: a vertex this deep inside another item counts


def collisions(arm, items, outfit, clips, steps=6):
    """Strapped items inside each other, and the outfit inside them, per clip: the vertices of
    one item (master) deeper than `INSIDE_M` inside another (nearest face's normal), sampled
    `steps` times per clip; pairs in `TOUCHING` are left out."""
    import bpy
    import numpy as np
    from mathutils import Vector
    from mathutils.bvhtree import BVHTree

    from . import armature as am

    keys = sorted(items)
    out, worst = {}, (0.0, None)
    for clip in clips:
        if clip not in bpy.data.actions:
            continue
        f0, f1 = (int(round(x)) for x in bpy.data.actions[clip].frame_range)
        pairs, deep = {}, 0.0
        for f in sorted({int(round(f0 + (f1 - f0) * k / max(1, steps - 1))) for k in range(steps)}):
            am.set_clip(arm, clip, f)
            dg = bpy.context.evaluated_depsgraph_get()
            pos, trees = {}, {}
            for k in [*keys, "outfit"]:
                o = outfit if k == "outfit" else items[k]
                me = o.evaluated_get(dg).data
                co = np.empty(len(me.vertices) * 3)
                me.vertices.foreach_get("co", co)
                mw = np.array(o.matrix_world)
                pos[k] = co.reshape(-1, 3) @ mw[:3, :3].T + mw[:3, 3]
                if k != "outfit":
                    trees[k] = BVHTree.FromPolygons(pos[k].tolist(), [p.vertices for p in me.polygons])
            for kb in keys:
                lo, hi = pos[kb].min(0) - 0.01, pos[kb].max(0) + 0.01
                for ka in [*keys, "outfit"]:
                    if ka == kb or (ka, kb) in TOUCHING or (kb, ka) in TOUCHING:
                        continue
                    n = 0
                    near = pos[ka][np.all((pos[ka] >= lo) & (pos[ka] <= hi), axis=1)]
                    for p in near:
                        p = Vector(p)
                        loc, nrm, _i, _d = trees[kb].find_nearest(p)
                        if loc is None:
                            continue
                        s = (p - loc).dot(nrm)
                        if s < -INSIDE_M:
                            n += 1
                            deep = min(deep, s)
                    if n:
                        key = f"{ka}>{kb}"
                        pairs[key] = max(pairs.get(key, 0), n)
            if deep < worst[0]:
                worst = (deep, [clip, f])
        out[clip] = {"pairs": pairs, "verts": sum(v for k, v in pairs.items() if not k.startswith("outfit")),
                     "outfit_verts": sum(v for k, v in pairs.items() if k.startswith("outfit")), "deepest_m": round(deep, 4)}
    arm.animation_data.action = None
    return {"clips": out, "item_verts": sum(c["verts"] for c in out.values()),
            "outfit_verts": sum(c["outfit_verts"] for c in out.values()), "deepest_m": round(worst[0], 4), "worst": worst[1]}
