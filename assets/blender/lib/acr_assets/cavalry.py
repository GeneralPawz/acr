"""Mounted units on the WildMesh horse (#287, #275): the dragoon (EngD18), the cavalry
standard bearers (*KF18), the Spanish reiters (SpnReiG1/2) and the native riders.

A unit script gives a spec and calls `build(ctx, spec)`:

    md        {key: ".MD line"} (`rig.md_clip_frames` reads the frame counts)
    palette   material keys -> colour (the sprite reference refines them)
    dress     "coat" (18th-century coat, waistcoat, breeches, high boots, cap), "native"
              (bare chest, breechcloth, leggings)
    tack      "military" (scripted saddle with pistol holsters on a saddlecloth),
              "blanket" (a blanket, no saddle, a rope bridle), "bare" (a rope bridle)
    held      gear in the right hand: "flag" (a standard, the fist round the pole), "lance"
              (couched, thrust in ATTACK), None (reins)
    hat       "cap" (tall mitre cap), "tricorne", "morion" (crested helmet with a plume),
              "feathers" (long hair, a feather or two upright), "crown" (long hair, a
              headband of feathers), None
    shirt     natives: a shirt over the bare chest (`tunic`)
    armour    a steel cuirass over the coat (`metal`)
    shield    a round hide shield on the left forearm (`knapsack`)
    gun       "carbine" or "pistol" in the right fist: upright on the thigh while he rides,
              level for the shot (ATTACK1, PATTACK1...)
    sabre     False: no sabre at the hip
    drawn     True: a drawn sabre for the melee clips besides the gun (EngD18): it sits on a
              second right-hand socket (`socket.blade_r`); the clips key the sockets' scale,
              so the melee clips show the blade and hide the gun and the sheathed sabre
              (`SHOW_MELEE`), all others the reverse (`SHOW_REST`)
    coat      the horse's coat tint {"body": rgb, "hair": rgb} (`mount.COAT`)
    motions   {MD suffix: (source gait, strides per sprite cycle)} (`MOTIONS`)

Attacks follow the .MD: index 0 (`ATTACK`, `PATTACK`, `UATTACK`, `PSTAND`) is the cold
weapon (sabre cut, lance thrust), index 1 (`ATTACK1`, `PATTACK1`...) the shot.

The horse is `acr_assets.mount`; the rider stands on skeleton H, his root on the horse's
`saddle` bone. Seated pose: legs to the stirrups, both arms solved (two-bone IK) to closed
hands on the reins over the withers; the right arm leaves the reins for aiming, cutting or
the flag. Death: the horse falls onto its side with bent legs; the rider is thrown clear,
a metre beyond its back, lying along it.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

from . import armature as am
from . import body as bd
from . import crawl, faces, gear, heads, horse, mount, poses, rig
from .humans import Character, prepare_export

P = "rider."
TORSO = {"hips", "spine", "spine1", "spine2", "clavicle_l", "clavicle_r"}
ARMS = {"upperarm_l", "upperarm_r", "forearm_l", "forearm_r"}
LEGS = {"thigh_l", "thigh_r"}
SHINS = {"shin_l", "shin_r"}
FEET = {"foot_l", "foot_r", "toe_l", "toe_r"}
BUILD = {"width": 1.0, "shoulders": 1.08, "limbs": 1.12}
# The dragoon's colours; other units override some (the sprites refine all).
PALETTE = {
    "skin": (0.80, 0.58, 0.45), "bone": (0.85, 0.81, 0.69), "coat": (0.70, 0.08, 0.05),
    "waistcoat": (0.85, 0.80, 0.62), "breeches": (0.85, 0.80, 0.62), "boots_high": (0.08, 0.07, 0.06),
    "boots": (0.30, 0.20, 0.12), "belts": (0.85, 0.80, 0.62), "cap": (0.70, 0.08, 0.05), "metal": (0.70, 0.70, 0.72),
    "wood": (0.42, 0.24, 0.12), "saddle": (0.25, 0.15, 0.08), "tack": (0.08, 0.07, 0.06), "saddlecloth": (0.72, 0.10, 0.06), "sash": (0.75, 0.10, 0.10), "hat": (0.10, 0.09, 0.08),
    "tunic": (0.80, 0.75, 0.62), "hair": (0.05, 0.04, 0.04), "knapsack": (0.80, 0.70, 0.55),
}

# The horse's clips: {key: (source action, mode, frames[, a, b])}; frames come from the MD.
GAITS = {
    "STAND": ("Idle", "pose", None, 0.0),
    "REST": ("Idle", "loop", None),
    "MOTION_L": ("G1_Trot", "loop", None, 0.0, 0.5),
    "MOTION_R": ("G1_Trot", "loop", None, 0.5, 1.0),
    "WALK": ("G0_Walk", "loop", 16),
    "GALLOP": ("G3_Gallop", "loop", 12),
    "ATTACK": ("Idle_Fidget_08", "once", None, 0.0, 0.7),
    "ATTACK1": ("Idle_Fidget_07", "once", None),
    "DEATH": ("Idle", "fall", None, 0.0, -90),
}

LEGS_SEAT = {"thigh_l": (-70, -12, 0), "shin_l": (96, 0, 0), "foot_l": (-22, 0, 0),
             "thigh_r": (-70, 12, 0), "shin_r": (96, 0, 0), "foot_r": (-22, 0, 0),
             "spine": (-2, 0, 0), "head": (4, 0, 0)}
# Right arm poses (armature axes) of the original dragoon (#183).
AIM_R = {"upperarm_r": (-70, -30, -20), "forearm_r": (-40, 0, 30), "head": (8, 0, -10)}
AIM_L = {"upperarm_l": (-75, 45, 25), "forearm_l": (-15, 0, -15)}
CUT_UP_R = {"upperarm_r": (-150, -20, 0), "forearm_r": (-20, 0, 0), "spine": (4, 0, -10)}
CUT_DOWN_R = {"upperarm_r": (-60, -10, 0), "forearm_r": (-30, 0, 0), "spine": (-14, 0, 15)}
FLAG_R = {"upperarm_r": (-20, -10, 0), "forearm_r": (-75, 0, 10)}
# Fists (`close_hands`): the mitten's fingers, from `KNUCKLE` metres past the wrist, wrap
# round a cylinder of radius `GRIP` before the palm, where pole, lance and grips go.
KNUCKLE, GRIP = 0.08, 0.03
# The flag hand: fist axis (rest Y) upright, knuckles forward, palm towards the rider.
FLAG_HAND = (-90, 0, 0)
LANCE = (2.9, 0.65)  # length, and how far it reaches behind the grip (metres)
BLADE = (0.86, 0.10)  # a drawn sabre: blade length, hilt behind the grip (metres)
# Socket scales (1 shown, 0 hidden) of a rider with a drawn sabre (`drawn`).
SHOW_REST = {"socket.hand_r": 1.0, "socket.hip_l": 1.0, "socket.blade_r": 0.0}
SHOW_MELEE = {"socket.hand_r": 0.0, "socket.hip_l": 0.0, "socket.blade_r": 1.0}


def rig_lie():
    """Rider lying prone (armature space); `world_loc` set by `lie_spot`."""
    return {
        "world": {"root": (90, 0, 0)},
        "upperarm_l": (12, -45, 0), "forearm_l": (0, -16, 0), "upperarm_r": (-2, 47, 0), "forearm_r": (0, 50, 0),
        "thigh_l": (-14, -6, 0), "thigh_r": (-10, 8, 0), "shin_l": (14, 0, 0), "shin_r": (15, 0, 0),
        "head": (-10, 18, 62), "foot_l": (55, 0, 0), "foot_r": (50, 0, 0),
    }


# ----------------------------------------------------------------------------- arms

def _ik(s, t, l1, l2, pole):
    """Elbow of a two-bone chain from `s` to `t` bent towards `pole`."""
    v = t - s
    d = min(v.length, (l1 + l2) * 0.995)
    u = v.normalized()
    a = (l1 * l1 - l2 * l2 + d * d) / (2 * d)
    h = math.sqrt(max(l1 * l1 - a * a, 0.0))
    p = (pole - u * pole.dot(u)).normalized()
    return s + u * a + p * h


def _turn(d0, d):
    e = d0.rotation_difference(d).to_euler("XYZ")
    return tuple(math.degrees(x) for x in e)


def reins_world(arm, targets, sides=("l", "r")):
    """{"world": {...}} orientations of upper arm, forearm and hand per side so the wrist
    reaches `targets[side]` (armature space, rest) and the hand, closed, points forward
    and down along the reins."""
    out = {}
    B = arm.data.bones
    for sd in sides:
        sgn = 1.0 if sd == "l" else -1.0
        up, fo, ha = B[P + "upperarm_" + sd], B[P + "forearm_" + sd], B[P + "hand_" + sd]
        s = up.head_local
        l1, l2 = (up.tail_local - up.head_local).length, (ha.head_local - fo.head_local).length
        t = targets[sd]
        e = _ik(s, t, l1, l2, Vector((0.6 * sgn, 0.7, -0.4)))
        out["upperarm_" + sd] = _turn((up.tail_local - up.head_local).normalized(), (e - s).normalized())
        df = (t - e).normalized()
        out["forearm_" + sd] = _turn((fo.tail_local - fo.head_local).normalized(), df)
        dh = (df + Vector((0.0, 0.0, -0.8))).normalized()
        out["hand_" + sd] = _turn((ha.tail_local - ha.head_local).normalized(), dh)
    return out


def _with_world(spec, world):
    s = dict(spec)
    w = dict(world)
    w.update(spec.get("world", {}))
    s["world"] = w
    for k in world:
        s.pop(k, None)
    return s


# ----------------------------------------------------------------------------- tack

def _body_section(hz, y, z):
    """Half width of the horse's body (M_Body) at length `y` around height `z`."""
    body = hz.meshes[0]
    mats = body.data.materials
    w = 0.0
    for poly in body.data.polygons:
        m = mats[poly.material_index]
        if not m or "body" not in m.name.lower():
            continue
        c = poly.center
        if abs(c.y - y) < 0.06 and abs(c.z - z) < 0.06:
            w = max(w, abs(c.x))
    return w


