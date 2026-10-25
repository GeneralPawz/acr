"""Worn and saddle kit for the mounted units (SPEC `kit`, m2-cav-kit): the foot units' kit
rules (`line_kit`, `kit_hd`) on the rider of `cavalry.make` and on his horse.

- **Straps** (`carbine_belt`, `waist_belt`): solid straps over the rider's coat
  (`kit_hd.lay_strap`, the inner face `kit_hd.BELT_OFFSET` off it, the inner face dropped),
  weighted from the garment under each section without the arm bones (`line_kit._weigh`,
  at most 4). The carbine belt runs left shoulder to right hip, a steel swivel at its end
  (the carbine rides in the right fist, `cavalry.gun`); the waist belt goes round the
  waist (`kit_hd.ring_path`), a buckle over its open ends. They replace the painted-on
  ribbon `belt` (`bd.strap`).
- **Rider's items** (`pouch`, `scabbard`), rigid on the rider's `hips`: the cartridge
  pouch on the waist belt over the right hip (seated on the coat, `line_kit.seat_back`);
  the sword's scabbard (`gear_hd.scabbard_mesh`) from the left of the waist belt, back and
  down at `SCABBARD_TILT` along the horse's left flank, seated `GAP` outside the hide,
  saddle, saddlecloth and the rider's own leg in the seated pose. The hilt (`gear_hd`
  hanger hilt) sits in the scabbard's mouth on `socket.hip_l`, so the melee clips hide it
  with the sheathed blade while the drawn one shows (`cavalry.SHOW_MELEE`); it replaces the
  box sabre of `gear.sabre`. The scabbard hangs on its own bone (`hang`: it goes with the
  hips at the belt's hook but keeps its angle against the horse, swung clear per clip).
  They go with the rider when he is thrown.
- **Saddle kit** (`portmanteau`, `roll`): the valise across the croup behind the cantle and
  the rolled blanket on it, each laid down onto the surface under it slice by slice
  (`sag`: `SADDLE_GAP` off the hide, the roll off the valise) in `STAND`, rigid on their
  own horse bone keyed on every frame from the croup under them (`follow_croup`). They
  stay with the horse.
- **LODs:** no `acr_lod` nodes (as the line kit); every material through the unit's keys
  (`MAT_KEY`), so `acr_decay` and the decay masks apply as to the other gear.

`ctx.stats["kit"]`: triangles and placement per piece, and `collisions` per clip: the
rider's kit against the coat, hide, saddle and saddlecloth, the saddle kit against the hide
and the rider (`collide`). With no `kit` the same check runs on the old sabre box.
"""

import math

# Per kind of rider. 18th-century dragoons: carbine belt and swivel, waist belt with the
# sabre, pouch, valise and rolled blanket behind the saddle (British 1751 and 1768
# warrants, French 1750s ordonnances: the cloak or blanket rolled behind the cantle). The
# 17th-century dragoon: the same over his buff coat (the carbine on a broad belt and
# swivel). The reiter: no carbine; his pistols in the saddle holsters.
KITS = {
    "d18": ("carbine_belt", "waist_belt", "pouch", "scabbard", "portmanteau", "roll"),
    "d17": ("carbine_belt", "waist_belt", "pouch", "scabbard", "portmanteau", "roll"),
    "rei": ("waist_belt", "pouch", "scabbard", "portmanteau"),
}
PIECES = ("carbine_belt", "waist_belt", "pouch", "scabbard", "portmanteau", "roll")
SADDLE_PIECES = ("portmanteau", "roll")
# Source material of the meshes -> cavalry palette key.
MAT_KEY = {"leather": "tack", "buff": "belts", "brass": "brass", "steel": "metal", "iron": "metal",
           "wood": "wood", "canvas": "saddlecloth", "cord": "belts", "walnut": "wood"}
TRIS = {"pouch": 200, "scabbard": 160, "hilt": 140, "swivel": 80, "buckle": 60, "portmanteau": 1200, "roll": 900}  # the valise and roll whole: decimated they crumpled
CARBINE_W, WAIST_W = 0.070, 0.045  # m: a dragoon's broad carbine belt; the sword belt
WAIST_Z = 1.00  # m above the rider's origin: the waist belt's centre line
GAP = 0.005  # m: an item off the surface it rests on
SADDLE_GAP = 0.008  # m: the valise off the hide (the croup works under it in the gaits)
SCABBARD = {"sabre": (0.88, 0.040), "sword": (0.82, 0.036)}  # length, width at the mouth
SCABBARD_TILT = (40.0, -12.0)  # degrees: the tip back (X), and out from the flank (Y)
# Valise and roll (m): length across the horse, radius.
PORTMANTEAU = (0.50, 0.095)
ROLL = (0.58, 0.060)
VALISE_BONE = "kit_valise"  # the saddle kit's horse bone, keyed from the croup (`follow_croup`)
PATCH_MARGIN = 0.06  # m: the croup patch reaches this far round the kit's footprint
PATCH_DEPTH = 0.10  # m: ... and this far below its lowest point
LIFT_CLEAR = 0.004  # m: the least gap kept under the valise in every frame
HANGER_BONE = "kit_scabbard"  # the scabbard's hanger (rider bone, `hang`)
HOOK = (-0.015, 0.0, 0.07)  # m: its hook on the waist belt from the seated mouth (rest)
STRAP = 0.15  # m: the furthest the carriage straps let the scabbard out from the flank
HANG_OUT = (0.0, 10.0, 20.0, 30.0)  # degrees it may swing out about the hook ...
HANG_COST = 0.003  # m of push one degree of swing is worth (the least of both wins)
BEND = 0.015  # m: a stuffed valise bends at most this much over the croup (`sag`)
INSIDE_M = 0.003  # m: a vertex this deep inside a surface counts (`kit_hd.INSIDE_M`)


def check(kit):
    unknown = [p for p in kit or () if p not in PIECES]
    if unknown:
        raise KeyError(f"unknown cavalry kit pieces {unknown} (have {PIECES})")
    return tuple(kit or ())


def material_keys(slot_names):
    return [MAT_KEY[n.rsplit(".", 1)[-1]] for n in slot_names]


