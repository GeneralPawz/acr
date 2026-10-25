"""Real worn kit for the line infantry (SPEC `kit`, m2-line-kit): the `gear_hd` kit at
regular-unit scale over the draped coat of `line.make`.

- **Straps** (`straps`): solid straps (`kit_hd.lay_strap`: box section, closed, the inner
  face a fixed `kit_hd.BELT_OFFSET` off the layer below, pits bridged, the inner face dropped: `_open_under`), the pouch belt left
  shoulder to right hip, the waist (bayonet) belt right shoulder to left hip over it, the
  knapsack's straps over both; `ACROSS` (`PACK_ACROSS`) samples over the width, a row every `STEP` m.
  Weights from the garment or strap right under each section (`drape.transfer_weights`),
  then without the arm bones (`kit_hd.trunk_weights`, at most 4): a severed arm leaves the
  straps on the trunk. They replace the ribbons of `straps.crossbelts` and
  `straps.carry` (`covered`), material key `belts`.
- **Items** (`items`): the `gear_hd` meshes (cartridge box, belt plate, bayonet and hanger
  scabbards with the hanger's hilt, knapsack, canteen) without their baked atlases: each
  collapsed to `TRIS` and coloured by the unit's palette through the line keys of
  `MAT_KEY` (`acr_material_keys`, one slot per source material), so the unit's own paint,
  decay mask and `acr_decay` (`decay.material`) apply as to any line gear. Placed as
  `kit_hd.ITEMS` places them and seated `kit_hd.CLEAR` outside the trunk of the clothes and
  the body, each rigid on one bone (`BONES`; the knapsack on `spine2`, so it cannot crumple
  in a deep bend). The hip items rest on the first surface
  under their back, the coat skirt included (`seat_back`: turned onto it, `SEAT_GAP` off it). They replace the boxes of `line._cartridge` and
  `line._knapsack`.
- **LODs:** no authored levels: the line soldier carries none, and one `acr_lod` node would
  make the whole model "authored" in `model_renderer::prepare_model` (no far meshes, the
  body drawn whole at every level). The kit takes the index levels of `lod.rs` with the rest.

- **Canteens** (`canteen` wood, `canteen_tin`): on their own narrow strap over the right
  shoulder (`CANTEEN_STRAP`), behind the left hip, seated on the skirt; the bone (`hips` or
  `thigh_l`) chosen per build on the fewest clothes vertices inside it in walk (`_canteen_bone`).
- **17th century** (`KIT17`, musketeers): the baldric (right shoulder to left hip) with the
  sword's scabbard, the bandolier over it with the twelve apostles, the bullet bag and the
  priming flask (`_hung`: each piece seated on the clothes and the baldric under it, the
  apostles all rigid on `spine`); the falling band goes under the straps
  (`UNDER_KIT`). They replace `belt_l`, `belt_r`, `pike_apostles` and `off_scabbard`.

`KITS` holds a kit per nation; `ctx.stats["kit"]`: triangles per piece, seat shifts, the
items' collisions per clip (`kit_hd.collisions`).
"""

# Per nation, the pieces worn (period notes in docs/families/line.md, "Worn kit").
KITS = {
    "eng": ("crossbelts", "belt_plate", "cartridge_box", "bayonet_scabbard", "knapsack", "canteen_tin"),
    "frn": ("crossbelts", "cartridge_box", "bayonet_scabbard", "hanger_scabbard", "knapsack", "canteen_tin"),
    "spn": ("crossbelts", "cartridge_box", "bayonet_scabbard", "hanger_scabbard", "knapsack", "canteen"),
    "usa": ("crossbelts", "cartridge_box", "bayonet_scabbard", "knapsack", "canteen"),
}
# The 17th-century musketeer (`pike.musketeer_spec`; docs/families/pike.md "Worn kit"): the
# sword on a buff baldric over the right shoulder, the bandolier over the left on top of it
# with the twelve apostles, the bullet bag and the priming flask.
KIT17 = ("baldric", "bandolier")
# Pieces that are straps with their hung items (`strap_specs`, `_hung`), not one rigid item.
STRAP_PIECES = ("crossbelts", "baldric", "bandolier")
PIECES = ("crossbelts", "baldric", "bandolier", "belt_plate", "buckle", "cartridge_box", "bayonet_scabbard",
          "hanger_scabbard", "knapsack", "canteen", "canteen_tin")