def back_top(hz, y):
    body = hz.meshes[0]
    mats = body.data.materials
    top = -1.0
    for poly in body.data.polygons:
        m = mats[poly.material_index]
        if m and "body" in m.name.lower() and abs(poly.center.y - y) < 0.05 and abs(poly.center.x) < 0.06:
            top = max(top, poly.center.z)
    return top


def _box(bm, centre, size, rot=None):
    m = Matrix.Translation(centre) @ (rot or Matrix.Identity(4)) @ Matrix.Diagonal((*size, 1.0))
    bmesh.ops.create_cube(bm, size=1.0, matrix=m)


def _tube(bm, a, b, r1, r2, segs=10):
    d = b - a
    rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
    m = Matrix.Translation((a + b) / 2) @ rot
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=r1, radius2=r2, depth=d.length, matrix=m)


def _mat(name, rgb, rough=0.8):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    return mat


def military_saddle(hz, name, leather=(0.20, 0.12, 0.06), caps=(0.05, 0.04, 0.03)):
    """An 18th-century military saddle: padded seat between a high pommel and cantle,
    flaps down the barrel, and two pistol holsters with bearskin caps before the knees.
    Rigid on the `saddle` bone."""
    sad = hz.head("saddle")
    y0 = sad.y
    top = back_top(hz, y0)
    w = max(_body_section(hz, y0, top - 0.08), 0.12)
    wl = max(_body_section(hz, y0 - 0.05, top - 0.30), w)
    bm = bmesh.new()
    _box(bm, Vector((0, y0 + 0.02, top + 0.035)), (0.30, 0.50, 0.07))  # seat
    _box(bm, Vector((0, y0 - 0.22, top + 0.10)), (0.24, 0.08, 0.16), Matrix.Rotation(math.radians(-15), 4, "X"))  # pommel
    _box(bm, Vector((0, y0 + 0.25, top + 0.09)), (0.30, 0.07, 0.13), Matrix.Rotation(math.radians(20), 4, "X"))  # cantle
    for sx in (-1, 1):
        # Flap: from the seat's edge down the barrel.
        a = Vector((sx * (w + 0.015), y0 - 0.02, top - 0.02))
        b = Vector((sx * (wl + 0.03), y0 - 0.04, top - 0.34))
        ang = math.atan2((b - a).x, -(b - a).z)
        _box(bm, (a + b) / 2, (0.012, 0.36, (b - a).length), Matrix.Rotation(ang, 4, "Y"))
    for sx in (-1, 1):
        a = Vector((sx * (w + 0.06), y0 - 0.28, top + 0.04))
        b = a + Vector((sx * 0.06, -0.10, -0.30))
        _tube(bm, a, b, 0.045, 0.03)
    holster_n = len(bm.faces)
    for sx in (-1, 1):
        a = Vector((sx * (w + 0.06), y0 - 0.28, top + 0.04))
        _tube(bm, a + Vector((sx * 0.005, 0.0, -0.03)), a + Vector((-sx * 0.01, 0.015, 0.06)), 0.058, 0.05)
    for i, f in enumerate(bm.faces):
        f.material_index = 1 if i >= holster_n else 0
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    me.materials.append(_mat(name + ".leather", leather))
    me.materials.append(_mat(name + ".bearskin", caps, 0.95))
    gear.rigid(obj, hz.arm, "saddle", Matrix.Identity(4), part=horse.HORSE_PART)
    return obj


