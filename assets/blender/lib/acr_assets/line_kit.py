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
  the body (`kit_hd.seat`), each rigid on one bone (`BONES`; the knapsack on `spine2`, so it
  cannot crumple in a deep bend). They replace the boxes of `line._cartridge` and
  `line._knapsack`.
- **LODs:** no authored levels: the line soldier carries none, and one `acr_lod` node would
  make the whole model "authored" in `model_renderer::prepare_model` (no far meshes, the
  body drawn whole at every level). The kit takes the index levels of `lod.rs` with the rest.

`KITS` holds a kit per nation; `ctx.stats["kit"]`: triangles per piece, seat shifts, the
items' collisions per clip (`kit_hd.collisions`).
"""

# Per nation, the pieces worn (period notes in docs/families/line.md, "Worn kit").
KITS = {
    "eng": ("crossbelts", "belt_plate", "cartridge_box", "bayonet_scabbard", "knapsack"),
    "frn": ("crossbelts", "cartridge_box", "bayonet_scabbard", "hanger_scabbard", "knapsack"),
    "spn": ("crossbelts", "cartridge_box", "bayonet_scabbard", "hanger_scabbard", "knapsack"),
    "usa": ("crossbelts", "cartridge_box", "bayonet_scabbard", "knapsack"),
}
PIECES = ("crossbelts", "belt_plate", "buckle", "cartridge_box", "bayonet_scabbard", "hanger_scabbard", "knapsack", "canteen")
# The `line` gear each piece replaces (the painted-on ribbons and the plain boxes).
COVERS = {"crossbelts": ("crossbelts",), "cartridge_box": ("cartridge",), "knapsack": ("knapsack",)}
# Source material of `gear_hd.MATS` -> line material key.
MAT_KEY = {"leather": "cartridge", "buff": "belts", "brass": "brass", "steel": "metal", "iron": "metal",
           "wood": "wood", "canvas": "knapsack", "cord": "belts", "walnut": "wood"}
# Triangles per item at regular-unit scale (collapse decimation of the `gear_hd` mesh).
TRIS = {"cartridge_box": 260, "belt_plate": 60, "buckle": 60, "bayonet_scabbard": 110, "hanger_scabbard": 130,
        "hanger_hilt": 140, "knapsack": 420, "canteen": 200}
BONES = {"cartridge_box": "hips", "bayonet_scabbard": "hips", "hanger_scabbard": "hips", "hanger_hilt": "hips",
         "canteen": "hips", "knapsack": "spine2", "belt_plate": "spine1", "buckle": "spine2"}
# Where the line soldier's items sit when not as `kit_hd.ITEMS` (same fields): the box
# lower and further round the hip, so the pack's bottom does not swing into it in a deep bend
# (EngM18: 7 vertices 0.05 m deep in ATTACK with the HD place). The pack stays at the HD
# height: 5 cm higher, the coat's shoulders rose 0.08 m into its top corners when aiming.
PLACE = {
    "cartridge_box": ("hips", (0.0, 0.0, 40.0), (-0.16, 0.07, 0.89), (-0.8, 1.0, 0.0)),
}
# Samples over a strap's width (crossbelts, pack straps) and rows along it (m): with 2 across
# the coat's folds came up between the edges (EngM18 coat through belts 235 vertices at rest).
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
    if "crossbelts" in kit:
        out += [("crossbelts", straps.LEFT_SHOULDER, straps.RIGHT_HIP, kit_hd.BELT_W),
                ("crossbelt2", straps.RIGHT_SHOULDER, straps.LEFT_HIP, kit_hd.BELT_W)]
    if "knapsack" in kit and PACK_STRAPS:
        out += [(f"knapsack_strap_{s}", Vector((0.09 * sx, 0, 1.50)), Vector((0.135 * sx, 0, 1.30)), kit_hd.PACK_W)
                for s, sx in (("l", 1), ("r", -1))]
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
        across = PACK_ACROSS if suffix.startswith("knapsack") else ACROSS
        b = kit_hd.lay_strap(f"{name}.{suffix}", tree, frm, to, w, kit_hd.BELT_OFFSET, kit_hd.BELT_THICK, across, STEP)
        _weigh(b, under, laid, across)
        _open_under(b, across)
        am.bind(b, ch.arm)
        laid.append(b)
    for b in laid:
        ch.add("clothes", b, "belts", symmetric=True)
    from . import gear_hd

    ch.kit_stats = {"straps_tris": {b.name.rsplit(".", 1)[-1]: gear_hd._tris(b) for b in laid}}
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
    if "belt_plate" in kit:
        L, R = straps.RIGHT_SHOULDER, straps.LEFT_HIP  # on the waist belt, mid-chest
        z = L.z + (R.z - L.z) * 0.12 / 0.29
        placed["belt_plate"] = ("spine1", (0.0, 0.0, 0.0), (0.0, -0.10, z), (0.0, -1.0, 0.0))
    if "buckle" in kit:
        L, R = straps.LEFT_SHOULDER, straps.RIGHT_HIP
        ang = math.degrees(math.atan2(L.z - R.z, L.x - R.x))
        placed["buckle"] = ("spine2", (0.0, 90.0 - ang, 0.0), (L.x + (R.x - L.x) * 0.22, -0.10, L.z + (R.z - L.z) * 0.22),
                            (0.0, -1.0, 0.0))
    order = [k for k in PIECES if k in placed]
    if "hanger_scabbard" in placed:
        order.append("hanger_hilt")
    st = ch.kit_stats.setdefault("items", {})
    finals, out = {}, {}
    for key in order:
        fn, _px = gear_hd.ITEMS[key]
        ob = fn(f"{name}.{key}")
        keys = material_keys([m.name for m in ob.data.materials])
        tris = _decimate(ob, TRIS[key])
        if key in kit_hd.FOLLOW:
            other, offset = kit_hd.FOLLOW[key]
            shift, _f = kit_hd.seat([ob], None, finals[other] @ Matrix.Translation(offset), (0.0, 0.0, 1.0))
        else:
            _bone, rot, centre, axis = placed[key]
            m = Matrix.Translation(centre) @ Euler([math.radians(r) for r in rot], "ZXY").to_matrix().to_4x4()
            shift, finals[key] = kit_hd.seat([ob], belt_tree if key in ("belt_plate", "buckle") else tree, m, axis)
        kit_hd.rigid(ob, ch.arm, BONES[key])
        ob["acr_material_keys"] = keys
        ch.add("gear", ob, keys[0])
        st[key] = {"bone": BONES[key], "shift_m": shift, "tris": tris}
        out[key] = ob
    return out


def collisions(ch, clips):
    """Items in items and the coat in items per clip (`kit_hd.collisions`)."""
    coat = next((o for o in ch.layers["clothes"] if ch.keys.get(o.name) == "coat" and o.name.endswith(".coat")), None)
    coat = coat or next(o for o in ch.layers["clothes"] if ch.keys.get(o.name) == "coat")
    from . import kit_hd

    return kit_hd.collisions(ch.arm, ch.kit_items, coat, clips)
