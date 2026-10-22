"""ArtPus: the field cannon. The barrel is rivetech's "18th Century Cannon" (CC-BY 4.0, see
assets/LICENSES.md; its truck carriage is a naval one, so it is not used); the two-wheel
field carriage with its trail is scripted here.

Rig: `gun` (whole piece: recoil, tilt) > `carriage` > `intact`/`wreck` (cheeks, transoms,
charred copy), `barrel` (at the trunnions: kick, elevation); `wheel_l`/`wheel_r` (roll).
Clips on the sprite frames (24 fps, the `.MD` frame counts): `STAND`, `MOTION_L`/`MOTION_R`
(36 frames: one turn of the wheels), `ATTACK` (420 frames: lay, fire at 29, recoil, sponge
and load, run back into battery), `DEATH` (73: a wheel smashed, the barrel thrown off its
trunnions, the carriage charred) and `DEATHLIE1` (its end).
The crew of the original's sprites is not modelled.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from acr_assets import surfaces
from acr_assets import vessels as vs

TYPE = "ArtPus"

AXLE_Z = 0.72  # wheel radius = axle height
WHEEL_X = 0.82
TRUNNION = Vector((0.0, 0.05, 1.02))


def _box(name, verts):
    """A closed box from 8 corners (front face 0-3, back face 4-7, same winding)."""
    faces = [(0, 1, 2, 3), (7, 6, 5, 4), (0, 4, 5, 1), (1, 5, 6, 2), (2, 6, 7, 3), (3, 7, 4, 0)]
    me = bpy.data.meshes.new(name)
    me.from_pydata([Vector(v) for v in verts], [], faces)
    me.update()
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    return o


def _section(y, x0, x1, z0, z1):
    return [(x0, y, z0), (x1, y, z0), (x1, y, z1), (x0, y, z1)]


def _cheek(side):
    """A cheek: from beside the barrel's breech down to the trail end on the ground."""
    xi, xo = side * 0.13, side * 0.25
    xi2, xo2 = side * 0.06, side * 0.17
    front = _section(-0.42, xi, xo, 0.66, 1.00)
    back = _section(2.75, xi2, xo2, 0.0, 0.24)
    return _box(f"artpus.cheek_{'l' if side < 0 else 'r'}", front + back)


def _beam(name, a, b, w, h):
    a, b = Vector(a), Vector(b)
    d = (b - a).normalized()
    up = Vector((0, 0, 1)) if abs(d.z) < 0.9 else Vector((0, 1, 0))
    s = d.cross(up).normalized() * (w / 2)
    u = s.cross(d).normalized() * (h / 2)
    ring = lambda c: [c - s - u, c + s - u, c + s + u, c - s + u]
    return _box(name, ring(a) + ring(b))


def _wheel(name, x):
    """A spoked wheel in the YZ plane at (x, 0, AXLE_Z): felloe, iron tyre, hub, 12 spokes."""
    parts = []
    bpy.ops.mesh.primitive_torus_add(major_radius=AXLE_Z - 0.06, minor_radius=0.055, major_segments=28,
                                     minor_segments=5, location=(x, 0, AXLE_Z), rotation=(0, math.pi / 2, 0))
    felloe = bpy.context.active_object
    felloe.scale = (1.0, 1.0, 1.25)
    parts.append(felloe)
    bpy.ops.mesh.primitive_cylinder_add(vertices=12, radius=0.13, depth=0.32, location=(x, 0, AXLE_Z),
                                        rotation=(0, math.pi / 2, 0))
    parts.append(bpy.context.active_object)
    for k in range(12):
        a = 2 * math.pi * k / 12
        d = Vector((0, math.cos(a), math.sin(a)))
        c = Vector((x, 0, AXLE_Z))
        parts.append(_beam(f"{name}.spoke{k}", c + d * 0.1, c + d * (AXLE_Z - 0.1), 0.05, 0.035))
    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    return vs.join(parts, name)


