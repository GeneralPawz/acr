"""Mounted units on the WildMesh horse (#287, #275): the dragoons and mounted officers of
all nations (family 5, docs/families/cavalry.md), the cavalry standard bearers (*KF18),
the Spanish reiter (SpnRei) and the native riders.

A unit script gives a spec and calls `build(ctx, spec)`:

    md        {key: ".MD line"} (`rig.md_clip_frames` reads the frame counts)
    palette   material keys -> colour (the sprite reference refines them)
    dress     "coat" (18th-century coat, waistcoat, breeches, high boots, cap), "native"
              (bare chest, breechcloth, leggings), "buff17" (17th century: a buff coat with
              skirts to above the knee, sleeves of `sleeves`, a falling band, bucket boots;
              `drape.falling_band`, `drape.bucket_boots`)
    sleeves   buff17: "dsleeve" (cloth sleeves, troopers) or "buff" (officers)
    arms      True: steel arm defences over the sleeves (`metal`, the reiter)
    holster   True: a second pistol in the left holster (`holster_pistol`, the reiter's pair)
    paint     True: the realistic atlas (`outfit.dress`: the buff painter, linen, leather)
    fixed     material keys that keep their palette colour (`Character.colours`)
    tack      "military" (scripted saddle with pistol holsters on a saddlecloth),
              "blanket" (a blanket, no saddle, a rope bridle), "bare" (a rope bridle)
    held      gear in the right hand: "flag" (a standard, the fist round the pole), "lance"
              (couched, thrust in ATTACK), "bow" (a mounted archer: the bow in the left fist,
              an arrow in the right, a quiver on the back; ATTACK1/ATTACK2 draw, loose and
              nock the next, `bow_poses`), None (reins)
    hat       "cap" (tall mitre cap), "tricorne" (the line's felt tricorn), "bonnet" (a
              dragoon's stocking cap on a fur turban), "helmet" (light dragoon's leather
              helmet, brass comb, horsehair crest), "broad" (17th-century broad-brimmed hat
              with a plume), "morion" (crested helmet with a plume), "zischagge" (lobster-pot
              helmet: skull, peak with face bars, cheek pieces, a laminated tail; `plume`), "feathers" (long hair,
              a feather or two upright), "crown" (long hair, a headband of feathers), None
    plume     tricorne: a feather edging round the brim (`crest`)
    facings   True: turned-back cuffs and lapels in `palette["facings"]`
    lace      True (officers): lace on cuffs and lapel edges and round the tricorne's brim
              (`palette["lace"]`, geometry: the rider's clothes are shells, not painted)
    sash      True (officers): a waist sash (`sash`)
    blade     the drawn blade (`drawn`): "sabre" (dragoons), "sword" (officers:
              `held.officer_sword`) or "club" (native riders: `held.war_club`)
    shirt     natives: a shirt over the bare chest (`tunic`)
    armour    a steel cuirass over the coat (`metal`)
    shield    a round hide shield on the left forearm (`knapsack`)
    gun       "carbine", "wheellock" (a short `held.matchlock`, the 17th century's carbine) or
              "pistol" (`held.pistol`) in the right fist: upright on the thigh while he rides,
              level for the shot (ATTACK1, PATTACK1...)
    sabre     False: no sabre at the hip
    drawn     True: a drawn sabre for the melee clips besides the gun (EngD18): it sits on a
              second right-hand socket (`socket.blade_r`); the clips key the sockets' scale,
              so the melee clips show the blade and hide the gun and the sheathed sabre
              (`SHOW_MELEE`), all others the reverse (`SHOW_REST`)
    coat      the horse's coat tint {"body": rgb, "hair": rgb} (`mount.COAT`; `HORSE_COATS`)
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
from . import aim as aiming
from . import crawl, faces, gear, heads, horse, mount, poses, registry, rig
from .humans import Character, prepare_export

P = "rider."
# The family's own material keys (`registry`): a horsehair crest or feather plume.
registry.material("crest", "hair", fallback=(0.92, 0.90, 0.86))
registry.material("lace", "wool", fallback=(0.80, 0.64, 0.28))  # as `masks.py`
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
    "facings": (0.15, 0.20, 0.50), "lace": (0.80, 0.64, 0.28), "crest": (0.92, 0.90, 0.86), "brass": (0.72, 0.58, 0.30),
}
# The 17th-century riders (`dress` "buff17"): buff, linen band and blacked bucket boots keep
# these colours (`FIXED17`); the cloth sleeves (`dsleeve`) take the sprite's coat colour.
PALETTE17 = {"buff": (0.74, 0.58, 0.36), "fallband": (0.93, 0.92, 0.88), "jackboots": (0.16, 0.11, 0.07)}
FIXED17 = ("buff", "fallband", "jackboots")
# Horse coats by the sprites (`mount.COAT` tints: body, mane and tail).
HORSE_COATS = {
    "black": {"body": (0.18, 0.15, 0.14), "hair": (0.08, 0.07, 0.06)},
    "bay": {"body": (0.45, 0.28, 0.16), "hair": (0.12, 0.09, 0.07)},
    "grey": {"body": (0.74, 0.73, 0.70), "hair": (0.58, 0.57, 0.55)},
    "dapple": {"body": (0.40, 0.40, 0.40), "hair": (0.14, 0.14, 0.14)},
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
# The cut down left (`strike_view::TURN_SABRE_L`: the man 40° left of the facing, 0.95 m
# out, his near part at `SABRE_DOWN_L_AT`): standing in the stirrups, the seat shifted
# left and forward; leaning out to the left with the sabre in the right hand keeps the
# right shoulder high (m2-melee6: a side lean, 0.54 m short). Now the trunk is bent far
# forward (armature Euler X, positive: forward; m2-melee6's negative X leaned back) and
# then turned left (Z) with a little side lean (Y): the right shoulder comes forward and
# down over the horse's left shoulder (about 0.36 left, 0.51 ahead, 1.59 m up) and the
# blade goes almost straight down past the neck: a cross-body cut (Blender's solve: 0.03 m).
SABRE_DOWN_L = {"root_loc": (0.25, -0.15, -0.10), "spine": (60, 20, 25), "spine1": (25, 10, 15), "spine2": (12, 5, 10)}
SABRE_DOWN_R = {"root_loc": (-0.22, 0.0, -0.02), "spine": (-20, -38, 0), "spine1": (-8, -22, 0), "spine2": (-4, -14, 0)}
# The war club down right (SiuKVog, SiuKLut): the club reaches 0.42 m along its handle
# (its head 0.165 m before it, `weapon_tip`), so the rider hangs far out over the right
# side, the seat over the right stirrup.
CLUB_DOWN_R = {"root_loc": (-0.40, -0.05, -0.18), "spine": (20, -80, 0), "spine1": (10, -35, 0), "spine2": (5, -20, 0)}
CLUB_DOWN_R_AXIS = (60, 70)
SABRE_DOWN_L_AT = Vector((0.61, -0.73, 0.20))
SABRE_DOWN_L_AXIS = (80, -5)  # degrees down, out (negative: left); `axis_of`
# Fists (`close_hands`): the mitten's fingers, from `KNUCKLE` metres past the wrist, wrap
# round a cylinder of radius `GRIP` before the palm, where pole, lance and grips go.
KNUCKLE, GRIP = 0.08, 0.03
# The flag hand: fist axis (rest Y) upright, knuckles forward, palm towards the rider.
FLAG_HAND = (-90, 0, 0)
LANCE = (2.9, 0.65)  # length, and how far it reaches behind the grip (metres)
BLADE = (0.86, 0.10)  # a drawn sabre: blade length, hilt behind the grip (metres)
CLUB_TIP = 0.52  # a war club: grip to the striking face (`clips.CLUB_TIP`)
ARROW = 0.72  # nock to head (`held.arrow`)
BOW_NOCK = 0.105  # the string behind the bow's grip at brace (`held.longbow`)
# Mounted archer (`bow_poses`): the trunk turned left (negative Z brings the left shoulder
# forward), the head turned back to the target ahead.
BOW_TRUNK = {"spine": (0, 0, -18), "spine1": (0, 0, -16), "spine2": (0, 0, -12), "head": (4, 0, 40)}
# Where the arrow is shot (armature space: -y forward, +x left), relative to the left
# shoulder as posed: the bow arm out at shoulder height.
BOW_FULL = (0.02, -0.58, 0.04)
BOW_LOW = (0.06, -0.36, -0.30)
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


def reins_world(arm, targets, sides=("l", "r"), shoulders=None):
    """{"world": {...}} orientations of upper arm, forearm and hand per side so the wrist
    reaches `targets[side]` (armature space, rest) and the hand, closed, points forward
    and down along the reins. `shoulders` ({side: point}): the shoulders as the trunk is
    posed (a rider leaning out); else at rest, and a leaning trunk carries the hand off
    the target (m2-melee6: why the sabre's hand stopped ~1.45 m up)."""
    out = {}
    B = arm.data.bones
    for sd in sides:
        sgn = 1.0 if sd == "l" else -1.0
        up, fo, ha = B[P + "upperarm_" + sd], B[P + "forearm_" + sd], B[P + "hand_" + sd]
        s = shoulders[sd] if shoulders else up.head_local
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


def blade(ch, name, kind="sabre"):
    """A drawn sabre in the closed right hand along the fist's axis (rest -Y), on the second
    right-hand socket `socket.blade_r` (shown only in the melee clips): a flat blade narrowing
    to the point, a grip and a guard (`metal`), the point marked by `socket.tip`."""
    c, rot = fist_centre(ch)
    m = Matrix.Translation(c) @ rot.to_4x4()
    if kind == "club":
        # A war club (`held.war_club`): its handle (+Z) along the fist, `socket.tip` on the ball.
        return gear.hold(ch, "war_club", name, socket=P + "socket.blade_r",
                         matrix=m @ Matrix.Rotation(math.radians(90), 4, "X"), slot="blade")
    if kind == "sword":
        # An officer's sword (`held.officer_sword`, brass hilt): its +Z along the fist (-Y).
        return gear.hold(ch, "officer_sword", name, socket=P + "socket.blade_r",
                         matrix=m @ Matrix.Rotation(math.radians(90), 4, "X"), slot="blade")
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
    (the socket level), the barrel forward. Keeps the carbine's frame in the socket's
    (`ch.carbine`: origin the fist's centre, +Y to the muzzle, +Z up) for the reload."""
    c, _rot = fist_centre(ch)
    s_aim = ch.arm.pose.bones[P + "socket.hand_r"].matrix.copy()
    ch.carbine = s_aim.inverted() @ _frame(c, Vector((0.0, -1.0, 0.0)), Vector((0.0, 0.0, 1.0)))
    if kind == "pistol":
        # The library's flintlock (`held.pistol`): barrel (+Z) forward, butt down.
        m = Matrix.Translation(c) @ Matrix.Rotation(math.radians(90), 4, "X") @ Matrix.Rotation(math.pi, 4, "Z")
        obj = gear.hold(ch, "pistol", f"{name}", socket=P + "socket.hand_r", matrix=m, slot="gun")
        # No `socket.muzzle`: under the hand's socket it would be the strike layer's weapon
        # (`strike_view::weapon` takes a muzzle first) and hide a drawn sword's `socket.tip`.
        for e in [c for c in obj.children if c.name.endswith("socket.muzzle")]:
            bpy.data.objects.remove(e)
        return obj
    if kind == "wheellock":
        # A short 17th-century long arm (`held.matchlock` at 0.95 m) held like the pistol.
        from . import held as library

        m = Matrix.Translation(c) @ Matrix.Rotation(math.radians(90), 4, "X") @ Matrix.Rotation(math.pi, 4, "Z")
        obj = gear.hold(ch, library.matchlock(length=0.95), f"{name}", socket=P + "socket.hand_r", matrix=m, slot="gun")
        for e in [c for c in obj.children if c.name.endswith("socket.muzzle")]:
            bpy.data.objects.remove(e)
        return obj
    ln = 1.05
    bm = bmesh.new()
    _box(bm, Vector((0, -ln / 2 + 0.12, 0.0)), (0.03, ln, 0.035))  # barrel and stock
    _box(bm, Vector((0, 0.02, -0.05)), (0.03, 0.08, 0.10), Matrix.Rotation(math.radians(-20), 4, "X"))  # grip
    for v in bm.verts:
        v.co += c
    return _held(ch, name, "wood", bm, "socket.hand_r", Matrix.Identity(4))


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


def zischagge(ch, name, plume=None):
    """A lobster-pot helmet (zischagge): a rounded skull with a low comb, a peak with three
    face bars, cheek pieces and a tail of four lames over the nape (`metal`); `plume`: a
    plume rising from the back of the comb (`sash` and `crest`, red and white)."""
    sock = am.world(ch.arm, P + "socket.hat").translation + Vector((0, 0, -0.03))
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=16, v_segments=8, radius=0.12)
    for v in bm.verts:
        v.co.z = max(v.co.z, -0.005) * 1.12
        v.co.y *= 1.08
    _box(bm, Vector((0, 0.0, 0.14)), (0.010, 0.17, 0.03))  # comb
    _box(bm, Vector((0, -0.15, 0.0)), (0.19, 0.08, 0.008), Matrix.Rotation(math.radians(-12), 4, "X"))  # peak
    for x in (-0.035, 0.0, 0.035):  # face bars from the peak to the chin
        _tube(bm, Vector((x, -0.18, -0.005)), Vector((x * 0.8, -0.15, -0.20)), 0.005, 0.005, 5)
    for sx in (-1, 1):  # cheek pieces
        _box(bm, Vector((sx * 0.115, -0.03, -0.09)), (0.010, 0.10, 0.13), Matrix.Rotation(math.radians(sx * 6), 4, "Y"))
    for k in range(4):  # the lobster tail
        _box(bm, Vector((0, 0.12 + 0.018 * k, -0.02 - 0.045 * k)), (0.20 - 0.008 * k, 0.012, 0.05),
             Matrix.Rotation(math.radians(20 + 6 * k), 4, "X"))
    for v in bm.verts:
        v.co += sock
    obj = _held(ch, f"{name}_zischagge", "metal", bm, "socket.hat", Matrix.Identity(4))
    if plume:
        for i, (key, dx) in enumerate((("sash", -0.025), ("crest", 0.025))):
            bm = bmesh.new()
            pts = [sock + Vector((dx, 0.06 + 0.10 * t, 0.15 + 0.16 * t - 0.10 * t * t)) for t in (0.0, 0.35, 0.7, 1.0)]
            for j, r in zip(range(3), (0.025, 0.035, 0.03)):
                _tube(bm, pts[j], pts[j + 1], r, r * 0.8, 7)
            _held(ch, f"{name}_plume{i}", key, bm, "socket.hat", Matrix.Identity(4))
    return obj


def holster_pistol(ch, hz, name):
    """The second of a pair of pistols in the left saddle holster (`military_saddle`), its
    butt out of the bearskin cap; rigid on the horse's `saddle` bone."""
    sad = hz.head("saddle")
    top = back_top(hz, sad.y)
    w = max(_body_section(hz, sad.y, top - 0.08), 0.12)
    a = Vector((w + 0.06, sad.y - 0.28, top + 0.04))
    down = Vector((0.06, -0.10, -0.30)).normalized()
    # `held.pistol`: the barrel +Z into the holster, the butt rising out of the cap.
    rot = Vector((0, 0, 1)).rotation_difference(down).to_matrix().to_4x4()
    obj = gear.held("pistol", f"{name}.holster_pistol")
    for e in [c for c in obj.children if ".socket." in c.name]:
        bpy.data.objects.remove(e)
    gear.rigid(obj, hz.arm, "saddle", Matrix.Translation(a - down * 0.10) @ rot, part=horse.HORSE_PART)
    return obj


def _ring_tubes(bm, pts, r, closed=True):
    n = len(pts)
    for i in range(n if closed else n - 1):
        _tube(bm, pts[i], pts[(i + 1) % n], r, r, 5)


def tricorne(ch, name, lace=False, plume=None):
    """The felt tricorne of the line (`gear.tricorn`, `hat`) on the hat socket; `lace`: a
    braid round the brim's edge (`lace`, a ring of thin tubes: the rider's outfit is not
    painted, so lace is geometry); `plume`: a feather edging along the brim (`crest`)."""
    sock = am.world(ch.arm, P + "socket.hat").translation
    hat = gear.tricorn(f"{name}.hat")
    for v in hat.data.vertices:
        v.co += sock
    am.attach(hat, ch.arm, P + "socket.hat", Matrix.Identity(4))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "hat"
    rim = sorted((v.co.copy() for v in hat.data.vertices if (v.co - sock).xy.length > 0.135),
                 key=lambda c: math.atan2((c - sock).y, (c - sock).x))
    if lace and rim:
        bm = bmesh.new()
        _ring_tubes(bm, rim, 0.007)
        _held(ch, f"{name}.hat_lace", "lace", bm, "socket.hat", Matrix.Identity(4))
    if plume and rim:
        bm = bmesh.new()
        _ring_tubes(bm, [c + Vector((0, 0, 0.012)) for c in rim], 0.016)
        _held(ch, f"{name}.hat_plume", "crest", bm, "socket.hat", Matrix.Identity(4))
    return hat


def bonnet(ch, name):
    """A dragoon's bonnet (stocking cap): a fur turban round the head (`hat`) and the cloth
    bag (`cap`) rising from it and falling back, its end with a tassel."""
    sock = am.world(ch.arm, P + "socket.hat").translation
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=14, radius1=0.118, radius2=0.122, depth=0.07,
                          matrix=Matrix.Translation(sock + Vector((0, 0.005, -0.005))))
    for v in bm.verts:
        v.co.y = sock.y + (v.co.y - sock.y) * 1.1
    _held(ch, f"{name}.turban", "hat", bm, "socket.hat", Matrix.Identity(4))
    bm = bmesh.new()
    pts = [sock + Vector((0, 0.01 + 0.10 * t * t, 0.03 + 0.20 * t - 0.12 * t ** 3)) for t in (0.0, 0.3, 0.55, 0.8, 1.0)]
    rad = [0.105, 0.09, 0.07, 0.05, 0.035]
    for i in range(len(pts) - 1):
        _tube(bm, pts[i], pts[i + 1], rad[i], rad[i + 1], 10)
    _tube(bm, pts[-1], pts[-1] + Vector((0, 0.03, -0.06)), 0.03, 0.012, 6)
    return _held(ch, f"{name}.bonnet", "cap", bm, "socket.hat", Matrix.Identity(4))