CANTEENS = ("canteen", "canteen_tin")
# The `line` gear each piece replaces (the painted-on ribbons and the plain boxes).
COVERS = {"crossbelts": ("crossbelts",), "cartridge_box": ("cartridge",), "knapsack": ("knapsack",),
          "bandolier": ("belt_l", "pike_apostles"), "baldric": ("belt_r", "off_scabbard")}
# Source material of `gear_hd.MATS` -> line material key.
MAT_KEY = {"leather": "cartridge", "buff": "belts", "brass": "brass", "steel": "metal", "iron": "metal",
           "wood": "wood", "canvas": "knapsack", "cord": "belts", "walnut": "wood"}
# Triangles per item at regular-unit scale (collapse decimation of the `gear_hd` mesh).
TRIS = {"cartridge_box": 260, "belt_plate": 60, "buckle": 60, "bayonet_scabbard": 110, "hanger_scabbard": 130,
        "hanger_hilt": 140, "knapsack": 420, "canteen": 200, "canteen_tin": 160, "sword_scabbard": 140,
        "apostle": 60, "bullet_bag": 90, "priming_flask": 70}
# The canteens' bone is chosen per build from `CANTEEN_BONES` (`_canteen_bone`).
BONES = {"cartridge_box": "hips", "bayonet_scabbard": "hips", "hanger_scabbard": "hips", "hanger_hilt": "hips",
         "canteen": "hips", "canteen_tin": "hips", "knapsack": "spine2", "belt_plate": "spine1", "buckle": "spine2",
         "sword_scabbard": "hips", "apostle": "spine", "bullet_bag": "hips", "priming_flask": "hips"}
# Bones a canteen may ride, tried in walk and kept on the fewest coat vertices inside it: on
# the back of the left hip the skirt swings with the thigh (the HD kit: 98 coat vertices).
CANTEEN_BONES = ("hips", "thigh_l")
WALK = ("MOTION_L", "MOTION_R")
# Where the line soldier's items sit when not as `kit_hd.ITEMS` (same fields): the box
# lower and further round the hip, so the pack's bottom does not swing into it in a deep bend
# (EngM18: 7 vertices 0.05 m deep in ATTACK with the HD place). The pack stays at the HD
# height: 5 cm higher, the coat's shoulders rose 0.08 m into its top corners when aiming.
PLACE = {
    "cartridge_box": ("hips", (0.0, 0.0, 30.0), (-0.13, 0.10, 0.90), (-0.55, 1.0, 0.0)),
    # The canteen behind the left hip, below the pack's bottom and inside the scabbards
    # (at x 0.13 the scabbards' tips met it: SpnM18 457 vertices); its face along the skirt.
    "canteen": ("hips", (0.0, 0.0, -10.0), (0.05, 0.14, 0.86), (0.2, 1.0, 0.0)),
    "canteen_tin": ("hips", (0.0, 0.0, -10.0), (0.06, 0.14, 0.87), (0.2, 1.0, 0.0)),
    # The musketeer's sword on the baldric's end, as the line's hanger hangs.
    "sword_scabbard": ("hips", (28.0, 0.0, 90.0), (0.18, 0.05, 0.93), (1.0, 0.25, 0.0)),
}
# Straps (m): the canteen's narrow strap over the right shoulder nearer the neck than the
# waist belt (so it shows as its own strap), to the left hip; baldric and bandolier.
CANTEEN_STRAP = ((-0.08, 0.0, 1.47), (0.17, 0.0, 0.95), 0.022)
BALDRIC_W, BANDOLIER_W = 0.065, 0.045
# The apostles: (run, t from the shoulder to the hip along the strap's line) per bottle,
# five down the front (the bullet bag at its end), seven down the back (the priming flask
# below its end); spaced so the hanging
# bottles (0.030 m across) do not touch: the strap spans 0.29 m in X.
APOSTLES = tuple(("front", 0.26 + 0.135 * k) for k in range(5)) + tuple(("back", 0.26 + 0.105 * k) for k in range(7))
APOSTLE_W = 0.030
# Gear built before the kit's straps so they lie over it (the falling band poked out through
# the solid straps 139 vertices at rest when cast onto them).
UNDER_KIT = ("falling_band",)
# Samples over a strap's width (crossbelts, pack straps) and rows along it (m): with 2 across
# the coat's folds came up between the edges (EngM18 coat through belts 235 vertices at rest).
PACK_CLEAR = 0.004  # m: the knapsack off the coat's back (`kit_hd.seat`)
ACROSS, PACK_ACROSS, STEP = 3, 2, 0.03
PACK_STRAPS = True  # the knapsack's straps over the crossbelts


