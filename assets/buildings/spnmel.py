"""SpnMel (Spanish windmill, scripted): a whitewashed-stone tower mill of La Mancha with a
conical cap, its four lattice sails with cloth on their own node `sails` turning in the clip
`WORK` (one turn in `TURN_S` seconds, looping).

The sprite draws the sails as the separate `#WORK` layer (the `#STANDLO` picture has a hole
where they turn); the model keeps them as the node `sails` (origin on the axle, axle along
Y, the sails facing -Y like the picture) and the root's `acr_clock = "world"` makes the game
play `WORK` on the world clock while the mill stands finished (docs/rendering-3d.md, "Ships
and guns"). The tower (plinth, tapered drum, door, windows, cap, axle and tail pole) is
static. Sizes from the sprite: tower 3.8 m across, eaves 5.3 m, cap 7.5 m.

Stats: `triangles`, `size_m`, `sails_radius_m`, `hub_m`, `work_s`, `normal_maps`.
"""

import math
import random

import bpy

from acr_assets import scripted as sc

TYPE = "SpnMel"
CHECKS = ["--anim STANDLO"]

R0, R1, EAVE, APEX = 1.95, 1.75, 5.3, 7.5  # tower radius foot/top, eaves, cap apex (m)
HUB = (0.0, -2.35, 5.7)  # axle hub (m)
SAIL = 3.8  # sail radius (m)
SAIL_W = 1.1  # lattice width (m)
TURN_S = 5.0  # seconds per turn (~12 rpm)
SIDES = 16


def tower(stone, dark):
    def drum(u, v):
        a = 2 * math.pi * u
        r = R0 + (R1 - R0) * v
        return (r * math.cos(a), r * math.sin(a), EAVE * v)

    sc.grid(stone, drum, SIDES, 4, outward=lambda c: c.copy().to_2d().to_3d(), uv_scale=(2 * math.pi * R0 / 2.0, EAVE / 2.0))

    def plinth(u, v):
        a = 2 * math.pi * u
        r = R0 + 0.35 - 0.1 * v
        return (r * math.cos(a), r * math.sin(a), -0.2 + 0.6 * v)

    sc.grid(stone, plinth, SIDES, 1, outward=lambda c: c.copy().to_2d().to_3d(), uv_scale=(6.0, 0.3))
    sc.grid(stone, lambda u, v: ((R0 + 0.25) * (1 - v) * math.cos(2 * math.pi * u) + R0 * v * math.cos(2 * math.pi * u),
                                 (R0 + 0.25) * (1 - v) * math.sin(2 * math.pi * u) + R0 * v * math.sin(2 * math.pi * u), 0.4),
            SIDES, 1, outward=(0, 0, 1), uv_scale=(6.0, 0.2))
    # Door (south) and two small windows: dark recesses just proud of the drum.
    for (a, z0, z1, w) in ((-90.0, 0.4, 2.2, 0.5), (-60.0, 2.9, 3.4, 0.22), (150.0, 3.1, 3.6, 0.22)):
        ar = math.radians(a)
        cx, cy = math.cos(ar), math.sin(ar)
        tx, ty = -cy, cx

        def door(u, v, cx=cx, cy=cy, tx=tx, ty=ty, z0=z0, z1=z1, w=w):
            z = z0 + (z1 - z0) * v
            r = R0 + (R1 - R0) * z / EAVE + 0.02
            return (cx * r + tx * (u - 0.5) * 2 * w, cy * r + ty * (u - 0.5) * 2 * w, z)

        sc.grid(dark, door, 1, 1, outward=(cx, cy, 0), uv_scale=(0.5, 0.5))


def cap(roof):
    def cone(u, v):
        a = 2 * math.pi * u
        r = (R1 + 0.3) * (1 - v)
        return (r * math.cos(a), r * math.sin(a), EAVE - 0.1 + (APEX - EAVE + 0.1) * v)

    sc.grid(roof, cone, SIDES, 3, outward=lambda c: (c.x, c.y, 0.6), uv_scale=(6.0, 1.5))