def broad_hat(ch, name, plume=True):
    """A 17th-century broad-brimmed felt hat (`gear.slouch_hat`, `hat`), the brim turned
    up on the left, with an ostrich plume (`crest`) lying along it."""
    sock = am.world(ch.arm, P + "socket.hat").translation
    hat = gear.slouch_hat(f"{name}.hat")
    for v in hat.data.vertices:
        r = v.co.xy.length
        if r > 0.12:
            v.co *= 1.12
            if v.co.x > 0.0:
                v.co.z += 0.10 * (r - 0.12) / 0.09 * (v.co.x / r)
        v.co += sock + Vector((0, 0, 0.01))
    am.attach(hat, ch.arm, P + "socket.hat", Matrix.Identity(4))
    ch.held["hat"] = hat
    ch.keys[hat.name] = "hat"
    if plume:
        bm = bmesh.new()
        pts = [sock + Vector((0.06 - 0.12 * t, 0.12 * math.sin(math.pi * t) - 0.02, 0.10 + 0.05 * math.sin(math.pi * t)))
               for t in (0.0, 0.25, 0.5, 0.75, 1.0)]
        for i, r in zip(range(4), (0.03, 0.045, 0.04, 0.025)):
            _tube(bm, pts[i], pts[i + 1], r, r * 0.85, 8)
        _held(ch, f"{name}.plume", "crest", bm, "socket.hat", Matrix.Identity(4))
    return hat