# ----------------------------------------------------------------------------- held gear

def flag(ch, name):
    """A cavalry standard in the right fist (the pole through it, upright): a pole
    (`wood`) and a square cloth (`sash`, the standard's colour), two held objects. Call
    with the flag pose set. Returns (pole, cloth)."""
    c, _rot = fist_centre(ch)
    out = []
    for part, key in (("pole", "wood"), ("cloth", "sash")):
        bm = bmesh.new()
        if part == "pole":
            _tube(bm, Vector((0, 0, -0.55)), Vector((0, 0, 1.75)), 0.016, 0.014, 8)
            _tube(bm, Vector((0, 0, 1.75)), Vector((0, 0, 1.86)), 0.03, 0.002, 8)
        else:
            _box(bm, Vector((0.0, 0.27, 1.42)), (0.01, 0.52, 0.48))
        me = bpy.data.meshes.new(f"{name}_{part}")
        bm.to_mesh(me)
        bm.free()
        obj = bpy.data.objects.new(f"{name}_{part}", me)
        bpy.context.scene.collection.objects.link(obj)
        am.attach(obj, ch.arm, P + "socket.hand_r", Matrix.Translation(c))
        ch.held["flag_" + part] = obj
        ch.keys[obj.name] = key
        out.append(obj)
    return out


def _palm(d, side):
    """The palm's normal (rest): towards the thigh, square to the hand's axis `d`."""
    n = Vector((1.0 if side == "r" else -1.0, 0.0, 0.0))
    return (n - d * n.dot(d)).normalized()


def close_hands(ch):
    """Curls both hands into fists. The MakeHuman mittens carry no finger weights, so the
    fingers are bent in the mesh: every hand vertex past the knuckles (`KNUCKLE` along the
    hand) is rolled round the grip's axis (`GRIP` before the palm, along the knuckles), as
    far round as it lies past them. A rider's hands always hold something (reins, a
    standard, a lance, a gun)."""
    me = ch.flesh.data
    ch.fist_vertices = 0
    for sd in ("l", "r"):
        g = ch.flesh.vertex_groups.get(P + "hand_" + sd)
        if g is None:
            continue
        b = ch.arm.data.bones[P + "hand_" + sd]
        d = (b.tail_local - b.head_local).normalized()
        n = _palm(d, sd)
        axis = d.cross(n)
        k = b.head_local + d * KNUCKLE
        c = k + n * GRIP
        for v in me.vertices:
            if next((e.weight for e in v.groups if e.group == g.index), 0.0) < 0.5:
                continue
            s = (v.co - k).dot(d)
            if s > 0.0:
                v.co = c + Matrix.Rotation(s / GRIP, 3, axis) @ (v.co - d * s - c)
                ch.fist_vertices += 1
    me.update()


def fist_centre(ch, side="r"):
    """The middle of the fist's grip (current pose). Returns (point, the hand's rotation
    from rest)."""
    pb = ch.arm.pose.bones[P + "hand_" + side]
    m = ch.arm.matrix_world @ pb.matrix
    rot = m.to_3x3() @ pb.bone.matrix_local.to_3x3().inverted()
    d0 = (pb.bone.tail_local - pb.bone.head_local).normalized()
    d = rot @ d0
    return m.translation + d * KNUCKLE + (rot @ _palm(d0, side)) * GRIP, rot


def _held(ch, name, key, bm, socket, matrix):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    am.attach(obj, ch.arm, P + socket, matrix)
    ch.held[name.split(".", 1)[-1]] = obj
    ch.keys[obj.name] = key
    return obj


def lance(ch, name):
    """A lance in the closed right hand along the fist's axis (rest -Y: forward): the
    shaft (`wood`) and an iron head (`metal`)."""
    c, rot = fist_centre(ch)
    m = Matrix.Translation(c) @ rot.to_4x4()
    ln, back = LANCE
    bm = bmesh.new()
    _tube(bm, Vector((0, back, 0)), Vector((0, back - ln, 0)), 0.018, 0.014, 8)
    _held(ch, f"{name}_shaft", "wood", bm, "hand_r", m)
    bm = bmesh.new()
    _tube(bm, Vector((0, back - ln, 0)), Vector((0, back - ln - 0.30, 0)), 0.03, 0.002, 6)
    head = _held(ch, f"{name}_head", "metal", bm, "hand_r", m)
    tip(head, f"{name}.socket.tip", (0, back - ln - 0.30, 0))
    return head


def tip(obj, name, at):
    """An empty `*socket.tip` at the point of a held weapon `obj` (its local `at`): the strike
    layer reads the weapon's axis and reach from the hand to it (`strike_view::weapon`)."""
    e = bpy.data.objects.new(name, None)
    e.empty_display_size = 0.05
    bpy.context.scene.collection.objects.link(e)
    e.parent = obj
    e.location = at
    return e


