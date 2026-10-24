"""ArtMrt: the mortar of the optional rule `mortars` (#301). Barrel and bed are MisterF1sh's
"Old Mortar Cannon" (CC-BY 4.0, see assets/LICENSES.md) without its four truck wheels: a
short iron mortar on a planked bed with iron rings, set on the ground; the barrel raised
from the model's 34° to the shell's 60° (`GunMode::Mortar`).

Rig: `gun` (whole piece: the jolt of the shot) > `bed` > `intact`/`wreck` (bed, charred
copy), `barrel` (at the trunnions: kick) > `flash` (the fire at the muzzle, shown from the
shot frame for 5 frames). The sim draws the mortar with the field gun's `.MD` (`ArtPus`),
so the clips are the cannon's (`artpus.py`): `STAND`, `GUN_PUSH`, `CHEST_CARRY`,
`TRAIL_LIFT`, `TRAIL_WALK`, `DEPLOY`, `ATTACK` (420 frames, the shot at 29), `DEATH` (73)
and `DEATHLIE1`. The crew of three (`acr_assets.mortarcrew`, bones `c0.`..`c2.`): two
loaders and the firer with the linstock.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from acr_assets import guncrew, mortarcrew
from acr_assets import vessels as vs

# The `.MD` the mortar draws with (the sim keeps `md_name` "ArtPus"); the game finds this model
# by its file name, `artmrt.glb`, from the type id `ArtMrt(XX)` (`scene::definition_name`).
TYPE = "ArtPus"

SRC = ("sketchfab3", "artillery", "old_mortar_cannon-160f87", "scene.gltf")
SCALE = 1.1
ELEVATION = 60.0  # degrees (the shell's, acr-sim `mortar::ELEVATION`)
FLASH_FRAMES = 5


def _import():
    objs = vs.import_file(vs.source(*SRC))
    keep, barrel = [], []
    for o in objs:
        if "Wheel" in o.name:
            bpy.data.objects.remove(o, do_unlink=True)
        elif o.name.startswith(("Barrel", "Rope")):
            barrel.append(o)
        else:
            keep.append(o)
    # The bed down on the ground (it stood on its wheels), at the build's scale.
    allv = [v.co for o in keep for v in o.data.vertices]
    lift = -min(v.z for v in allv)
    vs.transform(keep + barrel, Matrix.Scale(SCALE, 4) @ Matrix.Translation((0, 0, lift)))
    return keep, barrel


def _axis(obj):
    """(centre, unit axis towards the muzzle) of the barrel: its longest principal axis."""
    import numpy as np

    ws = [v.co.copy() for v in obj.data.vertices]
    c = sum(ws, Vector()) / len(ws)
    a = np.array([list(v - c) for v in ws])
    _w, V = np.linalg.eigh(a.T @ a)
    ax = Vector(V[:, 2].tolist()).normalized()
    if ax.y > 0:  # towards the muzzle (-Y, up)
        ax = -ax
    return c, ax


def _elevate(barrel):
    """Raises the barrel about its trunnions to ELEVATION; returns (trunnion, axis, muzzle
    centre, muzzle rim radius, vent)."""
    b = barrel[0]
    vs_ = [v.co for v in b.data.vertices]
    wide = max(abs(v.x) for v in vs_)
    side = [v for v in vs_ if abs(v.x) > wide - 0.06 * SCALE]
    t = sum(side, Vector()) / len(side)
    t.x = 0.0
    _c, ax = _axis(b)
    now = math.degrees(math.asin(max(-1.0, min(1.0, ax.z))))
    turn = Matrix.Translation(t) @ Matrix.Rotation(math.radians(-(ELEVATION - now)), 4, "X") @ Matrix.Translation(-t)
    vs.transform(barrel, turn)
    _c, ax = _axis(b)
    vs_ = [v.co.copy() for v in b.data.vertices]
    along = [(v - t).dot(ax) for v in vs_]
    front, back = max(along), min(along)
    rim = [v for v, a in zip(vs_, along) if a > front - 0.03]
    centre = t + ax * front
    radius = max(((v - t) - ax * (v - t).dot(ax)).length for v in rim)
    # The vent: on top of the breech (the side away from the ground), a little before its end.
    up = Vector((0, 0, 1)) - ax * ax.z
    up.normalize()
    at = back + 0.12 * SCALE
    ring = [v for v, a in zip(vs_, along) if abs(a - at) < 0.04 and abs(v.x) < 0.05] or vs_
    top = max(ring, key=lambda v: (v - t).dot(up))
    vent = top + up * 0.01
    return t, ax, centre, radius, vent, now


def _flash(centre, ax, radius):
    """The fire of the shot: a cone of flame out of the muzzle and a puff of smoke round it,
    built at the muzzle along the barrel's axis."""
    bm = bmesh.new()
    rot = ax.to_track_quat("Z", "Y").to_matrix().to_4x4()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=10, radius1=radius * 0.9, radius2=0.02, depth=0.9,
                          matrix=Matrix.Translation(centre + ax * 0.45) @ rot)
    fire = bpy.data.meshes.new("artmrt.flash_fire")
    bm.to_mesh(fire)
    bm.free()
    bm = bmesh.new()
    for i, (d, r) in enumerate(((0.15, 0.32), (0.45, 0.28), (0.25, 0.22))):
        side = Vector((0.18 * (1 if i % 2 else -1), 0, 0))
        bmesh.ops.create_icosphere(bm, subdivisions=2, radius=r * SCALE,
                                   matrix=Matrix.Translation(centre + ax * d + side))
    smoke = bpy.data.meshes.new("artmrt.flash_smoke")
    bm.to_mesh(smoke)
    bm.free()
    out = []
    for me, rgb in ((fire, (1.0, 0.62, 0.12)), (smoke, (0.62, 0.60, 0.56))):
        o = bpy.data.objects.new(me.name, me)
        bpy.context.scene.collection.objects.link(o)
        o.data.materials.append(vs.flat_material(me.name, rgb))
        out.append(o)
    return vs.join(out, "artmrt.flash")