def _barrel():
    objs = vs.import_file(vs.source("sketchfab", "18th-century-cannon", "source", "Cannon.fbx"))
    barrel = next(o for o in objs if o.name.startswith("Cannon_Barrel"))
    for o in objs:
        if o is not barrel:
            bpy.data.objects.remove(o, do_unlink=True)
    # Trunnions: the vertices furthest out sideways.
    vs_ = [v.co for v in barrel.data.vertices]
    wide = max(abs(v.x) for v in vs_)
    side = [v for v in vs_ if abs(v.x) > wide - 0.06]
    t = sum(side, Vector()) / len(side)
    t.x = 0.0
    vs.transform([barrel], Matrix.Translation(TRUNNION - t))
    tex = bpy.data.images.load(vs.source("sketchfab", "18th-century-cannon", "textures", "Cannon_Albedo.png"))
    mat = bpy.data.materials.new("artpus.bronze")
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = nt.nodes.get("Principled BSDF")
    node = nt.nodes.new("ShaderNodeTexImage")
    node.image = tex
    nt.links.new(node.outputs["Color"], bsdf.inputs["Base Color"])
    barrel.data.materials.clear()
    barrel.data.materials.append(mat)
    barrel.name = "artpus.barrel"
    return barrel


def build(ctx):
    root = ctx.root
    root["acr_machine"] = True
    barrel = _barrel()
    vs.base_colour_only([barrel], max_size=1024)
    wood = [_cheek(-1), _cheek(1),
            _beam("artpus.transom_front", (-0.2, -0.2, 0.86), (0.2, -0.2, 0.86), 0.16, 0.18),
            _beam("artpus.transom_mid", (-0.17, 0.9, 0.5), (0.17, 0.9, 0.5), 0.16, 0.16),
            _beam("artpus.transom_rear", (-0.12, 2.3, 0.18), (0.12, 2.3, 0.18), 0.14, 0.14),
            _beam("artpus.axletree", (-WHEEL_X + 0.1, 0, AXLE_Z), (WHEEL_X - 0.1, 0, AXLE_Z), 0.13, 0.13),
            _beam("artpus.quoin", (0, 0.55, 0.78), (0, 0.85, 0.74), 0.2, 0.08)]
    surfaces.apply(wood, [], default="timber")
    carriage = vs.join(wood, "artpus.carriage")
    wheels = [_wheel("artpus.wheel_l", -WHEEL_X), _wheel("artpus.wheel_r", WHEEL_X)]
    surfaces.apply(wheels, [], default="timber")
    iron = vs.flat_material("artpus.iron", (0.08, 0.08, 0.085))
    tyres = []
    for x in (-WHEEL_X, WHEEL_X):
        bpy.ops.mesh.primitive_cylinder_add(vertices=28, radius=AXLE_Z, depth=0.09, location=(x, 0, AXLE_Z),
                                            rotation=(0, math.pi / 2, 0), end_fill_type="NOTHING")
        t = bpy.context.active_object
        bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
        t.data.materials.append(iron)
        tyres.append(t)
    wheels = [vs.join([w, t], w.name) for w, t in zip(wheels, tyres)]
    burnt = vs.burnt_copy(carriage, "artpus.carriage_burnt", 10_000)
    arm = vs.rig(root, "artpus.rig", [
        ("gun", (0, 0, 0), None),
        ("carriage", (0, 0, AXLE_Z), "gun"),
        ("intact", (0, 0, AXLE_Z), "carriage"),
        ("wreck", (0, 0, AXLE_Z), "carriage"),
        ("barrel", tuple(TRUNNION), "carriage"),
        ("wheel_l", (-WHEEL_X, 0, AXLE_Z), "gun"),
        ("wheel_r", (WHEEL_X, 0, AXLE_Z), "gun"),
    ])
    vs.attach(carriage, arm, "intact")
    vs.attach(burnt, arm, "wreck")
    vs.attach(barrel, arm, "barrel")
    vs.attach(wheels[0], arm, "wheel_l")
    vs.attach(wheels[1], arm, "wheel_r")
    _clips(arm)
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    ctx.stats.update({"triangles": vs.triangles(meshes), "barrel": vs.triangles([barrel]),
                      "barrel_length_m": round(max(v.co.y for v in barrel.data.vertices)
                                               - min(v.co.y for v in barrel.data.vertices), 2)})


HIDE = {"wreck": {"scale": (vs.HIDDEN,) * 3}, "intact": {}}
STEP = ("wreck", "intact")