def sag_shifts(columns, clear, bend=None):
    """Per slice: the move along -Z that rests the slice `clear` above the surface under
    it. `columns` [(item z, surface z or None)] per slice, vertices of the slice; a slice
    with no surface under it takes its neighbours' mean. `bend`: no slice moves more than
    this below the slice that moves least (a stiff item rests on its highest support;
    pure Python, tested)."""
    out = []
    for col in columns:
        need = [s + clear - z for z, s in col if s is not None]
        out.append(max(need) if need else None)
    for i, v in enumerate(out):
        if v is None:
            near = [x for x in out[max(0, i - 2):i + 3] if x is not None]
            out[i] = sum(near) / len(near) if near else 0.0
    if bend is not None and out:
        top = max(out)
        out = [max(v, top - bend) for v in out]
    return out


def rigid_fit(P, Q):
    """The rotation R and translation t that best move points `P` onto `Q` (n x 3, least
    squares, Kabsch; numpy, tested)."""
    import numpy as np

    P, Q = np.asarray(P, float), np.asarray(Q, float)
    cp, cq = P.mean(0), Q.mean(0)
    U, _s, Vt = np.linalg.svd((P - cp).T @ (Q - cq))
    d = np.sign(np.linalg.det(Vt.T @ U.T))
    R = Vt.T @ np.diag([1.0, 1.0, d]) @ U.T
    return R, cq - R @ cp


def spread_max(xs, reach=1):
    """Each value the largest within `reach` places of it (a lift kept a frame before and
    after it is needed: no jump from frame to frame)."""
    return [max(xs[max(0, i - reach):i + reach + 1]) for i in range(len(xs))]


def strap_end(points, side_x):
    """The lowest point of a strap at the side `side_x` (sign) of the body: its hip end."""
    pts = [p for p in points if (p[0] > 0) == (side_x > 0)] or list(points)
    return min(pts, key=lambda p: p[2])


# ----------------------------------------------------------------------------- meshes

def portmanteau_mesh(name="portmanteau"):
    """A cylindrical cloth valise across the croup (`PORTMANTEAU`, axis X): rounded ends,
    two leather straps round it."""
    from . import gear_hd

    L, R = PORTMANTEAU
    b = gear_hd.Builder()
    # Rings every 3 cm along it, so `sag` can bend it over the croup (with only the end
    # rings the crest came up through its middle: EngD18 58 hide vertices at STAND).
    mid = [(0.08 + 0.84 * k / 14, R) for k in range(15)]
    b.tube(-L / 2, L / 2, [(0, R * 0.80), (0.03, R * 0.96), *mid, (0.97, R * 0.96), (1, R * 0.80)],
           "canvas", n=20, axis="x")
    for x in (-0.15, 0.15):
        b.tube(x - 0.014, x + 0.014, [(0, R + 0.003), (1, R + 0.003)], "leather", n=20, axis="x")
    return gear_hd._finish_mesh(b, name)


def roll_mesh(name="roll"):
    """The rolled blanket (`ROLL`, axis X), its ends tapering, two straps."""
    from . import gear_hd

    L, R = ROLL
    b = gear_hd.Builder()
    mid = [(0.12 + 0.76 * k / 15, R) for k in range(16)]
    b.tube(-L / 2, L / 2, [(0, R * 0.70), (0.04, R * 0.92), *mid, (0.96, R * 0.92), (1, R * 0.70)],
           "canvas", n=16, axis="x")
    for x in (-0.17, 0.17):
        b.tube(x - 0.011, x + 0.011, [(0, R + 0.003), (1, R + 0.003)], "leather", n=16, axis="x")
    return gear_hd._finish_mesh(b, name)


def swivel_mesh(name="swivel"):
    """The carbine belt's steel swivel: a bar and its spring hook hanging from a ring."""
    from . import gear_hd

    b = gear_hd.Builder()
    gear_hd._swivel(b, (0.0, 0.0, -0.02))
    b.box((0.0, 0.0, -0.05), (0.008, 0.006, 0.05), "steel", r=0.002, n=2)
    return gear_hd._finish_mesh(b, name)


# ----------------------------------------------------------------------------- placing

def _to_rest(ob, arm, bone):
    """Moves `ob`'s mesh (world, current pose) to where it is at rest under `bone`."""
    pb = arm.pose.bones[bone]
    A = arm.matrix_world
    ob.data.transform(A @ pb.bone.matrix_local @ pb.matrix.inverted() @ A.inverted())


def _from_rest(arm, bone):
    pb = arm.pose.bones[bone]
    A = arm.matrix_world
    return A @ pb.matrix @ pb.bone.matrix_local.inverted() @ A.inverted()


def _posed_tree(objs):
    """BVH of `objs` as evaluated now (world)."""
    import bpy
    from mathutils.bvhtree import BVHTree

    dg = bpy.context.evaluated_depsgraph_get()
    V, F, base = [], [], 0
    for o in objs:
        ev = o.evaluated_get(dg)
        me = ev.to_mesh()
        mw = ev.matrix_world
        V += [mw @ v.co for v in me.vertices]
        F += [[base + i for i in p.vertices] for p in me.polygons]
        base += len(me.vertices)
        ev.to_mesh_clear()
    return BVHTree.FromPolygons(V, F)