def helmet(ch, name):
    """A light dragoon's leather helmet: the skull (`cap`), a brass comb front to back
    (`brass`) and a horsehair crest along it falling back (`crest`)."""
    sock = am.world(ch.arm, P + "socket.hat").translation
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=16, v_segments=8, radius=0.118)
    for v in bm.verts:
        v.co.z = max(v.co.z, -0.02) * 1.15
        v.co.y *= 1.08
        v.co += sock + Vector((0, 0, -0.01))
    _held(ch, f"{name}.helmet", "cap", bm, "socket.hat", Matrix.Identity(4))
    bm = bmesh.new()
    _box(bm, sock + Vector((0, 0.0, 0.135)), (0.02, 0.22, 0.05))
    _held(ch, f"{name}.comb", "brass", bm, "socket.hat", Matrix.Identity(4))
    bm = bmesh.new()
    pts = [sock + Vector((0, -0.10 + 0.24 * t, 0.17 - 0.05 * t - 0.25 * max(t - 0.6, 0.0) ** 2 * 6)) for t in (0.0, 0.3, 0.6, 0.8, 1.0)]
    for i, r in zip(range(4), (0.03, 0.035, 0.035, 0.028)):
        _tube(bm, pts[i], pts[i + 1], r, r, 8)
    return _held(ch, f"{name}.crest", "crest", bm, "socket.hat", Matrix.Identity(4))


def uniform(ch, spec, oz, cy):
    """The coat's distinctions (shells over the coat): `facings` turned-back cuffs and
    lapels; `lace` (officers) a band of lace on the cuffs and down the lapels' edges; `sash`
    a waist sash (officers)."""
    B = ch.arm.data.bones
    wrist = {sd: B[P + "hand_" + sd].head_local.copy() for sd in "lr"}

    def cuff(c, h, near):
        return h in ("forearm_l", "forearm_r") and (c - wrist[h[-1]]).length < near

    def lapel(c, h, x0, x1):
        return h in TORSO and c.y - cy < -0.08 and x0 < abs(c.x) < x1 and 1.16 + oz < c.z < 1.45 + oz

    if spec.get("facings"):
        ch.shell("cuffs", "facings", lambda c, n, h, p: cuff(c, h, 0.11), 0.042)
        ch.shell("lapels", "facings", lambda c, n, h, p: lapel(c, h, 0.015, 0.085), 0.038)
    if spec.get("lace"):
        ch.shell("cuff_lace", "lace", lambda c, n, h, p: cuff(c, h, 0.115) and (c - wrist[h[-1]]).length > 0.085, 0.048)
        ch.shell("lapel_lace", "lace", lambda c, n, h, p: lapel(c, h, 0.07, 0.095), 0.044)
    if spec.get("sash"):
        ch.shell("sash", "sash", lambda c, n, h, p: h in TORSO | {"hips"} and 0.98 + oz < c.z < 1.07 + oz, 0.046)