def _marks(arm, vent, rims):
    """{name: (bone, local point)} for the crew's IK: the vent, the muzzle's rim each side."""
    for pb in arm.pose.bones:
        pb.location, pb.scale = (0, 0, 0), (1, 1, 1)
        pb.rotation_mode = "QUATERNION"
        pb.rotation_quaternion = (1, 0, 0, 0)
    bpy.context.view_layer.update()
    m = arm.pose.bones["barrel"].matrix.inverted()
    out = {"barrel": ("barrel", m @ vent)}
    for k, p in rims.items():
        out[k] = ("barrel", m @ p)
    return out


def build(ctx):
    root = ctx.root
    root["acr_machine"] = True
    bed_parts, barrel = _import()
    t, ax, muzzle, radius, vent, src_elev = _elevate(barrel)
    vs.base_colour_only(bed_parts + barrel, max_size=1024)
    bed = vs.join(bed_parts, "artmrt.bed")
    lo = min(v.co.z for v in bed.data.vertices)
    tube = vs.join(barrel, "artmrt.barrel")
    burnt = vs.burnt_copy(bed, "artmrt.bed_burnt", 2_500)
    flash = _flash(muzzle, ax, radius)
    arm = vs.rig(root, "artmrt.rig", [
        ("gun", (0, 0, 0), None),
        ("bed", (0, 0, 0), "gun"),
        ("intact", (0, 0, 0), "bed"),
        ("wreck", (0, 0, 0), "bed"),
        ("barrel", tuple(t), "bed"),
        ("flash", tuple(muzzle), "barrel"),
    ])
    vs.attach(bed, arm, "intact")
    vs.attach(burnt, arm, "wreck")
    vs.attach(tube, arm, "barrel")
    vs.attach(flash, arm, "flash")
    side = Vector((1, 0, 0))
    rims = {"muzzle_l": muzzle - side * (radius + 0.02), "muzzle_r": muzzle + side * (radius + 0.02)}
    marks = _marks(arm, vent, rims)
    crew = guncrew.build(root, arm, "artmrt", kits=mortarcrew.KITS)
    hands = guncrew.key_clips(arm, _clips(arm), plan=mortarcrew.clips(), marks=marks)
    from acr_assets.humans import prepare_export

    prepare_export([o for ch in crew for o in ch.all_meshes()])
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    ctx.stats.update({"triangles": vs.triangles(meshes), "barrel": vs.triangles([tube]),
                      "bed": vs.triangles([bed]),
                      "crew": vs.triangles([o for ch in crew for o in ch.all_meshes()]),
                      "elevation_deg": {"source": round(src_elev, 1), "built": ELEVATION},
                      "muzzle_m": [round(x, 3) for x in muzzle], "muzzle_radius_m": round(radius, 3),
                      "vent_m": [round(x, 3) for x in vent], "bed_bottom_m": round(lo, 3),
                      "hands": hands})