def blade(ch, name):
    """A drawn sabre in the closed right hand along the fist's axis (rest -Y), on the second
    right-hand socket `socket.blade_r` (shown only in the melee clips): a flat blade narrowing
    to the point, a grip and a guard (`metal`), the point marked by `socket.tip`."""
    c, rot = fist_centre(ch)
    m = Matrix.Translation(c) @ rot.to_4x4()
    ln, back = BLADE
    bm = bmesh.new()
    _box(bm, Vector((0, back / 2 - 0.02, 0)), (0.025, back + 0.04, 0.03))  # grip
    _box(bm, Vector((0, -0.03, 0)), (0.02, 0.012, 0.10))  # guard
    r = bmesh.ops.create_cube(bm, size=1.0)
    for v in r["verts"]:
        t = 0.5 - v.co.y  # 0 at the guard, 1 at the point
        v.co = Vector((v.co.x * 0.006, -0.04 - t * ln, (v.co.z * 0.034 + 0.004) * (1.0 - 0.7 * t) + 0.03 * t * t))
    obj = _held(ch, name, "metal", bm, "socket.blade_r", m)
    tip(obj, f"{name}.socket.tip", (0, -0.04 - ln, 0.03))
    return obj


def shield(ch, name, radius=0.27):
    """A round hide shield on the outside of the left forearm (`knapsack`), facing out."""
    pb = ch.arm.pose.bones[P + "forearm_l"]
    a = ch.arm.matrix_world @ pb.head
    b = ch.arm.matrix_world @ pb.tail
    centre = (a + b) / 2 + Vector((0.07, 0, 0))
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=20, radius1=radius, radius2=radius * 0.9, depth=0.03,
                          matrix=Matrix.Translation(centre) @ Matrix.Rotation(math.radians(90), 4, "Y"))
    return _held(ch, name, "knapsack", bm, "forearm_l", Matrix.Identity(4))


def gun(ch, name, kind):
    """A carbine or a pistol in the right hand, set for the shot: call with the aim pose
    (the socket level), the barrel forward."""
    c, _rot = fist_centre(ch)
    ln = 1.05 if kind == "carbine" else 0.42
    bm = bmesh.new()
    _box(bm, Vector((0, -ln / 2 + 0.12, 0.0)), (0.03, ln, 0.035))  # barrel and stock
    _box(bm, Vector((0, 0.02, -0.05)), (0.03, 0.08, 0.10), Matrix.Rotation(math.radians(-20), 4, "X"))  # grip
    for v in bm.verts:
        v.co += c
    return _held(ch, name, "metal" if kind == "pistol" else "wood", bm, "socket.hand_r", Matrix.Identity(4))


def morion(ch, name, plume=True):
    """A Spanish morion: a combed steel dome with a brim turned up front and back, and a
    plume at the back (`sash`)."""
    sock = am.world(ch.arm, P + "socket.hat").translation + Vector((0, 0, -0.02))
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=14, v_segments=8, radius=0.12)
    for v in bm.verts:
        v.co.z = max(v.co.z, -0.01) * 1.35
    rim = bmesh.ops.create_cone(bm, cap_ends=False, segments=16, radius1=0.155, radius2=0.115, depth=0.02,
                                matrix=Matrix.Translation((0, 0, -0.01)))
    for v in rim["verts"]:
        if v.co.xy.length > 0.13:
            v.co.x *= 0.85
            v.co.y *= 1.25
            v.co.z += 0.13 * (abs(v.co.y) / 0.19) ** 3
    _box(bm, Vector((0, 0, 0.16)), (0.012, 0.20, 0.07))  # comb
    for v in bm.verts:
        v.co += sock
    obj = _held(ch, f"{name}_morion", "metal", bm, "socket.hat", Matrix.Identity(4))
    if plume:
        bm = bmesh.new()
        _box(bm, sock + Vector((0, 0.10, 0.16)), (0.05, 0.06, 0.20), Matrix.Rotation(math.radians(-25), 4, "X"))
        _held(ch, f"{name}_plume", "sash", bm, "socket.hat", Matrix.Identity(4))
    return obj


def native_head(ch, name, kind, cy):
    """Long black hair down the back (`hair`) and feathers (`cap`): "feathers" (two
    upright at the back) or "crown" (a headband of feathers all round)."""
    head = am.world(ch.arm, P + "head").translation
    bm = bmesh.new()
    _box(bm, Vector((0, cy + 0.075, head.z - 0.02)), (0.17, 0.06, 0.36))
    for v in bm.verts:
        if v.co.z < head.z - 0.1:
            v.co.x *= 0.7
    _held(ch, f"{name}_hair", "hair", bm, "head", Matrix.Identity(4))
    sock = am.world(ch.arm, P + "socket.hat").translation
    bm = bmesh.new()
    if kind == "crown":
        n = 11
        for i in range(n):
            a = 2 * math.pi * i / n
            r = Vector((math.sin(a) * 0.105, -math.cos(a) * 0.115, 0))
            tilt = Matrix.Rotation(a, 4, "Z") @ Matrix.Rotation(math.radians(25), 4, "X")
            _box(bm, sock + r + Vector((0, 0, 0.04)), (0.045, 0.008, 0.20), tilt)
    else:
        for dx in (-0.03, 0.03):
            _box(bm, sock + Vector((dx, 0.09, 0.08)), (0.035, 0.006, 0.24), Matrix.Rotation(math.radians(20), 4, "X"))
    return _held(ch, f"{name}_feathers", "cap", bm, "socket.hat", Matrix.Identity(4))


# Where the dead horse's body (spine) ends, unit space (metres; the unit faces -Y): the
# sprites' corpse (DEATHLIE1) lies about 2.3 m behind the unit's position and 0.6 m to
# its right, rider included (`acr model-check --anim DEATHLIE1`: the sprite's centroid
# circles the anchor at about 55 px as the views turn; ours sat on it).
FALL_TO = (-0.6, 2.3)


def centre_fall(hz, bone="spine_02", to=FALL_TO):
    """Slides the fall so that the dead horse's body ends at `to`, where the sprites'
    corpse lies; eased over the fall, as the stumble of a falling horse."""
    W = hz.worlds.get("DEATH")
    if not W:
        return
    end = W[-1][bone].translation
    off = Vector((end.x - to[0], end.y - to[1], 0.0))
    n = len(W)
    for i, fr in enumerate(W):
        t = i / max(n - 1, 1)
        T = Matrix.Translation(-off * (t * t * (3 - 2 * t)))
        for b in fr:
            fr[b] = T @ fr[b]


