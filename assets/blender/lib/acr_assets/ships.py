"""Sailing ships from Poly Haven's 17th-century Dutch ships (#287; needs `bpy`).

`build(ctx, base, length, budget, draft)` imports `$ACR_SOURCES/polyhaven/<base>/<base>_2k.gltf`,
turns the bow south (models face south), scales it to `length` metres overall, sinks it so
the keel lies `draft` metres below the ground point (the waterline, where the 3D pass draws
the water surface, docs/water.md; the extra `acr_draft` records it), and rigs it:

    hull            rocking (swell), heel in turns, the jolt of a broadside, sinking
      intact        hull and rigging as built (hidden once burnt)
        sail.<k>    one bone per sail, at its centre: slack at anchor, full under way
      wreck         the charred hull and masts (hidden while afloat)

Clips (24 fps, on the game clock: `acr_clock = "world"`): `STAND` (at anchor, 6 s loop),
`MOTION_L`/`MOTION_R` (+ `LB`/`RB`, under sail, heeling), `ATTACK` (broadside), `DEATH` and
`DEATHLIE1` (the same: burnt at once, lists, goes down by the bow over 12 s).
"""

import math

import bpy
from mathutils import Matrix, Vector

from . import vessels as vs

FPS = 24


def _kind(mat_name):
    n = mat_name.lower()
    if "sail" in n:
        return "sails"
    if "rigging" in n:
        return "rigging"
    return "hull"


def _split_loose(obj):
    """Separates `obj` into its loose parts; returns them."""
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.separate(type="LOOSE")
    bpy.ops.object.mode_set(mode="OBJECT")
    return list(bpy.context.selected_objects)


def _centre(obj):
    vs_ = [v.co for v in obj.data.vertices]
    lo = Vector([min(v[i] for v in vs_) for i in range(3)])
    hi = Vector([max(v[i] for v in vs_) for i in range(3)])
    return (lo + hi) / 2, hi - lo


def _sail_groups(parts, max_groups=12):
    """Groups sail pieces into at most `max_groups` clusters by position (pieces of one sail
    or neighbouring sails move together)."""
    items = [(p, _centre(p)[0]) for p in parts]
    groups = [[it] for it in items]
    while len(groups) > max_groups:
        best = None
        for i in range(len(groups)):
            for j in range(i + 1, len(groups)):
                a = sum((c for _, c in groups[i]), Vector()) / len(groups[i])
                b = sum((c for _, c in groups[j]), Vector()) / len(groups[j])
                d = (a - b).length
                if best is None or d < best[0]:
                    best = (d, i, j)
        _, i, j = best
        groups[i] += groups.pop(j)
    return [[p for p, _ in g] for g in groups]


def build(ctx, base, length, budget, draft, sail_share=0.10, rigging_share=0.25, wreck_share=0.20):
    root = ctx.root
    root["acr_clock"] = "world"
    root["acr_machine"] = True
    objs = vs.import_file(vs.source("polyhaven", base, f"{base}_2k.gltf"))
    source_tris = vs.triangles(objs)
    vs.base_colour_only(objs)
    by = {"hull": [], "rigging": [], "sails": []}
    for o in objs:
        by[_kind(o.material_slots[0].material.name if o.material_slots else "hull")].append(o)
    parts = {k: vs.join(v, f"{root.name}.{k}") for k, v in by.items() if v}
    # Size and orientation: the long axis to X, the bow (where the bowsprit reaches beyond
    # the hull) to +X, then the bow to the south (-Y), `length` overall.
    allv = [v.co for o in parts.values() for v in o.data.vertices]
    if max(v.y for v in allv) - min(v.y for v in allv) > max(v.x for v in allv) - min(v.x for v in allv):
        vs.transform(parts.values(), Matrix.Rotation(math.pi / 2, 4, "Z"))
    hull = [v.co.x for v in parts["hull"].data.vertices]
    allv = [v.co for o in parts.values() for v in o.data.vertices]
    if max(v.x for v in allv) - max(hull) < min(hull) - min(v.x for v in allv):
        vs.transform(parts.values(), Matrix.Rotation(math.pi, 4, "Z"))
    allv = [v.co for o in parts.values() for v in o.data.vertices]
    x0, x1 = min(v.x for v in allv), max(v.x for v in allv)
    k = length / (x1 - x0)
    m = Matrix.Rotation(-math.pi / 2, 4, "Z") @ Matrix.Scale(k, 4) @ Matrix.Translation((-(x0 + x1) / 2, 0, 0))
    vs.transform(parts.values(), m)
    # The draft: the source's zero lies only ~2 m above its keel, so the hull floated on the
    # water surface. The keel goes to `draft` below the waterline (a loaded ship of the time).
    keel = min(v.co.z for v in parts["hull"].data.vertices)
    vs.transform(parts.values(), Matrix.Translation((0, 0, -draft - keel)))
    root["acr_draft"] = draft
    hull_budget = budget * (1 - sail_share - rigging_share - wreck_share)
    stats = {"source_triangles": source_tris}
    stats["hull"] = vs.decimate(parts["hull"], int(hull_budget))
    if "rigging" in parts:
        stats["rigging"] = vs.decimate(parts["rigging"], int(budget * rigging_share))
    # The wreck: charred hull and rigging (fewer triangles; the sails burnt away).
    wreck = [vs.burnt_copy(parts["hull"], f"{root.name}.wreck_hull", int(budget * wreck_share * 0.7))]
    if "rigging" in parts:
        wreck.append(vs.burnt_copy(parts["rigging"], f"{root.name}.wreck_rigging", int(budget * wreck_share * 0.3)))
    stats["wreck"] = vs.triangles(wreck)
    # Afloat, the hull ends 1 m below the waterline (the wreck keeps its bottom): deeper, the
    # terrain's depth hides it anyway (the water does not write depth), and so does the
    # original's tint under `water=original`.
    vs.cut_below(parts["hull"], -1.0)
    stats["hull"] = vs.triangles([parts["hull"]])
    sails = []
    if "sails" in parts:
        vs.decimate(parts["sails"], int(budget * sail_share))
        pieces = _split_loose(parts["sails"])
        groups = _sail_groups(pieces)
        for i, g in enumerate(groups):
            sails.append(vs.join(g, f"{root.name}.sail{i}"))
    stats["sails"] = vs.triangles(sails)
    zmax = max(v.co.z for o in parts.values() for v in o.data.vertices)
    bones = [("hull", (0, 0, 0), None), ("intact", (0, 0, 0), "hull"), ("wreck", (0, 0, 0), "hull")]
    for i, s in enumerate(sails):
        c, _ = _centre(s)
        bones.append((f"sail{i}", tuple(c), "intact"))
    arm = vs.rig(root, f"{root.name}.rig", bones)
    for p in (parts["hull"], parts.get("rigging")):
        if p is not None:
            vs.attach(p, arm, "intact")
    for w in wreck:
        vs.attach(w, arm, "wreck")
    for i, s in enumerate(sails):
        vs.attach(s, arm, f"sail{i}")
    _clips(arm, len(sails), length)
    stats["triangles"] = vs.triangles([o for o in bpy.context.scene.objects if o.type == "MESH"])
    stats["length_m"] = round(length, 2)
    stats["height_m"] = round(zmax, 2)
    stats["draft_m"] = round(draft, 2)
    ctx.stats.update(stats)