HIDDEN = (vs.HIDDEN,) * 3
HIDE = {"wreck": {"scale": HIDDEN}, "intact": {}, "flash": {"scale": HIDDEN}}
REST = {b: {} for b in ("gun", "bed", "barrel")}
STEP = ("wreck", "intact", "flash")


def _still(arm, name, n):
    return vs.clip(arm, name, [(0, {**HIDE, **REST}), (n, {**REST})], step=STEP)


def _clips(arm):
    acts = {"STAND": vs.clip(arm, "STAND", [(0, {**HIDE, **REST})], step=STEP)}
    for name, n in (("GUN_PUSH", 36), ("CHEST_CARRY", 41), ("TRAIL_LIFT", 70), ("TRAIL_WALK", 40), ("DEPLOY", 170)):
        acts[name] = _still(arm, name, n)
    # The shot at 29: the flash out of the muzzle for FLASH_FRAMES, the bed jolted down and
    # back on the ground, the barrel kicking in its trunnions; it settles by 40.
    f = mortarcrew.FIRE
    keys = [(0, {**HIDE, **REST}),
            (f - 1, {"gun": {}, "barrel": {}, "flash": {"scale": HIDDEN}}),
            (f, {"flash": {"scale": (0.6, 0.6, 0.6)}}),
            (f + 1, {"gun": {"loc": (0, 0.06, -0.025)}, "barrel": {"rot": (3, 0, 0)}, "flash": {"scale": (1, 1, 1)}}),
            (f + 3, {"flash": {"scale": (1.25, 1.25, 1.25)}}),
            (f + FLASH_FRAMES, {"flash": {"scale": HIDDEN}}),
            (f + 6, {"gun": {"loc": (0, 0.08, 0)}, "barrel": {"rot": (-1, 0, 0)}}),
            (40, {"gun": {"loc": (0, 0.08, 0)}, "barrel": {}}),
            (419, {"gun": {"loc": (0, 0.08, 0)}, "barrel": {}})]
    acts["ATTACK"] = vs.clip(arm, "ATTACK", keys, step=STEP)
    # Destroyed: the bed burst and charred, slumped to one side, the barrel thrown off its
    # trunnions onto the ground.
    end = {"gun": {"rot": (0, 9, 0), "loc": (0, 0, -0.04)},
           "barrel": {"rot": (62, 10, 35), "loc": (0.55, -0.35, -0.42)}}
    keys = [(0, {**HIDE, **REST}),
            (4, {"wreck": {"scale": (1, 1, 1)}, "intact": {"scale": HIDDEN},
                 "gun": {"rot": (-3, 3, 0), "loc": (0, 0.1, 0.06)}, "barrel": {"rot": (15, 0, 5), "loc": (0, 0, 0.2)}}),
            (16, {"barrel": {"rot": (45, 6, 25), "loc": (0.35, -0.2, -0.1)}}),
            (28, end), (72, end)]
    acts["DEATH"] = vs.clip(arm, "DEATH", keys, step=STEP)
    acts["DEATHLIE1"] = vs.clip(arm, "DEATHLIE1", [(0, {**REST, "flash": {"scale": HIDDEN},
                                                         "wreck": {"scale": (1, 1, 1)}, "intact": {"scale": HIDDEN},
                                                         **end})], step=STEP)
    return acts