def rope_bridle(hz):
    """Bareback tack: the pack's western saddle (`saddle1`) and its girth, stirrup
    leathers and stirrups (`saddle2` behind the reins' grip, below the back) go; its
    bridle and reins stay."""
    sad = hz.head("saddle")
    grip = hz.head("reins_01_l").y if "reins_01_l" in hz.arm.data.bones else sad.y - 0.35
    for m in hz.meshes:
        mats = m.data.materials
        bad = {i for i, mat in enumerate(mats) if mat and any(k in mat.name.lower() for k in ("saddle1", "saddle2"))}
        if not bad:
            continue
        bm = bmesh.new()
        bm.from_mesh(m.data)
        gone = []
        for f in bm.faces:
            if f.material_index not in bad:
                continue
            c = f.calc_center_median()
            if "saddle1" in mats[f.material_index].name.lower() or (c.y > grip + 0.04 and c.z < sad.z - 0.04):
                gone.append(f)
        bmesh.ops.delete(bm, geom=gone, context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(m.data)
        bm.free()
    hz.stats["triangles"] = sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in hz.meshes)


# ----------------------------------------------------------------------------- build

def frames(md, key):
    return rig.md_clip_frames(md[key])[0]


# The source gait and its strides per MOTION_L + MOTION_R (one sprite cycle), by .MD
# suffix, so that the stride matches the game's speed (no sliding hooves). Source strides
# at 1.5 m withers: walk 1.60 m, trot 2.14 m, canter 2.95 m, gallop 3.88 m (30 fps: 35, 20,
# 17, 15 frames). The dragoons move 13.6 px (0.45 m) a tick, 6.4 m per 14-frame cycle: two
# canter strides (5.9 m) play at 0.8 of the source's pace, where three trot strides played
# 1.4 times too fast; and 4.4 m/s is a canter's speed. The natives' 25 px a tick over 10
# frames (8.3 m) is two gallop strides of 3.9 m, at the source's pace.
MOTIONS = {"": ("G2_Canter", 2), "1": ("G3_Gallop", 2)}
NATIVE_MOTIONS = {"": ("G3_Gallop", 2)}


def horse_clips(md, motions=MOTIONS):
    out = {}
    for key, clip in GAITS.items():
        if key in ("ATTACK", "ATTACK1") and key not in md:
            continue
        n = clip[2] or (frames(md, key) if key in md else 1)
        out[key] = (clip[0], clip[1], n) + tuple(clip[3:])
    # MOTION_L covers its share of the cycles (by frames), MOTION_R the rest.
    for sfx, (gait, k) in motions.items():
        if "MOTION_L" + sfx not in md:
            continue
        nl, nr = frames(md, "MOTION_L" + sfx), frames(md, "MOTION_R" + sfx)
        mid = k * nl / (nl + nr)
        out["MOTION_L" + sfx] = (gait, "cycles", nl, 0.0, mid)
        out["MOTION_R" + sfx] = (gait, "cycles", nr, mid, float(k))
    return out


def lie_spot(hz):
    """The thrown rider's root (at his feet) a metre beyond the fallen horse's back (from
    its hooves through its spine), placed so that he lies along the back, head towards
    the horse's head; and his heading."""
    last = hz.worlds["DEATH"][-1]
    hooves = [b.name for b in hz.arm.data.bones if b.head_local.z < 0.12 and b.parent is not None and "root" not in b.name]
    s = last["spine_02"].translation
    f = sum((last[n].translation for n in hooves), Vector()) / max(len(hooves), 1)
    d = Vector((s.x - f.x, s.y - f.y, 0.0)).normalized()
    head = last["head"].translation
    along = Vector((head.x - s.x, head.y - s.y, 0.0)).normalized()
    return (s.x + d.x * 1.0 - along.x * 0.8, s.y + d.y * 1.0 - along.y * 0.8, 0.13), math.degrees(math.atan2(along.x, -along.y))