def _sails(n, fill):
    """Sail bones at `fill` (1 = as modelled; >1 bellied out forward, <1 slack)."""
    return {f"sail{i}": {"scale": (1.0, fill, 1.0 - 0.04 * (1 - fill))} for i in range(n)}


AFLOAT = {"wreck": {"scale": (vs.HIDDEN,) * 3}, "intact": {}}


def _clips(arm, n, length):
    big = min(1.0, 30.0 / length)  # small boats rock more
    stand = vs.swell(6 * FPS, 1.6 / big ** 0.5, 0.6, 0.12)
    keys = vs.merge(stand, [(0, {**AFLOAT, **_sails(n, 0.55)}), (6 * FPS, _sails(n, 0.55))])
    vs.clip(arm, "STAND", keys, step=("wreck", "intact"))
    for name, heel in (("MOTION_L", -4.0), ("MOTION_R", 4.0), ("MOTION_LB", -2.5), ("MOTION_RB", 2.5)):
        sw = vs.swell(4 * FPS, 1.0, 1.2, 0.18)
        sw = [(f, {"hull": {"rot": (s["hull"]["rot"][0] + 0.8, s["hull"]["rot"][1] + heel, 0),
                            "loc": s["hull"]["loc"]}}) for f, s in sw]
        fill = [(f, _sails(n, 1.25 + 0.08 * math.sin(f / (4 * FPS) * 4 * math.pi))) for f in range(0, 4 * FPS + 1, FPS)]
        keys = vs.merge(sw, fill, [(0, AFLOAT)])
        vs.clip(arm, name, keys, step=("wreck", "intact"))
    # A broadside: the recoil heels her away, she rolls back.
    keys = [(0, {**AFLOAT, "hull": {"rot": (0, 0, 0)}, **_sails(n, 0.9)}),
            (3, {"hull": {"rot": (0, 3.5, 0), "loc": (0, 0, 0.05)}}),
            (18, {"hull": {"rot": (0, -1.5, 0)}}),
            (36, {"hull": {"rot": (0, 0.6, 0)}}),
            (48, {"hull": {"rot": (0, 0, 0)}})]
    vs.clip(arm, "ATTACK", keys, step=("wreck", "intact"))
    # Burnt and sinking: she lists to port, goes down by the bow and under.
    depth = 0.45 * length
    keys = [(0, {"wreck": {"scale": (1, 1, 1)}, "intact": {"scale": (vs.HIDDEN,) * 3}, "hull": {}}),
            (3 * FPS, {"hull": {"rot": (2, -8, 0), "loc": (0, 0, -0.06 * depth)}}),
            (7 * FPS, {"hull": {"rot": (6, -18, 0), "loc": (0, 0, -0.25 * depth)}}),
            (10 * FPS, {"hull": {"rot": (12, -24, 0), "loc": (0, 0, -0.6 * depth)}}),
            (12 * FPS, {"hull": {"rot": (14, -26, 0), "loc": (0, 0, -depth)}})]
    for name in ("DEATH", "DEATHLIE1"):
        vs.clip(arm, name, keys, step=("wreck", "intact"))