def sails_mesh(wood, cloth):
    """Four sails in the XZ plane about the local origin (the hub), axle along Y."""
    sc.tube(wood, [(0, -0.35, 0), (0, 0.1, 0)], 0.32, sides=8)  # hub boss
    for k in range(4):
        a = math.radians(45 + 90 * k)
        d = (math.cos(a), math.sin(a))  # along the whip (x, z)
        n = (-d[1], d[0])  # across: the trailing side

        def p(r, w, y=0.0, d=d, n=n):
            return (d[0] * r + n[0] * w, y, d[1] * r + n[1] * w)

        sc.tube(wood, [p(0.2, 0, -0.25), p(SAIL + 0.1, 0, -0.25)], 0.07, sides=4, tile=1.5, seed=k)
        sc.tube(wood, [p(0.9, SAIL_W, -0.22), p(SAIL, SAIL_W, -0.22)], 0.035, sides=4, tile=1.5)
        sc.tube(wood, [p(0.9, -0.3, -0.22), p(SAIL, -0.3, -0.22)], 0.03, sides=4, tile=1.5)
        for i in range(10):
            r = 0.9 + (SAIL - 0.9) * i / 9
            sc.tube(wood, [p(r, -0.3, -0.22), p(r, SAIL_W, -0.22)], 0.025, sides=4)

        # The cloth on the lattice, bellied back by the wind.
        def c(u, v):
            r = 0.95 + (SAIL - 1.0) * v
            w = -0.25 + (SAIL_W + 0.2) * u
            return p(r, w, -0.17 + 0.12 * math.sin(math.pi * u) * math.sin(math.pi * v))

        sc.grid(cloth, c, 2, 6, outward=(0, -1, 0), uv_scale=(1.2, 3.0))


def build(ctx):
    random.seed(5)
    stone_rgb, stone_h = sc.from_surface("stone")
    stone_rgb = stone_rgb * 0.45 + 0.5  # whitewash over the masonry
    roof_rgb, roof_h = sc.from_surface("shingles")
    wood_rgb, wood_h = sc.wood(21)
    cloth_rgb, cloth_h = sc.canvas()
    dark_rgb, dark_h = sc.wood(22)
    m_stone = sc.material("spnmel_stone", stone_rgb, stone_h, roughness=0.9, strength=4.0)
    m_roof = sc.material("spnmel_roof", roof_rgb, roof_h, roughness=0.88, strength=5.0)
    m_wood = sc.material("spnmel_wood", wood_rgb, wood_h, roughness=0.85, strength=5.0)
    m_cloth = sc.material("spnmel_cloth", cloth_rgb, cloth_h, roughness=0.95, strength=2.0, double=True)
    m_dark = sc.material("spnmel_door", dark_rgb * 0.45, dark_h, roughness=0.9, strength=4.0)

    stone, roof, wood, dark = sc.new_bm(), sc.new_bm(), sc.new_bm(), sc.new_bm()
    tower(stone, dark)
    cap(roof)
    # Axle out through the cap to the hub; tail pole down the back to the ground.
    sc.tube(wood, [(0, 0.3, HUB[2]), (0, HUB[1] - 0.1, HUB[2])], 0.16, sides=8)
    sc.tube(wood, [(0, R1 + 0.2, EAVE - 0.1), (0, R0 + 3.0, 0.3)], 0.09, sides=5, tile=1.5)
    sc.to_object(stone, "spnmel.tower", m_stone, ctx.root)
    sc.to_object(roof, "spnmel.cap", m_roof, ctx.root)
    sc.to_object(wood, "spnmel.axle", m_wood, ctx.root)
    sc.to_object(dark, "spnmel.door", m_dark, ctx.root)

    # The sails: one node at the hub, its own two meshes as children.
    sails = bpy.data.objects.new("sails", None)
    sails.parent = ctx.root
    sails.location = HUB
    bpy.context.scene.collection.objects.link(sails)
    sw, scl = sc.new_bm(), sc.new_bm()
    sails_mesh(sw, scl)
    sc.to_object(sw, "sails.frame", m_wood, sails)
    sc.to_object(scl, "sails.cloth", m_cloth, sails)

    scene = bpy.context.scene
    fps = scene.render.fps
    frames = round(TURN_S * fps)
    scene.frame_start, scene.frame_end = 0, frames
    sails.rotation_mode = "XYZ"
    for f, ang in ((0, 0.0), (frames, -2 * math.pi)):
        sails.rotation_euler = (0.0, ang, 0.0)
        sails.keyframe_insert("rotation_euler", index=1, frame=f)
    act = sails.animation_data.action
    act.name = "WORK"
    for fc in sc_fcurves(act):
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    sails.rotation_euler = (0.0, 0.0, 0.0)
    scene.frame_set(0)
    ctx.root["acr_clock"] = "world"

    tris = sc.triangles(ctx.root)
    ctx.root["acr_triangles"] = tris
    ctx.stats = {"triangles": tris, "size_m": sc.size(ctx.root), "sails_radius_m": SAIL + 0.1,
                 "hub_m": list(HUB), "work_s": TURN_S, "normal_maps": 5}


def sc_fcurves(act):
    from acr_assets import armature

    return armature._fcurves(act)
