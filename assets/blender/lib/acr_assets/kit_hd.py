"""The `gear_hd` kit worn on an HD outfit (`outfit_hd`; m2-hd-kit-worn).

- **Tricorn** fitted to the HD head: the band slice of the skin `BAND_BELOW` under the crown
  of the head gives the crown's inner oval (plus `EASE`); a separate node on `socket.hat`
  named `<unit>.tricorn` (its levels `<unit>.lod<k>.tricorn`), so it drops at death (#206).
- **Crossbelts** laid on the outfit (`body.strap`, rays along Y): the pouch belt left
  shoulder to right hip `BELT_OFFSETS[0]` off the coat, the waist (bayonet) belt right
  shoulder to left hip over it; the knapsack straps over both shoulders. Weights from the
  outfit under each vertex with the arm bones taken out (a severed arm leaves the belts on
  the trunk); `_PART`, `_GORE` and the decay mask from those weights.
- **Strapped items** (cartridge box at the right hip, bayonet and hanger scabbards with the
  hanger's hilt at the left hip, knapsack and canteen at the back, belt plate and buckle on
  the chest), each skinned rigidly to one bone and seated by `seat`: pushed along its outward
  axis until every vertex lies `CLEAR` outside the trunk of the outfit and the body (rays from
  outside; the arms are left out of the test surface).

`ctx.stats["kit_hd"]`: per item its bone and seat shift, the belts' clearance from the coat
per clip (`clearance`), the hat's fit.
"""

import math

ARM_PREFIXES = ("upperarm", "forearm", "hand", "thumb", "index", "middle", "ring", "pinky")
BAND_BELOW = 0.075  # m: hat band under the top of the head
EASE = 0.006  # m: crown ease round the band slice
CLEAR = 0.004  # m: strapped items this far outside the trunk
BELT_OFFSETS = (0.005, 0.010)  # m: pouch belt, then waist belt over it, off the coat
BELT_W = 0.058
UNDER_M = 0.001  # m: a belt vertex closer than this to the coat (or under it) counts


def is_arm(bone):
    return bone.startswith(ARM_PREFIXES)


def trunk_weights(groups):
    """{bone: w} without the arm bones, normalised (`{"spine1": 1}` when nothing is left)."""
    w = {b: x for b, x in groups.items() if not is_arm(b) and x > 0.0}
    s = sum(w.values())
    if s <= 0.0:
        return {"spine1": 1.0}
    top = sorted(w.items(), key=lambda e: -e[1])[:4]
    s = sum(x for _b, x in top)
    return {b: x / s for b, x in top}


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


def inherit(strap, src):
    """The weights of the vertex of `src` nearest each `strap` vertex, arm bones removed."""
    from mathutils.bvhtree import BVHTree

    me = src.data
    tree = BVHTree.FromPolygons([v.co for v in me.vertices], [p.vertices for p in me.polygons])
    names = {g.index: g.name for g in src.vertex_groups}
    strap.vertex_groups.clear()
    groups = {}
    for v in strap.data.vertices:
        loc, _n, fi, _d = tree.find_nearest(v.co)
        if fi is None:
            w = {"spine1": 1.0}
        else:
            vi = min(me.polygons[fi].vertices, key=lambda i: (me.vertices[i].co - loc).length)
            w = trunk_weights({names[e.group]: e.weight for e in me.vertices[vi].groups})
        for b, x in w.items():
            if b not in groups:
                groups[b] = strap.vertex_groups.new(name=b)
            groups[b].add([v.index], x, "REPLACE")


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


def _decimated_levels(ob, ratios=(1.0, 0.5, 0.25)):
    import bpy

    from . import body as bd

    out = []
    for level, r in enumerate(ratios, 1):
        c = ob.copy()
        c.data = ob.data.copy()
        base, _, key = ob.name.rpartition(".")
        c.name = c.data.name = f"{base}.lod{level}.{key}"
        bd.link(c)
        if r < 1.0:
            d = c.modifiers.new("dec", "DECIMATE")
            d.ratio = r
            bpy.context.view_layer.objects.active = c
            c.modifiers.move(len(c.modifiers) - 1, 0)
            bpy.ops.object.modifier_apply(modifier="dec")
        c["acr_lod"] = level
        c["acr_lod_of"] = ob.name
        out.append(c)
    return out


# ----------------------------------------------------------------------------- worn kit