def native_head(ch, name, kind, cy):
    """Long black hair down the back (`hair`) and feathers (`cap`): "feathers" (two
    upright at the back), "crown" (a headband of feathers all round) or "band" (the Pueblo
    look: the hair cut to the jaw at the sides, a knot at the nape, a folded cloth band
    round the brow)."""
    head = am.world(ch.arm, P + "head").translation
    bm = bmesh.new()
    if kind == "band":
        sock = am.world(ch.arm, P + "socket.hat").translation
        _box(bm, Vector((0, cy + 0.045, head.z + 0.02)), (0.175, 0.11, 0.20))
        _box(bm, Vector((0, cy + 0.11, head.z - 0.06)), (0.07, 0.05, 0.08))  # the knot
        _held(ch, f"{name}_hair", "hair", bm, "head", Matrix.Identity(4))
        bm = bmesh.new()
        n = 14
        for i in range(n):
            a = 2 * math.pi * (i + 0.5) / n
            r = Vector((math.sin(a) * 0.098, -math.cos(a) * 0.108 + 0.01, 0))
            _box(bm, sock + r + Vector((0, 0, -0.06)), (0.048, 0.016, 0.035), Matrix.Rotation(a, 4, "Z"))
        return _held(ch, f"{name}_band", "cap", bm, "socket.hat", Matrix.Identity(4))
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
    corpse lies; eased over the stumble (`rig.fall_slide`), done once the body is down."""
    W = hz.worlds.get("DEATH")
    if not W:
        return
    end = W[-1][bone].translation
    off = Vector((end.x - to[0], end.y - to[1], 0.0))
    n = len(W)
    for i, fr in enumerate(W):
        t = i / max(n - 1, 1)
        T = Matrix.Translation(-off * rig.fall_slide(t))
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


def horse_clips(md, motions=MOTIONS, reload=False):
    """`reload`: the horse stands in its idle through ATTACK1 (the mounted reload) instead
    of the fidget."""
    out = {}
    for key, clip in GAITS.items():
        if key in ("ATTACK", "ATTACK1") and key not in md:
            continue
        if key == "ATTACK1" and reload:
            clip = ("Idle", "loop", None)
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


def bow(ch, name):
    """A mounted archer's bow in the left fist (its +Z along the fist's axis: the clips
    turn the fist upright), an arrow in the right (nock at the fist), a quiver on the back
    (its mouth behind the right shoulder). Call in the seat pose."""
    out = {}
    for item, side, socket in (("longbow", "l", "socket.hand_l"), ("arrow", "r", "socket.hand_r")):
        c, rot = fist_centre(ch, side)
        m = Matrix.Translation(c) @ rot.to_4x4() @ Matrix.Rotation(math.radians(90), 4, "X")
        out[item] = gear.hold(ch, item, f"{name}.{item}", socket=P + socket, matrix=m, slot=item)
    sh = ch.arm.pose.bones[P + "upperarm_r"].head
    at = Vector((sh.x * 0.3, sh.y + 0.20, sh.z - 0.30))
    out["quiver"] = gear.hold(ch, "quiver", f"{name}.quiver", socket=P + "spine2", slot="quiver",
                              matrix=Matrix.Translation(at) @ Matrix.Rotation(math.radians(-22), 4, "Y"))
    ch.quiver_mouth = at + Vector((-0.20, 0.0, 0.40))
    return out


def bow_pose(ch, left, trunk, bow_at, bow_axis, nock_at, arrow_dir, shown=True):
    """A pose of the bow clips: the trunk, the left fist at `bow_at` (the bow along
    `bow_axis`), the right fist at `nock_at` (the arrow along `arrow_dir`); without
    `shown` the arrow is gone (loosed)."""
    am.pose(ch.arm, trunk, prefix=P)
    sh = {sd: ch.arm.pose.bones[P + "upperarm_" + sd].head.copy() for sd in "lr"}
    ba, ad = Vector(bow_axis).normalized(), Vector(arrow_dir).normalized()
    w = reins_world(ch.arm, {"l": Vector(bow_at) - ba * KNUCKLE, "r": Vector(nock_at) - ad * KNUCKLE}, shoulders=sh)
    w["hand_l"] = _turn(Vector((0.0, -1.0, 0.0)), ba)
    w["hand_r"] = _turn(Vector((0.0, -1.0, 0.0)), ad)
    spec = _with_world(trunk, w)
    spec["show"] = {"socket.hand_r": 1.0 if shown else 0.0}
    return spec


def bow_poses(ch, seat):
    """{name: pose} of the mounted archer (`held` "bow"): `bow_ready` (nocked, the bow low
    before the body), `bow_half`, `bow_full` (drawn, the arrow level ahead), `bow_loosed`,
    `bow_follow`, `bow_low` (empty), `bow_grasp` (the hand at the quiver), `bow_drawn`
    (the arrow out), `bow_nocking`."""
    trunk = dict(seat)
    trunk.pop("world", None)
    trunk = dict(trunk, **BOW_TRUNK)
    am.pose(ch.arm, trunk, prefix=P)
    sh = ch.arm.pose.bones[P + "upperarm_l"].head.copy()
    full = sh + Vector(BOW_FULL)
    low = sh + Vector(BOW_LOW)
    half = (full + low) / 2
    up, tipped = Vector((0.0, -0.10, 1.0)), Vector((0.0, -0.60, 0.80))
    fwd = Vector((0.0, -1.0, 0.0))

    def string(b):
        return b + Vector((0.0, BOW_NOCK, 0.0))

    def at(b, axis, nock, shown=True):
        d = (b - nock).normalized() if shown else fwd
        return bow_pose(ch, {}, trunk, b, axis, nock, d, shown)

    mouth = getattr(ch, "quiver_mouth", sh + Vector((-0.30, 0.25, 0.10)))
    return {
        "bow_ready": at(low, tipped, string(low)),
        "bow_half": at(half, up, string(half) + Vector((0.0, 0.22, 0.0))),
        "bow_full": at(full, up, full + Vector((0.0, ARROW - 0.04, 0.01))),
        "bow_loosed": at(full, up, full + Vector((0.03, ARROW + 0.06, 0.03)), shown=False),
        "bow_follow": at(full + Vector((0.0, 0.03, -0.03)), up, full + Vector((0.06, ARROW + 0.10, 0.0)), shown=False),
        "bow_low": at(low, tipped, low + Vector((-0.10, 0.25, 0.10)), shown=False),
        "bow_grasp": bow_pose(ch, {}, trunk, low, tipped, mouth, Vector((0.0, 0.3, -1.0)), shown=True),
        "bow_drawn": bow_pose(ch, {}, trunk, low, tipped, mouth + Vector((0.05, -0.10, 0.20)), Vector((0.0, 0.2, -1.0))),
        "bow_nocking": at(low, tipped, string(low) + Vector((-0.04, 0.02, 0.06))),
    }


def bow_keys(n, release, p):
    """ATTACK1 / ATTACK2 of a mounted archer (`SiuKLut`: 86 frames, the arrow leaves on
    72): ready, raise and draw, hold, loose on `release`, follow through, lower, reach to
    the quiver, draw an arrow, nock it, ready (as `clips.bow` on foot)."""
    last = n - 1
    rel = min(release, last)
    rest = last - rel
    keys = [(0, "bow_ready"), (rel * 0.3, "bow_half"), (rel * 0.6, "bow_full"), (rel - 1, "bow_full"),
            (rel, "bow_loosed"), (rel + 2, "bow_follow"), (rel + rest * 0.25, "bow_low"), (rel + rest * 0.45, "bow_grasp"),
            (rel + rest * 0.6, "bow_drawn"), (rel + rest * 0.8, "bow_nocking"), (last, "bow_ready")]
    out = []
    for f, k in keys:
        f = min(max(int(round(f)), 0), last)
        if out and out[-1][0] == f:
            out[-1] = (f, p[k])
        else:
            out.append((f, p[k]))
    return out


def solve_strike(ch, left, trunk, point, axis, length, left_target, tip=None):
    """A blow measured at the weapon's point (m2-melee6): the trunk posed first, both
    arms solved from the shoulders as posed, the left hand to `left_target` (None: on
    the reins, `left`); the weapon (rest -Y along the fist) along `axis` and the right
    wrist moved until the point, `length` past the fist's centre, lies on `point`
    (armature space: the model's ground origin, -x right, -y forward) or as near as the
    arm reaches: then the weapon is turned from the fist straight at the point.
    `tip`: the point in the hand's rest frame from the fist's centre when it lies off the
    weapon's axis (a war club's head: `weapon_tip`); default `length` along rest -Y."""
    am.pose(ch.arm, trunk, prefix=P)
    sh = {sd: ch.arm.pose.bones[P + "upperarm_" + sd].head.copy() for sd in "lr"}
    axis = Vector(axis).normalized()
    tip = Vector(tip) if tip is not None else Vector((0.0, -length, 0.0))
    q = Vector((0.0, -1.0, 0.0)).rotation_difference(axis)
    lw = left if left_target is None else reins_world(ch.arm, {"l": left_target}, sides=("l",), shoulders=sh)
    wrist = point - (q @ tip) - axis * 0.1
    spec = None
    for k in range(10):
        w = dict(lw, **reins_world(ch.arm, {"r": wrist}, sides=("r",), shoulders=sh))
        w["hand_r"] = tuple(math.degrees(x) for x in q.to_euler("XYZ"))
        spec = _with_world(trunk, w)
        am.pose(ch.arm, spec, prefix=P)
        fist = fist_centre(ch)[0]
        at = fist + q @ tip
        if k >= 3 and (at - point).length > 0.05:
            # Out of the arm's reach: the weapon turned from the fist so its point lies
            # on the line to the target.
            q = (q @ tip).normalized().rotation_difference((point - fist).normalized()) @ q
        wrist = wrist + (point - at)
    ch.strike_off = (at - point).length
    hand = tuple(math.degrees(x) for x in q.to_euler("XYZ"))
    print(f"STRIKE pose: point {tuple(round(v, 2) for v in at)} for {tuple(round(v, 2) for v in point)}, "
          f"off {(at - point).length:.2f}, hand {tuple(round(v) for v in hand)}, "
          f"shoulder {tuple(round(v, 2) for v in sh['r'])}, fist {tuple(round(v, 2) for v in fist)}")
    return spec


def weapon_tip(kind):
    """The drawn weapon's point in the hand's rest frame from the fist's centre (`blade`
    puts the item's +Z along rest -Y: item (x, y, z) -> (x, -z, y))."""
    if kind in ("club", "sword"):
        from . import held as library

        x, y, z = library.build("war_club" if kind == "club" else "officer_sword").sockets["socket.tip"]
        return Vector((x, -z, y))
    return Vector((0.0, -0.04 - BLADE[0], 0.03))