def make(ctx, spec):
    name, root = ctx.name, ctx.root
    md = spec["md"]
    hz = mount.load(name, root, horse_clips(md, spec.get("motions", MOTIONS)), coat=spec.get("coat"))
    centre_fall(hz)
    tack = spec.get("tack", "military")
    if tack == "military":
        mount.drop_faces(hz, ["saddle1"])
    else:
        rope_bridle(hz)
    lie, heading = lie_spot(hz)
    sad = hz.head("saddle")
    origin = Vector((sad.x, sad.y + 0.02, sad.z - 0.90 + spec.get("seat_dz", 0.0)))
    oz, cy = origin.z, origin.y
    table = rig.skeleton_h(P, tuple(origin))
    if spec.get("drawn"):
        # The second right-hand socket for the drawn sabre, beside the gun's.
        hs = next(b for b in table if b.name == P + "socket.hand_r")
        table.append(rig.Bone(P + "socket.blade_r", hs.parent, hs.head, hs.tail, False))
    ch = Character(root, name, dict(BUILD, origin=tuple(origin)), prefix=P, table=table)
    mount.join(ch.arm, hz, P + "root")
    ch.body()
    close_hands(ch)
    if spec.get("dress", "coat") == "coat":
        ch.shell("waistcoat", "waistcoat", lambda c, n, h, p: h in TORSO and 0.97 + oz < c.z < 1.47 + oz, 0.012)
        ch.shell("coat", "coat", lambda c, n, h, p: (h in TORSO and 0.97 + oz < c.z < 1.49 + oz and not (c.y - cy < -0.14 and abs(c.x) < 0.07))
                 or h in ARMS, 0.03)
        ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0 + oz), 0.010)
        ch.shell("boots", "boots_high", lambda c, n, h, p: h in SHINS | FEET or (h in LEGS and c.z < 0.58 + oz), 0.02)
        skirt = bd.tube(ch.arm, f"{name}.coat_skirt", rig.SKIRT_TOP + oz, 0.72 + oz, (0.20, 0.15), (0.24, 0.20), gap_deg=80,
                        flare_back=0.03, columns=rig.SKIRT_COLUMNS, centre=(origin.x, origin.y), prefix=P)
        ch.add("clothes", skirt, "coat", symmetric=True)
        belt = bd.strap(ch.arm, f"{name}.belt", ch.flesh, Vector((0.12, cy, 1.47 + oz)), Vector((-0.17, cy, 0.95 + oz)),
                        0.06, 0.04, bones=[P + b for b in rig.BODY_BONES])
        ch.add("clothes", belt, "belts", symmetric=True)
    else:
        ch.shell("breechcloth", "breeches", lambda c, n, h, p: h in {"hips"} | LEGS and 0.80 + oz < c.z < 1.0 + oz, 0.012)
        ch.shell("leggings", "boots", lambda c, n, h, p: h in SHINS | FEET or (h in LEGS and c.z < 0.80 + oz), 0.012)
        if spec.get("shirt"):
            ch.shell("shirt", "tunic", lambda c, n, h, p: (h in TORSO and 0.97 + oz < c.z < 1.47 + oz) or h in ARMS, 0.014)
    if spec.get("armour"):
        ch.shell("cuirass", "metal", lambda c, n, h, p: h in TORSO and 1.0 + oz < c.z < 1.46 + oz, 0.05)
    ch.bone_layer()
    ch.caps()
    extra = []
    if tack != "bare":
        extra.append(mount.saddlecloth(hz, f"{name}.saddlecloth", spec["palette"]["saddlecloth"],
                                       half_len=0.36 if tack == "military" else 0.42))
    if tack == "military":
        sad_obj = military_saddle(hz, f"{name}.saddle")
        # Leather like the rider's gear: decay material and colour from the sprites.
        sad_obj["acr_material_keys"] = ["saddle", "tack"]
        ch.add("gear", sad_obj, "saddle", symmetric=True)
    ch.horse = {"mount": hz, "extra": extra}

    # Seated pose: legs, then arms solved onto the reins.
    reins = hz.head("reins_01_l") if "reins_01_l" in hz.arm.data.bones else sad + Vector((0.1, -0.27, 0.0))
    targets = {s: Vector((0.10 * k, reins.y + 0.06, sad.z + 0.17)) for s, k in (("l", 1), ("r", -1))}
    am.pose(ch.arm, LEGS_SEAT, prefix=P)
    both = reins_world(ch.arm, targets)
    left = {k: v for k, v in both.items() if k.endswith("_l")}
    seat = _with_world(LEGS_SEAT, both)
    def right(target, hand=None, socket=None):
        """The left hand on the reins, the right one solved to `target` (and turned)."""
        w = dict(left, **{k: v for k, v in reins_world(ch.arm, {"r": target}, sides=("r",)).items()})
        if hand is not None:
            w["hand_r"] = hand
        if socket is not None:
            w["socket.hand_r"] = socket
        return w

    def side(dx, dy, dz):
        """A point by the right hip (seated), moved by (dx, dy, dz)."""
        return Vector((-0.24 + dx, cy + dy, sad.z + dz))

    poses = {"seat": seat, "lean": _with_world(dict(LEGS_SEAT, spine=(-12, 0, 0)), both),
             "aim": _with_world(dict(LEGS_SEAT, **AIM_R, **AIM_L), {"socket.hand_r": (88, 0, 0)}),
             "cut_up": _with_world(dict(LEGS_SEAT, **CUT_UP_R), dict(left, **{"socket.hand_r": (-40, 0, 0)})),
             "cut_down": _with_world(dict(LEGS_SEAT, **CUT_DOWN_R), dict(left, **{"socket.hand_r": (110, 0, 0)})),
             "flag": _with_world(dict(LEGS_SEAT, **FLAG_R), dict(left, hand_r=FLAG_HAND, **{"socket.hand_r": (0, 0, 0)})),
             # Lance couched at the right hip, the point forward and up; the thrust.
             "lance": _with_world(dict(LEGS_SEAT), right(side(0, -0.12, 0.15), (-20, 0, 0))),
             "thrust": _with_world(dict(LEGS_SEAT, spine=(-10, 0, 8)), right(side(0.04, -0.62, 0.50), (-5, 0, 0))),
             # Strike clips (docs/melee.md): the lance drawn back high, then driven down past
             # the horse's right shoulder at a man on the ground, or level at a standing one.
             "lance_back": _with_world(dict(LEGS_SEAT, spine=(4, 0, -6)), right(side(0.02, 0.10, 0.42), (-30, 0, 0))),
             # Down (m2-melee5, measured offline at the hit frame): the hand raised to about
             # 1.95 m, a little behind the seat, the lance 44° down and 8° out to the right,
             # so its point (2.5 m from the hand) meets the ground about 1.8 m ahead and
             # 0.25 m out: the man lies there when the horse turns 25° from him
             # (`strike_view::TURN_LANCE`). The steep 65° of m2-melee4 drove the point 0.9 m
             # into the ground and left the layer to flatten it.
             "lance_down": _with_world(dict(LEGS_SEAT, spine=(-6, -8, 0), spine1=(-2, -4, 0)),
                                       right(side(-0.20, -0.08, 0.52), (44, 8, 0))),
             "lance_mid": _with_world(dict(LEGS_SEAT, spine=(-14, 0, 8)), right(side(0.02, -0.66, 0.36), (2, 0, 0))),
             # Drawn sabre (`drawn`): raised over the right shoulder, the blade up and back;
             # the cuts down at a man on the ground right and left of the horse, and the cut
             # at a standing man (the blade level with his shoulder).
             "sabre_up": _with_world(dict(LEGS_SEAT, spine=(4, 0, -10)), right(side(0.06, 0.04, 0.78), (-110, 0, 0))),
             # Down right (m2-melee5): leaning far out over the right side, the hand low
             # beside the horse, the blade driven almost straight down at a man lying
             # about 1.2 m out (the horse turned 75° from him, `strike_view::TURN_SABRE`).
             "sabre_down_r": _with_world(dict(LEGS_SEAT, spine=(-10, -30, 0), spine1=(-4, -14, 0)),
                                         right(side(-0.65, -0.48, -0.23), (80, 15, 0))),
             "sabre_down_l": _with_world(dict(LEGS_SEAT, spine=(-12, 14, -14), spine1=(-4, 6, 0)),
                                         right(side(0.42, -0.36, 0.14), (70, -30, 0))),
             "sabre_mid": _with_world(dict(LEGS_SEAT, spine=(-8, -8, 6)), right(side(-0.16, -0.46, 0.36), (25, 25, 0))),
             # A gun upright, its butt on the right thigh (the socket turned from the aim's).
             "gun": _with_world(dict(LEGS_SEAT), right(side(0.04, -0.22, 0.12), None, (-2, 0, 0)))}
    for k in ("lance", "gun", "flag"):
        poses[k + "_lean"] = dict(poses[k], spine=(-12, 0, 0))
    lie_spec = dict(rig_lie(), world={"root": (90, 0, heading)}, world_loc={"root": lie})
    ch.poses, ch.lie = poses, lie_spec
    ch.ride = ride_pose(spec)
    if spec.get("gun"):
        am.pose(ch.arm, poses["aim"], prefix=P)
        gun(ch, f"{name}.gun", spec["gun"])
    am.pose(ch.arm, poses[ch.ride], prefix=P)

    hat = spec.get("hat", "cap")
    if hat in ("feathers", "crown"):
        native_head(ch, name, hat, cy)
    elif hat == "morion":
        morion(ch, name)
    elif hat:
        if hat == "cap":
            cap = gear.box(f"{name}.cap", (0.15, 0.17, 0.26), bevel=0.3)
            for v in cap.data.vertices:
                if v.co.z > 0:
                    v.co.x *= 0.55
                    v.co.y *= 0.7
            lift = 0.10
        else:
            cap = gear.box(f"{name}.cap", (0.30, 0.28, 0.09), bevel=0.4)
            lift = 0.05
        am.attach(cap, ch.arm, P + "socket.hat", Matrix.Translation(am.world(ch.arm, P + "socket.hat").translation + Vector((0, 0, lift))))
        ch.held["cap"] = cap
        ch.keys[cap.name] = "cap"
    if spec.get("held") == "flag":
        flag(ch, f"{name}.flag")
    elif spec.get("held") == "lance":
        lance(ch, f"{name}.lance")
    if spec.get("shield"):
        shield(ch, f"{name}.shield")
    if spec.get("drawn"):
        am.pose(ch.arm, poses["seat"], prefix=P)
        blade(ch, f"{name}.blade")
        am.pose(ch.arm, poses[ch.ride], prefix=P)
        ch.show = dict(SHOW_REST)
    if spec.get("sabre", True):
        sab = gear.sabre(f"{name}.sabre")
        am.attach(sab, ch.arm, P + "socket.hip_l", Matrix.Translation(am.world(ch.arm, P + "socket.hip_l").translation)
                  @ Matrix.Rotation(math.radians(40), 4, "X"))
        ch.held["sabre"] = sab
        ch.keys[sab.name] = "metal"
    return ch