def covered(gear, kit):
    """The line `gear` names left when `kit` is worn (the pieces it replaces dropped)."""
    drop = {g for piece in (kit or ()) for g in COVERS.get(piece, ())}
    return tuple(g for g in gear if g not in drop)


def check(kit):
    unknown = [p for p in kit or () if p not in PIECES]
    if unknown:
        raise KeyError(f"unknown kit pieces {unknown} (have {PIECES})")
    return tuple(kit or ())


def material_keys(slot_names):
    """Line material keys of a `gear_hd` mesh's slots (`hd.src.<key>`)."""
    return [MAT_KEY[n.rsplit(".", 1)[-1]] for n in slot_names]


def strap_specs(kit):
    """(object suffix, from, to, width) of each strap, laid in this order."""
    from mathutils import Vector

    from . import kit_hd, straps

    out = []
    if "baldric" in kit:
        out.append(("baldric", straps.RIGHT_SHOULDER, straps.LEFT_HIP, BALDRIC_W))
    if "bandolier" in kit:
        out.append(("bandolier", straps.LEFT_SHOULDER, straps.RIGHT_HIP, BANDOLIER_W))
    if "crossbelts" in kit:
        out += [("crossbelts", straps.LEFT_SHOULDER, straps.RIGHT_HIP, kit_hd.BELT_W),
                ("crossbelt2", straps.RIGHT_SHOULDER, straps.LEFT_HIP, kit_hd.BELT_W)]
    if "knapsack" in kit and PACK_STRAPS:
        out += [(f"knapsack_strap_{s}", Vector((0.09 * sx, 0, 1.50)), Vector((0.135 * sx, 0, 1.30)), kit_hd.PACK_W)
                for s, sx in (("l", 1), ("r", -1))]
    if any(c in kit for c in CANTEENS):
        a, b, w = CANTEEN_STRAP
        out.append(("canteen_strap", Vector(a), Vector(b), w))
    return out


def _weigh(ob, under, laid, across):
    """Weights from the surface under each section, the outer twin like the inner; no arms."""
    from . import drape, kit_hd

    ws = drape.transfer_weights(ob, under[0], None, keep=4, under=[*under[1:], *laid])
    k = 2 * across
    for i in range(len(ws)):
        if (i % k) >= across:
            ws[i] = ws[i - across]
    ob.vertex_groups.clear()
    groups = {}
    for i, w in enumerate(ws):
        for b, x in sorted(kit_hd.trunk_weights(w).items()):
            if b not in groups:
                groups[b] = ob.vertex_groups.new(name=b)
            groups[b].add([i], x, "REPLACE")


def inner_faces(faces, across):
    """Indices of a solid strap's faces on its inner side (`kit_hd.strap_faces` numbering)."""
    k = 2 * across
    return [i for i, f in enumerate(faces) if len(f) == 4 and all(v % k < across for v in f)]


def _open_under(ob, across):
    """Drops the strap's inner face: it lies on the coat, never seen, and faces into it (the
    layer check read every coat vertex under a closed strap as poking through it)."""
    import bmesh

    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bm.faces.ensure_lookup_table()
    drop = [bm.faces[i] for i in inner_faces([[v.index for v in f.verts] for f in bm.faces], across)]
    bmesh.ops.delete(bm, geom=drop, context="FACES_ONLY")
    bm.to_mesh(ob.data)
    bm.free()