def axis_of(down, out):
    """Forward and `down` degrees under the horizontal, `out` degrees to the right
    (negative: to the left)."""
    d, o = math.radians(down), math.radians(out)
    return (-math.cos(d) * math.sin(o), -math.cos(d) * math.cos(o), -math.sin(d))


# ----------------------------------------------------------------------------- mounted reload

# The carbine's rammer (`aim._rod`): length, and its place in the pipes in the rod frame
# (`_rod_frame`: +Z along the barrel from the grip, -Y under it).
CARBINE_ROD = (0.75, Matrix.Translation((0.0, -0.02, 0.15)))
ROD_SOCKET = "socket.hand_l"
LOOK = {"head": (18, 0, -14), "spine": (6, 0, 0)}
# Steps of `poses.MOUNTED_RELOAD`: the carbine by "butt" (anchor, offset) and "at" (where
# the barrel points: anchor, offset), or by "muzzle" (anchor, offset) and "dir" (the
# barrel's direction); "right"/"left" a distance along the carbine from the grip, or
# (anchor, offset); "body" the trunk and head. Anchors (`_anchors`, the seated rider):
# "thigh" the right hip joint, "chest" between the shoulders, "mouth", "pouch" the
# cartridge pouch behind the right hip. Offsets and directions in the armature's axes:
# +x left, -y forward, +z up (metres). Loading, the carbine stands almost upright by the
# horse's right shoulder, the butt down by the right knee and the muzzle before the right
# shoulder, so the rammer stays within the arm's 0.65 m reach.
_ACROSS = dict(butt=("thigh", (0.03, -0.16, 0.10)), at=("chest", (0.45, -0.50, 0.10)), right=0.0, left=0.50, body=LOOK)
_UPRIGHT = dict(muzzle=("chest", (-0.28, -0.30, 0.15)), dir=(0.05, -0.12, 0.99), right=0.0, left=0.60,
                body={"spine": (2, 0, 0), "head": (6, 0, -8)})
RELOAD_STEPS = {
    "across": dict(_ACROSS),
    "half_cock": dict(_ACROSS, right=0.06),
    "cartridge": dict(_ACROSS, right=("pouch", (0.0, 0.0, 0.0)), body={"spine": (2, 0, 0), "head": (6, 0, -25)}),
    "bite": dict(_ACROSS, right=("mouth", (0.0, -0.06, -0.02)), body={"spine": (0, 0, 0), "head": (-4, 0, -6)}),
    "pan": dict(_ACROSS, right=0.08),
    "shut": dict(_ACROSS, right=0.04),
    "upright": dict(_UPRIGHT, right=poses.CARBINE_MUZZLE),
    "pour": dict(_UPRIGHT, right=poses.CARBINE_MUZZLE + 0.04, body={"spine": (2, 0, 0), "head": (2, 0, -8)}),
    "grasp_rod": dict(_UPRIGHT, right=poses.CARBINE_MUZZLE - 0.08),
    "rod_out": dict(_UPRIGHT, right=poses.CARBINE_MUZZLE + 0.36, body={"spine": (0, 0, 0), "head": (-2, 0, -8)}),
    "ram_up": dict(_UPRIGHT, right=poses.CARBINE_MUZZLE + 0.24),
    "ram_down": dict(_UPRIGHT, right=poses.CARBINE_MUZZLE + 0.04),
    "return_rod": dict(_UPRIGHT, right=poses.CARBINE_MUZZLE - 0.10),
    "poise": dict(butt=("thigh", (0.0, -0.14, 0.12)), at=("chest", (-0.10, -0.45, 0.60)), right=0.0, left=0.40,
                  body={"spine": (0, 0, 0), "head": (6, 0, -6)}),
}