def _keyed(arm, specs):
    still = ("index_", "middle_", "ring_", "pinky_", "thumb_", "socket.")
    rot = {b.name[len(P):] for b in arm.data.bones if b.name.startswith(P)
           and not any(b.name[len(P):].startswith(t) for t in still)}
    loc = set()
    for s in specs:
        r, lo = am.spec_bones(s)
        rot |= r
        loc |= lo
    return rot, loc


def _key(ch, name, horse_frames, rider_keys):
    keyed = _keyed(ch.arm, [{k: v for k, v in s.items() if k != "show"} for _f, s in rider_keys])
    show = getattr(ch, "show", None)

    def pose(arm, s, f):
        am.pose(arm, {k: v for k, v in s.items() if k != "show"}, frame=f, prefix=P, keyed=keyed)
        # A rider with a drawn sabre: every clip keys the sockets' scale (`SHOW_*`); a clip
        # without the channel would show them all.
        for bone, default in (show or {}).items():
            k = s.get("show", {}).get(bone, default)
            pb = arm.pose.bones[P + bone]
            pb.scale = (k, k, k)
            pb.keyframe_insert("scale", frame=f, group=pb.name)

    mount.key_clip(ch.arm, name, horse_frames, rider_keys, pose)


def thrown(ch, t):
    if t >= 1.0:
        return ch.lie
    s = ch.poses["seat"]
    return dict(s, spine=(-30 * t, 0, 0), thigh_l=(-50, -30, 0), thigh_r=(-50, 30, 0),
                world={k: v for k, v in s["world"].items() if not k.startswith("upperarm")},
                upperarm_l=(-120 * t, 40, 0), upperarm_r=(-120 * t, -40, 0))


def ride_pose(spec):
    """The pose the rider sits in (stand, rest, moves): his right hand's charge."""
    held = spec.get("held")
    if held in ("flag", "lance"):
        return held
    return "gun" if spec.get("gun") else "seat"