# item -> (bone, rotation (Euler degrees, order ZXY: turned about Z first, then tilted about
# X), centre, outward axis). The scabbards hang flat on the hip, tips back.
ITEMS = {
    "cartridge_box": ("hips", (0.0, 0.0, 19.0), (-0.12, 0.10, 0.93), (-0.35, 1.0, 0.0)),
    "hanger_scabbard": ("hips", (28.0, 0.0, 90.0), (0.17, 0.05, 0.93), (1.0, 0.25, 0.0)),
    "bayonet_scabbard": ("hips", (22.0, 0.0, 90.0), (0.16, -0.01, 0.92), (1.0, 0.1, 0.0)),
    "knapsack": ("spine2", (0.0, 0.0, 0.0), (0.0, 0.16, 1.24), (0.0, 1.0, 0.0)),
    "canteen": ("hips", (0.0, 0.0, 0.0), (0.10, 0.16, 0.90), (0.0, 1.0, 0.0)),
}
FOLLOW = {"hanger_hilt": ("hanger_scabbard", (0.0, 0.0, 0.012))}  # seated with another item


def wear(ctx, arm, body, outfit, levels_of_unit=3):
    """Builds and wears the kit (rest pose); returns the objects made (masters and levels)."""
    import bpy
    from mathutils import Euler, Matrix, Vector

    from . import body as bd
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
    from . import armature as am

    at = Matrix.Translation((cx, cy, z)) @ Euler((0.0, 0.0, 0.0)).to_matrix().to_4x4()
    for o in [hat, *hat_lods]:
        am.attach(o, arm, "socket.hat", at)
    st["tricorn"] = {"band_z": round(z, 3), "crown_rx_ry": [round(rx, 3), round(ry, 3)], "crown_h": round(crown, 3),
                     "head_top": round(top, 3)}
    made += [hat, *hat_lods]
    # Belts on the coat.
    buff = _material(f"{ctx.name}.belts.hd", (0.74, 0.66, 0.48), "leather", rough=0.85)
    belts = []
    for i, (frm, to, off) in enumerate(((straps.LEFT_SHOULDER, straps.RIGHT_HIP, BELT_OFFSETS[0]),
                                        (straps.RIGHT_SHOULDER, straps.LEFT_HIP, BELT_OFFSETS[1]))):
        b = bd.strap(arm, f"{ctx.name}.belt{i}", outfit, frm, to, BELT_W, off, samples=36)
        belts.append(b)
    for sx in (1, -1):  # knapsack straps over the shoulders
        b = bd.strap(arm, f"{ctx.name}.packstrap{sx}", outfit, Vector((0.09 * sx, 0, 1.50)), Vector((0.17 * sx, 0, 1.22)),
                     0.04, BELT_OFFSETS[1] + 0.004, samples=14)
        belts.append(b)
    for b in belts:
        inherit(b, outfit)
        b.data.materials.clear()
        b.data.materials.append(buff)
        for p in b.data.polygons:
            p.use_smooth = True
    belt, _w = outfit_hd.join(f"{ctx.name}.belts", belts, arm)
    if "part" in belt.data.attributes:
        belt.data.attributes.remove(belt.data.attributes["part"])
    outfit_hd.gore(belt, "leather")
    belt_lods = _decimated_levels(belt)
    for lo in belt_lods:
        outfit_hd.gore(lo, "leather")
    st["belts_tris"] = [len(belt.data.polygons)] + [len(lo.data.polygons) for lo in belt_lods]
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
    from . import decay

    finals = {}
    for key in [*placed, *FOLLOW]:
        ob, lods = gear_hd.build_item(ctx, key)
        if key in FOLLOW:
            other, offset = FOLLOW[key]
            bone, axis = placed[other][0], (0.0, 0.0, 1.0)
            shift, _full = seat([ob, *lods], None, finals[other] @ Matrix.Translation(offset), axis)
        else:
            bone, rot, centre, axis = placed[key]
            m = Matrix.Translation(centre) @ Euler([math.radians(r) for r in rot], "ZXY").to_matrix().to_4x4()
            shift, finals[key] = seat([ob, *lods], belt_tree if key in ("belt_plate", "buckle") else tree, m, axis)
        for o in [ob, *lods]:
            rigid(o, arm, bone)
            outfit_hd.gore(o, "leather")
            for mt in o.data.materials:
                mt["acr_decay"] = decay.extras("metal" if key in ("belt_plate", "buckle") else "leather")
        seats[key] = {"bone": bone, "shift_m": shift}
        made += [ob, *lods]
    st["items"] = seats
    arm.data.pose_position = "POSE"
    return made, belt


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