def _roll(metres):
    """Wheel turn (degrees about X) for rolling `metres` forward (towards -Y)."""
    return -math.degrees(metres / AXLE_Z)


def _clips(arm):
    vs.clip(arm, "STAND", [(0, {**HIDE, "gun": {}, "barrel": {}})], step=STEP)
    # One turn of the wheels per 36-frame cycle (seamless loop), the trail bobbing a little.
    keys = [(f, {"wheel_l": {"rot": (-360 * f / 36, 0, 0)}, "wheel_r": {"rot": (-360 * f / 36, 0, 0)},
                 "gun": {"rot": (0.6 * math.sin(f / 36 * 4 * math.pi), 0, 0)}}) for f in range(0, 37, 6)]
    keys[0][1].update(HIDE)
    for name in ("MOTION_L", "MOTION_R"):
        vs.clip(arm, name, keys, step=STEP)
    # Fire at frame 29: the piece jumps back 0.9 m, the muzzle kicks up; sponge and load;
    # the crew runs it back into battery.
    back = 0.9
    keys = [(0, {**HIDE, "gun": {}, "barrel": {}, "wheel_l": {}, "wheel_r": {}}),
            (28, {"gun": {}, "barrel": {}, "wheel_l": {}, "wheel_r": {}}),
            (30, {"gun": {"loc": (0, back * 0.6, 0.04), "rot": (-2.5, 0, 0)}, "barrel": {"rot": (-5, 0, 0)},
                  "wheel_l": {"rot": (-_roll(back * 0.6), 0, 0)}, "wheel_r": {"rot": (-_roll(back * 0.6), 0, 0)}}),
            (36, {"gun": {"loc": (0, back, 0)}, "barrel": {"rot": (1.0, 0, 0)},
                  "wheel_l": {"rot": (-_roll(back), 0, 0)}, "wheel_r": {"rot": (-_roll(back), 0, 0)}}),
            (44, {"gun": {"loc": (0, back, 0)}, "barrel": {}}),
            (300, {"gun": {"loc": (0, back, 0)}, "wheel_l": {"rot": (-_roll(back), 0, 0)},
                   "wheel_r": {"rot": (-_roll(back), 0, 0)}}),
            (400, {"gun": {}, "wheel_l": {}, "wheel_r": {}}),
            (419, {"gun": {}, "barrel": {}, "wheel_l": {}, "wheel_r": {}})]
    vs.clip(arm, "ATTACK", keys, step=STEP)
    # Destroyed: the left wheel smashed and down, the piece slumps onto that side, the barrel
    # thrown off its trunnions onto the ground, the carriage charred.
    end = {"gun": {"rot": (-4, -14, 0), "loc": (0, 0, -0.32)},
           "wheel_l": {"rot": (20, 78, 0), "loc": (-0.35, 0.3, -0.62)},
           "wheel_r": {"rot": (-30, 0, 0)},
           "barrel": {"rot": (9, 18, 25), "loc": (-0.35, -0.2, -0.62)}}
    keys = [(0, {"wreck": {"scale": (vs.HIDDEN,) * 3}, "intact": {}, "gun": {}, "wheel_l": {}, "wheel_r": {},
                 "barrel": {}}),
            (4, {"wreck": {"scale": (1, 1, 1)}, "intact": {"scale": (vs.HIDDEN,) * 3},
                 "gun": {"rot": (-3, 4, 0), "loc": (0, 0.15, 0.08)}, "barrel": {"rot": (-8, 0, 0)}}),
            (14, {"gun": {"rot": (-2, -8, 0), "loc": (0, 0.1, -0.15)},
                  "wheel_l": {"rot": (10, 40, 0), "loc": (-0.2, 0.15, -0.3)},
                  "barrel": {"rot": (4, 10, 12), "loc": (-0.15, -0.1, -0.25)}}),
            (28, end), (72, end)]
    vs.clip(arm, "DEATH", keys, step=STEP)
    vs.clip(arm, "DEATHLIE1", [(0, {"wreck": {"scale": (1, 1, 1)}, "intact": {"scale": (vs.HIDDEN,) * 3},
                                     **end})], step=STEP)