def sag(ob, tree, clear=GAP, slices=14):
    """Lays `ob` (world) down onto `tree` slice by slice along X: each slice of vertices
    moves down so its lowest rests `clear` above the surface under it, the moves blended
    between slice centres. Returns the gaps under the item after (min, median)."""
    from mathutils import Vector

    vs = ob.data.vertices
    xs = [v.co.x for v in vs]
    x0, x1 = min(xs), max(xs)
    w = (x1 - x0) / slices
    cols = [[] for _ in range(slices)]
    for v in vs:
        i = min(slices - 1, int((v.co.x - x0) / w))
        hit = tree.ray_cast(Vector((v.co.x, v.co.y, v.co.z + 1.0)), Vector((0, 0, -1)), 3.0)[0]
        cols[i].append((v.co.z, hit.z if hit is not None and hit.z < v.co.z + 0.5 else None))
    sh = sag_shifts(cols, clear, BEND)
    for v in vs:
        t = (v.co.x - x0) / w - 0.5
        i = max(0, min(slices - 1, int(math.floor(t))))
        j = min(slices - 1, i + 1)
        f = max(0.0, min(1.0, t - i))
        v.co.z += sh[i] * (1 - f) + sh[j] * f
    gaps = []
    for v in vs:
        hit = tree.ray_cast(v.co + Vector((0, 0, 0.001)), Vector((0, 0, -1)), 0.05)[0]
        if hit is not None:
            gaps.append(v.co.z - hit.z)
    gaps.sort()
    return [round(gaps[0], 4), round(gaps[len(gaps) // 2], 4)] if gaps else None


def _out_gap(ob, tree, axis):
    """Smallest gap from `ob`'s vertices (world) along -`axis` to `tree`."""
    from mathutils import Vector

    a = Vector(axis).normalized()
    best = None
    for v in ob.data.vertices:
        hit = tree.ray_cast(v.co, -a, 0.05)[0]
        if hit is not None:
            g = (v.co - hit).length
            best = g if best is None else min(best, g)
    return None if best is None else round(best, 4)


def _finish(ch, ob, key, tris, st, rider_bone=None):
    from . import kit_hd, line_kit

    keys = material_keys([m.name for m in ob.data.materials])
    st[key]["tris"] = line_kit._decimate(ob, tris)
    ob["acr_material_keys"] = keys
    if rider_bone:
        kit_hd.rigid(ob, ch.arm, rider_bone)
        ch.add("gear", ob, keys[0])
    return keys


def straps(ch, name, kit, origin):
    """Lays the carbine belt and the waist belt over the rider's clothes (rest pose)."""
    from mathutils import Vector

    from . import armature as am
    from . import gear_hd, kit_hd, line_kit, straps as st_

    cloth = [o for o in ch.layers["clothes"] if ch.keys.get(o.name) != "belts"]
    under = [o for o in cloth if ch.keys.get(o.name) in ("coat", "buff")] + [o for o in cloth if ch.keys.get(o.name) not in ("coat", "buff")]
    o = Vector(origin)
    laid = []
    specs = []
    if "carbine_belt" in kit:
        # Its end (and swivel) 5 cm above the line's hip point, clear of the saddle's seat.
        specs.append(("carbine_belt", o + st_.LEFT_SHOULDER, o + st_.RIGHT_HIP + Vector((0, 0, 0.05)), CARBINE_W, None))
    if "waist_belt" in kit:
        specs.append(("waist_belt", None, None, WAIST_W, "ring"))
    for suffix, a, b, w, kind in specs:
        # Round the waist the hanging arms (A-pose) are left out (`kit_hd.trunk_tree`).
        tree = (kit_hd.trunk_tree if kind == "ring" else kit_hd.surface_tree)([*cloth, *laid])
        path = kit_hd.ring_path(tree, (o.x, o.y), o.z + WAIST_Z) if kind == "ring" else None
        s = kit_hd.lay_strap(f"{name}.{suffix}", tree, a, b, w, kit_hd.BELT_OFFSET, kit_hd.BELT_THICK,
                             line_kit.ACROSS, line_kit.STEP, path=path)
        line_kit._weigh(s, [*under, ch.flesh], laid, line_kit.ACROSS)
        line_kit._open_under(s, line_kit.ACROSS)
        am.bind(s, ch.arm)
        laid.append(s)
    for s in laid:
        ch.add("clothes", s, "belts", symmetric=True)
    ch.cav_kit = {"straps": {s.name.rsplit(".", 1)[-1]: s for s in laid}, "items": {}, "saddle": {}}
    ch.cav_kit_stats = {"straps_tris": {s.name.rsplit(".", 1)[-1]: gear_hd._tris(s) for s in laid}, "items": {}}
    return laid


def rider_items(ch, name, kit, origin, spec):
    """The swivel, buckle, pouch, scabbard and hilt (current pose: seated)."""
    import bpy
    from mathutils import Euler, Matrix, Vector

    from . import armature as am
    from . import gear_hd, kit_hd, line_kit

    P = ch.prefix
    o = Vector(origin)
    st = ch.cav_kit_stats["items"]
    hips = P + "hips"
    bpy.context.view_layer.update()
    belts = ch.cav_kit["straps"]
    belt_tree = kit_hd.trunk_tree([*ch.layers["clothes"], ch.flesh])
    if "carbine_belt" in belts:
        pts = [tuple(v.co) for v in belts["carbine_belt"].data.vertices]
        end = Vector(strap_end(pts, -1))
        ob = swivel_mesh(f"{name}.swivel")
        m = Matrix.Translation(end + Vector((-0.004, 0.0, -0.004))) @ Matrix.Rotation(math.radians(90), 4, "Z")
        st["swivel"] = {"shift_m": kit_hd.seat([ob], belt_tree, m, (-1.0, 0.0, 0.0), clear=0.002)[0]}
        _finish(ch, ob, "swivel", TRIS["swivel"], st, hips)
        ch.cav_kit["items"]["swivel"] = ob
    # No buckle under a cuirass: pushed out by it onto the pommel (SpnRei 27 vertices at STAND).
    if "waist_belt" in belts and not spec.get("armour"):
        ob = gear_hd.buckle_mesh(f"{name}.buckle")
        c = Vector((o.x, o.y - 0.2, o.z + WAIST_Z))
        hit = belt_tree.ray_cast(c, Vector((0, 1, 0)), 0.5)[0]
        m = Matrix.Translation(Vector((o.x, (hit.y if hit else o.y - 0.12) - 0.05, o.z + WAIST_Z)))
        st["buckle"] = {"shift_m": kit_hd.seat([ob], belt_tree, m, (0.0, -1.0, 0.0), clear=0.001)[0]}
        _finish(ch, ob, "buckle", TRIS["buckle"], st, hips)
        ch.cav_kit["items"]["buckle"] = ob
    if "pouch" in kit:
        # On the waist belt over the right hip, its flap outwards (`gear_hd`: front +Y).
        ob = gear_hd.cartridge_box_mesh(f"{name}.pouch")
        ob.data.transform(Matrix.Diagonal((0.85, 0.85, 0.85, 1.0)))
        m = Matrix.Translation(o + Vector((-0.17, 0.05, WAIST_Z))) @ Euler((0.0, 0.0, math.radians(80)), "XYZ").to_matrix().to_4x4()
        # On the lowest gap: the skirt flares out under the hip (EngD18: 22 skirt vertices
        # inside it at the 0.3 quantile).
        shift, _f, rep = line_kit.seat_back(ob, ch, m, (-1.0, 0.18, 0.0), quantile=0.0)
        st["pouch"] = {"shift_m": shift, **rep}
        _finish(ch, ob, "pouch", TRIS["pouch"], st, hips)
        ch.cav_kit["items"]["pouch"] = ob
    if "scabbard" in kit:
        kind = spec.get("blade", "sabre") if spec.get("blade", "sabre") in SCABBARD else "sabre"
        length, width = SCABBARD[kind]
        ob = gear_hd.scabbard_mesh(f"{name}.scabbard", length=length, width=width)
        # The mouth by the left of the waist belt (rest, hips), the flat against the flank.
        mouth = o + Vector((0.20, 0.04, WAIST_Z - 0.05))
        tilt_x, tilt_y = SCABBARD_TILT
        rot = (Matrix.Rotation(math.radians(tilt_y), 4, "Y") @ Matrix.Rotation(math.radians(tilt_x), 4, "X")
               @ Matrix.Rotation(math.radians(90), 4, "Z"))
        rest_m = Matrix.Translation(mouth) @ rot
        world_m = _from_rest(ch.arm, hips) @ rest_m
        horse = [*ch.horse["mount"].meshes[:1], *[x for x in ch.layers["gear"] if ch.keys.get(x.name) == "saddle"],
                 *ch.horse.get("extra", [])]
        legs = [x for x in [ch.flesh, *ch.layers["clothes"]] if ch.keys.get(x.name) != "belts"]
        tree = _posed_tree([*horse, *legs])
        shift, final = kit_hd.seat([ob], tree, world_m, (1.0, 0.0, 0.0), clear=SADDLE_GAP)
        gap = _out_gap(ob, tree, (1.0, 0.0, 0.0))
        _to_rest(ob, ch.arm, hips)
        pb, A = ch.arm.pose.bones[hips], ch.arm.matrix_world
        # The seated mouth at rest: the hanger's hook is above it (`hang`).
        ch.cav_kit["mouth"] = (A @ pb.bone.matrix_local @ pb.matrix.inverted() @ A.inverted()) @ final.translation
        st["scabbard"] = {"shift_m": shift, "gap_m": gap, "length_m": length}
        _finish(ch, ob, "scabbard", TRIS["scabbard"], st, hips)
        ch.cav_kit["items"]["scabbard"] = ob
        # The hilt in the mouth, on the hip socket (hidden while the blade is drawn).
        hilt = gear_hd.hanger_mesh(f"{name}.hilt", blade=False)
        line_kit._decimate(hilt, TRIS["hilt"])
        hilt["acr_material_keys"] = material_keys([mt.name for mt in hilt.data.materials])
        am.attach(hilt, ch.arm, P + "socket.hip_l", final @ Matrix.Translation((0.0, 0.0, 0.012)) @ Matrix.Rotation(math.radians(-90), 4, "Z"))
        ch.held["sabre"] = hilt
        ch.keys[hilt.name] = hilt["acr_material_keys"][0]
        st["hilt"] = {"tris": gear_hd._tris(hilt), "socket": "socket.hip_l"}


def saddle_items(ch, name, kit):
    """The valise and the roll behind the cantle (current pose), on the saddle bone for now
    (`follow_croup` gives them their own bone once the clips exist)."""
    from mathutils import Vector

    from . import kit_hd

    hz = ch.horse["mount"]
    st = ch.cav_kit_stats["items"]
    sad_objs = [x for x in ch.layers["gear"] if ch.keys.get(x.name) == "saddle"]
    surf = [hz.meshes[0], *ch.horse.get("extra", [])]
    import bpy

    dg = bpy.context.evaluated_depsgraph_get()
    back = -1e9
    for s in sad_objs:
        ev = s.evaluated_get(dg)
        me = ev.to_mesh()
        back = max(back, max((ev.matrix_world @ v.co).y for v in me.vertices))
        ev.to_mesh_clear()
    sad = hz.head("saddle")
    if back < -1e8:
        back = sad.y + 0.30
    laid = []
    for key, mesh, (L, R) in (("portmanteau", portmanteau_mesh, PORTMANTEAU), ("roll", roll_mesh, ROLL)):
        if key not in kit:
            continue
        ob = mesh(f"{name}.{key}")
        y = back + 0.01 + (PORTMANTEAU[1] if key == "portmanteau" else PORTMANTEAU[1] * 0.9)
        ob.data.transform(__import__("mathutils").Matrix.Translation(Vector((sad.x, y, sad.z + 0.6))))
        tree = _posed_tree([*surf, *sad_objs]) if not laid else _union(surf + sad_objs, laid)
        gaps = sag(ob, tree, SADDLE_GAP)
        st[key] = {"gap_m": gaps}
        laid.append(ob)
    for ob in laid:
        key = ob.name.rsplit(".", 1)[-1]
        _to_rest(ob, ch.arm, "saddle")
        _finish(ch, ob, key, TRIS[key], st)
        kit_hd.rigid(ob, ch.arm, "saddle")
        ch.add("gear", ob, ob["acr_material_keys"][0])
        ch.cav_kit["saddle"][key] = ob
        st[key]["bone"] = "saddle"


def _union(posed, world_objs):
    """BVH of `posed` (evaluated) and `world_objs` (mesh data already in world)."""
    import bpy
    from mathutils.bvhtree import BVHTree

    dg = bpy.context.evaluated_depsgraph_get()
    V, F, base = [], [], 0
    for o in posed:
        ev = o.evaluated_get(dg)
        me = ev.to_mesh()
        V += [ev.matrix_world @ v.co for v in me.vertices]
        F += [[base + i for i in p.vertices] for p in me.polygons]
        base += len(me.vertices)
        ev.to_mesh_clear()
    for o in world_objs:
        V += [v.co.copy() for v in o.data.vertices]
        F += [[base + i for i in p.vertices] for p in o.data.polygons]
        base += len(o.data.vertices)
    return BVHTree.FromPolygons(V, F)


def wear(ch, name, spec, origin):
    """The kit of `spec["kit"]` (rest pose for the straps, the seated pose for the items)."""
    from . import armature as am

    kit = check(spec.get("kit"))
    straps(ch, name, kit, origin)
    am.pose(ch.arm, ch.poses[ch.ride], prefix=ch.prefix)
    rider_items(ch, name, kit, origin, spec)
    saddle_items(ch, name, kit)


# ----------------------------------------------------------------------------- checks

def _add_bone(arm, name, parent, head, tail, children=()):
    """Adds the deforming bone `name` (armature space) under `parent`; `children` move under
    it, keeping their rest."""
    import bpy

    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    b = eb.new(name)
    b.head, b.tail = head, tail
    b.parent = eb[parent]
    b.use_connect = False
    b.use_deform = True
    for c in children:
        if c in eb:
            eb[c].parent = b
    bpy.ops.object.mode_set(mode="OBJECT")
    arm.pose.bones[name].rotation_mode = "QUATERNION"


def _rest_pose(arm):
    for p in arm.pose.bones:
        p.location, p.scale = (0.0, 0.0, 0.0), (1.0, 1.0, 1.0)
        p.rotation_quaternion, p.rotation_euler = (1.0, 0.0, 0.0, 0.0), (0.0, 0.0, 0.0)


def _co(ob, dg):
    """Vertices of `ob` as evaluated (world, numpy)."""
    import numpy as np

    ev = ob.evaluated_get(dg)
    me = ev.to_mesh()
    co = np.empty(len(me.vertices) * 3)
    me.vertices.foreach_get("co", co)
    mw = np.array(ev.matrix_world)
    ev.to_mesh_clear()
    return co.reshape(-1, 3) @ mw[:3, :3].T + mw[:3, 3]


def _sweep(arm, clips, meshes, bones):
    """Per clip, every frame: the vertices `idx` (all with None) of each of `meshes` [(object,
    idx)] (world) and the pose matrices of `bones` (armature space), as played.
    {clip: [(frame, [co per mesh], {bone: matrix})]}."""
    import bpy

    from . import armature as am

    out = {}
    for clip in clips:
        act = bpy.data.actions.get(clip)
        if act is None:
            continue
        f0, f1 = (int(round(x)) for x in act.frame_range)
        rows = []
        for f in range(f0, f1 + 1):
            am.set_clip(arm, clip, f)
            dg = bpy.context.evaluated_depsgraph_get()
            co = [_co(ob, dg) if idx is None else _co(ob, dg)[idx] for ob, idx in meshes]
            rows.append((f, co, {b: arm.pose.bones[b].matrix.copy() for b in bones}))
        out[clip] = rows
    arm.animation_data.action = None
    return out


def _key_basis(arm, clip, bone, frames_basis):
    """Keys `bone`'s local transform [(frame, basis matrix)] into `clip` (linear)."""
    import bpy

    from . import armature as am

    act = bpy.data.actions[clip]
    ad = arm.animation_data
    ad.action = act
    if hasattr(ad, "action_slot") and act.slots:
        ad.action_slot = act.slots[0]
    pb = arm.pose.bones[bone]
    for f, B in frames_basis:
        loc, q, _s = B.decompose()
        pb.location, pb.rotation_quaternion, pb.scale = loc, q, (1.0, 1.0, 1.0)
        for path in ("location", "rotation_quaternion", "scale"):
            pb.keyframe_insert(path, frame=f, group=bone)
    for fc in am._fcurves(act):
        if f'"{bone}"' in fc.data_path:
            for kp in fc.keyframe_points:
                kp.interpolation = "LINEAR"
    ad.action = None


def _min_gap(tree, pts, reach=PATCH_MARGIN):
    """Least signed distance of `pts` to `tree` (within `reach`; None if none is near) and
    the number deeper than `INSIDE_M`."""
    from mathutils import Vector

    best, n = None, 0
    for p in pts:
        loc, nrm, _i, _d = tree.find_nearest(Vector(p), reach)
        if loc is None:
            continue
        s = (Vector(p) - loc).dot(nrm)
        best = s if best is None else min(best, s)
        n += s < -INSIDE_M
    return best, n


def follow_croup(ch, clips):
    """Gives the valise and roll their own horse bone (`VALISE_BONE`, under the saddle),
    keyed on every frame of `clips` from the croup under them: the rigid fit (`rigid_fit`)
    of the hide's vertices under their footprint at that frame to the same vertices in
    `STAND` frame 0, where `reseat` lays them down. A frame where the croup would come
    within `LIFT_CLEAR` of them (it bends under the fit) lifts them along the fit's up by
    the shortfall, spread to the frames beside (`spread_max`). No back bone follows the
    croup's 6-11 cm against the saddle in the gallop, and the hide's own weights swung the
    valise into the rider (m2-cav-kit). Returns the stats per clip."""
    import bpy
    import numpy as np
    from mathutils import Matrix, Vector
    from mathutils.bvhtree import BVHTree

    from . import armature as am

    items = ch.cav_kit["saddle"]
    if not items:
        return None
    arm = ch.arm
    hide = ch.horse["mount"].meshes[0]
    arm.animation_data.action = None
    _rest_pose(arm)
    bpy.context.view_layer.update()
    pts = [ob.matrix_world @ v.co for ob in items.values() for v in ob.data.vertices]
    c = arm.matrix_world.inverted() @ (sum(pts, Vector()) / len(pts))
    _add_bone(arm, VALISE_BONE, "saddle", c, c + Vector((0.0, 0.0, 0.1)))
    _rest_pose(arm)
    gaps = reseat(ch, VALISE_BONE)
    for k in items:
        ch.cav_kit_stats["items"][k].update(bone=VALISE_BONE, gap_m=gaps[k])
    # The reference: STAND frame 0, the bone not keyed yet (it follows the saddle).
    _rest_pose(arm)
    am.set_clip(arm, "STAND", 0)
    dg = bpy.context.evaluated_depsgraph_get()
    V0 = np.concatenate([_co(ob, dg) for ob in items.values()])
    B0 = arm.pose.bones[VALISE_BONE].matrix.copy()
    H0 = _co(hide, dg)
    lo, hi = V0.min(0), V0.max(0)
    m = PATCH_MARGIN
    top = H0[:, 2] > lo[2] - PATCH_DEPTH
    near = np.all((H0[:, :2] >= lo[:2] - m) & (H0[:, :2] <= hi[:2] + m), axis=1) & top
    under = np.all((H0[:, :2] >= lo[:2]) & (H0[:, :2] <= hi[:2]), axis=1) & top
    idx = np.nonzero(near)[0]
    pos = {int(i): k for k, i in enumerate(idx)}
    fit = np.array([pos[int(i)] for i in np.nonzero(under)[0]])
    polys = [[pos[i] for i in p.vertices] for p in hide.data.polygons if all(i in pos for i in p.vertices)]
    P0 = H0[idx][fit]
    arm.animation_data.action = None
    sweep = _sweep(arm, clips, [(hide, idx)], ["saddle"])
    rel = arm.pose.bones["saddle"].bone.matrix_local.inverted() @ arm.pose.bones[VALISE_BONE].bone.matrix_local
    per = {}
    for clip, rows in sweep.items():
        fits, lifts = [], []
        for _f, (co,), _m in rows:
            R, t = rigid_fit(P0, co[fit])
            tree = BVHTree.FromPolygons(co.tolist(), polys)
            up = R[:, 2]
            moved = V0 @ R.T + t
            lift = 0.0
            for _ in range(3):
                g = _min_gap(tree, moved + up * lift)[0]
                if g is None or g >= LIFT_CLEAR - 1e-4:
                    break
                lift += LIFT_CLEAR - g
            fits.append((R, t, up, tree))
            lifts.append(lift)
        lifts = spread_max(lifts)
        keys, after = [], []
        for (f, _hc, mats), (R, t, up, tree), lift in zip(rows, fits, lifts):
            T = Matrix([[*R[i], t[i] + up[i] * lift] for i in range(3)] + [[0.0, 0.0, 0.0, 1.0]])
            keys.append((f, (mats["saddle"] @ rel).inverted() @ (T @ B0)))
            after.append(_min_gap(tree, V0 @ R.T + t + up * lift))
        _key_basis(arm, clip, VALISE_BONE, keys)
        gs = [a[0] for a in after if a[0] is not None]
        per[clip] = {"lift_max_m": round(max(lifts), 4), "gap_min_m": round(min(gs), 4) if gs else None,
                     "inside": max(a[1] for a in after)}
    _rest_pose(arm)
    return {"bone": VALISE_BONE, "patch_verts": int(len(idx)), "fit_verts": int(len(fit)),
            "inside_total": sum(p["inside"] for p in per.values()), "clips": per}


def reseat(ch, bone):
    """Rigs the saddle kit on the horse's `bone` and lays it down again in `STAND` frame 0
    (`sag`; the horse stands otherwise than at rest): {piece: gaps}."""
    from . import armature as am
    from . import kit_hd

    items = ch.cav_kit["saddle"]
    hz = ch.horse["mount"]
    sad_objs = [x for x in ch.layers["gear"] if ch.keys.get(x.name) == "saddle"]
    surf = [hz.meshes[0], *ch.horse.get("extra", []), *sad_objs]
    am.set_clip(ch.arm, "STAND", 0)
    M = _from_rest(ch.arm, bone)
    for ob in items.values():
        ob.data.transform(M)
    gaps, laid = {}, []
    for k in [k for k in SADDLE_PIECES if k in items]:
        ob = items[k]
        tree = _posed_tree(surf) if not laid else _union(surf, laid)
        gaps[k] = sag(ob, tree, SADDLE_GAP)
        laid.append(ob)
    for ob in items.values():
        _to_rest(ob, ch.arm, bone)
        kit_hd.rigid(ob, ch.arm, bone)
    ch.arm.animation_data.action = None
    return gaps


def live_clips(clips):
    """The clips with the horse on its feet (not the falls, corpses and crawls)."""
    return [c for c in clips if not c.startswith(("DEATH", "CRAWL", "WRITHE"))]


def hang(ch, clips):
    """Hangs the scabbard from the waist belt on a hanger bone (`HANGER_BONE`, under the
    hips, its head at the belt's hook `HOOK` above the mouth; the hilt's `socket.hip_l`
    under it), keyed on every frame of the live clips: it goes with the hips at its hook but
    keeps the angle it hangs at in `STAND` against the horse, and where it would meet hide,
    saddle or saddlecloth its carriage straps let it out sideways (the horse's +X, its left)
    by the depth plus `GAP`, at most `STRAP`, or swing out about the hook (`HANG_OUT`),
    whichever moves it least (`HANG_COST`), both spread to the frames beside (`spread_max`).
    Rigid on the hips it pressed its mouth into the saddle when the rider stood in the
    stirrups (104-132 vertices in the leans), and swinging it out about the mouth did not
    help (the mouth stayed in). Returns the push per clip."""
    import numpy as np
    from mathutils import Matrix, Vector
    from mathutils.bvhtree import BVHTree

    from . import armature as am
    from . import kit_hd

    ob = ch.cav_kit["items"].get("scabbard")
    if ob is None:
        return None
    arm, P = ch.arm, ch.prefix
    hips, bone = P + "hips", P + HANGER_BONE
    hook = arm.matrix_world.inverted() @ (ch.cav_kit["mouth"] + Vector(HOOK))
    arm.animation_data.action = None
    _rest_pose(arm)
    _add_bone(arm, bone, hips, hook, hook + Vector((0.0, 0.0, 0.1)), children=(P + "socket.hip_l",))
    kit_hd.rigid(ob, arm, bone)
    _rest_pose(arm)
    import bpy

    bpy.context.view_layer.update()
    # The scabbard in the hanger's frame (its rest), and what it may meet near it.
    W = arm.matrix_world
    Bl = arm.pose.bones[bone].bone.matrix_local
    loc = np.array([list(Bl.inverted() @ (W.inverted() @ (ob.matrix_world @ v.co))) for v in ob.data.vertices])
    am.set_clip(arm, "STAND", 0)
    dg = bpy.context.evaluated_depsgraph_get()
    S0 = arm.pose.bones[bone].matrix.to_3x3()
    A0 = arm.pose.bones["saddle"].matrix.to_3x3()
    hz = ch.horse["mount"]
    others = [hz.meshes[0], *[x for x in ch.layers["gear"] if ch.keys.get(x.name) == "saddle"], *ch.horse.get("extra", [])]
    sc = _co(ob, dg)
    lo, hi = sc.min(0) - 0.35, sc.max(0) + 0.35
    meshes, polys, base = [], [], 0
    for o in others:
        co = _co(o, dg)
        keep = np.nonzero(np.all((co >= lo) & (co <= hi), axis=1))[0]
        pos = {int(i): k for k, i in enumerate(keep)}
        polys += [[base + pos[i] for i in p.vertices] for p in o.data.polygons if all(i in pos for i in p.vertices)]
        meshes.append((o, keep))
        base += len(keep)
    arm.animation_data.action = None
    live = live_clips(clips)
    sweep = _sweep(arm, clips, meshes, ["saddle", hips])
    rel = arm.pose.bones[hips].bone.matrix_local.inverted() @ Bl
    per = {}
    for clip, rows in sweep.items():
        if clip not in live:
            _key_basis(arm, clip, bone, [(f, Matrix.Identity(4)) for f, _c, _m in rows])
            continue
        def place(mats, cos, a):
            M_id = mats[hips] @ rel
            turn = mats["saddle"].to_3x3() @ A0.inverted()
            O = turn @ Matrix.Rotation(math.radians(-a), 3, "Y") @ S0
            out = np.array(list(turn @ Vector((1.0, 0.0, 0.0))))
            pts = loc @ np.array([list(r) for r in O]).T + np.array(list(M_id.translation))
            return M_id, O, out, pts

        def solve(tree, pts, out):
            push = 0.0
            for _ in range(5):
                g, n = _min_gap(tree, pts + out * push, reach=0.1)
                if g is None or g >= GAP - 1e-4 or push >= STRAP:
                    return push, n if push >= STRAP else 0
                push = min(STRAP, push + GAP - g)
            return push, _min_gap(tree, pts + out * push, reach=0.1)[1]

        trees, angles = [], []
        for _f, cos, mats in rows:
            tree = BVHTree.FromPolygons(np.concatenate(cos).tolist(), polys)
            trees.append(tree)
            cost = {}
            for a in HANG_OUT:
                M_id, O, out, pts = place(mats, cos, a)
                push, n = solve(tree, pts, out)
                cost[a] = (n, push + a * HANG_COST)
                if push == 0.0:
                    break
            angles.append(min(cost, key=lambda a: cost[a]))
        angles = spread_max(angles)
        frames, pushes = [], []
        for (_f, cos, mats), tree, a in zip(rows, trees, angles):
            M_id, O, out, pts = place(mats, cos, a)
            frames.append((M_id, O, out, tree, pts))
            pushes.append(solve(tree, pts, out)[0])
        pushes = spread_max(pushes)
        keys, inside = [], 0
        for (f, _c, _m), (M_id, O, out, tree, pts), push in zip(rows, frames, pushes):
            M = Matrix.Translation(M_id.translation + Vector(list(out * push))) @ O.to_4x4()
            keys.append((f, M_id.inverted() @ M))
            inside = max(inside, _min_gap(tree, pts + out * push, reach=0.1)[1])
        _key_basis(arm, clip, bone, keys)
        if max(pushes) > 0 or inside:
            per[clip] = {"push_max_m": round(max(pushes), 4), "out_max_deg": max(angles), "inside": inside,
                         "push_cm": [round(x * 100) for x in pushes]}
    _rest_pose(arm)
    return {"bone": HANGER_BONE, "hook_m": list(HOOK), "pushed": per}


def collide(arm, groups, clips, steps=6):
    """Per clip, sampled `steps` times (None: every frame): for each (name, item, others) the vertices of
    `others` deeper than `INSIDE_M` inside the item and the item's vertices inside each of
    `others` (nearest face's normal), the worst frame per pair. {clips: {clip: {pair: n}},
    total, deepest_m, worst}."""
    import bpy
    import numpy as np
    from mathutils import Vector
    from mathutils.bvhtree import BVHTree

    from . import armature as am

    out, total, worst = {}, 0, (0.0, None)
    for clip in clips:
        if clip not in bpy.data.actions:
            continue
        f0, f1 = (int(round(x)) for x in bpy.data.actions[clip].frame_range)
        pairs = {}
        picks = range(f0, f1 + 1) if steps is None else {int(round(f0 + (f1 - f0) * k / max(1, steps - 1))) for k in range(steps)}
        for f in sorted(picks):
            am.set_clip(arm, clip, f)
            dg = bpy.context.evaluated_depsgraph_get()
            cache = {}

            def get(o):
                if o.name not in cache:
                    ev = o.evaluated_get(dg)
                    me = ev.to_mesh()
                    co = np.empty(len(me.vertices) * 3)
                    me.vertices.foreach_get("co", co)
                    mw = np.array(ev.matrix_world)
                    pos = co.reshape(-1, 3) @ mw[:3, :3].T + mw[:3, 3]
                    polys = [tuple(p.vertices) for p in me.polygons]
                    ev.to_mesh_clear()
                    cache[o.name] = (pos, polys, None)
                pos, polys, tree = cache[o.name]
                return pos, polys

            def tree_of(o):
                pos, polys = get(o)
                p, q, t = cache[o.name]
                if t is None:
                    t = BVHTree.FromPolygons(p.tolist(), q)
                    cache[o.name] = (p, q, t)
                return t

            for nm, item, others in groups:
                ip, _ = get(item)
                lo, hi = ip.min(0) - 0.01, ip.max(0) + 0.01
                for other in others:
                    op, _ = get(other)
                    n = 0
                    for src, dst in ((op, item), (ip, other)):
                        near = src[np.all((src >= lo) & (src <= hi), axis=1)] if dst is item else src
                        t = tree_of(dst)
                        for p in near:
                            loc, nrm, _i, _d = t.find_nearest(Vector(p), 0.05)
                            if loc is None:
                                continue
                            s = (Vector(p) - loc).dot(nrm)
                            if s < -INSIDE_M:
                                n += 1
                                if s < worst[0]:
                                    worst = (s, [clip, f, nm, other.name.rsplit(".", 1)[-1]])
                    if n:
                        key = f"{nm}|{other.name.rsplit('.', 1)[-1]}"
                        pairs[key] = max(pairs.get(key, 0), n)
        out[clip] = pairs
        total += sum(pairs.values())
    arm.animation_data.action = None
    return {"clips": {c: p for c, p in out.items() if p}, "total": total, "deepest_m": round(worst[0], 4), "worst": worst[1]}


# The clips where the rider leaves the seat (`rider_clear`): the cuts (`cavalry.add_clips`; ATTACK_DOWN and
# ATTACK_CUT_DOWN_R play ATTACK_CUT_DOWN's keys, ATTACK_CUT_HIGH ATTACK_CUT_MID's), and STAND.
SEAT_CLIPS = ("STAND", "ATTACK", "ATTACK_CUT_DOWN", "ATTACK_CUT_DOWN_L", "ATTACK_CUT_MID")


LEG_BONES = ("thigh", "shin", "foot", "toe")
ARM_BONES = ("upperarm", "forearm", "hand", "index", "middle", "ring", "pinky", "thumb")


def rider_clear(ch, clips):
    """`ctx.stats["seat"]`: on every frame of `SEAT_CLIPS`, the rider's vertices (body and
    clothes, not the belts) deeper than `INSIDE_M` inside hide, saddle or saddlecloth, split
    by the bone that moves them most: "trunk" (hips, spine, skirts, head: the seat), "arms"
    (the fists hold reins and pommel) and "legs" (thighs down, which straddle the barrel in
    every seated pose); the worst frame
    per clip ([count, frame]), "trunk_frames" the trunk's count per frame, and "rise_m" the
    hips' lowest and highest place against STAND frame 0."""
    import bpy
    import numpy as np
    from mathutils import Vector
    from mathutils.bvhtree import BVHTree

    from . import armature as am

    arm, P = ch.arm, ch.prefix
    hz = ch.horse["mount"]
    saddle = [x for x in ch.layers["gear"] if ch.keys.get(x.name) == "saddle"]
    others = [hz.meshes[0], *saddle, *ch.horse.get("extra", [])]
    rider = [x for x in [ch.flesh, *ch.layers["clothes"]] if x.type == "MESH" and ch.keys.get(x.name) != "belts"]
    legs = []
    for ob in rider:
        names = {g.index: g.name for g in ob.vertex_groups}
        leg = []
        for v in ob.data.vertices:
            g = max(v.groups, key=lambda e: e.weight, default=None)
            b = names.get(g.group, "")[len(P):] if g is not None else ""
            leg.append("legs" if b.startswith(LEG_BONES) else "arms" if b.startswith(ARM_BONES) else "trunk")
        legs.append(leg)
    out, hips0 = {}, None
    for clip in [c for c in SEAT_CLIPS if c in clips]:
        act = bpy.data.actions.get(clip)
        if act is None:
            continue
        f0, f1 = (int(round(x)) for x in act.frame_range)
        worst = {"trunk": [0, None], "arms": [0, None], "legs": [0, None]}
        rise, per = [], []
        for f in range(f0, f1 + 1):
            am.set_clip(arm, clip, f)
            dg = bpy.context.evaluated_depsgraph_get()
            z = arm.pose.bones[P + "hips"].head.z
            if hips0 is None:
                hips0 = z
            rise.append(z - hips0)
            pts, polys, base = [], [], 0
            for o in others:
                co = _co(o, dg)
                pts.append(co)
                polys += [[base + i for i in p.vertices] for p in o.data.polygons]
                base += len(co)
            allp = np.concatenate(pts)
            tree = BVHTree.FromPolygons(allp.tolist(), polys)
            lo, hi = allp.min(0) - 0.01, allp.max(0) + 0.01
            n = {"trunk": 0, "arms": 0, "legs": 0}
            for ob, leg in zip(rider, legs):
                co = _co(ob, dg)
                for k in np.nonzero(np.all((co >= lo) & (co <= hi), axis=1))[0]:
                    loc, nrm, _i, _d = tree.find_nearest(Vector(co[k]), 0.05)
                    if loc is not None and (Vector(co[k]) - loc).dot(nrm) < -INSIDE_M:
                        n[leg[k]] += 1
            per.append(n["trunk"])
            for cls in n:
                if n[cls] > worst[cls][0]:
                    worst[cls] = [n[cls], f - f0]
        out[clip] = {"trunk": worst["trunk"], "arms": worst["arms"], "legs": worst["legs"],
                     "rise_m": [round(min(rise), 3), round(max(rise), 3)], "trunk_frames": per}
    arm.animation_data.action = None
    return {"clips": out, "inside_m": INSIDE_M}


def report(ch, clips):
    """`ctx.stats["kit"]`: the pieces, then the collisions of the rider's items (or the old
    sabre box) and the saddle kit (`collide`)."""
    st = dict(getattr(ch, "cav_kit_stats", None) or {})
    hz = ch.horse["mount"]
    hide = hz.meshes[0]
    saddle = [x for x in ch.layers["gear"] if ch.keys.get(x.name) == "saddle"]
    cloth = [x for x in ch.layers["clothes"] if ch.keys.get(x.name) in ("coat", "buff")]
    rider = [x for x in [ch.flesh, *ch.layers["clothes"]] if ch.keys.get(x.name) != "belts"]
    kit = getattr(ch, "cav_kit", None)
    if kit:
        every = list(ch.arm.get("acr_clips", clips))
        st["valise"] = follow_croup(ch, every)
        st["scabbard_hang"] = hang(ch, every)
        items = dict(kit["items"])
        sad = kit["saddle"]
    else:
        items = {k: ch.held[k] for k in ("sabre",) if k in ch.held}
        sad = {}
    groups = [(k, ob, [*cloth, hide, *saddle, *ch.horse.get("extra", [])]) for k, ob in items.items() if ob.type == "MESH"
              and ob.modifiers]
    groups += [(k, ob, [hide, *rider]) for k, ob in sad.items()]
    # The old sabre is parented to its socket (no armature modifier): measure it too.
    groups += [(k, ob, [*cloth, hide, *saddle]) for k, ob in items.items() if not ob.modifiers]
    st["collisions"] = collide(ch.arm, groups, clips, steps=6)
    return st