def straps(ch, name, kit):
    """Lays the kit's straps over the draped clothes (rest pose); returns them."""
    from . import armature as am
    from . import kit_hd

    cloth = list(ch.layers["clothes"])
    under = list(ch.strap_under)
    laid = []
    for suffix, frm, to, w in strap_specs(kit):
        tree = kit_hd.surface_tree([*cloth, *laid])
        across = PACK_ACROSS if suffix.startswith(("knapsack", "canteen")) else ACROSS
        b = kit_hd.lay_strap(f"{name}.{suffix}", tree, frm, to, w, kit_hd.BELT_OFFSET, kit_hd.BELT_THICK, across, STEP)
        _weigh(b, under, laid, across)
        _open_under(b, across)
        am.bind(b, ch.arm)
        laid.append(b)
    for b in laid:
        ch.add("clothes", b, "belts", symmetric=True)
    from . import gear_hd

    ch.kit_stats = {"straps_tris": {b.name.rsplit(".", 1)[-1]: gear_hd._tris(b) for b in laid}}
    ch.kit_straps = {b.name.rsplit(".", 1)[-1]: b for b in laid}
    return laid


def _decimate(ob, target):
    import bpy

    from . import gear_hd

    tris = gear_hd._tris(ob)
    if tris > target:
        d = ob.modifiers.new("dec", "DECIMATE")
        d.ratio = target / tris
        d.use_collapse_triangulate = True
        gear_hd._select_only(ob)
        bpy.ops.object.modifier_apply(modifier="dec")
    return gear_hd._tris(ob)


BACK_BAND = 0.015  # m: the item's vertices this near its back (along the seat axis) rest on the surface
SEAT_GAP = 0.004  # m between the item's back and the surface under it
SEAT_TURN = 35.0  # degrees at most the item turns onto the surface under it
# The gap is set on this quantile of the back's rays: on the outermost ray the pleats at the
# skirt's side vent held the cartridge box 0.11 m off the rest of the skirt (median gap).
SEAT_QUANTILE = 0.3
# A flat canteen on the curved skirt: on the lower quantile (EngM18 tin canteen at 0.3: 65
# clothes vertices inside it over the walk).
SEAT_QUANTILES = {"canteen": 0.1, "canteen_tin": 0.1}


def _back_gaps(pts, srcs, a):
    """[(gap, surface name, normal)] under the back of `pts` (within `BACK_BAND` of the
    rearmost along `a`): the outermost surface each ray from outside meets."""
    back = min(p.dot(a) for p in pts)
    out = []
    for p in pts:
        if p.dot(a) > back + BACK_BAND:
            continue
        best = None
        for nm, tree in srcs:
            hit, nrm, _i, _d = tree.ray_cast(p + a * 0.25, -a, 0.5)
            if hit is not None:
                g = (p - hit).dot(a)
                if best is None or g < best[0]:
                    best = (g, nm, nrm if nrm.dot(a) >= 0 else -nrm)
        if best is not None:
            out.append(best)
    out.sort(key=lambda e: e[0])
    return out