def reloads(spec):
    """Whether the rider loads his long arm in ATTACK1 (`poses.MOUNTED_RELOAD`): a carbine
    or a wheellock, an ATTACK1 (the shot) and the right hand free of flag, lance and bow."""
    return spec.get("gun") in ("carbine", "wheellock") and "ATTACK1" in spec["md"] and spec.get("held") is None


def _frame(origin, y, up):
    """A matrix at `origin` with +Y along `y` and +Z towards `up`."""
    y = Vector(y).normalized()
    z = (Vector(up) - y * Vector(up).dot(y)).normalized()
    x = y.cross(z)
    m = Matrix((x, y, z)).transposed().to_4x4()
    m.translation = origin
    return m


def _rod_frame(c):
    """`aim._rod`'s musket frame (+Z along the barrel, -Y under it) of carbine frame `c`."""
    return c @ Matrix(((-1, 0, 0, 0), (0, 0, 1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))


def _root(ch):
    """The rider's root matrix (armature space; the saddle carries it)."""
    return ch.arm.pose.bones[P + "root"].matrix.copy()


def _anchors(ch):
    """The anchor points of the seated rider (armature space, current pose)."""
    pb = ch.arm.pose.bones
    thigh = pb[P + "thigh_r"].head.copy()
    chest = (pb[P + "upperarm_l"].head + pb[P + "upperarm_r"].head) * 0.5
    mouth = pb[P + "head"].head + Vector((0.0, -0.11, 0.05))
    return {"thigh": thigh, "chest": chest, "mouth": mouth, "pouch": thigh + Vector((-0.06, 0.20, 0.22))}


def reload_steps(ch):
    """{step: {"carbine": frame, "goals": {side: point in the carbine's frame}, "body": spec}}
    in the rider's root frame (`_root`), from `RELOAD_STEPS` on the seated pose, plus "@aim"
    (the aim's carbine and fists as posed)."""
    seat = {k: v for k, v in ch.poses["seat"].items() if k != "world"}
    am.pose(ch.arm, ch.poses["seat"], prefix=P)
    r0 = _root(ch).inverted()
    a = {k: r0 @ v for k, v in _anchors(ch).items()}
    axes = r0.to_3x3()  # armature axes into the root's frame

    def point(t):
        return a[t[0]] + axes @ Vector(t[1])

    out = {}
    for name, st in RELOAD_STEPS.items():
        if "muzzle" in st:
            d = (axes @ Vector(st["dir"])).normalized()
            grip = point(st["muzzle"]) - d * poses.CARBINE_MUZZLE
        else:
            butt = point(st["butt"])
            d = (point(st["at"]) - butt).normalized()
            grip = butt + d * 0.12
        c = _frame(grip, d, axes @ Vector((0.0, 0.5, 0.85)))
        goals = {}
        for sd in ("r", "l"):
            t = st["right" if sd == "r" else "left"]
            g = c @ Vector((0.0, t, 0.0)) if isinstance(t, (int, float)) else point(t)
            goals[sd] = c.inverted() @ g
        out[name] = {"carbine": c, "goals": goals, "body": dict(seat, **st["body"])}
    # The aim: the carbine as the hand holds it, both fists where they are.
    am.pose(ch.arm, ch.poses["aim"], prefix=P)
    r0 = _root(ch).inverted()
    c = r0 @ ch.arm.pose.bones[P + "socket.hand_r"].matrix @ ch.carbine
    goals = {sd: c.inverted() @ (r0 @ fist_centre(ch, sd)[0]) for sd in ("r", "l")}
    body = {k: v for k, v in ch.poses["aim"].items() if k != "world" and not k.startswith(("upperarm", "forearm", "hand"))}
    out["@aim"] = {"carbine": c, "goals": goals, "body": body}
    return out


def _blend(a, b, t):
    """Step `a` a fraction `t` of the way to `b`: the carbine's frame slerped and lerped,
    the hands' goals lerped in its frame, the body's angles lerped."""
    ca, cb = a["carbine"], b["carbine"]
    c = ca.to_quaternion().slerp(cb.to_quaternion(), t).to_matrix().to_4x4()
    c.translation = ca.translation.lerp(cb.translation, t)
    goals = {sd: a["goals"][sd].lerp(b["goals"][sd], t) for sd in ("r", "l")}
    body = {}
    for k in sorted(set(a["body"]) | set(b["body"])):
        va, vb = a["body"].get(k, (0.0, 0.0, 0.0)), b["body"].get(k, (0.0, 0.0, 0.0))
        body[k] = tuple(x + (y - x) * t for x, y in zip(va, vb))
    return {"carbine": c, "goals": goals, "body": body}


def _solve_step(ch, step):
    """The rider's spec for `step` (`_blend`) on the horse as posed now: the body first,
    then both arms solved from the shoulders as posed so each fist's centre reaches its
    goal, the carbine placed in armature space on the right-hand socket. Returns (spec,
    carbine frame in armature space)."""
    am.pose(ch.arm, {}, prefix=P)
    c = _root(ch) @ step["carbine"]
    goals = {sd: c @ g for sd, g in step["goals"].items()}
    body = step["body"]
    am.pose(ch.arm, body, prefix=P)
    sh = {sd: ch.arm.pose.bones[P + "upperarm_" + sd].head.copy() for sd in "lr"}
    sock = c @ ch.carbine.inverted()
    rest = ch.arm.pose.bones[P + "socket.hand_r"].bone.matrix_local.to_3x3()
    rot = tuple(math.degrees(x) for x in (sock.to_3x3() @ rest.inverted()).to_euler("XYZ"))
    wrist = {sd: goals[sd].copy() for sd in "lr"}
    spec = None
    for _ in range(6):
        w = reins_world(ch.arm, wrist, shoulders=sh)
        w["socket.hand_r"] = rot
        spec = _with_world(body, w)
        spec["world_loc"] = {"socket.hand_r": tuple(sock.translation)}
        am.pose(ch.arm, spec, prefix=P)
        for sd in "lr":
            wrist[sd] = wrist[sd] + (goals[sd] - fist_centre(ch, sd)[0])
    return spec, c


def reload_clip(ch, md, horse_frames):
    """ATTACK1 of a rider who loads in the saddle (`poses.MOUNTED_RELOAD`): the aim and the
    shot as before, then every frame of the drill solved on the horse as posed (`_key`
    solves the lazy specs on their frame), the rammer keyed per frame (`aim._rod`); writes
    `ch.reload_stats` (each fist's centre from its goal on the carbine, as played)."""
    keys, rod = poses.mounted_reload_plan(md)
    steps, p = ch.reload, ch.poses
    rod_kw = {"prefix": P, "length": CARBINE_ROD[0], "stowed": CARBINE_ROD[1]}
    goals_at = {}

    def lazy(a, b, t):
        def solve(f):
            step = _blend(steps[a], steps[b], t)
            spec, c = _solve_step(ch, step)
            goals_at[f] = (step["goals"], a if t < 0.5 else b)
            shown = 1.0 if rod[0][0] < f < rod[-1][0] else 0.0
            spec["show"] = {ROD_SOCKET: shown}

            def after(frame):
                pb = ch.arm.pose.bones[P + ROD_SOCKET]
                aiming._key(pb, aiming._rod_at(rod, frame, ch.arm.pose.bones, _rod_frame(c), **rod_kw), frame)
                pb.scale = (shown, shown, shown)
                pb.keyframe_insert("scale", frame=frame, group=pb.name)
            spec["_after"] = after
            return spec
        return solve

    fixed = {"@aim": p["aim"], "@fire": dict(p["aim"], spine=(4, 0, 0))}
    rider = []
    for i, (f0, a) in enumerate(keys):
        rider.append((f0, fixed[a] if a.startswith("@") else lazy(a, a, 0.0)))
        if i + 1 == len(keys):
            break
        f1, b = keys[i + 1]
        if a.startswith("@") and b.startswith("@"):
            continue
        for f in range(f0 + 1, f1):
            rider.append((f, lazy(a, b, (f - f0) / (f1 - f0))))
    _key(ch, "ATTACK1", horse_frames, rider)
    # As played: each fist's centre from its goal on the carbine (socket x `ch.carbine`).
    am.set_clip(ch.arm, "ATTACK1")
    miss = []
    for f in sorted(goals_at):
        bpy.context.scene.frame_set(f)
        bpy.context.view_layer.update()
        c = ch.arm.pose.bones[P + "socket.hand_r"].matrix @ ch.carbine
        goals, near = goals_at[f]
        for sd in ("r", "l"):
            miss.append(((fist_centre(ch, sd)[0] - c @ goals[sd]).length, f, sd, near))
    ch.arm.animation_data.action = None
    if miss:
        worst = max(miss)
        by_step = {}
        for d, _f, sd, near in miss:
            k = f"{near}.{sd}"
            by_step[k] = round(max(by_step.get(k, 0.0), d), 3)
        ch.reload_stats = {"frames": len(goals_at), "mean_m": round(sum(d for d, *_ in miss) / len(miss), 4),
                           "max_m": round(worst[0], 4), "worst": [worst[1], worst[2]], "max_by_step": by_step}
        print("ACR mounted reload:", ch.reload_stats)


def dress17(ch, spec, name, origin):
    """The 17th-century rider (`dress` "buff17"): breeches, a buff coat (hooked shut, its
    skirts to above the knee, sleeves of `spec["sleeves"]`), a waist belt, the sash
    (`uniform`), bucket-top boots over the shins and a falling band on the shoulders
    (`drape.bucket_boots`, `drape.falling_band`: modelled on the shells, skinned like them)."""
    from . import drape, frontier, pike  # noqa: F401  (register dsleeve, the band and the boots)

    oz, cy = origin.z, origin.y
    ch.shell("breeches", "breeches", lambda c, n, h, p: (h in {"hips"} | LEGS and c.z < 1.0 + oz), 0.010)
    ch.shell("boots", "boots_high", lambda c, n, h, p: h in SHINS | FEET, 0.006)
    ch.shell("buffcoat", "buff", lambda c, n, h, p: h in TORSO and 0.97 + oz < c.z < 1.48 + oz, 0.03)
    ch.shell("sleeves", spec.get("sleeves", "dsleeve"), lambda c, n, h, p: h in ARMS, 0.03)
    skirt = bd.tube(ch.arm, f"{name}.buff_skirt", rig.SKIRT_TOP + oz, 0.60 + oz, (0.20, 0.155), (0.25, 0.21), gap_deg=40,
                    flare_back=0.03, columns=rig.SKIRT_COLUMNS, centre=(origin.x, origin.y), prefix=P)
    ch.add("clothes", skirt, "buff", symmetric=True)
    belt = bd.strap(ch.arm, f"{name}.belt", ch.flesh, Vector((0.12, cy, 1.47 + oz)), Vector((-0.17, cy, 0.95 + oz)),
                    0.06, 0.045, bones=[P + b for b in rig.BODY_BONES])
    ch.add("clothes", belt, "belts", symmetric=True)
    uniform(ch, spec, oz, cy)
    drape.bucket_boots(ch, name)
    drape.falling_band(ch, name)


def make(ctx, spec):
    name, root = ctx.name, ctx.root
    md = spec["md"]
    hz = mount.load(name, root, horse_clips(md, spec.get("motions", MOTIONS), reloads(spec)), coat=spec.get("coat"))
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
        uniform(ch, spec, oz, cy)
    elif spec.get("dress") == "buff17":
        dress17(ch, spec, name, origin)
    else:
        ch.shell("breechcloth", "breeches", lambda c, n, h, p: h in {"hips"} | LEGS and 0.80 + oz < c.z < 1.0 + oz, 0.012)
        ch.shell("leggings", "boots", lambda c, n, h, p: h in SHINS | FEET or (h in LEGS and c.z < 0.80 + oz), 0.012)
        if spec.get("shirt"):
            ch.shell("shirt", "tunic", lambda c, n, h, p: (h in TORSO and 0.97 + oz < c.z < 1.47 + oz) or h in ARMS, 0.014)
    if spec.get("armour"):
        ch.shell("cuirass", "metal", lambda c, n, h, p: h in TORSO and 1.0 + oz < c.z < 1.46 + oz, 0.05)
    if spec.get("arms"):
        ch.shell("vambraces", "metal", lambda c, n, h, p: h in ARMS, 0.042)
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
        if spec.get("holster"):
            ch.add("gear", holster_pistol(ch, hz, name), "wood")
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

    def strike(trunk, point, axis, length, left_target, tip=None):
        return solve_strike(ch, left, trunk, point, axis, length, left_target, tip)

    # Standing in the stirrups: the seat raised, the knees opened (m2-melee6).
    stirrups = dict(LEGS_SEAT, thigh_l=(-42, -16, 0), shin_l=(56, 0, 0), foot_l=(-14, 0, 0),
                    thigh_r=(-42, 16, 0), shin_r=(56, 0, 0), foot_r=(-14, 0, 0))
    pommel = Vector((0.0, sad.y - 0.22, sad.z + 0.12))
    lance_len = LANCE[0] - LANCE[1] + 0.30
    sabre_len = (CLUB_TIP if spec.get("blade") == "club" else BLADE[0]) + 0.04
    club = spec.get("blade") == "club"
    blade_tip = weapon_tip(spec.get("blade", "sabre"))

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
             # m2-melee6: solved at the point (right 0.81, forward 1.73, up 0.20), the hand
             # held out to the right of the shoulder and the lance only 3° out, so the shaft
             # behind the grip passes outside the rider's body instead of through it.
             "lance_down": strike(dict(LEGS_SEAT, spine=(-6, -8, 0), spine1=(-2, -4, 0)),
                                  Vector((-0.81, -1.73, 0.20)), axis_of(44, 3), lance_len, None),
             "lance_mid": _with_world(dict(LEGS_SEAT, spine=(-14, 0, 8)), right(side(0.02, -0.66, 0.36), (2, 0, 0))),
             # Drawn sabre (`drawn`): raised over the right shoulder, the blade up and back;
             # the cuts down at a man on the ground right and left of the horse, and the cut
             # at a standing man (the blade level with his shoulder).
             "sabre_up": _with_world(dict(LEGS_SEAT, spine=(4, 0, -10)), right(side(0.06, 0.04, 0.78), (-110, 0, 0))),
             # Down right (m2-melee5): leaning far out over the right side, the hand low
             # beside the horse, the blade driven almost straight down at a man lying
             # about 1.2 m out (the horse turned 75° from him, `strike_view::TURN_SABRE`).
             # m2-melee6: standing in the stirrups, the seat shifted right, leaning far out
             # over the right side (the seat over the right stirrup) with the left hand on
             # the pommel, the arm extended down and the blade angled down at the near part
             # of a man lying 0.95 m out at 75° (`strike_view::TURN_SABRE`, `NEAR_BODY`);
             # solved at the point. 1.2 m out is beyond a seated man's arm and blade. The arm IK used the
             # shoulder at rest before, so the lean carried the hand off its target.
             "sabre_down_r": strike(dict(stirrups, **(CLUB_DOWN_R if club else SABRE_DOWN_R)),
                                    Vector((-0.92, -0.25, 0.20)), axis_of(*(CLUB_DOWN_R_AXIS if club else (62, 40))),
                                    sabre_len, pommel, blade_tip),
             # Down left (`strike_view::TURN_SABRE_L`, the man 40° left of the facing, 0.95 m
             # out): across the neck, leaning out over the left shoulder.
             "sabre_down_l": strike(dict(stirrups, **SABRE_DOWN_L), SABRE_DOWN_L_AT, axis_of(*SABRE_DOWN_L_AXIS),
                                    sabre_len, pommel, blade_tip),
             "sabre_mid": _with_world(dict(LEGS_SEAT, spine=(-8, -8, 6)), right(side(-0.16, -0.46, 0.36), (25, 25, 0))),
             # A gun upright, its butt on the right thigh (the socket turned from the aim's).
             "gun": _with_world(dict(LEGS_SEAT), right(side(0.04, -0.22, 0.12), None, (-2, 0, 0)))}
    for k in ("lance", "gun", "flag"):
        poses[k + "_lean"] = dict(poses[k], spine=(-12, 0, 0))
    ch.strike_ctx = {"left": left, "stirrups": stirrups, "pommel": pommel, "sabre_len": sabre_len}
    lie_spec = dict(rig_lie(), world={"root": (90, 0, heading)}, world_loc={"root": lie})
    ch.poses, ch.lie = poses, lie_spec
    ch.ride = ride_pose(spec)
    if spec.get("gun"):
        am.pose(ch.arm, poses["aim"], prefix=P)
        gun(ch, f"{name}.gun", spec["gun"])
    if reloads(spec):
        am.pose(ch.arm, poses["seat"], prefix=P)
        rod = gear.rammer(f"{name}.rammer", length=CARBINE_ROD[0])
        am.attach(rod, ch.arm, P + ROD_SOCKET, am.world(ch.arm, P + ROD_SOCKET))
        ch.held["rammer"] = rod
        ch.keys[rod.name] = "metal"
        ch.reload = reload_steps(ch)
    am.pose(ch.arm, poses[ch.ride], prefix=P)

    hat = spec.get("hat", "cap")
    if hat in ("feathers", "crown", "band"):
        native_head(ch, name, hat, cy)
    elif hat == "morion":
        morion(ch, name)
    elif hat == "zischagge":
        zischagge(ch, name, plume=spec.get("plume"))
    elif hat == "tricorne":
        tricorne(ch, name, lace=bool(spec.get("lace")), plume=spec.get("plume"))
    elif hat == "bonnet":
        bonnet(ch, name)
    elif hat == "broad":
        broad_hat(ch, name)
    elif hat == "helmet":
        helmet(ch, name)
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
    if spec.get("held") == "bow":
        am.pose(ch.arm, poses["seat"], prefix=P)
        bow(ch, name)
        poses.update(bow_poses(ch, poses["seat"]))
        am.pose(ch.arm, poses[ch.ride], prefix=P)
        ch.show = {"socket.hand_r": 1.0}
    if spec.get("drawn"):
        am.pose(ch.arm, poses["seat"], prefix=P)
        blade(ch, f"{name}.blade", spec.get("blade", "sabre"))
        am.pose(ch.arm, poses[ch.ride], prefix=P)
        ch.show = dict(SHOW_REST)
    if reloads(spec):
        # The rammer shows only while it is out of its pipes (`reload_clip`).
        ch.show = dict(getattr(ch, "show", None) or {}, **{ROD_SOCKET: 0.0})
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
    keyed = _keyed(ch.arm, [{k: v for k, v in s.items() if k != "show"} for _f, s in rider_keys if not callable(s)])
    show = getattr(ch, "show", None)

    def pose(arm, s, f):
        # A callable is solved on its frame, the horse already posed (`reload_clip`); its
        # "_after" keys what depends on the posed rider (the rammer).
        if callable(s):
            s = s(f)
        after = s.get("_after")
        am.pose(arm, {k: v for k, v in s.items() if k not in ("show", "_after")}, frame=f, prefix=P, keyed=keyed)
        # A rider with a drawn sabre: every clip keys the sockets' scale (`SHOW_*`); a clip
        # without the channel would show them all.
        for bone, default in (show or {}).items():
            k = s.get("show", {}).get(bone, default)
            pb = arm.pose.bones[P + bone]
            pb.scale = (k, k, k)
            pb.keyframe_insert("scale", frame=f, group=pb.name)
        if after:
            after(f)

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
    if held == "bow":
        return "seat"
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
    if "ATTACK1" in W and "bow_ready" in p:
        # The mounted archer (`held` "bow"): ATTACK1 (fire arrow) and ATTACK2 (arrow) draw and
        # loose on the `.MD`'s SETACTIVEPOINT (`bow_release`); the ready stances hold the bow.
        n = len(W["ATTACK1"])
        keys = bow_keys(n, getattr(ch, "bow_release", int(n * 0.83)), p)
        for k in ("ATTACK1", "ATTACK2"):
            if k in md:
                plan[k] = (W["ATTACK1"], [(f / max(n - 1, 1), s) for f, s in keys])
        for k in ("PATTACK1", "UATTACK1", "PSTAND1", "PATTACK2", "UATTACK2", "PSTAND2"):
            if k in md:
                plan[k] = (W["STAND"], [(0.0, p["bow_ready"])])
    elif "ATTACK1" in W and not getattr(ch, "reload", None):
        plan["ATTACK1"] = (W["ATTACK1"], [(0.0, p["aim"]), (0.08, dict(p["aim"], spine=(4, 0, 0))), (0.25, p["aim"]), (1.0, p["aim"])])
    for name, (hf, rk) in plan.items():
        n = len(hf)
        _key(ch, name, hf, [(int(round(t * (n - 1))), s) for t, s in rk])
    if "ATTACK1" in W and getattr(ch, "reload", None):
        reload_clip(ch, md, W["ATTACK1"])
        plan["ATTACK1"] = None
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
    if spec.get("release"):
        ch.bow_release = spec["release"]
    hz = ch.horse["mount"]
    heads.variants(ch)
    ch.morphs()
    rider = ch.layers["flesh"] + ch.layers["clothes"]
    ch.masks()
    made = add_clips(ch, spec["md"], spec.get("motions", MOTIONS))
    am.set_clip(ch.arm, "STAND")
    report = {}
    colours = ch.colours(ctx.ref, spec["palette"], report=report, fixed=spec.get("fixed", ()))
    if spec.get("paint"):
        # The realistic atlas (as the foot units): buff, linen, leather and cloth painters.
        from . import outfit
        from .humans import LAYER_OF

        outfit.dress(ch, colours, LAYER_OF)
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
    if getattr(ch, "reload_stats", None):
        ctx.stats["reload"] = ch.reload_stats