def add_clips(ch, md, motions=MOTIONS):
    W = ch.horse["mount"].worlds
    p = ch.poses
    ride = p[ch.ride]
    lean = p["lean"] if ch.ride == "seat" else p[ch.ride + "_lean"]
    hold = [(0.0, ride), (1.0, ride)]
    aim = [(0.0, p["aim"])]
    plan = {
        "STAND": (W["STAND"], [(0.0, ride)]), "REST": (W["REST"], hold),
        "WALK": (W["WALK"], hold), "GALLOP": (W["GALLOP"], [(0.0, lean), (1.0, lean)]),
        # Index 0 (cold weapon) stands as he rides; index 1 (the shot) aims.
        "PATTACK": (W["STAND"], [(0.0, ride)]), "UATTACK": (W["STAND"], [(0.0, ride)]),
        "PSTAND": (W["STAND"], [(0.0, ride)]),
        "DEATH": (W["DEATH"], [(0.0, ride), (0.35, thrown(ch, 0.5)), (0.75, thrown(ch, 0.9)), (1.0, ch.lie)]),
        "DEATHLIE1": (W["DEATH"][-1:], [(0.0, ch.lie)]),
    }
    for k in ("PATTACK1", "UATTACK1", "PSTAND1"):
        if k in md:
            plan[k] = (W["STAND"], aim)
    for sfx, (gait, _k) in motions.items():
        if "MOTION_L" + sfx not in W:
            continue
        pose = [(0.0, lean), (1.0, lean)] if gait == "G3_Gallop" else hold
        plan["MOTION_L" + sfx], plan["MOTION_R" + sfx] = (W["MOTION_L" + sfx], pose), (W["MOTION_R" + sfx], pose)
        plan["MOTION_LB" + sfx], plan["MOTION_RB" + sfx] = (W["MOTION_R" + sfx][::-1], pose), (W["MOTION_L" + sfx][::-1], pose)
    if "ATTACK" in W:
        if ch.ride == "lance":
            cold = [(0.0, ride), (0.3, p["thrust"]), (0.5, p["thrust"]), (0.8, ride), (1.0, ride)]
        else:
            cold = [(0.0, ride), (0.3, p["cut_up"]), (0.45, p["cut_down"]), (0.7, ride), (1.0, ride)]
        plan["ATTACK"] = (W["ATTACK"], cold)
    if "ATTACK" in W and ch.ride == "lance":
        # Strike clips on ATTACK's frames, the blow on `poses.STRIKE_HIT_FRAME`.
        n = len(W["ATTACK"])
        down = poses.strike_keys(n, p["lance_back"], p["lance_down"], ride)
        mid = poses.strike_keys(n, p["lance_back"], p["lance_mid"], ride)
        for name, keys in (("ATTACK_THRUST_DOWN", down), ("ATTACK_DOWN", down), ("ATTACK_THRUST_MID", mid),
                           ("ATTACK_THRUST_HIGH", mid)):
            plan[name] = (W["ATTACK"], keys)
    if "ATTACK" in W and getattr(ch, "show", None):
        # The drawn sabre (`drawn`): ATTACK and its strike clips show the blade, hide the gun.
        n = len(W["ATTACK"])

        def drawn(s):
            return dict(s, show=SHOW_MELEE)

        def cut(hit):
            return [(t, drawn(s)) for t, s in poses.strike_keys(n, p["sabre_up"], hit, ride)]

        ride_d = drawn(ride)
        plan["ATTACK"] = (W["ATTACK"], [(0.0, ride_d), (0.3, drawn(p["sabre_up"])), (0.45, drawn(p["sabre_down_r"])),
                                         (0.7, ride_d), (1.0, ride_d)])
        for name, keys in (("ATTACK_CUT_DOWN", cut(p["sabre_down_r"])), ("ATTACK_DOWN", cut(p["sabre_down_r"])),
                           ("ATTACK_CUT_DOWN_R", cut(p["sabre_down_r"])), ("ATTACK_CUT_DOWN_L", cut(p["sabre_down_l"])),
                           ("ATTACK_CUT_MID", cut(p["sabre_mid"])), ("ATTACK_CUT_HIGH", cut(p["sabre_mid"]))):
            plan[name] = (W["ATTACK"], keys)
    if "ATTACK1" in W:
        plan["ATTACK1"] = (W["ATTACK1"], [(0.0, p["aim"]), (0.08, dict(p["aim"], spine=(4, 0, 0))), (0.25, p["aim"]), (1.0, p["aim"])])
    for name, (hf, rk) in plan.items():
        n = len(hf)
        _key(ch, name, hf, [(int(round(t * (n - 1))), s) for t, s in rk])
    lie_loc = ch.lie["world_loc"]["root"]
    for name, ks in crawl.clips().items():
        rider_keys = []
        for f, s in ks:
            s = dict(s)
            loc = s.pop("root_loc")
            s["world"] = {"root": s.pop("root")}
            s["world_loc"] = {"root": (lie_loc[0] + loc[0], lie_loc[1] + loc[1] - crawl.PRONE_LOC[1], loc[2])}
            rider_keys.append((f, s))
        _key(ch, name, W["DEATH"][-1:], rider_keys)
    return list(plan)


def unscale(ch):
    """Every rider bone back to scale 1 (a clip's keyed socket scale stays on the pose when
    its action is unset): the exporter takes the held objects' nodes relative to their
    bone as posed, and a socket at 0 would bake a 0 scale into the drawn sabre for good."""
    for pb in ch.arm.pose.bones:
        if pb.name.startswith(P):
            pb.scale = (1.0, 1.0, 1.0)
    bpy.context.view_layer.update()


def build(ctx, spec):
    ch = make(ctx, spec)
    hz = ch.horse["mount"]
    heads.variants(ch)
    ch.morphs()
    rider = ch.layers["flesh"] + ch.layers["clothes"]
    ch.masks()
    made = add_clips(ch, spec["md"], spec.get("motions", MOTIONS))
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, spec["palette"], report=report)
    heads.finish(ch, rider)
    am.set_clip(ch.arm, "STAND")
    ch.arm.animation_data.action = None
    horse_meshes = hz.meshes + ch.horse["extra"]
    for o in horse_meshes:
        part = o.data.attributes.get("_part") or o.data.attributes.new("_part", "FLOAT", "POINT")
        for d in part.data:
            d.value = float(horse.HORSE_PART)
    tris = ch.triangles()
    tris["horse"] = hz.stats["triangles"]
    for o in ch.horse["extra"]:
        o.data.calc_loop_triangles()
        tris[o.name] = len(o.data.loop_triangles)
    unscale(ch)
    extra_clips = [c for c in made if c not in spec["md"]]
    prepare_export(ch.all_meshes() + horse_meshes, ctx.root, acr_triangles=sum(tris.values()),
                   acr_clips=[c for c in made if c in spec["md"]] + list(crawl.CLIPS), acr_limb_radius=ch.limb_radius,
                   acr_layers=list(ch.layers), acr_extra_clips=extra_clips, **faces.extras(spec.get("faces", "regular")))
    # Where things are in the corpse (a check that the rider lies clear of the horse).
    am.set_clip(ch.arm, "DEATHLIE1")
    pb = ch.arm.pose.bones
    corpse = {k: [round(x, 2) for x in pb[b].head] for k, b in (("horse_spine", "spine_02"), ("horse_head", "head"),
                                                                ("rider_root", P + "root"), ("rider_head", P + "head"))}
    ch.arm.animation_data.action = None
    am.set_clip(ch.arm, "STAND")
    ch.arm.animation_data.action = None
    unscale(ch)
    ctx.stats = {"triangles": tris, "colours": colours, "horse": hz.stats, "corpse": corpse,
                 "fist_vertices": getattr(ch, "fist_vertices", 0)}