def seat_back(ob, ch, matrix, axis, gap=SEAT_GAP, over=(), quantile=SEAT_QUANTILE):
    """Places `ob` (rest at the origin) by `matrix`, turns it (at most `SEAT_TURN`) so its
    back lies along the surface under it, then moves it along the surface's normal so its
    back (the vertices within `BACK_BAND` of its rearmost) rests `gap` off the first surface
    its rays meet: the clothes other than belts (the coat skirt included, which stays under
    it) and the skin, arm faces left out, and the kit straps named in `over` (a bottle of the
    bandolier hangs over the baldric). Returns (shift, the final matrix, report: the surface,
    the gaps under the back after the seat)."""
    import math

    from mathutils import Matrix, Quaternion, Vector

    from . import kit_hd

    a = Vector(axis).normalized()
    srcs = [(o.name.rsplit(".", 1)[-1], kit_hd.trunk_tree([o]))
            for o in [*ch.layers["clothes"], ch.flesh]
            if ch.keys.get(o.name) != "belts" or o.name.rsplit(".", 1)[-1] in over]
    full = matrix
    pts = [full @ v.co for v in ob.data.vertices]
    gaps = _back_gaps(pts, srcs, a)
    near = [n for g, _nm, n in gaps if g < gaps[0][0] + 0.03] if gaps else []
    if near:
        n = sum(near, Vector()).normalized()
        q = a.rotation_difference(n)
        if q.angle > math.radians(SEAT_TURN):
            q = Quaternion(q.axis, math.radians(SEAT_TURN))
        c = sum(pts, Vector()) / len(pts)
        full = Matrix.Translation(c) @ q.to_matrix().to_4x4() @ Matrix.Translation(-c) @ full
        a = q @ a
        pts = [full @ v.co for v in ob.data.vertices]
        gaps = _back_gaps(pts, srcs, a)
    report = {"rays": len(gaps)}
    shift = 0.0
    if gaps:
        g = gaps[min(len(gaps) - 1, int(len(gaps) * quantile))][0]
        shift = gap - g
        mid = gaps[len(gaps) // 2][0]
        report.update({"on": gaps[0][1], "gap_min_m": round(gaps[0][0] + shift, 4),
                       "gap_median_m": round(mid + shift, 4), "gap_max_m": round(gaps[-1][0] + shift, 4)})
    full = Matrix.Translation(a * shift) @ full
    ob.data.transform(full)
    ob.matrix_world = Matrix.Identity(4)
    return round(shift, 4), full, report


def items(ch, name, kit):
    """Builds and seats the kit's rigid items (rest pose); returns {piece: object}."""
    import math

    import bpy
    from mathutils import Euler, Matrix

    from . import gear_hd, kit_hd, straps

    bpy.context.view_layer.update()
    trunk = [o for o in ch.layers["clothes"] if ch.keys.get(o.name) != "belts"]
    tree = kit_hd.trunk_tree([*trunk, ch.flesh])
    belt_tree = kit_hd.trunk_tree([*ch.layers["clothes"], ch.flesh])
    placed = {k: PLACE.get(k, v) for k, v in kit_hd.ITEMS.items() if k in kit}
    for k in CANTEENS:
        if k in kit:
            placed[k] = PLACE[k]
    if "baldric" in kit:
        placed["sword_scabbard"] = PLACE["sword_scabbard"]
    if "belt_plate" in kit:
        L, R = straps.RIGHT_SHOULDER, straps.LEFT_HIP  # on the waist belt, mid-chest
        z = L.z + (R.z - L.z) * 0.12 / 0.29
        placed["belt_plate"] = ("spine1", (0.0, 0.0, 0.0), (0.0, -0.10, z), (0.0, -1.0, 0.0))
    if "buckle" in kit:
        L, R = straps.LEFT_SHOULDER, straps.RIGHT_HIP
        ang = math.degrees(math.atan2(L.z - R.z, L.x - R.x))
        placed["buckle"] = ("spine2", (0.0, 90.0 - ang, 0.0), (L.x + (R.x - L.x) * 0.22, -0.10, L.z + (R.z - L.z) * 0.22),
                            (0.0, -1.0, 0.0))
    order = [k for k in (*PIECES, "sword_scabbard") if k in placed]
    if "hanger_scabbard" in placed:
        order.append("hanger_hilt")
    st = ch.kit_stats.setdefault("items", {})
    finals, out = {}, {}
    for key in order:
        ob = MESHES[key](f"{name}.{key}") if key in MESHES else gear_hd.ITEMS[key][0](f"{name}.{key}")
        keys = material_keys([m.name for m in ob.data.materials])
        tris = _decimate(ob, TRIS[key])
        rep = {}
        if key in FOLLOW:
            other, offset = FOLLOW[key]
            shift, _f = kit_hd.seat([ob], None, finals[other] @ Matrix.Translation(offset), (0.0, 0.0, 1.0))
        else:
            _bone, rot, centre, axis = placed[key]
            m = Matrix.Translation(centre) @ Euler([math.radians(r) for r in rot], "ZXY").to_matrix().to_4x4()
            if key in ("belt_plate", "buckle", "knapsack"):
                shift, finals[key] = kit_hd.seat([ob], belt_tree if key != "knapsack" else tree, m, axis,
                                                 clear=PACK_CLEAR if key == "knapsack" else kit_hd.CLEAR)
            else:
                shift, finals[key], rep = seat_back(ob, ch, m, axis, quantile=SEAT_QUANTILES.get(key, SEAT_QUANTILE))
        # The knapsack too: with the coat's spine weights (`kit_hd.follow_back`) the items met
        # in 234 vertices (EngM18) against 0 rigid on spine2.
        kit_hd.rigid(ob, ch.arm, BONES[key])
        ob["acr_material_keys"] = keys
        ch.add("gear", ob, keys[0])
        st[key] = {"bone": BONES[key], "shift_m": shift, "tris": tris, **rep}
        out[key] = ob
    if "bandolier" in kit:
        out.update(_hung(ch, name, st))
    return out


# Items seated with another. The musketeer's scabbard has no hilt: he draws the sword for
# melee (`line.add_sidearm`, shown by its socket) and a rigid hilt could not be hidden then.
FOLLOW = {"hanger_hilt": ("hanger_scabbard", (0.0, 0.0, 0.012))}  # as `kit_hd.FOLLOW`


def tin_canteen_mesh(name="canteen_tin"):
    """A flat oval tin canteen (the British kidney tin, the French `bidon`): 0.19 x 0.22 m,
    0.055 m deep, axis along Y, a rolled seam round the rim, the spout and its cork, two
    strap loops."""
    from mathutils import Matrix

    from . import gear_hd

    b = gear_hd.Builder()
    R, D = 0.11, 0.055
    prof = [(0.0, R - 0.025), (0.05, R - 0.006), (0.12, R - 0.001), (0.5, R), (0.88, R - 0.001), (0.95, R - 0.006),
            (1.0, R - 0.025)]
    b.tube(-D / 2, D / 2, prof, "steel", n=32, axis="y")
    b.tube(-0.003, 0.003, [(0, R + 0.0005), (0.5, R + 0.003), (1, R + 0.0005)], "steel", n=32, axis="y", caps=(False, False))
    b.tube(R - 0.01, R + 0.02, [(0, 0.011), (0.7, 0.010), (1.0, 0.009)], "steel", n=12)
    b.tube(R + 0.018, R + 0.036, [(0, 0.0085), (0.7, 0.0095), (1.0, 0.007)], "cord", n=10)
    for x in (0.075, -0.075):
        b.box((x, 0.0, R * 0.62), (0.014, 0.012, 0.022), "steel", r=0.003, n=2)
    ob = gear_hd._finish_mesh(b, name)
    ob.data.transform(Matrix.Diagonal((0.86, 1.0, 1.0, 1.0)))
    return ob


def apostle_mesh(name="apostle"):
    """One of the twelve apostles: a turned wooden powder bottle with a leather-covered cap
    on its cord, 0.11 m tall, origin at its middle."""
    from . import gear_hd

    b = gear_hd.Builder()
    b.tube(-0.045, 0.025, [(0, 0.011), (0.1, 0.014), (0.85, 0.014), (1, 0.010)], "wood", n=10)
    b.tube(0.025, 0.052, [(0, APOSTLE_W / 2), (0.6, 0.014), (1, 0.006)], "leather", n=10)
    b.tube(0.052, 0.066, [(0, 0.0025), (1, 0.0025)], "cord", n=4)
    return gear_hd._finish_mesh(b, name)


def bullet_bag_mesh(name="bullet_bag"):
    """The leather bullet bag at the bandolier's end, drawn shut with a cord (flat against
    the hip: 0.09 x 0.05 m, 0.12 m tall)."""
    from mathutils import Matrix

    from . import gear_hd

    b = gear_hd.Builder()
    b.tube(-0.06, 0.045, [(0, 0.012), (0.15, 0.036), (0.5, 0.045), (0.85, 0.032), (1, 0.014)], "leather", n=12)
    b.tube(0.045, 0.06, [(0, 0.016), (0.5, 0.020), (1, 0.010)], "leather", n=12)
    b.tube(0.04, 0.05, [(0, 0.0175), (1, 0.0175)], "cord", n=12, caps=(False, False))
    ob = gear_hd._finish_mesh(b, name)
    ob.data.transform(Matrix.Diagonal((1.0, 0.55, 1.0, 1.0)))
    return ob


def priming_flask_mesh(name="priming_flask"):
    """The small priming flask (fine powder for the pan): a flattened horn with a brass
    nozzle, 0.15 m, hung at the right hip behind the bullet bag."""
    from mathutils import Matrix

    from . import gear_hd

    b = gear_hd.Builder()
    b.tube(-0.065, 0.045, [(0, 0.008), (0.3, 0.018), (0.75, 0.022), (1, 0.019)], "wood", n=10)
    b.tube(0.045, 0.052, [(0, 0.020), (1, 0.019)], "brass", n=10)
    b.tube(-0.085, -0.065, [(0, 0.0045), (1, 0.007)], "brass", n=8)
    ob = gear_hd._finish_mesh(b, name)
    ob.data.transform(Matrix.Diagonal((1.0, 0.6, 1.0, 1.0)))
    return ob


def _sword_scabbard(name):
    from . import gear_hd

    return gear_hd.scabbard_mesh(name, length=0.70, width=0.040)

MESHES = {"canteen_tin": tin_canteen_mesh, "sword_scabbard": _sword_scabbard,
          "apostle": apostle_mesh, "bullet_bag": bullet_bag_mesh, "priming_flask": priming_flask_mesh}


def _strap_point(strap, run, t, a, b):
    """The vertex of `strap` (rest, world) on its `run` ("front": -Y, "back": +Y) nearest the
    line `a` -> `b` at `t` in X-Z, and the strap's dominant bone there (reported only)."""
    from mathutils import Vector

    c = a.lerp(b, t)
    back = run == "back"
    best = None
    for v in strap.data.vertices:
        if (v.co.y > 0) != back:
            continue
        d = (Vector((v.co.x, v.co.z)) - Vector((c.x, c.z))).length
        if best is None or d < best[0]:
            best = (d, v)
    v = best[1]
    names = {g.index: g.name for g in strap.vertex_groups}
    g = max(v.groups, key=lambda e: e.weight)
    return v.co.copy(), names[g.group]


def _hung(ch, name, st):
    """The bandolier's apostles (`APOSTLES`), bullet bag and priming flask: each hung just
    under the strap, seated on the clothes (and the baldric) under it (`seat_back`) and rigid
    on its `BONES` bone (the apostles all on one)."""
    import math

    from mathutils import Euler, Matrix, Vector

    from . import kit_hd, straps

    strap = ch.kit_straps["bandolier"]
    a, b = straps.LEFT_SHOULDER, straps.RIGHT_HIP
    out, rows = {}, []
    pieces = [("apostle", run, t) for run, t in APOSTLES] + [("bullet_bag", "front", 0.97), ("priming_flask", "front", 0.97)]
    for i, (key, run, t) in enumerate(pieces):
        p, _bone = _strap_point(strap, run, t, a, b)
        sign = 1 if run == "back" else -1
        if key == "apostle":
            # under the strap's lower edge (a 61 degree diagonal: 0.05 m below its centre line)
            centre = Vector((p.x, p.y + sign * 0.03, p.z - 0.055 - 0.066))
            rot, axis = (0.0, 0.0, 0.0), (0.0, sign, 0.0)
        elif key == "bullet_bag":
            centre = Vector((-0.17, -0.20, 0.87))
            rot, axis = (25.0, 0.0, 0.0), (-0.45, -1.0, 0.0)
        else:
            centre = Vector((-0.215, 0.0, 0.90))
            rot, axis = (0.0, 0.0, 0.0), (-1.0, -0.2, 0.0)
        nm = f"{name}.{key}{i:02d}" if key == "apostle" else f"{name}.{key}"
        ob = MESHES[key](nm)
        keys = material_keys([m.name for m in ob.data.materials])
        tris = _decimate(ob, TRIS[key])
        m = Matrix.Translation(centre) @ Euler([math.radians(r) for r in rot], "ZXY").to_matrix().to_4x4()
        shift, _f, rep = seat_back(ob, ch, m, axis, over=("baldric",))
        # One bone for the row: on the strap's dominant bone each, neighbours on spine1 and
        # spine2 swung into each other (EngMus 188 vertices over the clips).
        bone = BONES[key]
        kit_hd.rigid(ob, ch.arm, bone)
        ob["acr_material_keys"] = keys
        ch.add("gear", ob, keys[0])
        out[nm.rsplit(".", 1)[-1]] = ob
        row = {"bone": bone, "shift_m": shift, "tris": tris, **rep}
        if key == "apostle":
            rows.append(row)
        else:
            st[key] = row
    gaps = [r["gap_median_m"] for r in rows if "gap_median_m" in r]
    st["apostles"] = {"n": len(rows), "tris": sum(r["tris"] for r in rows), "bones": sorted({r["bone"] for r in rows}),
                      "gap_median_m": [min(gaps), max(gaps)] if gaps else None,
                      "shift_m": [min(r["shift_m"] for r in rows), max(r["shift_m"] for r in rows)]}
    return out


def _coat(ch):
    """The outer body garment: the line coat, else the doublet or buff coat, else the
    largest of the clothes."""
    clothes = [o for o in ch.layers["clothes"] if ch.keys.get(o.name) != "belts"]
    for key in ("coat", "doublet", "buffcoat"):
        exact = [o for o in clothes if ch.keys.get(o.name) == key and o.name.endswith("." + key)]
        any_ = [o for o in clothes if ch.keys.get(o.name) == key]
        if exact or any_:
            return (exact or any_)[0]
    return max(clothes, key=lambda o: len(o.data.vertices))


def _canteen_bone(ch, clips):
    """Rigs the canteen on the bone of `CANTEEN_BONES` with the fewest clothes vertices
    inside it over the walk clips (every garment but the belts: the skirts are their own
    object); returns {bone: vertices} and the bone kept."""
    from . import kit_hd

    key = next(k for k in CANTEENS if k in ch.kit_items)
    ob = ch.kit_items[key]
    walk = [c for c in clips if c in WALK]
    clothes = [o for o in ch.layers["clothes"] if ch.keys.get(o.name) != "belts"]
    tried = {}
    for bone in CANTEEN_BONES:
        kit_hd.rigid(ob, ch.arm, bone)
        tried[bone] = sum(kit_hd.collisions(ch.arm, {key: ob}, o, walk)["outfit_verts"] for o in clothes)
    best = min(CANTEEN_BONES, key=lambda b: (tried[b], CANTEEN_BONES.index(b)))
    kit_hd.rigid(ob, ch.arm, best)
    ch.kit_stats["items"][key]["bone"] = best
    return {"walk_clothes_verts": tried, "bone": best}


def collisions(ch, clips):
    """Items in items and the coat in items per clip (`kit_hd.collisions`); the canteen's
    bone chosen first (`_canteen_bone`)."""
    from . import kit_hd

    coat = _coat(ch)
    canteen = _canteen_bone(ch, clips) if any(k in ch.kit_items for k in CANTEENS) else None
    out = kit_hd.collisions(ch.arm, ch.kit_items, coat, clips)
    if canteen:
        out["canteen"] = canteen
    if out["worst"] and "knapsack" in ch.kit_items:
        out["coat_in_pack"] = _inside(ch, coat, ch.kit_items["knapsack"], *out["worst"])
    return out


def _inside(ch, coat, item, clip, frame):
    """The coat's vertices deepest inside `item` at `clip` `frame`: [rest x, y, z, depth, bone]."""
    import bpy
    from mathutils.bvhtree import BVHTree

    from . import armature as am
    from . import kit_hd

    am.set_clip(ch.arm, clip, frame)
    dg = bpy.context.evaluated_depsgraph_get()
    ie = item.evaluated_get(dg).data
    tree = BVHTree.FromPolygons([item.matrix_world @ v.co for v in ie.vertices], [p.vertices for p in ie.polygons])
    ce = coat.evaluated_get(dg).data
    dom = kit_hd._dominant(coat)
    lo = [min((item.matrix_world @ v.co)[k] for v in ie.vertices) - 0.01 for k in range(3)]
    hi = [max((item.matrix_world @ v.co)[k] for v in ie.vertices) + 0.01 for k in range(3)]
    hits = []
    for v, r in zip(ce.vertices, coat.data.vertices):
        p = coat.matrix_world @ v.co
        if any(p[k] < lo[k] or p[k] > hi[k] for k in range(3)):
            continue
        loc, nrm, _i, _d = tree.find_nearest(p)
        if loc is not None:
            d = (p - loc).dot(nrm)
            if d < -kit_hd.INSIDE_M:
                hits.append([*(round(x, 3) for x in r.co), round(d, 4), dom[r.index]])
    ch.arm.animation_data.action = None
    return sorted(hits, key=lambda h: h[3])[:8]
